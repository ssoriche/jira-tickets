//! The ⌘K palette: a field and its rows, drawn over every screen.

use crate::app::App;
use crate::palette::{self, Hit};
use crate::theme::{self, Palette};
use egui::{Align, Color32, Layout, Sense, UiBuilder, vec2};

/// The palette's content width: wider than the pickers' DIALOG_W, so titles fit.
pub const PALETTE_W: f32 = 600.0;
/// One row: 14 px text plus padding, like the picker rows.
const ROW_H: f32 = 32.0;
/// The key column; PRS-12345 in 12 px mono fits.
const KEY_W: f32 = 84.0;
/// A row's text inset, the pickers' ROW_INSET.
const INSET: f32 = crate::views::pickers::ROW_INSET;
/// The rows' most height before they scroll: ten rows.
const LIST_MAX_H: f32 = ROW_H * 10.0;

const HINT: &str = "Search tickets, type a key or number, or > for commands";

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
    let top = crate::views::pickers::dialog_top(ctx.content_rect().height());
    let mut confirm = None;
    egui::Area::new(egui::Id::new("palette")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        crate::views::components::dialog_frame(p).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            crate::views::dialog_fields(ui, p);
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
            let searched = pal.remote_for.as_deref() == Some(q.as_str());
            let note = footer(&q, hits.len(), pal.pending, searched, pal.error.as_deref());
            if !hits.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(LIST_MAX_H).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut text_header = false;
                    for (i, hit) in hits.iter().enumerate() {
                        if matches!(hit, Hit::Issue { text_only: true, .. }) && !text_header {
                            text_header = true;
                            section(ui, p, "Matched in descriptions or comments");
                        }
                        let r = row(ui, app, hit, i == cursor, p);
                        if i == cursor && scroll {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            confirm = Some(i);
                        }
                    }
                });
            }
            ui.add_space(8.0);
            let sp = theme::spacing(ui.ctx());
            ui.horizontal(|ui| {
                ui.add_space(INSET);
                ui.label(egui::RichText::new(note).size(sp.small).color(crate::views::dialog_muted(p)));
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

/// A small muted label over a group of rows.
fn section(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(INSET);
        ui.label(egui::RichText::new(text).size(12.0).color(crate::views::dialog_muted(p)));
    });
    ui.add_space(4.0);
}

/// One row: a ticket's key, title and status pill, or a command's name and its own key. The cursor row takes
/// the dialog's cursor fill; hover only tints it, so a resting mouse never fights the keys.
fn row(ui: &mut egui::Ui, app: &App, hit: &Hit, on: bool, p: &Palette) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
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
    // The right-hand part first, so the title knows how much room is left.
    let mut right = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    let (key, title) = match hit {
        Hit::Issue { key, .. } => {
            let issue = app.store.get(key);
            if let Some(i) = issue
                && !i.status.name.is_empty()
            {
                let t = theme::status_tint(&i.status.name, &i.status.category, p, &app.cfg.issues.status_colors);
                crate::views::components::status_pill(&mut right, &i.status.name, t);
            }
            let title = issue.map(|i| i.summary.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Open this ticket".to_string());
            (key.clone(), title)
        }
        Hit::Command(c) => {
            let (name, keys) = palette::command_name(*c);
            right.label(egui::RichText::new(keys).font(theme::mono(12.0)).color(muted));
            ("".to_string(), name.to_string())
        }
    };
    let right_w = inner.right() - right.min_rect().left();
    let painter = ui.painter_at(rect);
    let key_g = painter.layout_no_wrap(key, theme::mono(12.0), muted);
    painter.galley(egui::pos2(inner.left(), rect.center().y - key_g.size().y / 2.0), key_g, muted);
    let lead = if matches!(hit, Hit::Command(_)) { 0.0 } else { KEY_W };
    let room = (inner.width() - lead - right_w - 12.0).max(0.0);
    let mut job = egui::text::LayoutJob::single_section(title, egui::TextFormat { font_id: theme::body(14.0), color: p.text, ..Default::default() });
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let g = painter.layout_job(job);
    painter.galley(egui::pos2(inner.left() + lead, rect.center().y - g.size().y / 2.0), g, p.text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The line under the rows: what an empty box offers, that Jira is being asked, why nothing matched, or how
/// many rows there are and where they came from.
pub fn footer(query: &str, n: usize, pending: bool, searched: bool, error: Option<&str>) -> String {
    if let Some(e) = error {
        return e.to_string();
    }
    if query.is_empty() {
        return "Recently updated. Type a title, a key like 123, or > for commands.".to_string();
    }
    let rows = if n == 1 { "1 result".to_string() } else { format!("{n} results") };
    match (pending, searched, n) {
        (true, _, 0) => "Searching Jira…".to_string(),
        (true, _, _) => format!("{rows} · searching Jira…"),
        (false, true, 0) => "Nothing in titles, descriptions or comments matches.".to_string(),
        (false, true, _) => format!("{rows} · Jira full text included"),
        (false, false, 0) => "Nothing matches.".to_string(),
        (false, false, _) => rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_footer_says_what_the_rows_are() {
        assert_eq!(footer("", 8, false, false, None), "Recently updated. Type a title, a key like 123, or > for commands.");
        assert_eq!(footer("disk", 0, true, false, None), "Searching Jira…");
        assert_eq!(footer("disk", 3, true, false, None), "3 results · searching Jira…");
        assert_eq!(footer("disk", 1, false, true, None), "1 result · Jira full text included");
        assert_eq!(footer("disk", 0, false, true, None), "Nothing in titles, descriptions or comments matches.");
        assert_eq!(footer("> x", 0, false, false, None), "Nothing matches.");
        assert_eq!(footer("disk", 2, false, false, Some("Jira search failed: boom")), "Jira search failed: boom");
    }
}
