//! System telemetry for the dashboard: CPU (with a minute of history), memory and the system disk, read every 1.5 s
//! on a background thread.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use sysinfo::{Disks, System};

#[derive(Clone, Default)]
pub struct Snapshot {
    pub cpu: f32,
    pub cpu_history: VecDeque<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_free: u64,
    pub disk_total: u64,
    pub ready: bool,
}

#[derive(Clone, Default)]
pub struct SysMon {
    state: Arc<Mutex<Snapshot>>,
}

impl SysMon {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let me = Self::default();
        let state = me.state.clone();
        thread::spawn(move || {
            // Telemetry is a nice-to-have: if the system cannot be read, the panel simply stays on "a ler".
            let (Ok(mut sys), Ok(mut disks)) = (System::new(), Disks::new_with_refreshed_list()) else { return };
            loop {
                sys.refresh_cpu_usage();
                sys.refresh_memory();
                disks.refresh(false);
                let system_disk = disks.iter().find(|d| d.mount_point().to_string_lossy().to_uppercase().starts_with("C:"));
                {
                    let mut s = state.lock().unwrap();
                    let cpu = sys.global_cpu_usage();
                    s.cpu = cpu;
                    s.cpu_history.push_back(cpu);
                    while s.cpu_history.len() > 60 {
                        s.cpu_history.pop_front();
                    }
                    s.mem_used = sys.used_memory();
                    s.mem_total = sys.total_memory();
                    if let Some(d) = system_disk {
                        s.disk_free = d.available_space();
                        s.disk_total = d.total_space();
                    }
                    s.ready = true;
                }
                ctx.request_repaint();
                thread::sleep(Duration::from_millis(1500));
            }
        });
        me
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
}
