//! Every keyboard binding in one list: the help overlay, the hint bar, and the
//! README table are all drawn from it.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Global,
    Issues,
    Grooming,
    Team,
    Roadmap,
    Me,
    Standup,
    Pickers,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Global => "Global",
            Group::Issues => "Issues",
            Group::Grooming => "Grooming",
            Group::Team => "Team",
            Group::Roadmap => "Roadmap",
            Group::Me => "Me",
            Group::Standup => "Standup",
            Group::Pickers => "Pickers and Edit",
        }
    }
}

/// `keys` lists alternatives separated by ", ", drawn as chips joined by a muted "or" ("⌘F or /"); " to "
/// joins the ends of a range ("⌘1 to ⌘6"), drawn as two chips with a muted "to"; "/" inside one chip is a
/// pair ("j/k").
pub struct KeyBinding {
    pub group: Group,
    pub keys: &'static str,
    pub what: &'static str,
}

const fn b(group: Group, keys: &'static str, what: &'static str) -> KeyBinding {
    KeyBinding { group, keys, what }
}

pub const BINDINGS: &[KeyBinding] = &[
    b(Group::Global, "⌘1 to ⌘6", "Issues, Grooming, Team, Roadmap, Me, Standup"),
    b(Group::Global, "⌘R", "Refresh"),
    b(Group::Global, "⌘⇧T", "Theme: System, Light, or Dark"),
    b(Group::Global, "⌘⇧D", "Density: Compact or Comfortable"),
    b(Group::Global, "⌘W", "Close the window"),
    // Shift+D on every screen with a focused ticket; plain `d` stays Team's done footer.
    b(Group::Global, "D", "Move the focused ticket to Done"),
    b(Group::Global, "⌘K", "Search tickets, open one by key or number, or run a command"),
    // Esc is global: it cancels a picker or the Edit bar and closes the palette (round 13 H3).
    b(Group::Global, "Esc", "Cancel or close"),
    b(Group::Global, "?, Esc", "Close this help"),
    // ⌘O is bound on the Issues screen only (`App::shortcuts`).
    b(Group::Issues, "⌘O", "Open in Jira"),
    b(Group::Issues, "⌘F, /", "Focus search"),
    b(Group::Issues, "j/k, ↑/↓", "Next or previous issue"),
    b(Group::Issues, "t", "Transition"),
    b(Group::Issues, "a", "Assign"),
    b(Group::Issues, "s", "Sprint"),
    b(Group::Issues, "e", "Edit labels, priority, or story points"),
    b(Group::Issues, "l", "Add the picked team's label"),
    b(Group::Issues, "m", "Toggle Roadmap filter"),
    b(Group::Issues, "J/K", "Move down or up in rank"),
    b(Group::Issues, "c", "Copy the issue link"),
    b(Group::Issues, "⌘Enter", "Post the comment"),
    b(Group::Grooming, "j/k, ↑/↓", "Next or previous ticket"),
    b(Group::Grooming, "r", "Ready"),
    b(Group::Grooming, "w/n/e", "Unclear what, unclear when, or empty"),
    b(Group::Grooming, "u", "Undo the last verdict"),
    b(Group::Grooming, "Enter", "Pre-check summary and Promote"),
    b(Group::Grooming, "y", "Accept into Ready for Work (↑/↓ place, Enter commits)"),
    b(Group::Grooming, "b", "Bounce with a comment (⌘Enter posts)"),
    b(Group::Grooming, "p", "Park in Backlog"),
    b(Group::Grooming, "J/K", "Move a Ready for Work ticket down or up in rank"),
    b(Group::Grooming, "l", "Add the team label to the focused ticket"),
    b(Group::Grooming, "a", "Assign the focused ticket"),
    b(Group::Grooming, "c", "Close the focused ticket as Done or Won't Do"),
    b(Group::Grooming, "s", "Move the focused ticket to a sprint or the backlog"),
    // Shift lands on epics with nothing in Planning too; plain ]/[ steps over them.
    b(Group::Grooming, "]/[, ⇧]/⇧[", "Next or previous epic (with Shift, empty ones too)"),
    b(Group::Grooming, "Tab", "Switch between Pre-check and Session"),
    b(Group::Team, "h/l, ←/→", "Previous or next member"),
    b(Group::Team, "j/k, ↑/↓", "Next or previous ticket"),
    b(Group::Team, "Enter", "Open in Issues"),
    b(Group::Team, "d", "Show or hide done in the last 7 days"),
    // Plain digits on Roadmap only; ⌘1 to ⌘6 stay the screen switches.
    b(Group::Roadmap, "1 to 4", "Now / Next / Later, Timeline, Status review, Outcomes"),
    b(Group::Roadmap, "j/k, ↑/↓", "Next or previous initiative"),
    b(Group::Roadmap, "Enter", "Open in Issues"),
    b(Group::Me, "j/k, ↑/↓", "Next or previous ticket"),
    b(Group::Me, "Enter", "Open in Issues"),
    b(Group::Me, "⌘O", "Open in Jira"),
    b(Group::Standup, "j/k, ↑/↓", "Next or previous ticket or suggestion"),
    b(Group::Standup, "c", "Add to the current sprint"),
    b(Group::Standup, "n", "Add to the next sprint"),
    b(Group::Standup, "b", "Add to the backlog"),
    b(Group::Standup, "x", "Dismiss for good"),
    b(Group::Standup, "e", "Edit the title before adding"),
    b(Group::Standup, "o", "Open the source"),
    b(Group::Standup, "y", "Copy the standup"),
    b(Group::Standup, "r", "Run the review again"),
    b(Group::Pickers, "j/k, ↑/↓", "Move"),
    b(Group::Pickers, "Tab/⇧Tab", "Next or previous field"),
    b(Group::Pickers, "Enter", "Confirm"),
    b(Group::Pickers, "⌘Enter", "Open the ⌘K result in Jira"),
];

/// The alternatives of one binding; each is one chip, or a range (see `range`).
pub fn chips(keys: &str) -> impl Iterator<Item = &str> {
    keys.split(", ")
}

/// The chips of one alternative: ["⌘1", "⌘6"] for the range "⌘1 to ⌘6", else just the key.
pub fn range(alt: &str) -> Vec<&str> {
    alt.split(" to ").collect()
}

/// The README keyboard table, in `BINDINGS` order, with a bold row per group.
#[cfg(test)]
pub fn readme_table() -> String {
    let mut out = String::from("| Keys | Action |\n| --- | --- |\n");
    let mut group = None;
    for k in BINDINGS {
        if group != Some(k.group) {
            group = Some(k.group);
            out.push_str(&format!("| **{}** | |\n", k.group.name()));
        }
        let keys = chips(k.keys).map(|c| range(c).iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(" to ")).collect::<Vec<_>>().join(" or ");
        out.push_str(&format!("| {keys} | {} |\n", k.what));
    }
    out
}

/// One token of a hint: a key (drawn mono) or a word (drawn small and muted).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    K(&'static str),
    W(&'static str),
}
use Tok::{K, W};

/// One hint, e.g. `[K("t"), W("transition")]`; the bar joins hints with " · ".
pub type Hint = &'static [Tok];

/// What the hint bar describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HintContext {
    Help,
    Palette,
    /// The search box has focus and Esc would clear it (text, or a search showing).
    Search,
    /// The search box has focus and holds plain text (already filtered and text-searched); Enter runs it as JQL.
    SearchText,
    /// The search box has focus with nothing to clear, so Esc leaves it.
    SearchEmpty,
    /// The comment composer has focus.
    Composer,
    EditPicker,
    /// The Assign picker: its filter box has focus.
    AssignPicker,
    Picker,
    Selected,
    Browse,
    /// The sign-in screen: Enter signs in.
    SignIn,
    /// Selected, with a server search (text or JQL) on screen: Esc clears it.
    SelectedSearch,
    /// Browse, with a server search on screen.
    BrowseSearch,
    /// The Grooming screen in Pre-check mode.
    GroomPreCheck,
    /// The pre-check summary dialog: Enter promotes.
    GroomSummary,
    /// The pre-check summary dialog in read-only mode: promoting is off, so only Esc.
    GroomSummaryReadOnly,
    /// The "report for today exists" confirm.
    GroomOverwrite,
    /// The Grooming screen in Session mode.
    GroomSession,
    /// Session: placing an accepted ticket in the Ready for Work queue.
    GroomPlacement,
    /// Session: the bounce comment box is open.
    GroomBounce,
    /// The bounce box in read-only mode: posting is off, so only Esc.
    GroomBounceReadOnly,
    /// The Team screen.
    Team,
    /// The Roadmap screen on a tab with a cursor (Now / Next / Later, Status review).
    Roadmap,
    /// The Roadmap screen on a tab with nothing to move through or open (Timeline, Outcomes).
    RoadmapView,
    /// The Me screen.
    Me,
    /// The Standup screen.
    Standup,
    /// The Standup screen with no cards (no review yet, or nothing untracked): no card keys.
    StandupEmpty,
    /// The Standup screen with a card's title field focused.
    StandupTitle,
}

/// Which text field has keyboard focus, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    None,
    /// The search box: `clear` when Esc would clear it rather than leave, `plain` when it holds plain text
    /// (not JQL), which Enter runs as JQL.
    Search {
        clear: bool,
        plain: bool,
    },
    Composer,
    /// Some other field: plain letters would type into it.
    OtherText,
}

/// What the Grooming screen is showing, for the hint bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroomHint {
    PreCheck,
    /// The pre-check summary dialog is open.
    Summary,
    /// The overwrite confirm is open.
    Overwrite,
    Session,
    /// Session, placing an accepted ticket.
    Placement,
    /// Session, with the bounce box open.
    Bounce,
}

/// Which kind of picker is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerHint {
    Edit,
    Assign,
    Other,
}

/// What the hint bar needs to know about the screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HintState {
    pub help_open: bool,
    pub palette_open: bool,
    pub picker: Option<PickerHint>,
    pub has_selection: bool,
    pub focus: Focus,
    /// A server search (text or JQL) is on screen, so Esc clears it even with the box unfocused.
    pub search_shown: bool,
    /// Writes are blocked and no composer is drawn (`detail::composer_read_only`): no composer keys.
    pub read_only: bool,
    /// The Grooming screen is showing, and what it shows.
    pub grooming: Option<GroomHint>,
    /// The Team screen is showing.
    pub team: bool,
    /// The Roadmap screen is showing, on this tab (1 to 4).
    pub roadmap: Option<u8>,
    /// The Me screen is showing.
    pub me: bool,
    /// The Standup screen is showing.
    pub standup: bool,
    /// The Standup screen has at least one suggestion card.
    pub standup_cards: bool,
}

impl HintState {
    #[cfg(test)]
    pub fn new(focus: Focus) -> Self {
        HintState {
            help_open: false,
            palette_open: false,
            picker: None,
            has_selection: false,
            focus,
            search_shown: false,
            read_only: false,
            grooming: None,
            team: false,
            roadmap: None,
            me: false,
            standup: false,
            standup_cards: false,
        }
    }
}

pub fn hint_context(s: HintState) -> HintContext {
    let HintState { help_open, palette_open, picker, has_selection, focus, search_shown, read_only, grooming, team, roadmap, me, standup, standup_cards } = s;
    match (help_open, palette_open, picker) {
        (true, _, _) => HintContext::Help,
        // An open picker or the palette owns the bar on every screen, Grooming included.
        (_, _, Some(PickerHint::Edit)) => HintContext::EditPicker,
        (_, _, Some(PickerHint::Assign)) => HintContext::AssignPicker,
        (_, _, Some(PickerHint::Other)) => HintContext::Picker,
        (_, true, None) => HintContext::Palette,
        _ if grooming == Some(GroomHint::PreCheck) => HintContext::GroomPreCheck,
        _ if grooming == Some(GroomHint::Summary) && read_only => HintContext::GroomSummaryReadOnly,
        _ if grooming == Some(GroomHint::Summary) => HintContext::GroomSummary,
        _ if grooming == Some(GroomHint::Overwrite) => HintContext::GroomOverwrite,
        _ if grooming == Some(GroomHint::Session) => HintContext::GroomSession,
        _ if grooming == Some(GroomHint::Placement) => HintContext::GroomPlacement,
        _ if grooming == Some(GroomHint::Bounce) && read_only => HintContext::GroomBounceReadOnly,
        _ if grooming == Some(GroomHint::Bounce) => HintContext::GroomBounce,
        _ if team => HintContext::Team,
        _ if me => HintContext::Me,
        // The title field takes plain letters, so no card keys while it has focus.
        _ if standup && focus == Focus::None && !standup_cards => HintContext::StandupEmpty,
        _ if standup && focus == Focus::None => HintContext::Standup,
        _ if standup => HintContext::StandupTitle,
        // Timeline and Outcomes have no cursor (`views::roadmap::tab_rows` is empty), so no move or open hint.
        _ if matches!(roadmap, Some(2 | 4)) => HintContext::RoadmapView,
        _ if roadmap.is_some() => HintContext::Roadmap,
        _ => match focus {
            Focus::Search { clear: true, plain: true } => HintContext::SearchText,
            Focus::Search { clear: true, plain: false } => HintContext::Search,
            Focus::Search { clear: false, .. } => HintContext::SearchEmpty,
            Focus::Composer if !read_only => HintContext::Composer,
            Focus::None if has_selection && search_shown => HintContext::SelectedSearch,
            Focus::None if has_selection => HintContext::Selected,
            Focus::None if search_shown => HintContext::BrowseSearch,
            _ => HintContext::Browse,
        },
    }
}

/// The hints for what the keyboard does right now on the Issues screen.
pub fn hints(c: HintContext) -> &'static [Hint] {
    match c {
        // Alternatives are separate chips; a slash only joins an up/down pair.
        HintContext::Help => &[&[K("?"), W("or"), K("Esc"), W("close")]],
        HintContext::Palette => &[&[K("↑/↓"), W("move")], &[K("Enter"), W("open")], &[K("⌘Enter"), W("open in Jira")], &[K("Esc"), W("close")]],
        HintContext::Search => &[&[K("Enter"), W("run as JQL")], &[K("Esc"), W("clear")]],
        HintContext::SearchText => &[&[K("Enter"), W("run as JQL")], &[K("Esc"), W("clear")]],
        HintContext::SearchEmpty => &[&[K("Enter"), W("run as JQL")], &[K("Esc"), W("done")]],
        HintContext::Composer => &[&[K("⌘Enter"), W("post")], &[K("Esc"), W("done (draft kept)")]],
        HintContext::EditPicker => &[&[K("Tab/⇧Tab"), W("next or previous field")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::AssignPicker => &[&[W("Type to filter")], &[K("↑/↓"), W("move")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::Picker => &[&[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::Selected => &[
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("t"), W("transition")],
            &[K("a"), W("assign")],
            &[K("s"), W("sprint")],
            &[K("e"), W("edit")],
            &[K("l"), W("label")],
            &[K("J/K"), W("rank")],
            &[K("c"), W("copy")],
            &[K("⌘O"), W("open")],
            &[K("?"), W("help")],
        ],
        HintContext::SelectedSearch => &[
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("Esc"), W("clear")],
            &[K("t"), W("transition")],
            &[K("a"), W("assign")],
            &[K("s"), W("sprint")],
            &[K("e"), W("edit")],
            &[K("l"), W("label")],
            &[K("J/K"), W("rank")],
            &[K("c"), W("copy")],
            &[K("⌘O"), W("open")],
            &[K("?"), W("help")],
        ],
        HintContext::GroomPreCheck => &[
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("r"), W("ready")],
            &[K("w/n/e"), W("needs detail")],
            &[K("u"), W("undo")],
            &[K("Enter"), W("summary")],
            &[K("Tab"), W("session")],
        ],
        HintContext::GroomSummary => &[&[K("Enter"), W("promote")], &[K("Esc"), W("cancel")]],
        HintContext::GroomSummaryReadOnly => &[&[K("Esc"), W("cancel")]],
        HintContext::GroomOverwrite => &[&[K("Enter"), W("overwrite")], &[K("Esc"), W("keep the old one")]],
        HintContext::GroomSession => &[
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("]/["), W("epic")],
            &[K("y"), W("accept")],
            &[K("b"), W("bounce")],
            &[K("p"), W("park")],
            &[K("J/K"), W("rank")],
            &[K("l"), W("label")],
            &[K("a"), W("assign")],
            &[K("c"), W("close")],
            &[K("s"), W("sprint")],
            &[K("Tab"), W("pre-check")],
            &[K("?"), W("help")],
        ],
        HintContext::GroomPlacement => &[&[K("↑/↓"), W("move")], &[K("Enter"), W("place")], &[K("Esc"), W("cancel")]],
        HintContext::GroomBounce => &[&[K("⌘Enter"), W("post")], &[K("Esc"), W("cancel")]],
        HintContext::GroomBounceReadOnly => &[&[K("Esc"), W("cancel")]],
        HintContext::Team => &[
            &[K("h/l"), W("or"), K("←/→"), W("member")],
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("Enter"), W("open")],
            &[K("d"), W("done")],
            &[K("?"), W("help")],
        ],
        HintContext::Roadmap => {
            &[&[K("1"), W("to"), K("4"), W("tab")], &[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("Enter"), W("open")], &[K("?"), W("help")]]
        }
        HintContext::RoadmapView => &[&[K("1"), W("to"), K("4"), W("tab")], &[K("?"), W("help")]],
        HintContext::Me => &[&[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("Enter"), W("open")], &[K("⌘O"), W("Jira")], &[K("?"), W("help")]],
        HintContext::Standup => &[
            &[K("j/k"), W("or"), K("↑/↓"), W("move")],
            &[K("c"), W("current sprint")],
            &[K("n"), W("next sprint")],
            &[K("b"), W("backlog")],
            &[K("x"), W("dismiss")],
            &[K("e"), W("edit")],
            &[K("o"), W("open")],
            &[K("y"), W("copy")],
            &[K("r"), W("run again")],
            &[K("?"), W("help")],
        ],
        HintContext::StandupEmpty => &[&[K("r"), W("run")], &[K("y"), W("copy")], &[K("?"), W("help")]],
        HintContext::StandupTitle => &[&[K("Enter"), W("or"), K("Esc"), W("done (title kept)")]],
        HintContext::SignIn => &[&[K("Enter"), W("sign in")]],
        HintContext::Browse => &[&[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("⌘K"), W("go to")], &[K("⌘F"), W("search")], &[K("?"), W("help")]],
        HintContext::BrowseSearch => {
            &[&[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("Esc"), W("clear")], &[K("⌘K"), W("go to")], &[K("⌘F"), W("search")], &[K("?"), W("help")]]
        }
    }
}

/// The Move to Done hint (Shift+D, `BINDINGS`' global "D").
pub const DONE_HINT: Hint = &[K("D"), W("move to Done")];

/// The bar contexts with a focused ticket that `D` can move: the Issues selection, Grooming's cursor row, the Team
/// card, the Roadmap initiative, the Me ticket and the Standup card.
fn done_applies(c: HintContext) -> bool {
    use HintContext::*;
    matches!(c, Selected | SelectedSearch | GroomPreCheck | GroomSession | Team | Roadmap | Me | Standup)
}

/// `hs` with DONE_HINT where `D` would act: a screen context from `done_applies` with a `focused` ticket
/// (`App::done_target`). It goes right before "? help" (or last), so help stays the bar's end.
pub fn with_done_hint(c: HintContext, mut hs: Vec<Hint>, focused: bool) -> Vec<Hint> {
    if focused && done_applies(c) {
        let at = hs.iter().position(|h| h.first() == Some(&K("?"))).unwrap_or(hs.len());
        hs.insert(at, DONE_HINT);
    }
    hs
}

/// The Standup header's run button (and so the `r` key), labelled for the state it is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StandupRun {
    /// No review yet.
    Now,
    /// A review is on screen.
    Again,
    /// The last run failed.
    Retry,
}

impl StandupRun {
    /// The button's label; the hint says the same words.
    pub fn label(self) -> &'static str {
        match self {
            StandupRun::Now => "Run now",
            StandupRun::Again => "Run again",
            StandupRun::Retry => "Try again",
        }
    }

    fn hint(self) -> Hint {
        match self {
            StandupRun::Now => &[K("r"), W("run now")],
            StandupRun::Again => &[K("r"), W("run again")],
            StandupRun::Retry => &[K("r"), W("try again")],
        }
    }
}

/// What the Standup screen offers right now, for its hint bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StandupHint {
    /// At least one suggestion card is showing.
    pub cards: bool,
    /// There is a review to copy.
    pub copy: bool,
    /// The run button, when it shows (not while a review runs).
    pub run: Option<StandupRun>,
    /// Writes are blocked: the cards have no add buttons, so no add keys.
    pub read_only: bool,
}

/// The Standup hint bar with its title field unfocused: only the keys that do something now, the run key named
/// like its button.
pub fn standup_hints(s: StandupHint) -> Vec<Hint> {
    let mut out: Vec<Hint> = Vec::new();
    if s.cards {
        out.push(&[K("j/k"), W("or"), K("↑/↓"), W("move")]);
        if !s.read_only {
            out.extend_from_slice(&[&[K("c"), W("current sprint")], &[K("n"), W("next sprint")], &[K("b"), W("backlog")]]);
        }
        out.extend_from_slice(&[&[K("x"), W("dismiss")], &[K("e"), W("edit")], &[K("o"), W("open")]]);
    }
    if s.copy {
        out.push(&[K("y"), W("copy")]);
    }
    if let Some(r) = s.run {
        out.push(r.hint());
    }
    out.push(&[K("?"), W("help")]);
    out
}

/// Plain text of a hint line, for tests.
#[cfg(test)]
pub fn hint_text(hs: &[Hint]) -> String {
    hs.iter()
        .map(|h| {
            h.iter()
                .map(|t| match t {
                    K(s) | W(s) => *s,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// How many leading hints fit in `available`, given each hint's width, the separator's, and the
/// trailing "…"'s. Returns (count, elided): when not all fit, room is kept for the ellipsis.
pub fn fit_hints(widths: &[f32], sep: f32, ellipsis: f32, available: f32) -> (usize, bool) {
    let total: f32 = widths.iter().sum::<f32>() + sep * widths.len().saturating_sub(1) as f32;
    if total <= available {
        return (widths.len(), false);
    }
    let mut used = 0.0;
    let mut n = 0;
    for (i, w) in widths.iter().enumerate() {
        let next = used + if i > 0 { sep } else { 0.0 } + w;
        if next + sep + ellipsis > available {
            break;
        }
        used = next;
        n += 1;
    }
    (n, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_matches_bindings() {
        let readme = include_str!("../README.md").replace("\r\n", "\n");
        let start = readme.find("<!-- keys:start -->").expect("start marker") + "<!-- keys:start -->".len();
        let end = readme.find("<!-- keys:end -->").expect("end marker");
        assert_eq!(readme[start..end].trim(), readme_table().trim());
    }

    #[test]
    fn readme_table_groups_and_quotes_keys() {
        let t = readme_table();
        assert!(t.starts_with("| Keys | Action |\n| --- | --- |\n| **Global** | |\n| `⌘1` to `⌘6` | Issues, Grooming, Team, Roadmap, Me, Standup |\n"));
        assert_eq!(range("⌘1 to ⌘6"), ["⌘1", "⌘6"]);
        assert_eq!(range("j/k"), ["j/k"]);
        assert!(t.contains("| `⌘F` or `/` | Focus search |\n"));
        assert!(t.contains("| `⌘K` | Search tickets, open one by key or number, or run a command |\n"));
        assert!(t.contains("| `J/K` | Move a Ready for Work ticket down or up in rank |\n"));
        assert!(t.contains("| `j/k` or `↑/↓` | Move |\n"), "one alternatives order, no keys in prose");
        let global = BINDINGS.iter().filter(|k| k.group == Group::Global).count();
        assert_eq!(global, 9, "only the keys that work on every screen live in Global (D and ⌘K included); ⌘O is an Issues key");
        assert!(t.contains("| **Issues** | |\n| `⌘O` | Open in Jira |\n"));
        assert!(t.contains("| `Esc` | Cancel or close |\n| `?` or `Esc` | Close this help |\n"), "Esc right above the help row");
        assert!(t.contains("| `?` or `Esc` | Close this help |\n| **Issues** | |\n"), "one help row, the last of Global");
        assert!(t.contains("| `J/K` | Move down or up in rank |\n"));
        assert!(t.contains("| `⌘Enter` | Post the comment |\n"));
        assert!(!t.contains("Close the palette"), "the palette shows its own Esc");
        assert!(t.contains("| `j/k` or `↑/↓` | Next or previous issue |\n"));
        assert!(t.contains("| **Grooming** | |\n| `j/k` or `↑/↓` | Next or previous ticket |\n| `r` | Ready |\n"));
        assert!(t.contains("| `Tab` | Switch between Pre-check and Session |\n| **Team** | |\n| `h/l` or `←/→` | Previous or next member |\n"));
        assert!(t.contains(
            "| `d` | Show or hide done in the last 7 days |\n| **Roadmap** | |\n| `1` to `4` | Now / Next / Later, Timeline, Status review, Outcomes |\n"
        ));
        assert!(t.contains("| `Enter` | Open in Issues |\n| **Me** | |\n| `j/k` or `↑/↓` | Next or previous ticket |\n"));
        assert!(t.contains("| `⌘O` | Open in Jira |\n| **Standup** | |\n| `j/k` or `↑/↓` | Next or previous ticket or suggestion |\n"));
        assert!(t.contains("| `r` | Run the review again |\n| **Pickers and Edit** | |\n"));
        assert!(t.contains("| `⌘1` to `⌘6` | Issues, Grooming, Team, Roadmap, Me, Standup |\n"));
        assert_eq!(t.matches("| **").count(), 8);
    }

    fn st(help_open: bool, palette_open: bool, picker: Option<PickerHint>, has_selection: bool, focus: Focus) -> HintState {
        HintState { help_open, palette_open, picker, has_selection, ..HintState::new(focus) }
    }

    #[test]
    fn hints_follow_context() {
        use HintContext::*;
        let search = |sel| HintState { has_selection: sel, search_shown: true, ..HintState::new(Focus::None) };
        assert_eq!(hint_context(search(true)), SelectedSearch, "a search on screen keeps Esc clear after the box blurs");
        assert_eq!(hint_context(search(false)), BrowseSearch);
        let groom = HintState { grooming: Some(GroomHint::PreCheck), ..HintState::new(Focus::None) };
        assert_eq!(hint_context(HintState { help_open: true, ..groom }), Help, "help still wins");
        assert_eq!(hint_text(hints(BrowseSearch)), "j/k or ↑/↓ move · Esc clear · ⌘K go to · ⌘F search · ? help");
        assert!(hint_text(hints(SelectedSearch)).starts_with("j/k or ↑/↓ move · Esc clear · t transition"));
        assert_eq!(hint_context(st(true, true, Some(PickerHint::Other), true, Focus::OtherText)), Help, "help wins");
        assert_eq!(hint_context(st(false, false, Some(PickerHint::Edit), true, Focus::OtherText)), EditPicker);
        assert_eq!(
            hint_context(st(false, false, Some(PickerHint::Other), false, Focus::OtherText)),
            Picker,
            "a picker's own text field still means picker keys"
        );
        assert_eq!(hint_context(st(false, false, Some(PickerHint::Assign), true, Focus::OtherText)), AssignPicker);
        assert_eq!(hint_text(hints(AssignPicker)), "Type to filter · ↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hint_context(st(false, true, None, true, Focus::OtherText)), Palette);
        assert_eq!(hint_context(st(false, false, None, true, Focus::Search { clear: true, plain: false })), Search);
        assert_eq!(hint_context(st(false, false, None, true, Focus::Search { clear: true, plain: true })), SearchText);
        assert_eq!(hint_text(hints(SearchText)), "Enter run as JQL · Esc clear");
        assert_eq!(hint_context(st(false, false, None, true, Focus::Search { clear: false, plain: false })), SearchEmpty);
        assert_eq!(hint_text(hints(SearchEmpty)), "Enter run as JQL · Esc done");
        assert_eq!(hint_context(st(false, false, None, true, Focus::Composer)), Composer, "the bar follows focus into the composer");
        assert_eq!(hint_context(st(false, false, None, true, Focus::None)), Selected);
        assert_eq!(hint_context(st(false, false, None, true, Focus::OtherText)), Browse, "letters type into the field, so no selection hints");
        assert_eq!(hint_context(st(false, false, None, false, Focus::None)), Browse);
        assert_eq!(hint_text(hints(Composer)), "⌘Enter post · Esc done (draft kept)");
        let ro = HintState { read_only: true, has_selection: true, ..HintState::new(Focus::Composer) };
        let bar = hint_text(hints(hint_context(ro)));
        assert!(!bar.contains("⌘Enter") && !bar.contains("draft"), "read-only has no composer, so no composer keys: {bar}");
        assert_eq!(hint_text(hints(Help)), "? or Esc close");
        assert_eq!(hint_text(hints(SignIn)), "Enter sign in");
        let groom = |g| HintState { grooming: Some(g), has_selection: true, ..HintState::new(Focus::None) };
        assert_eq!(hint_context(groom(GroomHint::PreCheck)), GroomPreCheck, "the Issues selection does not leak onto Grooming");
        assert_eq!(hint_context(groom(GroomHint::Session)), GroomSession);
        assert_eq!(hint_context(groom(GroomHint::Summary)), GroomSummary);
        assert_eq!(hint_context(groom(GroomHint::Overwrite)), GroomOverwrite);
        assert_eq!(hint_context(HintState { help_open: true, ..groom(GroomHint::Session) }), Help, "help wins on Grooming too");
        assert_eq!(hint_text(hints(GroomPreCheck)), "j/k or ↑/↓ move · r ready · w/n/e needs detail · u undo · Enter summary · Tab session");
        assert_eq!(hint_text(hints(GroomSummary)), "Enter promote · Esc cancel");
        assert_eq!(hint_context(HintState { read_only: true, ..groom(GroomHint::Summary) }), GroomSummaryReadOnly);
        assert_eq!(hint_text(hints(GroomSummaryReadOnly)), "Esc cancel", "read-only cannot promote");
        assert_eq!(hint_text(hints(GroomOverwrite)), "Enter overwrite · Esc keep the old one");
        assert_eq!(
            hint_text(hints(GroomSession)),
            "j/k or ↑/↓ move · ]/[ epic · y accept · b bounce · p park · J/K rank · l label · a assign · c close · s sprint · Tab pre-check · ? help"
        );
        assert_eq!(hint_context(groom(GroomHint::Placement)), GroomPlacement);
        assert_eq!(hint_text(hints(GroomPlacement)), "↑/↓ move · Enter place · Esc cancel");
        assert_eq!(hint_context(HintState { focus: Focus::OtherText, ..groom(GroomHint::Bounce) }), GroomBounce, "the box has focus");
        assert_eq!(hint_text(hints(GroomBounce)), "⌘Enter post · Esc cancel");
        assert_eq!(hint_context(HintState { read_only: true, focus: Focus::OtherText, ..groom(GroomHint::Bounce) }), GroomBounceReadOnly);
        assert_eq!(hint_text(hints(GroomBounceReadOnly)), "Esc cancel", "read-only cannot post");
        let team = HintState { team: true, has_selection: true, ..HintState::new(Focus::None) };
        assert_eq!(hint_context(team), Team, "the Issues selection does not leak onto Team");
        assert_eq!(hint_context(HintState { help_open: true, ..team }), Help);
        assert_eq!(hint_text(hints(Team)), "h/l or ←/→ member · j/k or ↑/↓ move · Enter open · d done · ? help");
        let roadmap = |tab| HintState { roadmap: Some(tab), has_selection: true, ..HintState::new(Focus::None) };
        assert_eq!(hint_context(roadmap(1)), Roadmap, "the Issues selection does not leak onto Roadmap");
        assert_eq!(hint_context(roadmap(3)), Roadmap, "Status review has a cursor");
        assert_eq!(hint_context(HintState { help_open: true, ..roadmap(1) }), Help);
        assert_eq!(hint_text(hints(Roadmap)), "1 to 4 tab · j/k or ↑/↓ move · Enter open · ? help");
        assert_eq!(hint_context(roadmap(2)), RoadmapView, "Timeline has no cursor");
        assert_eq!(hint_context(roadmap(4)), RoadmapView, "Outcomes has no cursor");
        assert_eq!(hint_context(HintState { help_open: true, ..roadmap(2) }), Help);
        assert_eq!(hint_text(hints(RoadmapView)), "1 to 4 tab · ? help");
        let me = HintState { me: true, has_selection: true, ..HintState::new(Focus::None) };
        assert_eq!(hint_context(me), Me, "the selection the Me screen sets does not bring the Issues keys");
        assert_eq!(hint_context(HintState { help_open: true, ..me }), Help);
        assert_eq!(hint_text(hints(Me)), "j/k or ↑/↓ move · Enter open · ⌘O Jira · ? help");
        let standup = |focus| HintState { standup: true, standup_cards: true, has_selection: true, ..HintState::new(focus) };
        let no_cards = HintState { standup_cards: false, ..standup(Focus::None) };
        assert_eq!(hint_context(no_cards), StandupEmpty, "no cards, no card keys");
        assert_eq!(hint_text(hints(StandupEmpty)), "r run · y copy · ? help");
        assert_eq!(hint_context(standup(Focus::None)), Standup, "the Issues selection does not leak onto Standup");
        assert_eq!(hint_context(standup(Focus::OtherText)), StandupTitle, "editing a title: letters type");
        assert_eq!(hint_text(hints(StandupTitle)), "Enter or Esc done (title kept)", "both end the edit and keep the text");
        assert_eq!(hint_context(HintState { help_open: true, ..standup(Focus::None) }), Help);
        assert_eq!(
            hint_text(hints(Standup)),
            "j/k or ↑/↓ move · c current sprint · n next sprint · b backlog · x dismiss · e edit · o open · y copy · r run again · ? help"
        );
        assert_eq!(
            hint_text(&standup_hints(StandupHint { cards: true, copy: true, run: Some(StandupRun::Again), read_only: false })),
            hint_text(hints(Standup))
        );
        assert_eq!(
            hint_text(&standup_hints(StandupHint { cards: false, copy: false, run: Some(StandupRun::Now), read_only: false })),
            "r run now · ? help",
            "no review: nothing to copy"
        );
        assert_eq!(
            hint_text(&standup_hints(StandupHint { cards: false, copy: false, run: None, read_only: false })),
            "? help",
            "a first run: no r while it runs"
        );
        assert_eq!(
            hint_text(&standup_hints(StandupHint { cards: false, copy: true, run: Some(StandupRun::Retry), read_only: false })),
            "y copy · r try again · ? help"
        );
        assert_eq!(
            hint_text(&standup_hints(StandupHint { cards: true, copy: true, run: Some(StandupRun::Again), read_only: true })),
            "j/k or ↑/↓ move · x dismiss · e edit · o open · y copy · r run again · ? help",
            "read-only: no add keys"
        );
        let running = hint_text(&standup_hints(StandupHint { cards: true, copy: true, run: None, read_only: false }));
        assert!(running.ends_with("o open · y copy · ? help"), "{running}");
        for r in [StandupRun::Now, StandupRun::Again, StandupRun::Retry] {
            assert_eq!(hint_text(&[r.hint()]), format!("r {}", r.label().to_lowercase()), "the hint says the button's words");
        }
        assert_eq!(hint_text(hints(Palette)), "↑/↓ move · Enter open · ⌘Enter open in Jira · Esc close");
        assert_eq!(hint_text(hints(Search)), "Enter run as JQL · Esc clear", "one Enter wording for every search text");
        assert_eq!(hint_text(hints(EditPicker)), "Tab/⇧Tab next or previous field · Enter confirm · Esc cancel");
        assert_eq!(hint_text(hints(Picker)), "j/k or ↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hint_text(hints(Browse)), "j/k or ↑/↓ move · ⌘K go to · ⌘F search · ? help");
        assert_eq!(
            hint_text(hints(Selected)),
            "j/k or ↑/↓ move · t transition · a assign · s sprint · e edit · l label · J/K rank · c copy · ⌘O open · ? help"
        );
    }

    #[test]
    fn move_to_done_hint_shows_where_d_acts() {
        use HintContext::*;
        let with = |c, focused| hint_text(&with_done_hint(c, hints(c).to_vec(), focused));
        assert_eq!(
            with(Selected, true),
            "j/k or ↑/↓ move · t transition · a assign · s sprint · e edit · l label · J/K rank · c copy · ⌘O open · D move to Done · ? help"
        );
        assert_eq!(with(Team, true), "h/l or ←/→ member · j/k or ↑/↓ move · Enter open · d done · D move to Done · ? help");
        assert_eq!(with(Me, true), "j/k or ↑/↓ move · Enter open · ⌘O Jira · D move to Done · ? help");
        assert_eq!(with(Roadmap, true), "1 to 4 tab · j/k or ↑/↓ move · Enter open · D move to Done · ? help");
        assert_eq!(with(GroomPreCheck, true), format!("{} · D move to Done", hint_text(hints(GroomPreCheck))), "no help hint: last");
        assert!(with(GroomSession, true).ends_with("Tab pre-check · D move to Done · ? help"));
        assert!(with(SelectedSearch, true).ends_with("⌘O open · D move to Done · ? help"));
        let standup = standup_hints(StandupHint { cards: true, copy: true, run: Some(StandupRun::Again), read_only: false });
        assert!(hint_text(&with_done_hint(Standup, standup, true)).ends_with("r run again · D move to Done · ? help"));
        // Nothing focused (an unadded Standup card, an empty Grooming list) or no ticket on screen: no D.
        assert_eq!(with(Team, false), hint_text(hints(Team)));
        for c in [Browse, BrowseSearch, RoadmapView, StandupEmpty, StandupTitle, Search, Composer, Picker, EditPicker, Palette, Help, GroomSummary] {
            assert_eq!(with(c, true), hint_text(hints(c)), "{c:?}");
        }
        assert!(BINDINGS.iter().any(|b| b.keys == "D" && b.group == Group::Global), "the hint names a bound key");
    }

    #[test]
    fn an_open_picker_or_the_palette_owns_the_bar_on_grooming() {
        let on_grooming = |picker, palette_open| HintState { picker, palette_open, grooming: Some(GroomHint::PreCheck), ..HintState::new(Focus::None) };
        assert_eq!(hint_context(on_grooming(Some(PickerHint::Other), false)), HintContext::Picker);
        assert_eq!(hint_context(on_grooming(Some(PickerHint::Edit), false)), HintContext::EditPicker);
        assert_eq!(hint_context(on_grooming(None, true)), HintContext::Palette);
        assert_eq!(hint_context(on_grooming(None, false)), HintContext::GroomPreCheck);
    }

    #[test]
    fn hint_bar_elides_from_the_right() {
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 40.0), (3, false), "exactly fits");
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 39.0), (2, true), "two plus separator and ellipsis");
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 18.0), (0, true));
        assert_eq!(fit_hints(&[], 5.0, 4.0, 0.0), (0, false));
    }

    #[test]
    fn notation_is_slash_pairs() {
        assert!(!BINDINGS.iter().any(|k| k.keys.contains("↓/↑")), "up before down everywhere");
        assert!(!BINDINGS.iter().any(|k| k.keys.contains("↑/↓, j/k")), "j/k or ↑/↓, one order everywhere");
        assert!(hint_text(hints(HintContext::Picker)).starts_with("j/k or ↑/↓"));
        for k in BINDINGS {
            assert!(!k.keys.contains("j, k") && !k.keys.contains("J, K") && !k.keys.contains("↑, ↓"), "{}", k.keys);
        }
    }
}
