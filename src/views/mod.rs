//! Header with the screen switcher, sign-in, toast, shared widgets.

pub mod adf_view;
pub mod components;
pub mod detail;
pub mod grooming;
pub mod grooming_guide;
pub mod help;
pub mod icons;
pub mod issues;
pub mod me;
pub mod palette;
pub mod pickers;
pub mod roadmap;
pub mod standup;
pub mod team;

use crate::app::{Action, App, Screen};
use crate::theme::{self, Palette};
use egui::{Color32, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};

pub fn main(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let (bar, active_tab) = header(app, ui, p);
    // Every signed-in screen keeps the hint bar.
    issues::hints_panel(app, ui, p);
    if app.offline {
        offline_banner(app, ui, p);
    }
    match app.screen {
        Screen::Issues => issues::show(app, ui),
        Screen::Grooming => grooming::show(app, ui),
        Screen::Team => team::show(app, ui),
        Screen::Roadmap => roadmap::show(app, ui),
        Screen::Me => me::show(app, ui),
        Screen::Standup => standup::show(app, ui),
    }
    // One 1 px rule, the header panel's own last point under its 48 px bar (egui's separator line is off, so
    // nothing stacks on it), then the active tab's 2 px accent bar over the rule's y-range. Painted after the
    // panels below; modals are other layers.
    let rule = header_rule(bar);
    ui.painter().rect_filled(rule, 0.0, p.border);
    if let Some(x) = active_tab {
        ui.painter().rect_filled(underline_rect(x, rule), 0.0, p.accent);
    }
}

/// The header: the 48 px bar plus its 1 px rule.
pub const HEADER_H: f32 = 49.0;
/// The bar above the rule.
const BAR_H: f32 = HEADER_H - 1.0;

/// The one empty-state line ("Select an issue", "Nothing in Now") with no hint: `components::empty_state`.
pub fn empty_state(ui: &mut Ui, p: &Palette, text: &str) {
    components::empty_state(ui, p, text, None);
}

/// The empty-state text size in Compact (it is the density's `body`).
pub const EMPTY_STATE_SIZE: f32 = 14.0;

/// The widths a split's first pane may be dragged to in `available`: no cap of its own, only the room that
/// leaves the pane beside it `other_min`. Never under `min`, so a window narrower than both mins keeps the range valid.
pub fn split_range(available: f32, min: f32, other_min: f32) -> std::ops::RangeInclusive<f32> {
    min..=(available - other_min).max(min)
}

/// The first pane's width this frame: the saved (dragged) width, else `default`, held to `split_range` and to
/// `available`, so a width saved in a wider window shrinks to fit this one.
pub fn split_width(saved: Option<f32>, default: f32, available: f32, min: f32, other_min: f32) -> f32 {
    let r = split_range(available, min, other_min);
    saved.unwrap_or(default).clamp(*r.start(), *r.end()).min(available.max(0.0))
}

/// A drag handle over the `gap` between two panes, the first starting at `left`. A drag saves the first pane's
/// new width under `id` (persisted with the window state); `saved_split` reads it back for `split_width`.
pub fn splitter(ui: &mut Ui, id: egui::Id, left: f32, gap: egui::Rect) {
    let resp = ui.interact(gap, id.with("handle"), Sense::drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let w = pos.x - gap.width() / 2.0 - left;
        ui.data_mut(|d| d.insert_persisted(id, w));
    }
}

/// A splitter's saved width, if the user has dragged it.
pub fn saved_split(ui: &Ui, id: egui::Id) -> Option<f32> {
    ui.data_mut(|d| d.get_persisted::<f32>(id))
}

/// The top space that centres `content_h` on the window's middle (`window_mid`) for an area starting at
/// `avail_top` and `avail_h` tall: the window, not the area, so the hint bar under it does not lift the card
/// (round 3: it sat half the bar high). Kept inside the area and on whole points.
pub fn centre_top_on(window_mid: f32, avail_top: f32, avail_h: f32, content_h: f32) -> f32 {
    (window_mid - content_h / 2.0 - avail_top).clamp(0.0, (avail_h - content_h).max(0.0)).round()
}

/// The one centring rule (sign-in, finding the token): `content` centred horizontally in the space left and
/// vertically on the window (`centre_top_on`), using its height from the previous frame.
fn centred(ui: &mut Ui, id: &str, content: impl FnOnce(&mut Ui)) {
    let id = egui::Id::new(("centred", id));
    let last_h: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    ui.vertical_centered(|ui| {
        let mid = ui.ctx().content_rect().center().y;
        ui.add_space(centre_top_on(mid, ui.cursor().top(), ui.available_height(), last_h));
        let top = ui.cursor().top();
        content(ui);
        let h = ui.min_rect().bottom() - top;
        if (h - last_h).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|d| d.insert_temp(id, h));
    });
}

/// The tabs, in their ⌘-digit order (`tab_key`).
const TABS: [(Screen, &str); 6] = [
    (Screen::Issues, "Issues"),
    (Screen::Grooming, "Grooming"),
    (Screen::Team, "Team"),
    (Screen::Roadmap, "Roadmap"),
    (Screen::Me, "Me"),
    (Screen::Standup, "Standup"),
];

/// The digit that, with ⌘, switches to `s` (`App::shortcuts`).
pub fn tab_key(s: Screen) -> u8 {
    match s {
        Screen::Issues => 1,
        Screen::Grooming => 2,
        Screen::Team => 3,
        Screen::Roadmap => 4,
        Screen::Me => 5,
        Screen::Standup => 6,
    }
}

/// Each tab's origin x for a title whose galley starts at `left`: TITLE_GAP from the title's ink to the first
/// tab's ink, TAB_GAP between the tabs' ink (round 13 A1 / A2: gaps between the boxes read 33.5 and 24.5–25.5).
/// Every tab label is regular 14 (the underline marks the active one), so its slot is its label's width whatever
/// its state and switching tabs never moves anything.
pub fn tab_origins(ctx: &egui::Context, left: f32, title: &egui::Galley, tabs: &[std::sync::Arc<egui::Galley>]) -> Vec<f32> {
    // On whole pixels, so each galley paints exactly the raster its ink was measured on. Between tabs, the
    // rasters' visible coverage (round 15 A1: "Team"'s faint last column read Team → Roadmap 25); the title → first
    // tab gap keeps the raster boxes it was ruled on (32), so the title and the first tabs never move.
    let ppp = title.pixels_per_point.max(1.0);
    let raster = |g: &egui::Galley| {
        let r = theme::ink_rect(g);
        if r.is_positive() { (r.left(), r.right()) } else { (0.0, g.size().x) }
    };
    let visible = |g: &egui::Galley| {
        let r = theme::visible_ink(ctx, g);
        if r.is_positive() { (r.left(), r.right()) } else { (0.0, g.size().x) }
    };
    let (tl, tr) = raster(title);
    let mut items = vec![(tl, tr, 0.0)];
    for (i, g) in tabs.iter().enumerate() {
        let (l, r) = if i == 0 { (raster(g).0, visible(g).1) } else { visible(g) };
        items.push((l, r, if i == 0 { TITLE_GAP } else { TAB_GAP }));
    }
    let (xs, _) = issues::ink_origins(&items);
    // `ink_origins` puts the first ink at 0; the title's galley stays at `left`.
    xs.iter().skip(1).map(|x| issues::snap_px(left + tl + x, ppp)).collect()
}

/// The header rule: the header panel's last whole point, under the 48 px bar.
pub fn header_rule(header: egui::Rect) -> egui::Rect {
    let bottom = header.bottom().round();
    egui::Rect::from_x_y_ranges(header.x_range(), bottom - 1.0..=bottom)
}

/// The active tab's underline span for a label galley at `origin`: its ink, not its advance (round 13 J3: the bar
/// overhung "Team"'s ink by 2 on the right), on whole pixels.
pub fn tab_underline_x(origin: f32, label: &egui::Galley) -> egui::Rangef {
    let ink = theme::ink_rect(label);
    let ppp = label.pixels_per_point.max(1.0);
    let (l, r) = if ink.is_positive() { (ink.left(), ink.right()) } else { (0.0, label.size().x) };
    egui::Rangef::new(issues::snap_px(origin + l, ppp), issues::snap_px(origin + r, ppp))
}

/// The active tab's 2 px accent bar across the tab's width, its bottom on the rule's bottom, so it covers the
/// rule there and shows whole (1 px above it, 1 px on it).
pub fn underline_rect(tab_x: egui::Rangef, rule: egui::Rect) -> egui::Rect {
    egui::Rect::from_x_y_ranges(tab_x, rule.bottom() - 2.0..=rule.bottom())
}

/// Gap between tabs.
const TAB_GAP: f32 = 24.0;
/// Gap from the title to the first tab: 32, a step past TAB_GAP, so the title never reads as a fifth tab
/// (round-10 ruling, final).
const TITLE_GAP: f32 = 32.0;
/// Between the header's labels (the name, "·", "Read-only"): separate labels at the 8 item spacing (round 11 A1;
/// one 20 gap with a centred dot read ~10 a side).
const LABEL_GAP: f32 = 8.0;
/// The labels → the sync button: 12, a step wider than the 8 between the buttons, so the text reads as its own group.
const LABELS_TO_BUTTONS: f32 = 12.0;
/// The longest the signed-in name may grow leftward, in "m" glyphs of the 12 px name font; longer names are cut.
const NAME_MAX_M: f32 = 20.0;
/// The header's icon buttons (BUTTON_H square, like every other control) and the gap between the right cluster's parts.
const HEADER_BUTTON: f32 = icons::BUTTON;
const HEADER_GAP: f32 = 8.0;

/// The header's "Read-only" label colour: `text` (the name beside it stays `muted`).
pub fn read_only_color(p: &Palette) -> Color32 {
    p.text
}

/// The header's "Read-only" galley, laid out in `read_only_color` (the painter's colour argument only tints
/// placeholder-coloured glyphs, so the layout colour is the ink; round 12 A1).
pub fn read_only_galley(painter: &egui::Painter, p: &Palette) -> std::sync::Arc<egui::Galley> {
    painter.layout_no_wrap("Read-only".into(), theme::body(12.0), read_only_color(p))
}

/// The name's width cap: a run of "m" glyphs. It only cuts very long names; it never reserves space.
pub fn name_cap(m_w: f32) -> f32 {
    (m_w * NAME_MAX_M).ceil()
}

/// The header's right cluster, laid out from the bar's right edge (inside the 16 px margin), right to left:
/// "?" outermost, sync 8 left of it, then the labels 12 left of sync ("Read-only" nearest, then "·" and the name,
/// LABEL_GAP apart, the name growing leftward). The buttons' x depends only on `right` (round-8 A1 ruling); the
/// labels follow the buttons.
#[derive(Debug, PartialEq)]
pub struct HeaderRight {
    pub help: f32,
    pub sync: f32,
    /// The labels' right edge.
    pub labels_right: f32,
    /// The labels' total width.
    pub labels_w: f32,
    pub read_only: Option<f32>,
    /// The "·" label's left edge, between the name and "Read-only" when both show.
    pub dot: Option<f32>,
    pub name: Option<f32>,
}

/// `dot_w` is the "·" label's width; `gaps` the spacing between the boxes of the name and "·", and of "·" and
/// "Read-only" (LABEL_GAP to the ink, `theme::ink_gap`).
pub fn header_right(right: f32, name_w: Option<f32>, read_only_w: Option<f32>, dot_w: f32, gaps: [f32; 2]) -> HeaderRight {
    let help = right - HEADER_BUTTON;
    let sync = help - HEADER_GAP - HEADER_BUTTON;
    let labels_right = sync - LABELS_TO_BUTTONS;
    let read_only = read_only_w.map(|w| labels_right - w);
    let dot = read_only.filter(|_| name_w.is_some()).map(|x| x - gaps[1] - dot_w);
    let name_right = dot.map_or(labels_right, |x| x - gaps[0]);
    let name = name_w.map(|w| name_right - w);
    let labels_w = labels_right - name.or(read_only).unwrap_or(labels_right);
    HeaderRight { help, sync, labels_right, labels_w, read_only, dot, name }
}

/// The y of a galley's first baseline, from its top.
pub fn galley_baseline(g: &egui::Galley) -> f32 {
    baseline(g)
}

fn baseline(g: &egui::Galley) -> f32 {
    g.rows.first().map_or(0.0, |r| r.pos.y + r.row.glyphs.first().map_or(0.0, |gl| gl.pos.y))
}

/// The top at which to paint a galley so its baseline lands on `line_baseline`.
pub fn top_for_baseline(line_baseline: f32, galley_baseline: f32) -> f32 {
    line_baseline - galley_baseline
}

/// The header spinner is for syncing the refresh lists; a search shows its own spinner in the count row.
pub fn header_syncing<'a>(lists: impl IntoIterator<Item = (&'a crate::store::ListId, &'a crate::app::Loaded)>) -> bool {
    lists.into_iter().any(|(id, l)| *id != crate::store::ListId::Search && l.loading)
}

/// One spinner per operation: (header, count row). A first load (nothing on screen yet, whatever started it)
/// is the count row's: "⟳ Loading issues…", and the header stays still. Otherwise a header refresh (⌘R, and
/// the sync after a write) spins the header only, including the quick-filter search it reruns; a search
/// started from the filters or the search box spins the count row only.
pub fn spinner_slots(lists_syncing: bool, search_pending: bool, search_from_refresh: bool, first_load: bool) -> (bool, bool) {
    if first_load {
        return (false, true);
    }
    (lists_syncing || (search_pending && search_from_refresh), search_pending && !search_from_refresh)
}

/// The header bar; returns the header's rect (bar and rule) and the active tab's x-range.
fn header(app: &mut App, ui: &mut Ui, p: &Palette) -> (egui::Rect, Option<egui::Rangef>) {
    let mut active_tab = None;
    // The rule is the panel's last point (bottom margin 1), so the bar keeps its 48 and the buttons sit 8/8.
    let frame = Frame::new().fill(p.surface).inner_margin(Margin { left: 16, right: 16, top: 0, bottom: (HEADER_H - BAR_H) as i8 });
    let panel = egui::Panel::top("header").exact_size(HEADER_H).show_separator_line(false).frame(frame).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            // Every gap is placed by ink (`tab_origins`), so none comes from item spacing.
            ui.spacing_mut().item_spacing.x = 0.0;
            // The title and the tabs share the tabs' baseline (the 15 bold title sat 1 px below it).
            let tab_probe = ui.painter().layout_no_wrap("Issues".into(), theme::body(14.0), p.text);
            let title = ui.painter().layout_no_wrap("Jira Tickets".into(), theme::bold(15.0), p.text);
            let tabs: Vec<_> = TABS
                .iter()
                .map(|(s, name)| ui.painter().layout_no_wrap(name.to_string(), theme::body(14.0), if app.screen == *s { p.text } else { p.muted }))
                .collect();
            let origins = tab_origins(ui.ctx(), ui.cursor().left(), &title, &tabs);
            let (title_rect, _) = ui.allocate_exact_size(vec2(title.size().x.ceil(), 32.0), Sense::hover());
            let line = (title_rect.center().y - tab_probe.size().y / 2.0).round() + baseline(&tab_probe);
            ui.painter().galley(egui::pos2(title_rect.left(), top_for_baseline(line, baseline(&title))), title, p.text);
            for (i, ((s, name), galley)) in TABS.into_iter().zip(tabs).enumerate() {
                let active = app.screen == s;
                ui.add_space(origins[i] - ui.cursor().left());
                let (rect, r) = ui.allocate_exact_size(vec2(galley.size().x, 32.0), Sense::click());
                let pos = egui::pos2(rect.left(), top_for_baseline(line, baseline(&galley)));
                if active {
                    // Painted by `main` after the rule (see `underline_rect`), across the label's ink.
                    active_tab = Some(tab_underline_x(rect.left(), &galley));
                }
                ui.painter().galley(pos, galley, p.text);
                let r = tooltip(r, format!("{name} (⌘{})", tab_key(s)));
                if r.clicked() {
                    app.actions.push(Action::SetScreen(s));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = HEADER_GAP;
                // Right to left (`header_right`): "?" at the 16 px edge, sync, then the labels, which grow leftward,
                // so the buttons never depend on the name. The name and "Read-only", both small 12 muted so they
                // never compete with the active tab, sit on the tabs' baseline, NAME_GAP apart.
                let right = ui.max_rect().right();
                let read_only = crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed);
                let painter = ui.painter().clone();
                let m_w = painter.layout_no_wrap("m".into(), theme::body(12.0), p.muted).size().x;
                let cap = name_cap(m_w);
                let name = app.me.as_ref().map(|me| {
                    let shown = truncate_text(&me.display_name, cap, |t| painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.muted).size().x);
                    (painter.layout_no_wrap(shown, theme::body(12.0), p.muted), me.display_name.clone())
                });
                let ro = read_only.then(|| read_only_galley(&painter, p));
                let dot = painter.layout_no_wrap("·".into(), theme::body(12.0), p.muted);
                // LABEL_GAP between the labels' ink, not their boxes (round 13 A3: 8 to the boxes read 9).
                let gaps = [
                    name.as_ref().map_or(LABEL_GAP, |(g, _)| theme::ink_gap(g, &dot, LABEL_GAP)),
                    ro.as_ref().map_or(LABEL_GAP, |g| theme::ink_gap(&dot, g, LABEL_GAP)),
                ];
                // The name carries the user's avatar before it (AVATAR_GAP to the text).
                let lead = theme::AVATAR + theme::AVATAR_GAP;
                let at = header_right(right, name.as_ref().map(|(g, _)| lead + g.size().x), ro.as_ref().map(|g| g.size().x), dot.size().x, gaps);
                let help = icons::icon_button(ui, icons::Icon::Help, "Keyboard help (?)", p);
                if help.clicked() {
                    app.actions.push(Action::ToggleHelp);
                }
                // One fixed slot: the spinner replaces the button at the same size, only while a list loads.
                let first_load = app.screen == Screen::Issues && issues::first_load(app);
                if spinner_slots(header_syncing(&app.lists), app.search_pending.is_some(), app.search_from_refresh, first_load).0 {
                    icons::spinner_button(ui, p);
                } else if icons::icon_button(ui, icons::Icon::Sync, "Refresh (⌘R)", p).clicked() {
                    app.actions.push(Action::Refresh);
                }
                if at.labels_w > 0.0 {
                    // The labels' width plus the 4 their gap to sync adds over the buttons' 8 item spacing.
                    ui.allocate_exact_size(vec2(at.labels_w + LABELS_TO_BUTTONS - HEADER_GAP, 32.0), Sense::hover());
                }
                if let (Some((g, full)), Some(x)) = (name, at.name) {
                    let r = egui::Rect::from_min_size(egui::pos2(x + lead, top_for_baseline(line, baseline(&g))), g.size());
                    let slot = egui::Rect::from_min_size(egui::pos2(x, r.top()), vec2(theme::AVATAR, r.height()));
                    let id = app.me.as_ref().map(|m| m.account_id.clone());
                    ui.scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| crate::avatar::inline(ui, id.as_deref(), &full, theme::AVATAR, r.height()));
                    ui.painter().galley(r.min, g, p.muted);
                    let resp = ui.interact(r, egui::Id::new("header-name"), Sense::click());
                    tooltip(resp, format!("{full} · right-click to sign out")).context_menu(|ui| {
                        if ui.button("Sign out").clicked() {
                            app.actions.push(Action::SignOut);
                        }
                    });
                }
                if let Some(x) = at.dot {
                    // A muted "·" label between the name and "Read-only", so they read as two labels.
                    ui.painter().galley(egui::pos2(x, top_for_baseline(line, baseline(&dot))), dot, p.muted);
                }
                if let (Some(g), Some(x)) = (ro, at.read_only) {
                    // "Read-only" in `text`, the name muted: a hierarchy without a new colour (A1).
                    let r = egui::Rect::from_min_size(egui::pos2(x, top_for_baseline(line, baseline(&g))), g.size());
                    ui.painter().galley(r.min, g, read_only_color(p));
                    tooltip(ui.interact(r, egui::Id::new("header-read-only"), Sense::hover()), "Writes are blocked (JIRA_TICKETS_READONLY)");
                }
            });
        });
    });
    (panel.response.rect, active_tab)
}

fn offline_banner(app: &mut App, ui: &mut Ui, p: &Palette) {
    let when = app
        .store
        .fetched_at
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or("earlier".into());
    Frame::new().fill(p.danger_subtle).inner_margin(Margin::symmetric(16, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Offline. Showing data from {when}.")).color(p.off_track).size(13.0));
            if ui.add(button("Retry", p)).clicked() {
                app.actions.push(Action::Refresh);
            }
        });
    });
}

/// The sign-in panel's first line: the token error when there is one (it replaces the prompt), else the prompt.
pub fn sign_in_lead(token_error: Option<&str>) -> (&str, bool) {
    match token_error {
        Some(e) => (e, true),
        None => ("Paste an Atlassian API token to connect.", false),
    }
}

/// The sign-in aside: prose in muted 12 and the identifiers in mono 12.5, on one line.
fn sign_in_aside(p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let prose = egui::TextFormat { font_id: theme::body(12.0), color: p.muted, ..Default::default() };
    let code = egui::TextFormat { font_id: theme::mono(12.5), color: p.muted, ..Default::default() };
    for (i, part) in SIGN_IN_ASIDE.iter().enumerate() {
        job.append(part, 0.0, if i % 2 == 1 { code.clone() } else { prose.clone() });
    }
    job
}

/// Alternating prose and identifiers.
/// Final wording (round 11 J1): two verbs, so neither identifier reads as the other's alternative name.
pub const SIGN_IN_ASIDE: [&str; 7] = ["Or export ", "JIRA_API_TOKEN", ", or set ", "api_token", " in ", "config.toml", ", then check again."];

/// The sign-in panel's padding: its inner edge → the title's ink, and "Check again"'s box → its inner edge.
const SIGN_IN_PAD: f32 = 16.0;
/// The panel's margin: 16 at the sides; top and bottom are set to the ink (`sign_in_title`, `check_again`).
const SIGN_IN_MARGIN: Margin = Margin { left: 16, right: 16, top: 0, bottom: 0 };
pub const SIGN_IN_TITLE: &str = "Sign in to Jira";
pub const CHECK_AGAIN: &str = "Check again";

/// The title, SIGN_IN_PAD under the panel's inner edge to its ink (the frame's content starts inside its edge).
fn sign_in_title(ui: &mut Ui, p: &Palette) {
    let g = ui.painter().layout_no_wrap(SIGN_IN_TITLE.to_string(), theme::bold(22.0), p.text);
    ui.add_space(SIGN_IN_PAD - theme::ink_rect(&g).top());
    ui.add(egui::Label::new(g));
}

/// "Check again": the shared secondary `button`, full width and BUTTON_H under "Sign in", so it reads as a
/// control, quieter than the primary above it. SIGN_IN_PAD from its box to the panel's inner edge. Returns the
/// response and the space from its box's bottom to the panel's outer edge.
fn check_again(ui: &mut Ui, p: &Palette, w: f32) -> (egui::Response, f32) {
    let r = ui.add_sized([w, BUTTON_H], button(CHECK_AGAIN, p));
    ui.add_space(SIGN_IN_PAD - ui.spacing().item_spacing.y);
    (r, SIGN_IN_PAD + SIGN_IN_STROKE)
}

/// The sign-in panel's edge.
const SIGN_IN_STROKE: f32 = 1.0;

/// "Check again"'s tooltip: its identifiers in mono like the aside's (round 15 J3).
pub const CHECK_AGAIN_TIP: [&str; 6] = ["Look again for ", "JIRA_API_TOKEN", " or ", "api_token", " in ", "config.toml"];

/// The tooltip's clearance below the sign-in panel (round 15 J3: it overlapped the panel's edge).
const TIP_PANEL_CLEAR: f32 = 4.0;

/// "Sign in" → "Check again", box to box: the 8 between stacked controls.
const SIGN_IN_TO_AGAIN: f32 = 8.0;

/// Body → aside in code: the 14 px line's leading under its ink and the aside's over its ink add ~6, so 2 reads
/// as the 8 that makes the aside a footnote of the line above (round 12 J1 measured 14 at 8).
/// Round 14 J1: the aside 0.5 lower (it read 7.5 above, 15.5 below).
const BODY_TO_ASIDE: f32 = 2.5;

/// The sign-in aside → token field, in code (see the comment at its use).
/// Round 15 J1: +1 (it read 15).
const ASIDE_TO_FIELD: f32 = 15.5;

/// The sign-in panel's content width: 480, or the aside's one-line width plus 24 of margin when wider, so
/// the aside never fits with only a pixel to spare.
pub fn sign_in_width(aside_w: f32) -> f32 {
    (aside_w.ceil() + 24.0).max(480.0)
}

/// Title → body in code: the 22 px title's line box carries ~5.5 of leading under its ink, so 11 reads as 16.
const TITLE_TO_BODY: f32 = 11.0;

pub fn sign_in(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    if app.finding_token {
        centred(ui, "finding-token", |ui| {
            icons::spinner(ui, icons::INLINE_SPINNER, p);
            ui.add_space(8.0);
            ui.label(RichText::new("Looking for your Jira token…").color(p.muted));
        });
        return;
    }
    // The form's one key, in the same bar every signed-in screen has.
    issues::sign_in_hints(ui, p);
    let aside = sign_in_aside(p);
    let aside_w = ui.fonts_mut(|f| f.layout_job(aside.clone())).size().x;
    centred(ui, "sign-in", |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(SIGN_IN_MARGIN).show(ui, |ui| {
            let w = sign_in_width(aside_w);
            ui.set_width(w);
            ui.vertical(|ui| {
                // Every gap is explicit, ink to ink: 16 title → body, 8 body → aside, 16 aside → field, 12 → button, 12 → Check again.
                // The aside's mono runs make its line box ~1 px taller than its prose, so 15 reads as 16.
                ui.spacing_mut().item_spacing.y = 0.0;
                sign_in_title(ui, p);
                ui.add_space(TITLE_TO_BODY);
                let (lead, is_error) = sign_in_lead(app.token_error.as_deref());
                ui.label(RichText::new(lead).size(14.0).color(if is_error { p.off_track } else { p.text }));
                // A login means Jira rejected it (401): show what was sent.
                if let Some(login) = &app.login {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "Site: {}\nEmail: {}\nToken: {} ({})\nCheck that the email matches your Atlassian account and the token is valid (https://id.atlassian.com/manage-profile/security/api-tokens).",
                            app.cfg.site,
                            login.email,
                            crate::auth::mask_token(&login.token),
                            login.source.describe()
                        ))
                        .color(p.muted)
                        .size(12.0),
                    );
                    ui.add_space(8.0);
                    if tooltip(ui.add(button("Sign out", p)), "Forget the saved token").clicked() {
                        app.actions.push(Action::SignOut);
                    }
                }
                // After the Sign out button (no text leading above the aside's) the plain 8.
                ui.add_space(if app.login.is_some() { 8.0 } else { BODY_TO_ASIDE });
                ui.add(egui::Label::new(aside).extend());
                ui.add_space(ASIDE_TO_FIELD);
                // The page's one field fill (`Palette::field`), as the search box and the filter combos.
                ui.visuals_mut().text_edit_bg_color = Some(p.field());
                let r = add_field(ui, None, &mut app.token_input, |t| input_field(t, "API token", theme::body(14.0), p).id(egui::Id::new("token-input")).password(true));
                if ui.memory(|m| m.focused().is_none()) {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(12.0);
                // Full width and the field's height: the two stack as one unit.
                if ui.add_sized([w, INPUT_H], primary_button("Sign in", p)).clicked() {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(SIGN_IN_TO_AGAIN);
                let (again, below) = check_again(ui, p, w);
                if tooltip_job(again, check_again_tip(p), below + TIP_PANEL_CLEAR).clicked() {
                    app.actions.push(Action::FindToken);
                }
            });
        });
    });
}

/// A one-line tooltip's height at most (12 px text, TIP_MARGIN and the 1 px edge), for deciding where it fits:
/// smaller than the 28 controls it labels.
pub const TIP_H: f32 = 24.0;
/// The tooltip's gap from its control (egui's 4 read as attached; round 11 F1).
pub const TIP_GAP: f32 = 8.0;
/// The tooltip's text size: the hint bar's.
pub const TIP_TEXT: f32 = 12.0;
/// Inside the tooltip's edge: 3 above and below; the sides are TIP_INK_X to the ink (`tip_row`).
pub const TIP_MARGIN: Margin = Margin { left: 0, right: 0, top: 3, bottom: 3 };
/// The tooltip's side padding, from its edge to the text's ink, the same on both sides (round 13 F2: 8 to the
/// line box read 9–11.5 left and 8 right with "⌘"'s bearing).
pub const TIP_INK_X: f32 = 8.0;

/// A tooltip's text split around its key token, which takes the hint bar's key style (round 13 F1): the whole
/// text when it is a key ("⌘Enter"), else a trailing "(…)" that holds a key ("Copy link (c)", "Refresh (⌘R)").
/// Returns (before, key, after); the key is empty when the text has none.
pub fn tip_key_split(text: &str) -> (&str, &str, &str) {
    let modified = |t: &str| t.starts_with(['⌘', '⇧', '⌥', '⌃']);
    let is_key = |t: &str| !t.is_empty() && !t.contains(' ') && (modified(t) || t.chars().count() <= 3);
    if modified(text) && !text.contains(' ') {
        return ("", text, "");
    }
    if let Some(inner) = text.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
        && is_key(&inner[open + 1..])
    {
        return (&text[..open + 1], &inner[open + 1..], ")");
    }
    (text, "", "")
}

/// A tooltip's text: TIP_TEXT `text`, its key token in the key style (`tip_key_split`).
pub fn tip_job(text: &str, p: &Palette) -> egui::text::LayoutJob {
    let (before, key, after) = tip_key_split(text);
    let plain = egui::TextFormat { font_id: theme::body(TIP_TEXT), color: p.text, ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    job.append(before, 0.0, plain.clone());
    issues::append_job(&mut job, issues::key_job(key, p.text));
    job.append(after, 0.0, plain);
    job
}

/// How far a tip that is only a key ("⌘Enter") sits above the line box's place: its mono ink rode 1 pt low,
/// 9 above and 8 below to the frame's outer edge (round 14 F2); 0.5 evens it.
pub const TIP_KEY_RAISE: f32 = 0.5;

/// A tooltip's one line, TIP_INK_X from the edge to the ink on both sides; a key-only tip TIP_KEY_RAISE higher.
fn tip_row(ui: &mut Ui, text: &str, p: &Palette) {
    let g = ui.painter().layout_job(tip_job(text, p));
    let raise = if tip_key_split(text).0.is_empty() { TIP_KEY_RAISE } else { 0.0 };
    tip_row_job(ui, g, raise, p);
}

/// A tooltip's laid-out line `g`, TIP_INK_X from the edge to the ink on both sides, `raise` above its place.
fn tip_row_job(ui: &mut Ui, g: std::sync::Arc<egui::Galley>, raise: f32, p: &Palette) {
    let ink = theme::ink_rect(&g);
    let (lb, rb) = if ink.is_positive() { (ink.left(), g.size().x - ink.right()) } else { (0.0, 0.0) };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(TIP_INK_X - lb);
        let (rect, _) = ui.allocate_exact_size(g.size(), egui::Sense::hover());
        ui.painter().galley(rect.min - vec2(0.0, raise), g, p.text);
        ui.add_space(TIP_INK_X - rb);
    });
}

/// The app's one tooltip frame (F1): the hint bar's fill, a 1 px `border` (no control-weight edge, no shadow),
/// so a tip never reads as a sibling button.
pub fn tip_frame(p: &Palette) -> Frame {
    Frame::new().fill(issues::hint_bar_fill(p)).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(TIP_MARGIN)
}

/// `on_hover_text` in the app's tooltip style (`tip_frame`, TIP_TEXT `text`, TIP_GAP off the control).
pub fn tooltip(resp: egui::Response, text: impl Into<String>) -> egui::Response {
    show_tip(&resp, text.into(), None);
    resp
}

/// A tooltip of `job` (its own runs), `gap` below its control, in the app's tooltip style.
pub fn tooltip_job(resp: egui::Response, job: egui::text::LayoutJob, gap: f32) -> egui::Response {
    let p = theme::palette(&resp.ctx);
    let mut tip = egui::Tooltip::for_enabled(&resp);
    tip.popup = tip.popup.frame(tip_frame(p)).gap(gap).align(egui::RectAlign::BOTTOM_START).align_alternatives(&[egui::RectAlign::TOP_START]);
    tip.show(|ui| tip_row_job(ui, ui.painter().layout_job(job), 0.0, p));
    resp
}

/// "Check again"'s tooltip text: prose at TIP_TEXT, identifiers in mono at the same size.
pub fn check_again_tip(p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let prose = egui::TextFormat { font_id: theme::body(TIP_TEXT), color: p.text, ..Default::default() };
    let code = egui::TextFormat { font_id: theme::mono(TIP_TEXT), color: p.text, ..Default::default() };
    for (i, part) in CHECK_AGAIN_TIP.iter().enumerate() {
        job.append(part, 0.0, if i % 2 == 1 { code.clone() } else { prose.clone() });
    }
    job
}

fn show_tip(resp: &egui::Response, text: String, align: Option<egui::RectAlign>) {
    let p = theme::palette(&resp.ctx);
    let mut tip = egui::Tooltip::for_enabled(resp);
    tip.popup = tip.popup.frame(tip_frame(p)).gap(TIP_GAP);
    if let Some(align) = align {
        tip.popup = tip.popup.align(align).align_alternatives(&[]);
    }
    tip.show(|ui| tip_row(ui, &text, p));
}

/// Whether a tooltip goes above its control: when, below it, it would cross `limit` (the hint bar's top rule,
/// or the window's bottom), as the Comment tooltip did (F2, H4).
pub fn tip_above(anchor_bottom: f32, limit: f32) -> bool {
    anchor_bottom + TIP_GAP + TIP_H > limit
}

/// Where a tooltip goes, when not egui's default (below, start-aligned): beside the control on its left, its
/// right edge at the control and growing leftward, for a control right under a field (the Comment button under
/// the focused composer: above it covered the box, F2); else above when there is no room below (`tip_above`).
pub fn tip_align(above: bool, under_a_field: bool) -> Option<egui::RectAlign> {
    if under_a_field {
        Some(egui::RectAlign::LEFT)
    } else if above {
        Some(egui::RectAlign::TOP_START)
    } else {
        None
    }
}

/// `tooltip`, but above the control when there is no room for it below, over the hint bar.
pub fn hover_tip(resp: egui::Response, text: impl Into<String>, app: &App) -> egui::Response {
    hover_tip_placed(resp, text, app, false)
}

/// `hover_tip` for a control right under a field: the tip sits to its left (`tip_align`).
pub fn hover_tip_beside(resp: egui::Response, text: impl Into<String>, app: &App) -> egui::Response {
    hover_tip_placed(resp, text, app, true)
}

fn hover_tip_placed(resp: egui::Response, text: impl Into<String>, app: &App, under_a_field: bool) -> egui::Response {
    let limit = resp.ctx.content_rect().bottom() - hint_bar_height(app).unwrap_or(0.0);
    show_tip(&resp, text.into(), tip_align(tip_above(resp.rect.bottom(), limit), under_a_field));
    resp
}

/// The hint bar's height when it shows (signed in, on any screen, and on the sign-in form), else None.
pub fn hint_bar_height(app: &App) -> Option<f32> {
    hint_bar_shown(app.client.is_some(), app.finding_token).then_some(issues::HINT_BAR_H)
}

/// The hint bar shows signed in and on the sign-in form; not while the token is still being looked for.
pub fn hint_bar_shown(signed_in: bool, finding_token: bool) -> bool {
    signed_in || !finding_token
}

/// The toast's edge: `Palette::toast_edge` (dark only) on top of its shadow. Painted over the frame, inside it,
/// so the toast is the same size in both themes.
pub fn toast_stroke(p: &Palette) -> Stroke {
    p.toast_edge().map_or(Stroke::NONE, |c| Stroke::new(1.0, c))
}

/// The toast's gap above the hint bar (or the window's bottom edge when there is none): 16 on the spacing scale.
pub const TOAST_GAP: f32 = 16.0;

/// How far the toast's bottom sits above the window bottom: TOAST_GAP above the hint bar when one shows, else
/// TOAST_GAP plus 8 off the bare window edge.
pub fn toast_offset(hint_bar: Option<f32>) -> f32 {
    hint_bar.map_or(TOAST_GAP + 8.0, |h| h + TOAST_GAP)
}

/// The toast's dot marks its type, one rule for every kind: green for done, red for failures, neutral grey for a
/// notice (read-only refusals, "No changes"). The toast is a dark fill in both themes, so the dot takes the dark
/// palette's tokens.
/// The failure dot: off_track lifted a step, so it holds 3:1 on the dark toast fill (#575753).
const TOAST_ERROR_DOT: Color32 = Color32::from_rgb(0xf4, 0x8a, 0x83);

pub fn toast_dot(kind: crate::app::ToastKind) -> Color32 {
    match kind {
        crate::app::ToastKind::Success => theme::DARK.on_track,
        crate::app::ToastKind::Info => theme::DARK.muted,
        crate::app::ToastKind::Error => TOAST_ERROR_DOT,
    }
}

/// The toast's status mark: a dot only where it says something (green done, red failure); a plain notice
/// (`Info`: read-only refusals, "Theme: Dark", "Copied") has none, so the dot never decorates.
pub fn toast_mark(kind: crate::app::ToastKind) -> Option<Color32> {
    match kind {
        crate::app::ToastKind::Info => None,
        k => Some(toast_dot(k)),
    }
}

/// The toast dot's diameter.
const TOAST_DOT: f32 = 9.0;

/// The dot's centre below the text's line-box top: midway between the cap line and the baseline, so it sits on
/// the text's visual middle rather than the line box's (which descender room pushes low).
pub fn toast_dot_centre_y(ink: &theme::Ink) -> f32 {
    (ink.top + ink.base) / 2.0
}

/// The toast's centre x: the window's, on every screen and whatever the panes under it (it is window-wide
/// feedback, not the detail pane's). The toast is pinned by its own bottom centre, so it is exact.
pub fn toast_centre_x(window: egui::Rect) -> f32 {
    window.center().x
}

/// The toast's frame margin: none; the line's spaces above and below are TOAST_INK_Y to the ink (`toast_body`) and
/// the sides TOAST_INK_X to the ink (`toast_row`).
const TOAST_MARGIN: Margin = Margin::ZERO;
/// The toast's vertical padding, from the fill to the text's ink: the tallest glyphs ("Hdlk") above, the
/// descenders ("gpqy") below, so every toast is one height (round 14 D1: 6 / 10 to the line box read 10.5 / 11).
pub const TOAST_INK_Y: f32 = 12.0;
/// The toast's side padding, from the fill to the dot or the text's ink (round 13 D2: 16 to the text's box read
/// 17–18.3 to its ink).
pub const TOAST_INK_X: f32 = 16.0;

/// From the dot to the text's ink.
const TOAST_DOT_GAP: f32 = 8.0;

/// The toast's contents: TOAST_INK_Y to the 14 pt line's ink above and below, the row between.
fn toast_body(ui: &mut Ui, p: &Palette, dot: Option<Color32>, text: &str) {
    let ink = theme::Ink::of(ui.ctx(), &theme::body(14.0), None);
    ui.spacing_mut().item_spacing.y = 0.0;
    // The row is the line's own height, not centred in the interact height.
    ui.spacing_mut().interact_size.y = 0.0;
    ui.add_space(ink.pad_top(TOAST_INK_Y));
    toast_row(ui, p, dot, text);
    ui.add_space(ink.pad_deep(TOAST_INK_Y));
}

/// The widest a toast gets, fill edge to fill edge; longer text wraps (a one-line read-only notice ran ~665 wide
/// across both panes).
pub const TOAST_MAX_W: f32 = 480.0;

/// The width the toast's text wraps at: TOAST_MAX_W less the side padding and, with a dot, its slot and gap.
pub fn toast_wrap_w(dot: bool) -> f32 {
    TOAST_MAX_W - 2.0 * TOAST_INK_X - if dot { TOAST_DOT + TOAST_DOT_GAP } else { 0.0 }
}

/// The toast's row: the type dot (if any, on the first line's middle), 8, the text wrapped at `toast_wrap_w`, with
/// TOAST_INK_X from the fill to the ink either side.
fn toast_row(ui: &mut Ui, p: &Palette, dot: Option<Color32>, text: &str) {
    let g = ui.painter().layout(text.to_string(), theme::body(14.0), p.tooltip_text, toast_wrap_w(dot.is_some()));
    // The rasters' visible coverage: a faint outer column read the right inset 15.5 (round 15 D2).
    let ink = theme::visible_ink(ui.ctx(), &g);
    let (lb, rb) = if ink.is_positive() { (ink.left(), g.size().x - ink.right()) } else { (0.0, 0.0) };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TOAST_DOT_GAP;
        // The dot's circle fills its slot's width, so its edge is its ink. The slot is the line box's height, top
        // to top with the text, and the dot sits on the text's middle (`toast_dot_centre_y`).
        if let Some(dot) = dot {
            ui.add_space(TOAST_INK_X);
            let line = theme::Ink::of(ui.ctx(), &theme::body(14.0), None);
            let (slot, _) = ui.allocate_exact_size(vec2(TOAST_DOT, g.size().y), Sense::hover());
            ui.painter().circle_filled(egui::pos2(slot.center().x, slot.top() + toast_dot_centre_y(&line)), TOAST_DOT / 2.0, dot);
            // The text's left bearing is part of the 8 to its ink.
            ui.add_space(-lb);
        } else {
            // No dot: TOAST_INK_X from the fill to the text's ink.
            ui.add_space(TOAST_INK_X - lb);
        }
        ui.add(egui::Label::new(g).wrap_mode(egui::TextWrapMode::Extend));
        // The layout's item spacing already follows the text.
        ui.add_space(TOAST_INK_X - rb - ui.spacing().item_spacing.x);
    });
}

/// The toast's shadow: heavier in dark, where a light shadow vanishes on the near-black page.
pub fn toast_shadow(p: &Palette) -> egui::Shadow {
    if p.dark {
        egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(200) }
    } else {
        egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(60) }
    }
}

pub fn toast(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    if std::time::Instant::now() > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = theme::palette(ctx);
    let shadow = toast_shadow(p);
    let window = ctx.content_rect();
    let dot = toast_mark(t.kind);
    let x = toast_centre_x(window);
    // The pivot on the device-pixel grid, so the fill's bottom lands exactly 16 over the bar (round 15 D1).
    let ppp = ctx.pixels_per_point();
    let at = egui::pos2(issues::snap_px(x, ppp), issues::snap_px(window.bottom() - toast_offset(hint_bar_height(app)), ppp));
    egui::Area::new(egui::Id::new("toast")).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(at).show(ctx, |ui| {
        let r = Frame::new().fill(p.tooltip).shadow(shadow).corner_radius(theme::RADIUS).inner_margin(TOAST_MARGIN).show(ui, |ui| {
            toast_body(ui, p, dot, &t.text);
        });
        ui.painter().rect_stroke(r.response.rect, f32::from(theme::RADIUS), toast_stroke(p), egui::StrokeKind::Inside);
    });
}

// ---------- Shared widgets ----------

/// Height of every text button (and the filter combos), so rows of controls line up.
pub const BUTTON_H: f32 = 28.0;

/// The page dimming behind a modal (`Palette::scrim_alpha`): light needs much less to read as "behind"; dark
/// needs enough that the dialog clearly sits on top.
pub fn backdrop_alpha(p: &Palette) -> u8 {
    p.scrim_alpha()
}

/// A picker's cursor row: the one selection fill (`selection`), fill-only (`pickers::row_list`): a ring round a
/// full-width dialog row reads as a focused field.
pub fn dialog_cursor_fill(p: &Palette) -> Color32 {
    p.selection()
}

/// A modal dialog's one edge colour, used for its 1 px stroke and for the fields and buttons inside it, in both
/// themes: a step darker than `border` in light (`border` sits between white and the dimmed page and
/// vanishes), the lifted #504f4b in dark (1.54:1 on the #333331 dialog).
pub fn dialog_stroke(p: &Palette) -> Stroke {
    Stroke::new(1.0, if p.dark { DARK_EDGE } else { Color32::from_rgb(0xb4, 0xb3, 0xac) })
}

pub use crate::theme::DARK_EDGE;

/// Inside a dialog: fields in the page's one field fill (`theme::dialog_input_fill`, `Palette::field`) with the
/// dialog's edge (focus stays the accent).
pub fn dialog_fields(ui: &mut Ui, p: &Palette) {
    let edge = dialog_stroke(p).color;
    let v = ui.visuals_mut();
    v.text_edit_bg_color = Some(theme::dialog_input_fill(p));
    v.widgets.inactive.bg_stroke.color = edge;
    v.widgets.hovered.bg_stroke.color = edge;
}

/// Placeholder text for a field, in the `placeholder` token, `lead` right of the text origin.
pub fn field_hint_at(text: &str, font: egui::FontId, p: &Palette, lead: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, lead, egui::TextFormat { font_id: font, color: theme::placeholder(p), ..Default::default() });
    job
}

/// How far left of the text origin an empty field's caret sits (round 11 I1, every field; it replaces round 10's
/// placeholder 2 right of the caret in the focused-on-open fields).
pub const CARET_LEAD: i8 = 2;

/// A field's margin for its content: an empty field's text origin moves CARET_LEAD left (so egui draws, and
/// blinks, its caret there) while its placeholder is led back CARET_LEAD right (`empty_field_hint`), so the
/// placeholder and typed text share one origin; the first keystroke restores the margin.
pub fn caret_margin(m: Margin, empty: bool) -> Margin {
    if empty { Margin { left: m.left - CARET_LEAD, ..m } } else { m }
}

/// How far past the last glyph's advance a trailing caret's left edge sits, in device pixels (round 13 I3: egui
/// centres the caret's stroke on the advance, so it touched round glyphs).
pub const CARET_TRAIL_PX: f32 = 1.0;

/// The centre x of a trailing caret of `stroke_w` after an advance ending at `end`: its left edge CARET_TRAIL_PX
/// past the advance, rounded up to the device pixel grid (round 14 I3: the stroke landed between pixels).
pub fn caret_trail_x(end: f32, stroke_w: f32, ppp: f32) -> f32 {
    let left = ((end * ppp).ceil() + CARET_TRAIL_PX) / ppp;
    left + stroke_w / 2.0
}

/// Adds a field (`edit`, made over `text`) and, when its caret sits after the last glyph of non-empty text, puts
/// the painted caret at `caret_trail_x` past that glyph's advance, unclipped. Every field goes through here (search, go-to, sign-in, pickers,
/// composer); an empty field's caret keeps the CARET_LEAD rule.
pub fn add_field(ui: &mut Ui, size: Option<egui::Vec2>, text: &mut String, edit: impl FnOnce(&mut String) -> egui::TextEdit<'_>) -> egui::Response {
    let from = ui.painter().add(egui::Shape::Noop);
    let r = match size {
        Some(s) => ui.add_sized(s, edit(text)),
        None => ui.add(edit(text)),
    };
    let n = text.chars().count();
    let at_end = n > 0
        && r.has_focus()
        && egui::TextEdit::load_state(ui.ctx(), r.id).and_then(|s| s.cursor.char_range()).is_some_and(|c| c.is_empty() && c.primary.index.0 == n);
    if at_end {
        let caret = ui.visuals().text_cursor.stroke;
        let ppp = ui.ctx().pixels_per_point();
        let layer = ui.layer_id();
        ui.ctx().graphics_mut(|g| {
            let list = g.entry(layer);
            let range = from.0..list.next_idx().0;
            // The text's last advance, from the field's painted galley.
            let mut end = None;
            for c in list.all_entries().skip(from.0).take(range.len()) {
                if let egui::Shape::Text(t) = &c.shape
                    && let Some(row) = t.galley.rows.last()
                    && let Some(gl) = row.row.glyphs.last()
                {
                    // The tessellator puts the galley on a whole pixel.
                    end = Some((t.pos.x * ppp).round() / ppp + row.pos.x + gl.max_x());
                }
            }
            let Some(end) = end else { return };
            let x = caret_trail_x(end, caret.width, ppp);
            for i in range {
                list.mutate_shape(egui::layers::ShapeIdx(i), |c| {
                    if let egui::Shape::LineSegment { points, stroke } = &mut c.shape
                        && *stroke == caret
                    {
                        points[0].x = x;
                        points[1].x = x;
                        // egui clips the field's painter 1 past the text, which cut the moved caret to one device
                        // pixel (round 14 I1); let the whole stroke show.
                        c.clip_rect.max.x = c.clip_rect.max.x.max(x + caret.width);
                    }
                });
            }
        });
    }
    r
}

/// The placeholder for a field laid out with `caret_margin`: shown only while empty, so always led CARET_LEAD.
pub fn empty_field_hint(text: &str, font: egui::FontId, p: &Palette) -> egui::text::LayoutJob {
    field_hint_at(text, font, p, f32::from(CARET_LEAD))
}

/// The one text-field inset, from the field's edge to its typed text and placeholder, in every single-line field
/// (search, go-to, sign-in, pickers, the standup title): 8, aligned with the dialogs' rows. Round 3: the search
/// box's 12 against the dialogs' 8 read as two field styles.
pub const TEXT_FIELD_PAD_X: f32 = 8.0;
/// Every dialog field's inset: `TEXT_FIELD_PAD_X`.
pub const INPUT_PAD_X: f32 = TEXT_FIELD_PAD_X;
/// The list pane's search box inset: `TEXT_FIELD_PAD_X`, like every other field.
pub const SEARCH_PAD_X: f32 = TEXT_FIELD_PAD_X;

/// Height of every single-line input (search, go-to, token, picker fields): BUTTON_H, like every control.
pub const INPUT_H: f32 = BUTTON_H;

/// The one single-line input: 8 px side margin (typed text and placeholder both start 8 in), INPUT_H tall, full width, `font` for text and placeholder. Focus draws the theme's one
/// 1 px accent stroke.
pub fn input_field<'a>(text: &'a mut String, hint: &str, font: egui::FontId, p: &Palette) -> egui::TextEdit<'a> {
    input_field_in(text, hint, font, p, INPUT_MARGIN)
}

/// `input_field` with its own margin (the search box's, a picker's row inset), the empty-field caret rule applied.
pub fn input_field_in<'a>(text: &'a mut String, hint: &str, font: egui::FontId, p: &Palette, margin: Margin) -> egui::TextEdit<'a> {
    let empty = text.is_empty();
    egui::TextEdit::singleline(text)
        .font(font.clone())
        .hint_text(empty_field_hint(hint, font, p))
        .desired_width(f32::INFINITY)
        .margin(caret_margin(margin, empty))
        .min_size(vec2(0.0, INPUT_H))
        // The text row (and caret, and placeholder) centred in the box, not riding at the top margin.
        .vertical_align(egui::Align::Center)
}

/// An input's margin: INPUT_PAD_X either side, the shared vertical margin above and below.
const INPUT_MARGIN: Margin = input_margin(INPUT_PAD_X);

/// Every single-line field's margin above and below its text: an even 4, so the galley is centred in the box like
/// the combos' and chips' text (round 11 B2 fixed the search box; round 12 I1 gives go-to, sign-in and the pickers
/// the same one vertical offset; their 2 / 5 read 1–1.5 high).
pub const INPUT_PAD_Y: i8 = 4;

/// An input's margin for a text inset of `pad_x` (12 on the page, the picker rows' 8 in picker dialogs).
pub const fn input_margin(pad_x: f32) -> Margin {
    Margin { left: pad_x as i8, right: pad_x as i8, top: INPUT_PAD_Y, bottom: INPUT_PAD_Y }
}

/// The list pane's search box margin: SEARCH_PAD_X either side, the shared vertical margin less a point above, so
/// the centred text sits half a point (one device pixel) higher, like the combos' and chips' (round 15 B1); the box
/// keeps INPUT_H.
pub const fn search_margin() -> Margin {
    let m = input_margin(SEARCH_PAD_X);
    Margin { top: m.top - 1, ..m }
}

/// A modal dialog's fill: plain surface in light; in dark #383836, two steps past surface_2, so the dialog lifts
/// off the dimmed page about as far as the light one does (the backdrop stays at 110).
pub fn dialog_fill(p: &Palette) -> Color32 {
    if p.dark { theme::DARK_DIALOG } else { p.surface }
}

/// The dimmed area: the window, less the hint bar at the bottom (it carries the dialog's keys, so it stays lit).
pub fn backdrop_rect(window: egui::Rect, hint_bar: Option<f32>) -> egui::Rect {
    let mut r = window;
    r.max.y -= hint_bar.unwrap_or(0.0);
    r
}

/// Paints the modal backdrop over the window above the hint bar and swallows its clicks.
pub fn backdrop(ctx: &egui::Context, id: &str, order: egui::Order, hint_bar: Option<f32>, p: &Palette) -> egui::Response {
    egui::Area::new(egui::Id::new(id))
        .order(order)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            let r = backdrop_rect(ui.ctx().content_rect(), hint_bar);
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(backdrop_alpha(p)));
            ui.allocate_rect(r, Sense::click())
        })
        .inner
}

pub fn button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.text).size(14.0))
        .fill(theme::button_fill(p))
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::RADIUS)
        .min_size(vec2(0.0, BUTTON_H))
}

/// A filter chip's inner padding on each side.
pub const CHIP_PAD: f32 = 12.0;

/// A chip is its label plus 12 either side in both states (the accent fill marks it on), so toggling never
/// moves its neighbours.
pub fn chip_width(label_w: f32) -> f32 {
    (2.0 * CHIP_PAD + label_w).ceil()
}

/// `rect` moved onto whole points, keeping its size (at least BUTTON_H tall), so a chip on a half-point row
/// draws 28 px edges, not 27 and a blurred half (B3).
pub fn whole_points(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(rect.min.round(), vec2(rect.width(), rect.height().max(BUTTON_H)))
}

/// A config name in sentence case for a chip: words after the first are lowercased unless they are
/// acronyms (two or more capitals, e.g. "PLAT", "JQL"). "Only Opened" → "Only opened", "To Do" → "To do".
pub fn sentence_case(name: &str) -> String {
    name.split(' ')
        .enumerate()
        .map(|(i, w)| if i == 0 || w.chars().filter(|c| c.is_uppercase()).count() > 1 { w.to_string() } else { w.to_lowercase() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A filter toggle chip: `components::toggle_chip` (kept here for its callers' argument order).
pub fn toggle_chip(ui: &mut Ui, text: &str, active: bool, p: &Palette) -> egui::Response {
    components::toggle_chip(ui, p, text, active)
}

pub fn primary_button(text: &str, p: &Palette) -> egui::Button<'static> {
    primary_button_if(text, true, p)
}

/// The enabled primary button's fill and text: accent with `on_accent`, in both themes.
pub fn primary_colors(p: &Palette) -> (Color32, Color32) {
    (p.accent, p.on_accent)
}

/// A text link (no frame) in `color`. The one link rule, as in descriptions (`adf_view::underline_y`): accent text
/// with a 1 pt underline LINK_UNDERLINE_DROP under the baseline, at rest and hovered, so a link never looks like
/// muted metadata; hover adds the pointing hand.
pub fn text_link(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    link_in(ui, galley, color, 0.0)
}

/// A link's slot: the galley's size less `trim` on the right.
fn link_in(ui: &mut Ui, galley: std::sync::Arc<egui::Galley>, color: Color32, trim: f32) -> egui::Response {
    let (rect, slot) = ui.allocate_exact_size(galley.size() - vec2(trim, 0.0), Sense::hover());
    // The click area grows round the text to MIN_TARGET_H; the layout keeps the text's own box.
    let resp = ui.interact(link_hit(rect), slot.id, Sense::click());
    if ui.is_rect_visible(rect) {
        let ink = theme::ink_rect(&galley);
        let x = if ink.is_positive() { egui::Rangef::new(rect.left() + ink.left(), (rect.left() + ink.right()).min(rect.right())) } else { rect.x_range() };
        let y = adf_view::underline_y(rect.top() + galley_baseline(&galley));
        ui.painter().hline(x, y, Stroke::new(1.0, color));
        ui.painter().galley(rect.min, galley, color);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The least height of a click area (WCAG's 24 px target).
pub const MIN_TARGET_H: f32 = 24.0;

/// A link's click area: its text `slot` grown evenly above and below to MIN_TARGET_H (never shrunk).
pub fn link_hit(slot: Rect) -> Rect {
    slot.expand2(vec2(0.0, ((MIN_TARGET_H - slot.height()) / 2.0).max(0.0)))
}

/// A primary button; disabled, `components::primary_button_off` (no fill, faded accent edge, hover only), so a
/// disabled Confirm is quieter than the Cancel beside it in both themes. Add it with `ui.add`, not
/// `ui.add_enabled(false, ..)`: egui would fade it a second time.
pub fn primary_button_if(text: &str, enabled: bool, p: &Palette) -> egui::Button<'static> {
    if !enabled {
        return components::primary_button_off(text, p);
    }
    let (fill, fg) = primary_colors(p);
    egui::Button::new(RichText::new(text.to_string()).color(fg).size(14.0))
        .fill(fill)
        .stroke(Stroke::new(1.0, fill))
        .corner_radius(theme::RADIUS)
        .min_size(vec2(0.0, BUTTON_H))
}

/// Pill inner padding (8/2) and radius (10).
pub const PILL_MARGIN: egui::Vec2 = vec2(8.0, 2.0);

pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    pill_sized(ui, text, bg, fg, Stroke::NONE, PILL_MARGIN)
}

/// A neutral pill (type, priority, labels, team, assignee): outline only, 1 px `border`, `muted` text, so
/// only the status (and epic) pills carry a filled tint.
pub fn outline_pill(ui: &mut Ui, text: &str, p: &Palette, margin: egui::Vec2) -> egui::Response {
    pill_sized(ui, text, Color32::TRANSPARENT, p.muted, Stroke::new(1.0, p.border), margin)
}

/// An outline pill on a selected row: the `selected_pill_edge` outline so it never sinks into the fill; its text
/// (assignee, labels) stays secondary (`selected_pill_text`): only the row's key switches to `text`.
pub fn outline_pill_selected(ui: &mut Ui, text: &str, p: &Palette, margin: egui::Vec2) -> egui::Response {
    pill_sized(ui, text, Color32::TRANSPARENT, selected_pill_text(p), Stroke::new(1.0, selected_pill_edge(p)), margin)
}

/// A neutral pill's outline on a selected row: light #77766f (≥ 3.3:1 on the #dfdfda selection; the
/// old rule tone read 2.88), dark #888782: 3.03:1 on the #3d3d3a selection and 1.70 off the pill text
/// (round 15 L1: #868580 read 2.95; the suggested #8f8e89 reached 3.32 but fell to 1.55 off the text, and the
/// text's 5.13 on the selection leaves no tone with 3.0 and 1.7 both above about #888782).
pub fn selected_pill_edge(p: &Palette) -> Color32 {
    if p.dark { Color32::from_rgb(0x88, 0x87, 0x82) } else { Color32::from_rgb(0x77, 0x76, 0x6f) }
}

/// A scrollbar thumb at rest and hovered: at rest an opaque quiet neutral, so row separators never show through it
/// (round 13 K2) and it still shows over the selected row (round 14 S1: the old faded `muted` read 1.03:1 on the
/// dark selection, 1.17 on the light): light #b5b4ae, dark #5c5b56, ≥ 1.5:1 on the selection; hovered `muted`
/// itself.
pub fn scroll_thumb(p: &Palette) -> (Color32, Color32) {
    let rest = if p.dark { Color32::from_rgb(0x5c, 0x5b, 0x56) } else { Color32::from_rgb(0xb5, 0xb4, 0xae) };
    (rest, p.muted)
}

/// The scrollbar thumb's width at rest (round 12 C1: the 2 px flush bar read as a partial right border).
pub const THUMB_W: f32 = 4.0;
/// The floating thumb's inset from its pane's (or card's) right edge.
pub const THUMB_INSET: f32 = 2.0;
/// The thumb's corner radius: round ends at THUMB_W.
pub const THUMB_RADIUS: u8 = 2;
/// The thumb's track stops this far short of its viewport's top and bottom, so its round ends never sit on a
/// card's border (round 13 K1, C3): 2 pt inside the 1 pt border's inner edge (round 15 C3/K2: 2 from the
/// viewport read 1–1.5 ink to ink).
pub const THUMB_TRACK_INSET: f32 = 3.0;

/// The thumb style for the floating bars (the list and the detail pane): THUMB_W wide, THUMB_INSET in from the
/// edge, hidden at rest and shown opaque while the pointer is over the area (calm surfaces, like macOS overlay
/// scrollbars; an idle thumb read as two extra bars of chrome and sat on the rows' borders). The tones are in
/// `scroll_thumb`. Call before the ScrollArea, with `set_thumb_fills` and `thumb_track`.
pub fn floating_thumb(scroll: &mut egui::style::ScrollStyle) {
    scroll.floating_width = THUMB_W;
    scroll.bar_outer_margin = THUMB_INSET;
    scroll.dormant_handle_opacity = 0.0;
    scroll.active_handle_opacity = 1.0;
}

/// The rect a scroll area's bar runs in: its viewport less THUMB_TRACK_INSET at the top and bottom.
pub fn thumb_track(viewport: egui::Rect) -> egui::Rect {
    viewport.shrink2(vec2(0.0, THUMB_TRACK_INSET))
}

/// What `set_thumb_fills` replaced: the inactive, hovered and active widget visuals.
pub type ThumbSaved = [egui::style::WidgetVisuals; 3];

/// Gives the scroll areas shown in `ui` the `scroll_thumb` colours (a bar paints in its outer ui's widget
/// `bg_fill`) and the THUMB_RADIUS ends (it takes the widgets' corner radius), returning what it replaced so
/// content can put it back (`restore_fills`).
pub fn set_thumb_fills(ui: &mut Ui, p: &Palette) -> ThumbSaved {
    let (rest, hover) = scroll_thumb(p);
    // Floating bars paint their thumb in the widgets' text colour unless told otherwise (egui's cool grey,
    // round 11 F2): use the fills, so every thumb is the warm `muted`.
    ui.spacing_mut().scroll.foreground_color = false;
    let w = &mut ui.visuals_mut().widgets;
    let saved = [w.inactive, w.hovered, w.active];
    for (v, fill) in [(&mut w.inactive, rest), (&mut w.hovered, hover), (&mut w.active, hover)] {
        v.bg_fill = fill;
        v.corner_radius = egui::CornerRadius::same(THUMB_RADIUS);
    }
    saved
}

pub fn restore_fills(ui: &mut Ui, [inactive, hovered, active]: ThumbSaved) {
    ui.spacing_mut().scroll.foreground_color = egui::style::ScrollStyle::default().foreground_color;
    let w = &mut ui.visuals_mut().widgets;
    (w.inactive, w.hovered, w.active) = (inactive, hovered, active);
}

/// Secondary text inside a modal dialog (labels, "Unassigned", "N matches", help section heads, "or"): `muted`
/// on the dialog fill, lifted where it would fall under 5.0:1 (dark).
pub fn dialog_muted(p: &Palette) -> Color32 {
    theme::muted_on(dialog_fill(p), p)
}

/// Turns off egui's scroll-edge fade for the scroll areas `ui` starts (the list and the detail pane), so they end
/// flush on the hint bar.
pub fn no_scroll_fade(ui: &mut Ui) {
    ui.spacing_mut().scroll.fade = NO_FADE;
}

pub const NO_FADE: egui::style::ScrollFadeStyle = egui::style::ScrollFadeStyle { strength: 0.0, size: 0.0 };

/// Muted text on a dialog's cursor row (the picker's "current" marker and muted names there): `muted` lifted
/// on `dialog_cursor_fill` where it would fall under 5.0:1.
pub fn muted_on_selected(p: &Palette) -> Color32 {
    theme::muted_on(dialog_cursor_fill(p), p)
}

/// Outline-pill text on the selected row: `muted`, lifted only where it would fall under 5.0:1 (`muted_on`).
pub fn selected_pill_text(p: &Palette) -> Color32 {
    theme::muted_on(p.selected_row, p)
}

/// The edge every list-row pill gets: its own text colour at 35%, so a wash stays visible on any row fill.
#[allow(dead_code)] // Kept stable for other screens; not called from one right now.
pub fn row_pill_stroke(fg: Color32) -> Stroke {
    Stroke::new(1.0, fg.gamma_multiply(0.35))
}

/// List-row pills: the one pill padding, 8/2.
pub const ROW_PILL_MARGIN: egui::Vec2 = PILL_MARGIN;

/// A list-row status pill: one treatment on every row (plain, hovered, selected) and in both themes: the opaque
/// fill (`pill_sized`), no edge (`components::status_pill_edge`). The selection is chosen to stay clear of every
/// status fill, so the pill keeps its shape there without a stroke.
pub fn pill_outlined(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    let edge = components::status_pill_edge(bg, fg, theme::palette(ui.ctx()));
    pill_sized(ui, text, bg, fg, edge, ROW_PILL_MARGIN)
}

/// Small text in an exact `text + 2×margin` rect (radius 10); the stroke is drawn inside, so outlined
/// and plain pills are the same size and share a baseline. A translucent wash (`Tint::bg`) is resolved over
/// `surface` first (`pill_fill`), so a status pill is one opaque colour on every row, card, pane and selection.
pub fn pill_sized(ui: &mut Ui, text: &str, bg: Color32, fg: Color32, stroke: Stroke, margin: egui::Vec2) -> egui::Response {
    let bg = pill_fill(bg, theme::palette(ui.ctx()));
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::body(12.0), fg);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + margin * 2.0, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, 10.0, bg, stroke, egui::StrokeKind::Inside);
        ui.painter().galley(rect.min + margin, galley, fg);
    }
    resp
}

/// A pill's painted fill: a translucent wash resolved over `surface` (opaque everywhere); a transparent fill
/// (outline pills) or an opaque one unchanged.
pub fn pill_fill(bg: Color32, p: &Palette) -> Color32 {
    match bg.a() {
        0 | 255 => bg,
        _ => theme::over(bg, p.surface),
    }
}

/// `text` cut to fit `max_w` (as `measure` lays it out) with "…"; trailing whitespace is trimmed before the
/// ellipsis. Unchanged when it already fits.
/// `truncate_text` for titles: a cut inside a word (a run of letters, digits and `/-_`) backs up to the last
/// whitespace, hyphen or slash, the pills' boundaries ("…reusable workflo…" → "…reusable…", "aws-node-term…" →
/// "aws-node…"); only a single word that fills the column is cut mid-word. Trailing `,;:` go before the "…"
/// ("sign-ins,…" → "sign-ins…"), and so does a trailing article or preposition (`TITLE_STOP_WORDS`: "…on the…"
/// → "…on…" → "…").
/// Unless backing up to the word boundary (and dropping stop words) would leave more than TITLE_SLACK_MAX empty:
/// then the plain cut that fills the room (a long identifier at the cut would empty ~135 pt).
pub fn truncate_title(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    let words = word_cut(text, max_w, &measure);
    if words == text || max_w - measure(&words) <= TITLE_SLACK_MAX {
        return words;
    }
    truncate_text(text, max_w, measure)
}

/// The most width a title's word-boundary cut may leave empty before it is cut mid-word instead (about one
/// short word at 13.5 bold).
pub const TITLE_SLACK_MAX: f32 = 48.0;

/// `truncate_title`'s word-boundary cut.
fn word_cut(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    let cut = truncate_text(text, max_w, measure);
    let Some(kept) = cut.strip_suffix('…') else { return cut };
    let word = |c: char| c.is_alphanumeric() || matches!(c, '/' | '-' | '_');
    // Mid-word: the kept text ends inside a word that the original continues.
    let mid_word = kept.ends_with(|c: char| !c.is_whitespace()) && text[kept.len()..].starts_with(word);
    // A slash breaks like a hyphen ("environments/kubern…" → "environments…", round 12 C3).
    let brk = |c: char| c.is_whitespace() || matches!(c, '-' | '/');
    let kept = match kept.rfind(brk) {
        Some(at) if mid_word && !kept[..at].trim_end_matches(brk).is_empty() => kept[..at].trim_end_matches(brk),
        _ => kept,
    };
    format!("{}…", drop_stop_words(trim_trailing_punctuation(kept)))
}

/// Words a cut title never ends on: they promise a word the "…" hides.
pub const TITLE_STOP_WORDS: [&str; 18] =
    ["the", "a", "an", "of", "on", "to", "from", "in", "for", "and", "or", "by", "with", "at", "into", "onto", "are", "is"];

/// `kept` without trailing stop words (case-insensitive) and the punctuation they leave, keeping at least one word.
fn drop_stop_words(mut kept: &str) -> &str {
    while let Some(at) = kept.rfind(char::is_whitespace) {
        let last = &kept[at..].trim_start();
        if !TITLE_STOP_WORDS.iter().any(|w| w.eq_ignore_ascii_case(last)) {
            break;
        }
        let rest = trim_trailing_punctuation(&kept[..at]);
        if rest.is_empty() {
            break;
        }
        kept = rest;
    }
    kept
}

/// `kept` without trailing whitespace and `,;:`, so an ellipsis never follows a dangling comma.
fn trim_trailing_punctuation(kept: &str) -> &str {
    let t = kept.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'));
    if t.is_empty() { kept.trim_end() } else { t }
}

/// `truncate_text` for pills: a cut leaving a word fragment of 3 characters or fewer after the last space or
/// hyphen ("secondary-E…") backs up to that boundary ("secondary…").
pub fn truncate_pill(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    let cut = truncate_text(text, max_w, measure);
    let Some(kept) = cut.strip_suffix('…') else { return cut };
    match pill_back_off(kept, &text[kept.len()..]) {
        Some(k) => format!("{k}…"),
        None => cut,
    }
}

/// The pill cut rule (`truncate_pill`): `kept` backed up to its last space or hyphen when it ends in a fragment
/// of ≤ 3 characters that `rest` continues; None when the cut stands.
pub fn pill_back_off<'a>(kept: &'a str, rest: &str) -> Option<&'a str> {
    if !rest.starts_with(char::is_alphanumeric) || !kept.ends_with(char::is_alphanumeric) {
        return None;
    }
    let at = kept.rfind(|c: char| c.is_whitespace() || c == '-')?;
    let prefix = kept[..at].trim_end_matches(|c: char| c.is_whitespace() || c == '-');
    (kept[at..].trim_start_matches(|c: char| c.is_whitespace() || c == '-').chars().count() <= 3 && !prefix.is_empty()).then_some(prefix)
}

pub fn truncate_text(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).chain(std::iter::once(text.len())).collect();
    let cut = |n: usize| format!("{}…", text[..if n == 0 { 0 } else { ends[n - 1] }].trim_end());
    // The longest prefix (in chars) whose cut form fits; widths grow with length.
    let (mut lo, mut hi) = (0, ends.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if measure(&cut(mid)) <= max_w { lo = mid } else { hi = mid - 1 }
    }
    cut(lo)
}

pub fn health_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// A load error: the column's full width, card padding, the title, the raw error and, with `retry`, a Retry
/// button. Pass `retry: false` when another control on screen already retries (the offline banner's Retry).
/// Returns true when Retry is clicked.
pub fn error_banner(ui: &mut Ui, p: &Palette, title: &str, raw: &str, retry: bool) -> bool {
    let sp = theme::spacing(ui.ctx());
    let mut clicked = false;
    Frame::new().fill(p.danger_subtle).corner_radius(theme::RADIUS).inner_margin(Margin::same(sp.card_pad as i8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).font(theme::bold(sp.body)).color(p.off_track));
        ui.add_space(sp.line_gap);
        ui.add(egui::Label::new(RichText::new(raw).color(p.muted).size(sp.small)).wrap());
        if retry {
            ui.add_space(sp.line_gap * 2.0);
            clicked = ui.add(button("Retry", p)).clicked();
        }
    });
    clicked
}

#[cfg(test)]
mod widget_tests {
    /// Renders one field in a real pass and returns (first glyph x, galley top) of the galley showing `shown`,
    /// relative to the field's rect.
    type MakeField<'f> = &'f dyn Fn(&mut String) -> egui::TextEdit<'_>;

    fn ink_at(make: MakeField<'_>, typed: &str, shown: &str) -> (f32, f32) {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut rect = egui::Rect::NOTHING;
        let mut text = typed.to_string();
        let mut out = None;
        for _ in 0..2 {
            let mut o = ctx.run_ui(input(), |ui| {
                ui.set_width(400.0);
                rect = ui.add_sized([400.0, 28.0], make(&mut text)).rect;
            });
            o.textures_delta.clear();
            out = Some(o);
        }
        fn walk(s: &egui::Shape, shown: &str, found: &mut Vec<(f32, f32)>) {
            match s {
                egui::Shape::Text(t) if t.galley.job.text == shown => {
                    let g = t.galley.rows.first().and_then(|r| r.glyphs.first().map(|gl| r.pos.x + gl.pos.x)).unwrap_or(0.0);
                    found.push((t.pos.x + g, t.pos.y));
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, shown, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        out.expect("a pass").shapes.iter().for_each(|c| walk(&c.shape, shown, &mut found));
        let (x, y) = *found.first().unwrap_or_else(|| panic!("{shown:?} not painted"));
        (x - rect.left(), y - rect.top())
    }

    #[test]
    fn every_placeholder_starts_where_typed_text_does() {
        // F1: the composer, search, go-to, sign-in and dialog fields each ink the placeholder at the typed
        // text's origin (the empty caret sits CARET_LEAD left); I1: the single-line fields share one vertical
        // offset.
        let p = &crate::theme::LIGHT;
        let font = || crate::theme::body(14.0);
        let fields: [(&str, MakeField<'_>, f32); 5] = [
            ("composer", &|t| super::detail::composer_edit(t, p), 0.0),
            ("search", &|t| super::input_field_in(t, "Hint", font(), p, super::search_margin()), super::SEARCH_PAD_X),
            ("go-to", &|t| super::input_field(t, "Hint", font(), p), super::INPUT_PAD_X),
            ("sign-in", &|t| super::input_field(t, "Hint", font(), p).password(true), super::INPUT_PAD_X),
            ("picker", &|t| super::pickers::field(t, "Hint", p), super::pickers::ROW_INSET),
        ];
        let mut tops = Vec::new();
        for (name, make, inset) in fields {
            let hint = if name == "composer" { super::detail::COMPOSER_HINT } else { "Hint" };
            let (hx, _) = ink_at(make, "", hint);
            let typed = if name == "sign-in" { "\u{2022}" } else { "A" };
            let (tx, ty) = ink_at(make, "A", typed);
            assert!((hx - tx).abs() < 0.5, "{name}: placeholder at {hx}, typed at {tx}");
            assert!((tx - inset).abs() < 1.5, "{name}: typed text at {tx}, inset {inset}");
            if name != "composer" {
                tops.push((name, ty));
            }
        }
        // The dialog fields share one offset; the page's search box sits half a point higher, with the combos'
        // and chips' text (round 15 B1).
        let lift = |name: &str| if name == "search" { 0.5 } else { 0.0 };
        let base = tops.iter().find(|(n, _)| *n != "search").expect("a dialog field").1;
        assert!(tops.iter().all(|(n, y)| (y + lift(n) - base).abs() < 0.25), "one vertical offset: {tops:?}");
    }

    #[test]
    fn read_only_is_inked_in_text() {
        // A1: the galley's glyphs carry `text`, the colour the painter actually draws; the name stays muted.
        crate::testutil::with_ui(|ui| {
            for p in [&crate::theme::LIGHT, &crate::theme::DARK] {
                let g = super::read_only_galley(ui.painter(), p);
                assert!(g.rows.iter().all(|r| r.visuals.mesh.vertices.iter().all(|v| v.color == p.text)), "dark={}", p.dark);
                assert!(g.job.sections.iter().all(|s| s.format.color == p.text), "dark={}", p.dark);
                assert_ne!(p.text, p.muted);
            }
        });
    }

    #[test]
    fn sign_in_shows_the_hint_bar() {
        assert!(super::hint_bar_shown(false, false), "the sign-in form");
        assert!(!super::hint_bar_shown(false, true), "still looking for a token");
        assert!(super::hint_bar_shown(true, false));
    }

    #[test]
    fn an_empty_fields_caret_sits_left_of_the_origin() {
        // I1: empty, the content origin moves CARET_LEAD left (egui's caret goes there) and the placeholder is
        // led back, so placeholder and typed text start at one x, in every field.
        let m = super::input_margin(12.0);
        let empty = super::caret_margin(m, true);
        assert_eq!(empty.left, m.left - 2, "caret 2 left of the text origin");
        assert_eq!((empty.right, empty.top, empty.bottom), (m.right, m.top, m.bottom));
        assert_eq!(super::caret_margin(m, false), m, "typed text keeps the origin");
        let job = super::empty_field_hint("API token", crate::theme::body(14.0), &crate::theme::LIGHT);
        assert_eq!(f32::from(empty.left) + job.sections[0].leading_space, f32::from(m.left), "placeholder on the typed text's origin");
        assert_eq!(super::caret_margin(egui::Margin::ZERO, true).left, -2, "the composer's zero margin too");
    }

    use super::*;

    #[test]
    fn chips_pad_12_either_side() {
        assert_eq!(chip_width(40.0), 64.0);
        assert_eq!(chip_width(40.2), 65.0);
        assert_eq!(INPUT_PAD_X, 8.0, "dialog fields");
        assert_eq!(SEARCH_PAD_X, INPUT_PAD_X, "one text-field inset in every field");
        let r = whole_points(egui::Rect::from_min_size(egui::pos2(10.5, 40.5), vec2(64.0, BUTTON_H)));
        assert_eq!((r.min, r.height()), (egui::pos2(11.0, 41.0), 28.0), "whole points, 28 tall");
        assert_eq!(INPUT_H, BUTTON_H, "one control height");
    }

    #[test]
    fn chip_names_are_sentence_case() {
        assert_eq!(sentence_case("Only Opened"), "Only opened");
        assert_eq!(sentence_case("To Do"), "To do");
        assert_eq!(sentence_case("Mine"), "Mine");
        assert_eq!(sentence_case("Open PLAT Work"), "Open PLAT work", "acronyms stay");
        assert_eq!(sentence_case("already sentence"), "already sentence");
    }

    #[test]
    fn truncation_trims_before_the_ellipsis() {
        let w = |t: &str| t.chars().count() as f32;
        assert_eq!(truncate_text("short", 10.0, w), "short");
        assert_eq!(truncate_text("hello world again", 7.0, w), "hello…", "no space before …");
        assert_eq!(truncate_text("abcdefgh", 5.0, w), "abcd…");
        assert_eq!(truncate_text("abc", 0.0, w), "…");
        assert_eq!(truncate_text("ééééé", 3.0, w), "éé…", "char boundaries");
        assert_eq!(truncate_title("Move the chart to PR checks", 23.0, w), "Move the chart to PR…", "a 1-letter fragment backs up a word");
        assert_eq!(truncate_title("Move the chart to PR checks", 21.0, w), "Move the chart to PR…", "an exact word end stays");
        assert_eq!(truncate_title("Move the chart to PR checks", 26.0, w), "Move the chart to PR…", "any fragment backs up a word");
        assert_eq!(truncate_title("Supercalifragilistic", 3.0, w), "Su…", "one long word is still cut");
        assert_eq!(truncate_title("Build a reusable workflow for it", 25.0, w), "Build a reusable…");
        assert_eq!(truncate_title("Move secondary-Environment checks", 20.0, w), "Move secondary…", "a hyphen is a boundary (K1)");
        assert_eq!(truncate_title("Investigate aws-node-termination leaks", 27.0, w), "Investigate aws-node…", "the last hyphen before the cut");
        assert_eq!(truncate_title("Fix ci/cd_pipeline steps now", 12.0, w), "Fix ci…", "/ breaks like a hyphen (round 12 C3); _ stays inside a word");
        assert_eq!(truncate_title("Add plugin to environments/cluster", 33.0, w), "Add plugin to environments…", "C3: the slash is the boundary");
        assert_eq!(truncate_title("Fix snake_case_names now", 13.0, w), "Fix…", "_ stays inside a word");
        assert_eq!(truncate_title("Fix-it", 3.0, w), "Fi…", "a word that fills the column is still cut");
        assert_eq!(truncate_title("Support sign-ins, logins and more", 18.0, w), "Support sign-ins…", "a trailing comma goes");
        assert_eq!(truncate_title("One: two three", 6.0, w), "One…", "a trailing colon goes");
        assert_eq!(truncate_title("Alpha beta", 8.0, w), "Alpha…");
        // C3: no title ends on an article or preposition.
        assert_eq!(truncate_title("Move the chart to PR checks", 18.0, w), "Move the chart…", "'to' goes");
        assert_eq!(truncate_title("Drill from a dashboard view", 15.0, w), "Drill…", "'from a' both go");
        assert_eq!(truncate_title("Verify the label on the axis", 24.0, w), "Verify the label…", "'on the' both go");
        assert_eq!(truncate_title("Fix tests for The release", 19.0, w), "Fix tests…", "case-insensitive");
        assert_eq!(truncate_title("The quick brown", 6.0, w), "The…", "a lone stop word stays");
        assert_eq!(truncate_title("Mirror of, the thing", 15.0, w), "Mirror…", "punctuation left behind goes too");
        assert_eq!(truncate_title("In the beginning", 30.0, w), "In the beginning", "uncut titles keep every word");
        // Round 11 C2: and, or, by, with, at, into, onto, are, is go too, repeatedly.
        assert_eq!(truncate_title("Ship logs into the store by default", 20.0, w), "Ship logs…", "'into the' both go");
        assert_eq!(truncate_title("Pods are pending and stuck", 18.0, w), "Pods are pending…", "'and' goes");
        assert_eq!(truncate_title("Alerts with or without labels", 16.0, w), "Alerts…", "'with or' both go");
        assert_eq!(truncate_title("Logs are at rest now", 13.0, w), "Logs…", "'are at' both go");
        for word in ["and", "or", "by", "with", "at", "into", "onto", "are", "is"] {
            assert!(TITLE_STOP_WORDS.contains(&word), "{word}");
        }
    }

    #[test]
    fn pill_cuts_back_up_to_a_space_or_hyphen() {
        let w = |t: &str| t.chars().count() as f32;
        assert_eq!(truncate_pill("secondary-Environment", 12.0, w), "secondary…", "hyphen is a boundary for pills");
        assert_eq!(truncate_pill("secondary-Environment", 15.0, w), "secondary-Envi…", "a 4-letter fragment stays");
        assert_eq!(truncate_pill("Platform Team", 11.0, w), "Platform…");
        assert_eq!(truncate_pill("Supercalifragilistic", 5.0, w), "Supe…", "one word is still cut");
        assert_eq!(truncate_pill("short", 10.0, w), "short");
    }

    #[test]
    fn the_comment_tip_sits_left_of_the_button() {
        assert_eq!(tip_align(true, true), Some(egui::RectAlign::LEFT), "under the composer: beside, never over the box");
        assert_eq!(tip_align(false, true), Some(egui::RectAlign::LEFT));
        assert_eq!(tip_align(true, false), Some(egui::RectAlign::TOP_START));
        assert_eq!(tip_align(false, false), None);
        // LEFT: the tip's right edge on the control's left edge, centred on it.
        assert_eq!(egui::RectAlign::LEFT.parent(), egui::Align2::LEFT_CENTER);
        assert_eq!(egui::RectAlign::LEFT.child(), egui::Align2::RIGHT_CENTER);
    }

    #[test]
    fn tooltips_go_above_when_the_hint_bar_is_close() {
        // The Comment button's bottom 20 above the hint bar: a 24 tooltip 8 below would cross its rule.
        assert!(tip_above(780.0, 800.0));
        assert!(!tip_above(700.0, 800.0), "room below: below, like every tooltip");
        assert!(!tip_above(800.0 - TIP_H - TIP_GAP, 800.0), "exactly fits");
    }

    #[test]
    fn tooltips_are_quieter_than_controls() {
        // F1: the hint bar's fill and text size, a 1 px border, 8 off the control, shorter than a control.
        for p in [&theme::LIGHT, &theme::DARK] {
            let f = tip_frame(p);
            assert_eq!(f.fill, issues::hint_bar_fill(p));
            assert_eq!(f.stroke, Stroke::new(1.0, p.border));
            assert!(theme::contrast(p.text, f.fill) >= 7.0, "dark={}", p.dark);
        }
        assert_eq!((TIP_GAP, TIP_TEXT), (8.0, 12.0));
        // The real one-line height: 12 px text, the margins, the edge.
        let h = crate::testutil::with_ui(|ui| ui.painter().layout_no_wrap("⌘Enter".into(), theme::body(TIP_TEXT), Color32::WHITE).size().y);
        let tip_h = h + f32::from(TIP_MARGIN.top + TIP_MARGIN.bottom) + 2.0;
        assert!(tip_h <= TIP_H && TIP_H < BUTTON_H, "{tip_h}");
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn a_key_tip_sits_even_in_its_frame() {
        // Round 14 F2: "⌘Enter" rode 1 low (9 above, 8 below to the frame's outer edge); raised 0.5, the two
        // insets are within half a point. Prose tips keep their place.
        for p in [theme::LIGHT, theme::DARK] {
            let inset = |t: &str| {
                let shapes = crate::testutil::paint_at2(vec2(800.0, 200.0), |ui| {
                    tip_frame(&p).show(ui, |ui| tip_row(ui, t, &p));
                });
                let (r, _) = *crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == issues::hint_bar_fill(&p)).expect("the tip");
                let ink = crate::testutil::ink_of(&shapes, t);
                (ink.top() - r.top(), r.bottom() - ink.bottom())
            };
            let (top, bottom) = inset("⌘Enter");
            assert!((top - bottom).abs() <= 0.5, "⌘Enter: {top} above, {bottom} below");
            assert_eq!(inset("Forget the saved token").0, inset("Look for JIRA_API_TOKEN or the config api_token again").0);
        }
    }

    #[test]
    fn one_caret_empty_or_typed() {
        // Round 14 I1/I3: the empty and the trailing caret are one stroke, CARET_W wide, painted whole (egui's
        // clip cut the trailing one to a device pixel); the trailing one starts on a device pixel, 1 px past the
        // last glyph's advance; the empty one keeps the CARET_LEAD rule.
        let p = theme::LIGHT;
        assert_eq!(theme::CARET_W, 1.0);
        for typed in ["abco", "CI-1860", ""] {
            let id = egui::Id::new("caret-test");
            let mut text = typed.to_string();
            let mut end_x = 0.0;
            let shapes = crate::testutil::paint_at2(vec2(800.0, 200.0), |ui| {
                let mut st = egui::text_edit::TextEditState::default();
                let n = text.chars().count();
                st.cursor.set_char_range(Some(egui::text_selection::CCursorRange::one(egui::text::CCursor::new(n))));
                st.store(ui.ctx(), id);
                ui.memory_mut(|m| m.request_focus(id));
                add_field(ui, None, &mut text, |t| input_field(t, "", theme::body(14.0), &p).id(id));
            });
            let mut carets = Vec::new();
            for c in &shapes {
                match &c.shape {
                    egui::Shape::LineSegment { points, stroke } if points[0].x == points[1].x => carets.push((points[0].x, stroke.width, c.clip_rect)),
                    egui::Shape::Text(t) if t.galley.job.text == typed && !typed.is_empty() => {
                        end_x = (t.pos.x * 2.0).round() / 2.0 + t.galley.rows[0].row.glyphs.last().map_or(0.0, |g| g.max_x())
                    }
                    _ => {}
                }
            }
            let (x, w, clip) = *carets.last().expect("a caret");
            assert_eq!(w, theme::CARET_W, "{typed:?}: one caret width");
            assert!(clip.min.x <= x - w / 2.0 && x + w / 2.0 <= clip.max.x, "{typed:?}: the whole stroke shows ({x}, clip {clip:?})");
            if typed.is_empty() {
                continue;
            }
            let left = (x - w / 2.0) * 2.0;
            assert_eq!(left, left.round(), "{typed:?}: on the pixel grid");
            let gap = left - end_x * 2.0;
            assert!((1.0..2.0).contains(&gap), "{typed:?}: caret {x}, advance end {end_x}, gap {gap} px");
        }
    }

    #[test]
    fn tooltip_keys_take_the_key_style() {
        // Round 13 F1: the key token in a tip is mono like the hint bar's; prose stays proportional.
        assert_eq!(tip_key_split("⌘Enter"), ("", "⌘Enter", ""));
        assert_eq!(tip_key_split("Copy link (c)"), ("Copy link (", "c", ")"));
        assert_eq!(tip_key_split("Refresh (⌘R)"), ("Refresh (", "⌘R", ")"));
        assert_eq!(tip_key_split("Keyboard help (?)"), ("Keyboard help (", "?", ")"));
        assert_eq!(tip_key_split("Writes are blocked (JIRA_TICKETS_READONLY)"), ("Writes are blocked (JIRA_TICKETS_READONLY)", "", ""));
        assert_eq!(tip_key_split("Forget the saved token"), ("Forget the saved token", "", ""));
        let job = tip_job("Copy link (c)", &theme::LIGHT);
        assert_eq!(job.text, "Copy link (c)");
        let font_at = |i: usize| job.sections.iter().find(|s| (s.byte_range.start.0..s.byte_range.end.0).contains(&i)).map(|s| s.format.font_id.clone());
        assert_eq!(font_at(0), Some(theme::body(TIP_TEXT)));
        assert_eq!(font_at(11), Some(theme::key_font()), "the key in the key style");
        assert_eq!(font_at(12), Some(theme::body(TIP_TEXT)));
    }

    #[test]
    fn tooltips_inset_their_ink_equally() {
        // Round 13 F2: TIP_INK_X from the edge to the ink on both sides, "⌘"'s bearing counted, real fonts.
        let p = theme::LIGHT;
        for text in [detail::COMMENT_TIP, "Copy link (c)", "Forget the saved token"] {
            let shapes = crate::testutil::paint_at2(vec2(800.0, 200.0), |ui| {
                tip_frame(&p).show(ui, |ui| tip_row(ui, text, &p));
            });
            let fill = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == issues::hint_bar_fill(&p)).expect("tip").0;
            let ink = crate::testutil::ink_of(&shapes, text);
            let (l, r) = (ink.left() - fill.left() - 1.0, fill.right() - 1.0 - ink.right());
            assert!((l - TIP_INK_X).abs() <= 0.5 && (r - TIP_INK_X).abs() <= 0.5, "{text}: {l} / {r}");
        }
    }

    #[test]
    fn one_centring_rule() {
        assert_eq!(centre_top_on(400.0, 0.0, 800.0, 300.0), 250.0);
        assert_eq!(centre_top_on(50.0, 0.0, 100.0, 300.0), 0.0, "never negative");
        assert_eq!(centre_top_on(400.5, 0.0, 801.0, 300.0), 251.0, "whole pixels");
        assert_eq!(centre_top_on(450.0, 0.0, 866.0, 300.0), 300.0, "on the window's middle, not the area's");
        assert_eq!(centre_top_on(450.0, 48.0, 818.0, 300.0), 252.0, "an area under a header");
        assert_eq!(centre_top_on(450.0, 0.0, 400.0, 300.0), 100.0, "kept inside the area");
    }

    #[test]
    fn sign_in_aside_reads_as_one_sentence() {
        assert_eq!(SIGN_IN_ASIDE.concat(), "Or export JIRA_API_TOKEN, or set api_token in config.toml, then check again.");
        assert_eq!(sign_in_width(300.0), 480.0);
        assert_eq!(sign_in_width(451.0), 480.0, "round 6: 480 so the aside has margin");
        assert_eq!(sign_in_width(470.3), 495.0, "a wider aside keeps 24 of margin");
    }

    #[test]
    fn the_sign_in_panel_pads_16_to_the_ink() {
        // 16 from the panel's inner edge to the title's ink (real fonts), and from "Check again"'s box to the
        // inner edge below.
        let p = crate::theme::LIGHT;
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(SIGN_IN_MARGIN).show(ui, |ui| {
                ui.set_width(480.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    sign_in_title(ui, &p);
                    ui.add_space(40.0);
                    check_again(ui, &p, 480.0);
                });
            });
        });
        let panel = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == p.surface).expect("panel").0;
        let title = crate::testutil::ink_of(&shapes, SIGN_IN_TITLE);
        assert!((title.top() - (panel.top() + 1.0) - SIGN_IN_PAD).abs() <= 0.5, "top {}", title.top() - panel.top() - 1.0);
        let again = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == theme::button_fill(&p)).expect("a filled button").0;
        assert_eq!(again.height(), BUTTON_H, "the shared button's height");
        assert!(((panel.bottom() - 1.0) - again.bottom() - SIGN_IN_PAD).abs() <= 0.5, "bottom {}", panel.bottom() - 1.0 - again.bottom());
    }

    #[test]
    fn a_link_is_at_least_24_tall_to_click() {
        let slot = Rect::from_min_size(egui::pos2(10.0, 100.0), vec2(40.0, 16.0));
        let hit = link_hit(slot);
        assert_eq!((hit.height(), hit.center(), hit.width()), (MIN_TARGET_H, slot.center(), 40.0), "grown evenly, text in place");
        let tall = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(40.0, 30.0));
        assert_eq!(link_hit(tall), tall, "never shrunk");
    }

    #[test]
    fn one_empty_state_size() {
        assert_eq!(EMPTY_STATE_SIZE, 14.0);
    }

    #[test]
    fn sign_in_error_replaces_the_prompt() {
        assert_eq!(sign_in_lead(None), ("Paste an Atlassian API token to connect.", false));
        assert_eq!(sign_in_lead(Some("op read failed")), ("op read failed", true));
    }

    #[test]
    fn tab_underline_is_two_points_on_whole_points() {
        // Round 7 measured a 2 pt rule (egui's panel separator stacked on ours) and the underline 1 pt above it.
        let header = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1440.0, HEADER_H));
        let rule = header_rule(header);
        assert_eq!((rule.top(), rule.bottom(), rule.width()), (48.0, 49.0, 1440.0), "one 1 pt rule under the 48 bar");
        assert_eq!(header_rule(header.translate(vec2(0.0, 0.3))), rule, "on whole points");
        let u = underline_rect(egui::Rangef::new(10.0, 60.0), rule);
        assert_eq!((u.top(), u.bottom(), u.width()), (47.0, 49.0, 50.0), "2 pt, over the rule's y-range");
        assert!(u.top() < rule.top() && u.bottom() == rule.bottom());
        assert_eq!(BAR_H, 48.0);
        assert_eq!((TITLE_GAP, TAB_GAP), (32.0, 24.0), "32 after the title (final ruling)");
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn header_gaps_are_measured_to_the_ink() {
        // Round 13 A1–A3, real fonts at the captures' density: 32 from the title's ink to the first tab's, 24
        // between tabs, and 8 between the right-hand labels' ink.
        let p = &theme::LIGHT;
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        ctx.set_pixels_per_point(2.0);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))), ..Default::default() };
        for _ in 0..3 {
            let mut o = ctx.run_ui(input(), |ui| {
                let lay = |t: &str, f: egui::FontId| ui.painter().layout_no_wrap(t.to_string(), f, p.text);
                let title = lay("Jira Tickets", theme::bold(15.0));
                let tabs: Vec<_> = TABS.iter().map(|(_, n)| lay(n, theme::body(14.0))).collect();
                let xs = tab_origins(ui.ctx(), 16.0, &title, &tabs);
                let ink = |x: f32, g: &egui::Galley| {
                    let r = theme::visible_ink(ui.ctx(), g);
                    egui::Rangef::new(r.left() + x, r.right() + x)
                };
                let raster = |x: f32, g: &egui::Galley| {
                    let r = theme::ink_rect(g);
                    egui::Rangef::new(r.left() + x, r.right() + x)
                };
                assert!((raster(xs[0], &tabs[0]).min - raster(16.0, &title).max - TITLE_GAP).abs() <= 0.25, "title gap");
                for i in 1..tabs.len() {
                    let gap = ink(xs[i], &tabs[i]).min - ink(xs[i - 1], &tabs[i - 1]).max;
                    assert!((gap - TAB_GAP).abs() <= 0.5, "tab {i}: {gap}");
                    assert_eq!(xs[i] * 2.0, (xs[i] * 2.0).round(), "on whole pixels");
                }
                for (i, g) in tabs.iter().enumerate() {
                    let u = tab_underline_x(xs[i], g);
                    // The underline keeps the raster box (the header's active tab does not move).
                    let r = theme::ink_rect(g);
                    let ink = egui::Rangef::new(r.left() + xs[i], r.right() + xs[i]);
                    assert!((u.min - ink.min).abs() <= 0.25 && (u.max - ink.max).abs() <= 0.25, "J3: the underline spans the ink: {u:?} vs {ink:?}");
                }
                let (name, dot, ro) = (lay("Shawn Sorichetti", theme::body(12.0)), lay("·", theme::body(12.0)), read_only_galley(ui.painter(), p));
                let gaps = [theme::ink_gap(&name, &dot, LABEL_GAP), theme::ink_gap(&dot, &ro, LABEL_GAP)];
                let at = header_right(1424.0, Some(name.size().x), Some(ro.size().x), dot.size().x, gaps);
                let (n, d, r) = (ink(at.name.unwrap(), &name), ink(at.dot.unwrap(), &dot), ink(at.read_only.unwrap(), &ro));
                assert!((d.min - n.max - LABEL_GAP).abs() < 0.01 && (r.min - d.max - LABEL_GAP).abs() < 0.01, "{n:?} {d:?} {r:?}");
            });
            o.textures_delta.clear();
        }
    }

    #[test]
    fn tab_keys_match_the_tab_order() {
        let keys: Vec<u8> = TABS.iter().map(|(s, _)| tab_key(*s)).collect();
        assert_eq!(keys, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn small_label_sits_on_the_names_baseline() {
        // A 14 px name with its baseline 13 down, drawn at top 10: a 12 px label (baseline 11 down) goes at 12.
        assert_eq!(top_for_baseline(10.0 + 13.0, 11.0), 12.0);
    }

    #[test]
    fn header_spinner_ignores_search() {
        use crate::app::Loaded;
        use crate::store::ListId;
        let loading = || Loaded { loading: true, ..Default::default() };
        let search = [(ListId::Search, loading())];
        assert!(!header_syncing(search.iter().map(|(i, l)| (i, l))));
        let sync = [(ListId::Search, Loaded::default()), (ListId::Epics, loading())];
        assert!(header_syncing(sync.iter().map(|(i, l)| (i, l))));
        assert!(!header_syncing(std::iter::empty()));
    }

    #[test]
    fn primary_text_is_on_accent_in_both_themes() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(primary_colors(p), (p.accent, p.on_accent));
            assert!(theme::contrast(p.on_accent, p.accent) >= 4.5, "dark={}", p.dark);
            assert_ne!(components::toggle_chip_colors(p, true, false).0, p.accent, "an on chip is not a primary button");
        }
    }

    #[test]
    fn backdrop_stops_above_the_hint_bar() {
        let w = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1440.0, 900.0));
        assert_eq!(backdrop_rect(w, Some(24.0)).max.y, 876.0);
        assert_eq!(backdrop_rect(w, None), w);
    }

    #[test]
    fn row_pill_edge_is_the_text_colour_faded() {
        let s = row_pill_stroke(Color32::from_rgb(200, 100, 0));
        assert_eq!(s.width, 1.0);
        assert!(s.color.a() > 80 && s.color.a() < 100, "{}", s.color.a());
    }

    #[test]
    fn dialog_edge_shows_against_dialog_and_dimmed_page() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = dialog_stroke(p).color;
            let dimmed = theme::over(Color32::from_black_alpha(backdrop_alpha(p)), p.bg);
            assert!(theme::contrast(edge, dialog_fill(p)) >= 1.5, "dark={}", p.dark);
            // What separates the dialog from the dimmed page: its edge in dark; in light the white fill itself
            // (the lighter 64 backdrop leaves the edge close to the page, 1.06:1, but the fill stands ~2:1).
            let lift = if p.dark { theme::contrast(edge, dimmed) } else { theme::contrast(dialog_fill(p), dimmed) };
            assert!(lift >= 1.3, "dark={}: {lift:.2}", p.dark);
        }
    }

    #[test]
    fn thumbs_take_the_warm_fill() {
        // F2: floating bars paint the widgets' fill (our `muted` thumb), not their text colour.
        crate::testutil::with_ui(|ui| {
            let p = theme::palette(ui.ctx());
            assert!(ui.spacing().scroll.foreground_color, "egui's floating default");
            let saved = set_thumb_fills(ui, p);
            assert!(!ui.spacing().scroll.foreground_color);
            assert_eq!(ui.visuals().widgets.inactive.bg_fill, scroll_thumb(p).0);
            // C1: round ends at THUMB_W, inset THUMB_INSET, shown at rest.
            assert_eq!(ui.visuals().widgets.inactive.corner_radius, egui::CornerRadius::same(THUMB_RADIUS));
            assert!((3.0..=4.0).contains(&THUMB_W) && f32::from(THUMB_RADIUS) * 2.0 <= THUMB_W);
            let mut bar = egui::style::ScrollStyle::floating();
            floating_thumb(&mut bar);
            assert_eq!((bar.floating_width, bar.bar_outer_margin, bar.dormant_handle_opacity), (THUMB_W, THUMB_INSET, 0.0), "hidden at rest");
            assert_eq!(bar.active_handle_opacity, 1.0, "opaque on hover");
            assert_eq!(scroll_thumb(p).0.a(), 255, "K2: separators never show through");
            let t = thumb_track(egui::Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(10.0, 500.0)));
            assert_eq!((t.top(), t.bottom()), (100.0 + THUMB_TRACK_INSET, 500.0 - THUMB_TRACK_INSET), "K1: inset both ends");
            restore_fills(ui, saved);
            assert!(ui.spacing().scroll.foreground_color, "content keeps egui's default");
        });
    }

    #[test]
    fn scroll_thumbs_show_but_stay_quiet() {
        for p in [&theme::LIGHT, &theme::DARK] {
            // Quiet on the surface the thumbs float over (the list's cards, the detail pane, popups), and still
            // there over the selected row (round 14 S1).
            let (rest, hover) = scroll_thumb(p);
            assert_eq!(rest.a(), 255);
            let c = theme::contrast(rest, p.surface);
            assert!((1.3..=2.5).contains(&c), "dark={}: {c:.2}", p.dark);
            let sel = theme::contrast(rest, p.selected_row);
            assert!(sel >= 1.5, "dark={}: {sel:.2} on the selection", p.dark);
            assert!(theme::contrast(hover, p.surface) > c);
        }
    }

    #[test]
    fn dialog_secondary_text_holds_five_to_one() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert!(theme::contrast(dialog_muted(p), dialog_fill(p)) >= 5.0, "dark={}", p.dark);
            assert!(theme::contrast(selected_pill_text(p), p.selected_row) >= 5.0, "dark={}", p.dark);
        }
        assert_eq!(dialog_muted(&theme::LIGHT), theme::LIGHT.muted, "light muted already reads on white");
    }

    #[test]
    fn backdrop_is_heavier_in_dark() {
        assert!(backdrop_alpha(&theme::DARK) > backdrop_alpha(&theme::LIGHT));
        assert_eq!(dialog_fill(&theme::DARK), theme::DARK_DIALOG);
        assert_eq!(dialog_fill(&theme::LIGHT), theme::LIGHT.surface);
    }

    #[test]
    fn header_buttons_are_outermost_and_never_move() {
        let edge = 1424.0;
        let m_w = 7.5;
        let measure = |t: &str| t.chars().count() as f32 * m_w;
        let long = "Bartholomew Featherstonehaugh-Smythe III"; // 40 chars
        assert_eq!(long.chars().count(), 40);
        let names = [None, Some(measure("Shawn")), Some(measure(&truncate_text(long, name_cap(m_w), measure)))];
        for ro in [None, Some(52.0)] {
            let first = header_right(edge, None, ro, 4.0, [LABEL_GAP; 2]);
            assert_eq!(first.help, edge - BUTTON_H, "? at the 16 px edge");
            assert_eq!(first.sync, first.help - BUTTON_H - 8.0, "sync 8 left of ?");
            for name_w in names {
                let at = header_right(edge, name_w, ro, 4.0, [LABEL_GAP; 2]);
                assert_eq!((at.help, at.sync), (first.help, first.sync), "ro={ro:?} name_w={name_w:?}");
                // The text nearest sync (Read-only, else the name) ends 12 left of it (A1).
                let nearest_right = match (at.read_only, at.name) {
                    (Some(x), _) => x + ro.unwrap(),
                    (None, Some(x)) => x + name_w.unwrap(),
                    (None, None) => continue,
                };
                assert_eq!(at.sync - nearest_right, 12.0, "gap {}", at.sync - nearest_right);
                if let (Some(n), Some(r)) = (at.name, at.read_only) {
                    let d = at.dot.expect("a dot between them");
                    assert_eq!(r - (d + 4.0), LABEL_GAP, "· 8 before Read-only");
                    assert_eq!(d - (n + name_w.unwrap()), LABEL_GAP, "name 8 before the ·, growing leftward");
                } else {
                    assert_eq!(at.dot, None);
                }
            }
        }
        assert!(measure(&truncate_text(long, name_cap(m_w), measure)) <= name_cap(m_w), "a long name is cut to the cap");
        assert_eq!(header_right(edge, None, None, 4.0, [LABEL_GAP; 2]).labels_w, 0.0);
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(read_only_color(p), p.text, "Read-only in text, the name muted");
        }
    }

    #[test]
    fn toast_dot_marks_the_type_only() {
        use crate::app::ToastKind;
        assert_eq!(toast_dot(ToastKind::Error), TOAST_ERROR_DOT);
        assert_eq!(toast_dot(ToastKind::Success), theme::DARK.on_track);
        assert_eq!(toast_dot(ToastKind::Info), theme::DARK.muted);
        assert_eq!(toast_mark(ToastKind::Info), None, "a plain notice has no dot");
        assert_eq!(toast_mark(ToastKind::Error), Some(TOAST_ERROR_DOT));
        assert_eq!(toast_mark(ToastKind::Success), Some(theme::DARK.on_track));
        let kinds = [ToastKind::Error, ToastKind::Success, ToastKind::Info];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(toast_dot(*a), toast_dot(*b), "one colour per kind");
            }
        }
        for p in [&theme::LIGHT, &theme::DARK] {
            for k in kinds {
                let c = toast_dot(k);
                assert!(theme::contrast(c, p.tooltip) >= 3.0, "dark={} {k:?}: {:.2}", p.dark, theme::contrast(c, p.tooltip));
            }
        }
        let fill = theme::DARK.tooltip;
        let (b, f) = (theme::DARK.border, fill);
        // Neutral like `border`: R = G, B a touch lower in about the same ratio (#4a4a47, ruled final in round 8).
        assert_eq!(f.r(), f.g());
        assert!(((f32::from(f.b()) / f32::from(f.r())) - (f32::from(b.b()) / f32::from(b.r()))).abs() < 0.015, "{f:?}");
        assert_eq!((theme::LIGHT.tooltip, fill), (Color32::from_rgb(0x3a, 0x3a, 0x37), Color32::from_rgb(0x4a, 0x4a, 0x47)), "round-8 L4 ruling");
        assert!(theme::contrast(theme::DARK.tooltip_text, fill) >= 6.0, "{:.2}", theme::contrast(theme::DARK.tooltip_text, fill));
        assert_eq!(toast_stroke(&theme::LIGHT), Stroke::NONE, "light: the inverted fill alone");
        let edge = toast_stroke(&theme::DARK).color;
        assert!(theme::contrast(edge, fill) >= 1.5, "dark edge off the toast {:.2}", theme::contrast(edge, fill));
        for pane in [theme::DARK.bg, theme::DARK.surface] {
            assert!(theme::contrast(edge, pane) >= 3.0, "dark edge off the pane {:.2}", theme::contrast(edge, pane));
        }
    }

    #[test]
    fn toast_centres_on_the_window() {
        assert_eq!(toast_centre_x(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))), 720.0);
        assert_eq!(toast_offset(Some(24.0)), 40.0);
        assert_eq!(toast_offset(None), 24.0);
        assert!(toast_shadow(&theme::DARK).color.a() > toast_shadow(&theme::LIGHT).color.a());
        assert_eq!(TOAST_MARGIN.left, TOAST_MARGIN.right);
    }

    #[test]
    fn the_toast_insets_16_to_the_ink() {
        // Round 13 D2: 16 from the fill to the dot on the left and to the text's ink on the right, real fonts.
        let p = theme::DARK;
        for (dot, text, x) in [theme::DARK.muted, theme::DARK.on_track]
            .into_iter()
            .flat_map(|d| ["Read-only mode: transition not sent to Jira", "Read-only mode: rank not sent to Jira"].map(move |t| (d, t)))
            .flat_map(|(d, t)| [0.0, 0.5, 884.5].map(move |x| (d, t, x)))
        {
            let shapes = crate::testutil::paint_at2(vec2(1440.0, 400.0), |ui| {
                let at = egui::Rect::from_min_size(egui::pos2(x, 10.0), vec2(1000.0, 200.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                    Frame::new().fill(p.tooltip).inner_margin(TOAST_MARGIN).show(ui, |ui| toast_body(ui, &p, Some(dot), text));
                });
            });
            let fill = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == p.tooltip).expect("toast").0;
            // Round 15 D2: to the rasters' visible coverage.
            let (mut l, mut r) = (0.0, 0.0);
            let _ = crate::testutil::paint_at2(vec2(1440.0, 100.0), |ui| {
                let g = ui.painter().layout_no_wrap(text.to_string(), theme::body(14.0), p.tooltip_text);
                let (a, v) = (theme::ink_rect(&g), theme::visible_ink(ui.ctx(), &g));
                (l, r) = (v.left() - a.left(), a.right() - v.right());
            });
            let raster = crate::testutil::ink_of(&shapes, text);
            let ink = egui::Rect::from_x_y_ranges(raster.left() + l..=raster.right() - r, raster.y_range());
            // Round 14 D1: 12 to the ink above (the "R") and below (the "y" of "only").
            assert!((ink.top() - fill.top() - TOAST_INK_Y).abs() <= 0.5, "top {}", ink.top() - fill.top());
            assert!((fill.bottom() - ink.bottom() - TOAST_INK_Y).abs() <= 0.5, "bottom {}", fill.bottom() - ink.bottom());
            assert!((fill.right() - ink.right() - TOAST_INK_X).abs() <= 0.01, "{text} at {x}: right {}", fill.right() - ink.right());
            let circle = shapes
                .iter()
                .find_map(|c| match &c.shape {
                    egui::Shape::Circle(k) if k.fill == dot => Some(*k),
                    _ => None,
                })
                .expect("the dot");
            assert!((circle.center.x - circle.radius - fill.left() - TOAST_INK_X).abs() <= 0.01, "{text} at {x}: dot left");
            assert!(
                (ink.left() - (circle.center.x + circle.radius) - 8.0).abs() <= 0.6,
                "{text} at {x}: dot to ink {}",
                ink.left() - circle.center.x - circle.radius
            );
            // On the text's middle: the cap ink ("R") above it and the baseline below it are the same distance away.
            let line = crate::testutil::with_ui(|ui| theme::Ink::of(ui.ctx(), &theme::body(14.0), None));
            let base = ink.top() - line.top + line.base;
            assert!(((circle.center.y - ink.top()) - (base - circle.center.y)).abs() <= 0.6, "{text} at {x}: dot off centre");
        }
    }

    #[test]
    fn a_plain_toast_insets_16_to_the_ink_either_side() {
        let p = theme::DARK;
        let text = "Theme: Dark";
        let shapes = crate::testutil::paint_at2(vec2(1440.0, 400.0), |ui| {
            let at = egui::Rect::from_min_size(egui::pos2(0.5, 10.0), vec2(1000.0, 200.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                Frame::new().fill(p.tooltip).inner_margin(TOAST_MARGIN).show(ui, |ui| toast_body(ui, &p, None, text));
            });
        });
        assert!(!shapes.iter().any(|c| matches!(c.shape, egui::Shape::Circle(_))), "no dot");
        let fill = crate::testutil::fills(&shapes).iter().find(|(_, c)| *c == p.tooltip).expect("toast").0;
        let (mut l, mut r) = (0.0, 0.0);
        let _ = crate::testutil::paint_at2(vec2(1440.0, 100.0), |ui| {
            let g = ui.painter().layout_no_wrap(text.to_string(), theme::body(14.0), p.tooltip_text);
            let (a, v) = (theme::ink_rect(&g), theme::visible_ink(ui.ctx(), &g));
            (l, r) = (v.left() - a.left(), a.right() - v.right());
        });
        let raster = crate::testutil::ink_of(&shapes, text);
        let (left, right) = (raster.left() + l - fill.left(), fill.right() - (raster.right() - r));
        assert!((left - TOAST_INK_X).abs() <= 0.6 && (right - TOAST_INK_X).abs() <= 0.6, "left {left} right {right}");
    }

    #[test]
    fn dialog_parts_share_one_edge() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = dialog_stroke(p).color;
            assert!(theme::contrast(edge, p.bg) >= 1.3, "field edge on its bg fill, dark={}", p.dark);
            // Cancel is the shared `button` everywhere: its edge shows, its text reads.
            assert!(theme::contrast(p.border, theme::button_fill(p)) >= 1.2, "Cancel's edge shows, dark={}", p.dark);
            assert!(theme::contrast(p.text, theme::button_fill(p)) >= 7.0);
        }
    }

    #[test]
    fn long_toasts_wrap_inside_the_cap() {
        assert_eq!(toast_wrap_w(false) + 2.0 * TOAST_INK_X, TOAST_MAX_W);
        assert!(toast_wrap_w(true) < toast_wrap_w(false), "the dot's slot comes off the text");
    }

    #[test]
    fn one_spinner_per_operation() {
        assert_eq!(spinner_slots(true, false, false, false), (true, false), "⌘R: header");
        assert_eq!(spinner_slots(true, true, true, false), (true, false), "⌘R with a quick filter: header only");
        assert_eq!(spinner_slots(false, true, false, false), (false, true), "filter or JQL: count row only");
        assert_eq!(spinner_slots(false, false, false, false), (false, false));
        assert_eq!(spinner_slots(true, true, false, true), (false, true), "filtered first load: the count row only");
        assert_eq!(spinner_slots(true, false, false, true), (false, true), "first load: the count row only");
    }

    #[test]
    fn toast_sits_above_the_hint_bar_when_there_is_one() {
        assert_eq!(toast_offset(Some(24.0)), 40.0);
        assert_eq!(toast_offset(None), 24.0);
    }

    #[test]
    fn splits_have_no_cap_but_the_other_pane() {
        // Wide window: the first pane may take all but the other pane's minimum.
        assert_eq!(split_range(2560.0, 320.0, 576.0), 320.0..=1984.0);
        // Narrow window: the range collapses to the minimum instead of inverting.
        assert_eq!(split_range(600.0, 320.0, 576.0), 320.0..=320.0);
        // A saved width wider than this window shrinks to fit it; one that fits is kept as dragged.
        assert_eq!(split_width(Some(3000.0), 400.0, 1440.0, 320.0, 576.0), 864.0);
        assert_eq!(split_width(Some(1500.0), 400.0, 2560.0, 320.0, 576.0), 1500.0);
        // Nothing saved: the default; dragged under the minimum: the minimum.
        assert_eq!(split_width(None, 400.0, 1440.0, 320.0, 576.0), 400.0);
        assert_eq!(split_width(Some(10.0), 400.0, 1440.0, 320.0, 576.0), 320.0);
        // A window narrower than the minimum: never wider than the window, never negative.
        assert_eq!(split_width(Some(900.0), 400.0, 200.0, 320.0, 576.0), 200.0);
        assert_eq!(split_width(None, 400.0, -5.0, 320.0, 576.0), 0.0);
    }
}

#[cfg(test)]
mod tip_gap_tests {
    /// Hovers a Comment-like button with its tooltip open (below, or beside on the left) and returns the painted
    /// gap between the button's rect and the tip's frame rect.
    fn painted_gap(beside: bool) -> f32 {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let p = crate::theme::LIGHT;
        let mut btn = egui::Rect::NOTHING;
        let mut rects = Vec::new();
        for i in 0..4 {
            let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
            input.time = Some(f64::from(i));
            input.events.push(egui::Event::PointerMoved(egui::pos2(310.0, 114.0)));
            let mut o = ctx.run_ui(input, |ui| {
                ui.add_space(100.0);
                ui.horizontal(|ui| {
                    ui.add_space(300.0);
                    let r = ui.add(super::primary_button_if("Comment", true, &p));
                    btn = r.rect;
                    super::show_tip(&r, super::detail::COMMENT_TIP.into(), beside.then_some(egui::RectAlign::LEFT));
                });
            });
            o.textures_delta.clear();
            rects = o.shapes.iter().filter_map(|c| if let egui::Shape::Rect(r) = &c.shape { Some(r.rect) } else { None }).collect();
        }
        let tip = rects.into_iter().find(|r| *r != btn).expect("the tip's frame");
        if beside { btn.left() - tip.right() } else { tip.top() - btn.bottom() }
    }

    #[test]
    fn tooltips_paint_tip_gap_off_the_control() {
        // F2 (round 12): the gap is measured between painted rects, below and beside.
        for beside in [false, true] {
            let g = painted_gap(beside);
            assert!((g - super::TIP_GAP).abs() <= 0.5, "beside={beside}: {g}");
        }
    }
}
