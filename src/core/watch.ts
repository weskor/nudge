// Pure logic for the herdr plugin: which agent status changes become desk notifications,
// and how the pane's terminal output is trimmed and redacted before it leaves the machine.
import type { PaneState, State } from "./state.ts";

/** A `done` turn shorter than this is not worth a notification. */
export const MIN_TURN_MS = 2 * 60_000;
/** At most one automatic notification per pane in this window. */
export const COOLDOWN_MS = 5 * 60_000;
/** Lines of terminal output sent with an automatic notification. */
export const TAIL_LINES = 40;
const TAIL_MAX_CHARS = 4_000;

export type HerdrStatus = "idle" | "working" | "blocked" | "done" | "unknown";

export type Decision = { notify: true; status: "done" | "blocked" } | { notify: false; reason: string };

/**
 * Apply one `pane.agent_status_changed` event to `state` (mutated in place) and decide whether to notify.
 * - `working` starts a turn; `idle` (seen by you) and `done` end it.
 * - `blocked` always qualifies; `done` only after a turn of at least MIN_TURN_MS with a known start.
 * - Skipped when auto-notify is off, you are looking at the pane (`focused`), the agent already ran
 *   `nudge notify` this turn, or the pane is in cooldown.
 */
export function decide(state: State, paneId: string, status: HerdrStatus, now: number, focused: boolean): Decision {
  const pane: PaneState = (state.panes[paneId] ??= {});
  const turnStart = pane.working_since;

  switch (status) {
    case "working":
      pane.working_since ??= now;
      return { notify: false, reason: "turn started" };
    case "idle":
    case "unknown":
      delete pane.working_since;
      return { notify: false, reason: `status ${status}` };
    case "done":
      delete pane.working_since;
      break;
    case "blocked":
      break;
  }

  if (!state.auto) return { notify: false, reason: "auto-notify is off" };
  if (focused) return { notify: false, reason: "you are looking at this pane" };
  const selfNotifiedSince = turnStart ?? now - MIN_TURN_MS;
  if (pane.self_notified_at !== undefined && pane.self_notified_at >= selfNotifiedSince) {
    return { notify: false, reason: "agent already notified the desk this turn" };
  }
  if (status === "done") {
    if (turnStart === undefined) return { notify: false, reason: "turn start unknown" };
    if (now - turnStart < MIN_TURN_MS) return { notify: false, reason: `turn shorter than ${MIN_TURN_MS / 1000}s` };
  }
  if (pane.last_auto_at !== undefined && now - pane.last_auto_at < COOLDOWN_MS) {
    return { notify: false, reason: "cooldown" };
  }
  pane.last_auto_at = now;
  return { notify: true, status };
}

const REDACTED = "[REDACTED]";

/** Replace things that look like credentials. Errs toward over-redacting; this text goes to a remote webhook. */
export function redact(text: string): string {
  return (
    text
      .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)/g, REDACTED)
      .replace(/\b(Bearer|Basic|Token)\s+[A-Za-z0-9._~+/=-]{8,}/gi, `$1 ${REDACTED}`)
      .replace(
        /\b([A-Za-z0-9_]*(?:secret|token|passw(?:or)?d|api[_-]?key|private[_-]?key|credential)[A-Za-z0-9_]*)(\s*[=:]\s*)("[^"]*"|'[^']*'|\S+)/gi,
        `$1$2${REDACTED}`,
      )
      .replace(
        /\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[A-Za-z0-9_-]{30,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})/g,
        REDACTED,
      )
      // Long random-looking tokens (letters and digits mixed); pure hex (commit SHAs) is kept.
      .replace(/(?<![A-Za-z0-9_/.-])[A-Za-z0-9_-]{32,}(?![A-Za-z0-9_/.-])/g, (token) =>
        /^[0-9a-f]+$/i.test(token) || !/\d/.test(token) || !/[A-Za-z]/.test(token) ? token : REDACTED,
      )
  );
}

/** Last `TAIL_LINES` meaningful lines of a pane: no ANSI, no box-drawing rules, no blank runs. */
export function cleanTail(raw: string, lines = TAIL_LINES): string {
  const kept: string[] = [];
  for (const line of raw.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "").split(/\r?\n/)) {
    const trimmed = line.trimEnd();
    if (/^[\s─━═│┃┄┅┈┉╌╍—_=~·•-]+$/.test(trimmed) && trimmed.trim() !== "") continue;
    if (trimmed === "" && (kept.length === 0 || kept.at(-1) === "")) continue;
    kept.push(trimmed);
  }
  while (kept.at(-1) === "") kept.pop();
  const tail = kept.slice(-lines).join("\n");
  return tail.length > TAIL_MAX_CHARS ? `…${tail.slice(-TAIL_MAX_CHARS)}` : tail;
}

/** Message for an automatic or on-demand notification built from the pane's output. */
export function autoMessage(kind: "done" | "blocked" | "manual", tail: string): string {
  const lead = {
    done: "Agent finished a turn (auto-detected by herdr; this message was not written by the agent).",
    blocked: "Agent is waiting for approval or input (auto-detected by herdr; this message was not written by the agent).",
    manual: "The user asked you to look at this agent (sent from herdr; this message was not written by the agent).",
  }[kind];
  const output = redact(tail).trim();
  return output ? `${lead}\nLast terminal output (secrets redacted):\n\n${output}` : `${lead}\nNo terminal output captured.`;
}
