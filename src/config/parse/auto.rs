//! The `[auto]` table: whole-number settings in range, unknown keys reported, missing ones defaulted.
use std::ops::RangeInclusive;

use toml::Value;

use super::super::model::Auto;
use super::super::problems::Problems;

impl Problems {
    pub(super) fn auto(&mut self, value: Option<&Value>) -> Auto {
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

    /// An integer in `range`; a number outside it, or a fraction, is reported with `message`.
    fn integer(&mut self, path: &str, value: &Value, range: RangeInclusive<u32>, message: &str) -> Option<u32> {
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
}
