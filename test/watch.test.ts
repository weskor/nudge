import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { updateState, type State } from "../src/core/state.ts";
import { autoMessage, cleanTail, COOLDOWN_MS, decide, MIN_TURN_MS, redact, type HerdrStatus } from "../src/core/watch.ts";

const P = "w1:p3";
const T0 = 1_800_000_000_000;
const MIN = 60_000;

const fresh = (): State => ({ auto: true, panes: {} });

/** Feed a sequence of [minutesFromT0, status] events; returns each decision. */
function run(state: State, events: [number, HerdrStatus][], focused = false) {
  return events.map(([minutes, status]) => decide(state, P, status, T0 + minutes * MIN, focused));
}

describe("decide", () => {
  test("a long turn ending in done notifies", () => {
    expect(run(fresh(), [[0, "working"], [5, "done"]]).at(-1)).toEqual({ notify: true, status: "done" });
  });

  test("a turn shorter than MIN_TURN_MS does not", () => {
    const short = (MIN_TURN_MS - 1) / MIN;
    expect(run(fresh(), [[0, "working"], [short, "done"]]).at(-1)).toMatchObject({
      notify: false,
      reason: expect.stringMatching(/shorter/),
    });
  });

  test("done with no recorded start (plugin just installed) does not", () => {
    expect(run(fresh(), [[5, "done"]])[0]).toMatchObject({ notify: false, reason: "turn start unknown" });
  });

  test("turn length counts from the first working, through blocked/approval", () => {
    const state = fresh();
    run(state, [[0, "working"], [1, "blocked"]]);
    state.panes[P]!.last_auto_at = undefined; // ignore the blocked notification's cooldown here
    expect(run(state, [[1.5, "working"], [6, "done"]]).at(-1)).toEqual({ notify: true, status: "done" });
  });

  test("idle ends the turn without notifying (the user saw it finish)", () => {
    const state = fresh();
    expect(run(state, [[0, "working"], [5, "idle"], [6, "done"]]).slice(1)).toMatchObject([
      { notify: false },
      { notify: false, reason: "turn start unknown" },
    ]);
  });

  test("blocked notifies immediately, even early in a turn", () => {
    expect(run(fresh(), [[0, "working"], [0.1, "blocked"]]).at(-1)).toEqual({ notify: true, status: "blocked" });
  });

  test("cooldown: at most one automatic notification per pane per COOLDOWN_MS", () => {
    const state = fresh();
    const cooldown = COOLDOWN_MS / MIN;
    const decisions = run(state, [
      [0, "working"],
      [1, "blocked"],
      [2, "working"],
      [4, "done"],
      [5, "working"],
      [1 + cooldown + 2, "done"],
    ]);
    expect(decisions.filter((d) => d.notify)).toHaveLength(2);
    expect(decisions[3]).toMatchObject({ notify: false, reason: "cooldown" });
  });

  test("cooldown is per pane", () => {
    const state = fresh();
    run(state, [[0, "working"], [1, "blocked"]]);
    expect(decide(state, "other:p1", "blocked", T0 + 1.5 * MIN, false)).toEqual({ notify: true, status: "blocked" });
  });

  test("skipped when the agent already ran `nudge notify` this turn", () => {
    const state = fresh();
    run(state, [[0, "working"]]);
    state.panes[P]!.self_notified_at = T0 + 4 * MIN;
    expect(run(state, [[5, "done"]])[0]).toMatchObject({ notify: false, reason: expect.stringMatching(/already notified/) });
  });

  test("a self-notify from a previous turn does not suppress this one", () => {
    const state = fresh();
    state.panes[P] = { self_notified_at: T0 - MIN };
    expect(run(state, [[0, "working"], [5, "done"]]).at(-1)).toEqual({ notify: true, status: "done" });
  });

  test("focused panes and auto off are skipped, without starting a cooldown", () => {
    const state = fresh();
    expect(run(state, [[0, "working"], [1, "blocked"]], true).at(-1)).toMatchObject({
      notify: false,
      reason: expect.stringMatching(/looking at/),
    });
    expect(state.panes[P]!.last_auto_at).toBeUndefined();
    state.auto = false;
    expect(run(state, [[2, "blocked"]])[0]).toMatchObject({ reason: "auto-notify is off" });
    state.auto = true;
    expect(run(state, [[3, "blocked"]])[0]).toEqual({ notify: true, status: "blocked" });
  });
});

describe("redact", () => {
  test.each([
    ["Authorization: Bearer abcdef0123456789xyz", "Authorization: Bearer [REDACTED]"],
    ["export OPENAI_API_KEY=sk-proj-abcdefghijklmnop1234", "export OPENAI_API_KEY=[REDACTED]"],
    ['password: "hunter2 with spaces"', "password: [REDACTED]"],
    ["token ghp_abcdefghijklmnopqrstuvwxyz0123456789 leaked", "token [REDACTED] leaked"],
    ["AWS key AKIAABCDEFGHIJKLMNOP here", "AWS key [REDACTED] here"],
    ["jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U", "jwt [REDACTED]"],
    ["blob Zx9Qw2Er8Ty4Ui6Op0As3Df5Gh7Jk1Lm9Nb", "blob [REDACTED]"],
    ["-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----", "[REDACTED]"],
  ])("%s", (input, expected) => {
    expect(redact(input)).toBe(expected);
  });

  test("keeps ordinary output: commit SHAs, paths, prose, test counts", () => {
    const text = [
      "commit 8a25740f3c2d1e0b9a8f7e6d5c4b3a2f1e0d9c8b",
      "src/core/very_long_module_name_for_configuration_loading.ts",
      " 51 pass, 0 fail",
      "Finished the refactor; tokens are now rotated nightly.",
    ].join("\n");
    expect(redact(text)).toBe(text);
  });
});

describe("cleanTail", () => {
  test("drops ANSI, rule lines and blank runs; keeps the last N lines", () => {
    const raw = ["\x1b[32mline 1\x1b[0m", "", "", "────────────", "line 2", "line 3", "", "━━━", "❯", "", ""].join("\n");
    expect(cleanTail(raw)).toBe("line 1\n\nline 2\nline 3\n\n❯");
    expect(cleanTail(raw, 2)).toBe("\n❯");
  });

  test("caps very long output", () => {
    expect(cleanTail("x".repeat(10_000)).length).toBeLessThanOrEqual(4_001);
  });
});

test("autoMessage says it was auto-detected and redacts the output", () => {
  const message = autoMessage("blocked", "Approve rm -rf build? token=abc12345secret");
  expect(message).toStartWith("Agent is waiting for approval or input (auto-detected by herdr");
  expect(message).toContain("token=[REDACTED]");
  expect(autoMessage("done", "  ")).toEndWith("No terminal output captured.");
});

test("updateState serializes concurrent writers (no lost updates)", async () => {
  const env = { NUDGE_STATE_DIR: mkdtempSync(join(tmpdir(), "nudge-state-")) };
  await Promise.all(
    Array.from({ length: 20 }, (_, i) =>
      updateState((state) => {
        state.panes[`p${i}`] = { last_auto_at: i };
      }, env),
    ),
  );
  expect(Object.keys(await updateState((state) => state.panes, env))).toHaveLength(20);
});
