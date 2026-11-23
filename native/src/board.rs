//! Keeps the board fresh: loads it on a background thread at start, every N minutes and on demand.

use crate::azdo::{self, Dashboard};
use eframe::egui;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct BoardState {
    pub data: Option<Dashboard>,
    pub error: Option<String>,
    pub loading: bool,
    pub fetched_at: Option<Instant>,
}

#[derive(Clone)]
pub struct Board {
    state: Arc<Mutex<BoardState>>,
    ctx: egui::Context,
}

impl Board {
    pub fn start(ctx: egui::Context, poll_minutes: u32) -> Self {
        let board = Self { state: Arc::default(), ctx };
        board.refresh();
        let every = Duration::from_secs(60 * u64::from(poll_minutes.max(1)));
        let b = board.clone();
        thread::spawn(move || loop {
            thread::sleep(every);
            b.refresh();
        });
        board
    }

    /// Safe to call from anywhere, also from inside `with` (the drawing code does): the state is only locked on the
    /// background thread, never by the caller.
    pub fn refresh(&self) {
        if let Some(d) = azdo::demo() {
            let mut s = self.state.lock().unwrap();
            s.data = Some(d);
            s.fetched_at = Some(Instant::now());
            return;
        }
        let (state, ctx) = (self.state.clone(), self.ctx.clone());
        thread::spawn(move || {
            {
                let mut s = state.lock().unwrap();
                if s.loading {
                    return;
                }
                s.loading = true;
            }
            ctx.request_repaint();
            let result = azdo::dashboard();
            let mut s = state.lock().unwrap();
            match result {
                Ok(d) => {
                    s.data = Some(d);
                    s.error = None;
                    s.fetched_at = Some(Instant::now());
                }
                Err(e) => s.error = Some(e),
            }
            s.loading = false;
            drop(s);
            ctx.request_repaint();
        });
    }

    /// Short read access for drawing a frame.
    pub fn with<R>(&self, f: impl FnOnce(&BoardState) -> R) -> R {
        f(&self.state.lock().unwrap())
    }
}
