//! A wizard's step list, drawn left of its content: where each step stands, the column the list takes, and the room
//! it leaves the content.
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::layout::FRAME_WIDTH;
use super::style::{BORDER, DIM, GOOD, HEADER};

/// Where a step stands, for the step list beside a wizard's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepMark {
    Done,
    Current,
    Pending,
}

/// Columns the step list takes, its rule included; the content starts two columns after it.
const STEP_COLUMN: u16 = 22;
/// Below this inner width the step list is left out (the title still says which step this is).
const STEPS_MIN_WIDTH: u16 = 58;

/// Columns left for a wizard's content in a box `width` wide, beside the step list when it fits.
pub(crate) const fn content_width(width: u16) -> u16 {
    let inner = width.saturating_sub(FRAME_WIDTH);
    if inner >= STEPS_MIN_WIDTH {
        inner.saturating_sub(STEP_COLUMN + 2)
    } else {
        inner
    }
}

/// The step list: done steps ticked, the current one marked, the rest dim.
fn step_lines(steps: &[(&str, StepMark)]) -> Vec<Line<'static>> {
    steps
        .iter()
        .map(|(title, mark)| match mark {
            StepMark::Done => Line::from(vec![Span::styled("✓ ", GOOD), Span::raw((*title).to_owned())]),
            StepMark::Current => Line::styled(format!("▸ {title}"), HEADER),
            StepMark::Pending => Line::styled(format!("○ {title}"), DIM),
        })
        .collect()
}

/// Draw the step list at the left of `body` when it fits, and return the area left for the content.
pub(super) fn render_steps(frame: &mut Frame<'_>, body: Rect, steps: &[(&str, StepMark)]) -> Rect {
    if body.width < STEPS_MIN_WIDTH {
        return body;
    }
    let [list_area, _, content] = Layout::horizontal([
        Constraint::Length(STEP_COLUMN),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(body);
    let list = Paragraph::new(step_lines(steps)).block(Block::new().borders(Borders::RIGHT).border_style(BORDER));
    frame.render_widget(list, list_area);
    content
}
