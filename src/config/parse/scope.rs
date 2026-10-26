//! The `[scope]` table: lists of non-empty workspace and agent names.
use toml::Value;

use super::super::model::Scope;
use super::super::problems::Problems;

impl Problems {
    pub(super) fn scope(&mut self, value: Option<&Value>) -> Scope {
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
}
