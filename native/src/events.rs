//! What changed since the last load (port of diffAndNotify / diffPrs): items sent back by QA, new items, column
//! changes, new comments, and new review comments or votes on my PRs. The first load only records what exists.

use crate::{azdo::Dashboard, prs::PrData, settings};

pub struct News {
    pub item_id: Option<i64>,
    pub pr_key: Option<String>,
    /// Conversation title when a new one has to be created.
    pub chat_title: String,
    pub title: String,
    pub body: String,
    /// Markdown for the conversation.
    pub chat: String,
}

fn quote(t: &str, n: usize) -> String {
    if t.is_empty() {
        return "(só imagem)".into();
    }
    let s: String = if t.chars().count() > n { format!("{}…", t.chars().take(n - 1).collect::<String>()) } else { t.to_owned() };
    format!("\"{s}\"")
}

fn cut(t: &str, n: usize) -> String {
    t.chars().take(n).collect()
}

pub fn board(d: &Dashboard) -> Vec<News> {
    let my_name = d.me.name.clone();
    let mut out: Vec<News> = Vec::new();
    settings::with_state(|st| {
        for it in &d.now {
            let key = it.id.to_string();
            let item_title = format!("#{} {}", it.id, it.title);
            if let Some(b) = &it.back {
                if st.seen_back.get(&key) != Some(&b.at) {
                    let c = b.comments.last();
                    let comments = if b.comments.is_empty() {
                        "\n\nSem comentários desde então.".to_owned()
                    } else {
                        format!("\n\n{}", b.comments.iter().map(|x| format!("> {}: {}", x.by, quote(&x.text, 400))).collect::<Vec<_>>().join("\n\n"))
                    };
                    out.push(News {
                        item_id: Some(it.id),
                        pr_key: None,
                        chat_title: item_title.clone(),
                        title: format!("#{} voltou para trás", it.id),
                        body: format!("{}: {}", b.by, c.map(|c| cut(&c.text, 140)).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("{} → {}", b.from, b.to))),
                        chat: format!("**O #{} voltou para trás** ({} → {}), por {}.{comments}", it.id, b.from, b.to, b.by),
                    });
                    st.seen_back.insert(key.clone(), b.at.clone());
                }
            }
            if !st.seen_now.contains(&it.id) && it.owner.is_some() && it.back.is_none() {
                out.push(News {
                    item_id: Some(it.id),
                    pr_key: None,
                    chat_title: item_title.clone(),
                    title: format!("Novo item para ti: #{}", it.id),
                    body: it.title.clone(),
                    chat: format!("**Tens um item novo: #{}** ({}, {}).\n\n{}", it.id, it.kind, it.column, it.title),
                });
            }
        }
        for it in d.now.iter().chain(&d.review) {
            let key = it.id.to_string();
            if let Some(was) = st.seen_column.get(&key) {
                if *was != it.column && !out.iter().any(|e| e.item_id == Some(it.id)) {
                    out.push(News {
                        item_id: Some(it.id),
                        pr_key: None,
                        chat_title: format!("#{} {}", it.id, it.title),
                        title: format!("#{}: {} → {}", it.id, was, it.column),
                        body: it.title.clone(),
                        chat: format!("**O #{} mudou de coluna:** {} → {}.", it.id, was, it.column),
                    });
                }
            }
            st.seen_column.insert(key.clone(), it.column.clone());
            let Some(c) = &it.last_comment else { continue };
            if st.seen_comment.get(&key) != Some(&c.id) {
                let in_back = it.back.as_ref().is_some_and(|b| b.comments.iter().any(|x| x.id == c.id));
                if c.by != my_name && !in_back {
                    out.push(News {
                        item_id: Some(it.id),
                        pr_key: None,
                        chat_title: format!("#{} {}", it.id, it.title),
                        title: format!("Comentário em #{}", it.id),
                        body: format!("{}: {}", c.by, if c.text.is_empty() { "(imagem)".into() } else { cut(&c.text, 160) }),
                        chat: format!("**Comentário novo no #{}** de {}:\n\n> {}", it.id, c.by, quote(&c.text, 600)),
                    });
                }
                st.seen_comment.insert(key, c.id);
            }
        }
        st.seen_now = d.now.iter().map(|i| i.id).collect();
        if !st.initialized {
            st.initialized = true;
            out.clear();
        }
    });
    out
}

pub fn prs(p: &PrData, my_name: &str) -> Vec<News> {
    let mut out = Vec::new();
    settings::with_state(|st| {
        for pr in &p.active {
            let comments: Vec<(String, &crate::prs::PrComment, &crate::prs::Thread)> =
                pr.threads.iter().flat_map(|t| t.comments.iter().map(move |c| (format!("{}:{}", t.id, c.id), c, t))).collect();
            let votes: std::collections::HashMap<String, i64> = pr.reviewers.iter().map(|r| (r.name.clone(), r.vote)).collect();
            let chat_title = format!("PR !{} {}", pr.id, pr.title);
            if let Some(seen) = st.seen_pr.get(&pr.key) {
                for (key, c, t) in &comments {
                    if seen.comments.contains(key) || c.by == my_name {
                        continue;
                    }
                    let place = match (&t.file, t.line) {
                        (Some(f), Some(l)) => format!(" em `{f}:{l}`"),
                        (Some(f), None) => format!(" em `{f}`"),
                        _ => String::new(),
                    };
                    let q = if c.text.chars().count() > 600 { format!("{}…", c.text.chars().take(599).collect::<String>()) } else { c.text.clone() };
                    out.push(News {
                        item_id: None,
                        pr_key: Some(pr.key.clone()),
                        chat_title: chat_title.clone(),
                        title: format!("Comentário no PR !{}", pr.id),
                        body: format!("{}: {}", c.by, cut(&c.text, 160)),
                        chat: format!("**Comentário novo no PR !{}** de {}{place}:\n\n> {}", pr.id, c.by, q.replace('\n', "\n> ")),
                    });
                }
                for r in &pr.reviewers {
                    if r.name != my_name && seen.votes.get(&r.name).is_some_and(|v| *v != r.vote) {
                        out.push(News {
                            item_id: None,
                            pr_key: Some(pr.key.clone()),
                            chat_title: chat_title.clone(),
                            title: format!("PR !{}: {}", pr.id, r.label),
                            body: format!("{} · {}", r.name, pr.title),
                            chat: format!("**{}** votou **{}** no PR !{}.", r.name, r.label, pr.id),
                        });
                    }
                }
            }
            st.seen_pr.insert(pr.key.clone(), settings::SeenPr { comments: comments.iter().map(|(k, _, _)| k.clone()).collect(), votes });
        }
    });
    out
}
