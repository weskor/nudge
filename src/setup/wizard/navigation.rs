//! Where the wizard is and moving around it: the step, the focus, the step-3 cursor, going on, going back and
//! leaving.
use super::field::Field;
use super::input::Outcome;
use super::step::{At, Step};
use super::{Edits, Wizard};

impl Wizard {
    pub(in crate::setup) const fn step(&self) -> Step {
        self.at.step()
    }

    /// The focused text field, on steps 1 and 2.
    pub(in crate::setup) const fn focus(&self) -> Option<Field> {
        match self.at {
            At::Connect { focus } | At::Name { focus } => Some(focus),
            At::Options { .. } | At::Save(_) => None,
        }
    }

    /// The checkbox under the cursor, while step 3 shows checkboxes.
    pub(in crate::setup) const fn editing(&self) -> Option<usize> {
        match self.at {
            At::Options { editing } => editing,
            At::Connect { .. } | At::Name { .. } | At::Save(_) => None,
        }
    }

    /// Where each step stands, for the step list.
    pub(in crate::setup) fn done(&self, step: Step) -> bool {
        step != self.step() && step.number() < self.reached.number()
    }

    /// Move the focus to `field`, on steps 1 and 2.
    pub(super) const fn set_focus(&mut self, field: Field) {
        match &mut self.at {
            At::Connect { focus } | At::Name { focus } => *focus = field,
            At::Options { .. } | At::Save(_) => {}
        }
    }

    /// Move the focus within the step's fields (wrapping), checking the field left.
    pub(super) fn move_focus(&mut self, forward: bool) {
        let fields = self.step().fields();
        let Some(current) = self.focus() else { return };
        let Some(index) = fields.iter().position(|field| *field == current) else {
            return;
        };
        self.check_field(current);
        let next = if forward {
            (index + 1) % fields.len()
        } else {
            (index + fields.len() - 1) % fields.len()
        };
        if let Some(field) = fields.get(next) {
            self.set_focus(*field);
        }
    }

    /// Check the step and go on to the next one; a field that needs fixing gets the focus instead.
    pub(super) fn advance(&mut self) -> Outcome {
        for &field in self.step().fields() {
            self.check_field(field);
            let entry = self.entry_mut(field);
            if entry.value.is_empty() {
                entry.problem = field.missing();
            }
            if entry.problem.is_some() {
                self.set_focus(field);
                return Outcome::Continue;
            }
        }
        self.status = None;
        match self.step() {
            Step::Connect => self.go(Step::Name),
            Step::Name => self.go(Step::Options),
            Step::Options => return self.enter_save(),
            Step::Save => {}
        }
        Outcome::Continue
    }

    /// Show `step` from its start. Answers are untouched.
    pub(super) fn go(&mut self, step: Step) {
        if step.number() > self.reached.number() {
            self.reached = step;
        }
        self.at = At::start(step);
    }

    pub(super) fn back(&mut self) {
        if let Some(previous) = self.step().previous() {
            self.status = None;
            self.go(previous);
        }
    }

    /// Leave, asking once more if anything was entered.
    pub(super) fn leave(&mut self, key: &str) -> Outcome {
        if self.edits == Edits::Unsaved {
            self.edits = Edits::LeaveOnNextPress;
            self.status = Some(format!("Press {key} again to leave without saving."));
            return Outcome::Continue;
        }
        Outcome::Quit
    }
}
