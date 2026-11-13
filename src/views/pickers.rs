//! Modal pickers for transitions, resolutions, assignees, sprints, and field edits.
//! Keys (↑/↓/j/k, Enter, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{
    Action, App, Picker, assign_current_row, assign_cursor_after_filter, assign_empty_text, assign_matches, assign_note, assign_rows, assign_search_term,
    sprint_current_row, sprint_row_label, transition_row,
};
use crate::theme::{self, Palette};
use egui::{Frame, Margin, RichText, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = theme::palette(ctx);
    if app.picker_loading {
        egui::Area::new(egui::Id::new("picker-loading")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        });
    }
    let help_open = app.help_open;
    let hint_bar = crate::views::hint_bar_height(app);
    // The issue's current status and sprint, for the "current" marker.
    let issue = app.picker.as_ref().map(Picker::key).and_then(|k| app.store.get(k));
    let status = issue.map(|i| i.status.name.clone()).unwrap_or_default();
    let sprint = issue.and_then(|i| i.sprint.clone());
    let app_pills = app.detail_pills_top;
    let Some(picker) = app.picker.as_mut() else { return };
    // Dim the page behind the dialog and swallow its clicks.
    crate::views::backdrop(ctx, "picker-backdrop", egui::Order::Middle, hint_bar, p);
    let mut confirm = false;
    let mut cancel = false;
    // Off when nothing is selected (an Assign filter that matched nobody) or the choice is the current value.
    let mut can_confirm = true;
    // A fixed top edge, so a list that shrinks while filtering never moves the dialog: PILLS_MARGIN over the
    // detail pane's pill row when it shows, else a fifth down the window.
    let top = picker_top(ctx.content_rect().height(), app_pills);
    // Every picker writes: in read-only mode Confirm is off and a note under the title says why, before the choices.
    let read_only = crate::jira::check_writable().is_err().then(|| read_only_note(picker));
    let note = read_only.as_deref();
    // Read-only, the body is view-only too: rows and fields in the disabled style (muted, no cursor, no caret).
    let live = read_only.is_none();
    egui::Area::new(egui::Id::new("picker")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        crate::views::components::dialog_frame(p).inner_margin(DIALOG_MARGIN).show(ui, |ui| {
            ui.set_width(DIALOG_W);
            // Fields on `bg` with the dialog's own edge, in both themes.
            crate::views::dialog_fields(ui, p);
            let buttons_gap = match picker {
                Picker::Transition { key, items, cursor, done } => {
                    let below = title_with_note(ui, p, &if *done { format!("Move {key} to Done") } else { format!("Transition {key}") }, note);
                    let names = items.iter().map(|t| Row::plain(transition_row(t)).current_if(t.to.eq_ignore_ascii_case(&status))).collect();
                    rows(ui, p, names, live, &mut confirm, cursor, None, Some(below))
                }
                Picker::Resolution { key, items, cursor, close, .. } => {
                    let below = title_with_note(ui, p, &if *close { format!("Close {key} as") } else { format!("Resolution for {key}") }, note);
                    rows(ui, p, items.iter().map(|r| Row::plain(r.name.clone())).collect(), live, &mut confirm, cursor, None, Some(below))
                }
                Picker::Assign { key, items, current, cursor, filter, search } => {
                    title_with_note(ui, p, &format!("Assign {key}"), note);
                    ui.add_space(TITLE_GAP);
                    let r = crate::views::add_field(ui, None, filter, |t| field_if(t, "Type a name", live, p));
                    ui.add_space(assign_field_space(ui));
                    // Not under the help overlay, or typing would land in the filter behind it.
                    if !help_open && live {
                        r.request_focus();
                    }
                    let (shown, cut) = assign_rows(items, filter, current.as_ref());
                    if r.changed() {
                        *cursor = assign_cursor_after_filter(&shown, filter, current.is_some());
                        search.edited = Some(std::time::Instant::now());
                    }
                    if Picker::assign_empty(filter, shown.len()) {
                        // Nothing matched: no row is selected, so Enter cannot unassign.
                        can_confirm = false;
                        let searching = search.pending.is_some() || assign_search_term(filter, 0, search.asked.as_deref()).is_some();
                        let (message, hint) = assign_empty_text(filter, searching);
                        Frame::new().inner_margin(Margin::symmetric(ROW_INSET as i8, ROW_PAD_Y)).show(ui, |ui| {
                            crate::views::components::empty_state(ui, p, &message, Some(hint));
                        });
                        return_gap(ui)
                    } else {
                        let now = assign_current_row(&shown, current.as_ref());
                        let mut names = vec![Row { avatar: Some(None), ..Row::muted("Unassigned") }];
                        names.extend(shown.iter().map(|u| Row { avatar: Some(Some(u.account_id.clone())), ..Row::plain(u.display_name.clone()) }));
                        // Unassigned and the capped users: the list never scrolls.
                        names.truncate(crate::app::ASSIGN_USERS + 1);
                        for (i, n) in names.iter_mut().enumerate() {
                            n.current = now == Some(i);
                        }
                        if *cursor >= names.len() {
                            *cursor = 0;
                        }
                        let note = assign_note(filter, shown.len(), assign_matches(items, filter, current.as_ref()), cut);
                        // The field sits under the title; the rows' own top gap is ASSIGN_FIELD_GAP (above).
                        rows(ui, p, names, live, &mut confirm, cursor, Some(note.as_deref()), None)
                    }
                }
                Picker::Sprint { key, items, cursor } => {
                    let below = title_with_note(ui, p, &format!("Move {key} to sprint"), note);
                    let now = sprint_current_row(items, sprint.as_deref());
                    let mut names = vec![Row::plain("Backlog".to_string())];
                    names.extend(items.iter().map(|s| {
                        let (name, tag) = sprint_row_label(&s.name, &s.state);
                        Row { tag, ..Row::plain(name) }
                    }));
                    for (i, n) in names.iter_mut().enumerate() {
                        n.current = now == Some(i);
                    }
                    rows(ui, p, names, live, &mut confirm, cursor, None, Some(below))
                }
                Picker::Edit { key, labels, priority, points, orig_priority, .. } => {
                    let below = title_with_note(ui, p, &format!("Edit {key}"), note);
                    ui.add_space(edit_title_space(ui, below, "Labels, comma separated"));
                    let labels_id = egui::Id::new("edit-labels");
                    let r = edit_group(ui, p, "Labels, comma separated", labels, |t| field_if(t, "", live, p).id(labels_id));
                    // Labels takes focus on open (and whenever no field holds it); Tab moves on.
                    if !help_open && live && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    ui.add_space(group_space(ui, "Priority"));
                    priority_group(ui, p, priority, orig_priority, live);
                    ui.add_space(group_space(ui, "Story points"));
                    edit_group(ui, p, "Story points", points, |t| field_if(t, crate::app::POINTS_HINT, live, p));
                    BUTTONS_GAP
                }
            };
            ui.add_space(buttons_gap);
            can_confirm &= picker.changes(&status, sprint.as_deref());
            if read_only.is_some() {
                can_confirm = false;
            }
            // The hint bar carries the keys; the dialog carries one primary action.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let confirm_button =
                    if can_confirm { crate::views::primary_button_if("Confirm", true, p) } else { crate::views::components::primary_button_off("Confirm", p) };
                if ui.add(confirm_button).clicked() && can_confirm {
                    confirm = true;
                }
                // The app's one secondary button, as beside Copy link and Open in Jira.
                if ui.add(crate::views::button("Cancel", p)).clicked() {
                    cancel = true;
                }
            });
        });
    });
    if confirm {
        app.actions.push(Action::PickerConfirm);
    }
    if cancel {
        app.actions.push(Action::PickerCancel);
    }
}

/// The Edit dialog's Priority group. Jira's five defaults are segments ("Not set" first and selected when the
/// issue has none); view-only (`live` false) they are the same segments, disabled, so a choice from a list reads as
/// one in both modes. A priority outside the defaults (the site has its own) is typed, and disabled when view-only.
fn priority_group(ui: &mut egui::Ui, p: &Palette, priority: &mut String, orig_priority: &str, live: bool) {
    if custom_priority(orig_priority) {
        if live {
            edit_group(ui, p, "Priority", priority, |t| field(t, crate::app::PRIORITY_HINT, p));
        } else {
            let mut shown = read_only_priority(priority).to_string();
            edit_group(ui, p, "Priority", &mut shown, |t| field_if(t, "", false, p));
        }
        return;
    }
    edit_label(ui, p, "Priority");
    let unset = orig_priority.trim().is_empty();
    let options = priority_options(unset);
    let picked = ui.add_enabled_ui(live, |ui| crate::views::components::segmented(ui, p, &options, priority_segment(priority, unset))).inner;
    if let Some(i) = picked.filter(|_| live) {
        *priority = priority_from_segment(i, unset).to_string();
    }
}

/// The space before the buttons after Assign's empty state (its hint's descenders are its ink).
fn return_gap(ui: &egui::Ui) -> f32 {
    let ink = theme::Ink::of(ui.ctx(), &theme::body(theme::spacing(ui.ctx()).small), None);
    LIST_INK_GAP - f32::from(ROW_PAD_Y) - (ink.line - (ink.base + ink.descent)) - ui.spacing().item_spacing.y
}

/// Why Confirm is off in read-only mode, naming the picker's action: "Read-only mode is on, so Close is off. …",
/// the restart hint on its own line so the note breaks the same way in every picker.
pub fn read_only_note(picker: &Picker) -> String {
    let (name, verb) = match picker {
        Picker::Resolution { close: true, .. } => ("Close", "close"),
        Picker::Transition { done: true, .. } => ("Move to Done", "move to done"),
        Picker::Transition { .. } | Picker::Resolution { .. } => ("Transition", "transition"),
        Picker::Assign { .. } => ("Assign", "assign"),
        Picker::Sprint { .. } => ("Move", "move"),
        Picker::Edit { .. } => ("Edit", "edit"),
    };
    format!("Read-only mode is on, so {name} is off.\nRestart without JIRA_TICKETS_READONLY to {verb}.")
}

/// The Edit dialog's priority is free text only when the issue's own priority is not one of Jira's five
/// defaults (the site has custom priorities); otherwise it is a choice.
pub fn custom_priority(orig: &str) -> bool {
    !orig.trim().is_empty() && Picker::priority_index(orig).is_none()
}

/// The Edit dialog's priority segment for "no priority". Choosing it writes nothing (`edit_fields` skips an empty
/// priority), so it only shows for an issue that has none: Jira's five defaults are never cleared by it.
pub const PRIORITY_UNSET: &str = "Not set";

/// The priority segments: Jira's five defaults, after PRIORITY_UNSET when the issue has no priority (`unset`).
pub fn priority_options(unset: bool) -> Vec<&'static str> {
    unset.then_some(PRIORITY_UNSET).into_iter().chain(Picker::PRIORITIES).collect()
}

/// The selected segment for `priority` among `priority_options(unset)`; "Not set" while it is empty.
pub fn priority_segment(priority: &str, unset: bool) -> usize {
    let shift = usize::from(unset);
    Picker::priority_index(priority).map_or(if unset { 0 } else { usize::MAX }, |i| i + shift)
}

/// The priority text a click on segment `i` stores ("" for "Not set").
pub fn priority_from_segment(i: usize, unset: bool) -> &'static str {
    match (unset, i) {
        (true, 0) => "",
        (true, i) => Picker::PRIORITIES[i - 1],
        (false, i) => Picker::PRIORITIES[i],
    }
}

/// A custom priority in read-only mode, where it shows as a disabled field: "Not set" when empty.
pub fn read_only_priority(priority: &str) -> &str {
    if priority.trim().is_empty() { PRIORITY_UNSET } else { priority }
}

/// The dialog's top edge: 20% down the window less DIALOG_LIFT, so at 900 (167) it covers the detail pane's pill row
/// under a two-line title (ink 175–192.5) with 8 of margin above it; 174 put its edge on the pills' first pixel row and
/// 188 cut across them (round 11 G1, round 12 G2: up, not down, since lower would show the pills' tops).
pub fn dialog_top(window_h: f32) -> f32 {
    ((window_h * 0.2).round() - DIALOG_LIFT).max(0.0)
}

/// The dialog's top edge with the detail pane's pill row at `pills_top` (its fill is its ink): PILLS_MARGIN above
/// it, so the dialog covers the pills and leaves the title readable (round 15 G1: 20% less 13 sat 3 over the
/// one-line-title pills); without a pill row on screen, `dialog_top`.
pub fn picker_top(window_h: f32, pills_top: Option<f32>) -> f32 {
    pills_top.map_or_else(|| dialog_top(window_h), |t| (t - PILLS_MARGIN).max(0.0))
}

/// The dialog's outer edge above the detail pane's pill row.
const PILLS_MARGIN: f32 = 8.0;

/// How far the dialog sits above the window's 20% line (round 13 G4: 10 left 5 over the pills; 13 leaves 8).
const DIALOG_LIFT: f32 = 13.0;

/// The dialog's margin: 16 at the sides and bottom; the top is the title's (`title`), set to its ink.
pub(crate) const DIALOG_MARGIN: Margin = Margin { left: 16, right: 16, top: 0, bottom: 16 };
/// Every modal's content width (pickers and go-to), inside DIALOG_MARGIN.
pub(crate) const DIALOG_W: f32 = 440.0;
/// The dialog edge's stroke: egui starts the frame's content inside it, and the title's ink is measured from it.
#[cfg(test)]
const DIALOG_EDGE: f32 = 1.0;
/// The dialog's inner edge → the title's ink (round 13 G1: 19.5 with a 16 margin to its line box).
const TITLE_INK_TOP: f32 = 16.0;
/// An Edit field → the next group's label ink (round 13 G3: 20.5 with a 12 space).
const GROUP_INK_GAP: f32 = 16.0;

/// The space before an Edit group's `label`: GROUP_INK_GAP from the field above (its fill is its ink) to the
/// label's ink, less the layout's item spacing.
fn group_space(ui: &egui::Ui, label: &str) -> f32 {
    let top = theme::ink_rect(&ui.painter().layout_no_wrap(label.to_string(), theme::body(12.0), egui::Color32::WHITE)).top();
    gap_space(GROUP_INK_GAP - top, ui.spacing().item_spacing.y)
}

/// The Assign filter field → the first row's ink ("Unassigned"; round 14 G3: 12 to the row's box read 21.5 to
/// its ink).
const ASSIGN_FIELD_GAP: f32 = 12.0;
/// A picker row's margin above and below its text.
const ROW_PAD_Y: i8 = 6;

/// The space after the Assign field for ASSIGN_FIELD_GAP to the first row's ink (the field's fill is its ink):
/// negative, so it backs up over the layout's item spacing.
fn assign_field_space(ui: &egui::Ui) -> f32 {
    let ink = theme::Ink::of(ui.ctx(), &theme::body(14.0), None);
    ASSIGN_FIELD_GAP - f32::from(ROW_PAD_Y) - ink.top - ui.spacing().item_spacing.y
}

/// The Edit title's ink → the first label's ink (round 14 G2: 26; the other pickers' title gaps are 24).
const EDIT_TITLE_INK_GAP: f32 = 24.0;
/// An Edit label's descenders → its field (round 14 G2: 4 to the line box read 10.5).
const LABEL_TO_FIELD_INK: f32 = 8.0;

/// The space after the Edit title (whose line box runs `below` past its ink) for EDIT_TITLE_INK_GAP to `label`'s
/// ink.
fn edit_title_space(ui: &egui::Ui, below: f32, label: &str) -> f32 {
    let top = theme::ink_rect(&ui.painter().layout_no_wrap(label.to_string(), theme::body(12.0), egui::Color32::WHITE)).top();
    gap_space(EDIT_TITLE_INK_GAP - below - top, ui.spacing().item_spacing.y)
}

/// The space to add for a `gap` between two widgets: the layout's item spacing already lands after the first
/// (the Assign gap measured 18 = 12 + 6; round 12 G1).
pub fn gap_space(gap: f32, item_spacing_y: f32) -> f32 {
    (gap - item_spacing_y).max(0.0)
}

/// One picker row: 14 px text plus 6 px above and below.
const ROW_H: f32 = 30.0;

/// Assign's title → its field (the field's fill is its ink).
const TITLE_GAP: f32 = 14.0;
/// Edit's last field → the buttons, both edges their ink (24).
const BUTTONS_GAP: f32 = 18.0;
/// The list pickers' title ink → the first row's text ink, and the last row's (or note's) text ink → the
/// buttons' edge (round 15 G2: the row boxes' slack read 33 and 35).
const LIST_INK_GAP: f32 = 24.0;

/// The dialog's title, TITLE_INK_TOP under the dialog's inner edge to its ink, then TITLE_GAP.
pub(crate) fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    title_line(ui, p, text);
    ui.add_space(TITLE_GAP);
}

/// The title line, then (read-only mode, or why an action is off) `note` under it in the small muted step, wrapped at
/// the dialog's full content width (every picker's and the Grooming dialogs'). Returns how far the last line box runs below its ink, like `title_line`.
pub(crate) fn title_with_note(ui: &mut egui::Ui, p: &Palette, text: &str, note: Option<&str>) -> f32 {
    let below = title_line(ui, p, text);
    let Some(note) = note else { return below };
    let sp = theme::spacing(ui.ctx());
    let job = egui::text::LayoutJob::simple(note.to_string(), theme::body(sp.small), crate::views::dialog_muted(p), ui.available_width());
    let g = ui.painter().layout_job(job);
    let ink = theme::ink_rect(&g);
    // NOTE_INK_GAP from the title's ink to the note's.
    ui.add_space(gap_space(NOTE_INK_GAP - below - ink.top(), ui.spacing().item_spacing.y));
    let after = g.size().y - ink.bottom();
    ui.add(egui::Label::new(g).selectable(false));
    after
}

/// A picker title's ink → its read-only note's ink.
const NOTE_INK_GAP: f32 = 8.0;

/// The dialog's title line, TITLE_INK_TOP under the dialog's inner edge to its ink; returns how far its line box
/// runs below its ink.
fn title_line(ui: &mut egui::Ui, p: &Palette, text: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::bold(15.0), p.text);
    let ink = theme::ink_rect(&g);
    // The frame's content already starts inside its 1 px edge.
    ui.add_space(TITLE_INK_TOP - ink.top());
    let below = g.size().y - ink.bottom();
    ui.add(egui::Label::new(g));
    below
}

/// A picker row's text inset (its frame margin): fields in pickers share it.
pub const ROW_INSET: f32 = 8.0;

/// The app's one single-line input (`views::input_field`), its text and placeholder at the rows' 8 px inset.
pub(crate) fn field<'a>(text: &'a mut String, hint: &str, p: &Palette) -> egui::TextEdit<'a> {
    crate::views::input_field_in(text, hint, theme::body(14.0), p, crate::views::input_margin(ROW_INSET))
}

/// `field`, or (`live` false: read-only mode) the same box view-only: muted text, no caret, no focus ring.
fn field_if<'a>(text: &'a mut String, hint: &str, live: bool, p: &Palette) -> egui::TextEdit<'a> {
    let f = field(text, hint, p);
    if live { f } else { f.interactive(false).text_color(crate::views::dialog_muted(p)) }
}

/// A small muted label, LABEL_TO_FIELD_INK from its descenders, then the field (full width).
fn edit_group(ui: &mut egui::Ui, p: &Palette, label: &str, text: &mut String, field: impl FnOnce(&mut String) -> egui::TextEdit<'_>) -> egui::Response {
    edit_label(ui, p, label);
    crate::views::add_field(ui, None, text, field)
}

/// An Edit group's small muted label, then the space for LABEL_TO_FIELD_INK to the control under it.
fn edit_label(ui: &mut egui::Ui, p: &Palette, label: &str) {
    ui.label(RichText::new(label).color(crate::views::dialog_muted(p)).size(12.0));
    let ink = theme::Ink::of(ui.ctx(), &theme::body(12.0), None);
    // The layout's item spacing comes on top of the space (it may be negative: a space backs the cursor up).
    ui.add_space(ink.pad_deep(LABEL_TO_FIELD_INK) - ui.spacing().item_spacing.y);
}

/// One picker row's text; `muted` for the synthetic "no value" row (Unassigned); `tag` a secondary note after it
/// ("future"); `current` marks the issue's present value in semibold with a muted "current" tag, wherever the
/// cursor is, so the value and the cursor (fill and ring) never share a look.
struct Row {
    text: String,
    muted: bool,
    current: bool,
    tag: Option<String>,
    /// A person row's avatar slot (Assign): `Some(Some(id))` draws theirs, `Some(None)` keeps the slot blank so
    /// "Unassigned" lines up with the names.
    avatar: Option<Option<String>>,
}

impl Row {
    fn plain(text: String) -> Self {
        Row { text, muted: false, current: false, tag: None, avatar: None }
    }
    fn muted(text: &str) -> Self {
        Row { muted: true, ..Row::plain(text.to_string()) }
    }
    fn current_if(mut self, current: bool) -> Self {
        self.current = current;
        self
    }
}

/// A row's secondary tags in order: its own (`tag`, "future"), then "current" for the issue's present value.
fn row_tags(tag: Option<&str>, current: bool) -> Vec<&str> {
    tag.into_iter().chain(current.then_some("current")).collect()
}

/// A row's text → its first tag, and between tags.
const ROW_TAG_GAP: f32 = 8.0;

/// Draws the rows with `cursor` highlighted. A click picks the row and confirms;
/// hover does not move the cursor, so a resting mouse can't fight the keys.
/// `fixed`: Assign's capped list (never scrolls; at most ASSIGN_USERS users), sized to the rows the filter leaves,
/// with its note ("2 matches", "9 of N · type to see more") right under them.
/// `title_below`: how far the title's line box runs below its ink, when the rows follow the title directly; the
/// space before them puts LIST_INK_GAP from the title's ink to the first row's text ink. Returns the space before
/// the buttons for LIST_INK_GAP from the last row's (or the note's) text ink to the buttons' edge.
#[allow(clippy::too_many_arguments)]
fn rows(
    ui: &mut egui::Ui,
    p: &Palette,
    names: Vec<Row>,
    live: bool,
    confirm: &mut bool,
    cursor: &mut usize,
    fixed: Option<Option<&str>>,
    title_below: Option<f32>,
) -> f32 {
    let item_y = ui.spacing().item_spacing.y;
    if let (Some(below), Some(first)) = (title_below, names.first()) {
        ui.add_space(LIST_INK_GAP - below - f32::from(ROW_PAD_Y) - row_ink(ui, &first.text).top() - item_y);
    }
    if names.is_empty() {
        ui.label(RichText::new("Nothing available").color(crate::views::dialog_muted(p)));
        return BUTTONS_GAP;
    }
    if let Some(note) = fixed {
        // Sized to its rows (the dialog grows and shrinks with the filter), the note right under them.
        ui.scope(|ui| row_list(ui, p, &names, live, confirm, cursor));
        if let Some(note) = note {
            Frame::new().inner_margin(Margin::symmetric(ROW_INSET as i8, ROW_PAD_Y)).show(ui, |ui| {
                ui.set_min_height(ROW_H - 12.0);
                ui.label(RichText::new(note).size(12.0).color(crate::views::dialog_muted(p)));
            });
            // The note's descenders are its ink.
            let note = theme::Ink::of(ui.ctx(), &theme::body(12.0), None);
            let under = f32::from(ROW_PAD_Y) + (ROW_H - 12.0).max(note.line) - (note.base + note.descent);
            return LIST_INK_GAP - under - item_y;
        }
        let last = names.last().map(|n| row_ink(ui, &n.text)).unwrap_or(egui::Rect::NOTHING);
        let line = theme::Ink::of(ui.ctx(), &theme::body(14.0), None).line;
        return LIST_INK_GAP - (f32::from(ROW_PAD_Y) + (ROW_H - 12.0).max(line) - last.bottom()) - item_y;
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| row_list(ui, p, &names, live, confirm, cursor));
    let last = names.last().map(|n| row_ink(ui, &n.text)).unwrap_or(egui::Rect::NOTHING);
    let line = theme::Ink::of(ui.ctx(), &theme::body(14.0), None).line;
    LIST_INK_GAP - (f32::from(ROW_PAD_Y) + (ROW_H - 12.0).max(line) - last.bottom()) - item_y
}

/// A picker row's text ink, from its line box's top-left.
fn row_ink(ui: &egui::Ui, text: &str) -> egui::Rect {
    theme::ink_rect(&ui.painter().layout_no_wrap(text.to_string(), theme::body(14.0), egui::Color32::WHITE))
}

/// A row name's colour: `text`, or `muted` for a synthetic row (Unassigned) and for every row of a view-only
/// (read-only) picker, so its rows read as off.
fn row_text_color(synthetic: bool, live: bool, text: egui::Color32, muted: egui::Color32) -> egui::Color32 {
    if synthetic || !live { muted } else { text }
}

/// A picker row's font: bold for the current value.
fn font_for(current: bool) -> egui::FontId {
    if current { theme::bold(14.0) } else { theme::body(14.0) }
}

/// A picker row's fill: the accent `dialog_cursor_fill` on the cursor row; read-only (`live` false), the neutral
/// `dialog_input_fill` step instead (the disabled fields' tone), so the cursor still shows yet reads as off.
fn cursor_fill(on_cursor: bool, live: bool, p: &Palette) -> egui::Color32 {
    match (on_cursor, live) {
        (true, true) => crate::views::dialog_cursor_fill(p),
        (true, false) => theme::dialog_input_fill(p),
        (false, _) => egui::Color32::TRANSPARENT,
    }
}

/// `live` false (read-only mode): the rows are view-only, in the disabled style: the quiet neutral cursor fill
/// (`cursor_fill`), every name muted, no clicks. The current value keeps its semibold and "current" tag.
fn row_list(ui: &mut egui::Ui, p: &Palette, names: &[Row], live: bool, confirm: &mut bool, cursor: &mut usize) {
    {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, n) in names.iter().enumerate() {
            let on_cursor = i == *cursor;
            let fill = cursor_fill(on_cursor, live, p);
            // The cursor is fill-only: an accent ring round a full-width row in a short list reads as a focused text
            // field. The clear 1 px edge stays so the rows keep their size.
            let edge = egui::Stroke::new(1.0, egui::Color32::TRANSPARENT);
            let r = Frame::new()
                .fill(fill)
                .stroke(edge)
                .corner_radius(crate::views::components::ROW_RADIUS)
                // The edge takes 1 px of the padding, so the text stays at ROW_INSET / ROW_PAD_Y.
                .inner_margin(Margin::symmetric(ROW_INSET as i8 - 1, ROW_PAD_Y - 1))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(ROW_H - 12.0);
                    // Secondary text lifts on the cursor fill where muted would fall under 5.0:1.
                    let muted = match (on_cursor, live) {
                        (true, true) => crate::views::muted_on_selected(p),
                        (true, false) => theme::muted_on(fill, p),
                        (false, _) => crate::views::dialog_muted(p),
                    };
                    let color = row_text_color(n.muted, live, p.text, muted);
                    let mut job = egui::text::LayoutJob::default();
                    let font = font_for(n.current);
                    job.append(&n.text, 0.0, egui::TextFormat { font_id: font, color, ..Default::default() });
                    for tag in row_tags(n.tag.as_deref(), n.current) {
                        job.append(tag, ROW_TAG_GAP, egui::TextFormat { font_id: theme::body(12.0), color: muted, ..Default::default() });
                    }
                    let label = egui::Label::new(job).truncate().selectable(false);
                    match &n.avatar {
                        Some(id) => {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = theme::AVATAR_GAP;
                                let line_h = ui.painter().layout_no_wrap(n.text.clone(), font_for(n.current), p.text).size().y;
                                match id {
                                    Some(id) => {
                                        crate::avatar::inline(ui, Some(id), &n.text, theme::AVATAR, line_h);
                                    }
                                    None => {
                                        ui.allocate_exact_size(egui::vec2(theme::AVATAR, line_h), egui::Sense::hover());
                                    }
                                }
                                ui.add(label);
                            });
                        }
                        None => {
                            ui.add(label);
                        }
                    }
                })
                .response
                .interact(if live { egui::Sense::click() } else { egui::Sense::hover() });
            if r.clicked() {
                *cursor = i;
                *confirm = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn read_only_says_what_is_off() {
        let close = crate::app::Picker::Resolution { key: "CI-1".into(), transition_id: "2".into(), items: vec![], cursor: 0, close: true };
        assert_eq!(super::read_only_note(&close), "Read-only mode is on, so Close is off.\nRestart without JIRA_TICKETS_READONLY to close.");
        let sprint = crate::app::Picker::Sprint { key: "CI-1".into(), items: vec![], cursor: 0 };
        assert!(super::read_only_note(&sprint).contains("so Move is off"));
    }

    #[test]
    fn read_only_pickers_are_view_only() {
        let (text, muted) = (egui::Color32::BLACK, egui::Color32::GRAY);
        assert_eq!(super::row_text_color(false, true, text, muted), text);
        assert_eq!(super::row_text_color(true, true, text, muted), muted, "Unassigned is muted");
        assert_eq!(super::row_text_color(false, false, text, muted), muted, "read-only: every row reads as off");
        assert_eq!(super::read_only_priority(" "), "Not set");
        assert_eq!(super::read_only_priority("High"), "High");
        // A view-only list keeps a quiet neutral cursor row, never the live accent one.
        for p in [crate::theme::LIGHT, crate::theme::DARK] {
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
                ui.set_width(440.0);
                let names = vec![super::Row::plain("Done".into()), super::Row::plain("Won't Do".into())];
                super::rows(ui, &p, names, false, &mut false, &mut 0, None, None);
            });
            let fills = crate::testutil::fills(&shapes);
            let cursor = crate::views::dialog_cursor_fill(&p);
            assert!(fills.iter().all(|(_, c)| *c != cursor), "no live cursor row");
            let quiet = crate::theme::dialog_input_fill(&p);
            assert_eq!(fills.iter().filter(|(_, c)| *c == quiet).count(), 1, "one quiet cursor row, dark={}", p.dark);
            assert!(crate::theme::contrast(quiet, crate::views::dialog_fill(&p)) > 1.05, "it shows on the dialog, dark={}", p.dark);
            let muted = crate::theme::muted_on(quiet, &p);
            assert!(crate::theme::contrast(muted, quiet) >= 4.5, "its text reads, dark={}", p.dark);
        }
    }

    #[test]
    fn row_tags_share_one_style_and_order() {
        assert_eq!(super::row_tags(Some("future"), true), ["future", "current"]);
        assert_eq!(super::row_tags(None, true), ["current"]);
        assert!(super::row_tags(None, false).is_empty());
    }

    #[test]
    fn only_a_custom_priority_is_typed() {
        assert!(!super::custom_priority("High"));
        assert!(!super::custom_priority(""), "no priority yet: choose one");
        assert!(super::custom_priority("P1 - Critical"));
    }

    #[test]
    fn an_unset_priority_is_a_selected_segment() {
        use super::{priority_from_segment, priority_options, priority_segment};
        assert_eq!(priority_options(true), ["Not set", "Highest", "High", "Medium", "Low", "Lowest"]);
        assert_eq!(priority_options(false).len(), 5, "an issue with a priority cannot pick Not set");
        assert_eq!(priority_segment("", true), 0, "Not set is selected, inside the control");
        assert_eq!(priority_segment("High", true), 2);
        assert_eq!(priority_segment("High", false), 1);
        for unset in [true, false] {
            for (i, o) in priority_options(unset).iter().enumerate() {
                let stored = priority_from_segment(i, unset);
                assert_eq!(priority_segment(stored, unset), i, "{o} round-trips");
            }
        }
        assert_eq!(priority_from_segment(0, true), "", "Not set writes nothing");
    }

    #[test]
    fn picker_fields_share_the_row_inset() {
        assert_eq!(crate::views::input_margin(super::ROW_INSET).left, 8);
        assert_eq!(super::ROW_INSET, crate::views::INPUT_PAD_X, "one input padding everywhere");
    }

    #[test]
    fn dialog_sits_a_fifth_down() {
        assert_eq!(super::dialog_top(900.0), 167.0, "8 over the two-line-title pills' first ink row (175)");
        assert!(super::dialog_top(900.0) + 8.0 <= 175.0, "the dialog covers the pill row with margin");
        assert_eq!(super::dialog_top(0.0), 0.0);
        assert_eq!(super::ASSIGN_FIELD_GAP, 12.0);
    }

    #[test]
    fn list_pickers_sit_24_to_the_text_ink() {
        // Round 15 G2: title ink → first row text ink 24, last row (or Assign's note) text ink → buttons 24, wherever
        // the cursor is. Real fonts, 2x.
        for p in [crate::theme::LIGHT, crate::theme::DARK] {
            for cursor in [0, 2] {
                let mut gaps = (0.0, 0.0);
                let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 900.0), |ui| {
                    egui::Frame::new().fill(crate::views::dialog_fill(&p)).inner_margin(super::DIALOG_MARGIN).show(ui, |ui| {
                        ui.set_width(440.0);
                        let below = super::title_line(ui, &p, "Transition CI-3779");
                        let names = vec![super::Row::plain("New".into()), super::Row::plain("Backlog".into()), super::Row::plain("Closed".into())];
                        let gap = super::rows(ui, &p, names, true, &mut false, &mut cursor.clone(), None, Some(below));
                        ui.add_space(gap);
                        let b = ui.add(crate::views::primary_button("Confirm", &p)).rect;
                        gaps.0 = b.top();
                        // A full list: the cut note sits at the block's foot.
                        let mut names = vec![super::Row::muted("Unassigned")];
                        names.extend((0..crate::app::ASSIGN_USERS).map(|i| super::Row::plain(format!("User {i}"))));
                        let gap = super::rows(ui, &p, names, true, &mut false, &mut cursor.clone(), Some(Some("9 of 40 · type to see more")), None);
                        ui.add_space(gap);
                        gaps.1 = ui.add(crate::views::button("Cancel", &p)).rect.top();
                    });
                });
                let title = crate::testutil::ink_of(&shapes, "Transition CI-3779");
                let first = crate::testutil::ink_of(&shapes, "New");
                let last = crate::testutil::ink_of(&shapes, "Closed");
                let note = crate::testutil::ink_of(&shapes, "9 of 40 · type to see more");
                assert!((first.top() - title.bottom() - 24.0).abs() <= 0.5, "title → first row {}", first.top() - title.bottom());
                assert!((gaps.0 - last.bottom() - 24.0).abs() <= 0.5, "last row → buttons {}", gaps.0 - last.bottom());
                assert!((gaps.1 - note.bottom() - 24.0).abs() <= 0.5, "note → buttons {}", gaps.1 - note.bottom());
            }
        }
    }

    #[test]
    fn dialog_sits_8_over_the_pill_row() {
        assert_eq!(super::picker_top(900.0, Some(170.0)), 162.0, "8 over the pills' fill");
        assert_eq!(super::picker_top(900.0, None), super::dialog_top(900.0), "no pill row: a fifth down");
    }

    #[test]
    fn titles_and_edit_groups_sit_16_to_the_ink() {
        // Round 13 G1 / G3: 16 from the dialog's inner edge to the title's ink, 16 from an Edit field to the next
        // label's ink, with the real fonts at the captures' density.
        let p = crate::theme::LIGHT;
        let mut labels = String::from("a");
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
            egui::Frame::new().fill(crate::views::dialog_fill(&p)).stroke(crate::views::dialog_stroke(&p)).inner_margin(super::DIALOG_MARGIN).show(ui, |ui| {
                ui.set_width(440.0);
                crate::views::dialog_fields(ui, &p);
                super::title(ui, &p, "Edit CI-3779");
                super::edit_group(ui, &p, "Labels, comma separated", &mut labels, |t| super::field(t, "", &p));
                ui.add_space(super::group_space(ui, "Priority"));
                super::edit_group(ui, &p, "Priority", &mut String::new(), |t| super::field(t, "Hint", &p));
            });
        });
        let fills = crate::testutil::fills(&shapes);
        let dialog = fills.iter().find(|(_, c)| *c == crate::views::dialog_fill(&p)).expect("dialog").0;
        let title = crate::testutil::ink_of(&shapes, "Edit CI-3779");
        assert!((title.top() - (dialog.top() + super::DIALOG_EDGE) - 16.0).abs() <= 0.5, "title {}", title.top() - dialog.top());
        let field = fills.iter().find(|(_, c)| *c == crate::theme::dialog_input_fill(&p)).expect("field").0;
        let gap = crate::testutil::ink_of(&shapes, "Priority").top() - field.bottom();
        assert!((gap - 16.0).abs() <= 0.5, "field → label {gap}");
    }

    #[test]
    fn the_assign_field_sits_12_over_its_list() {
        // Round 14 G3: 12 from the field's fill to "Unassigned"'s ink. Real fonts, 2x.
        assert_eq!(super::gap_space(4.0, 6.0), 0.0, "never negative");
        for p in [crate::theme::LIGHT, crate::theme::DARK] {
            let mut filter = String::new();
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 900.0), |ui| {
                ui.set_width(440.0);
                crate::views::dialog_fields(ui, &p);
                crate::views::add_field(ui, None, &mut filter, |t| super::field(t, "Type a name", &p));
                ui.add_space(super::assign_field_space(ui));
                let names = vec![super::Row::muted("Unassigned"), super::Row::plain("Ada Lovelace".into())];
                super::rows(ui, &p, names, true, &mut false, &mut 1, Some(None), None);
            });
            let field = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == crate::theme::dialog_input_fill(&p)).expect("field").0;
            let gap = crate::testutil::ink_of(&shapes, "Unassigned").top() - field.bottom();
            assert!((gap - 12.0).abs() <= 0.25, "field → Unassigned {gap}");
        }
    }

    #[test]
    fn a_read_only_priority_shows_its_segments_disabled() {
        // Round 5: view-only, Priority was a plain text box ("Not set"), unlike the list choice it is.
        let p = crate::theme::LIGHT;
        let mut priority = String::new();
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            ui.set_width(440.0);
            super::priority_group(ui, &p, &mut priority, "", false);
        });
        for name in ["Not set", "Highest", "Medium", "Lowest"] {
            crate::testutil::ink_of(&shapes, name);
        }
        assert!(priority.is_empty(), "nothing chosen");
    }

    #[test]
    fn the_edit_picker_sits_24_and_8_to_the_ink() {
        // Round 14 G2: title ink → first label ink 24; a label's descenders → its field 8.
        let p = crate::theme::LIGHT;
        let mut labels = String::from("a");
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 600.0), |ui| {
            egui::Frame::new().fill(crate::views::dialog_fill(&p)).inner_margin(super::DIALOG_MARGIN).show(ui, |ui| {
                ui.set_width(440.0);
                crate::views::dialog_fields(ui, &p);
                let below = super::title_line(ui, &p, "Edit CI-3779");
                ui.add_space(super::edit_title_space(ui, below, "Labels, comma separated"));
                super::edit_group(ui, &p, "Labels, comma separated", &mut labels, |t| super::field(t, "", &p));
            });
        });
        let title = crate::testutil::ink_of(&shapes, "Edit CI-3779");
        let label = crate::testutil::ink_of(&shapes, "Labels, comma separated");
        assert!((label.top() - title.bottom() - 24.0).abs() <= 0.25, "title → label {}", label.top() - title.bottom());
        let field = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == crate::theme::dialog_input_fill(&p)).expect("field").0;
        assert!((field.top() - label.bottom() - 8.0).abs() <= 0.5, "label → field {}", field.top() - label.bottom());
    }
}
