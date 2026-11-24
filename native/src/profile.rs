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
    /// Legacy single active pack; read once and moved to `active_packs` (see `store`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub active_pack: String,
    /// The packs that are on, in order (skills, guides, board, repositories come from these). None: an older profile
    /// that still has `active_pack`, or no choice yet (the first installed pack counts as on).
    pub active_packs: Option<Vec<String>>,
    /// The board's personal access token, DPAPI-encrypted, base64.
    pub board_token: String,
    /// What this person uses Jarvis for: "dev" (board, PRs, hours, cleanup) and/or "rh" (the personal HR assistant).
    /// The menu and the Hoje screen only show what belongs to the modes that are on. Missing in older files: ["dev"].
    pub modes: Vec<String>,
    /// Blocks of the Hoje screen this person turned off (see `dashboard::BLOCKS`); everything else shows.
    pub today_hidden: Vec<String>,
    /// Menu entries this person hid (keys of `app::menu_entries`); Hoje and Definições always stay.
    pub menu_hidden: Vec<String>,
    /// The guided first run is done (or was skipped). A profile file from before it existed reads as done: only a
    /// person who never had a profile is guided.
    #[serde(default = "yes")]
    pub welcomed: bool,
    /// The calendars the agenda reads. Missing (older profiles): the classic Outlook on this PC, as before.
    pub calendars: Option<Vec<CalSource>>,
}

/// One calendar the agenda reads: the classic Outlook on this PC, or an iCalendar link.
#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct CalSource {
    pub name: String,
    /// "outlook" or "ics".
    pub kind: String,
    /// The .ics link, DPAPI-encrypted and base64 (the secret address gives read access to the whole calendar).
    pub url: String,
}

impl CalSource {
    pub fn outlook() -> Self {
        Self { name: "Outlook neste PC".into(), kind: "outlook".into(), url: String::new() }
    }
    pub fn ics(name: &str, link: &str) -> Self {
        use base64::Engine;
        let url = crate::tasks::dpapi::protect(link.trim().as_bytes()).map(|b| base64::engine::general_purpose::STANDARD.encode(b)).unwrap_or_default();
        Self { name: name.trim().into(), kind: "ics".into(), url }
    }
    /// The decrypted link of an ics source.
    pub fn link(&self) -> Option<String> {
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD.decode(&self.url).ok()?;
        crate::tasks::dpapi::unprotect(&raw).and_then(|b| String::from_utf8(b).ok())
    }
}

/// The calendars in use: no choice made yet means the Outlook of this PC, as before.
pub fn calendars() -> Vec<CalSource> {
    get().calendars.unwrap_or_else(|| vec![CalSource::outlook()])
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
            active_packs: None,
            board_token: String::new(),
            modes: vec![DEV.into()],
            today_hidden: Vec::new(),
            menu_hidden: Vec::new(),
            welcomed: true,
            calendars: None,
        }
    }
}

pub const DEV: &str = "dev";
pub const RH: &str = "rh";
/// Not a profile mode: the free conversation in the Jarvis menu entry (talk, tasks, emails), for everyone.
pub const GERAL: &str = "geral";

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
    /// Tabs this pack adds to the menu (`[[tab]]`, with `[[tab.action]]` inside).
    #[serde(rename = "tab")]
    pub tabs: Vec<TabCfg>,
    /// The board skill's folder in the pack, e.g. "skills/board"; its name is the slash command.
    pub skill: String,
    /// The board script Antigravity may run for an item's preparation (show / apply), relative to the pack.
    pub skill_cli: String,
}

/// A menu tab from a pack: a title, a line about it and its actions.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct TabCfg {
    pub title: String,
    pub about: String,
    /// KPI cards read from a command of the pack (`[tab.kpis]`).
    pub kpis: Option<KpiCfg>,
    /// A table read from a command of the pack (`[tab.table]`).
    pub table: Option<TableCfg>,
    #[serde(rename = "action")]
    pub actions: Vec<TabAction>,
    /// The pack's folder name (filled by `pack_tabs`).
    #[serde(skip)]
    pub pack: String,
}

/// Where a tab's KPI cards come from: a command of the pack that prints JSON, and where in that JSON each part is
/// (dotted paths, e.g. `value.v`). Only `run` and `items` are required.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct KpiCfg {
    /// The command, relative to the pack (the first word is the program, e.g. `tools/painel/painel.exe report --json`).
    pub run: String,
    /// Path of the list of KPIs in the output.
    pub items: String,
    pub label: String,
    pub value: String,
    pub detail: String,
    pub unit: String,
    /// `up` or `down`: which way is better (colours the change); empty: neutral.
    pub better: String,
    /// Path of a list of [label, number] pairs.
    pub trend: String,
    pub group: String,
    /// Path of an error text; the card shows it instead of the value.
    pub error: String,
    /// Optional top-level texts: a line above the cards and when it was read.
    pub title: String,
    pub updated: String,
    /// Minutes before the numbers are read again on their own (0: only with the button).
    pub refresh_minutes: u64,
    /// A button on a card: `action` is the path of its label (empty or missing on a card: no button), `action_arg` the path
    /// of what goes into `{arg}`, and `action_prompt` the request sent to the AI when it is clicked (`{arg}` and `{label}` filled in).
    pub action: String,
    pub action_arg: String,
    pub action_prompt: String,
    /// "cards" (default: every group a section of cards) or "dashboard": the `hero` group as big numbers on top, the other
    /// groups as panels side by side, one line per KPI with its trend.
    pub layout: String,
    pub hero: String,
}

/// One thing a pack tab can do: ask the AI (`prompt`) or open a link or file (`open`). `{input}` is what the person types in
/// the text box (`input` is its label; `default` its first value) and `{sprint}` the current sprint's number.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct TabAction {
    pub label: String,
    pub hint: String,
    pub prompt: String,
    pub open: String,
    pub input: String,
    pub default: String,
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
    /// No per-item worktree: an item is a branch in the repository's main folder. For a back end that runs in Docker
    /// Compose from that folder, where a worktree never reaches the containers. Antigravity's preparation then only
    /// reads the folder's state and creates the branch (no switch); whoever implements switches to it.
    pub branch_only: bool,
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
    /// The team's rules for reviewing a pull request; given to the AI in the PRs → Rever conversation.
    pub review: String,
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
        let mut p = p;
        migrate(&mut p);
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

/// An older profile (one `active_pack`, or none chosen) becomes an explicit list of packs that are on.
fn migrate(p: &mut Profile) {
    if p.active_packs.is_none() {
        let first = if p.active_pack.is_empty() { p.packs.first().cloned() } else { Some(p.active_pack.clone()) };
        p.active_packs = Some(first.into_iter().collect());
    }
    p.active_pack.clear();
}

/// The folders of the packs that are on, in order (only the installed ones).
pub fn active_dirs() -> Vec<String> {
    let p = get();
    p.active_packs.unwrap_or_default().into_iter().filter(|d| p.packs.contains(d)).collect()
}

/// Every pack that is on and readable, in order.
pub fn packs_active() -> Vec<Pack> {
    active_dirs().iter().filter_map(|d| read_pack(Path::new(d)).ok()).collect()
}

/// The tabs of every pack that is on, in order.
pub fn pack_tabs() -> Vec<TabCfg> {
    // Read from disk at most every 3 seconds: the menu asks every frame.
    static CACHE: Mutex<Option<(std::time::Instant, Vec<TabCfg>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((at, tabs)) = c.as_ref() {
        if at.elapsed().as_secs() < 3 {
            return tabs.clone();
        }
    }
    let tabs = read_pack_tabs();
    *c = Some((std::time::Instant::now(), tabs.clone()));
    tabs
}

fn read_pack_tabs() -> Vec<TabCfg> {
    packs_active()
        .into_iter()
        .flat_map(|p| {
            let name = p.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            p.m.tabs.into_iter().filter(|t| !t.title.trim().is_empty()).map(move |mut t| {
                t.pack = name.clone();
                t
            })
        })
        .collect()
}

/// The first pack that is on (what a single-pack question, like the name in `--profile`, asks).
pub fn pack() -> Option<Pack> {
    packs_active().into_iter().next()
}

/// The pack that has the repositories: the first active one with `[[repo]]`, else the first active one.
fn code_pack() -> Option<Pack> {
    let all = packs_active();
    let i = all.iter().position(|p| !p.m.repos.is_empty()).unwrap_or(0);
    all.into_iter().nth(i)
}

/// The pack of the board shown in Tarefas: the first active one with a usable `[board]`.
fn board_pack() -> Option<Pack> {
    packs_active().into_iter().find(|p| p.m.board.as_ref().is_some_and(valid_board))
}

fn valid_board(b: &BoardCfg) -> bool {
    b.kind == "azure-devops" && !b.org.is_empty() && !b.project.is_empty()
}

pub fn projects_dir() -> PathBuf {
    PathBuf::from(get().projects_dir)
}

pub fn repos() -> Vec<RepoCfg> {
    code_pack().map(|p| p.m.repos).unwrap_or_default()
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
    code_pack().map(|p| p.m.branches).unwrap_or_default()
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
    board_pack().and_then(|p| p.m.board)
}

fn has_skill(p: &Pack) -> bool {
    !p.m.skill.is_empty() && rel(&p.dir, &p.m.skill).join("SKILL.md").exists()
}

/// The pack of "the" board skill (the /command and the script Antigravity runs): the board's pack when it has one,
/// else the first active pack that does.
fn skill_pack() -> Option<Pack> {
    board_pack().filter(has_skill).or_else(|| packs_active().into_iter().find(has_skill))
}

pub fn skill_dir() -> Option<PathBuf> {
    skill_pack().map(|p| rel(&p.dir, &p.m.skill))
}

/// The board skill's slash command ("/board"), when a pack has one.
pub fn skill_command() -> Option<String> {
    skill_dir().and_then(|d| d.file_name().map(|n| format!("/{}", n.to_string_lossy())))
}

pub fn skill_cli() -> Option<PathBuf> {
    let p = skill_pack()?;
    let f = rel(&p.dir, &p.m.skill_cli);
    (!p.m.skill_cli.is_empty() && f.is_file()).then_some(f)
}

/// Every active pack that is a Claude Code plugin (.claude-plugin/plugin.json): Claude starts with one --plugin-dir each.
pub fn plugin_dirs() -> Vec<PathBuf> {
    packs_active().into_iter().map(|p| p.dir).filter(|d| d.join(".claude-plugin").join("plugin.json").exists()).collect()
}

pub enum Guide {
    Persona,
    PersonaRh,
    Agy,
    Prepare,
    Test,
    Review,
}

/// The guide of every active pack that has one, in the packs' order; Jarvis's generic one when none does.
pub fn guides(g: Guide) -> Vec<PathBuf> {
    let from_packs: Vec<PathBuf> = packs_active()
        .into_iter()
        .filter_map(|p| {
            let rel = match g {
                Guide::Persona => p.m.guides.persona,
                // A dev pack's rules (branches, repositories) do not belong in an HR conversation.
                Guide::PersonaRh if p.m.guides.persona_rh.is_empty() && !dev() => p.m.guides.persona,
                Guide::PersonaRh => p.m.guides.persona_rh,
                Guide::Agy => p.m.guides.agy,
                Guide::Prepare => p.m.guides.prepare,
                Guide::Test => p.m.guides.test,
                Guide::Review => p.m.guides.review,
            };
            let f = self::rel(&p.dir, &rel);
            f.is_file().then_some(f)
        })
        .collect();
    if !from_packs.is_empty() {
        return from_packs;
    }
    let generic = match g {
        Guide::Prepare => Some("preparacao.md"),
        Guide::Test => Some("testar-no-browser.md"),
        _ => None,
    };
    generic.map(|n| crate::settings::resources_dir().join(n)).filter(|f| f.is_file()).into_iter().collect()
}

/// The first guide (for the ones that are a single path, like the preparation guide the AI is pointed at).
pub fn guide(g: Guide) -> Option<PathBuf> {
    guides(g).into_iter().next()
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
    guides(g).iter().filter_map(|f| fs::read_to_string(f).ok()).map(|t| render(t.trim())).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n---\n\n")
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
    // Every active pack with a board also gets its own variables (AZDO_ORG__<PACK> …, <PACK> being the pack's folder
    // name in capitals), so a skill of any pack reads the board it belongs to even when another pack's board is the
    // one in Tarefas. The token is the person's own, the same for all.
    for p in packs_active() {
        let Some(b) = p.m.board.as_ref().filter(|b| valid_board(b)) else { continue };
        let slug: String = p.dir.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        cmd.env(format!("AZDO_ORG__{slug}"), &b.org).env(format!("AZDO_PROJECT__{slug}"), &b.project).env(format!("AZDO_AREA__{slug}"), &b.area);
    }
    cmd.env("JARVIS_PACKS", active_dirs().join(";"));
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
        // A pack that is added is on.
        let list = x.active_packs.get_or_insert_with(Vec::new);
        if !list.contains(&d) {
            list.push(d.clone());
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
        if let Some(l) = x.active_packs.as_mut() {
            l.retain(|y| y != dir);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_single_active_pack_becomes_a_list() {
        let mut p = Profile { packs: vec!["a".into(), "b".into()], active_pack: "b".into(), ..Default::default() };
        migrate(&mut p);
        assert_eq!(p.active_packs, Some(vec!["b".to_owned()]));
        assert!(p.active_pack.is_empty());
        // No choice yet: the first installed pack is on.
        let mut q = Profile { packs: vec!["a".into(), "b".into()], ..Default::default() };
        migrate(&mut q);
        assert_eq!(q.active_packs, Some(vec!["a".to_owned()]));
        // An explicit list (even an empty one: all off) is left alone.
        let mut r = Profile { packs: vec!["a".into()], active_packs: Some(vec![]), ..Default::default() };
        migrate(&mut r);
        assert_eq!(r.active_packs, Some(vec![]));
    }
}

/// Where a tab's table comes from: a command of the pack that prints JSON, the list of rows in it, and the columns (each one a
/// title and a dotted path into a row). Columns with `sum = true` are added up in a last row; `align` is "left" or "right"
/// (numbers: right).
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct TableCfg {
    pub run: String,
    pub items: String,
    /// Heading above the table.
    pub title: String,
    pub refresh_minutes: u64,
    #[serde(rename = "column")]
    pub columns: Vec<ColCfg>,
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct ColCfg {
    pub title: String,
    pub path: String,
    pub align: String,
    pub sum: bool,
}
