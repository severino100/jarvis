//! Main window: sidebar (sprint, sections with counts, refresh), HUD background with the reactor, and the views.

use crate::{
    agenda::{self, Agenda, AgendaUi},
    azdo,
    board::Board,
    brain,
    chat::{self, ChatEvent, Chats},
    chat_view::{self, Action, ChatUi},
    cleanup::{self, CleanupUi},
    dashboard, events, motion, notify, prs::{self, Prs},
    sysmon::SysMon,
    settings, settings_view,
    tasks::{self, TasksUi},
    theme, ui_kit, views,
    voice::Voice,
};
use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, OnceLock,
    },
    time::Instant,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Today,
    Agenda,
    Tasks,
    Now,
    Prs,
    Review,
    Refined,
    Chat,
    Hours,
    Cleanup,
    Settings,
}

const NAV: [(View, &str); 10] = [
    (View::Today, "Hoje"),
    (View::Agenda, "Agenda"),
    (View::Tasks, "Tarefas"),
    (View::Now, "Agora"),
    (View::Prs, "PRs"),
    (View::Review, "Revisão e QA"),
    (View::Refined, "Refined livre"),
    (View::Chat, "Chat"),
    (View::Hours, "Horas do sprint"),
    (View::Cleanup, "Limpeza"),
];

/// Views of the development mode (board, PRs, hours, cleanup): not in the menu when that mode is off.
fn dev_only(v: View) -> bool {
    matches!(v, View::Now | View::Prs | View::Review | View::Refined | View::Hours | View::Cleanup)
}

/// Whether a view belongs to the modes that are on (Definições → O meu Jarvis).
pub fn shown(v: View) -> bool {
    !dev_only(v) || crate::profile::dev()
}

#[derive(Clone)]
pub enum AppEvent {
    Show,
    /// Ctrl+Alt+J: talk to Jarvis.
    Hotkey,
    Voice,
    Refresh,
    Quit,
    OpenUrl(String),
    OpenChat(String),
    /// A new general conversation, opened in the Chat view.
    NewChat,
    Go(View),
    /// Something changed (a notice): just draw a new frame.
    Repaint,
    Music(crate::music::Cmd),
}

static POST: OnceLock<(Sender<AppEvent>, egui::Context)> = OnceLock::new();

pub fn init_post(tx: Sender<AppEvent>, ctx: egui::Context) {
    let _ = POST.set((tx, ctx));
}

/// Sends an event to the app from anywhere (toasts, voice, background threads) and wakes it up.
pub fn post(ev: AppEvent) {
    if let Some((tx, ctx)) = POST.get() {
        let _ = tx.send(ev);
        ctx.request_repaint();
    }
}

pub struct JarvisApp {
    view: View,
    acc: theme::Accent,
    board: Board,
    events: Receiver<AppEvent>,
    quitting: bool,
    started: Instant,
    hotkey_note: Option<String>,
    shot: Option<(PathBuf, bool, String)>,
    shot_done: Arc<AtomicBool>,
    chats: Chats,
    chat_ui: ChatUi,
    agenda: Agenda,
    agenda_ui: AgendaUi,
    tasks_ui: TasksUi,
    prs: Prs,
    cleanup_ui: CleanupUi,
    hours_ui: views::HoursUi,
    voice: Voice,
    reminded: std::collections::HashSet<String>,
    last_board: Option<Instant>,
    last_prs: u64,
    last_tick: Instant,
    sysmon: SysMon,
    music: crate::music::Music,
    /// Accumulated reactor rotation and the time of the last frame (the reactor speeds up while Jarvis works).
    reactor_phase: f32,
    last_time: f64,
    /// The view on screen and when it arrived (for the entrance), and a counter that restarts the section rule.
    view_shown: View,
    view_at: f64,
    view_seq: u64,
    /// "Começar" waiting for the engine choice.
    start_choice: Option<StartChoice>,
    /// Start-up sequence: when it began (None once done or when skipped).
    boot: Option<f64>,
    // Kept alive for the whole run: dropping them removes the tray icon / unregisters the hotkey.
    _tray: Option<tray_icon::TrayIcon>,
    _hotkey: Option<global_hotkey::GlobalHotKeyManager>,
}

fn view_from(name: &str) -> View {
    match name {
        "now" => View::Now,
        "review" => View::Review,
        "refined" => View::Refined,
        "hours" => View::Hours,
        "chat" | "chat-rh" => View::Chat,
        "agenda" => View::Agenda,
        "tasks" => View::Tasks,
        "prs" => View::Prs,
        "cleanup" => View::Cleanup,
        "settings" => View::Settings,
        _ => View::Today,
    }
}

impl JarvisApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        events: Receiver<AppEvent>,
        tray: Option<tray_icon::TrayIcon>,
        hotkey: Option<global_hotkey::GlobalHotKeyManager>,
        hotkey_note: Option<String>,
    ) -> Self {
        let s = settings::load();
        let acc = theme::accent(&s.accent);
        theme::install_fonts(&cc.egui_ctx);
        theme::install_style(&cc.egui_ctx, acc);
        cc.egui_ctx.set_zoom_factor(settings_view::zoom(&s));
        let poll = if s.poll_minutes == 0 { 5 } else { s.poll_minutes };
        let board = Board::start(cc.egui_ctx.clone(), poll);
        // Dev aid: JARVIS_SHOT_RS=<file.png>[ JARVIS_SHOT_VIEW=now|review|refined|hours] saves one screenshot and quits.
        let shot_view = std::env::var("JARVIS_SHOT_VIEW").unwrap_or_default();
        let chats = Chats::load(cc.egui_ctx.clone());
        if shot_view == "chat-rh" {
            chats.seed_demo_rh();
        }
        if shot_view == "chat" {
            chats.seed_demo();
        }
        let mut app = Self {
            view: view_from(&shot_view),
            acc,
            board: board.clone(),
            events,
            quitting: false,
            started: Instant::now(),
            hotkey_note,
            shot: std::env::var("JARVIS_SHOT_RS").ok().map(|p| (PathBuf::from(p), false, shot_view.clone())),
            shot_done: Arc::new(AtomicBool::new(false)),
            chats: chats.clone(),
            chat_ui: ChatUi::default(),
            agenda: Agenda::new(cc.egui_ctx.clone()),
            agenda_ui: AgendaUi::default(),
            tasks_ui: TasksUi::default(),
            prs: Prs::start(cc.egui_ctx.clone(), poll),
            cleanup_ui: CleanupUi::default(),
            hours_ui: views::HoursUi::default(),
            voice: Voice::new(cc.egui_ctx.clone(), chats.clone(), board),
            reminded: Default::default(),
            last_board: None,
            last_prs: 0,
            last_tick: Instant::now(),
            sysmon: SysMon::start(cc.egui_ctx.clone()),
            music: crate::music::Music::start(cc.egui_ctx.clone()),
            reactor_phase: 0.0,
            start_choice: None,
            last_time: 0.0,
            view_shown: View::Today,
            view_at: 0.0,
            view_seq: 0,
            boot: (!std::env::args().any(|a| a == "--hidden") && std::env::var("JARVIS_SHOT_RS").is_err() && !motion::reduced()).then_some(-1.0),
            _tray: tray,
            _hotkey: hotkey,
        };
        // The team packs follow their git repositories, so a skill changed by the team reaches everyone.
        std::thread::spawn(|| {
            let lines = crate::profile::update_packs();
            if !lines.is_empty() {
                crate::notify::notify("Pacotes de equipa", &lines.join(" "), AppEvent::Go(View::Settings));
                post(AppEvent::Refresh);
            }
        });
        // The start-up sound, with the start-up sequence (not when starting hidden in the tray, nor for screenshots).
        if !std::env::args().any(|a| a == "--hidden") && std::env::var("JARVIS_SHOT_RS").is_err() {
            crate::sound::play_startup(if s.startup_sound.is_empty() { "original" } else { &s.startup_sound }, &s.startup_sound_file);
        }
        // JARVIS_SHOT_START=<id>: the engine question of "Começar", for screenshots.
        if let Some(id) = std::env::var("JARVIS_SHOT_START").ok().and_then(|v| v.parse().ok()) {
            app.start_choice = Some(StartChoice { id, title: "Exemplo de item".into(), agy: false, model: "auto".into(), remember: false });
        }
        // JARVIS_SHOT_NOTIFY=1: two notices inside the window, for screenshots.
        if std::env::var("JARVIS_SHOT_NOTIFY").is_ok() {
            notify::notify("#8777 voltou para trás", "Ana Silva: a localização continua a ficar disabled ao manter no fornecedor", AppEvent::Show);
            notify::notify("Comentário no PR !3211", "Nuno Chumbo: falta o data-test-id no botão novo", AppEvent::Show);
        }
        if std::env::var("JARVIS_SHOT_VOICE").is_ok() {
            app.voice.demo();
        }
        // JARVIS_SHOT_WINDOW=1 also opens the demo conversation in its own window (checks the extra viewports).
        if shot_view == "chat" && std::env::var("JARVIS_SHOT_WINDOW").is_ok() {
            if let Some(id) = app.chats.with(|cs| cs.first().map(|c| c.id.clone())) {
                app.open_window(&id);
            }
        }
        app
    }

    fn show_window(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}

impl eframe::App for JarvisApp {
    // Runs even while the window is hidden in the tray, whenever the tray or the hotkey wakes us up.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                AppEvent::Show => Self::show_window(ctx),
                AppEvent::Repaint => {}
                AppEvent::Music(c) => self.music.send(c),
                AppEvent::Hotkey | AppEvent::Voice => self.voice.toggle(),
                AppEvent::Refresh => {
                    self.board.refresh();
                    self.prs.refresh();
                }
                AppEvent::OpenUrl(u) => brain::open(&u),
                AppEvent::OpenChat(id) => self.open_window(&id),
                AppEvent::NewChat => {
                    self.chat_ui.selected = Some(self.chats.create(None, None, None));
                    self.view = View::Chat;
                    Self::show_window(ctx);
                }
                AppEvent::Go(v) => {
                    self.view = v;
                    Self::show_window(ctx);
                }
                AppEvent::Quit => {
                    self.voice.end();
                    self.chats.stop_all();
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        for ev in self.chats.take_events() {
            match ev {
                ChatEvent::Handoff(url) => brain::open(&url),
                ChatEvent::Implement(id) => {
                    if let Some(w) = self.chats.with(|cs| cs.iter().find(|c| c.id == id).and_then(|c| c.worktree.clone())) {
                        brain::exclude_jarvis_dir(&w);
                    }
                    self.board.refresh();
                    self.open_window(&id);
                    if let Some(title) = self.chats.with(|cs| cs.iter().find(|c| c.id == id).map(|c| c.title.clone())) {
                        notify::notify("Pronto para implementar", &format!("{title}: escolhe Claude Code ou Antigravity."), AppEvent::OpenChat(id.clone()));
                    }
                }
                ChatEvent::TurnEnd(id) => self.turn_ended(ctx, &id),
            }
        }
        self.background(ctx);
        // Closing the window hides it in the tray; Sair in the tray menu quits.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if let Some((path, requested, _)) = &mut self.shot {
            let ready = self.board.with(|b| b.data.is_some() || b.error.is_some());
            if self.shot_done.load(Ordering::SeqCst) {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if !*requested && ready && self.started.elapsed().as_secs_f32() > std::env::var("JARVIS_SHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(2.0) {
                *requested = true;
                let (path, done, repaint) = (path.clone(), self.shot_done.clone(), ctx.clone());
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let [w, h] = img.size;
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                        let _ = buf.save(&path);
                    }
                    done.store(true, Ordering::SeqCst);
                    repaint.request_repaint();
                })));
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let acc = self.acc;
        // The reactor turns slowly; 30 fps is plenty and keeps the CPU quiet.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));

        // Jarvis at work (a reply, the board or the PRs loading, the voice thinking) spins the reactor up and makes
        // the core pulse; it settles back when the work is done.
        let now = motion::now(ui.ctx());
        let dt = if self.last_time > 0.0 { (now - self.last_time).clamp(0.0, 0.1) as f32 } else { 0.0 };
        self.last_time = now;
        let working = self.chats.with(|cs| cs.iter().any(|c| c.busy))
            || self.board.with(|b| b.loading)
            || self.prs.with(|p| p.loading)
            || self.voice.with(|v| v.on && v.phase != "idle");
        let energy = motion::tween(ui.ctx(), egui::Id::unique("reactor-energy"), if working { 1.0 } else { 0.0 }, 0.6, 0.0);
        let speed = if motion::reduced() { 0.5 } else { 1.0 + 2.6 * energy };
        self.reactor_phase += dt * speed;
        let t = self.reactor_phase;
        // A view of a mode that was switched off (or asked for by a screenshot) falls back to Hoje.
        if !shown(self.view) {
            self.view = View::Today;
        }

        egui::Panel::left("nav")
            .exact_size(256.0)
            .resizable(false)
            .frame(Frame::new().fill(theme::SIDEBAR).inner_margin(Margin { left: 14, right: 14, top: 18, bottom: 14 }).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14))))
            .show(ui, |ui| self.sidebar(ui, acc, t, energy));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin { left: 34, right: 34, top: 28, bottom: 10 }))
            .show(ui, |ui| {
                paint_grid(ui, acc);
                // Each section arrives with a short fade and a small rise (state change, not choreography).
                if self.view != self.view_shown {
                    self.view_shown = self.view;
                    self.view_at = now;
                    self.view_seq += 1;
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("view-seq"), self.view_seq));
                }
                let k = motion::ease_out(((now - self.view_at) as f32 / 0.26).min(1.0));
                if k < 1.0 {
                    ui.set_opacity(0.15 + 0.85 * k);
                    if !motion::reduced() {
                        ui.add_space((1.0 - k) * 10.0);
                    }
                    ui.ctx().request_repaint();
                }
                self.content(ui, acc);
            });
        self.start_dialog(&ui.ctx().clone(), acc);
        self.boot_overlay(ui.ctx(), acc, t, now);
        notify::draw(ui.ctx(), acc);

        let ctx = ui.ctx().clone();
        if let Some(id) = ui_kit::take_start(&ctx) {
            self.start_item(id);
        }
        if let Some((key, thread)) = prs::take_fix(&ctx) {
            self.fix_pr(&key, thread);
        }
        self.chat_windows(&ctx, acc);
        self.voice_panel(&ctx, acc, t);
    }
}

impl JarvisApp {
    /// Start-up: the reactor lights up in the middle of the window and its rings draw in, then the HUD shows through.
    /// About a second; a click skips it. Not shown when starting hidden, in screenshots or with reduced motion.
    fn boot_overlay(&mut self, ctx: &egui::Context, acc: theme::Accent, phase: f32, now: f64) {
        let Some(start) = self.boot else { return };
        let start = if start < 0.0 {
            self.boot = Some(now);
            now
        } else {
            start
        };
        let e = (now - start) as f32;
        if e > 1.35 || ctx.input(|i| i.pointer.any_click() || i.key_pressed(egui::Key::Escape)) {
            self.boot = None;
            // The screen behind arrives as the start-up sequence leaves.
            self.view_at = now;
            return;
        }
        ctx.request_repaint();
        let screen = ctx.content_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("boot")));
        let fade = 1.0 - motion::ease_out(((e - 0.95) / 0.4).max(0.0));
        p.rect_filled(screen, 0.0, theme::alpha(theme::BG, fade));
        let drawn = motion::ease_out(e / 0.8);
        let size = 120.0 + 40.0 * motion::ease_out(e / 1.0);
        let c = screen.center() - vec2(0.0, 24.0);
        let ui = egui::Ui::new(ctx.clone(), egui::Id::unique("boot-ui"), egui::UiBuilder::new().layer_id(p.layer_id()).max_rect(screen));
        let mut ui = ui;
        ui.set_opacity(fade);
        paint_reactor(&ui, Rect::from_center_size(c, vec2(size, size)), acc, phase * 3.0, 1.0 - drawn * 0.6, drawn);
        let name_k = motion::ease_out(((e - 0.3) / 0.5).max(0.0));
        ui.painter().text(
            c + vec2(0.0, size / 2.0 + 34.0),
            egui::Align2::CENTER_CENTER,
            "J A R V I S",
            ui_kit::display(26.0),
            theme::alpha(theme::BRIGHT, name_k * fade),
        );
        let _ = &mut ui;
    }

    fn sidebar(&mut self, ui: &mut Ui, acc: theme::Accent, t: f32, energy: f32) {
        let board = self.board.clone();
        let voice_on = self.voice.with(|v| v.on);
        let mut talk = false;
        let dev = crate::profile::dev();
        let me = if dev { None } else { Some(crate::profile::name()) };
        board.with(|b| {
            // Without the dev mode the board is not part of the screen (nor its sprint box and refresh).
            let d = b.data.as_ref().filter(|_| dev);
            ui.horizontal(|ui| {
                let (rect, r) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::click());
                paint_reactor(ui, rect, acc, t, energy, 1.0);
                if r.on_hover_text("Falar com o Jarvis (Ctrl+Alt+J)").clicked() {
                    talk = true;
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("JARVIS").font(ui_kit::display(22.0)).color(theme::BRIGHT).extra_letter_spacing(4.0));
                    ui.label(RichText::new(me.as_deref().or(d.map(|d| d.me.name.as_str())).unwrap_or("A ligar…")).monospace().size(11.0).color(theme::MUTED));
                });
            });
            ui.add_space(14.0);

            if dev {
                Frame::new()
                    .fill(theme::alpha(acc.main, 0.1))
                    .stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14)))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match d {
                            Some(d) => sprint_box(ui, d, acc),
                            None => {
                                ui.label(RichText::new("SPRINT").font(ui_kit::display(13.0)).color(acc.light).extra_letter_spacing(2.0));
                                ui.label(RichText::new(if b.loading { "A ler o Azure DevOps…" } else { "Sem dados" }).small().color(theme::MUTED));
                            }
                        }
                    });
                ui.add_space(10.0);
            }
            if voice_button(ui, voice_on, acc).clicked() {
                talk = true;
            }
            ui.add_space(10.0);

            // The active highlight is one shape that slides between entries; its slot is reserved under them.
            let highlight = ui.painter().add(egui::Shape::Noop);
            let mut active_rect = None;
            for (view, label) in NAV.into_iter().filter(|(v, _)| shown(*v)) {
                let count = d.and_then(|d| match view {
                    View::Now => Some(d.now.len()),
                    View::Review => Some(d.review.len()),
                    View::Refined => Some(d.refined.len()),
                    View::Hours => Some(d.tasks.len()),
                    _ => None,
                });
                let count = count.or(match view {
                    View::Tasks => Some(self.tasks_ui.open_count()).filter(|n| *n > 0),
                    View::Prs => self.prs.with(|p| p.data.as_ref().map(|x| x.active.len())),
                    View::Chat => Some(self.chats.with(|cs| cs.iter().filter(|c| c.busy).count())).filter(|n| *n > 0),
                    _ => None,
                });
                let pr_alert = view == View::Prs && self.prs.with(|p| p.data.as_ref().is_some_and(|x| x.active.iter().any(|pr| !pr.open_threads.is_empty())));
                let alert = d.is_some_and(|d| match view {
                    View::Now => d.now.iter().any(|i| i.back.is_some()),
                    View::Hours => d.tasks.iter().any(|t| !views::task_flags(t).is_empty()),
                    _ => false,
                });
                let r = nav_item(ui, label, self.view == view, count, alert || pr_alert, acc);
                if self.view == view {
                    active_rect = Some(r.rect);
                }
                if r.clicked() {
                    self.view = view;
                }
            }
            let shown = motion::tween(ui.ctx(), egui::Id::unique("nav-highlight-alpha"), if active_rect.is_some() { 1.0 } else { 0.0 }, 0.2, 0.0);
            if let Some(r) = active_rect.or_else(|| ui.ctx().data(|d| d.get_temp::<Rect>(egui::Id::unique("nav-highlight-last")))) {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("nav-highlight-last"), r));
                let y = motion::tween(ui.ctx(), egui::Id::unique("nav-highlight-y"), r.top(), 0.3, r.top());
                let rect = Rect::from_min_size(pos2(r.left(), y), r.size());
                ui.painter().set(
                    highlight,
                    egui::Shape::Rect(egui::epaint::RectShape::new(rect, CornerRadius::same(4), theme::alpha(acc.main, 0.13 * shown), Stroke::new(1.0, theme::alpha(acc.main, 0.3 * shown)), StrokeKind::Inside)),
                );
            }

            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.horizontal(|ui| {
                    if dev && ui_kit::button(ui, "Atualizar", false, acc).clicked() {
                        board.refresh();
                    }
                    if ui_kit::button(ui, "Definições", false, acc).clicked() {
                        self.view = View::Settings;
                    }
                });
                if !dev {
                    if let Some(note) = &self.hotkey_note {
                        ui.label(RichText::new(note).small().color(theme::AMBER));
                    }
                    return;
                }
                let status = if b.loading {
                    "A atualizar…".to_owned()
                } else {
                    match b.fetched_at {
                        Some(at) => {
                            let m = at.elapsed().as_secs() / 60;
                            if m == 0 { "Atualizado agora mesmo".into() } else { format!("Atualizado há {m} min") }
                        }
                        None => "Sem dados".into(),
                    }
                };
                ui.label(RichText::new(status).monospace().size(11.0).color(theme::FAINT));
                if let Some(note) = &self.hotkey_note {
                    ui.label(RichText::new(note).small().color(theme::AMBER));
                }
            });
        });
        if talk {
            self.voice.toggle();
        }
    }

    fn content(&mut self, ui: &mut Ui, acc: theme::Accent) {
        let (title, sub) = match self.view {
            View::Today => ("Hoje", ""),
            View::Agenda => ("Agenda", "A tua semana do Outlook, como no Teams. Clica numa reunião para os detalhes e para entrar no Teams."),
            View::Tasks => ("Tarefas", "A tua lista pessoal e o que vem do Notion."),
            View::Now => ("Agora", "Os teus In Progress primeiro, depois os Planned. Os que o QA devolveu aparecem com o comentário."),
            View::Prs => ("PRs", "Os teus pull requests, os votos e os comentários do review. Corrigir abre uma conversa com o comentário."),
            View::Review => ("Revisão e QA", "Os teus itens em Code Review, To Test e Testing (deste sprint ou mexidos nas últimas duas semanas)."),
            View::Refined => ("Refined livre", "Itens refinados sem ninguém atribuído, pela ordem do backlog."),
            View::Chat if !crate::profile::dev() => ("Chat", "Escreve ou fala com o Jarvis: escreve, resume, analisa e propõe. Nada é enviado nem apagado sem o teu sim."),
            View::Chat => ("Chat", "A análise é feita aqui, por escrito ou por voz. A implementação segue para o Claude Code ou o Antigravity."),
            View::Hours => ("Horas do sprint", "As tuas tarefas deste sprint: estimativa, Work Done e o que falta preencher."),
            View::Cleanup => ("Limpeza", "Worktrees dos itens e anexos descarregados. Só limpa o que já não tem trabalho por enviar."),
            View::Settings => ("Definições", "Cor, tamanho, avisos, quem responde e voz. O Notion está em Tarefas."),
        };
        if self.view != View::Today {
            ui.label(RichText::new(title.to_uppercase()).heading().color(theme::BRIGHT).extra_letter_spacing(4.0));
            ui.label(RichText::new(sub).color(theme::MUTED));
            paint_ticks(ui, acc);
            ui.add_space(16.0);
        }

        if self.view == View::Chat {
            let chats = self.chats.clone();
            let acts = chats.with(|cs| chat_view::page(ui, cs, &mut self.chat_ui, acc));
            let ctx = ui.ctx().clone();
            self.apply(&ctx, acts);
            return;
        }
        let board = self.board.clone();
        let mut go = None;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Cap at 1600 like the CSS max-width; set_max_width alone would also widen a narrower window.
            ui.set_max_width(ui.available_width().min(1600.0));
            match self.view {
                View::Agenda => return agenda::view(ui, &self.agenda, &mut self.agenda_ui, acc),
                View::Tasks => return tasks::view(ui, &mut self.tasks_ui, acc),
                View::Prs => return prs::view(ui, &self.prs, acc),
                View::Cleanup => return cleanup::view(ui, &mut self.cleanup_ui, acc),
                View::Settings => {
                    if settings_view::view(ui, acc, self.voice_ready()) {
                        let s = settings::load();
                        self.acc = theme::accent(&s.accent);
                        theme::install_style(ui.ctx(), self.acc);
                        ui.ctx().set_zoom_factor(settings_view::zoom(&s));
                    }
                    return;
                }
                // Without the dev mode, Hoje does not wait for the board.
                View::Today if !crate::profile::dev() => {
                    let extras = self.dashboard_extras();
                    let since = (motion::now(ui.ctx()) - self.view_at) as f32;
                    dashboard::view(ui, None, &extras, acc, &mut go, since);
                    ui.add_space(24.0);
                    return;
                }
                _ => {}
            }
            board.with(|b| {
                if let Some(e) = &b.error {
                    ui_kit::hint(ui, e, true, acc);
                    ui.add_space(10.0);
                }
                let Some(d) = &b.data else {
                    if b.error.is_none() {
                        ui_kit::quiet(ui, "A ler o Azure DevOps…", acc);
                    }
                    return;
                };
                match self.view {
                    View::Today => {
                        let extras = self.dashboard_extras();
                        let since = (motion::now(ui.ctx()) - self.view_at) as f32;
                        dashboard::view(ui, Some(d), &extras, acc, &mut go, since);
                    }
                    View::Now => views::now(ui, d, acc),
                    View::Review => views::review(ui, d, acc),
                    View::Refined => views::refined(ui, d, acc),
                    View::Hours => views::hours(ui, d, acc, &mut self.hours_ui, &board),
                    _ => ui_kit::quiet(ui, "Esta secção ainda não existe.", acc),
                }
            });
            ui.add_space(24.0);
        });
        if let Some(v) = go {
            self.view = v;
        }
    }
}

impl JarvisApp {
    /// Everything the start screen shows besides the board.
    fn dashboard_extras(&mut self) -> dashboard::Extras {
        let prs = self.prs.with(|p| p.data.clone()).unwrap_or_default();
        let busy = self.chats.with(|cs| {
            cs.iter()
                .filter(|c| c.busy)
                .map(|c| {
                    let start = c.messages.iter().rposition(|m| m.role == "user").unwrap_or(0);
                    let steps: usize = c.messages[start..].iter().map(|m| m.tools.len()).sum();
                    let secs = c.messages.get(start).and_then(|m| ui_kit::parse(&m.at)).map(|t| (chrono::Local::now() - t).num_seconds().max(0)).unwrap_or(0);
                    let engine = if c.engine.as_deref() == Some("antigravity") { "Antigravity" } else { "Claude Code" };
                    (c.id.clone(), c.title.clone(), engine.to_owned(), secs, steps)
                })
                .collect()
        });
        let mut feed: Vec<(String, String)> = self.chats.with(|cs| {
            cs.iter()
                .flat_map(|c| c.messages.iter().filter(|m| m.role == "inform"))
                .map(|m| {
                    let line = m.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                    (m.at.clone(), line.replace("**", "").trim_start_matches("> ").to_owned())
                })
                .collect()
        });
        feed.sort_by(|a, b| b.0.cmp(&a.0));
        feed.truncate(8);
        dashboard::Extras {
            dev: crate::profile::dev(),
            meetings: agenda::today_all(&self.agenda),
            todos: tasks::open_list(),
            prs: prs.active.iter().filter(|p| !p.open_threads.is_empty()).map(|p| (p.title.clone(), p.id, p.open_threads.len(), p.url.clone())).collect(),
            prs_open: prs.active.len(),
            busy,
            feed,
            sys: self.sysmon.snapshot(),
            music: self.music.now(),
            favorites: crate::music::favorites(),
            item_chats: self.chats.with(|cs| {
                let mut m = std::collections::HashMap::new();
                for c in cs.iter().filter(|c| !c.handed_off) {
                    if let Some(i) = c.item_id {
                        let talked = c.messages.iter().any(|m| m.role != "inform");
                        let e = m.entry(i).or_insert((false, false));
                        e.0 |= talked;
                        e.1 |= c.busy;
                    }
                }
                m
            }),
        }
    }

    fn voice_ready(&self) -> (bool, bool) {
        let v = settings::jarvis_dir().join("vendor");
        let whisper = v.join("models").join("ggml-small.bin").exists()
            && (v.join("whisper").join("Release").join("whisper-cli.exe").exists() || v.join("whisper").join("whisper-cli.exe").exists());
        let piper = v.join("piper").join("piper.exe").exists() && v.join("voices").join("pt_PT-tugao-medium.onnx").exists();
        (whisper, piper)
    }

    /// A reply finished: a toast when Jarvis is not in front (and it was not a spoken turn, which is heard).
    fn turn_ended(&mut self, ctx: &egui::Context, id: &str) {
        self.board.refresh();
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if focused || self.voice.with(|v| v.on && v.chat_id.as_deref() == Some(id)) {
            return;
        }
        let Some((title, last, error)) = self.chats.with(|cs| {
            cs.iter().find(|c| c.id == id).map(|c| {
                let m = c.messages.iter().rev().find(|m| m.role == "assistant" || m.role == "error");
                (c.title.clone(), m.map(|m| m.text.clone()).unwrap_or_default(), m.is_some_and(|m| m.role == "error"))
            })
        }) else {
            return;
        };
        let text: String = last.chars().filter(|c| !"#*`|>".contains(*c)).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
        let head = if error { format!("Erro em \"{title}\"") } else { format!("Jarvis respondeu: {title}") };
        notify::notify(&head, if text.is_empty() { "Resposta pronta." } else { &text }, AppEvent::OpenChat(id.to_owned()));
    }

    /// Work that runs every frame, cheaply: news from the board and the PRs, meeting reminders, cleaned items.
    fn background(&mut self, _ctx: &egui::Context) {
        let fetched = self.board.with(|b| b.fetched_at);
        if fetched.is_some() && fetched != self.last_board {
            self.last_board = fetched;
            let news = self.board.with(|b| b.data.as_ref().map(events::board).unwrap_or_default());
            self.apply_news(news);
            let tip = self.board.with(|b| match (&b.data, &b.error) {
                (Some(d), _) => {
                    let back = d.now.iter().filter(|i| i.back.is_some()).count();
                    format!("Jarvis — {} para fazer{}", d.now.len(), if back > 0 { format!(", {back} devolvido{} pelo QA", if back > 1 { "s" } else { "" }) } else { String::new() })
                }
                _ => "Jarvis — sem ligação ao Azure DevOps".to_owned(),
            });
            if let Some(t) = &self._tray {
                let _ = t.set_tooltip(Some(tip));
            }
        }
        let (version, news) = self.prs.with(|p| {
            if p.version == self.last_prs {
                return (p.version, Vec::new());
            }
            let me = self.board.with(|b| b.data.as_ref().map(|d| d.me.name.clone())).unwrap_or_default();
            (p.version, p.data.as_ref().map(|d| events::prs(d, &me)).unwrap_or_default())
        });
        self.last_prs = version;
        self.apply_news(news);
        if self.last_tick.elapsed().as_secs() >= 30 {
            self.last_tick = Instant::now();
            agenda::remind(&self.agenda, &mut self.reminded);
        }
        for id in self.cleanup_ui.take_cleaned_items() {
            self.chats.remove_for_item(id);
        }
    }

    fn apply_news(&mut self, news: Vec<events::News>) {
        if news.is_empty() {
            return;
        }
        let s = settings::load();
        let mut chat_ids = Vec::new();
        if s.inform_chat {
            for n in &news {
                chat_ids.push(Some(self.chats.inform(n.item_id, n.pr_key.as_deref(), &n.chat_title, &n.chat)));
            }
            if let Some(Some(first)) = chat_ids.first() {
                let first = first.clone();
                self.open_window(&first);
            }
        } else {
            chat_ids = news.iter().map(|_| None).collect();
        }
        if news.len() > 3 {
            notify::notify(&format!("Jarvis: {} novidades", news.len()), &news.iter().map(|n| n.title.clone()).collect::<Vec<_>>().join(" · "), AppEvent::Show);
        } else {
            for (n, id) in news.iter().zip(chat_ids) {
                notify::notify(&n.title, &n.body, id.map(AppEvent::OpenChat).unwrap_or(AppEvent::Show));
            }
        }
    }

    /// "Corrigir" on a PR: its conversation, opened on the review comment(s), in a conversation window.
    fn fix_pr(&mut self, key: &str, thread: Option<i64>) {
        let Some(pr) = self.prs.with(|p| p.data.as_ref().and_then(|d| d.active.iter().find(|x| x.key == key).cloned())) else { return };
        let id = match self.chats.for_pr(&pr.key) {
            Some(id) if self.chats.is_busy(&id) => {
                self.open_window(&id);
                return;
            }
            Some(id) => id,
            None => self.chats.create(Some(format!("PR !{} {}", pr.id, pr.title)), None, Some(pr.key.clone())),
        };
        self.chats.send(&id, &prs::fix_prompt(&pr, thread), false, chat::engine());
        self.open_window(&id);
    }

    /// Below the board on Hoje: today's meetings, open personal tasks and PRs waiting on me.
    fn today_extras(&mut self, ui: &mut Ui, acc: theme::Accent, go: &mut Option<View>) {
        ui.add_space(12.0);
        let meetings = agenda::today_left(&self.agenda);
        let todos: Vec<tasks::Todo> = {
            let _ = self.tasks_ui.open_count();
            tasks::open_list()
        };
        let pr_waiting: Vec<(String, i64, usize)> =
            self.prs.with(|p| p.data.as_ref().map(|d| d.active.iter().filter(|x| !x.open_threads.is_empty()).map(|x| (x.title.clone(), x.id, x.open_threads.len())).collect()).unwrap_or_default());
        ui.columns(3, |cols| {
            ui_kit::section_label(&mut cols[0], "Agenda de hoje", acc.main, acc);
            if meetings.is_empty() {
                ui_kit::quiet(&mut cols[0], "Sem mais reuniões hoje.", acc);
            }
            for e in meetings.iter().take(5) {
                let r = ui_kit::hud_panel(&mut cols[0], acc, false, |ui| {
                    ui.label(RichText::new(e.start_at().map(|s| s.format("%H:%M").to_string()).unwrap_or_default()).monospace().color(acc.light));
                    ui.add(egui::Label::new(RichText::new(&e.subject).strong().color(theme::BRIGHT)).truncate());
                    if let Some(u) = e.teams_url() {
                        if ui_kit::button(ui, "Entrar no Teams", false, acc).clicked() {
                            brain::open(u);
                        }
                    }
                });
                let _ = r;
                cols[0].add_space(6.0);
            }
            ui_kit::section_label(&mut cols[1], "Tarefas", acc.main, acc);
            if todos.is_empty() {
                ui_kit::quiet(&mut cols[1], "Sem tarefas pessoais abertas.", acc);
            }
            for t in todos.iter().take(6) {
                cols[1].horizontal(|ui| {
                    ui_kit::diamond(ui, false, acc.main, 4.0);
                    ui.add(egui::Label::new(RichText::new(&t.title).color(theme::TEXT)).truncate());
                });
            }
            if !todos.is_empty() && ui_kit::button(&mut cols[1], "Ver tarefas", false, acc).clicked() {
                *go = Some(View::Tasks);
            }
            ui_kit::section_label(&mut cols[2], "PRs com comentários", acc.main, acc);
            if pr_waiting.is_empty() {
                ui_kit::quiet(&mut cols[2], "Nenhum PR à tua espera.", acc);
            }
            for (title, id, n) in &pr_waiting {
                cols[2].horizontal(|ui| {
                    ui.label(RichText::new(format!("!{id}")).monospace().color(theme::AMBER));
                    ui.add(egui::Label::new(RichText::new(format!("{title} · {n} por resolver")).color(theme::TEXT)).truncate());
                });
            }
            if !pr_waiting.is_empty() && ui_kit::button(&mut cols[2], "Ver PRs", false, acc).clicked() {
                *go = Some(View::Prs);
            }
        });
    }

    /// The voice panel: a small window always on top, bottom right, while a spoken conversation is on.
    fn voice_panel(&mut self, ctx: &egui::Context, acc: theme::Accent, t: f32) {
        if !self.voice.with(|v| v.visible) {
            return;
        }
        let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1920.0, 1080.0));
        let size = vec2(440.0, 420.0);
        let builder = egui::ViewportBuilder::default()
            .with_title("Jarvis · voz")
            .with_inner_size(size)
            .with_position(pos2(monitor.x - size.x - 16.0, monitor.y - size.y - 64.0))
            .with_always_on_top()
            .with_resizable(false);
        let voice = self.voice.clone();
        let mut open_chat = None;
        let draft_id = egui::Id::unique("voice-draft");
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("voice-panel"), builder, |ui, _| {
            if ui.ctx().input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
                voice.hide();
            }
            egui::CentralPanel::default().frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(16))).show(ui, |ui| {
                voice.with(|v| {
                    ui.horizontal(|ui| {
                        let grow = 1.0 + v.level;
                        let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                        paint_reactor(ui, Rect::from_center_size(rect.center(), vec2(52.0 * grow, 52.0 * grow)), acc, t, if v.phase == "thinking" { 1.0 } else { v.level * 1.5 }, 1.0);
                        ui.vertical(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new("JARVIS").font(ui_kit::display(18.0)).color(theme::BRIGHT).extra_letter_spacing(3.0));
                            let c = match v.phase {
                                "listening" => theme::GREEN,
                                "speaking" => acc.light,
                                "thinking" => theme::AMBER,
                                _ => theme::MUTED,
                            };
                            ui.label(RichText::new(&v.status).color(c));
                        });
                    });
                    ui.add_space(8.0);
                    ScrollArea::vertical().max_height(250.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                        for l in &v.log {
                            match l.who {
                                "me" => {
                                    ui.label(RichText::new(&l.text).color(theme::BRIGHT).italics());
                                }
                                "jarvis" => {
                                    ui.label(RichText::new(&l.text).color(acc.light));
                                }
                                _ => {
                                    ui.label(RichText::new(&l.text).small().color(theme::FAINT));
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
                ui.add_space(6.0);
                let mut draft: String = ui.ctx().data_mut(|d| d.get_temp(draft_id).unwrap_or_default());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut draft).hint_text("Ou escreve aqui…").desired_width(ui.available_width() - 4.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !draft.trim().is_empty() {
                        voice.write(draft.trim());
                        draft.clear();
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(draft_id, draft));
                ui.horizontal(|ui| {
                    let on = voice.with(|v| v.on);
                    if ui_kit::button(ui, if on { "Terminar" } else { "Falar outra vez" }, !on, acc).clicked() {
                        if on { voice.end() } else { voice.toggle() }
                    }
                    if let Some(id) = voice.chat_id() {
                        if ui_kit::button(ui, "Abrir conversa", false, acc).clicked() {
                            open_chat = Some(id);
                        }
                    }
                    if ui_kit::button(ui, "Fechar", false, acc).clicked() {
                        voice.hide();
                    }
                });
            });
        });
        if let Some(id) = open_chat {
            self.open_window(&id);
        }
    }

    fn open_window(&mut self, id: &str) {
        if !self.chat_ui.windows.iter().any(|w| w == id) {
            self.chat_ui.windows.push(id.to_owned());
        }
        self.chat_ui.focus.push(id.to_owned());
    }

    /// Each open conversation in its own OS window (egui immediate viewport).
    fn chat_windows(&mut self, ctx: &egui::Context, acc: theme::Accent) {
        let chats = self.chats.clone();
        let mut closed = Vec::new();
        let mut acts = Vec::new();
        for wid in self.chat_ui.windows.clone() {
            let Some(title) = chats.with(|cs| cs.iter().find(|c| c.id == wid).map(|c| c.title.clone())) else {
                closed.push(wid);
                continue;
            };
            let builder = egui::ViewportBuilder::default().with_title(format!("Jarvis · {title}")).with_inner_size([780.0, 860.0]).with_min_inner_size([420.0, 420.0]);
            let vid = egui::ViewportId::from_hash_of(("chat-window", &wid));
            if self.chat_ui.focus.contains(&wid) {
                ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Focus);
            }
            let chat_ui = &mut self.chat_ui;
            ctx.show_viewport_immediate(vid, builder, |ui, _| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    closed.push(wid.clone());
                }
                egui::CentralPanel::default().frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(18))).show(ui, |ui| {
                    chats.with(|cs| {
                        if let Some(c) = cs.iter().find(|c| c.id == wid) {
                            acts.extend(chat_view::conversation(ui, c, chat_ui, acc, true));
                        }
                    });
                });
            });
        }
        self.chat_ui.windows.retain(|w| !closed.contains(w));
        self.chat_ui.focus.clear();
        self.apply(ctx, acts);
    }

    fn apply(&mut self, ctx: &egui::Context, acts: Vec<Action>) {
        for a in acts {
            match a {
                Action::New => self.chat_ui.selected = Some(self.chats.create(None, None, None)),
                Action::Select(id) => self.chat_ui.selected = Some(id),
                Action::Send(id, text) => {
                    self.chats.send(&id, &text, false, chat::engine());
                }
                Action::Stop(id) => self.chats.stop(&id),
                Action::Delete(id) => {
                    self.chats.remove(&id);
                    self.chat_ui.windows.retain(|w| w != &id);
                }
                Action::OpenInClaude(id) => match self.chats.open_in_claude(&id) {
                    Some(url) => brain::open(&url),
                    None => self.chats.add_note(&id, "Esta conversa ainda não tem sessão do Claude.", "error"),
                },
                Action::PopOut(id) => self.open_window(&id),
                Action::Implement(id, target) => self.implement(ctx, &id, target),
                Action::SetEngine(id, engine, model) => self.chats.set_engine(&id, engine, model),
                Action::SetMode(id, mode) => self.chats.set_mode(&id, mode),
                Action::Forget(id, msg) => self.chats.undo_remember(&id, &msg),
                Action::Compact(id) => {
                    self.chats.compact(&id, chat::engine());
                }
            }
        }
    }

    /// "Começar": a conversation that already started just opens; a new analysis first asks which engine and model
    /// to use (`start_dialog`), then follows the normal flow (`start_confirmed`).
    fn start_item(&mut self, id: i64) {
        let existing = self.chats.with(|cs| cs.iter().find(|c| c.item_id == Some(id) && !c.handed_off && c.messages.iter().any(|m| m.role != "inform")).map(|c| c.id.clone()));
        if let Some(c) = existing {
            if let Some(b) = self.item_branch(id) {
                self.chats.set_branch(&c, b);
            }
            self.chat_ui.selected = Some(c.clone());
            self.open_window(&c);
            return;
        }
        let title = self.board.with(|b| {
            b.data.as_ref().and_then(|d| d.now.iter().chain(&d.review).chain(&d.refined).find(|i| i.id == id).map(|i| i.title.clone())).unwrap_or_default()
        });
        let defaults = chat::engine();
        let agy = defaults.model == "gemini" && brain::gemini_installed();
        let model = if agy { defaults.agy_model } else { defaults.claude_model };
        self.start_choice = Some(StartChoice { id, title, agy, model: if model.is_empty() { "auto".into() } else { model }, remember: false });
    }

    /// The question before an analysis starts: Claude Code or Antigravity, and which model.
    fn start_dialog(&mut self, ctx: &egui::Context, acc: theme::Accent) {
        let Some(choice) = self.start_choice.as_mut() else { return };
        let mut go = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("start-choice"))
            .backdrop_color(theme::alpha(theme::BG, 0.75))
            .frame(Frame::new().fill(theme::GLASS).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.3))).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(24)))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.horizontal(|ui| {
                    ui_kit::id_link(ui, choice.id, &azdo::item_url(choice.id), acc);
                    ui.label(RichText::new("Com quem queres analisar?").font(ui_kit::display(theme::size::SUBTITLE)).color(theme::BRIGHT));
                });
                if !choice.title.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&choice.title).color(theme::MUTED)).truncate());
                }
                ui.add_space(14.0);
                let agy_ok = brain::gemini_installed();
                ui.columns(2, |cols| {
                    for (col, agy, name, text) in [
                        (0, false, "Claude Code", "Lê o código e o item, propõe o plano, cria as tarefas e o worktree quando aprovares. Gasta a tua utilização do Claude."),
                        (1, true, "Antigravity", "Gemini, Claude ou GPT pela tua conta Google. Lê o código, propõe o plano e, com o teu sim, cria as tarefas e o worktree. Não gasta a utilização do Claude."),
                    ] {
                        let ui = &mut cols[col];
                        let selected = choice.agy == agy;
                        let enabled = !agy || agy_ok;
                        let r = Frame::new()
                            .fill(if selected { theme::alpha(acc.main, 0.14) } else { theme::alpha(acc.main, 0.03) })
                            .stroke(Stroke::new(if selected { 1.5 } else { 1.0 }, theme::alpha(acc.main, if selected { 0.8 } else { 0.2 })))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(Margin::same(14))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_min_height(118.0);
                                ui.horizontal(|ui| {
                                    ui_kit::diamond(ui, selected, if selected { acc.main } else { theme::MUTED }, 5.0);
                                    ui.label(RichText::new(name.to_uppercase()).font(ui_kit::display(15.0)).color(if selected { theme::BRIGHT } else { theme::TEXT }).extra_letter_spacing(1.4));
                                });
                                ui.add_space(4.0);
                                ui.label(RichText::new(if enabled { text } else { "Indisponível: falta o Antigravity CLI (agy)." }).size(13.0).color(theme::MUTED));
                            })
                            .response
                            .interact(egui::Sense::click());
                        if r.clicked() && enabled && !selected {
                            choice.agy = agy;
                            choice.model = "auto".into();
                        }
                        if r.double_clicked() && enabled {
                            choice.agy = agy;
                            go = true;
                        }
                    }
                });
                ui.add_space(12.0);
                let models = chat_view::model_options(choice.agy);
                let label = models.iter().find(|(id, _)| *id == choice.model).map(|(_, l)| l.clone()).unwrap_or_else(|| choice.model.clone());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("MODELO").font(ui_kit::display(11.5)).color(theme::MUTED).extra_letter_spacing(1.4));
                    egui::ComboBox::from_id_salt("start-model").selected_text(label).width(300.0).show_ui(ui, |ui| {
                        for (id, l) in &models {
                            ui.selectable_value(&mut choice.model, id.clone(), l);
                        }
                    });
                });
                ui.add_space(6.0);
                ui.checkbox(&mut choice.remember, "Usar sempre esta escolha nas conversas novas");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui_kit::button(ui, "Começar análise", true, acc).clicked() {
                        go = true;
                    }
                    if ui_kit::button(ui, "Cancelar", false, acc).clicked() {
                        cancel = true;
                    }
                    ui.label(RichText::new("Enter começa · Esc cancela").small().color(theme::FAINT));
                });
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
            });
        if modal.should_close() {
            cancel = true;
        }
        if go {
            let c = self.start_choice.take().unwrap();
            if c.remember {
                settings::update(|s| {
                    s.model = if c.agy { "gemini".into() } else { "auto".into() };
                    if c.agy {
                        s.agy_model = c.model.clone();
                    } else {
                        s.claude_model = c.model.clone();
                    }
                });
            }
            self.start_confirmed(c.id, if c.agy { "antigravity" } else { "claude" }, &c.model);
        } else if cancel {
            self.start_choice = None;
        }
    }

    /// The normal flow once the engine is chosen: the item's conversation in its own window, the analysis prompt,
    /// and the item claimed (assigned to me, current sprint, Planned).
    /// The item's branch by the team convention in the pack (e.g. fix/S{sprint}/{id} for a Bug), from the board data.
    fn item_branch(&self, id: i64) -> Option<String> {
        self.board.with(|b| {
            let d = b.data.as_ref()?;
            let it = d.now.iter().chain(&d.review).chain(&d.refined).find(|i| i.id == id)?;
            crate::profile::branch_for(&it.kind, &d.sprint.name, id)
        })
    }

    fn start_confirmed(&mut self, id: i64, engine: &str, model: &str) {
        let (title, prompt) = self.board.with(|b| {
            let d = b.data.as_ref();
            let all = d.map(|d| d.now.iter().chain(&d.review).chain(&d.refined).collect::<Vec<_>>()).unwrap_or_default();
            let it = all.iter().find(|i| i.id == id);
            let working = d.is_some_and(|d| d.now.iter().chain(&d.review).any(|x| x.id == id));
            let title = it.map(|i| format!("#{id} {}", i.title)).unwrap_or_else(|| format!("#{id}"));
            // The pack's board skill drives the flow when there is one; otherwise the request goes in plain words.
            let skill = crate::profile::skill_command().map(|c| format!("{c} ")).unwrap_or_default();
            let prompt = match it.and_then(|i| i.back.as_ref()) {
                Some(b) => format!("{skill}o #{id} voltou do QA ({} → {}). Lê os comentários desde que voltou, vê as imagens, encontra a causa e propõe a correção numa tarefa nova.", b.from, b.to),
                None if working => format!("{skill}continua o #{id}: vê o estado do item, das tarefas e do worktree e diz-me o que falta fazer."),
                None if skill.is_empty() => format!("Analisa o item #{id}: lê-o, encontra no código o que é preciso e propõe um plano."),
                None => format!("{skill}pega no {id}"),
            };
            (title, prompt)
        });
        let existing = self.chats.with(|cs| cs.iter().find(|c| c.item_id == Some(id) && !c.handed_off).map(|c| c.id.clone()));
        let chat_id = existing.unwrap_or_else(|| self.chats.create(Some(title), Some(id), None));
        self.chats.set_engine(&chat_id, Some(engine.to_owned()), Some(model.to_owned()).filter(|m| m != "auto"));
        if let Some(b) = self.item_branch(id) {
            self.chats.set_branch(&chat_id, b);
        }
        self.chats.send(&chat_id, &prompt, false, chat::engine());
        self.chat_ui.selected = Some(chat_id.clone());
        self.open_window(&chat_id);
        claim_in_background(&self.chats, &self.board, &chat_id, id);
    }

    /// Implementation leaves Jarvis: the Claude app resumes the analysis session; Antigravity (or any other AI) gets
    /// the worktree with .jarvis/HANDOFF.md, and the request goes to the clipboard.
    fn implement(&mut self, ctx: &egui::Context, id: &str, target: &str) {
        let Some((title, worktree)) = self.chats.with(|cs| cs.iter().find(|c| c.id == id).map(|c| (c.title.clone(), c.worktree.clone()))) else { return };
        let Some(w) = worktree else {
            self.chats.add_note(id, "Esta conversa ainda não tem worktree preparado.", "error");
            return;
        };
        let dir = std::path::Path::new(&w);
        if !dir.exists() {
            self.chats.add_note(id, &format!("Não encontrei a pasta {w}."), "error");
            return;
        }
        brain::exclude_jarvis_dir(&w);
        let _ = std::fs::create_dir_all(dir.join(".jarvis"));
        // No package from the analysis (e.g. it ran somewhere that could not write it): Jarvis writes one from the
        // conversation, so the implementation never starts blind.
        let handoff = dir.join(".jarvis").join("HANDOFF.md");
        if !handoff.exists() {
            let body = self.chats.with(|cs| {
                cs.iter().find(|c| c.id == id).map(|c| {
                    let answers: Vec<&str> = c.messages.iter().rev().filter(|m| m.role == "assistant").take(3).map(|m| m.text.as_str()).collect();
                    let item = c.item_id.map(|i| format!("\n\nItem: [#{i}]({})", azdo::item_url(i))).unwrap_or_default();
                    format!(
                        "# {}{item}\n\n_Escrito pelo Jarvis a partir da conversa de análise (a análise não deixou pacote de passagem). Lê também o item no Azure DevOps._\n\n{}",
                        c.title,
                        answers.into_iter().rev().collect::<Vec<_>>().join("\n\n---\n\n")
                    )
                })
            });
            if let Some(b) = body {
                let _ = std::fs::write(&handoff, b);
            }
        }
        let pack = handoff.exists();
        // The browser testing guide travels with every hand-off, so any AI tests the same way.
        let tested = crate::profile::guide(crate::profile::Guide::Test).is_some_and(|g| std::fs::write(dir.join(".jarvis").join("TESTAR.md"), crate::profile::render(&std::fs::read_to_string(g).unwrap_or_default())).is_ok());
        let prompt = format!(
            "Implementa o {title}. {}{}Trabalha só em {w}, segue o CLAUDE.md e as regras do repositório, e não faças commit nem push sem eu pedir.",
            if pack { "Lê primeiro o ficheiro .jarvis/HANDOFF.md deste workspace e as imagens em .jarvis/img/: tem a análise, o plano, como testar e as regras. " } else { "" },
            if tested {
                "Quando acabares, testa no browser seguindo .jarvis/TESTAR.md (porta própria, login feito por mim, consola e rede sem erros, capturas e resultado em .jarvis/test/) e diz-me o que passou e o que não conseguiste testar. "
            } else {
                ""
            }
        );
        ctx.copy_text(prompt.clone());
        let mut flash = |text: &str| {
            self.chat_ui.flash.insert(id.to_owned(), (text.to_owned(), Instant::now()));
        };
        match target {
            // With a hand-off package, the implementation starts as a NEW Claude Code session in the worktree, on the
            // implementation model (fixed in the worktree's .claude/settings.local.json, which git ignores): it reads
            // HANDOFF.md instead of re-reading the whole analysis. Without a package, the analysis session is resumed.
            "claude" if pack => {
                let model = settings::load().implement_model;
                let model = if model.is_empty() { "sonnet".to_owned() } else { model };
                set_worktree_model(dir, &model);
                let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
                // The same link the Claude app builds itself (folder=), plus q= to pre-fill the request; capped like its own check.
                let q: String = prompt.chars().take(14000).collect();
                brain::open(&format!("claude://code/new?folder={}&q={}", enc(&w), enc(&q)));
                self.chats.mark_implemented(id, "claude", &format!("Abri uma sessão nova do Claude Code no worktree, com o {model}: começa pelo HANDOFF.md. O pedido também ficou copiado, se não aparecer escrito."));
                flash("Sessão nova no Claude Code. Pedido copiado.");
            }
            "claude" => match self.chats.open_in_claude(id) {
                Some(url) => {
                    brain::open(&url);
                    flash("Aberto no Claude Code. Pedido copiado.");
                }
                None => self.chats.add_note(id, "Esta conversa ainda não tem sessão do Claude.", "error"),
            },
            "antigravity" => {
                let exe = dirs::data_local_dir().unwrap_or_default().join("Programs").join("antigravity").join("Antigravity.exe");
                if !exe.exists() {
                    self.chats.add_note(id, "Não encontrei o Antigravity instalado.", "error");
                    return;
                }
                let _ = std::process::Command::new(exe).arg(&w).spawn();
                self.chats.mark_implemented(id, "antigravity", &format!("Abri o Antigravity em {w} e copiei o pedido: cola-o no agente."));
                flash("Antigravity aberto. Pedido copiado.");
            }
            _ => {
                let folder = if pack { dir.join(".jarvis") } else { dir.to_path_buf() };
                brain::open(&folder.to_string_lossy());
                self.chats.mark_implemented(id, "other", "Abri a pasta do pacote e copiei o pedido para dares a outra IA.");
                flash("Pasta aberta. Pedido copiado.");
            }
        }
    }
}

/// Opening an item's conversation claims it (assigned to me, current sprint, Planned), off the UI thread.
pub fn claim_in_background(chats: &Chats, board: &Board, chat_id: &str, id: i64) {
    let Some((me, sprint)) = board.with(|b| b.data.as_ref().map(|d| (d.me.clone(), d.sprint.clone()))) else { return };
    let (chats, board, chat_id) = (chats.clone(), board.clone(), chat_id.to_owned());
    std::thread::spawn(move || match azdo::claim(id, &me, &sprint) {
        Ok(text) if !text.is_empty() => {
            chats.add_note(&chat_id, &text, "note");
            board.refresh();
        }
        Ok(_) => {}
        Err(e) => chats.add_note(&chat_id, &format!("Não consegui atualizar o #{id} no Azure DevOps: {}", e.lines().next().unwrap_or_default()), "error"),
    });
}

fn sprint_box(ui: &mut Ui, d: &crate::azdo::Dashboard, acc: theme::Accent) {
    let start = ui_kit::parse(&d.sprint.start);
    let finish = ui_kit::parse(&d.sprint.finish);
    let now = chrono::Local::now();
    let (pct, left) = match (start, finish) {
        (Some(s), Some(f)) => {
            let total = (f - s).num_seconds() as f32 + 86400.0;
            (((now - s).num_seconds() as f32 / total).clamp(0.0, 1.0), (f.date_naive() - now.date_naive()).num_days().max(0))
        }
        _ => (0.0, 0),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(d.sprint.name.to_uppercase()).font(ui_kit::display(13.0)).color(acc.light).extra_letter_spacing(2.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(if left == 0 { "último dia".to_owned() } else { format!("{left} {}", if left == 1 { "dia" } else { "dias" }) }).monospace().size(12.0));
        });
    });
    // Segmented bar; it fills up to today when the sprint first shows, and the leading segment breathes.
    let shown = motion::tween(ui.ctx(), egui::Id::unique("sprint-fill"), pct, 1.1, 0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let p = ui.painter();
    let edge = rect.left() + rect.width() * shown;
    let pulse = if motion::reduced() { 1.0 } else { 0.65 + 0.35 * (motion::now(ui.ctx()) as f32 * 2.4).sin().abs() };
    let mut x = rect.left();
    while x < rect.right() {
        let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(6.0_f32.min(rect.right() - x), 6.0));
        let color = if x + 8.0 >= edge && x < edge {
            theme::alpha(acc.light, pulse)
        } else if x < edge {
            acc.main
        } else {
            theme::alpha(acc.main, 0.16)
        };
        p.rect_filled(seg, 0.0, color);
        x += 8.0;
    }
    ui.label(RichText::new(format!("{} – {}", ui_kit::short_date(&d.sprint.start), ui_kit::short_date(&d.sprint.finish))).monospace().size(11.0).color(theme::MUTED));
}

/// One sidebar entry. The active highlight is drawn by the sidebar (it slides between entries); here only the hover
/// wash, the diamond, the label and the badges.
fn nav_item(ui: &mut Ui, label: &str, active: bool, count: Option<usize>, alert: bool, acc: theme::Accent) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let p = ui.painter();
    let h = ui.ctx().animate_bool_with_time_and_easing(resp.id.with("nav-hover"), resp.hovered() && !active, 0.15, egui::emath::easing::cubic_out);
    if h > 0.0 {
        p.rect_filled(rect, CornerRadius::same(4), theme::alpha(acc.main, 0.06 * h));
    }
    let on = ui.ctx().animate_bool_with_time_and_easing(resp.id.with("nav-active"), active, 0.22, egui::emath::easing::cubic_out);
    let dot = pos2(rect.left() + 18.0 + 2.0 * h, rect.center().y);
    let d = 4.0 + 1.0 * on;
    p.add(egui::Shape::convex_polygon(
        vec![pos2(dot.x, dot.y - d), pos2(dot.x + d, dot.y), pos2(dot.x, dot.y + d), pos2(dot.x - d, dot.y)],
        theme::mix(theme::alpha(theme::MUTED, 0.6), acc.main, on),
        Stroke::NONE,
    ));
    p.text(pos2(rect.left() + 36.0 + 3.0 * h, rect.center().y), egui::Align2::LEFT_CENTER, label.to_uppercase(), ui_kit::display(13.0), theme::mix(theme::MUTED, theme::BRIGHT, on.max(h * 0.6)));
    let mut right = rect.right() - 10.0;
    if let Some(n) = count {
        let text = n.to_string();
        let w = 14.0 + 8.0 * text.len() as f32;
        let badge = Rect::from_min_size(pos2(right - w, rect.center().y - 10.0), vec2(w, 20.0));
        p.rect(badge, CornerRadius::same(2), Color32::TRANSPARENT, Stroke::new(1.0, theme::alpha(acc.main, 0.14 + 0.16 * on)), StrokeKind::Inside);
        p.text(badge.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(11.0), theme::mix(theme::TEXT, acc.light, on));
        right = badge.left() - 10.0;
    }
    if alert {
        let c = pos2(right - 4.0, rect.center().y);
        let k = if motion::reduced() { 4.0 } else { 3.6 + 0.8 * (motion::now(ui.ctx()) as f32 * 3.0).sin().abs() };
        p.add(egui::Shape::convex_polygon(vec![pos2(c.x, c.y - k), pos2(c.x + k, c.y), pos2(c.x, c.y + k), pos2(c.x - k, c.y)], theme::AMBER, Stroke::NONE));
    }
    resp
}

/// The blueprint grid, plus a faint scan line that sweeps down every few seconds (HUD life, nothing to read).
fn paint_grid(ui: &Ui, acc: theme::Accent) {
    let rect = ui.max_rect().expand2(vec2(34.0, 28.0));
    let p = ui.painter();
    let line = Stroke::new(1.0, theme::alpha(acc.main, 0.035));
    let mut x = rect.left() - rect.left().rem_euclid(32.0);
    while x < rect.right() {
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], line);
        x += 32.0;
    }
    let mut y = rect.top() - rect.top().rem_euclid(32.0);
    while y < rect.bottom() {
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], line);
        y += 32.0;
    }
    if motion::reduced() {
        return;
    }
    let period = 9.0;
    let k = (motion::now(ui.ctx()) % period) as f32 / 4.5;
    if k < 1.0 {
        let y = rect.top() + rect.height() * motion::ease_out(k);
        let fade = 1.0 - k;
        for (dy, a) in [(0.0, 0.10), (-2.0, 0.05), (-6.0, 0.025), (-14.0, 0.012)] {
            p.line_segment([pos2(rect.left(), y + dy), pos2(rect.right(), y + dy)], Stroke::new(1.0, theme::alpha(acc.main, a * fade)));
        }
    }
}

fn paint_ticks(ui: &mut Ui, acc: theme::Accent) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(1600.0), 14.0), Sense::hover());
    let p = ui.painter();
    let y = rect.bottom() - 1.0;
    // The rule draws itself in from the left when a section opens.
    let k = motion::tween(ui.ctx(), egui::Id::unique(("ticks", ui.ctx().data(|d| d.get_temp::<u64>(egui::Id::unique("view-seq")).unwrap_or(0)))), 1.0, 0.5, 0.0);
    let right = rect.left() + rect.width() * k;
    p.line_segment([pos2(rect.left(), y), pos2(right, y)], Stroke::new(1.0, theme::alpha(acc.main, 0.14)));
    let mut x = rect.left();
    while x <= right {
        p.line_segment([pos2(x, y - 5.0), pos2(x, y)], Stroke::new(1.0, theme::alpha(acc.main, 0.3)));
        x += 24.0;
    }
}

/// Concentric rotating rings with a glowing core. `phase` is the accumulated rotation (it speeds up while Jarvis
/// works), `energy` 0..1 brightens and pulses the core, `drawn` 0..1 draws the rings in (the start-up sequence).
pub fn paint_reactor(ui: &Ui, rect: Rect, acc: theme::Accent, phase: f32, energy: f32, drawn: f32) {
    let p = ui.painter();
    let c = rect.center();
    let r = rect.width() / 2.0;
    let tau = std::f32::consts::TAU;
    let arc = |radius: f32, start: f32, sweep: f32, width: f32, color: Color32| motion::arc(p, c, radius, start, sweep, Stroke::new(width, color));
    let ticks = (60.0 * drawn).round() as usize;
    for i in 0..ticks {
        let a = i as f32 / 60.0 * tau + phase * tau / 24.0;
        arc(r * 0.92, a, tau / 60.0 * 0.35, 1.6, theme::alpha(acc.main, 0.55));
    }
    for (start, len) in [(0.0, 26.0), (30.0, 6.0), (40.0, 26.0), (70.0, 6.0), (80.0, 26.0), (110.0, 6.0)] {
        let a = start / 120.0 * tau - phase * tau / 9.0;
        arc(r * 0.7, a, len / 120.0 * tau * drawn, 2.6, acc.main);
    }
    arc(r * 0.5, -tau / 4.0, tau * drawn, 1.2, theme::alpha(acc.light, 0.7));
    let pulse = 1.0 + energy * 0.18 * (phase * 1.7).sin();
    if energy > 0.01 {
        p.circle_filled(c, r * 0.46 * pulse, theme::alpha(acc.main, 0.10 * energy));
    }
    for (k, a) in [(0.34, 0.12), (0.27, 0.3), (0.2, 0.6), (0.12, 1.0)] {
        let a = (a * (0.85 + 0.3 * energy)).min(1.0) * drawn.powf(0.5);
        p.circle_filled(c, r * k * pulse, theme::alpha(if k < 0.15 { Color32::WHITE } else { acc.light }, a));
    }
}

/// "Começar" waiting for the user to pick the engine (and model) for a new analysis.
struct StartChoice {
    id: i64,
    title: String,
    agy: bool,
    model: String,
    remember: bool,
}

/// The way into voice: a microphone and "Falar com o Jarvis"; while a spoken conversation is on, it pulses green and
/// stops it.
fn voice_button(ui: &mut Ui, on: bool, acc: theme::Accent) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let h = motion::hover(ui, &resp);
    let p = ui.painter();
    let t = motion::now(ui.ctx()) as f32;
    let color = if on { theme::GREEN } else { acc.main };
    let pulse = if on && !motion::reduced() { 0.5 + 0.5 * (t * 3.0).sin().abs() } else { 0.0 };
    p.rect(
        rect,
        CornerRadius::same(4),
        theme::alpha(color, 0.08 + 0.08 * h + 0.06 * pulse),
        Stroke::new(1.0, theme::alpha(color, 0.35 + 0.4 * h + 0.4 * pulse)),
        StrokeKind::Inside,
    );
    // Microphone: capsule, cradle and stand.
    let c = pos2(rect.left() + 22.0, rect.center().y - 2.0);
    let cap = Rect::from_center_size(c - vec2(0.0, 3.0), vec2(8.0, 13.0));
    p.rect_filled(cap, CornerRadius::same(4), color);
    motion::arc(p, c - vec2(0.0, 2.0), 7.0, 0.0, std::f32::consts::PI, Stroke::new(1.6, color));
    p.line_segment([c + vec2(0.0, 5.0), c + vec2(0.0, 9.0)], Stroke::new(1.6, color));
    p.line_segment([c + vec2(-4.0, 9.0), c + vec2(4.0, 9.0)], Stroke::new(1.6, color));
    if on {
        ui.ctx().request_repaint();
    }
    p.text(
        pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        if on { "A OUVIR · PARAR" } else { "FALAR COM O JARVIS" },
        ui_kit::display(12.5),
        theme::mix(theme::TEXT, theme::BRIGHT, h),
    );
    resp.on_hover_text(if on { "Terminar a conversa por voz" } else { "Ctrl+Alt+J. Fala e o Jarvis responde em voz alta; diz \"obrigado\" para terminar." })
}

/// Puts `"model": "<model>"` in the worktree's .claude/settings.local.json (git-ignored in the ERP), keeping whatever
/// else is there, so a Claude Code session opened in that folder starts on that model.
fn set_worktree_model(dir: &std::path::Path, model: &str) {
    let file = dir.join(".claude").join("settings.local.json");
    let mut v: serde_json::Value = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v["model"] = serde_json::Value::String(model.to_owned());
    let _ = std::fs::create_dir_all(dir.join(".claude"));
    let _ = std::fs::write(&file, serde_json::to_string_pretty(&v).unwrap_or_default());
}
