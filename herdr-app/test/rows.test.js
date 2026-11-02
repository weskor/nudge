import { describe, expect, test } from "bun:test";
import * as R from "../mods/nudge/rows.js";
import laptop from "./fixtures/status-laptop.json";
import box from "./fixtures/status-box.json";
import app from "./fixtures/app.json";

const clone = (v) => structuredClone(v);
const hooks = (...names) => names.map((name) => ({ name, host: `${name}.example.com`, signed: false }));

describe("route resolution (mirror of nudge's)", () => {
  const status = {
    default_webhook: "main",
    webhooks: hooks("main", "client-a", "client-b", "tui-only"),
    routes: [
      { workspace: "client-a", webhook: "client-a" },
      { pane: "api-refactor", webhook: "client-b" },
      { pane: "docs-site", webhook: "tui-only", backend: "tui" },
      { pane: "t9", webhook: "client-a" },
      { workspace: "s3", webhook: "client-b", backend: "core" },
    ],
  };

  test("a pane route beats a workspace route, which beats the default", () => {
    expect(R.resolveRoute(status, { pane: ["api-refactor", "t3"], workspace: ["client-a", "s1"] })).toEqual({ webhook: "client-b", via: "pane" });
    expect(R.resolveRoute(status, { pane: ["other", "t4"], workspace: ["client-a", "s1"] })).toEqual({ webhook: "client-a", via: "workspace" });
    expect(R.resolveRoute(status, { pane: ["other", "t4"], workspace: ["nudge", "s2"] })).toEqual({ webhook: "main", via: "default" });
  });

  test("the backend qualifier narrows a route: a tui route doesn't match a core terminal", () => {
    expect(R.resolveRoute(status, { pane: ["docs-site", "t5"], workspace: ["nudge", "s2"] }).via).toBe("default");
    expect(R.resolveRoute(status, { pane: ["docs-site", "w1:p3"], backend: "tui" })).toEqual({ webhook: "tui-only", via: "pane" });
  });

  test("routes match by id as well as by name", () => {
    expect(R.resolveRoute(status, { pane: [null, "t9"] })).toEqual({ webhook: "client-a", via: "pane" });
    expect(R.resolveRoute(status, { workspace: ["scratch", "s3"] })).toEqual({ webhook: "client-b", via: "workspace" });
  });

  test("without a default, the only webhook is used; with several and no default, nowhere", () => {
    expect(R.resolveRoute({ webhooks: hooks("solo"), routes: [] }, { pane: ["x"] })).toEqual({ webhook: "solo", via: "default" });
    expect(R.resolveRoute({ webhooks: hooks("a", "b"), routes: [] }, { pane: ["x"] })).toEqual({ webhook: null, via: null });
  });
});

describe("agent and workspace rows", () => {
  test("each local agent shows where it goes and why", () => {
    const rows = R.agentRows(app.agents, app.terminals, laptop, null);
    expect(rows.map((r) => [r.label, r.status, R.routeLabel(r)])).toEqual([
      ["api-refactor", "blocked", "→ client-a (pane)"],
      ["docs-site", "working", "→ main (default)"],
      ["pi", "idle", "→ main (default)"],
    ]);
  });

  test("an agent nudge doesn't list yet is resolved by the mirror", () => {
    const status = clone(laptop);
    status.core.terminals = [];
    const row = R.agentRows(app.agents, app.terminals, status, null).find((r) => r.local === "t3");
    expect(R.routeLabel(row)).toBe("→ client-a (pane)");
  });

  test("a connected machine shows only its own agents, by their address there", () => {
    const rows = R.agentRows(app.agents, app.terminals, box, "box");
    expect(rows.map((r) => [r.label, r.local, r.machine, R.routeLabel(r)])).toEqual([["trainer", "t7", "box", "→ ops (default)"]]);
  });

  test("workspaces show their route, including ones nudge doesn't list", () => {
    const rows = R.spaceRows(app.spaces, laptop, null);
    expect(rows.map((r) => [r.label, R.routeLabel(r)])).toEqual([
      ["client-a", "→ client-a (workspace)"],
      ["nudge", "→ main (default)"],
      ["scratch", "→ main (default)"],
    ]);
    expect(R.spaceRows(app.spaces, box, "box").map((r) => [r.label, r.local])).toEqual([["gpu", "s1"]]);
  });

  test("a focused plain terminal still gets a row to route or send", () => {
    const row = R.terminalRow("t11", app.agents, app.terminals, laptop);
    expect([row.label, row.local, R.routeLabel(row)]).toEqual(["t11", "t11", "→ client-a (workspace)"]);
  });

  test("no webhook at all reads as nowhere", () => {
    expect(R.routeLabel({ webhook: null, via: null })).toBe("→ nowhere (no webhook)");
  });
});

describe("header, watcher line and recent decisions", () => {
  test("nudge missing there: only 'nudge not found'", () => {
    expect(R.header(R.readStatus(null), "box")).toEqual({ kind: "missing", text: "nudge not found" });
  });

  test("a nudge without status --json says to update it", () => {
    const state = R.readStatus({ code: 2, stdout: "", stderr: "nudge: unknown command status\n" });
    expect(R.header(state, null).kind).toBe("error");
  });

  test("off reads 'Off: nothing is sent from' this computer or the machine", () => {
    const off = { kind: "ok", status: { ...laptop, on: false } };
    expect(R.header(off, null)).toMatchObject({ muted: true, text: "Off: nothing is sent from this computer" });
    expect(R.header(off, "box").text).toBe("Off: nothing is sent from box");
    expect(R.header({ kind: "ok", status: laptop }, null).kind).toBe("on");
  });

  test("watching: running by the service, started here, or not watching with Start", () => {
    expect(R.watcherLine(box)).toEqual({ running: true, text: "● 1 agent · service running", start: false });
    const here = clone(box);
    here.core.watcher.started_by = "terminal";
    here.core.terminals.push({ ...here.core.terminals[0], address: "t8" });
    expect(R.watcherLine(here).text).toBe("● 2 agents · started here");
    expect(R.watcherLine(laptop)).toEqual({ running: false, text: "○ Not watching", start: true });
    expect(R.watcherLine({ ...laptop, on: false }).start).toBe(false);
  });

  test("recent decisions name the agent and say what nudge did", () => {
    expect(R.recentRows(laptop).map((d) => [d.text, d.outcome])).toEqual([
      ["api-refactor blocked", "sent"],
      ["docs-site done", "you were watching"],
    ]);
  });

  test("the picker's machine notes", () => {
    expect(R.machineNote(R.readStatus(null))).toBe("○ nudge not found");
    expect(R.machineNote({ kind: "ok", status: box })).toBe("● on · watching");
    expect(R.machineNote({ kind: "ok", status: { ...laptop, on: false } })).toBe("○ off");
  });
});

describe("machines", () => {
  test("addresses split into machine and address there", () => {
    expect(R.splitAddress("box/t7")).toEqual({ machine: "box", address: "t7" });
    expect(R.splitAddress("t7")).toEqual({ machine: null, address: "t7" });
  });

  test("the picker lists this computer first, then connected machines", () => {
    expect(R.machineList(app.machines)).toEqual([null, "box", "gpu-vm"]);
  });

  test("the watcher starts in a core terminal there", () => {
    expect(R.watchStartArgs(null)).toEqual(["run", "--bg", "--name", "nudge-watch", "--label", "nudge=watch", "--", "nudge", "core", "watch"]);
    const args = R.watchStartArgs("box");
    expect(args.slice(args.indexOf("--on"), args.indexOf("--on") + 2)).toEqual(["--on", "box"]);
    expect(args.indexOf("--on")).toBeLessThan(args.indexOf("--"));
  });

  test("a terminal tab opens on the picked machine", () => {
    expect(R.tabCommand(app.spaces, "box", ["nudge", "setup"], "nudge")).toEqual({ name: "tab.new", args: { space: "box/s1", argv: ["nudge", "setup"], name: "nudge" } });
    expect(R.tabCommand(app.spaces, "gpu-vm", ["nudge", "setup"], "nudge")).toEqual({ name: "space.new", args: { argv: ["nudge", "setup"], name: "nudge", machine: "gpu-vm" } });
  });
});

describe("the context plan", () => {
  const base = { machines: [null, "box"], windowActive: true, spaces: app.spaces };
  const parse = (sends) => Object.fromEntries(sends.map((s) => [s.machine ?? "here", JSON.parse(s.json)]));
  const start = R.contextPlan({ ...base, sent: {}, focused: app.focused, now: 1000 });

  test("on load, the focus owner gets its terminal and every machine its spaces", () => {
    const sent = parse(start.sends);
    expect(sent.here).toEqual({ terminal: "t3", window_active: true, spaces: [{ id: "s1", name: "client-a" }, { id: "s2", name: "nudge" }, { id: "s3", name: "scratch" }] });
    expect(sent.box).toEqual({ terminal: null, window_active: true, spaces: [{ id: "s1", name: "gpu" }] });
    expect(sent.here.at).toBeUndefined();
  });

  test("nothing changed: nothing is sent", () => {
    expect(R.contextPlan({ ...base, sent: start.sent, focused: app.focused, now: 2000 }).sends).toEqual([]);
  });

  test("focus moving to box/t7 sends t7 to box and null to this computer", () => {
    const moved = R.contextPlan({ ...base, sent: start.sent, focused: app.focused_box, now: 2000 });
    const sent = parse(moved.sends);
    expect(sent.box.terminal).toBe("t7");
    expect(sent.here.terminal).toBeNull();
    expect(moved.sends).toHaveLength(2);
  });

  test("the 30 s heartbeat goes only to the machine with focus", () => {
    const moved = R.contextPlan({ ...base, sent: start.sent, focused: app.focused_box, now: 2000 });
    expect(R.contextPlan({ ...base, sent: moved.sent, focused: app.focused_box, now: 31000 }).sends).toEqual([]);
    const beat = R.contextPlan({ ...base, sent: moved.sent, focused: app.focused_box, now: 32000 });
    expect(beat.sends.map((s) => s.machine)).toEqual(["box"]);
    expect(JSON.parse(beat.sends[0].json).terminal).toBe("t7");
  });

  test("a machine whose spaces change gets the new list, and no one else", () => {
    const renamed = app.spaces.map((s) => (s.id === "s2" ? { ...s, name: "nudge-cli" } : s));
    const plan = R.contextPlan({ ...base, spaces: renamed, sent: start.sent, focused: app.focused, now: 2000 });
    expect(plan.sends.map((s) => s.machine)).toEqual([null]);
    expect(JSON.parse(plan.sends[0].json).spaces[1]).toEqual({ id: "s2", name: "nudge-cli" });
  });

  test("closing tells the machine with focus that nothing is watched", () => {
    const clear = R.clearPlan(start.sent);
    expect(clear.map((s) => [s.machine, JSON.parse(s.json).terminal])).toEqual([[null, null]]);
  });
});
