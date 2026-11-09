//! The ⌘K palette: one box, on every screen, that opens a ticket by key or number, matches the cached tickets
//! as you type, searches Jira's full text in the background, and runs the app's commands.

use crate::app::Screen;
use crate::jira::Issue;
use std::collections::HashMap;
use std::time::Duration;

/// How long the query sits still before Jira is asked; the cached matches answer on every keystroke.
pub const DEBOUNCE: Duration = Duration::from_millis(150);
/// The most rows the palette lists.
pub const MAX_HITS: usize = 30;
/// The most issues one Jira text search returns.
pub const REMOTE_MAX: usize = 25;
/// The rows an empty query shows: the most recently updated cached tickets.
pub const RECENT: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Screen(Screen),
    Refresh,
    Theme,
    Density,
    Help,
}

/// Every command, its name, and the key that runs it without the palette.
pub const COMMANDS: &[(Command, &str, &str)] = &[
    (Command::Screen(Screen::Issues), "Go to Issues", "⌘1"),
    (Command::Screen(Screen::Grooming), "Go to Grooming", "⌘2"),
    (Command::Screen(Screen::Team), "Go to Team", "⌘3"),
    (Command::Screen(Screen::Roadmap), "Go to Roadmap", "⌘4"),
    (Command::Screen(Screen::Me), "Go to Me", "⌘5"),
    (Command::Screen(Screen::Standup), "Go to Standup", "⌘6"),
    (Command::Refresh, "Refresh", "⌘R"),
    (Command::Theme, "Switch theme", "⌘⇧T"),
    (Command::Density, "Switch density", "⌘⇧D"),
    (Command::Help, "Keyboard shortcuts", "?"),
];

pub fn command_name(c: Command) -> (&'static str, &'static str) {
    COMMANDS.iter().find(|(k, _, _)| *k == c).map(|(_, n, key)| (*n, *key)).unwrap_or(("", ""))
}

#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    /// A ticket by key. `text_only`: Jira's full-text search found it, but not in its key or title.
    Issue {
        key: String,
        text_only: bool,
    },
    Command(Command),
}

/// The ticket a query names outright: a key ("prs-12"), a number in the default project ("12", "#12"), a
/// browse link, or a configured project with its number run on or spaced ("prs12", "prs 12").
pub fn shorthand(query: &str, project: &str, projects: &[String]) -> Option<String> {
    let t = query.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if let Some(key) = crate::filter::parse_goto(t, project) {
        return Some(key);
    }
    let digits = t.len() - t.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, number) = t.split_at(t.len() - digits);
    let prefix = prefix.trim_end();
    let known = projects.iter().chain(std::iter::once(&project.to_string())).any(|p| p.eq_ignore_ascii_case(prefix));
    (digits > 0 && !prefix.is_empty() && known).then(|| format!("{}-{number}", prefix.to_uppercase()))
}

/// The query as words for Jira's text search: Lucene's operators and punctuation become spaces, so a pasted
/// title never errors. None under two characters, for commands (`>`), and for a ticket shorthand.
pub fn remote_term(query: &str, project: &str, projects: &[String]) -> Option<String> {
    let t = query.trim();
    if t.starts_with('>') || shorthand(t, project, projects).is_some() {
        return None;
    }
    let cleaned: String = t.chars().map(|c| if c.is_alphanumeric() || c == '\'' || c == '_' || c == '.' { c } else { ' ' }).collect();
    let words = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (words.chars().count() >= 2).then_some(words)
}

/// Jira's full-text search over the configured projects, all statuses, most recently updated first.
pub fn remote_jql(projects: &[String], project: &str, term: &str) -> String {
    let list: Vec<String> = if projects.is_empty() { vec![project.to_string()] } else { projects.to_vec() };
    let list = list.iter().map(|p| crate::store::jql_quote(p)).collect::<Vec<_>>().join(", ");
    format!("project in ({list}) AND {} ORDER BY updated DESC", crate::store::text_clause(term))
}

fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether `word` starts a word of `hay` (both lowercase).
fn starts_word(hay: &str, word: &str) -> bool {
    hay.match_indices(word).any(|(i, _)| hay[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric()))
}

/// How well a cached ticket matches: every word must appear in its key, title, labels, status, type, assignee
/// or epic; a key or title hit counts most, and a word's start more than its middle. None when a word misses.
pub fn score(issue: &Issue, words: &[String]) -> Option<u32> {
    if words.is_empty() {
        return None;
    }
    let key = issue.key.to_lowercase();
    let summary = issue.summary.to_lowercase();
    let rest = [
        issue.labels.join(" "),
        issue.status.name.clone(),
        issue.issue_type.clone(),
        issue.assignee.as_ref().map(|u| u.display_name.clone()).unwrap_or_default(),
        issue.parent_summary.clone().unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase();
    let mut total = 0;
    for w in words {
        total += if key == *w {
            200
        } else if key.starts_with(w.as_str()) {
            80
        } else if starts_word(&summary, w) {
            30
        } else if summary.contains(w.as_str()) {
            15
        } else if rest.contains(w.as_str()) {
            5
        } else {
            return None;
        };
    }
    let phrase = words.join(" ");
    if words.len() > 1 && summary.contains(&phrase) {
        total += 40;
    }
    if summary.starts_with(&phrase) {
        total += 20;
    }
    Some(total)
}

/// The palette's rows for `query`, best first: the ticket a shorthand names, the cached tickets that match
/// (best score, then most recently updated) with matching commands after them (before them when none match), then what Jira's full-text search found that
/// the cache did not (`remote`, in Jira's order). An empty query lists the most recently updated tickets; `>`
/// lists the commands only.
pub fn hits(query: &str, project: &str, projects: &[String], issues: &HashMap<String, Issue>, remote: &[String]) -> Vec<Hit> {
    let q = query.trim();
    if let Some(cmd) = q.strip_prefix('>') {
        let w = words(cmd);
        return COMMANDS.iter().filter(|(_, name, _)| command_matches(name, &w)).map(|(c, _, _)| Hit::Command(*c)).collect();
    }
    let mut out = Vec::new();
    if q.is_empty() {
        let mut recent: Vec<&Issue> = issues.values().collect();
        recent.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.key.cmp(&b.key)));
        out.extend(recent.into_iter().take(RECENT).map(|i| Hit::Issue { key: i.key.clone(), text_only: false }));
        return out;
    }
    let exact = shorthand(q, project, projects);
    if let Some(k) = &exact {
        out.push(Hit::Issue { key: k.clone(), text_only: false });
    }
    let w = words(q);
    let commands: Vec<Hit> = if q.chars().count() >= 2 {
        COMMANDS.iter().filter(|(_, name, _)| command_matches(name, &w)).map(|(c, _, _)| Hit::Command(*c)).collect()
    } else {
        Vec::new()
    };
    let mut scored: Vec<(u32, &Issue)> = issues.values().filter(|i| Some(&i.key) != exact.as_ref()).filter_map(|i| score(i, &w).map(|s| (s, i))).collect();
    scored.sort_by(|(sa, a), (sb, b)| sb.cmp(sa).then_with(|| b.updated.cmp(&a.updated)).then_with(|| a.key.cmp(&b.key)));
    let listed: Vec<String> = scored.iter().map(|(_, i)| i.key.clone()).collect();
    // A command leads only when no ticket matches ("groo"); otherwise it follows them, so Enter on a common
    // word ("the") opens a ticket rather than running "Switch theme".
    if exact.is_none() && listed.is_empty() {
        out.extend(commands.iter().cloned());
    }
    out.extend(listed.iter().map(|k| Hit::Issue { key: k.clone(), text_only: false }));
    if exact.is_some() || !listed.is_empty() {
        out.extend(commands);
    }
    for k in remote {
        if Some(k) != exact.as_ref() && !listed.contains(k) {
            out.push(Hit::Issue { key: k.clone(), text_only: true });
        }
    }
    out.truncate(MAX_HITS);
    out
}

/// Every query word starts a word of the command's name ("groo" → Go to Grooming); an empty query matches all.
fn command_matches(name: &str, words: &[String]) -> bool {
    let name = name.to_lowercase();
    words.iter().all(|w| starts_word(&name, w))
}

/// The cursor after a move of `delta` over `n` rows, wrapping at both ends.
pub fn step(cursor: usize, delta: i32, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (cursor as i64 + i64::from(delta)).rem_euclid(n as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Status, User};

    fn issue(key: &str, summary: &str, updated: &str) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            updated: updated.into(),
            status: Status { name: "In Progress".into(), category: "indeterminate".into() },
            ..Default::default()
        }
    }

    fn cache(items: &[Issue]) -> HashMap<String, Issue> {
        items.iter().map(|i| (i.key.clone(), i.clone())).collect()
    }

    fn keys(h: &[Hit]) -> Vec<String> {
        h.iter()
            .map(|h| match h {
                Hit::Issue { key, text_only: false } => key.clone(),
                Hit::Issue { key, text_only: true } => format!("{key}*"),
                Hit::Command(c) => command_name(*c).0.to_string(),
            })
            .collect()
    }

    fn projects() -> Vec<String> {
        vec!["PRS".into(), "CI".into()]
    }

    #[test]
    fn shorthand_takes_keys_numbers_and_known_projects() {
        let p = projects();
        assert_eq!(shorthand("123", "PRS", &p).as_deref(), Some("PRS-123"));
        assert_eq!(shorthand("#123", "PRS", &p).as_deref(), Some("PRS-123"));
        assert_eq!(shorthand("ci-7", "PRS", &p).as_deref(), Some("CI-7"));
        assert_eq!(shorthand("ci7", "PRS", &p).as_deref(), Some("CI-7"));
        assert_eq!(shorthand("prs 42", "PRS", &p).as_deref(), Some("PRS-42"));
        assert_eq!(shorthand("https://x.atlassian.net/browse/CI-9", "PRS", &p).as_deref(), Some("CI-9"));
        assert_eq!(shorthand("ipv6", "PRS", &p), None, "an unknown project is just a word");
        assert_eq!(shorthand("canary", "PRS", &p), None);
        assert_eq!(shorthand("", "PRS", &p), None);
    }

    #[test]
    fn remote_term_strips_operators_and_skips_shorthand() {
        let p = projects();
        assert_eq!(remote_term("  disk (full)! ", "PRS", &p).as_deref(), Some("disk full"));
        assert_eq!(remote_term("a", "PRS", &p), None);
        assert_eq!(remote_term("123", "PRS", &p), None, "a number opens the ticket");
        assert_eq!(remote_term(">theme", "PRS", &p), None);
        assert_eq!(remote_term("-+*", "PRS", &p), None);
        assert_eq!(remote_jql(&p, "PRS", "disk full"), r#"project in ("PRS", "CI") AND text ~ "disk full*" ORDER BY updated DESC"#);
        assert_eq!(remote_jql(&[], "PRS", "x1"), r#"project in ("PRS") AND text ~ "x1*" ORDER BY updated DESC"#);
    }

    #[test]
    fn scores_rank_key_then_title_start_then_middle() {
        let w = |q: &str| words(q);
        let i = issue("PRS-12", "Disk full on canary nodes", "");
        assert!(score(&i, &w("prs-12")) > score(&i, &w("disk")));
        assert!(score(&i, &w("disk")) > score(&i, &w("isk")));
        assert!(score(&i, &w("isk")) > score(&i, &w("progress")), "status counts least");
        assert_eq!(score(&i, &w("disk zebra")), None, "every word must match");
        assert!(score(&i, &w("full on")) > score(&i, &w("on full")), "the phrase in order counts");
    }

    #[test]
    fn hits_order_shorthand_commands_cache_then_full_text() {
        let c = cache(&[issue("PRS-1", "Grow the disk", "2026-01-01"), issue("PRS-2", "Disk alerts", "2026-02-01"), issue("PRS-3", "Unrelated", "2026-03-01")]);
        assert_eq!(keys(&hits("disk", "PRS", &projects(), &c, &["PRS-3".into(), "PRS-1".into(), "PRS-9".into()])), ["PRS-2", "PRS-1", "PRS-3*", "PRS-9*"]);
        assert_eq!(keys(&hits("3", "PRS", &projects(), &c, &[])), ["PRS-3"]);
        assert_eq!(keys(&hits("groo", "PRS", &projects(), &c, &[])), ["Go to Grooming"]);
        assert_eq!(keys(&hits("grow", "PRS", &projects(), &c, &[])), ["PRS-1"]);
        let t = cache(&[issue("PRS-7", "Update the theme docs", "")]);
        assert_eq!(keys(&hits("theme", "PRS", &projects(), &t, &[])), ["PRS-7", "Switch theme"], "a ticket match leads");
        assert_eq!(keys(&hits("", "PRS", &projects(), &c, &[])), ["PRS-3", "PRS-2", "PRS-1"], "empty: most recently updated");
        assert_eq!(keys(&hits("> them", "PRS", &projects(), &c, &[])), ["Switch theme"]);
        assert_eq!(hits(">", "PRS", &projects(), &c, &[]).len(), COMMANDS.len());
    }

    #[test]
    fn hits_match_labels_assignee_and_epic() {
        let mut i = issue("PRS-5", "Rotate keys", "");
        i.labels = vec!["PLAT".into()];
        i.assignee = Some(User { account_id: "a".into(), display_name: "Ada Lovelace".into() });
        let c = cache(&[i]);
        assert_eq!(keys(&hits("ada rotate", "PRS", &projects(), &c, &[])), ["PRS-5"]);
        assert_eq!(keys(&hits("plat", "PRS", &projects(), &c, &[])), ["PRS-5"]);
    }

    #[test]
    fn step_wraps() {
        assert_eq!(step(0, -1, 3), 2);
        assert_eq!(step(2, 1, 3), 0);
        assert_eq!(step(1, 1, 3), 2);
        assert_eq!(step(0, 1, 0), 0);
    }
}
