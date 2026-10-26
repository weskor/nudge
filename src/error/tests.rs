use super::{Blame, WebhookError};

#[test]
fn blames_points_at_the_authorization_header_or_the_url() {
    let blame = |status| {
        WebhookError {
            message: String::new(),
            http_status: status,
        }
        .blames()
    };
    assert_eq!(blame(Some(401)), Some(Blame::Authorization));
    assert_eq!(blame(Some(403)), Some(Blame::Authorization));
    assert_eq!(blame(Some(404)), Some(Blame::Url));
    assert_eq!(blame(Some(308)), Some(Blame::Url));
    assert_eq!(blame(Some(500)), None);
    assert_eq!(blame(None), None);
}
