//! Notifications: a Windows toast and, always, a notice inside the Jarvis window (bottom right), so nothing is lost
//! when Windows notifications are switched off. Clicking either sends an event to the app.

use crate::{
    app::{post, AppEvent},
    motion,
    theme::{self, Accent},
};
use eframe::egui::{self, pos2, vec2, Align2, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind};
use std::{
    os::windows::process::CommandExt,
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri_winrt_notification::Toast;

// The Start menu shortcut (scripts/install-native.ps1) registers this AppUserModelID, so toasts show as "Jarvis".
const APP_ID: &str = crate::system::APP_ID;
const SHOW_FOR: Duration = Duration::from_secs(9);

struct Notice {
    title: String,
    body: String,
    click: AppEvent,
    at: Instant,
    id: u64,
}

fn notices() -> &'static Mutex<Vec<Notice>> {
    static N: OnceLock<Mutex<Vec<Notice>>> = OnceLock::new();
    N.get_or_init(Default::default)
}

pub fn notify(title: &str, body: &str, click: AppEvent) {
    if !crate::settings::load().notify {
        return;
    }
    let (title, body) = (title.to_owned(), body.chars().take(220).collect::<String>());
    {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut n = notices().lock().unwrap();
        n.push(Notice { title: title.clone(), body: body.clone(), click: click.clone(), at: Instant::now(), id: SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) });
        let len = n.len();
        if len > 4 {
            n.drain(..len - 4);
        }
    }
    post(AppEvent::Repaint);
    // The toast API is COM: keep it off the UI thread.
    std::thread::spawn(move || {
        let toast = |id: &str| {
            let click = click.clone();
            Toast::new(id).title(&title).text1(&body).on_activated(move |_| {
                post(click.clone());
                Ok(())
            })
        };
        if toast(APP_ID).show().is_err() {
            let _ = toast(Toast::POWERSHELL_APP_ID).show();
        }
        // Keep the thread (and the activation handler) alive while the toast can still be clicked.
        std::thread::sleep(Duration::from_secs(25));
    });
}

/// Whether Windows notifications are switched on (Definições → Sistema → Notificações). Read at most every 30 s.
pub fn windows_enabled() -> bool {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((at, v)) = *c {
        if at.elapsed() < Duration::from_secs(30) {
            return v;
        }
    }
    let out = Command::new("reg.exe")
        .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications", "/v", "ToastEnabled"])
        .creation_flags(0x0800_0000)
        .output();
    let v = !out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("0x0"));
    *c = Some((Instant::now(), v));
    v
}

/// The notices inside the window: cards in the bottom-right corner that slide in, wait and fade away.
pub fn draw(ctx: &egui::Context, acc: Accent) {
    let mut list = notices().lock().unwrap();
    list.retain(|n| n.at.elapsed() < SHOW_FOR);
    if list.is_empty() {
        return;
    }
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("notices")));
    let (w, h, gap) = (360.0, 74.0, 10.0);
    let mut y = screen.bottom() - 18.0;
    let mut clicked = None;
    let mut dismissed = None;
    for n in list.iter().rev() {
        let age = n.at.elapsed().as_secs_f32();
        let enter = motion::ease_out(age / 0.35);
        let leave = 1.0 - ((age - (SHOW_FOR.as_secs_f32() - 0.5)) / 0.5).clamp(0.0, 1.0);
        let slide = if motion::reduced() { 0.0 } else { (1.0 - enter) * 40.0 };
        let r = Rect::from_min_size(pos2(screen.right() - w - 18.0 + slide, y - h), vec2(w, h));
        let a = enter * leave;
        p.add(egui::epaint::Shadow { offset: [0, 6], blur: 22, spread: 0, color: theme::alpha(egui::Color32::BLACK, 0.5 * a) }.as_shape(r, CornerRadius::same(5)));
        p.rect(r, CornerRadius::same(5), theme::alpha(theme::GLASS, 0.97 * a), Stroke::new(1.0, theme::alpha(acc.main, 0.55 * a)), StrokeKind::Inside);
        // Time left, as a thin bar along the bottom.
        let left = 1.0 - (age / SHOW_FOR.as_secs_f32()).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_size(pos2(r.left(), r.bottom() - 2.0), vec2(r.width() * left, 2.0)), 0.0, theme::alpha(acc.main, 0.8 * a));
        p.text(pos2(r.left() + 14.0, r.top() + 12.0), Align2::LEFT_TOP, &n.title, FontId::new(14.0, theme::display()), theme::alpha(theme::BRIGHT, a));
        // One line, cut with an ellipsis to the card width.
        let mut job = egui::text::LayoutJob::simple(n.body.clone(), FontId::proportional(13.0), theme::alpha(theme::MUTED, a), w - 28.0);
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        p.galley(pos2(r.left() + 14.0, r.top() + 36.0), galley, theme::MUTED);
        let close = Rect::from_center_size(pos2(r.right() - 16.0, r.top() + 16.0), vec2(16.0, 16.0));
        p.text(close.center(), Align2::CENTER_CENTER, "×", FontId::proportional(16.0), theme::alpha(theme::FAINT, a));
        let resp = ctx.input(|i| (i.pointer.primary_clicked(), i.pointer.interact_pos()));
        if let (true, Some(pos)) = resp {
            if close.contains(pos) {
                dismissed = Some(n.id);
            } else if r.contains(pos) {
                clicked = Some((n.id, n.click.clone()));
            }
        }
        y -= h + gap;
    }
    if let Some(id) = dismissed {
        list.retain(|n| n.id != id);
    }
    if let Some((id, ev)) = clicked {
        list.retain(|n| n.id != id);
        post(ev);
    }
    ctx.request_repaint();
    let _ = Sense::hover();
}
