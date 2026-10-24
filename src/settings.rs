//! `nudge settings`: a terminal UI for auto-notify, who can notify, and the default webhook. herdr opens it as an
//! overlay (`weskor.nudge.settings`). It edits the same config and state as the actions and `nudge setup`.
use std::ffi::OsStr;
use std::io::IsTerminal as _;
use std::path::Path;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::config::{Auto, Config, Scope, default_config_path, load_config, seconds_in_words, write_config};
use crate::context::{Env, ListedAgent, ListedWorkspace, Run as _, System, herdr_agents, herdr_workspaces};
use crate::error::{Blame, Error, Result};
use crate::notify::{Origin, current_dir, ping_webhook};
use crate::state::{State, current_state, update_state};
use crate::ui::{self, DIM, GOOD, HEADER, VALUE, View, WARN, aligned, check, radio, toggle};

const TURN_STEPS: [u32; 9] = [0, 30, 60, 120, 180, 300, 600, 900, 1800];
const COOLDOWN_STEPS: [u32; 8] = [0, 60, 120, 300, 600, 900, 1800, 3600];
const TAIL_STEPS: [u32; 6] = [10, 20, 40, 80, 120, 200];

const HELP: [(&str, &str); 6] = [
    ("↑↓", "move"),
    ("space", "pick"),
    ("←→", "change"),
    ("t", "test"),
    ("s", "save"),
    ("q", "quit"),
];

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
    Away,
    Toast,
    MinTurn,
    Cooldown,
    TailLines,
    Everyone,
    OnlyThese,
    Workspace(usize),
    Agent(usize),
    Webhook(usize),
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
    /// Shown dimmed after the name: the agents a workspace holds, where an agent runs, or `(not open)`.
    detail: String,
}

/// A workspace or agent open in herdr: how it is shown and stored, every name `[scope]` may know it by, and its
/// detail.
struct Open {
    name: String,
    ids: Vec<String>,
    detail: String,
}

/// A webhook as shown: never its URL path or secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WebhookRow {
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
    /// Send a test ping to this webhook.
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
    away: bool,
    auto: Auto,
    only: bool,
    workspaces: Vec<Choice>,
    agents: Vec<Choice>,
    webhooks: Vec<WebhookRow>,
    default_webhook: Option<String>,
    cursor: usize,
    status: Option<String>,
    edits: Edits,
}

/// Open ones first (ticked if `[scope]` names them by any of their `ids`), then names only the config has.
fn choices(open: Vec<Open>, saved: &[String]) -> Vec<Choice> {
    let mut list: Vec<Choice> = Vec::new();
    for Open { name, ids, detail } in open {
        if list.iter().any(|choice| choice.name == name) {
            continue;
        }
        let checked = saved.iter().any(|entry| ids.contains(entry));
        list.push(Choice {
            name,
            checked,
            live: true,
            detail,
        });
    }
    for entry in saved {
        if !list.iter().any(|choice| &choice.name == entry) {
            list.push(Choice {
                name: entry.clone(),
                checked: true,
                live: false,
                detail: "(not open)".into(),
            });
        }
    }
    list
}

/// What the row under the cursor does, shown under the list.
const fn describe(row: Row) -> &'static str {
    match row {
        Row::AutoOn => {
            "Tell the webhook when an agent is blocked, or done after a long turn, in a pane you aren't looking at."
        }
        Row::Away => "Count the pane on screen as unwatched too, and notify even with auto-notify off.",
        Row::Toast => "Show a herdr toast each time an automatic notification goes out.",
        Row::MinTurn => "Skip done turns shorter than this. Blocked agents always count.",
        Row::Cooldown => "At most one automatic notification per pane in this window; a brake on reply loops.",
        Row::TailLines => "How many of the pane's last lines go with the event, with credentials redacted.",
        Row::Everyone => "Any agent in herdr can reach the webhook.",
        Row::OnlyThese => "Only agents in the ticked workspaces, and the ticked agents, can reach the webhook.",
        Row::Workspace(_) => "Agents in this workspace can notify.",
        Row::Agent(_) => "This agent can notify, whichever workspace it is in.",
        Row::Webhook(_) => "Notifications go here unless they name another webhook. t sends a test ping.",
    }
}

impl Settings {
    pub(crate) fn new(config: &Config, saved: &State, workspaces: &[ListedWorkspace], agents: &[ListedAgent]) -> Self {
        let open_workspaces = workspaces
            .iter()
            .map(|workspace| {
                let held = agents
                    .iter()
                    .filter(|agent| agent.workspace_id.as_deref() == Some(workspace.workspace_id.as_str()))
                    .count();
                Open {
                    name: workspace.name().to_owned(),
                    ids: vec![workspace.name().to_owned(), workspace.workspace_id.clone()],
                    detail: match held {
                        0 => String::new(),
                        1 => "1 agent".into(),
                        many => format!("{many} agents"),
                    },
                }
            })
            .collect();
        let mut workspace_choices = choices(open_workspaces, &config.scope.workspaces);
        // A saved workspace id that matched a live workspace is already shown under its name.
        workspace_choices
            .retain(|choice| choice.live || !workspaces.iter().any(|workspace| workspace.workspace_id == choice.name));
        let open_agents = agents
            .iter()
            .map(|agent| {
                let workspace = agent.workspace_id.as_deref().map(|id| {
                    workspaces
                        .iter()
                        .find(|workspace| workspace.workspace_id == id)
                        .map_or(id, ListedWorkspace::name)
                });
                let dir = agent
                    .cwd
                    .as_deref()
                    .and_then(|cwd| Path::new(cwd).file_name())
                    .and_then(OsStr::to_str);
                Open {
                    name: agent.name.clone(),
                    ids: vec![agent.name.clone()],
                    detail: workspace.into_iter().chain(dir).collect::<Vec<_>>().join(" · "),
                }
            })
            .collect();
        Self {
            auto_on: saved.auto,
            away: saved.away,
            auto: config.auto,
            only: !config.scope.is_everyone(),
            workspaces: workspace_choices,
            agents: choices(open_agents, &config.scope.agents),
            webhooks: config
                .webhook
                .iter()
                .map(|webhook| WebhookRow {
                    name: webhook.name.clone(),
                    host: url::Url::parse(&webhook.url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .unwrap_or_default(),
                    signed: webhook.signing_secret.is_some(),
                })
                .collect(),
            default_webhook: crate::config::default_webhook_name(config).map(str::to_owned),
            cursor: 0,
            status: None,
            edits: Edits::None,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![
            Row::AutoOn,
            Row::Away,
            Row::Toast,
            Row::MinTurn,
            Row::Cooldown,
            Row::TailLines,
            Row::Everyone,
            Row::OnlyThese,
        ];
        // The lists only matter for "Only these", so they are hidden (and not cursor stops) for "Every agent".
        if self.only {
            rows.extend((0..self.workspaces.len()).map(Row::Workspace));
            rows.extend((0..self.agents.len()).map(Row::Agent));
        }
        rows.extend((0..self.webhooks.len()).map(Row::Webhook));
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
            | Row::Away
            | Row::Toast
            | Row::Everyone
            | Row::OnlyThese
            | Row::Workspace(_)
            | Row::Agent(_)
            | Row::Webhook(_) => return,
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
            Row::Away => self.away = !self.away,
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
            }
            Row::Agent(index) => {
                let Some(choice) = self.agents.get_mut(index) else {
                    return;
                };
                choice.checked = !choice.checked;
            }
            Row::Webhook(index) => {
                let Some(webhook) = self.webhooks.get(index) else {
                    return;
                };
                self.default_webhook = Some(webhook.name.clone());
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
            (Key::Test, Some(Row::Webhook(index))) => {
                if let Some(webhook) = self.webhooks.get(index) {
                    return Outcome::Test(webhook.name.clone());
                }
            }
            (Key::Test, _) => self.status = Some("Move to a webhook to test it.".into()),
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

    /// Write the edited settings into `config` (other settings and webhooks stay as they are).
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
        if let Some(name) = &self.default_webhook
            && config.webhook.iter().any(|webhook| &webhook.name == name)
        {
            config.default_webhook = Some(name.clone());
        }
    }

    /// Write the edited toggles into `state` (pane timestamps stay as they are).
    pub(crate) const fn apply_state(&self, state: &mut State) {
        state.auto = self.auto_on;
        state.away = self.away;
    }

    /// The lines to draw, and which one the cursor is on.
    fn lines(&self) -> (Vec<Line<'static>>, usize) {
        let cursor_row = self.current();
        let away = if self.away {
            Span::styled("[x]", WARN)
        } else {
            check(false)
        };
        let mut items: Vec<(Option<Row>, Line<'static>)> = vec![
            (None, Line::styled("Mode", HEADER)),
            (Some(Row::AutoOn), toggle(check(self.auto_on), " Auto-notify")),
            (Some(Row::Away), toggle(away, " I'm away")),
            (None, Line::default()),
            (None, Line::styled("Automatic notifications", HEADER)),
            (
                Some(Row::Toast),
                toggle(check(self.auto.toast), " Toast when one is sent"),
            ),
        ];
        // Values line up with the checkbox labels; only the row under the cursor shows the ◂ ▸ that change it.
        for (row, label, text) in [
            (
                Row::MinTurn,
                "Shortest turn sent",
                seconds_in_words(self.auto.min_turn_seconds),
            ),
            (
                Row::Cooldown,
                "Cooldown per pane",
                seconds_in_words(self.auto.cooldown_seconds),
            ),
            (Row::TailLines, "Terminal lines sent", self.auto.tail_lines.to_string()),
        ] {
            let (before, after) = if cursor_row == Some(row) {
                ("◂ ", " ▸")
            } else {
                ("  ", "  ")
            };
            let line = Line::from(vec![
                Span::raw(format!("      {label:<22}{before}")),
                Span::styled(text, VALUE),
                Span::raw(after),
            ]);
            items.push((Some(row), line));
        }
        let only = if self.only { " Only these:" } else { " Only these…" };
        items.extend([
            (None, Line::default()),
            (None, Line::styled("Who can notify", HEADER)),
            (Some(Row::Everyone), toggle(radio(!self.only), " Every agent")),
            (Some(Row::OnlyThese), toggle(radio(self.only), only)),
        ]);
        if self.only {
            let sections: [Section<'_>; 2] = [
                ("Workspaces", &self.workspaces, Row::Workspace),
                ("Agents", &self.agents, Row::Agent),
            ];
            for (label, list, row) in sections {
                items.push((None, Line::styled(format!("      {label}"), DIM)));
                if list.is_empty() {
                    items.push((None, Line::styled("        none open in herdr", DIM)));
                }
                let rows = list
                    .iter()
                    .map(|choice| {
                        let name = Span::styled(choice.name.clone(), if choice.live { Style::new() } else { DIM });
                        let detail = Some(&choice.detail)
                            .filter(|detail| !detail.is_empty())
                            .map(|detail| Span::styled(detail.clone(), DIM));
                        (check(choice.checked), name, detail.into_iter().collect())
                    })
                    .collect();
                let lines = aligned("        ", rows).into_iter().enumerate();
                items.extend(lines.map(|(index, line)| (Some(row(index)), line)));
            }
        }
        items.extend([(None, Line::default()), (None, Line::styled("Default webhook", HEADER))]);
        let rows = self
            .webhooks
            .iter()
            .map(|webhook| {
                let is_default = self.default_webhook.as_deref() == Some(webhook.name.as_str());
                let mut rest = vec![Span::styled(webhook.host.clone(), DIM)];
                if webhook.signed {
                    rest.push(Span::styled("  signed", GOOD));
                }
                (radio(is_default), Span::raw(webhook.name.clone()), rest)
            })
            .collect();
        let lines = aligned("  ", rows).into_iter().enumerate();
        items.extend(lines.map(|(index, line)| (Some(Row::Webhook(index)), line)));
        let selected = items
            .iter()
            .position(|(row, _)| row.is_some() && *row == cursor_row)
            .unwrap_or(0);
        (items.into_iter().map(|(_, line)| line).collect(), selected)
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
    ui::draw(
        terminal,
        View {
            title: "nudge settings",
            unsaved: settings.edits != Edits::None,
            steps: None,
            lines,
            selected: Some(selected),
            caret: None,
            note: ui::note(settings.status.as_deref(), settings.current().map_or("", describe)),
            help: &HELP,
            width: None,
        },
    )
}

/// Test-ping `webhook` and describe the result for the status line.
fn test_webhook(config: &Config, webhook: &str, env: &Env) -> Result<String> {
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    Ok(match ping_webhook(config, Some(webhook), None, origin) {
        Ok(sent) => format!("✓ \"{webhook}\" received a test ping (HTTP {})", sent.http_status),
        Err(Error::Webhook(error)) => {
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

/// Save: re-read the config so webhooks edited meanwhile are kept, apply the edits, write it and the toggle.
fn save(settings: &Settings, env: &Env) -> Result<()> {
    let path = default_config_path(env);
    let mut config = load_config(&path)?;
    settings.apply(&mut config);
    write_config(&path, &config)?;
    update_state(env, |state| settings.apply_state(state))?;
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
            Outcome::Test(webhook) => {
                settings.status = Some(format!("Testing \"{webhook}\"…"));
                draw(terminal, settings)?;
                settings.status = Some(test_webhook(config, &webhook, env)?);
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
    let mut settings = Settings::new(
        &config,
        &current_state(env),
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
    use crate::config::{Auto, Config, Scope, Webhook};
    use crate::context::{ListedAgent, ListedWorkspace};
    use crate::state::State;

    fn webhook(name: &str) -> Webhook {
        Webhook {
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
            default_webhook: Some("bridge".into()),
            webhook: vec![webhook("bridge"), webhook("research")],
            scope,
            ..Config::default()
        };
        Settings::new(
            &config,
            &State::default(),
            &[workspace("w1", "Base"), workspace("wD", "team-web")],
            &[
                ListedAgent {
                    name: "builder".into(),
                    workspace_id: Some("wD".into()),
                    cwd: Some("/home/user/src/nudge".into()),
                },
                ListedAgent {
                    name: "reviewer".into(),
                    workspace_id: Some("wX".into()),
                    cwd: None,
                },
            ],
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

    fn screen_text(settings: &Settings) -> String {
        let (lines, _) = settings.lines();
        lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    }

    fn saved(settings: &Settings) -> Config {
        let mut config = Config {
            webhook: vec![webhook("bridge"), webhook("research")],
            default_webhook: Some("bridge".into()),
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
        tap(&mut ui, Row::Webhook(1), Key::Toggle);
        let config = saved(&ui);
        let expected = Auto {
            min_turn_seconds: 60,
            tail_lines: 80,
            toast: false,
            ..Auto::default()
        };
        assert_eq!(config.auto, expected);
        let mut state = State::default();
        ui.apply_state(&mut state);
        assert!(state.auto && !state.away);
        tap(&mut ui, Row::Away, Key::Toggle);
        ui.apply_state(&mut state);
        assert!(state.away);
        assert_eq!(config.default_webhook.as_deref(), Some("research"));
        assert!(config.scope.is_everyone());
    }

    #[test]
    fn ticking_a_workspace_or_agent_limits_the_scope_to_the_ticked_names() {
        let mut ui = settings(Scope::default());
        tap(&mut ui, Row::OnlyThese, Key::Toggle);
        tap(&mut ui, Row::Workspace(1), Key::Toggle);
        tap(&mut ui, Row::Agent(0), Key::Toggle);
        let scope = saved(&ui).scope;
        assert_eq!(scope.workspaces, ["team-web"]);
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
                ("team-web", true, true),
                ("old-project", true, false)
            ]
        );
        let scope = saved(&ui).scope;
        assert_eq!(scope.workspaces, ["team-web", "old-project"]);
        assert_eq!(scope.agents, ["reviewer"]);
    }

    #[test]
    fn workspace_and_agent_lists_show_only_for_only_these() {
        let mut ui = settings(Scope::default());
        assert!(!screen_text(&ui).contains("builder"));
        tap(&mut ui, Row::OnlyThese, Key::Down);
        assert_eq!(ui.current(), Some(Row::Webhook(0)), "hidden rows are not cursor stops");
        tap(&mut ui, Row::OnlyThese, Key::Toggle);
        tap(&mut ui, Row::OnlyThese, Key::Down);
        assert_eq!(ui.current(), Some(Row::Workspace(0)));
        tap(&mut ui, Row::Workspace(0), Key::Toggle);
        tap(&mut ui, Row::Everyone, Key::Toggle);
        assert!(!screen_text(&ui).contains("Base"));
        tap(&mut ui, Row::OnlyThese, Key::Toggle);
        assert!(screen_text(&ui).contains("[x] Base"), "ticks survive hiding the lists");
    }

    #[test]
    fn open_agents_show_their_workspace_and_directory() {
        let ui = settings(Scope {
            workspaces: vec![],
            agents: vec!["gone".into()],
        });
        let text = screen_text(&ui);
        for expected in [
            "[ ] Base\n",
            "[ ] team-web  1 agent",
            "[ ] builder   team-web · nudge",
            "[ ] reviewer  wX",
            "[x] gone      (not open)",
        ] {
            assert!(text.contains(expected), "{expected:?} in\n{text}");
        }
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
    fn test_pings_only_from_a_webhook_row_and_webhooks_show_no_url_path() {
        let mut ui = settings(Scope::default());
        assert_eq!(press_on(&mut ui, Row::AutoOn, Key::Test), Outcome::Continue);
        assert_eq!(
            press_on(&mut ui, Row::Webhook(1), Key::Test),
            Outcome::Test("research".into())
        );
        let text = screen_text(&ui);
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
        assert_eq!(ui.current(), Some(Row::Webhook(1)));
    }
}
