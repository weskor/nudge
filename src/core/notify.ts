import { findDesk, type Config, type DeskConfig } from "./config.ts";
import { detectAgent, gitInfo, runCommand, type Run } from "./context.ts";
import { updateState } from "./state.ts";
import { VERSION } from "../version.ts";

export const NOTIFY_STATUSES = ["done", "blocked", "question"] as const;
export type NotifyStatus = (typeof NOTIFY_STATUSES)[number];

export class DeskError extends Error {
  override name = "DeskError";
  /** HTTP status when the webhook answered with an error; undefined for network failures and input errors. */
  constructor(
    message: string,
    readonly httpStatus?: number,
  ) {
    super(message);
  }

  /** Which setup value the failure points at: the Authorization header (401/403), the URL (404/405/410), or neither. */
  get blames(): "authorization" | "url" | undefined {
    if (this.httpStatus === 401 || this.httpStatus === 403) return "authorization";
    if (this.httpStatus === 404 || this.httpStatus === 405 || this.httpStatus === 410) return "url";
    return undefined;
  }
}

/** The JSON body POSTed to a desk webhook; the desk sees it inside <webhook_event>. */
export interface Notification {
  /** herdr agent target the desk can reply to; null if it could not be detected. */
  from_agent: string | null;
  cwd: string;
  repo: string | null;
  branch: string | null;
  /** `test` is only used by `nudge test` pings. */
  status: NotifyStatus | "test";
  message: string;
  reply_hint: string;
}

/** Where the notification is sent from. Injectable for tests. */
export interface Origin {
  env: Record<string, string | undefined>;
  cwd: string;
  run: Run;
}

export interface Deps {
  origin: Origin;
  fetch: (url: string, init: RequestInit) => Promise<Response>;
}

export const defaultDeps = (): Deps => ({
  origin: { env: process.env, cwd: process.cwd(), run: runCommand },
  fetch: (url, init) => fetch(url, init),
});

const WEBHOOK_TIMEOUT_MS = 15_000;

export function replyHint(agent: string | null): string {
  if (agent === null) return "sender's herdr agent could not be detected, so it cannot be prompted directly";
  const target = /^[A-Za-z0-9._:@%+=,-]+$/.test(agent) ? agent : `'${agent.replaceAll("'", `'\\''`)}'`;
  return `reply with: herdr agent prompt ${target} "..."`;
}

export async function buildNotification(
  input: { status: Notification["status"]; message: string; agent?: string },
  origin: Origin,
): Promise<Notification> {
  const message = input.message.trim();
  if (message === "") throw new DeskError("message is empty");
  const [agent, git] = await Promise.all([
    input.agent ?? detectAgent(origin.env, origin.cwd, origin.run),
    gitInfo(origin.cwd, origin.run),
  ]);
  return {
    from_agent: agent,
    cwd: origin.cwd,
    repo: git.repo,
    branch: git.branch,
    status: input.status,
    message,
    reply_hint: replyHint(agent),
  };
}

/** POST `payload` to the desk webhook with its Authorization header; returns the 2xx status. Throws DeskError on failure. */
export async function postToDesk(desk: DeskConfig, payload: Notification, fetchFn: Deps["fetch"]): Promise<number> {
  let response: Response;
  try {
    response = await fetchFn(desk.url, {
      method: "POST",
      headers: { authorization: desk.authorization, "content-type": "application/json", "user-agent": `nudge/${VERSION}` },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(WEBHOOK_TIMEOUT_MS),
    });
  } catch (error) {
    throw new DeskError(`could not reach desk "${desk.name}" webhook: ${(error as Error).message}`);
  }
  if (!response.ok) {
    const body = (await response.text().catch(() => "")).trim();
    const detail = body.length > 300 ? `${body.slice(0, 300)}…` : body;
    throw new DeskError(
      `desk "${desk.name}" webhook returned HTTP ${response.status}${detail ? `: ${detail}` : ""}`,
      response.status,
    );
  }
  return response.status;
}

export interface Sent {
  desk: string;
  /** HTTP status the webhook answered with. */
  http_status: number;
  notification: Notification;
}

/**
 * Tell a desk (default desk unless `desk` is given) that this agent is done, blocked or has a question.
 * Inside a herdr pane, records that the agent notified, so the herdr plugin doesn't send a duplicate.
 */
export async function notifyDesk(
  config: Config,
  input: { desk?: string; status: NotifyStatus; message: string; agent?: string },
  deps: Deps = defaultDeps(),
): Promise<Sent> {
  const desk = findDesk(config, input.desk);
  const notification = await buildNotification(input, deps.origin);
  const http_status = await postToDesk(desk, notification, deps.fetch);
  const paneId = deps.origin.env.HERDR_PANE_ID;
  if (paneId) {
    const now = Date.now();
    await updateState((state) => {
      (state.panes[paneId] ??= {}).self_notified_at = now;
    }, deps.origin.env).catch(() => {
      // Best effort: a missing record only risks one duplicate automatic notification.
    });
  }
  return { desk: desk.name, http_status, notification };
}

/** Send a harmless `status: "test"` notification to check the webhook URL and Authorization header. */
export async function pingDesk(
  config: Config,
  input: { desk?: string; agent?: string },
  deps: Deps = defaultDeps(),
): Promise<Sent> {
  const desk = findDesk(config, input.desk);
  const notification = await buildNotification(
    { status: "test", message: "nudge test ping. No action needed.", agent: input.agent },
    deps.origin,
  );
  const http_status = await postToDesk(desk, notification, deps.fetch);
  return { desk: desk.name, http_status, notification };
}
