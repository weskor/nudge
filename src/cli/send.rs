//! `nudge notify` and `nudge test`: send to a webhook from the command line and report what was sent.
use std::io::Read as _;

use super::args::{Args, no_args};
use crate::config::{default_config_path, load_config};
use crate::env::{Env, System};
use crate::error::{Error, Result};
use crate::webhook::{NotifyStatus, Origin, Sent, current_dir, notify_webhook, ping_webhook};

fn report_sent(verb: &str, sent: &Sent) {
    let notification = &sent.notification;
    let status = serde_json::to_value(notification.status).ok();
    let status = status.as_ref().and_then(serde_json::Value::as_str).unwrap_or_default();
    let agent = notification.from_agent.as_deref().unwrap_or("<none>");
    println!(
        "{verb} webhook \"{}\" (status {status}, HTTP {}) as agent {agent}.",
        sent.webhook, sent.http_status
    );
    if notification.from_agent.is_none() {
        eprintln!(
            "nudge: note: not sent from a herdr agent pane, so the webhook can't reply directly. Pass --agent NAME or set NUDGE_AGENT."
        );
    }
}

/// `nudge notify <message...|->`.
pub(super) fn notify(rest: &[String], args: &Args, env: &Env) -> Result<u8> {
    if rest.is_empty() {
        return Err(Error::Usage(
            "usage: nudge notify [--webhook NAME] [--status STATUS] <message...|->".into(),
        ));
    }
    let status_name = args.status.as_deref().unwrap_or("done");
    let Some(status) = NotifyStatus::parse(status_name) else {
        let all: Vec<&str> = NotifyStatus::ALL.into_iter().map(NotifyStatus::as_str).collect();
        return Err(Error::Usage(format!("--status must be one of: {}", all.join(", "))));
    };
    let message = if rest == ["-"] {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        input
    } else {
        rest.join(" ")
    };
    let config = load_config(&default_config_path(env))?;
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    report_sent(
        "Notified",
        &notify_webhook(
            &config,
            args.webhook.as_deref(),
            status,
            &message,
            args.agent.clone(),
            origin,
        )?,
    );
    Ok(0)
}

/// `nudge test`.
pub(super) fn ping(rest: &[String], args: Args, env: &Env) -> Result<u8> {
    no_args(rest, "test")?;
    let config = load_config(&default_config_path(env))?;
    let cwd = current_dir()?;
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    report_sent(
        "Pinged",
        &ping_webhook(&config, args.webhook.as_deref(), args.agent, origin)?,
    );
    Ok(0)
}
