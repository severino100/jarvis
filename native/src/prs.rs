//! My pull requests in Azure Repos: reviewers' votes, comment threads, the ones finished recently, and "Corrigir"
//! (opens a conversation with the review comment to fix it). Needs a PAT with Code (Read).

use crate::{
    azdo,
    theme::{self, Accent},
    ui_kit::{self, button, display},
};
use eframe::egui::{self, Align, Layout, RichText, Ui};
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
    let (active, done) = thread::scope(|sc| {
        let a = sc.spawn(|| q("active", 30));
        let d = sc.spawn(|| q("completed", 20));
        (a.join().unwrap(), d.join().unwrap())
    });
    let active = active?;
    let active = thread::scope(|sc| {
        let hs: Vec<_> = active.iter().map(|pr| sc.spawn(|| details(pr, &mine))).collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let recent = done?
        .iter()
        .map(|pr| {
            let repo = s(&pr["repository"]["name"]);
            let id = pr["pullRequestId"].as_i64().unwrap_or(0);
            Recent { id, url: azdo::web_url(&format!("_git/{}/pullrequest/{id}", repo.replace(' ', "%20"))), repo, title: s(&pr["title"]), closed_at: s(&pr["closedDate"]) }
        })
        .collect();
    Ok(PrData { active, recent })
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
        "Review do PR !{} \"{}\" (repositório {}, branch {} → {}{items}).\n\n{}:\n{body}\n\nAnalisa no código do branch {} (usa o worktree desse branch se existir; se não existir, prepara um a partir de origin/{} só depois do meu sim), explica o que o revisor pede e propõe a correção. Quando eu aprovar, prepara o pacote de passagem e termina com a linha de implementar.",
        pr.id,
        pr.title,
        pr.repo,
        pr.source,
        pr.target,
        if threads.len() > 1 { "Comentários por resolver" } else { "Comentário" },
        pr.source,
        pr.source
    )
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
        ui_kit::section_label(ui, &format!("Abertos · {}", d.active.len()), acc.main, acc);
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
