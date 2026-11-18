//! Windows integration: one instance only, the AppUserModelID (taskbar grouping and toasts), and starting with
//! Windows (HKCU Run, value APP_ID).

use std::{os::windows::process::CommandExt, process::Command};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
    System::Threading::CreateMutexW,
    UI::{
        Shell::SetCurrentProcessExplicitAppUserModelID,
        WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW},
    },
};

pub const APP_ID: &str = "jarvis.desktop";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const NO_WINDOW: u32 = 0x0800_0000;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// False when another Rust Jarvis is already running: that one is brought to the front instead.
pub fn single_instance() -> bool {
    let name = wide("Local\\jarvis.desktop.single");
    // SAFETY: plain Win32 calls with valid, NUL-terminated strings. The mutex handle stays open for the whole run.
    unsafe {
        let _ = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        let w = FindWindowW(std::ptr::null(), wide("Jarvis").as_ptr());
        if !w.is_null() {
            ShowWindow(w, SW_SHOW);
            ShowWindow(w, SW_RESTORE);
            SetForegroundWindow(w);
        }
    }
    false
}

pub fn set_app_id() {
    // SAFETY: valid NUL-terminated string.
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(wide(APP_ID).as_ptr());
    }
}

fn reg(args: &[&str]) -> (bool, String) {
    match Command::new("reg.exe").args(args).creation_flags(NO_WINDOW).output() {
        Ok(o) => (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned()),
        Err(_) => (false, String::new()),
    }
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// On when the Run entry points at this executable.
pub fn autostart() -> bool {
    let (ok, out) = reg(&["query", RUN_KEY, "/v", APP_ID]);
    ok && out.to_lowercase().contains(&exe().to_lowercase())
}

pub fn set_autostart(on: bool) -> bool {
    if on {
        reg(&["add", RUN_KEY, "/v", APP_ID, "/t", "REG_SZ", "/d", &format!("\"{}\" --hidden", exe()), "/f"]).0
    } else {
        reg(&["delete", RUN_KEY, "/v", APP_ID, "/f"]).0
    }
}
