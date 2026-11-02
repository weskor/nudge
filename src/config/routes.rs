//! `[[route]]`: the webhook one pane's or one workspace's notifications go to, and which route applies to a sender.
use std::fmt;

use serde::ser::{Serialize, SerializeMap as _, Serializer};

use crate::source::Backend;

/// What a route matches, by name or id like `[scope]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selector {
    /// An agent or terminal name, or a pane id / core address.
    Pane(String),
    /// A workspace label or id, or a desktop space name or id.
    Workspace(String),
}

impl Selector {
    /// The config key: `pane` or `workspace`.
    pub(crate) const fn key(&self) -> &'static str {
        match self {
            Self::Pane(_) => "pane",
            Self::Workspace(_) => "workspace",
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Pane(name) | Self::Workspace(name) => name,
        }
    }
}

/// One `[[route]]` table: notifications from `selector` go to `webhook`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) selector: Selector,
    /// The name of a configured webhook (checked when the config loads).
    pub(crate) webhook: String,
    /// Only senders on this backend; `None` matches both.
    pub(crate) backend: Option<Backend>,
}

impl Route {
    /// Whether this route and `other` match the same senders: same selector and qualifiers.
    pub(crate) fn same_target(&self, other: &Self) -> bool {
        self.selector == other.selector && self.backend == other.backend
    }
}

/// `pane "api-refactor" → client-a`, with ` (core only)` when the route names a backend.
impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} \"{}\" → {}",
            self.selector.key(),
            self.selector.name(),
            self.webhook
        )?;
        if let Some(backend) = self.backend {
            write!(f, " ({} only)", backend.as_str())?;
        }
        Ok(())
    }
}

/// `{"pane": "api-refactor", "webhook": "client-a"}`, with `"backend"` when the route names one: names only.
impl Serialize for Route {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(self.selector.key(), self.selector.name())?;
        map.serialize_entry("webhook", &self.webhook)?;
        if let Some(backend) = self.backend {
            map.serialize_entry("backend", backend.as_str())?;
        }
        map.end()
    }
}

/// The route for a sender on `backend` known by any of `panes` (agent name, pane id) in a workspace known by any of
/// `workspaces` (label, id): the first matching pane route in file order, else the first matching workspace route;
/// `None` means the default webhook.
pub(crate) fn resolve<'a>(
    routes: &'a [Route],
    backend: Backend,
    panes: &[&str],
    workspaces: &[&str],
) -> Option<&'a Route> {
    let on_backend = || {
        routes
            .iter()
            .filter(move |route| route.backend.is_none_or(|only| only == backend))
    };
    on_backend()
        .find(|route| matches!(&route.selector, Selector::Pane(name) if panes.contains(&name.as_str())))
        .or_else(|| {
            on_backend().find(
                |route| matches!(&route.selector, Selector::Workspace(name) if workspaces.contains(&name.as_str())),
            )
        })
}

/// Whether a route on `backend` names a workspace, so a sender's workspace has to be looked up to route it.
pub(crate) fn routes_name_workspaces(routes: &[Route], backend: Backend) -> bool {
    routes.iter().any(|route| {
        matches!(route.selector, Selector::Workspace(_)) && route.backend.is_none_or(|only| only == backend)
    })
}

/// Add `route`, or point the route with the same selector and qualifiers at its webhook (keeping its place, so the
/// order routes are tried in doesn't change). Returns whether a route was replaced.
pub(crate) fn set_route(routes: &mut Vec<Route>, route: Route) -> bool {
    if let Some(existing) = routes.iter_mut().find(|existing| existing.same_target(&route)) {
        existing.webhook = route.webhook;
        return true;
    }
    routes.push(route);
    false
}

/// Remove the route for `selector` with exactly these qualifiers; returns it, or `None` when there was none.
pub(crate) fn remove_route(routes: &mut Vec<Route>, selector: &Selector, backend: Option<Backend>) -> Option<Route> {
    let index = routes
        .iter()
        .position(|route| &route.selector == selector && route.backend == backend)?;
    Some(routes.remove(index))
}

#[cfg(test)]
mod tests;
