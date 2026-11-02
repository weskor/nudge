use super::{Backend, Route, Selector, remove_route, resolve, routes_name_workspaces, set_route};

fn route(selector: Selector, webhook: &str, backend: Option<Backend>) -> Route {
    Route {
        selector,
        webhook: webhook.to_owned(),
        backend,
    }
}

fn pane(name: &str, webhook: &str) -> Route {
    route(Selector::Pane(name.to_owned()), webhook, None)
}

fn workspace(name: &str, webhook: &str) -> Route {
    route(Selector::Workspace(name.to_owned()), webhook, None)
}

/// The webhook a TUI sender known as `panes`, in a workspace known as `workspaces`, is routed to; `default` if none.
fn routed<'a>(routes: &'a [Route], panes: &[&str], workspaces: &[&str]) -> &'a str {
    resolve(routes, Backend::Tui, panes, workspaces).map_or("default", |route| route.webhook.as_str())
}

#[test]
fn a_pane_route_beats_a_workspace_route_which_beats_the_default() {
    // The workspace route comes first in the file: pane routes still win.
    let routes = [workspace("client-a", "ws-hook"), pane("api-refactor", "pane-hook")];
    for (panes, workspaces, expected) in [
        (&["api-refactor", "w1:p1"][..], &["w1", "client-a"][..], "pane-hook"),
        (&["reviewer", "w1:p2"], &["w1", "client-a"], "ws-hook"),
        (&["reviewer", "w1:p2"], &["w2", "other"], "default"),
        (&["api-refactor"], &[], "pane-hook"),
        (&[], &[], "default"),
    ] {
        assert_eq!(
            routed(&routes, panes, workspaces),
            expected,
            "{panes:?} in {workspaces:?}"
        );
    }
}

#[test]
fn routes_match_by_name_or_id_and_the_first_in_file_order_wins() {
    let routes = [
        pane("w1:p3", "by-pane-id"),
        pane("api-refactor", "by-name"),
        workspace("w9", "by-workspace-id"),
        workspace("client-a", "by-label"),
    ];
    assert_eq!(routed(&routes, &["api-refactor", "w1:p3"], &[]), "by-pane-id");
    assert_eq!(routed(&routes, &["api-refactor", "w2:p1"], &[]), "by-name");
    assert_eq!(routed(&routes, &["x"], &["w9", "client-a"]), "by-workspace-id");
    assert_eq!(routed(&routes, &["x"], &["w2", "client-a"]), "by-label");
}

#[test]
fn the_backend_qualifier_narrows_a_route() {
    let routes = [
        route(Selector::Pane("api-refactor".into()), "core-hook", Some(Backend::Core)),
        route(Selector::Pane("api-refactor".into()), "tui-hook", Some(Backend::Tui)),
        workspace("client-a", "any-hook"),
    ];
    assert_eq!(resolve(&routes, Backend::Core, &["api-refactor"], &[]), routes.first());
    assert_eq!(resolve(&routes, Backend::Tui, &["api-refactor"], &[]), routes.get(1));
    assert_eq!(resolve(&routes, Backend::Core, &["x"], &["client-a"]), routes.get(2));
    assert!(routes_name_workspaces(&routes, Backend::Tui));
    let core_only = [route(Selector::Workspace("client-a".into()), "h", Some(Backend::Core))];
    assert!(
        !routes_name_workspaces(&core_only, Backend::Tui),
        "no TUI lookup for a core-only workspace route"
    );
    assert!(!routes_name_workspaces(&[pane("a", "h")], Backend::Tui));
}

#[test]
fn set_route_replaces_the_same_selector_in_place_and_remove_needs_the_same_qualifiers() {
    let mut routes = vec![pane("a", "one"), workspace("ws", "two")];
    assert!(!set_route(
        &mut routes,
        route(Selector::Pane("a".into()), "core", Some(Backend::Core))
    ));
    assert!(set_route(&mut routes, pane("a", "three")));
    assert_eq!(
        routes,
        [
            pane("a", "three"),
            workspace("ws", "two"),
            route(Selector::Pane("a".into()), "core", Some(Backend::Core))
        ]
    );
    assert_eq!(
        remove_route(&mut routes, &Selector::Pane("a".into()), Some(Backend::Tui)),
        None
    );
    assert_eq!(
        remove_route(&mut routes, &Selector::Pane("a".into()), None),
        Some(pane("a", "three"))
    );
    assert_eq!(routes.len(), 2);
}

#[test]
fn a_route_reads_as_selector_name_and_webhook() {
    assert_eq!(
        pane("api-refactor", "client-a").to_string(),
        r#"pane "api-refactor" → client-a"#
    );
    assert_eq!(
        route(Selector::Workspace("ops".into()), "pager", Some(Backend::Core)).to_string(),
        r#"workspace "ops" → pager (core only)"#
    );
}
