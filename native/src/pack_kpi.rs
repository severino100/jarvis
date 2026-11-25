//! KPI cards of a pack tab (`[tab.kpis]` in the pack's jarvis.toml): a command of the pack prints JSON, the pack says where
//! each part is (dotted paths), and Jarvis draws the cards: value, how it moved since the last reading, a small trend
//! line and the detail. Nothing about any tool or team is known here.

use crate::{
    profile::TabCfg,
    theme::{self, Accent},
    ui_kit::{self, button},
};
use eframe::egui::{self, RichText, Ui};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

/// One card, read from the command's JSON with the paths the pack gave.
#[derive(Clone, Default)]
struct Kpi {
    label: String,
    value: Option<f64>,
    detail: String,
    unit: String,
    better: String,
    trend: Vec<(String, f64)>,
    group: String,
    error: String,
    /// The card's button: its label and what it is about (both empty: no button).
    action: String,
    action_arg: String,
    /// The tag the filter chips select by (empty: no filter).
    filter: String,
}

#[derive(Default)]
struct State {
    loading: bool,
    items: Vec<Kpi>,
    /// The filter tags in the order the pack gave them (empty: in order of appearance).
    filters: Vec<String>,
    title: String,
    updated: String,
    error: Option<String>,
    at: Option<Instant>,
}

type Shared = Arc<Mutex<State>>;

/// The readings of every tab, by "<pack>/<tab>".
#[derive(Default)]
pub struct Kpis {
    tabs: HashMap<String, Shared>,
    /// The filter chips switched on, by "<pack>/<tab>" (none: every card).
    picked: HashMap<String, Vec<String>>,
}

fn at<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(v, |v, k| match k.parse::<usize>() {
        Ok(i) if v.is_array() => v.get(i),
        _ => v.get(k),
    })
}

fn text(v: &serde_json::Value, path: &str) -> String {
    at(v, path).map(|x| x.as_str().map(str::to_owned).unwrap_or_else(|| if x.is_null() { String::new() } else { x.to_string() })).unwrap_or_default()
}

/// Like `text`, but an empty path means nothing (not the whole object).
fn opt(v: &serde_json::Value, path: &str) -> String {
    if path.is_empty() { String::new() } else { text(v, path) }
}

/// Reads the numbers off the UI thread. Does nothing while a reading is running.
fn load(tab: &TabCfg, state: &Shared, ctx: &egui::Context) {
    let Some(cfg) = tab.kpis.clone() else { return };
    {
        let mut s = state.lock().unwrap();
        if s.loading {
            return;
        }
        s.loading = true;
    }
    let dir = crate::profile::packs_active().into_iter().find(|p| p.dir.file_name().is_some_and(|n| n.to_string_lossy() == tab.pack)).map(|p| p.dir);
    let (state, ctx) = (state.clone(), ctx.clone());
    std::thread::spawn(move || {
        let result = run(&cfg, dir);
        let mut s = state.lock().unwrap();
        match result {
            Ok((items, title, updated, filters)) => {
                s.items = items;
                s.filters = filters;
                s.title = title;
                s.updated = updated;
                s.error = None;
            }
            Err(e) => s.error = Some(e),
        }
        s.at = Some(Instant::now());
        s.loading = false;
        drop(s);
        ctx.request_repaint();
    });
}

/// The command runs in the pack's folder with the same environment as the skills (the board variables and the token).
fn run(cfg: &crate::profile::KpiCfg, dir: Option<std::path::PathBuf>) -> Result<(Vec<Kpi>, String, String, Vec<String>), String> {
    use std::os::windows::process::CommandExt;
    let dir = dir.ok_or("O pacote deste separador já não está ligado.")?;
    let mut words = cfg.run.split_whitespace();
    let prog = words.next().ok_or("O separador não diz que comando corre (run).")?;
    let local = dir.join(prog.replace('/', "\\"));
    let mut cmd = std::process::Command::new(if local.is_file() { local } else { std::path::PathBuf::from(prog) });
    cmd.args(words).current_dir(&dir).creation_flags(0x0800_0000).stdin(std::process::Stdio::null());
    crate::profile::child_env(&mut cmd);
    let out = cmd.output().map_err(|e| format!("Não consegui correr {prog}: {e}. Se for a ferramenta do pacote, falta compilá-la."))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{prog} falhou: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("sem mensagem")));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("A resposta de {prog} não é JSON: {e}"))?;
    let list = at(&v, &cfg.items).and_then(|l| l.as_array()).ok_or_else(|| format!("Não encontrei a lista \"{}\" na resposta.", cfg.items))?;
    let items = list
        .iter()
        .map(|k| Kpi {
            label: text(k, &cfg.label),
            value: at(k, &cfg.value).and_then(|x| x.as_f64()),
            detail: text(k, &cfg.detail),
            unit: text(k, &cfg.unit),
            better: text(k, &cfg.better),
            trend: at(k, &cfg.trend)
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_str().unwrap_or_default().to_owned(), p.get(1)?.as_f64()?))).collect())
                .unwrap_or_default(),
            group: text(k, &cfg.group),
            error: text(k, &cfg.error),
            action: opt(k, &cfg.action),
            action_arg: opt(k, &cfg.action_arg),
            filter: opt(k, &cfg.filter),
        })
        .collect();
    let filters = if cfg.filters.is_empty() {
        Vec::new()
    } else {
        at(&v, &cfg.filters).and_then(|l| l.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    Ok((items, text(&v, &cfg.title), text(&v, &cfg.updated), filters))
}

fn number(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}").replace('.', ",")
    }
}

/// The KPI cards of a tab, grouped; read again when the pack's `refresh_minutes` have passed or on "Atualizar".
/// Returns the request of a card's button when it was clicked: (conversation title, prompt).
pub fn view(ui: &mut Ui, tab: &TabCfg, st: &mut Kpis, acc: Accent) -> Option<(String, String)> {
    let mut asked: Option<(String, String)> = None;
    let prompt = tab.kpis.as_ref().map(|c| c.action_prompt.clone()).unwrap_or_default();
    let state = st.tabs.entry(format!("{}/{}", tab.pack, tab.title)).or_default().clone();
    let every = tab.kpis.as_ref().map(|c| c.refresh_minutes).unwrap_or(0);
    let stale = state.lock().unwrap().at.is_none_or(|at| every > 0 && at.elapsed().as_secs() > every * 60);
    if stale {
        load(tab, &state, ui.ctx());
    }
    let mut reload = false;
    let s = state.lock().unwrap();
    ui.horizontal(|ui| {
        if button(ui, "Atualizar", false, acc).clicked() && !s.loading {
            reload = true;
        }
        if s.loading {
            crate::motion::spinner(ui, 16.0, acc.main);
            ui.label(RichText::new("A ler os números…").small().color(theme::MUTED));
        } else {
            let line = [s.title.as_str(), s.updated.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" · ");
            ui.label(RichText::new(line).monospace().size(12.0).color(theme::MUTED));
        }
    });
    ui.add_space(8.0);
    if let Some(e) = &s.error {
        ui_kit::hint(ui, e, true, acc);
        ui.add_space(8.0);
    }
    if let Some(c) = tab.kpis.as_ref().filter(|c| c.layout == "dashboard") {
        dashboard(ui, &s.items, &c.hero, acc);
        drop(s);
        if reload {
            state.lock().unwrap().at = None;
            ui.ctx().request_repaint();
        }
        return asked;
    }
    // Filter chips: "Todos" plus one per tag; several can be on at once, none on means every card.
    let filter_on = tab.kpis.as_ref().is_some_and(|c| !c.filter.is_empty());
    let key = format!("{}/{}", tab.pack, tab.title);
    let mut picked = st.picked.get(&key).cloned().unwrap_or_default();
    if filter_on {
        let mut tags: Vec<String> = s.filters.clone();
        for k in &s.items {
            if !k.filter.is_empty() && !tags.contains(&k.filter) {
                tags.push(k.filter.clone());
            }
        }
        picked.retain(|p| tags.contains(p));
        ui.horizontal_wrapped(|ui| {
            if button(ui, &format!("Todos · {}", s.items.len()), picked.is_empty(), acc).clicked() {
                picked.clear();
            }
            for t in &tags {
                let n = s.items.iter().filter(|k| &k.filter == t).count();
                if n > 0 && button(ui, &format!("{t} · {n}"), picked.contains(t), acc).clicked() {
                    match picked.iter().position(|p| p == t) {
                        Some(i) => {
                            picked.remove(i);
                        }
                        None => picked.push(t.clone()),
                    }
                }
            }
        });
        ui.add_space(8.0);
        st.picked.insert(key, picked.clone());
    }
    let shown: Vec<&Kpi> = s.items.iter().filter(|k| picked.is_empty() || picked.contains(&k.filter)).collect();
    if filter_on && shown.is_empty() {
        ui_kit::quiet(ui, "Nada com esta seleção.", acc);
    }
    let mut groups: Vec<&str> = Vec::new();
    for k in &shown {
        if !groups.contains(&k.group.as_str()) {
            groups.push(&k.group);
        }
    }
    for g in groups {
        let cards: Vec<&Kpi> = shown.iter().copied().filter(|k| k.group == g).collect();
        if !g.is_empty() {
            let label = if filter_on { format!("{g} ({})", cards.len()) } else { g.to_owned() };
            ui_kit::section_label(ui, &label, acc.main, acc);
        }
        let per_row = ((ui.available_width() / 300.0).floor() as usize).clamp(1, 8);
        for row in cards.chunks(per_row) {
            ui.columns(per_row, |cols| {
                for (col, k) in cols.iter_mut().zip(row) {
                    if card(col, k, acc) {
                        asked = Some((format!("{}: {} {}", tab.title, k.action, k.action_arg), prompt.replace("{arg}", &k.action_arg).replace("{label}", &k.label)));
                    }
                }
            });
            ui.add_space(8.0);
        }
    }
    drop(s);
    if reload {
        state.lock().unwrap().at = None;
        ui.ctx().request_repaint();
    }
    ui.add_space(6.0);
    asked
}

/// One card; true when its button was clicked.
fn card(ui: &mut Ui, k: &Kpi, acc: Accent) -> bool {
    let mut clicked = false;
    let last = k.trend.last().map(|t| t.1);
    let prev = k.trend.len().checked_sub(2).and_then(|i| k.trend.get(i)).map(|t| t.1);
    let delta = last.zip(prev).map(|(a, b)| a - b).filter(|d| d.abs() >= 0.05);
    let tone = match (delta, k.better.as_str()) {
        (Some(d), "up") => if d > 0.0 { theme::GREEN } else { theme::AMBER },
        (Some(d), "down") => if d < 0.0 { theme::GREEN } else { theme::AMBER },
        _ => theme::MUTED,
    };
    ui_kit::hud_panel(ui, acc, !k.error.is_empty(), |ui| {
        ui.set_min_height(if k.action.is_empty() { 112.0 } else { 168.0 });
        ui.set_width(ui.available_width());
        ui.label(RichText::new(k.label.to_uppercase()).font(ui_kit::display(11.5)).color(theme::MUTED).extra_letter_spacing(1.2));
        if !k.error.is_empty() {
            ui.label(RichText::new(&k.error).small().color(theme::AMBER));
            return;
        }
        ui.horizontal(|ui| {
            let big = match k.value {
                Some(v) => format!("{}{}", number(v), if k.unit.is_empty() { String::new() } else { format!(" {}", k.unit) }),
                None => "—".to_owned(),
            };
            ui.label(RichText::new(big).monospace().size(theme::size::STAT).strong().color(acc.light));
            if let Some(d) = delta {
                ui.label(RichText::new(format!("{}{}", if d > 0.0 { "+" } else { "" }, number(d))).monospace().size(12.0).color(tone));
            }
        });
        if !k.detail.is_empty() {
            ui.add(egui::Label::new(RichText::new(&k.detail).small().color(theme::MUTED)).wrap());
        }
        if k.trend.len() > 1 {
            spark(ui, &k.trend, tone, acc);
        }
        if !k.action.is_empty() {
            ui.add_space(6.0);
            if button(ui, &k.action, true, acc).clicked() {
                clicked = true;
            }
        }
    });
    clicked
}

/// The last readings as a small line with a dot on the newest, and their labels under the ends.
fn spark(ui: &mut Ui, trend: &[(String, f64)], tone: egui::Color32, acc: Accent) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
    let (lo, hi) = trend.iter().fold((f64::MAX, f64::MIN), |(l, h), t| (l.min(t.1), h.max(t.1)));
    let span = (hi - lo).max(1e-9);
    let pts: Vec<egui::Pos2> = trend
        .iter()
        .enumerate()
        .map(|(i, t)| egui::pos2(rect.left() + 4.0 + (rect.width() - 8.0) * i as f32 / (trend.len() - 1) as f32, rect.bottom() - 4.0 - (rect.height() - 8.0) * ((t.1 - lo) / span) as f32))
        .collect();
    let p = ui.painter();
    p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.5, theme::alpha(acc.main, 0.7))));
    if let Some(end) = pts.last() {
        p.circle_filled(*end, 3.0, tone);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(&trend[0].0).monospace().size(10.0).color(theme::FAINT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(&trend[trend.len() - 1].0).monospace().size(10.0).color(theme::FAINT));
        });
    });
}

/* ---------- layout "dashboard" ---------- */

fn tone_of(k: &Kpi) -> (Option<f64>, egui::Color32) {
    let last = k.trend.last().map(|t| t.1);
    let prev = k.trend.len().checked_sub(2).and_then(|i| k.trend.get(i)).map(|t| t.1);
    let delta = last.zip(prev).map(|(a, b)| a - b).filter(|d| d.abs() >= 0.05);
    let tone = match (delta, k.better.as_str()) {
        (Some(d), "up") => if d > 0.0 { theme::GREEN } else { theme::AMBER },
        (Some(d), "down") => if d < 0.0 { theme::GREEN } else { theme::AMBER },
        _ => theme::MUTED,
    };
    (delta, tone)
}

fn big(k: &Kpi) -> String {
    match k.value {
        Some(v) => format!("{}{}", number(v), if k.unit.is_empty() { String::new() } else { format!(" {}", k.unit) }),
        None => "—".to_owned(),
    }
}

/// The trend as a thin line with a dot on the newest reading, inside `r`.
fn mini_spark(ui: &Ui, r: egui::Rect, trend: &[(String, f64)], tone: egui::Color32) {
    if trend.len() < 2 {
        return;
    }
    let (lo, hi) = trend.iter().fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t.1), b.max(t.1)));
    let span = (hi - lo).max(f64::EPSILON);
    let pts: Vec<egui::Pos2> = trend
        .iter()
        .enumerate()
        .map(|(i, t)| egui::pos2(r.left() + r.width() * i as f32 / (trend.len() - 1) as f32, r.bottom() - r.height() * ((t.1 - lo) / span) as f32))
        .collect();
    let p = ui.painter();
    p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.5, theme::alpha(tone, 0.85))));
    if let Some(last) = pts.last() {
        p.circle_filled(*last, 2.8, tone);
    }
}

/// One KPI as a line of a panel: name, value, change since the last reading and the trend. The explanation is on hover.
fn kpi_row(ui: &mut Ui, k: &Kpi, acc: Accent) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::hover());
    let (delta, tone) = tone_of(k);
    let (spark_w, delta_w, val_w) = (74.0, 52.0, 96.0);
    let p = ui.painter();
    p.line_segment([rect.left_bottom(), rect.right_bottom()], egui::Stroke::new(1.0, theme::alpha(acc.main, 0.08)));
    let spark = egui::Rect::from_min_max(egui::pos2(rect.right() - spark_w, rect.top() + 9.0), egui::pos2(rect.right(), rect.bottom() - 9.0));
    let delta_right = spark.left() - 10.0;
    let val_right = delta_right - delta_w;
    if !k.error.is_empty() {
        p.text(egui::pos2(rect.right(), rect.center().y), egui::Align2::RIGHT_CENTER, "erro", egui::FontId::proportional(13.0), theme::AMBER);
    } else {
        p.text(egui::pos2(val_right, rect.center().y), egui::Align2::RIGHT_CENTER, big(k), egui::FontId::monospace(18.0), acc.light);
        if let Some(d) = delta {
            p.text(egui::pos2(delta_right, rect.center().y), egui::Align2::RIGHT_CENTER, format!("{}{}", if d > 0.0 { "+" } else { "" }, number(d)), egui::FontId::monospace(11.5), tone);
        }
        mini_spark(ui, spark, &k.trend, tone);
    }
    let mut job = egui::text::LayoutJob::simple(k.label.clone(), egui::FontId::proportional(13.5), theme::TEXT, (val_right - val_w - rect.left()).max(60.0));
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
    let tip = if k.error.is_empty() { k.detail.clone() } else { k.error.clone() };
    if !tip.is_empty() {
        resp.on_hover_text(format!("{}\n\n{}", k.label, tip));
    }
}

/// The whole tab as a dashboard.
fn dashboard(ui: &mut Ui, items: &[Kpi], hero: &str, acc: Accent) {
    let mut groups: Vec<&str> = Vec::new();
    for k in items {
        if k.group != hero && !groups.contains(&k.group.as_str()) {
            groups.push(&k.group);
        }
    }
    // The big numbers.
    let top: Vec<&Kpi> = items.iter().filter(|k| !hero.is_empty() && k.group == hero).collect();
    if !top.is_empty() {
        let per_row = ((ui.available_width() / 190.0).floor() as usize).clamp(1, top.len());
        for row in top.chunks(per_row) {
            ui.columns(per_row, |cols| {
                for (col, k) in cols.iter_mut().zip(row) {
                    ui_kit::hud_panel(col, acc, false, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(k.label.to_uppercase()).font(ui_kit::display(11.0)).color(theme::MUTED).extra_letter_spacing(1.2));
                        ui.label(RichText::new(big(k)).monospace().size(34.0).strong().color(acc.light));
                        if !k.detail.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&k.detail).small().color(theme::MUTED)).truncate());
                        }
                    });
                }
            });
            ui.add_space(10.0);
        }
    }
    // The panels, side by side: each group a panel, dealt out one column after the other.
    let cols_n = ((ui.available_width() / 430.0).floor() as usize).clamp(1, 4);
    ui.columns(cols_n, |cols| {
        for (c, col) in cols.iter_mut().enumerate() {
            for g in groups.iter().skip(c).step_by(cols_n) {
                ui_kit::hud_panel(col, acc, false, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(g.to_uppercase()).font(ui_kit::display(12.0)).color(acc.main).extra_letter_spacing(1.6));
                    ui.add_space(4.0);
                    for k in items.iter().filter(|k| k.group == *g) {
                        kpi_row(ui, k, acc);
                    }
                });
                col.add_space(10.0);
            }
        }
    });
}
