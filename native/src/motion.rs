//! Motion: eased tweens, count-ups, the HUD arc spinner, and the Windows "show animations" switch (when it is off,
//! movement is dropped and only fades and colour changes stay).

use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Context, Id, Painter, Pos2, Sense, Stroke, Ui};
use std::sync::OnceLock;

/// Windows → Accessibility → Visual effects → Animation effects.
pub fn reduced() -> bool {
    static R: OnceLock<bool> = OnceLock::new();
    *R.get_or_init(|| {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION};
        let mut on: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL into the pointed value.
        let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut on as *mut i32).cast(), 0) };
        ok != 0 && on == 0
    })
}

/// Confident arrival: fast start, long soft landing.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 { 1.0 } else { 1.0 - 2f32.powf(-10.0 * t) }
}

pub fn now(ctx: &Context) -> f64 {
    ctx.input(|i| i.time)
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
}

/// Eased value towards `target`; a change restarts from wherever it was. `first` is where it starts the first time
/// the id is seen (pass the target to start still).
pub fn tween(ctx: &Context, id: Id, target: f32, secs: f32, first: f32) -> f32 {
    let t = now(ctx);
    let secs = if reduced() { secs.min(0.12) } else { secs };
    let mut tw = ctx.data(|d| d.get_temp::<Tween>(id)).unwrap_or(Tween { from: first, to: first, start: t });
    let value_at = |tw: &Tween| tw.from + (tw.to - tw.from) * ease_out(((t - tw.start) as f32 / secs.max(0.001)).min(1.0));
    if (tw.to - target).abs() > f32::EPSILON {
        tw = Tween { from: value_at(&tw), to: target, start: t };
    }
    ctx.data_mut(|d| d.insert_temp(id, tw));
    let v = value_at(&tw);
    if (v - target).abs() > 0.001 {
        ctx.request_repaint();
    }
    v
}

/// A number that counts up from zero the first time it shows, then glides between values.
pub fn count(ui: &Ui, salt: impl std::hash::Hash + std::fmt::Debug, target: f64) -> f64 {
    let id = Id::unique(("count", salt));
    f64::from(tween(ui.ctx(), id, target as f32, 0.9, 0.0))
}

/// 0 → 1 over `secs` after the timestamp (ISO); used for messages that just arrived.
pub fn since(iso: &str, secs: f32) -> f32 {
    let Some(at) = crate::ui_kit::parse(iso) else { return 1.0 };
    let age = (chrono::Local::now() - at).num_milliseconds() as f32 / 1000.0;
    ease_out(age / secs)
}

pub fn arc(p: &Painter, c: Pos2, r: f32, start: f32, sweep: f32, stroke: Stroke) {
    let steps = ((sweep.abs() / std::f32::consts::TAU) * 48.0).ceil().max(4.0) as usize;
    let pts: Vec<Pos2> = (0..=steps).map(|i| {
        let a = start + sweep * i as f32 / steps as f32;
        pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect();
    p.add(egui::Shape::line(pts, stroke));
}

/// Two counter-rotating arcs, like the reactor rings, instead of egui's generic spinner.
pub fn spinner(ui: &mut Ui, size: f32, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = now(ui.ctx()) as f32;
    let p = ui.painter();
    let c = rect.center();
    let tau = std::f32::consts::TAU;
    let speed = if reduced() { 0.4 } else { 1.0 };
    arc(p, c, size * 0.45, t * tau * 0.9 * speed, tau * 0.3, Stroke::new(1.8, color));
    arc(p, c, size * 0.45, t * tau * 0.9 * speed + tau * 0.5, tau * 0.12, Stroke::new(1.8, theme::alpha(color, 0.5)));
    arc(p, c, size * 0.26, -t * tau * 1.4 * speed, tau * 0.45, Stroke::new(1.4, theme::alpha(color, 0.75)));
    ui.ctx().request_repaint();
    resp
}

/// Hover glow amount (0..1) for a response, with a quick ease.
pub fn hover(ui: &Ui, r: &egui::Response) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(r.id.with("hover-glow"), r.hovered() && r.enabled(), 0.16, egui::emath::easing::cubic_out)
}
