//! The ⌘K palette's search: cached matches every frame, Jira's full-text search once the query sits still.

use super::App;
use super::actions::Action;
use super::msg::Msg;
use crate::jira;
use crate::palette::{self, Command, Hit};
use std::time::Duration;

/// What one palette query asks Jira for.
enum Remote {
    /// A shorthand key the cache does not hold: fetch it, so its row shows a title.
    Key(String),
    /// A full-text search.
    Text(String),
}

impl App {
    /// The palette's rows for its current query (see `palette::hits`). Jira's answer counts only for the query
    /// it was sent for.
    pub fn palette_hits(&self) -> Vec<Hit> {
        let Some(pal) = &self.palette else { return Vec::new() };
        let q = pal.query.trim();
        let remote: &[String] = if pal.remote_for.as_deref() == Some(q) { &pal.remote } else { &[] };
        palette::hits(q, &self.cfg.scope.project, &self.cfg.scope.projects, &self.store.issues, remote)
    }

    /// Each frame with the palette open: once the query has sat still for the debounce, ask Jira for it (once
    /// per query). A newer query bumps `palette_seq`, so an older answer is dropped when it lands.
    pub(super) fn palette_tick(&mut self, ctx: &egui::Context) {
        let Some(pal) = &self.palette else { return };
        let q = pal.query.trim().to_string();
        if pal.sent.as_deref() == Some(q.as_str()) {
            return;
        }
        let since = pal.edited.map_or(Duration::MAX, |t| t.elapsed());
        if since < palette::DEBOUNCE {
            ctx.request_repaint_after(palette::DEBOUNCE - since);
            return;
        }
        let (project, projects) = (&self.cfg.scope.project, &self.cfg.scope.projects);
        let remote = match palette::shorthand(&q, project, projects) {
            Some(k) if self.store.issues.contains_key(&k) => None,
            Some(k) => Some(Remote::Key(k)),
            None => palette::remote_term(&q, project, projects).map(|t| Remote::Text(palette::remote_jql(projects, project, &t))),
        };
        self.palette_seq += 1;
        let seq = self.palette_seq;
        let client = self.client.is_some();
        let Some(pal) = self.palette.as_mut() else { return };
        pal.sent = Some(q.clone());
        pal.error = None;
        // Without a client nothing is spawned, so nothing would ever clear `pending`.
        pal.pending = remote.is_some() && client;
        let Some(remote) = remote.filter(|_| client) else { return };
        self.spawn(move |c, tx| {
            let result = match remote {
                Remote::Key(k) => c.issue(&k, &[]).map(|i| vec![i]),
                Remote::Text(jql) => c.search_first(&jql, jira::LIST_FIELDS, palette::REMOTE_MAX),
            };
            let _ = tx.send(Msg::PaletteHits { seq, query: q, result });
        });
    }

    /// Jira answered a palette query: its issues join the cache (a cached one is kept, since it may hold the
    /// detail), and its keys list under the cached matches while the query is unchanged.
    pub(super) fn palette_answer(&mut self, seq: u64, query: String, result: jira::Result<Vec<jira::Issue>>) {
        if seq != self.palette_seq {
            return;
        }
        let Some(pal) = self.palette.as_mut() else { return };
        pal.pending = false;
        match result {
            Ok(issues) => {
                pal.remote = issues.iter().map(|i| i.key.clone()).collect();
                pal.remote_for = Some(query);
                for i in issues {
                    self.store.issues.entry(i.key.clone()).or_insert(i);
                }
            }
            Err(e) => pal.error = Some(palette_error(&e)),
        }
    }

    pub(super) fn palette_move(&mut self, delta: i32) {
        let n = self.palette_hits().len();
        if let Some(pal) = self.palette.as_mut() {
            pal.cursor = palette::step(pal.cursor.min(n.saturating_sub(1)), delta, n);
            pal.scroll = true;
        }
    }

    /// Opens the row under the cursor: a ticket on the Issues screen (or, `browser`, in Jira), or a command.
    pub(super) fn palette_confirm(&mut self, browser: bool, ctx: &egui::Context) {
        let hits = self.palette_hits();
        let Some(pal) = &self.palette else { return };
        let Some(hit) = hits.get(pal.cursor.min(hits.len().saturating_sub(1))).cloned() else { return };
        self.palette = None;
        match hit {
            Hit::Issue { key, .. } if browser => self.actions.push(Action::OpenUrl(self.cfg.browse_url(&key))),
            Hit::Issue { key, .. } => self.open_in_issues(key, ctx),
            Hit::Command(c) => self.actions.push(command_action(c)),
        }
    }
}

fn command_action(c: Command) -> Action {
    match c {
        Command::Screen(s) => Action::SetScreen(s),
        Command::Refresh => Action::Refresh,
        Command::Theme => Action::CycleTheme,
        Command::Density => Action::ToggleDensity,
        Command::Help => Action::ToggleHelp,
    }
}

/// A failed palette search, short: the footer has one line.
fn palette_error(e: &jira::Error) -> String {
    match e.to_string() {
        s if s.contains("returned 404") => "Jira has no such issue.".to_string(),
        s => format!("Jira search failed: {}", s.lines().next().unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PaletteState, Screen};
    use crate::config::Config;
    use crate::jira::Issue;
    use std::time::Instant;

    fn cfg() -> Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n[scope]\nproject = \"CI\"\nprojects = [\"CI\"]\n").unwrap()
    }

    fn issue(key: &str, summary: &str) -> Issue {
        Issue { key: key.into(), summary: summary.into(), ..Default::default() }
    }

    fn open(app: &mut App, q: &str) {
        app.palette = Some(PaletteState { query: q.into(), edited: Some(Instant::now() - Duration::from_secs(1)), ..Default::default() });
    }

    #[test]
    fn only_the_latest_answer_lands() {
        let mut app = App::for_test(cfg(), None);
        open(&mut app, "disk");
        app.palette_seq = 2;
        app.palette_answer(1, "disk".into(), Ok(vec![issue("CI-1", "Old")]));
        assert!(app.palette.as_ref().unwrap().remote.is_empty(), "a stale answer is dropped");
        app.palette_answer(2, "disk".into(), Ok(vec![issue("CI-2", "Disk full")]));
        assert_eq!(app.palette_hits(), vec![Hit::Issue { key: "CI-2".into(), text_only: false }]);
        assert!(app.store.issues.contains_key("CI-2"), "Jira's answer joins the cache");
    }

    #[test]
    fn an_answer_for_an_older_query_does_not_list() {
        let mut app = App::for_test(cfg(), None);
        open(&mut app, "disk full");
        app.palette_seq = 1;
        app.palette_answer(1, "disk".into(), Ok(vec![issue("CI-3", "Nothing in the title")]));
        assert!(app.palette_hits().is_empty(), "the description match was for another query");
    }

    #[test]
    fn without_a_client_nothing_stays_pending() {
        let mut app = App::for_test(cfg(), None);
        open(&mut app, "disk");
        app.palette_tick(&egui::Context::default());
        let pal = app.palette.as_ref().unwrap();
        assert_eq!(pal.sent.as_deref(), Some("disk"));
        assert!(!pal.pending);
    }

    #[test]
    fn confirm_opens_the_ticket_on_issues() {
        let mut app = App::for_test(cfg(), None);
        let ctx = egui::Context::default();
        app.screen = Screen::Team;
        open(&mut app, "77");
        app.palette_confirm(false, &ctx);
        assert_eq!(app.screen, Screen::Issues);
        assert_eq!(app.selected.as_deref(), Some("CI-77"));
        assert!(app.palette.is_none());
    }

    #[test]
    fn confirm_runs_a_command_or_opens_jira() {
        let mut app = App::for_test(cfg(), None);
        let ctx = egui::Context::default();
        open(&mut app, ">roadmap");
        app.palette_confirm(false, &ctx);
        assert!(matches!(app.actions.as_slice(), [Action::SetScreen(Screen::Roadmap)]));
        app.actions.clear();
        open(&mut app, "CI-5");
        app.palette_confirm(true, &ctx);
        assert!(matches!(app.actions.as_slice(), [Action::OpenUrl(u)] if u == "https://x.atlassian.net/browse/CI-5"));
    }

    #[test]
    fn move_wraps_over_the_rows() {
        let mut app = App::for_test(cfg(), None);
        app.store.issues.insert("CI-1".into(), issue("CI-1", "disk a"));
        app.store.issues.insert("CI-2".into(), issue("CI-2", "disk b"));
        open(&mut app, "disk");
        app.palette_move(-1);
        assert_eq!(app.palette.as_ref().unwrap().cursor, 1);
        app.palette_move(1);
        assert_eq!(app.palette.as_ref().unwrap().cursor, 0);
    }
}
