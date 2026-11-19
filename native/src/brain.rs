//! Claude Code (headless) and the Antigravity CLI (Gemini) as child processes, read as a JSON stream.
//! Jarvis's rules live in docs/persona*.md (templates filled from the profile); the team's rules, skills and guides
//! come from the active pack.

use regex::Regex;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

pub const VOICE_TAG: &str = "[voz]";

const PERSONA: &str = include_str!("../../docs/persona.md");
const GEMINI_NOTES: &str = include_str!("../../docs/persona-gemini.md");
const CODEX_NOTES: &str = include_str!("../../docs/persona-codex.md");

fn with_team(own: &str, team: crate::profile::Guide) -> String {
    let own = crate::profile::render(own.trim());
    let team = crate::profile::read_guide(team);
    if team.is_empty() { own } else { format!("{own}\n\nRegras da equipa:\n{team}") }
}

const PERSONA_RH: &str = include_str!("../../docs/persona-rh.md");
const GEMINI_NOTES_RH: &str = include_str!("../../docs/persona-rh-gemini.md");
const CODEX_NOTES_RH: &str = include_str!("../../docs/persona-rh-codex.md");

const PERSONA_GERAL: &str = include_str!("../../docs/persona-geral.md");

/// A personal conversation (rh, or the free conversation "geral"): no code, no shell; it starts in the person's
/// own folder. The rest (dev) is the analysis of board items.
fn is_rh(mode: &str) -> bool {
    mode == crate::profile::RH || mode == crate::profile::GERAL
}

/// Jarvis's rules for the conversation's mode (dev: analysis and preparation; rh: the personal HR assistant; geral:
/// the free conversation that can create tasks and draft emails), with the team's rules on top.
fn persona(mode: &str) -> String {
    if mode == crate::profile::GERAL {
        crate::profile::render(PERSONA_GERAL.trim()).replace("{hoje}", &chrono::Local::now().format("%Y-%m-%d (%A)").to_string())
    } else if is_rh(mode) {
        with_team(PERSONA_RH, crate::profile::Guide::PersonaRh)
    } else {
        with_team(PERSONA, crate::profile::Guide::Persona)
    }
}

fn agy_notes(mode: &str) -> String {
    if is_rh(mode) { crate::profile::render(GEMINI_NOTES_RH.trim()) } else { with_team(GEMINI_NOTES, crate::profile::Guide::Agy) }
}

/// The rules plus what the person taught Jarvis (instructions and memory), below them.
fn with_personal(rules: String) -> String {
    let personal = crate::eu::prompt_block();
    if personal.is_empty() { rules } else { format!("{rules}\n\n{personal}") }
}

/// What a turn is about: the conversation's mode (dev | rh) and, in an item conversation, the item as read now.
#[derive(Default)]
pub struct Turn<'a> {
    pub mode: &'a str,
    pub item: &'a str,
}

/// Where the engine starts: the pack's main repository for dev; for rh, which has no code to read, the personal
/// folder (%APPDATA%\Jarvis\eu).
pub fn work_dir(mode: &str) -> PathBuf {
    if is_rh(mode) {
        crate::eu::ensure()
    } else {
        crate::profile::main_dir()
    }
}

fn codex_notes(mode: &str) -> String {
    crate::profile::render(if is_rh(mode) { CODEX_NOTES_RH } else { CODEX_NOTES }.trim())
}

fn projects_arg() -> String {
    crate::profile::projects_dir().to_string_lossy().into_owned()
}

/// The folder the engine may read besides where it starts: the projects for dev; the person's own folder for rh (the
/// documents, downloads and desktop where a CV or a spreadsheet is).
fn extra_dir(mode: &str) -> String {
    if is_rh(mode) { home().to_string_lossy().into_owned() } else { projects_arg() }
}
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Jarvis only uses subscriptions (Claude Pro/Max, ChatGPT, the Google account), never pay-per-use APIs: these
/// variables would switch an engine to an API key or a cloud provider that bills per token, so no engine gets them.
const PAID_ENV: &[&str] = &[
    "CLAUDE", "ANTHROPIC", "OPENAI_", "CODEX_API_KEY", "AZURE_OPENAI", "GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_GENAI",
    "GOOGLE_APPLICATION_CREDENTIALS", "GOOGLE_CLOUD_PROJECT", "VERTEX", "AWS_",
];

/// The message when an engine would cost money beyond the subscription.
fn paid_refusal(engine: &str, why: &str) -> String {
    format!("Bloqueado: o {engine} {why}. O Jarvis só usa subscrições e nunca gasta dinheiro por utilização.")
}
/// Built-in tools the Jarvis analysis needs; the rest (web, notebooks, agents, MCP search) only add context.
const LEAN_TOOLS: &str = "Read,Grep,Glob,Edit,Write,Bash,Skill,TodoWrite";
/// The HR assistant writes and reads documents; no shell (nothing leaves the PC through a command).
const LEAN_TOOLS_RH: &str = "Read,Grep,Glob,Edit,Write,Skill,TodoWrite";

const ALLOWED: &[&str] = &[
    "Read", "Grep", "Glob", "Edit", "Write", "Skill", "TodoWrite",
    "Bash(node:*)", "Bash(npm ls:*)", "Bash(mkdir:*)",
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)", "Bash(git fetch:*)", "Bash(git branch:*)",
    "Bash(git worktree add:*)", "Bash(git worktree list:*)", "Bash(git -C:*)", "Bash(cmd //c mklink:*)",
    "Bash(git add:*)", "Bash(git commit:*)", "Bash(git push:*)", "Bash(cd:*)",
    "Bash(ls:*)", "Bash(cat:*)", "Bash(head:*)", "Bash(grep:*)", "Bash(find:*)", "Bash(wc:*)", "Bash(curl:*)",
];
const DENIED: &[&str] = &[
    "Bash(git reset:*)", "Bash(git checkout:*)", "Bash(git switch:*)", "Bash(git merge:*)", "Bash(git rebase:*)",
    "Bash(git push --force:*)", "Bash(git push -f:*)", "Bash(git worktree remove:*)", "Bash(rm:*)", "Bash(az:*)",
    "Bash(npm start:*)", "Bash(npx ng serve:*)", "Bash(dotnet run:*)",
    // Subagents multiply token use; the Jarvis conversation does its own reading.
    "Agent", "Task",
];

pub enum Ev {
    /// Streamed text; `true` when it starts a new text block.
    Delta(String, bool),
    /// A finished text block (what the voice reads aloud).
    Block(String),
    Tool { name: String, input: Value, id: String },
    ToolResult { id: String, error: bool },
    /// Claude session id, or agy conversation id.
    Session(String),
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub stopped: bool,
    pub tokens: u64,
    pub cost: Option<f64>,
    pub model: Option<String>,
    pub max_turns: bool,
    /// Tokens the model re-reads on every call of this conversation (the last call of the turn), when known.
    pub context: Option<u64>,
}

/// Lets the UI stop a running turn.
#[derive(Clone, Default)]
pub struct StopHandle {
    child: Arc<Mutex<Option<Child>>>,
    stopped: Arc<AtomicBool>,
}

impl StopHandle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn claude_exe() -> PathBuf {
    let p = home().join(".local").join("bin").join("claude.exe");
    if p.exists() { p } else { PathBuf::from("claude") }
}

// Installed outside AppData on purpose: AppData writes from the Claude app are virtualized (see scripts/restart.ps1).
fn agy_exe() -> Option<PathBuf> {
    let mut list = vec![home().join(".agy").join("bin").join("agy.exe")];
    if let Some(d) = dirs::data_local_dir() {
        list.push(d.join("agy").join("bin").join("agy.exe"));
    }
    list.into_iter().find(|p| p.exists())
}

/// Claude Code is on this PC (its installer's folder, or anywhere on the PATH).
pub fn claude_installed() -> bool {
    claude_exe().is_absolute()
        || std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| ["claude.exe", "claude.cmd"].iter().any(|n| d.join(n).is_file())))
}

pub fn gemini_installed() -> bool {
    agy_exe().is_some()
}

/// Claude Code models Jarvis offers: (id for --model, label).
pub const CLAUDE_MODELS: [(&str, &str); 3] = [("sonnet", "Sonnet"), ("opus", "Opus"), ("haiku", "Haiku")];

/// Models the Antigravity CLI offers (`agy models`), read once in the background; a known list until then.
pub fn agy_models() -> Vec<(String, String)> {
    static LIST: OnceLock<Mutex<Option<Vec<(String, String)>>>> = OnceLock::new();
    let cell = LIST.get_or_init(|| {
        thread::spawn(|| {
            let Some(exe) = agy_exe() else { return };
            let Ok(o) = Command::new(exe).arg("models").creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output() else { return };
            let found: Vec<(String, String)> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.split_once('\t'))
                .map(|(id, label)| (id.trim().to_owned(), label.trim().to_owned()))
                .filter(|(id, _)| !id.is_empty() && !id.contains(' '))
                .collect();
            if !found.is_empty() {
                if let Some(c) = LIST.get() {
                    *c.lock().unwrap() = Some(found);
                }
            }
        });
        Mutex::new(None)
    });
    cell.lock().unwrap().clone().unwrap_or_else(|| {
        [
            ("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)"),
            ("gemini-3.8-flash-medium", "Gemini 3.8 Flash (Medium)"),
            ("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)"),
            ("claude-sonnet-4-6", "Claude Sonnet 4.6 (Thinking)"),
            ("claude-opus-4-6-thinking", "Claude Opus 4.6 (Thinking)"),
            ("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)"),
        ]
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .to_vec()
    })
}

// If Jarvis was started from inside a Claude Code session, its CLAUDE_*/ANTHROPIC_* variables point the CLI at that
// host's auth instead of the user's own login.
fn command(exe: &Path, mode: &str) -> Command {
    let mut c = Command::new(exe);
    for (k, _) in std::env::vars() {
        let up = k.to_uppercase();
        if PAID_ENV.iter().any(|p| up.starts_with(p)) {
            c.env_remove(k);
        }
    }
    crate::profile::child_env(&mut c);
    c.current_dir(work_dir(mode)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
    c
}

/// Runs the process, calls `line` for every JSON line of stdout, and returns stderr and the exit code.
fn run(mut cmd: Command, input: Option<String>, stop: &StopHandle, mut line: impl FnMut(Value)) -> Result<(String, Option<i32>), String> {
    stop.stopped.store(false, Ordering::SeqCst);
    let mut child = cmd.spawn().map_err(|e| {
        // Claude Code runs as "claude" from the PATH when its installer's folder is not there: not found = not installed.
        if e.kind() == std::io::ErrorKind::NotFound && cmd.get_program() == "claude" {
            crate::requisitos::missing_message("Claude Code")
        } else {
            format!("Não consegui arrancar {:?}: {e}", cmd.get_program())
        }
    })?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    if let (Some(text), Some(mut w)) = (input, child.stdin.take()) {
        thread::spawn(move || {
            let _ = w.write_all(text.as_bytes());
        });
    }
    *stop.child.lock().unwrap() = Some(child);
    let err_reader = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    for l in BufReader::new(stdout).lines() {
        let Ok(l) = l else { break };
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(l) {
            line(v);
        }
    }
    let code = stop.child.lock().unwrap().take().and_then(|mut c| c.wait().ok()).and_then(|s| s.code());
    let err = err_reader.join().unwrap_or_default();
    Ok((err, code))
}

fn with_item(text: &str, item_context: &str) -> String {
    if item_context.is_empty() {
        text.to_owned()
    } else {
        format!("Dados atuais do item (lidos agora pelo Jarvis):\n{item_context}\n\nMensagem de {}:\n{text}", crate::profile::name())
    }
}

/// One Claude Code turn; `session` resumes the conversation's session.
pub fn ask_claude(text: &str, turn: &Turn, model: &str, session: Option<&str>, stop: &StopHandle, on: &mut dyn FnMut(Ev)) -> Result<Outcome, String> {
    let mut cmd = command(&claude_exe(), turn.mode);
    cmd.args(["-p", &with_item(text, turn.item), "--model", model, "--max-turns", "30"])
        .args(["--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--append-system-prompt", &with_personal(persona(turn.mode)), "--permission-mode", "acceptEdits"])
        .args(["--allowedTools", &ALLOWED.join(" "), "--disallowedTools", &DENIED.join(" ")]);
    // The HR assistant reads the documents the person points at (CVs, spreadsheets), not the code folders.
    cmd.args(["--add-dir", &extra_dir(turn.mode)]);
    // Lean context (measured: 34.5k → 18.2k tokens on every API call of a turn): no MCP servers, only the tools the
    // analysis uses, and no user-level settings (plugins, hooks); the team's skills come from the pack, which is a
    // Claude Code plugin.
    cmd.args(["--strict-mcp-config", "--tools", if is_rh(turn.mode) { LEAN_TOOLS_RH } else { LEAN_TOOLS }, "--settings", r#"{"effortLevel":"medium"}"#]);
    if let Some(plugin) = crate::profile::plugin_dir() {
        cmd.args(["--setting-sources", "project,local", "--plugin-dir"]).arg(plugin);
    }
    // The personal skills (%APPDATA%\Jarvis\eu\skills), a plugin of their own.
    cmd.arg("--plugin-dir").arg(crate::eu::plugin_dir());
    if let Some(s) = session {
        cmd.args(["--resume", s]);
    }
    let mut result: Option<Value> = None;
    let (mut streamed, mut new_block, mut last_session) = (false, false, String::new());
    let mut context: Option<u64> = None;
    let mut paid: Option<String> = None;
    let (err, code) = run(cmd, None, stop, |j| {
        if paid.is_none() {
            if j["type"] == "system" && j["subtype"] == "init" {
                if let Some(src) = j["apiKeySource"].as_str().filter(|s| *s != "none") {
                    paid = Some(paid_refusal("Claude Code", &format!("está ligado por chave de API ({src}), que é paga por utilização. Entra com a conta Pro/Max (claude, depois /login) e tira a chave")));
                }
            }
            let rl = &j["rate_limit_info"];
            if j["type"] == "rate_limit_event" && (rl["isUsingOverage"] == true || rl["status"] == "allowed_overage") {
                paid = Some(paid_refusal("Claude Code", "chegou ao limite do plano e ia continuar em utilização extra paga. Espera que o limite renove ou usa outro motor"));
            }
            if paid.is_some() {
                stop.stop();
                return;
            }
        }
        if let Some(s) = j["session_id"].as_str() {
            if s != last_session {
                last_session = s.to_owned();
                on(Ev::Session(s.to_owned()));
            }
        }
        match j["type"].as_str().unwrap_or_default() {
            "stream_event" => {
                let ev = &j["event"];
                if ev["type"] == "content_block_start" && ev["content_block"]["type"] == "text" {
                    new_block = true;
                }
                if ev["type"] == "content_block_delta" && ev["delta"]["type"] == "text_delta" {
                    if let Some(t) = ev["delta"]["text"].as_str().filter(|t| !t.is_empty()) {
                        on(Ev::Delta(t.to_owned(), new_block));
                        new_block = false;
                        streamed = true;
                    }
                }
            }
            "assistant" => {
                let u = &j["message"]["usage"];
                if u.is_object() {
                    context = Some(["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"].iter().map(|k| u[*k].as_u64().unwrap_or(0)).sum());
                }
                for c in j["message"]["content"].as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("text") => {
                            let t = c["text"].as_str().unwrap_or_default();
                            if !t.trim().is_empty() {
                                if !streamed {
                                    on(Ev::Delta(t.to_owned(), true));
                                }
                                on(Ev::Block(t.trim().to_owned()));
                            }
                        }
                        Some("tool_use") => on(Ev::Tool {
                            name: c["name"].as_str().unwrap_or_default().to_owned(),
                            input: c["input"].clone(),
                            id: c["id"].as_str().unwrap_or_default().to_owned(),
                        }),
                        _ => {}
                    }
                }
                streamed = false;
            }
            "user" => {
                for c in j["message"]["content"].as_array().into_iter().flatten() {
                    if c["type"] == "tool_result" {
                        on(Ev::ToolResult { id: c["tool_use_id"].as_str().unwrap_or_default().to_owned(), error: c["is_error"].as_bool().unwrap_or(false) });
                    }
                }
            }
            "result" => result = Some(j),
            _ => {}
        }
    })?;
    if let Some(p) = paid {
        return Err(p);
    }
    if stop.stopped() {
        return Ok(Outcome { stopped: true, ..Default::default() });
    }
    let r = result.unwrap_or(Value::Null);
    let said = format!("{} {err}", r["result"].as_str().unwrap_or_default()).to_lowercase();
    if said.contains("not logged in") || said.contains("please run /login") {
        return Err("O Claude Code não tem sessão iniciada. Abre um terminal, corre claude e faz /login.".into());
    }
    let model = r["modelUsage"].as_object().and_then(|m| m.keys().find(|k| !k.to_lowercase().contains("haiku")).cloned());
    let max_turns = r["subtype"] == "error_max_turns";
    if !max_turns && (r["is_error"].as_bool() == Some(true) || (r.is_null() && code.unwrap_or(1) != 0)) {
        let msg = r["result"].as_str().map(str::to_owned).filter(|s| !s.is_empty()).unwrap_or_else(|| {
            let e = err.trim();
            if e.is_empty() { format!("claude terminou com código {}", code.unwrap_or(-1)) } else { e.to_owned() }
        });
        return Err(msg);
    }
    Ok(Outcome { stopped: false, tokens: tokens(&r["usage"]), cost: r["total_cost_usd"].as_f64(), model, max_turns, context })
}

fn tokens(u: &Value) -> u64 {
    ["input_tokens", "output_tokens", "cache_creation_input_tokens"].iter().map(|k| u[*k].as_u64().unwrap_or(0)).sum::<u64>()
        + u["cache_read_input_tokens"].as_u64().or_else(|| u["cache_read_tokens"].as_u64()).unwrap_or(0)
}

/// One Gemini turn through agy. The first turn of a conversation gets the conversation so far as text; later turns
/// resume the agy conversation by id.
pub fn ask_gemini(text: &str, history: &str, turn: &Turn, conversation: Option<&str>, model: Option<&str>, prep: Option<&Prep>, stop: &StopHandle, on: &mut dyn FnMut(Ev)) -> Result<Outcome, String> {
    let exe = agy_exe().ok_or_else(|| crate::requisitos::missing_message("Antigravity"))?;
    let (rh, item_context) = (is_rh(turn.mode), turn.item);
    let mut parts: Vec<String> = Vec::new();
    if conversation.is_none() {
        parts.push(format!("{}\n\n{}", persona(turn.mode), agy_notes(turn.mode)));
        if !rh {
            parts.push(preloaded_context());
        }
    }
    // Every turn, not only the first: a rule remembered in this conversation counts from the next message.
    let personal = crate::eu::prompt_block();
    if !personal.is_empty() {
        parts.push(personal);
    }
    if !history.is_empty() {
        // The rules always go in whole; an old conversation is cut from the start.
        parts.push(format!("Conversa até agora (noutro motor):\n{}", tail_chars(history, 20000)));
    }
    if !item_context.is_empty() {
        parts.push(format!("Dados atuais do item no Azure DevOps:\n{item_context}"));
    }
    // "/<skill> …" is a Claude Code skill command; agy gets the same request in words, pointing at the skill.
    let skill = crate::profile::skill_dir().filter(|_| !rh);
    let text = match (crate::profile::skill_command(), &skill) {
        (Some(cmd), Some(dir)) if text.starts_with(&cmd) => format!("Segue o fluxo da skill {} ({}\\SKILL.md):{}", &cmd[1..], dir.display(), &text[cmd.len()..]),
        _ => text.to_owned(),
    };
    if let Some(p) = prep {
        parts.push(prep_text(p));
    }
    parts.push(format!("Mensagem atual de {}:\n{text}", crate::profile::name()));
    let prompt = parts.join("\n\n");
    let mut cmd = command(&exe, turn.mode);
    // Headless agy only accepts exact command allow-rules: only this item's preparation commands are allowed.
    // A read outside the workspace asks for permission, which print mode cannot answer: agy then ends the turn after
    // one sentence. So the skill folder is part of the workspace too. The time limit stops a run that never ends.
    cmd.args(["-p", &prompt, "--output-format", "stream-json", "--mode", "accept-edits", "--disable-slash-commands", "--print-timeout", "900s"])
        .args(["--add-dir", &extra_dir(turn.mode), "--settings", &agy_settings(prep)]);
    if let Some(d) = &skill {
        cmd.arg("--add-dir").arg(d);
    }
    if let Some(c) = conversation {
        cmd.args(["--conversation", c]);
    }
    if let Some(m) = model.filter(|m| !m.is_empty()) {
        cmd.args(["--model", m]);
    }
    let mut result: Option<Value> = None;
    let mut block = String::new();
    let mut last_step = -1i64;
    let mut noise = String::new();
    let flush = |block: &mut String, on: &mut dyn FnMut(Ev)| {
        if !block.trim().is_empty() {
            on(Ev::Block(block.trim().to_owned()));
        }
        block.clear();
    };
    let (err, code) = run(cmd, None, stop, |j| {
        if j["event"] == "init" {
            if let Some(c) = j["conversation_id"].as_str() {
                on(Ev::Session(c.to_owned()));
            }
        }
        if j["event"] == "step_update" {
            let s = &j["step_update"];
            if let Some(c) = s["conversation_id"].as_str() {
                on(Ev::Session(c.to_owned()));
            }
            let step = s["step_index"].as_i64().unwrap_or(0);
            if s["step_type"] == "agent_response" {
                if let Some(t) = s["text_delta"].as_str().filter(|t| !t.is_empty()) {
                    on(Ev::Delta(t.to_owned(), step != last_step));
                    last_step = step;
                    block.push_str(t);
                }
            } else if s["step_type"] == "tool" {
                let id = format!("agy-{step}");
                match s["state"].as_str() {
                    Some("ACTIVE") => {
                        flush(&mut block, on);
                        last_step = step;
                        let (name, input) = gemini_tool(s["tool_name"].as_str().unwrap_or_default(), &s["tool_info"]["parameters"]);
                        on(Ev::Tool { name, input, id });
                    }
                    Some(st @ ("DONE" | "ERROR")) => on(Ev::ToolResult { id, error: st == "ERROR" }),
                    _ => {}
                }
            }
        }
        if j["event"] == "result" {
            result = Some(j["result"].clone());
        }
        if j.get("event").is_none() {
            noise.push_str(&j.to_string());
        }
    })?;
    flush(&mut block, on);
    if stop.stopped() {
        return Ok(Outcome { stopped: true, ..Default::default() });
    }
    let all = format!("{err}\n{noise}");
    let Some(r) = result else {
        if Regex::new(r"(?i)sign.?in|log.?in|authenticat|unauthori").unwrap().is_match(&all) {
            return Err("O Antigravity CLI não tem sessão iniciada. Corre agy num terminal e entra com a tua conta Google.".into());
        }
        let last = err.trim().lines().last().unwrap_or_default().to_owned();
        return Err(if last.is_empty() { format!("agy terminou com código {}", code.unwrap_or(-1)) } else { last });
    };
    if r["status"].as_str().is_some_and(|s| s != "SUCCESS") {
        return Err(r["error"]["message"].as_str().unwrap_or("O Gemini não conseguiu responder.").to_owned());
    }
    let label = model.and_then(|m| agy_models().into_iter().find(|(id, _)| id == m).map(|(_, l)| l)).unwrap_or_else(|| "Antigravity".into());
    Ok(Outcome { stopped: false, tokens: tokens(&r["usage"]), cost: None, model: Some(label), max_turns: false, context: None })
}

/* ---------- Codex (ChatGPT) ---------- */

/// The Codex CLI: the one inside the Codex app when it is installed (kept up to date by the app), else the npm one.
/// The answer is kept (asking the package manager takes a moment, and the screens ask every frame);
/// `forget_codex` makes the next call look again, after the person installs it.
pub fn codex_exe() -> Option<PathBuf> {
    let mut cached = CODEX.lock().unwrap();
    if let Some(found) = cached.as_ref() {
        return found.clone();
    }
    let found = {
        // WindowsApps cannot be listed by a normal process; the package manager says where the app lives.
        let app = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-AppxPackage OpenAI.Codex).InstallLocation"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().last().unwrap_or_default().trim().to_owned())
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("app").join("resources").join("codex.exe"))
            .filter(|p| p.exists());
        let npm = dirs::config_dir().map(|d| d.join(r"npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\codex\codex.exe"));
        app.or(npm.filter(|p| p.exists()))
    };
    *cached = Some(found.clone());
    found
}

static CODEX: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

pub fn forget_codex() {
    *CODEX.lock().unwrap() = None;
}

pub fn codex_installed() -> bool {
    codex_exe().is_some()
}

/// Whether an installed engine has the person's account signed in, without spending any usage: `claude auth status`,
/// `codex login status`, and Antigravity's saved Google sign-in. None when it cannot tell (not installed, or the
/// command failed); the conversation still says what to do then.
pub fn signed_in(engine: &str) -> Option<bool> {
    let quiet = |mut c: Command| {
        for (k, _) in std::env::vars() {
            let up = k.to_uppercase();
            if up.starts_with("CLAUDE") || up.starts_with("ANTHROPIC") {
                c.env_remove(k);
            }
        }
        c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output().ok()
    };
    match engine {
        "claude" => {
            if !claude_installed() {
                return None;
            }
            let mut c = Command::new(claude_exe());
            c.args(["auth", "status"]);
            let o = quiet(c)?;
            let j: Value = serde_json::from_slice(&o.stdout).ok()?;
            j["loggedIn"].as_bool()
        }
        "codex" => {
            let mut c = Command::new(codex_exe()?);
            c.args(["login", "status"]);
            let o = quiet(c)?;
            let said = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).to_lowercase();
            Some(o.status.success() && said.contains("logged in") && !said.contains("not logged in"))
        }
        "antigravity" => {
            agy_exe()?;
            let creds = fs::read_to_string(home().join(".gemini").join("oauth_creds.json")).unwrap_or_default();
            Some(creds.contains("refresh_token"))
        }
        _ => None,
    }
}

/// Opens a terminal with the engine's sign-in (the person signs in there with their own account).
pub fn open_sign_in(command: &str) {
    let _ = Command::new("cmd.exe").args(["/c", "start", "Jarvis - iniciar sessão", "powershell.exe", "-NoExit", "-Command", command]).creation_flags(CREATE_NO_WINDOW).spawn();
}

/// Models the Codex CLI offers to this account (`codex debug models`, the listed ones), read once in the background.
pub fn codex_models() -> Vec<(String, String)> {
    static LIST: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();
    let cell = LIST.get_or_init(|| {
        thread::spawn(|| {
            let Some(exe) = codex_exe() else { return };
            let Ok(o) = Command::new(exe).args(["debug", "models"]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output() else { return };
            let Ok(j) = serde_json::from_slice::<Value>(&o.stdout) else { return };
            let list = j["models"].as_array().or(j.as_array()).cloned().unwrap_or_default();
            let found: Vec<(String, String)> = list
                .iter()
                .filter(|m| m["visibility"] == "list")
                .filter_map(|m| Some((m["slug"].as_str()?.to_owned(), m["display_name"].as_str().unwrap_or(m["slug"].as_str()?).to_owned())))
                .collect();
            if let Some(cell) = LIST.get() {
                *cell.lock().unwrap() = found;
            }
        });
        Mutex::new(Vec::new())
    });
    cell.lock().unwrap().clone()
}

/// The model `model = "…"` in ~/.codex/config.toml, when there is one.
fn codex_config_model() -> Option<String> {
    let text = fs::read_to_string(home().join(".codex").join("config.toml")).ok()?;
    let v: toml::Table = toml::from_str(&text).ok()?;
    v.get("model")?.as_str().map(str::to_owned)
}

/// The models Codex offers this account (`codex debug models`, visibility "list"), in its order; empty when this
/// Codex cannot say (an old CLI).
fn codex_listed(exe: &Path) -> Vec<String> {
    let Ok(o) = Command::new(exe).args(["debug", "models"]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output() else { return Vec::new() };
    let Ok(j) = serde_json::from_slice::<Value>(&o.stdout) else { return Vec::new() };
    let list = j["models"].as_array().or(j.as_array()).cloned().unwrap_or_default();
    list.iter().filter(|m| m["visibility"] == "list").filter_map(|m| m["slug"].as_str().map(str::to_owned)).collect()
}

/// For "Predefinido": None keeps the config's model; Some(first listed) when the config names a model this account
/// does not have, which Codex would refuse ("not supported when using Codex with a ChatGPT account").
fn codex_default_fallback(exe: &Path) -> Option<String> {
    let configured = codex_config_model()?;
    let listed = codex_listed(exe);
    (!listed.is_empty() && !listed.contains(&configured)).then(|| listed[0].clone())
}

/// One Codex turn (`codex exec --json`): the first turn of a conversation gets the rules and the conversation so far;
/// later turns resume the Codex thread. Codex works in its sandbox with write access to the projects folder only.
pub fn ask_codex(text: &str, history: &str, turn: &Turn, thread: Option<&str>, model: Option<&str>, stop: &StopHandle, on: &mut dyn FnMut(Ev)) -> Result<Outcome, String> {
    let (rh, item_context) = (is_rh(turn.mode), turn.item);
    let exe = codex_exe().ok_or_else(|| crate::requisitos::missing_message("Codex"))?;
    let login = Command::new(&exe).args(["login", "status"]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    let login = format!("{}{}", String::from_utf8_lossy(&login.stdout), String::from_utf8_lossy(&login.stderr));
    if !login.contains("ChatGPT") {
        return Err(if login.to_lowercase().contains("api key") {
            paid_refusal("Codex", "está ligado por chave de API, que é paga por utilização. Corre codex logout e depois codex login com a tua conta ChatGPT")
        } else {
            "O Codex não tem sessão iniciada. Abre a app Codex (ou corre codex login) e entra com a tua conta ChatGPT.".into()
        });
    }
    let mut parts: Vec<String> = Vec::new();
    if thread.is_none() {
        parts.push(format!("{}\n\n{}", persona(turn.mode), codex_notes(turn.mode)));
        if !rh {
            parts.push(preloaded_context());
        }
    }
    // Every turn, not only the first: a rule remembered in this conversation counts from the next message.
    parts.push(crate::eu::prompt_block());
    if !history.is_empty() {
        parts.push(format!("Conversa até agora (noutro motor):\n{}", tail_chars(history, 20000)));
    }
    if !item_context.is_empty() {
        parts.push(format!("Dados atuais do item no board:\n{item_context}"));
    }
    let skill = crate::profile::skill_dir().filter(|_| !rh);
    let text = match (crate::profile::skill_command(), &skill) {
        (Some(cmd), Some(dir)) if text.starts_with(&cmd) => format!("Segue o fluxo da skill {} ({}\\SKILL.md):{}", &cmd[1..], dir.display(), &text[cmd.len()..]),
        _ => text.to_owned(),
    };
    parts.push(format!("Mensagem atual de {}:\n{text}", crate::profile::name()));
    let prompt = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n");

    let mut cmd = command(&exe, turn.mode);
    cmd.arg("exec");
    if let Some(t) = thread {
        cmd.args(["resume", t]);
    }
    // dev writes in the projects folder and reaches the board; rh writes in the person's folders and has no network
    // (nothing leaves the PC through a command, as with Claude Code, which gets no shell in rh).
    let roots = format!("sandbox_workspace_write.writable_roots=['{}']", extra_dir(turn.mode));
    let network = format!("sandbox_workspace_write.network_access={}", !rh);
    cmd.args(["--json", "--skip-git-repo-check"])
        .args(["-c", "sandbox_mode=\"workspace-write\"", "-c", &roots, "-c", &network])
        // The user's notify hook (e.g. the Codex app's) would run after every Jarvis turn.
        .args(["-c", "notify=[]", "-c", "model_reasoning_effort=\"medium\""])
        // Only the ChatGPT subscription login, never an API key (pay-per-use).
        .args(["-c", "forced_login_method=\"chatgpt\""]);
    // "Predefinido" uses the model in the person's config.toml, which may be one the ChatGPT account no longer has
    // (an old gpt-5.5 left there): then the first model Codex lists for this account.
    let model = model.filter(|m| !m.is_empty()).map(str::to_owned).or_else(|| codex_default_fallback(&exe));
    if let Some(m) = &model {
        cmd.args(["-m", m]);
    }
    // The elevated Windows sandbox needs a console: with Jarvis's hidden one, its commands never return.
    cmd.args(["-c", "windows.sandbox=\"unelevated\""]).arg("-").stdin(Stdio::piped());

    let mut usage: Option<Value> = None;
    let mut failure: Option<String> = None;
    let mut noise = String::new();
    let (err, code) = run(cmd, Some(prompt), stop, |j| {
        let item = &j["item"];
        let id = item["id"].as_str().map(|i| format!("codex-{i}")).unwrap_or_default();
        match (j["type"].as_str().unwrap_or_default(), item["type"].as_str().unwrap_or_default()) {
            ("thread.started", _) => {
                if let Some(t) = j["thread_id"].as_str() {
                    on(Ev::Session(t.to_owned()));
                }
            }
            ("item.completed", "agent_message") => {
                if let Some(t) = item["text"].as_str().filter(|t| !t.trim().is_empty()) {
                    on(Ev::Delta(t.to_owned(), true));
                    on(Ev::Block(t.trim().to_owned()));
                }
            }
            ("item.started", "command_execution") => on(Ev::Tool { name: "Bash".into(), input: serde_json::json!({ "command": codex_command(item["command"].as_str().unwrap_or_default()) }), id }),
            ("item.completed", "command_execution") => on(Ev::ToolResult { id, error: item["exit_code"].as_i64().unwrap_or(0) != 0 || item["status"] == "failed" }),
            ("item.started" | "item.completed", "file_change") => {
                if j["type"] == "item.started" || !item["changes"].is_null() {
                    let path = item["changes"][0]["path"].as_str().unwrap_or_default().to_owned();
                    on(Ev::Tool { name: "Edit".into(), input: serde_json::json!({ "file_path": path }), id: id.clone() });
                }
                if j["type"] == "item.completed" {
                    on(Ev::ToolResult { id, error: item["status"] == "failed" });
                }
            }
            ("item.completed", "error") => noise.push_str(item["message"].as_str().unwrap_or_default()),
            ("turn.completed", _) => usage = Some(j["usage"].clone()),
            ("turn.failed", _) => failure = j["error"]["message"].as_str().map(str::to_owned),
            ("error", _) => failure = failure.clone().or_else(|| j["message"].as_str().map(str::to_owned)),
            _ => {}
        }
    })?;
    if stop.stopped() {
        return Ok(Outcome { stopped: true, ..Default::default() });
    }
    let Some(u) = usage else {
        let all = format!("{} {err} {noise}", failure.clone().unwrap_or_default());
        if Regex::new(r"(?i)not logged in|run codex login|401 unauthorized").unwrap().is_match(&all) {
            return Err("O Codex não tem sessão iniciada. Abre a app Codex (ou corre codex login) e entra com a tua conta ChatGPT.".into());
        }
        if let Some(f) = failure {
            // The error comes as JSON text from the server ({"detail": …}).
            let f = serde_json::from_str::<Value>(&f).ok().and_then(|v| v["detail"].as_str().or(v["error"]["message"].as_str()).map(str::to_owned)).unwrap_or(f);
            return Err(if f.contains("newer version of Codex") { format!("{f} Atualiza a app Codex ou o CLI (npm i -g @openai/codex).") } else { f });
        }
        let last = err.trim().lines().last().unwrap_or_default().to_owned();
        return Err(if last.is_empty() { format!("O Codex terminou com código {}", code.unwrap_or(-1)) } else { last });
    };
    let input = u["input_tokens"].as_u64().unwrap_or(0);
    let label = model.as_deref().and_then(|m| codex_models().into_iter().find(|(id, _)| id == m).map(|(_, l)| l)).unwrap_or_else(|| "Codex".into());
    Ok(Outcome { stopped: false, tokens: input + u["output_tokens"].as_u64().unwrap_or(0), cost: None, model: Some(label), max_turns: false, context: Some(input) })
}

/// Codex runs commands through PowerShell (`"…pwsh.exe" -Command '…'`): show only the command.
fn codex_command(c: &str) -> String {
    match c.split_once(" -Command ") {
        Some((_, rest)) => rest.trim().trim_matches('\'').trim_matches('"').to_owned(),
        None => c.to_owned(),
    }
}

fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n { s.to_owned() } else { s.chars().skip(count - n).collect() }
}

/* ---------- marks, hand-off and tool labels ---------- */

fn mark_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[(ABRIR_NO_CLAUDE|IMPLEMENTAR:([^\]\n]+)|LEMBRAR:([^\]\n]+)|TAREFA:([^\]\n]+))\]\]").unwrap())
}

/// The personal tasks the reply asked for ([[TAREFA: título | AAAA-MM-DD]] lines): (title, due date if any).
pub fn read_tasks(text: &str) -> Vec<(String, Option<String>)> {
    mark_re()
        .captures_iter(text)
        .filter_map(|c| c.get(4))
        .filter_map(|m| {
            let mut parts = m.as_str().split('|');
            let title = parts.next()?.trim().to_owned();
            let due = parts.next().map(str::trim).filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()).map(str::to_owned);
            (!title.is_empty()).then_some((title, due))
        })
        .collect()
}

/// An email the reply drafted ([[EMAIL]] … [[/EMAIL]]).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Email {
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
}

/// Splits a reply into text and email drafts, in order: Err(text) for the prose, Ok(email) for each draft. An email
/// still arriving (no [[/EMAIL]] yet) stays text until it closes.
pub fn split_emails(text: &str) -> Vec<Result<Email, String>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?s)\[\[EMAIL\]\](.*?)\[\[/EMAIL\]\]").unwrap());
    let mut out = Vec::new();
    let mut last = 0;
    for m in re.captures_iter(text) {
        let whole = m.get(0).unwrap();
        let before = text[last..whole.start()].trim();
        if !before.is_empty() {
            out.push(Err(before.to_owned()));
        }
        out.push(Ok(parse_email(&m[1])));
        last = whole.end();
    }
    let rest = text[last..].trim();
    if !rest.is_empty() {
        out.push(Err(rest.to_owned()));
    }
    out
}

fn parse_email(block: &str) -> Email {
    let mut e = Email::default();
    let (head, body) = match block.split_once("\n---") {
        Some((h, b)) => (h, b.trim_start_matches('-')),
        None => ("", block),
    };
    for line in head.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim().to_owned();
        match k.trim().to_lowercase().as_str() {
            "para" | "to" => e.to = v,
            "cc" => e.cc = if v.starts_with('(') { String::new() } else { v },
            "assunto" | "subject" => e.subject = v,
            _ => {}
        }
    }
    e.body = body.trim().to_owned();
    e
}

/// Opens the draft in the classic Outlook of this PC, ready to send (never sends it); without it, the default mail
/// app through a mailto: link.
pub fn open_email(e: &Email) {
    let q = |s: &str| s.replace('\'', "''");
    let script = format!(
        "$o = New-Object -ComObject Outlook.Application; $m = $o.CreateItem(0); $m.To = '{}'; $m.CC = '{}'; $m.Subject = '{}'; $m.Body = '{}'; $m.Display()",
        q(&e.to), q(&e.cc), q(&e.subject), q(&e.body)
    );
    let ok = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~@".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
        open(&format!("mailto:{}?cc={}&subject={}&body={}", enc(&e.to), enc(&e.cc), enc(&e.subject), enc(&e.body)));
    }
}

/// The draft as text, to paste anywhere.
pub fn email_text(e: &Email) -> String {
    let mut t = format!("Para: {}\n", e.to);
    if !e.cc.is_empty() {
        t.push_str(&format!("Cc: {}\n", e.cc));
    }
    format!("{t}Assunto: {}\n\n{}", e.subject, e.body)
}

/// The rules the reply asked Jarvis to remember ([[LEMBRAR: …]] lines), in order.
pub fn read_remember(text: &str) -> Vec<String> {
    mark_re().captures_iter(text).filter_map(|c| c.get(3)).map(|m| m.as_str().trim().to_owned()).filter(|r| !r.is_empty()).collect()
}

/// (open in the Claude app, worktree to implement in)
pub fn read_marks(text: &str) -> (bool, Option<String>) {
    let (mut browser, mut implement) = (false, None);
    for c in mark_re().captures_iter(text) {
        if &c[1] == "ABRIR_NO_CLAUDE" {
            browser = true;
        } else if let Some(p) = c.get(2) {
            implement = Some(p.as_str().trim().to_owned());
        }
    }
    (browser, implement)
}

pub fn strip_marks(text: &str) -> String {
    mark_re().replace_all(text, "").trim_end().to_owned()
}

/// Claude CLI errors that mean "no usage left right now"; anything else is a real error and stays with Claude.
pub fn is_limit_error(msg: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)usage limit|limit reached|rate.?limit|\b429\b|quota|overloaded|out of (extra )?usage|credit balance|limite de utiliza|limite do plano|hit your usage|usage cap|resource.?exhausted").unwrap()).is_match(msg)
}

fn is_session_id(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap()).is_match(s)
}

/// Gives the session its title (the same line the Claude app writes on rename) and returns the link that opens it there.
pub fn hand_off(session: Option<&str>, title: &str) -> Option<String> {
    let s = session.filter(|s| is_session_id(s))?;
    let folder: String = crate::profile::main_dir().to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let file = home().join(".claude").join("projects").join(folder).join(format!("{s}.jsonl"));
    if file.exists() && !title.trim().is_empty() {
        let title: String = title.trim().chars().take(100).collect();
        let line = serde_json::json!({ "type": "custom-title", "customTitle": title, "sessionId": s });
        if let Ok(mut f) = OpenOptions::new().append(true).open(&file) {
            let _ = writeln!(f, "{line}");
        }
    }
    Some(format!("claude://resume?session={s}"))
}

/// agy tool names and parameters, mapped onto the Claude ones the chat knows how to label.
fn gemini_tool(name: &str, params: &Value) -> (String, Value) {
    let has = |w: &str| name.contains(w);
    let mapped = if has("command") {
        "Bash"
    } else if has("grep") || has("search") {
        "Grep"
    } else if has("find") || has("list") || has("glob") {
        "Glob"
    } else if has("replace") || has("edit") {
        "Edit"
    } else if has("write") || has("create") {
        "Write"
    } else if has("read") || has("view") {
        "Read"
    } else {
        name
    };
    let pick = |keys: &[&str]| keys.iter().find_map(|k| params[*k].as_str()).map(str::to_owned);
    let mut input = params.clone();
    if !input.is_object() {
        input = serde_json::json!({});
    }
    input["file_path"] = pick(&["Path", "AbsolutePath", "TargetFile", "FilePath", "file_path", "path"]).into();
    input["command"] = pick(&["CommandLine", "command"]).into();
    input["pattern"] = pick(&["Query", "Pattern", "pattern"]).into();
    (mapped.to_owned(), input)
}

pub fn tool_label(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().unwrap_or_default();
    let label = match name {
        "Bash" => {
            let c = s("command");
            let azdo = |re: &str| Regex::new(re).unwrap().is_match(c);
            if azdo(r"azdo\.mjs\s+(show|next|refined)") {
                "A ler o Azure DevOps…"
            } else if azdo(r"azdo\.mjs\s+download") {
                "A descarregar um anexo…"
            } else if azdo(r"azdo\.mjs\s+(apply|done|set|comment)") {
                "A atualizar o Azure DevOps…"
            } else if c.contains("ng build") {
                "A compilar…"
            } else if c.contains("git worktree add") {
                "A criar o worktree…"
            } else if c.contains("mklink") {
                "A ligar o node_modules…"
            } else {
                "A correr um comando…"
            }
        }
        "Write" | "Edit" if s("file_path").contains(r"\.jarvis\") || s("file_path").contains("/.jarvis/") => "A escrever o pacote de passagem…",
        "Read" => "A ler código…",
        "Grep" => "A procurar no código…",
        "Glob" => "A procurar ficheiros…",
        "Edit" => "A alterar um ficheiro…",
        "Write" => "A escrever um ficheiro…",
        "Agent" => "A investigar…",
        "Skill" => "A seguir o fluxo da skill…",
        "TodoWrite" => "A atualizar a lista de passos…",
        "ToolSearch" => "A preparar ferramentas…",
        _ => "A trabalhar…",
    };
    label.to_owned()
}

/// Short, specific detail for the tool lines (file name, command, pattern).
pub fn tool_detail(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().unwrap_or_default().to_owned();
    let file = |p: String| Path::new(&p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(p);
    let cut = |t: String| if t.chars().count() > 90 { format!("{}…", t.chars().take(89).collect::<String>()) } else { t };
    match name {
        "Read" | "Edit" | "Write" | "NotebookEdit" => file(s("file_path")),
        "Bash" => cut(s("command").split_whitespace().collect::<Vec<_>>().join(" ")),
        "Grep" => {
            let p = s("path");
            cut(if p.is_empty() { s("pattern") } else { format!("{} em {}", s("pattern"), file(p)) })
        }
        "Glob" => cut(s("pattern")),
        "Skill" => s("skill"),
        "Agent" => cut(s("description")),
        "WebFetch" => cut(s("url")),
        _ => String::new(),
    }
}

/* ---------- item text for the prompt ---------- */

/// The pack's board script `show <id>` output (cached 10 minutes): Gemini has no shell, and Claude saves a tool call
/// on the first turn.
pub fn item_context(id: i64) -> String {
    static CACHE: OnceLock<Mutex<HashMap<i64, (Instant, String)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, text)) = cache.lock().unwrap().get(&id) {
        if at.elapsed() < Duration::from_secs(600) {
            return text.clone();
        }
    }
    let Some(cli) = crate::profile::skill_cli() else { return String::new() };
    let mut cmd = Command::new("node");
    crate::profile::child_env(&mut cmd);
    let out = cmd.arg(cli).args(["show", &id.to_string()]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).output();
    let text = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).chars().take(9000).collect::<String>(),
        _ => String::new(),
    };
    if !text.is_empty() {
        cache.lock().unwrap().insert(id, (Instant::now(), text.clone()));
    }
    text
}

/// Keeps the hand-off package (.jarvis/) out of git in that worktree, through the repository's info/exclude.
pub fn exclude_jarvis_dir(worktree: &str) {
    let Ok(o) = Command::new("git").args(["-C", worktree, "rev-parse", "--git-common-dir"]).creation_flags(CREATE_NO_WINDOW).output() else { return };
    if !o.status.success() {
        return;
    }
    let out = String::from_utf8_lossy(&o.stdout).trim().to_owned();
    let common = if Path::new(&out).is_absolute() { PathBuf::from(out) } else { Path::new(worktree).join(out) };
    let file = common.join("info").join("exclude");
    let current = fs::read_to_string(&file).unwrap_or_default();
    if !current.lines().any(|l| matches!(l.trim(), ".jarvis" | ".jarvis/" | "/.jarvis" | "/.jarvis/")) {
        let _ = fs::create_dir_all(file.parent().unwrap());
        let _ = fs::write(&file, format!("{}\n.jarvis/\n", current.trim_end()));
    }
}

/// Opens a URL (also claude://) or a folder with the Windows shell (ShellExecute, like a double click): explorer.exe
/// with a long protocol link can open a folder window instead.
pub fn open(target: &str) {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(target));
    // SAFETY: valid NUL-terminated wide strings; null for parameters and directory.
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    if (r as isize) <= 32 {
        let _ = Command::new("explorer.exe").arg(target).spawn();
    }
}

/// What agy would otherwise spend its first steps reading: the repository rules and the board skill's flow. Sent
/// once, with the first turn of an agy conversation; it stays in that conversation after.
fn preloaded_context() -> String {
    let files: Vec<PathBuf> = [Some(crate::profile::main_dir().join("CLAUDE.md")), crate::profile::skill_dir().map(|d| d.join("SKILL.md"))].into_iter().flatten().filter(|p| p.is_file()).collect();
    if files.is_empty() {
        return String::new();
    }
    let body = files.iter().map(|p| format!("--- {} ---\n{}", p.display(), fs::read_to_string(p).unwrap_or_default().trim())).collect::<Vec<_>>().join("\n\n");
    format!("Já carregados pelo Jarvis (não voltes a ler estes ficheiros):\n\n{body}")
}

/* ---------- preparation by agy ---------- */

/// An item conversation that may be prepared from Antigravity: the item and its branch (from the team convention).
#[derive(Clone)]
pub struct Prep {
    pub id: i64,
    pub branch: String,
}

pub fn plan_file(id: i64) -> PathBuf {
    crate::settings::data_dir().join("planos").join(format!("{id}.json"))
}

/// agy allow-rules only match a command line exactly (no prefixes), so every preparation command of this item is
/// written out in full: these, and nothing else, can run. A reset, a checkout or a push stay blocked.
pub fn prep_commands(p: &Prep) -> Vec<String> {
    use crate::profile;
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let (id, branch, base) = (p.id, &p.branch, profile::branches().base);
    let mut out = Vec::new();
    if let Some(cli) = profile::skill_cli() {
        let (cli, plan) = (slash(&cli), slash(&plan_file(id)));
        out.push(format!("node {cli} show {id}"));
        out.push(format!("node {cli} apply {id} {plan}"));
    }
    for r in profile::repos() {
        let (repo, wt) = (profile::repo_dir(&r), profile::worktree_dir(&r, id));
        let (repo_s, wt_s) = (slash(&repo), slash(&wt));
        out.push(format!("git -C {repo_s} fetch origin {base}"));
        out.push(format!("git -C {repo_s} worktree add -b {branch} {wt_s} origin/{base}"));
        if r.link_node_modules {
            out.push(format!(r"cmd /c mklink /J {}\node_modules {}\node_modules", wt.display(), repo.display()));
        }
    }
    out
}

fn agy_settings(prep: Option<&Prep>) -> String {
    let allow: Vec<String> = prep.map(prep_commands).unwrap_or_default().into_iter().map(|c| format!("command({c})")).collect();
    serde_json::json!({ "permissions": { "allow": allow } }).to_string()
}

fn prep_text(p: &Prep) -> String {
    let list = prep_commands(p).iter().map(|c| format!("- `{c}`")).collect::<Vec<_>>().join("\n");
    format!(
        "Comandos de preparação autorizados para o #{} (só estes correm; escreve-os exatamente assim, sem mudar nem acrescentar nada):\n{list}\n\n{}Corre só os do lado que o plano toca ({}), e só depois de um sim claro de {} nesta conversa.",
        p.id,
        match crate::profile::skill_dir() {
            Some(d) if crate::profile::skill_cli().is_some() => format!("O plano para o apply vai em {} (no formato da skill {}). ", plan_file(p.id).display(), d.file_name().unwrap_or_default().to_string_lossy()),
            _ => String::new(),
        },
        crate::profile::repos().iter().map(|r| format!("{} = {}", r.side, if r.label.is_empty() { &r.dir } else { &r.label })).collect::<Vec<_>>().join(", "),
        crate::profile::name()
    )
}
