//! One doctor check: how bad it is, what it found, how to fix it, and how it's printed.
use crate::terminal::tty::{Mark, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug)]
pub(super) struct Check {
    pub(super) level: Level,
    title: String,
    detail: Option<String>,
    fix: Option<String>,
}

impl Check {
    pub(super) fn new(level: Level, title: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            detail: None,
            fix: None,
        }
    }

    pub(super) fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub(super) fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    pub(super) fn print(&self) {
        let mark = match self.level {
            Level::Ok => Mark::Ok,
            Level::Warn => Mark::Warn,
            Level::Fail => Mark::Fail,
        };
        let detail = self
            .detail
            .as_ref()
            .map(|detail| format!("  {}", Style::Dim.paint(detail)))
            .unwrap_or_default();
        println!("{mark} {}{detail}", self.title);
        if let Some(fix) = &self.fix {
            println!("    {} {fix}", Style::Dim.paint("→"));
        }
    }
}
