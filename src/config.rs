//! The desk config (`~/.config/nudge/config.toml`): parsing with every problem reported, and safe writes.
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use toml::Value;

use crate::context::Env;
use crate::error::{Error, Result};

const PLACEHOLDER_PREFIX: &str = "REPLACE_WITH";
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const AUTHORIZATION_PREFIX: &str = "authorization:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Desk {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) url: String,
    /// Sent verbatim as the webhook's Authorization header value.
    pub(crate) authorization: String,
    /// When set, requests are signed with HMAC-SHA256 (`x-nudge-signature`) so the desk can verify them.
    pub(crate) signing_secret: Option<String>,
}

/// `[auto]`: when the herdr plugin notifies the desk on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Auto {
    /// A `done` turn shorter than this is not worth a notification.
    pub(crate) min_turn_seconds: u32,
    /// At most one automatic notification per pane in this window.
    pub(crate) cooldown_seconds: u32,
    /// Lines of terminal output sent with an automatic notification.
    pub(crate) tail_lines: u32,
    /// Show a herdr toast when an automatic notification was sent.
    pub(crate) toast: bool,
}

impl Default for Auto {
    fn default() -> Self {
        Self {
            min_turn_seconds: 120,
            cooldown_seconds: 300,
            tail_lines: 40,
            toast: true,
        }
    }
}

impl Auto {
    pub(crate) fn min_turn_ms(self) -> i64 {
        i64::from(self.min_turn_seconds) * 1000
    }

    pub(crate) fn cooldown_ms(self) -> i64 {
        i64::from(self.cooldown_seconds) * 1000
    }
}

/// A duration in words: "2 minutes", "1 minute", "90 seconds", "0 seconds".
pub(crate) fn seconds_in_words(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (1, 0) => "1 minute".to_owned(),
        (minutes @ 2.., 0) => format!("{minutes} minutes"),
        (_, _) if seconds == 1 => "1 second".to_owned(),
        (_, _) => format!("{seconds} seconds"),
    }
}

/// `[scope]`: which agents may notify the desk. Empty means every agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Scope {
    /// herdr workspace labels or ids.
    pub(crate) workspaces: Vec<String>,
    /// herdr agent names or pane ids.
    pub(crate) agents: Vec<String>,
}

impl Scope {
    pub(crate) const fn is_everyone(&self) -> bool {
        self.workspaces.is_empty() && self.agents.is_empty()
    }

    /// Whether an agent known by any of `agent_ids` (name, pane id), in a workspace known by any of
    /// `workspace_ids` (label, id), may notify.
    pub(crate) fn allows(&self, agent_ids: &[&str], workspace_ids: &[&str]) -> bool {
        self.is_everyone()
            || self.agents.iter().any(|agent| agent_ids.contains(&agent.as_str()))
            || self
                .workspaces
                .iter()
                .any(|workspace| workspace_ids.contains(&workspace.as_str()))
    }
}

/// A config as read from disk. `Default` is the empty config `nudge setup` starts from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) default_desk: Option<String>,
    pub(crate) desk: Vec<Desk>,
    pub(crate) auto: Auto,
    pub(crate) scope: Scope,
}

/// Validation problems, collected so one error lists all of them. Messages never include the input value,
/// so secrets are not echoed.
#[derive(Debug, Default)]
struct Problems {
    lines: Vec<String>,
    /// A value had the wrong type somewhere; cross-field checks over it are skipped.
    wrong_types: usize,
}

impl Problems {
    fn add(&mut self, path: &str, message: &str) {
        self.lines.push(if path.is_empty() {
            format!("  {message}")
        } else {
            format!("  {path}: {message}")
        });
    }

    fn wrong_type(&mut self, path: &str, expected: &str, got: Option<&Value>) {
        let received = match got {
            None => "undefined",
            Some(Value::String(_)) => "string",
            Some(Value::Float(number)) if number.is_nan() => "NaN",
            Some(Value::Integer(_) | Value::Float(_)) => "number",
            Some(Value::Boolean(_)) => "boolean",
            Some(Value::Datetime(_)) => "Date",
            Some(Value::Array(_)) => "array",
            Some(Value::Table(_)) => "object",
        };
        self.add(
            path,
            &format!("Invalid input: expected {expected}, received {received}"),
        );
        self.wrong_types += 1;
    }

    fn into_result<T>(self, value: T, label: &str) -> Result<T> {
        if self.lines.is_empty() {
            Ok(value)
        } else {
            Err(Error::Config(format!("{label}\n{}", self.lines.join("\n"))))
        }
    }

    fn string<'a>(&mut self, path: &str, value: Option<&'a Value>) -> Option<&'a str> {
        if let Some(Value::String(text)) = value {
            Some(text)
        } else {
            self.wrong_type(path, "string", value);
            None
        }
    }

    /// Trimmed `raw`, which must be non-empty and free of `REPLACE_WITH…` placeholders.
    fn non_empty(&mut self, path: &str, raw: &str) -> String {
        let value = raw.trim();
        if value.is_empty() {
            self.add(path, "must be a non-empty string");
        }
        if value.contains(PLACEHOLDER_PREFIX) {
            self.add(path, &format!("still contains a {PLACEHOLDER_PREFIX}… placeholder"));
        }
        value.to_owned()
    }

    fn desk_name(&mut self, path: &str, raw: &str) -> String {
        let name = self.non_empty(path, raw);
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|first| first.is_ascii_alphanumeric())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid {
            self.add(path, r#"may only contain letters, digits, "-" and "_""#);
        }
        name
    }

    fn webhook_url(&mut self, path: &str, raw: &str) -> String {
        let url = self.non_empty(path, raw);
        let allowed = url::Url::parse(&url).is_ok_and(|parsed| match parsed.scheme() {
            "https" => true,
            "http" => parsed.host_str().is_some_and(|host| LOOPBACK_HOSTS.contains(&host)),
            _ => false,
        });
        if !allowed {
            self.add(path, "must be an https URL (http is allowed only for localhost)");
        }
        url
    }

    /// The header value; a pasted "Authorization:" header name is stripped.
    fn authorization(&mut self, path: &str, raw: &str) -> String {
        let trimmed = raw.trim();
        let value = match trimmed.split_at_checked(AUTHORIZATION_PREFIX.len()) {
            Some((name, rest)) if name.eq_ignore_ascii_case(AUTHORIZATION_PREFIX) => rest.trim_start(),
            Some(_) | None => trimmed,
        };
        let value = self.non_empty(path, value);
        if value.is_empty() || !value.chars().all(|c| matches!(c, ' '..='~')) {
            self.add(path, "must be printable ASCII on a single line");
        }
        value
    }

    fn signing_secret(&mut self, path: &str, raw: &str) -> String {
        let value = self.non_empty(path, raw);
        if value.chars().count() < 16 || !value.chars().all(|c| c.is_ascii_graphic()) {
            self.add(path, "must be at least 16 printable ASCII characters, without spaces");
        }
        value
    }

    /// An integer in `range`; a number outside it, or a fraction, is reported with `message`.
    fn integer(
        &mut self,
        path: &str,
        value: &Value,
        range: std::ops::RangeInclusive<u32>,
        message: &str,
    ) -> Option<u32> {
        if let Value::Integer(number) = value {
            let fits = u32::try_from(*number).ok().filter(|number| range.contains(number));
            if fits.is_none() {
                self.add(path, message);
            }
            fits
        } else if let Value::Float(_) = value {
            self.add(path, message);
            None
        } else {
            self.wrong_type(path, "number", Some(value));
            None
        }
    }

    fn auto(&mut self, value: Option<&Value>) -> Auto {
        const SECONDS: &str = "must be a whole number of seconds, 0 or more";
        let mut auto = Auto::default();
        let Some(value) = value else { return auto };
        let Value::Table(table) = value else {
            self.wrong_type("auto", "object", Some(value));
            return auto;
        };
        for (key, value) in table {
            let path = format!("auto.{key}");
            match key.as_str() {
                "min_turn_seconds" => {
                    auto.min_turn_seconds = self.integer(&path, value, 0..=u32::MAX, SECONDS).unwrap_or_default();
                }
                "cooldown_seconds" => {
                    auto.cooldown_seconds = self.integer(&path, value, 0..=u32::MAX, SECONDS).unwrap_or_default();
                }
                "tail_lines" => {
                    auto.tail_lines = self
                        .integer(&path, value, 1..=200, "must be a whole number from 1 to 200")
                        .unwrap_or_default();
                }
                "toast" => {
                    if let Value::Boolean(toast) = value {
                        auto.toast = *toast;
                    } else {
                        self.wrong_type(&path, "boolean", Some(value));
                    }
                }
                _ => self.add(&path, "unknown setting"),
            }
        }
        auto
    }

    /// A list of non-empty names.
    fn names(&mut self, path: &str, value: &Value) -> Vec<String> {
        let Value::Array(items) = value else {
            self.wrong_type(path, "array", Some(value));
            return Vec::new();
        };
        let mut names = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let item_path = format!("{path}.{index}");
            if let Some(raw) = self.string(&item_path, Some(item)) {
                names.push(self.non_empty(&item_path, raw));
            }
        }
        names
    }

    fn scope(&mut self, value: Option<&Value>) -> Scope {
        let mut scope = Scope::default();
        let Some(value) = value else { return scope };
        let Value::Table(table) = value else {
            self.wrong_type("scope", "object", Some(value));
            return scope;
        };
        for (key, value) in table {
            let path = format!("scope.{key}");
            match key.as_str() {
                "workspaces" => scope.workspaces = self.names(&path, value),
                "agents" => scope.agents = self.names(&path, value),
                _ => self.add(&path, "unknown setting"),
            }
        }
        scope
    }

    fn desk(&mut self, path: &str, value: &Value) -> Option<Desk> {
        let Value::Table(table) = value else {
            self.wrong_type(path, "object", Some(value));
            return None;
        };
        let field = |key: &str| format!("{path}.{key}");
        let name = self
            .string(&field("name"), table.get("name"))
            .map(|raw| self.desk_name(&field("name"), raw));
        let description = match table.get("description") {
            None => Some(None),
            Some(Value::String(text)) => Some(Some(text.clone())),
            Some(other) => {
                self.wrong_type(&field("description"), "string", Some(other));
                None
            }
        };
        let url = self
            .string(&field("url"), table.get("url"))
            .map(|raw| self.webhook_url(&field("url"), raw));
        let authorization = self
            .string(&field("authorization"), table.get("authorization"))
            .map(|raw| self.authorization(&field("authorization"), raw));
        let signing_secret = match table.get("signing_secret") {
            None => Some(None),
            Some(value) => self
                .string(&field("signing_secret"), Some(value))
                .map(|raw| Some(self.signing_secret(&field("signing_secret"), raw))),
        };
        Some(Desk {
            name: name?,
            description: description?,
            url: url?,
            authorization: authorization?,
            signing_secret: signing_secret?,
        })
    }

    fn config(&mut self, raw: &toml::Table) -> Option<Config> {
        let default_desk = match raw.get("default_desk") {
            None => Some(None),
            Some(value) => self
                .string("default_desk", Some(value))
                .map(|name| Some(self.desk_name("default_desk", name))),
        };
        let desks = match raw.get("desk") {
            Some(Value::Array(items)) => {
                let before = self.wrong_types;
                let desks: Vec<Option<Desk>> = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| self.desk(&format!("desk.{index}"), item))
                    .collect();
                if items.is_empty() {
                    self.add("desk", "define at least one [[desk]] table");
                }
                if self.wrong_types == before {
                    for (index, name) in desks.iter().flatten().map(|desk| &desk.name).enumerate() {
                        if desks.iter().flatten().take(index).any(|earlier| &earlier.name == name) {
                            self.add(
                                &format!("desk.{index}.name"),
                                &format!("duplicate desk name \"{name}\""),
                            );
                        }
                    }
                }
                desks.into_iter().collect::<Option<Vec<Desk>>>()
            }
            other => {
                self.wrong_type("desk", "array", other);
                None
            }
        };
        let auto = self.auto(raw.get("auto"));
        let scope = self.scope(raw.get("scope"));
        let config = Config {
            default_desk: default_desk?,
            desk: desks?,
            auto,
            scope,
        };
        if let Some(name) = &config.default_desk
            && !config.desk.iter().any(|desk| &desk.name == name)
        {
            self.add("default_desk", &format!("no [[desk]] named \"{name}\""));
        }
        Some(config)
    }
}

/// Validate a webhook URL entered during setup.
pub(crate) fn check_webhook_url(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let url = problems.webhook_url("", value);
    problems.into_result(url, "invalid webhook URL")
}

/// Validate an Authorization header value entered during setup (strips a pasted "Authorization:" prefix).
pub(crate) fn check_authorization(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let authorization = problems.authorization("", value);
    problems.into_result(authorization, "invalid Authorization value")
}

/// Validate one desk entered via `nudge setup`.
pub(crate) fn validate_desk(
    name: &str,
    url: &str,
    authorization: &str,
    description: Option<&str>,
    signing_secret: Option<&str>,
) -> Result<Desk> {
    let mut problems = Problems::default();
    let desk = Desk {
        name: problems.desk_name("name", name),
        description: description.map(str::to_owned),
        url: problems.webhook_url("url", url),
        authorization: problems.authorization("authorization", authorization),
        signing_secret: signing_secret.map(|secret| problems.signing_secret("signing_secret", secret)),
    };
    problems.into_result(desk, "invalid desk")
}

/// `$NUDGE_CONFIG`, else `$XDG_CONFIG_HOME/nudge/config.toml`, else `~/.config/nudge/config.toml`.
pub(crate) fn default_config_path(env: &Env) -> PathBuf {
    if let Some(path) = env.get("NUDGE_CONFIG") {
        return PathBuf::from(path);
    }
    let base = env
        .get("XDG_CONFIG_HOME")
        .map_or_else(|| env.home().join(".config"), PathBuf::from);
    base.join("nudge").join("config.toml")
}

/// Parse and validate config TOML. `source` names the file in error messages.
pub(crate) fn parse_config(text: &str, source: &str) -> Result<Config> {
    let raw: toml::Table =
        toml::from_str(text).map_err(|error| Error::Config(format!("{source}: invalid TOML: {}", error.message())))?;
    let mut problems = Problems::default();
    let config = problems.config(&raw);
    let label = format!("{source}: invalid config");
    problems.into_result(config, &label)?.ok_or(Error::Config(label))
}

/// Load the config, or `None` if the file does not exist.
pub(crate) fn read_config(path: &Path) -> Result<Option<Config>> {
    match fs::read_to_string(path) {
        Ok(text) => parse_config(&text, &path.to_string_lossy()).map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Other(format!("{}: {error}", path.display()))),
    }
}

pub(crate) fn load_config(path: &Path) -> Result<Config> {
    read_config(path)?.ok_or_else(|| {
        Error::Config(format!(
            "no config at {}. Run \"nudge setup\" to add a desk.",
            path.display()
        ))
    })
}

/// `default_desk`, else the only desk's name; `None` when several desks and no default.
pub(crate) fn default_desk_name(config: &Config) -> Option<&str> {
    let only = match config.desk.as_slice() {
        [only] => Some(only.name.as_str()),
        [] | [_, _, ..] => None,
    };
    config.default_desk.as_deref().or(only)
}

/// Pick the desk to notify: `name` if given, else the default desk.
pub(crate) fn find_desk<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a Desk> {
    let known = || {
        config
            .desk
            .iter()
            .map(|desk| desk.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(wanted) = name.or_else(|| default_desk_name(config)) else {
        return Err(Error::Config(format!(
            "several desks configured ({}) and no default_desk; pass --desk",
            known()
        )));
    };
    config
        .desk
        .iter()
        .find(|desk| desk.name == wanted)
        .ok_or_else(|| Error::Config(format!("unknown desk \"{wanted}\". Configured desks: {}", known())))
}

/// Add or replace `desk` by name, keeping the other settings. The first desk, or any desk with `make_default`,
/// becomes `default_desk`.
pub(crate) fn upsert_desk(config: Option<Config>, desk: Desk, make_default: bool) -> Config {
    let mut config = config.unwrap_or_default();
    if make_default || config.default_desk.is_none() {
        config.default_desk = Some(desk.name.clone());
    }
    match config.desk.iter_mut().find(|existing| existing.name == desk.name) {
        Some(existing) => *existing = desk,
        None => config.desk.push(desk),
    }
    config
}

/// A TOML basic string. JSON string escapes are a subset of TOML's.
fn toml_string(value: &str) -> String {
    serde_json::Value::from(value).to_string()
}

fn toml_strings(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|value| toml_string(value)).collect();
    format!("[{}]", items.join(", "))
}

pub(crate) fn serialize_config(config: &Config) -> String {
    let mut lines = vec![
        "# nudge config, written by `nudge setup`.".to_owned(),
        "# Contains webhook secrets: keep it chmod 600 and out of git.".to_owned(),
        String::new(),
    ];
    if let Some(name) = &config.default_desk {
        lines.extend([format!("default_desk = {}", toml_string(name)), String::new()]);
    }
    for desk in &config.desk {
        lines.extend(["[[desk]]".to_owned(), format!("name = {}", toml_string(&desk.name))]);
        if let Some(description) = &desk.description {
            lines.push(format!("description = {}", toml_string(description)));
        }
        lines.extend([
            format!("url = {}", toml_string(&desk.url)),
            format!("authorization = {}", toml_string(&desk.authorization)),
        ]);
        if let Some(secret) = &desk.signing_secret {
            lines.push(format!("signing_secret = {}", toml_string(secret)));
        }
        lines.push(String::new());
    }
    // Only settings that differ from the defaults, so a later default change reaches this config too.
    let defaults = Auto::default();
    let auto = config.auto;
    let changed: Vec<String> = [
        (auto.min_turn_seconds != defaults.min_turn_seconds)
            .then(|| format!("min_turn_seconds = {}", auto.min_turn_seconds)),
        (auto.cooldown_seconds != defaults.cooldown_seconds)
            .then(|| format!("cooldown_seconds = {}", auto.cooldown_seconds)),
        (auto.tail_lines != defaults.tail_lines).then(|| format!("tail_lines = {}", auto.tail_lines)),
        (auto.toast != defaults.toast).then(|| format!("toast = {}", auto.toast)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !changed.is_empty() {
        lines.push("[auto]".to_owned());
        lines.extend(changed);
        lines.push(String::new());
    }
    if !config.scope.is_everyone() {
        lines.push("[scope]".to_owned());
        if !config.scope.workspaces.is_empty() {
            lines.push(format!("workspaces = {}", toml_strings(&config.scope.workspaces)));
        }
        if !config.scope.agents.is_empty() {
            lines.push(format!("agents = {}", toml_strings(&config.scope.agents)));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

/// Atomically write the config with mode 600 (a new directory gets mode 700).
pub(crate) fn write_config(path: &Path, config: &Config) -> Result<()> {
    let text = serialize_config(config);
    parse_config(&text, &path.to_string_lossy())?; // never write a file we can't read back
    if let Some(dir) = path.parent() {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let temp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(text.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    fs::rename(&temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use super::{
        Auto, Config, Desk, Scope, check_authorization, check_webhook_url, default_config_path, find_desk, load_config,
        parse_config, read_config, serialize_config, upsert_desk, validate_desk, write_config,
    };
    use crate::context::Env;
    use crate::error::Error;

    const VALID: &str = r#"
default_desk = "bridge"

[[desk]]
name = "bridge"
description = "Coordinator desk"
url = "https://desks.example/hook/bridge"
authorization = "Bearer desk-token"

[[desk]]
name = "research"
url = "https://desks.example/hook/research"
authorization = "Token xyz"
"#;

    fn desk(name: &str, authorization: &str) -> Desk {
        Desk {
            name: name.to_owned(),
            description: None,
            url: format!("https://desks.example/hook/{name}"),
            authorization: authorization.to_owned(),
            signing_secret: None,
        }
    }

    fn config_of(default_desk: Option<&str>, desk: Vec<Desk>) -> Config {
        Config {
            default_desk: default_desk.map(str::to_owned),
            desk,
            ..Config::default()
        }
    }

    fn config_error(result: Result<impl std::fmt::Debug, Error>) -> String {
        match result {
            Err(Error::Config(message)) => message,
            other => panic!("expected a config error, got {other:?}"),
        }
    }

    #[test]
    fn parses_desks_and_default_desk() {
        let config = parse_config(VALID, "cfg").unwrap();
        assert_eq!(config.default_desk.as_deref(), Some("bridge"));
        assert_eq!(
            config.desk.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["bridge", "research"]
        );
        assert_eq!(config.desk[1].description, None);
    }

    #[test]
    fn the_shipped_example_is_rejected_until_placeholders_are_filled_in() {
        let example = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml")).unwrap();
        assert!(config_error(parse_config(&example, "example")).contains("placeholder"));
    }

    #[test]
    fn http_is_accepted_only_for_loopback_hosts() {
        let loopback = VALID.replace("https://desks.example/hook/bridge", "http://127.0.0.1:8788/x");
        assert!(parse_config(&loopback, "cfg").is_ok());
        let remote = VALID.replace("https://desks.example/hook/bridge", "http://desks.example/x");
        assert!(config_error(parse_config(&remote, "cfg")).contains("desk.0.url: must be an https URL"));
    }

    #[test]
    fn rejects_invalid_configs_naming_the_field() {
        for (label, text, expected) in [
            ("no desks", r#"default_desk = "bridge""#.to_owned(), "desk:"),
            (
                "duplicate desk names",
                VALID.replace(r#"name = "research""#, r#"name = "bridge""#),
                r#"desk.1.name: duplicate desk name "bridge""#,
            ),
            (
                "unknown default_desk",
                VALID.replace(r#"default_desk = "bridge""#, r#"default_desk = "nope""#),
                r#"default_desk: no [[desk]] named "nope""#,
            ),
            (
                "multi-line authorization",
                VALID.replace(r#""Token xyz""#, r#""Token x\ny""#),
                "desk.1.authorization",
            ),
            (
                "bad desk name",
                VALID.replace(r#"name = "research""#, r#"name = "re search""#),
                "desk.1.name",
            ),
            ("invalid TOML", "[[desk]".to_owned(), "invalid TOML"),
        ] {
            assert!(config_error(parse_config(&text, "cfg")).contains(expected), "{label}");
        }
    }

    #[test]
    fn reports_every_problem_and_skips_cross_checks_over_wrong_types() {
        let message = config_error(parse_config("[[desk]]\nname = \"\"\nurl = 5\n", "cfg"));
        assert_eq!(
            message,
            "cfg: invalid config\n  desk.0.name: must be a non-empty string\n  \
             desk.0.name: may only contain letters, digits, \"-\" and \"_\"\n  \
             desk.0.url: Invalid input: expected string, received number\n  \
             desk.0.authorization: Invalid input: expected string, received undefined"
        );
    }

    #[test]
    fn default_config_path_precedence() {
        let path = |env: Env| default_config_path(&env).display().to_string();
        assert_eq!(
            path(Env::from([("NUDGE_CONFIG", "/x/c.toml"), ("XDG_CONFIG_HOME", "/xdg")])),
            "/x/c.toml"
        );
        assert_eq!(path(Env::from([("XDG_CONFIG_HOME", "/xdg")])), "/xdg/nudge/config.toml");
        assert_eq!(
            path(Env::from([("HOME", "/home/u")])),
            "/home/u/.config/nudge/config.toml"
        );
    }

    #[test]
    fn missing_config_reads_as_none_and_load_points_at_setup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.toml");
        assert_eq!(read_config(&path).unwrap(), None);
        assert!(config_error(load_config(&path)).contains("nudge setup"));
    }

    #[test]
    fn find_desk_by_name_default_or_only_desk() {
        let config = parse_config(VALID, "cfg").unwrap();
        assert_eq!(find_desk(&config, Some("research")).unwrap().name, "research");
        assert_eq!(find_desk(&config, None).unwrap().name, "bridge");
        let single = config_of(None, vec![desk("research", "Bearer b")]);
        assert_eq!(find_desk(&single, None).unwrap().name, "research");
    }

    #[test]
    fn find_desk_errors_name_the_configured_desks() {
        let two = config_of(None, vec![desk("bridge", "Bearer a"), desk("research", "Bearer b")]);
        assert!(config_error(find_desk(&two, None)).contains("no default_desk; pass --desk"));
        let config = parse_config(VALID, "cfg").unwrap();
        assert!(config_error(find_desk(&config, Some("nope"))).contains("Configured desks: bridge, research"));
    }

    #[test]
    fn validate_desk_strips_a_pasted_header_name_and_whitespace() {
        let desk = validate_desk(
            "bridge",
            " https://desks.example/x ",
            "Authorization: Bearer abc \n",
            None,
            None,
        )
        .unwrap();
        assert_eq!(desk.authorization, "Bearer abc");
        assert_eq!(desk.url, "https://desks.example/x");
    }

    #[test]
    fn validate_desk_reports_every_problem() {
        let message = config_error(validate_desk("bridge", "not a url", "", None, None));
        let url = message.find("url: must be an https URL").unwrap();
        let authorization = message.find("authorization: must be a non-empty string").unwrap();
        assert!(url < authorization);
    }

    #[test]
    fn setup_field_checks_never_echo_the_rejected_value() {
        assert_eq!(
            check_webhook_url(" https://desks.example/hook\n").unwrap(),
            "https://desks.example/hook"
        );
        assert_eq!(check_authorization("authorization: Bearer abc").unwrap(), "Bearer abc");
        for message in [
            config_error(check_webhook_url("http://secret-host.example/x")),
            config_error(check_authorization("Bearer super-secret\nsecond-line")),
            config_error(check_authorization("Bearer s3cr\u{e9}t")),
        ] {
            assert!(!message.contains("secret") && !message.contains("s3cr"), "{message}");
        }
    }

    #[test]
    fn upsert_first_desk_becomes_default_and_later_ones_only_when_asked() {
        let bridge = desk("bridge", "Bearer a");
        let research = desk("research", "Bearer b");
        let one = upsert_desk(None, bridge.clone(), false);
        assert_eq!(one, config_of(Some("bridge"), vec![bridge.clone()]));
        let two = upsert_desk(Some(one.clone()), research.clone(), false);
        assert_eq!(two, config_of(Some("bridge"), vec![bridge, research.clone()]));
        assert_eq!(
            upsert_desk(Some(one), research, true).default_desk.as_deref(),
            Some("research")
        );
    }

    #[test]
    fn upsert_existing_name_replaces_it_in_place_and_keeps_other_settings() {
        let mut two = config_of(Some("research"), vec![desk("bridge", "a"), desk("research", "b")]);
        two.auto.toast = false;
        two.scope.agents = vec!["api".into()];
        let rotated = desk("bridge", "Bearer rotated");
        let updated = upsert_desk(Some(two.clone()), rotated.clone(), false);
        assert_eq!(updated.desk, [rotated, desk("research", "b")]);
        assert_eq!(
            (updated.default_desk, updated.auto, updated.scope),
            (two.default_desk, two.auto, two.scope)
        );
    }

    #[test]
    fn write_config_uses_mode_600_in_a_mode_700_dir_and_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("nudge");
        let path = dir.join("config.toml");
        let mut bridge = desk("bridge", "Bearer a");
        bridge.description = Some("Says \"hi\"\\ and\tmore".into());
        bridge.signing_secret = Some("0123456789abcdef-secret".into());
        let mut config = config_of(Some("bridge"), vec![bridge, desk("research", "Bearer b")]);
        config.auto = Auto {
            min_turn_seconds: 30,
            cooldown_seconds: 0,
            tail_lines: 80,
            toast: false,
        };
        config.scope = Scope {
            workspaces: vec!["desk \"bridge\"".into()],
            agents: vec!["api-refactor".into(), "w1:p2".into()],
        };
        write_config(&path, &config).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(load_config(&path).unwrap(), config);
    }

    #[test]
    fn write_config_tightens_an_existing_world_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, VALID).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_config(
            &path,
            &upsert_desk(Some(load_config(&path).unwrap()), desk("research", "Bearer b"), false),
        )
        .unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let names: Vec<String> = load_config(&path).unwrap().desk.into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["bridge", "research"]);
    }

    #[test]
    fn serialized_output_omits_absent_and_default_settings() {
        let mut config = config_of(None, vec![desk("bridge", "Bearer a")]);
        let text = serialize_config(&config);
        for absent in ["default_desk", "description", "signing_secret", "[auto]", "[scope]"] {
            assert!(!text.contains(absent), "{absent}");
        }
        config.auto.cooldown_seconds = 60;
        let text = serialize_config(&config);
        assert!(text.contains("[auto]\ncooldown_seconds = 60\n"), "{text}");
        assert!(!text.contains("min_turn_seconds"), "{text}");
    }

    #[test]
    fn auto_and_scope_settings_parse_with_defaults_for_missing_keys() {
        let text = format!("{VALID}\n[auto]\nmin_turn_seconds = 0\ntail_lines = 200\n\n[scope]\nagents = [\"api\"]\n");
        let config = parse_config(&text, "cfg").unwrap();
        let expected = Auto {
            min_turn_seconds: 0,
            tail_lines: 200,
            ..Auto::default()
        };
        assert_eq!(config.auto, expected);
        assert_eq!(config.scope.agents, ["api"]);
        assert!(config.scope.workspaces.is_empty());
    }

    #[test]
    fn rejects_bad_auto_and_scope_settings_naming_each() {
        let text = format!(
            "{VALID}\n[auto]\nmin_turn_seconds = -1\ncooldown_seconds = 1.5\ntail_lines = 0\ntoast = \"yes\"\n\
             min_turn = 5\n\n[scope]\nagents = [\"\"]\nworkspaces = \"base\"\nteams = []\n"
        );
        let message = config_error(parse_config(&text, "cfg"));
        for expected in [
            "auto.min_turn_seconds: must be a whole number of seconds, 0 or more",
            "auto.cooldown_seconds: must be a whole number of seconds, 0 or more",
            "auto.tail_lines: must be a whole number from 1 to 200",
            "auto.toast: Invalid input: expected boolean, received string",
            "auto.min_turn: unknown setting",
            "scope.agents.0: must be a non-empty string",
            "scope.workspaces: Invalid input: expected array, received string",
            "scope.teams: unknown setting",
        ] {
            assert!(message.contains(expected), "missing {expected:?} in\n{message}");
        }
    }

    #[test]
    fn signing_secrets_must_be_long_and_are_never_echoed() {
        let short = VALID.replace(
            "authorization = \"Token xyz\"",
            "authorization = \"Token xyz\"\nsigning_secret = \"tiny-secret\"",
        );
        let message = config_error(parse_config(&short, "cfg"));
        assert!(message.contains("desk.1.signing_secret: must be at least 16 printable ASCII characters"));
        assert!(!message.contains("tiny-secret"));
        let desk = validate_desk("d", "https://x.example/h", "Bearer a", None, Some(" 0123456789abcdef ")).unwrap();
        assert_eq!(desk.signing_secret.as_deref(), Some("0123456789abcdef"));
    }

    #[test]
    fn durations_in_words() {
        assert_eq!(
            [120, 60, 90, 1, 0].map(super::seconds_in_words),
            ["2 minutes", "1 minute", "90 seconds", "1 second", "0 seconds"]
        );
    }
}
