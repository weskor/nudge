//! `nudge doctor`: check every piece and say how to fix what's broken.
mod check;
mod config;
mod local;
mod plugin;

use crate::env::Env;
use crate::error::Result;
use crate::herdr::nudge_root;
use crate::state::state_dir;
use crate::terminal::tty::{Mark, Style, wait_for_key};
use check::Level;
use config::check_config;
use local::{check_clipboard, check_command};
use plugin::check_plugin;

/// Run every check and print them with a summary; exits 1 when something failed. `pane`: wait for a key before
/// returning, so a herdr pane stays open.
pub(crate) fn doctor(ping: bool, pane: bool, env: &Env) -> Result<u8> {
    println!("{}", Style::Bold.paint("nudge doctor"));
    println!(
        "{}",
        Style::Dim.paint(format!(
            "nudge at {}, state in {}",
            nudge_root(env).display(),
            state_dir(env).display()
        ))
    );
    println!();
    let mut checks = check_config(ping, env)?;
    checks.extend(check_plugin(env));
    checks.extend([check_command(env), check_clipboard(env)]);
    for check in &checks {
        check.print();
    }
    let failures = checks.iter().filter(|check| check.level == Level::Fail).count();
    println!();
    if failures == 0 {
        println!("{} All good.", Mark::Ok);
    } else {
        println!(
            "{} {failures} problem{} found.",
            Mark::Fail,
            if failures == 1 { "" } else { "s" }
        );
    }
    if pane {
        wait_for_key();
    }
    Ok(u8::from(failures != 0))
}
