//! Chat view (list + conversation) and the conversation windows. Drawing only: every click becomes an `Action` that
//! the app applies after the frame, outside the conversations lock.

use crate::{
    azdo,
    chat::{Chat, Msg},
    markdown,
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use eframe::egui::{self, vec2, Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Modifiers, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui};
use std::collections::HashMap;

pub enum Action {
    New,
    Select(String),
    Send(String, String),
    Stop(String),
    Delete(String),
    OpenInClaude(String),
    PopOut(String),
    /// claude | antigravity | other
    Implement(String, &'static str),
    /// conversation, engine (claude | antigravity), model
    SetEngine(String, Option<String>, Option<String>),
    /// Summarise and continue in a new session.
    Compact(String),
    /// conversation, mode (dev | rh)
    SetMode(String, String),
    /// "Desfazer" on a remembered rule: conversation, message
    Forget(String, String),
    /// "Desfazer" on a personal command the Jarvis created: conversation, message
    UndoSkill(String, String),
    /// "Começar de novo" in the Jarvis page: a new free conversation (the old one stays in Chat).
    FreshJarvis,
    /// The microphone button of a free conversation: talk in this conversation (or stop talking).
    Voice(String),
}

#[derive(Default)]
pub struct ChatUi {
    pub selected: Option<String>,
    pub drafts: HashMap<String, String>,
    /// Conversations open in their own window.
    pub windows: Vec<String>,
    /// Windows to bring to the front on the next frame (just opened, or opened again).
    pub focus: Vec<String>,
    pub confirm_delete: Option<String>,
    /// Short feedback after an action (e.g. "pedido copiado"), per conversation.
    pub flash: HashMap<String, (String, std::time::Instant)>,
    /// A spoken conversation is on (set by the app each frame).
    pub voice_on: bool,
}

/// Chat section of the main window: conversation list on the left, the selected conversation on the right.
pub fn page(ui: &mut Ui, chats: &[Chat], st: &mut ChatUi, acc: Accent) -> Vec<Action> {
    let mut acts = Vec::new();
    if st.selected.as_ref().is_none_or(|s| !chats.iter().any(|c| &c.id == s)) {
        st.selected = chats.first().map(|c| c.id.clone());
    }
    let h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(290.0, h), Layout::top_down(Align::Min), |ui| {
            ui.set_width(290.0);
            if button(ui, "Nova conversa", true, acc).clicked() {
                acts.push(Action::New);
            }
            ui.add_space(10.0);
            ScrollArea::vertical().id_salt("chat-list").auto_shrink([false, false]).show(ui, |ui| {
                if chats.is_empty() {
                    ui.label(RichText::new("Ainda não há conversas. Carrega em Começar num item ou abre uma nova.").color(theme::MUTED));
                }
                for c in chats {
                    if list_entry(ui, c, st.selected.as_deref() == Some(&c.id), acc).clicked() {
                        acts.push(Action::Select(c.id.clone()));
                    }
                }
            });
        });
        ui.add_space(18.0);
        ui.vertical(|ui| {
            ui.set_min_height(h);
            match st.selected.clone().and_then(|s| chats.iter().find(|c| c.id == s)) {
                Some(c) => acts.extend(conversation(ui, c, st, acc, false)),
                None => ui_kit::quiet(ui, "Escolhe uma conversa ou começa uma nova.", acc),
            }
        });
    });
    acts
}

fn list_entry(ui: &mut Ui, c: &Chat, active: bool, acc: Accent) -> egui::Response {
    let r = Frame::new()
        .fill(if active { theme::alpha(acc.main, 0.13) } else { theme::GLASS })
        .stroke(Stroke::new(1.0, theme::alpha(acc.main, if active { 0.3 } else { 0.1 })))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if c.busy {
                    crate::motion::spinner(ui, 12.0 + 2.0, acc.main);
                } else if c.handed_off {
                    ui_kit::diamond(ui, false, theme::FAINT, 4.0);
                } else {
                    ui_kit::diamond(ui, true, if active { acc.main } else { theme::alpha(theme::MUTED, 0.6) }, 4.0);
                }
                ui.add(egui::Label::new(RichText::new(&c.title).strong().color(if active { theme::BRIGHT } else { theme::TEXT })).truncate());
            });
            let preview = c.preview();
            if !preview.is_empty() {
                ui.add(egui::Label::new(RichText::new(preview).small().color(theme::MUTED)).truncate());
            }
            ui.label(RichText::new(ui_kit::when(&c.updated_at)).monospace().size(10.5).color(theme::FAINT));
        })
        .response
        .interact(Sense::click());
    ui.add_space(6.0);
    r
}

/// One conversation: header, messages and the composer. `in_window` hides the pop-out button.
pub fn conversation(ui: &mut Ui, c: &Chat, st: &mut ChatUi, acc: Accent, in_window: bool) -> Vec<Action> {
    let mut acts = Vec::new();
    header(ui, c, st, acc, in_window, &mut acts);
    ui.add_space(6.0);
    let composer_h = if c.handed_off { 56.0 } else { 150.0 };
    let msgs_h = (ui.available_height() - composer_h).max(120.0);
    Frame::new().fill(theme::alpha(Color32::BLACK, 0.18)).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.1))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ScrollArea::vertical()
            .id_salt(("msgs", &c.id))
            .max_height(msgs_h - 24.0)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(980.0));
                if c.messages.is_empty() {
                    let empty = if c.mode() == crate::profile::GERAL {
                        "Fala à vontade: pergunta o que quiseres, pede uma piada, \"cria uma tarefa para amanhã: ligar ao contabilista\" ou \"escreve um email ao João a adiar a reunião\". Os emails ficam em rascunho para enviares tu."
                    } else if c.mode() == crate::profile::RH {
                        "Escreve ou fala com o Jarvis: um email, o resumo de uma reunião, uma vaga, uma análise. Ele escreve e propõe; nada é enviado nem apagado sem o teu sim."
                    } else {
                        "Escreve abaixo. A análise é feita aqui; a implementação abre no Claude Code ou no Antigravity."
                    };
                    ui.label(RichText::new(empty).color(theme::MUTED));
                }
                for m in &c.messages {
                    // A message that just arrived fades in and settles from a little lower.
                    let k = crate::motion::since(&m.at, 0.35);
                    ui.scope(|ui| {
                        if k < 1.0 {
                            ui.set_opacity(0.2 + 0.8 * k);
                            if !crate::motion::reduced() {
                                ui.add_space((1.0 - k) * 8.0);
                            }
                            ui.ctx().request_repaint();
                        }
                        message(ui, c, m, acc, &mut acts);
                    });
                    if m.live {
                        typing(ui, acc);
                    }
                    ui.add_space(10.0);
                }
                let last_live = c.messages.last().is_some_and(|m| m.live);
                if c.busy && !last_live {
                    ui.horizontal(|ui| {
                        crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
                        ui.label(RichText::new("A pensar…").color(theme::MUTED));
                    });
                }
            });
    });
    ui.add_space(8.0);
    composer(ui, c, st, acc, &mut acts);
    acts
}

fn header(ui: &mut Ui, c: &Chat, st: &mut ChatUi, acc: Accent, in_window: bool, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if let Some(id) = c.item_id {
            ui_kit::id_link(ui, id, &azdo::item_url(id), acc);
        }
        let title = match c.item_id {
            Some(id) => c.title.trim_start_matches(&format!("#{id}")).trim().to_owned(),
            None => c.title.clone(),
        };
        ui.add(egui::Label::new(RichText::new(title).font(display(theme::size::SUBTITLE)).color(theme::BRIGHT)).truncate());
    });
    ui.horizontal(|ui| {
        let (_, tokens) = c.cost_today();
        let mut info = Vec::new();
        if c.busy {
            // How long this reply has been running and how many steps it took, so a long run is visibly alive.
            let start = c.messages.iter().rposition(|m| m.role == "user").unwrap_or(0);
            let steps: usize = c.messages[start..].iter().map(|m| m.tools.len()).sum();
            let secs = c.messages.get(start).and_then(|m| crate::ui_kit::parse(&m.at)).map(|t| (chrono::Local::now() - t).num_seconds().max(0)).unwrap_or(0);
            let engine = crate::chat::engine_label(c.engine.as_deref().unwrap_or_else(|| crate::chat::engine_of(&crate::chat::engine().model)));
            info.push(format!("{engine} a trabalhar há {}:{:02} · {steps} passo{}", secs / 60, secs % 60, if steps == 1 { "" } else { "s" }));
        }
        if c.handed_off {
            info.push("continua na app do Claude".into());
        }
        if let Some(ctx) = c.context_tokens {
            info.push(format!("contexto {} mil", ctx / 1000));
        }
        if tokens > 0 {
            info.push(format!("hoje {} mil tokens", format!("{:.1}", tokens as f64 / 1000.0).replace('.', ",")));
        }
        if let Some(w) = &c.worktree {
            info.push(w.rsplit('\\').next().unwrap_or(w).to_owned());
        }
        ui.label(RichText::new(info.join(" · ")).monospace().size(11.0).color(theme::MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if st.confirm_delete.as_deref() == Some(&c.id) {
                if button(ui, "Sim, apagar", true, acc).clicked() {
                    acts.push(Action::Delete(c.id.clone()));
                    st.confirm_delete = None;
                }
                if button(ui, "Não", false, acc).clicked() {
                    st.confirm_delete = None;
                }
                ui.label(RichText::new("Apagar esta conversa do Jarvis?").color(theme::AMBER));
                return;
            }
            if !c.busy && button(ui, "Apagar", false, acc).clicked() {
                st.confirm_delete = Some(c.id.clone());
            }
            if c.session_id.is_some() && !c.busy && button(ui, if c.handed_off { "Abrir no Claude" } else { "Continuar no Claude" }, false, acc).on_hover_text("Abre esta sessão na app do Claude (deixa de continuar aqui)").clicked() {
                acts.push(Action::OpenInClaude(c.id.clone()));
            }
            if !in_window && button(ui, "Janela", false, acc).on_hover_text("Abrir numa janela à parte").clicked() {
                acts.push(Action::PopOut(c.id.clone()));
            }
            if c.mode() == crate::profile::GERAL && !c.handed_off {
                let label = if st.voice_on { "A ouvir · parar" } else { "Falar" };
                if button(ui, label, !st.voice_on, acc).on_hover_text("Fala por voz neste assunto (o Jarvis responde em voz alta); diz \"obrigado\" para terminar. Também podes escrever.").clicked() {
                    acts.push(Action::Voice(c.id.clone()));
                }
            }
            if !in_window && !c.busy && c.mode() == crate::profile::GERAL && !c.messages.is_empty() && button(ui, "Começar de novo", false, acc).on_hover_text("Uma conversa nova; esta fica em Chat").clicked() {
                acts.push(Action::FreshJarvis);
            }
            if let Some((text, at)) = st.flash.get(&c.id) {
                if at.elapsed().as_secs() < 6 {
                    ui.label(RichText::new(text).color(theme::GREEN));
                }
            }
        });
    });
}

fn who(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).font(display(11.5)).color(color).extra_letter_spacing(1.6));
}

fn message(ui: &mut Ui, c: &Chat, m: &Msg, acc: Accent, acts: &mut Vec<Action>) {
    match m.role.as_str() {
        "user" => {
            who(ui, if m.voice { "TU · POR VOZ" } else { "TU" }, theme::MUTED);
            Frame::new().fill(theme::alpha(acc.main, 0.1)).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.2))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(&m.text).color(theme::BRIGHT)).wrap());
            });
        }
        "assistant" => {
            who(ui, &format!("JARVIS · {}", crate::chat::engine_label(crate::chat::msg_engine(m)).to_uppercase()), acc.main);
            // The marks for Jarvis ([[LEMBRAR: …]] and the others) are not for the person, also while streaming.
            if m.live {
                // A mark or a [[SKILL]] block still arriving is not shown either.
                markdown::show(ui, &crate::brain::hide_partial(&m.text), acc);
            } else {
                // An [[EMAIL]] block is a draft card; the rest is text.
                for (i, part) in crate::brain::split_emails(&m.text).into_iter().enumerate() {
                    match part {
                        Err(text) => {
                            // A [[REVIEW]] draft is a card to approve, never published by itself.
                            for (j, p) in crate::prs::split_reviews(&text).into_iter().enumerate() {
                                match p {
                                    Err(t) => markdown::show(ui, &t, acc),
                                    Ok(d) => review_card(ui, c, &d, egui::Id::unique(("review-edit", &m.id, i, j)), acc),
                                }
                            }
                        }
                        Ok(e) => email_card(ui, &e, egui::Id::unique(("email-edit", &m.id, i)), acc),
                    }
                }
            }
        }
        "task" => {
            ui.horizontal(|ui| {
                ui_kit::diamond(ui, true, theme::GREEN, 3.5);
                ui.add(egui::Label::new(RichText::new(format!("Tarefa criada: {}", m.text)).size(12.5).color(theme::MUTED)).wrap());
                if ui.add(egui::Label::new(RichText::new("Ver em Pessoais").size(12.5).underline().color(acc.light)).sense(Sense::click())).clicked() {
                    crate::app::post(crate::app::AppEvent::Go(crate::app::View::Tasks));
                }
            });
        }
        "remembered" => {
            ui.horizontal_wrapped(|ui| {
                ui_kit::diamond(ui, true, acc.main, 3.5);
                ui.add(egui::Label::new(RichText::new(format!("Guardei: {}", m.text)).size(12.5).color(theme::MUTED)).wrap());
                if ui.add(egui::Label::new(RichText::new("Desfazer").size(12.5).underline().color(acc.light)).sense(Sense::click())).on_hover_text("Tira esta regra da memória (Definições → O meu Jarvis)").clicked() {
                    acts.push(Action::Forget(c.id.clone(), m.id.clone()));
                }
            });
        }
        "skill" => {
            ui.horizontal_wrapped(|ui| {
                ui_kit::diamond(ui, true, acc.main, 3.5);
                let what = if m.replaced.is_some() { "Atualizei o teu comando" } else { "Criei o teu comando" };
                ui.add(egui::Label::new(RichText::new(format!("{what} {}", m.text)).size(12.5).color(theme::MUTED)).wrap());
                if ui.add(egui::Label::new(RichText::new("Desfazer").size(12.5).underline().color(acc.light)).sense(Sense::click())).on_hover_text("Apaga este comando (ou volta à versão anterior)").clicked() {
                    acts.push(Action::UndoSkill(c.id.clone(), m.id.clone()));
                }
            });
        }
        "skill-undone" => {
            ui.horizontal_wrapped(|ui| {
                ui_kit::diamond(ui, false, theme::FAINT, 3.5);
                ui.label(RichText::new("Desfeito:").size(12.5).color(theme::FAINT));
                ui.add(egui::Label::new(RichText::new(&m.text).size(12.5).strikethrough().color(theme::FAINT)).wrap());
            });
        }
        "forgotten" => {
            ui.horizontal_wrapped(|ui| {
                ui_kit::diamond(ui, false, theme::FAINT, 3.5);
                ui.label(RichText::new("Desfeito:").size(12.5).color(theme::FAINT));
                ui.add(egui::Label::new(RichText::new(&m.text).size(12.5).strikethrough().color(theme::FAINT)).wrap());
            });
        }
        "tools" => {
            Frame::new().stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.1))).corner_radius(CornerRadius::same(3)).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for t in &m.tools {
                    ui.horizontal(|ui| {
                        match t.status.as_str() {
                            "run" => {
                                crate::motion::spinner(ui, 10.0 + 2.0, acc.main);
                            }
                            s => ui_kit::diamond(ui, true, match s {
                                "ok" => theme::GREEN,
                                "error" => theme::RED,
                                _ => theme::FAINT,
                            }, 3.5),
                        }
                        ui.label(RichText::new(&t.label).size(12.5).color(theme::TEXT));
                        if !t.detail.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&t.detail).monospace().size(11.0).color(theme::FAINT)).truncate());
                        }
                    });
                }
            });
        }
        "inform" => {
            ui_kit::hud_panel(ui, acc, true, |ui| {
                who(ui, "NOVIDADE DO BOARD", theme::AMBER);
                markdown::show(ui, &m.text, acc);
            });
        }
        "note" => {
            ui.label(RichText::new(&m.text).monospace().size(11.0).color(theme::FAINT));
        }
        "error" => {
            ui.label(RichText::new(&m.text).color(theme::RED));
        }
        "implement" => {
            ui_kit::hud_panel(ui, acc, false, |ui| {
                who(ui, "PRONTO PARA IMPLEMENTAR", acc.main);
                ui.label(RichText::new(&m.text).size(15.0).strong().color(theme::BRIGHT));
                if let Some(w) = &m.worktree {
                    ui.label(RichText::new(w).monospace().size(11.5).color(theme::MUTED));
                }
                ui.add_space(6.0);
                match c.implemented_in.as_deref() {
                    Some(t) => {
                        let name = match t {
                            "claude" => "no Claude Code",
                            "antigravity" => "no Antigravity",
                            "codex" => "no Codex",
                            _ => "noutra IA",
                        };
                        ui.label(RichText::new(format!("A implementação seguiu {name}.")).color(theme::GREEN));
                    }
                    None => {
                        ui.horizontal(|ui| {
                            if button(ui, "Claude Code", true, acc).on_hover_text("Abre esta sessão na app do Claude, com a análise toda").clicked() {
                                acts.push(Action::Implement(c.id.clone(), "claude"));
                            }
                            if button(ui, "Antigravity", false, acc).on_hover_text("Abre o worktree no Antigravity e copia o pedido").clicked() {
                                acts.push(Action::Implement(c.id.clone(), "antigravity"));
                            }
                            if crate::brain::codex_installed() && button(ui, "Codex", false, acc).on_hover_text("Abre o Codex num terminal no worktree, já com o pedido").clicked() {
                                acts.push(Action::Implement(c.id.clone(), "codex"));
                            }
                            if button(ui, "Outra IA", false, acc).on_hover_text("Abre a pasta .jarvis e copia o pedido").clicked() {
                                acts.push(Action::Implement(c.id.clone(), "other"));
                            }
                        });
                        ui.label(RichText::new("O pedido de implementação fica copiado: cola-o no agente.").small().color(theme::MUTED));
                    }
                }
            });
        }
        "divider" => {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 360.0).max(40.0) / 2.0;
                let (l, _) = ui.allocate_exact_size(vec2(w, 10.0), Sense::hover());
                ui.painter().line_segment([l.left_center(), l.right_center()], Stroke::new(1.0, theme::alpha(acc.main, 0.4)));
                ui.label(RichText::new(m.text.to_uppercase()).font(display(11.0)).color(acc.light).extra_letter_spacing(1.2));
                let (r, _) = ui.allocate_exact_size(vec2(w, 10.0), Sense::hover());
                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, theme::alpha(acc.main, 0.4)));
            });
            ui.add_space(4.0);
        }
        "handoff" => {
            ui_kit::hint(ui, &m.text, false, acc);
        }
        _ => {}
    }
}

/// The default model of an engine in Definições.
pub fn default_model(engine: &str) -> String {
    let d = crate::chat::engine();
    let m = match engine {
        "antigravity" => d.agy_model,
        "codex" => d.codex_model,
        _ => d.claude_model,
    };
    if m.is_empty() { "auto".into() } else { m }
}

/// Who answers in this conversation (Claude Code, Antigravity or Codex) and with which model. Switching
/// mid-conversation is fine: the other engine gets what was said since as text.
fn engine_row(ui: &mut Ui, c: &Chat, acts: &mut Vec<Action>) {
    let default_engine = crate::chat::engine_of(&crate::chat::engine().model);
    let engine = c.engine.clone().unwrap_or_else(|| default_engine.to_owned());
    let models = model_options(&engine);
    let model = c.engine_model.clone().unwrap_or_else(|| if engine == default_engine { default_model(&engine) } else { "auto".into() });
    let model_label = models.iter().find(|(id, _)| *id == model).map(|(_, l)| l.clone()).unwrap_or_else(|| model.clone());
    // Sits in the composer's bottom bar: no captions, the tooltips say what each one is.
    ui.add_enabled_ui(!c.busy, |ui| {
        // With both modes on, a general conversation can be dev or RH (the rules the engine gets).
        if c.item_id.is_none() && c.pr_key.is_none() && c.mode() != crate::profile::GERAL && crate::profile::dev() && crate::profile::has_mode(crate::profile::RH) {
            let mode = c.mode();
            let mut pick = mode.clone();
            egui::ComboBox::from_id_salt(("mode", &c.id))
                .selected_text(if mode == crate::profile::RH { "RH" } else { "Dev" })
                .width(56.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, crate::profile::DEV.to_owned(), "Dev");
                    ui.selectable_value(&mut pick, crate::profile::RH.to_owned(), "RH");
                })
                .response
                .on_hover_text("Modo: as regras que o motor recebe nesta conversa");
            if pick != mode {
                acts.push(Action::SetMode(c.id.clone(), pick));
            }
        }
        let mut pick = engine.clone();
        egui::ComboBox::from_id_salt(("engine", &c.id))
            .selected_text(crate::chat::engine_label(&engine))
            .width(116.0)
            .show_ui(ui, |ui| {
                for e in ["claude", "antigravity", "codex"] {
                    if crate::chat::engine_installed(e) {
                        ui.selectable_value(&mut pick, e.to_owned(), crate::chat::engine_label(e));
                    }
                }
            })
            .response
            .on_hover_text("Motor: quem responde. Podes trocar a meio; o outro motor recebe o que já foi dito.");
        if pick != engine {
            acts.push(Action::SetEngine(c.id.clone(), Some(pick), None));
        }
        let mut m = model.clone();
        // Never wider than what is left of the bar (the send button and the mode picker share it).
        let w = (ui.available_width() - 260.0).clamp(110.0, 230.0);
        egui::ComboBox::from_id_salt(("model", &c.id))
            .selected_text(model_label)
            .width(w)
            .show_ui(ui, |ui| {
                for (id, label) in &models {
                    ui.selectable_value(&mut m, id.clone(), label);
                }
            })
            .response
            .on_hover_text("Modelo deste motor nesta conversa");
        if m != model {
            acts.push(Action::SetEngine(c.id.clone(), Some(engine.clone()), Some(m)));
        }
    });
}

fn composer(ui: &mut Ui, c: &Chat, st: &mut ChatUi, acc: Accent, acts: &mut Vec<Action>) {
    if c.handed_off {
        ui_kit::quiet(ui, "Esta conversa continua na app do Claude. Para outra análise, abre uma conversa nova.", acc);
        return;
    }
    let id = egui::Id::unique(("composer", &c.id));
    // Enter sends, Shift+Enter breaks the line: take the key before the text box sees it.
    let focused = ui.memory(|m| m.has_focus(id));
    let enter = focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    context_warning(ui, c, acc, acts);
    let draft = st.drafts.entry(c.id.clone()).or_default();
    // One card: the text on top, and under it who answers (mode, engine, model) on the left and Enviar on the right.
    let edge = if focused { theme::alpha(acc.main, 0.75) } else { theme::alpha(acc.main, 0.28) };
    Frame::new()
        .fill(theme::alpha(acc.main, 0.035))
        .stroke(Stroke::new(1.0, edge))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin { left: 4, right: 8, top: 4, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let hint = if c.busy { "A responder… podes ir escrevendo a próxima." } else { "Escreve ao Jarvis" };
            ui.add(TextEdit::multiline(draft).id(id).frame(Frame::NONE).hint_text(hint).desired_rows(3).desired_width(f32::INFINITY).margin(Margin::symmetric(8, 6)));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                engine_row(ui, c, acts);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if c.busy {
                        if button(ui, "Parar", false, acc).clicked() {
                            acts.push(Action::Stop(c.id.clone()));
                        }
                    } else {
                        let send = button(ui, "Enviar", true, acc).on_hover_text("Enter envia; Shift+Enter muda de linha").clicked() || enter;
                        if send && !draft.trim().is_empty() {
                            acts.push(Action::Send(c.id.clone(), std::mem::take(draft)));
                        }
                    }
                    if ui.available_width() > 190.0 {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Enter envia · Shift+Enter muda de linha").small().color(theme::FAINT));
                    }
                });
            });
        });
}

/// An email the Jarvis drafted: who, subject and body, with "Editar", "Abrir no Outlook" (a draft ready to send;
/// Jarvis never sends it) and "Copiar". The edits live in egui's memory under `key` while the window is open; when an
/// edited version leaves (Outlook or Copiar), its final text is kept as a style example (eu::add_example).
fn email_card(ui: &mut Ui, original: &crate::brain::Email, key: egui::Id, acc: Accent) {
    let mut edit: Option<crate::brain::Email> = ui.ctx().data_mut(|d| d.get_temp(key));
    // JARVIS_SHOT_EDIT=1 opens every draft for editing, for screenshots.
    let editing = ui.ctx().data_mut(|d| d.get_temp::<bool>(key.with("on"))).unwrap_or(std::env::var("JARVIS_SHOT_EDIT").is_ok());
    ui_kit::hud_panel(ui, acc, false, |ui| {
        who(ui, if edit.as_ref().is_some_and(|x| x != original) { "RASCUNHO DE EMAIL · EDITADO" } else { "RASCUNHO DE EMAIL" }, acc.main);
        let e = edit.get_or_insert_with(|| original.clone());
        if editing {
            for (label, value) in [("Para", &mut e.to), ("Cc", &mut e.cc), ("Assunto", &mut e.subject)] {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().color(theme::MUTED));
                    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
                });
            }
            ui.add_space(4.0);
            ui.add(egui::TextEdit::multiline(&mut e.body).desired_rows(8).desired_width(f32::INFINITY));
        } else {
            for (label, value) in [("Para", &e.to), ("Cc", &e.cc), ("Assunto", &e.subject)] {
                if label == "Cc" && value.is_empty() {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().color(theme::MUTED));
                    ui.add(egui::Label::new(RichText::new(if value.is_empty() { "(por preencher)" } else { value.as_str() }).color(if value.is_empty() { theme::AMBER } else { theme::BRIGHT })).wrap());
                });
            }
            ui.add_space(4.0);
            Frame::new().fill(theme::alpha(Color32::BLACK, 0.2)).corner_radius(CornerRadius::same(3)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(&e.body).color(theme::TEXT)).wrap());
            });
        }
        ui.add_space(6.0);
        let mut out = false;
        ui.horizontal(|ui| {
            if button(ui, "Abrir no Outlook", true, acc).on_hover_text("Abre um email novo com isto no Outlook; és tu que o envias").clicked() {
                let e = e.clone();
                std::thread::spawn(move || crate::brain::open_email(&e));
                out = true;
            }
            if button(ui, "Copiar", false, acc).clicked() {
                ui.ctx().copy_text(crate::brain::email_text(e));
                out = true;
            }
            let toggle = if editing { "Fechar edição" } else { "Editar" };
            if button(ui, toggle, false, acc).on_hover_text("Muda o rascunho aqui antes de o levares; o Jarvis aprende com a versão final").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(key.with("on"), !editing));
            }
        });
        if out && crate::eu::add_example(&e.subject, &original.body, &e.body) {
            ui.ctx().data_mut(|d| d.insert_temp(key.with("learned"), true));
        }
        if ui.ctx().data_mut(|d| d.get_temp::<bool>(key.with("learned")).unwrap_or(false)) {
            ui.label(RichText::new("Guardei a tua versão como exemplo de estilo (Definições → O meu Jarvis).").small().color(theme::MUTED));
        }
    });
    if let Some(e) = edit {
        ui.ctx().data_mut(|d| d.insert_temp(key, e));
    }
}

/// The review the Jarvis drafted for a PR: vote, general summary and comments, each one editable and removable.
/// Nothing reaches the PR until "Publicar no PR" is clicked; a draft already published shows as done.
fn review_card(ui: &mut Ui, c: &Chat, original: &crate::prs::ReviewDraft, key: egui::Id, acc: Accent) {
    let pr_key = c.pr_key.clone().filter(|k| !k.is_empty()).unwrap_or_else(|| original.pr.clone());
    let pub_key = crate::prs::draft_key(&pr_key, original);
    let done = crate::prs::is_published(&pub_key);
    // (draft, which comments stay)
    let mut st: (crate::prs::ReviewDraft, Vec<bool>) = ui.ctx().data_mut(|d| d.get_temp(key)).unwrap_or_else(|| (original.clone(), vec![true; original.comments.len()]));
    ui_kit::hud_panel(ui, acc, false, |ui| {
        who(ui, if done { "REVIEW PUBLICADA" } else { "RASCUNHO DE REVIEW · POR APROVAR" }, if done { theme::GREEN } else { acc.main });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Voto").small().color(theme::MUTED));
            if done {
                ui.label(RichText::new(crate::prs::vote_name(st.0.vote)).color(theme::BRIGHT));
            } else {
                egui::ComboBox::from_id_salt(key.with("vote")).selected_text(crate::prs::vote_name(st.0.vote)).show_ui(ui, |ui| {
                    for v in [10, 5, 0, -5, -10] {
                        ui.selectable_value(&mut st.0.vote, v, crate::prs::vote_name(v));
                    }
                });
            }
        });
        ui.add_space(4.0);
        if done {
            if !st.0.summary.is_empty() {
                ui.add(egui::Label::new(RichText::new(&st.0.summary).color(theme::TEXT)).wrap());
            }
        } else {
            ui.label(RichText::new("Resumo geral (comentário no PR)").small().color(theme::MUTED));
            ui.add(egui::TextEdit::multiline(&mut st.0.summary).desired_rows(2).desired_width(f32::INFINITY));
        }
        let mut kept = 0;
        for (i, cm) in st.0.comments.iter_mut().enumerate() {
            ui.add_space(6.0);
            let on = st.1.get(i).copied().unwrap_or(true);
            ui.horizontal(|ui| {
                if !done {
                    let mut on = on;
                    if ui.checkbox(&mut on, "").changed() {
                        st.1[i] = on;
                    }
                }
                let place = match (&cm.file, cm.line) {
                    (Some(f), Some(l)) => format!("{f}:{l}"),
                    (Some(f), None) => f.clone(),
                    _ => "Comentário geral".into(),
                };
                ui.label(RichText::new(place).monospace().size(12.0).color(if on { acc.light } else { theme::FAINT }));
            });
            if on {
                kept += 1;
                if done {
                    ui.add(egui::Label::new(RichText::new(&cm.text).color(theme::TEXT)).wrap());
                } else {
                    ui.add(egui::TextEdit::multiline(&mut cm.text).desired_rows(2).desired_width(f32::INFINITY));
                }
            }
        }
        ui.add_space(8.0);
        if done {
            ui.label(RichText::new("Já está no PR.").small().color(theme::GREEN));
        } else {
            let state = crate::prs::publish_state(&pub_key);
            let running = matches!(&state, Some(Ok(t)) if t == "…");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running && !pr_key.is_empty(), |ui| {
                    let label = format!("Publicar no PR ({kept} comentário{}{})", if kept == 1 { "" } else { "s" }, if st.0.vote != 0 { " e voto" } else { "" });
                    if button(ui, &label, true, acc).on_hover_text("Só agora é que os comentários e o voto vão para o Azure DevOps").clicked() {
                        let mut d = st.0.clone();
                        d.comments = st.0.comments.iter().zip(&st.1).filter(|(_, on)| **on).map(|(c, _)| c.clone()).collect();
                        crate::prs::publish(pub_key.clone(), pr_key.clone(), d, ui.ctx().clone());
                    }
                });
                if running {
                    crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
                }
                if let Some(Err(e)) = &state {
                    ui.label(RichText::new(format!("{e} Se alguns comentários já ficaram, não carregues outra vez sem ver o PR.")).small().color(theme::RED));
                }
            });
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, st));
}

/// Three dots that light up in turn while the reply is still being written.
fn typing(ui: &mut Ui, acc: Accent) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 10.0), Sense::hover());
    let t = crate::motion::now(ui.ctx()) as f32;
    for i in 0..3 {
        let k = ((t * 4.0 - i as f32 * 0.7).sin() * 0.5 + 0.5).powf(2.0);
        let c = egui::pos2(rect.left() + 5.0 + i as f32 * 11.0, rect.center().y);
        ui.painter().circle_filled(c, 2.4 + 0.8 * k, theme::alpha(acc.main, 0.3 + 0.7 * k));
    }
    ui.ctx().request_repaint();
}

/// Models offered for an engine: (id, label), "auto" first.
pub fn model_options(engine: &str) -> Vec<(String, String)> {
    if engine == "antigravity" {
        std::iter::once(("auto".to_owned(), "Predefinido do Antigravity".to_owned())).chain(crate::brain::agy_models()).collect()
    } else if engine == "codex" {
        std::iter::once(("auto".to_owned(), "Predefinido do Codex".to_owned())).chain(crate::brain::codex_models()).collect()
    } else {
        std::iter::once(("auto".to_owned(), "Automático (Sonnet; Opus se pedires)".to_owned()))
            .chain(crate::brain::CLAUDE_MODELS.iter().map(|(a, b)| (a.to_string(), b.to_string())))
            .collect()
    }
}

/// Long conversations re-read everything on every message: past the threshold, offer to continue in a new session
/// from a summary.
fn context_warning(ui: &mut Ui, c: &Chat, acc: Accent, acts: &mut Vec<Action>) {
    let Some(ctx) = c.context_tokens.filter(|t| *t >= crate::chat::CONTEXT_WARN) else { return };
    if c.compacting {
        ui_kit::hint(ui, "A escrever o resumo para continuar numa sessão nova…", false, acc);
        ui.add_space(6.0);
        return;
    }
    if c.busy {
        return;
    }
    // A slim line over the composer, not a box: it is advice, not an error.
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui_kit::diamond(ui, true, theme::AMBER, 3.5);
        ui.label(RichText::new(format!("Conversa longa: {} mil tokens, e cada mensagem relê tudo.", ctx / 1000)).small().color(theme::TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if button(ui, "Resumir e continuar", false, acc).on_hover_text("O motor escreve um resumo (item, decisões, plano, ficheiros, estado, perguntas) e a conversa segue numa sessão nova a partir dele. O histórico continua visível aqui.").clicked() {
                acts.push(Action::Compact(c.id.clone()));
            }
        });
    });
    ui.add_space(6.0);
}
