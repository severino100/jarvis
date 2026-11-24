//! Cleanup of the per-item worktrees of the pack's repositories (<repo>-<id>) and of the downloaded attachments in
//! %TEMP%\azdo. Same rules as cleanup.mjs: refuses when there is unsaved or unpushed work, never forces, and removes
//! only the node_modules junction (a recursive delete through it would wipe the main node_modules).

use crate::{
    azdo,
    theme::{self, Accent},
    ui_kit::{self, button},
};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use regex::Regex;
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

const DONE_COLUMNS: [&str; 5] = ["To Test", "Testing", "Done", "Closed", "Ready for QA"];
const DONE_STATES: [&str; 4] = ["Done", "Closed", "Removed", "Resolved"];

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Wait,
    Block,
}

#[derive(Clone)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub repo_dir: String,
    /// The repository's node_modules is shared with its worktrees through a junction.
    pub linked: bool,
    pub name: String,
    pub id: Option<i64>,
    pub exists: bool,
    pub upstream: bool,
    pub ahead: u32,
    pub item: Option<(String, String)>,
    pub item_done: bool,
    pub level: Level,
    pub status: String,
}

fn run(cmd: &str, args: &[&str]) -> (bool, String, String) {
    match Command::new(cmd).args(args).creation_flags(0x0800_0000).output() {
        Ok(o) => (o.status.success(), String::from_utf8_lossy(&o.stdout).trim().to_owned(), String::from_utf8_lossy(&o.stderr).trim().to_owned()),
        Err(e) => (false, String::new(), e.to_string()),
    }
}

fn git(dir: &str, args: &[&str]) -> (bool, String, String) {
    let mut a = vec!["-C", dir];
    a.extend_from_slice(args);
    run("git", &a)
}

fn list(r: &crate::profile::RepoCfg) -> Vec<Worktree> {
    let dir = crate::profile::repo_dir(r).to_string_lossy().into_owned();
    let (repo, linked) = (if r.side.is_empty() { r.dir.clone() } else { r.side.clone() }, r.link_node_modules);
    // A worktree of this repository named by the team convention: <projects>\<pattern with {repo} and {id}>.
    let pattern = crate::profile::worktree_dir(r, 987654321).to_string_lossy().into_owned();
    let pattern = regex::escape(&pattern.replace("\\", "/")).replace("987654321", r"([^/]+)");
    let Ok(managed) = Regex::new(&format!("(?i)^{pattern}$")) else { return Vec::new() };
    let (ok, out, _) = git(&dir, &["worktree", "list", "--porcelain"]);
    if !ok {
        return Vec::new();
    }
    out.split("\n\n")
        .filter_map(|block| {
            let mut path = String::new();
            let mut branch = None;
            let mut prunable = false;
            for l in block.lines() {
                let (k, v) = l.split_once(' ').unwrap_or((l, ""));
                match k {
                    "worktree" => path = v.replace('/', "\\"),
                    "branch" => branch = Some(v.trim_start_matches("refs/heads/").to_owned()),
                    "prunable" => prunable = true,
                    _ => {}
                }
            }
            let unix = path.replace('\\', "/");
            let m = managed.captures(&unix)?;
            let suffix = m[1].to_owned();
            let id = Regex::new(r"^\d{3,6}").unwrap().find(&suffix).and_then(|x| x.as_str().parse().ok()).or_else(|| {
                branch.as_ref().and_then(|b| Regex::new(r"/(\d{3,6})(?:\D|$)").unwrap().captures(b).and_then(|c| c[1].parse().ok()))
            });
            let exists = !prunable && Path::new(&path).exists();
            Some(Worktree {
                path,
                branch,
                repo_dir: dir.clone(),
                linked,
                name: format!("{repo}-{suffix}"),
                id,
                exists,
                upstream: false,
                ahead: 0,
                item: None,
                item_done: false,
                level: Level::Wait,
                status: String::new(),
            })
        })
        .collect()
}

/// (dirty files, has upstream, commits ahead)
fn git_state(path: &str) -> (usize, bool, u32) {
    let (_, status, _) = git(path, &["status", "--porcelain"]);
    let (up, ahead, _) = git(path, &["rev-list", "--count", "@{u}..HEAD"]);
    (status.lines().filter(|l| !l.is_empty()).count(), up, ahead.parse().unwrap_or(0))
}

pub fn scan() -> Vec<Worktree> {
    let mut all: Vec<Worktree> = crate::profile::repos().iter().flat_map(list).collect();
    let mut ids: Vec<i64> = all.iter().filter_map(|w| w.id).collect();
    ids.sort();
    ids.dedup();
    let items: Vec<serde_json::Value> = if ids.is_empty() { Vec::new() } else { azdo::items_raw(&ids).unwrap_or_default() };
    for w in &mut all {
        let it = w.id.and_then(|id| items.iter().find(|x| x["id"].as_i64() == Some(id)));
        if let Some(it) = it {
            let f = &it["fields"];
            let state = f["System.State"].as_str().unwrap_or_default().to_owned();
            let column = f["System.BoardColumn"].as_str().map(str::to_owned).unwrap_or_else(|| state.clone());
            w.item_done = DONE_COLUMNS.contains(&column.as_str()) || DONE_STATES.contains(&state.as_str());
            w.item = Some((f["System.Title"].as_str().unwrap_or_default().to_owned(), column));
        }
        let (level, status) = if !w.exists {
            (Level::Ok, "A pasta já não existe: só falta tirar o registo do git.".to_owned())
        } else {
            let (dirty, up, ahead) = git_state(&w.path);
            w.upstream = up;
            w.ahead = ahead;
            if dirty > 0 {
                (Level::Block, format!("{dirty} ficheiro{} por commitar.", if dirty > 1 { "s" } else { "" }))
            } else if !up && w.branch.is_some() {
                (Level::Block, "O branch nunca foi enviado (sem push).".into())
            } else if ahead > 0 {
                (Level::Block, format!("{ahead} commit{} por enviar.", if ahead > 1 { "s" } else { "" }))
            } else if w.item_done {
                (Level::Ok, format!("O item está em {}: o trabalho já saiu daqui.", w.item.as_ref().unwrap().1))
            } else if let Some((_, col)) = &w.item {
                (Level::Wait, format!("O item ainda está em {col}."))
            } else {
                (Level::Wait, "Sem item associado: confirma antes de limpar.".into())
            }
        };
        w.level = level;
        w.status = status;
    }
    all.sort_by(|a, b| a.level.cmp(&b.level).then(a.id.cmp(&b.id)));
    all
}

fn stop_processes_in(path: &str) -> usize {
    let ps = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.Name -in @('node.exe','dotnet.exe','esbuild.exe') -and $_.CommandLine -like '*{}*' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue; $_.Name }}",
        path.replace('\'', "''")
    );
    let (_, out, _) = run("powershell.exe", &["-NoProfile", "-Command", &ps]);
    out.lines().filter(|l| !l.is_empty()).count()
}

fn remove_node_modules(path: &str, log: &mut Vec<String>) -> bool {
    let nm = Path::new(path).join("node_modules");
    let Ok(meta) = fs::symlink_metadata(&nm) else { return true };
    if meta.file_type().is_symlink() || is_junction(&nm) {
        let nm_s = nm.to_string_lossy().to_string();
        let (ok, _, err) = run("cmd.exe", &["/d", "/c", "rmdir", &nm_s]);
        if !ok || nm.exists() {
            log.push(format!("Não consegui tirar a ligação do node_modules: {err}"));
            return false;
        }
        log.push("Ligação do node_modules removida (o node_modules principal não foi tocado).".into());
        return true;
    }
    let _ = fs::remove_dir_all(&nm);
    log.push("node_modules próprio do worktree apagado.".into());
    true
}

/// Junctions are reparse points; std reports them as symlinks only on some Rust versions, so check the attribute.
fn is_junction(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & 0x400 != 0)
}

fn remove_launch_entries(id: i64, log: &mut Vec<String>) {
    let file = crate::profile::main_dir().join(".claude").join("launch.json");
    let Ok(text) = fs::read_to_string(&file) else { return };
    let Ok(mut j) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let re = Regex::new(&format!(r"(^|\D){id}(\D|$)")).unwrap();
    let Some(list) = j["configurations"].as_array_mut() else { return };
    let before = list.len();
    list.retain(|c| !re.is_match(c["name"].as_str().unwrap_or_default()));
    if list.len() != before {
        let _ = fs::write(&file, format!("{}\n", serde_json::to_string_pretty(&j).unwrap_or_default()));
        log.push(format!("Entrada do servidor de desenvolvimento do #{id} retirada do launch.json."));
    }
}

/// Removes one worktree. Refuses when there is unsaved or unpushed work; never forces.
pub fn clean(w: &Worktree) -> (bool, Vec<String>) {
    let mut log = Vec::new();
    let exists = Path::new(&w.path).exists();
    let (dirty, up, ahead) = if exists { git_state(&w.path) } else { (0, false, 0) };
    if exists && (dirty > 0 || (w.branch.is_some() && (!up || ahead > 0))) {
        return (false, vec!["Não limpei: há trabalho por commitar ou por enviar. Resolve isso primeiro.".into()]);
    }
    if exists {
        let killed = stop_processes_in(&w.path);
        if killed > 0 {
            log.push(format!("{killed} processo{} a correr nesta pasta parado{}.", if killed > 1 { "s" } else { "" }, if killed > 1 { "s" } else { "" }));
        }
        if !remove_node_modules(&w.path, &mut log) {
            return (false, log);
        }
        if w.linked && !Path::new(&w.repo_dir).join("node_modules").exists() {
            log.push(format!("ATENÇÃO: o node_modules principal desapareceu. Corre npm install em {}.", w.repo_dir));
        }
        let (ok, _, err) = git(&w.repo_dir, &["worktree", "remove", &w.path]);
        if !ok {
            log.push(format!("O git recusou remover o worktree: {}", err.lines().next().unwrap_or_default()));
            return (false, log);
        }
        log.push("Worktree removido.".into());
    } else {
        git(&w.repo_dir, &["worktree", "prune"]);
        log.push("Registo do worktree apagado (a pasta já não existia).".into());
    }
    if let Some(b) = &w.branch {
        if exists && up && ahead == 0 {
            let (ok, _, err) = git(&w.repo_dir, &["branch", "-D", b]);
            log.push(if ok { format!("Branch local {b} apagado (continua no servidor).") } else { format!("Branch local {b} mantido: {}", err.lines().next().unwrap_or_default()) });
        } else {
            log.push(format!("Branch local {b} mantido."));
        }
    }
    // The dev-server entry belongs to the item, not to one side: keep it while the other side's worktree is still there.
    if let Some(id) = w.id {
        let siblings: Vec<String> = crate::profile::repos().iter().map(|r| crate::profile::worktree_dir(r, id).to_string_lossy().into_owned()).collect();
        if !siblings.iter().any(|p| !p.eq_ignore_ascii_case(&w.path) && Path::new(p).exists()) {
            remove_launch_entries(id, &mut log);
        }
    }
    (true, log)
}

pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("azdo")
}

pub fn temp_info() -> (usize, u64) {
    fs::read_dir(temp_dir())
        .map(|rd| rd.filter_map(|e| e.ok()?.metadata().ok()).filter(|m| m.is_file()).fold((0, 0), |(n, b), m| (n + 1, b + m.len())))
        .unwrap_or((0, 0))
}

/* ---------- view ---------- */

#[derive(Default)]
pub struct CleanupUi {
    state: Arc<Mutex<Scan>>,
    confirm: Option<String>,
}

#[derive(Default)]
struct Scan {
    list: Option<Vec<Worktree>>,
    scanning: bool,
    working: Option<String>,
    logs: Vec<(String, bool, Vec<String>)>,
    /// Item ids whose conversations can go once their worktree is gone.
    cleaned_items: Vec<i64>,
}

impl CleanupUi {
    fn rescan(&self, ctx: &egui::Context) {
        let mut s = self.state.lock().unwrap();
        if s.scanning {
            return;
        }
        s.scanning = true;
        let (st, ctx) = (self.state.clone(), ctx.clone());
        std::thread::spawn(move || {
            let l = scan();
            let mut s = st.lock().unwrap();
            s.list = Some(l);
            s.scanning = false;
            ctx.request_repaint();
        });
    }

    pub fn take_cleaned_items(&self) -> Vec<i64> {
        std::mem::take(&mut self.state.lock().unwrap().cleaned_items)
    }
}

pub fn view(ui: &mut Ui, st: &mut CleanupUi, acc: Accent) {
    let ctx = ui.ctx().clone();
    if st.state.lock().unwrap().list.is_none() && !st.state.lock().unwrap().scanning {
        st.rescan(&ctx);
    }
    let s = st.state.lock().unwrap();
    let busy = s.scanning || s.working.is_some();
    let list = s.list.clone();
    let logs = s.logs.clone();
    let working = s.working.clone();
    drop(s);

    ui.horizontal(|ui| {
        if !busy && button(ui, "Procurar de novo", false, acc).clicked() {
            st.rescan(&ctx);
        }
        if busy {
            crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
            ui.label(RichText::new(if working.is_some() { "A limpar…" } else { "A ver os worktrees…" }).color(theme::MUTED));
        }
    });
    ui.add_space(8.0);
    for (name, ok, lines) in logs.iter().rev().take(3) {
        ui_kit::hint(ui, &format!("{name}: {}", lines.join(" ")), !ok, acc);
        ui.add_space(4.0);
    }

    ui_kit::section_label(ui, "Worktrees", acc.main, acc);
    match &list {
        None => ui_kit::quiet(ui, "A ver os worktrees…", acc),
        Some(l) if l.is_empty() => ui_kit::quiet(ui, "Não há worktrees de itens. Está tudo limpo.", acc),
        Some(l) => {
            for w in l {
                ui_kit::hud_panel(ui, acc, w.level == Level::Block, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&w.name).monospace().strong().color(theme::BRIGHT));
                        if let Some(id) = w.id {
                            ui_kit::id_link(ui, id, &azdo::item_url(id), acc);
                        }
                        if let Some((title, col)) = &w.item {
                            ui_kit::column_pill(ui, col, acc);
                            ui.add(egui::Label::new(RichText::new(title).color(theme::MUTED)).truncate());
                        }
                    });
                    ui.label(RichText::new(format!("{}{}", w.path, w.branch.as_ref().map(|b| format!(" · {b}")).unwrap_or_default())).monospace().size(11.5).color(theme::FAINT));
                    ui.horizontal(|ui| {
                        let c = match w.level {
                            Level::Ok => theme::GREEN,
                            Level::Wait => theme::AMBER,
                            Level::Block => theme::RED,
                        };
                        ui.label(RichText::new(&w.status).color(c));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if w.level == Level::Block || busy {
                                return;
                            }
                            if st.confirm.as_deref() == Some(&w.path) {
                                if button(ui, "Sim, limpar", true, acc).clicked() {
                                    st.confirm = None;
                                    start_clean(st, w.clone(), &ctx);
                                }
                                if button(ui, "Não", false, acc).clicked() {
                                    st.confirm = None;
                                }
                                ui.label(RichText::new("Remove o worktree e o branch local.").color(theme::AMBER));
                            } else if button(ui, "Limpar", w.level == Level::Ok, acc).clicked() {
                                st.confirm = Some(w.path.clone());
                            }
                        });
                    });
                });
                ui.add_space(8.0);
            }
        }
    }

    ui.add_space(6.0);
    ui_kit::section_label(ui, "Anexos descarregados", acc.main, acc);
    let (files, bytes) = temp_info();
    ui_kit::hud_panel(ui, acc, false, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if files == 0 { "Nada em %TEMP%\\azdo.".to_owned() } else { format!("{files} ficheiro{} em %TEMP%\\azdo ({:.1} MB)", if files > 1 { "s" } else { "" }, bytes as f64 / 1_048_576.0) }).color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if files > 0 && button(ui, "Apagar", false, acc).clicked() {
                    let _ = fs::remove_dir_all(temp_dir());
                }
            });
        });
    });
}

fn start_clean(st: &mut CleanupUi, w: Worktree, ctx: &egui::Context) {
    st.state.lock().unwrap().working = Some(w.path.clone());
    let (state, ctx) = (st.state.clone(), ctx.clone());
    std::thread::spawn(move || {
        let (ok, log) = clean(&w);
        let fresh = scan();
        let mut s = state.lock().unwrap();
        if ok && w.item_done {
            if let Some(id) = w.id {
                s.cleaned_items.push(id);
            }
        }
        s.logs.push((w.name.clone(), ok, log));
        s.list = Some(fresh);
        s.working = None;
        drop(s);
        ctx.request_repaint();
    });
}
