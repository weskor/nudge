use super::redact;

#[test]
fn redacts_credentials() {
    for (input, expected) in [
        (
            "Authorization: Bearer abcdef0123456789xyz",
            "Authorization: Bearer [REDACTED]",
        ),
        (
            "export OPENAI_API_KEY=sk-proj-abcdefghijklmnop1234",
            "export OPENAI_API_KEY=[REDACTED]",
        ),
        (r#"password: "hunter2 with spaces""#, "password: [REDACTED]"),
        (
            "token ghp_abcdefghijklmnopqrstuvwxyz0123456789 leaked",
            "token [REDACTED] leaked",
        ),
        ("AWS key AKIAABCDEFGHIJKLMNOP here", "AWS key [REDACTED] here"),
        (
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "jwt [REDACTED]",
        ),
        ("blob Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb", "blob [REDACTED]"),
        (
            "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----",
            "[REDACTED]",
        ),
    ] {
        assert_eq!(redact(input), expected, "{input}");
    }
}

#[test]
fn keeps_ordinary_output() {
    let text = [
        "commit 8a25740f3c2d1e0b9a8f7e6d5c4b3a2f1e0d9c8b",
        "src/core/very_long_module_name_for_configuration_loading.ts",
        "see /tmp/Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb/x and Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb.log",
        " 51 pass, 0 fail",
        "Finished the refactor; tokens are now rotated nightly.",
    ]
    .join("\n");
    assert_eq!(redact(&text), text);
}
