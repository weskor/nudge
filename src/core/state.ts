// Small persistent state shared by the CLI and the herdr plugin, e.g. ~/.local/state/nudge/state.json.
import { mkdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import { z } from "zod";

const PaneState = z.object({
  /** When the current turn started (first `working` after idle/done). */
  working_since: z.number().optional(),
  /** Last automatic notification sent by the plugin for this pane. */
  last_auto_at: z.number().optional(),
  /** Last time the agent in this pane ran `nudge notify` itself. */
  self_notified_at: z.number().optional(),
});
export type PaneState = z.infer<typeof PaneState>;

const State = z.object({
  /** Automatic notifications from the herdr plugin. Off until turned on (setup asks), since they send terminal output. */
  auto: z.boolean().default(false),
  panes: z.record(z.string(), PaneState).default({}),
});
export type State = z.infer<typeof State>;

type Env = Record<string, string | undefined>;

const LOCK_RETRIES = 100;
const LOCK_RETRY_MS = 20;
const LOCK_STALE_MS = 5_000;

export function stateDir(env: Env = process.env): string {
  if (env.NUDGE_STATE_DIR) return env.NUDGE_STATE_DIR;
  return join(env.XDG_STATE_HOME || join(homedir(), ".local", "state"), "nudge");
}

async function readState(path: string): Promise<State> {
  try {
    const parsed = State.safeParse(JSON.parse(await readFile(path, "utf8")));
    if (parsed.success) return parsed.data;
  } catch {
    // Missing or corrupt state starts fresh; it only holds timestamps and a toggle.
  }
  return State.parse({});
}

/** Hold an exclusive lock (a directory next to the state file) while `body` runs. */
async function withLock<T>(path: string, body: () => Promise<T>): Promise<T> {
  const lock = `${path}.lock`;
  for (let attempt = 0; ; attempt++) {
    try {
      await mkdir(lock);
      break;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      const age = await stat(lock).then((s) => Date.now() - s.mtimeMs, () => 0);
      if (age > LOCK_STALE_MS) await rm(lock, { recursive: true, force: true });
      else if (attempt >= LOCK_RETRIES) throw new Error(`timed out waiting for ${lock}`);
      else await Bun.sleep(LOCK_RETRY_MS);
    }
  }
  try {
    return await body();
  } finally {
    await rm(lock, { recursive: true, force: true });
  }
}

/** Read-modify-write the state under a lock. `change` mutates the state and returns a result. */
export async function updateState<T>(change: (state: State) => T, env: Env = process.env): Promise<T> {
  const dir = stateDir(env);
  await mkdir(dir, { recursive: true, mode: 0o700 });
  const path = join(dir, "state.json");
  return withLock(path, async () => {
    const state = await readState(path);
    const result = change(state);
    const temp = `${path}.${process.pid}.tmp`;
    await writeFile(temp, JSON.stringify(state), { mode: 0o600 });
    await rename(temp, path);
    return result;
  });
}
