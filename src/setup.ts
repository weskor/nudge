// `nudge setup`: connect a desk (clipboard-first), test it before saving, then offer the herdr
// plugin's auto-notify and the `nudge` command on PATH.
import { readClipboard } from "./core/clipboard.ts";
import {
  checkAuthorization,
  checkWebhookUrl,
  ConfigError,
  defaultConfigPath,
  readConfig,
  upsertDesk,
  validateDesk,
  writeConfig,
  type DeskConfig,
} from "./core/config.ts";
import { commandLink, commandLinkPath, linkCommand, nudgeRoot, pluginInfo } from "./core/install.ts";
import { DeskError, pingDesk } from "./core/notify.ts";
import { updateState } from "./core/state.ts";
import { Cancelled, confirm, mark, maskSecret, promptLine, say, style, waitForKey } from "./tty.ts";

export class SetupError extends Error {
  override name = "SetupError";
}

export interface SetupOptions {
  desk?: string;
  url?: string;
  authorization?: string;
  description?: string;
  default: boolean;
  test: boolean;
  /** Running inside a herdr plugin pane: wait for a key before exiting. */
  pane: boolean;
}

interface Field {
  label: string;
  hidden: boolean;
  check: (value: string) => string;
  current?: string;
}

const urlField = (current?: string): Field => ({ label: "Webhook URL", hidden: false, check: checkWebhookUrl, current });

const authorizationField = (current?: string): Field => ({
  label: "Authorization header",
  hidden: true,
  current,
  check: (value) => {
    const checked = checkAuthorization(value);
    if (/^https?:\/\//i.test(checked)) {
      throw new ConfigError("that's a URL, not the Authorization header. Copy the header value from your desk's webhook settings.");
    }
    return checked;
  },
});

const preview = (field: Field, value: string) =>
  field.hidden ? `${maskSecret(value)} ${style.dim(`(${value.length} characters)`)}` : value;

/**
 * Ask for one value, clipboard-first: Enter on an empty prompt reads the clipboard; typed or pasted
 * text wins. Without clipboard text, ask for manual entry (Enter keeps the current value).
 * Loops until the value validates. Hidden values are never echoed.
 */
async function capture(step: string, field: Field): Promise<string> {
  for (;;) {
    let value = await promptLine(`${step} Copy the ${field.label} from your desk's webhook settings, then press Enter`, field.hidden);
    if (value === "") {
      const clipboard = await readClipboard();
      if (clipboard !== null) {
        value = clipboard;
      } else {
        const keep = field.current ? ", or Enter to keep the current one" : "";
        value = await promptLine(`    No clipboard text found. Paste or type the ${field.label}${keep}`, field.hidden);
        if (value === "" && field.current) value = field.current;
      }
    }
    try {
      const checked = field.check(value);
      say(`    ${mark.ok} ${preview(field, checked)}`);
      return checked;
    } catch (error) {
      if (!(error instanceof ConfigError)) throw error;
      say(`    ${mark.fail} ${error.message.replace(/^invalid [^\n]*\n\s*/, "")}`);
    }
  }
}

/** Plain-language reason for a failed test ping. */
function explain(error: DeskError): string {
  const detail = error.message.replace(/^desk "[^"]*" webhook /, "");
  switch (error.blames) {
    case "authorization":
      return `the webhook rejected the Authorization header (${detail})`;
    case "url":
      return `there's no webhook at that URL (${detail})`;
    default:
      return detail;
  }
}

async function testDesk(desk: DeskConfig): Promise<DeskError | null> {
  process.stderr.write(`    Testing… `);
  try {
    const sent = await pingDesk({ desk: [desk] }, { desk: desk.name });
    say(`${mark.ok} HTTP ${sent.http_status}: the desk received a test ping`);
    return null;
  } catch (error) {
    if (!(error instanceof DeskError)) throw error;
    say(`${mark.fail} ${explain(error)}`);
    return error;
  }
}

/** Offer the herdr plugin's auto-notify and the `nudge` command on PATH. Interactive only. */
async function offerExtras(): Promise<void> {
  say();
  const plugin = await pluginInfo();
  if (plugin === undefined) {
    say(`${mark.warn} herdr isn't running, so the plugin step was skipped. Run ${style.bold("nudge setup")} again inside herdr.`);
  } else if (plugin === null) {
    say(`${mark.warn} The herdr plugin isn't installed: ${style.bold(`herdr plugin install weskor/nudge`)}`);
  } else {
    say(`herdr plugin ${mark.ok} ${plugin.enabled ? "enabled" : style.yellow("disabled")} ${style.dim(plugin.plugin_root)}`);
    say(style.dim("    Auto-notify sends an agent's last terminal lines (with secrets redacted) to the desk"));
    say(style.dim("    when it's blocked, or finishes a turn longer than 2 minutes while you're not watching it."));
    const auto = await confirm(`    Turn on auto-notify?`, true);
    await updateState((state) => (state.auto = auto));
    say(`    ${auto ? mark.ok : mark.warn} auto-notify ${auto ? "on" : "off"}`);
  }

  // Plugin panes get herdr's server PATH, which usually lacks ~/.local/bin, so also accept the link itself.
  const installed = Bun.which("nudge") ?? ((await commandLink()).exists ? commandLinkPath() : null);
  if (installed) {
    say(`nudge command ${mark.ok} ${installed}`);
  } else if (await confirm(`Install the ${style.bold("nudge")} command into ~/.local/bin so agents can run it?`, true)) {
    try {
      say(`    ${mark.ok} ${await linkCommand(nudgeRoot())}`);
    } catch (error) {
      say(`    ${mark.fail} ${(error as Error).message}`);
    }
  }
}

/** Interactive setup on a terminal; returns the exit code. */
async function interactive(options: SetupOptions): Promise<number> {
  const path = defaultConfigPath();
  const existing = await readConfig(path);
  const name = options.desk ?? "desk";
  const current = existing?.desk.find((d) => d.name === name);
  say(`${style.bold("nudge setup")}: ${current ? "update" : "add"} desk "${name}" ${style.dim(`(${path})`)}`);
  say();

  let url = options.url ? checkWebhookUrl(options.url) : await capture("1/2", urlField(current?.url));
  let authorization = options.authorization
    ? checkAuthorization(options.authorization)
    : await capture("2/2", authorizationField(current?.authorization));
  const description = options.description ?? current?.description;

  for (;;) {
    const desk = validateDesk({ name, url, authorization, description });
    const failure = options.test ? await testDesk(desk) : null;
    if (failure === null || !failure.blames) {
      if (failure && !(await confirm("    Save anyway?", false))) return 1;
      const config = upsertDesk(existing, desk, options.default);
      await writeConfig(path, config);
      const isDefault = config.default_desk === desk.name ? " as the default desk" : "";
      say(`    ${mark.ok} Saved "${desk.name}"${isDefault} ${style.dim(`(${path}, mode 600)`)}`);
      break;
    }
    if (failure.blames === "url") url = await capture("  ↻", urlField());
    else authorization = await capture("  ↻", authorizationField());
  }

  await offerExtras();
  say();
  say(`Done. Check everything any time with ${style.bold("nudge doctor")}.`);
  return 0;
}

/** Non-interactive setup from flags and stdin (URL, then Authorization, one per line). */
async function scripted(options: SetupOptions): Promise<number> {
  const path = defaultConfigPath();
  const existing = await readConfig(path);
  const name = options.desk ?? "desk";
  const current = existing?.desk.find((d) => d.name === name);
  let { url, authorization } = options;
  if (url === undefined || authorization === undefined) {
    const inputs = (await Bun.stdin.text())
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line !== "");
    url ??= inputs.shift();
    authorization ??= inputs.shift();
  }
  url ??= current?.url;
  authorization ??= current?.authorization;
  if (url === undefined || authorization === undefined) {
    throw new SetupError(
      "setup needs the webhook URL and Authorization value: run it in a terminal, pass --url/--authorization, or pipe them on stdin (one per line)",
    );
  }
  const desk = validateDesk({ name, url, authorization, description: options.description ?? current?.description });
  if (options.test) {
    try {
      const sent = await pingDesk({ desk: [desk] }, { desk: desk.name });
      console.log(`Test ping to "${desk.name}": HTTP ${sent.http_status}.`);
    } catch (error) {
      if (!(error instanceof DeskError)) throw error;
      throw new SetupError(`test ping failed, nothing saved: ${explain(error)}. Use --no-test to save without testing.`);
    }
  }
  const config = upsertDesk(existing, desk, options.default);
  await writeConfig(path, config);
  console.log(`Saved desk "${desk.name}"${config.default_desk === desk.name ? " (default)" : ""} to ${path} (mode 600).`);
  return 0;
}

export async function setup(options: SetupOptions): Promise<number> {
  if (!process.stdin.isTTY) return scripted(options);
  try {
    return await interactive(options);
  } catch (error) {
    if (error instanceof Cancelled) {
      say(style.dim("Cancelled; nothing saved."));
      return 1;
    }
    throw error;
  } finally {
    if (options.pane) await waitForKey();
  }
}
