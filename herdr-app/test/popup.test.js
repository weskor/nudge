import { describe, expect, test } from "bun:test";
import * as P from "../mods/nudge/popup.js";
import * as R from "../mods/nudge/rows.js";
import laptop from "./fixtures/status-laptop.json";
import box from "./fixtures/status-box.json";
import app from "./fixtures/app.json";

const agent = (status, machine, local) => R.agentRows(app.agents, app.terminals, status, machine).find((r) => r.local === local);
const space = (status, machine, local) => R.spaceRows(app.spaces, status, machine).find((r) => r.local === local);
const pick = (options, id) => options.find((o) => o.id === id);

describe("the route popup on an agent row", () => {
  const options = P.routeOptions(laptop, agent(laptop, null, "t3"));

  test("lists the webhooks by name with the pane's own route ticked", () => {
    expect(options.map((o) => [o.label, o.checked])).toEqual([
      ["main", false],
      ["client-a", true],
      ["Use workspace / default", false],
      ["Send this pane now", false],
    ]);
  });

  test("each option runs the exact nudge command on this computer", () => {
    expect(pick(options, "hook-main")).toMatchObject({ on: null, argv: ["route", "set", "--pane", "api-refactor", "--webhook", "main"] });
    expect(pick(options, "inherit").argv).toEqual(["route", "rm", "--pane", "api-refactor"]);
    expect(pick(options, "send").argv).toEqual(["core", "send", "t3", "--blocked"]);
  });

  test("an agent without a name is routed by id, with the hint; with no route, inherit is ticked and does nothing", () => {
    const opts = P.routeOptions(laptop, agent(laptop, null, "t9"));
    expect(pick(opts, "hook-client-a")).toMatchObject({ hint: P.BY_ID_HINT, argv: ["route", "set", "--pane", "t9", "--webhook", "client-a"] });
    expect(pick(opts, "inherit")).toMatchObject({ checked: true, argv: null });
    expect(pick(opts, "send").argv).toEqual(["core", "send", "t9"]);
  });

  test("on a connected machine, commands run there with the address on that machine", () => {
    const opts = P.routeOptions(box, agent(box, "box", "t7"));
    expect(opts.every((o) => o.on === "box")).toBe(true);
    expect(pick(opts, "hook-ops").argv).toEqual(["route", "set", "--pane", "trainer", "--webhook", "ops"]);
    expect(pick(opts, "send").argv).toEqual(["core", "send", "t7"]);
  });
});

describe("the route popup on a workspace row", () => {
  test("keeps the route's backend qualifier when replacing or removing it, and offers no send", () => {
    const opts = P.routeOptions(laptop, space(laptop, null, "s1"));
    expect(opts.map((o) => [o.label, o.checked])).toEqual([["main", false], ["client-a", true], ["Use default", false]]);
    expect(pick(opts, "hook-main").argv).toEqual(["route", "set", "--workspace", "client-a", "--webhook", "main", "--backend", "core"]);
    expect(pick(opts, "inherit").argv).toEqual(["route", "rm", "--workspace", "client-a", "--backend", "core"]);
  });

  test("a workspace without a route is written by its name", () => {
    const opts = P.routeOptions(laptop, space(laptop, null, "s2"));
    expect(pick(opts, "hook-client-a")).toMatchObject({ hint: undefined, argv: ["route", "set", "--workspace", "nudge", "--webhook", "client-a"] });
    expect(pick(opts, "inherit").checked).toBe(true);
  });
});

describe("the other popups", () => {
  test("the default webhook popup ticks the default and runs nudge default <name>", () => {
    expect(P.defaultOptions(laptop, null).map((o) => [o.label, o.checked, o.argv])).toEqual([
      ["main", true, ["default", "main"]],
      ["client-a", false, ["default", "client-a"]],
    ]);
  });

  test("the footer menu: setup and settings in a tab, the service, and the config folder only here", () => {
    expect(P.footerOptions(laptop, null).map((o) => [o.label, o.action, o.argv])).toEqual([
      ["Add a webhook…", "tab", ["nudge", "setup"]],
      ["Edit settings…", "tab", ["nudge", "settings"]],
      ["Install service…", "tab", ["nudge", "core", "service", "install"]],
      ["Open config folder", "folder", null],
    ]);
    const remote = P.footerOptions(box, "box");
    expect(remote.map((o) => o.label)).toEqual(["Add a webhook…", "Edit settings…", "Remove service…"]);
    expect(remote[2]).toMatchObject({ on: "box", action: "confirm", argv: null });
  });

  test("Remove service asks first, naming the machine, and only then runs the panel's command", () => {
    const { confirm } = P.footerOptions(box, "box")[2];
    expect(confirm.title).toBe("Remove the login service?");
    expect(confirm.detail).toContain("box's terminals");
    expect(confirm.command).toEqual({ name: "nudge.remove-service" });
    expect(P.removeServiceConfirm(null).detail).toContain("this computer's terminals");
  });

  test("the machine picker ticks the picked machine and says where nudge is missing", () => {
    const states = { "": { kind: "ok", status: laptop }, box: { kind: "ok", status: box }, "gpu-vm": R.readStatus(null) };
    expect(P.machineOptions(R.machineList(app.machines), states, "box").map((o) => [o.label, o.checked, o.note])).toEqual([
      ["This computer", false, "● on"],
      ["box", true, "● on · watching"],
      ["gpu-vm", false, "○ nudge not found"],
    ]);
  });
});

describe("toasts", () => {
  const row = agent(laptop, null, "t3");

  test("send: sent, nudge off, or couldn't reach the webhook", () => {
    expect(P.sendToast({ code: 0, stdout: "", stderr: "" }, row)).toMatchObject({ level: "success", title: "Sent api-refactor" });
    const off = { code: 1, stdout: "", stderr: "nudge: nudge is off: nothing was sent. Turn it on with nudge on\n" };
    expect(P.sendToast(off, row)).toEqual({ level: "warning", title: "Nudge is off: nothing was sent" });
    const failed = P.sendToast({ code: 1, stdout: "", stderr: "nudge: POST https://hooks.example.com/x failed\n" }, row);
    expect(failed).toEqual({ level: "error", title: "nudge couldn't reach client-a", action: { name: "nudge.open" } });
  });

  test("a failed switch says why, without any URL", () => {
    const t = P.switchToast({ code: 1, stdout: "", stderr: "nudge: cannot write https://example.com/secret\n" }, false);
    expect(t.title).toBe("Couldn't turn nudge off");
    expect(t.body).not.toContain("https://");
    expect(P.switchToast(null, true).body).toBe("nudge not found");
  });

  test("Start that leaves no watcher says why: hc's error, or where to read the stopped terminal", () => {
    const started = { code: 0, stdout: '{"address":"t5","name":"nudge-watch","state":"live"}', stderr: "" };
    expect(P.startToast(started, null).body).toBe("It stopped right away; hc read t5 shows why");
    expect(P.startToast(started, "box").body).toBe("It stopped right away; hc read t5 --on box shows why");
    const remote = { code: 0, stdout: '{"address":"box/t3","name":"nudge-watch"}', stderr: "" };
    expect(P.startToast(remote, "box").body).toBe("It stopped right away; hc read box/t3 shows why");
    expect(P.startToast({ code: 1, stdout: "", stderr: "hc: no such machine\n" }, "box").body).toBe("hc: no such machine");
    expect(P.startToast(null, null).body).toBe("hc not found");
  });

  test("removing the service: done, or why not", () => {
    expect(P.serviceToast({ code: 0, stdout: "", stderr: "" }, "box")).toEqual({ level: "success", title: "Removed the login service on box" });
    const failed = P.serviceToast({ code: 1, stdout: "", stderr: "nudge: `systemctl --user disable --now nudge-watch.service` failed\n" }, null);
    expect(failed).toMatchObject({ level: "error", title: "Couldn't remove the login service" });
    expect(failed.body).toContain("systemctl");
  });
});
