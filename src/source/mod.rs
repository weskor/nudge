//! What a notification comes from, independent of the herdr TUI or herdr-core: the agent (`AgentRef`), its status
//! change (`Transition`), its backend and what that backend can do (`AgentSource`), and this machine's name.
mod agent;
mod backend;
mod host;
mod transition;

pub(crate) use agent::{AgentRef, WorkspaceRef};
pub(crate) use backend::{AgentSource, Backend};
pub(crate) use host::host_name;
pub(crate) use transition::{AgentStatus, Transition};
