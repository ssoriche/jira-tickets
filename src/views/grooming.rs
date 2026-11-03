//! The Grooming screen: a header with the mode switch, the report line and the Ready for Work ceiling, then the
//! Pre-check or Session body.

use crate::app::{Action, App, GroomMode, GroomingState};
use crate::grooming::{self, Level, Promotion, Verdict};
use crate::jira;
use crate::keys::GroomHint;
use crate::report::{self, Badge};
use crate::theme::{self, Palette};
use crate::views::components as c;
use crate::views::grooming_guide as guide;
use crate::views::{PILL_MARGIN, empty_state, outline_pill, primary_button};
use egui::{Color32, Frame, Margin, RichText, Stroke, Ui, vec2};

/// The gap between the header's groups (the switch, the report line), and under the header.
const GROUP_GAP: f32 = 16.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            header(app, ui, p);
            ui.add_space(GROUP_GAP);
            match app.grooming.mode {
                GroomMode::PreCheck => pre_check_body(app, ui, p),
                GroomMode::Session => session_body(app, ui, p),
            }
        });
    dialogs(app, &ui.ctx().clone(), p);
}

/// The mode switch, the report line, and the Ready for Work count at the right.
fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    let sp = theme::spacing(ui.ctx());
    let mode = app.grooming.mode;
    let line = report_line(app.grooming.report.as_ref().map(|(path, r)| (path.as_path(), r)), &app.cfg.grooming.report_dir);
    let g = &app.cfg.grooming;
    let ready = app.ready_total();
    let level = grooming::ceiling_level(ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    let ceiling = ceiling_text(&g.status_ready, ready, g.ready_hard_ceiling, ceiling_warns(level));
    let tip = ceiling_tip(&g.status_ready, ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    let purposes = [GroomMode::PreCheck, GroomMode::Session].map(|m| guide::purpose(m, &g.status_new, &g.status_planning, &g.status_ready));
    let purpose = purposes[usize::from(mode == GroomMode::Session)].clone();
    ui.horizontal(|ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        let modes = [GroomMode::PreCheck, GroomMode::Session];
        let at = modes.iter().position(|m| *m == mode).unwrap_or(0);
        if let Some(i) = c::segmented(ui, p, &["Pre-check", "Session"], at) {
            app.actions.push(Action::GroomMode(modes[i]));
        }
        ui.add_space(GROUP_GAP - ui.spacing().item_spacing.x);
        ui.label(RichText::new(line).color(p.muted).size(sp.small));
        if ceiling_shown(mode) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if ceiling_warns(level) { p.at_risk } else { p.muted };
                crate::views::tooltip(ui.label(RichText::new(&ceiling).color(color).size(sp.small)), tip);
            });
        }
    });
    // Always present in both modes, at the prose measure, and as tall as the longer of the two, so the body never
    // jumps on a mode switch. The 8 is measured to the ink, so the layout's own item spacing comes off it.
    ui.add_space(8.0 - ui.spacing().item_spacing.y);
    let w = sp.prose_max_width.min(ui.available_width());
    // Each intro at the widest measure (at most `w`) that leaves no single word on its last line.
    let layout = |t: &str, w: f32| ui.painter().layout(t.to_string(), theme::body(sp.body), p.muted, w);
    let fit = |t: &str| widow_free_width(w, |w| last_row_words(&layout(t, w)));
    let tallest = purposes.iter().map(|t| layout(t, fit(t)).size().y).fold(0.0, f32::max);
    let purpose_w = fit(&purpose);
    ui.allocate_ui_with_layout(vec2(w, tallest), egui::Layout::top_down(egui::Align::Min), |ui| {
        // The box keeps `w` (allocated above); the text wraps at its own widow-free measure.
        ui.set_min_height(tallest);
        ui.set_max_width(purpose_w);
        ui.add(egui::Label::new(RichText::new(purpose).color(p.muted).size(sp.body)).wrap());
    });
}

/// The words on a wrapped galley's last line.
fn last_row_words(g: &egui::Galley) -> usize {
    g.rows.last().map_or(0, |r| r.glyphs.iter().map(|gl| gl.chr).collect::<String>().split_whitespace().count())
}

/// The narrowest a widow fix may take a paragraph, as a share of its measure.
const WIDOW_MIN_SHARE: f32 = 0.8;
/// The step a widow fix narrows a paragraph by.
const WIDOW_STEP: f32 = 8.0;

/// The widest width up to `w` at which a wrapped paragraph's last line holds two or more words (`last_words`
/// lays it out at a width and counts them). A one-line paragraph keeps `w`; past WIDOW_MIN_SHARE it gives up.
pub fn widow_free_width(w: f32, last_words: impl Fn(f32) -> usize) -> f32 {
    let mut at = w;
    while at >= w * WIDOW_MIN_SHARE {
        if last_words(at) != 1 {
            return at;
        }
        at -= WIDOW_STEP;
    }
    w
}

/// The list column's default share of the body sits between these widths. A drag may go past LIST_MAX: it only
/// has to leave the focused ticket RIGHT_MIN.
const LIST_MIN: f32 = 360.0;
const LIST_MAX: f32 = 560.0;
/// The focused ticket's narrowest, beside a dragged list.
const RIGHT_MIN: f32 = 360.0;

/// The list column's width in `avail`: the dragged width saved under `split`, else `share` held to LIST_MIN–LIST_MAX.
fn list_width(ui: &Ui, split: egui::Id, share: f32, avail: f32) -> f32 {
    crate::views::split_width(crate::views::saved_split(ui, split), share.clamp(LIST_MIN, LIST_MAX), avail, LIST_MIN, PANE_GAP + RIGHT_MIN)
}

/// The drag handle in the PANE_GAP right of a list that starts at `left` and is `list_w` wide.
fn list_splitter(ui: &mut Ui, split: egui::Id, left: f32, list_w: f32, top: f32, h: f32) {
    let gap = egui::Rect::from_x_y_ranges(left + list_w..=left + list_w + PANE_GAP, top..=top + h);
    crate::views::splitter(ui, split, left, gap);
}
/// The gap between the list and the focused ticket, and the cards' inner padding.
const PANE_GAP: f32 = 16.0;
/// A list row's padding: the density's card pad at the sides (so rows and the cards beside them share one text
/// inset), 8 above and below plus the density's row room.
fn row_pad(ui: &Ui) -> Margin {
    row_margin(theme::spacing(ui.ctx()))
}

/// `row_pad` for a density.
pub fn row_margin(sp: &theme::Spacing) -> Margin {
    let x = sp.card_pad as i8;
    let y = (8.0 + sp.row_extra) as i8;
    Margin { left: x, right: x, top: y, bottom: y }
}
/// Between a row's title line and its pill line (the density's line gap, 4 in Compact).
fn line_gap(ui: &Ui) -> f32 {
    theme::spacing(ui.ctx()).line_gap
}
const PILL_GAP: f32 = 6.0;
/// The Issues list's row overlap.
const ROW_GAP: f32 = -1.0;

/// What the Grooming hint bar describes: an open dialog, else the mode.
pub fn hint(g: &GroomingState) -> GroomHint {
    match (g.pending_report.is_some(), g.summary_open, g.mode) {
        (true, _, _) => GroomHint::Overwrite,
        (_, true, _) => GroomHint::Summary,
        (_, _, GroomMode::PreCheck) => GroomHint::PreCheck,
        (_, _, GroomMode::Session) if g.bounce_text.is_some() => GroomHint::Bounce,
        (_, _, GroomMode::Session) if g.placement.is_some() => GroomHint::Placement,
        (_, _, GroomMode::Session) => GroomHint::Session,
    }
}

/// A verdict's pills, in the readiness bar's own words: "Ready", or one pill per thing missing ("Empty" only when
/// nothing else applies, `guide::shown_codes`).
pub fn verdict_pills(v: &Verdict) -> Vec<&'static str> {
    match v {
        Verdict::Ready => vec!["Ready"],
        Verdict::NeedsDetail(codes) => guide::shown_codes(codes).into_iter().map(guide::code_words).collect(),
    }
}

/// A verdict pill's tooltip: what the verdict does at Promote.
fn verdict_tip(v: &Verdict) -> &'static str {
    match v {
        Verdict::Ready => "Ready: promoted at the summary (u undoes)",
        Verdict::NeedsDetail(_) => "Needs detail: stays in New at the summary (u undoes)",
    }
}

/// A pill's width (`pill_sized`: body 12 text plus its margin either side), to leave room for the pills after a label.
fn pill_w(ui: &Ui, text: &str) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), theme::body(12.0), Color32::WHITE).size().x + 2.0 * PILL_MARGIN.x
}

/// An epic name cut to `room` (the width left for the dot and the name) as `epic_label` lays it out; unchanged
/// when it fits, so a name is only cut when the row runs out of space.
fn fit_epic_name(ui: &Ui, name: &str, room: f32) -> String {
    let size = theme::spacing(ui.ctx()).caption;
    let measure = |t: &str| ui.painter().layout_no_wrap(t.to_string(), theme::body(size), Color32::WHITE).size().x;
    super::issues::fit_epic(name, room - c::EPIC_DOT - c::EPIC_DOT_GAP, measure)
}

/// The summary row's one line: "Summary · 3 ready · 2 need detail · 4 without a verdict".
pub fn summary_line(plan: &Promotion, unjudged: usize) -> String {
    let ready = plan.promoted.len() + plan.held_back.len();
    format!("Summary · {ready} ready · {} need detail · {unjudged} without a verdict", plan.needs_detail.len())
}

/// The summary's Promote button label.
pub fn promote_label(n: usize) -> String {
    format!("Promote {n}")
}

/// The `New` walk: the candidates (and the summary row) on the left, the focused ticket on the right.
fn pre_check_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    let n = app.pre_check_candidates().len();
    if n == 0 && app.grooming.promote_failures.is_empty() {
        empty_state(ui, p, &format!("Nothing in {}", app.cfg.grooming.status_new));
        return;
    }
    let cursor = app.grooming.cursor.min(n);
    let split = egui::Id::new("groom-pre-check-split");
    let avail = ui.available_width();
    let list_w = list_width(ui, split, avail * 0.42, avail);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PANE_GAP;
        let h = ui.available_height();
        let origin = ui.cursor().min;
        ui.allocate_ui_with_layout(vec2(list_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(list_w);
            candidate_list(app, ui, p, cursor);
        });
        list_splitter(ui, split, origin.x, list_w, origin.y, h);
        let right_w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(right_w);
            match app.pre_check_candidates().get(cursor).map(|c| c.issue.clone()) {
                Some(issue) => focused_ticket(app, ui, p, issue),
                None => summary_pane(app, ui, p),
            }
        });
    });
}

/// The candidates in rank order, each with its key, title, verdict (or report badge), epic, age and rank, then the
/// summary row.
fn candidate_list(app: &mut App, ui: &mut Ui, p: &Palette, cursor: usize) {
    let report = app.grooming.report.as_ref().map(|(path, r)| (report_date(path, r), r.clone()));
    let (plan, unjudged) = app.promotion_plan();
    let slots = app.epic_slots.clone();
    let epic_colors = app.cfg.issues.epic_colors.clone();
    let last_id = egui::Id::new("groom-last-cursor");
    let moved = ui.data(|d| d.get_temp::<usize>(last_id)) != Some(cursor);
    ui.data_mut(|d| d.insert_temp(last_id, cursor));
    let mut clicked: Option<usize> = None;
    crate::views::no_scroll_fade(ui);
    let out = egui::ScrollArea::vertical().id_salt("groom-list").auto_shrink([false, false]).show(ui, |ui| {
        // Rows stack as one list, as on Issues: neighbouring borders overlap into one divider.
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let cands = app.pre_check_candidates();
        let total = cands.len() + 1;
        let common_age = common_value(cands.iter().map(|c| c.age.as_str())).map(String::from);
        let common_age = common_age.as_deref();
        for (idx, c) in cands.iter().enumerate() {
            let i = c.issue;
            let sel = idx == cursor;
            let verdict = app.grooming.verdicts.get(&i.key);
            // The report's own words while the verdict is still the one it pre-filled.
            let badge = report.as_ref().and_then(|(d, r)| {
                let b = r.badge_for(&i.key)?;
                (verdict.is_none() || verdict == grooming::prefill(Some(r), &i.key).as_ref()).then(|| (b, d.clone()))
            });
            let resp = row_frame(ui, p, "groom-row", sel, true, idx, total, |ui| {
                title_line(ui, p, &i.key, &i.summary, sel, None);
                ui.add_space(line_gap(ui));
                // Epic, age and rank first, so they start at the same place on every row; the verdict (or the
                // report's badge) follows them.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = PILL_GAP;
                    let verdict_texts: Vec<String> = match (&badge, verdict) {
                        (Some((b, _)), _) => badge_pills(b),
                        (None, Some(v)) => verdict_pills(v).into_iter().map(String::from).collect(),
                        (None, None) => Vec::new(),
                    };
                    let age = (common_age != Some(c.age.as_str())).then_some(c.age.as_str());
                    let rank = format!("rank {}", c.rank_pos);
                    if let Some(k) = &i.parent_key {
                        let t = theme::epic_tint_in(k, &slots, p, &epic_colors);
                        let name = i.parent_summary.clone().unwrap_or_else(|| k.clone());
                        // Cut only to the room the age, rank and verdict pills leave.
                        let after: f32 =
                            age.into_iter().chain([rank.as_str()]).chain(verdict_texts.iter().map(String::as_str)).map(|t| pill_w(ui, t) + PILL_GAP).sum();
                        let text = fit_epic_name(ui, &name, ui.available_width() - after);
                        let on = if sel { p.selection() } else { p.surface };
                        crate::views::tooltip(c::epic_label(ui, p, &text, t, on), super::issues::epic_tip(k, &name));
                    }
                    if let Some(a) = age {
                        neutral_pill(ui, p, a, sel);
                    }
                    neutral_pill(ui, p, &rank, sel);
                    match (&badge, verdict) {
                        (Some((b, d)), _) => self::badge(ui, p, b, d),
                        (None, Some(v)) => {
                            for t in verdict_pills(v) {
                                crate::views::tooltip(c::status_pill(ui, t, verdict_tint(*v == Verdict::Ready, p)), verdict_tip(v));
                            }
                        }
                        (None, None) => {}
                    }
                });
            });
            if resp.clicked() {
                clicked = Some(idx);
            }
            if sel && moved {
                resp.scroll_to_me(None);
            }
        }
        let idx = cands.len();
        let sel = cursor == idx;
        let resp = row_frame(ui, p, "groom-row", sel, true, idx, total, |ui| {
            let g = ui.painter().layout_no_wrap(summary_line(&plan, unjudged), theme::bold(13.5), p.text);
            ui.label(g);
            ui.add_space(line_gap(ui));
            ui.label(RichText::new("Enter opens the promotion plan").color(p.muted).size(12.0));
        });
        if resp.clicked() {
            clicked = Some(idx);
        }
        if sel && moved {
            resp.scroll_to_me(None);
        }
        selection_ring(ui, p);
    });
    crate::views::components::scroll_edge_fade(ui, &out, p.bg);
    if let Some(i) = clicked {
        app.actions.push(Action::GroomMove(i as i32 - cursor as i32));
    }
}

/// The Proposed list's row salt.
const PROPOSED_SALT: &str = "groom-proposed";
/// The New list's row salt.
const NEW_SALT: &str = "groom-new";
/// The Unclaimed list's row salt.
const UNCLAIMED_SALT: &str = "groom-unclaimed";

/// A list row's widget id: unique per list (salt) and row.
fn row_id(salt: &str, idx: usize) -> egui::Id {
    egui::Id::new((salt, idx))
}

/// Where `row_frame` leaves this list's selected row for `selection_ring`.
fn ring_id() -> egui::Id {
    egui::Id::new("groom-selected-row")
}

/// A list row: the shared selection (`selection` fill, accent ring) under the cursor, the hover fill otherwise.
/// `focused`: whether this list is the one the keys move in; only it rings its selected row, so a list that only
/// marks what is current (the rail's epic beside the ticket list) shows the fill alone. Rows overlap by a pixel, so
/// the ring is drawn last by `selection_ring`; returns the row's click response.
#[allow(clippy::too_many_arguments)]
fn row_frame(ui: &mut Ui, p: &Palette, salt: &str, sel: bool, focused: bool, idx: usize, n: usize, content: impl FnOnce(&mut Ui)) -> egui::Response {
    let corners = super::issues::row_corners(idx, n);
    let bg = ui.painter().add(egui::Shape::Noop);
    let r = Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(corners).inner_margin(row_pad(ui)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        content(ui);
    });
    let rect = r.response.rect;
    let resp = ui.interact(rect, row_id(salt, idx), egui::Sense::click());
    let fill = if sel { p.selection() } else { super::issues::row_fill(false, resp.hovered(), p) };
    ui.painter().set(bg, egui::epaint::RectShape::filled(rect, corners, fill));
    if sel && focused {
        ui.data_mut(|d| d.insert_temp(ring_id(), rect));
    }
    resp
}

/// The selected row's accent ring (`Palette::selection_edge`), over the neighbouring rows'
/// dividers. Call once a list's rows are drawn.
fn selection_ring(ui: &mut Ui, p: &Palette) {
    let rect = ui.data_mut(|d| {
        let r = d.get_temp::<egui::Rect>(ring_id());
        d.remove::<egui::Rect>(ring_id());
        r
    });
    if let Some(rect) = rect {
        ui.painter().rect_stroke(rect, f32::from(c::ROW_RADIUS), Stroke::new(1.0, p.selection_edge()), egui::StrokeKind::Inside);
    }
}

/// What sits at the right of a row's title line.
enum Tail {
    /// The focus timer: seconds on the ticket and its colour.
    Timer(u64, Color32),
    /// A status pill (Unclaimed rows) after the row's neutral pills (age, "Unassigned"), all grouped at the right,
    /// so the row stays one line like the others.
    Status(String, theme::Tint, Vec<String>),
}

/// Key (mono, muted) and title (bold), the title cut to the row; `tail` (the focus timer or a status) sits at the right.
fn title_line(ui: &mut Ui, p: &Palette, key: &str, summary: &str, sel: bool, tail: Option<&Tail>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(key).font(theme::mono(12.5)).color(super::issues::row_key_color(sel, p)));
        let painter = ui.painter().clone();
        let tail_g = match tail {
            Some(Tail::Timer(secs, color)) => Some(timer_galleys(ui, p, *secs, *color, sel)),
            _ => None,
        };
        let status = match tail {
            Some(Tail::Status(t, tint, extra)) => Some((t.as_str(), *tint, extra.as_slice())),
            _ => None,
        };
        let tail_w = tail_g.as_ref().map_or(0.0, |(l, t)| l.size().x + TIMER_GAP + t.size().x + ui.spacing().item_spacing.x)
            + status.map_or(0.0, |(t, _, extra)| pill_w(ui, t) + ui.spacing().item_spacing.x + extra.iter().map(|e| pill_w(ui, e) + PILL_GAP).sum::<f32>());
        let title =
            crate::views::truncate_title(summary, ui.available_width() - tail_w, |t| painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x);
        let r = ui.label(RichText::new(&title).font(theme::bold(13.5)).color(p.text));
        if title != summary {
            crate::views::tooltip(r, summary.to_string());
        }
        if let Some((label, time)) = tail_g {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = TIMER_GAP;
                let r = ui.label(time);
                let r = r.union(ui.label(label));
                crate::views::tooltip(r, TIMER_TIP);
            });
        }
        if let Some((t, tint, extra)) = status {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = PILL_GAP;
                c::status_pill(ui, t, tint);
                for e in extra.iter().rev() {
                    neutral_pill(ui, p, e, sel);
                }
            });
        }
    });
}

/// The focus timer's label and time: "on this ticket" in caption `muted`, then the time in mono.
fn timer_galleys(ui: &Ui, p: &Palette, secs: u64, color: Color32, sel: bool) -> (std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>) {
    let muted = if sel { theme::muted_on(p.selection(), p) } else { p.muted };
    let size = theme::spacing(ui.ctx()).caption;
    let label = ui.painter().layout_no_wrap(TIMER_LABEL.to_string(), theme::body(size), muted);
    let time = ui.painter().layout_no_wrap(grooming::fmt_elapsed(secs), theme::mono(12.5), color);
    (label, time)
}

/// The focus timer's words, before the time.
pub const TIMER_LABEL: &str = "Time on ticket";
/// The focus timer's tooltip.
const TIMER_TIP: &str = "Time on this ticket: amber at 3 minutes, red at 5. Past 5, bounce it.";
/// Between the timer's label and its time.
const TIMER_GAP: f32 = 6.0;

fn neutral_pill(ui: &mut Ui, p: &Palette, text: &str, sel: bool) {
    if sel {
        crate::views::outline_pill_selected(ui, text, p, PILL_MARGIN);
    } else {
        c::neutral_pill(ui, p, text);
    }
}

/// The value most rows share, when more than half do: a pill showing it would sit on nearly every row and tell
/// the reader nothing, so only the rows that differ show theirs ("older", "Unassigned").
pub fn common_value<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let values: Vec<&str> = values.into_iter().collect();
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for v in &values {
        match counts.iter_mut().find(|(k, _)| k == v) {
            Some((_, n)) => *n += 1,
            None => counts.push((v, 1)),
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).filter(|(_, n)| *n * 2 > values.len()).map(|(k, _)| k)
}

/// A verdict's status tint: green when ready, amber when it needs detail. Filled, unlike the outlined category
/// pills (epic, age, rank), so a judged row reads apart at a glance.
fn verdict_tint(ready: bool, p: &Palette) -> theme::Tint {
    theme::tint_from_hue(if ready { p.on_track } else { p.at_risk }, p)
}

/// The focused ticket: key and epic, title, status, and the rendered description (its full detail is fetched).
fn focused_ticket(app: &mut App, ui: &mut Ui, p: &Palette, issue: jira::Issue) {
    let loading = app.detail_loading.contains(&issue.key);
    // Once per focus: a failed fetch is not retried every frame.
    if app.grooming.detail_for.as_deref() != Some(issue.key.as_str()) {
        app.actions.push(Action::GroomFocusDetail(issue.key.clone()));
    }
    let g = &app.cfg.grooming;
    rubric_card(ui, p, app.grooming.mode, &g.status_new, &g.status_planning, &g.status_ready, &app.cfg.scope.team_label);
    ui.add_space(PANE_GAP - ui.spacing().item_spacing.y);
    card(ui, p, |ui| {
        egui::ScrollArea::vertical().id_salt("groom-detail").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
                if let (Some(k), Some(s)) = (&issue.parent_key, &issue.parent_summary) {
                    let t = theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                    ui.label(RichText::new("·").color(p.muted));
                    crate::views::tooltip(c::epic_label(ui, p, s, t, p.surface), super::issues::epic_tip(k, s));
                }
                if loading {
                    crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                }
                // The focused row's one Move to Done (what `D` acts on), at the header's right end.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(a) = c::done_button(ui, p, &issue, p.surface) {
                        app.actions.push(a);
                    }
                });
            });
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text)).wrap());
            ui.add_space(8.0);
            let st = theme::status_tint(&issue.status.name, &issue.status.category, p, &app.cfg.issues.status_colors);
            c::status_pill(ui, &issue.status.name, st);
            description_header(ui, p, issue.description.as_ref());
            match &issue.description {
                Some(d) => super::adf_view::show_value(ui, d, p, p.surface),
                None if loading => {}
                None => super::adf_view::show_value(ui, &serde_json::Value::Null, p, p.surface),
            }
        });
    });
}

/// The "Description" pane label, as on the Issues detail (`components::pane_label`): `section_gap` from the status
/// pill to its ink, then the detail pane's one label-to-content gap (`detail::label_to_content`) to the
/// description's first ink, whatever block comes first.
fn description_header(ui: &mut Ui, p: &Palette, description: Option<&serde_json::Value>) {
    let sp = *theme::spacing(ui.ctx());
    let item = ui.spacing().item_spacing.y;
    let g = ui.painter().layout_no_wrap("Description".to_string(), c::pane_label_font(ui), p.muted);
    let ink = theme::ink_rect(&g);
    ui.add_space(super::detail::section_gap_space(sp.section_gap, 0.0, ink.top(), item));
    c::pane_label(ui, p, "Description", None);
    let doc = description.unwrap_or(&serde_json::Value::Null);
    let first = super::adf_view::first_ink_top(ui.ctx(), doc);
    ui.add_space(super::detail::label_to_content(&sp) - (g.size().y - ink.bottom()) - first - item);
}

/// The right pane on the summary row: the plan in brief, and the last Promote's failed transitions.
fn summary_pane(app: &mut App, ui: &mut Ui, p: &Palette) {
    let (plan, unjudged) = app.promotion_plan();
    let cap = app.cfg.grooming.planning_cap;
    let failures = app.grooming.promote_failures.clone();
    let promoting = app.grooming.promoting;
    let cands = app.pre_check_candidates();
    let judged = cands.iter().filter(|c| app.grooming.verdicts.contains_key(&c.issue.key)).count();
    let cue = guide::all_judged(judged, cands.len());
    card(ui, p, |ui| {
        ui.label(RichText::new("Pre-check summary").font(theme::bold(15.0)).color(p.text));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(cue).color(p.muted).size(14.0)).wrap());
        ui.add_space(12.0);
        for line in plan_lines(&plan, unjudged, cap) {
            ui.label(RichText::new(line).color(p.text).size(14.0));
        }
        if !failures.is_empty() {
            ui.add_space(16.0);
            ui.label(RichText::new("Failed transitions").font(theme::bold(12.0)).color(p.muted));
            ui.add_space(8.0);
            for (k, s, e) in &failures {
                ui.label(RichText::new(format!("{k} {s}: {e}")).color(p.off_track).size(12.0));
            }
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.add(promote_button("Review and promote", !promoting, p)).clicked() && !promoting {
                app.actions.push(Action::OpenSummary);
            }
            if promoting {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            }
        });
    });
}

/// The plan as counted lines.
pub fn plan_lines(plan: &Promotion, unjudged: usize, cap: usize) -> Vec<String> {
    vec![
        format!("{} to promote to Planning", plan.promoted.len()),
        format!("{} ready but held back (cap {cap})", plan.held_back.len()),
        format!("{} need detail", plan.needs_detail.len()),
        format!("{unjudged} without a verdict"),
    ]
}

/// A `surface` card (`components::card_frame`, the density's `card_pad`), filling the pane's height.
fn card(ui: &mut Ui, p: &Palette, content: impl FnOnce(&mut Ui)) {
    card_frame(ui, p, true, content);
}

/// The card's frame; `fill` stretches it to the pane's bottom, otherwise it hugs its content.
fn card_frame(ui: &mut Ui, p: &Palette, fill: bool, content: impl FnOnce(&mut Ui)) {
    c::card_frame(ui, p, false).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if fill {
            ui.set_min_height(ui.available_height() - 1.0);
        }
        content(ui);
    });
}

/// From the legend's key column to its text.
const LEGEND_KEY_GAP: f32 = 8.0;

/// The mode's rubric over the focused ticket: the title, then a two-column legend (every key in a key chip, in
/// one fixed-width column so the text column is straight), the lead line above it and the closing note a
/// paragraph below.
fn rubric_card(ui: &mut Ui, p: &Palette, mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str, team_label: &str) {
    let sp = *theme::spacing(ui.ctx());
    card_frame(ui, p, false, |ui| {
        ui.label(RichText::new(guide::rubric_title(mode)).font(theme::bold(sp.h3)).color(p.text));
        ui.add_space(sp.paragraph_gap - ui.spacing().item_spacing.y);
        let lines = guide::rubric_lines(mode, status_new, status_planning, status_ready, team_label);
        let keys: Vec<Option<std::sync::Arc<egui::Galley>>> =
            lines.iter().map(|(k, _)| (!k.is_empty()).then(|| ui.painter().layout_job(super::issues::key_job(k, p.text)))).collect();
        let key_col = keys.iter().flatten().map(|g| c::key_chip_rect(theme::ink_rect(g)).width()).fold(0.0, f32::max).ceil();
        ui.spacing_mut().item_spacing.y = sp.line_gap + 2.0 * c::KEY_CHIP_PAD.y;
        let font = theme::body(sp.small);
        let mut keyed_before = false;
        for ((_, text), key) in lines.into_iter().zip(keys) {
            match key {
                None => {
                    // A note: the lead (before any key) or the closing line, set a paragraph apart from the keys.
                    if keyed_before {
                        ui.add_space(sp.paragraph_gap - ui.spacing().item_spacing.y);
                    }
                    ui.add(egui::Label::new(RichText::new(text).font(font.clone()).color(p.muted)).wrap());
                    if !keyed_before {
                        ui.add_space(sp.paragraph_gap - ui.spacing().item_spacing.y);
                    }
                }
                Some(key) => {
                    keyed_before = true;
                    let wrap = (ui.available_width() - key_col - LEGEND_KEY_GAP).max(0.0);
                    let text_g = ui.painter().layout(text, font.clone(), p.text, wrap);
                    let h = text_g.size().y;
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let line = rect.top() + crate::views::galley_baseline(&text_g);
                        let ink = theme::ink_rect(&key);
                        let key_pos =
                            egui::pos2(rect.left() + c::KEY_CHIP_PAD.x - ink.left(), crate::views::top_for_baseline(line, crate::views::galley_baseline(&key)));
                        c::paint_key_chip(ui.painter(), key_pos, &key, p.surface, p);
                        ui.painter().galley(key_pos, key, p.text);
                        ui.painter().galley(egui::pos2(rect.left() + key_col + LEGEND_KEY_GAP, rect.top()), text_g, p.text);
                    }
                }
            }
        }
    });
}

/// The dialogs over the Grooming screen: the overwrite confirm, else the summary.
fn dialogs(app: &mut App, ctx: &egui::Context, p: &Palette) {
    if app.help_open {
        return;
    }
    let hint_bar = crate::views::hint_bar_height(app);
    if app.grooming.pending_report.is_some() {
        crate::views::backdrop(ctx, "groom-backdrop", egui::Order::Middle, hint_bar, p);
        let path = app.cfg.report_dir().join(format!("{}.md", chrono::Local::now().date_naive().format("%Y-%m-%d")));
        dialog(ctx, p, "groom-overwrite", 400.0, |ui| {
            dialog_title(ui, p, "Report for today exists. Overwrite?", None);
            ui.label(RichText::new(path.display().to_string()).font(theme::mono(12.5)).color(crate::views::dialog_muted(p)));
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(primary_button("Overwrite", p)).clicked() {
                    app.actions.push(Action::Overwrite(true));
                }
                if ui.add(crate::views::button("Keep the old one", p)).clicked() {
                    app.actions.push(Action::Overwrite(false));
                }
            });
        });
        return;
    }
    if !app.grooming.summary_open {
        return;
    }
    if crate::views::backdrop(ctx, "groom-backdrop", egui::Order::Middle, hint_bar, p).clicked() {
        app.actions.push(Action::CloseSummary);
    }
    let (plan, unjudged) = app.promotion_plan();
    let cap = app.cfg.grooming.planning_cap;
    let titles: std::collections::HashMap<String, String> = app.pre_check_candidates().iter().map(|c| (c.issue.key.clone(), c.issue.summary.clone())).collect();
    let writable = jira::check_writable().is_ok();
    let promoting = app.grooming.promoting;
    let can = promote_enabled(plan.promoted.len(), writable, promoting);
    let blocked = promote_blocked(plan.promoted.len(), writable, promoting);
    let max_h = (ctx.content_rect().height() * 0.5).max(160.0);
    let g = &app.cfg.grooming;
    let path = guide::report_path(&g.report_dir, chrono::Local::now().date_naive());
    let explainer = guide::promote_explainer(plan.promoted.len(), plan.held_back.len(), cap, &g.status_planning);
    let untouched = guide::untouched_line(unjudged, &g.status_new);
    dialog(ctx, p, "groom-summary", SUMMARY_W, |ui| {
        let sp = *theme::spacing(ui.ctx());
        let muted = crate::views::dialog_muted(p);
        // The read-only (or busy) note sits under the title, where every picker puts it.
        dialog_title(ui, p, "Promote to Planning", blocked);
        // One measure for the whole dialog: the intro wraps at the width the rows below are cut to.
        ui.add(egui::Label::new(RichText::new(explainer).color(p.text).size(sp.body)).wrap());
        // The report's path on its own line, so it never forces an early break in the sentence above.
        ui.label(RichText::new(path.as_str()).font(theme::mono(12.5)).color(muted));
        ui.add_space(sp.paragraph_gap - ui.spacing().item_spacing.y);
        ui.add(egui::Label::new(RichText::new(untouched).color(muted).size(sp.body)).wrap());
        egui::ScrollArea::vertical().id_salt("groom-summary-rows").max_height(max_h).show(ui, |ui| {
            let line = |ui: &mut Ui, key: &str, tail: &str| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(key).font(theme::mono(12.5)).color(muted));
                    let tail_w = if tail.is_empty() {
                        0.0
                    } else {
                        ui.painter().layout_no_wrap(tail.to_string(), theme::body(sp.small), muted).size().x + ui.spacing().item_spacing.x
                    };
                    let title = titles.get(key).map(String::as_str).unwrap_or_default();
                    let painter = ui.painter().clone();
                    let cut = crate::views::truncate_title(title, ui.available_width() - tail_w, |t| {
                        painter.layout_no_wrap(t.to_string(), theme::body(sp.body), p.text).size().x
                    });
                    let r = ui.label(RichText::new(&cut).color(p.text).size(sp.body));
                    if cut != title {
                        crate::views::tooltip(r, title.to_string());
                    }
                    if !tail.is_empty() {
                        ui.label(RichText::new(tail).color(muted).size(sp.small));
                    }
                });
            };
            section(ui, p, "Promote", plan.promoted.len());
            for k in &plan.promoted {
                line(ui, k, "");
            }
            if !plan.held_back.is_empty() {
                section(ui, p, &format!("Held back, past the cap of {cap}"), plan.held_back.len());
                for k in &plan.held_back {
                    line(ui, k, "");
                }
            }
            if !plan.needs_detail.is_empty() {
                section(ui, p, "Needs detail, stays in New", plan.needs_detail.len());
                for (k, codes) in &plan.needs_detail {
                    line(ui, k, &guide::shown_codes(codes).into_iter().map(guide::code_words).collect::<Vec<_>>().join(", "));
                }
            }
        });
        ui.add_space(sp.section_gap / 2.0 - ui.spacing().item_spacing.y);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(promote_button(&promote_label(plan.promoted.len()), can, p)).clicked() && can {
                app.actions.push(Action::Promote);
            }
            if ui.add(crate::views::button("Cancel", p)).clicked() {
                app.actions.push(Action::CloseSummary);
            }
            if promoting {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            }
        });
    });
}

/// A Promote primary: live, or the shared disabled primary (`components::primary_button_off`) while it is off.
fn promote_button(text: &str, enabled: bool, p: &Palette) -> egui::Button<'static> {
    if enabled { primary_button(text, p) } else { c::primary_button_off(text, p) }
}

/// The Promote dialog's width.
const SUMMARY_W: f32 = 560.0;

/// "Promote N" is live only with something to promote, writes allowed, and no Promote running.
pub fn promote_enabled(n: usize, writable: bool, promoting: bool) -> bool {
    n > 0 && writable && !promoting
}

/// Why "Promote N" is off, shown under the dialog's title; `None` when it is live. The read-only note breaks like
/// the pickers' (`pickers::read_only_note`).
pub fn promote_blocked(n: usize, writable: bool, promoting: bool) -> Option<&'static str> {
    if promoting {
        Some("Promoting now. The dialog closes when every move is done.")
    } else if !writable {
        Some("Read-only mode is on, so Promote is off.\nRestart without JIRA_TICKETS_READONLY to promote.")
    } else if n == 0 {
        Some("Nothing is ready to promote. Mark tickets ready with r first.")
    } else {
        None
    }
}

/// A list header inside the dialog: the shared section header, a paragraph below what came before.
fn section(ui: &mut Ui, p: &Palette, title: &str, n: usize) {
    let sp = theme::spacing(ui.ctx());
    ui.add_space(sp.paragraph_gap * 1.5 - ui.spacing().item_spacing.y);
    c::section_header(ui, p, title, Some(n));
    ui.add_space(sp.line_gap * 2.0 - ui.spacing().item_spacing.y);
}

/// The dialog title: `pickers::title`, the one every picker uses. With a `note` (read-only, or why the action is
/// off), the note sits under the title as in every picker (`pickers::title_with_note`), then a paragraph gap.
fn dialog_title(ui: &mut Ui, p: &Palette, text: &str, note: Option<&str>) {
    if note.is_none() {
        return super::pickers::title(ui, p, text);
    }
    super::pickers::title_with_note(ui, p, text, note);
    ui.add_space(theme::spacing(ui.ctx()).paragraph_gap - ui.spacing().item_spacing.y);
}

/// A dialog in the shared modal frame (`components::dialog_frame`) and the pickers' margin, a fifth down the window.
fn dialog(ctx: &egui::Context, p: &Palette, id: &str, width: f32, content: impl FnOnce(&mut Ui)) {
    let top = super::pickers::dialog_top(ctx.content_rect().height());
    egui::Area::new(egui::Id::new(id)).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        c::dialog_frame(p).inner_margin(super::pickers::DIALOG_MARGIN).show(ui, |ui| {
            ui.set_width(width);
            content(ui);
        });
    });
}

/// The rail's width.
const RAIL_W: f32 = 300.0;
/// The bounce box holds at most this many lines ("one or two lines"); it grows from one to this as the text wraps.
const BOUNCE_LINES: usize = 2;
/// The bounce box's placeholder.
pub const BOUNCE_HINT: &str = "What is missing? One or two lines (⌘Enter to post)";
/// The placeholder in read-only mode, where ⌘Enter posts nothing.
const BOUNCE_HINT_READ_ONLY: &str = "What is missing? One or two lines";

/// The bounce box's placeholder: the read-only one names no key that would post.
pub fn bounce_hint(writable: bool) -> &'static str {
    if writable { BOUNCE_HINT } else { BOUNCE_HINT_READ_ONLY }
}

/// The rail row's counts: tickets in the active sprint, the next sprint, Ready for Work, Proposed, New and Unclaimed.
pub struct RailCounts {
    pub sprint: usize,
    pub next: usize,
    pub ready: usize,
    pub planning: usize,
    pub new: usize,
    pub unclaimed: usize,
}

/// One epic on the session rail, taken from `grooming::epic_queues`.
struct RailRow {
    epic: Option<String>,
    summary: String,
    stage: grooming::Stage,
    counts: RailCounts,
}

/// What the centre column draws for the selected epic, cloned out of the store so the app can be borrowed mutably.
struct QueueView {
    active: Option<String>,
    next: Option<String>,
    sprint: Vec<jira::Issue>,
    next_sprint: Vec<jira::Issue>,
    ready: Vec<jira::Issue>,
    planning: Vec<jira::Issue>,
    new: Vec<jira::Issue>,
    unclaimed: Vec<jira::Issue>,
}

/// The epic-by-epic walk: the stage rail, the selected epic's six blocks (sprint, next sprint, Ready for Work,
/// Proposed, New) and the focused ticket's description.
fn session_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    // The per-ticket timer ticks.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    let (active, next) = grooming::sprint_names(&app.sprints);
    let (active, next) = (active.map(String::from), next.map(String::from));
    let queues = app.session_queues();
    let rail: Vec<RailRow> = queues
        .iter()
        .map(|q| RailRow {
            epic: q.epic.clone(),
            summary: q.summary.clone(),
            stage: q.stage.stage,
            counts: RailCounts {
                sprint: q.sprint.len(),
                next: q.next.len(),
                ready: q.ready.len(),
                planning: q.planning.len(),
                new: q.new.len(),
                unclaimed: q.unclaimed.len(),
            },
        })
        .collect();
    let at = app.grooming.epic_index.min(rail.len().saturating_sub(1));
    let clone_all = |v: &[&jira::Issue]| v.iter().map(|i| (*i).clone()).collect::<Vec<_>>();
    let view = queues.get(at).map(|q| QueueView {
        active: active.clone(),
        next: next.clone(),
        sprint: clone_all(&q.sprint),
        next_sprint: clone_all(&q.next),
        ready: clone_all(&q.ready),
        planning: clone_all(&q.planning),
        new: clone_all(&q.new),
        unclaimed: clone_all(&q.unclaimed),
    });
    drop(queues);
    if rail.is_empty() {
        let g = &app.cfg.grooming;
        empty_state(ui, p, &format!("Nothing in {}, {} or {}", g.status_planning, g.status_ready, g.status_new));
        return;
    }
    let view = view.expect("a non-empty rail has a selected epic");
    let rows = view.sprint.len() + view.next_sprint.len() + view.ready.len() + view.planning.len() + view.new.len() + view.unclaimed.len();
    let cursor = app.grooming.cursor.min(rows.saturating_sub(1));
    let focused: Option<jira::Issue> =
        view.sprint.iter().chain(&view.next_sprint).chain(&view.ready).chain(&view.planning).chain(&view.new).chain(&view.unclaimed).nth(cursor).cloned();
    // An epic with Ready rows only has nothing to groom, whatever the cursor is on.
    let nothing_to_groom = view.planning.is_empty() && view.new.is_empty() && view.unclaimed.is_empty();
    let split = egui::Id::new("groom-session-split");
    let avail = ui.available_width();
    // The default share is of the whole body, as before; the drag range is what the rail leaves.
    let list_w = list_width(ui, split, avail * 0.36, (avail - RAIL_W - PANE_GAP).max(0.0));
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PANE_GAP;
        let h = ui.available_height();
        ui.allocate_ui_with_layout(vec2(RAIL_W, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(RAIL_W);
            epic_rail(app, ui, p, &rail, at, active.as_deref(), next.as_deref(), rail_focused(rows));
        });
        let origin = ui.cursor().min;
        ui.allocate_ui_with_layout(vec2(list_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(list_w);
            epic_queue(app, ui, p, &view, cursor);
        });
        list_splitter(ui, split, origin.x, list_w, origin.y, h);
        let right_w = ui.available_width();
        let header_room = lane_header_room(ui, rail[0].stage.label());
        ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(right_w);
            // The cards start on the lists' first row, under the columns' headers.
            ui.add_space(header_room);
            match focused {
                Some(issue) if !nothing_to_groom => focused_ticket(app, ui, p, issue),
                _ => card(ui, p, |ui| {
                    ui.add(egui::Label::new(RichText::new(guide::nothing_to_groom(&app.cfg.grooming.status_new)).color(p.muted).size(14.0)).wrap());
                }),
            }
        });
    });
}

/// The rail row's count: the tickets the walk decides on here (Proposed, New and Unclaimed), "7 to groom", or
/// nothing when there are none. The other counts are in the row's tooltip (`rail_tip`).
pub fn rail_count(c: &RailCounts) -> Option<String> {
    let n = c.planning + c.new + c.unclaimed;
    (n > 0).then(|| format!("{n} to groom"))
}

/// The rail row's tooltip: the epic's key and name, then every count spelt out.
fn rail_tip(epic: Option<&str>, summary: &str, c: &RailCounts, active: Option<&str>, next: Option<&str>, status_ready: &str, status_new: &str) -> String {
    let name = match epic {
        Some(k) if k != summary => format!("{k} {summary}"),
        _ => summary.to_string(),
    };
    let mut parts = Vec::new();
    if let Some(a) = active {
        parts.push(format!("{} in {a}", c.sprint));
    }
    if let Some(n) = next {
        parts.push(format!("{} in {n}", c.next));
    }
    parts.push(format!("{} {status_ready}", c.ready));
    parts.push(format!("{} Proposed", c.planning));
    parts.push(format!("{} {status_new}", c.new));
    if c.unclaimed > 0 {
        parts.push(format!("{} unclaimed", c.unclaimed));
    }
    format!("{name}\n{}", parts.join(" · "))
}

/// The dot for the "(no epic)" row: a neutral grey, so it lines up with the epics' dots.
fn no_epic_tint(p: &Palette) -> theme::Tint {
    theme::Tint { bg: Color32::TRANSPARENT, fg: p.muted, stripe: p.no_health }
}

/// The epics under their stage headers: each a neutral name with its epic dot, and how many tickets it has to
/// groom at the right. `]`/`[` step over the ones with none.
/// The rail rings its current epic only when the epic has no rows for j/k to move through; otherwise the ticket list
/// holds the one focus ring and the rail's epic keeps the selection fill alone, as a quiet "current" marker.
pub fn rail_focused(queue_rows: usize) -> bool {
    queue_rows == 0
}

#[allow(clippy::too_many_arguments)]
fn epic_rail(app: &mut App, ui: &mut Ui, p: &Palette, rail: &[RailRow], at: usize, active: Option<&str>, next: Option<&str>, focused: bool) {
    let mut clicked = None;
    let g = app.cfg.grooming.clone();
    crate::views::no_scroll_fade(ui);
    let out = egui::ScrollArea::vertical().id_salt("groom-rail").auto_shrink([false, false]).show(ui, |ui| {
        let mut last_stage = None;
        for (idx, r) in rail.iter().enumerate() {
            if last_stage != Some(r.stage) {
                lane_header(ui, p, r.stage.label(), None, last_stage.is_none());
                last_stage = Some(r.stage);
            }
            let sel = idx == at;
            let tint = match &r.epic {
                Some(k) => theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors),
                None => no_epic_tint(p),
            };
            let resp = row_frame(ui, p, "groom-rail", sel, focused, idx, rail.len(), |ui| {
                let on = if sel { p.selection() } else { p.surface };
                let muted = theme::muted_on(on, p);
                let size = theme::spacing(ui.ctx()).caption;
                let count = rail_count(&r.counts).map(|t| ui.painter().layout_no_wrap(t, theme::body(size), muted));
                ui.horizontal(|ui| {
                    let count_w = count.as_ref().map_or(0.0, |g| g.size().x + ui.spacing().item_spacing.x);
                    let name = fit_epic_name(ui, &r.summary, ui.available_width() - count_w);
                    c::epic_label(ui, p, &name, tint, on);
                    if let Some(g) = count {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(g));
                    }
                });
            });
            let resp = crate::views::tooltip(resp, rail_tip(r.epic.as_deref(), &r.summary, &r.counts, active, next, &g.status_ready, &g.status_new));
            if resp.clicked() {
                clicked = Some(idx);
            }
        }
        selection_ring(ui, p);
    });
    crate::views::components::scroll_edge_fade(ui, &out, p.bg);
    if let Some(i) = clicked {
        app.actions.push(Action::GroomEpic { delta: i as i32 - at as i32, skip_empty: false });
    }
}

/// How far a column's first row sits below its top: the first lane header (`lane_header`) and the gap under it.
fn lane_header_room(ui: &Ui, name: &str) -> f32 {
    let sp = theme::spacing(ui.ctx());
    let title = ui.painter().layout_no_wrap(name.to_string(), theme::bold(sp.h3), Color32::WHITE).size().y;
    let count = ui.painter().layout_no_wrap("0".to_string(), theme::body(sp.h3), Color32::WHITE).size().y;
    title.max(count) + sp.line_gap * 2.0
}

/// The one lane (and rail stage) header: the shared section header, `section_gap` above it unless it is the
/// column's first, `line_gap` below. Leaves the rows' overlap set for the list that follows.
fn lane_header(ui: &mut Ui, p: &Palette, name: &str, n: Option<usize>, first: bool) {
    let sp = *theme::spacing(ui.ctx());
    let item = ui.style().spacing.item_spacing.y;
    ui.spacing_mut().item_spacing.y = item;
    if !first {
        ui.add_space(sp.section_gap - item);
    }
    c::section_header(ui, p, name, n);
    ui.add_space(sp.line_gap * 2.0 - item);
    ui.spacing_mut().item_spacing.y = ROW_GAP;
}

/// The centre column's lanes as they are drawn: a lane with rows gets its header; a run of empty lanes waits and
/// becomes one quiet line (`guide::empty_lanes_line`) before the next lane with rows, or at the end.
#[derive(Default)]
struct Lanes {
    /// Something is drawn already, so the next block takes `section_gap` above it.
    drawn: bool,
    empty: Vec<String>,
}

impl Lanes {
    /// A lane of `n` rows: its header when it has rows, else it joins the pending empty run.
    fn lane(&mut self, ui: &mut Ui, p: &Palette, name: &str, n: usize) {
        if n == 0 {
            self.empty.push(name.to_string());
            return;
        }
        self.flush(ui, p);
        lane_header(ui, p, name, Some(n), !self.drawn);
        self.drawn = true;
    }

    /// The pending empty lanes as one line.
    fn flush(&mut self, ui: &mut Ui, p: &Palette) {
        if self.empty.is_empty() {
            return;
        }
        let sp = *theme::spacing(ui.ctx());
        let item = ui.style().spacing.item_spacing.y;
        ui.spacing_mut().item_spacing.y = item;
        if self.drawn {
            ui.add_space(sp.section_gap - item);
        }
        let names: Vec<&str> = self.empty.iter().map(String::as_str).collect();
        c::empty_state(ui, p, &guide::empty_lanes_line(&names), None);
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        self.empty.clear();
        self.drawn = true;
    }
}

/// A sprint lane: the sprint's name over its rows (or a place in the empty run). Returns the index (within
/// `rows`) clicked this frame.
#[allow(clippy::too_many_arguments)]
fn sprint_block(
    ui: &mut Ui,
    p: &Palette,
    lanes: &mut Lanes,
    name: &str,
    salt: &str,
    rows: &[jira::Issue],
    offset: usize,
    cursor: usize,
    moved: bool,
) -> Option<usize> {
    let mut clicked = None;
    lanes.lane(ui, p, name, rows.len());
    for (idx, i) in rows.iter().enumerate() {
        let sel = offset + idx == cursor;
        let resp = ticket_row(ui, p, salt, i, sel, idx, rows.len(), None, &[]);
        if resp.clicked() {
            clicked = Some(idx);
        }
        if sel && moved {
            resp.scroll_to_me(None);
        }
    }
    clicked
}

/// One pill on a ticket row's pill line.
enum RowPill {
    /// What the last report said (`badge`), with the report's date.
    Badge(Badge, String),
    /// Age or "Unassigned": outlined.
    Neutral(String),
    /// The row's assignee: their avatar, then the outlined name.
    Person(String),
}

/// The one ticket row in the session column: key and bold title, the focus timer or status at the right (`tail`),
/// then a pill line only when there are pills. Context lanes (sprints, Ready for Work) have none: nothing is
/// decided there. Proposed and New rows show only the pills that set them apart.
#[allow(clippy::too_many_arguments)]
fn ticket_row(ui: &mut Ui, p: &Palette, salt: &str, i: &jira::Issue, sel: bool, idx: usize, n: usize, tail: Option<Tail>, pills: &[RowPill]) -> egui::Response {
    row_frame(ui, p, salt, sel, true, idx, n, |ui| {
        title_line(ui, p, &i.key, &i.summary, sel, tail.as_ref());
        if pills.is_empty() {
            return;
        }
        ui.add_space(line_gap(ui));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            for pill in pills {
                match pill {
                    RowPill::Badge(b, d) => badge(ui, p, b, d),
                    RowPill::Neutral(t) => neutral_pill(ui, p, t, sel),
                    RowPill::Person(t) => {
                        // The avatar centred on the pill's height, AVATAR_GAP before it.
                        let pill_h = ui.painter().layout_no_wrap("Ag".into(), theme::body(12.0), p.text).size().y + 2.0 * PILL_MARGIN.y;
                        let id = i.assignee.as_ref().map(|u| u.account_id.as_str());
                        ui.spacing_mut().item_spacing.x = theme::AVATAR_GAP;
                        crate::avatar::inline(ui, id, t, theme::AVATAR_PILL, pill_h);
                        ui.spacing_mut().item_spacing.x = PILL_GAP;
                        neutral_pill(ui, p, t, sel);
                    }
                }
            }
        });
    })
}

/// What a Proposed, New or Unclaimed row may show besides its title.
struct RowPills<'a> {
    report: Option<&'a (String, report::Report)>,
    today: chrono::NaiveDate,
    common_age: Option<&'a str>,
    common_assignee: Option<&'a str>,
    status_colors: &'a std::collections::BTreeMap<String, String>,
}

impl RowPills<'_> {
    /// The report's badge, and the age and assignee where they differ from what most rows here share.
    fn for_issue(&self, i: &jira::Issue) -> Vec<RowPill> {
        let mut out = Vec::new();
        if let Some((d, b)) = self.report.and_then(|(d, r)| r.badge_for(&i.key).map(|b| (d, b))) {
            out.push(RowPill::Badge(b, d.clone()));
        }
        let age = grooming::age_tag(&i.created, self.today);
        if self.common_age != Some(age.as_str()) {
            out.push(RowPill::Neutral(age));
        }
        let who = assignee_name(i);
        if self.common_assignee != Some(who) {
            out.push(if i.assignee.is_some() { RowPill::Person(who.to_string()) } else { RowPill::Neutral(who.to_string()) });
        }
        out
    }
}

/// An assignee pill's text.
fn assignee_name(i: &jira::Issue) -> &str {
    i.assignee.as_ref().map_or("Unassigned", |u| u.display_name.as_str())
}

/// A Proposed, New or Unclaimed row; the timer shows on the focused Proposed or New one, the status on Unclaimed ones.
#[allow(clippy::too_many_arguments)]
fn candidate_row(
    ui: &mut Ui,
    p: &Palette,
    salt: &str,
    i: &jira::Issue,
    idx: usize,
    n: usize,
    sel: bool,
    timed: bool,
    unclaimed: bool,
    elapsed: u64,
    k: &RowPills,
) -> egui::Response {
    let pills = k.for_issue(i);
    if unclaimed {
        // Age and assignee join the status at the right; only a report badge keeps the second line.
        let (neutral, rest): (Vec<RowPill>, Vec<RowPill>) = pills.into_iter().partition(|r| matches!(r, RowPill::Neutral(_) | RowPill::Person(_)));
        let extra = neutral.into_iter().filter_map(|r| if let RowPill::Neutral(t) | RowPill::Person(t) = r { Some(t) } else { None }).collect();
        let tint = theme::status_tint(&i.status.name, &i.status.category, p, k.status_colors);
        return ticket_row(ui, p, salt, i, sel, idx, n, Some(Tail::Status(i.status.name.clone(), tint, extra)), &rest);
    }
    let tail = timed.then(|| Tail::Timer(elapsed, timer_color(grooming::timer_level(elapsed), p)));
    ticket_row(ui, p, salt, i, sel, idx, n, tail, &pills)
}

/// The selected epic's lanes: the active sprint, the next sprint, Ready for Work (rank order, with the ghost row
/// while placing), then Proposed and New with the focused candidate's timer and, when open, the bounce box under it.
fn epic_queue(app: &mut App, ui: &mut Ui, p: &Palette, view: &QueueView, cursor: usize) {
    let report = app.grooming.report.as_ref().map(|(path, r)| (report_date(path, r), r.clone()));
    let placement = app.grooming.placement.map(|at| at.min(view.ready.len()));
    let elapsed = app.grooming.focused_since.map_or(0, |t| t.elapsed().as_secs());
    let today = chrono::Local::now().date_naive();
    let status = (app.cfg.grooming.status_ready.clone(), app.cfg.grooming.status_new.clone());
    let status_colors = app.cfg.issues.status_colors.clone();
    let mut clicked = None;
    let free = app.grooming.placement.is_none() && app.grooming.bounce_text.is_none();
    let lead = view.sprint.len() + view.next_sprint.len();
    let candidates = || view.planning.iter().chain(&view.new).chain(&view.unclaimed);
    let ages: Vec<String> = candidates().map(|i| grooming::age_tag(&i.created, today)).collect();
    let common_age = common_value(ages.iter().map(String::as_str));
    let common_assignee = common_value(candidates().map(assignee_name));
    let pills = RowPills { report: report.as_ref(), today, common_age, common_assignee, status_colors: &status_colors };
    // The queue follows the cursor only when it lands somewhere new (another row, epic or placement slot), so
    // a mouse-wheel scroll is not undone every frame; the same scheme as the pre-check list.
    let last_id = egui::Id::new("groom-session-last-cursor");
    let now = (app.grooming.epic_index, cursor, placement);
    let moved = ui.data(|d| d.get_temp::<(usize, usize, Option<usize>)>(last_id)) != Some(now);
    ui.data_mut(|d| d.insert_temp(last_id, now));
    crate::views::no_scroll_fade(ui);
    let out = egui::ScrollArea::vertical().id_salt("groom-queue").auto_shrink([false, false]).show(ui, |ui| {
        let mut lanes = Lanes::default();
        if let Some(name) = &view.active
            && let Some(at) = sprint_block(ui, p, &mut lanes, name, "groom-sprint", &view.sprint, 0, cursor, moved)
            && free
        {
            clicked = Some(at);
        }
        if let Some(name) = &view.next
            && let Some(at) = sprint_block(ui, p, &mut lanes, name, "groom-next", &view.next_sprint, view.sprint.len(), cursor, moved)
            && free
        {
            clicked = Some(view.sprint.len() + at);
        }
        let ready = &view.ready;
        let rows = ready.len() + usize::from(placement.is_some());
        lanes.lane(ui, p, &status.0, rows);
        let mut row = 0;
        for at in 0..=ready.len() {
            if placement == Some(at)
                && let Some(c) = view.planning.iter().chain(&view.new).nth(cursor.saturating_sub(lead + ready.len()))
            {
                let resp = ghost_row(ui, p, c);
                if moved {
                    resp.scroll_to_me(None);
                }
                row += 1;
            }
            if let Some(i) = ready.get(at) {
                let sel = cursor == lead + at;
                let resp = ticket_row(ui, p, "groom-ready", i, sel, row, rows, None, &[]);
                if resp.clicked() && free {
                    clicked = Some(lead + at);
                }
                if sel && moved {
                    resp.scroll_to_me(None);
                }
                row += 1;
            }
        }
        for (label, salt, list, offset, unclaimed) in [
            ("Proposed", PROPOSED_SALT, &view.planning, lead + ready.len(), false),
            (status.1.as_str(), NEW_SALT, &view.new, lead + ready.len() + view.planning.len(), false),
            ("Unclaimed", UNCLAIMED_SALT, &view.unclaimed, lead + ready.len() + view.planning.len() + view.new.len(), true),
        ] {
            // Unclaimed is an extra lane that only exists when an epic has unlabelled children.
            if unclaimed && list.is_empty() {
                continue;
            }
            lanes.lane(ui, p, label, list.len());
            for (idx, i) in list.iter().enumerate() {
                let here = offset + idx == cursor;
                // While placing, the ghost row is where the keys act; the source row steps back to a plain row.
                let sel = here && placement.is_none();
                let resp = candidate_row(ui, p, salt, i, idx, list.len(), sel, here, unclaimed, elapsed, &pills);
                if resp.clicked() && free {
                    clicked = Some(offset + idx);
                }
                if sel && moved {
                    resp.scroll_to_me(None);
                }
                if here && !unclaimed && app.grooming.bounce_text.is_some() {
                    let item = ui.style().spacing.item_spacing.y;
                    ui.spacing_mut().item_spacing.y = item;
                    ui.add_space(line_gap(ui) * 2.0);
                    bounce_box(app, ui, p);
                    // The note ends the row: the next lane header keeps its usual gap.
                    ui.add_space(ROW_GAP - item);
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                }
            }
        }
        lanes.flush(ui, p);
        selection_ring(ui, p);
    });
    crate::views::components::scroll_edge_fade(ui, &out, p.bg);
    if let Some(i) = clicked {
        app.actions.push(Action::GroomMove(i as i32 - cursor as i32));
    }
}

/// A placement ghost's dash and gap along its outline.
const GHOST_DASH: f32 = 4.0;
const GHOST_DASH_GAP: f32 = 3.0;

/// Where the accepted ticket would land: a drop target, not a selection. No fill (the page shows through), a dashed
/// accent outline, and the line saying what Enter does.
fn ghost_row(ui: &mut Ui, p: &Palette, c: &jira::Issue) -> egui::Response {
    let r = Frame::new().inner_margin(row_pad(ui)).outer_margin(Margin::symmetric(0, 1)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        title_line(ui, p, &c.key, &c.summary, false, None);
        ui.add_space(line_gap(ui));
        ui.label(RichText::new("Lands here · Enter to place").color(p.muted).size(theme::spacing(ui.ctx()).caption));
    });
    let rect = r.response.rect.shrink(0.5);
    let stroke = Stroke::new(1.0, p.selection_edge());
    let pts = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    ui.painter().extend(egui::Shape::dashed_line(&pts, stroke, GHOST_DASH, GHOST_DASH_GAP));
    r.response
}

/// The bounce comment box, under the focused candidate.
fn bounce_box(app: &mut App, ui: &mut Ui, p: &Palette) {
    let id = egui::Id::new("groom-bounce");
    let focus_now = std::mem::take(&mut app.grooming.bounce_focus);
    let status_new = app.cfg.grooming.status_new.clone();
    let already_new = app.session_focus().is_some_and(|i| i.status.name == status_new);
    let writable = jira::check_writable().is_ok();
    let hint = bounce_hint(writable);
    let Some(text) = app.grooming.bounce_text.as_mut() else { return };
    let focused = ui.memory(|m| m.has_focus(id));
    let sp = *theme::spacing(ui.ctx());
    let font = theme::body(sp.body);
    let pad = row_margin(&sp);
    // The page's one form-field surface, as tall as its text (one line, two once it wraps).
    Frame::new()
        .fill(p.field())
        .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        .inner_margin(Margin { left: pad.left - 1, right: pad.right - 1, top: pad.top, bottom: pad.bottom })
        .show(ui, |ui| {
            let empty = text.is_empty();
            let w = ui.available_width();
            let shown = if empty { hint } else { text.as_str() };
            let g = ui.painter().layout(shown.to_string(), font.clone(), p.text, w);
            let size = vec2(w, bounce_height(g.rows.len(), ui.painter().layout_no_wrap("A".to_string(), font.clone(), p.text).size().y));
            let r = crate::views::add_field(ui, Some(size), text, |t| {
                egui::TextEdit::multiline(t)
                    .id(id)
                    .font(font.clone())
                    .desired_rows(1)
                    .frame(Frame::new().inner_margin(crate::views::caret_margin(Margin::ZERO, empty)))
                    .hint_text(crate::views::empty_field_hint(hint, font.clone(), p))
            });
            if focus_now {
                r.request_focus();
            }
        });
    let (note, warn) = bounce_note(text, &status_new, already_new, writable);
    ui.add_space(sp.line_gap);
    ui.label(RichText::new(note).color(if warn { p.at_risk } else { p.muted }).size(sp.caption));
}

/// The bounce field's height for text that wraps to `rows` lines of `row_h`: one line at least, BOUNCE_LINES at most.
pub fn bounce_height(rows: usize, row_h: f32) -> f32 {
    rows.clamp(1, BOUNCE_LINES) as f32 * row_h
}

/// The line under the bounce box, and whether it warns. In read-only mode it says posting is off, in the pickers'
/// words (`pickers::read_only_note`), before anything is typed.
pub fn bounce_note(text: &str, status_new: &str, already_new: bool, writable: bool) -> (String, bool) {
    if !writable {
        ("Read-only mode is on, so posting is off. Restart without JIRA_TICKETS_READONLY to bounce.".to_string(), false)
    } else if grooming::bounce_comment_ok(text) && already_new {
        (format!("⌘Enter posts the comment; the ticket stays in {status_new}"), false)
    } else if grooming::bounce_comment_ok(text) {
        (format!("⌘Enter posts the comment, then moves the ticket to {status_new}"), false)
    } else if text.trim().is_empty() {
        ("A one- or two-line comment is required".to_string(), false)
    } else {
        ("Two lines at most".to_string(), true)
    }
}

/// The per-ticket timer's colour: plain text, then amber, then red.
pub fn timer_color(level: Level, p: &Palette) -> Color32 {
    match level {
        Level::Ok => p.text,
        Level::Amber => p.at_risk,
        Level::Red => p.off_track,
    }
}

/// "Report 2026-09-04 · 11 promoted · 14 need detail", or "No report found in <dir>" naming the configured dir.
pub fn report_line(report: Option<(&std::path::Path, &report::Report)>, dir: &str) -> String {
    match report {
        Some((path, r)) => format!("Report {} · {} promoted · {} need detail", report_date(path, r), r.promoted.len(), r.needs_detail.len()),
        None => format!("No report found in {dir}"),
    }
}

/// The report's own date, else its file name's stem.
pub fn report_date(path: &std::path::Path, r: &report::Report) -> String {
    if r.date.is_empty() { path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string() } else { r.date.clone() }
}

/// The header's queue count: "Ready for Work: 58 (limit 15)", and past the limit what to do about it.
pub fn ceiling_text(status_ready: &str, ready: usize, hard: usize, over: bool) -> String {
    let count = format!("{status_ready}: {ready} (limit {hard})");
    if over { format!("{count} · close or bounce before accepting more") } else { count }
}

/// The Ready for Work count shows only in Session, where accept, close and bounce act on it; Pre-check promotes to
/// Planning, so there the count is noise.
pub fn ceiling_shown(mode: GroomMode) -> bool {
    mode == GroomMode::Session
}

/// The count warns (amber) only past the hard limit; under it, it is a plain muted fact.
pub fn ceiling_warns(level: Level) -> bool {
    level == Level::Red
}

/// The count's tooltip: what the two limits mean and what to do past them.
pub fn ceiling_tip(status_ready: &str, ready: usize, soft: usize, hard: usize) -> String {
    let what = format!("{ready} team tickets are in {status_ready}. Aim for {soft} or fewer; {hard} is the limit.");
    if ready > hard {
        format!("{what} Over the limit: close, move out or bounce tickets before accepting more.")
    } else if ready > soft {
        format!("{what} Over the aim: accept only what the next sprint needs.")
    } else {
        what
    }
}

/// A report badge's text: what the last pre-check said about a ticket, in the readiness bar's words.
/// The report badge's pills: one per thing the report said was missing (unknown slugs as written, "Empty" only
/// when nothing else applies), like the live verdicts.
pub fn badge_pills(badge: &Badge) -> Vec<String> {
    match badge {
        Badge::Promoted => vec!["Promoted by pre-check".to_string()],
        Badge::NeedsDetail(codes) => {
            let known: Vec<grooming::Code> = codes.iter().filter_map(|s| grooming::Code::parse(s)).collect();
            let shown = guide::shown_codes(&known);
            codes
                .iter()
                .filter_map(|s| match grooming::Code::parse(s) {
                    Some(c) => shown.contains(&c).then(|| guide::code_words(c).to_string()),
                    None => Some(s.clone()),
                })
                .collect()
        }
        Badge::Struck => vec!["Closed after pre-check".to_string()],
    }
}

/// The report badge's tooltip: which report said it.
pub fn badge_tip(report_date: &str) -> String {
    format!("From the pre-check report of {report_date}")
}

/// The report badge: a verdict, so filled status pills like the live verdicts (green when promoted, amber when it
/// needs detail); neutral when closed since. The report's date is in the tooltip.
pub fn badge(ui: &mut Ui, p: &Palette, badge: &Badge, report_date: &str) {
    for text in badge_pills(badge) {
        let r = match badge {
            Badge::Promoted => c::status_pill(ui, &text, verdict_tint(true, p)),
            Badge::NeedsDetail(_) => c::status_pill(ui, &text, verdict_tint(false, p)),
            Badge::Struck => outline_pill(ui, &text, p, PILL_MARGIN),
        };
        crate::views::tooltip(r, badge_tip(report_date));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn report_line_counts_or_names_the_configured_dir() {
        let r = report::parse(
            "# x — 2026-09-04\n\n## Planning — by epic\n\n### (no epic)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: empty\n",
        );
        assert_eq!(report_line(Some((Path::new("/r/2026-09-04.md"), &r)), "~/x"), "Report 2026-09-04 · 1 promoted · 1 need detail");
        assert_eq!(report_line(None, "~/Documents/grooming-reports"), "No report found in ~/Documents/grooming-reports");
        let undated = report::Report { date: String::new(), ..r };
        assert_eq!(report_date(Path::new("/r/2026-10-01.md"), &undated), "2026-10-01");
    }

    #[test]
    fn badges_say_what_the_report_said() {
        assert_eq!(badge_pills(&Badge::Promoted), ["Promoted by pre-check"]);
        let nd = Badge::NeedsDetail(vec!["unclear-when".into(), "empty".into(), "odd".into()]);
        assert_eq!(badge_pills(&nd), ["Unclear when it is done", "odd"], "one pill each; Empty gives way to a real gap");
        assert_eq!(badge_pills(&Badge::NeedsDetail(vec!["empty".into()])), ["Empty"]);
        assert_eq!(badge_pills(&Badge::Struck), ["Closed after pre-check"]);
        assert_eq!(badge_tip("2026-09-04"), "From the pre-check report of 2026-09-04");
    }

    #[test]
    fn summary_wording_and_promote_gate() {
        let plan = Promotion {
            promoted: vec!["CI-1".into(), "CI-2".into()],
            held_back: vec!["CI-3".into()],
            needs_detail: vec![("CI-4".into(), vec![grooming::Code::UnclearWhat, grooming::Code::Empty])],
        };
        assert_eq!(summary_line(&plan, 4), "Summary · 3 ready · 1 need detail · 4 without a verdict");
        assert_eq!(plan_lines(&plan, 4, 2), ["2 to promote to Planning", "1 ready but held back (cap 2)", "1 need detail", "4 without a verdict"]);
        assert_eq!(promote_label(2), "Promote 2");
        assert!(promote_enabled(2, true, false));
        assert!(!promote_enabled(0, true, false), "nothing to promote");
        assert!(!promote_enabled(2, false, false), "read-only");
        assert!(!promote_enabled(2, true, true), "already running");
        assert_eq!(promote_blocked(2, true, false), None);
        assert!(promote_blocked(2, false, false).is_some_and(|w| w.starts_with("Read-only")), "read-only says why");
        assert!(promote_blocked(0, true, false).is_some_and(|w| w.starts_with("Nothing")));
        assert!(promote_blocked(2, true, true).is_some_and(|w| w.starts_with("Promoting")));
        for (n, w, r) in [(2, true, false), (0, true, false), (2, false, false), (2, true, true)] {
            assert_eq!(promote_enabled(n, w, r), promote_blocked(n, w, r).is_none(), "a reason exactly when off");
        }
        assert_eq!(verdict_pills(&Verdict::NeedsDetail(plan.needs_detail[0].1.clone())), ["Unclear what is being done"], "no contradictory Empty");
        assert_eq!(verdict_pills(&Verdict::NeedsDetail(vec![grooming::Code::UnclearWhen, grooming::Code::UnclearWhat])).len(), 2, "a pill per code");
        assert_eq!(verdict_pills(&Verdict::Ready), ["Ready"]);
    }

    #[test]
    fn the_hint_follows_the_open_dialog() {
        let mut g = GroomingState::default();
        assert_eq!(hint(&g), GroomHint::PreCheck);
        g.summary_open = true;
        assert_eq!(hint(&g), GroomHint::Summary);
        g.pending_report = Some(String::new());
        assert_eq!(hint(&g), GroomHint::Overwrite, "the confirm sits over everything");
        let mut g = GroomingState { mode: GroomMode::Session, ..Default::default() };
        assert_eq!(hint(&g), GroomHint::Session);
        g.placement = Some(0);
        assert_eq!(hint(&g), GroomHint::Placement);
        g.bounce_text = Some(String::new());
        assert_eq!(hint(&g), GroomHint::Bounce);
    }

    #[test]
    fn ceiling_reads_as_plain_copy_and_warns_only_past_the_limit() {
        assert_eq!(ceiling_text("Ready for Work", 12, 15, false), "Ready for Work: 12 (limit 15)");
        assert_eq!(
            ceiling_text("Ready for Work", 58, 15, true),
            "Ready for Work: 58 (limit 15) · close or bounce before accepting more",
            "past the limit, the next step"
        );
        assert!(!ceiling_warns(Level::Ok) && !ceiling_warns(Level::Amber));
        assert!(ceiling_warns(Level::Red));
        assert_eq!(ceiling_tip("Ready for Work", 5, 10, 15), "5 team tickets are in Ready for Work. Aim for 10 or fewer; 15 is the limit.");
        assert!(ceiling_tip("Ready for Work", 12, 10, 15).ends_with("Over the aim: accept only what the next sprint needs."));
        assert!(ceiling_tip("Ready for Work", 58, 10, 15).ends_with("before accepting more."));
    }

    #[test]
    fn a_pill_every_row_shares_is_dropped() {
        assert_eq!(common_value(["older", "older", "new (3d)"]), Some("older"));
        assert_eq!(common_value(["older", "new (3d)"]), None, "no majority, every row keeps its pill");
        assert_eq!(common_value(["older"]), Some("older"));
        assert_eq!(common_value(std::iter::empty::<&str>()), None);
    }

    #[test]
    fn verdict_pills_are_status_not_category() {
        for p in [&theme::LIGHT, &theme::DARK] {
            for ready in [true, false] {
                let t = verdict_tint(ready, p);
                assert_ne!(t.bg, Color32::TRANSPARENT, "filled, unlike the outlined category pills");
                for fill in [p.surface, p.selection()] {
                    let under = theme::over(t.bg, fill);
                    assert!(theme::contrast(t.fg, under) >= 4.5, "dark={} ready={ready}: {:.2}", p.dark, theme::contrast(t.fg, under));
                }
            }
        }
    }

    #[test]
    fn layout_rules_follow_the_density() {
        // A one-word last line narrows the measure until two words wrap down; a clean wrap keeps it.
        assert_eq!(widow_free_width(480.0, |w| if w > 460.0 { 1 } else { 3 }), 456.0);
        assert_eq!(widow_free_width(480.0, |_| 4), 480.0);
        assert_eq!(widow_free_width(480.0, |_| 1), 480.0, "gives up rather than squeeze");
        assert_eq!(row_margin(&theme::COMPACT).left, 12);
        assert_eq!(row_margin(&theme::COMFORTABLE).left, theme::COMFORTABLE.card_pad as i8, "rows share the cards' inset");
        assert_eq!(bounce_height(0, 18.0), 18.0);
        assert_eq!(bounce_height(5, 18.0), 36.0);
        assert!(ceiling_shown(GroomMode::Session) && !ceiling_shown(GroomMode::PreCheck));
    }

    #[test]
    fn both_modes_draw_headless() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let status = |n: &str| crate::jira::Status { name: n.into(), category: "new".into() };
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                crate::jira::Issue { key: "CI-1".into(), labels: vec!["PLAT".into()], status: status("New"), ..Default::default() },
                crate::jira::Issue { key: "CI-2".into(), labels: vec!["PLAT".into()], status: status("Ready for Work"), ..Default::default() },
            ],
        );
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let pre = texts(&mut app);
        // `for_test` swaps the default report dir for one that does not exist.
        let no_report = format!("No report found in {}", app.cfg.grooming.report_dir);
        for want in ["Pre-check", "Session", no_report.as_str(), "Summary · 0 ready · 0 need detail · 1 without a verdict", "rank 1"] {
            assert!(pre.iter().any(|t| t == want), "{want} in {pre:?}");
        }
        assert!(!pre.iter().any(|t| t.starts_with("Ready for Work: ")), "no ceiling on Pre-check: {pre:?}");
        let pre_purpose = guide::purpose(GroomMode::PreCheck, "New", "Planning", "Ready for Work");
        for want in [pre_purpose.as_str(), "Readiness bar", "Can a stranger tell WHAT is being done and WHEN it is done?", "When in doubt, pass."] {
            assert!(pre.iter().any(|t| t == want), "{want} in {pre:?}");
        }
        app.grooming.cursor = 1;
        let past = texts(&mut app);
        assert!(past.iter().any(|t| t == "0 of 1 judged. Enter opens the summary; j/k go back."), "{past:?}");
        assert!(!pre.iter().any(|t| t == "older"), "an age every row shares is not shown: {pre:?}");
        app.grooming.verdicts.insert("CI-1".into(), Verdict::Ready);
        let past = texts(&mut app);
        assert!(past.iter().any(|t| t == "All 1 judged. Enter opens the summary."), "{past:?}");
        app.grooming.cursor = 0;
        app.grooming.summary_open = true;
        let summary = texts(&mut app);
        let path = guide::report_path(&app.cfg.grooming.report_dir, chrono::Local::now().date_naive());
        let explainer = guide::promote_explainer(1, 0, app.cfg.grooming.planning_cap, "Planning");
        for want in [
            "Ready",
            "Promote to Planning",
            explainer.as_str(),
            path.as_str(),
            "Promote",
            "1",
            "Promote 1",
            "Cancel",
            "0 without a verdict stay in New, untouched.",
        ] {
            assert!(summary.iter().any(|t| t == want), "{want} in {summary:?}");
        }
        app.grooming.summary_open = false;
        app.grooming.pending_report = Some("text".into());
        let confirm = texts(&mut app);
        for want in ["Report for today exists. Overwrite?", "Overwrite", "Keep the old one"] {
            assert!(confirm.iter().any(|t| t == want), "{want} in {confirm:?}");
        }
        app.grooming.pending_report = None;
        app.grooming.mode = GroomMode::Session;
        let session = texts(&mut app);
        let session_purpose = guide::purpose(GroomMode::Session, "New", "Planning", "Ready for Work");
        let empty_run = guide::empty_lanes_line(&["Proposed"]);
        for want in [session_purpose.as_str(), "No epic", "(no epic)", "1 to groom", empty_run.as_str(), "New", "Ready for Work: 1 (limit 15)"] {
            assert!(session.iter().any(|t| t == want), "{want} in {session:?}");
        }
    }

    #[test]
    fn session_draws_stages_blocks_ghost_and_bounce_box() {
        // The bounce note reads the write gate: hold it so no other test flips read-only mid-draw.
        let _w = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let issue = |k: &str, st: &str, epic: Option<&str>, sprint: Option<&str>| crate::jira::Issue {
            key: k.into(),
            summary: format!("Summary {k}"),
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: st.into(), category: "new".into() },
            parent_key: epic.map(String::from),
            parent_summary: epic.map(|_| "Blue Green Ops".to_string()),
            sprint: sprint.map(String::from),
            ..Default::default()
        };
        app.sprints = vec![
            crate::jira::Sprint { id: 1, name: "Sprint 92".into(), state: "active".into() },
            crate::jira::Sprint { id: 2, name: "Sprint 93".into(), state: "future".into() },
        ];
        app.store.absorb(
            crate::store::ListId::Epics,
            vec![crate::jira::Issue {
                key: "CI-1800".into(),
                summary: "Blue Green Ops".into(),
                issue_type: "Epic".into(),
                labels: vec!["roadmap".into(), "P0".into()],
                ..Default::default()
            }],
        );
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                issue("CI-5", "Ready for Work", Some("CI-1800"), None),
                issue("CI-1", "Planning", Some("CI-1800"), None),
                issue("CI-3", "New", Some("CI-1800"), None),
                issue("CI-4", "Work in Progress", Some("CI-1800"), Some("Sprint 92")),
                issue("CI-6", "Ready for Work", Some("CI-1800"), Some("Sprint 93")),
                issue("CI-9", "New", None, None),
            ],
        );
        app.grooming.mode = GroomMode::Session;
        app.grooming.cursor = 3; // CI-1, the first candidate (after CI-4, CI-6 and CI-5)
        app.grooming.focused_since = Some(std::time::Instant::now());
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1400.0, 900.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let t = texts(&mut app);
        for want in [
            "Decide",
            "Ready for Work: accept and place it in rank.",
            "Move a Ready for Work ticket down or up in rank.",
            "Three to five minutes per ticket. Past that, bounce.",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        for want in [
            "Roadmap",
            "No epic",
            "(no epic)",
            "Blue Green Ops",
            "2 to groom",
            "1 to groom",
            "Sprint 92",
            "Sprint 93",
            "Ready for Work",
            "Proposed",
            "New",
            "0:00",
            TIMER_LABEL,
            "CI-5",
            "CI-4",
            "CI-6",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Unassigned"), "an assignee every row shares is not shown: {t:?}");
        assert!(!t.iter().any(|x| x == "Ongoing" || x == "Other epics"), "stage headers only for stages with epics: {t:?}");
        app.grooming.placement = Some(0);
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "Lands here · Enter to place"), "{t:?}");
        app.grooming.placement = None;
        app.grooming.bounce_text = Some(String::new());
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "A one- or two-line comment is required"), "{t:?}");
        {
            let _ro = crate::jira::tests::ReadOnlyGuard::new();
            let t = texts(&mut app);
            assert!(t.iter().any(|x| x.starts_with("Read-only mode is on, so posting is off.")), "read-only says so in the bounce box: {t:?}");
        }
        app.grooming.bounce_text = None;
        app.grooming.cursor = 0;
        let t = texts(&mut app);
        assert!(!t.iter().any(|x| x == "0:00"), "no timer on a sprint row: {t:?}");
        assert!(!t.iter().any(|x| x.starts_with("Unclaimed ·")), "no Unclaimed block without roadmap children: {t:?}");
        let mut child = issue("CI-77", "New", Some("CI-1800"), None);
        child.labels.clear();
        app.store.absorb(crate::store::ListId::EpicChildren, vec![child]);
        app.grooming.cursor = 5;
        let t = texts(&mut app);
        for want in ["Unclaimed", "CI-77", "Summary CI-77", "3 to groom"] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        assert!(!t.iter().any(|x| x == "0:00"), "no timer on an Unclaimed row: {t:?}");
    }

    #[test]
    fn proposed_and_new_rows_get_distinct_widget_ids() {
        // Both lists have a row 0; sharing an id would make egui treat them as one widget.
        assert_ne!(row_id(PROPOSED_SALT, 0), row_id(NEW_SALT, 0));
        assert_ne!(row_id(PROPOSED_SALT, 0), row_id("groom-ready", 0));
    }

    #[test]
    fn session_wording() {
        let counts = |s, n, r, p, new| RailCounts { sprint: s, next: n, ready: r, planning: p, new, unclaimed: 0 };
        assert_eq!(rail_count(&counts(2, 1, 7, 3, 12)).as_deref(), Some("15 to groom"), "only what the walk decides on");
        assert_eq!(rail_count(&counts(2, 1, 7, 0, 0)), None, "nothing to groom shows no count");
        assert!(!rail_focused(3) && rail_focused(0), "the ticket list holds the ring while it has rows");
        let with_u = RailCounts { unclaimed: 4, ..counts(0, 0, 0, 0, 2) };
        assert_eq!(rail_count(&with_u).as_deref(), Some("6 to groom"));
        assert_eq!(
            rail_tip(Some("CI-1800"), "Gradual Releases go", &counts(2, 1, 7, 3, 12), Some("Sprint 92"), Some("Sprint 93"), "Ready for Work", "New"),
            "CI-1800 Gradual Releases go\n2 in Sprint 92 · 1 in Sprint 93 · 7 Ready for Work · 3 Proposed · 12 New"
        );
        assert!(rail_tip(Some("CI-1"), "CI-1", &with_u, None, None, "Ready for Work", "New").starts_with("CI-1\n"), "a key-only epic shows the key once");
        assert!(rail_tip(None, "(no epic)", &with_u, None, None, "Ready for Work", "New").ends_with("4 unclaimed"));
        assert_eq!(bounce_note("Which clusters?", "New", false, true), ("⌘Enter posts the comment, then moves the ticket to New".to_string(), false));
        assert_eq!(bounce_note("Which clusters?", "New", true, true), ("⌘Enter posts the comment; the ticket stays in New".to_string(), false));
        assert_eq!(bounce_note(" ", "New", false, true).0, "A one- or two-line comment is required");
        assert_eq!(bounce_note("a\nb\nc", "New", false, true), ("Two lines at most".to_string(), true));
        for text in ["", "Which clusters?", "a\nb\nc"] {
            let (note, warn) = bounce_note(text, "New", false, false);
            assert!(note.starts_with("Read-only mode is on, so posting is off.") && !warn, "read-only says posting is off: {note}");
        }
        assert_eq!(bounce_hint(true), BOUNCE_HINT);
        assert!(!bounce_hint(false).contains('⌘'), "read-only names no key that posts");
        let p = &theme::LIGHT;
        assert_eq!(timer_color(Level::Ok, p), p.text);
        assert_eq!(timer_color(Level::Amber, p), p.at_risk);
        assert_eq!(timer_color(Level::Red, p), p.off_track);
    }

    #[test]
    fn an_epic_with_nothing_to_groom_says_so() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![crate::jira::Issue {
                key: "CI-2".into(),
                labels: vec!["PLAT".into()],
                status: crate::jira::Status { name: "Ready for Work".into(), category: "new".into() },
                parent_key: Some("CI-1".into()),
                parent_summary: Some("Metrics Retention".into()),
                ..Default::default()
            }],
        );
        app.grooming.mode = GroomMode::Session;
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let t: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        assert!(t.iter().any(|x| x == "Nothing Proposed or New here. Run a pre-check first, or pick another epic with ]."), "{t:?}");
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    use crate::app::App;

    fn long_session_app() -> App {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let issue = |k: String, st: &str| crate::jira::Issue {
            summary: format!("Summary {k}"),
            key: k,
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: st.into(), category: "new".into() },
            parent_key: Some("CI-1860".into()),
            parent_summary: Some("Blue Green Ops".into()),
            ..Default::default()
        };
        let mut team: Vec<_> = (1..=40).map(|n| issue(format!("CI-{n}"), "Ready for Work")).collect();
        team.push(issue("CI-99".into(), "New"));
        app.store.absorb(crate::store::ListId::TeamOpen, team);
        app.grooming.mode = GroomMode::Session;
        app.grooming.focused_since = Some(std::time::Instant::now());
        app
    }

    /// The queue rows on screen at 1400×900 after `moves` (frame → cursor) land on one context: egui paints only
    /// the rows inside the scroll area's viewport, so a painted key is a row on screen. The focused ticket's pane
    /// repeats the key to the right of the queue, so only inks in the queue column count. The cursor moves on a
    /// later frame, as in the app, since the first frame of a fresh context has no font sizes yet; 20 frames after
    /// the last move let the scroll animation (at most 0.3 s at 60 fps) finish.
    fn queue_keys(app: &mut App, moves: &[(usize, usize)]) -> Vec<String> {
        let frames = moves.iter().map(|(f, _)| f).max().copied().unwrap_or(0) + 20;
        let mut frame = 0;
        let shapes = crate::testutil::paint_frames_at2(egui::vec2(1400.0, 900.0), frames, |ui| {
            frame += 1;
            if let Some((_, to)) = moves.iter().find(|(f, _)| *f == frame) {
                app.grooming.cursor = *to;
            }
            show(app, ui);
        });
        crate::testutil::inks(&shapes).into_iter().filter(|(t, r)| t.starts_with("CI-") && r.left() < 700.0).map(|(t, _)| t).collect()
    }

    #[test]
    fn the_queue_scrolls_to_the_focused_row() {
        let mut app = long_session_app();
        // The list overflows: the last Ready row and the candidate are below the window while the first is focused.
        let top = queue_keys(&mut app, &[]);
        assert!(top.iter().any(|k| k == "CI-1") && !top.iter().any(|k| k == "CI-40" || k == "CI-99"), "{top:?}");
        // j/k down to the candidate under the 40 Ready rows: the queue scrolls so the focused row shows.
        let down = queue_keys(&mut app, &[(5, 40)]);
        assert!(down.iter().any(|k| k == "CI-99") && !down.iter().any(|k| k == "CI-1"), "{down:?}");
        // Back to the top: the first row comes back into view.
        let back = queue_keys(&mut app, &[(5, 40), (30, 0)]);
        assert!(back.iter().any(|k| k == "CI-1") && !back.iter().any(|k| k == "CI-99"), "{back:?}");
    }
}
