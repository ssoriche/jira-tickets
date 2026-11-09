# Jira Tickets

A fast native desktop app for working through Jira tickets, built with Rust and egui. It keeps a local cache so it opens instantly, searches and filters the team's issues from the keyboard, and shows each issue's description and comments with Jira's rich text rendered. The Grooming screen walks the New column and runs grooming sessions, the Team screen shows what each member is working on, the Me screen shows your own tickets, and the Roadmap screen turns the project's epics into a Now / Next / Later board, a quarter timeline, a status review, and outcomes by metric.

## Install

### Nix

    nix run github:ssoriche/jira-tickets
    nix profile install github:ssoriche/jira-tickets

On macOS the package also contains `Applications/Jira Tickets.app`, so app launchers that index a nix or flox profile's `Applications` folder can find it.

### Cargo

    cargo install --git https://github.com/ssoriche/jira-tickets

On Linux you need a C compiler and the X11 or Wayland libraries.

### Download

The [Releases page](https://github.com/ssoriche/jira-tickets/releases) has a macOS app for Apple Silicon (a zip) and a Linux x86_64 tarball, each with a `.sha256` checksum file. Check one with `shasum -a 256 -c <file>.sha256`.

The macOS app is ad-hoc signed, not notarised, so Gatekeeper blocks it on first launch. Either unzip it and clear the quarantine flag with `xattr -dr com.apple.quarantine "Jira Tickets.app"` (or download with a tool that sets no quarantine flag, such as `curl`), or open it once, then allow it under System Settings, Privacy & Security, "Open Anyway". The app has no icon yet.

## Develop

The toolchain comes from the nix dev shell (`nix develop`) or from rustup; with nix, run cargo as `nix develop --command cargo ...`.

    nix develop --command cargo run --release

To build a macOS app bundle and install it (default `/Applications`, or pass a folder), or only build and sign it under `target/release` with `--no-install`:

    scripts/bundle-macos.sh
    scripts/bundle-macos.sh ~/Applications
    scripts/bundle-macos.sh --no-install

The bundle is ad-hoc signed and has no icon yet.

Before every commit run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, the same with `--features snapshot`, and `cargo test`; all must be clean. CI runs fmt, clippy, tests and release builds on macOS and Linux plus `nix build`; releases are automatic from Conventional Commits (`fix:` is a patch, `feat:` a minor, `!` or a `BREAKING CHANGE` footer a major).

## Config

The config lives at `~/.config/jira-tickets/config.toml` (override with `JIRA_TICKETS_CONFIG`). On first run the app writes an example there and exits; edit it, then launch again. Set `site` and `email`, and adjust the scope (projects, team label, board or filter), team members, issue columns, and quick filters.

The Issues view loads every sub-team of the parent team listed under `[[scope.teams]]` (each with a `label`, `name`, and `board_id`; the defaults are PLAT · Platform and TOOLS · Developer Tools), plus `parent_board_id` for the parent board. In `[scope]`, `filter_id` is the PLAT board quick filter used for the report's board and epic links, and `parent_board_id` is the parent-team board linked in the report header. `team_label` stays your own team and `board_id` the sprint board. A ticket counts as a team's when it carries the team's label or is assigned to one of the team's `members` (Jira display names or account ids, matched case-insensitively). Your own team's `members` default to `[team].members`; other teams have none unless you list them.

Status pills are coloured by status name (falling back to the status category) and each epic gets a stable colour derived from its key, shown on its pills and as the left edge of each list row. Override either with `[issues.status_colors]` (status name, case-insensitive) or `[issues.epic_colors]` (epic key), mapping to `"#rrggbb"`.

`[appearance]` sets the look: `theme` is `system` (follow macOS, the default), `light`, or `dark`, and `density` is `compact` (the default) or `comfortable` (taller rows, more padding, slightly larger text). Switch either in the app with `⌘⇧T` and `⌘⇧D`. The app remembers that choice across restarts until you edit `[appearance]`; then the config wins again.

Roadmap settings name custom fields by id, such as `customfield_12345`. To find them, open `<site>/rest/api/3/field` in a browser while signed in to Jira and look for the field names.

## Signing in

The app looks for an API token in this order:

1. The `JIRA_API_TOKEN` environment variable. `JIRA_EMAIL` overrides the config's email.
2. `api_token` in the config. It can be a plain value, `op://vault/item/field` (the app runs `op read`), or `$(command)` (the app runs the command). It is resolved on every launch and never written to disk. If `op` knows more than one 1Password account (it fails with "multiple accounts found"), set `op_account` in the config to the account's sign-in address or email.
3. A token saved earlier from the paste prompt, at `~/Library/Application Support/jira-tickets/token` on macOS or `~/.config/jira-tickets/token` on Linux (mode 0600).
4. The paste prompt, where you paste an Atlassian API token.

Only a token you paste is saved. Signing out deletes that file.

## Keyboard

<!-- keys:start -->
| Keys | Action |
| --- | --- |
| **Global** | |
| `⌘1` to `⌘6` | Issues, Grooming, Team, Roadmap, Me, Standup |
| `⌘R` | Refresh |
| `⌘⇧T` | Theme: System, Light, or Dark |
| `⌘⇧D` | Density: Compact or Comfortable |
| `⌘W` | Close the window |
| `D` | Move the focused ticket to Done |
| `⌘K` | Search tickets, open one by key or number, or run a command |
| `Esc` | Cancel or close |
| `?` or `Esc` | Close this help |
| **Issues** | |
| `⌘O` | Open in Jira |
| `⌘F` or `/` | Focus search |
| `j/k` or `↑/↓` | Next or previous issue |
| `t` | Transition |
| `a` | Assign |
| `s` | Sprint |
| `e` | Edit labels, priority, or story points |
| `l` | Add the picked team's label |
| `m` | Toggle Roadmap filter |
| `J/K` | Move down or up in rank |
| `c` | Copy the issue link |
| `⌘Enter` | Post the comment |
| **Grooming** | |
| `j/k` or `↑/↓` | Next or previous ticket |
| `r` | Ready |
| `w/n/e` | Unclear what, unclear when, or empty |
| `u` | Undo the last verdict |
| `Enter` | Pre-check summary and Promote |
| `y` | Accept into Ready for Work (↑/↓ place, Enter commits) |
| `b` | Bounce with a comment (⌘Enter posts) |
| `p` | Park in Backlog |
| `J/K` | Move a Ready for Work ticket down or up in rank |
| `l` | Add the team label to the focused ticket |
| `a` | Assign the focused ticket |
| `c` | Close the focused ticket as Done or Won't Do |
| `s` | Move the focused ticket to a sprint or the backlog |
| `]/[` or `⇧]/⇧[` | Next or previous epic (with Shift, empty ones too) |
| `Tab` | Switch between Pre-check and Session |
| **Team** | |
| `h/l` or `←/→` | Previous or next member |
| `j/k` or `↑/↓` | Next or previous ticket |
| `Enter` | Open in Issues |
| `d` | Show or hide done in the last 7 days |
| **Roadmap** | |
| `1` to `4` | Now / Next / Later, Timeline, Status review, Outcomes |
| `j/k` or `↑/↓` | Next or previous initiative |
| `Enter` | Open in Issues |
| **Me** | |
| `j/k` or `↑/↓` | Next or previous ticket |
| `Enter` | Open in Issues |
| `⌘O` | Open in Jira |
| **Standup** | |
| `j/k` or `↑/↓` | Next or previous ticket or suggestion |
| `c` | Add to the current sprint |
| `n` | Add to the next sprint |
| `b` | Add to the backlog |
| `x` | Dismiss for good |
| `e` | Edit the title before adding |
| `o` | Open the source |
| `y` | Copy the standup |
| `r` | Run the review again |
| **Pickers and Edit** | |
| `j/k` or `↑/↓` | Move |
| `Tab/⇧Tab` | Next or previous field |
| `Enter` | Confirm |
| `⌘Enter` | Open the ⌘K result in Jira |
<!-- keys:end -->

Only the Global keys work on every screen: `⌘1`–`⌘6`, `⌘K`, `⌘R`, `⌘W`, `?`, and `Esc`, which cancels or closes whatever is open on the current screen. The Issues keys (`⌘O` among them) apply on the Issues screen only, and the Grooming keys on the Grooming screen only. Plain keys (letters, `/`, `?`) do nothing while a text field has focus. While a picker is open, only `Esc`, the arrow keys (and `j` / `k`, except in the Assign picker), and `Enter` apply (in the Edit dialog, `Tab` and `⇧Tab` move between fields and `Enter` confirms); `?` also works while a picker without a text field is open. Selecting a row shows its detail. `?` (or the "?" button in the header) opens this list in the app, and a bar at the bottom of every screen shows the keys that apply right now. While the help is open only the global keys apply; `?` or `Esc` closes it.

Above the Issues list are a team picker, an epic picker, a sprint picker, the config's quick filters, a Mine toggle, a Roadmap toggle, and the search box. The Roadmap toggle (`m`) shows only tickets under epics labelled `roadmap`; epics themselves are containers and never appear in the list. The sprint picker offers All sprints, the board's active and future sprints, and Backlog (issues in no active or future sprint, including ones only ever in closed sprints, as on the Jira board). It starts on the active sprint, or All sprints if the board has none.

The search box filters the rows on screen as you type. Pause on two or more characters and it also asks Jira, like the board's text box: `text ~ "<term>*"` within the current team, quick filter, sprint, and epic, which matches descriptions and comments too; the count then reads "N issues · text match". Press Enter to run whatever the box holds as JQL instead (the count then reads "· JQL"); JQL (anything with `=`, ` in `, `~`, or `!`) is never text-searched. Esc clears the box.

The team picker offers All teams, each configured team, and No team label (tickets that match no team by label or membership). It starts on your own team. No team label asks Jira for those tickets directly (no team label, and not assigned to any team's members), so it also finds tickets outside the loaded team lists; picking a team again goes back to the team list. Under All teams each row shows its team label; the epic picker lists only the epics of the picked team's tickets. When a ticket is in the picked team only through its assignee, its row and detail show an amber "no PLAT label" pill (named for the picked team), and `l` adds that label, keeping the ticket's other labels. The label is never added automatically; `l` on a ticket that already has it just says "Already labelled", and under All teams or No team label it asks you to pick a team. Quick filters follow the team picker: under one team they search that team's label or members, under All teams every team's labels and members, and under No team label only tickets no team claims. Changing team re-runs an active quick filter. If Jira rejects a configured member (a name or account id it does not know), the team queries are re-run on labels alone and stay that way until the app restarts.

Known limitation: the TOOLS board also includes unlabelled tickets assigned to specific people. Those show under TOOLS only once the account ids are listed as TOOLS's `members` (the example config has them commented out); otherwise a label-based filter does not see them.

## Grooming

The Grooming screen (`⌘2`) has two modes, switched with `Tab`: Pre-check and Session.

### Pre-check

Pre-check walks the `New` column in rank order. Give each ticket a verdict: `r` ready, or `w` (unclear what), `n` (unclear when), `e` (empty), which toggle failure codes (a ticket may carry several). `u` undoes the last verdict. `Enter` opens the summary: the ready set in rank order, with tickets beyond `planning_cap` marked as held back, and a Promote button.

Promote performs only `New → Planning` transitions. It sets no fields, no resolution, and posts no comment. Transitions that fail are collected and shown, and the rest continue.

After promotion the app fetches the existing Ready for Work siblings of the promoted epics and then writes the report to `~/Documents/grooming-reports/YYYY-MM-DD.md` (change the directory with `[grooming] report_dir`). If a report for today already exists, the app asks before overwriting it.

The report lists epics in the session's stage order (roadmap first, then no epic, ongoing, other), and each epic heading carries its stage, for example `· roadmap (Now, P0)`.

On launch and refresh the newest report in that directory is read. Its verdicts pre-fill Pre-check once per report file, and tickets show badges such as "needs detail: unclear-when (report 2026-10-07)" or "promoted by pre-check".

### Session

Session mode walks epics in four stages, so the meeting spends its time on the roadmap first: roadmap epics in the Roadmap screen's order (Now, Next, Later; P0 first; then rank), then the team's tickets with no epic, then ongoing epics, then the rest. The left rail shows a header per stage and each epic's counts: `Sp` in the active sprint and `Nx` in the next (left out when zero), `R` Ready for Work, `P` Proposed (Planning) and `N` New. Roadmap epics are always listed, even when the team has no tickets in them. Epics with nothing Proposed, nothing New and nothing Unclaimed are dimmed; `]` and `[` skip them (with Shift they land on them too).

The centre shows the selected epic in five blocks (the cursor walks all of them, sprint rows included, so any ticket can be focused to read its detail): the active sprint's tickets and the next sprint's (context only), the Ready for Work queue in rank order, Proposed and New. A roadmap epic gets a sixth, Unclaimed (`U` on the rail): its open children outside the team's scope, with no team label and not assigned to a member. `l` adds the team label and `a` opens the assign picker on any focused row; the next refresh moves the ticket into its queue. `j`/`k` walk the sprint, Ready, Proposed and New rows as one list; entering an epic starts on its first Proposed or New ticket. For a Proposed or New ticket:

- `y` accepts it: `↑`/`↓` move a ghost row through the Ready queue and `Enter` commits the move to Ready for Work at that place, from Planning or straight from New. `Esc` cancels.
- `b` bounces it: type a one- or two-line comment and press `⌘Enter`. A Planning ticket goes back to New after the comment posts; a New ticket gets the comment only.
- `p` parks it in Backlog.

For any focused row:

- `c` closes it: a picker offers the configured closing resolutions (Done, Won't Do) and Enter moves it to `status_closed` with that resolution.
- `s` opens the sprint picker: Backlog, the active sprint or a future sprint; Enter moves it and the row hops to its new block.

For a Ready for Work row, `J` and `K` move it one place down or up in rank with a single rank call (the row swaps at once and swaps back if the call fails).

A timer runs per candidate and turns amber at 3 minutes and red at 5. The Ready ceiling pill in the header reads "Ready for Work N · soft S · hard H", counts the team's (PLAT-labelled) Ready for Work tickets, and turns amber past the soft ceiling (10) and red past the hard one (15). Both limits are configurable under `[grooming]`, and warnings never block anything.

### How to groom

Pre-check: before the meeting. Judge each New ticket against the readiness bar, then promote the ready set to Planning and write the report. The bar is one question: can a stranger tell WHAT is being done and WHEN it is done? `r` yes, ready; `w` unclear what is being done; `n` unclear when it is done; `e` empty, nothing to judge. When in doubt, pass. Promote moves the ready set to Planning (the top `planning_cap` by rank) and writes today's report; paste the report into the team channel.

Session: in the meeting. Walk the roadmap epics first, then tickets with no epic, ongoing epics and the rest; accept each Proposed or New ticket into the epic's Ready for Work queue in rank. Decide each ticket: `y` Ready for Work, accept and place it in rank; `b` bounce with a one-line comment on what's missing, Planning goes back to New; `p` Backlog, real work, lower priority. `J`/`K` move a Ready for Work ticket down or up in rank. An Unclaimed ticket is not on the team's map yet: `l` labels it PLAT, `a` assigns it. `c` Close: Done, or Won't Do when the work no longer matters. `s` Sprint: move it into a sprint or back to the backlog. Three to five minutes per ticket. Past that, bounce.

## Team

The Team screen (`⌘3`) has one column per member listed in `[team].members`, in that order, then an Unassigned column.

- Each member column is headed by the member's name (as Jira shows it) and their counts, "2 in progress · 3 ready". A member no list matched reads "no tickets found", which usually means `[team].members` holds a name Jira does not know.
- The cards come in two groups: first the member's tickets in the In Progress status category across the scope's `projects` (any status in that category, any label), then the team's Ready for Work tickets assigned to them. Each card shows the key, the summary, the status and epic pills, and the days since the ticket's status last changed ("4 d").
- At the foot of each member column, "Done in the last 7 days" opens a list of the tickets they resolved in the past week (loaded when first opened).
- The Unassigned column shows the top of the team's Ready for Work queue with no assignee, up to 10 tickets in rank order.

`h`/`l` or `←`/`→` move between columns, `j`/`k` or `↑`/`↓` between cards, `d` opens or closes the focused member's done list, and `Enter` (or a double click) opens the ticket on the Issues screen.

Team columns cover only the people in `[team].members`. The TOOLS board also includes unlabelled tickets by assignee, and those people have no column here unless you list them.

## Roadmap

The Roadmap screen (`⌘4`) treats the project's epics as initiatives. If any epic carries `initiative_label` (default `roadmap`), the initiatives are the epics with that label. If none does, every epic whose summary lacks "(ongoing)" counts. Epics whose summary contains "(ongoing)" are keep-the-lights-on work: they are never initiatives and are listed in a footnote under the Now column. The epics come from the configured lane labels, the initiative label, the parents of the team's tickets, and the optional `[roadmap] epics` allowlist.

Each initiative's progress bar counts all of its children by status category (done, in progress, to do), Done ones included. Values that are not set show as small red "missing", "not set" or "no update yet" flags; they are the to-do list.

Four tabs, picked with `1` to `4`:

1. **Now / Next / Later**: three columns of cards, P0 first and then worst health. Now cards show a health dot and the latest update. Below them, "Shipped in {year}" lists the Done initiatives targeted this year with their target quarter, plus any Done initiative with no target, flagged "target not set".
2. **Timeline**: a row per lane and a column per quarter of the current year, with a today line. Each bar runs from the start quarter to the target quarter. Done bars are muted and marked "✓", and P0 bars have a red left edge. Initiatives with no target quarter sit in the Unscheduled tray below.
3. **Status review**: tiles counting Now initiatives by health and how many have an owner, then a table of Now initiatives, worst health first: initiative (with lane, key and owner), health, confidence, latest update, next milestone, definition of done, and progress.
4. **Outcomes**: one card per `metric:` label, listing the Now initiatives that move it and the later ones, and "Nothing in progress moves this metric." when none is in Now.

`j`/`k` or `↑`/`↓` move between initiatives on tabs 1 and 3, and `Enter` (or a double click) opens the epic on the Issues screen.

### Field mapping

Roadmap time is quarters, not dates. All settings live under `[roadmap]` in the config; custom fields are named by id (see Config).

| Concept | Default source | Override (`[roadmap]`) |
| --- | --- | --- |
| Horizon | `horizon_by_status` maps status names to `now`, `next` or `later`; otherwise Done category → Done, In Progress category → Now, the rest → Later | Label `horizon:now`, `horizon:next` or `horizon:later` wins over the status |
| Health | Label `health:on-track`, `health:at-risk` or `health:off-track`; none → no health | `health_field` (used when no `health:` label is set) |
| Priority chip | Jira priority Highest → P0, High → P1 | Label `P0` or `P1` wins |
| Target quarter | Label `target:YYYY-Qn`, for example `target:2026-Q4` | `target_field`: `duedate` or a custom field holding a date or `YYYY-Qn`; a date maps to its quarter. When set, the label is not read |
| Start quarter | Label `start:YYYY-Qn`; unset means the bar covers the target quarter only | `start_field`, as `target_field` |
| Initiative set | Label `initiative_label` (default `roadmap`); with no epic labelled, every epic without "(ongoing)" | `initiative_label` |
| Lane | `lanes` maps a label to a lane name (default `PLAT = "Platform"`); epics with no lane label go to "No subteam" | Add entries to `lanes` |
| Latest update | Newest epic comment: author, date, first paragraph | None |
| Confidence, next milestone, definition of done | Not set | `confidence_field`, `next_milestone_field`, `definition_of_done_field` |
| Metrics | Labels `metric:<name>`, for example `metric:Deployment Frequency` | None |

Labels are the lightweight way to fill the roadmap in from Jira: add `roadmap`, `start:2026-Q2`, `target:2026-Q4`, `health:at-risk`, `horizon:next`, `metric:<name>` or `P0` to an epic and refresh.

### Known gaps

- Apart from the eleven CI epics labelled `roadmap` (with `start:`/`target:` quarters on ten and `P0` on four), epics in the live project carry none of the roadmap labels (`roadmap`, `target:YYYY-Qn`, `start:YYYY-Qn`, `health:`, `horizon:`, `metric:`), have no priorities or roadmap custom fields, and few have comments. So most initiatives show no health, no update and no metric, and an epic without the labels shows "missing" flags (or, in a workspace with no `roadmap` labels at all, every non-"(ongoing)" epic counts as an initiative and sits in the Unscheduled tray). That is by design: the red flags are the to-do list, and the labels are how to clear them.
- The Outcomes tab says so when no epic has a `metric:` label, which is the case today.
- The TOOLS board includes unlabelled tickets by assignee; Team columns only cover members listed in `[team].members`.

## Me

The Me screen (`⌘5`) shows the tickets assigned to you, found by `currentUser()`, so it needs no config. It opens on your in-progress work, in three sections:

- In progress: your tickets in the In Progress status category, in rank order.
- To do in open sprints: your To Do tickets in an active or future sprint, in rank order.
- Done in the last 7 days: what you resolved this past week, newest first.

The cards match the Team screen's. The focused ticket's detail shows on the right. `j`/`k` or `↑`/`↓` move, `Enter` (or a double click) opens the ticket on the Issues screen, and `⌘O` opens it in Jira.

## Standup

The Standup screen (`⌘6`) prepares your standup and finds work no ticket tracks yet. The team meets Monday, Wednesday and Friday, so the review covers everything since the previous standup day: Monday looks back to Friday, Wednesday to Monday, Friday to Wednesday. On other days it looks back to the latest standup day (Tuesday to Monday, Thursday to Wednesday, the weekend to Friday).

On a standup day the review runs by itself once, when the app is open. **Run again** (`r`) runs it on demand any day. A run has two steps:

1. The app reads your Jira activity in the window: issues assigned to you, reported by you, or updated by you, with the status changes, assignments and comments you made.
2. It runs Claude Code headless (`claude -p --model <model> --output-format json`) with the prompt in [`prompts/standup-review.md`](prompts/standup-review.md) and that activity. Claude may only run read-only `gh` commands (search, `pr list`, `pr view`) and the Slack MCP readers; edits and Slack writes are refused. It finds your pull requests, reviews and Slack threads, and answers with a standup and a list of suggested tickets.

The screen shows the standup (Since the last standup, Today, Blockers; **Copy** or `y` copies it as text) and a card per suggestion with its source, the issue type it will be created as, confidence and reason. Edit the title (`e`), then add it to the **Current sprint** (`c`), **Next sprint** (`n`) or **Backlog** (`b`): the ticket is created in `scope.project` with `scope.team_label`, assigned to you, and moved into the active or first future sprint of `scope.board_id`. The card's main button adds to the current sprint whatever the review suggested; when the board has no active sprint it says "No current sprint" and the main button adds to the next sprint. The type is the one the review asked for when the project has it, else a close match (User Story or Feature for Story, Defect for Bug), else Task, else the first type the project allows; the toast says when it differs. When the board has no sprint for the add, or your Jira account can't be read to assign the ticket, nothing is created. `x` dismisses a suggestion for good; the same source never comes back on a rerun. The last review, the dismissals and the added tickets are kept between launches.

Configure the CLI under `[standup]`: `claude` is a path or a name (default `claude`, looked up on PATH and in `~/.local/bin`, `~/.claude/local`, `~/.npm-global/bin`, the flox default environment, the nix profiles, `/opt/homebrew/bin` and `/usr/local/bin`, since an app launched from the Dock has a minimal PATH), `model` (default `claude-opus-5-5`), and `timeout_secs` (default 900). Claude Code must already be signed in, with `gh` authenticated and the Slack MCP server set up for Slack to count.

## Developer switches

- `JIRA_TICKETS_READONLY` blocks every write to Jira.
- `JIRA_TICKETS_PERF` logs request timings.
- `JIRA_TICKETS_CONFIG` points at another config file.
- `JIRA_TICKETS_TOKEN_FILE` points at another saved-token file.
- `JIRA_TICKETS_CACHE_DIR` points at another cache folder.
- `JIRA_TICKETS_THEME` is `system`, `light`, or `dark`, and `JIRA_TICKETS_DENSITY` is `compact` or `comfortable`; each overrides `[appearance]` and the saved choice.

Screenshots need the `snapshot` feature:

    nix develop --command cargo build --release --features snapshot
    JIRA_TICKETS_SCREENSHOT=out.png JIRA_TICKETS_VIEW=issues JIRA_TICKETS_ROW=2 ./target/release/jira-tickets

`JIRA_TICKETS_SCREENSHOT` is the output path and switches the app to an offscreen render at 1440x900, 2x. `JIRA_TICKETS_VIEW` picks `issues`, `grooming`, `team`, `roadmap`, `me`, or `standup` (which runs a review first if none is saved, and waits for it). `JIRA_TICKETS_ROW=<n>` moves the selection by n rows, `JIRA_TICKETS_THEME` picks `light` or `dark` and `JIRA_TICKETS_DENSITY` picks `compact` or `comfortable`; unset, both follow `[appearance]` and the saved choice, and the `system` theme renders light here. Writes are always blocked in this mode. If nothing has loaded after 20 seconds (for example, no token), the picture shows whatever is on screen.

## How it works

The window is egui drawn on the GPU (wgpu), with no web view. All Jira traffic (REST v3 and Agile 1.0) runs on background threads and reports back over a channel, so the interface never waits on the network. One cached store feeds every screen, which is why the app opens instantly and the screens agree with each other. Links open in your browser through the operating system.

## Code map

| File | Role |
| --- | --- |
| `src/main.rs` | Startup, config load, window options |
| `src/app/` | App state, actions, keyboard shortcuts, background work (module directory) |
| `src/auth.rs` | Token lookup, saving, and sign-out |
| `src/config.rs` | Config file, example, secret references |
| `src/jira.rs` | Jira REST client, read-only guard |
| `src/store.rs` | Issue store and list ids |
| `src/cache.rs` | On-disk cache |
| `src/filter.rs` | Search, quick-filter, and sprint-scope logic |
| `src/adf.rs` | Atlassian document format parsing |
| `src/theme.rs` | Colors and styling |
| `src/views/mod.rs` | Screen shell, sign-in, toast, shared widgets |
| `src/views/issues.rs` | Issue list |
| `src/views/detail.rs` | Issue detail pane |
| `src/views/adf_view.rs` | Renders rich text |
| `src/views/pickers.rs` | Transition, assign, sprint, and edit pickers |
| `src/snapshot.rs` | Offscreen screenshots (`snapshot` feature) |
| `.github/workflows/` | CI, and the semantic-release and release-build workflows |
| `scripts/set-version.sh` | Stamps a release version into `Cargo.toml` and `Cargo.lock` |

Grooming lives in `src/views/grooming.rs`, `src/app/groom.rs`, `src/grooming.rs`, and `src/report.rs`. Team lives in `src/teamview.rs` (the columns, pure) and `src/views/team.rs`. Me lives in `src/me.rs` (the query and sections, pure) and `src/views/me.rs`. Roadmap lives in `src/roadmap.rs` (initiatives and the field mapping, pure), `src/views/roadmap.rs` (tabs 1 and 3), and `src/views/roadmap/timeline.rs` and `outcomes.rs` (tabs 2 and 4).
Standup lives in `src/standup.rs` (the window, the Jira activity, the `claude` run and its answer), `src/app/standup.rs`, `src/views/standup.rs`, and the prompt in `prompts/standup-review.md`.

## Licence

MIT, see [LICENSE](LICENSE).
