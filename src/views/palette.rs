//! The ⌘K palette: a field and its rows, drawn over every screen.

use crate::app::App;
use crate::keys::PaletteHint;
use crate::palette::{self, Hit};
use crate::theme::{self, Palette, Spacing};
use egui::{Align, Color32, Layout, Sense, UiBuilder, vec2};

/// The palette's content width: wider than the pickers' DIALOG_W, so titles fit.
pub const PALETTE_W: f32 = 600.0;
/// The dialog's margin on every side: the pickers' DIALOG_MARGIN sides.
const MARGIN: f32 = 16.0;
/// One compact row: 14 px text plus padding, like the picker rows. Comfortable adds row_extra above and below.
const ROW_H: f32 = 32.0;
/// The key column; PRS-12345 in 12 px mono fits. Command names start where titles do.
const KEY_W: f32 = 84.0;
/// A row's text inset, the pickers' ROW_INSET.
const INSET: f32 = crate::views::pickers::ROW_INSET;
/// The rows the list shows before it scrolls.
const LIST_ROWS: f32 = 10.0;
/// The field's edge to the first row's (or label's) edge: the gap above every later label.
const LIST_GAP: f32 = LABEL_ABOVE;
/// The ink gap above the footer, from the last row's edge (or the field's).
const FOOTER_GAP: f32 = 12.0;
/// A section label's ink: this far below the rows above it, and LABEL_BELOW above its own rows' edge.
const LABEL_ABOVE: f32 = 12.0;
const LABEL_BELOW: f32 = 4.0;
/// The gap between a title and the pill or key chip on its right.
const RIGHT_GAP: f32 = 12.0;
/// The scroll bar's width beside the rows: the floating bars' THUMB_W.
const BAR_W: f32 = crate::views::THUMB_W;

const HINT: &str = "Search tickets, type a key or number, or > for commands";

/// A row's height in the current density: the compact ROW_H plus `row_extra` above and below, like the other lists.
fn row_h(sp: &Spacing) -> f32 {
    ROW_H + 2.0 * sp.row_extra
}

/// The section label font: the caption step of the type scale.
fn label_font(sp: &Spacing) -> egui::FontId {
    theme::body(sp.caption)
}

/// The label over the group `hit` belongs to; a new label starts wherever it changes.
pub fn group_label(hit: &Hit, empty_query: bool) -> &'static str {
    match hit {
        Hit::Issue { text_only: true, .. } => "Matched in descriptions or comments",
        Hit::Issue { .. } if empty_query => "Recently updated",
        Hit::Issue { .. } => "Tickets",
        Hit::Command(_) => "Commands",
    }
}

/// The hit under the cursor: the cursor is clamped to the rows, so a shrinking list keeps one selected.
pub fn cursor_hit(hits: &[Hit], cursor: usize) -> Option<&Hit> {
    hits.get(cursor.min(hits.len().saturating_sub(1)))
}

/// What the hint bar offers with the palette open: open keys on a ticket, run on a command, only Esc on no rows.
pub fn hint(app: &App) -> Option<PaletteHint> {
    let pal = app.palette.as_ref()?;
    Some(match cursor_hit(&app.palette_hits(), pal.cursor) {
        Some(Hit::Issue { .. }) => PaletteHint::Ticket,
        Some(Hit::Command(_)) => PaletteHint::Command,
        None => PaletteHint::Empty,
    })
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.palette.is_none() {
        return;
    }
    let p = &theme::palette(ctx);
    if crate::views::backdrop(ctx, "palette-backdrop", egui::Order::Middle, crate::views::hint_bar_height(app), p).clicked() {
        app.palette = None;
        return;
    }
    let hits = app.palette_hits();
    let searching = app.palette_searching();
    let sp = theme::spacing(ctx);
    let note_font = theme::body(sp.small);
    let note_ink = theme::Ink::of(ctx, &note_font, None);
    let label_ink = theme::Ink::of(ctx, &label_font(sp), None);
    // Measured to the ink: the footer's lowest ink sits MARGIN above the frame, as the field's edge sits below it.
    let bottom = (MARGIN - (note_ink.line - (note_ink.base + note_ink.descent))).max(0.0);
    let margin = egui::Margin { left: MARGIN as i8, right: MARGIN as i8, top: MARGIN as i8, bottom: bottom.round() as i8 };
    let top = crate::views::pickers::dialog_top(ctx.content_rect().height());
    let mut confirm = None;
    egui::Area::new(egui::Id::new("palette")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        crate::views::components::dialog_frame(p).inner_margin(margin).show(ui, |ui| {
            crate::views::dialog_fields(ui, p);
            // Every vertical gap below is spelled out, so none comes from egui's item spacing.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_width(PALETTE_W);
            let Some(pal) = app.palette.as_mut() else { return };
            let r = crate::views::add_field(ui, None, &mut pal.query, |t| crate::views::pickers::field(t, HINT, p).desired_width(f32::INFINITY));
            r.request_focus();
            if r.changed() {
                pal.edited = Some(std::time::Instant::now());
                pal.cursor = 0;
            }
            let cursor = pal.cursor.min(hits.len().saturating_sub(1));
            let scroll = std::mem::take(&mut pal.scroll);
            let q = pal.query.trim().to_string();
            let note = footer(&q, hits.len(), searching, pal.error.as_deref());
            if !hits.is_empty() {
                ui.add_space(LIST_GAP);
                let rh = row_h(sp);
                let labels = group_runs(&hits, q.is_empty());
                let label_h = |first: bool| if first { 0.0 } else { LABEL_ABOVE } + label_ink.base + label_ink.descent - label_ink.top + LABEL_BELOW;
                let content_h: f32 = hits.len() as f32 * rh + labels.iter().enumerate().map(|(n, _)| label_h(n == 0)).sum::<f32>();
                // Room for LIST_ROWS rows and two labels, so ten commands or a short mixed list never scroll.
                let max_h = rh * LIST_ROWS + label_h(true) + label_h(false);
                let bar = content_h > max_h + 0.5;
                // A solid thumb beside the rows whenever they overflow, so "more below" never hides.
                ui.spacing_mut().scroll = egui::style::ScrollStyle { bar_width: BAR_W, ..egui::style::ScrollStyle::solid() };
                crate::views::no_scroll_fade(ui);
                let saved = crate::views::set_thumb_fills(ui, p);
                // The bar's gutter is kept whether or not it shows, so rows, pills and chips end on one edge.
                let s = ui.spacing().scroll;
                let row_w = ui.available_width() - (s.bar_width + s.bar_inner_margin + s.bar_outer_margin);
                let visibility = if bar { egui::scroll_area::ScrollBarVisibility::AlwaysVisible } else { egui::scroll_area::ScrollBarVisibility::AlwaysHidden };
                let out = egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).scroll_bar_visibility(visibility).show(ui, |ui| {
                    crate::views::restore_fills(ui, saved);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut next_label = labels.iter().peekable();
                    for (i, hit) in hits.iter().enumerate() {
                        if let Some((at, text)) = next_label.peek()
                            && *at == i
                        {
                            section(ui, p, sp, text, i == 0, row_w);
                            next_label.next();
                        }
                        let r = row(ui, app, hit, i == cursor, vec2(row_w, rh), p);
                        if i == cursor && scroll {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            confirm = Some(i);
                        }
                    }
                });
                crate::views::components::scroll_edge_fade(ui, &out, crate::views::dialog_fill(p));
            }
            ui.add_space((FOOTER_GAP - note_ink.top).max(0.0));
            let muted = crate::views::dialog_muted(p);
            ui.horizontal(|ui| {
                ui.add_space(INSET);
                if searching {
                    ui.add(egui::Spinner::new().size(sp.small).color(muted));
                }
                ui.label(egui::RichText::new(note).font(note_font.clone()).color(muted));
            });
        });
    });
    if let Some(i) = confirm {
        if let Some(pal) = app.palette.as_mut() {
            pal.cursor = i;
        }
        app.actions.push(crate::app::Action::PaletteConfirm { browser: false });
    }
}

/// Where each group starts: (the row's index, its label), one entry per run of rows under the same label.
pub fn group_runs(hits: &[Hit], empty_query: bool) -> Vec<(usize, &'static str)> {
    let mut out: Vec<(usize, &'static str)> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        let l = group_label(h, empty_query);
        if out.last().is_none_or(|(_, last)| *last != l) {
            out.push((i, l));
        }
    }
    out
}

/// A small muted label over a group of rows: its ink LABEL_ABOVE under the rows above (none at the list's top)
/// and LABEL_BELOW over its own, so it reads with the rows it names.
fn section(ui: &mut egui::Ui, p: &Palette, sp: &Spacing, text: &str, first: bool, w: f32) {
    let font = label_font(sp);
    let ink = theme::Ink::of(ui.ctx(), &font, None);
    let above = if first { 0.0 } else { LABEL_ABOVE };
    let h = above + ink.base + ink.descent - ink.top + LABEL_BELOW;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let color = crate::views::dialog_muted(p);
    let g = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let y = crate::views::issues::snap_px(rect.top() + above - ink.top, ui.ctx().pixels_per_point());
    ui.painter().galley(egui::pos2(rect.left() + INSET, y), g, color);
}

/// One row: a ticket's key, title and status pill, or a command's name and its key chip. The cursor row takes
/// the dialog's cursor fill; hover only tints it, so a resting mouse never fights the keys.
fn row(ui: &mut egui::Ui, app: &App, hit: &Hit, on: bool, size: egui::Vec2, p: &Palette) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if on {
        crate::views::dialog_cursor_fill(p)
    } else if resp.hovered() {
        crate::views::components::row_hover_fill(true, p)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, crate::views::components::ROW_RADIUS, fill);
    let muted = if on { crate::views::muted_on_selected(p) } else { crate::views::dialog_muted(p) };
    let inner = rect.shrink2(vec2(INSET, 0.0));
    let painter = ui.painter_at(rect);
    let ppp = ui.ctx().pixels_per_point();
    // The right-hand part first, so the title knows how much room is left.
    let (title, right_w) = match hit {
        Hit::Issue { key, .. } => {
            let issue = app.store.get(key);
            let mut right = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
            if let Some(i) = issue
                && !i.status.name.is_empty()
            {
                let t = theme::status_tint(&i.status.name, &i.status.category, p, &app.cfg.issues.status_colors);
                crate::views::components::status_pill(&mut right, &i.status.name, t);
            }
            let right_w = inner.right() - right.min_rect().left();
            let key_g = painter.layout_no_wrap(key.clone(), theme::mono(12.0), muted);
            painter.galley(egui::pos2(inner.left(), rect.center().y - key_g.size().y / 2.0), key_g, muted);
            (issue.map(|i| i.summary.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Open this ticket".to_string()), right_w)
        }
        Hit::Command(c) => {
            let (name, keys) = palette::command_name(*c);
            // The hint bar's key chip: the same face, fill and edge, its right edge on the rows' inset.
            let g = painter.layout_job(crate::views::issues::key_job(keys, p.text));
            let ink = theme::ink_rect(&g);
            let ink = if ink.is_positive() { ink } else { egui::Rect::from_min_size(egui::Pos2::ZERO, g.size()) };
            let pad = crate::views::components::KEY_CHIP_PAD;
            let pos = egui::pos2(
                crate::views::issues::snap_px(inner.right() - pad.x - ink.right(), ppp),
                crate::views::issues::snap_px(rect.center().y - ink.center().y, ppp),
            );
            let container = if on { crate::views::dialog_cursor_fill(p) } else { crate::views::dialog_fill(p) };
            crate::views::components::paint_key_chip(&painter, pos, &g, container, p);
            painter.galley(pos, g, p.text);
            (name.to_string(), ink.width() + 2.0 * pad.x)
        }
    };
    // Commands sit under their own label, so they start at the row's inset rather than after an empty key column.
    let lead = if matches!(hit, Hit::Command(_)) { 0.0 } else { KEY_W };
    let room = (inner.width() - lead - right_w - RIGHT_GAP).max(0.0);
    let mut job = egui::text::LayoutJob::single_section(title, egui::TextFormat { font_id: theme::body(14.0), color: p.text, ..Default::default() });
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let g = painter.layout_job(job);
    painter.galley(egui::pos2(inner.left() + lead, rect.center().y - g.size().y / 2.0), g, p.text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The line under the rows: what an empty box offers, that Jira is being asked, what to try when nothing
/// matched, or how many rows there are (the section labels say where they came from).
pub fn footer(query: &str, n: usize, searching: bool, error: Option<&str>) -> String {
    if let Some(e) = error {
        return e.to_string();
    }
    if query.is_empty() {
        return "Jira's descriptions and comments are searched as you type.".to_string();
    }
    let rows = if n == 1 { "1 result".to_string() } else { format!("{n} results") };
    match (searching, n) {
        (true, 0) => "Searching Jira…".to_string(),
        (true, _) => format!("{rows} · searching Jira…"),
        (false, 0) if query.starts_with('>') => "No matching commands. Try fewer words, or delete > to search tickets.".to_string(),
        (false, 0) => "No matches. Try fewer words, or a ticket number like 123.".to_string(),
        (false, _) => rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Command;

    #[test]
    fn the_footer_says_what_the_rows_are() {
        assert_eq!(footer("", 8, false, None), "Jira's descriptions and comments are searched as you type.", "the label says Recently updated");
        assert_eq!(footer("disk", 0, true, None), "Searching Jira…");
        assert_eq!(footer("disk", 3, true, None), "3 results · searching Jira…");
        assert_eq!(footer("disk", 1, false, None), "1 result", "no source suffix: the labels carry it");
        assert_eq!(footer("disk", 6, false, None), "6 results");
        assert_eq!(footer("disk", 0, false, None), "No matches. Try fewer words, or a ticket number like 123.");
        assert_eq!(footer("> x", 0, false, None), "No matching commands. Try fewer words, or delete > to search tickets.");
        assert_eq!(footer("disk", 2, false, Some("Jira search failed: boom")), "Jira search failed: boom");
    }

    #[test]
    fn every_group_has_one_label() {
        let t = |k: &str, text_only| Hit::Issue { key: k.to_string(), text_only };
        let hits = [Hit::Command(Command::Refresh), t("A-1", false), t("A-2", false), t("A-3", true), t("A-4", true)];
        assert_eq!(group_runs(&hits, false), vec![(0, "Commands"), (1, "Tickets"), (3, "Matched in descriptions or comments")]);
        assert_eq!(group_runs(&[t("A-1", false), t("A-2", false)], true), vec![(0, "Recently updated")]);
        assert_eq!(group_runs(&[t("A-1", false)], false), vec![(0, "Tickets")], "a single group keeps its label");
        assert!(group_runs(&[], false).is_empty());
    }

    #[test]
    fn the_cursor_hit_is_clamped() {
        let hits = [Hit::Command(Command::Refresh), Hit::Issue { key: "A-1".into(), text_only: false }];
        assert_eq!(cursor_hit(&hits, 0), Some(&hits[0]));
        assert_eq!(cursor_hit(&hits, 9), Some(&hits[1]), "past the end: the last row");
        assert_eq!(cursor_hit(&[], 0), None);
    }

    #[test]
    fn rows_grow_with_density() {
        assert_eq!(row_h(&theme::COMPACT), ROW_H);
        assert!(row_h(&theme::COMFORTABLE) > ROW_H, "comfortable rows grow like the other lists");
    }
}
