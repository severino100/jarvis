//! Conversations with Claude Code (or Gemini when Claude has no usage left), one session each, kept across restarts.
//! Same behaviour as chat.mjs; stored in chats-native.json so both Jarvis versions can run side by side.

use crate::brain::{self, Ev, StopHandle};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Instant,
};

const MAX_CHATS: usize = 40;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolLine {
    pub id: String,
    pub name: String,
    pub label: String,
    pub detail: String,
    /// run | ok | error | stopped
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Msg {
    pub id: String,
    /// user | assistant | tools | inform | note | error | implement | handoff
    pub role: String,
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub voice: bool,
    pub at: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip)]
    pub live: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub item_id: Option<i64>,
    pub pr_key: Option<String>,
    pub session_id: Option<String>,
    pub agy_conversation: Option<String>,
    pub codex_thread: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Msg>,
    pub handed_off: bool,
    pub worktree: Option<String>,
    pub implemented_in: Option<String>,
    /// claude | antigravity | codex, chosen in this conversation (None: the default from Definições).
    pub engine: Option<String>,
    /// Model for that engine (None: automatic).
    pub engine_model: Option<String>,
    /// The item branch (from the team convention), so Antigravity may run the preparation commands for it.
    pub branch: Option<String>,
    /// Tokens the model re-reads on every call (measured on Claude, estimated on Antigravity).
    pub context_tokens: Option<u64>,
    /// A summary is being written to continue in a new session.
    pub compacting: bool,
    /// Summary of the previous session, sent once with the next message of the new one.
    pub carry: Option<String>,
    #[serde(skip)]
    pub busy: bool,
}

impl Chat {
    pub fn cost_today(&self) -> (f64, u64) {
        let today = now()[..10].to_owned();
        self.messages.iter().filter(|m| m.role == "note" && m.at.starts_with(&today)).fold((0.0, 0), |(c, t), m| (c + m.cost.unwrap_or(0.0), t + m.tokens.unwrap_or(0)))
    }
    pub fn preview(&self) -> String {
        let m = self.messages.iter().rev().find(|m| matches!(m.role.as_str(), "assistant" | "user" | "inform"));
        let t: String = m.map(|m| m.text.as_str()).unwrap_or_default().chars().map(|c| if "#*`|>_".contains(c) { ' ' } else { c }).collect();
        t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect()
    }
}

/// What the app must do after a turn (the conversation id goes with it).
pub enum ChatEvent {
    /// The plan is ready: show the conversation with the "where to implement" card.
    Implement(String),
    /// Claude asked for the browser: the session was opened in the Claude app.
    Handoff(String),
    /// Any turn finished (a toast when the window is not in front).
    TurnEnd(String),
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    chats: Vec<Chat>,
}

struct Inner {
    chats: Vec<Chat>,
    running: HashMap<String, StopHandle>,
    events: Vec<ChatEvent>,
    /// The voice panel listening to a conversation: finished text blocks are read aloud.
    voice: HashMap<String, std::sync::mpsc::Sender<VoiceMsg>>,
}

pub enum VoiceMsg {
    Block(String),
    Tool(String),
    /// error, and "implement" | "handoff" when the turn ended in one of those.
    Done { error: Option<String>, kind: Option<String> },
}

/// Engine choice from the settings.
pub fn engine() -> Engine {
    let s = crate::settings::load();
    Engine { model: s.model, claude_model: s.claude_model, agy_model: s.agy_model, codex_model: s.codex_model }
}

#[derive(Clone)]
pub struct Chats {
    inner: Arc<Mutex<Inner>>,
    file: PathBuf,
    ctx: egui::Context,
}

/// auto (Claude, Gemini when Claude has no usage) | claude | gemini | codex, and each engine's model pick.
#[derive(Clone)]
pub struct Engine {
    pub model: String,
    pub claude_model: String,
    /// Antigravity model id (`agy models`), or auto for its own default.
    pub agy_model: String,
    /// Codex model id (`codex debug models`), or auto for the one in the user's Codex settings.
    pub codex_model: String,
}

/// The engine a settings value stands for: claude | antigravity | codex.
pub fn engine_of(model_setting: &str) -> &'static str {
    match model_setting {
        "gemini" => "antigravity",
        "codex" => "codex",
        _ => "claude",
    }
}

/// The settings value that makes an engine the default.
pub fn setting_of(engine: &str) -> &'static str {
    match engine {
        "antigravity" => "gemini",
        "codex" => "codex",
        _ => "auto",
    }
}

pub fn engine_label(engine: &str) -> &'static str {
    match engine {
        "antigravity" => "Antigravity",
        "codex" => "Codex",
        _ => "Claude Code",
    }
}

pub fn engine_installed(engine: &str) -> bool {
    match engine {
        "antigravity" => brain::gemini_installed(),
        "codex" => brain::codex_installed(),
        _ => true,
    }
}

/// Which engine wrote an assistant message.
pub fn msg_engine(m: &Msg) -> &'static str {
    match m.model.as_deref() {
        Some("gemini" | "antigravity") => "antigravity",
        Some("codex") => "codex",
        _ => "claude",
    }
}

pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn new_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("n{:x}{:04x}", t, N.fetch_add(1, Ordering::Relaxed) & 0xffff)
}

fn msg(role: &str, text: impl Into<String>) -> Msg {
    Msg { id: new_id(), role: role.into(), text: text.into(), at: now(), ..Default::default() }
}

fn wants_opus(text: &str) -> bool {
    let t = text.to_lowercase();
    ["pensa bem", "pensa a fundo", "opus"].iter().any(|w| t.contains(w))
}

impl Chats {
    pub fn load(ctx: egui::Context) -> Self {
        let file = crate::settings::data_dir().join("chats-native.json");
        let mut chats = fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<File>(&s).ok()).unwrap_or_default().chats;
        // Once: the Electron conversations come over (same format), so the switch keeps the history.
        let imported = crate::settings::with_state(|s| std::mem::replace(&mut s.imported_electron_chats, true));
        if !imported {
            let old = fs::read_to_string(crate::settings::data_dir().join("chats.json")).ok().and_then(|s| serde_json::from_str::<File>(&s).ok()).unwrap_or_default().chats;
            for c in old {
                if !chats.iter().any(|x| x.id == c.id) {
                    chats.push(c);
                }
            }
            chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            chats.truncate(MAX_CHATS);
            if let Ok(s) = serde_json::to_string_pretty(&File { chats: chats.clone() }) {
                let _ = fs::create_dir_all(crate::settings::data_dir());
                let _ = fs::write(&file, s);
            }
        }
        for c in &mut chats {
            for t in c.messages.iter_mut().flat_map(|m| m.tools.iter_mut()) {
                if t.status == "run" {
                    t.status = "stopped".into();
                }
            }
        }
        Self { inner: Arc::new(Mutex::new(Inner { chats, running: HashMap::new(), events: Vec::new(), voice: HashMap::new() })), file, ctx }
    }

    pub fn set_voice_hook(&self, id: &str, tx: Option<std::sync::mpsc::Sender<VoiceMsg>>) {
        let mut inner = self.inner.lock().unwrap();
        match tx {
            Some(t) => inner.voice.insert(id.to_owned(), t),
            None => inner.voice.remove(id),
        };
    }

    fn voice_send(&self, id: &str, m: VoiceMsg) {
        if let Some(tx) = self.inner.lock().unwrap().voice.get(id) {
            let _ = tx.send(m);
        }
    }

    /// A message from Jarvis itself (not Claude) about a change on the board or in a PR, in that conversation.
    pub fn inform(&self, item_id: Option<i64>, pr_key: Option<&str>, title: &str, text: &str) -> String {
        let found = self.with(|cs| {
            cs.iter().find(|c| !c.handed_off && ((item_id.is_some() && c.item_id == item_id) || (pr_key.is_some() && c.pr_key.as_deref() == pr_key))).map(|c| c.id.clone())
        });
        let id = found.unwrap_or_else(|| self.create(Some(title.to_owned()), item_id, pr_key.map(str::to_owned)));
        let mut inner = self.inner.lock().unwrap();
        if let Some(c) = inner.chats.iter_mut().find(|c| c.id == id) {
            c.messages.push(msg("inform", text));
            c.updated_at = now();
        }
        Self::to_front(&mut inner, &id);
        self.save(&inner);
        drop(inner);
        self.ctx.request_repaint();
        id
    }

    pub fn for_pr(&self, key: &str) -> Option<String> {
        self.with(|cs| cs.iter().find(|c| c.pr_key.as_deref() == Some(key) && !c.handed_off).map(|c| c.id.clone()))
    }

    pub fn is_busy(&self, id: &str) -> bool {
        self.with(|cs| cs.iter().any(|c| c.id == id && c.busy))
    }

    /// After an item's worktree is cleaned up and the item has moved on, its conversations go too.
    pub fn remove_for_item(&self, item_id: i64) -> usize {
        let ids: Vec<String> = self.with(|cs| cs.iter().filter(|c| c.item_id == Some(item_id)).map(|c| c.id.clone()).collect());
        for id in &ids {
            self.remove(id);
        }
        ids.len()
    }

    fn save(&self, inner: &Inner) {
        let data = File { chats: inner.chats.iter().take(MAX_CHATS).cloned().collect() };
        if let Ok(s) = serde_json::to_string_pretty(&data) {
            let _ = fs::create_dir_all(self.file.parent().unwrap());
            let tmp = self.file.with_extension("json.tmp");
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, &self.file);
            }
        }
    }

    /// Short read access for drawing a frame.
    pub fn with<R>(&self, f: impl FnOnce(&[Chat]) -> R) -> R {
        f(&self.inner.lock().unwrap().chats)
    }

    pub fn take_events(&self) -> Vec<ChatEvent> {
        std::mem::take(&mut self.inner.lock().unwrap().events)
    }

    /// Changes one conversation; `persist` also writes the file.
    fn edit<R>(&self, id: &str, persist: bool, f: impl FnOnce(&mut Chat) -> R) -> Option<R> {
        let mut inner = self.inner.lock().unwrap();
        let c = inner.chats.iter_mut().find(|c| c.id == id)?;
        let r = f(c);
        c.updated_at = now();
        if persist {
            self.save(&inner);
        }
        drop(inner);
        self.ctx.request_repaint();
        Some(r)
    }

    fn to_front(inner: &mut Inner, id: &str) {
        if let Some(i) = inner.chats.iter().position(|c| c.id == id) {
            let c = inner.chats.remove(i);
            inner.chats.insert(0, c);
        }
    }

    pub fn create(&self, title: Option<String>, item_id: Option<i64>, pr_key: Option<String>) -> String {
        let c = Chat {
            id: new_id(),
            title: title.unwrap_or_else(|| "Nova conversa".into()),
            item_id,
            pr_key,
            created_at: now(),
            updated_at: now(),
            ..Default::default()
        };
        let id = c.id.clone();
        let mut inner = self.inner.lock().unwrap();
        inner.chats.insert(0, c);
        let gone: Vec<String> = inner.chats.iter().skip(MAX_CHATS).map(|c| c.id.clone()).collect();
        for g in gone {
            if let Some(s) = inner.running.remove(&g) {
                s.stop();
            }
        }
        inner.chats.truncate(MAX_CHATS);
        self.save(&inner);
        drop(inner);
        self.ctx.request_repaint();
        id
    }

    pub fn for_item(&self, item_id: i64) -> Option<String> {
        self.with(|cs| {
            cs.iter().find(|c| c.item_id == Some(item_id) && !c.handed_off).or_else(|| cs.iter().find(|c| c.item_id == Some(item_id))).map(|c| c.id.clone())
        })
    }

    pub fn add_note(&self, id: &str, text: &str, role: &str) {
        self.edit(id, true, |c| c.messages.push(msg(role, text)));
    }

    pub fn stop(&self, id: &str) {
        if let Some(s) = self.inner.lock().unwrap().running.get(id) {
            s.stop();
        }
    }

    pub fn stop_all(&self) {
        for s in self.inner.lock().unwrap().running.values() {
            s.stop();
        }
    }

    pub fn remove(&self, id: &str) {
        self.stop(id);
        let mut inner = self.inner.lock().unwrap();
        inner.chats.retain(|c| c.id != id);
        self.save(&inner);
    }

    /// After the Claude app imports the session it owns it, so the conversation stays read-only here.
    pub fn open_in_claude(&self, id: &str) -> Option<String> {
        self.edit(id, true, |c| {
            if c.busy {
                return None;
            }
            let url = brain::hand_off(c.session_id.as_deref(), &c.title)?;
            c.handed_off = true;
            if c.worktree.is_some() {
                c.implemented_in = Some("claude".into());
            }
            Some(url)
        })
        .flatten()
    }

    pub fn mark_implemented(&self, id: &str, target: &str, text: &str) {
        self.edit(id, true, |c| {
            c.implemented_in = Some(target.into());
            c.messages.push(msg("note", text));
        });
    }

    /// voice: the reply is read aloud, so Claude answers in short plain speech (see [voz] in the persona).
    /// The engine and model chosen in this conversation; None goes back to the default from Definições.
    pub fn set_engine(&self, id: &str, engine: Option<String>, model: Option<String>) {
        self.edit(id, true, |c| {
            c.engine = engine;
            c.engine_model = model;
        });
    }

    pub fn send(&self, id: &str, text: &str, voice: bool, engine: Engine) -> bool {
        let text = text.trim().to_owned();
        let history;
        let item_id;
        let turn_start;
        let mut engine = engine;
        let mut prompt_text = text.clone();
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(c) = inner.chats.iter_mut().find(|c| c.id == id) else { return false };
            if text.is_empty() || c.busy || c.handed_off {
                return false;
            }
            if c.title == "Nova conversa" && c.item_id.is_none() {
                c.title = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
            }
            engine = c.engine_for(engine);
            // No engine can resume another's session: each gets, as text, what the others answered since.
            history = ["claude", "antigravity", "codex"].map(|e| catch_up(&c.messages, e));
            c.messages.push(Msg { voice, ..msg("user", text.clone()) });
            if let Some(summary) = c.carry.take() {
                prompt_text = format!("Resumo da sessão anterior desta conversa (é uma sessão nova; parte daqui):
{summary}

Mensagem de {}:
{text}", crate::profile::name());
            }
            c.busy = true;
            c.updated_at = now();
            item_id = c.item_id;
            turn_start = c.messages.len();
            let id = id.to_owned();
            Self::to_front(&mut inner, &id);
            inner.running.insert(id, StopHandle::default());
            self.save(&inner);
        }
        self.ctx.request_repaint();

        let this = self.clone();
        let id = id.to_owned();
        thread::spawn(move || this.turn(&id, &prompt_text, voice, &history, item_id, turn_start, engine));
        true
    }

    fn turn(&self, id: &str, text: &str, voice: bool, history: &[String; 3], item_id: Option<i64>, turn_start: usize, engine: Engine) {
        let started = Instant::now();
        let stop = self.inner.lock().unwrap().running.get(id).cloned().unwrap_or_default();
        let prompt = if voice { format!("{} {text}", brain::VOICE_TAG) } else { text.to_owned() };
        let claude_prompt = if history[0].is_empty() {
            prompt.clone()
        } else {
            format!("Entretanto a conversa continuou noutro motor, que não está na tua sessão:\n{}\n\nMensagem atual de {}:\n{prompt}", history[0], crate::profile::name())
        };
        let claude_model = match engine.claude_model.as_str() {
            m @ ("sonnet" | "opus" | "haiku") => m.to_owned(),
            _ if wants_opus(text) => "opus".into(),
            _ => "sonnet".into(),
        };
        let agy_model = Some(engine.agy_model.as_str()).filter(|m| !m.is_empty() && *m != "auto");
        let codex_model = Some(engine.codex_model.as_str()).filter(|m| !m.is_empty() && *m != "auto");

        let run = |kind: &'static str| -> Result<brain::Outcome, String> {
            let (session, conversation, thread, branch) = self.with(|cs| {
                cs.iter().find(|c| c.id == id).map(|c| (c.session_id.clone(), c.agy_conversation.clone(), c.codex_thread.clone(), c.branch.clone())).unwrap_or_default()
            });
            let prep = item_id.zip(branch).map(|(id, branch)| brain::Prep { id, branch });
            // Jarvis reads the item itself, once per session: each engine keeps it in its own conversation after that
            // (Claude resumes its session, agy its conversation). Asked again only with an explicit item request.
            let fresh = match kind {
                "antigravity" => conversation.is_none(),
                "codex" => thread.is_none(),
                _ => session.is_none(),
            };
            let item = match item_id {
                Some(i) if fresh || crate::profile::skill_command().is_some_and(|c| text.starts_with(&c)) => brain::item_context(i),
                _ => String::new(),
            };
            let mut on = |ev: Ev| self.apply(id, ev, kind);
            match kind {
                "antigravity" => brain::ask_gemini(&prompt, &history[1], &item, conversation.as_deref(), agy_model, prep.as_ref(), &stop, &mut on),
                "codex" => brain::ask_codex(&prompt, &history[2], &item, thread.as_deref(), codex_model, &stop, &mut on),
                _ => brain::ask_claude(&claude_prompt, &item, &claude_model, session.as_deref(), &stop, &mut on),
            }
        };

        let result = if engine.model == "gemini" {
            run("antigravity")
        } else if engine.model == "codex" {
            run("codex")
        } else {
            match run("claude") {
                Err(e) if engine.model == "auto" && brain::is_limit_error(&e) && brain::gemini_installed() => {
                    self.edit(id, false, |c| {
                        for m in &mut c.messages {
                            m.live = false;
                        }
                        c.messages.push(msg("note", "O Claude não tem utilização disponível agora. A continuar no Antigravity."));
                    });
                    run("antigravity")
                }
                r => r,
            }
        };

        let mut error = None;
        let mut stopped = false;
        match result {
            Ok(r) if r.stopped => {
                stopped = true;
                self.edit(id, false, |c| c.messages.push(msg("note", "Paraste a resposta.")));
            }
            Ok(r) => {
                let mut parts = vec![format!("Concluído em {} s", started.elapsed().as_secs().max(1))];
                if r.tokens >= 1000 {
                    parts.push(format!("{} mil tokens", format!("{:.1}", r.tokens as f64 / 1000.0).replace('.', ",")));
                } else if r.tokens > 0 {
                    parts.push(format!("{} tokens", r.tokens));
                }
                if let Some(c) = r.cost {
                    parts.push(format!("{} USD", format!("{c:.2}").replace('.', ",")));
                }
                if let Some(m) = &r.model {
                    parts.push(m.clone());
                }
                if r.max_turns {
                    parts.push("parei no limite de voltas: diz \"continua\" se quiseres que siga".into());
                }
                self.edit(id, false, |c| {
                    // A run that ends right after its tools (agy does when something stops it) left no answer to read.
                    let ended_on_tools = c.messages.last().is_some_and(|m| m.role == "tools");
                    c.context_tokens = r.context.or_else(|| Some(estimate_context(&c.messages)));
                    c.messages.push(Msg { cost: r.cost, tokens: (r.tokens > 0).then_some(r.tokens), ..msg("note", parts.join(" · ")) });
                    if ended_on_tools {
                        c.messages.push(msg("error", "Terminou depois das ferramentas sem escrever a resposta. Escreve \"continua\" para seguir de onde ficou."));
                    }
                });
            }
            Err(e) => {
                let first = e.lines().next().unwrap_or_default().to_owned();
                self.edit(id, false, |c| c.messages.push(msg("error", first.clone())));
                error = Some(first);
            }
        }

        let ok = error.is_none() && !stopped;
        let event = self
            .edit(id, true, |c| {
                let (mut browser, mut implement) = (false, None);
                for (k, m) in c.messages.iter_mut().enumerate() {
                    m.live = false;
                    for t in &mut m.tools {
                        if t.status == "run" {
                            t.status = "stopped".into();
                        }
                    }
                    if k >= turn_start && m.role == "assistant" {
                        let (b, i) = brain::read_marks(&m.text);
                        browser |= b;
                        if implement.is_none() {
                            implement = i;
                        }
                        m.text = brain::strip_marks(&m.text);
                    }
                }
                let before = c.messages.len().saturating_sub(1);
                c.busy = false;
                // The summary asked for by "continuar numa sessão nova" is ready: both engines start over from it.
                if c.compacting {
                    c.compacting = false;
                    let summary = c.messages[turn_start.min(c.messages.len())..].iter().filter(|m| m.role == "assistant").map(|m| m.text.clone()).collect::<Vec<_>>().join("

");
                    if ok && !summary.trim().is_empty() {
                        c.carry = Some(summary);
                        c.session_id = None;
                        c.agy_conversation = None;
                        c.codex_thread = None;
                        c.context_tokens = None;
                        c.messages.push(msg("divider", "Sessão nova a partir daqui: o resumo acima segue com a tua próxima mensagem."));
                    }
                    return None;
                }
                if let (Some(w), true) = (implement, ok) {
                    c.worktree = Some(w.clone());
                    c.messages.insert(before, Msg { worktree: Some(w), ..msg("implement", "Está tudo preparado. Onde queres implementar?") });
                    return Some(ChatEvent::Implement(c.id.clone()));
                }
                if browser && ok {
                    if let Some(url) = brain::hand_off(c.session_id.as_deref(), &c.title) {
                        c.handed_off = true;
                        c.messages.insert(before, msg("handoff", "Abri esta conversa na app do Claude para veres no browser. Continua lá."));
                        return Some(ChatEvent::Handoff(url));
                    }
                }
                None
            })
            .flatten();
        let kind = match &event {
            Some(ChatEvent::Implement(_)) => Some("implement".to_owned()),
            Some(ChatEvent::Handoff(_)) => Some("handoff".to_owned()),
            Some(ChatEvent::TurnEnd(_)) => None,
            None => None,
        };
        self.voice_send(id, VoiceMsg::Done { error, kind });
        let mut inner = self.inner.lock().unwrap();
        inner.running.remove(id);
        if let Some(e) = event {
            inner.events.push(e);
        }
        inner.events.push(ChatEvent::TurnEnd(id.to_owned()));
        drop(inner);
        self.ctx.request_repaint();
    }

    /// Dev aid for JARVIS_SHOT_VIEW=chat: a finished conversation to screenshot without calling Claude.
    pub fn seed_demo(&self) {
        if self.with(|cs| !cs.is_empty()) {
            return;
        }
        let id = self.create(Some("#1234 Exemplo de conversa (captura)".into()), Some(1234), None);
        let wt = crate::profile::projects_dir().join("app-1234").to_string_lossy().into_owned();
        let tool = |id: &str, name: &str, label: &str, detail: &str, status: &str| ToolLine { id: id.into(), name: name.into(), label: label.into(), detail: detail.into(), status: status.into() };
        self.edit(&id, true, |c| {
            c.worktree = Some(wt.clone());
            c.messages = vec![
                Msg { voice: true, ..msg("user", "Pega no 1234") },
                Msg { model: Some("claude".into()), ..msg("assistant", "Vou ler o item e procurar o código envolvido.") },
                Msg {
                    tools: vec![
                        tool("a", "Skill", "A seguir o fluxo da skill…", "board", "ok"),
                        tool("b", "Bash", "A ler o Azure DevOps…", "node azdo.mjs show 1234", "ok"),
                        tool("c", "Grep", "A procurar no código…", "exemplo em pages", "error"),
                        tool("d", "Read", "A ler código…", "exemplo.component.ts", "ok"),
                    ],
                    ..msg("tools", "")
                },
                Msg {
                    model: Some("claude".into()),
                    ..msg(
                        "assistant",
                        "## Causa\n\n**Exemplo** de resposta com markdown: um parágrafo, `código inline` e uma lista:\n\n- primeiro ponto\n- segundo ponto com `ficheiro.ts`\n\n| Tarefa | Lado | Estimativa |\n| --- | --- | --- |\n| Front | FRONT | 2 h |\n| Back | BACK | 1 h |\n\n```ts\nconst x = 1;\n```\n\nAvanço?",
                    )
                },
                msg("user", "Sim, avança."),
                Msg { worktree: Some(wt.clone()), ..msg("implement", "Está tudo preparado. Onde queres implementar?") },
                Msg { cost: Some(0.42), tokens: Some(48200), ..msg("note", "Concluído em 48 s · 48,2 mil tokens · 0,42 USD · claude-sonnet") },
            ];
        });
    }

    fn apply(&self, id: &str, ev: Ev, engine: &str) {
        match &ev {
            Ev::Block(t) => {
                let clean = brain::strip_marks(t);
                if !clean.trim().is_empty() {
                    self.voice_send(id, VoiceMsg::Block(clean));
                }
            }
            Ev::Tool { name, input, .. } => self.voice_send(id, VoiceMsg::Tool(brain::tool_label(name, input))),
            _ => {}
        }
        self.edit(id, false, |c| match ev {
            Ev::Session(s) => match engine {
                "antigravity" => c.agy_conversation = Some(s),
                "codex" => c.codex_thread = Some(s),
                _ => c.session_id = Some(s),
            },
            Ev::Delta(t, new_block) => match c.messages.last_mut() {
                Some(m) if m.role == "assistant" && m.live => {
                    if new_block && !m.text.is_empty() {
                        m.text.push_str("\n\n");
                    }
                    m.text.push_str(&t);
                }
                _ => c.messages.push(Msg { live: true, model: Some(engine.into()), ..msg("assistant", t.trim_start_matches('\n')) }),
            },
            Ev::Block(_) => {}
            Ev::Tool { name, input, id: tool_id } => {
                let tool = ToolLine { id: tool_id, label: brain::tool_label(&name, &input), detail: brain::tool_detail(&name, &input), name, status: "run".into() };
                match c.messages.last_mut() {
                    Some(m) if m.role == "tools" => m.tools.push(tool),
                    last => {
                        if let Some(m) = last {
                            m.live = false;
                        }
                        c.messages.push(Msg { tools: vec![tool], ..msg("tools", "") });
                    }
                }
            }
            Ev::ToolResult { id: tool_id, error } => {
                for t in c.messages.iter_mut().flat_map(|m| m.tools.iter_mut()) {
                    if t.id == tool_id {
                        t.status = if error { "error" } else { "ok" }.into();
                    }
                }
            }
        });
    }
}

pub fn is_agy(m: &Msg) -> bool {
    matches!(m.model.as_deref(), Some("gemini" | "antigravity"))
}

/// What the other engine answered since this engine's last reply, as text ("" when there is nothing new from it).
fn catch_up(msgs: &[Msg], engine: &str) -> String {
    let mine = |m: &Msg| m.role == "assistant" && msg_engine(m) == engine;
    let start = msgs.iter().rposition(mine).map(|i| i + 1).unwrap_or(0);
    let since = &msgs[start..];
    if !since.iter().any(|m| m.role == "assistant") {
        return String::new();
    }
    let user = crate::profile::name();
    let lines: Vec<String> = since
        .iter()
        .filter(|m| matches!(m.role.as_str(), "user" | "assistant" | "inform") && !m.text.is_empty())
        .map(|m| {
            let who = match m.role.as_str() {
                "user" => user.as_str(),
                "inform" => "Jarvis (novidade do board)",
                _ => engine_label(msg_engine(m)),
            };
            let t = if m.text.chars().count() > 1500 { format!("{}…", m.text.chars().take(1500).collect::<String>()) } else { m.text.clone() };
            format!("{who}: {t}")
        })
        .collect();
    lines[lines.len().saturating_sub(24)..].join("\n\n")
}

impl Chat {
    /// This conversation's engine choice on top of the default one.
    pub fn engine_for(&self, mut e: Engine) -> Engine {
        match self.engine.as_deref() {
            Some("antigravity") => e.model = "gemini".into(),
            Some("codex") => e.model = "codex".into(),
            Some("claude") if matches!(e.model.as_str(), "gemini" | "codex") => e.model = "auto".into(),
            _ => {}
        }
        if let Some(m) = &self.engine_model {
            match e.model.as_str() {
                "gemini" => e.agy_model = m.clone(),
                "codex" => e.codex_model = m.clone(),
                _ => e.claude_model = m.clone(),
            }
        }
        e
    }
}

/// The point where a conversation gets expensive: every message re-reads this much.
pub const CONTEXT_WARN: u64 = 80_000;

const SUMMARY_PROMPT: &str = "Vou continuar esta conversa numa sessão nova para poupar tokens. Escreve um resumo que baste para seguir sem o histórico: o item e o objetivo, o que já foi decidido, o plano atual, os ficheiros e linhas que importam, o estado das tarefas, do worktree e do branch, e as perguntas em aberto. Só o resumo, em markdown, sem usar ferramentas.";

/// Rough size of what an engine re-reads, when it does not report it (agy): about 4 characters per token, from the
/// last session start.
fn estimate_context(msgs: &[Msg]) -> u64 {
    let start = msgs.iter().rposition(|m| m.role == "divider").map(|i| i + 1).unwrap_or(0);
    let chars: usize = msgs[start..].iter().map(|m| m.text.len() + m.tools.iter().map(|t| t.detail.len() + 2000).sum::<usize>()).sum();
    (chars / 4) as u64 + 8_000
}

impl Chats {
    /// "Continuar numa sessão nova": the current engine writes a summary; when it is done the sessions are dropped and
    /// the summary goes with the next message.
    pub fn compact(&self, id: &str, engine: Engine) -> bool {
        let ok = self.edit(id, false, |c| {
            if c.busy || c.compacting {
                return false;
            }
            c.compacting = true;
            true
        }) == Some(true);
        if ok && !self.send(id, SUMMARY_PROMPT, false, engine) {
            self.edit(id, false, |c| c.compacting = false);
            return false;
        }
        ok
    }
}

impl Chats {
    pub fn set_branch(&self, id: &str, branch: String) {
        self.edit(id, true, |c| c.branch = Some(branch));
    }
}
