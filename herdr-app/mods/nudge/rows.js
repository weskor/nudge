// Pure helpers for the Nudge panel: no `herdr` import, so `bun test` runs them as they are.
// Everything here turns the app's live data and `nudge status --json` into what the panel shows.

/** Where `nudge status` and other commands run: null is this computer, else a `machines()` name. */
export const THIS_COMPUTER = null;

const HEARTBEAT_MS = 30000;
const GLYPH = { blocked: "▲", working: "●", done: "✓", idle: "○" };

/**
 * Splits an app address or id into its machine and the address on that machine.
 * `box/t7` → {machine: "box", address: "t7"}; `t7` → {machine: null, address: "t7"}.
 * @param {string} id
 */
export function splitAddress(id) {
  const at = id.indexOf("/");
  return at < 0 ? { machine: null, address: id } : { machine: id.slice(0, at), address: id.slice(at + 1) };
}

/** "This computer" or the machine's name, for "from box" and the picker. */
export function machineLabel(machine) {
  return machine ?? "This computer";
}

/** The machines the picker lists: this computer first, then every other machine by name. */
export function machineList(machines) {
  const others = machines.filter((m) => m.name !== null).map((m) => m.name).sort();
  return [THIS_COMPUTER, ...others];
}

/**
 * Turns what `exec("nudge", ["status", "--json"])` gave into the panel's state.
 * @param {{code: number, stdout: string, stderr: string} | null} result null when exec rejected
 * @returns {{kind: "missing"} | {kind: "error", text: string} | {kind: "ok", status: object}}
 */
export function readStatus(result) {
  if (!result) return { kind: "missing" };
  if (result.code !== 0) return { kind: "error", text: firstLine(result.stderr) || `nudge status failed (exit ${result.code})` };
  try {
    return { kind: "ok", status: JSON.parse(result.stdout) };
  } catch {
    return { kind: "error", text: "This nudge has no `nudge status --json`; update nudge there" };
  }
}

function firstLine(text) {
  return (text ?? "").trim().split("\n")[0].replace(/https?:\/\/\S+/g, "…");
}

/** The header: what the panel says about the picked machine's switch. */
export function header(state, machine) {
  if (state.kind === "missing") return { kind: "missing", text: "nudge not found" };
  if (state.kind === "error") return { kind: "error", text: state.text };
  if (state.status.on) return { kind: "on", text: "On" };
  const where = machine === THIS_COMPUTER ? "this computer" : machine;
  return { kind: "off", muted: true, text: `Off: nothing is sent from ${where}` };
}

function routeMatches(route, key, values, backend) {
  if (route[key] === undefined || !values.includes(route[key])) return false;
  return !route.backend || route.backend === backend;
}

/**
 * The JS mirror of nudge's route resolution, for terminals and spaces `nudge status` doesn't list yet.
 * Pane routes (file order), then workspace routes, then default_webhook, else the only webhook.
 * @param {object} status `nudge status --json`
 * @param {{pane?: string[], workspace?: string[], backend?: "core" | "tui"}} source names and ids to match
 * @returns {{webhook: string | null, via: "pane" | "workspace" | "default" | null}}
 */
export function resolveRoute(status, source) {
  const backend = source.backend ?? "core";
  const routes = status.routes ?? [];
  for (const key of ["pane", "workspace"]) {
    const values = (source[key] ?? []).filter((v) => v);
    const hit = routes.find((r) => routeMatches(r, key, values, backend));
    if (hit) return { webhook: hit.webhook, via: key };
  }
  if (status.default_webhook) return { webhook: status.default_webhook, via: "default" };
  const hooks = status.webhooks ?? [];
  if (hooks.length === 1) return { webhook: hooks[0].name, via: "default" };
  return { webhook: null, via: null };
}

/** The first route that names this row itself (its own pane or workspace route), or null. */
export function ownRoute(status, key, values) {
  return (status.routes ?? []).find((r) => routeMatches(r, key, values.filter((v) => v), "core")) ?? null;
}

/** `→ client-a (pane)`, `→ main (default)`, or `→ nowhere (no webhook)`. */
export function routeLabel({ webhook, via }) {
  if (!webhook) return "→ nowhere (no webhook)";
  return `→ ${webhook} (${via})`;
}

/** The status glyph an agent row starts with. */
export function statusGlyph(status) {
  return GLYPH[status] ?? "·";
}

/**
 * The picked machine's agents, as rows: name, status, where they'd be sent and why.
 * @param {object[]} agents `agents()`
 * @param {object[]} terminals `terminals()` (for names)
 * @param {object} status that machine's `nudge status --json`
 * @param {string | null} machine the picked machine
 */
export function agentRows(agents, terminals, status, machine) {
  const listed = status.core?.terminals ?? [];
  return agents.filter((a) => splitAddress(a.address).machine === machine).map((a) => {
    const local = splitAddress(a.address).address;
    const known = listed.find((t) => t.address === local);
    const name = terminals.find((t) => t.address === a.address)?.name || known?.name || null;
    const space = splitAddress(a.space).address;
    const route = known?.webhook !== undefined && known.via
      ? { webhook: known.webhook, via: known.via }
      : resolveRoute(status, { pane: [name, local, a.address], workspace: [a.space_name, space] });
    return {
      kind: "agent", address: a.address, local, machine, name, label: name || a.title || local,
      status: a.status, glyph: statusGlyph(a.status), space, space_name: a.space_name, ...route,
    };
  });
}

/** The picked machine's spaces, as rows with where their terminals go and why. */
export function spaceRows(spaces, status, machine) {
  const listed = status.core?.spaces ?? [];
  return spaces.filter((s) => s.machine === machine).map((s) => {
    const id = splitAddress(s.id).address;
    const known = listed.find((k) => k.id === id);
    const route = known?.via ? { webhook: known.webhook, via: known.via } : resolveRoute(status, { workspace: [s.name, id] });
    return { kind: "space", id: s.id, local: id, machine, name: s.name || null, label: s.name || id, ...route };
  });
}

/**
 * The row for one terminal (the focused pane), agent or not, on its own machine's status.
 * @returns {object | null} null when no such terminal is known
 */
export function terminalRow(address, agents, terminals, status) {
  const { machine } = splitAddress(address);
  const agent = agents.find((a) => a.address === address);
  if (agent) return agentRows([agent], terminals, status, machine)[0];
  const term = terminals.find((t) => t.address === address);
  if (!term) return null;
  const space = term.labels?.["herdr.space"] ?? "";
  const spaceName = (status.core?.spaces ?? []).find((s) => s.id === space)?.name ?? "";
  const synthetic = { address, space, space_name: spaceName, status: term.status, title: term.name || address };
  return agentRows([synthetic], terminals, status, machine)[0];
}

/**
 * The Watching line. `● 2 agents · service running` / `· started here`, or `○ Not watching` with [Start].
 * @returns {{running: boolean, text: string, start: boolean}}
 */
export function watcherLine(status) {
  const watcher = status.core?.watcher;
  if (!watcher?.running) return { running: false, text: "○ Not watching", start: Boolean(status.on) };
  const n = (status.core?.terminals ?? []).length;
  const how = watcher.started_by === "service" ? "service running" : "started here";
  return { running: true, text: `● ${n} ${n === 1 ? "agent" : "agents"} · ${how}`, start: false };
}

/** A decision's outcome, in the panel's words. */
export function outcomeText(outcome) {
  if (/^notified/.test(outcome)) return "sent";
  if (/looking at this pane/.test(outcome)) return "you were watching";
  const why = /\((.*)\)\s*$/.exec(outcome);
  return why ? why[1] : outcome;
}

/** Recent decisions, newest first: who, what, and what nudge did. `at` stays epoch ms. */
export function recentRows(status, limit = 5) {
  const names = new Map((status.core?.terminals ?? []).map((t) => [t.address, t.name]));
  return [...(status.core?.decisions ?? [])].sort((a, b) => b.at - a.at).slice(0, limit)
    .map((d) => ({ at: d.at, text: `${names.get(d.terminal) || d.terminal} ${d.status}`, outcome: outcomeText(d.outcome) }));
}

/** The picker's note for a machine: `● on · watching`, `○ off`, `○ nudge not found`. */
export function machineNote(state) {
  if (!state) return "…";
  if (state.kind === "missing") return "○ nudge not found";
  if (state.kind === "error") return "○ nudge error";
  const s = state.status;
  return `${s.on ? "● on" : "○ off"}${s.core?.watcher?.running ? " · watching" : ""}`;
}

/** The `hc` argv that starts the watcher in a core terminal there. */
export function watchStartArgs(machine) {
  const on = machine === THIS_COMPUTER ? [] : ["--on", machine];
  return ["run", "--bg", "--name", "nudge-watch", "--label", "nudge=watch", ...on, "--", "nudge", "core", "watch"];
}

/**
 * The app command that opens a terminal tab running argv on that machine: a tab in its first space,
 * else a new space there.
 */
export function tabCommand(spaces, machine, argv, name) {
  const space = spaces.find((s) => s.machine === machine);
  if (space) return { name: "tab.new", args: { space: space.id, argv, name } };
  return { name: "space.new", args: { argv, name, ...(machine === THIS_COMPUTER ? {} : { machine }) } };
}

function machineKey(machine) {
  return machine ?? "";
}

function spacesOf(spaces, machine) {
  return spaces.filter((s) => s.machine === machine).map((s) => ({ id: splitAddress(s.id).address, name: s.name }));
}

/**
 * What to send with `nudge core context`, and to whom.
 * @param {object} input
 * @param {Record<string, {terminal: string|null, window_active: boolean, spaces: object[], at: number}>} input.sent
 *   the last context sent per machine ("" is this computer)
 * @param {(string|null)[]} input.machines the machines to keep informed (null is this computer)
 * @param {{terminal: string|null} | null} input.focused `focused()`
 * @param {boolean} input.windowActive `window().active`
 * @param {object[]} input.spaces `spaces()`
 * @param {number} input.now epoch ms
 * @returns {{sends: {machine: string|null, json: string}[], sent: object}}
 */
export function contextPlan({ sent, machines, focused, windowActive, spaces, now }) {
  const focus = focused?.terminal ? splitAddress(focused.terminal) : null;
  const all = new Set(machines.map(machineKey));
  if (focus) all.add(machineKey(focus.machine));
  for (const [k, c] of Object.entries(sent)) if (c.terminal !== null) all.add(k);
  const next = { ...sent };
  const sends = [];
  for (const key of all) {
    const machine = key === "" ? null : key;
    const owner = focus !== null && machineKey(focus.machine) === key;
    const want = { terminal: owner ? focus.address : null, window_active: Boolean(windowActive), spaces: spacesOf(spaces, machine) };
    if (!needsSend(sent[key], want, owner, now)) continue;
    sends.push({ machine, json: JSON.stringify(want) });
    next[key] = { ...want, at: now };
  }
  return { sends, sent: next };
}

function needsSend(prev, want, owner, now) {
  if (!prev) return owner || want.spaces.length > 0;
  if (prev.terminal !== want.terminal) return true;
  if (JSON.stringify(prev.spaces) !== JSON.stringify(want.spaces)) return true;
  if (!owner) return false;
  return prev.window_active !== want.window_active || now - prev.at >= HEARTBEAT_MS;
}

/** On close: `terminal: null` to every machine that was last told it had focus. */
export function clearPlan(sent) {
  return Object.entries(sent).filter(([, c]) => c.terminal !== null).map(([key, c]) => ({
    machine: key === "" ? null : key,
    json: JSON.stringify({ terminal: null, window_active: false, spaces: c.spaces }),
  }));
}
