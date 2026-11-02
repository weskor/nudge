//! Pure logic for automatic notifications: which agent status changes become webhook notifications, and how the
//! pane's terminal output is trimmed and redacted before it leaves the machine.
mod message;
mod redact;
mod rules;

pub(crate) use message::{Trigger, auto_message};
pub(crate) use rules::{Decision, Settled, decide};
