//! The wizard's four steps, and the state only one step has: the focus, the step-3 cursor, where step 4 stands.
use super::field::Field;
use crate::error::Blame;

/// A step of the wizard, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::setup) enum Step {
    Connect,
    Name,
    Options,
    Save,
}

impl Step {
    pub(in crate::setup) const ALL: [Self; 4] = [Self::Connect, Self::Name, Self::Options, Self::Save];

    pub(in crate::setup) const fn title(self) -> &'static str {
        match self {
            Self::Connect => "Connect webhook",
            Self::Name => "Name",
            Self::Options => "Options",
            Self::Save => "Test and save",
        }
    }

    pub(in crate::setup) const fn number(self) -> usize {
        match self {
            Self::Connect => 1,
            Self::Name => 2,
            Self::Options => 3,
            Self::Save => 4,
        }
    }

    /// The text fields on this step, in order.
    pub(super) const fn fields(self) -> &'static [Field] {
        match self {
            Self::Connect => &[Field::Url, Field::Authorization],
            Self::Name => &[Field::Name, Field::Description],
            Self::Options | Self::Save => &[],
        }
    }

    pub(super) const fn previous(self) -> Option<Self> {
        match self {
            Self::Connect => None,
            Self::Name => Some(Self::Connect),
            Self::Options => Some(Self::Name),
            Self::Save => Some(Self::Options),
        }
    }
}

/// An option offered on step 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::setup) enum Choice {
    AutoNotify,
    Command,
    Default,
}

/// Where step 4 stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::setup) enum SaveState {
    /// The test ping is running; `frame` turns the spinner.
    Testing {
        frame: usize,
    },
    /// Saving, with the test ping's HTTP status if one passed.
    Saving {
        tested: Option<u16>,
    },
    Failed {
        blame: Option<Blame>,
        choice: FailChoice,
    },
    /// Saving failed; Enter tries again with the same answers.
    NotSaved {
        tested: Option<u16>,
    },
}

/// What to do after a failed test ping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::setup) enum FailChoice {
    Retry,
    Back,
    SaveAnyway,
}

impl FailChoice {
    pub(in crate::setup) const ALL: [Self; 3] = [Self::Retry, Self::Back, Self::SaveAnyway];
}

/// Where the wizard is, with the state only that step has. Answers live elsewhere ([`super::Answers`]), so moving
/// between steps can't lose one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum At {
    Connect {
        focus: Field,
    },
    Name {
        focus: Field,
    },
    /// `editing`: the checkbox under the cursor, while `e` has the checkboxes open.
    Options {
        editing: Option<usize>,
    },
    Save(SaveState),
}

impl At {
    pub(super) const fn step(&self) -> Step {
        match self {
            Self::Connect { .. } => Step::Connect,
            Self::Name { .. } => Step::Name,
            Self::Options { .. } => Step::Options,
            Self::Save(_) => Step::Save,
        }
    }

    /// Where a step starts.
    pub(super) const fn start(step: Step) -> Self {
        match step {
            Step::Connect => Self::Connect { focus: Field::Url },
            Step::Name => Self::Name { focus: Field::Name },
            Step::Options => Self::Options { editing: None },
            Step::Save => Self::Save(SaveState::Saving { tested: None }),
        }
    }
}
