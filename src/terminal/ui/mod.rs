//! The look shared by the full-screen editors (`nudge settings`, `nudge setup`): a centered rounded box holding a
//! list, a rule, a note (the status, or what the row under the cursor does) and the key help.
mod draw;
mod layout;
mod lines;
mod steps;
mod style;

pub(crate) use draw::{View, draw};
pub(crate) use lines::{aligned, check, note, radio, toggle};
pub(crate) use steps::{StepMark, content_width};
pub(crate) use style::{BAD, DIM, GOOD, HEADER, VALUE, WARN};
