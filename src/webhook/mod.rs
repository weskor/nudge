//! Build a notification and POST it to a webhook.
mod hex;
mod id;
mod notify;
mod origin;
mod payload;
mod response;
mod send;
mod status;
mod time;

pub(crate) use notify::{Sent, notify_webhook, ping_webhook};
pub(crate) use origin::{Origin, current_dir};
pub(crate) use payload::{Notification, build_notification};
pub(crate) use send::post_to_webhook;
pub(crate) use status::NotifyStatus;

#[cfg(test)]
pub(crate) mod tests;
