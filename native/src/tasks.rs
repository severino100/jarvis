//! Personal to-do list in %APPDATA%\Jarvis\tasks.json (the same file as the Electron Jarvis), plus the Notion import
//! (read-only: unchecked to_do blocks of a page, or rows of a database). The Notion token is encrypted with DPAPI.

use crate::{
    settings::data_dir,
    theme::{self, Accent},
    ui_kit::{self, button},
};
use base64::Engine;
use chrono::{Local, NaiveDate};
use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime},
};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub due: Option<String>,
    pub done: bool,
    pub done_at: Option<String>,
    pub item_id: Option<i64>,
    pub source: String,
    pub notion_id: Option<String>,
    pub notion_url: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct TasksFile {
    tasks: Vec<Todo>,
}

fn file() -> std::path::PathBuf {
    data_dir().join("tasks.json")
}

fn read() -> Vec<Todo> {
    fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str::<TasksFile>(&s).ok()).unwrap_or_default().tasks
}

/// Read, change, write: the Electron version may have written in between.
fn change(f: impl FnOnce(&mut Vec<Todo>)) -> Vec<Todo> {
    let mut tasks = read();
    f(&mut tasks);
    let _ = fs::create_dir_all(data_dir());
    let _ = fs::write(file(), serde_json::to_string_pretty(&TasksFile { tasks: tasks.clone() }).unwrap_or_default());
    tasks
}

fn now() -> String {
    crate::chat::now()
}

fn new_id() -> String {
    format!("t{:x}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos())
}

/* ---------- Notion ---------- */

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct NotionConfig {
    /// DPAPI-encrypted, base64.
    pub token: Option<String>,
    pub sources: Vec<String>,
    pub last_import: Option<Value>,
}

fn notion_file() -> std::path::PathBuf {
    data_dir().join("notion-native.json")
}

pub fn notion_config() -> NotionConfig {
    fs::read_to_string(notion_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| {
        // The pages come over from the Electron settings; its token is encrypted with Electron's own key, so it is
        // asked again once.
        let old: Value = fs::read_to_string(data_dir().join("notion.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        NotionConfig { sources: serde_json::from_value(old["sources"].clone()).unwrap_or_default(), ..Default::default() }
    })
}

fn save_notion(c: &NotionConfig) {
    let _ = fs::create_dir_all(data_dir());
    let _ = fs::write(notion_file(), serde_json::to_string_pretty(c).unwrap_or_default());
}

pub mod dpapi {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB},
    };

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: plain Win32 calls with valid input/output blobs; the output buffer is freed with LocalFree.
        unsafe {
            let ok = if protect {
                CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
            };
            if ok == 0 {
                return None;
            }
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            Some(v)
        }
    }
    pub fn protect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, true)
    }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, false)
    }
}

fn notion_token(c: &NotionConfig) -> Option<String> {
    let raw = base64::engine::general_purpose::STANDARD.decode(c.token.as_ref()?).ok()?;
    String::from_utf8(dpapi::unprotect(&raw)?).ok()
}

pub fn notion_id(input: &str) -> Option<String> {
    let s = input.trim();
    let hex32 = regex::Regex::new(r"(?i)([0-9a-f]{32})(?:[?#].*)?$").unwrap();
    let uuid = regex::Regex::new(r"(?i)([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})").unwrap();
    hex32.captures(s).or_else(|| uuid.captures(s)).map(|c| c[1].replace('-', ""))
}

struct NotionErr {
    status: u16,
    message: String,
}

fn call(token: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value, NotionErr> {
    let url = format!("https://api.notion.com/v1{path}");
    let auth = format!("Bearer {token}");
    let r = match body {
        Some(b) => ureq::post(&url).header("Authorization", &auth).header("Notion-Version", "2022-06-28").config().http_status_as_error(false).build().send_json(b),
        None if method == "GET" => ureq::get(&url).header("Authorization", &auth).header("Notion-Version", "2022-06-28").config().http_status_as_error(false).build().call(),
        None => ureq::post(&url).header("Authorization", &auth).header("Notion-Version", "2022-06-28").config().http_status_as_error(false).build().send_json(json!({})),
    };
    let mut resp = r.map_err(|e| NotionErr { status: 0, message: format!("Sem ligação ao Notion: {e}") })?;
    let status = resp.status().as_u16();
    let v: Value = resp.body_mut().read_json().unwrap_or_default();
    if status >= 400 {
        return Err(NotionErr { status, message: v["message"].as_str().map(str::to_owned).unwrap_or_else(|| format!("O Notion respondeu {status}")) });
    }
    Ok(v)
}

struct Found {
    notion_id: String,
    title: String,
    due: Option<String>,
    done: bool,
    url: Option<String>,
}

fn plain(rich: &Value) -> String {
    rich.as_array().map(|a| a.iter().filter_map(|t| t["plain_text"].as_str()).collect::<String>()).unwrap_or_default().trim().to_owned()
}

fn is_done_name(s: &str) -> bool {
    regex::Regex::new(r"(?i)^(done|conclu[ií]d[oa]|feito|completed?|fechad[oa]|closed)$").unwrap().is_match(s)
}

fn from_row(p: &Value) -> Option<Found> {
    let props: Vec<&Value> = p["properties"].as_object().map(|o| o.values().collect()).unwrap_or_default();
    let title = props.iter().find(|x| x["type"] == "title").map(|x| plain(&x["title"])).unwrap_or_default();
    if title.is_empty() {
        return None;
    }
    let due = props.iter().find_map(|x| (x["type"] == "date").then(|| x["date"]["start"].as_str()).flatten()).map(|d| d.chars().take(10).collect());
    let done = props.iter().any(|x| {
        (x["type"] == "checkbox" && x["checkbox"] == true)
            || (x["type"] == "status" && is_done_name(x["status"]["name"].as_str().unwrap_or_default()))
            || (x["type"] == "select" && is_done_name(x["select"]["name"].as_str().unwrap_or_default()))
    });
    Some(Found { notion_id: p["id"].as_str()?.to_owned(), title, due, done, url: p["url"].as_str().map(str::to_owned) })
}

fn read_database(token: &str, id: &str) -> Result<Vec<Found>, NotionErr> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut body = json!({ "page_size": 100 });
        if let Some(c) = &cursor {
            body["start_cursor"] = json!(c);
        }
        let r = call(token, "POST", &format!("/databases/{id}/query"), Some(body))?;
        out.extend(r["results"].as_array().into_iter().flatten().filter_map(from_row));
        cursor = (r["has_more"] == true).then(|| r["next_cursor"].as_str().map(str::to_owned)).flatten();
        if cursor.is_none() {
            return Ok(out);
        }
    }
}

fn read_page(token: &str, id: &str, depth: u32) -> Result<Vec<Found>, NotionErr> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let q = cursor.as_ref().map(|c| format!("&start_cursor={c}")).unwrap_or_default();
        let r = call(token, "GET", &format!("/blocks/{id}/children?page_size=100{q}"), None)?;
        for b in r["results"].as_array().into_iter().flatten() {
            if b["type"] == "to_do" {
                let title = plain(&b["to_do"]["rich_text"]);
                if !title.is_empty() {
                    out.push(Found { notion_id: b["id"].as_str().unwrap_or_default().to_owned(), title, due: None, done: b["to_do"]["checked"] == true, url: None });
                }
            }
            // Checklists often sit inside toggles or columns; go one level down.
            if b["has_children"] == true && depth < 1 && b["type"] != "child_page" && b["type"] != "child_database" {
                out.extend(read_page(token, b["id"].as_str().unwrap_or_default(), depth + 1)?);
            }
        }
        cursor = (r["has_more"] == true).then(|| r["next_cursor"].as_str().map(str::to_owned)).flatten();
        if cursor.is_none() {
            return Ok(out);
        }
    }
}

/// Reads every source and merges: adds what is new, updates title/due, closes what is done there.
pub fn import_notion() -> Result<String, String> {
    let mut c = notion_config();
    let token = notion_token(&c).ok_or("Falta o token do Notion.")?;
    if c.sources.is_empty() {
        return Err("Falta indicar as páginas do Notion.".into());
    }
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for src in &c.sources {
        let Some(id) = notion_id(src) else {
            errors.push(format!("Não percebi o link: {src}"));
            continue;
        };
        let is_db = match call(&token, "GET", &format!("/databases/{id}"), None) {
            Ok(_) => Ok(true),
            Err(e) if e.status == 404 || e.status == 400 => Ok(false),
            Err(e) => Err(e),
        };
        let r = is_db.and_then(|db| if db { read_database(&token, &id) } else { read_page(&token, &id, 0) });
        match r {
            Ok(v) => found.extend(v),
            Err(e) => errors.push(match e.status {
                401 => "O token do Notion foi recusado.".to_owned(),
                404 => format!("Sem acesso a {src}: partilha a página com a integração (⋯ → Ligações)."),
                _ => format!("{src}: {}", e.message),
            }),
        }
    }
    let (mut added, mut updated) = (0, 0);
    change(|tasks| {
        for f in &found {
            match tasks.iter_mut().find(|t| t.notion_id.as_deref() == Some(&f.notion_id)) {
                None if !f.done => {
                    tasks.insert(0, Todo { id: new_id(), title: f.title.chars().take(300).collect(), due: f.due.clone(), source: "notion".into(), notion_id: Some(f.notion_id.clone()), notion_url: f.url.clone(), created_at: now(), ..Default::default() });
                    added += 1;
                }
                None => {}
                Some(t) if t.title != f.title || t.due != f.due || (f.done && !t.done) => {
                    t.title = f.title.chars().take(300).collect();
                    t.due = f.due.clone();
                    if f.done && !t.done {
                        t.done = true;
                        t.done_at = Some(now());
                    }
                    updated += 1;
                }
                Some(_) => {}
            }
        }
    });
    c.last_import = Some(json!({ "at": now(), "found": found.len(), "added": added, "updated": updated, "errors": errors }));
    save_notion(&c);
    if !errors.is_empty() && found.is_empty() {
        return Err(errors.join(" "));
    }
    let mut msg = format!("{} encontradas no Notion: {added} novas, {updated} atualizadas.", found.len());
    if !errors.is_empty() {
        msg.push_str(&format!(" Avisos: {}", errors.join(" ")));
    }
    Ok(msg)
}

/* ---------- view ---------- */

pub struct TasksUi {
    tasks: Vec<Todo>,
    read_at: Option<Instant>,
    title: String,
    due: String,
    item: String,
    show_done: bool,
    error: Option<String>,
    notion_open: bool,
    token: String,
    sources: String,
    status: Arc<Mutex<Option<(bool, String)>>>,
    importing: Arc<Mutex<bool>>,
}

impl Default for TasksUi {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            read_at: None,
            title: String::new(),
            due: String::new(),
            item: String::new(),
            show_done: false,
            error: None,
            notion_open: false,
            token: String::new(),
            sources: notion_config().sources.join("\n"),
            status: Arc::default(),
            importing: Arc::default(),
        }
    }
}

impl TasksUi {
    pub fn open_count(&mut self) -> usize {
        self.refresh(false);
        self.tasks.iter().filter(|t| !t.done).count()
    }
    fn refresh(&mut self, force: bool) {
        if force || self.read_at.is_none_or(|a| a.elapsed().as_secs() > 5) {
            self.tasks = read();
            self.read_at = Some(Instant::now());
        }
    }
}

fn due_label(due: &str) -> (String, egui::Color32) {
    let Ok(d) = NaiveDate::parse_from_str(due, "%Y-%m-%d") else { return (due.to_owned(), theme::MUTED) };
    let days = (d - Local::now().date_naive()).num_days();
    match days {
        ..0 => (format!("atrasada ({})", d.format("%d/%m")), theme::RED),
        0 => ("hoje".into(), theme::AMBER),
        1 => ("amanhã".into(), theme::TEXT),
        _ => (d.format("%d/%m").to_string(), theme::MUTED),
    }
}

pub fn view(ui: &mut Ui, st: &mut TasksUi, acc: Accent) {
    st.refresh(false);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        ui.horizontal(|ui| {
            let w = ui.available_width() - 420.0;
            let enter = ui.add(TextEdit::singleline(&mut st.title).hint_text("Nova tarefa…").desired_width(w.max(160.0))).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add(TextEdit::singleline(&mut st.due).hint_text("aaaa-mm-dd").desired_width(120.0)).on_hover_text("Data limite (opcional)");
            ui.add(TextEdit::singleline(&mut st.item).hint_text("#item").desired_width(90.0)).on_hover_text("Item do Azure DevOps (opcional)");
            if (button(ui, "Adicionar", true, acc).clicked() || enter) && !st.title.trim().is_empty() {
                let due = st.due.trim();
                let item = st.item.trim().trim_start_matches('#');
                if !due.is_empty() && NaiveDate::parse_from_str(due, "%Y-%m-%d").is_err() {
                    st.error = Some("A data tem de ser aaaa-mm-dd.".into());
                } else if !item.is_empty() && item.parse::<i64>().is_err() {
                    st.error = Some("O item tem de ser um número.".into());
                } else {
                    let t = Todo {
                        id: new_id(),
                        title: st.title.trim().chars().take(300).collect(),
                        due: (!due.is_empty()).then(|| due.to_owned()),
                        item_id: item.parse().ok(),
                        source: "jarvis".into(),
                        created_at: now(),
                        ..Default::default()
                    };
                    st.tasks = change(|ts| ts.insert(0, t));
                    st.title.clear();
                    st.due.clear();
                    st.item.clear();
                    st.error = None;
                }
            }
        });
        if let Some(e) = &st.error {
            ui.label(RichText::new(e).color(theme::RED));
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let open = st.tasks.iter().filter(|t| !t.done).count();
        let done = st.tasks.len() - open;
        if button(ui, &format!("Abertas · {open}"), !st.show_done, acc).clicked() {
            st.show_done = false;
        }
        if button(ui, &format!("Feitas · {done}"), st.show_done, acc).clicked() {
            st.show_done = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if button(ui, "Notion", st.notion_open, acc).clicked() {
                st.notion_open = !st.notion_open;
            }
            let has_source = !notion_config().sources.is_empty() && notion_config().token.is_some();
            let busy = *st.importing.lock().unwrap();
            if busy {
                crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
            } else if has_source && button(ui, "Importar do Notion", false, acc).clicked() {
                start_import(st, ui.ctx());
            }
        });
    });
    if let Some((ok, msg)) = st.status.lock().unwrap().clone() {
        ui.label(RichText::new(msg).color(if ok { theme::GREEN } else { theme::RED }));
    }
    if st.notion_open {
        notion_panel(ui, st, acc);
    }
    ui.add_space(8.0);

    let mut list: Vec<Todo> = st.tasks.iter().filter(|t| t.done == st.show_done).cloned().collect();
    if !st.show_done {
        list.sort_by(|a, b| (a.due.is_none(), a.due.clone()).cmp(&(b.due.is_none(), b.due.clone())));
    }
    if list.is_empty() {
        ui_kit::quiet(ui, if st.show_done { "Ainda não fechaste nenhuma tarefa." } else { "Sem tarefas abertas. Escreve uma acima ou importa do Notion." }, acc);
        return;
    }
    let mut toggle = None;
    let mut delete = None;
    for t in &list {
        egui::Frame::new().fill(theme::GLASS).stroke(egui::Stroke::new(1.0, theme::alpha(acc.main, 0.1))).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let mut done = t.done;
                if ui.checkbox(&mut done, "").changed() {
                    toggle = Some((t.id.clone(), done));
                }
                let title = RichText::new(&t.title).color(if t.done { theme::MUTED } else { theme::BRIGHT });
                ui.add(egui::Label::new(if t.done { title.strikethrough() } else { title }).wrap());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text("Apagar").clicked() {
                        delete = Some(t.id.clone());
                    }
                    if let Some(u) = &t.notion_url {
                        if ui.small_button("Notion").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(u));
                        }
                    } else if t.source == "notion" {
                        ui.label(RichText::new("Notion").small().color(theme::FAINT));
                    }
                    if let Some(id) = t.item_id {
                        ui_kit::id_link(ui, id, &crate::azdo::item_url(id), acc);
                    }
                    if let Some(d) = &t.due {
                        let (text, c) = due_label(d);
                        ui.label(RichText::new(text).monospace().size(12.0).color(c));
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
    if let Some((id, done)) = toggle {
        st.tasks = change(|ts| {
            if let Some(t) = ts.iter_mut().find(|t| t.id == id) {
                t.done = done;
                t.done_at = done.then(now);
            }
        });
    }
    if let Some(id) = delete {
        st.tasks = change(|ts| ts.retain(|t| t.id != id));
    }
}

fn start_import(st: &mut TasksUi, ctx: &egui::Context) {
    *st.importing.lock().unwrap() = true;
    let (status, busy, ctx) = (st.status.clone(), st.importing.clone(), ctx.clone());
    st.read_at = None;
    std::thread::spawn(move || {
        let r = import_notion();
        *status.lock().unwrap() = Some(match r {
            Ok(m) => (true, m),
            Err(e) => (false, e),
        });
        *busy.lock().unwrap() = false;
        ctx.request_repaint();
    });
}

fn notion_panel(ui: &mut Ui, st: &mut TasksUi, acc: Accent) {
    ui.add_space(8.0);
    ui_kit::hud_panel(ui, acc, false, |ui| {
        let c = notion_config();
        ui.label(RichText::new("NOTION").font(ui_kit::display(13.0)).color(acc.main).extra_letter_spacing(2.0));
        ui.label(RichText::new("Cria uma integração interna em notion.so/my-integrations, copia o token e partilha as páginas ou bases de dados com ela (⋯ → Ligações). O Jarvis só lê.").color(theme::MUTED));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(if c.token.is_some() { "Token guardado." } else { "Token:" });
            ui.add(TextEdit::singleline(&mut st.token).password(true).hint_text(if c.token.is_some() { "substituir o token…" } else { "ntn_…" }).desired_width(320.0));
        });
        ui.label("Páginas ou bases de dados (um link por linha):");
        ui.add(TextEdit::multiline(&mut st.sources).desired_rows(3).desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            if button(ui, "Guardar", true, acc).clicked() {
                let mut c = notion_config();
                let t = st.token.trim();
                let ok_token = t.is_empty() || regex::Regex::new(r"^(secret_|ntn_)[A-Za-z0-9]{20,}$").unwrap().is_match(t);
                let sources: Vec<String> = st.sources.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
                let bad = sources.iter().find(|s| notion_id(s).is_none());
                *st.status.lock().unwrap() = Some(if !ok_token {
                    (false, "Isto não parece um token de integração do Notion (começa por ntn_ ou secret_).".into())
                } else if let Some(b) = bad {
                    (false, format!("Não percebi: {b}"))
                } else {
                    if !t.is_empty() {
                        c.token = dpapi::protect(t.as_bytes()).map(|b| base64::engine::general_purpose::STANDARD.encode(b));
                    }
                    c.sources = sources;
                    save_notion(&c);
                    st.token.clear();
                    (true, "Notion guardado.".into())
                });
            }
            if c.token.is_some() && button(ui, "Esquecer o token", false, acc).clicked() {
                let mut c = notion_config();
                c.token = None;
                save_notion(&c);
            }
        });
        if let Some(l) = &c.last_import {
            ui.label(RichText::new(format!("Última importação: {} · {} encontradas", ui_kit::when(l["at"].as_str().unwrap_or_default()), l["found"])).small().color(theme::FAINT));
        }
    });
}

/// Open personal tasks, soonest due first (for Hoje).
pub fn open_list() -> Vec<Todo> {
    let mut v: Vec<Todo> = read().into_iter().filter(|t| !t.done).collect();
    v.sort_by(|a, b| (a.due.is_none(), a.due.clone()).cmp(&(b.due.is_none(), b.due.clone())));
    v
}
