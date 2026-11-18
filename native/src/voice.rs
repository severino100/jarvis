//! Voice: microphone with speech detection (rodio), Whisper (vendor/whisper) to text, quick local answers about the
//! board (same rules as voice.mjs), Claude for everything else in the same conversations as the written chat, and
//! Piper (vendor/piper, pt-PT "tugão") reading the answers aloud. The panel is drawn by the app (`panel`).

use crate::{
    app::{post, AppEvent, View},
    azdo::{Dashboard, Item},
    board::Board,
    chat::{self, Chats, VoiceMsg},
    settings::jarvis_dir,
};
use eframe::egui;
use rodio::{microphone::MicrophoneBuilder, Source};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const RATE: u32 = 16_000;
const SPEECH_RMS: f32 = 0.02;
const SILENCE_RMS: f32 = 0.012;
const END_SILENCE: Duration = Duration::from_millis(1300);
const MAX_UTTERANCE: Duration = Duration::from_secs(20);
/// Nothing said for this long ends the conversation (the microphone is not left open forever).
const IDLE_END: Duration = Duration::from_secs(90);
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Default)]
pub struct Line {
    /// me | jarvis | note
    pub who: &'static str,
    pub text: String,
}

#[derive(Default)]
pub struct VoiceState {
    pub on: bool,
    pub visible: bool,
    /// idle | listening | thinking | speaking
    pub phase: &'static str,
    pub status: String,
    pub level: f32,
    pub log: Vec<Line>,
    pub chat_id: Option<String>,
}

#[derive(Clone)]
pub struct Voice {
    st: Arc<Mutex<VoiceState>>,
    stop: Arc<AtomicBool>,
    ctx: egui::Context,
    chats: Chats,
    board: Board,
}

fn whisper() -> Option<(PathBuf, PathBuf)> {
    let v = jarvis_dir().join("vendor");
    let exe = [v.join("whisper").join("Release").join("whisper-cli.exe"), v.join("whisper").join("whisper-cli.exe")].into_iter().find(|p| p.exists())?;
    let model = v.join("models").join("ggml-small.bin");
    model.exists().then_some((exe, model))
}

fn piper() -> Option<(PathBuf, PathBuf)> {
    let v = jarvis_dir().join("vendor");
    let (exe, model) = (v.join("piper").join("piper.exe"), v.join("voices").join("pt_PT-tugao-medium.onnx"));
    (exe.exists() && model.exists()).then_some((exe, model))
}

impl Voice {
    pub fn new(ctx: egui::Context, chats: Chats, board: Board) -> Self {
        Self { st: Arc::default(), stop: Arc::default(), ctx, chats, board }
    }

    pub fn with<R>(&self, f: impl FnOnce(&VoiceState) -> R) -> R {
        f(&self.st.lock().unwrap())
    }

    fn set(&self, f: impl FnOnce(&mut VoiceState)) {
        f(&mut self.st.lock().unwrap());
        self.ctx.request_repaint();
    }

    fn status(&self, phase: &'static str, text: &str) {
        self.set(|s| {
            s.phase = phase;
            s.status = text.to_owned();
        });
    }

    fn log(&self, who: &'static str, text: &str) {
        self.set(|s| {
            s.log.push(Line { who, text: text.to_owned() });
            let n = s.log.len();
            if n > 40 {
                s.log.drain(..n - 40);
            }
        });
    }

    /// Ctrl+Alt+J / "Falar com o Jarvis": starts a spoken conversation, or ends the one running.
    pub fn toggle(&self) {
        if self.with(|s| s.on) {
            self.end();
            return;
        }
        if whisper().is_none() {
            crate::notify::notify("Voz indisponível", "Falta o Whisper em vendor/ (whisper-cli.exe e ggml-small.bin).", AppEvent::Show);
            return;
        }
        self.stop.store(false, Ordering::SeqCst);
        self.set(|s| {
            s.on = true;
            s.visible = true;
            s.log.clear();
        });
        let me = self.clone();
        thread::spawn(move || me.session());
    }

    pub fn end(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(id) = self.with(|s| s.chat_id.clone()) {
            self.chats.set_voice_hook(&id, None);
        }
        self.set(|s| {
            s.on = false;
            s.phase = "idle";
            s.status = "Conversa terminada".into();
            s.level = 0.0;
        });
    }

    pub fn hide(&self) {
        self.end();
        self.set(|s| s.visible = false);
    }

    /// Written text in the voice panel goes to the same conversation, without being read aloud.
    pub fn write(&self, text: &str) {
        let id = self.conversation(None);
        self.chats.send(&id, text, false, chat::engine());
        self.log("me", text);
    }

    pub fn chat_id(&self) -> Option<String> {
        self.with(|s| s.chat_id.clone())
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn session(&self) {
        let mut convo = Convo::default();
        let mut idle_since = Instant::now();
        while !self.stopped() {
            self.status("listening", "A ouvir…");
            let audio = match self.listen() {
                Ok(Some(a)) => a,
                Ok(None) => {
                    if self.stopped() {
                        return;
                    }
                    if idle_since.elapsed() > IDLE_END {
                        self.log("note", "Sem conversa há algum tempo: terminei.");
                        self.end();
                        thread::sleep(Duration::from_millis(1500));
                        if !self.with(|s| s.on) {
                            self.set(|s| s.visible = false);
                        }
                        return;
                    }
                    continue;
                }
                Err(e) => {
                    self.log("jarvis", &e);
                    self.end();
                    return;
                }
            };
            idle_since = Instant::now();
            self.status("thinking", "A perceber…");
            let heard = match transcribe(&audio) {
                Ok(t) => t,
                Err(e) => {
                    self.log("jarvis", &format!("Tive um problema a ouvir: {e}"));
                    continue;
                }
            };
            if heard.is_empty() || (heard.starts_with('(') && heard.ends_with(')')) || (heard.starts_with('[') && heard.ends_with(']')) {
                continue;
            }
            self.log("me", &heard);
            let r = self.board.with(|b| understand(&heard, b.data.as_ref(), &mut convo));
            if let Some(v) = r.view {
                post(AppEvent::Go(v));
            }
            if let Some(id) = r.open {
                post(AppEvent::OpenUrl(crate::azdo::item_url(id)));
            }
            if let (Some(c), Some(m)) = (r.music, crate::music::current()) {
                m.send(c);
            }
            if r.refresh {
                self.board.refresh();
            }
            if r.brain {
                convo.engaged = true;
                self.brain_turn(&heard, r.focus);
                idle_since = Instant::now();
                continue;
            }
            self.log("jarvis", &r.say);
            self.speak(&r.say);
            if r.end {
                self.end();
                thread::sleep(Duration::from_millis(1500));
                if !self.with(|s| s.on) {
                    self.set(|s| s.visible = false);
                }
                return;
            }
        }
    }

    /// The conversation for this voice turn: the item's (claimed on the way), the one already in use, or a new one.
    fn conversation(&self, focus: Option<i64>) -> String {
        let usable = |id: &str| self.chats.with(|cs| cs.iter().any(|c| c.id == id && !c.handed_off));
        if let Some(item) = focus {
            let existing = self.chats.with(|cs| cs.iter().find(|c| c.item_id == Some(item) && !c.handed_off).map(|c| c.id.clone()));
            let id = existing.unwrap_or_else(|| {
                let title = self.board.with(|b| {
                    b.data.as_ref().and_then(|d| d.now.iter().chain(&d.review).chain(&d.refined).find(|i| i.id == item).map(|i| format!("#{item} {}", i.title)))
                });
                self.chats.create(Some(title.unwrap_or_else(|| format!("#{item}"))), Some(item), None)
            });
            crate::app::claim_in_background(&self.chats, &self.board, &id, item);
            self.set(|s| s.chat_id = Some(id.clone()));
            return id;
        }
        if let Some(id) = self.chat_id().filter(|id| usable(id)) {
            return id;
        }
        let id = self.chats.create(None, None, None);
        self.set(|s| s.chat_id = Some(id.clone()));
        id
    }

    fn brain_turn(&self, text: &str, focus: Option<i64>) {
        let id = self.conversation(focus);
        let (tx, rx) = mpsc::channel();
        self.chats.set_voice_hook(&id, Some(tx));
        self.status("thinking", "A pensar…");
        if !self.chats.send(&id, text, true, chat::engine()) {
            self.chats.set_voice_hook(&id, None);
            let say = "Ainda estou a trabalhar na resposta anterior. Dá-me um momento.";
            self.log("jarvis", say);
            self.speak(say);
            return;
        }
        let mut tool = String::from("A pensar…");
        while let Ok(m) = rx.recv() {
            if self.stopped() {
                break;
            }
            match m {
                VoiceMsg::Block(t) => {
                    self.log("jarvis", &t);
                    self.speak(&t);
                    self.status("thinking", &tool);
                }
                VoiceMsg::Tool(label) => {
                    tool = label;
                    self.status("thinking", &tool);
                }
                VoiceMsg::Done { error, kind } => {
                    match kind.as_deref() {
                        Some("implement") => self.speak("Está tudo preparado. Escolhe no Jarvis onde queres implementar: Claude Code ou Antigravity."),
                        Some("handoff") => self.speak("Abri esta conversa no Claude para veres no browser."),
                        _ => {}
                    }
                    if let Some(e) = error {
                        self.log("jarvis", &e);
                        self.speak("Tive um problema. Está escrito na janela.");
                    }
                    break;
                }
            }
        }
        self.chats.set_voice_hook(&id, None);
    }

    /// One utterance: waits for speech, records until a pause, returns 16 kHz mono. None when nothing was said.
    fn listen(&self) -> Result<Option<Vec<f32>>, String> {
        let mic = MicrophoneBuilder::new()
            .default_device()
            .and_then(|b| b.default_config())
            .map_err(|e| format!("Não encontrei o microfone: {e}"))?
            .prefer_channel_counts([1.try_into().unwrap(), 2.try_into().unwrap()])
            .prefer_sample_rates([16_000.try_into().unwrap(), 48_000.try_into().unwrap()])
            .open_stream()
            .map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
        let rate = mic.sample_rate().get();
        let ch = mic.channels().get() as usize;
        let frame = (rate as usize / 50).max(1);
        let started = Instant::now();
        let mut pre: std::collections::VecDeque<Vec<f32>> = Default::default();
        let mut chunks: Vec<Vec<f32>> = Vec::new();
        let (mut heard_at, mut quiet_since, mut loud) = (None::<Instant>, None::<Instant>, 0u32);
        let mut buf = Vec::with_capacity(frame);
        let mut it = mic.into_iter();
        loop {
            if self.stopped() {
                return Ok(None);
            }
            buf.clear();
            for _ in 0..frame {
                let mut sum = 0.0;
                for _ in 0..ch {
                    sum += it.next().ok_or("O microfone deixou de responder.")?;
                }
                buf.push(sum / ch as f32);
            }
            let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
            self.set(|s| s.level = (rms * 8.0).min(0.6));
            let now = Instant::now();
            if rms > SPEECH_RMS {
                heard_at.get_or_insert(now);
                loud += 1;
                quiet_since = None;
            } else if heard_at.is_some() && rms < SILENCE_RMS {
                quiet_since.get_or_insert(now);
            }
            if heard_at.is_some() {
                if chunks.is_empty() {
                    chunks.extend(pre.drain(..));
                }
                chunks.push(buf.clone());
            } else {
                // ~0.5 s before the first loud frame, so the start of the sentence isn't clipped.
                pre.push_back(buf.clone());
                if pre.len() > 25 {
                    pre.pop_front();
                }
                if started.elapsed() > Duration::from_secs(15) {
                    return Ok(None);
                }
            }
            if let Some(h) = heard_at {
                if quiet_since.is_some_and(|q| now - q > END_SILENCE) || now - h > MAX_UTTERANCE {
                    break;
                }
            }
        }
        self.set(|s| s.level = 0.0);
        // ~0.25 s of loud audio: a click or a cough is not a sentence.
        if loud < 12 {
            return Ok(None);
        }
        let mono: Vec<f32> = chunks.concat();
        Ok(Some(resample(&mono, rate, RATE)))
    }

    fn speak(&self, text: &str) {
        if self.stopped() || text.trim().is_empty() {
            return;
        }
        self.status("speaking", "A falar…");
        let Some((exe, model)) = piper() else { return };
        let out = std::env::temp_dir().join(format!("jarvis-say-{}.wav", std::process::id()));
        let child = Command::new(exe)
            .args(["-m"])
            .arg(model)
            .arg("-f")
            .arg(&out)
            .args(["--length_scale", "0.95"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(NO_WINDOW)
            .spawn();
        let Ok(mut child) = child else { return };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(for_speech(text).as_bytes());
        }
        if !child.wait().is_ok_and(|s| s.success()) {
            return;
        }
        let Ok(file) = fs::File::open(&out) else { return };
        let Ok(sink) = rodio::DeviceSinkBuilder::open_default_sink() else { return };
        if let Ok(player) = rodio::play(sink.mixer(), std::io::BufReader::new(file)) {
            while !player.empty() {
                if self.stopped() {
                    player.stop();
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        }
        let _ = fs::remove_file(&out);
    }
}

fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (x.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let j = p as usize;
            let f = (p - j as f64) as f32;
            let a = x[j];
            let b = *x.get(j + 1).unwrap_or(&a);
            a + (b - a) * f
        })
        .collect()
}

const PROMPT: &str = "Jarvis, o que tenho para fazer? Pega no 8125. Abre o 8420. O que disse o QA? O que há no Refined? Refined, Planned, In Progress, Code Review, To Test, sprint, horas, Work Done.";

fn transcribe(audio: &[f32]) -> Result<String, String> {
    let (exe, model) = whisper().ok_or("O Whisper não está instalado em vendor/.")?;
    let file = std::env::temp_dir().join(format!("jarvis-heard-{}.wav", std::process::id()));
    {
        let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&file, spec).map_err(|e| e.to_string())?;
        for s in audio {
            w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
    }
    let threads = thread::available_parallelism().map(|n| n.get().saturating_sub(1).clamp(2, 8)).unwrap_or(4);
    let out = Command::new(exe)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&file)
        .args(["-l", "pt", "-nt", "-np", "-t", &threads.to_string(), "--prompt", PROMPT])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&file);
    // whisper-cli on Windows sometimes writes accented letters in the ANSI code page instead of UTF-8.
    let text = String::from_utf8(out.stdout.clone()).unwrap_or_else(|_| out.stdout.iter().map(|&b| b as char).collect());
    let text = regex::Regex::new(r"\[[^\]]*\]").unwrap().replace_all(&text, " ");
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn for_speech(text: &str) -> String {
    let t = regex::Regex::new(r"\bQA\b").unwrap().replace_all(text, "quê á");
    let p = crate::profile::get();
    let t = if p.spoken_name.trim().is_empty() || p.name.trim().is_empty() {
        t
    } else {
        regex::Regex::new(&format!(r"\b{}\b", regex::escape(p.name.trim()))).unwrap().replace_all(&t, p.spoken_name.trim()).into_owned().into()
    };
    let t = regex::Regex::new(r"\bPBI\b").unwrap().replace_all(&t, "pê bê í");
    let t = regex::Regex::new(r"#(\d+)").unwrap().replace_all(&t, "$1");
    t.replace('/', " ou ").replace('…', ".").replace(['"', '“', '”'], "")
}

/* ---------- understanding (port of voice.mjs) ---------- */

#[derive(Default)]
struct Convo {
    last_id: Option<i64>,
    order: Vec<i64>,
    engaged: bool,
}

#[derive(Default)]
struct Reply {
    say: String,
    brain: bool,
    focus: Option<i64>,
    open: Option<i64>,
    view: Option<View>,
    end: bool,
    refresh: bool,
    music: Option<crate::music::Cmd>,
}

fn say(s: impl Into<String>) -> Reply {
    Reply { say: s.into(), ..Default::default() }
}

fn norm(s: &str) -> String {
    let folded: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() || c == '_' || c.is_whitespace() => c,
            _ => ' ',
        })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whisper writes "8125", "8 125" or "8.125"; glue digit groups back together.
fn find_id(raw: &str, known: &[i64]) -> Option<i64> {
    let re = regex::Regex::new(r"\d[\d\s.,]*\d|\d").unwrap();
    let cands: Vec<i64> = re.find_iter(raw).filter_map(|m| m.as_str().chars().filter(char::is_ascii_digit).collect::<String>().parse().ok()).filter(|n| *n >= 100).collect();
    cands.iter().find(|n| known.contains(n)).or(cands.first()).copied()
}

/// "Grupos - Equipas - Quando temos um utilizador, que..." → "Grupos, Equipas: Quando temos um utilizador"
fn topic(title: &str) -> String {
    let parts: Vec<&str> = title.split(" - ").map(str::trim).filter(|p| !p.is_empty()).collect();
    let head: Vec<&str> = if parts.len() > 1 { parts[..(parts.len() - 1).min(2)].to_vec() } else { Vec::new() };
    let rest = parts[head.len()..].join(" - ");
    let clause = rest.split([',', '.', ';', ':']).next().unwrap_or("").trim();
    let words: Vec<&str> = clause.split_whitespace().collect();
    let clause = if words.len() > 12 { words[..12].join(" ") } else { clause.to_owned() };
    if head.is_empty() { clause } else { format!("{}: {clause}", head.join(", ")) }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn sentence(t: &str) -> String {
    let t = t.trim();
    if t.ends_with(['.', '!', '?', '…']) { t.to_owned() } else { format!("{t}.") }
}

fn short_text(t: &str) -> String {
    if t.chars().count() <= 180 {
        return sentence(t);
    }
    let cut: String = t.chars().take(180).collect();
    let cut = cut.rsplit_once(char::is_whitespace).map(|(a, _)| a.to_owned()).unwrap_or(cut);
    sentence(&format!("{cut}…"))
}

fn first_name(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or(s)
}

fn describe_back(it: &Item) -> String {
    let b = it.back.as_ref().unwrap();
    let by = if b.by.is_empty() { String::new() } else { format!(", pelo {}", first_name(&b.by)) };
    match b.comments.last().filter(|c| !c.text.is_empty()) {
        Some(c) => format!("O {} voltou do QA{by}: {}", it.id, short_text(&c.text)),
        None => format!("O {} voltou do QA{by}.", it.id),
    }
}

fn kind_name(k: &str) -> String {
    if k == "Bug" { "um bug".into() } else { format!("um {k}") }
}

fn understand(text: &str, d: Option<&Dashboard>, ctx: &mut Convo) -> Reply {
    let r = understand_inner(text, d, ctx);
    if let Some(id) = r.focus.or(r.open) {
        ctx.last_id = Some(id);
    }
    r
}

fn understand_inner(text: &str, d: Option<&Dashboard>, ctx: &mut Convo) -> Reply {
    let t = norm(text);
    if t.is_empty() {
        return say("Não ouvi nada.");
    }
    let first = d.map(|d| first_name(&d.me.name).to_owned()).unwrap_or_default();
    let end_re = regex::Regex::new(r"\b(obrigad|ate logo|ate ja|adeus|xau|tchau|chega|e tudo|mais nada|pode(s)? ir|desliga|terminar|termina (a conversa|aqui|por aqui)|fecha (a conversa|isso|aqui))").unwrap();
    if end_re.is_match(&t) && t.split(' ').count() <= 6 {
        return Reply { end: true, ..say(format!("De nada{}. Até já.", if first.is_empty() { String::new() } else { format!(", {first}") })) };
    }
    // Once Claude is part of the conversation, answers like "sim" or "usa duas horas" belong to it.
    if ctx.engaged {
        return Reply { brain: true, ..Default::default() };
    }
    let Some(d) = d else { return Reply { brain: true, ..Default::default() } };
    let all: Vec<&Item> = d.now.iter().chain(&d.review).chain(&d.refined).collect();
    let known: Vec<i64> = all.iter().map(|i| i.id).collect();
    let has = |words: &[&str]| words.iter().any(|w| regex::Regex::new(&format!(r"\b{w}")).unwrap().is_match(&t));
    let mut id = find_id(text, &known);
    if id.is_none() && ctx.last_id.is_some() && has(&["seguinte", "proximo", "outro", "depois"]) {
        let last = ctx.last_id.unwrap();
        let order: Vec<i64> = if ctx.order.contains(&last) {
            ctx.order.clone()
        } else if d.now.iter().any(|i| i.id == last) {
            d.now.iter().map(|i| i.id).collect()
        } else if d.review.iter().any(|i| i.id == last) {
            d.review.iter().map(|i| i.id).collect()
        } else {
            d.refined.iter().map(|i| i.id).collect()
        };
        match order.iter().position(|x| *x == last).and_then(|p| order.get(p + 1)) {
            Some(n) => id = Some(*n),
            None => return say("Não há mais nenhum depois desse."),
        }
    } else if id.is_none() && ctx.last_id.is_some() && has(&["esse", "este", "dele", "desse", "nesse", "deste", "isso", "nele", "o mesmo"]) {
        id = ctx.last_id;
    }
    let item = id.and_then(|i| all.iter().find(|x| x.id == i).copied());

    if has(&["ajuda", "o que podes", "que sabes"]) {
        return say("Posso dizer-te o que tens para fazer, o que o QA disse de um item, as horas do sprint e o que está em revisão ou no Refined, e abro itens no Azure DevOps. Se disseres \"começa o 8125\", trabalho contigo nesse item: explico o plano, faço-te perguntas, crio as tarefas e implemento depois do teu sim. Para terminar, diz obrigado.");
    }
    // Music, through the Windows media controls (Spotify, Apple Music…), answered here without the Claude.
    if has(&["musica", "cancao", "faixa", "som "]) || has(&["proxima", "anterior"]) && !has(&["item", "tarefa", "reuniao"]) {
        use crate::music::Cmd;
        let now = crate::music::current().map(|m| m.now()).unwrap_or_default();
        if has(&["o que esta a tocar", "que musica", "quem canta", "que som"]) {
            return say(if now.title.is_empty() { "Não está nada a tocar.".to_owned() } else { format!("Está a tocar {}, de {}, no {}.", now.title, now.artist, now.app) });
        }
        if now.title.is_empty() && !has(&["abre", "abrir", "liga"]) {
            return say("Não está nada a tocar. Põe música no Spotify ou no Apple Music primeiro.");
        }
        if has(&["abre", "abrir", "liga"]) {
            crate::music::open_app(if has(&["apple"]) { "apple" } else { "spotify" });
            return say(if has(&["apple"]) { "A abrir o Apple Music." } else { "A abrir o Spotify." });
        }
        let (cmd, text) = if has(&["proxima", "seguinte", "salta", "passa"]) {
            (Cmd::Next, "Próxima.")
        } else if has(&["anterior", "volta"]) {
            (Cmd::Previous, "Anterior.")
        } else if has(&["pausa", "para", "parar", "silencio"]) {
            (Cmd::Pause, "Em pausa.")
        } else if has(&["toca", "continua", "retoma", "play"]) {
            (Cmd::Play, "A tocar.")
        } else {
            (Cmd::Toggle, if now.playing { "Em pausa." } else { "A tocar." })
        };
        return Reply { music: Some(cmd), ..say(text) };
    }
    if has(&["atualiza", "refresca"]) {
        return Reply { refresh: true, ..say("A atualizar o board.") };
    }
    if let Some(i) = id {
        if has(&["abre", "abrir", "mostra o", "ver o"]) {
            return Reply { open: Some(i), ..say(if item.is_some() { format!("A abrir o {i}.") } else { format!("Não encontrei o {i} nas tuas listas, mas vou abri-lo na mesma.") }) };
        }
        if has(&["pega", "pegar", "comeca", "comecar", "inicia", "trabalhar no", "fazer o", "vamos ao", "vamos a"]) {
            return Reply { brain: true, focus: Some(i), ..Default::default() };
        }
        if has(&["qa", "disse", "comentario", "comentou", "devolv"]) {
            let Some(it) = item else { return say(format!("Não encontrei o {i} nas tuas listas.")) };
            if it.back.is_some() {
                return Reply { focus: Some(i), ..say(describe_back(it)) };
            }
            if let Some(c) = &it.last_comment {
                return Reply { focus: Some(i), ..say(format!("O último comentário no {i} é do {}: {}", first_name(&c.by), short_text(if c.text.is_empty() { "só uma imagem" } else { &c.text }))) };
            }
            return Reply { focus: Some(i), ..say(format!("O {i} não tem comentários.")) };
        }
        let Some(it) = item else { return say(format!("Não encontrei o {i} nas tuas listas. Posso abri-lo se disseres \"abre o {i}\".")) };
        let tasks = if it.tasks.is_empty() { String::new() } else { format!(" Tem {}.", plural(it.tasks.len(), "tarefa", "tarefas")) };
        let back = if it.back.is_some() { format!(" {}", describe_back(it)) } else { String::new() };
        return Reply { focus: Some(i), ..say(format!("O {i} é {} em {}: {}.{tasks}{back}", kind_name(&it.kind), it.column, topic(&it.title))) };
    }
    if has(&["hora", "work done", "estimativa"]) {
        let est: f64 = d.tasks.iter().filter_map(|x| x.estimate).sum();
        let done: f64 = d.tasks.iter().filter_map(|x| x.work_done).sum();
        let missing: Vec<_> = d.tasks.iter().filter(|x| (x.state == "Done" && x.work_done.is_none_or(|w| w <= 0.0)) || x.estimate.is_none_or(|e| e <= 0.0) || x.activity.is_none()).collect();
        let tail = match missing.first() {
            Some(m) => format!(" {} campos por preencher, a começar pela {}.", plural(missing.len(), "tarefa tem", "tarefas têm"), m.id),
            None => " Está tudo preenchido.".into(),
        };
        return Reply { view: Some(View::Hours), ..say(format!("Neste sprint tens {est} horas estimadas e {done} feitas, em {}.{tail}", plural(d.tasks.len(), "tarefa", "tarefas"))) };
    }
    if has(&["dias", "sprint", "acaba", "falta"]) {
        let left = crate::ui_kit::parse(&d.sprint.finish).map(|f| (f.date_naive() - chrono::Local::now().date_naive()).num_days().max(0)).unwrap_or(0);
        return say(if left > 0 { format!("Faltam {} para o fim do {}.", plural(left as usize, "dia", "dias"), d.sprint.name) } else { format!("Hoje é o último dia do {}.", d.sprint.name) });
    }
    if has(&["refin", "refaind", "rifain", "raifain", "refind", "livre", "backlog"]) {
        let top = d.refined.iter().take(3).map(|i| format!("o {}, {}", i.id, topic(&i.title))).collect::<Vec<_>>().join("; ");
        ctx.order = d.refined.iter().map(|i| i.id).collect();
        if let Some(f) = d.refined.first() {
            ctx.last_id = Some(f.id);
        }
        let warn = if d.now.is_empty() { String::new() } else { format!(" Mas ainda tens {} em curso ou planeados, por isso o Refined fica para depois.", plural(d.now.len(), "item", "itens")) };
        return Reply {
            view: Some(View::Refined),
            ..say(if d.refined.is_empty() { "O Refined está vazio.".into() } else { format!("Há {} livres no Refined. Os primeiros são {top}.{warn}", plural(d.refined.len(), "item", "itens")) })
        };
    }
    if has(&["revis", "review", "testar", "to test", "qa"]) {
        let cr = d.review.iter().filter(|i| i.column == "Code Review").count();
        return Reply { view: Some(View::Review), ..say(format!("Tens {} em Code Review e {} à espera ou em teste de QA.", plural(cr, "item", "itens"), d.review.len() - cr)) };
    }
    if has(&["para fazer", "o que tenho", "que ha para", "trabalho para", "pego a seguir", "proximo item", "bom dia", "boa tarde", "ola"]) {
        let greet = if has(&["bom dia"]) {
            format!("Bom dia, {first}. ")
        } else if has(&["boa tarde"]) {
            format!("Boa tarde, {first}. ")
        } else if has(&["ola"]) {
            format!("Olá, {first}. ")
        } else {
            String::new()
        };
        if d.now.is_empty() {
            return Reply { view: Some(View::Refined), ..say(format!("{greet}Não tens nada em curso nem planeado. Há {} livres no Refined.", plural(d.refined.len(), "item", "itens"))) };
        }
        let prog = d.now.iter().filter(|i| i.column == "In Progress").count();
        let backs: Vec<&Item> = d.now.iter().filter(|i| i.back.is_some()).collect();
        let next = backs.first().copied().unwrap_or(&d.now[0]);
        ctx.order = backs.iter().map(|i| i.id).chain(d.now.iter().filter(|i| i.back.is_none()).map(|i| i.id)).collect();
        let backs_text = if backs.is_empty() { String::new() } else { format!(" {}", backs.iter().map(|i| describe_back(i)).collect::<Vec<_>>().join(" ")) };
        return Reply {
            view: Some(View::Now),
            focus: Some(next.id),
            ..say(format!(
                "{greet}Tens {} em curso e {}.{backs_text} Sugiro começares pelo {}: {}.",
                plural(prog, "item", "itens"),
                plural(d.now.len() - prog, "planeado", "planeados"),
                next.id,
                topic(&next.title)
            ))
        };
    }
    Reply { brain: true, ..Default::default() }
}

/// Dev aid (`jarvis --voice-test "texto"`): Piper writes the text to a WAV, Whisper reads it back. Nothing is played.
pub fn self_test(text: &str) -> Result<String, String> {
    let (exe, model) = piper().ok_or("Falta a Piper em vendor/.")?;
    let out = std::env::temp_dir().join("jarvis-selftest.wav");
    let mut child = Command::new(exe).arg("-m").arg(model).arg("-f").arg(&out).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(NO_WINDOW).spawn().map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(for_speech(text).as_bytes()).map_err(|e| e.to_string())?;
    }
    child.wait().map_err(|e| e.to_string())?;
    let mut r = hound::WavReader::open(&out).map_err(|e| e.to_string())?;
    let rate = r.spec().sample_rate;
    let samples: Vec<f32> = r.samples::<i16>().filter_map(Result::ok).map(|s| s as f32 / 32768.0).collect();
    let _ = fs::remove_file(&out);
    transcribe(&resample(&samples, rate, RATE))
}

/// Dev aid (`jarvis --mic-test`): opens the microphone for a second and reports what it hears.
pub fn mic_test() -> Result<String, String> {
    let mic = MicrophoneBuilder::new().default_device().and_then(|b| b.default_config()).map_err(|e| e.to_string())?.open_stream().map_err(|e| e.to_string())?;
    let (rate, ch) = (mic.sample_rate().get(), mic.channels().get());
    let n = rate as usize * ch as usize;
    let v: Vec<f32> = mic.into_iter().take(n).collect();
    let rms = (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt();
    Ok(format!("{rate} Hz, {ch} canais, {} amostras, rms {rms:.4}", v.len()))
}

impl Voice {
    /// Dev aid for screenshots (JARVIS_SHOT_VOICE=1): the panel with a short made-up exchange, microphone off.
    pub fn demo(&self) {
        self.set(|s| {
            s.visible = true;
            s.phase = "speaking";
            s.status = "A falar…".into();
            s.log = vec![
                Line { who: "me", text: "Jarvis, o que tenho para fazer?".into() },
                Line { who: "jarvis", text: "Tens 0 itens em curso e 2 planeados. Sugiro começares pelo 8779: listagem de ordens de corte.".into() },
            ];
        });
    }
}
