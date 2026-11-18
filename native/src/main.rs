// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agenda;
mod app;
mod azdo;
mod cleanup;
mod dashboard;
mod events;
mod notify;
mod prs;
mod settings_view;
mod sound;
mod system;
mod sysmon;
mod tasks;
mod voice;
mod board;
mod brain;
mod chat;
mod chat_view;
mod markdown;
mod motion;
mod music;
mod ui_kit;
mod views;
mod settings;
mod profile;
mod eu;
mod theme;

use app::{AppEvent, JarvisApp};
use eframe::egui;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, TrayIconBuilder, TrayIconEvent,
};

// Embedded, so an installed copy of the executable does not depend on the project folder for its icon.
const ICON: &[u8] = include_bytes!("../../assets/icon.ico");

fn load_icon() -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(ICON).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

fn main() -> eframe::Result {
    // Dev aid: `jarvis --profile` prints what the profile and the active pack resolve to.
    if std::env::args().any(|a| a == "--profile") {
        let p = profile::get();
        println!("nome {} ({}) | projetos {} | principal {}", p.name, p.spoken_name, p.projects_dir, profile::main_dir().display());
        println!("modos {:?} | dev {} | rh {}", profile::modes(), profile::dev(), profile::has_mode(profile::RH));
        match profile::pack() {
            Some(pk) => println!("pacote {} em {} | board {} | skill {:?} | cli {:?} | plugin {:?}", pk.m.name, pk.dir.display(), profile::board().is_some(), profile::skill_command(), profile::skill_cli(), profile::plugin_dir()),
            None => println!("sem pacote"),
        }
        println!("ramo Bug {:?} | outro {:?} | token {}", profile::branch_for("Bug", "Sprint 33", 8780), profile::branch_for("PBI", "Sprint 33", 8780), profile::board_token().is_some());
        for r in profile::repos() {
            println!("repo {} {} -> worktree {}", r.side, profile::repo_dir(&r).display(), profile::worktree_dir(&r, 8780).display());
        }
        for g in [profile::Guide::Persona, profile::Guide::PersonaRh, profile::Guide::Agy, profile::Guide::Prepare, profile::Guide::Test] {
            println!("guia {:?}", profile::guide(g));
        }
        let prep = brain::Prep { id: 8780, branch: profile::branch_for("Bug", "Sprint 1", 8780).unwrap_or_default() };
        for c in brain::prep_commands(&prep) {
            println!("  $ {c}");
        }
        println!("--- persona ---\n{}", profile::render(include_str!("../../docs/persona.md")));
        println!("--- o que a pessoa ensinou ({}) ---\n{}", eu::ensure().display(), eu::prompt_block());
        return Ok(());
    }
    // Dev aid: `jarvis --dump` loads the board once and prints a summary.
    if std::env::args().any(|a| a == "--dump") {
        let t = std::time::Instant::now();
        match azdo::dashboard() {
            Ok(d) => {
                println!("{} | {} | {:.1}s", d.me.name, d.sprint.name, t.elapsed().as_secs_f32());
                println!("agora {} | revisão {} | refined {} | tarefas {}", d.now.len(), d.review.len(), d.refined.len(), d.tasks.len());
                for it in &d.now {
                    println!("  #{} [{}] {} | tarefas {} | voltou {} | coment. {}", it.id, it.column, it.title, it.tasks.len(), it.back.is_some(), it.last_comment.is_some());
                }
            }
            Err(e) => println!("erro: {e}"),
        }
        return Ok(());
    }
    // Dev aid: `jarvis --ask claude|gemini "texto"` runs one turn and prints the stream (no window).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--voice-test") {
        let t = std::time::Instant::now();
        println!("{:?} em {:.1}s", voice::self_test(args.get(2).map(String::as_str).unwrap_or("Jarvis, o que tenho para fazer hoje?")), t.elapsed().as_secs_f32());
        return Ok(());
    }
    // Dev aid: `jarvis --prep-test <id> <branch>` checks that agy runs an allowed preparation command (the read-only
    // show) and that anything else (git status) stays blocked. Creates nothing.
    if args.get(1).map(String::as_str) == Some("--prep-test") {
        let id: i64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8780);
        let prep = brain::Prep { id, branch: args.get(3).cloned().unwrap_or_else(|| profile::branch_for("", "1", id).unwrap_or_default()) };
        let show = brain::prep_commands(&prep)[0].clone();
        let text = format!("Teste de permissões, não faças a preparação. 1) Corre exatamente: {show} e diz o título do item. 2) Corre: git -C {} status e diz se correu ou foi bloqueado. Responde em duas linhas.", profile::main_dir().display());
        let stop = brain::StopHandle::default();
        let mut on = |ev: brain::Ev| match ev {
            brain::Ev::Tool { name, input, .. } => println!("[{}] {}", name, brain::tool_detail(&name, &input)),
            brain::Ev::ToolResult { error, .. } => println!("   -> {}", if error { "BLOQUEADO/ERRO" } else { "correu" }),
            _ => {}
        };
        let r = brain::ask_gemini(&text, "", &brain::Turn { mode: profile::DEV, item: "" }, None, Some("gemini-3.8-flash-low"), Some(&prep), &stop, &mut on);
        println!("{:?}", r.map(|o| o.tokens));
        return Ok(());
    }
    // Dev aid: `jarvis --sound-wav <file>` writes the original start-up sound to a WAV (nothing is played).
    if args.get(1).map(String::as_str) == Some("--sound-wav") {
        let samples = sound::original();
        let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(args.get(2).cloned().unwrap_or_else(|| "startup.wav".into()), spec).unwrap();
        for s in &samples {
            w.write_sample((s * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let peak = samples.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        println!("{:.2} s, pico {peak:.2}", samples.len() as f32 / 44_100.0);
        return Ok(());
    }
    // Dev aid: `jarvis --music-test` prints what the Windows media controls report as playing.
    if args.get(1).map(String::as_str) == Some("--music-test") {
        let m = music::Music::start(egui::Context::default());
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let n = m.now();
        println!("app: {:?} | {:?} - {:?} | a tocar: {} | {} / {} | capa: {:?}", n.app, n.artist, n.title, n.playing, music::clock(n.position_now()), music::clock(n.duration), n.cover.as_ref().map(|c| c.1));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--mic-test") {
        println!("{:?}", voice::mic_test());
        return Ok(());
    }
    // Dev aid: `jarvis --compact-test` runs a tiny conversation on Haiku, continues it in a new session from a summary,
    // and checks the new session still knows what was said.
    if args.get(1).map(String::as_str) == Some("--compact-test") {
        let chats = chat::Chats::load(egui::Context::default());
        let engine = chat::Engine { model: "claude".into(), claude_model: "haiku".into(), agy_model: "auto".into() };
        let id = chats.create(Some("teste de resumo (apagar)".into()), None, None);
        let wait = || {
            while chats.with(|cs| cs.iter().any(|c| c.id == id && c.busy)) {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        };
        let show = |label: &str| {
            chats.with(|cs| {
                let c = cs.iter().find(|c| c.id == id).unwrap();
                println!("--- {label}: sessão {:?} · contexto {:?} · resumo guardado {}", c.session_id.as_deref().map(|s| &s[..8]), c.context_tokens, c.carry.is_some());
                for m in c.messages.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev() {
                    println!("  [{}] {}", m.role, m.text.chars().take(160).collect::<String>().replace('\n', " "));
                }
            })
        };
        chats.send(&id, "O código secreto do teste é ABACAXI-42. Responde só: anotado.", false, engine.clone());
        wait();
        show("1.ª volta");
        chats.compact(&id, engine.clone());
        wait();
        show("depois do resumo");
        chats.send(&id, "Qual é o código secreto do teste? Responde só com o código.", false, engine.clone());
        wait();
        show("sessão nova");
        chats.remove(&id);
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--ask") {
        let gemini = args.get(2).map(String::as_str) == Some("gemini");
        let text = args.get(3).cloned().unwrap_or_else(|| "Responde só: ok".into());
        let stop = brain::StopHandle::default();
        let mut on = |ev: brain::Ev| match ev {
            brain::Ev::Delta(t, _) => print!("{t}"),
            brain::Ev::Tool { name, input, .. } => println!("\n[{}] {}", brain::tool_label(&name, &input), brain::tool_detail(&name, &input)),
            brain::Ev::Session(s) => println!("[sessão {s}]"),
            _ => {}
        };
        let t = std::time::Instant::now();
        // The mode of a new general conversation: rh when the HR mode is on (JARVIS_SHOT_MODES=rh to try it).
        let about = brain::Turn { mode: profile::default_mode(false), item: "" };
        println!("[modo {}]", about.mode);
        let r = if gemini { brain::ask_gemini(&text, "", &about, None, args.get(4).map(String::as_str), None, &stop, &mut on) } else { brain::ask_claude(&text, &about, "sonnet", None, &stop, &mut on) };
        println!("\n{:?} em {:.1}s", r, t.elapsed().as_secs_f32());
        return Ok(());
    }
    // Dev screenshots run next to the installed Jarvis.
    if std::env::var("JARVIS_SHOT_RS").is_err() && std::env::var("JARVIS_DEV").is_err() && !system::single_instance() {
        return Ok(());
    }
    system::set_app_id();
    // Started with Windows (--hidden): straight to the tray, the window opens from there.
    let hidden = std::env::args().any(|a| a == "--hidden");
    let icon = load_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Jarvis")
        // JARVIS_SHOT_TALL=1: a tall window, to screenshot the whole start screen.
        .with_inner_size(if std::env::var("JARVIS_SHOT_TALL").is_ok() { [1280.0, 1500.0] } else { [1280.0, 860.0] })
        .with_min_inner_size([420.0, 560.0])
        .with_visible(!hidden);
    if let Some((rgba, w, h)) = &icon {
        viewport = viewport.with_icon(egui::IconData { rgba: rgba.clone(), width: *w, height: *h });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "Jarvis",
        options,
        Box::new(move |cc| {
            let (tx, rx) = mpsc::channel::<AppEvent>();
            let ctx = cc.egui_ctx.clone();
            app::init_post(tx.clone(), ctx.clone());

            // Tray: left click opens the window; menu with Abrir / Falar / Atualizar / Sair.
            let open = MenuItem::new("Abrir o Jarvis", true, None);
            let talk = MenuItem::new("Falar com o Jarvis\tCtrl+Alt+J", true, None);
            let refresh = MenuItem::new("Atualizar agora", true, None);
            let quit = MenuItem::new("Sair", true, None);
            let menu = Menu::new();
            let _ = menu.append_items(&[&open, &talk, &refresh, &PredefinedMenuItem::separator(), &quit]);
            let (talk_id, refresh_id) = (talk.id().clone(), refresh.id().clone());
            let tray = icon.as_ref().and_then(|(rgba, w, h)| {
                let tray_icon = tray_icon::Icon::from_rgba(rgba.clone(), *w, *h).ok()?;
                TrayIconBuilder::new().with_tooltip("Jarvis (Rust)").with_icon(tray_icon).with_menu(Box::new(menu)).build().ok()
            });
            let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
            {
                let tx = tx.clone();
                let ctx = ctx.clone();
                MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                    let ev = if e.id == quit_id {
                        AppEvent::Quit
                    } else if e.id == open_id {
                        AppEvent::Show
                    } else if e.id == talk_id {
                        AppEvent::Voice
                    } else if e.id == refresh_id {
                        AppEvent::Refresh
                    } else {
                        return;
                    };
                    let _ = tx.send(ev);
                    ctx.request_repaint();
                }));
            }
            {
                let tx = tx.clone();
                let ctx = ctx.clone();
                TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, .. } = e {
                        let _ = tx.send(AppEvent::Show);
                        ctx.request_repaint();
                    }
                }));
            }

            // Ctrl+Alt+J. Another app that registered it first keeps it.
            let mut hotkey_note = None;
            let manager = GlobalHotKeyManager::new().ok();
            if let Some(m) = &manager {
                if m.register(HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyJ)).is_err() {
                    hotkey_note = Some("Ctrl+Alt+J está ocupado por outra app: usa \"Falar com o Jarvis\" no ícone da bandeja.".to_owned());
                }
            }
            {
                let ctx = ctx.clone();
                GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
                    if e.state == global_hotkey::HotKeyState::Pressed {
                        let _ = tx.send(AppEvent::Hotkey);
                        ctx.request_repaint();
                    }
                }));
            }

            Ok(Box::new(JarvisApp::new(cc, rx, tray, manager, hotkey_note)))
        }),
    )
}
