//! A table in a pack tab (`[tab.table]` in the pack's jarvis.toml): a command of the pack prints JSON, the pack says which
//! list is the rows and which path fills each column, and Jarvis draws the table. Columns marked `sum` get a total row.
//! Nothing about any tool or team is known here.

use crate::{
    profile::{ColCfg, TabCfg},
    theme::{self, Accent},
    ui_kit,
};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

/// One cell: what to show, and the number behind it when it is one (for the total).
type Cell = (String, Option<f64>);

#[derive(Default)]
struct State {
    loading: bool,
    rows: Vec<Vec<Cell>>,
    error: Option<String>,
    at: Option<Instant>,
}

type Shared = Arc<Mutex<State>>;

/// The tables of every tab, by "<pack>/<tab>".
#[derive(Default)]
pub struct Tables {
    tabs: HashMap<String, Shared>,
}

fn at<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(v, |v, k| match k.parse::<usize>() {
        Ok(i) if v.is_array() => v.get(i),
        _ => v.get(k),
    })
}

fn number(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{v:.0}") } else { format!("{v:.1}").replace('.', ",") }
}

/// Numbers sit on the right, text on the left, unless the pack says `align`.
fn right(c: &ColCfg) -> bool {
    match c.align.as_str() {
        "right" => true,
        "left" => false,
        _ => c.sum,
    }
}

fn read(tab: &TabCfg, dir: Option<std::path::PathBuf>) -> Result<Vec<Vec<Cell>>, String> {
    use std::os::windows::process::CommandExt;
    let cfg = tab.table.as_ref().ok_or("Sem tabela.")?;
    let dir = dir.ok_or("O pacote deste separador já não está ligado.")?;
    let mut words = cfg.run.split_whitespace();
    let prog = words.next().ok_or("A tabela não diz que comando corre (run).")?;
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
    Ok(list
        .iter()
        .map(|row| {
            cfg.columns
                .iter()
                .map(|c| match at(row, &c.path) {
                    Some(x) if x.is_number() => (number(x.as_f64().unwrap_or(0.0)), x.as_f64()),
                    Some(x) if x.is_string() => (x.as_str().unwrap_or_default().to_owned(), None),
                    _ => (String::new(), None),
                })
                .collect()
        })
        .collect())
}

fn load(tab: &TabCfg, state: &Shared, ctx: &egui::Context) {
    {
        let mut s = state.lock().unwrap();
        if s.loading {
            return;
        }
        s.loading = true;
    }
    let dir = crate::profile::packs_active().into_iter().find(|p| p.dir.file_name().is_some_and(|n| n.to_string_lossy() == tab.pack)).map(|p| p.dir);
    let (state, ctx, tab) = (state.clone(), ctx.clone(), tab.clone());
    std::thread::spawn(move || {
        let result = read(&tab, dir);
        let mut s = state.lock().unwrap();
        match result {
            Ok(rows) => {
                s.rows = rows;
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

fn put(ui: &mut Ui, c: &ColCfg, t: RichText) {
    if right(c) {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(t));
    } else {
        ui.label(t);
    }
}

pub fn view(ui: &mut Ui, tab: &TabCfg, st: &mut Tables, acc: Accent) {
    let Some(cfg) = tab.table.as_ref() else { return };
    let state = st.tabs.entry(format!("{}/{}", tab.pack, tab.title)).or_default().clone();
    if state.lock().unwrap().at.is_none_or(|at| cfg.refresh_minutes > 0 && at.elapsed().as_secs() > cfg.refresh_minutes * 60) {
        load(tab, &state, ui.ctx());
    }
    let s = state.lock().unwrap();
    if !cfg.title.is_empty() {
        ui_kit::section_label(ui, &cfg.title, acc.main, acc);
    }
    if let Some(e) = &s.error {
        ui_kit::hint(ui, e, true, acc);
        return;
    }
    if s.rows.is_empty() {
        if s.loading {
            ui.horizontal(|ui| {
                crate::motion::spinner(ui, 16.0, acc.main);
                ui.label(RichText::new("A ler…").small().color(theme::MUTED));
            });
        } else {
            ui_kit::quiet(ui, "Sem linhas.", acc);
        }
        return;
    }
    let n = cfg.columns.len().max(1) as f32;
    let col_w = ((ui.available_width() - 70.0 - 22.0 * (n - 1.0)) / n).max(60.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(format!("pack-table-{}-{}", tab.pack, tab.title)).striped(true).num_columns(cfg.columns.len()).min_col_width(col_w).spacing([22.0, 8.0]).show(ui, |ui| {
            for c in &cfg.columns {
                put(ui, c, RichText::new(c.title.to_uppercase()).small().color(theme::MUTED));
            }
            ui.end_row();
            for row in &s.rows {
                for (i, c) in cfg.columns.iter().enumerate() {
                    let (text, n) = &row[i];
                    let t = if n.is_some_and(|n| n == 0.0) {
                        RichText::new("·").color(theme::MUTED)
                    } else if i == 0 {
                        RichText::new(text).strong().color(theme::BRIGHT)
                    } else {
                        RichText::new(text).monospace()
                    };
                    put(ui, c, t);
                }
                ui.end_row();
            }
            if cfg.columns.iter().any(|c| c.sum) {
                for (i, c) in cfg.columns.iter().enumerate() {
                    let t = if c.sum {
                        RichText::new(number(s.rows.iter().filter_map(|r| r[i].1).sum())).monospace().strong().color(acc.light)
                    } else if i == 0 {
                        RichText::new("TOTAL").small().strong().color(theme::MUTED)
                    } else {
                        RichText::new("")
                    };
                    put(ui, c, t);
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(8.0);
}
