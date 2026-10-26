//! `nudge`: tell your webhook (an AI routine or automation) when a herdr agent is done, blocked or has a question.
mod auto;
mod cli;
mod clipboard;
mod config;
mod doctor;
mod env;
mod error;
mod git;
mod herdr;
mod mcp;
mod plugin;
mod settings;
mod setup;
mod state;
mod terminal;
mod webhook;

use std::process::ExitCode;

use crate::cli::{EXIT_ERROR, EXIT_USAGE, run, utf8};
use crate::env::Env;
use crate::error::{Error, Result};

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let env = Env::from_process();
    let argv: Result<Vec<String>> = std::env::args_os().skip(1).map(utf8).collect();
    let code = match argv.and_then(|argv| run(argv, &env)) {
        Ok(code) => code,
        Err(Error::Usage(message)) => {
            eprintln!("nudge: {message}\nRun \"nudge --help\" for usage.");
            EXIT_USAGE
        }
        Err(error) => {
            eprintln!("nudge: {error}");
            EXIT_ERROR
        }
    };
    ExitCode::from(code)
}
