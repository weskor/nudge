import { chmod, mkdir, rename, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { z } from "zod";

export class ConfigError extends Error {
  override name = "ConfigError";
}

const PLACEHOLDER_PREFIX = "REPLACE_WITH";
const LOOPBACK_HOSTS = ["localhost", "127.0.0.1", "[::1]"];

const NonEmpty = z
  .string()
  .trim()
  .min(1, "must be a non-empty string")
  .refine((value) => !value.includes(PLACEHOLDER_PREFIX), `still contains a ${PLACEHOLDER_PREFIX}… placeholder`);

const DeskName = NonEmpty.regex(/^[A-Za-z0-9][A-Za-z0-9_-]*$/, 'may only contain letters, digits, "-" and "_"');

const WebhookUrl = NonEmpty.refine((value) => {
  if (!URL.canParse(value)) return false;
  const url = new URL(value);
  return url.protocol === "https:" || (url.protocol === "http:" && LOOPBACK_HOSTS.includes(url.hostname));
}, "must be an https URL (http is allowed only for localhost)");

/** Sent verbatim as the webhook's Authorization header value. A pasted "Authorization:" prefix is stripped. */
const AuthorizationValue = z
  .string()
  .transform((value) => value.trim().replace(/^authorization:\s*/i, ""))
  .pipe(NonEmpty.regex(/^[\x20-\x7e]+$/, "must be printable ASCII on a single line"));

const Desk = z.object({
  name: DeskName,
  description: z.string().optional(),
  url: WebhookUrl,
  authorization: AuthorizationValue,
});
export type DeskConfig = z.infer<typeof Desk>;

const Config = z
  .object({
    default_desk: DeskName.optional(),
    desk: z
      .array(Desk)
      .min(1, "define at least one [[desk]] table")
      .superRefine((desks, ctx) => {
        const seen = new Set<string>();
        desks.forEach((desk, index) => {
          if (seen.has(desk.name)) {
            ctx.addIssue({ code: "custom", path: [index, "name"], message: `duplicate desk name "${desk.name}"` });
          }
          seen.add(desk.name);
        });
      }),
  })
  .superRefine((config, ctx) => {
    if (config.default_desk !== undefined && !config.desk.some((d) => d.name === config.default_desk)) {
      ctx.addIssue({ code: "custom", path: ["default_desk"], message: `no [[desk]] named "${config.default_desk}"` });
    }
  });
export type Config = z.infer<typeof Config>;

/**
 * Validate `raw` against `schema`, throwing a ConfigError that lists every problem.
 * Messages never include the input value, so secrets are not echoed.
 */
function parseOrThrow<T>(schema: z.ZodType<T>, raw: unknown, label: string): T {
  const parsed = schema.safeParse(raw);
  if (parsed.success) return parsed.data;
  const problems = parsed.error.issues.map((issue) =>
    issue.path.length > 0 ? `  ${issue.path.join(".")}: ${issue.message}` : `  ${issue.message}`,
  );
  throw new ConfigError(`${label}\n${problems.join("\n")}`);
}

/** Validate a webhook URL entered during setup. */
export function checkWebhookUrl(value: string): string {
  return parseOrThrow(WebhookUrl, value, "invalid webhook URL");
}

/** Validate an Authorization header value entered during setup (strips a pasted "Authorization:" prefix). */
export function checkAuthorization(value: string): string {
  return parseOrThrow(AuthorizationValue, value, "invalid Authorization value");
}

export function defaultConfigPath(env: Record<string, string | undefined> = process.env): string {
  if (env.NUDGE_CONFIG) return env.NUDGE_CONFIG;
  const base = env.XDG_CONFIG_HOME || join(homedir(), ".config");
  return join(base, "nudge", "config.toml");
}

/** Parse and validate config TOML. `source` names the file in error messages. */
export function parseConfig(text: string, source: string): Config {
  let raw: unknown;
  try {
    raw = Bun.TOML.parse(text);
  } catch (error) {
    throw new ConfigError(`${source}: invalid TOML: ${(error as Error).message}`);
  }
  return parseOrThrow(Config, raw, `${source}: invalid config`);
}

/** Load the config, or `undefined` if the file does not exist. */
export async function readConfig(path: string = defaultConfigPath()): Promise<Config | undefined> {
  const file = Bun.file(path);
  if (!(await file.exists())) return undefined;
  return parseConfig(await file.text(), path);
}

export async function loadConfig(path: string = defaultConfigPath()): Promise<Config> {
  const config = await readConfig(path);
  if (!config) throw new ConfigError(`no config at ${path}. Run "nudge setup" to add a desk.`);
  return config;
}

/** `default_desk`, else the only desk's name; undefined when several desks and no default. */
export function defaultDeskName(config: Config): string | undefined {
  return config.default_desk ?? (config.desk.length === 1 ? config.desk[0]!.name : undefined);
}

/** Pick the desk to notify: `name` if given, else the default desk. */
export function findDesk(config: Config, name?: string): DeskConfig {
  const known = config.desk.map((d) => d.name).join(", ");
  const wanted = name ?? defaultDeskName(config);
  if (wanted === undefined) {
    throw new ConfigError(`several desks configured (${known}) and no default_desk; pass --desk`);
  }
  const desk = config.desk.find((d) => d.name === wanted);
  if (!desk) throw new ConfigError(`unknown desk "${wanted}". Configured desks: ${known}`);
  return desk;
}

/** Validate one desk entered via `nudge setup`. */
export function validateDesk(input: { name: string; url: string; authorization: string; description?: string }): DeskConfig {
  return parseOrThrow(Desk, input, "invalid desk");
}

/** Add or replace `desk` by name. The first desk, or any desk with `makeDefault`, becomes default_desk. */
export function upsertDesk(config: Config | undefined, desk: DeskConfig, makeDefault: boolean): Config {
  const desks = config?.desk ?? [];
  const index = desks.findIndex((d) => d.name === desk.name);
  const next = index === -1 ? [...desks, desk] : desks.map((d, i) => (i === index ? desk : d));
  const defaultDesk = makeDefault || config?.default_desk === undefined ? desk.name : config.default_desk;
  return { default_desk: defaultDesk, desk: next };
}

export function serializeConfig(config: Config): string {
  // JSON string escapes are a subset of TOML basic-string escapes.
  const str = JSON.stringify;
  const lines = [
    "# nudge config, written by `nudge setup`.",
    "# Contains webhook secrets: keep it chmod 600 and out of git.",
    "",
  ];
  if (config.default_desk !== undefined) lines.push(`default_desk = ${str(config.default_desk)}`, "");
  for (const desk of config.desk) {
    lines.push("[[desk]]", `name = ${str(desk.name)}`);
    if (desk.description !== undefined) lines.push(`description = ${str(desk.description)}`);
    lines.push(`url = ${str(desk.url)}`, `authorization = ${str(desk.authorization)}`, "");
  }
  return lines.join("\n");
}

/** Atomically write the config with mode 600 (directory 700). */
export async function writeConfig(path: string, config: Config): Promise<void> {
  const text = serializeConfig(config);
  parseConfig(text, path); // never write a file we can't read back
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  const temp = `${path}.${process.pid}.tmp`;
  await writeFile(temp, text, { mode: 0o600 });
  await chmod(temp, 0o600);
  await rename(temp, path);
}
