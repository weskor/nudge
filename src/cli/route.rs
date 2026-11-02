//! `nudge route ls|set|rm` and `nudge default <webhook>`: where notifications go, edited in the config.
use std::path::Path;

use super::args::utf8;
use crate::config::{
    Backend, Config, Route, Selector, default_config_path, default_webhook_name, find_webhook, load_config,
    remove_route, set_route, write_config,
};
use crate::env::Env;
use crate::error::{Error, Result};

const ROUTE_USAGE: &str = "Usage:
  nudge route ls  [--json]
  nudge route set (--pane NAME | --workspace NAME) --webhook NAME [--backend tui|core]
  nudge route rm  (--pane NAME | --workspace NAME) [--backend tui|core]

A route sends one pane's or one workspace's notifications to a webhook other than the default.
The most specific wins: a pane route (agent name or pane id), then a workspace route (label or id),
then the default webhook. --backend narrows a route to herdr (tui) or herdr-core (core).";

/// The `nudge route` command line; it parses its own options because `--pane` takes a name here.
#[derive(Debug, Default)]
struct RouteArgs {
    command: Option<String>,
    pane: Option<String>,
    workspace: Option<String>,
    webhook: Option<String>,
    backend: Option<String>,
    json: bool,
    help: bool,
}

fn parse(words: &[String]) -> Result<RouteArgs> {
    use lexopt::Arg::{Long, Short, Value};

    let mut parsed = RouteArgs::default();
    let mut parser = lexopt::Parser::from_args(words.iter().cloned());
    while let Some(token) = parser.next()? {
        match token {
            Long("pane") => parsed.pane = Some(utf8(parser.value()?)?),
            Long("workspace") => parsed.workspace = Some(utf8(parser.value()?)?),
            Long("webhook") => parsed.webhook = Some(utf8(parser.value()?)?),
            Long("backend") => parsed.backend = Some(utf8(parser.value()?)?),
            Long("json") => parsed.json = true,
            Long("help") | Short('h') => parsed.help = true,
            Value(word) if parsed.command.is_none() => parsed.command = Some(utf8(word)?),
            Value(word) => {
                return Err(Error::Usage(format!("unexpected argument \"{}\"", utf8(word)?)));
            }
            unexpected @ (Long(_) | Short(_)) => return Err(unexpected.unexpected().into()),
        }
    }
    Ok(parsed)
}

impl RouteArgs {
    /// The one `--pane` or `--workspace` that `set` and `rm` need.
    fn selector(&self) -> Result<Selector> {
        let name = |raw: &str| {
            let name = raw.trim();
            if name.is_empty() {
                Err(Error::Usage("the pane or workspace name is empty".into()))
            } else {
                Ok(name.to_owned())
            }
        };
        match (&self.pane, &self.workspace) {
            (Some(pane), None) => Ok(Selector::Pane(name(pane)?)),
            (None, Some(workspace)) => Ok(Selector::Workspace(name(workspace)?)),
            (Some(_), Some(_)) | (None, None) => Err(Error::Usage(
                "route set and route rm need exactly one of --pane NAME or --workspace NAME".into(),
            )),
        }
    }

    fn backend(&self) -> Result<Option<Backend>> {
        self.backend
            .as_deref()
            .map(|raw| Backend::parse(raw).ok_or_else(|| Error::Usage("--backend must be tui or core".into())))
            .transpose()
    }

    /// Refuse options the subcommand doesn't take.
    fn only(&self, command: &str, allowed: &[&str]) -> Result<()> {
        let given = [
            ("--pane", self.pane.is_some()),
            ("--workspace", self.workspace.is_some()),
            ("--webhook", self.webhook.is_some()),
            ("--backend", self.backend.is_some()),
            ("--json", self.json),
        ];
        match given.iter().find(|(flag, set)| *set && !allowed.contains(flag)) {
            Some((flag, _)) => Err(Error::Usage(format!("route {command} doesn't take {flag}"))),
            None => Ok(()),
        }
    }
}

/// `nudge route …` (the words after `route`); returns the exit code.
pub(super) fn route(words: &[String], env: &Env) -> Result<u8> {
    let args = parse(words)?;
    if args.help {
        println!("{ROUTE_USAGE}");
        return Ok(0);
    }
    let path = default_config_path(env);
    match args.command.as_deref() {
        Some("ls") => {
            args.only("ls", &["--json"])?;
            list_routes(&load_config(&path)?, args.json);
            Ok(0)
        }
        Some("set") => {
            args.only("set", &["--pane", "--workspace", "--webhook", "--backend"])?;
            let webhook = args
                .webhook
                .as_deref()
                .ok_or_else(|| Error::Usage("route set needs --webhook NAME".into()))?;
            edit_routes(&args, &path, |config, selector, backend| {
                let route = Route {
                    selector,
                    webhook: find_webhook(config, Some(webhook.trim()))?.name.clone(),
                    backend,
                };
                let shown = route.to_string();
                let verb = if set_route(&mut config.route, route) {
                    "Changed"
                } else {
                    "Added"
                };
                Ok(format!("{verb} route: {shown}"))
            })
        }
        Some("rm") => {
            args.only("rm", &["--pane", "--workspace", "--backend"])?;
            edit_routes(&args, &path, |config, selector, backend| {
                remove(config, &selector, backend)
            })
        }
        Some(other) => Err(Error::Usage(format!(
            "unknown route command \"{other}\"; use ls, set or rm"
        ))),
        None => Err(Error::Usage("route needs a command: ls, set or rm".into())),
    }
}

/// Apply `change` to the config's routes for the selector and backend on the command line, save, and say what
/// changed.
fn edit_routes(
    args: &RouteArgs,
    path: &Path,
    change: impl FnOnce(&mut Config, Selector, Option<Backend>) -> Result<String>,
) -> Result<u8> {
    let selector = args.selector()?;
    let backend = args.backend()?;
    let mut config = load_config(path)?;
    let message = change(&mut config, selector, backend)?;
    write_config(path, &config)?;
    println!("{message}");
    Ok(0)
}

/// `route rm`: remove the route with exactly this selector and backend.
fn remove(config: &mut Config, selector: &Selector, backend: Option<Backend>) -> Result<String> {
    let removed = remove_route(&mut config.route, selector, backend).ok_or_else(|| {
        let only = backend
            .map(|backend| format!(" ({} only)", backend.as_str()))
            .unwrap_or_default();
        Error::Config(format!(
            "no route for {} \"{}\"{only}; see nudge route ls",
            selector.key(),
            selector.name()
        ))
    })?;
    Ok(format!("Removed route: {removed}"))
}

/// The routes in the order they're tried, as text or JSON (names only, never URLs or secrets).
fn list_routes(config: &Config, json: bool) {
    let default = default_webhook_name(config);
    if json {
        let routes: Vec<serde_json::Value> = config
            .route
            .iter()
            .map(|route| {
                let mut row =
                    serde_json::json!({ route.selector.key(): route.selector.name(), "webhook": route.webhook });
                if let (Some(backend), Some(row)) = (route.backend, row.as_object_mut()) {
                    row.insert("backend".into(), backend.as_str().into());
                }
                row
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "default_webhook": default, "routes": routes })
        );
        return;
    }
    let pane_routes = config
        .route
        .iter()
        .filter(|route| matches!(route.selector, Selector::Pane(_)));
    let workspace_routes = config
        .route
        .iter()
        .filter(|route| matches!(route.selector, Selector::Workspace(_)));
    for route in pane_routes.chain(workspace_routes) {
        println!("{route}");
    }
    match default {
        Some(name) => println!("everything else → {name} (default webhook)"),
        None => println!("everything else → no default webhook: several webhooks and no default_webhook"),
    }
}

/// `nudge default <webhook>`: make a configured webhook the default.
pub(super) fn set_default(rest: &[String], env: &Env) -> Result<u8> {
    let [name] = rest else {
        return Err(Error::Usage(
            "default takes one webhook name: nudge default <webhook>".into(),
        ));
    };
    let path = default_config_path(env);
    let mut config = load_config(&path)?;
    let name = find_webhook(&config, Some(name.trim()))?.name.clone();
    config.default_webhook = Some(name.clone());
    write_config(&path, &config)?;
    println!("Default webhook: {name}");
    Ok(0)
}
