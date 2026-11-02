//! The `nudge` command line: parse the arguments, answer help and version, and run the chosen command.
mod args;
mod list;
mod route;
mod run;
mod send;
mod switch;
mod usage;

pub(crate) use args::utf8;
pub(crate) use run::run;
pub(crate) use usage::{EXIT_ERROR, EXIT_USAGE};
