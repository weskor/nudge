//! The herdr-core backend: `hc events` parsed at the boundary, the watcher (`nudge core watch`, one per machine,
//! with its lock and `watch.json`), herdr-core as an `AgentSource` through `hc`, `nudge core send`, and who runs
//! `nudge notify` inside a core terminal.
mod events;
mod follow;
mod lock;
mod send;
mod source;
mod status_file;
mod tracker;
mod watch;

pub(crate) use send::send_terminal;
pub(crate) use source::{core_reply_hint, core_sender};
pub(crate) use watch::watch;
