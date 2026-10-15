// Where a notification comes from: the herdr agent name and the git repo/branch.
import { basename } from "node:path";
import { z } from "zod";

/** Run a command; trimmed stdout on exit 0, `null` on failure or a missing binary. */
export type Run = (cmd: string[], cwd: string) => Promise<string | null>;

export const runCommand: Run = async (cmd, cwd) => {
  try {
    const proc = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "ignore", stdin: "ignore", timeout: 3_000 });
    const [stdout, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    return code === 0 ? stdout.trim() : null;
  } catch {
    return null;
  }
};

const HerdrAgent = z.object({
  name: z.string().nullish(),
  pane_id: z.string(),
  cwd: z.string().nullish(),
  foreground_cwd: z.string().nullish(),
  focused: z.boolean().nullish(),
});
const AgentGet = z.object({ result: z.object({ agent: HerdrAgent }) });
const AgentList = z.object({ result: z.object({ agents: z.array(HerdrAgent) }) });

type Env = Record<string, string | undefined>;

async function herdrJson<T>(schema: z.ZodType<T>, args: string[], env: Env, cwd: string, run: Run): Promise<T | null> {
  const stdout = await run([env.HERDR_BIN_PATH || "herdr", ...args], cwd);
  if (stdout === null) return null;
  try {
    const parsed = schema.safeParse(JSON.parse(stdout));
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}

/**
 * The herdr agent target to reply to (`herdr agent prompt <target>`), in order:
 * 1. $NUDGE_AGENT;
 * 2. the agent in this pane ($HERDR_PANE_ID): its name, else the pane id;
 * 3. outside herdr panes, the single herdr agent whose cwd is `cwd`: its name, else its pane id.
 * `null` when none applies: a herdr pane without an agent (e.g. a plain shell), herdr unreachable,
 * or several agents sharing the cwd. Never guess: the desk types its reply into this target.
 */
export async function detectAgent(env: Env, cwd: string, run: Run = runCommand): Promise<string | null> {
  if (env.NUDGE_AGENT) return env.NUDGE_AGENT;
  if (env.HERDR_PANE_ID) {
    const got = await herdrJson(AgentGet, ["agent", "get", env.HERDR_PANE_ID], env, cwd, run);
    return got ? got.result.agent.name || got.result.agent.pane_id : null;
  }
  const list = await herdrJson(AgentList, ["agent", "list"], env, cwd, run);
  const here = list?.result.agents.filter((a) => a.cwd === cwd || a.foreground_cwd === cwd) ?? [];
  return here.length === 1 ? here[0]!.name || here[0]!.pane_id : null;
}

export interface HerdrAgentInfo {
  /** Name to reply to: the agent's name, else its pane id. */
  target: string;
  /** The agent's working directory, when herdr knows it. */
  cwd: string | null;
  /** Whether the pane is focused in the herdr client. */
  focused: boolean;
}

/** Look up the herdr agent in `paneId`; `null` if herdr is unavailable or there is no agent there. */
export async function herdrAgent(paneId: string, env: Env, cwd: string, run: Run = runCommand): Promise<HerdrAgentInfo | null> {
  const got = await herdrJson(AgentGet, ["agent", "get", paneId], env, cwd, run);
  if (!got) return null;
  const agent = got.result.agent;
  return { target: agent.name || agent.pane_id, cwd: agent.foreground_cwd || agent.cwd || null, focused: agent.focused === true };
}

/** "owner/name" from a hosted git remote URL (ssh, scp-style or https), else null. */
export function repoFromRemote(remote: string): string | null {
  const match = /^(?:[a-z][a-z0-9+.-]*:\/\/(?:[^@/]+@)?[^/]+\/|[^@/:]+@[^:/]+:)(.+?)(?:\.git)?\/?$/i.exec(remote);
  return match?.[1] ?? null;
}

export interface GitInfo {
  repo: string | null;
  branch: string | null;
}

/** Repo ("owner/name" from origin, else the checkout's directory name) and current branch. */
export async function gitInfo(cwd: string, run: Run = runCommand): Promise<GitInfo> {
  const toplevel = await run(["git", "rev-parse", "--show-toplevel"], cwd);
  if (toplevel === null) return { repo: null, branch: null };
  const [remote, branch] = await Promise.all([
    run(["git", "remote", "get-url", "origin"], cwd),
    run(["git", "branch", "--show-current"], cwd),
  ]);
  return {
    repo: (remote && repoFromRemote(remote)) || basename(toplevel),
    branch: branch || null,
  };
}
