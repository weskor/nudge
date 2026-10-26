//! The preset values the arrow keys and space step through, and how a value moves to the next one.

pub(super) const TURN_STEPS: [u32; 9] = [0, 30, 60, 120, 180, 300, 600, 900, 1800];
pub(super) const COOLDOWN_STEPS: [u32; 8] = [0, 60, 120, 300, 600, 900, 1800, 3600];
pub(super) const TAIL_STEPS: [u32; 6] = [10, 20, 40, 80, 120, 200];

/// The next preset above (`up`) or below `value`; stays put at either end. Values between presets snap to the
/// nearest one in that direction.
pub(super) fn step(value: u32, steps: &[u32], up: bool) -> u32 {
    if up {
        steps.iter().copied().find(|&preset| preset > value).unwrap_or(value)
    } else {
        steps
            .iter()
            .rev()
            .copied()
            .find(|&preset| preset < value)
            .unwrap_or(value)
    }
}

/// The next preset above `value`, wrapping to the smallest.
pub(super) fn cycle(value: u32, steps: &[u32]) -> u32 {
    steps
        .iter()
        .copied()
        .find(|&preset| preset > value)
        .or_else(|| steps.first().copied())
        .unwrap_or(value)
}

#[cfg(test)]
mod tests;
