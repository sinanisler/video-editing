// Mirrors the Rust types in src-tauri/crates/snn-core/src/model.rs and editor.rs.

export type Frame = number;
export type TrackKind = "video" | "audio" | "caption";
export type ClipKind = "video" | "graphic" | "caption" | "audio";

export interface Settings {
  width: number;
  height: number;
  fps_num: number;
  fps_den: number;
}

export interface Track {
  id: string;
  name: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  hidden: boolean;
}

export interface Transition {
  name: string;
  duration: Frame;
}

export interface Clip {
  id: string;
  track: string;
  start: Frame;
  duration: Frame;
  source_in: Frame;
  name: string;
  kind: ClipKind;
  media: string | null;
  text: string | null;
  template: string | null;
  props: Record<string, unknown>;
  effects: string[];
  transition_in: Transition | null;
  transition_out: Transition | null;
  last_actor: string | null;
}

export interface Marker {
  id: string;
  at: Frame;
  label: string;
  actor: string | null;
}

export type MediaKind = "video" | "audio" | "image";

export interface MediaItem {
  id: string;
  name: string;
  path: string;
  kind: MediaKind;
  /** Frames. Zero for images. */
  duration: Frame;
  width: number;
  height: number;
  /** Whether the file has a sound track. */
  has_audio: boolean;
}

export interface Sequence {
  name: string;
  tracks: Track[];
  clips: Clip[];
  markers: Marker[];
  in_point: Frame | null;
  out_point: Frame | null;
}

export interface Project {
  version: number;
  name: string;
  settings: Settings;
  media: MediaItem[];
  sequence: Sequence;
}

export interface LogEntry {
  seq: number;
  actor: string;
  tool: string;
  args: unknown;
  ok: boolean;
  result: unknown;
}

/** Mirrors snn-core's library.rs. */
export type PropDef = { key: string; label: string; group: string; applies: "visual" | "media" | "sound" | "text" } & (
  | { type: "number"; min: number; max: number; step: number; default: number; unit: string }
  | { type: "bool"; default: boolean }
  | { type: "choice"; options: string[]; default: string }
  | { type: "color"; default: string }
);

export interface Library {
  properties: PropDef[];
  effects: { name: string; hint: string; amount: number }[];
  transitions: { name: string; hint: string }[];
}

export interface McpStatus {
  running: boolean;
  url: string;
  error: string | null;
  tools: number;
}

export interface EditorState {
  project: Project;
  path: string | null;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
  history_len: number;
  log: LogEntry[];
}
