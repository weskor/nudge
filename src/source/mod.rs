//! What a notification comes from, independent of the herdr TUI or herdr-core: the agent (`AgentRef`), its status
//! change (`Transition`), its backend and what that backend can do (`AgentSource`), this machine's name, and
//! quoting a reply target for a reply hint.
mod agent;
mod backend;
mod host;
mod shell;
mod transition;

pub(crate) use agent::{AgentRef, WorkspaceRef};
pub(crate) use backend::{AgentSource, Backend};
pub(crate) use host::host_name;
pub(crate) use shell::shell_word;
pub(crate) use transition::{AgentStatus, Transition};
