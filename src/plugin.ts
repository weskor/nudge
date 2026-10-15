// herdr plugin entrypoints (see herdr-plugin.toml), reached through `nudge plugin …`:
//   event                 pane.agent_status_changed hook
//   action <id>           enable | disable | toggle | ask-desk | setup | status
//   startup               server start: keep ~/.local/bin/nudge pointing at this install
// Decisions and failures go to stderr, which herdr keeps in `herdr plugin log list`.
import { z } from "zod";
import { ConfigError, findDesk, loadConfig } from "./core/config.ts";
import { herdrAgent, runCommand } from "./core/context.ts";
import { nudgeRoot, PLUGIN_ID, refreshCommandLink } from "./core/install.ts";
import { buildNotification, DeskError, postToDesk, type NotifyStatus } from "./core/notify.ts";
import { updateState } from "./core/state.ts";
import { autoMessage, cleanTail, decide } from "./core/watch.ts";

const env = process.env;
const herdr = env.HERDR_BIN_PATH || "herdr";
const home = env.HOME || "/";

const StatusEvent = z.object({
  data: z.object({
    pane_id: z.string(),
    agent_status: z.enum(["idle", "working", "blocked", "done", "unknown"]),
  }),
});

const ActionContext = z.object({
  focused_pane_id: z.string().optional(),
  focused_pane_status: z.string().optional(),
});

function log(message: string): void {
  console.error(`[nudge] ${message}`);
}

async function toast(body: string): Promise<void> {
  await runCommand([herdr, "notification", "show", "nudge", "--body", body], home);
}

function parseJsonEnv<T>(schema: z.ZodType<T>, name: string): T | null {
  try {
    const parsed = schema.safeParse(JSON.parse(env[name] ?? ""));
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}

/** Send the pane's recent output to the default desk on behalf of its agent. Returns the HTTP status. */
async function sendPane(paneId: string, status: NotifyStatus, kind: "done" | "blocked" | "manual"): Promise<number> {
  const config = await loadConfig();
  const desk = findDesk(config);
  const [agent, raw] = await Promise.all([
    herdrAgent(paneId, env, home),
    runCommand([herdr, "pane", "read", paneId, "--source", "recent-unwrapped", "--lines", "200", "--format", "text"], home),
  ]);
  const notification = await buildNotification(
    { status, message: autoMessage(kind, cleanTail(raw ?? "")), agent: agent?.target ?? paneId },
    { env, cwd: agent?.cwd ?? home, run: runCommand },
  );
  return postToDesk(desk, notification, (url, init) => fetch(url, init));
}

async function onStatusChanged(): Promise<void> {
  const event = parseJsonEnv(StatusEvent, "HERDR_PLUGIN_EVENT_JSON");
  if (!event) return log("ignoring event without pane_id/agent_status");
  const { pane_id: paneId, agent_status: status } = event.data;
  const settles = status === "done" || status === "blocked";
  const focused = settles ? ((await herdrAgent(paneId, env, home))?.focused ?? false) : false;
  const decision = await updateState((state) => decide(state, paneId, status, Date.now(), focused));
  if (!decision.notify) {
    if (settles) log(`${paneId} ${status}: skipped (${decision.reason})`);
    return;
  }
  const httpStatus = await sendPane(paneId, decision.status, decision.status);
  log(`${paneId} ${status}: notified desk (HTTP ${httpStatus})`);
}

/** Open one of the plugin's overlay panes (declared in herdr-plugin.toml) over the active pane. */
async function openPane(entrypoint: "setup" | "status"): Promise<void> {
  // herdr places overlays over the active pane and rejects an explicit --target-pane for them.
  const args = [herdr, "plugin", "pane", "open", "--plugin", PLUGIN_ID, "--entrypoint", entrypoint, "--focus"];
  if ((await runCommand(args, home)) === null) throw new Error(`could not open the ${entrypoint} pane`);
}

async function onAction(action: string | undefined): Promise<void> {
  const context = parseJsonEnv(ActionContext, "HERDR_PLUGIN_CONTEXT_JSON");
  const paneId = context?.focused_pane_id ?? env.HERDR_PANE_ID;
  switch (action) {
    case "enable":
    case "disable":
    case "toggle": {
      const auto = await updateState((state) => (state.auto = action === "toggle" ? !state.auto : action === "enable"));
      await toast(`Auto-notify ${auto ? "on" : "off"}`);
      return;
    }
    case "ask-desk": {
      if (!paneId) return toast("No pane to send");
      const status: NotifyStatus = context?.focused_pane_status === "blocked" ? "blocked" : "question";
      const httpStatus = await sendPane(paneId, status, "manual");
      await toast(`Sent this pane to the desk (HTTP ${httpStatus})`);
      return;
    }
    case "setup":
    case "status":
      return openPane(action);
    default:
      throw new Error(`unknown action "${action}"`);
  }
}

/** Run a plugin entrypoint; returns the exit code. */
export async function runPlugin(args: string[]): Promise<number> {
  const [mode, action] = args;
  try {
    if (mode === "event") await onStatusChanged();
    else if (mode === "action") await onAction(action);
    else if (mode === "startup") {
      if (await refreshCommandLink(nudgeRoot())) log("repointed ~/.local/bin/nudge at this install");
    } else throw new Error("usage: nudge plugin event | action <id> | startup");
    return 0;
  } catch (error) {
    const message = (error as Error).message;
    log(`failed: ${message}`);
    if (error instanceof ConfigError || error instanceof DeskError) await toast(`Desk notify failed: ${message.split("\n")[0]}`);
    return 1;
  }
}
