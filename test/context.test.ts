import { describe, expect, test } from "bun:test";
import { detectAgent, gitInfo, repoFromRemote, type Run } from "../src/core/context.ts";

/** Fake command runner keyed by the joined argv; unknown commands fail like a missing binary. */
function fakeRun(outputs: Record<string, string>): Run & { calls: string[] } {
  const calls: string[] = [];
  const run = async (cmd: string[]) => {
    const key = cmd.join(" ");
    calls.push(key);
    return outputs[key] ?? null;
  };
  return Object.assign(run, { calls });
}

const agentJson = (agent: Record<string, unknown>) => JSON.stringify({ id: "cli:agent:get", result: { agent } });
const listJson = (agents: Record<string, unknown>[]) => JSON.stringify({ id: "cli:agent:list", result: { agents } });

describe("detectAgent", () => {
  test("NUDGE_AGENT wins without calling herdr", async () => {
    const run = fakeRun({});
    expect(await detectAgent({ NUDGE_AGENT: "me", HERDR_PANE_ID: "w1:p1" }, "/repo", run)).toBe("me");
    expect(run.calls).toEqual([]);
  });

  test("named agent in this pane", async () => {
    const run = fakeRun({ "herdr agent get w1:p1": agentJson({ name: "api-refactor", pane_id: "w1:p1" }) });
    expect(await detectAgent({ HERDR_PANE_ID: "w1:p1" }, "/repo", run)).toBe("api-refactor");
  });

  test("unnamed agent in this pane falls back to the pane id", async () => {
    const run = fakeRun({ "herdr agent get w1:p5": agentJson({ pane_id: "w1:p5", cwd: "/repo" }) });
    expect(await detectAgent({ HERDR_PANE_ID: "w1:p5" }, "/repo", run)).toBe("w1:p5");
  });

  test("a pane without an agent (plain shell) or an unreachable herdr gives null, not the pane id", async () => {
    const shellPane = fakeRun({}); // `herdr agent get` exits non-zero: agent_not_found
    expect(await detectAgent({ HERDR_PANE_ID: "w1:p2" }, "/repo", shellPane)).toBeNull();
    const junk = fakeRun({ "herdr agent get w1:p5": "not json" });
    expect(await detectAgent({ HERDR_PANE_ID: "w1:p5" }, "/repo", junk)).toBeNull();
    expect(shellPane.calls).toEqual(["herdr agent get w1:p2"]); // no cwd guessing inside a herdr pane
  });

  test("uses HERDR_BIN_PATH when set", async () => {
    const run = fakeRun({ "/opt/herdr agent get p": agentJson({ name: "x", pane_id: "p" }) });
    expect(await detectAgent({ HERDR_PANE_ID: "p", HERDR_BIN_PATH: "/opt/herdr" }, "/repo", run)).toBe("x");
  });

  test("outside a pane: the single agent whose cwd matches", async () => {
    const run = fakeRun({
      "herdr agent list": listJson([
        { name: "other", pane_id: "w1:p1", cwd: "/elsewhere" },
        { name: "builder", pane_id: "w2:p2", cwd: "/home/x", foreground_cwd: "/repo" },
      ]),
    });
    expect(await detectAgent({}, "/repo", run)).toBe("builder");
  });

  test("outside a pane: ambiguous or no cwd match gives null", async () => {
    const two = fakeRun({
      "herdr agent list": listJson([
        { name: "a", pane_id: "p1", cwd: "/repo" },
        { name: "b", pane_id: "p2", cwd: "/repo" },
      ]),
    });
    expect(await detectAgent({}, "/repo", two)).toBeNull();
    expect(await detectAgent({}, "/repo", fakeRun({ "herdr agent list": listJson([]) }))).toBeNull();
    expect(await detectAgent({}, "/repo", fakeRun({}))).toBeNull();
  });
});

describe("repoFromRemote", () => {
  test.each([
    ["git@github.com:acme/api.git", "acme/api"],
    ["https://github.com/acme/api.git", "acme/api"],
    ["https://github.com/acme/api", "acme/api"],
    ["https://user:token@github.com/acme/api.git", "acme/api"],
    ["ssh://git@gitlab.example:2222/group/sub/proj.git", "group/sub/proj"],
    ["/srv/git/proj.git", null],
  ])("%s -> %s", (remote, repo) => {
    expect(repoFromRemote(remote)).toBe(repo);
  });
});

describe("gitInfo", () => {
  test("repo from origin and current branch", async () => {
    const run = fakeRun({
      "git rev-parse --show-toplevel": "/home/user/src/api",
      "git remote get-url origin": "https://github.com/acme/api.git",
      "git branch --show-current": "main",
    });
    expect(await gitInfo("/home/user/src/api/src", run)).toEqual({ repo: "acme/api", branch: "main" });
  });

  test("no origin: directory name; detached HEAD: null branch", async () => {
    const run = fakeRun({ "git rev-parse --show-toplevel": "/tmp/scratch", "git branch --show-current": "" });
    expect(await gitInfo("/tmp/scratch", run)).toEqual({ repo: "scratch", branch: null });
  });

  test("not a git checkout", async () => {
    expect(await gitInfo("/tmp", fakeRun({}))).toEqual({ repo: null, branch: null });
  });
});
