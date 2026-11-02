// The panel's popups, drawn as an overlay of this same mod (`openOverlay({mod: "nudge", props})`).
// `props().popup` says which: "route" (with `routeFor`), "default", "footer" or "machines".
// The options and the argv they run come from popup.js; this file only draws and runs them.
import { div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { exec, notify, props, closeOverlay, openOverlay, setShared, run, spaces, machines, home, theme } from "herdr";
import { routeOptions, defaultOptions, footerOptions, machineOptions, sendToast, SSH_NOTE } from "./popup.js";
import { machineList, readStatus, tabCommand } from "./rows.js";

/** Runs nudge there; null when it can't start (not installed, machine unreachable). */
export async function nudge(args, on) {
  try {
    return await exec("nudge", args, { on });
  } catch {
    return null;
  }
}

/** Tells the panel to read status again (every mod re-renders on a shared change). */
export function bump() {
  setShared("nudge.bump", Date.now());
}

/** The machine picker asks each machine for its status, so its notes are current. */
export function initPopup(view, cx) {
  if (props().popup !== "machines") return;
  view.states = {};
  for (const m of machineList(machines())) {
    cx.spawn(async (cx) => {
      view.states[m ?? ""] = readStatus(await nudge(["status", "--json"], m));
      cx.notify();
    });
  }
}

function options(view) {
  const p = props();
  if (p.popup === "route") return { title: p.routeFor.label, items: routeOptions(p.status, p.routeFor) };
  if (p.popup === "default") return { title: "Default webhook", items: defaultOptions(p.status, p.machine) };
  if (p.popup === "footer") return { title: "Nudge", items: footerOptions(p.status, p.machine) };
  return { title: "Machine", items: machineOptions(machineList(machines()), view.states ?? {}, p.machine), note: SSH_NOTE };
}

// Closing the overlay ends its tasks, so it closes after the command ran (exec has a 30 s limit).
async function choose(item, cx) {
  const p = props();
  if (item.machine !== undefined) setShared("nudge.machine", item.machine);
  else if (item.action === "tab") {
    const c = tabCommand(spaces(), item.on, item.argv, "nudge");
    run(c.name, c.args);
  } else if (item.action === "folder") cx.open_url(`file://${home()}/.config/nudge`);
  else if (item.action === "confirm") {
    openOverlay({ mod: "confirm", kind: "dialog", modal: true, width: 380, height: 170, props: item.confirm });
  }
  else if (item.argv) {
    const result = await nudge(item.argv, item.on);
    if (item.action === "send") notify(sendToast(result, p.routeFor));
    bump();
  }
  closeOverlay(p.overlay);
}

export function renderPopup(view) {
  const t = theme();
  const { title, items, note } = options(view);
  return v_flex().id("nudge-popup").w_full().p(5).bg(t.popover).text_size(12.8).text_color(t.foreground)
    .child(div().px(8).pt(4).pb(6).text_size(11).text_color(t.dim).truncate().child(title))
    .children(items.map((it) => h_flex().id(`opt-${it.id}`).h(30).px(8).gap(8).rounded(7).items_center()
      .cursor_pointer().hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => cx.spawn((cx) => choose(it, cx)))
      .child(div().w(12).flex_none().text_color(t.accent).child(it.checked ? "✓" : ""))
      .child(div().flex_1().min_w(0).truncate().child(it.label))
      .when(Boolean(it.hint), (el) => el.child(div().flex_none().text_size(10.5).text_color(t.dim).child(it.hint)))
      .when(Boolean(it.note), (el) => el.child(div().flex_none().text_size(11).text_color(t.muted).child(it.note)))))
    .when(Boolean(note), (el) => el.child(div().mt(4).px(8).pt(6).pb(4).border_t_1().border_color(t.border)
      .text_size(11).text_color(t.dim).child(note)));
}
