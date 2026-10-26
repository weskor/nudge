//! The old config format (`[[desk]]`, `default_desk`) and its upgrade to the new key names.
/// Top-level keys of the old config format, as (old, new). A config that uses only the old keys is rewritten in
/// the new format when nudge reads it.
pub(super) const RENAMED_KEYS: [(&str, &str); 2] = [("desk", "webhook"), ("default_desk", "default_webhook")];

/// Rename the old keys of [`RENAMED_KEYS`] in `raw` when it uses only old ones; whether it did. A table mixing
/// old and new keys is left alone (validation reports the old ones).
pub(super) fn rename_old_keys(raw: &mut toml::Table) -> bool {
    let old_format = RENAMED_KEYS.iter().any(|(old, _)| raw.contains_key(*old))
        && !RENAMED_KEYS.iter().any(|(_, new)| raw.contains_key(*new));
    if old_format {
        for (old, new) in RENAMED_KEYS {
            if let Some(value) = raw.remove(old) {
                raw.insert(new.to_owned(), value);
            }
        }
    }
    old_format
}
