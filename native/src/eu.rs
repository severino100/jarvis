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
    let (ins, rules) = (instructions(), rules());
    if ins.is_empty() && rules.is_empty() {
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
    out
}
