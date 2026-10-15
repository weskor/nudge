import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readlinkSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { linkCommand, refreshCommandLink } from "../src/core/install.ts";

function home() {
  const dir = mkdtempSync(join(tmpdir(), "nudge-home-"));
  mkdirSync(join(dir, ".local", "bin"), { recursive: true });
  return { env: { HOME: dir }, link: join(dir, ".local", "bin", "nudge") };
}

describe("linkCommand", () => {
  test("creates ~/.local/bin/nudge pointing at <root>/bin/nudge, replacing an old symlink", async () => {
    const { env, link } = home();
    symlinkSync("/somewhere/else", link);
    await linkCommand("/plugins/nudge-v2", env);
    expect(readlinkSync(link)).toBe("/plugins/nudge-v2/bin/nudge");
  });

  test("refuses to replace a regular file", async () => {
    const { env, link } = home();
    writeFileSync(link, "#!/bin/sh\n");
    await expect(linkCommand("/plugins/nudge", env)).rejects.toThrow(/not a symlink/);
  });
});

describe("refreshCommandLink (plugin startup after an update)", () => {
  test("repoints a link into an older nudge install", async () => {
    const { env, link } = home();
    symlinkSync("/plugins/nudge-v1/bin/nudge", link);
    expect(await refreshCommandLink("/plugins/nudge-v2", env)).toBe(true);
    expect(readlinkSync(link)).toBe("/plugins/nudge-v2/bin/nudge");
  });

  test("leaves current, foreign and missing links alone", async () => {
    const current = home();
    symlinkSync("/plugins/nudge-v2/bin/nudge", current.link);
    expect(await refreshCommandLink("/plugins/nudge-v2", current.env)).toBe(false);

    const foreign = home();
    symlinkSync("/usr/local/bin/some-other-nudge", foreign.link);
    expect(await refreshCommandLink("/plugins/nudge-v2", foreign.env)).toBe(false);
    expect(readlinkSync(foreign.link)).toBe("/usr/local/bin/some-other-nudge");

    expect(await refreshCommandLink("/plugins/nudge-v2", home().env)).toBe(false);
  });
});
