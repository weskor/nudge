//! The test ping: sent on a worker thread so the screen keeps answering, its result described for the status line.
use std::sync::mpsc::Sender;
use std::thread;

use crate::config::Config;
use crate::env::{Env, System};
use crate::error::{Blame, Error};
use crate::webhook::{Origin, current_dir, ping_webhook};

/// Test-ping `webhook` and describe the result for the status line.
fn test_webhook(config: &Config, webhook: &str, env: &Env) -> String {
    let cwd = match current_dir() {
        Ok(cwd) => cwd,
        Err(error) => return format!("✗ couldn't test \"{webhook}\": {error}"),
    };
    let origin = Origin {
        env,
        cwd: &cwd,
        run: &System,
    };
    match ping_webhook(config, Some(webhook), None, origin) {
        Ok(sent) => format!("✓ \"{webhook}\" received a test ping (HTTP {})", sent.http_status),
        Err(Error::Webhook(error)) => {
            let hint = match error.blames() {
                Some(Blame::Authorization) => "; copy the Authorization header again with nudge setup",
                Some(Blame::Url) => "; copy the Webhook URL again with nudge setup",
                None => "",
            };
            format!("✗ {}{hint}", error.message)
        }
        Err(other) => format!(
            "✗ couldn't test \"{webhook}\": {}",
            other.to_string().replace('\n', " ")
        ),
    }
}

/// Test-ping `webhook` on a worker thread that sends the status line to `sender` when done.
pub(super) fn start_test(config: &Config, env: &Env, webhook: String, sender: &Sender<String>) {
    let (config, env, sender) = (config.clone(), env.clone(), sender.clone());
    // Not joined: it writes nothing; a result that arrives after the editor closed is never read.
    thread::spawn(move || {
        sender.send(test_webhook(&config, &webhook, &env)).ok();
    });
}
