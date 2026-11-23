//! Tabs a team pack adds to the menu (`[[tab]]` in its jarvis.toml): a title, a line about it and a few actions, each one
//! either a request to the AI ("faz-me o ppt do sprint {input}", answered in a conversation that uses the pack's skills)
//! or something to open (a link or a file). Nothing about any team is in Jarvis: the pack says what its tab holds.

use crate::{
    profile::{TabAction, TabCfg},
    theme::{self, Accent},
    ui_kit::{self, button},
};
use eframe::egui::{self, RichText, Ui};
use std::collections::HashMap;

/// What was typed in the actions' text boxes, by "<pack>/<tab>/<action>".
#[derive(Default)]
pub struct PackUi {
    pub inputs: HashMap<String, String>,
    pub kpis: crate::pack_kpi::Kpis,
}

/// What a click asks the app to do.
pub enum Do {
    /// Open a conversation titled this and send it this request.
    Ask(String, String),
    /// Open a link or file.
    Open(String),
}

/// `{input}` and `{sprint}` filled in.
fn fill(text: &str, input: &str, sprint: &str) -> String {
    text.replace("{input}", input.trim()).replace("{sprint}", sprint)
}

pub fn view(ui: &mut Ui, tab: &TabCfg, st: &mut PackUi, sprint: &str, acc: Accent) -> Option<Do> {
    let mut out = None;
    if tab.kpis.is_some() {
        crate::pack_kpi::view(ui, tab, &mut st.kpis, acc);
    }
    if tab.actions.is_empty() && tab.kpis.is_none() {
        ui_kit::quiet(ui, "Este separador do pacote ainda não tem ações.", acc);
    }
    for a in &tab.actions {
        let key = format!("{}/{}/{}", tab.pack, tab.title, a.label);
        ui_kit::hud_panel(ui, acc, false, |ui| {
            ui.label(RichText::new(&a.label).size(theme::size::CARD).strong().color(theme::BRIGHT));
            if !a.hint.is_empty() {
                ui.label(RichText::new(&a.hint).color(theme::MUTED));
            }
            let value = st.inputs.entry(key.clone()).or_insert_with(|| fill(&a.default, "", sprint));
            if !a.input.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&a.input).small().color(theme::MUTED));
                    ui.add(egui::TextEdit::singleline(value).desired_width(160.0));
                });
            }
            ui.add_space(4.0);
            let needs = !a.input.is_empty() && value.trim().is_empty();
            ui.add_enabled_ui(!needs, |ui| {
                if button(ui, if a.prompt.is_empty() { "Abrir" } else { "Pedir ao Jarvis" }, true, acc).clicked() {
                    out = action(a, tab, value, sprint);
                }
            });
        });
        ui.add_space(10.0);
    }
    out
}

fn action(a: &TabAction, tab: &TabCfg, input: &str, sprint: &str) -> Option<Do> {
    if !a.prompt.is_empty() {
        return Some(Do::Ask(format!("{}: {}", tab.title, fill(&a.label, input, sprint)), fill(&a.prompt, input, sprint)));
    }
    (!a.open.is_empty()).then(|| Do::Open(fill(&a.open, input, sprint)))
}
