//! The herdr-core backend: `hc events` parsed at the boundary, the watcher (`nudge core watch`, one per machine,
//! with its lock and `watch.json`), its login service (`nudge core service`), the desktop app's context
//! (`nudge core context`, for focus and space names), herdr-core as an `AgentSource` through `hc`,
//! `nudge core send`, who runs `nudge notify` inside a core terminal, and the core half of `nudge status --json`.
mod context;
mod events;
mod follow;
mod lock;
mod panel;
mod send;
mod service;
mod source;
mod status_file;
mod tracker;
mod unit_file;
mod watch;

pub(crate) use context::save_app_context;
pub(crate) use panel::{CoreStatus, core_status};
pub(crate) use send::send_terminal;
pub(crate) use service::{ServiceAction, service};
pub(crate) use source::{core_reply_hint, core_sender};
pub(crate) use watch::watch;
