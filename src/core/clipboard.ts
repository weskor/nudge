import { readdirSync } from "node:fs";
import { runCommand, type Run } from "./context.ts";

/** Clipboard tools nudge setup can use, for doctor's report. */
export const CLIPBOARD_TOOLS = ["wl-paste", "xclip"];

const WL_PASTE = ["wl-paste", "--no-newline", "--type", "text"];
const XCLIP = ["xclip", "-selection", "clipboard", "-o"];

type Env = Record<string, string | undefined>;

/**
 * Clipboard commands to try, in order: wl-paste, then xclip, each as-is first.
 * Processes started by the herdr server (plugin panes) may lack the desktop session's
 * XDG_RUNTIME_DIR, WAYLAND_DISPLAY or DISPLAY, so we also try every Wayland socket in the
 * runtime dir and DISPLAY=:0.
 */
export function clipboardCommands(env: Env, runtimeDir: string, waylandSockets: string[]): string[][] {
  const commands = [WL_PASTE];
  for (const socket of waylandSockets) {
    if (socket === env.WAYLAND_DISPLAY && runtimeDir === env.XDG_RUNTIME_DIR) continue;
    commands.push(["env", `XDG_RUNTIME_DIR=${runtimeDir}`, `WAYLAND_DISPLAY=${socket}`, ...WL_PASTE]);
  }
  commands.push(XCLIP);
  if (!env.DISPLAY) commands.push(["env", "DISPLAY=:0", ...XCLIP]);
  return commands;
}

function waylandSocketsIn(dir: string): string[] {
  try {
    return readdirSync(dir).filter((name) => /^wayland-\d+$/.test(name));
  } catch {
    return [];
  }
}

/** Trimmed text from the first clipboard command that works and isn't empty; `null` if none do. */
export async function readClipboard(
  run: Run = runCommand,
  env: Env = process.env,
  runtimeDir: string = env.XDG_RUNTIME_DIR || `/run/user/${process.getuid?.() ?? 0}`,
): Promise<string | null> {
  for (const cmd of clipboardCommands(env, runtimeDir, waylandSocketsIn(runtimeDir))) {
    const text = await run(cmd, "/");
    if (text) return text;
  }
  return null;
}
