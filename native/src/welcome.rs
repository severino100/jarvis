//! The guided first run: whoever opens Jarvis without a profile is taken through it, one step at a time: the name,
//! what Jarvis is for (dev, rh or both), the AI account, the team pack (or none), what shows on Hoje and, if they
//! want, a first thing Jarvis should know about them. Everything can be changed later in Definições; "Saltar" keeps
//! the defaults. Screenshots: JARVIS_SHOT_VIEW=welcome [JARVIS_SHOT_STEP=0..5] (with JARVIS_DATA_DIR on a
//! throw-away folder, so the real profile is not touched).

use crate::{
    eu, profile,
    settings::{self, Settings},
    settings_view::{pack_job, PACK_MSG},
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use eframe::egui::{self, RichText, Ui};

const STEPS: [&str; 6] = ["Olá", "Para quê", "A tua IA", "A tua equipa", "O teu Hoje", "Pronto"];

pub struct Welcome {
    step: usize,
    url: String,
    instructions: String,
    /// Claude Code, Antigravity and Codex on this PC, checked when the step opens and on "Verificar outra vez".
    engines: Option<(bool, bool, bool)>,
}

impl Welcome {
    pub fn new() -> Self {
        let step = std::env::var("JARVIS_SHOT_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(0usize).min(STEPS.len() - 1);
        Self { step, url: String::new(), instructions: String::new(), engines: None }
    }
}

/// Ends the first run: the profile counts as set up from now on.
fn finish() {
    profile::update(|p| p.welcomed = true);
    crate::app::post(crate::app::AppEvent::Refresh);
}

fn title(ui: &mut Ui, text: &str, sub: &str) {
    ui.label(RichText::new(text).font(display(theme::size::TITLE)).color(theme::BRIGHT));
    ui.add_space(4.0);
    ui.label(RichText::new(sub).color(theme::MUTED));
    ui.add_space(16.0);
}

fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).font(display(13.0)).color(theme::BRIGHT).extra_letter_spacing(1.6));
}

fn status(ui: &mut Ui, ok: bool, yes: &str, no: &str) {
    ui.horizontal(|ui| {
        ui_kit::diamond(ui, ok, if ok { theme::GREEN } else { theme::AMBER }, 4.0);
        ui.label(RichText::new(if ok { yes } else { no }).color(if ok { theme::GREEN } else { theme::AMBER }));
    });
}

/// Draws the current step. Returns true when the first run is over (done or skipped).
pub fn view(ui: &mut Ui, w: &mut Welcome, acc: Accent) -> bool {
    let last = STEPS.len() - 1;
    // Where we are: the steps in a row, the current one lit.
    ui.horizontal_wrapped(|ui| {
        for (i, s) in STEPS.iter().enumerate() {
            let (c, filled) = if i == w.step { (acc.main, true) } else if i < w.step { (acc.light, true) } else { (theme::FAINT, false) };
            ui_kit::diamond(ui, filled, c, 4.0);
            ui.label(RichText::new(s.to_uppercase()).font(display(11.5)).extra_letter_spacing(1.2).color(if i == w.step { theme::BRIGHT } else { theme::MUTED }));
            ui.add_space(10.0);
        }
    });
    ui.add_space(14.0);

    let mut p = profile::get();
    let before = p.clone();
    let mut s: Settings = settings::load();
    let s_before = s.clone();
    ui_kit::hud_panel(ui, acc, false, |ui| match w.step {
        0 => {
            title(ui, "Olá. Eu sou o Jarvis.", "O teu assistente neste PC. Antes de começar, umas perguntas rápidas; tudo se muda depois em Definições.");
            label(ui, "Como te chamas?");
            ui.add(egui::TextEdit::singleline(&mut p.name).hint_text("o teu nome").desired_width(260.0));
            ui.add_space(10.0);
            label(ui, "Dito como (opcional)");
            ui.label(RichText::new("Se a voz ler mal o teu nome, escreve-o como se diz (por exemplo com um acento que o nome não tem).").small().color(theme::MUTED));
            ui.add(egui::TextEdit::singleline(&mut p.spoken_name).hint_text("igual").desired_width(260.0));
        }
        1 => {
            title(ui, "Para que vais usar o Jarvis?", "Cada modo traz os seus ecrãs. Podes escolher os dois; pelo menos um fica ligado.");
            for (mode, name, hint) in [
                (profile::RH, "Recursos Humanos", "O teu assistente pessoal: escreve emails e mensagens, resume, analisa perfis e propõe. Nada é enviado nem apagado sem o teu sim."),
                (profile::DEV, "Desenvolvimento", "O board da equipa, os PRs, as horas do sprint e a preparação de cada item para o Claude Code ou o Antigravity."),
            ] {
                let mut on = p.modes.iter().any(|m| m == mode);
                // At least one mode stays on: unticking the only one does nothing.
                if ui.checkbox(&mut on, RichText::new(name).color(theme::BRIGHT).size(theme::size::CARD)).changed() {
                    if on {
                        p.modes.push(mode.to_owned());
                    } else if p.modes.len() > 1 {
                        p.modes.retain(|m| m != mode);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.add(egui::Label::new(RichText::new(hint).color(theme::MUTED)).wrap());
                });
                ui.add_space(8.0);
            }
        }
        2 => {
            title(ui, "A tua conta de IA", "O Jarvis não tem IA própria: usa a tua subscrição do Claude (Claude Code), a tua conta Google (Antigravity) ou a tua conta ChatGPT (Codex). Basta um. Não há chaves nem pagamentos por utilização.");
            let (claude, agy, codex) = *w.engines.get_or_insert_with(|| (crate::brain::claude_installed(), crate::brain::gemini_installed(), crate::brain::codex_installed()));
            label(ui, "Claude Code");
            status(ui, claude, "Instalado neste PC.", "Não o encontrei neste PC.");
            if !claude {
                ui.label(RichText::new("Instala-o e, numa linha de comandos, escreve claude e entra com a tua conta Claude.").small().color(theme::MUTED));
                if button(ui, "Como instalar o Claude Code", false, acc).clicked() {
                    crate::brain::open("https://claude.com/claude-code");
                }
            }
            ui.add_space(10.0);
            label(ui, "Antigravity");
            status(ui, agy, "Instalado neste PC.", "Não está instalado (é opcional).");
            ui.add_space(10.0);
            label(ui, "Codex");
            status(ui, codex, "Instalado neste PC.", "Não está instalado (é opcional).");
            if !codex {
                ui.label(RichText::new("Com a app Codex instalada e a tua conta ChatGPT, o Codex passa a ser outro motor.").small().color(theme::MUTED));
            }
            if !(claude || agy || codex) {
                ui.add_space(6.0);
                ui_kit::hint(ui, "Sem nenhum destes o Jarvis não consegue responder. Instala um e carrega em Verificar outra vez.", true, acc);
            }
            ui.add_space(10.0);
            label(ui, "Quem responde nas conversas");
            if s.model == "claude" {
                s.model = "auto".into();
                s.fallback = false;
            }
            ui.horizontal_wrapped(|ui| {
                let mut opts = vec![("auto", "Claude Code")];
                if agy {
                    opts.push(("gemini", "Antigravity"));
                }
                if codex {
                    opts.push(("codex", "Codex"));
                }
                for (v, l) in opts {
                    if button(ui, l, s.model == v, acc).clicked() {
                        s.model = v.into();
                    }
                }
            });
            ui.checkbox(&mut s.fallback, "Quando o plano de um motor acaba, continuar a conversa noutro motor instalado");
            ui.add_space(10.0);
            label(ui, "Email e agenda");
            ui.label(RichText::new("A agenda vem do Outlook deste PC, sem nada para ligar. Se não usas o Outlook, desliga a agenda no passo do Hoje.").color(theme::MUTED));
            ui.add_space(6.0);
            if button(ui, "Verificar outra vez", false, acc).clicked() {
                w.engines = None;
            }
        }
        3 => {
            title(ui, "A tua equipa", "O pacote da equipa traz as regras, os modelos e as skills de todos: normalmente é um link git que alguém da equipa te dá. Sem pacote, o Jarvis funciona na mesma.");
            for dir in &p.packs {
                let name = profile::read_pack(std::path::Path::new(dir)).map(|pk| pk.m.name).ok().filter(|n| !n.is_empty()).unwrap_or_else(|| dir.clone());
                status(ui, true, &format!("Pacote \"{name}\" instalado."), "");
            }
            label(ui, "Link do pacote");
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut w.url).hint_text("https://…/equipa-jarvis.git").desired_width(320.0));
                if button(ui, "Adicionar", true, acc).clicked() && !w.url.trim().is_empty() {
                    let url = std::mem::take(&mut w.url);
                    pack_job(ui.ctx().clone(), move || profile::add_from_git(&url));
                }
                if button(ui, "Escolher pasta…", false, acc).clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title("Pasta com jarvis.toml").pick_folder() {
                        *PACK_MSG.lock().unwrap() = Some(match profile::add_folder(&d) {
                            Ok(m) => (m, false),
                            Err(e) => (e, true),
                        });
                    }
                }
            });
            if let Some((m, err)) = PACK_MSG.lock().unwrap().clone() {
                ui.label(RichText::new(m).small().color(if err { theme::AMBER } else { theme::GREEN }));
            }
            if p.modes.iter().any(|m| m == profile::DEV) {
                ui.add_space(10.0);
                label(ui, "Pasta dos projetos");
                ui.label(RichText::new("Onde estão os teus repositórios. Os worktrees de cada item são criados aqui.").small().color(theme::MUTED));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&p.projects_dir).monospace().size(12.0).color(theme::MUTED));
                    if button(ui, "Escolher…", false, acc).clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_directory(&p.projects_dir).pick_folder() {
                            p.projects_dir = d.to_string_lossy().into_owned();
                        }
                    }
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Se não tens pacote, carrega em Seguinte. Podes adicioná-lo depois em Definições.").small().color(theme::FAINT));
        }
        4 => {
            title(ui, "O teu ecrã Hoje", "O que queres ver quando abres o Jarvis.");
            let dev = p.modes.iter().any(|m| m == profile::DEV);
            for (key, name, dev_only) in crate::dashboard::BLOCKS {
                if dev_only && !dev {
                    continue;
                }
                let mut on = !p.today_hidden.iter().any(|h| h == key);
                if ui.checkbox(&mut on, RichText::new(name).color(theme::TEXT)).changed() {
                    if on {
                        p.today_hidden.retain(|h| h != key);
                    } else {
                        p.today_hidden.push(key.to_owned());
                    }
                }
            }
        }
        _ => {
            let n = p.name.trim();
            title(ui, &if n.is_empty() { "Está tudo pronto.".to_owned() } else { format!("Está tudo pronto, {n}.") }, "O Jarvis aprende contigo: diz-lhe \"lembra-te que…\" numa conversa e ele guarda a regra. Vês e apagas tudo em Definições → O meu Jarvis.");
            label(ui, "Uma coisa que o Jarvis deve saber de ti (opcional)");
            ui.label(RichText::new("O tom, como assinas, o que nunca fazer. Vale para todas as conversas.").small().color(theme::MUTED));
            let hint = if p.modes.iter().any(|m| m == profile::RH) { "Por exemplo: Trabalho em Recursos Humanos. Trata-me por tu. Emails curtos, sem emojis." } else { "Por exemplo: Trata-me por tu. Respostas curtas." };
            ui.add(egui::TextEdit::multiline(&mut w.instructions).hint_text(hint).desired_rows(4).desired_width(f32::INFINITY));
        }
    });
    if p != before {
        profile::update(|x| *x = p);
    }
    if s.model != s_before.model || s.fallback != s_before.fallback {
        settings::update(|x| {
            x.model = s.model;
            x.fallback = s.fallback;
        });
    }

    ui.add_space(14.0);
    let mut done = false;
    ui.horizontal(|ui| {
        if w.step > 0 && button(ui, "Voltar", false, acc).clicked() {
            w.step -= 1;
        }
        if w.step < last {
            if button(ui, "Seguinte", true, acc).clicked() {
                w.step += 1;
                w.engines = None;
            }
        } else if button(ui, "Começar", true, acc).clicked() {
            if !w.instructions.trim().is_empty() && eu::instructions().trim().is_empty() {
                eu::set_instructions(w.instructions.trim());
            }
            done = true;
        }
        ui.add_space(12.0);
        if w.step < last && ui.add(egui::Label::new(RichText::new("Saltar e configurar depois").small().color(theme::FAINT)).sense(egui::Sense::click())).on_hover_text("Fica com o que já escolheste; o resto muda-se em Definições").clicked() {
            done = true;
        }
    });
    ui.add_space(8.0);
    if done {
        finish();
    }
    done
}
