//! The `[core]` table: which terminals the herdr-core watcher handles.
use toml::Value;

use super::super::model::CoreSettings;
use super::super::problems::Problems;

impl Problems {
    pub(super) fn core(&mut self, value: Option<&Value>) -> CoreSettings {
        let mut core = CoreSettings::default();
        let Some(value) = value else { return core };
        let Value::Table(table) = value else {
            self.wrong_type("core", "object", Some(value));
            return core;
        };
        for (key, value) in table {
            let path = format!("core.{key}");
            match key.as_str() {
                "watch" => match self.string(&path, Some(value)) {
                    Some("own") => core.watch_connected = false,
                    Some("connected") => core.watch_connected = true,
                    Some(_) => self.add(&path, r#"must be "own" or "connected""#),
                    None => {}
                },
                _ => self.add(&path, "unknown setting"),
            }
        }
        core
    }
}
