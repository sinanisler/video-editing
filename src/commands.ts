import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { importFromPaths, importPaths, MEDIA_EXTENSIONS } from "./media";
import { core, exec, execBatch, openProject, project, redo, saveProject, seek, setPlaying, toast, ui, undo } from "./store";
import type { Clip } from "./types";
import { clamp, clipEnd, cutTargets, fpsOf, linkedIds, MAX_PPS, MIN_PPS, timelineFrames } from "./util";

export interface EditorCommand {
  id: string;
  title: string;
  /** Key combos such as "Ctrl+Shift+Z". The first one is shown in the UI. */
  keys?: string[];
  run: () => void;
}

/** Set by the timeline, which is the only place that knows its own geometry. */
export const timelineApi = {
  fit: () => {},
  /** The track and frame under a screen point, or null when the point is outside the timeline. */
  dropTarget: (_x: number, _y: number): { track: string | null; frame: number } | null => null,
};

const FILTERS = [{ name: "snn-editor project", extensions: ["snn"] }];

const fps = () => {
  const p = project();
  return p ? fpsOf(p) : 30;
};
const frame = () => ui.get().frame;
const allClips = (): Clip[] => project()?.sequence.clips ?? [];
/** The selected clips that still exist, with their link groups when linked selection is on. */
const selected = (): Clip[] => {
  const ids = linkedIds(allClips(), ui.get().selection, ui.get().linkedSel);
  return allClips().filter((c) => ids.includes(c.id));
};

function editPoints(): number[] {
  return [...new Set(allClips().flatMap((c) => [c.start, clipEnd(c)]))].sort((a, b) => a - b);
}

function shuttle(direction: 1 | -1) {
  const { playing, rate } = ui.get();
  const same = playing && Math.sign(rate) === direction;
  setPlaying(true, same ? clamp(rate * 2, -8, 8) : direction);
}

/** Runs something that opens a system dialog and reports failures instead of failing silently. */
async function guarded(action: () => Promise<unknown>) {
  try {
    await action();
  } catch (e) {
    toast(`Could not complete that: ${e}`);
  }
}

async function confirmDiscard(): Promise<boolean> {
  if (!core.get().state?.dirty) return true;
  return confirm("Discard unsaved changes to this project?", { title: "snn-editor", kind: "warning" });
}

export const importMedia = () =>
  guarded(async () => {
    const picked = await open({
      multiple: true,
      title: "Import media",
      filters: [{ name: "Video, audio and images", extensions: MEDIA_EXTENSIONS }],
    });
    if (picked) await importPaths(Array.isArray(picked) ? picked : [picked]);
  });

/** Pick one or more folders; every supported file inside them, at any depth, is imported. */
export const importFolders = () =>
  guarded(async () => {
    const picked = await open({ directory: true, multiple: true, title: "Import folders" });
    if (picked) await importFromPaths(Array.isArray(picked) ? picked : [picked]);
  });

async function saveAs() {
  const path = await save({ filters: FILTERS, defaultPath: `${project()?.name ?? "project"}.snn` });
  if (path && (await saveProject(path))) toast("Saved");
}

/**
 * Cuts `clipId` if given, else the selected clips under `at`, else every clip under `at`. A linked clip is cut
 * together with its group, as the razor does in Resolve and Premiere, so the parts stay in step.
 */
export function splitAt(at: number, clipId?: string) {
  const inside = (c: Clip) => at > c.start && at < clipEnd(c);
  const group = clipId ? linkedIds(allClips(), [clipId], ui.get().linkedSel) : [];
  const targets = clipId ? allClips().filter((c) => group.includes(c.id) && inside(c)) : selected().filter(inside);
  return exec("timeline.split_clip", targets.length ? { at, clips: targets.map((c) => c.id) } : { at });
}

/** The selected clips a cut at `at` would split. A linked clip's group is included, as with every selection action. */
export const selectedUnder = (at: number): Clip[] => cutTargets(selected(), project()?.sequence.tracks ?? [], at);

/** Splits only the selected clips at `at`. Unlike Ctrl+B, it never cuts unselected clips when none is selected. */
export function splitSelectedAt(at: number) {
  const targets = selectedUnder(at);
  if (!targets.length) return toast("No selected clip under the playhead");
  return exec("timeline.split_clip", { at, clips: targets.map((c) => c.id) });
}

/** Links these clips into one group, so they select and move in time together. */
export const linkClips = (ids: string[]) => exec("timeline.link", { clips: ids });
/** Breaks the links of the groups these clips belong to. Each clip then moves on its own. */
export const unlinkClips = (ids: string[]) => exec("timeline.unlink", { clips: ids });

/** Ctrl+L: unlinks the selected clips when any of them is linked, otherwise links the selected clips together. */
function linkSelection() {
  const clips = allClips().filter((c) => ui.get().selection.includes(c.id));
  if (clips.some((c) => c.link)) return unlinkClips(clips.map((c) => c.id));
  if (clips.length < 2) return toast("Select two or more clips to link them");
  return linkClips(clips.map((c) => c.id));
}

function setRange(patch: { in_point?: number | null; out_point?: number | null }) {
  const s = project()?.sequence;
  if (s) exec("timeline.set_range", { in_point: s.in_point, out_point: s.out_point, ...patch });
}

/** Runs `tool` once per selected clip, as one undo step. */
function onSelected(tool: string) {
  const clips = selected();
  if (!clips.length) return toast("Select a clip first");
  execBatch(clips.map((c) => ({ tool, args: { clip: c.id } })));
}

function nudge(delta: number) {
  const clips = selected();
  if (clips.length) exec("timeline.move_clips", { clips: clips.map((c) => c.id), delta });
}

/** Copies clips `delta` frames away and selects the copies. */
async function duplicate(clips: Clip[], delta: number) {
  const r = await exec("timeline.duplicate_clips", { clips: clips.map((c) => c.id), delta });
  if (r) ui.set({ selection: r.created as string[] });
}

let clipboard: string[] = [];

/**
 * Pastes copied clips onto the paste-target track, keeping their layout relative to it: a copy from V1 landing
 * on V2 stays on V2 and the next track up for a clip that was one track up. The copies are made beyond the end of
 * the timeline first, because a copy cannot be placed over its own source, then moved into place.
 */
async function pasteTo(target: string, copied: Clip[], delta: number) {
  const p = project();
  if (!p) return;
  const tracks = p.sequence.tracks;
  const ofKind = (kind: string) => tracks.filter((t) => t.kind === kind);
  const trackOf = (c: Clip) => tracks.find((t) => t.id === c.track)!;
  const first = copied.reduce((a, c) => (c.start < a.start ? c : a));
  const kind = trackOf(first).kind;
  const dest = tracks.find((t) => t.id === target);
  if (!dest) return;
  if (dest.kind !== kind) return toast(`The copied clips are ${kind} clips. Make a ${kind} track the paste target.`);
  const offset = ofKind(kind).findIndex((t) => t.id === target) - ofKind(kind).findIndex((t) => t.id === first.track);
  const landing = copied.map((c) => {
    const list = ofKind(trackOf(c).kind);
    return list[list.findIndex((t) => t.id === c.track) + offset];
  });
  if (landing.some((t) => !t)) return toast("Some copies would land past the last track");
  if (landing.some((t) => t!.locked)) return toast("A track the copies would land on is locked");

  const end = Math.max(0, ...allClips().map(clipEnd)) + fps();
  const made = await exec("timeline.duplicate_clips", { clips: copied.map((c) => c.id), delta: end - first.start });
  if (!made) return;
  const ids = made.created as string[];
  const moved = await execBatch(
    ids.map((id, i) => ({ tool: "timeline.move_clip", args: { clip: id, start: copied[i].start + delta, track: landing[i]!.id } })),
  );
  // Blocked by a clip already there: take the copies back out so nothing is left half pasted.
  if (!moved) await undo();
  else toast("Pasted");
}

/** Removes the time between the playhead and the nearest edit point on one side, closing the gap. */
function rippleTrim(side: "before" | "after") {
  const at = frame();
  const points = editPoints();
  const other = side === "before" ? points.filter((x) => x < at).pop() : points.find((x) => x > at);
  if (other === undefined) return toast("No edit point on that side of the playhead");
  const [from, to] = side === "before" ? [other, at] : [at, other];
  exec("timeline.ripple_delete_range", { from, to }).then((ok) => ok && seek(from));
}

export const COMMANDS: EditorCommand[] = [
  { id: "play", title: "Play / pause", keys: ["Space"], run: () => setPlaying(!ui.get().playing) },
  { id: "reverse", title: "Play backwards (press again for faster)", keys: ["J"], run: () => shuttle(-1) },
  { id: "pause", title: "Pause", keys: ["K"], run: () => setPlaying(false) },
  { id: "forward", title: "Play forwards (press again for faster)", keys: ["L"], run: () => shuttle(1) },
  { id: "back", title: "Back one frame", keys: ["Left"], run: () => seek(frame() - 1) },
  { id: "fwd", title: "Forward one frame", keys: ["Right"], run: () => seek(frame() + 1) },
  { id: "back1", title: "Back one second", keys: ["Shift+Left"], run: () => seek(frame() - fps()) },
  { id: "fwd1", title: "Forward one second", keys: ["Shift+Right"], run: () => seek(frame() + fps()) },
  { id: "prev", title: "Previous edit point", keys: ["Up"], run: () => seek(editPoints().filter((x) => x < frame()).pop() ?? 0) },
  { id: "next", title: "Next edit point", keys: ["Down"], run: () => seek(editPoints().find((x) => x > frame()) ?? frame()) },
  { id: "home", title: "Go to start", keys: ["Home"], run: () => seek(0) },
  { id: "end", title: "Go to end", keys: ["End"], run: () => seek(editPoints().pop() ?? 0) },

  { id: "select", title: "Selection tool", keys: ["V"], run: () => ui.set({ tool: "select" }) },
  { id: "razor", title: "Razor tool", keys: ["C"], run: () => ui.set({ tool: "razor" }) },
  { id: "selectall", title: "Select all clips", keys: ["Ctrl+A"], run: () => ui.set({ selection: allClips().map((c) => c.id) }) },
  { id: "deselect", title: "Deselect all", keys: ["Ctrl+Shift+A"], run: () => ui.set({ selection: [] }) },

  { id: "split", title: "Split at playhead", keys: ["Ctrl+B", "S"], run: () => splitAt(frame()) },
  { id: "splitall", title: "Split every unlocked track at playhead", keys: ["Ctrl+Shift+B"], run: () => exec("timeline.split_clip", { at: frame() }) },
  { id: "delete", title: "Delete selected clips", keys: ["Delete", "Backspace"], run: () => onSelected("timeline.delete_clip") },
  { id: "ripple", title: "Ripple delete selected clips", keys: ["Shift+Delete"], run: () => onSelected("timeline.ripple_delete_clip") },
  { id: "trimstart", title: "Ripple trim: previous edit point to playhead", keys: ["Q"], run: () => rippleTrim("before") },
  { id: "trimend", title: "Ripple trim: playhead to next edit point", keys: ["W"], run: () => rippleTrim("after") },
  {
    id: "duplicate",
    title: "Duplicate selected clips after themselves",
    keys: ["Ctrl+D"],
    run: () => {
      const clips = selected();
      if (!clips.length) return toast("Select a clip first");
      const span = Math.max(...clips.map(clipEnd)) - Math.min(...clips.map((c) => c.start));
      duplicate(clips, span);
    },
  },
  {
    id: "copy",
    title: "Copy selected clips",
    keys: ["Ctrl+C"],
    run: () => {
      clipboard = selected().map((c) => c.id);
      toast(clipboard.length ? `Copied ${clipboard.length} clip${clipboard.length > 1 ? "s" : ""}` : "Select a clip first");
    },
  },
  {
    id: "paste",
    title: "Paste clips at the playhead",
    keys: ["Ctrl+V"],
    run: () => {
      const clips = allClips().filter((c) => clipboard.includes(c.id));
      if (!clips.length) return toast("Nothing to paste; the copied clips no longer exist");
      const delta = frame() - Math.min(...clips.map((c) => c.start));
      const target = ui.get().targetTrack;
      if (target) pasteTo(target, clips, delta);
      else duplicate(clips, delta);
    },
  },
  { id: "nudgeleft", title: "Nudge selection one frame left", keys: ["Alt+Left"], run: () => nudge(-1) },
  { id: "nudgeright", title: "Nudge selection one frame right", keys: ["Alt+Right"], run: () => nudge(1) },
  { id: "nudgeleft5", title: "Nudge selection five frames left", keys: ["Shift+Alt+Left"], run: () => nudge(-5) },
  { id: "nudgeright5", title: "Nudge selection five frames right", keys: ["Shift+Alt+Right"], run: () => nudge(5) },

  { id: "marker", title: "Add marker", keys: ["M"], run: () => exec("timeline.add_marker", { at: frame(), label: "Marker" }) },
  { id: "in", title: "Mark in", keys: ["I"], run: () => setRange({ in_point: frame() }) },
  { id: "out", title: "Mark out", keys: ["O"], run: () => setRange({ out_point: frame() }) },
  { id: "clearrange", title: "Clear in and out", keys: ["Alt+X"], run: () => setRange({ in_point: null, out_point: null }) },
  {
    id: "cutrange",
    title: "Ripple delete in–out range",
    keys: ["Ctrl+Shift+X"],
    run: () => {
      const s = project()?.sequence;
      if (s?.in_point == null || s.out_point == null) return toast("Mark in and out first");
      const from = s.in_point;
      exec("timeline.ripple_delete_range", { from, to: s.out_point }).then((ok) => ok && seek(from));
    },
  },
  { id: "addvideo", title: "Add video track", run: () => exec("timeline.add_track", { kind: "video" }) },
  { id: "addaudio", title: "Add audio track", run: () => exec("timeline.add_track", { kind: "audio" }) },

  { id: "link", title: "Link or unlink the selected clips", keys: ["Ctrl+L"], run: linkSelection },
  {
    id: "linkedsel",
    title: "Toggle linked selection (selecting one linked clip selects its group)",
    run: () => ui.set({ linkedSel: !ui.get().linkedSel }),
  },
  { id: "snap", title: "Toggle snapping", keys: ["N"], run: () => ui.set({ snap: !ui.get().snap }) },
  { id: "pace", title: "Toggle pace lane", run: () => ui.set({ showPace: !ui.get().showPace }) },
  { id: "zoomin", title: "Zoom in", keys: ["+", "="], run: () => ui.set({ pps: clamp(ui.get().pps * 1.3, MIN_PPS, MAX_PPS) }) },
  { id: "zoomout", title: "Zoom out", keys: ["-"], run: () => ui.set({ pps: clamp(ui.get().pps / 1.3, MIN_PPS, MAX_PPS) }) },
  { id: "fit", title: "Fit timeline to window", keys: ["Shift+Z"], run: () => timelineApi.fit() },

  { id: "undo", title: "Undo", keys: ["Ctrl+Z"], run: () => undo() },
  { id: "redo", title: "Redo", keys: ["Ctrl+Shift+Z", "Ctrl+Y"], run: () => redo() },
  {
    id: "new",
    title: "New project",
    keys: ["Ctrl+N"],
    run: () =>
      guarded(async () => {
        if (await confirmDiscard()) exec("project.new", { name: "Untitled" }).then(() => seek(0));
      }),
  },
  {
    id: "open",
    title: "Open project (.snn)…",
    keys: ["Ctrl+O"],
    run: () =>
      guarded(async () => {
        if (!(await confirmDiscard())) return;
        const path = await open({ filters: FILTERS, multiple: false, title: "Open project" });
        if (typeof path === "string" && (await openProject(path))) seek(0);
      }),
  },
  {
    id: "save",
    title: "Save project",
    keys: ["Ctrl+S"],
    run: () =>
      guarded(async () => {
        if (!core.get().state?.path) return saveAs();
        if (await saveProject()) toast("Saved");
      }),
  },
  { id: "saveas", title: "Save project as…", keys: ["Ctrl+Shift+S"], run: () => guarded(saveAs) },
  { id: "import", title: "Import media…", keys: ["Ctrl+I"], run: importMedia },
  { id: "importfolder", title: "Import folder…", run: importFolders },
  { id: "export", title: "Export video…", keys: ["Ctrl+E"], run: () => ui.set({ overlay: "export" }) },
  { id: "projectsettings", title: "Project settings (size, frame rate)…", run: () => ui.set({ overlay: "project" }) },
  { id: "palette", title: "Command palette", keys: ["Ctrl+K"], run: () => ui.set({ overlay: "palette" }) },
  { id: "shortcuts", title: "Keyboard shortcuts", keys: ["?"], run: () => ui.set({ overlay: "shortcuts" }) },
];

export const runCommand = (id: string) => COMMANDS.find((c) => c.id === id)?.run();

const KEY_NAMES: Record<string, string> = {
  " ": "Space",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",
};

function combo(e: KeyboardEvent): string {
  const letter = e.key.length === 1 && /[a-z]/i.test(e.key);
  const key = KEY_NAMES[e.key] ?? (letter ? e.key.toUpperCase() : e.key);
  // Shift is already part of symbols such as "?" and "+".
  const shift = e.shiftKey && (letter || key.length > 1);
  return [(e.ctrlKey || e.metaKey) && "Ctrl", shift && "Shift", e.altKey && "Alt", key].filter(Boolean).join("+");
}

export function handleKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    if (ui.get().overlay) ui.set({ overlay: null });
    else ui.set({ selection: [], selectedTrack: null });
    (document.activeElement as HTMLElement | null)?.blur?.();
    return;
  }
  const typing = /^(input|textarea|select)$/i.test((e.target as HTMLElement).tagName);
  const pressed = combo(e);
  if (typing && pressed !== "Ctrl+K" && pressed !== "Ctrl+S") return;
  if (ui.get().overlay && pressed !== "Ctrl+K") return;
  const command = COMMANDS.find((c) => c.keys?.includes(pressed));
  if (command) {
    e.preventDefault();
    command.run();
  }
}

/** Keeps the playhead inside the timeline after the project changes. */
export function clampPlayhead() {
  const p = project();
  if (p) ui.set({ frame: clamp(ui.get().frame, 0, timelineFrames(p)) });
}
