// The Nudge panel: shows and changes nudge's settings on this computer or a connected machine,
// only by running the `nudge` CLI there. It never reads nudge's files and never sees a URL or secret.
// The same mod draws its popups as overlays (`props().popup`, see popup-view.js).
import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import {
  agents, terminals, spaces, machines, focused, window as appWindow, hc, notify, openOverlay,
  nextCommand, watchFolder, home, theme, props, shared,
} from "herdr";
import * as R from "./rows.js";
import { sendOption, sendToast, serviceToast, startToast, switchToast } from "./popup.js";
import { nudge, bump, initPopup, renderPopup } from "./popup-view.js";

const key = (m) => m ?? "";

export default class Nudge extends View {
  init(_props, cx) {
    if (props()?.popup) return initPopup(this, cx);
    this.states = {}; // per machine ("" is this computer): readStatus() of its `nudge status --json`
    this.sent = {}; // the last `nudge core context` sent, per machine
    this.shown = undefined; // the machine and bump the panel last loaded
    this.testNote = null;
    cx.spawn(async (cx) => {
      for (;;) {
        try { await watchFolder(`${home()}/.local/state/nudge`); } catch { await cx.sleep(10000); }
        await this.load(cx, null);
      }
    });
    cx.spawn(async (cx) => {
      for (;;) await this.command(await nextCommand(), cx);
    });
    cx.timer.every(10000, (cx) => {
      const m = this.picked();
      if (m !== null) cx.spawn((cx) => this.load(cx, m));
      this.pushContext();
    });
  }

  async close() {
    if (props()?.popup) return;
    await Promise.all(R.clearPlan(this.sent).map((s) => nudge(["core", "context", s.json], s.machine)));
  }

  picked() {
    const m = shared("nudge.machine") ?? null;
    return R.machineList(machines()).includes(m) ? m : null;
  }

  status(m) {
    const state = this.states[key(m)];
    return state?.kind === "ok" ? state.status : null;
  }

  async load(cx, m) {
    this.states[key(m)] = R.readStatus(await nudge(["status", "--json"], m));
    cx.notify();
  }

  // Focus and spaces for each machine's watcher: `nudge core context` to the machines that need it.
  pushContext() {
    const ready = machines().filter((m) => m.name !== null && m.layout === "ready").map((m) => m.name);
    const plan = R.contextPlan({
      sent: this.sent, machines: [null, ...ready], focused: focused(), windowActive: appWindow()?.active ?? false,
      spaces: spaces(), now: Date.now(),
    });
    this.sent = plan.sent;
    for (const s of plan.sends) nudge(["core", "context", s.json], s.machine);
  }

  async command(name, cx) {
    if (name === "toggle") {
      if (!this.status(null)) await this.load(cx, null);
      return this.flip(cx, null, "on");
    }
    if (name === "open") return this.load(cx, this.picked());
    if (name === "remove-service") return this.removeService(cx);
    const address = focused()?.terminal;
    if (!address || (name !== "send" && name !== "route")) return;
    const m = R.splitAddress(address).machine;
    await this.load(cx, m);
    const status = this.status(m);
    if (!status) return;
    const row = R.terminalRow(address, agents(), terminals(), status);
    if (!row) return;
    if (name === "route") return this.popup("route", { routeFor: row, status, machine: m });
    notify(sendToast(await nudge(sendOption(row).argv, m), row));
  }

  // Optimistic: the row flips at once and flips back if nudge says no (the switch also toasts why).
  flip(cx, m, field) {
    const s = this.status(m);
    if (!s) return;
    const want = !s[field];
    s[field] = want;
    cx.notify();
    cx.spawn(async (cx) => {
      const args = field === "on" ? [want ? "on" : "off"] : [field, want ? "on" : "off"];
      const result = await nudge(args, m);
      if (result?.code !== 0) {
        s[field] = !want;
        if (field === "on") notify(switchToast(result, want));
      }
      cx.notify();
    });
  }

  popup(kind, extra) {
    openOverlay({ mod: "nudge", kind: "popup", width: 260, height: 320, props: { popup: kind, ...extra } });
  }

  // `hc run --bg` only says the terminal started; the watcher can still stop at once (an old nudge on the
  // core's PATH, no config), so look again and say why when it isn't running.
  startWatcher(cx, m) {
    cx.spawn(async (cx) => {
      const result = await hc(R.watchStartArgs(m)).catch(() => null);
      await cx.sleep(2500);
      await this.load(cx, m);
      if (!this.status(m)?.core?.watcher?.running) notify(startToast(result, m));
    });
  }

  async removeService(cx) {
    const m = this.picked();
    notify(serviceToast(await nudge(["core", "service", "remove"], m), m));
    await this.load(cx, m);
  }

  test(cx, m, status) {
    this.testNote = "sending…";
    cx.notify();
    cx.spawn(async (cx) => {
      const result = await nudge(["test"], m);
      this.testNote = result?.code === 0 ? "✓ sent" : null;
      if (result?.code !== 0) notify(sendToast(result, { label: "test", webhook: status.default_webhook }));
      cx.notify();
    });
  }

  render(cx) {
    if (props()?.popup) return renderPopup(this);
    this.pushContext();
    const m = this.picked();
    const seen = `${key(m)}|${shared("nudge.bump") ?? ""}`;
    if (seen !== this.shown) {
      if (!this.shown?.startsWith(`${key(m)}|`)) this.testNote = null;
      this.shown = seen;
      cx.spawn((cx) => this.load(cx, m));
    }
    const t = theme();
    const state = this.states[key(m)];
    const root = v_flex().id("nudge").size_full().min_h(0).bg(t.chrome).text_color(t.foreground).text_size(12.5);
    if (!state) return root.child(this.top(t, m, null)).child(this.line(t, "Loading…", t.dim));
    const head = R.header(state, m);
    if (state.kind !== "ok") return root.child(this.top(t, m, null)).child(this.line(t, head.text, t.muted));
    const s = state.status;
    const agentList = R.agentRows(agents(), terminals(), s, m);
    return root
      .child(this.top(t, m, s))
      .when(head.kind === "off", (el) => el.child(this.line(t, head.text, t.muted)))
      .when(Boolean(s.config_error), (el) => el.child(this.line(t, `Config: ${s.config_error}`, t.blocked)))
      .child(this.toggleRow(t, "auto", "Auto-notify", s.auto, (cx) => this.flip(cx, m, "auto")))
      .child(this.toggleRow(t, "away", "I'm away", s.away, (cx) => this.flip(cx, m, "away")))
      .child(this.watching(t, m, s))
      .child(v_flex().id("nudge-body").flex_1().min_h(0).overflow_y_scrollbar()
        .child(this.section(t, "Agents"))
        .children(this.rowsOrEmpty(t, agentList.map((r) => this.routeRow(t, r, s)), "No agents"))
        .child(this.section(t, "Workspaces"))
        .children(this.rowsOrEmpty(t, R.spaceRows(spaces(), s, m).map((r) => this.routeRow(t, r, s)), "No workspaces"))
        .child(this.section(t, "Recent"))
        .children(this.rowsOrEmpty(t, R.recentRows(s, agentList).map((d, i) => this.recent(t, d, i)), "Nothing yet")))
      .child(this.footer(t, m, s));
  }

  top(t, m, s) {
    const on = s?.on;
    return h_flex().h(40).flex_none().px(10).gap(8).items_center().border_b_1().border_color(t.border)
      .child(this.button(t, "machine", `${R.machineLabel(m)} ▾`, () => this.popup("machines", { machine: m })))
      .child(div().flex_1())
      .when(Boolean(s), (el) => el.child(h_flex().id("switch").h(24).px(10).gap(6).items_center().rounded(12)
        .cursor_pointer().bg(on ? t.solid : t.raised).text_color(on ? t.solid_foreground : t.muted)
        .border_1().border_color(on ? t.solid : t.input)
        .on_click((_e, cx) => this.flip(cx, m, "on"))
        .child(on ? "● On" : "○ Off")));
  }

  line(t, text, color) {
    return div().px(12).py(6).text_size(12).text_color(color).child(text);
  }

  button(t, id, label, onClick) {
    return h_flex().id(id).h(24).px(8).flex_none().items_center().rounded(6).border_1().border_color(t.input)
      .bg(t.raised).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => onClick(cx)).child(label);
  }

  toggleRow(t, id, label, value, onClick) {
    return h_flex().h(28).flex_none().px(12).items_center()
      .child(div().flex_1().child(label))
      .child(h_flex().id(`toggle-${id}`).h(20).w(40).justify_center().items_center().rounded(10).cursor_pointer()
        .bg(value ? t.accent : t.raised).text_color(value ? t.accent_foreground : t.muted).text_size(11)
        .on_click((_e, cx) => onClick(cx)).child(value ? "on" : "off"));
  }

  watching(t, m, s) {
    const w = R.watcherLine(s);
    return h_flex().h(28).flex_none().px(12).gap(8).items_center()
      .child(div().text_color(t.dim).child("Watching"))
      .child(div().flex_1().min_w(0).truncate().text_color(w.running ? t.working : t.muted).child(w.text))
      .when(w.start, (el) => el.child(this.button(t, "start", "Start", (cx) => this.startWatcher(cx, m))));
  }

  section(t, title) {
    return div().px(12).pt(10).pb(4).mt(2).border_t_1().border_color(t.border).text_size(11).text_color(t.dim).child(title);
  }

  rowsOrEmpty(t, rows, empty) {
    return rows.length ? rows : [this.line(t, empty, t.dim)];
  }

  routeRow(t, r, s) {
    const id = `${r.kind}-${r.address ?? r.id}`;
    return h_flex().id(id).h(26).px(12).gap(6).items_center().hover((el) => el.bg(t.hover))
      .when(r.kind === "agent", (el) => el.child(div().w(12).flex_none().text_color(t[r.status] ?? t.unknown).child(r.glyph)))
      .child(div().w(r.kind === "agent" ? 92 : 104).flex_none().truncate().child(r.label))
      .when(r.kind === "agent", (el) => el.child(div().w(52).flex_none().text_size(11).text_color(t.muted).child(r.status)))
      .child(div().flex_1().min_w(0).truncate().text_size(11.5).text_color(t.muted).child(R.routeLabel(r)))
      .child(div().id(`${id}-more`).px(4).rounded(4).cursor_pointer().text_color(t.dim).hover((el) => el.bg(t.raised))
        .on_click((_e, cx) => { cx.stop_propagation(); this.popup("route", { routeFor: r, status: s, machine: r.machine }); })
        .child("⋯"));
  }

  recent(t, d, i) {
    return h_flex().id(`recent-${i}`).h(24).px(12).gap(8).items_center().text_size(11.5)
      .child(div().flex_none().text_color(t.dim).child(new Date(d.at).toTimeString().slice(0, 5)))
      .child(div().flex_1().min_w(0).truncate().child(d.text))
      .child(div().flex_none().text_color(t.muted).child(d.outcome));
  }

  footer(t, m, s) {
    const n = (s.webhooks ?? []).length;
    return h_flex().h(40).flex_none().px(10).gap(6).items_center().border_t_1().border_color(t.border).text_size(12)
      .child(div().text_color(t.dim).child("Default webhook:"))
      .child(this.button(t, "default", `${s.default_webhook ?? "none"} ⋯`, () => this.popup("default", { status: s, machine: m })))
      .child(div().flex_1().min_w(0).truncate().text_color(t.dim).child(`· ${n} ${n === 1 ? "webhook" : "webhooks"}`))
      .when(Boolean(this.testNote), (el) => el.child(div().flex_none().text_size(11).text_color(t.muted).child(this.testNote)))
      .child(this.button(t, "test", "Test", (cx) => this.test(cx, m, s)))
      .child(this.button(t, "menu", "⋯", () => this.popup("footer", { status: s, machine: m })));
  }
}
