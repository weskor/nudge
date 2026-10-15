import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Config } from "../src/core/config.ts";
import type { Run } from "../src/core/context.ts";
import { DeskError, notifyDesk, pingDesk, replyHint, type Deps, type Notification } from "../src/core/notify.ts";
import { updateState } from "../src/core/state.ts";

const config: Config = {
  default_desk: "bridge",
  desk: [
    { name: "bridge", url: "https://desks.test/bridge", authorization: "Bearer bridge-secret" },
    { name: "research", url: "https://desks.test/research", authorization: "Token research-secret" },
  ],
};

const herdrAndGit: Record<string, string> = {
  "herdr agent get w1:p1": JSON.stringify({ result: { agent: { name: "api-refactor", pane_id: "w1:p1" } } }),
  "git rev-parse --show-toplevel": "/home/user/src/api",
  "git remote get-url origin": "git@github.com:acme/api.git",
  "git branch --show-current": "main",
};
const run: Run = async (cmd) => herdrAndGit[cmd.join(" ")] ?? null;

interface Call {
  url: string;
  method: string;
  headers: Headers;
  body: string;
}

/** Each fake gets its own state dir so tests never touch ~/.local/state/nudge. */
function fakeDeps(respond: () => Response | Promise<Response>, env: Record<string, string> = { HERDR_PANE_ID: "w1:p1" }) {
  const calls: Call[] = [];
  const stateDir = mkdtempSync(join(tmpdir(), "nudge-state-"));
  const deps: Deps = {
    origin: { env: { ...env, NUDGE_STATE_DIR: stateDir }, cwd: "/home/user/src/api", run },
    fetch: async (url, init) => {
      calls.push({ url, method: init.method ?? "GET", headers: new Headers(init.headers), body: String(init.body) });
      return respond();
    },
  };
  return { deps, calls, stateDir };
}

describe("notifyDesk", () => {
  test("POSTs the notification to the default desk with its Authorization header", async () => {
    const { deps, calls } = fakeDeps(() => new Response("queued", { status: 202 }));
    const sent = await notifyDesk(config, { status: "blocked", message: "  Need the staging DB password.\n" }, deps);

    const expected: Notification = {
      from_agent: "api-refactor",
      cwd: "/home/user/src/api",
      repo: "acme/api",
      branch: "main",
      status: "blocked",
      message: "Need the staging DB password.",
      reply_hint: 'reply with: herdr agent prompt api-refactor "..."',
    };
    expect(sent).toEqual({ desk: "bridge", http_status: 202, notification: expected });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe("https://desks.test/bridge");
    expect(calls[0]!.method).toBe("POST");
    expect(calls[0]!.headers.get("authorization")).toBe("Bearer bridge-secret");
    expect(calls[0]!.headers.get("content-type")).toBe("application/json");
    expect(JSON.parse(calls[0]!.body)).toEqual(expected);
  });

  test("--desk and --agent override the default desk and detected agent", async () => {
    const { deps, calls } = fakeDeps(() => new Response(null, { status: 200 }));
    const sent = await notifyDesk(config, { desk: "research", status: "question", message: "q", agent: "reviewer" }, deps);
    expect(calls[0]!.url).toBe("https://desks.test/research");
    expect(calls[0]!.headers.get("authorization")).toBe("Token research-secret");
    expect(sent.notification.from_agent).toBe("reviewer");
    expect(sent.notification.reply_hint).toBe('reply with: herdr agent prompt reviewer "..."');
  });

  test("still sends when the agent cannot be detected, with from_agent null", async () => {
    const { deps } = fakeDeps(() => new Response(null, { status: 200 }), {});
    const sent = await notifyDesk(config, { status: "done", message: "finished" }, deps);
    expect(sent.notification.from_agent).toBeNull();
    expect(sent.notification.reply_hint).toContain("could not be detected");
  });

  test("webhook rejection surfaces status and body without leaking the secret", async () => {
    const { deps } = fakeDeps(() => new Response("bad auth", { status: 403 }));
    const error = await notifyDesk(config, { status: "done", message: "x" }, deps).catch((e) => e);
    expect(error).toBeInstanceOf(DeskError);
    expect(error.message).toBe('desk "bridge" webhook returned HTTP 403: bad auth');
    expect(error.message).not.toContain("bridge-secret");
  });

  test("network failure is a DeskError", async () => {
    const { deps } = fakeDeps(() => Promise.reject(new TypeError("connection refused")));
    await expect(notifyDesk(config, { status: "done", message: "x" }, deps)).rejects.toThrow(
      'could not reach desk "bridge" webhook: connection refused',
    );
  });

  test("empty message and unknown desk fail before any request", async () => {
    const { deps, calls } = fakeDeps(() => new Response(null));
    await expect(notifyDesk(config, { status: "done", message: " \n " }, deps)).rejects.toThrow("message is empty");
    await expect(notifyDesk(config, { desk: "nope", status: "done", message: "x" }, deps)).rejects.toThrow(/unknown desk/);
    expect(calls).toHaveLength(0);
  });

  test("records the notifying pane so the herdr plugin skips its duplicate; failures record nothing", async () => {
    const ok = fakeDeps(() => new Response(null, { status: 200 }));
    const before = Date.now();
    await notifyDesk(config, { status: "done", message: "x" }, ok.deps);
    const panes = await updateState((state) => state.panes, ok.deps.origin.env);
    expect(panes["w1:p1"]!.self_notified_at).toBeGreaterThanOrEqual(before);

    const failed = fakeDeps(() => new Response("nope", { status: 500 }));
    await notifyDesk(config, { status: "done", message: "x" }, failed.deps).catch(() => {});
    expect(await updateState((state) => state.panes, failed.deps.origin.env)).toEqual({});
  });
});

test("pingDesk sends a status test notification and does not count as the agent notifying", async () => {
  const { deps, calls } = fakeDeps(() => new Response(null, { status: 204 }));
  const sent = await pingDesk(config, {}, deps);
  expect(await updateState((state) => state.panes, deps.origin.env)).toEqual({});
  expect(sent.desk).toBe("bridge");
  expect(JSON.parse(calls[0]!.body)).toMatchObject({ status: "test", from_agent: "api-refactor", repo: "acme/api" });
});

test("replyHint shell-quotes agent targets that need it", () => {
  expect(replyHint("w1:p1")).toBe('reply with: herdr agent prompt w1:p1 "..."');
  expect(replyHint("my agent")).toBe(`reply with: herdr agent prompt 'my agent' "..."`);
  expect(replyHint("it's")).toBe(`reply with: herdr agent prompt 'it'\\''s' "..."`);
});
