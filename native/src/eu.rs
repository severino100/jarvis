//! The person's own Jarvis, in %APPDATA%\Jarvis\eu (never in a pack, never shared):
//! - instrucoes.md: personal instructions, written by the person (tone, signature, what never to do);
//! - memoria.md: what the person asked Jarvis to remember, one rule per line with the date ("- 2026-11-18: …");
//! - skills\: personal Claude Code skills. The folder is a Claude Code plugin (.claude-plugin\plugin.json), loaded with
//!   --plugin-dir like the active pack.
//!
//! Both files go into every conversation (dev and rh), after Jarvis's rules and the team's, which stay above them.

use std::{fs, path::PathBuf};

pub fn dir() -> PathBuf {
    crate::settings::data_dir().join("eu")
}

pub fn instructions_file() -> PathBuf {
    dir().join("instrucoes.md")
}

pub fn memory_file() -> PathBuf {
    dir().join("memoria.md")
}

pub fn skills_dir() -> PathBuf {
    dir().join("skills")
}

/// Creates the folder, its two files and the plugin manifest when they are missing. Returns the folder.
pub fn ensure() -> PathBuf {
    let d = dir();
    let _ = fs::create_dir_all(skills_dir());
    let _ = fs::create_dir_all(d.join(".claude-plugin"));
    let manifest = d.join(".claude-plugin").join("plugin.json");
    if !manifest.exists() {
        let _ = fs::write(&manifest, r#"{ "name": "eu", "description": "As skills pessoais de quem usa este Jarvis.", "version": "1.0.0" }"#);
    }
    for f in [instructions_file(), memory_file()] {
        if !f.exists() {
            let _ = fs::write(&f, "");
        }
    }
    d
}

/// The folder as a Claude Code plugin, for --plugin-dir.
pub fn plugin_dir() -> PathBuf {
    ensure()
}

pub fn instructions() -> String {
    fs::read_to_string(instructions_file()).unwrap_or_default().trim().to_owned()
}

/// One remembered rule: the date it was saved and the rule.
#[derive(Clone, PartialEq)]
pub struct Rule {
    pub date: String,
    pub text: String,
}

fn parse(line: &str) -> Option<Rule> {
    let l = line.trim().trim_start_matches(['-', '*']).trim();
    if l.is_empty() || l.starts_with('#') {
        return None;
    }
    // "2026-11-18: regra"; a line written by hand without a date is a rule too.
    let dated = l.get(..10).filter(|d| d.len() == 10 && d.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() }));
    match dated {
        Some(d) if l[10..].trim_start().starts_with(':') => Some(Rule { date: d.to_owned(), text: l[10..].trim_start()[1..].trim().to_owned() }),
        _ => Some(Rule { date: String::new(), text: l.to_owned() }),
    }
}

pub fn rules() -> Vec<Rule> {
    fs::read_to_string(memory_file()).unwrap_or_default().lines().filter_map(parse).filter(|r| !r.text.is_empty()).collect()
}

impl Rule {
    fn line(&self) -> String {
        if self.date.is_empty() { format!("- {}", self.text) } else { format!("- {}: {}", self.date, self.text) }
    }
}

fn save(rules: &[Rule]) {
    ensure();
    let body: String = rules.iter().map(|r| r.line() + "\n").collect();
    let _ = fs::write(memory_file(), body);
}

/// Saves a rule with today's date; the same rule twice is kept once. Returns the rule as saved (None when empty).
pub fn remember(text: &str) -> Option<Rule> {
    let text = text.trim().trim_end_matches('.').trim().to_owned();
    if text.is_empty() {
        return None;
    }
    let mut all = rules();
    if let Some(r) = all.iter().find(|r| r.text.eq_ignore_ascii_case(&text)) {
        return Some(r.clone());
    }
    let r = Rule { date: chrono::Local::now().format("%Y-%m-%d").to_string(), text };
    all.push(r.clone());
    save(&all);
    Some(r)
}

/// Changes a rule's text, keeping its date; an empty text removes it.
pub fn edit(old: &str, new: &str) {
    let new = new.trim();
    if new.is_empty() {
        forget(old);
        return;
    }
    let mut all = rules();
    if let Some(r) = all.iter_mut().find(|r| r.text == old) {
        r.text = new.to_owned();
        save(&all);
    }
}

pub fn set_instructions(text: &str) {
    ensure();
    let t = text.trim();
    let _ = fs::write(instructions_file(), if t.is_empty() { String::new() } else { format!("{t}\n") });
}

/// A skill folder name: lower case letters, digits and hyphens ("Triagem rápida" → "triagem-rapida").
pub fn slug(name: &str) -> String {
    let plain: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    plain.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(40).collect()
}

/// Writes a personal skill (eu\skills\<slug>\SKILL.md). Returns the slug and the SKILL.md it replaced, if any, so
/// "Desfazer" can put it back.
pub fn save_skill(name: &str, about: &str, body: &str) -> Option<(String, Option<String>)> {
    let s = slug(name);
    if s.is_empty() || body.trim().is_empty() {
        return None;
    }
    ensure();
    let d = skills_dir().join(&s);
    let file = d.join("SKILL.md");
    let previous = fs::read_to_string(&file).ok();
    fs::create_dir_all(&d).ok()?;
    let about = about.trim().replace('\n', " ").replace('"', "'");
    fs::write(&file, format!("---\nname: {s}\ndescription: \"{about}\"\n---\n\n{}\n", body.trim())).ok()?;
    Some((s, previous))
}

/// Undoes `save_skill`: the previous SKILL.md comes back, or the new skill goes.
pub fn undo_skill(slug: &str, previous: Option<&str>) {
    if slug.is_empty() || slug.contains(['/', '\\', '.']) {
        return;
    }
    let d = skills_dir().join(slug);
    match previous {
        Some(p) => {
            let _ = fs::write(d.join("SKILL.md"), p);
        }
        None => {
            let _ = fs::remove_file(d.join("SKILL.md"));
            let _ = fs::remove_dir(&d);
        }
    }
}

/// The personal skills: (name, what it does from its SKILL.md, folder).
pub fn skills() -> Vec<(String, String, PathBuf)> {
    let mut out: Vec<(String, String, PathBuf)> = fs::read_dir(skills_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("SKILL.md").is_file())
        .map(|p| {
            let text = fs::read_to_string(p.join("SKILL.md")).unwrap_or_default();
            let about = text.lines().find_map(|l| l.trim().strip_prefix("description:")).unwrap_or_default().trim().trim_matches('"').to_owned();
            (p.file_name().unwrap_or_default().to_string_lossy().into_owned(), about, p)
        })
        .collect();
    out.sort();
    out
}

/// Removes a rule (by its text). Returns true when it was there.
pub fn forget(text: &str) -> bool {
    let mut all = rules();
    let n = all.len();
    all.retain(|r| r.text != text);
    if all.len() != n {
        save(&all);
    }
    all.len() != n
}

/// What goes into every conversation after Jarvis's rules and the team's: the personal instructions and the memory.
/// Empty when there is neither.
pub fn prompt_block() -> String {
    let (ins, rules, examples, shortcuts) = (instructions(), rules(), examples(), skills());
    if ins.is_empty() && rules.is_empty() && examples.is_empty() && shortcuts.is_empty() {
        return String::new();
    }
    let name = crate::profile::name();
    let mut out = format!(
        "O que {name} te ensinou (fica abaixo das regras acima: se alguma coisa daqui for contra elas, seguem-se as regras do Jarvis e da equipa, e dizes a {name} porquê. A confirmação antes de enviar, criar ou apagar e as regras de dados pessoais (RGPD) nunca se desligam):"
    );
    if !ins.is_empty() {
        out.push_str(&format!("\n\nInstruções pessoais de {name}:\n{ins}"));
    }
    if !rules.is_empty() {
        let list: String = rules.iter().map(|r| if r.date.is_empty() { format!("\n- {}", r.text) } else { format!("\n- {} (desde {})", r.text, r.date) }).collect();
        out.push_str(&format!("\n\nMemória (o que {name} te pediu para lembrares):{list}"));
    }
    // The shortcuts in words too: Antigravity and Codex do not load Claude Code skills.
    if !shortcuts.is_empty() {
        let list: String = shortcuts
            .iter()
            .map(|(n, about, d)| {
                let body = fs::read_to_string(d.join("SKILL.md")).unwrap_or_default();
                let body = body.splitn(3, "---").nth(2).unwrap_or(&body).trim().chars().take(1500).collect::<String>();
                format!("\n\n--- {n}: {about} ---\n{body}")
            })
            .collect();
        out.push_str(&format!("\n\nAtalhos pessoais de {name} (comandos que {name} criou; quando os usar, segue as instruções de cada um):{list}"));
    }
    if !examples.is_empty() {
        let list: String = examples.iter().map(|e| format!("\n\n--- {} ---\n{}", if e.subject.is_empty() { "(sem assunto)" } else { &e.subject }, e.body)).collect();
        out.push_str(&format!(
            "\n\nExemplos de estilo (rascunhos teus que {name} editou antes de enviar, na versão final). Escreve os próximos como nestes: o tom, o comprimento, as saudações e a assinatura. Usa-os só pelo estilo: nunca reutilizes os nomes, os dados nem o conteúdo deles.{list}"
        ));
    }
    out
}

/* ---------- style examples: the final version of a draft the person edited ---------- */

pub fn examples_file() -> PathBuf {
    dir().join("exemplos.md")
}

/// How many examples are kept (the newest); older ones drop off, so they never grow into a store of old emails.
const MAX_EXAMPLES: usize = 5;

#[derive(Clone, PartialEq)]
pub struct Example {
    pub date: String,
    pub subject: String,
    pub body: String,
}

/// exemplos.md: one "## <date> · <subject>" heading per example, the body under it.
pub fn examples() -> Vec<Example> {
    let text = fs::read_to_string(examples_file()).unwrap_or_default();
    let mut out: Vec<Example> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            let (date, subject) = h.split_once(" · ").unwrap_or((h, ""));
            out.push(Example { date: date.trim().to_owned(), subject: subject.trim().to_owned(), body: String::new() });
        } else if let Some(e) = out.last_mut() {
            e.body.push_str(line);
            e.body.push('\n');
        }
    }
    for e in &mut out {
        e.body = e.body.trim().to_owned();
    }
    out.retain(|e| !e.body.is_empty());
    out
}

fn save_examples(all: &[Example]) {
    ensure();
    let body: String = all.iter().map(|e| format!("## {} · {}\n\n{}\n\n", e.date, e.subject.replace('\n', " "), e.body.trim())).collect();
    let _ = fs::write(examples_file(), body);
}

/// Keeps the final version of a draft the person changed before sending (nothing when it was not changed).
/// Returns true when it was saved.
pub fn add_example(subject: &str, original: &str, final_body: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    if final_body.trim().is_empty() || norm(original) == norm(final_body) {
        return false;
    }
    let mut all = examples();
    all.retain(|e| norm(&e.body) != norm(final_body));
    all.push(Example { date: chrono::Local::now().format("%Y-%m-%d").to_string(), subject: subject.trim().to_owned(), body: final_body.trim().to_owned() });
    let extra = all.len().saturating_sub(MAX_EXAMPLES);
    all.drain(..extra);
    save_examples(&all);
    true
}

pub fn remove_example(index: usize) {
    let mut all = examples();
    if index < all.len() {
        all.remove(index);
        save_examples(&all);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test only: it points JARVIS_DATA_DIR at a temporary folder for the whole process.
    #[test]
    fn style_examples() {
        let d = std::env::temp_dir().join(format!("jarvis-eu-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        // SAFETY: the only test that touches the environment, before anything else reads it.
        unsafe { std::env::set_var("JARVIS_DATA_DIR", &d) };
        // Unchanged drafts teach nothing.
        assert!(!add_example("A", "Olá Ana,\n\nAté já.", "Olá Ana,  \n\nAté já."));
        assert!(examples().is_empty());
        assert!(add_example("Entrevista", "Olá Ana,", "Viva Ana!\n\nAbç,\nR."));
        let e = examples();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].subject, "Entrevista");
        assert_eq!(e[0].body, "Viva Ana!\n\nAbç,\nR.");
        assert!(prompt_block().contains("Viva Ana!"));
        // The same final text twice is kept once; only the newest five stay.
        assert!(add_example("Entrevista", "x", "Viva Ana!\n\nAbç,\nR."));
        for i in 0..6 {
            add_example(&format!("S{i}"), "x", &format!("corpo {i}"));
        }
        let e = examples();
        assert_eq!(e.len(), MAX_EXAMPLES);
        assert_eq!(e.last().unwrap().body, "corpo 5");
        assert_eq!(e.first().unwrap().body, "corpo 1");
        remove_example(0);
        assert_eq!(examples().first().unwrap().body, "corpo 2");
        // A [[SKILL]] block from a reply becomes a personal skill; "Desfazer" takes it away again.
        let reply = "Feito.

[[SKILL: Triagem rápida | Quando disser \"triagem\": avalia os perfis]]
1. Lê a vaga.
2. Faz a tabela.
[[/SKILL]]

Usa assim.";
        let s = crate::brain::read_skills(reply);
        assert_eq!(s.len(), 1);
        assert_eq!(crate::brain::strip_marks(reply), "Feito.



Usa assim.");
        let (slug, replaced) = save_skill(&s[0].name, &s[0].about, &s[0].body).unwrap();
        assert_eq!((slug.as_str(), replaced.is_none()), ("triagem-rapida", true));
        assert_eq!(skills().len(), 1);
        assert!(prompt_block().contains("2. Faz a tabela."));
        assert_eq!(crate::brain::hide_partial("Ok.
[[SKILL: x | y]]
1. a"), "Ok.
");
        undo_skill(&slug, None);
        assert!(skills().is_empty());
        let _ = fs::remove_dir_all(&d);
    }
}
