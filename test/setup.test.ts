import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test";
import type { Server } from "bun";
import { mkdtempSync } from "node:fs";
import { stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readConfig } from "../src/core/config.ts";
import { DeskError } from "../src/core/notify.ts";
import { setup, SetupError } from "../src/setup.ts";
import { maskSecret } from "../src/tty.ts";

describe("maskSecret", () => {
  test("keeps the scheme and the last 4 characters of long secrets", () => {
    expect(maskSecret("Bearer abcdefghijklmnopqrstuvwxyz")).toBe("Bearer ••••••••wxyz");
    expect(maskSecret("abcdefghijklmnopqrstuvwxyz")).toBe("••••••••wxyz");
  });

  test("reveals nothing of short secrets", () => {
    expect(maskSecret("Bearer short-token-1")).toBe("Bearer ••••••••");
    expect(maskSecret("Bearer 123456789012345")).toBe("Bearer ••••••••");
    expect(maskSecret("Bearer 1234567890123456")).toBe("Bearer ••••••••3456");
  });
});

test("DeskError.blames points at the Authorization header or the URL", () => {
  expect(new DeskError("x", 401).blames).toBe("authorization");
  expect(new DeskError("x", 403).blames).toBe("authorization");
  expect(new DeskError("x", 404).blames).toBe("url");
  expect(new DeskError("x", 500).blames).toBeUndefined();
  expect(new DeskError("network").blames).toBeUndefined();
});

describe("setup without a terminal", () => {
  const GOOD = "Bearer good-token-0123456789";
  const pings: string[] = [];
  let server: Server<undefined>;
  let url: string;
  const configPath = () => process.env.NUDGE_CONFIG!;

  beforeAll(() => {
    server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      fetch(req) {
        pings.push(req.headers.get("authorization") ?? "");
        if (new URL(req.url).pathname !== "/hook") return new Response("no such hook", { status: 404 });
        return req.headers.get("authorization") === GOOD
          ? new Response("ok", { status: 200 })
          : Response.json({ message: "Invalid API key" }, { status: 401 });
      },
    });
    url = `http://127.0.0.1:${server.port}/hook`;
  });
  afterAll(() => server.stop(true));
  afterEach(() => {
    pings.length = 0;
    process.env.NUDGE_CONFIG = join(mkdtempSync(join(tmpdir(), "nudge-setup-")), "config.toml");
  });
  process.env.NUDGE_CONFIG = join(mkdtempSync(join(tmpdir(), "nudge-setup-")), "config.toml");

  const base = { default: false, test: true, pane: false };

  test("tests the webhook before saving; saves mode 600 on success", async () => {
    expect(await setup({ ...base, url, authorization: GOOD })).toBe(0);
    expect(pings).toEqual([GOOD]);
    expect((await readConfig(configPath()))?.desk[0]).toMatchObject({ name: "desk", url, authorization: GOOD });
    expect((await stat(configPath())).mode & 0o777).toBe(0o600);
  });

  test("a rejected Authorization header saves nothing and says why", async () => {
    const failure = setup({ ...base, url, authorization: "Bearer wrong-token-0123456789" });
    await expect(failure).rejects.toThrow(SetupError);
    await expect(failure).rejects.toThrow(/rejected the Authorization header.*nothing saved|nothing saved.*rejected the Authorization header/);
    expect(await readConfig(configPath())).toBeUndefined();
  });

  test("a wrong URL is reported as such", async () => {
    await expect(setup({ ...base, url: url.replace("/hook", "/nope"), authorization: GOOD })).rejects.toThrow(
      /no webhook at that URL/,
    );
  });

  test("--no-test saves without contacting the webhook", async () => {
    expect(await setup({ ...base, test: false, url, authorization: "Bearer offline-token-0123456789" })).toBe(0);
    expect(pings).toEqual([]);
    expect((await readConfig(configPath()))?.desk).toHaveLength(1);
  });

  test("errors never contain the Authorization value", async () => {
    const secret = "Bearer do-not-print-0123456789";
    const error = await setup({ ...base, url, authorization: secret }).catch((e) => e);
    expect(String(error)).not.toContain("do-not-print");
  });
});
