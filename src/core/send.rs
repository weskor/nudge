//! `nudge core send <terminal> [--blocked]`: send a core terminal's recent output to its webhook now, the core's
//! "send this pane".
use super::source::CoreSource;
use crate::auto::Trigger;
use crate::config::{default_config_path, load_config};
use crate::env::{Env, System};
use crate::error::{Error, Result};
use crate::pipeline::{Context, locate, send};
use crate::webhook::NotifyStatus;

/// Send `terminal` (an address or name) as a question, or as blocked; you chose it, so `[scope]` doesn't apply,
/// its route does.
pub(crate) fn send_terminal(terminal: &str, blocked: bool, env: &Env) -> Result<u8> {
    let path = default_config_path(env);
    let config = load_config(&path)?;
    let home = env.home();
    let source = CoreSource {
        home: &home,
        run: &System,
    };
    let agent = source
        .agent(terminal)
        .ok_or_else(|| Error::Other(format!("herdr-core has no terminal \"{terminal}\" (see hc ls)")))?;
    let agent = locate(agent, &config, false, &source);
    let status = if blocked {
        NotifyStatus::Blocked
    } else {
        NotifyStatus::Question
    };
    let ctx = Context {
        env,
        run: &System,
        home: &home,
        config_path: &path,
    };
    let sent = send(&agent, status, Trigger::Manual, &source, &config, &ctx)?;
    println!("Sent {} to the webhook (HTTP {}).", agent.pane, sent.http_status);
    Ok(0)
}
