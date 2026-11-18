//! Music through the Windows media controls (the same as the keyboard media keys): whatever is playing in Spotify,
//! Apple Music or any other player. Read every second on a background thread; play/pause, next and previous go to the
//! same thread. No account or login is involved.

use std::os::windows::process::CommandExt;
use std::{
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status},
    Storage::Streams::DataReader,
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
};

#[derive(Clone, Default)]
pub struct NowPlaying {
    /// Spotify, Apple Music…; empty when nothing is playing anywhere.
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position: f32,
    pub duration: f32,
    /// When `position` was read, to move the progress bar between reads.
    pub read_at: Option<Instant>,
    /// Album art as RGBA, with its size; `cover_key` changes with the track.
    pub cover: Option<(Vec<u8>, [usize; 2])>,
    pub cover_key: String,
}

impl NowPlaying {
    pub fn position_now(&self) -> f32 {
        let extra = if self.playing { self.read_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0) } else { 0.0 };
        (self.position + extra).min(self.duration.max(0.0))
    }
}

#[derive(Clone, Copy)]
pub enum Cmd {
    Toggle,
    Next,
    Previous,
    Play,
    Pause,
}

#[derive(Clone)]
pub struct Music {
    state: Arc<Mutex<NowPlaying>>,
    tx: Sender<Cmd>,
}

fn app_name(id: &str) -> String {
    let low = id.to_lowercase();
    if low.contains("spotify") {
        "Spotify".into()
    } else if low.contains("applemusic") || low.contains("itunes") {
        "Apple Music".into()
    } else if low.contains("chrome") || low.contains("msedge") || low.contains("firefox") {
        "Browser".into()
    } else {
        id.split(['!', '.']).next().unwrap_or(id).to_owned()
    }
}

fn secs(t: windows::Foundation::TimeSpan) -> f32 {
    t.Duration as f32 / 10_000_000.0
}

fn read_cover(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<(Vec<u8>, [usize; 2])> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 8 << 20 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(160, 160).into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), [w as usize, h as usize]))
}

impl Music {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let state: Arc<Mutex<NowPlaying>> = Arc::default();
        let st = state.clone();
        thread::spawn(move || {
            // SAFETY: WinRT needs the thread initialised once before any call.
            unsafe {
                let _ = RoInitialize(RO_INIT_MULTITHREADED);
            }
            let Ok(manager) = Manager::RequestAsync().and_then(|op| op.join()) else { return };
            loop {
                if let Ok(cmd) = rx.recv_timeout(Duration::from_millis(1000)) {
                    if let Ok(s) = manager.GetCurrentSession() {
                        let op = match cmd {
                            Cmd::Toggle => s.TryTogglePlayPauseAsync(),
                            Cmd::Next => s.TrySkipNextAsync(),
                            Cmd::Previous => s.TrySkipPreviousAsync(),
                            Cmd::Play => s.TryPlayAsync(),
                            Cmd::Pause => s.TryPauseAsync(),
                        };
                        let _ = op.and_then(|o| o.join());
                        thread::sleep(Duration::from_millis(250));
                    }
                }
                let mut now = NowPlaying::default();
                if let Ok(s) = manager.GetCurrentSession() {
                    now.app = s.SourceAppUserModelId().map(|h| app_name(&h.to_string())).unwrap_or_default();
                    if let Ok(props) = s.TryGetMediaPropertiesAsync().and_then(|o| o.join()) {
                        now.title = props.Title().map(|h| h.to_string()).unwrap_or_default();
                        now.artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
                        now.cover_key = format!("{}|{}", now.title, now.artist);
                        let same = st.lock().unwrap().cover_key == now.cover_key;
                        now.cover = if same { st.lock().unwrap().cover.clone() } else { read_cover(&props) };
                    }
                    now.playing = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|p| p == Status::Playing);
                    if let Ok(t) = s.GetTimelineProperties() {
                        now.position = t.Position().map(secs).unwrap_or(0.0);
                        now.duration = t.EndTime().map(secs).unwrap_or(0.0);
                        now.read_at = Some(Instant::now());
                    }
                }
                *st.lock().unwrap() = now;
                ctx.request_repaint();
            }
        });
        let me = Self { state, tx };
        let _ = CURRENT.set(me.clone());
        me
    }

    pub fn now(&self) -> NowPlaying {
        self.state.lock().unwrap().clone()
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

/// "3:07"
pub fn clock(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Opens Spotify or Apple Music: the app when it is installed (its link protocol is registered), the web player if not.
pub fn open_app(which: &str) {
    let registered = |proto: &str| {
        std::process::Command::new("reg.exe")
            .args(["query", &format!(r"HKCR{proto}")])
            .creation_flags(0x0800_0000)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let target = match which {
        "spotify" => if registered("spotify") { "spotify:" } else { "https://open.spotify.com" },
        _ => if registered("itms") { "itms://music.apple.com" } else { "https://music.apple.com" },
    };
    crate::brain::open(target);
}

/// Favourite playlists from Definições: one per line, "Nome | link".
pub fn favorites() -> Vec<(String, String)> {
    crate::settings::load()
        .music_links
        .lines()
        .filter_map(|l| {
            let (n, u) = l.split_once('|')?;
            let (n, u) = (n.trim(), u.trim());
            (!n.is_empty() && (u.starts_with("http") || u.starts_with("spotify:") || u.starts_with("itms"))).then(|| (n.to_owned(), u.to_owned()))
        })
        .collect()
}

static CURRENT: std::sync::OnceLock<Music> = std::sync::OnceLock::new();

/// The music player Jarvis started, for the voice commands.
pub fn current() -> Option<Music> {
    CURRENT.get().cloned()
}
