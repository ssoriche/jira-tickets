#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adf;
mod app;
mod auth;
mod avatar;
mod cache;
mod config;
mod filter;
mod grooming;
mod jira;
mod keys;
mod me;
mod report;
mod roadmap;
#[cfg(feature = "snapshot")]
mod snapshot;
mod standup;
mod store;
mod teamview;
mod theme;
mod views;

fn main() -> eframe::Result {
    let path = config::path();
    let cfg = match config::load(&path) {
        Ok(c) => c,
        Err(e) if !path.exists() => {
            if let Err(w) = config::write_example(&path) {
                eprintln!("Error writing example config: {w}");
                std::process::exit(1);
            }
            println!("Welcome to Jira Tickets!\n\nA config file has been created at:\n  {}\n", path.display());
            println!("Edit it: set site and email, add an api_token (plain, op://…, or $(cmd)) or export JIRA_API_TOKEN,");
            println!("then run jira-tickets again.\n\n  $EDITOR {}\n\n({e})", path.display());
            return Ok(());
        }
        Err(e) => {
            eprintln!("Error loading config: {e}\n\nConfig file: {}", path.display());
            std::process::exit(1);
        }
    };
    #[cfg(feature = "snapshot")]
    {
        let out = std::env::var("JIRA_TICKETS_SCREENSHOT").ok();
        if out.is_some() || std::env::var_os("JIRA_TICKETS_SCRIPT").is_some() {
            snapshot::run(out.as_deref(), cfg);
            return Ok(());
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jira Tickets")
            .with_app_id("jira-tickets")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native("jira-tickets", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, cfg)))))
}

#[cfg(test)]
pub mod testutil {
    /// Tests that set process env vars lock this so they never interleave.
    pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs `f` in a pass of a headless context with the app's theme and fonts (they load on the pass after
    /// `theme::apply`, so one empty pass goes first).
    pub fn with_ui<R>(f: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), ..Default::default() };
        ctx.run_ui(input(), |_| {}).textures_delta.clear();
        let mut f = Some(f);
        let mut out = None;
        ctx.run_ui(input(), |ui| {
            if let Some(f) = f.take() {
                out = Some(f(ui));
            }
        })
        .textures_delta
        .clear();
        out.expect("one pass")
    }

    /// Paints `f` headless at the captures' 2 px per point with the app's theme and fonts, and returns the last
    /// pass's shapes (three passes, so fonts, density and sizes settle). Measure with `inks`, `ink_of` and `fills`.
    pub fn paint_at2(size: egui::Vec2, f: impl FnMut(&mut egui::Ui)) -> Vec<egui::epaint::ClippedShape> {
        paint_frames_at2(size, 3, f)
    }

    /// `paint_at2` over `frames` frames of one context. Without a clock in the input egui advances 1/60 s a
    /// frame, so 30 frames let a scroll-to animation (at most 0.3 s) finish.
    pub fn paint_frames_at2(size: egui::Vec2, frames: usize, mut f: impl FnMut(&mut egui::Ui)) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        ctx.set_pixels_per_point(2.0);
        // A focused window, so focused fields paint their caret.
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), focused: true, ..Default::default() };
        let mut shapes = Vec::new();
        for _ in 0..frames {
            let mut o = ctx.run_ui(input(), &mut f);
            o.textures_delta.clear();
            shapes = o.shapes;
        }
        shapes
    }

    /// Every painted text's (text, ink rect in points), in paint order.
    pub fn inks(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match s {
                egui::Shape::Text(t) => {
                    let r = crate::theme::ink_rect(&t.galley);
                    if r.is_positive() {
                        // The tessellator puts a galley on whole pixels (2x here).
                        let pos = (t.pos.to_vec2() * 2.0).round() / 2.0;
                        out.push((t.galley.job.text.clone(), r.translate(pos)));
                    }
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    /// The ink rect of the first painted text equal to `text`.
    pub fn ink_of(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Rect {
        inks(shapes).into_iter().find(|(t, _)| t == text).unwrap_or_else(|| panic!("{text:?} not painted")).1
    }

    /// Every painted rect's (rect, fill), in paint order.
    pub fn fills(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, egui::Color32)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(egui::Rect, egui::Color32)>) {
            match s {
                egui::Shape::Rect(r) => out.push((r.rect, r.fill)),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }
}
