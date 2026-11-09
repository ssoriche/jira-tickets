//! Keyboard shortcuts: each frame's key presses become `Action`s.

use super::actions::Action;
use super::pickers::Picker;
use super::{App, GroomMode, Screen};
use crate::grooming::{Code, Verdict, bounce_comment_ok};
use egui::{Key, KeyboardShortcut, Modifiers};

impl App {
    /// The palette's keys: ↑/↓ (or ⌃N/⌃P) move, Enter opens, ⌘Enter opens in Jira, Esc closes. ⌘Enter is
    /// read before Enter, which would also match it.
    fn palette_shortcuts(&mut self, ctx: &egui::Context) {
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if shortcut(Modifiers::NONE, Key::Escape) {
            self.actions.push(Action::TogglePalette);
            return;
        }
        if shortcut(Modifiers::NONE, Key::ArrowDown) || shortcut(Modifiers::CTRL, Key::N) {
            self.actions.push(Action::PaletteMove(1));
        }
        if shortcut(Modifiers::NONE, Key::ArrowUp) || shortcut(Modifiers::CTRL, Key::P) {
            self.actions.push(Action::PaletteMove(-1));
        }
        if shortcut(Modifiers::COMMAND, Key::Enter) {
            self.actions.push(Action::PaletteConfirm { browser: true });
        } else if shortcut(Modifiers::NONE, Key::Enter) {
            self.actions.push(Action::PaletteConfirm { browser: false });
        }
    }

    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let screens = [
            (Key::Num1, Screen::Issues),
            (Key::Num2, Screen::Grooming),
            (Key::Num3, Screen::Team),
            (Key::Num4, Screen::Roadmap),
            (Key::Num5, Screen::Me),
            (Key::Num6, Screen::Standup),
        ];
        for (k, s) in screens {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, k))) {
                self.actions.push(Action::SetScreen(s));
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::R))) {
            self.actions.push(Action::Refresh);
        }
        // Global like ⌘R: ⌘ keeps them off the plain `t` and `d` keys.
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::T))) {
            self.actions.push(Action::CycleTheme);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::D))) {
            self.actions.push(Action::ToggleDensity);
        }
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        // ⌘K on every screen, over help too; an open picker keeps the keyboard until it closes.
        if self.picker.is_none() && shortcut(cmd, Key::K) {
            self.actions.push(Action::TogglePalette);
            return;
        }
        // The palette owns the keyboard while open; its field has focus, so letters type there.
        if self.palette.is_some() {
            self.palette_shortcuts(ctx);
            return;
        }
        // Help owns the keyboard while open, ahead of pickers and the Issues keys.
        if self.help_open {
            if shortcut(Modifiers::NONE, Key::Escape) || shortcut(Modifiers::NONE, Key::Questionmark) {
                self.actions.push(Action::ToggleHelp);
            }
            return;
        }
        // `?` is Shift+/; its logical key is Questionmark, so it never reads as `/`.
        if !ctx.text_edit_focused() && shortcut(Modifiers::NONE, Key::Questionmark) {
            self.actions.push(Action::ToggleHelp);
            return;
        }
        // An open picker owns the keyboard. Assign's filter box has focus, so
        // j/k type there; Edit has text fields, so only Esc and Enter apply.
        if let Some((edit, assign)) = self.picker.as_ref().map(|p| (matches!(p, Picker::Edit { .. }), matches!(p, Picker::Assign { .. }))) {
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.actions.push(Action::PickerCancel);
                return;
            }
            // Edit's fields are single-line, so Enter confirms from any of them (Tab moves between them).
            if edit {
                if shortcut(Modifiers::NONE, Key::Enter) {
                    self.actions.push(Action::PickerConfirm);
                }
                return;
            }
            let letters = !assign;
            if shortcut(Modifiers::NONE, Key::ArrowDown) || (letters && shortcut(Modifiers::NONE, Key::J)) {
                self.actions.push(Action::PickerMove(1));
            }
            if shortcut(Modifiers::NONE, Key::ArrowUp) || (letters && shortcut(Modifiers::NONE, Key::K)) {
                self.actions.push(Action::PickerMove(-1));
            }
            if shortcut(Modifiers::NONE, Key::Enter) {
                self.actions.push(Action::PickerConfirm);
            }
            return;
        }
        if self.screen == Screen::Grooming {
            self.grooming_shortcuts(ctx);
            return;
        }
        // Team's `l` is the next column, never the Issues screen's team label.
        if self.screen == Screen::Team {
            self.team_shortcuts(ctx);
            return;
        }
        if self.screen == Screen::Me {
            self.me_shortcuts(ctx);
            return;
        }
        // Roadmap's plain 1 to 4 are its tabs; ⌘1 to ⌘6 were taken above.
        if self.screen == Screen::Roadmap {
            self.roadmap_shortcuts(ctx);
            return;
        }
        if self.screen == Screen::Standup {
            self.standup_shortcuts(ctx);
            return;
        }
        if self.screen != Screen::Issues {
            return;
        }
        if shortcut(cmd, Key::F) {
            self.focus_search = true;
        }
        let browse = self.selected.as_ref().map(|k| self.cfg.browse_url(k));
        if shortcut(cmd, Key::O)
            && let Some(u) = &browse
        {
            self.actions.push(Action::OpenUrl(u.clone()));
        }
        // Plain keys belong to a focused text field.
        if ctx.text_edit_focused() {
            return;
        }
        // A search the box ran stays clearable with the box unfocused (the hint bar keeps "Esc clear").
        if crate::views::issues::esc_clears_search(self.search_active, self.typed_search, self.text_search.is_some()) && shortcut(Modifiers::NONE, Key::Escape)
        {
            self.filter.text.clear();
            self.actions.push(Action::RunSearch(String::new()));
        }
        if shortcut(Modifiers::NONE, Key::Slash) {
            self.focus_search = true;
        }
        // Before plain j/k: consume_key(NONE, J) also matches Shift+J.
        if shortcut(Modifiers::SHIFT, Key::J) {
            self.actions.push(Action::Rank(1));
        }
        if shortcut(Modifiers::SHIFT, Key::K) {
            self.actions.push(Action::Rank(-1));
        }
        for (k, a) in [
            (Key::T, Action::OpenTransition),
            (Key::A, Action::OpenAssign),
            (Key::S, Action::OpenSprint),
            (Key::E, Action::OpenEdit),
            (Key::L, Action::ApplyTeamLabel),
            (Key::M, Action::ToggleRoadmap),
        ] {
            if shortcut(Modifiers::NONE, k) {
                self.actions.push(a);
            }
        }
        if shortcut(Modifiers::NONE, Key::J) | shortcut(Modifiers::NONE, Key::ArrowDown) {
            self.actions.push(Action::Move(1));
        }
        if shortcut(Modifiers::NONE, Key::K) | shortcut(Modifiers::NONE, Key::ArrowUp) {
            self.actions.push(Action::Move(-1));
        }
        if shortcut(Modifiers::NONE, Key::C)
            && let Some(u) = browse
        {
            self.actions.push(Action::Copy(u));
        }
    }
}

impl App {
    /// The Grooming screen's keys (`keys::Group::Grooming`).
    fn grooming_shortcuts(&mut self, ctx: &egui::Context) {
        let key = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, k)));
        // The overwrite confirm, then the summary dialog, own the keyboard while open.
        if self.grooming.pending_report.is_some() {
            if key(Key::Enter) {
                self.actions.push(Action::Overwrite(true));
            } else if key(Key::Escape) {
                self.actions.push(Action::Overwrite(false));
            }
            return;
        }
        if self.grooming.summary_open {
            if key(Key::Enter) {
                self.actions.push(Action::Promote);
            } else if key(Key::Escape) {
                self.actions.push(Action::CloseSummary);
            }
            return;
        }
        if self.grooming.mode == GroomMode::Session {
            // The bounce box has focus: only ⌘Enter (when the comment would do) and Esc act.
            if let Some(text) = &self.grooming.bounce_text {
                if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter))) {
                    if bounce_comment_ok(text) {
                        self.actions.push(Action::Bounce);
                    }
                } else if key(Key::Escape) {
                    self.actions.push(Action::CancelBounce);
                }
                return;
            }
            if self.grooming.placement.is_some() {
                if key(Key::ArrowUp) {
                    self.actions.push(Action::PlacementMove(-1));
                }
                if key(Key::ArrowDown) {
                    self.actions.push(Action::PlacementMove(1));
                }
                if key(Key::Enter) {
                    self.actions.push(Action::Accept);
                } else if key(Key::Escape) {
                    self.actions.push(Action::CancelPlacement);
                }
                return;
            }
        }
        if ctx.text_edit_focused() {
            return;
        }
        if key(Key::Tab) {
            self.actions.push(Action::GroomMode(other_mode(self.grooming.mode)));
        }
        // Shift+J/K re-rank a Ready row in the session; checked before plain j/k, which consume_shortcut(NONE, J)
        // would otherwise match too.
        let shifted = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::SHIFT, k)));
        if self.grooming.mode == GroomMode::Session {
            if shifted(Key::J) {
                self.actions.push(Action::RankReady(1));
            }
            if shifted(Key::K) {
                self.actions.push(Action::RankReady(-1));
            }
        }
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::GroomMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::GroomMove(-1));
        }
        match self.grooming.mode {
            GroomMode::PreCheck => {
                if key(Key::R) {
                    self.actions.push(Action::Verdict(Verdict::Ready));
                }
                for (k, c) in [(Key::W, Code::UnclearWhat), (Key::N, Code::UnclearWhen), (Key::E, Code::Empty)] {
                    if key(k) {
                        self.actions.push(Action::ToggleCode(c));
                    }
                }
                if key(Key::U) {
                    self.actions.push(Action::UndoVerdict);
                }
                if key(Key::Enter) {
                    self.actions.push(Action::OpenSummary);
                }
            }
            GroomMode::Session => {
                // Shift lands on epics with nothing in Planning too. Shift+] arrives as `}` on most layouts; checked
                // before plain ] because consume_shortcut(NONE, ]) also matches Shift+].
                for (keys, delta) in [([Key::CloseBracket, Key::CloseCurlyBracket], 1), ([Key::OpenBracket, Key::OpenCurlyBracket], -1)] {
                    if shifted(keys[0]) || key(keys[1]) {
                        self.actions.push(Action::GroomEpic { delta, skip_empty: false });
                    } else if key(keys[0]) {
                        self.actions.push(Action::GroomEpic { delta, skip_empty: true });
                    }
                }
                for (k, a) in [
                    (Key::Y, Action::StartAccept),
                    (Key::B, Action::StartBounce),
                    (Key::P, Action::Park),
                    (Key::L, Action::ApplyTeamLabel),
                    (Key::A, Action::OpenAssign),
                    (Key::C, Action::StartClose),
                    (Key::S, Action::OpenSprint),
                ] {
                    if key(k) {
                        self.actions.push(a);
                    }
                }
            }
        }
    }
}

impl App {
    /// The Team screen's keys (`keys::Group::Team`).
    fn team_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, k)));
        if key(Key::H) | key(Key::ArrowLeft) {
            self.actions.push(Action::TeamCol(-1));
        }
        if key(Key::L) | key(Key::ArrowRight) {
            self.actions.push(Action::TeamCol(1));
        }
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::TeamMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::TeamMove(-1));
        }
        if key(Key::Enter) {
            self.actions.push(Action::TeamOpen);
        }
        if key(Key::D) {
            self.actions.push(Action::TeamToggleDone);
        }
    }
}

impl App {
    /// The Roadmap screen's keys (`keys::Group::Roadmap`).
    fn roadmap_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, k)));
        for (k, tab) in [(Key::Num1, 1), (Key::Num2, 2), (Key::Num3, 3), (Key::Num4, 4)] {
            if ctx.input_mut(|i| bare_key(i, k)) {
                self.actions.push(Action::RoadmapTab(tab));
            }
        }
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::RoadmapMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::RoadmapMove(-1));
        }
        if key(Key::Enter) {
            self.actions.push(Action::RoadmapOpen);
        }
    }
}

impl App {
    /// The Me screen's keys (`keys::Group::Me`).
    fn me_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)))
            && let Some(key) = self.me_focused()
        {
            self.actions.push(Action::OpenUrl(self.cfg.browse_url(&key)));
        }
        if ctx.text_edit_focused() {
            return;
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, k)));
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::MeMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::MeMove(-1));
        }
        if key(Key::Enter) {
            self.actions.push(Action::MeOpen);
        }
    }

    /// The Standup screen's keys (`keys::Group::Standup`).
    fn standup_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let key = |k: Key| ctx.input_mut(|i| bare_key(i, k));
        let arrow = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, k)));
        if key(Key::J) | arrow(Key::ArrowDown) {
            self.actions.push(Action::StandupMove(1));
        }
        if key(Key::K) | arrow(Key::ArrowUp) {
            self.actions.push(Action::StandupMove(-1));
        }
        for (k, a) in [
            (Key::X, Action::StandupDismiss),
            (Key::C, Action::StandupAdd(crate::standup::Target::CurrentSprint)),
            (Key::N, Action::StandupAdd(crate::standup::Target::NextSprint)),
            (Key::B, Action::StandupAdd(crate::standup::Target::Backlog)),
            (Key::E, Action::StandupEditTitle),
            (Key::O, Action::StandupOpenSource),
            (Key::Y, Action::StandupCopy),
            (Key::R, Action::StandupRun),
        ] {
            if key(k) {
                self.actions.push(a);
            }
        }
    }
}

/// Consumes a press of `k` with no modifier at all: `consume_key` ignores extra Shift and Alt, so ⌥1 or ⇧1 would
/// otherwise read as a plain 1.
fn bare_key(i: &mut egui::InputState, k: Key) -> bool {
    let bare = i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == k && modifiers.is_none()));
    bare && i.consume_key(Modifiers::NONE, k)
}

/// The mode Tab switches to.
fn other_mode(m: GroomMode) -> GroomMode {
    match m {
        GroomMode::PreCheck => GroomMode::Session,
        GroomMode::Session => GroomMode::PreCheck,
    }
}

/// ⌘W closes the window on every screen, signed in or not.
pub(super) fn close_shortcut(ctx: &egui::Context) {
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W))) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
