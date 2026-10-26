use std::collections::HashMap;
use std::path::Path;

use parking_lot::Mutex;

use super::Run;

/// Fake command runner keyed by the joined argv; unknown commands fail like a missing binary.
#[derive(Debug, Default)]
pub(crate) struct FakeRun {
    outputs: HashMap<String, String>,
    pub(crate) calls: Mutex<Vec<String>>,
}

impl FakeRun {
    pub(crate) fn new<const N: usize>(outputs: [(&str, &str); N]) -> Self {
        Self {
            outputs: outputs
                .into_iter()
                .map(|(cmd, out)| (cmd.to_owned(), out.to_owned()))
                .collect(),
            calls: Mutex::default(),
        }
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl Run for FakeRun {
    fn run(&self, cmd: &[&str], _cwd: &Path) -> Option<String> {
        let key = cmd.join(" ");
        let out = self.outputs.get(&key).cloned();
        self.calls.lock().push(key);
        out
    }
}
