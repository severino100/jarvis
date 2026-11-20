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
pub(crate) static PACK_MSG: std::sync::Mutex<Option<(String, bool)>> = std::sync::Mutex::new(None);
static PACK_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn pack_job(ctx: egui::Context, job: impl FnOnce() -> Result<String, String> + Send + 'static) {
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

/// Which part of Definições is open: the general settings or "O meu Jarvis".
static MINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Opens Definições on "O meu Jarvis" (screenshots: JARVIS_SHOT_VIEW=eu).
pub fn open_mine() {
    MINE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Returns true when the look changed (accent or size), so the app re-applies the style.
pub fn view(ui: &mut Ui, acc: Accent, voice_ready: (bool, bool)) -> bool {
    use std::sync::atomic::Ordering;
    let mine = MINE.load(Ordering::Relaxed);
    ui.horizontal(|ui| {
        if button(ui, "Geral", !mine, acc).clicked() {
            MINE.store(false, Ordering::Relaxed);
        }
        if button(ui, "O meu Jarvis", mine, acc).on_hover_text("O que usas, o que vês no Hoje e o que o Jarvis aprendeu contigo").clicked() {
            MINE.store(true, Ordering::Relaxed);
        }
    });
    ui.add_space(12.0);
    if mine {
        mine_panel(ui, acc);
        return false;
    }
    let mut s: Settings = settings::load();
    let before = s.clone();
    let mut look = false;
    profile_panel(ui, acc);
    ui.add_space(10.0);
    calendars_panel(ui, acc);
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
        row(ui, "O que o Jarvis precisa", "Os motores de IA e o git não vêm com o Jarvis: cada um instala-se à parte e usa a tua conta. Basta um motor; os outros servem para quando o plano de um acaba.", |ui| {
            ui.vertical(|ui| crate::requisitos::panel(ui, acc));
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
            ui.label(RichText::new("Antigravity indisponível: não está instalado (vê O que o Jarvis precisa, acima).").small().color(theme::FAINT));
        }
        if crate::brain::codex_installed() {
            row(ui, "Modelo do Codex", "Os modelos que a tua conta ChatGPT tem no Codex. Predefinido usa o do teu config.toml do Codex.", |ui| {
                let mut opts: Vec<(String, String)> = vec![("auto".into(), "Predefinido".into())];
                opts.extend(crate::brain::codex_models());
                let refs: Vec<(String, &str)> = opts.iter().map(|(a, b)| (a.clone(), b.as_str())).collect();
                chips(ui, &mut s.codex_model, &refs, acc);
            });
        } else {
            ui.label(RichText::new("Codex indisponível: não está instalado (vê O que o Jarvis precisa, acima).").small().color(theme::FAINT));
        }
    });
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Voz", "Ctrl+Alt+J ou \"Falar com o Jarvis\" no ícone da bandeja. Diz \"obrigado\" para terminar.", |ui| {
            let (w, p) = voice_ready;
            ui.label(RichText::new(if w { "Whisper pronto" } else { "Falta o Whisper (instalador)" }).color(if w { theme::GREEN } else { theme::AMBER }));
            ui.label(RichText::new(if p { "Voz Piper pronta" } else { "Falta a Piper (instalador)" }).color(if p { theme::GREEN } else { theme::AMBER }));
            voice_test(ui, acc);
            ui.end_row();
            mic_picker(ui, &mut s.mic, acc);
        });
    });
    ui.add_space(10.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        use crate::update::{self, State};
        row(ui, "Versão", "O Jarvis vê no GitHub se há versão nova ao arrancar, de hora a hora e quando abres a janela. Instalar descarrega o instalador novo, fecha o Jarvis e volta a abri-lo; as conversas e as definições ficam.", |ui| {
            ui.label(RichText::new(format!("Jarvis {}", update::current())).color(theme::BRIGHT));
            if !update::installed() {
                ui.label(RichText::new("Compilado a partir do código: não se atualiza sozinho.").small().color(theme::FAINT));
                return;
            }
            match update::state() {
                State::Available { version, .. } => {
                    ui.label(RichText::new(format!("Há a {version}")).color(theme::GREEN));
                    if ui_kit::button(ui, "Instalar", true, acc).clicked() {
                        update::install();
                    }
                }
                State::Downloading { version } => {
                    ui.label(RichText::new(format!("A descarregar a {version}…")).color(acc.light));
                }
                st => {
                    let (text, color) = match &st {
                        State::Checking => ("A ver…".to_owned(), theme::MUTED),
                        State::UpToDate => ("É a mais recente.".to_owned(), theme::GREEN),
                        State::Failed(e) => (e.clone(), theme::AMBER),
                        _ => (String::new(), theme::MUTED),
                    };
                    if !text.is_empty() {
                        ui.label(RichText::new(text).small().color(color));
                    }
                    if st != State::Checking && ui_kit::button(ui, "Procurar agora", false, acc).clicked() {
                        update::check_now();
                    }
                }
            }
        });
    });
    if s.accent != before.accent || s.ui_scale != before.ui_scale || s.poll_minutes != before.poll_minutes || s.notify != before.notify || s.inform_chat != before.inform_chat || s.meeting_reminder != before.meeting_reminder || s.model != before.model || s.claude_model != before.claude_model || s.agy_model != before.agy_model || s.codex_model != before.codex_model || s.fallback != before.fallback || s.implement_model != before.implement_model || s.startup_sound != before.startup_sound || s.startup_sound_file != before.startup_sound_file || s.music_links != before.music_links || s.mic != before.mic {
        settings::update(|x| *x = s);
    }
    look
}

/// "Testar a voz": checks every piece the voice needs (in the background: Whisper takes a few seconds) and lists
/// what failed with the button that fixes it.
fn voice_test(ui: &mut Ui, acc: Accent) {
    use crate::voice::{diagnose, Check};
    use std::sync::Mutex;
    /// None: never run; Some(None): running; Some(Some(list)): done.
    static RESULT: Mutex<Option<Option<Vec<Check>>>> = Mutex::new(None);
    let state = RESULT.lock().unwrap().clone();
    let running = matches!(state, Some(None));
    ui.add_enabled_ui(!running, |ui| {
        if ui_kit::button(ui, "Testar a voz", false, acc).on_hover_text("Vê os ficheiros, o Visual C++, a Piper e o Whisper, o microfone e as colunas").clicked() {
            *RESULT.lock().unwrap() = Some(None);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let list = diagnose();
                *RESULT.lock().unwrap() = Some(Some(list));
                ctx.request_repaint();
            });
        }
    });
    match state {
        Some(None) => {
            ui.end_row();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("A testar (a Piper diz uma frase e o Whisper lê-a)…").color(theme::MUTED));
            });
        }
        Some(Some(list)) => {
            ui.end_row();
            ui.vertical(|ui| {
                for c in list {
                    ui.horizontal_wrapped(|ui| {
                        ui_kit::diamond(ui, c.ok, if c.ok { theme::GREEN } else { theme::AMBER }, 4.0);
                        ui.label(RichText::new(c.what).color(theme::BRIGHT).strong());
                        ui.label(RichText::new(&c.detail).small().color(if c.ok { theme::MUTED } else { theme::AMBER }));
                        if let Some((label, link)) = &c.fix {
                            if ui_kit::button(ui, label, true, acc).on_hover_text(link).clicked() {
                                crate::brain::open(link);
                            }
                        }
                    });
                }
            });
        }
        None => {}
    }
}

/// "O meu Jarvis": the modes, the blocks of Hoje, and what this person taught Jarvis (instructions, memory, skills).
/// Everything here lives on this PC (%APPDATA%\Jarvis\perfil.json and %APPDATA%\Jarvis\eu).
fn mine_panel(ui: &mut Ui, acc: Accent) {
    use crate::{eu, profile};
    use std::sync::Mutex;
    /// The instructions being edited (None: not loaded yet) and whether they were just saved.
    static INSTRUCTIONS: Mutex<Option<String>> = Mutex::new(None);
    static SAVED: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    /// The rule being edited: (its text before, the new text).
    static EDITING: Mutex<Option<(String, String)>> = Mutex::new(None);

    let mut p = profile::get();
    let before = p.clone();
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Para que usas o Jarvis", "Cada modo traz os seus ecrãs. Podes ter os dois.", |ui| {
            ui.vertical(|ui| {
                for (mode, label, hint) in [
                    (profile::DEV, "Desenvolvimento", "o board, os PRs, as horas do sprint e a limpeza de worktrees"),
                    (profile::RH, "Recursos Humanos", "o assistente pessoal de RH: escreve, resume, analisa e propõe"),
                ] {
                    let mut on = p.modes.iter().any(|m| m == mode);
                    ui.horizontal(|ui| {
                        // At least one mode stays on.
                        let last = on && p.modes.len() == 1;
                        if ui.add_enabled(!last, egui::Checkbox::new(&mut on, RichText::new(label).color(theme::BRIGHT))).changed() {
                            if on {
                                p.modes.push(mode.to_owned());
                            } else {
                                p.modes.retain(|m| m != mode);
                            }
                        }
                        ui.label(RichText::new(hint).small().color(theme::MUTED));
                    });
                }
            });
        });
        row(ui, "No ecrã Hoje", "Os blocos que queres ver quando abres o Jarvis.", |ui| {
            let dev = p.modes.iter().any(|m| m == profile::DEV);
            for (key, label, dev_only) in crate::dashboard::BLOCKS {
                if dev_only && !dev {
                    continue;
                }
                let mut on = !p.today_hidden.iter().any(|h| h == key);
                if ui.checkbox(&mut on, label).changed() {
                    if on {
                        p.today_hidden.retain(|h| h != key);
                    } else {
                        p.today_hidden.push(key.to_owned());
                    }
                }
            }
        });
        row(ui, "No menu", "Os ecrãs que ficam no menu à esquerda. Hoje e Definições ficam sempre; os do desenvolvimento só aparecem com esse modo.", |ui| {
            let dev = p.modes.iter().any(|m| m == profile::DEV);
            for (key, label, dev_only) in crate::app::menu_entries() {
                if dev_only && !dev {
                    continue;
                }
                let mut on = !p.menu_hidden.contains(&key);
                if ui.checkbox(&mut on, label).changed() {
                    if on {
                        p.menu_hidden.retain(|h| *h != key);
                    } else {
                        p.menu_hidden.push(key);
                    }
                }
            }
        });
    });
    if p != before {
        profile::update(|x| *x = p);
    }
    ui.add_space(10.0);

    let name = profile::name();
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "As tuas instruções", &format!("O que o Jarvis deve saber de ti em todas as conversas: o tom, como assinas, o que nunca fazer. As regras do Jarvis (confirmar antes de enviar, criar ou apagar, e os dados pessoais) ficam sempre por cima destas. Ficam em {}.", eu::instructions_file().display()), |ui| {
            let mut guard = INSTRUCTIONS.lock().unwrap();
            let text = guard.get_or_insert_with(eu::instructions);
            ui.vertical(|ui| {
                ui.add(egui::TextEdit::multiline(text).hint_text(format!("Por exemplo: Trabalho em Recursos Humanos. Trata-me por tu. Assino como \"{name} · Recursos Humanos\". Emails curtos, sem emojis.")).desired_rows(5).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    let changed = *text != eu::instructions();
                    if ui.add_enabled_ui(changed, |ui| button(ui, "Guardar", true, acc)).inner.clicked() {
                        eu::set_instructions(text);
                        *SAVED.lock().unwrap() = Some(std::time::Instant::now());
                    }
                    if changed && button(ui, "Desfazer alterações", false, acc).clicked() {
                        *text = eu::instructions();
                    }
                    if SAVED.lock().unwrap().is_some_and(|t| t.elapsed().as_secs() < 4) {
                        ui.label(RichText::new("Guardado. Conta a partir da próxima mensagem.").color(theme::GREEN));
                    }
                });
            });
        });
    });
    ui.add_space(10.0);

    ui_kit::hud_panel(ui, acc, false, |ui| {
        let rules = eu::rules();
        row(ui, "O que o Jarvis aprendeu", "Quando dizes \"lembra-te que…\" ou corriges uma preferência, o Jarvis guarda a regra aqui e lê-a em todas as conversas. Podes mudar ou apagar qualquer uma.", |ui| {
            ui.vertical(|ui| {
                if rules.is_empty() {
                    ui.label(RichText::new("Ainda nada. Numa conversa, diz por exemplo: \"lembra-te que assino sempre como Rita\".").color(theme::MUTED));
                }
                let mut editing = EDITING.lock().unwrap();
                let (mut forget, mut save) = (None, None);
                for r in &rules {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if r.date.is_empty() { "—".to_owned() } else { ui_kit::short_date(&format!("{}T12:00:00Z", r.date)) }).monospace().size(11.5).color(theme::FAINT));
                        match editing.as_mut() {
                            Some((old, new)) if *old == r.text => {
                                let w = (ui.available_width() - 220.0).max(160.0);
                                let enter = ui.add(egui::TextEdit::singleline(new).desired_width(w)).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if button(ui, "Guardar", true, acc).clicked() || enter {
                                    save = Some((old.clone(), new.clone()));
                                }
                                if button(ui, "Cancelar", false, acc).clicked() {
                                    save = Some((String::new(), String::new()));
                                }
                            }
                            _ => {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if button(ui, "Apagar", false, acc).clicked() {
                                        forget = Some(r.text.clone());
                                    }
                                    if button(ui, "Editar", false, acc).clicked() {
                                        save = None;
                                        *editing = Some((r.text.clone(), r.text.clone()));
                                    }
                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        ui.add(egui::Label::new(RichText::new(&r.text).color(theme::TEXT)).wrap());
                                    });
                                });
                            }
                        }
                    });
                }
                if let Some((old, new)) = save {
                    if !old.is_empty() {
                        eu::edit(&old, &new);
                    }
                    *editing = None;
                }
                if let Some(t) = forget {
                    eu::forget(&t);
                }
            });
        });
        let examples = eu::examples();
        row(ui, "Exemplos do teu estilo", "Quando editas um rascunho de email do Jarvis e o levas (Abrir no Outlook ou Copiar), a tua versão final fica aqui e o Jarvis escreve os próximos como ela. Ficam só os 5 mais recentes, neste PC; apaga os que tiverem dados que não queres guardar.", |ui| {
            ui.vertical(|ui| {
                if examples.is_empty() {
                    ui.label(RichText::new("Ainda nenhum. Carrega em Editar num rascunho de email, muda o que quiseres e leva-o.").color(theme::MUTED));
                }
                let mut remove = None;
                for (i, e) in examples.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(ui_kit::short_date(&format!("{}T12:00:00Z", e.date))).monospace().size(11.5).color(theme::FAINT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if button(ui, "Apagar", false, acc).clicked() {
                                remove = Some(i);
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let first = e.body.lines().find(|l| !l.trim().is_empty()).unwrap_or_default();
                                let head = if e.subject.is_empty() { first.to_owned() } else { format!("{} · {first}", e.subject) };
                                ui.add(egui::Label::new(RichText::new(head).color(theme::TEXT)).truncate()).on_hover_text(&e.body);
                            });
                        });
                    });
                }
                if let Some(i) = remove {
                    eu::remove_example(i);
                }
            });
        });
    });
    ui.add_space(10.0);

    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "As tuas skills", "Comandos teus para o Claude Code: cada skill é uma pasta com um SKILL.md dentro da pasta skills. Só tu as tens; as da equipa vêm do pacote.", |ui| {
            ui.vertical(|ui| {
                let skills = eu::skills();
                if skills.is_empty() {
                    ui.label(RichText::new("Ainda não tens skills pessoais.").color(theme::MUTED));
                }
                for (name, about, _) in &skills {
                    ui.horizontal(|ui| {
                        ui_kit::diamond(ui, true, acc.main, 3.5);
                        ui.label(RichText::new(format!("/eu:{name}")).monospace().color(acc.light));
                        if !about.is_empty() {
                            ui.add(egui::Label::new(RichText::new(about).small().color(theme::MUTED)).truncate());
                        }
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if button(ui, "Abrir a pasta das skills", false, acc).clicked() {
                        eu::ensure();
                        crate::brain::open(&eu::skills_dir().to_string_lossy());
                    }
                    if button(ui, "Abrir a pasta \"eu\"", false, acc).on_hover_text("Instruções, memória e skills, em ficheiros de texto").clicked() {
                        crate::brain::open(&eu::ensure().to_string_lossy());
                    }
                });
            });
        });
    });
}

/// Zoom from the "Tamanho do texto" setting (on top of the Windows scale, which egui already applies).
pub fn zoom(s: &Settings) -> f32 {
    match s.ui_scale.as_str() {
        "110" => 1.1,
        "125" => 1.25,
        _ => 1.0,
    }
}

/* ---------- calendars ---------- */

static CAL_MSG: std::sync::Mutex<Option<(String, bool)>> = std::sync::Mutex::new(None);

fn set_calendars(list: Vec<crate::profile::CalSource>) {
    crate::profile::update(|p| p.calendars = Some(list));
    crate::agenda::CALENDARS_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    crate::app::post(crate::app::AppEvent::Refresh);
}

/// Which calendars the agenda reads: the classic Outlook on this PC and/or any iCalendar (.ics) link, which is what
/// Google Calendar, Outlook.com / Microsoft 365, iCloud, Nextcloud and Proton publish.
fn calendars_panel(ui: &mut Ui, acc: Accent) {
    use crate::profile::{self, CalSource};
    static NAME: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    static LINK: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let list = profile::calendars();
    let (mut remove, mut add_outlook, mut add_link) = (None, false, false);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        row(ui, "Calendários", "A agenda e os avisos de reuniões juntam os calendários que ligares. Podes ter vários. Só leitura: o Jarvis nunca cria nem altera eventos.", |ui| {
            ui.vertical(|ui| {
                for (i, c) in list.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&c.name).color(theme::BRIGHT));
                        ui.label(RichText::new(if c.kind == "ics" { "link iCal" } else { "Outlook clássico" }).small().color(theme::FAINT));
                        if button(ui, "Remover", false, acc).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if list.is_empty() {
                    ui.label(RichText::new("Nenhum calendário ligado.").small().color(theme::MUTED));
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let mut name = NAME.lock().unwrap();
                    let mut link = LINK.lock().unwrap();
                    ui.add(egui::TextEdit::singleline(&mut *name).hint_text("Nome (ex.: Pessoal)").desired_width(140.0));
                    ui.add(egui::TextEdit::singleline(&mut *link).password(true).hint_text("link iCal (https:// ou webcal://)").desired_width(300.0));
                    if button(ui, "Adicionar link", true, acc).clicked() && !link.trim().is_empty() {
                        add_link = true;
                    }
                });
                if !list.iter().any(|c| c.kind == "outlook") && button(ui, "Adicionar o Outlook deste PC", false, acc).clicked() {
                    add_outlook = true;
                }
                if let Some((m, err)) = CAL_MSG.lock().unwrap().clone() {
                    ui.label(RichText::new(m).small().color(if err { theme::AMBER } else { theme::GREEN }));
                }
                ui.label(RichText::new("Google Calendar: Definições do calendário → \"Endereço secreto no formato iCal\". Outlook.com / Microsoft 365: Definições → Calendário → Calendários partilhados → Publicar um calendário → ICS. iCloud: partilhar o calendário → Calendário público. O link dá acesso de leitura ao calendário: fica cifrado neste PC.").small().color(theme::FAINT));
            });
        });
    });
    if let Some(i) = remove {
        let mut l = list.clone();
        l.remove(i);
        set_calendars(l);
        *CAL_MSG.lock().unwrap() = None;
    }
    if add_outlook {
        let mut l = list.clone();
        l.push(CalSource::outlook());
        set_calendars(l);
    }
    if add_link {
        let link = std::mem::take(&mut *LINK.lock().unwrap());
        let name = std::mem::take(&mut *NAME.lock().unwrap());
        let name = if name.trim().is_empty() { format!("Calendário {}", list.len() + 1) } else { name };
        *CAL_MSG.lock().unwrap() = Some(("A verificar o link…".into(), false));
        let ctx = ui.ctx().clone();
        std::thread::spawn(move || {
            *CAL_MSG.lock().unwrap() = Some(match crate::ics::check(&link) {
                Ok(n) => {
                    let mut l = profile::calendars();
                    l.push(CalSource::ics(&name, &crate::ics::normalize_url(&link)));
                    set_calendars(l);
                    (format!("\"{}\" ligado ({n} eventos no calendário).", name.trim()), false)
                }
                Err(e) => (e, true),
            });
            ctx.request_repaint();
        });
    }
}

/// Which microphone the voice listens to, with a test that shows whether it hears anything.
fn mic_picker(ui: &mut Ui, mic: &mut String, acc: Accent) {
    use std::sync::Mutex;
    static NAMES: Mutex<Option<Vec<String>>> = Mutex::new(None);
    /// None: not tested; Some(None): listening; Some(Some(..)): the result.
    static TEST: Mutex<Option<Option<Result<(f32, f32), String>>>> = Mutex::new(None);
    let names = NAMES.lock().unwrap().get_or_insert_with(crate::voice::mic_names).clone();
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Microfone").color(theme::BRIGHT).strong());
            let shown = if mic.is_empty() { "Predefinido do Windows".to_owned() } else { mic.clone() };
            egui::ComboBox::from_id_salt("mic-choice").selected_text(shown).width(320.0).show_ui(ui, |ui| {
                if ui.selectable_label(mic.is_empty(), "Predefinido do Windows").clicked() {
                    mic.clear();
                }
                for n in &names {
                    if ui.selectable_label(mic == n, n).clicked() {
                        *mic = n.clone();
                    }
                }
            });
            if button(ui, "Atualizar a lista", false, acc).on_hover_text("Volta a ver os microfones ligados").clicked() {
                *NAMES.lock().unwrap() = None;
            }
            let state = TEST.lock().unwrap().clone();
            let listening = matches!(state, Some(None));
            ui.add_enabled_ui(!listening, |ui| {
                if button(ui, "Testar o microfone", false, acc).on_hover_text("Ouve 3 segundos: fala enquanto testa").clicked() {
                    // The choice is read from the saved settings: save it first.
                    let chosen = mic.clone();
                    crate::settings::update(|x| x.mic = chosen);
                    *TEST.lock().unwrap() = Some(None);
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let r = crate::voice::mic_level(3);
                        *TEST.lock().unwrap() = Some(Some(r));
                        ctx.request_repaint();
                    });
                }
            });
        });
        match TEST.lock().unwrap().clone() {
            Some(None) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("A ouvir… fala agora.").color(theme::MUTED));
                });
            }
            Some(Some(Ok((_, loudest)))) => {
                let (ok, msg) = if loudest < 0.002 {
                    (false, "Não ouvi nada (silêncio). Escolhe outro microfone, ou vê se o Windows deixa as apps usar o microfone e se não está sem som.".to_owned())
                } else if loudest < 0.02 {
                    (true, format!("Ouvi, mas muito baixo (nível {loudest:.3}). Funciona: o Jarvis aumenta o volume. Aproxima-te ou sobe a entrada em Definições do Windows → Som."))
                } else {
                    (true, format!("Ouvi bem (nível {loudest:.3})."))
                };
                ui.label(RichText::new(msg).small().color(if ok { theme::GREEN } else { theme::AMBER }));
                if !ok && button(ui, "Abrir a privacidade do microfone", true, acc).clicked() {
                    crate::brain::open("ms-settings:privacy-microphone");
                }
            }
            Some(Some(Err(e))) => {
                ui.label(RichText::new(e).small().color(theme::AMBER));
            }
            None => {}
        }
    });
}
