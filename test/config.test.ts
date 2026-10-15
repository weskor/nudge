import { describe, expect, test } from "bun:test";
import { mkdtemp, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  checkAuthorization,
  checkWebhookUrl,
  ConfigError,
  defaultConfigPath,
  findDesk,
  loadConfig,
  parseConfig,
  readConfig,
  serializeConfig,
  upsertDesk,
  validateDesk,
  writeConfig,
  type Config,
} from "../src/core/config.ts";

const VALID = `
default_desk = "bridge"

[[desk]]
name = "bridge"
description = "Coordinator desk"
url = "https://desks.example/hook/bridge"
authorization = "Bearer desk-token"

[[desk]]
name = "research"
url = "https://desks.example/hook/research"
authorization = "Token xyz"
`;

const bridge = { name: "bridge", url: "https://desks.example/hook/bridge", authorization: "Bearer a" };
const research = { name: "research", url: "https://desks.example/hook/research", authorization: "Bearer b" };

async function tempDir(): Promise<string> {
  return mkdtemp(join(tmpdir(), "nudge-"));
}

describe("parseConfig", () => {
  test("parses desks and default_desk", () => {
    const config = parseConfig(VALID, "cfg");
    expect(config.default_desk).toBe("bridge");
    expect(config.desk.map((d) => d.name)).toEqual(["bridge", "research"]);
    expect(config.desk[1]!.description).toBeUndefined();
  });

  test("the shipped example is rejected until placeholders are filled in", async () => {
    const example = await Bun.file(join(import.meta.dir, "..", "config.example.toml")).text();
    expect(() => parseConfig(example, "example")).toThrow(/placeholder/);
  });

  test("http is accepted only for loopback hosts", () => {
    expect(() => parseConfig(VALID.replace("https://desks.example/hook/bridge", "http://127.0.0.1:8788/x"), "cfg")).not.toThrow();
    expect(() => parseConfig(VALID.replace("https://desks.example/hook/bridge", "http://desks.example/x"), "cfg")).toThrow(
      /desk\.0\.url: must be an https URL/,
    );
  });

  test.each([
    ["no desks", 'default_desk = "bridge"', /desk/],
    ["duplicate desk names", VALID.replace('name = "research"', 'name = "bridge"'), /duplicate desk name "bridge"/],
    ["unknown default_desk", VALID.replace('default_desk = "bridge"', 'default_desk = "nope"'), /default_desk: no \[\[desk\]\] named "nope"/],
    ["multi-line authorization", VALID.replace('"Token xyz"', '"Token x\\ny"'), /desk\.1\.authorization/],
    ["bad desk name", VALID.replace('name = "research"', 'name = "re search"'), /desk\.1\.name/],
    ["invalid TOML", "[[desk]", /invalid TOML/],
  ])("rejects %s", (_label, text, message) => {
    expect(() => parseConfig(text, "cfg")).toThrow(message);
    expect(() => parseConfig(text, "cfg")).toThrow(ConfigError);
  });
});

test("defaultConfigPath precedence: NUDGE_CONFIG > XDG_CONFIG_HOME > ~/.config", () => {
  expect(defaultConfigPath({ NUDGE_CONFIG: "/x/c.toml", XDG_CONFIG_HOME: "/xdg" })).toBe("/x/c.toml");
  expect(defaultConfigPath({ XDG_CONFIG_HOME: "/xdg" })).toBe("/xdg/nudge/config.toml");
  expect(defaultConfigPath({})).toEndWith("/.config/nudge/config.toml");
});

test("missing config: readConfig is undefined, loadConfig points at setup", async () => {
  const path = join(await tempDir(), "nope.toml");
  expect(await readConfig(path)).toBeUndefined();
  await expect(loadConfig(path)).rejects.toThrow(/nudge setup/);
});

describe("findDesk", () => {
  const config = parseConfig(VALID, "cfg");

  test("explicit name, else default_desk", () => {
    expect(findDesk(config, "research").name).toBe("research");
    expect(findDesk(config).name).toBe("bridge");
  });

  test("a single desk is the default even without default_desk", () => {
    expect(findDesk({ desk: [research] }).name).toBe("research");
  });

  test("several desks and no default requires a name", () => {
    expect(() => findDesk({ desk: [bridge, research] })).toThrow(/no default_desk; pass --desk/);
  });

  test("unknown names list configured desks", () => {
    expect(() => findDesk(config, "nope")).toThrow("Configured desks: bridge, research");
  });
});

describe("validateDesk", () => {
  test('strips a pasted "Authorization:" header name and whitespace', () => {
    const desk = validateDesk({ ...bridge, url: " https://desks.example/x ", authorization: "Authorization: Bearer abc \n" });
    expect(desk.authorization).toBe("Bearer abc");
    expect(desk.url).toBe("https://desks.example/x");
  });

  test("reports every problem", () => {
    expect(() => validateDesk({ name: "bridge", url: "not a url", authorization: "" })).toThrow(
      /url: must be an https URL[\s\S]*authorization: must be a non-empty string/,
    );
  });

  test("field checks for setup prompts never echo the rejected value", () => {
    expect(checkWebhookUrl(" https://desks.example/hook\n")).toBe("https://desks.example/hook");
    expect(checkAuthorization("authorization: Bearer abc")).toBe("Bearer abc");
    for (const [check, value] of [
      [checkWebhookUrl, "http://secret-host.example/x"],
      [checkAuthorization, "Bearer super-secret\nsecond-line"],
      [checkAuthorization, "Bearer s3cr\u00e9t"],
    ] as const) {
      const error = (() => {
        try {
          check(value);
        } catch (e) {
          return e;
        }
      })();
      expect(error).toBeInstanceOf(ConfigError);
      expect(String(error)).not.toContain("secret");
      expect(String(error)).not.toContain("s3cr");
    }
  });
});

describe("upsertDesk", () => {
  test("first desk becomes the default", () => {
    expect(upsertDesk(undefined, bridge, false)).toEqual({ default_desk: "bridge", desk: [bridge] });
  });

  test("adding a second desk keeps the default unless asked", () => {
    const one: Config = { default_desk: "bridge", desk: [bridge] };
    expect(upsertDesk(one, research, false)).toEqual({ default_desk: "bridge", desk: [bridge, research] });
    expect(upsertDesk(one, research, true).default_desk).toBe("research");
  });

  test("re-running for an existing name replaces it in place", () => {
    const two: Config = { default_desk: "research", desk: [bridge, research] };
    const updated = { ...bridge, authorization: "Bearer rotated" };
    expect(upsertDesk(two, updated, false)).toEqual({ default_desk: "research", desk: [updated, research] });
  });
});

describe("writeConfig", () => {
  test("writes mode 600 into a mode 700 directory and round-trips", async () => {
    const dir = join(await tempDir(), "nudge");
    const path = join(dir, "config.toml");
    const config: Config = {
      default_desk: "bridge",
      desk: [{ ...bridge, description: 'Says "hi"\\ and\tmore' }, research],
    };
    await writeConfig(path, config);
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    expect((await stat(dir)).mode & 0o777).toBe(0o700);
    expect(await loadConfig(path)).toEqual(config);
  });

  test("tightens an existing world-readable file", async () => {
    const path = join(await tempDir(), "config.toml");
    await writeFile(path, VALID, { mode: 0o644 });
    await writeConfig(path, upsertDesk(await loadConfig(path), research, false));
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    expect((await loadConfig(path)).desk.map((d) => d.name)).toEqual(["bridge", "research"]);
  });

  test("serialized output omits absent optional fields", () => {
    const text = serializeConfig({ desk: [bridge] });
    expect(text).not.toContain("default_desk");
    expect(text).not.toContain("description");
  });
});
