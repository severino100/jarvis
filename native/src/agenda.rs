//! Agenda from the calendars the person connected (the classic Outlook on this PC through scripts/calendar.ps1, and
//! iCalendar links through ics.rs; read-only), and the week view like
//! Teams: days side by side, hours down, a block per meeting, the "now" line, and joining Teams calls.

use crate::{
    app::{post, AppEvent},
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike};
use eframe::egui::{self, pos2, vec2, Align2, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    os::windows::process::CommandExt,
    process::Command,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub subject: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: String,
    pub organizer: String,
    pub busy: i64,
    pub response: i64,
    pub teams: String,
}

impl Event {
    pub fn start_at(&self) -> Option<DateTime<Local>> {
        local_time(&self.start)
    }
    pub fn end_at(&self) -> Option<DateTime<Local>> {
        local_time(&self.end)
    }
    /// Only real Teams meeting links are ever opened.
    pub fn teams_url(&self) -> Option<&str> {
        Some(self.teams.as_str()).filter(|u| u.starts_with("https://teams.microsoft.com/l/meetup-join/"))
    }
}

/// Outlook writes local times without an offset ("2026-11-16T11:30:00.0000000").
fn local_time(s: &str) -> Option<DateTime<Local>> {
    ui_kit::parse(s).or_else(|| {
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
        naive.and_local_timezone(Local).earliest()
    })
}

#[derive(Clone, Default)]
pub struct Week {
    pub ok: bool,
    pub error: Option<String>,
    pub account: String,
    pub events: Vec<Event>,
    pub at: Option<Instant>,
    /// `CALENDARS_GEN` when it was read: a week read before the calendars changed is read again.
    pub epoch: u64,
}

#[derive(Default)]
struct Inner {
    weeks: HashMap<NaiveDate, Week>,
    loading: HashSet<NaiveDate>,
}

#[derive(Clone)]
pub struct Agenda {
    inner: Arc<Mutex<Inner>>,
    ctx: egui::Context,
}

pub fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// The week from every calendar the person connected, side by side; a calendar that fails does not hide the others.
fn load(from: NaiveDate) -> Week {
    let sources = crate::profile::calendars();
    let many = sources.len() > 1;
    let results: Vec<(String, Result<Week, String>)> = std::thread::scope(|s| {
        let jobs: Vec<_> = sources
            .iter()
            .map(|src| {
                s.spawn(move || {
                    let r = match src.kind.as_str() {
                        "ics" => match src.link() {
                            Some(link) => crate::ics::events(&link, from, 7).map(|events| Week { ok: true, events, ..Default::default() }),
                            None => Err("Não consegui ler o link guardado: volta a adicionar este calendário.".into()),
                        },
                        _ => {
                            let w = load_outlook(from);
                            match w.error.clone() {
                                Some(e) if !w.ok => Err(e),
                                _ => Ok(w),
                            }
                        }
                    };
                    (src.name.clone(), r)
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).collect()
    });
    let mut week = Week { at: Some(Instant::now()), epoch: CALENDARS_GEN.load(std::sync::atomic::Ordering::SeqCst), ..Default::default() };
    let mut errors = Vec::new();
    for (i, (name, r)) in results.into_iter().enumerate() {
        match r {
            Ok(w) => {
                week.ok = true;
                if week.account.is_empty() {
                    week.account = w.account;
                }
                for mut e in w.events {
                    e.id = format!("{i}:{}", e.id);
                    week.events.push(e);
                }
            }
            Err(e) => errors.push(if many { format!("{name}: {e}") } else { e }),
        }
    }
    if sources.is_empty() {
        week.ok = true;
    }
    week.events.sort_by(|a, b| a.start.cmp(&b.start));
    if !errors.is_empty() {
        week.error = Some(errors.join(" "));
    }
    week
}

fn load_outlook(from: NaiveDate) -> Week {
    let script = crate::settings::resources_dir().join("calendar.ps1");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .args(["-Days", "7", "-StartDate", &from.format("%Y-%m-%d").to_string()])
        .creation_flags(0x0800_0000)
        .output();
    let fail = |e: String| Week { ok: false, error: Some(e), at: Some(Instant::now()), ..Default::default() };
    let Ok(o) = out else { return fail("Não consegui arrancar o PowerShell.".into()) };
    let text = String::from_utf8_lossy(&o.stdout);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim()) else { return fail("Não consegui ler a agenda do Outlook.".into()) };
    if v["ok"] != true {
        return fail(v["error"].as_str().unwrap_or("O Outlook não respondeu.").to_owned());
    }
    // ConvertTo-Json writes a single event as an object, not a list.
    let events = match &v["events"] {
        serde_json::Value::Array(a) => a.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect(),
        e @ serde_json::Value::Object(_) => serde_json::from_value(e.clone()).ok().into_iter().collect(),
        _ => Vec::new(),
    };
    Week { ok: true, error: None, account: v["account"].as_str().unwrap_or_default().to_owned(), events, at: Some(Instant::now()), epoch: 0 }
}

impl Agenda {
    pub fn new(ctx: egui::Context) -> Self {
        Self { inner: Arc::default(), ctx }
    }

    /// The week starting on `from` (a Monday); loads it in the background when missing or older than 10 minutes.
    pub fn week(&self, from: NaiveDate, force: bool) -> Option<Week> {
        let mut inner = self.inner.lock().unwrap();
        let hit = inner.weeks.get(&from).cloned();
        let epoch = CALENDARS_GEN.load(std::sync::atomic::Ordering::SeqCst);
        let stale = hit.as_ref().is_none_or(|w| w.epoch != epoch || w.at.is_none_or(|a| a.elapsed().as_secs() > 600));
        if (force || stale) && inner.loading.insert(from) {
            let (me, ctx) = (self.inner.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let w = load(from);
                let mut i = me.lock().unwrap();
                i.loading.remove(&from);
                // Keep the last good week when Outlook fails for a moment.
                if w.ok || !i.weeks.get(&from).is_some_and(|x| x.ok) {
                    i.weeks.insert(from, w);
                }
                drop(i);
                ctx.request_repaint();
            });
        }
        hit
    }

    pub fn loading(&self, from: NaiveDate) -> bool {
        self.inner.lock().unwrap().loading.contains(&from)
    }
}

/// Meeting reminders: a toast a few minutes before each meeting; clicking it joins the Teams call.
pub fn remind(agenda: &Agenda, reminded: &mut HashSet<String>) {
    if !crate::settings::load().meeting_reminder {
        return;
    }
    let today = Local::now().date_naive();
    let Some(w) = agenda.week(monday(today), false) else { return };
    let now = Local::now();
    for e in w.events.iter().filter(|e| !e.all_day) {
        let Some(start) = e.start_at() else { continue };
        let mins = (start - now).num_seconds() as f32 / 60.0;
        if mins > 0.0 && mins <= 5.5 && reminded.insert(e.id.clone()) {
            let at = start.format("%H:%M");
            let body = match (e.teams_url(), e.location.is_empty()) {
                (Some(_), _) => format!("{} · clica para entrar no Teams", e.subject),
                (None, false) => format!("{} · {}", e.subject, e.location),
                _ => e.subject.clone(),
            };
            let click = e.teams_url().map(|u| AppEvent::OpenUrl(u.to_owned())).unwrap_or(AppEvent::Show);
            crate::notify::notify(&format!("Reunião às {at}"), &body, click);
        }
    }
}

pub struct AgendaUi {
    pub week: NaiveDate,
    pub selected: Option<String>,
}

impl Default for AgendaUi {
    fn default() -> Self {
        Self { week: monday(Local::now().date_naive()), selected: None }
    }
}

const HOUR_H: f32 = 52.0;

pub fn view(ui: &mut Ui, agenda: &Agenda, st: &mut AgendaUi, acc: Accent) {
    let today = Local::now().date_naive();
    let mut force = false;
    ui.horizontal(|ui| {
        if button(ui, "‹", false, acc).on_hover_text("Semana anterior").clicked() {
            st.week -= Duration::days(7);
        }
        if button(ui, "Hoje", st.week == monday(today), acc).clicked() {
            st.week = monday(today);
        }
        if button(ui, "›", false, acc).on_hover_text("Semana seguinte").clicked() {
            st.week += Duration::days(7);
        }
        let end = st.week + Duration::days(6);
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{} – {} {}", st.week.day(), end.day(), ui_kit::MONTHS[end.month0() as usize])).font(display(theme::size::SUBTITLE)).color(theme::BRIGHT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button(ui, "Atualizar", false, acc).clicked() {
                force = true;
            }
            if agenda.loading(st.week) {
                crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
            }
        });
    });
    ui.add_space(10.0);
    let Some(w) = agenda.week(st.week, force) else {
        ui_kit::quiet(ui, "A ler a agenda…", acc);
        return;
    };
    if crate::profile::calendars().is_empty() {
        ui_kit::hint(ui, "Não tens nenhum calendário ligado. Liga um em Definições → Calendários: o Outlook deste PC, ou o link iCal do Google, do Outlook.com, do iCloud…", false, acc);
        return;
    }
    if let Some(e) = &w.error {
        let outlook = crate::profile::calendars().iter().any(|c| c.kind == "outlook");
        let extra = if outlook { " O Outlook vem do Outlook clássico deste PC: tem de estar instalado e com a conta aberta." } else { "" };
        ui_kit::hint(ui, &format!("{e}{extra} Os calendários ligam-se em Definições → Calendários."), true, acc);
        if w.events.is_empty() {
            return;
        }
    }

    // Weekend columns only when something is on them.
    let days: Vec<NaiveDate> = (0..7)
        .map(|i| st.week + Duration::days(i))
        .filter(|d| d.weekday().num_days_from_monday() < 5 || w.events.iter().any(|e| e.start_at().is_some_and(|s| s.date_naive() == *d)))
        .collect();
    let timed: Vec<&Event> = w.events.iter().filter(|e| !e.all_day).collect();
    let first = timed.iter().filter_map(|e| e.start_at()).map(|s| s.hour()).min().unwrap_or(8).min(8);
    let last = timed.iter().filter_map(|e| e.end_at()).map(|s| s.hour() + u32::from(s.minute() > 0)).max().unwrap_or(19).max(19).min(24);

    // All-day row.
    let all_day: Vec<&Event> = w.events.iter().filter(|e| e.all_day).collect();
    let gutter = 52.0;
    let width = ui.available_width();
    let col_w = (width - gutter) / days.len() as f32;

    let (head, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
    let p = ui.painter().clone();
    for (i, d) in days.iter().enumerate() {
        let x = head.left() + gutter + col_w * i as f32;
        let is_today = *d == today;
        let wd = ui_kit::WEEKDAYS[d.weekday().num_days_from_monday() as usize];
        p.text(pos2(x + 8.0, head.top() + 4.0), Align2::LEFT_TOP, d.day().to_string(), FontId::new(20.0, theme::display()), if is_today { acc.main } else { theme::BRIGHT });
        p.text(pos2(x + 38.0, head.top() + 11.0), Align2::LEFT_TOP, wd.split('-').next().unwrap_or(wd), FontId::proportional(12.5), if is_today { acc.light } else { theme::MUTED });
        if is_today {
            p.line_segment([pos2(x + 4.0, head.bottom() - 2.0), pos2(x + col_w - 4.0, head.bottom() - 2.0)], Stroke::new(2.0, acc.main));
        }
    }
    if !all_day.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(gutter);
            for d in &days {
                ui.allocate_ui(vec2(col_w - 4.0, 26.0), |ui| {
                    for e in all_day.iter().filter(|e| e.start_at().is_some_and(|s| s.date_naive() <= *d) && e.end_at().is_some_and(|s| s.date_naive() > *d)) {
                        ui_kit::pill(ui, &e.subject, acc.light, theme::alpha(acc.main, 0.14), theme::alpha(acc.main, 0.3));
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    let hours = (last - first) as f32;
    egui::ScrollArea::vertical().id_salt("agenda-grid").max_height(ui.available_height().max(300.0)).show(ui, |ui| {
        let (grid, _) = ui.allocate_exact_size(vec2(width, hours * HOUR_H + 8.0), Sense::hover());
        let p = ui.painter().clone();
        for h in 0..=(last - first) {
            let y = grid.top() + h as f32 * HOUR_H;
            p.line_segment([pos2(grid.left() + gutter, y), pos2(grid.right(), y)], Stroke::new(1.0, theme::alpha(acc.main, 0.08)));
            if h < last - first {
                p.text(pos2(grid.left() + gutter - 10.0, y + 2.0), Align2::RIGHT_TOP, format!("{:02}:00", first + h), FontId::monospace(11.0), theme::FAINT);
            }
        }
        for (i, d) in days.iter().enumerate() {
            let x = grid.left() + gutter + col_w * i as f32;
            p.line_segment([pos2(x, grid.top()), pos2(x, grid.bottom())], Stroke::new(1.0, theme::alpha(acc.main, 0.08)));
            if *d == today {
                p.rect_filled(Rect::from_min_size(pos2(x, grid.top()), vec2(col_w, grid.height())), 0.0, theme::alpha(acc.main, 0.025));
            }
            let mut evs: Vec<&Event> = timed.iter().copied().filter(|e| e.start_at().is_some_and(|s| s.date_naive() == *d)).collect();
            evs.sort_by_key(|e| e.start.clone());
            // Overlapping meetings share the column: greedy lanes.
            let mut lanes: Vec<DateTime<Local>> = Vec::new();
            let mut placed = Vec::new();
            for e in &evs {
                let (Some(s), Some(en)) = (e.start_at(), e.end_at()) else { continue };
                let lane = lanes.iter().position(|end| *end <= s).unwrap_or_else(|| {
                    lanes.push(s);
                    lanes.len() - 1
                });
                lanes[lane] = en;
                placed.push((e, s, en, lane));
            }
            let n = lanes.len().max(1) as f32;
            for (e, s, en, lane) in placed {
                let top = grid.top() + ((s.hour() as f32 + s.minute() as f32 / 60.0) - first as f32) * HOUR_H;
                let bottom = grid.top() + ((en.hour() as f32 + en.minute() as f32 / 60.0) - first as f32).max(0.0) * HOUR_H;
                let bottom = if en.date_naive() > *d { grid.bottom() } else { bottom.max(top + 22.0) };
                let lw = (col_w - 6.0) / n;
                let r = Rect::from_min_max(pos2(x + 3.0 + lw * lane as f32, top + 1.0), pos2(x + 3.0 + lw * (lane as f32 + 1.0) - 2.0, bottom - 1.0));
                let past = en < Local::now();
                let declined = e.response == 4;
                let selected = st.selected.as_deref() == Some(&e.id);
                let resp = ui.interact(r, ui.scope_id().with(("ev", &e.id)), Sense::click());
                let fill = theme::alpha(if e.busy == 3 { theme::AMBER } else { acc.main }, if past { 0.08 } else if selected || resp.hovered() { 0.28 } else { 0.18 });
                p.rect(r, CornerRadius::same(3), fill, Stroke::new(1.0, theme::alpha(acc.main, if selected { 0.9 } else { 0.35 })), StrokeKind::Inside);
                p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, if past { theme::FAINT } else { acc.main });
                let color = if past || declined { theme::MUTED } else { theme::BRIGHT };
                let clip = p.with_clip_rect(r.shrink(2.0));
                clip.text(pos2(r.left() + 8.0, r.top() + 4.0), Align2::LEFT_TOP, &e.subject, FontId::proportional(13.0), color);
                if r.height() > 38.0 {
                    clip.text(pos2(r.left() + 8.0, r.top() + 21.0), Align2::LEFT_TOP, format!("{}–{}", s.format("%H:%M"), en.format("%H:%M")), FontId::monospace(11.0), theme::MUTED);
                }
                if resp.clicked() {
                    st.selected = if selected { None } else { Some(e.id.clone()) };
                }
                resp.on_hover_text(format!("{}\n{}–{}{}", e.subject, s.format("%H:%M"), en.format("%H:%M"), if e.location.is_empty() { String::new() } else { format!("\n{}", e.location) }));
            }
        }
        let now = Local::now();
        if let Some(i) = days.iter().position(|d| *d == now.date_naive()) {
            let y = grid.top() + ((now.hour() as f32 + now.minute() as f32 / 60.0) - first as f32) * HOUR_H;
            if y > grid.top() && y < grid.bottom() {
                let x = grid.left() + gutter + col_w * i as f32;
                p.line_segment([pos2(x, y), pos2(x + col_w, y)], Stroke::new(2.0, theme::RED));
                p.circle_filled(pos2(x, y), 4.0, theme::RED);
            }
        }
    });

    if let Some(e) = st.selected.as_ref().and_then(|id| w.events.iter().find(|e| &e.id == id)) {
        ui.add_space(10.0);
        ui_kit::hud_panel(ui, acc, false, |ui| {
            ui.label(RichText::new(&e.subject).size(theme::size::CARD).strong().color(theme::BRIGHT));
            if let (Some(s), Some(en)) = (e.start_at(), e.end_at()) {
                let wd = ui_kit::WEEKDAYS[s.weekday().num_days_from_monday() as usize];
                ui.label(RichText::new(format!("{wd}, {} · {}–{}", s.format("%d/%m"), s.format("%H:%M"), en.format("%H:%M"))).monospace().color(theme::MUTED));
            }
            for line in [&e.location, &e.organizer].into_iter().filter(|l| !l.is_empty()) {
                ui.label(RichText::new(line).color(theme::MUTED));
            }
            if let Some(u) = e.teams_url() {
                ui.add_space(6.0);
                if button(ui, "Entrar no Teams", true, acc).clicked() {
                    post(AppEvent::OpenUrl(u.to_owned()));
                }
            }
        });
    }
}

/// Today's remaining meetings, for the Hoje screen.
pub fn today_left(agenda: &Agenda) -> Vec<Event> {
    let today = Local::now().date_naive();
    let now = Local::now();
    agenda
        .week(monday(today), false)
        .map(|w| w.events.into_iter().filter(|e| !e.all_day && e.start_at().is_some_and(|s| s.date_naive() == today) && e.end_at().is_some_and(|en| en > now)).collect())
        .unwrap_or_default()
}


/// All of today's meetings (past ones too), for the start screen's timeline.
pub fn today_all(agenda: &Agenda) -> Vec<Event> {
    let today = Local::now().date_naive();
    let mut v: Vec<Event> = agenda.week(monday(today), false).map(|w| w.events.into_iter().filter(|e| e.start_at().is_some_and(|s| s.date_naive() == today)).collect()).unwrap_or_default();
    v.sort_by(|a, b| a.start.cmp(&b.start));
    v
}

/// Bumped when the person changes the connected calendars, so the weeks already read are read again.
pub static CALENDARS_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
