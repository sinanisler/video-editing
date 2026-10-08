import { runCommand, splitAt } from "../commands";
import { exec, ui } from "../store";
import type { MenuItem } from "../store";
import type { Clip } from "../types";
import { clipEnd } from "../util";
import { openMenu } from "./ContextMenu";
import { TRACK_COLOURS } from "./Tracks";

/** Options for one clip. Clicking a clip selects it first, as Resolve does, so the commands act on it. */
export function clipMenu(c: Clip): MenuItem[] {
  const frame = ui.get().frame;
  const under = frame > c.start && frame < clipEnd(c);
  const select = () => ui.set({ selection: [c.id] });
  // Runs a shortcut command on this clip alone.
  const shortcut = (id: string) => () => {
    select();
    runCommand(id);
  };
  const items: MenuItem[] = [
    { label: "Copy", hint: "Ctrl+C", action: shortcut("copy") },
    { label: "Paste at playhead", hint: "Ctrl+V", action: shortcut("paste") },
    { label: "Duplicate", hint: "Ctrl+D", action: shortcut("duplicate") },
    "sep",
    {
      label: "Split at playhead",
      hint: "Ctrl+B",
      disabled: !under,
      action: () => {
        select();
        splitAt(frame, c.id);
      },
    },
    "sep",
    { label: "Delete", hint: "Del", danger: true, action: shortcut("delete") },
    { label: "Ripple delete", hint: "Shift+Del", danger: true, action: shortcut("ripple") },
    "sep",
    {
      label: "Clip colour",
      swatches: [null, ...TRACK_COLOURS],
      current: ui.get().clipColors[c.id],
      onPick: (colour) => {
        const next = { ...ui.get().clipColors };
        if (colour) next[c.id] = colour;
        else delete next[c.id];
        ui.set({ clipColors: next });
      },
    },
    "sep",
    {
      label: "Show in Inspector",
      action: () => {
        select();
        ui.set({ rightTab: "inspector" });
      },
    },
  ];
  if (c.transition_in) {
    items.push({ label: "Remove transition at start", action: () => exec("clip.set_transition", { clip: c.id, edge: "in", transition: null }) });
  }
  if (c.transition_out) {
    items.push({ label: "Remove transition at end", action: () => exec("clip.set_transition", { clip: c.id, edge: "out", transition: null }) });
  }
  return items;
}

export function openClipMenu(e: React.MouseEvent, c: Clip) {
  e.preventDefault();
  e.stopPropagation();
  if (!ui.get().selection.includes(c.id)) ui.set({ selection: [c.id] });
  openMenu(e.clientX, e.clientY, clipMenu(c));
}
