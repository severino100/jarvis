//! Definições: accent colour, text size, refresh interval, notices and the AI engines. Saved in state-native.json.

use crate::{
    settings::{self, Settings},
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use eframe::egui::{self, RichText, Ui};

const ACCENTS: [(&str, &str); 6] = [("cyan", "Ciano"), ("orange", "Laranja"), ("green", "Verde"), ("violet", "Violeta"), ("red", "Vermelho"), ("ice", "Gelo")];

fn row(ui: &mut Ui, title: &str, hint: &str, add: impl FnOnce(&mut Ui)) {
    ui.add_space(4.0);
    ui.label(RichText::new(title.to_uppercase()).font(display(13.0)).color(theme::BRIGHT).extra_letter_spacing(1.6));
    if !hint.is_empty() {
        ui.label(RichText::new(hint).small().color(theme::MUTED));
    }
    ui.horizontal_wrapped(add);
    ui.add_space(10.0);
}

fn chips<T: PartialEq + Clone>(ui: &mut Ui, current: &mut T, options: &[(T, &str)], acc: Accent) -> bool {
    let mut changed = false;
    for (v, label) in options {
        if button(ui, label, *current == *v, acc).clicked() && *current != *v {
            *current = v.clone();
            changed = true;
        }
    }
    changed
}

/// Pack actions that run off the UI thread (git clone / pull) report here.
static PACK_MSG: std::sync::Mutex<Option<(String, bool)>> = std::sync::Mutex::new(None);
static PACK_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn pack_job(ctx: egui::Context, job: impl FnOnce() -> Result<String, String> + Send + 'static) {
    use std::sync::atomic::Ordering;
    if PACK_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    *PACK_MSG.lock().unwrap() = Some(("A trabalhar…".into(), false));
    std::thread::spawn(move || {
        let r = job();
        *PACK_MSG.lock().unwrap() = Some(match r {
            Ok(m) => (m, false),
            Err(e) => (e, true),
        });
        PACK_BUSY.store(false, Ordering::SeqCst);
        crate::app::post(crate::app::AppEvent::Refresh);
        ctx.request_repaint();
    });
}

/// Who you are and which team packs you use: the only place where Jarvis learns about a company.
fn profile_panel(ui: &mut Ui, acc: Accent) {
    use crate::profile;
    static URL: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    static TOKEN: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let mut p = profile::get();
    let before = p.clone();
    let (mut add_folder, mut add_git, mut remove, mut pull, mut save_token, mut clear_token) = (None, false, None, false, false, false);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "O teu nome", "Como o Jarvis te trata. \"Dito como\" corrige a pronúncia na voz (por exemplo um acento que o nome não tem).", |ui| {
            ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(200.0));
            ui.label(RichText::new("Dito como").small().color(theme::MUTED));
            ui.add(egui::TextEdit::singleline(&mut p.spoken_name).hint_text("igual").desired_width(140.0));
        });
        row(ui, "Pasta dos projetos", "Onde estão os teus repositórios. Os worktrees de cada item são criados aqui.", |ui| {
            ui.label(RichText::new(&p.projects_dir).monospace().size(12.0).color(theme::MUTED));
            if button(ui, "Escolher…", false, acc).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&p.projects_dir).pick_folder() {
                    p.projects_dir = d.to_string_lossy().into_owned();
                }
            }
        });
        row(ui, "Pacotes de equipa", "O pacote da tua equipa traz o board, os repositórios, as skills e os guias, iguais para todos. Normalmente é um repositório git privado: o Jarvis clona-o com a tua conta e atualiza-o ao arrancar. Sem pacote, o Jarvis funciona sem board.", |ui| {
            ui.vertical(|ui| {
                let active = if p.active_pack.is_empty() { p.packs.first().cloned().unwrap_or_default() } else { p.active_pack.clone() };
                for dir in p.packs.clone() {
                    ui.horizontal(|ui| {
                        let (name, err) = match profile::read_pack(std::path::Path::new(&dir)) {
                            Ok(pk) => (if pk.m.name.is_empty() { dir.clone() } else { pk.m.name }, None),
                            Err(e) => (dir.clone(), Some(e)),
                        };
                        if ui.radio(dir == active, RichText::new(name).color(theme::BRIGHT)).on_hover_text("Usar este pacote").clicked() {
                            p.active_pack = dir.clone();
                        }
                        ui.label(RichText::new(err.unwrap_or_else(|| dir.clone())).small().color(theme::FAINT));
                        if button(ui, "Remover", false, acc).clicked() {
                            remove = Some(dir.clone());
                        }
                    });
                }
                if p.packs.is_empty() {
                    ui.label(RichText::new("Nenhum pacote instalado.").small().color(theme::MUTED));
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let mut url = URL.lock().unwrap();
                    ui.add(egui::TextEdit::singleline(&mut *url).hint_text("https://…/equipa-jarvis.git").desired_width(300.0));
                    if button(ui, "Adicionar do git", true, acc).clicked() && !url.trim().is_empty() {
                        add_git = true;
                    }
                    if button(ui, "Adicionar pasta…", false, acc).clicked() {
                        add_folder = rfd::FileDialog::new().set_title("Pasta com jarvis.toml").pick_folder();
                    }
                    if !p.packs.is_empty() && button(ui, "Atualizar pacotes", false, acc).clicked() {
                        pull = true;
                    }
                });
                if let Some((m, err)) = PACK_MSG.lock().unwrap().clone() {
                    ui.label(RichText::new(m).small().color(if err { theme::AMBER } else { theme::GREEN }));
                }
            });
        });
        if let Some(b) = profile::board() {
            row(ui, "Token do board", &format!("O teu token pessoal do Azure DevOps ({}), com Work Items (Read & Write) e Code (Read). Fica cifrado neste PC e só o Jarvis e as skills do pacote o usam.", b.org), |ui| {
                if profile::has_saved_token() {
                    ui.label(RichText::new("Guardado").color(theme::GREEN));
                    if button(ui, "Apagar", false, acc).clicked() {
                        clear_token = true;
                    }
                } else if profile::board_token().is_some() {
                    ui.label(RichText::new("A usar o ficheiro ~/.config/azdo/pat").small().color(theme::MUTED));
                }
                let mut t = TOKEN.lock().unwrap();
                ui.add(egui::TextEdit::singleline(&mut *t).password(true).hint_text("novo token").desired_width(220.0));
                if button(ui, "Guardar", true, acc).clicked() && !t.trim().is_empty() {
                    save_token = true;
                }
            });
        }
    });
    if p != before {
        profile::update(|x| *x = p);
        crate::app::post(crate::app::AppEvent::Refresh);
    }
    if save_token {
        let mut t = TOKEN.lock().unwrap();
        profile::set_board_token(&t);
        t.clear();
        crate::app::post(crate::app::AppEvent::Refresh);
    }
    if clear_token {
        profile::set_board_token("");
    }
    if let Some(d) = remove {
        profile::remove(&d);
        crate::app::post(crate::app::AppEvent::Refresh);
    }
    if let Some(d) = add_folder {
        *PACK_MSG.lock().unwrap() = Some(match profile::add_folder(&d) {
            Ok(m) => (m, false),
            Err(e) => (e, true),
        });
        crate::app::post(crate::app::AppEvent::Refresh);
    }
    if add_git {
        let url = std::mem::take(&mut *URL.lock().unwrap());
        pack_job(ui.ctx().clone(), move || profile::add_from_git(&url));
    }
    if pull {
        pack_job(ui.ctx().clone(), || {
            let lines = profile::update_packs();
            Ok(if lines.is_empty() { "Os pacotes já estão atualizados.".into() } else { lines.join(" ") })
        });
    }
}

/// Returns true when the look changed (accent or size), so the app re-applies the style.
pub fn view(ui: &mut Ui, acc: Accent, voice_ready: (bool, bool)) -> bool {
    let mut s: Settings = settings::load();
    let before = s.clone();
    let mut look = false;
    profile_panel(ui, acc);
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Cor principal", "", |ui| {
            for (key, label) in ACCENTS {
                let a = theme::accent(key);
                let active = s.accent == key || (s.accent.is_empty() && key == "cyan");
                let b = egui::Button::new(RichText::new(label.to_uppercase()).font(display(12.0)).color(if active { theme::BG } else { a.light }))
                    .fill(if active { a.main } else { theme::alpha(a.main, 0.08) })
                    .stroke(egui::Stroke::new(1.0, a.main))
                    .corner_radius(egui::CornerRadius::same(4));
                if ui.add(b).clicked() {
                    s.accent = key.into();
                    look = true;
                }
            }
        });
        row(ui, "Tamanho do texto", "Automático segue a escala do Windows.", |ui| {
            if chips(ui, &mut s.ui_scale, &[("auto".into(), "Automático"), ("100".into(), "100%"), ("110".into(), "110%"), ("125".into(), "125%")], acc) {
                look = true;
            }
        });
        row(ui, "Atualizar o board", "De quanto em quanto tempo o Jarvis lê o Azure DevOps e os PRs (aplica-se ao reabrir).", |ui| {
            chips(ui, &mut s.poll_minutes, &[(2, "2 min"), (5, "5 min"), (10, "10 min"), (15, "15 min"), (30, "30 min")], acc);
        });
    });
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Arranque", "Arranca escondido na bandeja quando entras no Windows.", |ui| {
            static AUTO: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);
            let mut cached = AUTO.lock().unwrap();
            let mut on = *cached.get_or_insert_with(crate::system::autostart);
            if ui.checkbox(&mut on, "Arrancar com o Windows").changed() {
                crate::system::set_autostart(on);
                *cached = Some(crate::system::autostart());
            }
        });
        row(ui, "Som ao arrancar", "O som original do Jarvis, um ficheiro teu (MP3, WAV ou FLAC; toca no máximo 20 segundos) ou nenhum. Não toca quando arranca escondido com o Windows.", |ui| {
            chips(ui, &mut s.startup_sound, &[("original".into(), "Original"), ("file".into(), "Ficheiro meu"), ("off".into(), "Desligado")], acc);
            if s.startup_sound == "file" {
                if button(ui, "Escolher…", false, acc).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Áudio", &["mp3", "wav", "flac"]).pick_file() {
                        s.startup_sound_file = p.to_string_lossy().into_owned();
                    }
                }
                let name = std::path::Path::new(&s.startup_sound_file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "nenhum ficheiro escolhido".into());
                ui.label(RichText::new(name).monospace().size(12.0).color(theme::MUTED));
            }
            if s.startup_sound != "off" && button(ui, "Ouvir", false, acc).clicked() {
                crate::sound::play_startup(&s.startup_sound, &s.startup_sound_file);
            }
        });
        if !crate::notify::windows_enabled() {
            ui_kit::hint(ui, "As notificações do Windows estão desligadas (Definições do Windows → Sistema → Notificações): os avisos do Jarvis só aparecem dentro da janela dele.", true, acc);
            if button(ui, "Abrir as definições de notificações", false, acc).clicked() {
                crate::brain::open("ms-settings:notifications");
            }
            ui.add_space(8.0);
        }
        row(ui, "Playlists favoritas", "Uma por linha, no formato Nome | link (Spotify ou Apple Music). Aparecem no painel Música do ecrã inicial.", |ui| {
            ui.add(egui::TextEdit::multiline(&mut s.music_links).hint_text("Foco | https://open.spotify.com/playlist/…").desired_rows(3).desired_width(f32::INFINITY));
        });
        row(ui, "Avisos", "", |ui| {
            ui.checkbox(&mut s.notify, "Notificações");
            ui.checkbox(&mut s.inform_chat, "Mensagem na conversa do item quando algo muda no board ou num PR");
            ui.checkbox(&mut s.meeting_reminder, "Aviso 5 minutos antes de cada reunião");
        });
    });
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Motor das conversas novas", "Cada conversa pode trocar de motor e de modelo por cima da caixa de escrita. Cada motor usa a tua subscrição dele; quando o plano de um acaba, a conversa pode seguir noutro (Claude Code → Antigravity → Codex, só os instalados) e fica lá até mudares.", |ui| {
            // "Só Claude Code" was the old way to turn the fallback off.
            if s.model == "claude" {
                s.model = "auto".into();
                s.fallback = false;
            }
            let mut opts: Vec<(String, &str)> = vec![("auto".into(), "Claude Code")];
            if crate::brain::gemini_installed() {
                opts.push(("gemini".into(), "Antigravity"));
            }
            if crate::brain::codex_installed() {
                opts.push(("codex".into(), "Codex"));
            }
            chips(ui, &mut s.model, &opts, acc);
            ui.end_row();
            ui.checkbox(&mut s.fallback, "Quando o plano de um motor acaba, continuar a conversa noutro motor instalado");
        });
        row(ui, "Claude Code na análise", "O modelo das conversas do Jarvis (análise e preparação). Automático usa o Sonnet e o Opus quando pedes (\"pensa a fundo\"). Cada conversa pode mudar por cima da caixa de escrita.", |ui| {
            chips(ui, &mut s.claude_model, &[("opus".into(), "Opus"), ("sonnet".into(), "Sonnet"), ("auto".into(), "Automático"), ("haiku".into(), "Haiku")], acc);
        });
        row(ui, "Claude Code na implementação", "A sessão que o Jarvis abre no worktree quando escolhes \"Claude Code\" para implementar. Fica escrito no .claude/settings.local.json do worktree (fora do git).", |ui| {
            chips(ui, &mut s.implement_model, &[("sonnet".into(), "Sonnet"), ("opus".into(), "Opus"), ("haiku".into(), "Haiku")], acc);
        });
        if crate::brain::gemini_installed() {
            row(ui, "Modelo do Antigravity", "", |ui| {
                let mut opts: Vec<(String, String)> = vec![("auto".into(), "Predefinido".into())];
                opts.extend(crate::brain::agy_models());
                let refs: Vec<(String, &str)> = opts.iter().map(|(a, b)| (a.clone(), b.as_str())).collect();
                chips(ui, &mut s.agy_model, &refs, acc);
            });
        } else {
            ui.label(RichText::new("Antigravity indisponível: falta o Antigravity CLI (agy).").small().color(theme::FAINT));
        }
        if crate::brain::codex_installed() {
            row(ui, "Modelo do Codex", "Os modelos que a tua conta ChatGPT tem no Codex. Predefinido usa o do teu config.toml do Codex.", |ui| {
                let mut opts: Vec<(String, String)> = vec![("auto".into(), "Predefinido".into())];
                opts.extend(crate::brain::codex_models());
                let refs: Vec<(String, &str)> = opts.iter().map(|(a, b)| (a.clone(), b.as_str())).collect();
                chips(ui, &mut s.codex_model, &refs, acc);
            });
        } else {
            ui.label(RichText::new("Codex indisponível: instala a app Codex ou o Codex CLI.").small().color(theme::FAINT));
        }
    });
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Voz", "Ctrl+Alt+J ou \"Falar com o Jarvis\" no ícone da bandeja. Diz \"obrigado\" para terminar.", |ui| {
            let (w, p) = voice_ready;
            ui.label(RichText::new(if w { "Whisper pronto" } else { "Falta o Whisper em vendor/" }).color(if w { theme::GREEN } else { theme::AMBER }));
            ui.label(RichText::new(if p { "Voz Piper pronta" } else { "Falta a Piper em vendor/" }).color(if p { theme::GREEN } else { theme::AMBER }));
        });
    });
    if s.accent != before.accent || s.ui_scale != before.ui_scale || s.poll_minutes != before.poll_minutes || s.notify != before.notify || s.inform_chat != before.inform_chat || s.meeting_reminder != before.meeting_reminder || s.model != before.model || s.claude_model != before.claude_model || s.agy_model != before.agy_model || s.codex_model != before.codex_model || s.fallback != before.fallback || s.implement_model != before.implement_model || s.startup_sound != before.startup_sound || s.startup_sound_file != before.startup_sound_file || s.music_links != before.music_links {
        settings::update(|x| *x = s);
    }
    look
}

/// Zoom from the "Tamanho do texto" setting (on top of the Windows scale, which egui already applies).
pub fn zoom(s: &Settings) -> f32 {
    match s.ui_scale.as_str() {
        "110" => 1.1,
        "125" => 1.25,
        _ => 1.0,
    }
}
