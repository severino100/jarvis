//! Azure DevOps client for the active pack's board (organization, project, area and columns come from its jarvis.toml).
//! Blocking HTTP (ureq) on a background thread; independent calls run in parallel with scoped threads.

use base64::Engine;
use regex::Regex;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::OnceLock, thread};

fn cfg() -> crate::profile::BoardCfg {
    crate::profile::board().unwrap_or_default()
}

fn org() -> String {
    cfg().org.trim_end_matches('/').to_owned()
}

fn project() -> String {
    cfg().project
}

/// The project as it goes in a URL.
fn proj() -> String {
    project().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A board is configured in the active pack.
pub fn configured() -> bool {
    crate::profile::board().is_some()
}
const V: &str = "api-version=7.1";
const COMMENTS_V: &str = "api-version=7.1-preview.4";
const LATER_STATES: [&str; 5] = ["Code Review", "In review", "Ready for QA", "Testing", "Done"];

pub type Res<T> = Result<T, String>;

#[derive(Clone, Default, Debug)]
pub struct Me {
    pub email: String,
    pub name: String,
}

#[derive(Clone, Default, Debug)]
pub struct Sprint {
    pub path: String,
    pub name: String,
    pub start: String,
    pub finish: String,
}

#[derive(Clone, Default, Debug)]
pub struct Comment {
    pub id: i64,
    pub by: String,
    pub at: String,
    pub text: String,
    pub images: Vec<String>,
}

#[derive(Clone, Default, Debug)]
pub struct Back {
    pub from: String,
    pub to: String,
    pub at: String,
    pub by: String,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Default, Debug)]
pub struct Parent {
    pub id: i64,
    pub title: String,
    pub column: String,
    pub url: String,
}

#[derive(Clone, Default, Debug)]
pub struct Task {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub state: String,
    pub side: Option<String>,
    pub estimate: Option<f64>,
    pub work_done: Option<f64>,
    pub activity: Option<String>,
    pub parent_id: Option<i64>,
    pub parent: Option<Parent>,
}

#[derive(Clone, Default, Debug)]
pub struct Item {
    pub id: i64,
    pub url: String,
    pub kind: String,
    pub title: String,
    pub column: String,
    pub in_sprint: bool,
    pub effort: Option<f64>,
    pub priority: Option<i64>,
    pub severity: Option<String>,
    pub tags: Vec<String>,
    pub owner: Option<String>,
    pub owner_email: Option<String>,
    pub changed_at: String,
    pub created_by: Option<String>,
    pub child_ids: Vec<i64>,
    pub back: Option<Back>,
    pub last_comment: Option<Comment>,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Default, Debug)]
pub struct Dashboard {
    pub me: Me,
    pub sprint: Sprint,
    pub now: Vec<Item>,
    pub review: Vec<Item>,
    pub refined: Vec<Item>,
    /// Items in the available column (Refined) already assigned to me.
    pub mine_refined: Vec<Item>,
    pub tasks: Vec<Task>,
}

pub fn item_url(id: i64) -> String {
    format!("{}/{}/_workitems/edit/{id}", org(), proj())
}

fn auth() -> Res<String> {
    let pat = crate::profile::board_token().ok_or("Falta o token do Azure DevOps: guarda-o em Definições → Perfil e equipa.")?;
    Ok(format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!(":{}", pat.trim()))))
}

fn request(method: &str, path: &str, body: Option<Value>) -> Res<Value> {
    if !configured() {
        return Err("Sem board: adiciona o pacote da tua equipa em Definições → Perfil e equipa.".to_owned());
    }
    let url = if path.starts_with("http") { path.to_owned() } else { format!("{}/{path}", org()) };
    let a = auth()?;
    let a = a.as_str();
    let result = match (method, body) {
        ("POST", Some(b)) => ureq::post(&url).header("Authorization", a).header("Accept", "application/json").send_json(b),
        ("PUT", Some(b)) => ureq::put(&url).header("Authorization", a).header("Accept", "application/json").send_json(b),
        ("PATCH", Some(b)) => ureq::patch(&url)
            .header("Authorization", a)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json-patch+json")
            .send(b.to_string()),
        _ => ureq::get(&url).header("Authorization", a).header("Accept", "application/json").call(),
    };
    let mut resp = result.map_err(|e| match e {
        ureq::Error::StatusCode(401) => "O token do Azure DevOps foi recusado (401). Gera um novo e guarda-o em Definições → Perfil e equipa.".to_owned(),
        ureq::Error::StatusCode(code) => format!("{code} {method} {url}"),
        other => format!("Sem ligação ao Azure DevOps: {other}"),
    })?;
    let text = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn get(path: &str) -> Res<Value> {
    request("GET", path, None)
}

fn wiql(query: &str, top: u32) -> Res<Vec<i64>> {
    let r = request("POST", &format!("{}/_apis/wit/wiql?$top={top}&{V}", proj()), Some(json!({ "query": query })))?;
    Ok(r["workItems"].as_array().map(|a| a.iter().filter_map(|w| w["id"].as_i64()).collect()).unwrap_or_default())
}

fn get_items(ids: &[i64], expand: bool) -> Res<Vec<Value>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(200) {
        if chunk.is_empty() {
            continue;
        }
        let list = chunk.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let r = get(&format!("{}/_apis/wit/workitems?ids={list}{}&{V}", proj(), if expand { "&$expand=relations" } else { "" }))?;
        out.extend(r["value"].as_array().cloned().unwrap_or_default());
    }
    Ok(out)
}

fn f<'a>(w: &'a Value, k: &str) -> &'a Value {
    &w["fields"][k]
}
fn fs_(w: &Value, k: &str) -> String {
    f(w, k).as_str().unwrap_or_default().to_owned()
}
fn person(w: &Value, k: &str) -> Option<String> {
    f(w, k)["displayName"].as_str().map(str::to_owned)
}

fn relation_ids(w: &Value, rel: &str) -> Vec<i64> {
    w["relations"]
        .as_array()
        .map(|a| a.iter().filter(|r| r["rel"] == rel).filter_map(|r| r["url"].as_str()?.rsplit('/').next()?.parse().ok()).collect())
        .unwrap_or_default()
}

fn decode(s: &str) -> String {
    s.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// Comments mix HTML <img> and markdown ![x](url): plain text plus image URLs.
fn parse_rich(html: &str) -> (String, Vec<String>) {
    static RE: OnceLock<[Regex; 6]> = OnceLock::new();
    let [img, md, br, close, li, tag] = RE.get_or_init(|| {
        [
            Regex::new(r#"(?i)<img[^>]*src="([^"]+)"[^>]*>"#).unwrap(),
            Regex::new(r"!\[[^\]]*\]\((https?:[^)\s]+)\)").unwrap(),
            Regex::new(r"(?i)<br\s*/?>").unwrap(),
            Regex::new(r"(?i)</(p|div|li|h\d|tr)>").unwrap(),
            Regex::new(r"(?i)<li[^>]*>").unwrap(),
            Regex::new(r"<[^>]+>").unwrap(),
        ]
    });
    let mut images: Vec<String> = Vec::new();
    for c in img.captures_iter(html) {
        images.push(decode(&c[1]));
    }
    for c in md.captures_iter(html) {
        images.push(c[1].to_owned());
    }
    images.dedup();
    let s = img.replace_all(html, " ");
    let s = md.replace_all(&s, " ");
    let s = br.replace_all(&s, "\n");
    let s = close.replace_all(&s, "\n");
    let s = li.replace_all(&s, "• ");
    let s = tag.replace_all(&s, "");
    let mut text = decode(&s);
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    (text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_owned(), images)
}

fn me() -> Res<Me> {
    let d = get("_apis/connectionData?api-version=7.1-preview")?;
    let u = &d["authenticatedUser"];
    Ok(Me { email: u["properties"]["Account"]["$value"].as_str().unwrap_or_default().to_owned(), name: u["providerDisplayName"].as_str().unwrap_or_default().to_owned() })
}

fn current_sprint() -> Res<Sprint> {
    let root = get(&format!("{}/_apis/wit/classificationnodes/Iterations?$depth=2&{V}", proj()))?;
    let now = chrono::Utc::now();
    let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&chrono::Utc));
    for c in root["children"].as_array().cloned().unwrap_or_default() {
        let (Some(start), Some(finish)) = (c["attributes"]["startDate"].as_str(), c["attributes"]["finishDate"].as_str()) else { continue };
        if let (Some(a), Some(b)) = (parse(start), parse(finish)) {
            if a <= now && now < b + chrono::Duration::days(1) {
                let name = c["name"].as_str().unwrap_or_default().to_owned();
                return Ok(Sprint { path: format!("{}\\{name}", project()), name, start: start.to_owned(), finish: finish.to_owned() });
            }
        }
    }
    Err("Não encontrei o sprint atual no Azure DevOps.".to_owned())
}

fn back_event(id: i64) -> Res<Option<Back>> {
    let u = get(&format!("{}/_apis/wit/workItems/{id}/updates?{V}", proj()))?;
    let hit = u["value"].as_array().and_then(|a| {
        a.iter()
            .filter(|x| {
                let s = &x["fields"]["System.State"];
                let (old, new) = (s["oldValue"].as_str().unwrap_or(""), s["newValue"].as_str().unwrap_or(""));
                !s.is_null() && LATER_STATES.contains(&old) && !LATER_STATES.contains(&new)
            })
            .last()
            .cloned()
    });
    Ok(hit.map(|x| {
        let s = &x["fields"]["System.State"];
        Back {
            from: s["oldValue"].as_str().unwrap_or_default().to_owned(),
            to: s["newValue"].as_str().unwrap_or_default().to_owned(),
            at: x["fields"]["System.ChangedDate"]["newValue"].as_str().unwrap_or_default().to_owned(),
            by: x["revisedBy"]["displayName"].as_str().unwrap_or_default().to_owned(),
            comments: Vec::new(),
        }
    }))
}

fn comments(id: i64) -> Res<Vec<Comment>> {
    let c = get(&format!("{}/_apis/wit/workItems/{id}/comments?$top=50&{COMMENTS_V}", proj()))?;
    let mut out: Vec<Comment> = c["comments"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| {
                    let (text, images) = parse_rich(x["text"].as_str().unwrap_or_default());
                    Comment {
                        id: x["id"].as_i64().unwrap_or_default(),
                        by: x["createdBy"]["displayName"].as_str().unwrap_or_default().to_owned(),
                        at: x["createdDate"].as_str().unwrap_or_default().to_owned(),
                        text,
                        images,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

fn summarize(w: &Value, sprint_path: &str) -> Item {
    let id = w["id"].as_i64().unwrap_or_default();
    let iteration = fs_(w, "System.IterationPath");
    Item {
        id,
        url: item_url(id),
        kind: fs_(w, "System.WorkItemType"),
        title: fs_(w, "System.Title"),
        column: fs_(w, "System.BoardColumn"),
        in_sprint: sprint_path.is_empty() || iteration == sprint_path,
        effort: f(w, "Microsoft.VSTS.Scheduling.Effort").as_f64(),
        priority: f(w, "Microsoft.VSTS.Common.Priority").as_i64(),
        severity: f(w, "Microsoft.VSTS.Common.Severity").as_str().map(str::to_owned),
        tags: fs_(w, "System.Tags").split(';').map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect(),
        owner: person(w, "System.AssignedTo"),
        owner_email: f(w, "System.AssignedTo")["uniqueName"].as_str().map(str::to_lowercase),
        changed_at: fs_(w, "System.ChangedDate"),
        created_by: person(w, "System.CreatedBy"),
        child_ids: relation_ids(w, "System.LinkTypes.Hierarchy-Forward"),
        ..Default::default()
    }
}

fn summarize_task(t: &Value) -> Task {
    let id = t["id"].as_i64().unwrap_or_default();
    Task {
        id,
        url: item_url(id),
        title: fs_(t, "System.Title"),
        state: fs_(t, "System.State"),
        side: fs_(t, "System.Tags").split(';').map(str::trim).find(|x| x.eq_ignore_ascii_case("FRONT") || x.eq_ignore_ascii_case("BACK")).map(str::to_uppercase),
        estimate: f(t, "Custom.Estimate").as_f64(),
        work_done: f(t, "Custom.WorkDone").as_f64(),
        activity: f(t, "Microsoft.VSTS.Common.Activity").as_str().map(str::to_owned),
        parent_id: f(t, "System.Parent").as_i64().or_else(|| relation_ids(t, "System.LinkTypes.Hierarchy-Reverse").first().copied()),
        parent: None,
    }
}

fn query_base() -> String {
    let c = cfg();
    let area = if c.area.is_empty() { String::new() } else { format!(" AND [System.AreaPath] UNDER '{}'", c.area) };
    format!("[System.TeamProject]='{}'{area} AND [System.WorkItemType] <> 'Task'", c.project)
}

// My In Progress, then Planned (any sprint: items sent back by QA lose theirs), plus unowned ones in this sprint.
fn now_items(sprint: &Sprint, email: &str) -> Res<Vec<Item>> {
    let ids = wiql(
        &format!(
            "SELECT [System.Id] FROM WorkItems WHERE {} AND [System.BoardColumn] IN ({}) AND ([System.AssignedTo]=@Me OR ([System.AssignedTo]='' AND [System.IterationPath]='{}')) ORDER BY [Microsoft.VSTS.Common.BacklogPriority]",
            query_base(),
            cfg().working.iter().map(|c| format!("'{c}'")).collect::<Vec<_>>().join(","),
            sprint.path
        ),
        200,
    )?;
    let mut items: Vec<Item> = get_items(&ids, true)?.iter().map(|w| summarize(w, &sprint.path)).filter(|x| x.owner_email.as_deref().map_or(true, |e| e == email)).collect();
    let working = cfg().working;
    items.sort_by_key(|x| (working.iter().position(|c| *c == x.column).unwrap_or(working.len()), if x.owner.is_some() { 0 } else { 1 }));
    thread::scope(|s| {
        for it in items.iter_mut() {
            s.spawn(move || {
                let back = back_event(it.id).ok().flatten();
                let all = comments(it.id).unwrap_or_default();
                it.last_comment = all.last().cloned();
                it.back = back.map(|mut b| {
                    let day = b.at.get(..10).unwrap_or("").to_owned();
                    b.comments = all.iter().filter(|c| c.at.as_str() >= day.as_str()).cloned().collect();
                    b
                });
            });
        }
    });
    Ok(items)
}

// QA columns pile up across sprints; keep this sprint's plus anything touched in the last two weeks.
fn review_items(sprint: &Sprint) -> Res<Vec<Item>> {
    let ids = wiql(
        &format!(
            "SELECT [System.Id] FROM WorkItems WHERE {} AND [System.AssignedTo]=@Me AND [System.BoardColumn] IN ('Code Review','To Test','Testing') AND ([System.IterationPath]='{}' OR [System.ChangedDate] >= @Today-14) ORDER BY [System.ChangedDate] DESC",
            query_base(),
            sprint.path
        ),
        200,
    )?;
    let mut items: Vec<Item> = get_items(&ids, true)?.iter().map(|w| summarize(w, &sprint.path)).collect();
    thread::scope(|s| {
        for chunk in items.chunks_mut(8) {
            s.spawn(move || {
                for it in chunk {
                    it.last_comment = comments(it.id).ok().and_then(|c| c.last().cloned());
                }
            });
        }
    });
    Ok(items)
}

fn refined_items(sprint: &Sprint) -> Res<Vec<Item>> {
    let ids = wiql(
        &format!("SELECT [System.Id] FROM WorkItems WHERE {} AND [System.BoardColumn]='{}' AND [System.AssignedTo]='' ORDER BY [Microsoft.VSTS.Common.BacklogPriority]", query_base(), cfg().available),
        100,
    )?;
    let by_id: HashMap<i64, Item> = get_items(&ids, false)?.iter().map(|w| summarize(w, &sprint.path)).map(|i| (i.id, i)).collect();
    Ok(ids.iter().filter_map(|i| by_id.get(i).cloned()).collect())
}

/// Refined items already assigned to me (taken, not started).
fn mine_refined_items(sprint: &Sprint) -> Res<Vec<Item>> {
    let ids = wiql(
        &format!("SELECT [System.Id] FROM WorkItems WHERE {} AND [System.BoardColumn]='{}' AND [System.AssignedTo]=@Me ORDER BY [Microsoft.VSTS.Common.BacklogPriority]", query_base(), cfg().available),
        100,
    )?;
    let by_id: HashMap<i64, Item> = get_items(&ids, false)?.iter().map(|w| summarize(w, &sprint.path)).map(|i| (i.id, i)).collect();
    Ok(ids.iter().filter_map(|i| by_id.get(i).cloned()).collect())
}

fn sprint_tasks(sprint: &Sprint) -> Res<Vec<Task>> {
    let ids = wiql(
        &format!(
            "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject]='{}' AND [System.WorkItemType]='Task' AND [System.AssignedTo]=@Me AND [System.IterationPath]='{}' AND [System.State] <> 'Removed' ORDER BY [System.Id] DESC",
            project(),
            sprint.path
        ),
        500,
    )?;
    Ok(get_items(&ids, true)?.iter().map(summarize_task).collect())
}

pub fn dashboard() -> Res<Dashboard> {
    let (who, sprint) = thread::scope(|s| {
        let a = s.spawn(me);
        let b = s.spawn(current_sprint);
        (a.join().unwrap_or_else(|_| Err("falhou".into())), b.join().unwrap_or_else(|_| Err("falhou".into())))
    });
    let (who, sprint) = (who?, sprint?);
    let email = who.email.to_lowercase();
    fn join<T>(h: thread::ScopedJoinHandle<'_, Res<T>>) -> Res<T> {
        h.join().unwrap_or_else(|_| Err("Falhou a leitura do Azure DevOps.".to_owned()))
    }
    let (now, review, refined, tasks, mine_refined) = thread::scope(|s| {
        let a = s.spawn(|| now_items(&sprint, &email));
        let b = s.spawn(|| review_items(&sprint));
        let c = s.spawn(|| refined_items(&sprint));
        let d = s.spawn(|| sprint_tasks(&sprint));
        let e = s.spawn(|| mine_refined_items(&sprint));
        (join(a), join(b), join(c), join(d), join(e))
    });
    let (mut now, mut review, refined, mut tasks, mine_refined) = (now?, review?, refined?, tasks?, mine_refined?);

    let mut kid_ids: Vec<i64> = now.iter().chain(review.iter()).flat_map(|i| i.child_ids.clone()).collect();
    kid_ids.sort_unstable();
    kid_ids.dedup();
    let kids: HashMap<i64, Task> = get_items(&kid_ids, false)?.iter().filter(|t| fs_(t, "System.WorkItemType") == "Task").map(summarize_task).map(|t| (t.id, t)).collect();
    for it in now.iter_mut().chain(review.iter_mut()) {
        it.tasks = it.child_ids.iter().filter_map(|id| kids.get(id).cloned()).collect();
    }

    let mut parent_ids: Vec<i64> = tasks.iter().filter_map(|t| t.parent_id).collect();
    parent_ids.sort_unstable();
    parent_ids.dedup();
    let parents: HashMap<i64, Parent> = get_items(&parent_ids, false)?
        .iter()
        .map(|w| {
            let id = w["id"].as_i64().unwrap_or_default();
            (id, Parent { id, title: fs_(w, "System.Title"), column: fs_(w, "System.BoardColumn"), url: item_url(id) })
        })
        .collect();
    for t in tasks.iter_mut() {
        t.parent = t.parent_id.and_then(|p| parents.get(&p).cloned());
    }
    Ok(Dashboard { me: who, sprint, now, review, refined, mine_refined, tasks })
}


/// Opening an item's conversation claims it: assigned to me, in the current sprint, and Planned if it was still before
/// Planned on the board. Never takes an item from someone else and never moves it backwards. Returns what it did.
pub fn claim(id: i64, me: &Me, sprint: &Sprint) -> Res<String> {
    let w = get_items(&[id], false)?.into_iter().next().ok_or_else(|| format!("Não encontrei o #{id}."))?;
    let owner = f(&w, "System.AssignedTo");
    let my_email = me.email.to_lowercase();
    if !owner.is_null() {
        let mine = owner["uniqueName"].as_str().is_some_and(|u| u.to_lowercase() == my_email);
        if !mine {
            return Ok(format!("O #{id} está atribuído a {}: não lhe mexi.", owner["displayName"].as_str().unwrap_or("outra pessoa")));
        }
    }
    let mut ops = Vec::new();
    let mut done = Vec::new();
    if owner.is_null() && !my_email.is_empty() {
        ops.push(json!({ "op": "add", "path": "/fields/System.AssignedTo", "value": me.email }));
        done.push("atribuí-to".to_owned());
    }
    if !sprint.path.is_empty() && fs_(&w, "System.IterationPath") != sprint.path {
        ops.push(json!({ "op": "add", "path": "/fields/System.IterationPath", "value": sprint.path }));
        done.push(format!("pus no {}", sprint.name));
    }
    let c = cfg();
    if c.before_claim.contains(&fs_(&w, "System.BoardColumn")) {
        ops.push(json!({ "op": "add", "path": "/fields/System.State", "value": c.claimed_state }));
        done.push(format!("passei para {}", c.claimed_state));
    }
    if ops.is_empty() {
        return Ok(String::new());
    }
    request("PATCH", &format!("{}/_apis/wit/workitems/{id}?{V}", proj()), Some(Value::Array(ops)))?;
    let text = done.join(", ");
    let mut cs = text.chars();
    let first = cs.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    Ok(format!("#{id}: {first}{}.", cs.as_str()))
}

/// GET relative to the organization (for other modules: PRs, cleanup).
pub fn get_json(path: &str) -> Res<Value> {
    get(path)
}

/// POST / PUT with a JSON body, relative to the organization (PR comments and votes).
pub fn send_json(method: &str, path: &str, body: Value) -> Res<Value> {
    request(method, path, Some(body))
}

pub fn project_path(rest: &str) -> String {
    format!("{}/{rest}{}{V}", proj(), if rest.contains('?') { "&" } else { "?" })
}

/// Raw work items (fields only), e.g. to know the column of the items behind some worktrees.
pub fn items_raw(ids: &[i64]) -> Res<Vec<Value>> {
    get_items(ids, false)
}

/// Sets fields on a work item (json-patch "add").
pub fn set_fields(id: i64, fields: &[(&str, Value)]) -> Res<()> {
    let ops: Vec<Value> = fields.iter().map(|(k, v)| json!({ "op": "add", "path": format!("/fields/{k}"), "value": v })).collect();
    request("PATCH", &format!("{}/_apis/wit/workitems/{id}?{V}", proj()), Some(Value::Array(ops))).map(|_| ())
}

pub fn web_url(path: &str) -> String {
    format!("{}/{}/{path}", org(), proj())
}

/// Screenshots only (`JARVIS_SHOT_DEMO=1`): a made-up board, so the Tarefas page can be shown without a connection.
pub fn demo() -> Option<Dashboard> {
    std::env::var("JARVIS_SHOT_DEMO").ok()?;
    let item = |id: i64, kind: &str, title: &str, column: &str, owner: Option<&str>, effort: Option<f64>, in_sprint: bool| Item {
        id,
        url: format!("https://dev.azure.com/demo/_workitems/edit/{id}"),
        kind: kind.into(),
        title: title.into(),
        column: column.into(),
        in_sprint,
        effort,
        priority: Some(2),
        owner: owner.map(str::to_owned),
        created_by: Some("Ana Silva".into()),
        changed_at: "2026-11-20T10:00:00Z".into(),
        ..Default::default()
    };
    let me = "Rita Demo";
    let mut doing = item(9101, "Bug", "Localização continua disabled ao manter no fornecedor", "In Progress", Some(me), Some(3.0), true);
    doing.tasks = vec![Task { id: 9201, title: "Corrigir o estado do campo".into(), state: "Active".into(), side: Some("FRONT".into()), estimate: Some(3.0), work_done: Some(1.0), activity: Some("Development".into()), ..Default::default() }];
    let tasks = vec![
        Task { parent_id: Some(9101), parent: Some(Parent { id: 9101, title: doing.title.clone(), column: "In Progress".into(), url: String::new() }), ..doing.tasks[0].clone() },
        Task { id: 9202, title: "Testar no browser".into(), state: "Done".into(), side: Some("BACK".into()), estimate: Some(1.0), work_done: None, activity: None, parent_id: Some(9101), parent: Some(Parent { id: 9101, title: doing.title.clone(), column: "In Progress".into(), url: String::new() }), ..Default::default() },
    ];
    Some(Dashboard {
        me: Me { email: "rita@demo.pt".into(), name: me.into() },
        sprint: Sprint { path: "Demo Sprint 42".into(), name: "Sprint 42".into(), start: "2026-11-16T00:00:00Z".into(), finish: "2026-11-27T00:00:00Z".into() },
        now: vec![
            doing,
            item(9102, "PBI", "Exportar a lista de fornecedores para Excel", "Planned", Some(me), Some(5.0), true),
            item(9103, "Improvement", "Mostrar o NIF na ficha do cliente", "Planned", None, Some(2.0), true),
        ],
        review: vec![item(9090, "Bug", "Total do documento sem IVA arredondado", "Code Review", Some(me), Some(2.0), true)],
        refined: vec![item(9110, "Bug", "Pesquisa ignora acentos nos artigos", "Refined", None, Some(3.0), false), item(9111, "PBI", "Filtro por estado nas encomendas", "Refined", None, None, false)],
        mine_refined: vec![item(9105, "PBI", "Alertas de stock mínimo por armazém", "Refined", Some(me), Some(8.0), false)],
        tasks,
    })
}
