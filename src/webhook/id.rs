//! Notification ids: random UUID v4s, so the webhook can drop duplicates.
use ring::rand::{SecureRandom as _, SystemRandom};

use super::hex::hex;
use crate::error::{Error, Result};

/// A random UUID v4.
pub(super) fn event_id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| Error::Other("no system randomness for the notification id".into()))?;
    if let Some(version) = bytes.get_mut(6) {
        *version = (*version & 0x0f) | 0x40;
    }
    if let Some(variant) = bytes.get_mut(8) {
        *variant = (*variant & 0x3f) | 0x80;
    }
    let mut rest = bytes.iter().copied();
    let groups: Vec<String> = [4, 2, 2, 2, 6]
        .into_iter()
        .map(|len| hex(&rest.by_ref().take(len).collect::<Vec<u8>>()))
        .collect();
    Ok(groups.join("-"))
}

#[cfg(test)]
mod tests;
