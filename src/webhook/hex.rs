//! Lowercase hex encoding, for notification ids and request signatures.

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|digit| char::from_digit(u32::from(digit), 16))
        .collect()
}
