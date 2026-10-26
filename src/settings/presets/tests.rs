use super::{cycle, step};

#[test]
fn values_step_through_presets_and_snap_from_custom_values() {
    assert_eq!(step(120, &[0, 60, 120, 300], true), 300);
    assert_eq!(step(120, &[0, 60, 120, 300], false), 60);
    assert_eq!(step(300, &[0, 60, 120, 300], true), 300, "stays at the top");
    assert_eq!(step(90, &[0, 60, 120, 300], true), 120, "custom value snaps up");
    assert_eq!(step(90, &[0, 60, 120, 300], false), 60, "custom value snaps down");
    assert_eq!(cycle(300, &[0, 60, 120, 300]), 0, "space wraps around");
}
