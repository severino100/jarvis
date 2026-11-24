//! Calendars from an iCalendar (.ics) link: Google Calendar ("secret address in iCal format"), Outlook.com and
//! Microsoft 365 ("publish calendar"), iCloud, Nextcloud, Proton… Read-only. Recurring events are expanded with the
//! `rrule` crate for the week asked for.

use crate::agenda::Event;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use rrule::{RRuleSet, Tz};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};

/// A feed is downloaded at most once in this many seconds, whatever weeks are shown.
const CACHE_SECS: u64 = 300;
const MAX_BYTES: u64 = 20 * 1024 * 1024;

static CACHE: Mutex<Vec<(String, Instant, String)>> = Mutex::new(Vec::new());

pub fn normalize_url(url: &str) -> String {
    let u = url.trim();
    match u.strip_prefix("webcal://").or_else(|| u.strip_prefix("WEBCAL://")) {
        Some(rest) => format!("https://{rest}"),
        None => u.to_owned(),
    }
}

fn fetch(url: &str) -> Result<String, String> {
    let url = normalize_url(url);
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("O link do calendário tem de começar por https:// (ou webcal://).".into());
    }
    if let Some((_, at, text)) = CACHE.lock().unwrap().iter().find(|(u, ..)| *u == url) {
        if at.elapsed().as_secs() < CACHE_SECS {
            return Ok(text.clone());
        }
    }
    let mut res = ureq::get(&url).header("Accept", "text/calendar, */*").call().map_err(|e| match e {
        ureq::Error::StatusCode(404) | ureq::Error::StatusCode(410) => "O link do calendário já não existe (404). Gera um novo no teu calendário.".to_owned(),
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => "O calendário recusou o link (sem permissão). Usa o endereço secreto em formato iCal.".to_owned(),
        ureq::Error::StatusCode(c) => format!("O calendário respondeu com o erro {c}."),
        _ => "Não consegui ligar ao calendário (sem rede?).".to_owned(),
    })?;
    let text = res.body_mut().with_config().limit(MAX_BYTES).read_to_string().map_err(|_| "Não consegui ler o calendário.".to_owned())?;
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("O link não devolve um calendário iCal (.ics). Usa o endereço secreto \"iCal\" do calendário.".into());
    }
    let mut c = CACHE.lock().unwrap();
    c.retain(|(u, ..)| *u != url);
    c.push((url, Instant::now(), text.clone()));
    Ok(text)
}

/// Checks a link: downloads it and counts the events (used by "Adicionar").
pub fn check(url: &str) -> Result<usize, String> {
    let text = fetch(url)?;
    Ok(text.matches("BEGIN:VEVENT").count())
}

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
}

fn parse_line(l: &str) -> Option<Prop> {
    // NAME;PARAM=v;PARAM="v:w":value — the first ':' outside quotes ends the name and parameters.
    let mut q = false;
    let at = l.char_indices().find(|(_, c)| {
        if *c == '"' {
            q = !q;
        }
        *c == ':' && !q
    })?;
    let (head, value) = (&l[..at.0], &l[at.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts.filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_owned())).collect();
    Some(Prop { name, params, value: value.to_owned() })
}

fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in text.lines() {
        if (l.starts_with(' ') || l.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&l[1..]);
        } else {
            out.push(l.to_owned());
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// The IANA name for a TZID: IANA names pass, the common Windows names (Outlook) are translated, anything else is
/// read as the local time of this PC.
fn tzid(raw: &str) -> Option<chrono_tz::Tz> {
    let raw = raw.trim().trim_matches('"');
    if let Ok(t) = raw.parse::<chrono_tz::Tz>() {
        return Some(t);
    }
    let iana = match raw {
        "UTC" | "Coordinated Universal Time" => "UTC",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "Romance Standard Time" => "Europe/Paris",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "GTB Standard Time" => "Europe/Bucharest",
        "FLE Standard Time" => "Europe/Kiev",
        "Russian Standard Time" => "Europe/Moscow",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "Pacific Standard Time" => "America/Los_Angeles",
        "E. South America Standard Time" => "America/Sao_Paulo",
        "Argentina Standard Time" => "America/Argentina/Buenos_Aires",
        "India Standard Time" => "Asia/Kolkata",
        "China Standard Time" => "Asia/Shanghai",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "South Africa Standard Time" => "Africa/Johannesburg",
        _ => return None,
    };
    iana.parse().ok()
}

struct When {
    at: DateTime<Local>,
    all_day: bool,
    /// The `DTSTART`/`EXDATE` form that `rrule` reads, with the time zone normalized.
    tz: Option<chrono_tz::Tz>,
}

fn parse_when(p: &Prop) -> Option<When> {
    let v = p.value.trim();
    if p.param("VALUE") == Some("DATE") || (v.len() == 8 && !v.contains('T')) {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        let at = d.and_hms_opt(0, 0, 0)?.and_local_timezone(Local).earliest()?;
        return Some(When { at, all_day: true, tz: None });
    }
    if let Some(z) = v.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(z, "%Y%m%dT%H%M%S").ok()?;
        return Some(When { at: Utc.from_utc_datetime(&n).with_timezone(&Local), all_day: false, tz: Some(chrono_tz::UTC) });
    }
    let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    match p.param("TZID").and_then(tzid) {
        Some(t) => Some(When { at: t.from_local_datetime(&n).earliest()?.with_timezone(&Local), all_day: false, tz: Some(t) }),
        None => Some(When { at: n.and_local_timezone(Local).earliest()?, all_day: false, tz: None }),
    }
}

/// A `DTSTART`/`EXDATE` line for `rrule`, in the same zone as the event.
fn rrule_line(name: &str, p: &Prop, w: &When) -> String {
    let v = p.value.trim();
    if w.all_day {
        format!("{name};VALUE=DATE:{}", &v[..8.min(v.len())])
    } else if v.ends_with('Z') {
        format!("{name}:{v}")
    } else if let Some(t) = w.tz {
        format!("{name};TZID={}:{v}", t.name())
    } else {
        format!("{name}:{v}")
    }
}

#[derive(Default)]
struct Raw {
    props: Vec<Prop>,
    text: String,
}

impl Raw {
    fn get(&self, n: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name == n)
    }
}

fn events_of(text: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    let mut cur: Option<Raw> = None;
    for l in unfold(text) {
        let t = l.trim_end();
        if t.eq_ignore_ascii_case("BEGIN:VEVENT") {
            cur = Some(Raw::default());
        } else if t.eq_ignore_ascii_case("END:VEVENT") {
            out.extend(cur.take());
        } else if let Some(c) = cur.as_mut() {
            if let Some(p) = parse_line(t) {
                c.text.push_str(t);
                c.text.push('\n');
                c.props.push(p);
            }
        }
    }
    out
}

fn teams_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"https://teams\.microsoft\.com/l/meetup-join/[^\s>"<\\]+"#).unwrap())
}

fn build(raw: &Raw, start: DateTime<Local>, end: DateTime<Local>, all_day: bool) -> Event {
    let text = |n: &str| raw.get(n).map(|p| unescape(&p.value)).unwrap_or_default();
    let uid = text("UID");
    let organizer = raw.get("ORGANIZER").map(|p| p.param("CN").map(str::to_owned).unwrap_or_else(|| p.value.trim_start_matches("mailto:").to_owned())).unwrap_or_default();
    let iso = |d: DateTime<Local>| d.format("%Y-%m-%dT%H:%M:%S").to_string();
    Event {
        id: format!("{uid}|{}", iso(start)),
        subject: text("SUMMARY"),
        start: iso(start),
        end: iso(end),
        all_day,
        location: text("LOCATION"),
        organizer,
        busy: if raw.get("TRANSP").is_some_and(|p| p.value.eq_ignore_ascii_case("TRANSPARENT")) { 0 } else { 2 },
        response: 0,
        teams: teams_re().find(&raw.text).map(|m| m.as_str().to_owned()).unwrap_or_default(),
    }
}

/// The events of the calendar at `url` that touch the `days` days from `from` (local time).
pub fn events(url: &str, from: NaiveDate, days: i64) -> Result<Vec<Event>, String> {
    let text = fetch(url)?;
    let win_start = from.and_hms_opt(0, 0, 0).and_then(|n| n.and_local_timezone(Local).earliest()).ok_or("Data inválida.")?;
    let win_end = win_start + Duration::days(days);
    let raws = events_of(&text);

    // Single occurrences of a series that were moved or edited: (UID, original start) → the edited event.
    let mut moved: HashMap<(String, String), &Raw> = HashMap::new();
    for r in &raws {
        if let (Some(uid), Some(rid)) = (r.get("UID"), r.get("RECURRENCE-ID")) {
            if let Some(w) = parse_when(rid) {
                moved.insert((uid.value.clone(), w.at.format("%Y%m%dT%H%M%S").to_string()), r);
            }
        }
    }

    let mut out: Vec<Event> = Vec::new();
    let mut place = |raw: &Raw, start: DateTime<Local>, end: DateTime<Local>, all_day: bool| {
        if start < win_end && end > win_start && !raw.get("STATUS").is_some_and(|s| s.value.eq_ignore_ascii_case("CANCELLED")) {
            out.push(build(raw, start, end, all_day));
        }
    };

    for r in &raws {
        let Some(ds) = r.get("DTSTART") else { continue };
        let Some(w) = parse_when(ds) else { continue };
        let len = match (r.get("DTEND").and_then(parse_when), r.get("DURATION")) {
            (Some(e), _) => (e.at - w.at).max(Duration::zero()),
            (None, Some(d)) => parse_duration(&d.value).unwrap_or_else(Duration::zero),
            (None, None) if w.all_day => Duration::days(1),
            _ => Duration::zero(),
        };
        let len = if w.all_day && len < Duration::days(1) { Duration::days(1) } else { len };
        if r.get("RECURRENCE-ID").is_some() {
            place(r, w.at, w.at + len, w.all_day);
            continue;
        }
        let Some(rule) = r.get("RRULE") else {
            place(r, w.at, w.at + len, w.all_day);
            continue;
        };
        // Series: expand the dates that fall in the week, minus the ones moved elsewhere.
        let mut src = vec![rrule_line("DTSTART", ds, &w), format!("RRULE:{}", rule.value)];
        for ex in r.props.iter().filter(|p| p.name == "EXDATE") {
            // One EXDATE line can carry several comma-separated dates.
            for v in ex.value.split(',') {
                let one = Prop { name: ex.name.clone(), params: ex.params.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), value: v.to_owned() };
                if let Some(ew) = parse_when(&one) {
                    src.push(rrule_line("EXDATE", &one, &ew));
                }
            }
        }
        let Ok(set) = src.join("\n").parse::<RRuleSet>() else { continue };
        let lo = (win_start - len).with_timezone(&Tz::UTC);
        let hi = win_end.with_timezone(&Tz::UTC);
        let uid = r.get("UID").map(|u| u.value.clone()).unwrap_or_default();
        for d in set.after(lo).before(hi).all(400).dates {
            let at = d.with_timezone(&Local);
            if moved.contains_key(&(uid.clone(), at.format("%Y%m%dT%H%M%S").to_string())) {
                continue;
            }
            place(r, at, at + len, w.all_day);
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start));
    Ok(out)
}

/// `PT1H30M`, `P1D`, `PT45M`…
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().trim_start_matches('-');
    let s = s.strip_prefix('P')?;
    let (date, time) = s.split_once('T').unwrap_or((s, ""));
    let mut secs = 0i64;
    let mut num = String::new();
    for c in date.chars() {
        match c {
            'W' => secs += num.parse::<i64>().ok()? * 604800,
            'D' => secs += num.parse::<i64>().ok()? * 86400,
            d if d.is_ascii_digit() => {
                num.push(d);
                continue;
            }
            _ => return None,
        }
        num.clear();
    }
    for c in time.chars() {
        match c {
            'H' => secs += num.parse::<i64>().ok()? * 3600,
            'M' => secs += num.parse::<i64>().ok()? * 60,
            'S' => secs += num.parse::<i64>().ok()?,
            d if d.is_ascii_digit() => {
                num.push(d);
                continue;
            }
            _ => return None,
        }
        num.clear();
    }
    Some(Duration::seconds(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Europe/Lisbon:20261116T093000\r\nDTEND;TZID=Europe/Lisbon:20261116T100000\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEXDATE;TZID=Europe/Lisbon:20261117T093000\r\nSUMMARY:Daily\\, equipa\r\nDESCRIPTION:https://teams.microsoft.com/l/meetup-join/19%3ameeting\\nfim\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20261118\r\nDTEND;VALUE=DATE:20261119\r\nSUMMARY:Feriado\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:c\r\nDTSTART:20261119T100000Z\r\nDURATION:PT1H30M\r\nSTATUS:CANCELLED\r\nSUMMARY:Cancelada\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn week(feed: &str) -> Vec<Event> {
        CACHE.lock().unwrap().push(("https://x.test/c.ics".into(), Instant::now(), feed.to_owned()));
        events("https://x.test/c.ics", NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), 7).unwrap()
    }

    #[test]
    fn expands_and_filters() {
        let ev = week(FEED);
        let subjects: Vec<_> = ev.iter().map(|e| (e.subject.as_str(), e.start.get(..10).unwrap_or(""))).collect();
        // Daily ×3 minus the excluded 17th, the all-day event, and no cancelled one.
        assert_eq!(subjects, vec![("Daily, equipa", "2026-11-16"), ("Feriado", "2026-11-18"), ("Daily, equipa", "2026-11-18")]);
        assert!(ev.iter().any(|e| e.all_day && e.subject == "Feriado"));
        assert!(ev[0].teams.starts_with("https://teams.microsoft.com/l/meetup-join/19%3ameeting"));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1D"), Some(Duration::days(1)));
        assert_eq!(normalize_url("webcal://a/b.ics"), "https://a/b.ics");
    }
}
