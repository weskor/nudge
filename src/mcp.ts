// Thin stdio MCP server over the nudge core: notify_desk, list_desks. Started by `nudge mcp`.
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { ConfigError, defaultDeskName, loadConfig } from "./core/config.ts";
import { DeskError, NOTIFY_STATUSES, notifyDesk } from "./core/notify.ts";
import { VERSION } from "./version.ts";

type ToolResult = { content: { type: "text"; text: string }[]; isError?: boolean };

/** Run a tool body; config/webhook failures become MCP tool errors instead of protocol errors. */
async function guarded(body: () => Promise<unknown>): Promise<ToolResult> {
  try {
    return { content: [{ type: "text", text: JSON.stringify(await body(), null, 2) }] };
  } catch (error) {
    if (error instanceof ConfigError || error instanceof DeskError) {
      return { content: [{ type: "text", text: error.message }], isError: true };
    }
    throw error;
  }
}

const server = new McpServer({ name: "nudge", version: VERSION });

server.registerTool(
  "list_desks",
  {
    description: "List the desks notify_desk can reach, and which one is the default.",
    inputSchema: {},
  },
  () =>
    guarded(async () => {
      const config = await loadConfig();
      const defaultName = defaultDeskName(config);
      return config.desk.map(({ name, description }) => ({
        name,
        default: name === defaultName,
        description: description ?? "",
      }));
    }),
);

server.registerTool(
  "notify_desk",
  {
    description:
      "Notify your desk (the webhook automation that coordinates your agents) that you finished, are blocked, " +
      "or have a question. Sends your herdr agent name, cwd, repo and branch with the message; the desk replies " +
      "later by prompting you through herdr, so do not wait or poll for an answer.",
    inputSchema: {
      message: z.string().min(1).describe("What you did, or exactly what you need. Self-contained."),
      status: z.enum(NOTIFY_STATUSES).default("done").describe("done, blocked, or question."),
      desk: z.string().optional().describe("Desk name from list_desks; omit for the default desk."),
      agent: z.string().optional().describe("Your herdr agent name, if auto-detection would get it wrong."),
    },
  },
  ({ message, status, desk, agent }) =>
    guarded(async () => {
      const config = await loadConfig();
      const sent = await notifyDesk(config, { desk, status, message, agent });
      return { ok: true, desk: sent.desk, http_status: sent.http_status, ...sent.notification };
    }),
);

/** Serve over stdio until the client disconnects (`nudge mcp`). */
export async function runMcp(): Promise<void> {
  await server.connect(new StdioServerTransport());
}
