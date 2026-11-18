//! Settings and "already seen" board state, in %APPDATA%\Jarvis\state-native.json. The first run copies them from
//! the Electron Jarvis (state.json), so both versions can run side by side without overwriting each other.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub accent: String,
    pub poll_minutes: u32,
    pub notify: bool,
    /// A message from Jarvis in the item's conversation when something changes on the board or in a PR.
    pub inform_chat: bool,
    pub meeting_reminder: bool,
    /// auto | claude | gemini
    pub model: String,
    /// Claude Code model for the analysis in Jarvis: auto (Sonnet, Opus when asked) | sonnet | opus | haiku
    pub claude_model: String,
    /// Claude Code model for the implementation session opened in the worktree: sonnet | opus | haiku
    pub implement_model: String,
    /// Which default changes were already applied to an existing settings file.
    // Missing in older files: must read as 0 (not the struct default), so the migration below runs.
    #[serde(default)]
    pub defaults_version: u32,
    /// Antigravity model id, or auto
    pub agy_model: String,
    /// auto | 100 | 110 | 125
    pub ui_scale: String,
    /// Sound when Jarvis opens: original | file | off
    pub startup_sound: String,
    /// The audio file for startup_sound = file (mp3, wav, flac)
    pub startup_sound_file: String,
    /// Favourite playlists: one per line, "Nome | link" (Spotify or Apple Music)
    pub music_links: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            accent: "cyan".into(),
            poll_minutes: 5,
            notify: true,
            inform_chat: true,
            meeting_reminder: true,
            model: "auto".into(),
            claude_model: "opus".into(),
            implement_model: "sonnet".into(),
            defaults_version: DEFAULTS_VERSION,
            agy_model: "auto".into(),
            ui_scale: "auto".into(),
            startup_sound: "original".into(),
            startup_sound_file: String::new(),
            music_links: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SeenPr {
    pub comments: Vec<String>,
    pub votes: HashMap<String, i64>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub initialized: bool,
    pub seen_back: HashMap<String, String>,
    pub seen_comment: HashMap<String, i64>,
    pub seen_now: Vec<i64>,
    pub seen_column: HashMap<String, String>,
    pub seen_pr: HashMap<String, SeenPr>,
    pub settings: Settings,
    /// The Electron conversations (chats.json) were copied into chats-native.json.
    pub imported_electron_chats: bool,
}

const DEFAULTS_VERSION: u32 = 1;

pub fn data_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Jarvis")
}

/// The Jarvis folder (scripts/, vendor/ with Whisper and Piper, docs/). The Rust build lives in its native/ folder.
pub fn jarvis_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn file() -> PathBuf {
    data_dir().join("state-native.json")
}

fn store() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        let own = fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str::<State>(&s).ok());
        let state = own.unwrap_or_else(|| {
            // First run: the Electron settings and what it has already seen (so old news is not repeated).
            let old = fs::read_to_string(data_dir().join("state.json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or_default();
            let mut s: State = serde_json::from_value(old.clone()).unwrap_or_default();
            s.settings = serde_json::from_value(old["settings"].clone()).unwrap_or_default();
            s
        });
        let mut state = state;
        // Defaults changed on 2026-11-18: Opus to analyse, Sonnet to implement (an existing "auto" moves to Opus).
        if state.settings.defaults_version < 1 {
            if state.settings.claude_model.is_empty() || state.settings.claude_model == "auto" {
                state.settings.claude_model = "opus".into();
            }
            if state.settings.implement_model.is_empty() {
                state.settings.implement_model = "sonnet".into();
            }
            state.settings.defaults_version = DEFAULTS_VERSION;
        }
        Mutex::new(state)
    })
}

pub fn load() -> Settings {
    store().lock().unwrap().settings.clone()
}

pub fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = store().lock().unwrap();
    let r = f(&mut s);
    if let Ok(text) = serde_json::to_string_pretty(&*s) {
        let _ = fs::create_dir_all(data_dir());
        let _ = fs::write(file(), text);
    }
    r
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    with_state(|s| f(&mut s.settings));
}
