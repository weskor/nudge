//! `Problems`: validation problems collected so one error lists all of them.
use toml::Value;

use crate::error::{Error, Result};

/// Validation problems, collected so one error lists all of them. Messages never include the input value,
/// so secrets are not echoed.
#[derive(Debug, Default)]
pub(super) struct Problems {
    lines: Vec<String>,
    /// A value had the wrong type somewhere; cross-field checks over it are skipped.
    pub(super) wrong_types: usize,
}

impl Problems {
    pub(super) fn add(&mut self, path: &str, message: &str) {
        self.lines.push(if path.is_empty() {
            format!("  {message}")
        } else {
            format!("  {path}: {message}")
        });
    }

    pub(super) fn wrong_type(&mut self, path: &str, expected: &str, got: Option<&Value>) {
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

    pub(super) fn into_result<T>(self, value: T, label: &str) -> Result<T> {
        if self.lines.is_empty() {
            Ok(value)
        } else {
            Err(Error::Config(format!("{label}\n{}", self.lines.join("\n"))))
        }
    }
}
