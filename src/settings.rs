//! `nudge settings`: a terminal UI for auto-notify, who can notify, and the default desk. herdr opens it as an
//! overlay (`weskor.nudge.settings`). It edits the same config and state as the actions and `nudge setup`.
use std::io::IsTerminal as _;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::config::{Auto, Config, Scope, default_config_path, load_config, seconds_in_words, write_config};
use crate::context::{Env, ListedWorkspace, Run as _, System, herdr_agents, herdr_workspaces};
use crate::error::{Blame, Error, Result};
use crate::notify::{Origin, current_dir, ping_desk};
use crate::state::{auto_enabled, update_state};

const TURN_STEPS: [u32; 9] = [0, 30, 60, 120, 180, 300, 600, 900, 1800];
const COOLDOWN_STEPS: [u32; 8] = [0, 60, 120, 300, 600, 900, 1800, 3600];
const TAIL_STEPS: [u32; 6] = [10, 20, 40, 80, 120, 200];

/// The next preset above (`up`) or below `value`; stays put at either end. Values between presets snap to the
/// nearest one in that direction.
fn step(value: u32, steps: &[u32], up: bool) -> u32 {
    if up {
        steps.iter().copied().find(|&preset| preset > value).unwrap_or(value)
    } else {
        steps
            .iter()
            .rev()
            .copied()
            .find(|&preset| preset < value)
            .unwrap_or(value)
    }
}

/// The next preset above `value`, wrapping to the smallest.
fn cycle(value: u32, steps: &[u32]) -> u32 {
    steps
        .iter()
        .copied()
        .find(|&preset| preset > value)
        .or_else(|| steps.first().copied())
        .unwrap_or(value)
}

/// A line the cursor can sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    AutoOn,
    Toast,
    MinTurn,
    Cooldown,
    TailLines,
    Everyone,
    OnlyThese,
    Workspace(usize),
    Agent(usize),
    Desk(usize),
}

/// A titled list of choices in the "Who can notify" section, and the row each entry is on.
type Section<'a> = (&'a str, &'a [Choice], fn(usize) -> Row);

/// A workspace or agent that can be ticked for `[scope]`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    /// What is stored in `[scope]`.
    name: String,
    checked: bool,
    /// Open in herdr right now; `false` for names only the config mentions.
    live: bool,
}

/// A desk as shown: never its URL path or secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DeskRow {
    name: String,
    host: String,
    signed: bool,
}

/// What the user pressed, independent of the terminal library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Up,
    Down,
    Left,
    Right,
    Toggle,
    Test,
    Save,
    Quit,
}

/// What the UI loop should do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Continue,
    Save,
    Quit,
    /// Send a test ping to this desk.
    Test(String),
}

/// Whether there is anything to lose on quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edits {
    None,
    Unsaved,
    /// The user pressed quit once with unsaved edits; the next quit discards them.
    DiscardOnNextQuit,
}

/// The settings being edited. Pure state: no terminal, no files.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    auto_on: bool,
    auto: Auto,
    only: bool,
    workspaces: Vec<Choice>,
    agents: Vec<Choice>,
    desks: Vec<DeskRow>,
    default_desk: Option<String>,
    cursor: usize,
    status: Option<String>,
    edits: Edits,
}

/// Live names first (ticked if `[scope]` names them by any of their `ids`), then names only the config has.
fn choices(live: Vec<(String, Vec<String>)>, saved: &[String]) -> Vec<Choice> {
    let mut list: Vec<Choice> = Vec::new();
    for (name, ids) in live {
        if list.iter().any(|choice| choice.name == name) {
            continue;
        }
        let checked = saved.iter().any(|entry| ids.contains(entry));
        list.push(Choice {
            name,
            checked,
            live: true,
        });
    }
    for entry in saved {
        if !list.iter().any(|choice| &choice.name == entry) {
            list.push(Choice {
                name: entry.clone(),
                checked: true,
                live: false,
            });
        }
    }
    list
}

impl Settings {
    pub(crate) fn new(config: &Config, auto_on: bool, workspaces: &[ListedWorkspace], agents: &[String]) -> Self {
        let live_workspaces = workspaces
            .iter()
            .map(|workspace| {
                (
                    workspace.name().to_owned(),
                    vec![workspace.name().to_owned(), workspace.workspace_id.clone()],
                )
            })
            .collect();
        let mut workspace_choices = choices(live_workspaces, &config.scope.workspaces);
        // A saved workspace id that matched a live workspace is already shown under its name.
        workspace_choices
            .retain(|choice| choice.live || !workspaces.iter().any(|workspace| workspace.workspace_id == choice.name));
        let live_agents = agents
            .iter()
            .map(|agent| (agent.clone(), vec![agent.clone()]))
            .collect();
        Self {
            auto_on,
            auto: config.auto,
            only: !config.scope.is_everyone(),
            workspaces: workspace_choices,
            agents: choices(live_agents, &config.scope.agents),
            desks: config
                .desk
                .iter()
                .map(|desk| DeskRow {
                    name: desk.name.clone(),
                    host: url::Url::parse(&desk.url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .unwrap_or_default(),
                    signed: desk.signing_secret.is_some(),
                })
                .collect(),
            default_desk: crate::config::default_desk_name(config).map(str::to_owned),
            cursor: 0,
            status: None,
            edits: Edits::None,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![
            Row::AutoOn,
            Row::Toast,
            Row::MinTurn,
            Row::Cooldown,
            Row::TailLines,
            Row::Everyone,
            Row::OnlyThese,
        ];
        rows.extend((0..self.workspaces.len()).map(Row::Workspace));
        rows.extend((0..self.agents.len()).map(Row::Agent));
        rows.extend((0..self.desks.len()).map(Row::Desk));
        rows
    }

    fn current(&self) -> Option<Row> {
        self.rows().get(self.cursor).copied()
    }

    fn changed(&mut self) {
        self.edits = Edits::Unsaved;
        self.status = None;
    }

    fn adjust(&mut self, row: Row, up: bool) {
        let auto = &mut self.auto;
        let (value, steps) = match row {
            Row::MinTurn => (&mut auto.min_turn_seconds, &TURN_STEPS[..]),
            Row::Cooldown => (&mut auto.cooldown_seconds, &COOLDOWN_STEPS[..]),
            Row::TailLines => (&mut auto.tail_lines, &TAIL_STEPS[..]),
            Row::AutoOn
            | Row::Toast
            | Row::Everyone
            | Row::OnlyThese
            | Row::Workspace(_)
            | Row::Agent(_)
            | Row::Desk(_) => return,
        };
        let next = step(*value, steps, up);
        if next != *value {
            *value = next;
            self.changed();
        }
    }

    fn toggle(&mut self, row: Row) {
        match row {
            Row::AutoOn => self.auto_on = !self.auto_on,
            Row::Toast => self.auto.toast = !self.auto.toast,
            Row::MinTurn => self.auto.min_turn_seconds = cycle(self.auto.min_turn_seconds, &TURN_STEPS),
            Row::Cooldown => self.auto.cooldown_seconds = cycle(self.auto.cooldown_seconds, &COOLDOWN_STEPS),
            Row::TailLines => self.auto.tail_lines = cycle(self.auto.tail_lines, &TAIL_STEPS),
            Row::Everyone => self.only = false,
            Row::OnlyThese => self.only = true,
            Row::Workspace(index) => {
                let Some(choice) = self.workspaces.get_mut(index) else {
                    return;
                };
                choice.checked = !choice.checked;
                self.only = true;
            }
            Row::Agent(index) => {
                let Some(choice) = self.agents.get_mut(index) else {
                    return;
                };
                choice.checked = !choice.checked;
                self.only = true;
            }
            Row::Desk(index) => {
                let Some(desk) = self.desks.get(index) else { return };
                self.default_desk = Some(desk.name.clone());
            }
        }
        self.changed();
    }

    pub(crate) fn handle(&mut self, key: Key) -> Outcome {
        let armed = self.edits == Edits::DiscardOnNextQuit;
        if armed {
            self.edits = Edits::Unsaved;
        }
        let last = self.rows().len().saturating_sub(1);
        match (key, self.current()) {
            (Key::Up, _) => self.cursor = self.cursor.saturating_sub(1),
            (Key::Down, _) => self.cursor = (self.cursor + 1).min(last),
            (Key::Left | Key::Right, Some(row)) => self.adjust(row, key == Key::Right),
            (Key::Toggle, Some(row)) => self.toggle(row),
            (Key::Test, Some(Row::Desk(index))) => {
                if let Some(desk) = self.desks.get(index) {
                    return Outcome::Test(desk.name.clone());
                }
            }
            (Key::Test, _) => self.status = Some("Move to a desk to test it.".into()),
            (Key::Save, _) => {
                if self.only && !self.workspaces.iter().chain(&self.agents).any(|choice| choice.checked) {
                    self.status = Some("Tick at least one workspace or agent, or choose Every agent.".into());
                } else {
                    return Outcome::Save;
                }
            }
            (Key::Quit, _) => {
                if self.edits == Edits::None || armed {
                    return Outcome::Quit;
                }
                self.edits = Edits::DiscardOnNextQuit;
                self.status = Some("Unsaved changes: press q again to discard them, or s to save.".into());
            }
            (Key::Left | Key::Right | Key::Toggle, None) => {}
        }
        Outcome::Continue
    }

    /// Write the edited settings into `config` (other settings and desks stay as they are).
    pub(crate) fn apply(&self, config: &mut Config) {
        config.auto = self.auto;
        let ticked = |choices: &[Choice]| -> Vec<String> {
            choices
                .iter()
                .filter(|choice| choice.checked)
                .map(|choice| choice.name.clone())
                .collect()
        };
        config.scope = if self.only {
            Scope {
                workspaces: ticked(&self.workspaces),
                agents: ticked(&self.agents),
            }
        } else {
            Scope::default()
        };
        if let Some(name) = &self.default_desk
            && config.desk.iter().any(|desk| &desk.name == name)
        {
            config.default_desk = Some(name.clone());
        }
    }

    pub(crate) const fn auto_on(&self) -> bool {
        self.auto_on
    }

    /// The lines to draw, and which one the cursor is on.
    fn lines(&self) -> (Vec<Line<'static>>, usize) {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let dim = Style::new().add_modifier(Modifier::DIM);
        let check = |on: bool| if on { "[x]" } else { "[ ]" };
        let radio = |on: bool| if on { "(•)" } else { "( )" };
        let value = |label: &str, text: String| Line::from(format!("  {label:<22}◂ {text} ▸"));
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut selected = 0;
        let cursor_row = self.current();
        let mut push = |lines: &mut Vec<Line<'static>>, row: Option<Row>, line: Line<'static>| {
            if row.is_some() && row == cursor_row {
                selected = lines.len();
            }
            lines.push(line);
        };

        push(&mut lines, None, Line::styled("Automatic notifications", bold));
        push(
            &mut lines,
            Some(Row::AutoOn),
            Line::from(format!("  {} Auto-notify", check(self.auto_on))),
        );
        push(
            &mut lines,
            Some(Row::Toast),
            Line::from(format!("  {} Toast when one is sent", check(self.auto.toast))),
        );
        push(
            &mut lines,
            Some(Row::MinTurn),
            value("Shortest turn sent", seconds_in_words(self.auto.min_turn_seconds)),
        );
        push(
            &mut lines,
            Some(Row::Cooldown),
            value("Cooldown per pane", seconds_in_words(self.auto.cooldown_seconds)),
        );
        push(
            &mut lines,
            Some(Row::TailLines),
            value("Terminal lines sent", self.auto.tail_lines.to_string()),
        );
        push(&mut lines, None, Line::default());

        push(&mut lines, None, Line::styled("Who can notify", bold));
        push(
            &mut lines,
            Some(Row::Everyone),
            Line::from(format!("  {} Every agent", radio(!self.only))),
        );
        push(
            &mut lines,
            Some(Row::OnlyThese),
            Line::from(format!("  {} Only these:", radio(self.only))),
        );
        let list_style = if self.only { Style::new() } else { dim };
        let sections: [Section<'_>; 2] = [
            ("Workspaces", &self.workspaces, Row::Workspace),
            ("Agents", &self.agents, Row::Agent),
        ];
        for (label, list, row) in sections {
            push(&mut lines, None, Line::styled(format!("      {label}"), dim));
            if list.is_empty() {
                push(&mut lines, None, Line::styled("        none open in herdr", dim));
            }
            for (index, choice) in list.iter().enumerate() {
                let note = if choice.live { "" } else { "  (not open)" };
                let text = format!("        {} {}{note}", check(choice.checked), choice.name);
                push(&mut lines, Some(row(index)), Line::styled(text, list_style));
            }
        }
        push(&mut lines, None, Line::default());

        push(&mut lines, None, Line::styled("Desk the plugin notifies", bold));
        for (index, desk) in self.desks.iter().enumerate() {
            let is_default = self.default_desk.as_deref() == Some(desk.name.as_str());
            let signed = if desk.signed { "  signed" } else { "" };
            let line = Line::from(vec![
                Span::raw(format!("  {} {}", radio(is_default), desk.name)),
                Span::styled(format!("  {}{signed}", desk.host), dim),
            ]);
            push(&mut lines, Some(Row::Desk(index)), line);
        }
        (lines, selected)
    }
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return (code == KeyCode::Char('c')).then_some(Key::Quit);
    }
    #[expect(clippy::wildcard_enum_match_arm, reason = "every other key does nothing")]
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(Key::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Key::Down),
        KeyCode::Left | KeyCode::Char('h' | '-') => Some(Key::Left),
        KeyCode::Right | KeyCode::Char('l' | '+') => Some(Key::Right),
        KeyCode::Enter | KeyCode::Char(' ') => Some(Key::Toggle),
        KeyCode::Char('t') => Some(Key::Test),
        KeyCode::Char('s') => Some(Key::Save),
        KeyCode::Esc | KeyCode::Char('q') => Some(Key::Quit),
        _ => None,
    }
}

fn draw(terminal: &mut DefaultTerminal, settings: &Settings) -> Result<()> {
    let (lines, selected) = settings.lines();
    let help = "↑↓ move  space pick  ←→ change  t test  s save  q quit";
    let status = settings.status.clone().unwrap_or_default();
    terminal.draw(|frame| {
        let block = Block::bordered().title(" nudge settings ");
        let inner = block.inner(frame.area());
        frame.render_widget(block, frame.area());
        // The status wraps (up to 4 lines) so long errors and their fixes stay readable in narrow overlays.
        let width = usize::from(inner.width.max(1));
        let status_lines = status.chars().count().div_ceil(width).clamp(1, 4);
        let footer_height = u16::try_from(status_lines + 1).unwrap_or(2);
        let [body, status_area, help_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(footer_height - 1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let list = List::new(lines.into_iter().map(ListItem::new))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(selected));
        frame.render_stateful_widget(list, body, &mut state);
        let status =
            Paragraph::new(Line::styled(status, Style::new().add_modifier(Modifier::BOLD))).wrap(Wrap { trim: true });
        frame.render_widget(status, status_area);
        frame.render_widget(Line::styled(help, Style::new().add_modifier(Modifier::DIM)), help_area);
    })?;
    Ok(())
}

/// Test-ping `desk` and describe the result for the status line.
fn test_desk(config: &Config, desk: &str, env: &Env) -> Result<String> {
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    Ok(match ping_desk(config, Some(desk), None, origin) {
        Ok(sent) => format!("✓ \"{desk}\" received a test ping (HTTP {})", sent.http_status),
        Err(Error::Desk(error)) => {
            let hint = match error.blames() {
                Some(Blame::Authorization) => "; copy the Authorization header again with nudge setup",
                Some(Blame::Url) => "; copy the Webhook URL again with nudge setup",
                None => "",
            };
            format!("✗ {}{hint}", error.message)
        }
        Err(other) => return Err(other),
    })
}

/// Save: re-read the config so desks edited meanwhile are kept, apply the edits, write it and the toggle.
fn save(settings: &Settings, env: &Env) -> Result<()> {
    let path = default_config_path(env);
    let mut config = load_config(&path)?;
    settings.apply(&mut config);
    write_config(&path, &config)?;
    let auto_on = settings.auto_on();
    update_state(env, |state| state.auto = auto_on)?;
    Ok(())
}

/// Run the UI until saved (`true`) or quit (`false`).
fn run_ui(terminal: &mut DefaultTerminal, settings: &mut Settings, config: &Config, env: &Env) -> Result<bool> {
    loop {
        draw(terminal, settings)?;
        let Event::Key(pressed) = event::read()? else { continue };
        if pressed.kind != KeyEventKind::Press {
            continue;
        }
        let Some(key) = key(pressed.code, pressed.modifiers) else {
            continue;
        };
        match settings.handle(key) {
            Outcome::Continue => {}
            Outcome::Quit => return Ok(false),
            Outcome::Save => match save(settings, env) {
                Ok(()) => return Ok(true),
                Err(error) => settings.status = Some(format!("✗ not saved: {}", error.to_string().replace('\n', " "))),
            },
            Outcome::Test(desk) => {
                settings.status = Some(format!("Testing \"{desk}\"…"));
                draw(terminal, settings)?;
                settings.status = Some(test_desk(config, &desk, env)?);
            }
        }
    }
}

pub(crate) fn settings(pane: bool, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::Usage(format!(
            "settings needs a terminal; edit {} instead (see config.example.toml)",
            path.display()
        )));
    }
    let config = load_config(&path)?;
    let auto_on = auto_enabled(env);
    let mut settings = Settings::new(
        &config,
        auto_on,
        &herdr_workspaces(env, &System),
        &herdr_agents(env, &System),
    );
    let mut terminal = ratatui::try_init()?;
    let saved = run_ui(&mut terminal, &mut settings, &config, env);
    ratatui::restore();
    if saved? {
        println!("Saved nudge settings to {}.", path.display());
        if pane {
            let _shown = System.run(
                &[env.herdr(), "notification", "show", "nudge", "--body", "Settings saved"],
                &env.home(),
            );
        }
    } else {
        println!("No changes saved.");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::{Key, Outcome, Row, Settings, cycle, step};
    use crate::config::{Auto, Config, Desk, Scope};
    use crate::context::ListedWorkspace;

    fn desk(name: &str) -> Desk {
        Desk {
            name: name.into(),
            description: None,
            url: format!("https://{name}.example/hooks/secret-path"),
            authorization: "Bearer x".into(),
            signing_secret: None,
        }
    }

    fn workspace(id: &str, label: &str) -> ListedWorkspace {
        ListedWorkspace {
            workspace_id: id.into(),
            label: Some(label.into()),
        }
    }

    fn settings(scope: Scope) -> Settings {
        let config = Config {
            default_desk: Some("bridge".into()),
            desk: vec![desk("bridge"), desk("research")],
            scope,
            ..Config::default()
        };
        Settings::new(
            &config,
            false,
            &[workspace("w1", "Base"), workspace("wD", "web-app")],
            &["builder".into(), "reviewer".into()],
        )
    }

    /// Move the cursor to `row` and press `key` there.
    fn press_on(settings: &mut Settings, row: Row, key: Key) -> Outcome {
        settings.cursor = settings.rows().iter().position(|candidate| *candidate == row).unwrap();
        settings.handle(key)
    }

    /// Press `key` on `row`, expecting the UI to carry on.
    fn tap(settings: &mut Settings, row: Row, key: Key) {
        assert_eq!(press_on(settings, row, key), Outcome::Continue);
    }

    fn saved(settings: &Settings) -> Config {
        let mut config = Config {
            desk: vec![desk("bridge"), desk("research")],
            default_desk: Some("bridge".into()),
            ..Config::default()
        };
        settings.apply(&mut config);
        config
    }

    #[test]
    fn values_step_through_presets_and_snap_from_custom_values() {
        assert_eq!(step(120, &[0, 60, 120, 300], true), 300);
        assert_eq!(step(120, &[0, 60, 120, 300], false), 60);
        assert_eq!(step(300, &[0, 60, 120, 300], true), 300, "stays at the top");
        assert_eq!(step(90, &[0, 60, 120, 300], true), 120, "custom value snaps up");
        assert_eq!(step(90, &[0, 60, 120, 300], false), 60, "custom value snaps down");
        assert_eq!(cycle(300, &[0, 60, 120, 300]), 0, "space wraps around");
    }

    #[test]
    fn edits_land_in_the_config_and_untouched_settings_stay() {
        let mut ui = settings(Scope::default());
        tap(&mut ui, Row::AutoOn, Key::Toggle);
        tap(&mut ui, Row::Toast, Key::Toggle);
        tap(&mut ui, Row::MinTurn, Key::Left);
        tap(&mut ui, Row::TailLines, Key::Right);
        tap(&mut ui, Row::Desk(1), Key::Toggle);
        let config = saved(&ui);
        let expected = Auto {
            min_turn_seconds: 60,
            tail_lines: 80,
            toast: false,
            ..Auto::default()
        };
        assert_eq!(config.auto, expected);
        assert!(ui.auto_on());
        assert_eq!(config.default_desk.as_deref(), Some("research"));
        assert!(config.scope.is_everyone());
    }

    #[test]
    fn ticking_a_workspace_or_agent_limits_the_scope_to_the_ticked_names() {
        let mut ui = settings(Scope::default());
        tap(&mut ui, Row::Workspace(1), Key::Toggle);
        tap(&mut ui, Row::Agent(0), Key::Toggle);
        let scope = saved(&ui).scope;
        assert_eq!(scope.workspaces, ["web-app"]);
        assert_eq!(scope.agents, ["builder"]);
        tap(&mut ui, Row::Everyone, Key::Toggle);
        assert!(
            saved(&ui).scope.is_everyone(),
            "ticks are kept but not applied for every agent"
        );
    }

    #[test]
    fn saved_scope_shows_ticked_by_label_or_id_and_keeps_names_that_are_not_open() {
        let ui = settings(Scope {
            workspaces: vec!["wD".into(), "old-project".into()],
            agents: vec!["reviewer".into()],
        });
        let ticked: Vec<(&str, bool, bool)> = ui
            .workspaces
            .iter()
            .map(|choice| (choice.name.as_str(), choice.checked, choice.live))
            .collect();
        assert_eq!(
            ticked,
            [
                ("Base", false, true),
                ("web-app", true, true),
                ("old-project", true, false)
            ]
        );
        let scope = saved(&ui).scope;
        assert_eq!(scope.workspaces, ["web-app", "old-project"]);
        assert_eq!(scope.agents, ["reviewer"]);
    }

    #[test]
    fn only_these_with_nothing_ticked_does_not_save() {
        let mut ui = settings(Scope::default());
        tap(&mut ui, Row::OnlyThese, Key::Toggle);
        assert_eq!(ui.handle(Key::Save), Outcome::Continue);
        assert!(ui.status.as_deref().unwrap().contains("Tick at least one"));
        tap(&mut ui, Row::Agent(1), Key::Toggle);
        assert_eq!(ui.handle(Key::Save), Outcome::Save);
    }

    #[test]
    fn quitting_with_changes_asks_once_more() {
        let mut ui = settings(Scope::default());
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit, "nothing changed");
        tap(&mut ui, Row::Toast, Key::Toggle);
        assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
        assert_eq!(ui.handle(Key::Quit), Outcome::Quit);
        tap(&mut ui, Row::Toast, Key::Toggle);
        assert_eq!(ui.handle(Key::Quit), Outcome::Continue);
        assert_eq!(ui.handle(Key::Down), Outcome::Continue);
        assert_eq!(
            ui.handle(Key::Quit),
            Outcome::Continue,
            "another key disarms the discard"
        );
    }

    #[test]
    fn test_pings_only_from_a_desk_row_and_desks_show_no_url_path() {
        let mut ui = settings(Scope::default());
        assert_eq!(press_on(&mut ui, Row::AutoOn, Key::Test), Outcome::Continue);
        assert_eq!(
            press_on(&mut ui, Row::Desk(1), Key::Test),
            Outcome::Test("research".into())
        );
        let (lines, _) = ui.lines();
        let text: String = lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(
            text.contains("research.example") && !text.contains("secret-path"),
            "{text}"
        );
    }

    #[test]
    fn the_cursor_stays_on_selectable_rows() {
        let mut ui = settings(Scope::default());
        assert_eq!(ui.handle(Key::Up), Outcome::Continue);
        assert_eq!(ui.current(), Some(Row::AutoOn));
        for _ in 0..100 {
            assert_eq!(ui.handle(Key::Down), Outcome::Continue);
        }
        assert_eq!(ui.current(), Some(Row::Desk(1)));
    }
}
