//! Updates: the latest GitHub release, checked at start-up and every 6 hours. "Instalar" downloads its
//! Jarvis-Setup.exe and runs it silently: the setup closes Jarvis, installs the new version (keeping the options
//! chosen the first time) and opens it again.
//!
//! Only the installed Jarvis (%LOCALAPPDATA%\Programs\Jarvis) offers updates; a build from the source does not.
//! JARVIS_UPDATE_FROM=<version> pretends to be that version and to be installed, to test the whole flow.

use std::{
    fs,
    io,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use serde_json::Value;

use crate::app::{post, AppEvent};

const LATEST: &str = "https://api.github.com/repos/severino100/jarvis/releases/latest";
const SETUP: &str = "Jarvis-Setup.exe";
const EVERY: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Downloading { version: String },
    Failed(String),
}

fn cell() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn state() -> State {
    cell().lock().map(|s| s.clone()).unwrap_or_default()
}

fn set(s: State) {
    if let Ok(mut c) = cell().lock() {
        *c = s;
    }
    post(AppEvent::Repaint);
}

/// The version this Jarvis says it is (JARVIS_UPDATE_FROM overrides it for tests).
pub fn current() -> String {
    std::env::var("JARVIS_UPDATE_FROM").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned())
}

/// Whether this exe is the installed one, the only one that updates itself.
pub fn installed() -> bool {
    if std::env::var("JARVIS_UPDATE_FROM").is_ok() {
        return true;
    }
    let (Ok(exe), Some(local)) = (std::env::current_exe(), dirs::data_local_dir()) else { return false };
    exe.parent().is_some_and(|d| d == local.join("Programs").join("Jarvis"))
}

fn parts(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

/// a is newer than b (0.1.10 > 0.1.9; missing parts count as 0).
fn newer(a: &str, b: &str) -> bool {
    let (a, b) = (parts(a), parts(b));
    let n = a.len().max(b.len());
    let at = |v: &Vec<u64>, i| v.get(i).copied().unwrap_or(0);
    (0..n).map(|i| at(&a, i).cmp(&at(&b, i))).find(|o| o.is_ne()) == Some(std::cmp::Ordering::Greater)
}

/// The latest release: (version without the v, URL of its Jarvis-Setup.exe).
fn latest() -> Result<(String, String), String> {
    let mut r = ureq::get(LATEST)
        .header("User-Agent", "jarvis")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Não consegui ver se há versão nova ({e})."))?;
    let v: Value = r.body_mut().read_json().map_err(|e| format!("Resposta estranha do GitHub ({e})."))?;
    let tag = v["tag_name"].as_str().unwrap_or_default().trim_start_matches(['v', 'V']).to_owned();
    let url = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(SETUP)))
        .and_then(|x| x["browser_download_url"].as_str())
        .unwrap_or_default()
        .to_owned();
    if tag.is_empty() || url.is_empty() {
        return Err("O último release não tem o Jarvis-Setup.exe.".into());
    }
    Ok((tag, url))
}

/// One check now; tells the person once per version (a notice that opens Definições).
pub fn check_now() {
    if !installed() || matches!(state(), State::Checking | State::Downloading { .. }) {
        return;
    }
    set(State::Checking);
    thread::spawn(|| match latest() {
        Ok((version, url)) if newer(&version, &current()) => {
            let first = crate::settings::with_state(|st| {
                let first = st.update_seen != version;
                st.update_seen = version.clone();
                first
            });
            if first {
                crate::notify::notify("Há uma versão nova do Jarvis", &format!("A {version}. Carrega em Instalar, no fundo do menu."), AppEvent::Show);
            }
            set(State::Available { version, url });
        }
        Ok(_) => set(State::UpToDate),
        Err(e) => set(State::Failed(e)),
    });
}

/// Checks at start-up (after a minute, so it does not compete with the board) and every 6 hours.
pub fn start() {
    if !installed() {
        return;
    }
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(if std::env::var("JARVIS_UPDATE_FROM").is_ok() { 2 } else { 60 }));
        loop {
            if !matches!(state(), State::Available { .. } | State::Downloading { .. }) {
                check_now();
            }
            thread::sleep(EVERY);
        }
    });
}

fn download(url: &str, to: &PathBuf) -> Result<(), String> {
    let mut r = ureq::get(url).header("User-Agent", "jarvis").call().map_err(|e| format!("A descarga falhou ({e})."))?;
    let tmp = to.with_extension("partial");
    let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    io::copy(&mut r.body_mut().as_reader(), &mut f).map_err(|e| format!("A descarga falhou ({e})."))?;
    drop(f);
    fs::rename(&tmp, to).map_err(|e| e.to_string())
}

/// Downloads the new setup and runs it silently; Jarvis closes so the setup can replace it and open it again.
pub fn install() {
    let State::Available { version, url } = state() else { return };
    set(State::Downloading { version: version.clone() });
    thread::spawn(move || {
        let to = std::env::temp_dir().join(format!("Jarvis-Setup-{version}.exe"));
        let run = download(&url, &to).and_then(|_| {
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            Command::new(&to)
                .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/relancar=1"])
                .creation_flags(DETACHED_PROCESS)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Não consegui abrir o instalador ({e})."))
        });
        match run {
            Ok(()) => post(AppEvent::Quit),
            Err(e) => set(State::Failed(e)),
        }
    });
}

/// Dev aid (`jarvis --update-test`): reads the latest release and downloads its setup, without running it.
pub fn self_test() -> Result<String, String> {
    let (version, url) = latest()?;
    let to = std::env::temp_dir().join(format!("Jarvis-Setup-{version}-teste.exe"));
    download(&url, &to)?;
    let size = fs::metadata(&to).map(|m| m.len()).unwrap_or(0);
    Ok(format!("tenho {} · último {version} · mais nova: {} · instalado: {} · {} ({size} bytes)", current(), newer(&version, &current()), installed(), to.display()))
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn versions() {
        assert!(newer("0.1.1", "0.1.0"));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.1.1", "0.1.1"));
        assert!(!newer("0.1.0", "0.1.1"));
        assert!(!newer("0.1", "0.1.0"));
    }
}
