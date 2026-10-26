//! The `[[webhook]]` tables: each webhook's fields, and unique names across them.
use toml::Value;

use super::super::model::Webhook;
use super::super::problems::Problems;

impl Problems {
    /// The `[[webhook]]` array; at least one, with unique names.
    pub(super) fn webhooks(&mut self, value: Option<&Value>) -> Option<Vec<Webhook>> {
        let Some(Value::Array(items)) = value else {
            self.wrong_type("webhook", "array", value);
            return None;
        };
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
            self.duplicate_names(&webhooks);
        }
        webhooks.into_iter().collect::<Option<Vec<Webhook>>>()
    }

    fn duplicate_names(&mut self, webhooks: &[Option<Webhook>]) {
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
}
