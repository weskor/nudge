//! The webhook config (`~/.config/nudge/config.toml`): parsing with every problem reported, and safe writes.
use std::fmt;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use toml::Value;

use crate::env::Env;
use crate::error::{Error, Result};
use crate::terminal::tty::Redacted;

const PLACEHOLDER_PREFIX: &str = "REPLACE_WITH";
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const AUTHORIZATION_PREFIX: &str = "authorization:";
/// Top-level keys of the old config format, as (old, new). A config that uses only the old keys is rewritten in
/// the new format when nudge reads it.
const RENAMED_KEYS: [(&str, &str); 2] = [("desk", "webhook"), ("default_desk", "default_webhook")];

/// A configured webhook. `Debug` redacts the Authorization header and the signing secret.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Webhook {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) url: String,
    /// Sent verbatim as the webhook's Authorization header value.
    pub(crate) authorization: String,
    /// When set, requests are signed with HMAC-SHA256 (`x-nudge-signature`) so the webhook can verify them.
    pub(crate) signing_secret: Option<String>,
}

impl fmt::Debug for Webhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Webhook")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("url", &self.url)
            .field("authorization", &Redacted(&self.authorization))
            .field("signing_secret", &self.signing_secret.as_deref().map(Redacted))
            .finish()
    }
}

/// `[auto]`: when the herdr plugin notifies the webhook on its own.
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

/// `[scope]`: which agents may notify the webhook. Empty means every agent.
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
    pub(crate) default_webhook: Option<String>,
    pub(crate) webhook: Vec<Webhook>,
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

    fn webhook_name(&mut self, path: &str, raw: &str) -> String {
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

    fn webhook(&mut self, path: &str, value: &Value) -> Option<Webhook> {
        let Value::Table(table) = value else {
            self.wrong_type(path, "object", Some(value));
            return None;
        };
        let field = |key: &str| format!("{path}.{key}");
        let name = self
            .string(&field("name"), table.get("name"))
            .map(|raw| self.webhook_name(&field("name"), raw));
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
        Some(Webhook {
            name: name?,
            description: description?,
            url: url?,
            authorization: authorization?,
            signing_secret: signing_secret?,
        })
    }

    fn config(&mut self, raw: &toml::Table) -> Option<Config> {
        for (old, new) in RENAMED_KEYS {
            if raw.contains_key(old) {
                self.add(
                    old,
                    &format!("old name of {new}; rename it (a file mixing old and new names is not updated for you)"),
                );
            }
        }
        let default_webhook = match raw.get("default_webhook") {
            None => Some(None),
            Some(value) => self
                .string("default_webhook", Some(value))
                .map(|name| Some(self.webhook_name("default_webhook", name))),
        };
        let webhooks = match raw.get("webhook") {
            Some(Value::Array(items)) => {
                let before = self.wrong_types;
                let webhooks: Vec<Option<Webhook>> = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| self.webhook(&format!("webhook.{index}"), item))
                    .collect();
                if items.is_empty() {
                    self.add("webhook", "define at least one [[webhook]] table");
                }
                if self.wrong_types == before {
                    for (index, name) in webhooks.iter().flatten().map(|webhook| &webhook.name).enumerate() {
                        if webhooks
                            .iter()
                            .flatten()
                            .take(index)
                            .any(|earlier| &earlier.name == name)
                        {
                            self.add(
                                &format!("webhook.{index}.name"),
                                &format!("duplicate webhook name \"{name}\""),
                            );
                        }
                    }
                }
                webhooks.into_iter().collect::<Option<Vec<Webhook>>>()
            }
            other => {
                self.wrong_type("webhook", "array", other);
                None
            }
        };
        let auto = self.auto(raw.get("auto"));
        let scope = self.scope(raw.get("scope"));
        let config = Config {
            default_webhook: default_webhook?,
            webhook: webhooks?,
            auto,
            scope,
        };
        if let Some(name) = &config.default_webhook
            && !config.webhook.iter().any(|webhook| &webhook.name == name)
        {
            self.add("default_webhook", &format!("no [[webhook]] named \"{name}\""));
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

/// Validate a webhook name entered during setup.
pub(crate) fn check_webhook_name(value: &str) -> Result<String> {
    let mut problems = Problems::default();
    let name = problems.webhook_name("", value);
    problems.into_result(name, "invalid webhook name")
}

/// Validate one webhook entered via `nudge setup`.
pub(crate) fn validate_webhook(
    name: &str,
    url: &str,
    authorization: &str,
    description: Option<&str>,
    signing_secret: Option<&str>,
) -> Result<Webhook> {
    let mut problems = Problems::default();
    let webhook = Webhook {
        name: problems.webhook_name("name", name),
        description: description.map(str::to_owned),
        url: problems.webhook_url("url", url),
        authorization: problems.authorization("authorization", authorization),
        signing_secret: signing_secret.map(|secret| problems.signing_secret("signing_secret", secret)),
    };
    problems.into_result(webhook, "invalid webhook")
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

fn parse_toml(text: &str, source: &str) -> Result<toml::Table> {
    toml::from_str(text).map_err(|error| Error::Config(format!("{source}: invalid TOML: {}", error.message())))
}

fn validate_config(raw: &toml::Table, source: &str) -> Result<Config> {
    let mut problems = Problems::default();
    let config = problems.config(raw);
    let label = format!("{source}: invalid config");
    problems.into_result(config, &label)?.ok_or(Error::Config(label))
}

/// Parse and validate config TOML. `source` names the file in error messages.
pub(crate) fn parse_config(text: &str, source: &str) -> Result<Config> {
    validate_config(&parse_toml(text, source)?, source)
}

/// Load the config, or `None` if the file does not exist. A config in the old format is rewritten in the new one
/// first, with a note on stderr.
pub(crate) fn read_config(path: &Path) -> Result<Option<Config>> {
    let found = read_and_migrate(path)?;
    if let Some((_, Some(note))) = &found {
        eprintln!("nudge: {note}");
    }
    Ok(found.map(|(config, _)| config))
}

/// The config at `path`, if any. A file that uses only the old keys of [`RENAMED_KEYS`] is rewritten with the new
/// ones (keeping every other setting), and the note says so.
fn read_and_migrate(path: &Path) -> Result<Option<(Config, Option<String>)>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Other(format!("{}: {error}", path.display()))),
    };
    let source = path.to_string_lossy();
    let mut raw = parse_toml(&text, &source)?;
    let old_format = RENAMED_KEYS.iter().any(|(old, _)| raw.contains_key(*old))
        && !RENAMED_KEYS.iter().any(|(_, new)| raw.contains_key(*new));
    if old_format {
        for (old, new) in RENAMED_KEYS {
            if let Some(value) = raw.remove(old) {
                raw.insert(new.to_owned(), value);
            }
        }
    }
    let config = validate_config(&raw, &source)?;
    if !old_format {
        return Ok(Some((config, None)));
    }
    write_config(path, &config)?;
    let note = format!(
        "updated {} to the new format ([[webhook]], default_webhook)",
        path.display()
    );
    Ok(Some((config, Some(note))))
}

pub(crate) fn load_config(path: &Path) -> Result<Config> {
    read_config(path)?.ok_or_else(|| {
        Error::Config(format!(
            "no config at {}. Run \"nudge setup\" to add a webhook.",
            path.display()
        ))
    })
}

/// `default_webhook`, else the only webhook's name; `None` when several webhooks and no default.
pub(crate) fn default_webhook_name(config: &Config) -> Option<&str> {
    let only = match config.webhook.as_slice() {
        [only] => Some(only.name.as_str()),
        [] | [_, _, ..] => None,
    };
    config.default_webhook.as_deref().or(only)
}

/// Pick the webhook to notify: `name` if given, else the default webhook.
pub(crate) fn find_webhook<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a Webhook> {
    let known = || {
        config
            .webhook
            .iter()
            .map(|webhook| webhook.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(wanted) = name.or_else(|| default_webhook_name(config)) else {
        return Err(Error::Config(format!(
            "several webhooks configured ({}) and no default_webhook; pass --webhook",
            known()
        )));
    };
    config
        .webhook
        .iter()
        .find(|webhook| webhook.name == wanted)
        .ok_or_else(|| {
            Error::Config(format!(
                "unknown webhook \"{wanted}\". Configured webhooks: {}",
                known()
            ))
        })
}

/// Add or replace `webhook` by name, keeping the other settings. The first webhook, or any webhook with
/// `make_default`, becomes `default_webhook`.
pub(crate) fn upsert_webhook(config: Option<Config>, webhook: Webhook, make_default: bool) -> Config {
    let mut config = config.unwrap_or_default();
    if make_default || config.default_webhook.is_none() {
        config.default_webhook = Some(webhook.name.clone());
    }
    match config.webhook.iter_mut().find(|existing| existing.name == webhook.name) {
        Some(existing) => *existing = webhook,
        None => config.webhook.push(webhook),
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
    if let Some(name) = &config.default_webhook {
        lines.extend([format!("default_webhook = {}", toml_string(name)), String::new()]);
    }
    for webhook in &config.webhook {
        lines.extend([
            "[[webhook]]".to_owned(),
            format!("name = {}", toml_string(&webhook.name)),
        ]);
        if let Some(description) = &webhook.description {
            lines.push(format!("description = {}", toml_string(description)));
        }
        lines.extend([
            format!("url = {}", toml_string(&webhook.url)),
            format!("authorization = {}", toml_string(&webhook.authorization)),
        ]);
        if let Some(secret) = &webhook.signing_secret {
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

/// Atomically and durably write the config with mode 600 (a new directory gets mode 700). A symlinked config is
/// written through the link (the link stays), and nothing is left behind when the write fails.
pub(crate) fn write_config(path: &Path, config: &Config) -> Result<()> {
    let text = serialize_config(config);
    parse_config(&text, &path.to_string_lossy())?; // never write a file we can't read back
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if let Some(dir) = path.parent() {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let temp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    // A temp file left by a crashed run with the same pid (or a link planted there) is replaced, never followed.
    match fs::remove_file(&temp) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
        Ok(()) | Err(_) => {}
    }
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            // On disk before it replaces the old config, so a crash can't leave an empty file holding the secrets.
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temp, &path));
    if written.is_err() {
        fs::remove_file(&temp).ok();
    }
    Ok(written?)
}

#[cfg(test)]
mod tests;
