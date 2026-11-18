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
