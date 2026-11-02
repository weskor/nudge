//! The `[[route]]` tables: each route's selector, webhook and backend, then the checks across routes.
use toml::Value;

use super::super::model::Webhook;
use super::super::problems::Problems;
use super::super::routes::{Route, Selector};
use crate::source::Backend;

impl Problems {
    /// The `[[route]]` array; absent means no routes. Each valid route is checked against `webhooks` (when they
    /// parsed) and the routes before it, even when another route is invalid.
    pub(super) fn routes(&mut self, value: Option<&Value>, webhooks: Option<&[Webhook]>) -> Option<Vec<Route>> {
        let Some(value) = value else {
            return Some(Vec::new());
        };
        let Value::Array(items) = value else {
            self.wrong_type("route", "array", Some(value));
            return None;
        };
        let routes: Vec<Option<Route>> = items
            .iter()
            .enumerate()
            .map(|(index, item)| self.route(&format!("route.{index}"), item))
            .collect();
        if let Some(webhooks) = webhooks {
            self.route_links(&routes, webhooks);
        }
        routes.into_iter().collect()
    }

    fn route(&mut self, path: &str, value: &Value) -> Option<Route> {
        let Value::Table(table) = value else {
            self.wrong_type(path, "object", Some(value));
            return None;
        };
        let field = |key: &str| format!("{path}.{key}");
        for key in table.keys() {
            if !["pane", "workspace", "webhook", "backend"].contains(&key.as_str()) {
                self.add(&field(key), "unknown setting");
            }
        }
        let pane = table.get("pane").map(|raw| self.string(&field("pane"), Some(raw)));
        let workspace = table
            .get("workspace")
            .map(|raw| self.string(&field("workspace"), Some(raw)));
        let selector = match (pane, workspace) {
            (Some(pane), None) => pane.map(|raw| Selector::Pane(self.non_empty(&field("pane"), raw))),
            (None, Some(workspace)) => {
                workspace.map(|raw| Selector::Workspace(self.non_empty(&field("workspace"), raw)))
            }
            (Some(_), Some(_)) | (None, None) => {
                self.add(path, "set exactly one of pane or workspace");
                None
            }
        };
        let webhook = self
            .string(&field("webhook"), table.get("webhook"))
            .map(|raw| self.webhook_name(&field("webhook"), raw));
        let backend = match table.get("backend") {
            None => Some(None),
            Some(value) => self.string(&field("backend"), Some(value)).and_then(|raw| {
                let backend = Backend::parse(raw);
                if backend.is_none() {
                    self.add(&field("backend"), r#"must be "tui" or "core""#);
                }
                backend.map(Some)
            }),
        };
        Some(Route {
            selector: selector?,
            webhook: webhook?,
            backend: backend?,
        })
    }

    /// Every route names a configured webhook, and no route repeats an earlier one's selector and qualifiers (it
    /// would never apply).
    fn route_links(&mut self, routes: &[Option<Route>], webhooks: &[Webhook]) {
        for (index, route) in routes.iter().enumerate() {
            let Some(route) = route else { continue };
            if !webhooks.iter().any(|webhook| webhook.name == route.webhook) {
                self.add(
                    &format!("route.{index}.webhook"),
                    &format!("no [[webhook]] named \"{}\"", route.webhook),
                );
            }
            if routes
                .iter()
                .take(index)
                .flatten()
                .any(|earlier| earlier.same_target(route))
            {
                self.add(
                    &format!("route.{index}"),
                    &format!(
                        "duplicate route for {} \"{}\"; an earlier route already matches it",
                        route.selector.key(),
                        route.selector.name()
                    ),
                );
            }
        }
    }
}
