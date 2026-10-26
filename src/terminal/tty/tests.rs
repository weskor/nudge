use super::mask_secret;

#[test]
fn mask_keeps_the_scheme_and_the_last_4_characters_of_long_secrets() {
    assert_eq!(mask_secret("Bearer abcdefghijklmnopqrstuvwxyz"), "Bearer ••••••••wxyz");
    assert_eq!(mask_secret("abcdefghijklmnopqrstuvwxyz"), "••••••••wxyz");
}

#[test]
fn mask_reveals_nothing_of_short_secrets() {
    assert_eq!(mask_secret("Bearer short-token-1"), "Bearer ••••••••");
    assert_eq!(mask_secret("Bearer 123456789012345"), "Bearer ••••••••");
    assert_eq!(mask_secret("Bearer 1234567890123456"), "Bearer ••••••••3456");
}
