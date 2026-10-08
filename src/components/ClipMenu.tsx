import { linkClips, runCommand, splitAt, unlinkClips } from "../commands";
import { commitProps, exec, ui } from "../store";
import type { MenuItem } from "../store";
import type { Clip, Project } from "../types";
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
  const linkItems: MenuItem[] = [];
  if (c.link) linkItems.push({ label: "Unlink clips", hint: "Ctrl+L", action: () => { select(); unlinkClips([c.id]); } });
  if (ui.get().selection.length > 1) linkItems.push({ label: "Link selected clips", hint: "Ctrl+L", action: () => linkClips(ui.get().selection) });
  if (linkItems.length) items.push("sep", ...linkItems);
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

/** Transform keys a reset can clear. Null resets a key to its default, as the Inspector does. */
const POSITION = ["pos_x", "pos_y"];
const ZOOM = ["zoom_x", "zoom_y"];
const ANCHOR = ["anchor_x", "anchor_y"];
const CROP = ["crop_left", "crop_right", "crop_top", "crop_bottom"];
const ALL_TRANSFORM = [...POSITION, ...ZOOM, ...ANCHOR, ...CROP, "rotation", "pitch", "yaw", "flip_h", "flip_v"];

const resetKeys = (keys: string[]) => Object.fromEntries(keys.map((k) => [k, null]));

/**
 * Right-click options for a picture in the preview, after Resolve's Inspector transform controls: reset
 * each group, fit or fill the frame, flip and rotate.
 */
export function transformMenu(c: Clip, p: Project): MenuItem[] {
  const num = (key: string, fallback: number) => (typeof c.props[key] === "number" ? (c.props[key] as number) : fallback);
  const flipH = c.props.flip_h === true;
  const flipV = c.props.flip_v === true;
  const rotation = num("rotation", 0);
  const m = p.media.find((x) => x.id === c.media);
  const { width: W, height: H } = p.settings;
  // Zoom at which the picture just covers the frame: the contained size scaled up to the frame's larger side.
  const cover = (() => {
    if (!m || !m.width || !m.height) return null;
    const contain = Math.min(W / m.width, H / m.height);
    return Math.max(W / (m.width * contain), H / (m.height * contain));
  })();
  return [
    { label: "Reset position", action: () => commitProps(c.id, resetKeys(POSITION)) },
    { label: "Reset zoom", action: () => commitProps(c.id, resetKeys(ZOOM)) },
    { label: "Reset rotation", action: () => commitProps(c.id, resetKeys(["rotation"])) },
    { label: "Reset anchor point", action: () => commitProps(c.id, resetKeys(ANCHOR)) },
    { label: "Reset crop", action: () => commitProps(c.id, resetKeys(CROP)) },
    { label: "Reset all transform", danger: true, action: () => commitProps(c.id, resetKeys(ALL_TRANSFORM)) },
    "sep",
    { label: "Fit to frame", action: () => commitProps(c.id, { zoom_x: 1, zoom_y: 1, pos_x: 0, pos_y: 0 }) },
    {
      label: "Fill frame",
      disabled: cover === null,
      action: () => cover !== null && commitProps(c.id, { zoom_x: cover, zoom_y: cover, pos_x: 0, pos_y: 0 }),
    },
    "sep",
    { label: "Flip horizontal", checked: flipH, action: () => commitProps(c.id, { flip_h: !flipH }) },
    { label: "Flip vertical", checked: flipV, action: () => commitProps(c.id, { flip_v: !flipV }) },
    {
      label: "Rotate 90° clockwise",
      action: () => {
        let next = rotation + 90;
        if (next > 180) next -= 360;
        commitProps(c.id, { rotation: next });
      },
    },
    "sep",
    {
      label: "Show in Inspector",
      action: () => {
        ui.set({ selection: [c.id], rightTab: "inspector" });
      },
    },
  ];
}

/** The menu for a right-click on a picture in the preview. Right-clicks elsewhere on the stage do nothing. */
export function openTransformMenu(e: React.MouseEvent, p: Project) {
  const hit = (e.target as HTMLElement).closest<HTMLElement>("[data-hit]");
  const c = hit && p.sequence.clips.find((x) => x.id === hit.dataset.hit);
  if (!c) return;
  e.preventDefault();
  e.stopPropagation();
  ui.set({ selection: [c.id] });
  openMenu(e.clientX, e.clientY, transformMenu(c, p));
}
