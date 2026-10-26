//! The settings being edited, built from the config, the saved toggles and what herdr has open. Pure state: no
//! terminal, no files.
use super::choices::{Choice, agent_choices, workspace_choices};
use crate::config::{Auto, Config, default_webhook_name};
use crate::herdr::{ListedAgent, ListedWorkspace};
use crate::state::State;

/// A webhook as shown: never its URL path or secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WebhookRow {
    pub(super) name: String,
    pub(super) host: String,
    pub(super) signed: bool,
}

/// Whether there is anything to lose on quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Edits {
    None,
    Unsaved,
    /// The user pressed quit once with unsaved edits; the next quit discards them.
    DiscardOnNextQuit,
}

/// The settings being edited. Pure state: no terminal, no files.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub(super) auto_on: bool,
    pub(super) away: bool,
    pub(super) auto: Auto,
    pub(super) only: bool,
    pub(super) workspaces: Vec<Choice>,
    pub(super) agents: Vec<Choice>,
    pub(super) webhooks: Vec<WebhookRow>,
    pub(super) default_webhook: Option<String>,
    pub(super) cursor: usize,
    pub(super) status: Option<String>,
    pub(super) edits: Edits,
}

/// The configured webhooks as shown: name, host, and whether they are signed.
fn webhook_rows(config: &Config) -> Vec<WebhookRow> {
    config
        .webhook
        .iter()
        .map(|webhook| WebhookRow {
            name: webhook.name.clone(),
            host: url::Url::parse(&webhook.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_default(),
            signed: webhook.signing_secret.is_some(),
        })
        .collect()
}

impl Settings {
    pub(crate) fn new(config: &Config, saved: &State, workspaces: &[ListedWorkspace], agents: &[ListedAgent]) -> Self {
        Self {
            auto_on: saved.auto,
            away: saved.away,
            auto: config.auto,
            only: !config.scope.is_everyone(),
            workspaces: workspace_choices(&config.scope.workspaces, workspaces, agents),
            agents: agent_choices(&config.scope.agents, workspaces, agents),
            webhooks: webhook_rows(config),
            default_webhook: default_webhook_name(config).map(str::to_owned),
            cursor: 0,
            status: None,
            edits: Edits::None,
        }
    }
}
