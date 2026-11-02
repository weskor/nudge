// Pure option builders for the panel's popups: the route popup on an agent or workspace row,
// the default-webhook popup, the footer menu and the machine picker. Each option carries the
// exact `nudge` argv it runs and the machine it runs on (`on`), so the overlay only executes it.
import { THIS_COMPUTER, machineLabel, machineNote, ownRoute } from "./rows.js";

export const BY_ID_HINT = "(by id, ends with the session)";
export const SSH_NOTE =
  "Servers you reach only over SSH (herdr TUI) aren't listed. Change them there: nudge on|off, or the nudge.* actions.";

/**
 * How a row is named in a route: its name when it has one, else its id with a hint.
 * An existing route keeps the selector it was written with.
 */
function selector(row, key) {
  const values = key === "pane" ? [row.name, row.local, row.address] : [row.name, row.local];
  return { values, value: row.name || row.local, byId: !row.name };
}

/**
 * The route popup's options for an agent row (`kind: "agent"`) or a workspace row (`kind: "space"`).
 * @param {object} status that machine's `nudge status --json`
 * @param {object} row a row from agentRows() / spaceRows()
 * @returns {{id: string, label: string, hint?: string, checked: boolean, argv: string[] | null, on: string | null, action: string}[]}
 */
export function routeOptions(status, row) {
  const key = row.kind === "agent" ? "pane" : "workspace";
  const sel = selector(row, key);
  const own = ownRoute(status, key, sel.values);
  const target = own ? own[key] : sel.value;
  const hint = !own && sel.byId ? BY_ID_HINT : undefined;
  const backend = own?.backend ? ["--backend", own.backend] : [];
  const on = row.machine;
  const options = (status.webhooks ?? []).map((w) => ({
    id: `hook-${w.name}`, label: w.name, hint, checked: own?.webhook === w.name, on, action: "route",
    argv: ["route", "set", `--${key}`, target, "--webhook", w.name, ...backend],
  }));
  options.push({
    id: "inherit", label: key === "pane" ? "Use workspace / default" : "Use default", checked: !own, on, action: "route",
    argv: own ? ["route", "rm", `--${key}`, target, ...backend] : null,
  });
  if (key === "pane") options.push(sendOption(row));
  return options;
}

/** "Send this pane now": `nudge core send <address on its machine>` there. */
export function sendOption(row) {
  return {
    id: "send", label: "Send this pane now", checked: false, on: row.machine, action: "send",
    argv: ["core", "send", row.local, ...(row.status === "blocked" ? ["--blocked"] : [])],
  };
}

/** The default-webhook popup: every webhook, the default ticked, each running `nudge default <name>`. */
export function defaultOptions(status, machine) {
  return (status.webhooks ?? []).map((w) => ({
    id: `default-${w.name}`, label: w.name, checked: status.default_webhook === w.name, on: machine,
    action: "route", argv: ["default", w.name],
  }));
}

/**
 * The footer menu. `tab` options open a terminal tab running `argv` on that machine; `nudge` options
 * exec nudge there; `folder` opens nudge's config folder (this computer only).
 */
export function footerOptions(status, machine) {
  const installed = status.core?.service?.installed;
  const options = [
    { id: "setup", label: "Add a webhook…", action: "tab", argv: ["nudge", "setup"], on: machine },
    { id: "settings", label: "Edit settings…", action: "tab", argv: ["nudge", "settings"], on: machine },
    installed
      ? { id: "service", label: "Remove service", action: "nudge", argv: ["core", "service", "remove"], on: machine }
      : { id: "service", label: "Install service…", action: "tab", argv: ["nudge", "core", "service", "install"], on: machine },
  ];
  if (machine === THIS_COMPUTER) options.push({ id: "folder", label: "Open config folder", action: "folder", argv: null, on: null });
  return options;
}

/** The machine picker: this computer and each connected machine, the picked one ticked, with its note. */
export function machineOptions(list, states, picked) {
  return list.map((m) => ({
    id: `machine-${m ?? "here"}`, label: machineLabel(m), machine: m, checked: m === picked,
    note: machineNote(states[m ?? ""]),
  }));
}

/**
 * The toast after "Send this pane now", from the exec result (null: exec rejected).
 * Never shows nudge's stderr: it may name a webhook's address.
 */
export function sendToast(result, row) {
  const name = row.label;
  if (result && result.code === 0) return { level: "success", title: `Sent ${name}`, body: row.webhook ? `to ${row.webhook}` : undefined };
  if (result && /nudge is off/.test(result.stderr)) return { level: "warning", title: "Nudge is off: nothing was sent" };
  const to = row.webhook ?? "the webhook";
  return { level: "error", title: `nudge couldn't reach ${to}`, action: { name: "nudge.open" } };
}

/** The toast when the big switch fails and flips back. */
export function switchToast(result, wanted) {
  const why = result ? (result.stderr || result.stdout).trim().split("\n")[0].replace(/https?:\/\/\S+/g, "…") : "nudge not found";
  return { level: "error", title: `Couldn't turn nudge ${wanted ? "on" : "off"}`, body: why };
}
