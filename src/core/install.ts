// Where this copy of nudge lives, its herdr plugin registration, and the `nudge` command on PATH.
import { lstat, mkdir, readlink, rm, symlink } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { z } from "zod";
import { runCommand, type Run } from "./context.ts";

export const PLUGIN_ID = "weskor.nudge";

type Env = Record<string, string | undefined>;

/**
 * Root of this nudge checkout or plugin install (the directory with herdr-plugin.toml).
 * `bin/nudge` sets NUDGE_ROOT; otherwise derive it from the compiled binary (dist/nudge) or the source file.
 */
export function nudgeRoot(env: Env = process.env): string {
  if (env.NUDGE_ROOT) return env.NUDGE_ROOT;
  if (import.meta.dir.startsWith("/$bunfs")) return dirname(dirname(process.execPath));
  return join(import.meta.dir, "..", "..");
}

const PluginList = z.object({
  result: z.object({
    plugins: z.array(z.object({ enabled: z.boolean(), plugin_root: z.string(), version: z.string() })),
  }),
});
export type PluginInfo = z.infer<typeof PluginList>["result"]["plugins"][number];

const LogList = z.object({
  result: z.object({
    logs: z.array(
      z.object({
        started_unix_ms: z.number(),
        status: z.string(),
        stderr: z.string().default(""),
        action_id: z.string().nullish(),
        event: z.string().nullish(),
      }),
    ),
  }),
});
export type PluginLog = z.infer<typeof LogList>["result"]["logs"][number];

async function herdrJson<T>(schema: z.ZodType<T>, args: string[], env: Env, run: Run): Promise<T | null> {
  const stdout = await run([env.HERDR_BIN_PATH || "herdr", ...args], "/");
  if (stdout === null) return null;
  try {
    const parsed = schema.safeParse(JSON.parse(stdout));
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}

/** `undefined`: herdr isn't installed or running; `null`: herdr is fine but the plugin isn't registered. */
export async function pluginInfo(env: Env = process.env, run: Run = runCommand): Promise<PluginInfo | null | undefined> {
  const list = await herdrJson(PluginList, ["plugin", "list", "--plugin", PLUGIN_ID, "--json"], env, run);
  if (list === null) return undefined;
  return list.result.plugins[0] ?? null;
}

/** Most recent plugin hook/action runs, newest last. */
export async function pluginLogs(env: Env = process.env, run: Run = runCommand): Promise<PluginLog[]> {
  const list = await herdrJson(LogList, ["plugin", "log", "list", "--plugin", PLUGIN_ID], env, run);
  return (list?.result.logs ?? []).toSorted((a, b) => a.started_unix_ms - b.started_unix_ms);
}

export function commandLinkPath(env: Env = process.env): string {
  return join(env.HOME || homedir(), ".local", "bin", "nudge");
}

/** What `~/.local/bin/nudge` currently is. */
export async function commandLink(env: Env = process.env): Promise<{ path: string; target: string | null; exists: boolean }> {
  const path = commandLinkPath(env);
  try {
    const info = await lstat(path);
    return { path, exists: true, target: info.isSymbolicLink() ? await readlink(path) : null };
  } catch {
    return { path, exists: false, target: null };
  }
}

/** Point `~/.local/bin/nudge` at `<root>/bin/nudge`. Refuses to replace a regular file. */
export async function linkCommand(root: string, env: Env = process.env): Promise<string> {
  const link = await commandLink(env);
  if (link.exists && link.target === null) throw new Error(`${link.path} exists and is not a symlink; remove it first`);
  await mkdir(dirname(link.path), { recursive: true });
  if (link.exists) await rm(link.path);
  await symlink(join(root, "bin", "nudge"), link.path);
  return link.path;
}

/**
 * herdr installs each plugin update into a new directory. If the user linked `~/.local/bin/nudge`
 * into an older nudge install, repoint it at this one. Leaves other links and files alone.
 */
export async function refreshCommandLink(root: string, env: Env = process.env): Promise<boolean> {
  const link = await commandLink(env);
  const wanted = join(root, "bin", "nudge");
  if (link.target === null || link.target === wanted || !link.target.endsWith("/bin/nudge")) return false;
  await linkCommand(root, env);
  return true;
}
