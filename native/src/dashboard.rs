//! The start screen: a living HUD of the day. Clock and sprint on top; the core (sprint, hours and working day as
//! rings); what to focus on now with today's timeline; system telemetry and the conversations at work; PRs, review
//! and personal tasks; and the activity feed. Everything animates in once, then only what is live keeps moving.

use crate::{
    agenda::Event,
    app::View,
    azdo::{Dashboard, Item},
    motion,
    sysmon::Snapshot,
    tasks::Todo,
    theme::{self, size, Accent},
    ui_kit::{self, button, display},
};
use chrono::{Datelike, Local, Timelike};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

/// What the start screen shows besides the board (collected by the app each frame).
pub struct Extras {
    pub meetings: Vec<Event>,
    pub todos: Vec<Todo>,
    /// (title, id, open comments, url)
    pub prs: Vec<(String, i64, usize, String)>,
    pub prs_open: usize,
    /// (chat id, title, engine, seconds running, steps)
    pub busy: Vec<(String, String, String, i64, usize)>,
    /// (when, text) newest first
    pub feed: Vec<(String, String)>,
    /// Item id → (has a conversation with work in it, that conversation is busy)
    pub item_chats: std::collections::HashMap<i64, (bool, bool)>,
    pub sys: Snapshot,
    pub music: crate::music::NowPlaying,
    /// (name, link)
    pub favorites: Vec<(String, String)>,
}

const LONG_MONTHS: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];
/// The working day the third ring measures.
const DAY_START: f32 = 9.0;
const DAY_END: f32 = 18.0;

fn greeting(h: u32) -> &'static str {
    match h {
        0..=5 => "Boa noite",
        6..=12 => "Bom dia",
        13..=19 => "Boa tarde",
        _ => "Boa noite",
    }
}

/// Section title in the HUD voice; clickable when it leads somewhere.
fn heading(ui: &mut Ui, text: &str, acc: Accent, to: Option<View>, go: &mut Option<View>) {
    ui.horizontal(|ui| {
        ui_kit::diamond(ui, true, acc.main, 3.5);
        let r = ui.add(egui::Label::new(RichText::new(text.to_uppercase()).font(display(12.5)).color(acc.light).extra_letter_spacing(2.2)).sense(if to.is_some() { Sense::click() } else { Sense::hover() }));
        if let Some(v) = to {
            if r.on_hover_text("Abrir").clicked() {
                *go = Some(v);
            }
        }
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
        ui.painter().line_segment([pos2(line.left() + 6.0, line.center().y), pos2(line.right(), line.center().y)], Stroke::new(1.0, theme::alpha(acc.main, 0.25)));
    });
    ui.add_space(4.0);
}

fn panel(ui: &mut Ui, acc: Accent, add: impl FnOnce(&mut Ui)) {
    ui_kit::hud_panel(ui, acc, false, add);
}

/// `since`: seconds since the screen was opened, for the staggered entrance of its blocks.
pub fn view(ui: &mut Ui, d: &Dashboard, x: &Extras, acc: Accent, go: &mut Option<View>, since: f32) {
    // Leave room for the scroll bar on the right, which floats over the content.
    ui.set_max_width(ui.available_width() - 16.0);
    let enter = |ui: &mut Ui, order: usize| stagger(ui, since, order);
    ui.scope(|ui| {
        enter(ui, 0);
        hero(ui, d, acc);
    });
    ui.add_space(14.0);
    ui.scope(|ui| {
        enter(ui, 1);
        work(ui, d, x, acc, go);
    });
    ui.add_space(16.0);
    let wide = ui.available_width() > 1100.0;
    if wide {
        let w = ui.available_width();
        let gap = 18.0;
        let (a, b) = ((w - 2.0 * gap) * 0.27, (w - 2.0 * gap) * 0.43);
        let c = w - 2.0 * gap - a - b;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(vec2(a, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(a);
                enter(ui, 1);
                core(ui, d, acc, go);
            });
            ui.allocate_ui_with_layout(vec2(b, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(b);
                enter(ui, 2);
                focus(ui, d, x, acc, go);
            });
            ui.allocate_ui_with_layout(vec2(c, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(c);
                enter(ui, 3);
                system(ui, x, acc, go);
            });
        });
    } else if ui.available_width() > 700.0 {
        // Medium window: the core beside the focus, then telemetry beside the conversations.
        ui.columns(2, |cols| {
            stagger(&mut cols[0], since, 1);
            core(&mut cols[0], d, acc, go);
            stagger(&mut cols[1], since, 2);
            focus(&mut cols[1], d, x, acc, go);
        });
        ui.add_space(14.0);
        ui.columns(2, |cols| {
            stagger(&mut cols[0], since, 3);
            telemetry(&mut cols[0], x, acc, go);
            stagger(&mut cols[1], since, 3);
            conversations(&mut cols[1], x, acc, go);
            cols[1].add_space(14.0);
            music(&mut cols[1], x, acc, go);
        });
    } else {
        core(ui, d, acc, go);
        ui.add_space(12.0);
        focus(ui, d, x, acc, go);
        ui.add_space(12.0);
        system(ui, x, acc, go);
    }
    ui.add_space(16.0);
    ui.columns(3, |cols| {
        stagger(&mut cols[0], since, 4);
        prs(&mut cols[0], x, acc, go);
        stagger(&mut cols[1], since, 5);
        review(&mut cols[1], d, acc, go);
        stagger(&mut cols[2], since, 6);
        todos(&mut cols[2], x, acc, go);
    });
    ui.add_space(16.0);
    ui.scope(|ui| {
        enter(ui, 7);
        feed(ui, x, acc, go);
    });
}

/* ---------- hero: greeting, date, live clock, sprint ---------- */

fn hero(ui: &mut Ui, d: &Dashboard, acc: Accent) {
    let now = Local::now();
    let first = d.me.name.split_whitespace().next().unwrap_or("");
    let days_left = ui_kit::parse(&d.sprint.finish).map(|f| (f.date_naive() - now.date_naive()).num_days().max(0)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{}, {first}.", greeting(now.hour()))).font(display(size::HERO)).color(theme::BRIGHT));
            ui.label(
                RichText::new(format!(
                    "{}, {} de {} · {} · {}",
                    ui_kit::WEEKDAYS[now.weekday().num_days_from_monday() as usize],
                    now.day(),
                    LONG_MONTHS[now.month0() as usize],
                    d.sprint.name,
                    if days_left == 0 { "último dia".to_owned() } else { format!("faltam {days_left} {}", if days_left == 1 { "dia" } else { "dias" }) }
                ))
                .monospace()
                .color(theme::MUTED),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // The clock: hours and minutes large, seconds as a sweeping tick bar under them.
            let (rect, _) = ui.allocate_exact_size(vec2(190.0, 58.0), Sense::hover());
            let p = ui.painter();
            p.text(pos2(rect.right(), rect.top()), Align2::RIGHT_TOP, now.format("%H:%M").to_string(), FontId::new(40.0, theme::display()), theme::BRIGHT);
            let secs = now.second() as f32 + now.timestamp_subsec_millis() as f32 / 1000.0;
            let bar = Rect::from_min_size(pos2(rect.right() - 150.0, rect.bottom() - 6.0), vec2(150.0, 3.0));
            p.rect_filled(bar, 0.0, theme::alpha(acc.main, 0.15));
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * secs / 60.0, 3.0)), 0.0, acc.main);
            p.text(pos2(bar.left() - 8.0, bar.center().y), Align2::RIGHT_CENTER, format!("{:02}", now.second()), FontId::monospace(11.0), theme::FAINT);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        });
    });
}

/* ---------- core: three rings ---------- */

fn ring(p: &egui::Painter, c: egui::Pos2, r: f32, w: f32, frac: f32, color: Color32) {
    let tau = std::f32::consts::TAU;
    motion::arc(p, c, r, -tau / 4.0, tau, Stroke::new(w, theme::alpha(color, 0.12)));
    if frac > 0.002 {
        motion::arc(p, c, r, -tau / 4.0, tau * frac.min(1.0), Stroke::new(w, color));
        // Bright head at the end of the arc.
        let a = -tau / 4.0 + tau * frac.min(1.0);
        p.circle_filled(pos2(c.x + r * a.cos(), c.y + r * a.sin()), w * 0.75, theme::mix(color, Color32::WHITE, 0.4));
    }
}

fn core(ui: &mut Ui, d: &Dashboard, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Núcleo", acc, Some(View::Hours), go);
    let now = Local::now();
    let sprint = match (ui_kit::parse(&d.sprint.start), ui_kit::parse(&d.sprint.finish)) {
        (Some(s), Some(f)) => ((now - s).num_seconds() as f32 / ((f - s).num_seconds() as f32 + 86400.0)).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let est: f64 = d.tasks.iter().filter_map(|t| t.estimate).sum();
    let done: f64 = d.tasks.iter().filter_map(|t| t.work_done).sum();
    let hours = if est > 0.0 { (done / est) as f32 } else { 0.0 };
    let h = now.hour() as f32 + now.minute() as f32 / 60.0;
    let day = ((h - DAY_START) / (DAY_END - DAY_START)).clamp(0.0, 1.0);
    let ctx = ui.ctx().clone();
    let t_sprint = motion::tween(&ctx, egui::Id::unique("ring-sprint"), sprint, 1.4, 0.0);
    let t_hours = motion::tween(&ctx, egui::Id::unique("ring-hours"), hours, 1.6, 0.0);
    let t_day = motion::tween(&ctx, egui::Id::unique("ring-day"), day, 1.8, 0.0);
    panel(ui, acc, |ui| {
        let side = ui.available_width().min(250.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            let p = ui.painter();
            let c = rect.center();
            let r = side / 2.0 - 8.0;
            let tau = std::f32::consts::TAU;
            // Outer tick ring, turning slowly.
            let spin = motion::now(&ctx) as f32 * if motion::reduced() { 0.0 } else { 0.08 };
            for i in 0..72 {
                let a = i as f32 / 72.0 * tau + spin;
                let long = i % 6 == 0;
                let (r1, r2) = (r + 2.0, r + if long { 8.0 } else { 5.0 });
                p.line_segment([pos2(c.x + r1 * a.cos(), c.y + r1 * a.sin()), pos2(c.x + r2 * a.cos(), c.y + r2 * a.sin())], Stroke::new(1.0, theme::alpha(acc.main, if long { 0.5 } else { 0.22 })));
            }
            ring(p, c, r * 0.9, 7.0, t_sprint, acc.main);
            ring(p, c, r * 0.74, 7.0, t_hours, acc.light);
            ring(p, c, r * 0.58, 7.0, t_day, theme::GREEN);
            let working = d.now.iter().filter(|i| i.column == "In Progress").count();
            let n = motion::count(ui, "core-working", working as f64).round() as i64;
            p.text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, n.to_string(), FontId::new(38.0, theme::display()), theme::BRIGHT);
            p.text(c + vec2(0.0, 20.0), Align2::CENTER_CENTER, "EM CURSO", FontId::new(11.0, theme::display()), theme::MUTED);
        });
        ui.add_space(8.0);
        for (color, label, value) in [
            (acc.main, "Sprint decorrido", format!("{:.0}%", sprint * 100.0)),
            (acc.light, "Horas feitas / estimadas", format!("{:.0} / {:.0} h", done, est)),
            (theme::GREEN, "Dia de trabalho (9h–18h)", format!("{:.0}%", day * 100.0)),
        ] {
            ui.horizontal(|ui| {
                ui_kit::diamond(ui, true, color, 4.0);
                ui.label(RichText::new(label).small().color(theme::MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(value).monospace().color(theme::TEXT));
                });
            });
        }
    });
}

/* ---------- focus: the item to work on, and today's timeline ---------- */

/// Today's timeline (the item to work on now lives in the Trabalho block above).
fn focus(ui: &mut Ui, _d: &Dashboard, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Hoje na agenda", acc, Some(View::Agenda), go);
    panel(ui, acc, |ui| timeline(ui, &x.meetings, acc));
}

fn timeline(ui: &mut Ui, meetings: &[Event], acc: Accent) {
    let now = Local::now();
    let (from, to) = (8.0_f32, 19.0_f32);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    let p = ui.painter();
    let x_of = |h: f32| rect.left() + rect.width() * ((h - from) / (to - from)).clamp(0.0, 1.0);
    let track = Rect::from_min_max(pos2(rect.left(), rect.center().y - 9.0), pos2(rect.right(), rect.center().y + 9.0));
    p.rect_filled(track, CornerRadius::same(3), theme::alpha(acc.main, 0.06));
    for h in (from as u32)..=(to as u32) {
        let x = x_of(h as f32);
        p.line_segment([pos2(x, track.bottom()), pos2(x, track.bottom() + if h % 2 == 0 { 6.0 } else { 3.0 })], Stroke::new(1.0, theme::alpha(acc.main, 0.3)));
        if h % 2 == 0 {
            p.text(pos2(x, rect.bottom() + 2.0), Align2::CENTER_TOP, format!("{h}h"), FontId::monospace(10.0), theme::FAINT);
        }
    }
    let hour = |e: Option<chrono::DateTime<Local>>| e.map(|t| t.hour() as f32 + t.minute() as f32 / 60.0);
    for e in meetings.iter().filter(|e| !e.all_day) {
        let (Some(s), Some(en)) = (hour(e.start_at()), hour(e.end_at())) else { continue };
        let r = Rect::from_min_max(pos2(x_of(s), track.top() + 2.0), pos2(x_of(en).max(x_of(s) + 4.0), track.bottom() - 2.0));
        let past = e.end_at().is_some_and(|t| t < now);
        p.rect(r, CornerRadius::same(2), theme::alpha(acc.main, if past { 0.15 } else { 0.45 }), Stroke::new(1.0, theme::alpha(acc.light, if past { 0.2 } else { 0.8 })), StrokeKind::Inside);
    }
    let nh = now.hour() as f32 + now.minute() as f32 / 60.0;
    if nh >= from && nh <= to {
        let x = x_of(nh);
        let pulse = if motion::reduced() { 1.0 } else { 0.6 + 0.4 * (motion::now(ui.ctx()) as f32 * 2.5).sin().abs() };
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(2.0, theme::alpha(theme::RED, pulse)));
        p.circle_filled(pos2(x, rect.top()), 3.5, theme::RED);
    }
    ui.add_space(16.0);
    let next = meetings.iter().filter(|e| !e.all_day).find(|e| e.start_at().is_some_and(|s| s > now));
    let current = meetings.iter().filter(|e| !e.all_day).find(|e| e.start_at().is_some_and(|s| s <= now) && e.end_at().is_some_and(|en| en > now));
    ui.horizontal(|ui| {
        match (current, next) {
            (Some(e), _) => {
                ui.label(RichText::new(format!("A decorrer: {}", e.subject)).color(theme::GREEN));
            }
            (None, Some(e)) => {
                let mins = e.start_at().map(|s| (s - now).num_minutes()).unwrap_or(0);
                let when = if mins < 60 { format!("em {mins} min") } else { format!("às {}", e.start_at().unwrap().format("%H:%M")) };
                ui.label(RichText::new(format!("Próxima: {} · {when}", e.subject)).color(if mins <= 10 { theme::AMBER } else { theme::TEXT }));
            }
            _ => {
                ui.label(RichText::new("Sem mais reuniões hoje.").color(theme::MUTED));
            }
        }
        let joinable = current.or(next.filter(|e| e.start_at().is_some_and(|s| (s - now).num_minutes() <= 10)));
        if let Some(u) = joinable.and_then(|e| e.teams_url()) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, "Entrar no Teams", true, acc).clicked() {
                    crate::brain::open(u);
                }
            });
        }
    });
}

/* ---------- system telemetry and conversations at work ---------- */

fn bar(ui: &mut Ui, label: &str, frac: f32, value: String, acc: Accent, salt: &str) {
    let shown = motion::tween(ui.ctx(), egui::Id::unique(("bar", salt)), frac, 0.8, 0.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label.to_uppercase()).font(display(11.0)).color(theme::MUTED).extra_letter_spacing(1.2));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).monospace().size(12.0).color(theme::TEXT));
        });
    });
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let color = if frac > 0.9 { theme::RED } else if frac > 0.75 { theme::AMBER } else { acc.main };
    ui.painter().rect_filled(r, CornerRadius::same(2), theme::alpha(acc.main, 0.12));
    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * shown.clamp(0.0, 1.0), r.height())), CornerRadius::same(2), color);
    ui.add_space(6.0);
}

fn gb(b: u64) -> String {
    format!("{:.1}", b as f64 / 1_073_741_824.0).replace('.', ",")
}

fn system(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    telemetry(ui, x, acc, go);
    ui.add_space(14.0);
    conversations(ui, x, acc, go);
    ui.add_space(14.0);
    music(ui, x, acc, go);
}

fn telemetry(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Sistema", acc, None, go);
    panel(ui, acc, |ui| {
        let s = &x.sys;
        if !s.ready {
            ui.horizontal(|ui| {
                motion::spinner(ui, 14.0, acc.main);
                ui.label(RichText::new("A ler o sistema…").color(theme::MUTED));
            });
            return;
        }
        bar(ui, "CPU", s.cpu / 100.0, format!("{:.0}%", s.cpu), acc, "cpu");
        // A minute of CPU history.
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(2), theme::alpha(acc.main, 0.04));
        let n = s.cpu_history.len().max(2);
        let pts: Vec<egui::Pos2> = s.cpu_history.iter().enumerate().map(|(i, v)| pos2(r.left() + r.width() * (60 - n + i) as f32 / 59.0, r.bottom() - r.height() * (v / 100.0).clamp(0.0, 1.0))).collect();
        if pts.len() > 1 {
            p.add(egui::Shape::line(pts, Stroke::new(1.5, acc.light)));
        }
        ui.add_space(8.0);
        let mem = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 } else { 0.0 };
        bar(ui, "Memória", mem, format!("{} / {} GB", gb(s.mem_used), gb(s.mem_total)), acc, "mem");
        let disk = if s.disk_total > 0 { 1.0 - s.disk_free as f32 / s.disk_total as f32 } else { 0.0 };
        bar(ui, "Disco C:", disk, format!("{} GB livres", gb(s.disk_free)), acc, "disk");
    });
}

fn conversations(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Conversas a trabalhar", acc, Some(View::Chat), go);
    panel(ui, acc, |ui| {
        if x.busy.is_empty() {
            ui.label(RichText::new("Nenhuma conversa a trabalhar agora.").color(theme::MUTED));
            return;
        }
        for (id, title, engine, secs, steps) in &x.busy {
            let r = ui
                .horizontal(|ui| {
                    motion::spinner(ui, 14.0, acc.main);
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(title).color(theme::BRIGHT)).truncate());
                        ui.label(RichText::new(format!("{engine} · {}:{:02} · {steps} passos", secs / 60, secs % 60)).monospace().size(11.0).color(theme::MUTED));
                    });
                })
                .response
                .interact(Sense::click());
            if r.on_hover_text("Abrir a conversa").clicked() {
                crate::app::post(crate::app::AppEvent::OpenChat(id.clone()));
            }
        }
    });
}

/* ---------- PRs, review, personal tasks ---------- */

fn prs(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "PRs", acc, Some(View::Prs), go);
    panel(ui, acc, |ui| {
        ui.horizontal(|ui| {
            let n = motion::count(ui, "dash-prs", x.prs_open as f64).round() as i64;
            ui.label(RichText::new(n.to_string()).monospace().size(size::STAT).strong().color(acc.light));
            ui.label(RichText::new("abertos").color(theme::MUTED));
        });
        if x.prs.is_empty() {
            ui.label(RichText::new("Nenhum comentário por resolver.").color(theme::MUTED));
        }
        for (title, id, n, _) in x.prs.iter().take(4) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("!{id}")).monospace().color(theme::AMBER));
                ui.add(egui::Label::new(RichText::new(format!("{title} · {n} por resolver")).color(theme::TEXT)).truncate());
            });
        }
    });
}

fn review(ui: &mut Ui, d: &Dashboard, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Revisão e QA", acc, Some(View::Review), go);
    panel(ui, acc, |ui| {
        let total = d.review.len().max(1) as f32;
        for col in ["Code Review", "To Test", "Testing"] {
            let n = d.review.iter().filter(|i| i.column == col).count();
            let c = ui_kit::column_color(col, acc);
            let shown = motion::tween(ui.ctx(), egui::Id::unique(("review-bar", col)), n as f32 / total, 0.9, 0.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(col.to_uppercase()).font(display(11.5)).color(c).extra_letter_spacing(1.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(n.to_string()).monospace().color(theme::TEXT));
                });
            });
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(2), theme::alpha(c, 0.12));
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * shown, r.height())), CornerRadius::same(2), c);
            ui.add_space(4.0);
        }
    });
}

fn todos(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Tarefas", acc, Some(View::Tasks), go);
    panel(ui, acc, |ui| {
        if x.todos.is_empty() {
            ui.label(RichText::new("Sem tarefas pessoais abertas.").color(theme::MUTED));
        }
        for t in x.todos.iter().take(5) {
            ui.horizontal(|ui| {
                ui_kit::diamond(ui, false, acc.main, 4.0);
                ui.add(egui::Label::new(RichText::new(&t.title).color(theme::TEXT)).truncate());
                if let Some(due) = &t.due {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(due.get(5..).unwrap_or(due).replace('-', "/")).monospace().size(11.0).color(theme::FAINT));
                    });
                }
            });
        }
    });
}

/* ---------- activity feed ---------- */

fn feed(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    heading(ui, "Atividade", acc, Some(View::Chat), go);
    Frame::new().stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if x.feed.is_empty() {
            ui.label(RichText::new("Sem novidades do board nem dos PRs por agora.").color(theme::MUTED));
        }
        for (at, text) in x.feed.iter().take(6) {
            let k = motion::since(at, 0.5);
            ui.scope(|ui| {
                if k < 1.0 {
                    ui.set_opacity(0.2 + 0.8 * k);
                    ui.ctx().request_repaint();
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ui_kit::when(at)).monospace().size(11.0).color(theme::FAINT));
                    ui.add(egui::Label::new(RichText::new(text).color(theme::TEXT)).truncate());
                });
            });
        }
    });
}

/// Blocks arrive one after the other (55 ms apart): a fade and a short rise, then they stay still.
fn stagger(ui: &mut Ui, since: f32, order: usize) {
    let k = motion::ease_out(((since - order as f32 * 0.055) / 0.4).clamp(0.0, 1.0));
    if k < 1.0 {
        ui.set_opacity(k);
        if !motion::reduced() {
            ui.add_space((1.0 - k) * 14.0);
        }
        ui.ctx().request_repaint();
    }
}

/* ---------- work: mine and available, one click from the start screen ---------- */

fn work(ui: &mut Ui, d: &Dashboard, x: &Extras, acc: Accent, go: &mut Option<View>) {
    // Mine: returned by QA first, then In Progress, then Planned.
    let mut mine: Vec<&Item> = d.now.iter().collect();
    mine.sort_by_key(|i| (i.back.is_none(), i.column != "In Progress"));
    let two = ui.available_width() > 760.0;
    let mine_block = |ui: &mut Ui, go: &mut Option<View>| {
        heading(ui, &format!("O meu trabalho · {}", mine.len()), acc, Some(View::Now), go);
        panel(ui, acc, |ui| {
            if mine.is_empty() {
                ui.label(RichText::new("Não tens nada em curso nem planeado. Pega num item disponível ao lado.").color(theme::MUTED));
            }
            for (k, it) in mine.iter().enumerate() {
                if k > 0 {
                    separator(ui, acc);
                }
                let (talked, busy) = x.item_chats.get(&it.id).copied().unwrap_or((false, false));
                let sub = match (&it.back, busy, talked) {
                    (Some(b), _, _) => (format!("Voltou do QA · {} · {}", b.by, ui_kit::when(&b.at)), theme::AMBER),
                    (None, true, _) => ("O Jarvis está a trabalhar nisto".to_owned(), acc.light),
                    (None, false, true) => (format!("{} · conversa aberta", it.column), theme::MUTED),
                    _ => (it.column.clone(), theme::MUTED),
                };
                row(ui, it, &sub.0, sub.1, acc, |ui| {
                    if talked {
                        ui_kit::start_button_as(ui, it.id, "Continuar", "Abre a conversa deste item", true, acc);
                    } else {
                        ui_kit::start_button_as(ui, it.id, "Começar", "Escolhe o motor, abre a conversa e põe o item no sprint", true, acc);
                    }
                });
            }
        });
    };
    let available: Vec<&Item> = d.refined.iter().take(6).collect();
    let free_block = |ui: &mut Ui, go: &mut Option<View>| {
        heading(ui, &format!("Disponível · {}", d.refined.len()), acc, Some(View::Refined), go);
        panel(ui, acc, |ui| {
            if available.is_empty() {
                ui.label(RichText::new("Nada livre no Refined.").color(theme::MUTED));
            }
            for (k, it) in available.iter().enumerate() {
                if k > 0 {
                    separator(ui, acc);
                }
                let mut parts = vec![it.effort.map(|e| format!("Effort {e}")).unwrap_or_else(|| "Sem effort".into())];
                if let Some(p) = it.priority {
                    parts.push(format!("P{p}"));
                }
                if let Some(s) = &it.severity {
                    parts.push(s.clone());
                }
                row(ui, it, &parts.join(" · "), theme::MUTED, acc, |ui| {
                    ui_kit::start_button_as(ui, it.id, "Pegar", "Atribui-te o item, põe-no no sprint em Planned e abre a análise", false, acc);
                });
            }
            if d.refined.len() > available.len() && button(ui, &format!("Ver os {} disponíveis", d.refined.len()), false, acc).clicked() {
                *go = Some(View::Refined);
            }
            if !d.now.is_empty() && !d.refined.is_empty() {
                ui.label(RichText::new("Ainda tens trabalho teu: o Refined fica para depois dele.").small().color(theme::FAINT));
            }
        });
    };
    if two {
        ui.columns(2, |cols| {
            mine_block(&mut cols[0], go);
            free_block(&mut cols[1], go);
        });
    } else {
        mine_block(ui, go);
        ui.add_space(12.0);
        free_block(ui, go);
    }
}

fn separator(ui: &mut Ui, acc: Accent) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment([pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, theme::alpha(acc.main, 0.1)));
}

/// One item: type, number, title and a status line on the left; the action on the right.
fn row(ui: &mut Ui, it: &Item, sub: &str, sub_color: Color32, acc: Accent, action: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            action(ui);
            ui.add_space(6.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui_kit::type_pill(ui, &it.kind, acc);
                    ui_kit::id_link(ui, it.id, &it.url, acc);
                    ui.add(egui::Label::new(RichText::new(&it.title).color(theme::BRIGHT)).truncate());
                });
                ui.add(egui::Label::new(RichText::new(sub).small().color(sub_color)).truncate());
            });
        });
    });
}

/* ---------- music: what is playing, with the controls ---------- */

/// Round control button with a drawn icon: "prev" | "play" | "pause" | "next".
fn media_button(ui: &mut Ui, icon: &str, big: bool, acc: Accent) -> egui::Response {
    let d = if big { 40.0 } else { 30.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(d, d), Sense::click());
    let h = motion::hover(ui, &resp);
    let p = ui.painter();
    let c = rect.center();
    let fill = if big { theme::mix(acc.main, acc.light, h) } else { theme::alpha(acc.main, 0.08 + 0.12 * h) };
    p.circle(c, d / 2.0, fill, Stroke::new(1.0, theme::alpha(acc.main, 0.5 + 0.4 * h)));
    let col = if big { Color32::from_rgb(0x02, 0x10, 0x19) } else { theme::mix(theme::TEXT, theme::BRIGHT, h) };
    let s = d * 0.2;
    let tri = |dir: f32, x: f32| vec![pos2(x - s * 0.6 * dir, c.y - s), pos2(x + s * 0.7 * dir, c.y), pos2(x - s * 0.6 * dir, c.y + s)];
    match icon {
        "play" => {
            p.add(egui::Shape::convex_polygon(tri(1.0, c.x + s * 0.15), col, Stroke::NONE));
        }
        "pause" => {
            for dx in [-s * 0.45, s * 0.45] {
                p.rect_filled(Rect::from_center_size(pos2(c.x + dx, c.y), vec2(s * 0.45, s * 2.0)), 1.0, col);
            }
        }
        "next" => {
            p.add(egui::Shape::convex_polygon(tri(1.0, c.x - s * 0.2), col, Stroke::NONE));
            p.rect_filled(Rect::from_center_size(pos2(c.x + s * 0.75, c.y), vec2(s * 0.3, s * 2.0)), 0.0, col);
        }
        _ => {
            p.add(egui::Shape::convex_polygon(tri(-1.0, c.x + s * 0.2), col, Stroke::NONE));
            p.rect_filled(Rect::from_center_size(pos2(c.x - s * 0.75, c.y), vec2(s * 0.3, s * 2.0)), 0.0, col);
        }
    }
    resp
}

fn music(ui: &mut Ui, x: &Extras, acc: Accent, go: &mut Option<View>) {
    use crate::{
        app::{post, AppEvent},
        music::{clock, open_app, Cmd},
    };
    heading(ui, "Música", acc, None, go);
    panel(ui, acc, |ui| {
        let m = &x.music;
        if m.title.is_empty() {
            ui.label(RichText::new("Nada a tocar. Põe música no Spotify ou no Apple Music e controlas daqui.").color(theme::MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if button(ui, "Abrir Spotify", false, acc).clicked() {
                    open_app("spotify");
                }
                if button(ui, "Abrir Apple Music", false, acc).clicked() {
                    open_app("apple");
                }
            });
        } else {
            ui.horizontal(|ui| {
                // Album art, kept as a texture until the track changes; a drawn disc when there is none.
                let side = 64.0;
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                let key = egui::Id::unique("music-cover");
                let tex = match &m.cover {
                    Some((rgba, size)) => {
                        let cached: Option<(String, egui::TextureHandle)> = ui.ctx().data(|d| d.get_temp(key));
                        match cached {
                            Some((k, t)) if k == m.cover_key => Some(t),
                            _ => {
                                let t = ui.ctx().load_texture("music-cover", egui::ColorImage::from_rgba_unmultiplied(*size, rgba), Default::default());
                                ui.ctx().data_mut(|d| d.insert_temp(key, (m.cover_key.clone(), t.clone())));
                                Some(t)
                            }
                        }
                    }
                    None => None,
                };
                match tex {
                    Some(t) => {
                        egui::Image::new(&t).corner_radius(CornerRadius::same(4)).paint_at(ui, rect);
                    }
                    None => {
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(4), theme::alpha(acc.main, 0.1));
                        let spin = if m.playing && !motion::reduced() { motion::now(ui.ctx()) as f32 } else { 0.0 };
                        motion::arc(p, rect.center(), side * 0.32, spin, 4.5, Stroke::new(2.0, acc.main));
                        p.circle_filled(rect.center(), 4.0, acc.light);
                    }
                }
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(&m.title).strong().color(theme::BRIGHT)).truncate());
                    ui.add(egui::Label::new(RichText::new(&m.artist).color(theme::MUTED)).truncate());
                    ui.label(RichText::new(m.app.to_uppercase()).font(display(10.5)).color(acc.light).extra_letter_spacing(1.2));
                });
            });
            ui.add_space(6.0);
            if m.duration > 0.0 {
                let pos = m.position_now();
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(2), theme::alpha(acc.main, 0.15));
                ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * (pos / m.duration).clamp(0.0, 1.0), r.height())), CornerRadius::same(2), acc.main);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(clock(pos)).monospace().size(11.0).color(theme::FAINT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(clock(m.duration)).monospace().size(11.0).color(theme::FAINT));
                    });
                });
                if m.playing {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                }
            }
            ui.horizontal(|ui| {
                let w = 30.0 + 40.0 + 30.0 + 2.0 * ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                if media_button(ui, "prev", false, acc).on_hover_text("Anterior").clicked() {
                    post(AppEvent::Music(Cmd::Previous));
                }
                let (icon, tip) = if m.playing { ("pause", "Pausa") } else { ("play", "Tocar") };
                if media_button(ui, icon, true, acc).on_hover_text(tip).clicked() {
                    post(AppEvent::Music(Cmd::Toggle));
                }
                if media_button(ui, "next", false, acc).on_hover_text("Seguinte").clicked() {
                    post(AppEvent::Music(Cmd::Next));
                }
            });
        }
        if !x.favorites.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("PLAYLISTS").font(display(11.0)).color(theme::MUTED).extra_letter_spacing(1.4));
            ui.horizontal_wrapped(|ui| {
                for (name, link) in &x.favorites {
                    if button(ui, name, false, acc).on_hover_text(link).clicked() {
                        crate::brain::open(link);
                    }
                }
            });
        }
    });
}
