#!/usr/bin/env bun
import { parseArgs } from "node:util";
import { ConfigError, defaultDeskName, loadConfig } from "./core/config.ts";
import { DeskError, NOTIFY_STATUSES, notifyDesk, pingDesk, type Sent } from "./core/notify.ts";
import { doctor } from "./doctor.ts";
import { runMcp } from "./mcp.ts";
import { runPlugin } from "./plugin.ts";
import { setup, SetupError } from "./setup.ts";
import { VERSION } from "./version.ts";

const USAGE = `nudge ${VERSION}: tell your desk when a herdr agent is done, blocked or has a question

Usage:
  nudge setup  [--desk NAME] [--url URL] [--authorization VALUE] [--description TEXT] [--default] [--no-test]
  nudge notify [--desk NAME] [--status done|blocked|question] [--agent NAME] <message...|->
  nudge test   [--desk NAME] [--agent NAME]
  nudge doctor [--ping]
  nudge list   [--json]
  nudge mcp

  setup    Connect a desk: copy its Webhook URL and Authorization header from the
           webhook settings when asked (clipboard, or paste/type). The header is never
           shown. nudge test-pings the webhook before saving and asks again if it's
           rejected, then offers the herdr plugin's auto-notify and the nudge command.
           Without a terminal: values from flags or stdin (URL, then Authorization).
  notify   Send {from_agent, cwd, repo, branch, status, message, reply_hint} to the
           desk's webhook. Message "-" reads stdin. --status defaults to done.
  test     Send a status "test" ping.
  doctor   Check config, webhook (--ping sends a test ping), herdr plugin,
           auto-notify, the nudge command and clipboard tools.
  list     Show configured desks (* = default).
  mcp      Run the stdio MCP server (tools notify_desk, list_desks).

  --agent  herdr agent the desk should reply to. Default: $NUDGE_AGENT, else the
           agent in this herdr pane, else the only agent working in this directory.

Exit codes: 0 ok, 1 error, 2 usage.
Config: $NUDGE_CONFIG or ~/.config/nudge/config.toml`;

const EXIT = { ok: 0, error: 1, usage: 2 } as const;

class UsageError extends Error {}

function reportSent(verb: string, sent: Sent): void {
  const { from_agent, status } = sent.notification;
  console.log(`${verb} desk "${sent.desk}" (status ${status}, HTTP ${sent.http_status}) as agent ${from_agent ?? "<none>"}.`);
  if (from_agent === null) {
    console.error(
      "nudge: note: not sent from a herdr agent pane, so the desk can't reply directly. Pass --agent NAME or set NUDGE_AGENT.",
    );
  }
}

function noArgs(rest: string[], command: string): void {
  if (rest.length > 0) throw new UsageError(`${command} takes no arguments`);
}

async function main(argv: string[]): Promise<number> {
  const [first, ...afterFirst] = argv;
  // herdr plugin entrypoints parse their own arguments.
  if (first === "plugin") return runPlugin(afterFirst);

  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      desk: { type: "string" },
      status: { type: "string" },
      agent: { type: "string" },
      url: { type: "string" },
      authorization: { type: "string" },
      description: { type: "string" },
      default: { type: "boolean", default: false },
      "no-test": { type: "boolean", default: false },
      ping: { type: "boolean", default: false },
      pane: { type: "boolean", default: false },
      json: { type: "boolean", default: false },
      help: { type: "boolean", short: "h", default: false },
      version: { type: "boolean", short: "v", default: false },
    },
  });
  const [command, ...rest] = positionals;
  if (values.version || command === "version") {
    console.log(VERSION);
    return EXIT.ok;
  }
  if (values.help || command === undefined || command === "help") {
    console.log(USAGE);
    return command === undefined && !values.help ? EXIT.usage : EXIT.ok;
  }

  switch (command) {
    case "setup": {
      noArgs(rest, "setup");
      return setup({ ...values, test: !values["no-test"] });
    }

    case "notify": {
      if (rest.length === 0) throw new UsageError("usage: nudge notify [--desk NAME] [--status STATUS] <message...|->");
      const status = NOTIFY_STATUSES.find((s) => s === (values.status ?? "done"));
      if (status === undefined) throw new UsageError(`--status must be one of: ${NOTIFY_STATUSES.join(", ")}`);
      const message = rest.length === 1 && rest[0] === "-" ? await Bun.stdin.text() : rest.join(" ");
      const config = await loadConfig();
      reportSent("Notified", await notifyDesk(config, { desk: values.desk, status, message, agent: values.agent }));
      return EXIT.ok;
    }

    case "test": {
      noArgs(rest, "test");
      const config = await loadConfig();
      reportSent("Pinged", await pingDesk(config, { desk: values.desk, agent: values.agent }));
      return EXIT.ok;
    }

    case "doctor": {
      noArgs(rest, "doctor");
      return doctor({ ping: values.ping, pane: values.pane });
    }

    case "list": {
      noArgs(rest, "list");
      const config = await loadConfig();
      const defaultName = defaultDeskName(config);
      const desks = config.desk.map(({ name, description }) => ({
        name,
        default: name === defaultName,
        description: description ?? "",
      }));
      if (values.json) {
        console.log(JSON.stringify(desks));
      } else {
        const width = Math.max(...desks.map((d) => d.name.length));
        for (const desk of desks) {
          console.log(`${desk.default ? "*" : " "} ${desk.name.padEnd(width)}  ${desk.description}`.trimEnd());
        }
      }
      return EXIT.ok;
    }

    case "mcp": {
      noArgs(rest, "mcp");
      await runMcp();
      return EXIT.ok;
    }

    default:
      throw new UsageError(`unknown command "${command}"`);
  }
}

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  const argError = error instanceof TypeError && "code" in error && String(error.code).startsWith("ERR_PARSE_ARGS");
  if (error instanceof UsageError || argError) {
    console.error(`nudge: ${error.message}\nRun "nudge --help" for usage.`);
    process.exitCode = EXIT.usage;
  } else if (error instanceof ConfigError || error instanceof DeskError || error instanceof SetupError) {
    console.error(`nudge: ${error.message}`);
    process.exitCode = EXIT.error;
  } else {
    throw error;
  }
}
