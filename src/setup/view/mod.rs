//! The wizard on screen: its lines for each step, and the terminal loop that reads keys, runs the clipboard reads
//! and the test ping on worker threads (so the screen never freezes) and saves while the screen is still up.
mod content;
mod draw;
mod event_loop;
mod field;
mod keys;
mod options;
mod save;

pub(super) use self::event_loop::run;
