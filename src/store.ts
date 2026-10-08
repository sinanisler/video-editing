import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import type { ExportStatus } from "./export";
import type { Live } from "./look";
import { unimplemented } from "./look";
import type { EditorState, Library, McpStatus } from "./types";
import { clamp, fpsOf, timelineFrames } from "./util";

function createStore<T extends object>(initial: T) {
  let state = initial;
  const listeners = new Set<() => void>();
  const subscribe = (l: () => void) => {
    listeners.add(l);
    return () => listeners.delete(l);
  };
  return {
    get: () => state,
    set(patch: Partial<T>) {
      state = { ...state, ...patch };
      listeners.forEach((l) => l());
    },
    /** Select primitives or references that stay stable between unrelated updates. */
    use: <U>(select: (s: T) => U): U => useSyncExternalStore(subscribe, () => select(state)),
  };
}

/** Project state. Owned by the Rust core; this is a read-only copy. */
export const core = createStore<{ state: EditorState | null; mcp: McpStatus | null; library: Library | null; exportStatus: ExportStatus | null }>({
  state: null,
  mcp: null,
  library: null,
  exportStatus: null,
});

export type Tool = "select" | "razor";

/** One row of a right-click menu. A row of colour swatches, or a separator, are the other two kinds. */
export type MenuItem =
  | "sep"
  | { label: string; hint?: string; checked?: boolean; disabled?: boolean; danger?: boolean; action: () => void }
  | { label: string; swatches: (string | null)[]; current?: string; onPick: (colour: string | null) => void };
export type Overlay = "palette" | "shortcuts" | "export" | "project" | null;

/** View state that is not part of the project. */
export const ui = createStore({
  frame: 0,
  playing: false,
  rate: 1,
  /** Timeline zoom in pixels per second. */
  pps: 9,
  tool: "select" as Tool,
  snap: true,
  /** Ids of the selected clips. */
  selection: [] as string[],
  leftTab: "media",
  rightTab: "inspector",
  overlay: null as Overlay,
  safeArea: false,
  /** Inspector values being dragged, previewed before they are committed. */
  live: null as Live | null,
  /** Which end of a clip the Transitions tab applies to. */
  transitionEdge: "in" as "in" | "out",
  /** The pace lane (cuts per 10 seconds) above the tracks. */
  showPace: false,
  /** The track that paste lands on. Set by clicking a track header or an empty part of a track. */
  targetTrack: null as string | null,
  /** Heights the user dragged to, by track id. Tracks without one use the default for their kind. */
  trackHeights: {} as Record<string, number>,
  /** Collapsed tracks show as a thin strip. */
  collapsed: {} as Record<string, boolean>,
  /** Default height of video and audio tracks. The divider between the two groups changes them. */
  videoH: 46,
  audioH: 42,
  /** Width of the track headers column, in pixels. */
  headsW: 190,
  /** Height of the timeline panel, in pixels. Starts at 46% of the window. */
  timelineH: Math.round(window.innerHeight * 0.46),
  /** Width of the media and inspector panels, in pixels. */
  leftW: 330,
  rightW: 370,
  /** Colour stripe for a track, by track id. */
  trackColors: {} as Record<string, string>,
  /** Colour stripe for a clip, by clip id. Shown on the clip; view state only. */
  clipColors: {} as Record<string, string>,
  /** The track whose name is being edited. */
  renaming: null as string | null,
  /** An open right-click menu. */
  menu: null as { x: number; y: number; items: MenuItem[] } | null,
  /** Files from outside the app are being dragged over the window. */
  fileDrag: false,
  /** A media pool item being dragged with the pointer. */
  mediaDrag: null as { id: string; name: string; x: number; y: number } | null,
  toast: "",
});

let toastTimer = 0;
export function toast(message: string) {
  ui.set({ toast: message });
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => ui.set({ toast: "" }), 2600);
}

export async function init() {
  await listen<EditorState>("editor:state", (e) => core.set({ state: e.payload }));
  // An agent pointing the user at a frame or at clips.
  await listen<{ frame?: number; select?: string[] }>("editor:view", ({ payload }) => {
    if (typeof payload.frame === "number") seek(payload.frame);
    if (payload.select) ui.set({ selection: payload.select, rightTab: "inspector" });
  });
  const library = await invoke<Library>("library");
  core.set({ state: await invoke<EditorState>("get_state"), mcp: await invoke<McpStatus>("mcp_status"), library });
  const missing = unimplemented(library);
  if (missing.length) toast(`The preview cannot draw: ${missing.join(", ")}`);
}

async function call<T>(name: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    return await invoke<T>(name, args);
  } catch (e) {
    toast(String(e));
    return null;
  }
}

/** Runs one core command. Tool names and arguments are the same ones agents will use over MCP. */
export const exec = (tool: string, args: Record<string, unknown> = {}) =>
  call<Record<string, unknown>>("execute", { command: { tool, args } });
/** Runs several commands as one undo step. A single command goes through `exec`. */
export const execBatch = (commands: { tool: string; args: Record<string, unknown> }[]) =>
  commands.length === 1
    ? exec(commands[0].tool, commands[0].args)
    : call<Record<string, unknown>>("execute_batch", { commands });
/** Shows values in the preview while they are being dragged, without touching the project. */
export const previewProps = (clip: string, values: Record<string, unknown>) => ui.set({ live: { clip, values } });

/** Writes values to the clip as one undo step, then drops the live preview. Null resets a key. */
export function commitProps(clip: string, values: Record<string, unknown>) {
  const commands = Object.entries(values).map(([key, value]) => ({ tool: "clip.set_property", args: { clip, key, value } }));
  return execBatch(commands).finally(() => ui.set({ live: null }));
}

export const undo = () => call("undo");
export const redo = () => call("redo");
export const saveProject = (path?: string) => call<string>("save_project", { path: path ?? null });
export const openProject = (path: string) => call<string>("open_project", { path });

export const project = () => core.get().state?.project ?? null;

let position = 0;
let lastTick = 0;

export function seek(frame: number) {
  const p = project();
  position = clamp(Math.round(frame), 0, p ? timelineFrames(p) : 0);
  ui.set({ frame: position });
}

function tick(now: number) {
  const { playing, rate } = ui.get();
  const p = project();
  if (!playing || !p) return;
  position += ((now - lastTick) / 1000) * rate * fpsOf(p);
  lastTick = now;
  const max = timelineFrames(p);
  if (position <= 0 || position >= max) {
    ui.set({ frame: clamp(Math.round(position), 0, max), playing: false, rate: 1 });
    return;
  }
  ui.set({ frame: Math.round(position) });
  requestAnimationFrame(tick);
}

/** Until the compositor lands this only moves the playhead; there is no picture or sound yet. */
export function setPlaying(playing: boolean, rate = 1) {
  const wasPlaying = ui.get().playing;
  ui.set({ playing, rate: playing ? rate : 1 });
  if (playing && !wasPlaying) {
    position = ui.get().frame;
    lastTick = performance.now();
    requestAnimationFrame(tick);
  }
}
