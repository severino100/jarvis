//! My pull requests in Azure Repos: reviewers' votes, comment threads, the ones finished recently, and "Corrigir"
//! (opens a conversation with the review comment to fix it). Needs a PAT with Code (Read).

use crate::{
    azdo,
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use serde::Deserialize;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct PrComment {
    pub id: i64,
    pub by: String,
    pub by_id: String,
    pub at: String,
    pub text: String,
}

#[derive(Clone, Default)]
pub struct Thread {
    pub id: i64,
    pub open: bool,
    pub file: Option<String>,
    pub line: Option<i64>,
    pub comments: Vec<PrComment>,
}

#[derive(Clone, Default)]
pub struct Reviewer {
    pub name: String,
    pub vote: i64,
    pub label: String,
}

#[derive(Clone, Default)]
pub struct Pr {
    pub key: String,
    pub id: i64,
    pub repo: String,
    pub title: String,
    pub is_draft: bool,
    /// Opened by me (the "Os meus" list).
    pub mine: bool,
    pub author: String,
    /// My vote on it (10, 5, 0, -5, -10), when I am a reviewer.
    pub my_vote: i64,
    pub source: String,
    pub target: String,
    pub created_at: String,
    pub url: String,
    pub reviewers: Vec<Reviewer>,
    pub item_ids: Vec<i64>,
    pub threads: Vec<Thread>,
    /// Open threads with at least one comment by someone else.
    pub open_threads: Vec<Thread>,
}

#[derive(Clone, Default)]
pub struct Recent {
    pub id: i64,
    pub repo: String,
    pub title: String,
    pub closed_at: String,
    pub url: String,
}

#[derive(Clone, Default)]
pub struct PrData {
    pub active: Vec<Pr>,
    /// Other people's open PRs where I am a reviewer.
    pub to_review: Vec<Pr>,
    pub recent: Vec<Recent>,
}

#[derive(Default)]
pub struct PrState {
    pub data: Option<PrData>,
    pub error: Option<String>,
    pub loading: bool,
    pub fetched_at: Option<Instant>,
    /// Bumped on every successful load, so the app diffs each load once.
    pub version: u64,
}

#[derive(Clone)]
pub struct Prs {
    state: Arc<Mutex<PrState>>,
    ctx: egui::Context,
}

fn vote_label(v: i64) -> &'static str {
    match v {
        10 => "Aprovado",
        5 => "Aprovado com sugestões",
        0 => "Sem voto",
        -5 => "Aguarda o autor",
        -10 => "Rejeitado",
        _ => "?",
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn my_id() -> azdo::Res<String> {
    static ID: OnceLock<String> = OnceLock::new();
    if let Some(id) = ID.get() {
        return Ok(id.clone());
    }
    let d = azdo::get_json("_apis/connectionData?api-version=7.1-preview")?;
    let id = s(&d["authenticatedUser"]["id"]);
    let _ = ID.set(id.clone());
    Ok(id)
}

fn thread_of(t: &Value) -> Option<Thread> {
    let comments: Vec<PrComment> = t["comments"]
        .as_array()?
        .iter()
        .filter(|c| c["commentType"] != "system" && c["isDeleted"] != true && c["content"].as_str().is_some_and(|x| !x.is_empty()))
        .map(|c| PrComment {
            id: c["id"].as_i64().unwrap_or(0),
            by: c["author"]["displayName"].as_str().unwrap_or("?").to_owned(),
            by_id: s(&c["author"]["id"]),
            at: s(&c["publishedDate"]),
            text: s(&c["content"]).chars().take(4000).collect(),
        })
        .collect();
    if comments.is_empty() {
        return None;
    }
    let status = &t["status"];
    let open = matches!(status.as_str(), Some("active" | "pending")) || matches!(status.as_i64(), Some(1 | 6));
    let ctx = &t["threadContext"];
    Some(Thread {
        id: t["id"].as_i64().unwrap_or(0),
        open,
        file: ctx["filePath"].as_str().map(str::to_owned),
        line: ctx["rightFileStart"]["line"].as_i64().or_else(|| ctx["leftFileStart"]["line"].as_i64()),
        comments,
    })
}

fn branch(r: &Value) -> String {
    s(r).trim_start_matches("refs/heads/").to_owned()
}

fn details(pr: &Value, mine: &str) -> Pr {
    let repo_id = s(&pr["repository"]["id"]);
    let id = pr["pullRequestId"].as_i64().unwrap_or(0);
    let base = format!("_apis/git/repositories/{repo_id}/pullRequests/{id}");
    let (threads, items) = thread::scope(|sc| {
        let t = sc.spawn(|| azdo::get_json(&azdo::project_path(&format!("{base}/threads"))).map(|r| r["value"].as_array().cloned().unwrap_or_default()).unwrap_or_default());
        let i = sc.spawn(|| azdo::get_json(&azdo::project_path(&format!("{base}/workitems"))).map(|r| r["value"].as_array().cloned().unwrap_or_default()).unwrap_or_default());
        (t.join().unwrap_or_default(), i.join().unwrap_or_default())
    });
    let threads: Vec<Thread> = threads.iter().filter_map(thread_of).collect();
    let open_threads = threads.iter().filter(|t| t.open && t.comments.iter().any(|c| c.by_id != mine)).cloned().collect();
    let repo = s(&pr["repository"]["name"]);
    Pr {
        key: format!("{repo_id}:{id}"),
        id,
        url: azdo::web_url(&format!("_git/{}/pullrequest/{id}", repo.replace(' ', "%20"))),
        repo,
        title: s(&pr["title"]),
        is_draft: pr["isDraft"] == true,
        mine: s(&pr["createdBy"]["id"]) == mine,
        author: s(&pr["createdBy"]["displayName"]),
        my_vote: pr["reviewers"].as_array().and_then(|rs| rs.iter().find(|r| s(&r["id"]) == mine)).and_then(|r| r["vote"].as_i64()).unwrap_or(0),
        source: branch(&pr["sourceRefName"]),
        target: branch(&pr["targetRefName"]),
        created_at: s(&pr["creationDate"]),
        reviewers: pr["reviewers"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let vote = r["vote"].as_i64().unwrap_or(0);
                Reviewer { name: s(&r["displayName"]), vote, label: vote_label(vote).to_owned() }
            })
            .collect(),
        item_ids: items.iter().filter_map(|w| w["id"].as_str().and_then(|x| x.parse().ok()).or_else(|| w["id"].as_i64())).collect(),
        threads,
        open_threads,
    }
}

fn load() -> azdo::Res<PrData> {
    let mine = my_id()?;
    let q = |status: &str, top: u32| {
        azdo::get_json(&azdo::project_path(&format!("_apis/git/pullrequests?searchCriteria.creatorId={mine}&searchCriteria.status={status}&$top={top}")))
            .map(|r| r["value"].as_array().cloned().unwrap_or_default())
    };
    let (active, done, asked) = thread::scope(|sc| {
        let a = sc.spawn(|| q("active", 30));
        let d = sc.spawn(|| q("completed", 20));
        // The ones where I am a reviewer; a failure here must not hide my own PRs.
        let r = sc.spawn(|| {
            azdo::get_json(&azdo::project_path(&format!("_apis/git/pullrequests?searchCriteria.reviewerId={mine}&searchCriteria.status=active&$top=30")))
                .map(|r| r["value"].as_array().cloned().unwrap_or_default())
                .unwrap_or_default()
        });
        (a.join().unwrap(), d.join().unwrap(), r.join().unwrap_or_default())
    });
    let active = active?;
    let asked: Vec<Value> = asked.into_iter().filter(|pr| s(&pr["createdBy"]["id"]) != mine).collect();
    let (active, to_review) = thread::scope(|sc| {
        let hs: Vec<_> = active.iter().map(|pr| sc.spawn(|| details(pr, &mine))).collect();
        let rs: Vec<_> = asked.iter().map(|pr| sc.spawn(|| details(pr, &mine))).collect();
        (hs.into_iter().filter_map(|h| h.join().ok()).collect(), rs.into_iter().filter_map(|h| h.join().ok()).collect())
    });
    let recent = done?
        .iter()
        .map(|pr| {
            let repo = s(&pr["repository"]["name"]);
            let id = pr["pullRequestId"].as_i64().unwrap_or(0);
            Recent { id, url: azdo::web_url(&format!("_git/{}/pullrequest/{id}", repo.replace(' ', "%20"))), repo, title: s(&pr["title"]), closed_at: s(&pr["closedDate"]) }
        })
        .collect();
    Ok(PrData { active, to_review, recent })
}

impl Prs {
    pub fn start(ctx: egui::Context, poll_minutes: u32) -> Self {
        let me = Self { state: Arc::default(), ctx };
        me.refresh();
        let p = me.clone();
        let every = Duration::from_secs(60 * u64::from(poll_minutes.max(1)));
        thread::spawn(move || loop {
            thread::sleep(every);
            p.refresh();
        });
        me
    }

    /// Safe to call from inside `with` (the PRs view does): the state is only locked on the background thread.
    pub fn refresh(&self) {
        if let Some(d) = demo() {
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
            let r = load();
            let mut s = state.lock().unwrap();
            match r {
                Ok(d) => {
                    s.data = Some(d);
                    s.error = None;
                    s.fetched_at = Some(Instant::now());
                    s.version += 1;
                }
                Err(e) => {
                    s.error = Some(if e.contains("401") || e.contains("403") || e.contains("recusado") {
                        "O token do Azure DevOps não tem acesso aos repositórios. Gera um novo com Work Items (Read & Write) e Code (Read) e guarda-o em ~/.config/azdo/pat.".into()
                    } else {
                        e
                    })
                }
            }
            s.loading = false;
            drop(s);
            ctx.request_repaint();
        });
    }

    pub fn with<R>(&self, f: impl FnOnce(&PrState) -> R) -> R {
        f(&self.state.lock().unwrap())
    }
}

/// Where the PR's repository lives: always under the projects folder set in Definições (never a guessed default).
fn repo_path(pr: &Pr) -> std::path::PathBuf {
    let base = crate::profile::projects_dir();
    let dir = crate::profile::repos().into_iter().find(|r| r.dir.eq_ignore_ascii_case(&pr.repo)).map(|r| r.dir).unwrap_or_else(|| pr.repo.clone());
    base.join(dir)
}

/// What Claude gets to fix review comments: the PR, the branch and every open comment (or just one thread).
pub fn fix_prompt(pr: &Pr, thread_id: Option<i64>) -> String {
    let threads: Vec<&Thread> = match thread_id {
        Some(t) => pr.threads.iter().filter(|x| x.id == t).collect(),
        None => pr.open_threads.iter().collect(),
    };
    let body = threads
        .iter()
        .map(|t| {
            let place = match (&t.file, t.line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                (Some(f), None) => f.clone(),
                _ => "Comentário geral".into(),
            };
            let cs = t.comments.iter().map(|c| format!("  > {}: {}", c.by, c.text.replace('\n', "\n  > "))).collect::<Vec<_>>().join("\n");
            format!("- {place}\n{cs}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let items = if pr.item_ids.is_empty() { String::new() } else { format!(", itens {}", pr.item_ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(", ")) };
    format!(
        "Review do PR !{} \"{}\" (repositório {}, branch {} → {}{items}).\n\n{}:\n{body}\n\nO repositório está em {} (a pasta dos projetos definida nas Definições; não procures noutro sítio). Analisa no código do branch {} (usa o worktree desse branch se existir; se não existir, prepara um a partir de origin/{} só depois do meu sim), explica o que o revisor pede e propõe a correção. Quando eu aprovar, prepara o pacote de passagem e termina com a linha de implementar.",
        pr.id,
        pr.title,
        pr.repo,
        pr.source,
        pr.target,
        if threads.len() > 1 { "Comentários por resolver" } else { "Comentário" },
        repo_path(pr).display(),
        pr.source,
        pr.source
    )
}

/* ---------- review: the AI reads the PR, I approve, Jarvis publishes ---------- */

/// What the AI gets to review a PR: the PR, the comments already there, the team's review rules (the pack's
/// `[guides] review`) and the exact format of the draft it must end with. It publishes nothing itself.
pub fn review_prompt(pr: &Pr) -> String {
    let rules = crate::profile::read_guide(crate::profile::Guide::Review);
    let rules = if rules.is_empty() {
        "Esta equipa não definiu regras de review: usa o CLAUDE.md e as convenções do próprio repositório, e o bom senso (correção, segurança, testes, legibilidade).".to_owned()
    } else {
        rules
    };
    let existing: Vec<String> = pr
        .threads
        .iter()
        .map(|t| {
            let place = match (&t.file, t.line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                (Some(f), None) => f.clone(),
                _ => "geral".into(),
            };
            let cs = t.comments.iter().map(|c| format!("  > {}: {}", c.by, c.text.replace('\n', " "))).collect::<Vec<_>>().join("\n");
            format!("- {place} ({})\n{cs}", if t.open { "por resolver" } else { "resolvido" })
        })
        .collect();
    let existing = if existing.is_empty() { "Ainda não há comentários.".to_owned() } else { existing.join("\n") };
    let items = if pr.item_ids.is_empty() { String::new() } else { format!(", itens {}", pr.item_ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(", ")) };
    let repo_dir = repo_path(pr);
    // My own PR (usually a draft): a review before I publish it, comments only, never a vote on my own work.
    let own = if pr.mine {
        "\n\nEste PR é meu (em rascunho, se for o caso): é uma revisão antes de o publicar. Aponta o que devo corrigir antes de pedir review à equipa; no bloco final usa sempre `\"vote\":0` (não voto no meu próprio PR)."
    } else {
        ""
    };
    format!(
        "Faz o review do PR !{id} \"{title}\" de {author} (repositório {repo}, branch {source} → {target}{items}).{own}\n\n\
## Regras de review do repositório\n{rules}\n\n\
## Comentários que já lá estão\n{existing}\n\n\
## Como trabalhar\n\
- Lê o diff sem mexer em nada: o repositório deve estar em {dir}; faz `git fetch origin {source} {target}` e `git diff origin/{target}...origin/{source}` (só leitura: sem checkout, sem commits, sem push). Se não o encontrares, diz-me em vez de adivinhar. Lê também os itens do PR.\n\
- Confronta o diff com as regras acima. Diz-me primeiro, em texto, o que está bem e o que preocupa, por ordem de importância.\n\
- Não publiques nada no PR nem votes: quem publica sou eu, depois de aprovar o rascunho.\n\n\
## Termina com o rascunho\n\
Acaba a resposta com este bloco exato (JSON válido; `vote` é 10 aprovar, 5 aprovar com sugestões, 0 sem voto, -5 aguardar o autor, -10 rejeitar; `file` com / à frente e `line` só quando o comentário é sobre uma linha do código novo, senão omite-os):\n\
[[REVIEW]]\n{{\"pr\":\"{key}\",\"vote\":5,\"summary\":\"resumo geral, curto\",\"comments\":[{{\"file\":\"/src/exemplo.ts\",\"line\":12,\"text\":\"o comentário\"}}]}}\n[[/REVIEW]]\n\
Só comentários que valem a pena, claros e educados, em português de Portugal. Se eu pedir alterações, devolve o bloco outra vez, atualizado.",
        id = pr.id,
        title = pr.title,
        author = if pr.mine { "mim" } else if pr.author.is_empty() { "alguém" } else { &pr.author },
        repo = pr.repo,
        source = pr.source,
        target = pr.target,
        dir = repo_dir.display(),
        key = pr.key,
    )
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct DraftComment {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    #[serde(default)]
    pub text: String,
}

/// The review the AI drafted ([[REVIEW]] … [[/REVIEW]]): vote, a general summary and the comments to leave.
#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct ReviewDraft {
    #[serde(default)]
    pub pr: String,
    #[serde(default)]
    pub vote: i64,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub comments: Vec<DraftComment>,
}

/// Splits a reply into prose and review drafts, in order. A block still arriving, or with invalid JSON, stays text.
pub fn split_reviews(text: &str) -> Vec<Result<ReviewDraft, String>> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?s)\[\[REVIEW\]\](.*?)\[\[/REVIEW\]\]").unwrap());
    let mut out = Vec::new();
    let mut last = 0;
    for m in re.captures_iter(text) {
        let whole = m.get(0).unwrap();
        let body = m[1].trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        let Ok(mut d) = serde_json::from_str::<ReviewDraft>(body) else { continue };
        d.vote = [10, 5, 0, -5, -10].into_iter().min_by_key(|v| (v - d.vote).abs()).unwrap_or(0);
        d.comments.retain(|c| !c.text.trim().is_empty());
        let before = text[last..whole.start()].trim();
        if !before.is_empty() {
            out.push(Err(before.to_owned()));
        }
        out.push(Ok(d));
        last = whole.end();
    }
    let rest = text[last..].trim();
    if !rest.is_empty() {
        out.push(Err(rest.to_owned()));
    }
    out
}

pub fn vote_name(v: i64) -> &'static str {
    vote_label(v)
}

/// Already published drafts (so a card never posts twice, also after a restart).
fn published() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| {
        let file = crate::settings::data_dir().join("reviews-publicadas.txt");
        Mutex::new(std::fs::read_to_string(file).unwrap_or_default().lines().map(str::to_owned).collect())
    })
}

/// A key for the draft as it was when it was shown (PR + content).
pub fn draft_key(pr_key: &str, d: &ReviewDraft) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (d.vote, &d.summary, d.comments.iter().map(|c| (&c.file, c.line, &c.text)).collect::<Vec<_>>()).hash(&mut h);
    format!("{pr_key}#{:x}", h.finish())
}

pub fn is_published(key: &str) -> bool {
    published().lock().unwrap().contains(key)
}

/// Where each publish is: None (not started), Some(Ok(text)) when done, Some(Err(text)) when it failed, "…" while running.
fn progress() -> &'static Mutex<std::collections::HashMap<String, Result<String, String>>> {
    static S: OnceLock<Mutex<std::collections::HashMap<String, Result<String, String>>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn publish_state(key: &str) -> Option<Result<String, String>> {
    progress().lock().unwrap().get(key).cloned()
}

/// Posts the approved review: one thread per comment (on the file line when it has one), the summary as a general
/// thread, and my vote. Runs off the UI thread; `key` is `draft_key` of what was approved.
pub fn publish(key: String, pr_key: String, d: ReviewDraft, ctx: egui::Context) {
    if is_published(&key) || publish_state(&key).is_some_and(|r| r.is_ok_and(|t| t == "…")) {
        return;
    }
    progress().lock().unwrap().insert(key.clone(), Ok("…".into()));
    thread::spawn(move || {
        let r = post_review(&pr_key, &d);
        if r.is_ok() {
            let mut p = published().lock().unwrap();
            p.insert(key.clone());
            let _ = std::fs::create_dir_all(crate::settings::data_dir());
            let _ = std::fs::write(crate::settings::data_dir().join("reviews-publicadas.txt"), p.iter().cloned().collect::<Vec<_>>().join("\n"));
        }
        progress().lock().unwrap().insert(key, r);
        ctx.request_repaint();
    });
}

fn post_review(pr_key: &str, d: &ReviewDraft) -> Result<String, String> {
    let (repo, id) = pr_key.split_once(':').ok_or("PR desconhecido.")?;
    let threads = azdo::project_path(&format!("_apis/git/repositories/{repo}/pullRequests/{id}/threads"));
    let mut posted = 0;
    let mut post = |text: &str, at: Option<(&str, i64)>| -> Result<(), String> {
        let mut body = serde_json::json!({ "comments": [{ "parentCommentId": 0, "content": text, "commentType": 1 }] });
        match at {
            Some((file, line)) => {
                let file = if file.starts_with('/') { file.to_owned() } else { format!("/{file}") };
                body["status"] = 1.into();
                body["threadContext"] = serde_json::json!({ "filePath": file, "rightFileStart": { "line": line, "offset": 1 }, "rightFileEnd": { "line": line, "offset": 1 } });
            }
            None => {
                body["status"] = 1.into();
            }
        }
        azdo::send_json("POST", &threads, body).map_err(|e| format!("O Azure DevOps recusou um comentário: {}", e.lines().next().unwrap_or_default()))?;
        posted += 1;
        Ok(())
    };
    if !d.summary.trim().is_empty() {
        post(d.summary.trim(), None)?;
    }
    for c in &d.comments {
        post(c.text.trim(), c.file.as_deref().filter(|f| !f.trim().is_empty()).zip(c.line.filter(|l| *l > 0)))?;
    }
    let voted = if d.vote != 0 {
        let me = my_id()?;
        azdo::send_json("PUT", &azdo::project_path(&format!("_apis/git/repositories/{repo}/pullRequests/{id}/reviewers/{me}")), serde_json::json!({ "vote": d.vote }))
            .map_err(|e| format!("Os comentários ficaram, mas o voto foi recusado: {}", e.lines().next().unwrap_or_default()))?;
        format!(" Voto: {}.", vote_label(d.vote))
    } else {
        String::new()
    };
    Ok(format!("Publicado: {posted} comentário{}.{voted}", if posted == 1 { "" } else { "s" }))
}

/// "Rever" anywhere: the app picks it up after the frame (PR key).
fn request_review(ui: &Ui, key: &str) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("jarvis-prreview"), key.to_owned()));
}

pub fn take_review(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::unique("jarvis-prreview")))
}

/// "Corrigir" anywhere: the app picks it up after the frame (PR key, optional thread).
fn request_fix(ui: &Ui, key: &str, thread: Option<i64>) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("jarvis-prfix"), (key.to_owned(), thread)));
}

pub fn take_fix(ctx: &egui::Context) -> Option<(String, Option<i64>)> {
    ctx.data_mut(|d| d.remove_temp::<(String, Option<i64>)>(egui::Id::unique("jarvis-prfix")))
}

fn vote_color(v: i64, acc: Accent) -> egui::Color32 {
    match v {
        10 | 5 => theme::GREEN,
        -5 => theme::AMBER,
        -10 => theme::RED,
        _ => acc.light,
    }
}

pub fn view(ui: &mut Ui, prs: &Prs, acc: Accent) {
    prs.with(|s| {
        ui.horizontal(|ui| {
            if button(ui, "Atualizar", false, acc).clicked() {
                prs.refresh();
            }
            if s.loading {
                crate::motion::spinner(ui, 14.0 + 2.0, acc.main);
            }
        });
        ui.add_space(8.0);
        if let Some(e) = &s.error {
            ui_kit::hint(ui, e, true, acc);
            ui.add_space(8.0);
        }
        let Some(d) = &s.data else {
            if s.error.is_none() {
                ui_kit::quiet(ui, "A ler os teus PRs…", acc);
            }
            return;
        };
        // The numbers first, like a dashboard; the cards follow.
        let waiting: usize = d.active.iter().map(|p| p.open_threads.len()).sum();
        let approved = d.active.iter().filter(|p| p.reviewers.iter().any(|r| r.vote >= 5)).count();
        let tiles = [
            ("Para rever", d.to_review.len().to_string(), format!("{} sem o teu voto", d.to_review.iter().filter(|p| p.my_vote == 0).count()), false),
            ("Os meus abertos", d.active.len().to_string(), format!("{approved} com aprovação"), false),
            ("Comentários por resolver", waiting.to_string(), if waiting > 0 { "nos teus PRs".to_owned() } else { "nada à espera".to_owned() }, waiting > 0),
            ("Fechados", d.recent.len().to_string(), "recentemente".to_owned(), false),
        ];
        ui.columns(tiles.len(), |cols| {
            for (col, (label, value, sub, warn)) in cols.iter_mut().zip(tiles) {
                ui_kit::hud_panel(col, acc, warn, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(label.to_uppercase()).font(display(11.0)).color(theme::MUTED).extra_letter_spacing(1.2));
                    ui.label(RichText::new(value).monospace().size(theme::size::STAT).strong().color(if warn { theme::AMBER } else { acc.light }));
                    ui.add(egui::Label::new(RichText::new(sub).small().color(theme::MUTED)).truncate());
                });
            }
        });
        ui.add_space(12.0);
        if !d.to_review.is_empty() {
            ui_kit::section_label(ui, &format!("Para rever · {}", d.to_review.len()), acc.main, acc);
            for pr in &d.to_review {
                ui_kit::hud_panel(ui, acc, false, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("!{}", pr.id)).monospace().color(acc.main));
                        ui_kit::side_pill(ui, &pr.repo, acc);
                        if pr.is_draft {
                            ui_kit::warn_pill(ui, "Rascunho");
                        }
                        for i in &pr.item_ids {
                            ui_kit::id_link(ui, *i, &azdo::item_url(*i), acc);
                        }
                        if pr.my_vote != 0 {
                            let c = vote_color(pr.my_vote, acc);
                            ui_kit::pill(ui, &format!("O teu voto · {}", vote_label(pr.my_vote)), c, theme::alpha(c, 0.1), theme::alpha(c, 0.35));
                        }
                    });
                    ui.label(RichText::new(&pr.title).size(theme::size::CARD).strong().color(theme::BRIGHT));
                    ui.label(RichText::new(format!("{} · {} → {} · aberto {}", pr.author, pr.source, pr.target, ui_kit::when(&pr.created_at))).monospace().size(12.0).color(theme::MUTED));
                    let open = pr.open_threads.len();
                    if open > 0 {
                        ui.label(RichText::new(format!("{open} comentário{} por resolver", if open > 1 { "s" } else { "" })).small().color(theme::AMBER));
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if button(ui, "Rever com o Jarvis", true, acc).on_hover_text("Abre uma conversa: o Jarvis lê o diff com as regras da equipa e propõe os comentários e o voto. Só publica depois de aprovares.").clicked() {
                            request_review(ui, &pr.key);
                        }
                        if button(ui, "Abrir o PR", false, acc).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&pr.url));
                        }
                    });
                });
                ui.add_space(10.0);
            }
        }
        ui_kit::section_label(ui, &format!("Os meus · {}", d.active.len()), acc.main, acc);
        if d.active.is_empty() {
            ui_kit::quiet(ui, "Não tens PRs abertos.", acc);
        }
        for pr in &d.active {
            ui_kit::hud_panel(ui, acc, !pr.open_threads.is_empty(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("!{}", pr.id)).monospace().color(acc.main));
                    ui_kit::side_pill(ui, &pr.repo, acc);
                    if pr.is_draft {
                        ui_kit::warn_pill(ui, "Rascunho");
                    }
                    for i in &pr.item_ids {
                        ui_kit::id_link(ui, *i, &azdo::item_url(*i), acc);
                    }
                });
                ui.label(RichText::new(&pr.title).size(theme::size::CARD).strong().color(theme::BRIGHT));
                ui.label(RichText::new(format!("{} → {} · aberto {}", pr.source, pr.target, ui_kit::when(&pr.created_at))).monospace().size(12.0).color(theme::MUTED));
                // Votes stand out; reviewers who have not voted yet are one quiet line.
                let voted: Vec<&Reviewer> = pr.reviewers.iter().filter(|r| r.vote != 0).collect();
                let waiting: Vec<&str> = pr.reviewers.iter().filter(|r| r.vote == 0).map(|r| r.name.as_str()).collect();
                if !voted.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for r in &voted {
                            let c = vote_color(r.vote, acc);
                            ui_kit::pill(ui, &format!("{} · {}", r.name, r.label), c, theme::alpha(c, 0.1), theme::alpha(c, 0.35));
                        }
                    });
                }
                if !waiting.is_empty() {
                    ui.label(RichText::new(format!("Sem voto: {}", waiting.join(", "))).small().color(theme::MUTED));
                }
                if !pr.open_threads.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("{} COMENTÁRIO{} POR RESOLVER", pr.open_threads.len(), if pr.open_threads.len() > 1 { "S" } else { "" })).font(display(12.5)).color(theme::AMBER).extra_letter_spacing(1.4));
                    for t in &pr.open_threads {
                        ui.separator();
                        ui.horizontal(|ui| {
                            let place = match (&t.file, t.line) {
                                (Some(f), Some(l)) => format!("{f}:{l}"),
                                (Some(f), None) => f.clone(),
                                _ => "Comentário geral".into(),
                            };
                            ui.label(RichText::new(place).monospace().size(12.0).color(acc.light));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button(ui, "Corrigir", true, acc).on_hover_text("Abre uma conversa com este comentário").clicked() {
                                    request_fix(ui, &pr.key, Some(t.id));
                                }
                            });
                        });
                        for c in &t.comments {
                            ui.label(RichText::new(format!("{} · {}", c.by, ui_kit::when(&c.at))).small().color(theme::MUTED));
                            ui.add(egui::Label::new(RichText::new(&c.text).color(theme::TEXT)).wrap());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if pr.is_draft
                        && button(ui, "Rever com o Jarvis", pr.open_threads.len() <= 1, acc)
                            .on_hover_text("Revê o teu rascunho antes de o publicares: o Jarvis lê o diff com as regras da equipa e propõe comentários (sem voto). Só publica depois de aprovares.")
                            .clicked()
                    {
                        request_review(ui, &pr.key);
                    }
                    if pr.open_threads.len() > 1 && button(ui, "Corrigir tudo", true, acc).clicked() {
                        request_fix(ui, &pr.key, None);
                    }
                    if button(ui, "Abrir o PR", false, acc).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&pr.url));
                    }
                });
            });
            ui.add_space(10.0);
        }
        if !d.recent.is_empty() {
            ui_kit::section_label(ui, "Fechados recentemente", acc.main, acc);
            for r in d.recent.iter().take(10) {
                ui.horizontal(|ui| {
                    if ui.link(RichText::new(format!("!{}", r.id)).monospace()).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&r.url));
                    }
                    ui.label(RichText::new(&r.repo).small().color(theme::MUTED));
                    ui.add(egui::Label::new(&r.title).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(ui_kit::when(&r.closed_at)).monospace().size(11.0).color(theme::FAINT));
                    });
                });
            }
        }
    });
}

/// Screenshots only (`JARVIS_SHOT_DEMO=1`): made-up PRs.
pub fn demo() -> Option<PrData> {
    std::env::var("JARVIS_SHOT_DEMO").ok()?;
    let pr = |id: i64, repo: &str, title: &str, author: &str, vote: i64| Pr {
        key: format!("demo:{id}"),
        id,
        repo: repo.into(),
        title: title.into(),
        author: author.into(),
        my_vote: vote,
        source: format!("feature/{id}"),
        target: "main".into(),
        created_at: "2026-11-21T09:00:00Z".into(),
        url: "https://dev.azure.com/demo".into(),
        item_ids: vec![9100 + id % 10],
        reviewers: vec![Reviewer { name: "Nuno Chumbo".into(), vote: 5, label: vote_label(5).into() }],
        ..Default::default()
    };
    Some(PrData {
        active: vec![Pr { mine: true, is_draft: true, ..pr(3211,"Equipa.App", "Localização: manter disabled ao escolher fornecedor", "Rita Demo", 0) }],
        to_review: vec![pr(3220, "Equipa.Api", "Exportação de fornecedores para Excel", "João Pereira", 0), pr(3224, "Equipa.App", "NIF na ficha do cliente", "Ana Silva", 5)],
        recent: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_draft_is_read_from_the_block() {
        let text = "Resumo em texto.\n\n[[REVIEW]]\n{\"pr\":\"r:1\",\"vote\":4,\"summary\":\"ok\",\"comments\":[{\"file\":\"/a.ts\",\"line\":3,\"text\":\"x\"},{\"text\":\"  \"},{\"text\":\"geral\"}]}\n[[/REVIEW]]";
        let parts = split_reviews(text);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], Err("Resumo em texto.".to_owned()));
        let d = parts[1].clone().unwrap();
        // 4 is not a vote Azure DevOps knows: it snaps to the nearest (5).
        assert_eq!(d.vote, 5);
        assert_eq!(d.comments.len(), 2);
        assert_eq!(d.comments[0].file.as_deref(), Some("/a.ts"));
        assert_eq!(d.comments[1].line, None);
    }

    #[test]
    fn unfinished_or_invalid_block_stays_text() {
        assert!(split_reviews("[[REVIEW]]\n{\"vote\":5").iter().all(|p| p.is_err()));
        assert!(split_reviews("[[REVIEW]]\nnão é json\n[[/REVIEW]]").iter().all(|p| p.is_err()));
    }

    #[test]
    fn same_draft_same_key() {
        let d = ReviewDraft { vote: 5, summary: "a".into(), comments: vec![DraftComment { text: "b".into(), ..Default::default() }], ..Default::default() };
        assert_eq!(draft_key("r:1", &d), draft_key("r:1", &d));
        assert_ne!(draft_key("r:1", &d), draft_key("r:2", &d));
    }

    #[test]
    fn review_prompt_ends_with_the_format_and_never_publishes() {
        let pr = Pr { key: "r:7".into(), id: 7, repo: "Repo".into(), title: "T".into(), source: "feature/7".into(), target: "main".into(), ..Default::default() };
        let p = review_prompt(&pr);
        assert!(p.contains("[[REVIEW]]") && p.contains("\"pr\":\"r:7\""));
        assert!(p.contains("Não publiques nada"));
        assert!(!p.contains("Este PR é meu"));
        let own = review_prompt(&Pr { mine: true, is_draft: true, ..pr });
        assert!(own.contains("Este PR é meu") && own.contains("\"vote\":0"));
    }
}
