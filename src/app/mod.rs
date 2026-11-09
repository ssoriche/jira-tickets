//! App state and the main loop. Network calls run on background threads and
//! send a `Msg` back over a channel; the UI never waits on the network.

mod actions;
mod groom;
mod loads;
mod msg;
mod palette;
mod pickers;
mod shortcuts;
mod standup;

pub use actions::Action;
pub use msg::Msg;
pub use pickers::{
    ASSIGN_USERS, POINTS_HINT, PRIORITY_HINT, Picker, assign_current_row, assign_cursor_after_filter, assign_matches, assign_note, assign_rows,
    is_done_category, sprint_current_row, sprint_row_label, transition_row,
};

pub use standup::{StandupPhase, StandupState, target_sprint};

use shortcuts::close_shortcut;

use crate::auth::{self, Login};
use crate::config::Config;
use crate::jira::{self, Client, Sprint, User};
use crate::store::{ListId, Store};
use crate::theme::{Appearance, SavedAppearance};
use crate::{theme, views};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(5 * 60);
/// The disk cache key of the appearance chosen in the app (`SavedAppearance`).
const APPEARANCE_KEY: &str = "appearance";

/// A write run on the ordered write worker.
type WriteJob = Box<dyn FnOnce(&Client, &Sender<Msg>) + Send>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Issues,
    Grooming,
    Team,
    Roadmap,
    Me,
    Standup,
}

#[derive(Default)]
pub struct Loaded {
    pub loading: bool,
    /// A load was asked for while one was in flight; run it when that one lands.
    pub reload_after: bool,
    pub error: Option<String>,
    pub fetched: Option<Instant>,
}

/// What a toast reports: its dot is green, accent, or red.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    /// A notice, not a failure (read-only refusals, "No changes").
    Info,
    Error,
}

/// The Grooming screen's two modes: walking `New` before the meeting, and the epic-by-epic session.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GroomMode {
    #[default]
    PreCheck,
    Session,
}

/// Everything the Grooming screen keeps between frames.
#[derive(Default)]
pub struct GroomingState {
    pub mode: GroomMode,
    /// The newest report in `report_dir`, read on every refresh.
    pub report: Option<(PathBuf, crate::report::Report)>,
    /// Pre-check verdicts by issue key, pre-filled from the report.
    pub verdicts: BTreeMap<String, crate::grooming::Verdict>,
    /// The pre-check row, or the session candidate index.
    pub cursor: usize,
    /// Session: the selected epic in the rail.
    pub epic_index: usize,
    /// Session: the ghost row's index in the Ready queue while accepting.
    pub placement: Option<usize>,
    /// Session: the key the open placement or bounce box is for.
    pub decision_for: Option<String>,
    /// Session: the open bounce box's text.
    pub bounce_text: Option<String>,
    /// Session: the bounce box takes keyboard focus on its next frame.
    pub bounce_focus: bool,
    /// Session: the ticket whose Accept, Bounce or Park is in flight; no other decision starts meanwhile.
    pub deciding: Option<String>,
    /// Session: a sprint move in flight, as (key, sprint name or None for the backlog); applied locally when it lands.
    pub pending_sprint_move: Option<(String, Option<String>)>,
    /// When the focused ticket took focus, for the per-ticket timer.
    pub focused_since: Option<Instant>,
    /// The pre-check summary dialog is open.
    pub summary_open: bool,
    /// The pre-check ticket whose detail was last asked for on focus.
    pub detail_for: Option<String>,
    /// A Promote is in flight: from its first transition until the report is written (or waits on the confirm).
    pub promoting: bool,
    /// Promote's transitions that have not answered yet.
    pub promote_remaining: usize,
    /// What the running Promote acts on, taken when it started.
    pub promote_run: Option<PromoteRun>,
    /// (key, summary, error) for each transition the last Promote could not make.
    pub promote_failures: Vec<(String, String, String)>,
    /// The text of the last report Promote rendered, for copy.
    pub last_summary: Option<String>,
    /// A rendered report waiting on "Report for today exists. Overwrite?".
    pub pending_report: Option<String>,
    /// Keys this run's Promote moved to Planning. They stay out of the pre-check and its report prefill until a
    /// team list shows them out of `New`, so an eventually consistent search cannot offer them again.
    pub retired: std::collections::HashSet<String>,
}

/// Everything the Team screen keeps between frames.
#[derive(Default)]
pub struct TeamState {
    /// The focused column (`teamview::columns` order, Unassigned last).
    pub col: usize,
    /// The focused card in that column: its in-progress cards, then its ready ones.
    pub row: usize,
    /// Member ids whose "Done in the last 7 days" footer is open.
    pub done_open: HashSet<String>,
    /// Each member's recently done tickets, by member id, loaded when the footer opens and reloaded by every
    /// refresh while it stays open (a refresh drops a closed footer's list).
    pub done: HashMap<String, Vec<jira::Issue>>,
    /// Member ids with a done load in flight.
    pub done_loading: HashSet<String>,
    /// How many tickets each member finished in the last 7 days, by member id, loaded for every member on each
    /// refresh, so a closed footer can show its count. Kept through a refresh until the new count lands.
    pub done_count: HashMap<String, usize>,
    /// Member ids with a done count load in flight.
    pub done_count_loading: HashSet<String>,
}

/// Everything the Me screen keeps between frames.
#[derive(Default)]
pub struct MeState {
    /// The focused row, counting down through the sections (`me::issue_at`).
    pub cursor: usize,
}

/// Everything the Roadmap screen keeps between frames.
pub struct RoadmapState {
    /// The tab, 1 to 4: Now / Next / Later, Timeline, Status review, Outcomes.
    pub tab: u8,
    /// The focused row of the tab (`views::roadmap::tab_rows`).
    pub cursor: usize,
}

impl Default for RoadmapState {
    fn default() -> Self {
        RoadmapState { tab: 1, cursor: 0 }
    }
}

/// A Promote's inputs, frozen when it starts, so a refresh meanwhile cannot change the report.
pub struct PromoteRun {
    pub date: String,
    /// Every candidate in rank order.
    pub rows: Vec<crate::grooming::RunRow>,
    /// The keys being transitioned, in rank order.
    pub promoted: Vec<String>,
    pub held_back: usize,
    pub needs_detail: Vec<(String, Vec<crate::grooming::Code>)>,
}

/// The ⌘K palette's query, cursor, and the Jira search answering it.
#[derive(Default)]
pub struct PaletteState {
    pub query: String,
    pub cursor: usize,
    /// The cursor moved by key, so its row scrolls into view on the next frame.
    pub scroll: bool,
    /// When the query last changed, for the debounce.
    pub edited: Option<Instant>,
    /// The query the last Jira search was sent for (or skipped for: too short, a command, a cached key).
    pub sent: Option<String>,
    /// What Jira's search for `remote_for` found, in its order.
    pub remote: Vec<String>,
    pub remote_for: Option<String>,
    /// A Jira search for the current query is in flight.
    pub pending: bool,
    pub error: Option<String>,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: Instant,
}

pub struct App {
    pub cfg: Arc<Config>,
    pub store: Store,
    pub lists: HashMap<ListId, Loaded>,
    pub screen: Screen,
    pub me: Option<User>,
    pub login: Option<Login>,
    pub finding_token: bool,
    pub token_input: String,
    pub token_error: Option<String>,
    pub toast: Option<Toast>,
    pub actions: Vec<Action>,
    /// Set when a refresh failed on the network; the banner shows the store's fetched_at.
    pub offline: bool,
    pub selected: Option<String>,
    /// Issue keys with a detail fetch in flight.
    pub detail_loading: std::collections::HashSet<String>,
    /// When each issue's detail last arrived.
    detail_fetched: HashMap<String, Instant>,
    pub filter: crate::filter::IssueFilter,
    /// The board's active and future sprints in Jira's order, for the sprint scope picker.
    pub sprints: Vec<Sprint>,
    /// The sprint scope has been defaulted to the active sprint once; later refreshes keep the user's choice.
    pub sprint_default_applied: bool,
    /// The board's sprints have answered (or failed) once, so the sprint combo shows the real scope.
    pub sprints_answered: bool,
    /// The sprint scope came from the last run's cached sprint list (`filter::launch_sprint_scope`), so the combo
    /// and epic hues treat it as known before the board answers.
    pub sprints_cached: bool,
    /// Set by ⌘F or `/`; the search box takes focus on its next frame.
    pub focus_search: bool,
    /// The ⌘K palette while it is open, on any screen.
    pub palette: Option<PaletteState>,
    /// Bumped by every palette search sent; answers to older ones are dropped.
    pub palette_seq: u64,
    /// Keyboard cursor: an index into the Issues view's current rows.
    pub cursor: usize,
    /// The cursor the list last drew; when it differs the row scrolls into view.
    pub last_cursor: usize,
    /// The list shows ListId::Search instead of TeamOpen.
    pub search_active: bool,
    /// Bumped by every `RunSearch`; Search results from older runs are dropped.
    pub search_seq: u64,
    /// The seq of the search in flight; the list keeps its previous rows until it lands.
    pub search_pending: Option<u64>,
    /// The search in flight was rerun by a header refresh (⌘R), so the header spinner, not the count row, shows it.
    pub search_from_refresh: bool,
    /// The JQL of the running search, for Retry.
    pub last_search_jql: Option<String>,
    /// The active search was typed into the search box (not a quick filter or team scope).
    pub typed_search: bool,
    /// The search rows on screen came from a typed JQL search (set when its result is accepted),
    /// so the count row says "· JQL" only once those rows are showing.
    pub typed_shown: bool,
    /// When the search box text last changed, for the text search's debounce.
    pub text_edited: Option<Instant>,
    /// The term of the active text search (in flight or shown); None when the Search list is not one.
    pub text_search: Option<String>,
    /// The full scope + text JQL that search ran, so a scope change (or members dropping out) reruns it.
    pub text_search_jql: Option<String>,
    /// The term whose server rows are on screen (set when its result is accepted). While these rows
    /// match the box, the local text filter steps aside: the server already matched more than summary.
    pub text_shown: Option<String>,
    /// Jira rejected a configured member, so team queries match labels only until restart.
    pub members_disabled: bool,
    /// Each epic's hue slot, given to the epics on screen first and kept for the session (`theme::assign_epic_slots`).
    pub epic_slots: std::collections::BTreeMap<String, usize>,
    /// The slots above were given from the resolved sprint scope's rows and are kept from now on.
    pub epic_slots_final: bool,
    /// Unposted comment text, keyed by issue key.
    pub drafts: HashMap<String, String>,
    /// The issue key of the comment post in flight.
    pub posting: Option<String>,
    pub picker: Option<Picker>,
    /// The detail pane's x-range on the Issues screen last frame; the toast centres over it.
    pub detail_x: Option<(f32, f32)>,
    /// The detail pane's pill row top (its fill's edge) when it showed in the pane's viewport last frame; pickers
    /// anchor their dialog over it.
    pub detail_pills_top: Option<f32>,
    /// The keyboard help overlay is showing; it owns the keyboard while open.
    pub help_open: bool,
    /// A picker's items are being fetched; a spinner shows meanwhile.
    pub picker_loading: bool,
    /// (issue key, transition id) of the transition in flight, so a
    /// "resolution required" error can retry it with a resolution.
    pending_transition: Option<(String, String)>,
    /// Optimistic rank swaps awaiting their Done: (list, issue, neighbour), oldest first.
    pub(super) pending_ranks: Vec<(ListId, String, String)>,
    pub grooming: GroomingState,
    pub team: TeamState,
    pub roadmap: RoadmapState,
    /// The colour scheme and density in use (`theme::effective_appearance`, then ⌘⇧T / ⌘⇧D).
    pub appearance: Appearance,
    pub me_view: MeState,
    pub standup: StandupState,
    /// Runs the Standup review's `claude`; tests swap in a stand-in.
    pub(crate) review_runner: standup::ReviewRunner,
    /// Today's date for the Standup review's window; tests pin it.
    pub(crate) today: fn() -> chrono::NaiveDate,
    /// How many report reads were started, for tests.
    #[cfg(test)]
    pub(crate) report_reads: usize,
    pub(crate) client: Option<Arc<Client>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Feeds the single write worker, so writes reach Jira in the order made.
    writes: Sender<(Arc<Client>, WriteJob)>,
    /// The write worker's end of `writes`, held until the first write starts the worker.
    write_rx: Option<Receiver<(Arc<Client>, WriteJob)>>,
    /// Results are saved to the disk cache; off in `for_test`, so tests never touch the real cache.
    persist_cache: bool,
    ctx: egui::Context,
    last_refresh: Instant,
    was_focused: bool,
}

impl App {
    /// True once the lists `screen` draws have loaded; snapshots wait for it. Every screen needs the first team
    /// list; Me needs its own list; the Roadmap also needs the epics and, when any epic is an initiative, their children (the progress).
    #[cfg(feature = "snapshot")]
    pub fn shot_ready(&self, screen: Screen) -> bool {
        use crate::store::ListId;
        let fetched = |id: ListId| self.lists.get(&id).is_some_and(|l| l.fetched.is_some());
        if !fetched(ListId::TeamOpen) {
            return false;
        }
        if screen == Screen::Me {
            return fetched(ListId::Me);
        }
        if screen != Screen::Roadmap {
            return true;
        }
        if !fetched(ListId::Epics) {
            return false;
        }
        let epics = self.store.list(ListId::Epics);
        let (inits, _) = crate::roadmap::select(&epics, &self.cfg.roadmap.initiative_label);
        inits.is_empty() || fetched(ListId::EpicChildren)
    }

    /// Every field at its starting value, with no window behind it: no theme, no cache read, no token search,
    /// and the write worker not yet started.
    fn empty(cfg: Arc<Config>, tx: Sender<Msg>, rx: Receiver<Msg>, ctx: egui::Context) -> App {
        let (writes, write_rx) = channel::<(Arc<Client>, WriteJob)>();
        let filter = crate::filter::IssueFilter {
            team: crate::filter::TeamScope::Label(cfg.scope.team_label.clone()),
            team_labels: cfg.scope.team_labels(),
            team_members: cfg.scope.team_members(&cfg.team.members),
            ..Default::default()
        };
        App {
            cfg,
            store: Store::default(),
            lists: HashMap::new(),
            screen: Screen::Issues,
            me: None,
            login: None,
            finding_token: true,
            token_input: String::new(),
            token_error: None,
            toast: None,
            actions: Vec::new(),
            offline: false,
            selected: None,
            detail_loading: Default::default(),
            detail_fetched: HashMap::new(),
            filter,
            sprints_cached: false,
            sprints: Vec::new(),
            sprint_default_applied: false,
            sprints_answered: false,
            focus_search: false,
            palette: None,
            palette_seq: 0,
            cursor: 0,
            last_cursor: 0,
            search_active: false,
            search_seq: 0,
            search_pending: None,
            search_from_refresh: false,
            detail_x: None,
            detail_pills_top: None,
            last_search_jql: None,
            typed_search: false,
            typed_shown: false,
            text_edited: None,
            text_search: None,
            text_search_jql: None,
            text_shown: None,
            members_disabled: false,
            epic_slots: Default::default(),
            epic_slots_final: false,
            drafts: HashMap::new(),
            posting: None,
            picker: None,
            help_open: false,
            picker_loading: false,
            pending_transition: None,
            pending_ranks: Vec::new(),
            grooming: GroomingState::default(),
            team: TeamState::default(),
            roadmap: RoadmapState::default(),
            appearance: Appearance::default(),
            me_view: MeState::default(),
            standup: StandupState::default(),
            review_runner: crate::standup::run_review,
            today: || chrono::Local::now().date_naive(),
            #[cfg(test)]
            report_reads: 0,
            client: None,
            tx,
            rx,
            writes,
            write_rx: Some(write_rx),
            persist_cache: false,
            ctx,
            last_refresh: Instant::now(),
            was_focused: true,
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config) -> Self {
        theme::apply(&cc.egui_ctx);
        let saved: Option<SavedAppearance> = crate::cache::load(APPEARANCE_KEY);
        let appearance = launch_appearance(cfg.appearance, saved, std::env::var("JIRA_TICKETS_THEME").ok(), std::env::var("JIRA_TICKETS_DENSITY").ok());
        apply_appearance(&cc.egui_ctx, appearance);
        if std::env::var_os("JIRA_TICKETS_READONLY").is_some() {
            jira::READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let (tx, rx) = channel();
        let cfg = Arc::new(cfg);
        let mut app = App::empty(cfg.clone(), tx, rx, cc.egui_ctx.clone());
        app.persist_cache = true;
        app.appearance = appearance;
        app.store = Store::for_launch(Store::load_cached());
        let cached_sprints: Vec<Sprint> = crate::cache::load(&sprints_cache_key(cfg.scope.board_id)).unwrap_or_default();
        let launch_scope = crate::filter::launch_sprint_scope(&cached_sprints);
        if let Some(scope) = &launch_scope {
            app.filter.sprint = scope.clone();
        }
        app.sprints_cached = launch_scope.is_some();
        app.sprints = cached_sprints;
        app.standup.saved = crate::cache::load(crate::standup::CACHE_KEY).unwrap_or_default();
        // Finding the token may run `op read`, which can block; keep it off the UI thread.
        let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Login(auth::find_login(&cfg.email, cfg.api_token.as_deref(), cfg.op_account.as_deref())));
            ctx.request_repaint();
        });
        app
    }

    /// A headless App for unit tests: no window, no theme, optional client. A config left on the default
    /// `report_dir` reads reports from a path that does not exist, so no test sees the real ~/Documents folder.
    #[cfg(test)]
    pub fn for_test(mut cfg: Config, client: Option<Arc<Client>>) -> App {
        if cfg.grooming.report_dir == crate::config::Grooming::default().report_dir {
            cfg.grooming.report_dir = std::env::temp_dir().join("jira-tickets-for-test").join("no-reports").to_string_lossy().into_owned();
        }
        let (tx, rx) = channel();
        let mut app = App::empty(Arc::new(cfg), tx, rx, egui::Context::default());
        app.client = client;
        app.finding_token = false;
        app
    }

    /// Handles every result that has arrived, then applies the queued actions (a test's frame without the views).
    #[cfg(test)]
    pub fn drain(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        let ctx = self.ctx.clone();
        for a in std::mem::take(&mut self.actions) {
            self.apply(a, &ctx);
        }
    }

    /// `D` (Shift+D, no other modifier) moves the screen's focused ticket to Done (`done_target`) on every screen.
    /// Checked before the screens' own keys, which would read Shift+D as a plain `d` (Team's done footer). Help,
    /// an open picker or the ⌘K palette, a focused text field, and Grooming's open dialogs keep the key for themselves.
    pub(super) fn done_shortcut(&mut self, ctx: &egui::Context) {
        let g = &self.grooming;
        let modal = g.pending_report.is_some() || g.summary_open || g.bounce_text.is_some() || g.placement.is_some();
        let busy = self.help_open || self.picker.is_some() || self.palette.is_some() || (self.screen == Screen::Grooming && modal);
        if busy || ctx.text_edit_focused() {
            return;
        }
        let pressed = ctx.input_mut(|i| {
            let shift_only = i
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { key: egui::Key::D, pressed: true, modifiers, .. } if *modifiers == egui::Modifiers::SHIFT));
            shift_only && i.consume_key(egui::Modifiers::SHIFT, egui::Key::D)
        });
        if pressed && let Some(key) = self.done_target() {
            self.actions.push(Action::MoveToDone(key));
        }
    }

    /// The Team screen's columns, from the team lists in the store.
    pub fn team_columns(&self) -> Vec<crate::teamview::Column<'_>> {
        let in_progress = self.store.list(ListId::TeamInProgress);
        let team_open = self.store.list(ListId::TeamOpen);
        crate::teamview::columns(&self.cfg.team.members, &in_progress, &team_open, &self.cfg.grooming.status_ready, chrono::Utc::now())
    }

    /// The Me screen's sections, from the Me list in the store.
    pub fn me_sections(&self) -> Vec<(crate::me::Section, Vec<crate::teamview::Card<'_>>)> {
        crate::me::sections(&self.store.list(ListId::Me), chrono::Utc::now())
    }

    /// The issue under the Me cursor, clamped to the rows there are.
    pub fn me_focused(&self) -> Option<String> {
        let s = self.me_sections();
        let row = crate::me::moved(self.me_view.cursor, 0, crate::me::row_count(&s));
        crate::me::issue_at(&s, row).map(|i| i.key.clone())
    }

    /// The epics the current screen shows dots for, in reading order: Grooming's session rail or pre-check rows, the
    /// Team columns, the Me list. Issues tracks its own rows (`views::issues::track_epics`); Roadmap and Standup show
    /// no epic dots.
    fn screen_epics(&self) -> Option<Vec<String>> {
        let parents = |issues: &mut dyn Iterator<Item = &crate::jira::Issue>| issues.filter_map(|i| i.parent_key.clone()).collect::<Vec<_>>();
        Some(match self.screen {
            Screen::Grooming => match self.grooming.mode {
                GroomMode::Session => self.session_queues().into_iter().filter_map(|q| q.epic).collect(),
                GroomMode::PreCheck => parents(&mut self.pre_check_candidates().iter().map(|c| c.issue)),
            },
            Screen::Team => parents(&mut self.team_columns().iter().flat_map(|c| c.in_progress.iter().chain(&c.ready)).map(|c| c.issue)),
            Screen::Me => parents(&mut self.store.list(ListId::Me).into_iter()),
            Screen::Standup => parents(&mut self.standup.saved.tickets.all()),
            Screen::Issues | Screen::Roadmap => return None,
        })
    }

    /// Gives hue slots to the epics on the current screen first, then to the loaded epic list, on every screen,
    /// not only Issues: Grooming and Team read `epic_slots` too, and with none given every epic fell back to one
    /// slot (round 3: 10 of 16 Grooming epics one orange). Same phases as Issues (`views::issues::slot_phase`), so a
    /// hue given here holds when Issues takes over.
    fn track_screen_epics(&mut self) {
        let Some(visible) = self.screen_epics() else { return };
        let resolved = self.sprints_answered || self.sprints_cached || self.client.is_none();
        match views::issues::slot_phase(resolved, self.epic_slots_final) {
            views::issues::SlotPhase::Provisional => self.epic_slots.clear(),
            views::issues::SlotPhase::Reset => {
                self.epic_slots.clear();
                self.epic_slots_final = true;
            }
            views::issues::SlotPhase::Keep => {}
        }
        let rest: Vec<String> = self.store.list(ListId::Epics).iter().map(|e| e.key.clone()).collect();
        theme::assign_epic_slots(&mut self.epic_slots, visible.iter().map(String::as_str), rest.iter().map(String::as_str));
    }

    /// The roadmap's initiatives, derived from the epics in the store and their children, and the summaries of the
    /// ongoing (KTLO) epics.
    pub fn roadmap_items(&self) -> (Vec<crate::roadmap::Initiative>, Vec<String>) {
        let epics = self.store.list(ListId::Epics);
        let (inits, ongoing) = crate::roadmap::select(&epics, &self.cfg.roadmap.initiative_label);
        let items = inits.iter().map(|e| crate::roadmap::derive(e, &self.store.children_of(&e.key), &self.cfg.roadmap)).collect();
        (items, ongoing.iter().map(|e| e.summary.clone()).collect())
    }

    pub fn spawn(&self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            f(&client, &tx);
            ctx.request_repaint();
        });
    }

    /// Queues a write on the ordered write worker; a no-op without a client.
    pub fn spawn_write(&mut self, f: impl FnOnce(&Client, &Sender<Msg>) + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        self.ensure_write_worker();
        let _ = self.writes.send((client, Box::new(f)));
    }

    /// Starts the write worker on the first write; later calls do nothing.
    fn ensure_write_worker(&mut self) {
        let Some(write_rx) = self.write_rx.take() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        // Ends when the App (the only sender) is dropped.
        std::thread::spawn(move || {
            for (client, job) in write_rx {
                job(&client, &tx);
                ctx.request_repaint();
            }
        });
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast_kind(text, if error { ToastKind::Error } else { ToastKind::Success });
    }

    pub fn toast_kind(&mut self, text: impl Into<String>, kind: ToastKind) {
        let secs = match kind {
            ToastKind::Success => 3,
            ToastKind::Info => 5,
            ToastKind::Error => 8,
        };
        self.toast = Some(Toast { text: text.into(), kind, until: Instant::now() + Duration::from_secs(secs) });
    }

    /// Uses `a` from now on and saves it (with the config value it replaces) for the next launch.
    pub(super) fn set_appearance(&mut self, a: Appearance, ctx: &egui::Context) {
        self.appearance = a;
        apply_appearance(ctx, a);
        if self.persist_cache {
            crate::cache::store(APPEARANCE_KEY, &SavedAppearance { config: self.cfg.appearance, chosen: a });
        }
    }

    fn connect(&mut self, login: Login) {
        self.client = Some(Arc::new(Client::new(&self.cfg.site, &login)));
        crate::avatar::set_client(&self.ctx, self.client.clone());
        self.login = Some(login);
        self.finding_token = false;
        self.spawn(|c, tx| {
            let _ = tx.send(Msg::Me(c.myself()));
        });
        self.refresh_all();
    }
}

/// The launch's appearance: the config's, or the choice saved in the app while the config is unchanged
/// (`theme::effective_appearance`); `JIRA_TICKETS_THEME` and `JIRA_TICKETS_DENSITY` override both (snapshots).
fn launch_appearance(config: Appearance, saved: Option<SavedAppearance>, env_theme: Option<String>, env_density: Option<String>) -> Appearance {
    let mut a = theme::effective_appearance(config, saved);
    if let Some(t) = env_theme.as_deref().and_then(theme::Scheme::parse) {
        a.theme = t;
    }
    if let Some(d) = env_density.as_deref().and_then(theme::Density::parse) {
        a.density = d;
    }
    a
}

fn apply_appearance(ctx: &egui::Context, a: Appearance) {
    ctx.set_theme(a.theme.preference());
    theme::set_density(ctx, a.density);
}

/// The cache key of a board's sprint list (the first frame's sprint scope, `filter::launch_sprint_scope`).
pub fn sprints_cache_key(board: u64) -> String {
    format!("sprints-{board}")
}

/// Only web and mail links open; anything else (file:, custom schemes) is ignored.
pub fn openable(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|s| u.starts_with(s))
}

pub fn open_url(url: &str) {
    if !openable(url) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::process::Command::new("xdg-open").arg("--").arg(url).spawn();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let back = focused && !self.was_focused && self.last_refresh.elapsed() > Duration::from_secs(60);
        self.was_focused = focused;
        if self.client.is_some() && (back || self.last_refresh.elapsed() > AUTO_REFRESH) {
            self.refresh_all();
        }
        if self.client.is_some() {
            self.standup_auto_run();
        }
        ctx.request_repaint_after(AUTO_REFRESH);

        // The root ui has no background of its own in eframe 0.36.
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::palette(&ctx).bg);
        close_shortcut(&ctx);
        if self.client.is_some() {
            self.done_shortcut(&ctx);
            self.shortcuts(&ctx);
            // Set again by the detail pane when its pill row shows.
            self.detail_pills_top = None;
            // Set again by the screens that show a detail pane (Issues, Me), so nothing centres on a pane that is not
            // on screen.
            self.detail_x = None;
            self.track_screen_epics();
            views::main(self, ui);
            self.text_search_tick(&ctx);
            self.palette_tick(&ctx);
            views::pickers::show(self, &ctx);
            views::palette::show(self, &ctx);
            views::help::show(self, &ctx);
        } else {
            views::sign_in(self, ui);
        }
        views::toast(self, &ctx);

        // Actions run after the views drew, so draw once more to show their effect.
        let actions = std::mem::take(&mut self.actions);
        if !actions.is_empty() {
            ctx.request_repaint();
        }
        for a in actions {
            self.apply(a, &ctx);
        }
        let urls = ctx.output_mut(|o| {
            let mut urls = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) => {
                    urls.push(u.url.clone());
                    false
                }
                _ => true,
            });
            urls
        });
        for u in urls {
            open_url(&u);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::Config;
    use crate::store::ListId;

    fn cfg() -> Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap()
    }

    #[test]
    fn for_test_builds_without_a_window() {
        let app = App::for_test(cfg(), None);
        assert!(app.client.is_none());
        assert_eq!(app.screen, Screen::Issues);
        assert!(app.store.issues.is_empty());
    }

    #[test]
    fn for_test_never_reads_the_real_report_dir() {
        let app = App::for_test(cfg(), None);
        let default = crate::config::Grooming::default().report_dir;
        assert_ne!(app.cfg.grooming.report_dir, default, "the default ~/Documents/grooming-reports is swapped out");
        assert!(!app.cfg.report_dir().exists(), "{:?}", app.cfg.report_dir());
        let mut set = cfg();
        set.grooming.report_dir = "/tmp/a-test-dir".into();
        assert_eq!(App::for_test(set, None).cfg.grooming.report_dir, "/tmp/a-test-dir", "a test's own dir is kept");
    }

    #[test]
    fn changing_screen_closes_the_palette_and_the_summary() {
        let mut app = App::for_test(cfg(), None);
        let ctx = egui::Context::default();
        app.palette = Some(PaletteState { query: "CI-".into(), ..Default::default() });
        app.apply(Action::SetScreen(Screen::Grooming), &ctx);
        assert!(app.palette.is_none(), "a new screen starts with the palette closed");
        app.grooming.summary_open = true;
        app.apply(Action::SetScreen(Screen::Grooming), &ctx);
        assert!(app.grooming.summary_open, "staying on the screen keeps it");
        app.apply(Action::SetScreen(Screen::Issues), &ctx);
        assert!(!app.grooming.summary_open, "the summary is drawn on Grooming only");
    }

    #[test]
    fn unauthorized_me_result_returns_to_sign_in() {
        let mut app = App::for_test(cfg(), None);
        app.handle(Msg::Me(Err(crate::jira::Error::Unauthorized("401".into()))));
        assert!(app.client.is_none());
        assert_eq!(app.token_error.as_deref(), Some("401"));
    }

    #[test]
    fn team_open_result_populates_store_and_queues_epics() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let client = std::sync::Arc::new(crate::jira::Client::new(&base, &login));
        let mut app = App::for_test(cfg(), Some(client));
        let issue = crate::jira::Issue { key: "CI-1".into(), parent_key: Some("CI-9".into()), ..Default::default() };
        app.handle(Msg::List { id: ListId::TeamOpen, result: Ok(vec![issue]), seq: None, with_members: false });
        assert_eq!(app.store.list(ListId::TeamOpen).len(), 1);
        assert!(app.lists.get(&ListId::Epics).is_some_and(|l| l.loading), "epics load was started");
    }

    #[test]
    fn stale_search_result_is_dropped() {
        let mut app = App::for_test(cfg(), None);
        app.search_active = true;
        app.search_seq = 5;
        let issue = crate::jira::Issue { key: "CI-2".into(), ..Default::default() };
        app.handle(Msg::List { id: ListId::Search, result: Ok(vec![issue]), seq: Some(4), with_members: false });
        assert!(app.store.list(ListId::Search).is_empty());
    }

    #[test]
    fn report_loaded_prefills_pre_check_verdicts() {
        let mut app = App::for_test(cfg(), None);
        let r = crate::report::parse(
            "# x — 2026-10-01\n\n## Planning — by epic\n\n### [E-1 Epic](u?issueParent=1)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: empty\n",
        );
        app.handle(Msg::ReportLoaded(Some((std::path::PathBuf::from("/tmp/x.md"), r))));
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready));
        assert!(matches!(app.grooming.verdicts.get("CI-2"), Some(crate::grooming::Verdict::NeedsDetail(c)) if c == &vec![crate::grooming::Code::Empty]));
    }

    #[test]
    fn same_report_again_does_not_bring_back_removed_verdicts() {
        let mut app = App::for_test(cfg(), None);
        let text = "# x — 2026-10-01\n\n## Planning — by epic\n\n### (no epic)\n\n- [CI-1](u) — a · rank 1 · older\n";
        let at = |p: &str| Some((std::path::PathBuf::from(p), crate::report::parse(text)));
        app.handle(Msg::ReportLoaded(at("/r/2026-10-01.md")));
        assert!(app.grooming.verdicts.contains_key("CI-1"));
        app.grooming.verdicts.remove("CI-1");
        app.handle(Msg::ReportLoaded(at("/r/2026-10-01.md")));
        assert!(!app.grooming.verdicts.contains_key("CI-1"), "a refresh re-reading the same report keeps the undo");
        assert!(app.grooming.report.is_some(), "the report itself is still kept");
        app.handle(Msg::ReportLoaded(at("/r/2026-10-02.md")));
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready), "a new report prefills");
    }

    #[test]
    fn verdict_actions_set_and_undo() {
        let mut app = App::for_test(cfg(), None);
        app.screen = Screen::Grooming;
        app.store.absorb(
            ListId::TeamOpen,
            vec![crate::jira::Issue {
                key: "CI-1".into(),
                labels: vec!["PLAT".into()],
                status: crate::jira::Status { name: "New".into(), category: "new".into() },
                ..Default::default()
            }],
        );
        let ctx = egui::Context::default();
        app.apply(Action::Verdict(crate::grooming::Verdict::Ready), &ctx);
        assert_eq!(app.grooming.verdicts.get("CI-1"), Some(&crate::grooming::Verdict::Ready));
        app.apply(Action::UndoVerdict, &ctx);
        assert!(!app.grooming.verdicts.contains_key("CI-1"));
    }

    fn team_app() -> App {
        let mut c = cfg();
        c.team.members = vec!["abc".into()];
        let mut app = App::for_test(c, None);
        app.screen = Screen::Team;
        let who = Some(crate::jira::User { account_id: "abc".into(), display_name: "Grace Hopper".into() });
        let status = |n: &str, cat: &str| crate::jira::Status { name: n.into(), category: cat.into() };
        app.store.absorb(
            ListId::TeamInProgress,
            vec![crate::jira::Issue { key: "CI-1".into(), status: status("Work in Progress", "indeterminate"), assignee: who.clone(), ..Default::default() }],
        );
        app.store.absorb(
            ListId::TeamOpen,
            vec![crate::jira::Issue { key: "CI-2".into(), status: status("Ready for Work", "new"), assignee: who, ..Default::default() }],
        );
        app
    }

    #[test]
    fn team_move_clamps_and_open_selects() {
        let mut app = team_app();
        let ctx = egui::Context::default();
        app.apply(Action::TeamMove(1), &ctx);
        app.apply(Action::TeamMove(5), &ctx);
        assert_eq!((app.team.col, app.team.row), (0, 1), "in progress then ready; clamped to the last card");
        app.apply(Action::TeamMove(-9), &ctx);
        assert_eq!(app.team.row, 0);
        app.apply(Action::TeamMove(1), &ctx);
        app.apply(Action::TeamOpen, &ctx);
        app.drain();
        assert_eq!(app.selected.as_deref(), Some("CI-2"));
        assert_eq!(app.screen, Screen::Issues);
        app.screen = Screen::Team;
        app.apply(Action::TeamCol(9), &ctx);
        assert_eq!((app.team.col, app.team.row), (1, 0), "the Unassigned column is last; its row clamps to 0");
        app.apply(Action::TeamCol(-9), &ctx);
        assert_eq!(app.team.col, 0);
    }

    #[test]
    fn toggle_done_without_a_client_opens_but_does_not_load() {
        let mut app = team_app();
        let ctx = egui::Context::default();
        app.apply(Action::TeamToggleDone, &ctx);
        assert!(app.team.done_open.contains("abc"));
        assert!(app.team.done_loading.is_empty(), "no client: nothing would clear the flag");
        app.apply(Action::TeamToggleDone, &ctx);
        assert!(!app.team.done_open.contains("abc"));
    }

    #[test]
    fn done_recent_result_is_stored() {
        let mut app = team_app();
        app.team.done_loading.insert("abc".into());
        let done = crate::jira::Issue { key: "CI-7".into(), summary: "Shipped".into(), ..Default::default() };
        app.handle(Msg::DoneRecent { member: "abc".into(), result: Ok(vec![done]) });
        assert!(app.team.done_loading.is_empty());
        assert_eq!(app.team.done.get("abc").map(|v| v.iter().map(|i| i.key.as_str()).collect::<Vec<_>>()), Some(vec!["CI-7"]));
        assert_eq!(app.team.done_count.get("abc"), Some(&1), "the list's length is the count");
        app.team.done_loading.insert("abc".into());
        app.handle(Msg::DoneRecent { member: "abc".into(), result: Err(crate::jira::Error::Network("down".into())) });
        assert!(app.team.done_loading.is_empty());
        assert!(app.team.done.contains_key("abc"), "a failed reload keeps the last list");
        assert!(app.toast.is_some());
    }

    #[test]
    fn done_count_result_is_stored_and_kept_on_failure() {
        let mut app = team_app();
        app.team.done_count_loading.insert("abc".into());
        app.handle(Msg::DoneCount { member: "abc".into(), result: Ok(3) });
        assert!(app.team.done_count_loading.is_empty());
        assert_eq!(app.team.done_count.get("abc"), Some(&3));
        app.team.done_count_loading.insert("abc".into());
        app.handle(Msg::DoneCount { member: "abc".into(), result: Err(crate::jira::Error::Network("down".into())) });
        assert!(app.team.done_count_loading.is_empty());
        assert_eq!(app.team.done_count.get("abc"), Some(&3), "a failed reload keeps the last count");
        assert!(app.toast.is_none(), "a count is a hint, not worth a toast");
    }

    #[test]
    fn refresh_reloads_open_done_footers_and_drops_closed_ones() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let client = std::sync::Arc::new(crate::jira::Client::new(&base, &login));
        let mut app = team_app();
        app.client = Some(client);
        let done = |key: &str| vec![crate::jira::Issue { key: key.into(), ..Default::default() }];
        app.team.done_open.insert("abc".into());
        app.team.done.insert("abc".into(), done("CI-7"));
        app.team.done.insert("def".into(), done("CI-8"));
        app.apply(Action::Refresh, &egui::Context::default());
        assert!(app.team.done_loading.contains("abc"), "the open footer reloads");
        assert!(!app.team.done_loading.contains("def"), "a closed footer does not");
        assert!(app.team.done.contains_key("abc"), "the open footer keeps its list until the reload lands");
        assert!(!app.team.done.contains_key("def"), "a closed footer's list is dropped, so reopening reloads it");
        for m in &app.cfg.team.members {
            assert!(app.team.done_count_loading.contains(m), "every member's count reloads: {m}");
        }
    }

    fn roadmap_app() -> App {
        let mut app = App::for_test(cfg(), None);
        app.screen = Screen::Roadmap;
        let now = |key: &str| crate::jira::Issue {
            key: key.into(),
            summary: format!("Initiative {key}"),
            status: crate::jira::Status { name: "In Progress".into(), category: "indeterminate".into() },
            ..Default::default()
        };
        app.store.absorb(ListId::Epics, vec![now("CI-10"), now("CI-11")]);
        app
    }

    #[test]
    fn epic_children_count_done_children_once() {
        let mut app = App::for_test(cfg(), None);
        let st = |cat: &str| crate::jira::Status { name: "x".into(), category: cat.into() };
        let child = |key: &str, cat: &str| crate::jira::Issue { key: key.into(), status: st(cat), parent_key: Some("CI-10".into()), ..Default::default() };
        let list = |id, issues| Msg::List { id, result: Ok(issues), seq: None, with_members: false };
        app.store.absorb(ListId::TeamOpen, vec![child("CI-21", "new"), child("CI-22", "indeterminate")]);
        let epic = crate::jira::Issue { key: "CI-10".into(), summary: "Initiative".into(), status: st("indeterminate"), ..Default::default() };
        app.handle(list(ListId::Epics, vec![epic]));
        app.handle(list(ListId::EpicChildren, vec![child("CI-20", "done"), child("CI-21", "new")]));
        let (items, _) = app.roadmap_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].progress, crate::roadmap::Progress { todo: 1, doing: 1, done: 1 }, "CI-21 is in both lists and counts once");
    }

    #[test]
    fn epic_children_do_not_load_without_a_client() {
        let mut app = App::for_test(cfg(), None);
        let epic = crate::jira::Issue { key: "CI-10".into(), status: crate::jira::Status { name: "x".into(), category: "new".into() }, ..Default::default() };
        app.handle(Msg::List { id: ListId::Epics, result: Ok(vec![epic]), seq: None, with_members: false });
        app.load_epic_children();
        assert!(!app.lists.get(&ListId::EpicChildren).is_some_and(|l| l.loading), "no client: nothing would clear the flag");
    }

    #[test]
    fn roadmap_tab_and_move_clamp() {
        let mut app = roadmap_app();
        let ctx = egui::Context::default();
        assert_eq!(app.roadmap.tab, 1, "Now / Next / Later first");
        app.apply(Action::RoadmapTab(3), &ctx);
        assert_eq!(app.roadmap.tab, 3);
        app.apply(Action::RoadmapMove(5), &ctx);
        assert_eq!(app.roadmap.cursor, 1, "two Now epics: clamped to the last row");
        app.apply(Action::RoadmapMove(-9), &ctx);
        assert_eq!(app.roadmap.cursor, 0);
        app.apply(Action::RoadmapMove(1), &ctx);
        app.apply(Action::RoadmapOpen, &ctx);
        app.drain();
        assert_eq!(app.selected.as_deref(), Some("CI-11"));
        assert_eq!(app.screen, Screen::Issues);
        app.screen = Screen::Roadmap;
        app.apply(Action::RoadmapTab(1), &ctx);
        assert_eq!((app.roadmap.tab, app.roadmap.cursor), (1, 0), "a new tab starts at its first row");
        app.apply(Action::RoadmapTab(9), &ctx);
        assert_eq!(app.roadmap.tab, 1, "no such tab: unchanged");
        app.apply(Action::RoadmapTab(2), &ctx);
        app.apply(Action::RoadmapMove(1), &ctx);
        assert_eq!(app.roadmap.cursor, 0, "a tab without rows keeps the cursor at 0");
        app.apply(Action::RoadmapOpen, &ctx);
        assert_eq!(app.screen, Screen::Roadmap, "nothing to open");
    }

    fn me_app() -> App {
        let mut app = App::for_test(cfg(), None);
        app.screen = Screen::Me;
        let issue = |key: &str, cat: &str, rank: &str| crate::jira::Issue {
            key: key.into(),
            status: crate::jira::Status { name: "x".into(), category: cat.into() },
            rank: rank.into(),
            ..Default::default()
        };
        app.store.absorb(ListId::Me, vec![issue("CI-3", "done", ""), issue("CI-2", "new", "0|a"), issue("CI-1", "indeterminate", "0|b")]);
        app
    }

    #[test]
    fn me_move_selects_and_open_goes_to_issues() {
        let mut app = me_app();
        let ctx = egui::Context::default();
        assert_eq!(app.me_focused().as_deref(), Some("CI-1"), "opens on the in-progress ticket");
        app.apply(Action::MeMove(1), &ctx);
        assert_eq!((app.me_view.cursor, app.selected.as_deref()), (1, Some("CI-2")), "then To do");
        app.apply(Action::MeMove(9), &ctx);
        assert_eq!((app.me_view.cursor, app.selected.as_deref()), (2, Some("CI-3")), "clamped to the last done ticket");
        app.apply(Action::MeMove(-9), &ctx);
        assert_eq!(app.me_view.cursor, 0);
        app.apply(Action::MeOpen, &ctx);
        assert_eq!((app.screen, app.selected.as_deref()), (Screen::Issues, Some("CI-1")));
    }

    #[test]
    fn me_list_loads_only_with_a_client() {
        let mut app = App::for_test(cfg(), None);
        app.refresh_all();
        assert!(!app.lists.get(&ListId::Me).is_some_and(|l| l.loading), "no client: nothing would clear the flag");
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        app.client = Some(std::sync::Arc::new(crate::jira::Client::new(&base, &login)));
        app.refresh_all();
        assert!(app.lists.get(&ListId::Me).is_some_and(|l| l.loading), "a refresh loads the Me list");
    }

    #[test]
    fn me_keys() {
        let mut app = me_app();
        press(&mut app, egui::Key::J, egui::Modifiers::NONE);
        press(&mut app, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(matches!(app.actions.as_slice(), [Action::MeMove(1), Action::MeOpen]));
        app.actions.clear();
        press(&mut app, egui::Key::O, egui::Modifiers::COMMAND);
        assert!(matches!(app.actions.as_slice(), [Action::OpenUrl(u)] if u.ends_with("/browse/CI-1")));
        app.actions.clear();
        app.screen = Screen::Issues;
        press(&mut app, egui::Key::Num5, egui::Modifiers::COMMAND);
        assert!(matches!(app.actions.as_slice(), [Action::SetScreen(Screen::Me)]), "⌘5 is the Me screen");
    }

    #[test]
    fn shift_d_moves_the_focused_ticket_to_done_on_every_screen() {
        let mut app = me_app();
        app.me_view.cursor = 1;
        let want = app.me_focused().unwrap();
        press(&mut app, egui::Key::D, egui::Modifiers::SHIFT);
        assert!(matches!(app.actions.as_slice(), [Action::MoveToDone(k)] if *k == want), "Me");
        app.actions.clear();
        app.screen = Screen::Issues;
        app.selected = Some("CI-9".into());
        press(&mut app, egui::Key::D, egui::Modifiers::SHIFT);
        assert!(matches!(app.actions.as_slice(), [Action::MoveToDone(k)] if k == "CI-9"), "Issues");
        app.actions.clear();
        app.screen = Screen::Team;
        press(&mut app, egui::Key::D, egui::Modifiers::SHIFT);
        assert!(!app.actions.iter().any(|a| matches!(a, Action::TeamToggleDone)), "Shift+D is not Team's plain d");
        press(&mut app, egui::Key::D, egui::Modifiers::NONE);
        assert!(app.actions.iter().any(|a| matches!(a, Action::TeamToggleDone)), "plain d still is");
        app.actions.clear();
        app.screen = Screen::Issues;
        press(&mut app, egui::Key::D, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert!(matches!(app.actions.as_slice(), [Action::ToggleDensity]), "⌘⇧D stays density");
        app.actions.clear();
        app.help_open = true;
        press(&mut app, egui::Key::D, egui::Modifiers::SHIFT);
        assert!(app.actions.is_empty(), "help owns the keys");
    }

    /// Runs the shortcuts over one pass that presses `key` with `mods`.
    fn press(app: &mut App, key: egui::Key, mods: egui::Modifiers) {
        let ctx = egui::Context::default();
        let ev = egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods };
        let input = egui::RawInput { events: vec![ev], ..Default::default() };
        ctx.run_ui(input, |_| {
            app.done_shortcut(&ctx);
            app.shortcuts(&ctx);
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn plain_digits_switch_roadmap_tabs_and_command_digits_switch_screens() {
        let mut app = roadmap_app();
        press(&mut app, egui::Key::Num3, egui::Modifiers::NONE);
        assert!(matches!(app.actions.as_slice(), [Action::RoadmapTab(3)]), "plain 3 on Roadmap");
        app.actions.clear();
        press(&mut app, egui::Key::Num3, egui::Modifiers::COMMAND);
        assert!(matches!(app.actions.as_slice(), [Action::SetScreen(Screen::Team)]), "⌘3 stays the screen switch");
        app.actions.clear();
        press(&mut app, egui::Key::Num2, egui::Modifiers::ALT);
        assert!(app.actions.is_empty(), "a modified digit is not a tab key");
        app.screen = Screen::Team;
        press(&mut app, egui::Key::Num3, egui::Modifiers::NONE);
        assert!(!app.actions.iter().any(|a| matches!(a, Action::RoadmapTab(_))), "only on Roadmap");
    }

    #[test]
    fn launch_appearance_env_overrides_config_and_saved() {
        use crate::theme::{Density, Scheme};
        let config = Appearance::default();
        let chosen = Appearance { theme: Scheme::Light, density: Density::Comfortable };
        let saved = Some(SavedAppearance { config, chosen });
        assert_eq!(launch_appearance(config, saved, None, None), chosen);
        assert_eq!(launch_appearance(config, saved, Some("dark".into()), None), Appearance { theme: Scheme::Dark, ..chosen });
        assert_eq!(launch_appearance(config, None, None, Some("comfortable".into())).density, Density::Comfortable);
        assert_eq!(launch_appearance(config, None, Some("bogus".into()), Some("".into())), config, "unknown values are ignored");
    }

    #[test]
    fn theme_and_density_actions_switch_the_context() {
        let mut app = App::for_test(cfg(), None);
        let ctx = egui::Context::default();
        app.apply(Action::CycleTheme, &ctx);
        assert_eq!(app.appearance.theme, crate::theme::Scheme::Light);
        assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::Light);
        app.apply(Action::ToggleDensity, &ctx);
        assert_eq!(ctx.style_of(egui::Theme::Light).spacing.item_spacing, crate::theme::COMFORTABLE.item);
        assert!(app.toast.as_ref().is_some_and(|t| t.text == "Density: Comfortable"));
    }

    #[test]
    fn only_web_and_mail_links_open() {
        for ok in ["https://x.atlassian.net/browse/CI-1", "HTTP://example.com", "mailto:a@b.c"] {
            assert!(openable(ok), "{ok}");
        }
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "smb://host/share", "-n", "x-apple.systempreferences:"] {
            assert!(!openable(bad), "{bad}");
        }
    }
}
