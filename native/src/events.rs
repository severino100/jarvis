//! What changed since the last load (port of diffAndNotify / diffPrs): items sent back by QA, new items, column
//! changes, new comments, and new review comments or votes on my PRs. The first load only records what exists.

use crate::{azdo::Dashboard, prs::PrData, settings};

pub struct News {
    pub item_id: Option<i64>,
    pub pr_key: Option<String>,
    pub title: String,
    pub body: String,
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
            if let Some(b) = &it.back {
                if st.seen_back.get(&key) != Some(&b.at) {
                    let c = b.comments.last();
                    out.push(News {
                        item_id: Some(it.id),
                        pr_key: None,
                        title: format!("#{} voltou para trás", it.id),
                        body: format!("{}: {}", b.by, c.map(|c| cut(&c.text, 140)).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("{} → {}", b.from, b.to))),
                    });
                    st.seen_back.insert(key.clone(), b.at.clone());
                }
            }
            if !st.seen_now.contains(&it.id) && it.owner.is_some() && it.back.is_none() {
                out.push(News {
                    item_id: Some(it.id),
                    pr_key: None,
                    title: format!("Novo item para ti: #{}", it.id),
                    body: it.title.clone(),
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
                        title: format!("#{}: {} → {}", it.id, was, it.column),
                        body: it.title.clone(),
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
                        title: format!("Comentário em #{}", it.id),
                        body: format!("{}: {}", c.by, if c.text.is_empty() { "(imagem)".into() } else { cut(&c.text, 160) }),
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
            let comments: Vec<(String, &crate::prs::PrComment)> = pr.threads.iter().flat_map(|t| t.comments.iter().map(move |c| (format!("{}:{}", t.id, c.id), c))).collect();
            let votes: std::collections::HashMap<String, i64> = pr.reviewers.iter().map(|r| (r.name.clone(), r.vote)).collect();
            if let Some(seen) = st.seen_pr.get(&pr.key) {
                for (key, c) in &comments {
                    if seen.comments.contains(key) || c.by == my_name {
                        continue;
                    }
                    out.push(News {
                        item_id: None,
                        pr_key: Some(pr.key.clone()),
                        title: format!("Comentário no PR !{}", pr.id),
                        body: format!("{}: {}", c.by, cut(&c.text, 160)),
                    });
                }
                for r in &pr.reviewers {
                    if r.name != my_name && seen.votes.get(&r.name).is_some_and(|v| *v != r.vote) {
                        out.push(News {
                            item_id: None,
                            pr_key: Some(pr.key.clone()),
                            title: format!("PR !{}: {}", pr.id, r.label),
                            body: format!("{} · {}", r.name, pr.title),
                        });
                    }
                }
            }
            st.seen_pr.insert(pr.key.clone(), settings::SeenPr { comments: comments.iter().map(|(k, _)| k.clone()).collect(), votes });
        }
    });
    out
}
