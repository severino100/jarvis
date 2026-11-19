//! What Jarvis needs on this PC and does not install itself: git (team packs, worktrees) and at least one AI engine
//! (Claude Code, Antigravity, Codex), each with the person's own account. The same list as the setup's last page
//! (installer/jarvis.iss): what is missing comes with the command to copy and a link to the official instructions.
//! Checked in the background (asking Windows for the Codex app takes a moment) and again on "Verificar outra vez".

use std::sync::Mutex;

use eframe::egui::{self, RichText, Ui};

use crate::{
    brain,
    theme::{self, Accent},
    ui_kit::{self, button},
};

pub struct Need {
    pub name: &'static str,
    /// What it is for, shown when it is missing.
    pub purpose: &'static str,
    pub command: &'static str,
    /// After installing: how to sign in.
    pub then: &'static str,
    pub link: &'static str,
    pub engine: bool,
}

pub const NEEDS: [Need; 4] = [
    Need {
        name: "Claude Code",
        purpose: "motor de IA, com a tua subscrição Claude Pro ou Max",
        command: "irm https://claude.ai/install.ps1 | iex",
        then: "Depois, num terminal novo, escreve claude e entra com a tua conta (nunca com chave de API).",
        link: "https://code.claude.com/docs/en/setup",
        engine: true,
    },
    Need {
        name: "Antigravity",
        purpose: "motor de IA, com a tua conta Google",
        command: "irm https://antigravity.google/cli/install.ps1 | iex",
        then: "Depois, num terminal novo, escreve agy e entra com a tua conta Google.",
        link: "https://antigravity.google",
        engine: true,
    },
    Need {
        name: "Codex",
        purpose: "motor de IA, com a tua conta ChatGPT",
        command: "npm i -g @openai/codex",
        then: "Ou instala a app Codex. Depois: codex login e escolhe Sign in with ChatGPT (nunca chave de API).",
        link: "https://developers.openai.com/codex",
        engine: true,
    },
    Need {
        name: "Git",
        purpose: "para o pacote da tua equipa e, no modo de desenvolvimento, os worktrees",
        command: "winget install Git.Git",
        then: "Depois fecha e abre o Jarvis, para ele ver o git novo.",
        link: "https://git-scm.com/downloads/win",
        engine: false,
    },
];

fn git_installed() -> bool {
    let on_path = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("git.exe").is_file()));
    let program_files = std::env::var_os("ProgramFiles").map(|d| std::path::PathBuf::from(d).join(r"Git\cmd\git.exe"));
    let local = dirs::data_local_dir().map(|d| d.join(r"Programs\Git\cmd\git.exe"));
    on_path || [program_files, local].into_iter().flatten().any(|p| p.is_file())
}

/// One result per entry of NEEDS, in the same order.
fn check() -> Vec<bool> {
    // Screenshots of a PC without anything: JARVIS_SHOT_MISSING=1.
    if std::env::var("JARVIS_SHOT_MISSING").is_ok() {
        return vec![false; NEEDS.len()];
    }
    brain::forget_codex();
    vec![brain::claude_installed(), brain::gemini_installed(), brain::codex_installed(), git_installed()]
}

enum State {
    Idle,
    Checking,
    Done(Vec<bool>),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);

/// The last result; starts a check in the background when there is none yet.
pub fn get(ctx: &egui::Context) -> Option<Vec<bool>> {
    let mut st = STATE.lock().unwrap();
    match &*st {
        State::Done(v) => Some(v.clone()),
        State::Checking => None,
        State::Idle => {
            *st = State::Checking;
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let v = check();
                *STATE.lock().unwrap() = State::Done(v);
                ctx.request_repaint();
            });
            None
        }
    }
}

/// Looks again (after the person installs something).
pub fn recheck() {
    let mut st = STATE.lock().unwrap();
    if !matches!(*st, State::Checking) {
        *st = State::Idle;
    }
}

/// The list: each engine and git, OK or what is missing with the command, "Copiar" and "Como instalar".
pub fn panel(ui: &mut Ui, acc: Accent) {
    let Some(found) = get(ui.ctx()) else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("A ver o que está instalado…").color(theme::MUTED));
        });
        return;
    };
    let engines = NEEDS.iter().zip(&found).any(|(n, ok)| n.engine && *ok);
    for (need, ok) in NEEDS.iter().zip(found.iter().copied()) {
        ui.horizontal(|ui| {
            ui_kit::diamond(ui, ok, if ok { theme::GREEN } else { theme::AMBER }, 4.0);
            ui.label(RichText::new(need.name).color(theme::BRIGHT).strong());
            if ok {
                ui.label(RichText::new("instalado").small().color(theme::GREEN));
            } else {
                // An engine is optional once another one is there.
                let optional = need.engine && engines;
                ui.label(RichText::new(if optional { format!("não está (opcional): {}", need.purpose) } else { format!("falta: {}", need.purpose) }).small().color(if optional { theme::MUTED } else { theme::AMBER }));
            }
        });
        if !ok {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(need.command).monospace().size(12.0).color(theme::TEXT)).on_hover_text("Corre no PowerShell");
                if button(ui, "Copiar", false, acc).on_hover_text("Copia o comando para colares no PowerShell").clicked() {
                    ui.ctx().copy_text(need.command.to_owned());
                }
                if button(ui, "Como instalar", false, acc).on_hover_text(need.link).clicked() {
                    brain::open(need.link);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.add(egui::Label::new(RichText::new(need.then).small().color(theme::MUTED)).wrap());
            });
        }
        ui.add_space(6.0);
    }
    if !engines {
        ui_kit::hint(ui, "Sem nenhum motor de IA o Jarvis não consegue responder nas conversas. Instala um, entra com a tua conta e carrega em Verificar outra vez.", true, acc);
        ui.add_space(6.0);
    }
    if button(ui, "Verificar outra vez", false, acc).clicked() {
        recheck();
    }
}

/// The message for a conversation whose engine is not on this PC: what to run and where to read more.
pub fn missing_message(name: &str) -> String {
    match NEEDS.iter().find(|n| n.name.eq_ignore_ascii_case(name)) {
        Some(n) => format!("O {} não está instalado neste PC. Instala-o no PowerShell com:\n\n    {}\n\n{} Instruções: {}\n\nEm Definições → O que o Jarvis precisa vês o que está instalado.", n.name, n.command, n.then, n.link),
        None => format!("O {name} não está instalado neste PC."),
    }
}
