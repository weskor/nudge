import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { clipboardCommands, readClipboard } from "../src/core/clipboard.ts";
import type { Run } from "../src/core/context.ts";

/** Fake runner keyed by the joined argv; unmapped commands fail like a missing binary. */
function fakeRun(outputs: Record<string, string>): Run & { tried: string[] } {
  const tried: string[] = [];
  const run = async (cmd: string[]) => {
    const key = cmd.join(" ");
    tried.push(key);
    return outputs[key] ?? null;
  };
  return Object.assign(run, { tried });
}

const WL = "wl-paste --no-newline --type text";
const XC = "xclip -selection clipboard -o";
const desktop = { WAYLAND_DISPLAY: "wayland-1", XDG_RUNTIME_DIR: "/run/user/1000", DISPLAY: ":0" };
const emptyRuntime = () => mkdtempSync(join(tmpdir(), "nudge-run-"));

test("prefers wl-paste in the current session", async () => {
  const run = fakeRun({ [WL]: "https://desk/hook", [XC]: "other" });
  expect(await readClipboard(run, desktop, emptyRuntime())).toBe("https://desk/hook");
  expect(run.tried).toEqual([WL]);
});

test("falls back to xclip when wl-paste fails or is empty", async () => {
  expect(await readClipboard(fakeRun({ [XC]: "Bearer abc" }), desktop, emptyRuntime())).toBe("Bearer abc");
  expect(await readClipboard(fakeRun({ [WL]: "", [XC]: "Bearer abc" }), desktop, emptyRuntime())).toBe("Bearer abc");
});

test("from a herdr plugin pane (no session env) it finds the Wayland socket in the runtime dir", async () => {
  const runtime = emptyRuntime();
  writeFileSync(join(runtime, "wayland-1"), "");
  writeFileSync(join(runtime, "wayland-1.lock"), "");
  const run = fakeRun({ [`env XDG_RUNTIME_DIR=${runtime} WAYLAND_DISPLAY=wayland-1 ${WL}`]: "https://desk/hook" });
  expect(await readClipboard(run, {}, runtime)).toBe("https://desk/hook");
});

test("null when nothing yields text (caller falls back to manual input)", async () => {
  const run = fakeRun({});
  expect(await readClipboard(run, desktop, emptyRuntime())).toBeNull();
  expect(run.tried).toEqual([WL, XC]);
});

test("clipboardCommands adds session fallbacks only when the env lacks them", () => {
  expect(clipboardCommands(desktop, "/run/user/1000", ["wayland-1"]).map((c) => c.join(" "))).toEqual([WL, XC]);
  expect(clipboardCommands({}, "/run/user/1000", ["wayland-0", "wayland-1"]).map((c) => c.join(" "))).toEqual([
    WL,
    `env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 ${WL}`,
    `env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 ${WL}`,
    XC,
    `env DISPLAY=:0 ${XC}`,
  ]);
});
