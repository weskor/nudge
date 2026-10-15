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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) default_desk: Option<String>,
    pub(crate) desk: Vec<Desk>,
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
        Some(Desk {
            name: name?,
            description: description?,
            url: url?,
            authorization: authorization?,
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
        let config = Config {
            default_desk: default_desk?,
            desk: desks?,
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
pub(crate) fn validate_desk(name: &str, url: &str, authorization: &str, description: Option<&str>) -> Result<Desk> {
    let mut problems = Problems::default();
    let desk = Desk {
        name: problems.desk_name("name", name),
        description: description.map(str::to_owned),
        url: problems.webhook_url("url", url),
        authorization: problems.authorization("authorization", authorization),
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

/// Add or replace `desk` by name. The first desk, or any desk with `make_default`, becomes `default_desk`.
pub(crate) fn upsert_desk(config: Option<Config>, desk: Desk, make_default: bool) -> Config {
    let (mut desks, default_desk) =
        config.map_or_else(|| (Vec::new(), None), |config| (config.desk, config.default_desk));
    let default_desk = match default_desk {
        Some(name) if !make_default => name,
        Some(_) | None => desk.name.clone(),
    };
    match desks.iter_mut().find(|existing| existing.name == desk.name) {
        Some(existing) => *existing = desk,
        None => desks.push(desk),
    }
    Config {
        default_desk: Some(default_desk),
        desk: desks,
    }
}

/// A TOML basic string. JSON string escapes are a subset of TOML's.
fn toml_string(value: &str) -> String {
    serde_json::Value::from(value).to_string()
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
            String::new(),
        ]);
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
        Config, Desk, check_authorization, check_webhook_url, default_config_path, find_desk, load_config,
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
        let single = Config {
            default_desk: None,
            desk: vec![desk("research", "Bearer b")],
        };
        assert_eq!(find_desk(&single, None).unwrap().name, "research");
    }

    #[test]
    fn find_desk_errors_name_the_configured_desks() {
        let two = Config {
            default_desk: None,
            desk: vec![desk("bridge", "Bearer a"), desk("research", "Bearer b")],
        };
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
        )
        .unwrap();
        assert_eq!(desk.authorization, "Bearer abc");
        assert_eq!(desk.url, "https://desks.example/x");
    }

    #[test]
    fn validate_desk_reports_every_problem() {
        let message = config_error(validate_desk("bridge", "not a url", "", None));
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
        assert_eq!(
            one,
            Config {
                default_desk: Some("bridge".into()),
                desk: vec![bridge.clone()]
            }
        );
        let two = upsert_desk(Some(one.clone()), research.clone(), false);
        assert_eq!(
            two,
            Config {
                default_desk: Some("bridge".into()),
                desk: vec![bridge, research.clone()]
            }
        );
        assert_eq!(
            upsert_desk(Some(one), research, true).default_desk.as_deref(),
            Some("research")
        );
    }

    #[test]
    fn upsert_existing_name_replaces_it_in_place() {
        let two = Config {
            default_desk: Some("research".into()),
            desk: vec![desk("bridge", "a"), desk("research", "b")],
        };
        let rotated = desk("bridge", "Bearer rotated");
        let updated = upsert_desk(Some(two), rotated.clone(), false);
        assert_eq!(
            updated,
            Config {
                default_desk: Some("research".into()),
                desk: vec![rotated, desk("research", "b")]
            }
        );
    }

    #[test]
    fn write_config_uses_mode_600_in_a_mode_700_dir_and_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("nudge");
        let path = dir.join("config.toml");
        let mut bridge = desk("bridge", "Bearer a");
        bridge.description = Some("Says \"hi\"\\ and\tmore".into());
        let config = Config {
            default_desk: Some("bridge".into()),
            desk: vec![bridge, desk("research", "Bearer b")],
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
    fn serialized_output_omits_absent_optional_fields() {
        let text = serialize_config(&Config {
            default_desk: None,
            desk: vec![desk("bridge", "Bearer a")],
        });
        assert!(!text.contains("default_desk") && !text.contains("description"));
    }
}
