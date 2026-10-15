// `nudge doctor`: check every piece and say how to fix what's broken.
import { existsSync } from "node:fs";
import { join } from "node:path";
import { CLIPBOARD_TOOLS } from "./core/clipboard.ts";
import { ConfigError, defaultConfigPath, defaultDeskName, readConfig } from "./core/config.ts";
import { runCommand } from "./core/context.ts";
import { commandLink, nudgeRoot, pluginInfo, pluginLogs, PLUGIN_ID } from "./core/install.ts";
import { DeskError, pingDesk } from "./core/notify.ts";
import { stateDir, updateState } from "./core/state.ts";
import { mark, style, waitForKey } from "./tty.ts";

type Level = "ok" | "warn" | "fail";

interface Check {
  level: Level;
  title: string;
  detail?: string;
  fix?: string;
}

function print(check: Check): void {
  const icon = { ok: mark.ok, warn: mark.warn, fail: mark.fail }[check.level];
  console.log(`${icon} ${check.title}${check.detail ? `  ${style.dim(check.detail)}` : ""}`);
  if (check.fix) console.log(`    ${style.dim("→")} ${check.fix}`);
}

async function checkConfig(ping: boolean): Promise<Check[]> {
  const path = defaultConfigPath();
  let config;
  try {
    config = await readConfig(path);
  } catch (error) {
    if (!(error instanceof ConfigError)) throw error;
    return [{ level: "fail", title: "Config is invalid", detail: path, fix: error.message.replace(/\n/g, "\n      ") }];
  }
  if (!config) return [{ level: "fail", title: "No desk configured", detail: path, fix: "run nudge setup" }];
  const defaultName = defaultDeskName(config);
  const checks: Check[] = [
    {
      level: defaultName ? "ok" : "warn",
      title: `Config: ${config.desk.length} desk${config.desk.length === 1 ? "" : "s"}, default ${defaultName ? `"${defaultName}"` : "not set"}`,
      detail: path,
      fix: defaultName ? undefined : "run nudge setup --desk NAME --default (the plugin sends to the default desk)",
    },
  ];
  if (!defaultName) return checks;
  if (!ping) {
    checks.push({ level: "ok", title: "Webhook not tested", detail: "nudge doctor --ping sends a test ping" });
    return checks;
  }
  try {
    const sent = await pingDesk(config, { desk: defaultName });
    checks.push({ level: "ok", title: `Webhook accepted a test ping (HTTP ${sent.http_status})` });
  } catch (error) {
    if (!(error instanceof DeskError)) throw error;
    const fix =
      error.blames === "authorization"
        ? `copy the Authorization header again: nudge setup --desk ${defaultName}`
        : error.blames === "url"
          ? `copy the Webhook URL again: nudge setup --desk ${defaultName}`
          : "check that the desk is running, then nudge test";
    checks.push({ level: "fail", title: "Webhook test failed", detail: error.message, fix });
  }
  return checks;
}

async function checkPlugin(): Promise<Check[]> {
  const herdrVersion = await runCommand([process.env.HERDR_BIN_PATH || "herdr", "--version"], "/");
  if (herdrVersion === null) {
    return [{ level: "warn", title: "herdr not found", fix: "nudge notify works without herdr; the plugin and replies need https://herdr.dev" }];
  }
  const plugin = await pluginInfo();
  if (plugin === undefined) {
    return [{ level: "warn", title: `${herdrVersion}, but its server isn't reachable`, fix: "start herdr, then run nudge doctor again" }];
  }
  if (plugin === null) {
    return [{ level: "fail", title: "herdr plugin not installed", detail: herdrVersion, fix: "herdr plugin install weskor/nudge" }];
  }
  const checks: Check[] = [
    {
      level: plugin.enabled ? "ok" : "fail",
      title: `herdr plugin ${PLUGIN_ID} ${plugin.version} ${plugin.enabled ? "enabled" : "disabled"}`,
      detail: plugin.plugin_root,
      fix: plugin.enabled ? undefined : `herdr plugin enable ${PLUGIN_ID}`,
    },
  ];
  const binary = join(plugin.plugin_root, "dist", "nudge");
  checks.push(
    existsSync(binary)
      ? { level: "ok", title: "Plugin runs the prebuilt binary", detail: binary }
      : { level: "ok", title: "Plugin runs from source with Bun", detail: "no dist/nudge (linked checkout)" },
  );
  const auto = await updateState((state) => state.auto);
  checks.push(
    auto
      ? { level: "ok", title: "Auto-notify on", detail: "blocked, or done after 2+ minutes while you're not watching" }
      : { level: "warn", title: "Auto-notify off", fix: `herdr plugin action invoke ${PLUGIN_ID}.enable` },
  );
  const recent = (await pluginLogs()).filter((log) => log.stderr.trim() !== "").slice(-5);
  for (const log of recent) {
    const when = new Date(log.started_unix_ms).toLocaleTimeString();
    const line = log.stderr.trim().split("\n").at(-1)!.replace(/^\[nudge\] /, "");
    checks.push({ level: log.status === "succeeded" ? "ok" : "warn", title: `Recent: ${line}`, detail: when });
  }
  return checks;
}

async function checkCommand(): Promise<Check[]> {
  const onPath = Bun.which("nudge");
  if (onPath) return [{ level: "ok", title: "nudge command on PATH", detail: onPath }];
  const link = await commandLink();
  if (link.exists && process.env.HERDR_PLUGIN_ID) {
    // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin; agents' shells don't.
    return [{ level: "ok", title: "nudge command installed", detail: link.path }];
  }
  return [
    {
      level: "warn",
      title: "nudge command not on PATH",
      detail: link.exists ? `${link.path} exists, but ~/.local/bin isn't on PATH` : undefined,
      fix: link.exists ? "add ~/.local/bin to PATH" : "nudge setup offers to install it (agents need it for nudge notify)",
    },
  ];
}

async function checkClipboard(): Promise<Check[]> {
  const found = CLIPBOARD_TOOLS.filter((tool) => Bun.which(tool));
  return [
    found.length > 0
      ? { level: "ok", title: `Clipboard: ${found.join(", ")}`, detail: "used by nudge setup" }
      : { level: "warn", title: "No clipboard tool", detail: "nudge setup falls back to typing values", fix: "install wl-clipboard or xclip" },
  ];
}

export async function doctor(options: { ping: boolean; pane: boolean }): Promise<number> {
  console.log(style.bold("nudge doctor"));
  console.log(style.dim(`nudge at ${nudgeRoot()}, state in ${stateDir()}`));
  console.log();
  const checks = [
    ...(await checkConfig(options.ping)),
    ...(await checkPlugin()),
    ...(await checkCommand()),
    ...(await checkClipboard()),
  ];
  checks.forEach(print);
  const failures = checks.filter((c) => c.level === "fail").length;
  console.log();
  console.log(failures === 0 ? `${mark.ok} All good.` : `${mark.fail} ${failures} problem${failures === 1 ? "" : "s"} found.`);
  if (options.pane) await waitForKey();
  return failures === 0 ? 0 : 1;
}
