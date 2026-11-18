//! Who uses Jarvis and how their team works. Nothing company-specific is compiled in:
//! - the profile is personal (name, projects folder, installed packs, the board token encrypted with DPAPI) and lives in
//!   %APPDATA%\Jarvis\perfil.json;
//! - a team pack is a folder, usually a private git repository the team shares, with jarvis.toml (board, repositories,
//!   branch convention), the team's Claude Code skills (the pack is also a Claude Code plugin) and its guides.
//!   Everyone in the team uses the same skills; a `git pull` of the pack updates them.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// How the voice should say the name when the spelling reads wrong (e.g. an accent the name is written without).
    pub spoken_name: String,
    pub projects_dir: String,
    /// Installed pack folders.
    pub packs: Vec<String>,
    /// The pack whose board, repositories and skills Jarvis uses.
    pub active_pack: String,
    /// The board's personal access token, DPAPI-encrypted, base64.
    pub board_token: String,
    /// What this person uses Jarvis for: "dev" (board, PRs, hours, cleanup) and/or "rh" (the personal HR assistant).
    /// The menu and the Hoje screen only show what belongs to the modes that are on. Missing in older files: ["dev"].
    pub modes: Vec<String>,
    /// Blocks of the Hoje screen this person turned off (see `dashboard::BLOCKS`); everything else shows.
    pub today_hidden: Vec<String>,
    /// The guided first run is done (or was skipped). A profile file from before it existed reads as done: only a
    /// person who never had a profile is guided.
    #[serde(default = "yes")]
    pub welcomed: bool,
}

fn yes() -> bool {
    true
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            spoken_name: String::new(),
            projects_dir: String::new(),
            packs: Vec::new(),
            active_pack: String::new(),
            board_token: String::new(),
            modes: vec![DEV.into()],
            today_hidden: Vec::new(),
            welcomed: true,
        }
    }
}

pub const DEV: &str = "dev";
pub const RH: &str = "rh";

/// The modes that are on; never empty (an empty list reads as dev, as before modes existed).
pub fn modes() -> Vec<String> {
    // Dev aid for screenshots: JARVIS_SHOT_MODES=rh (or dev,rh) without touching the saved profile.
    if let Ok(m) = std::env::var("JARVIS_SHOT_MODES") {
        return m.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
    }
    let m = get().modes;
    if m.is_empty() { vec![DEV.into()] } else { m }
}

pub fn has_mode(mode: &str) -> bool {
    modes().iter().any(|m| m == mode)
}

/// The board, PRs, hours and cleanup.
pub fn dev() -> bool {
    has_mode(DEV)
}

/// The mode a conversation answers in when it does not say: an item or PR conversation is dev; a general one is rh
/// when the HR mode is on.
pub fn default_mode(work_item: bool) -> &'static str {
    if work_item || !has_mode(RH) { DEV } else { RH }
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub board: Option<BoardCfg>,
    #[serde(rename = "repo")]
    pub repos: Vec<RepoCfg>,
    pub branches: Branches,
    pub guides: Guides,
    /// The board skill's folder in the pack, e.g. "skills/board"; its name is the slash command.
    pub skill: String,
    /// The board script Antigravity may run for an item's preparation (show / apply), relative to the pack.
    pub skill_cli: String,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct BoardCfg {
    /// Only "azure-devops" for now.
    pub kind: String,
    pub org: String,
    pub project: String,
    /// Area path the board reads; empty for the whole project.
    pub area: String,
    /// Board columns of the work that is mine (in this order).
    pub working: Vec<String>,
    /// The board column of the work that is free to pick.
    pub available: String,
    /// Columns an item may still be in when I pick it up; picking it moves it to `claimed_state`.
    pub before_claim: Vec<String>,
    pub claimed_state: String,
}

impl Default for BoardCfg {
    fn default() -> Self {
        Self {
            kind: "azure-devops".into(),
            org: String::new(),
            project: String::new(),
            area: String::new(),
            working: vec!["In Progress".into(), "Planned".into()],
            available: "Refined".into(),
            before_claim: vec!["Backlog".into(), "Ready for Refinement".into(), "Refined".into()],
            claimed_state: "Planned".into(),
        }
    }
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct RepoCfg {
    /// Short tag shown on worktrees and used in plans ("APP", "API").
    pub side: String,
    /// Folder name inside the projects folder.
    pub dir: String,
    /// What it is, for the AI ("front", "back").
    pub label: String,
    /// Where the conversations start; the first repository when none is marked.
    pub main: bool,
    /// Worktrees get node_modules as a junction to the main folder's.
    pub link_node_modules: bool,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct Branches {
    pub base: String,
    /// {id}, {sprint} (the sprint's number) and {kind}.
    pub bug: String,
    pub other: String,
    /// {repo} (the repository folder) and {id}.
    pub worktree: String,
}

impl Default for Branches {
    fn default() -> Self {
        Self { base: "main".into(), bug: "fix/{id}".into(), other: "feature/{id}".into(), worktree: "{repo}-{id}".into() }
    }
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct Guides {
    /// Team rules added to Jarvis's own persona.
    pub persona: String,
    /// Team rules for the HR conversations. Without it, an HR-only person (no dev mode) gets `persona`.
    pub persona_rh: String,
    /// Extra notes for Antigravity.
    pub agy: String,
    /// How to prepare an implementation (tasks, worktrees, hand-off package).
    pub prepare: String,
    /// How to test in the browser; copied into every hand-off.
    pub test: String,
}

#[derive(Clone)]
pub struct Pack {
    pub dir: PathBuf,
    pub m: Manifest,
}

fn file() -> PathBuf {
    crate::settings::data_dir().join("perfil.json")
}

fn store() -> &'static Mutex<Profile> {
    static S: OnceLock<Mutex<Profile>> = OnceLock::new();
    S.get_or_init(|| {
        let p = fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            let projects = ["Projetos", "Projects", "source\\repos"].iter().map(|d| home.join(d)).find(|d| d.is_dir()).unwrap_or(home);
            // No profile yet: the first run guides this person through it (welcome.rs).
            Profile { name: std::env::var("USERNAME").unwrap_or_default(), projects_dir: projects.to_string_lossy().into_owned(), welcomed: false, ..Default::default() }
        });
        Mutex::new(p)
    })
}

pub fn get() -> Profile {
    store().lock().unwrap().clone()
}

pub fn update(f: impl FnOnce(&mut Profile)) {
    let mut p = store().lock().unwrap();
    f(&mut p);
    let _ = fs::create_dir_all(crate::settings::data_dir());
    let _ = fs::write(file(), serde_json::to_string_pretty(&*p).unwrap_or_default());
}

/// Whether the guided first run still has to show.
pub fn needs_welcome() -> bool {
    !get().welcomed
}

pub fn name() -> String {
    let n = get().name;
    if n.trim().is_empty() { "o utilizador".into() } else { n.trim().to_owned() }
}

/// A path from jarvis.toml (written with / ) inside the pack.
fn rel(dir: &Path, path: &str) -> PathBuf {
    dir.join(path.replace('/', "\\"))
}

pub fn read_pack(dir: &Path) -> Result<Pack, String> {
    let text = fs::read_to_string(dir.join("jarvis.toml")).map_err(|_| format!("{} não tem jarvis.toml.", dir.display()))?;
    let m: Manifest = toml::from_str(&text).map_err(|e| format!("jarvis.toml inválido: {e}"))?;
    Ok(Pack { dir: dir.to_owned(), m })
}

/// The active pack, or the first installed one.
pub fn pack() -> Option<Pack> {
    let p = get();
    let dir = if p.active_pack.is_empty() { p.packs.first().cloned()? } else { p.active_pack };
    read_pack(Path::new(&dir)).ok()
}

pub fn projects_dir() -> PathBuf {
    PathBuf::from(get().projects_dir)
}

pub fn repos() -> Vec<RepoCfg> {
    pack().map(|p| p.m.repos).unwrap_or_default()
}

pub fn repo_dir(r: &RepoCfg) -> PathBuf {
    projects_dir().join(&r.dir)
}

/// Where the conversations start: the pack's main repository, or the projects folder.
pub fn main_dir() -> PathBuf {
    let rs = repos();
    rs.iter().find(|r| r.main).or(rs.first()).map(repo_dir).filter(|d| d.is_dir()).unwrap_or_else(projects_dir)
}

pub fn branches() -> Branches {
    pack().map(|p| p.m.branches).unwrap_or_default()
}

pub fn worktree_dir(r: &RepoCfg, id: i64) -> PathBuf {
    projects_dir().join(branches().worktree.replace("{repo}", &r.dir).replace("{id}", &id.to_string()))
}

/// The item's branch from the team convention; None when the pattern needs a sprint number there is not.
pub fn branch_for(kind: &str, sprint: &str, id: i64) -> Option<String> {
    let b = branches();
    let pattern = if kind == "Bug" { b.bug } else { b.other };
    let sprint: String = sprint.chars().filter(char::is_ascii_digit).collect();
    if pattern.contains("{sprint}") && sprint.is_empty() {
        return None;
    }
    Some(pattern.replace("{id}", &id.to_string()).replace("{sprint}", &sprint).replace("{kind}", &kind.to_lowercase()))
}

pub fn board() -> Option<BoardCfg> {
    pack().and_then(|p| p.m.board).filter(|b| b.kind == "azure-devops" && !b.org.is_empty() && !b.project.is_empty())
}

pub fn skill_dir() -> Option<PathBuf> {
    let p = pack()?;
    let d = rel(&p.dir, &p.m.skill);
    (!p.m.skill.is_empty() && d.join("SKILL.md").exists()).then_some(d)
}

/// The board skill's slash command ("/board"), when the pack has one.
pub fn skill_command() -> Option<String> {
    skill_dir().and_then(|d| d.file_name().map(|n| format!("/{}", n.to_string_lossy())))
}

pub fn skill_cli() -> Option<PathBuf> {
    let p = pack()?;
    let f = rel(&p.dir, &p.m.skill_cli);
    (!p.m.skill_cli.is_empty() && f.is_file()).then_some(f)
}

/// The pack folder when it is a Claude Code plugin (.claude-plugin/plugin.json): its skills load with --plugin-dir.
pub fn plugin_dir() -> Option<PathBuf> {
    pack().map(|p| p.dir).filter(|d| d.join(".claude-plugin").join("plugin.json").exists())
}

pub enum Guide {
    Persona,
    PersonaRh,
    Agy,
    Prepare,
    Test,
}

/// A guide from the pack, or Jarvis's generic one.
pub fn guide(g: Guide) -> Option<PathBuf> {
    let from_pack = pack().and_then(|p| {
        let rel = match g {
            Guide::Persona => p.m.guides.persona,
            // A dev pack's rules (branches, repositories) do not belong in an HR conversation.
            Guide::PersonaRh if p.m.guides.persona_rh.is_empty() && !dev() => p.m.guides.persona,
            Guide::PersonaRh => p.m.guides.persona_rh,
            Guide::Agy => p.m.guides.agy,
            Guide::Prepare => p.m.guides.prepare,
            Guide::Test => p.m.guides.test,
        };
        let f = self::rel(&p.dir, &rel);
        f.is_file().then_some(f)
    });
    let generic = match g {
        Guide::Prepare => Some("preparacao.md"),
        Guide::Test => Some("testar-no-browser.md"),
        _ => None,
    };
    from_pack.or_else(|| generic.map(|n| crate::settings::resources_dir().join(n)).filter(|f| f.is_file()))
}

/// Fills {nome}, {projetos}, {principal}, {repos}, {base}, {ramo_bug}, {ramo}, {worktree}, {fluxo} and
/// {guia_preparacao} in a persona or guide.
pub fn render(text: &str) -> String {
    let b = branches();
    let rs = repos();
    let repos = if rs.is_empty() {
        "os repositórios dentro desta pasta".to_owned()
    } else {
        rs.iter().map(|r| if r.label.is_empty() { r.dir.clone() } else { format!("{} ({}, {})", r.dir, r.label, r.side) }).collect::<Vec<_>>().join(", ")
    };
    let fluxo = match skill_dir() {
        Some(d) => format!("o fluxo da skill {} ({}) e as regras do CLAUDE.md do repositório", d.file_name().unwrap_or_default().to_string_lossy(), d.join("SKILL.md").display()),
        None => "as regras do CLAUDE.md do repositório".to_owned(),
    };
    let prep = guide(Guide::Prepare).map(|p| p.display().to_string()).unwrap_or_default();
    text.replace("{nome}", &name())
        .replace("{projetos}", &projects_dir().display().to_string())
        .replace("{principal}", &main_dir().display().to_string())
        .replace("{repos}", &repos)
        .replace("{base}", &b.base)
        .replace("{ramo_bug}", &b.bug)
        .replace("{ramo}", &b.other)
        .replace("{worktree}", &b.worktree)
        .replace("{fluxo}", &fluxo)
        .replace("{guia_preparacao}", &prep)
}

pub fn read_guide(g: Guide) -> String {
    guide(g).and_then(|f| fs::read_to_string(f).ok()).map(|t| render(t.trim())).unwrap_or_default()
}

/* ---------- the board token ---------- */

pub fn set_board_token(token: &str) {
    use base64::Engine;
    let enc = if token.trim().is_empty() {
        String::new()
    } else {
        crate::tasks::dpapi::protect(token.trim().as_bytes()).map(|b| base64::engine::general_purpose::STANDARD.encode(b)).unwrap_or_default()
    };
    update(|p| p.board_token = enc);
}

/// The token saved in Definições; otherwise the file the board skill also reads (AZDO_PAT_FILE or ~/.config/azdo/pat).
pub fn board_token() -> Option<String> {
    use base64::Engine;
    let saved = get().board_token;
    if !saved.is_empty() {
        let raw = base64::engine::general_purpose::STANDARD.decode(saved).ok()?;
        return crate::tasks::dpapi::unprotect(&raw).and_then(|b| String::from_utf8(b).ok());
    }
    let path = std::env::var("AZDO_PAT_FILE").map(PathBuf::from).unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".config").join("azdo").join("pat"));
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

pub fn has_saved_token() -> bool {
    !get().board_token.is_empty()
}

/// What the pack's skills need from this person, for Claude Code and Antigravity (and the commands they run).
pub fn child_env(cmd: &mut Command) {
    if let Some(b) = board() {
        cmd.env("AZDO_ORG", &b.org).env("AZDO_PROJECT", &b.project).env("AZDO_AREA", &b.area);
    }
    if let Some(t) = board_token() {
        cmd.env("AZDO_PAT", t);
    }
    cmd.env("JARVIS_PROJECTS", projects_dir());
}

/* ---------- installing and updating packs ---------- */

fn git(args: &[&str]) -> Result<String, String> {
    let o = Command::new("git").args(args).creation_flags(0x0800_0000).output().map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("git falhou").trim().to_owned())
    }
}

/// Adds a pack folder; the first pack added becomes the active one.
pub fn add_folder(dir: &Path) -> Result<String, String> {
    let p = read_pack(dir)?;
    let d = dir.to_string_lossy().into_owned();
    update(|x| {
        if !x.packs.iter().any(|y| y.eq_ignore_ascii_case(&d)) {
            x.packs.push(d.clone());
        }
        if x.active_pack.is_empty() {
            x.active_pack = d.clone();
        }
    });
    Ok(if p.m.name.is_empty() { "Pacote adicionado.".into() } else { format!("Pacote \"{}\" adicionado.", p.m.name) })
}

/// Clones a pack with the person's own git credentials into %APPDATA%\Jarvis\packs, then adds it.
pub fn add_from_git(url: &str) -> Result<String, String> {
    let url = url.trim();
    let name: String = url.trim_end_matches('/').trim_end_matches(".git").rsplit(['/', ':']).next().unwrap_or("pacote").chars().filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c)).collect();
    if name.is_empty() {
        return Err("Link inválido.".into());
    }
    let dest = crate::settings::data_dir().join("packs").join(&name);
    if !dest.exists() {
        let _ = fs::create_dir_all(dest.parent().unwrap());
        git(&["clone", "--quiet", url, &dest.to_string_lossy()])?;
    }
    add_folder(&dest)
}

pub fn remove(dir: &str) {
    update(|x| {
        x.packs.retain(|y| y != dir);
        if x.active_pack == dir {
            x.active_pack = x.packs.first().cloned().unwrap_or_default();
        }
    });
}

/// `git pull` of every pack that is a git repository with a remote, so the team's skills stay the same for everyone.
/// Returns one line per pack that changed or failed.
pub fn update_packs() -> Vec<String> {
    get()
        .packs
        .iter()
        .filter(|d| Path::new(d).join(".git").exists())
        .filter_map(|d| {
            git(&["-C", d, "remote"]).ok().filter(|r| !r.is_empty())?;
            let before = git(&["-C", d, "rev-parse", "HEAD"]).unwrap_or_default();
            let name = read_pack(Path::new(d)).map(|p| p.m.name).unwrap_or_else(|_| d.clone());
            match git(&["-C", d, "pull", "--ff-only", "--quiet"]) {
                Ok(_) => (git(&["-C", d, "rev-parse", "HEAD"]).unwrap_or_default() != before).then(|| format!("Pacote \"{name}\" atualizado.")),
                Err(e) => Some(format!("Não atualizei o pacote \"{name}\": {e}")),
            }
        })
        .collect()
}
