import type { Clip, MediaItem, MediaSort, Project, Track } from "./types";

export const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

export const fpsOf = (p: Project) => p.settings.fps_num / p.settings.fps_den;

export const clipEnd = (c: Clip) => c.start + c.duration;

const pad = (n: number) => String(n).padStart(2, "0");

/** HH:MM:SS:FF */
export function timecode(frame: number, fps: number): string {
  const f = Math.max(0, Math.round(frame));
  const base = Math.round(fps);
  const s = Math.floor(f / base);
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}:${pad(f % base)}`;
}

/** m:ss, or h:mm:ss from one hour up. */
export function mmss(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor(seconds / 60) % 60;
  return h ? `${h}:${pad(m)}:${pad(Math.floor(seconds % 60))}` : `${m}:${pad(Math.floor(seconds % 60))}`;
}

/** Timeline zoom limits in pixels per second: from about a day across the window down to single frames. */
export const MIN_PPS = 0.01;
export const MAX_PPS = 240;

export function fits(clip: Clip["kind"], track: Track["kind"]): boolean {
  if (clip === "audio") return track === "audio";
  if (clip === "caption") return track === "caption";
  return track === "video";
}

/** Stable hue from a string, used for placeholder colours. */
export function hue(s: string): number {
  let h = 0;
  for (const ch of s) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

export const COLLAPSED_PX = 18;
export const TRACK_MIN_PX = 24;
/**
 * Lanes shorter than this show only their clips' colour. Frames and waveforms at this height are noise, and they
 * are not even made, which saves the work of making them.
 */
export const THIN_LANE_PX = 30;
export const TRACK_MAX_PX = 120;

/** How tall each track is on screen. Kept in the view state, not the project. */
export interface TrackLayout {
  videoH: number;
  audioH: number;
  heights: Record<string, number>;
  collapsed: Record<string, boolean>;
}

export function trackHeight(t: Track, layout: TrackLayout): number {
  if (layout.collapsed[t.id]) return COLLAPSED_PX;
  if (layout.heights[t.id]) return layout.heights[t.id];
  return t.kind === "caption" ? 36 : t.kind === "audio" ? layout.audioH : layout.videoH;
}

/** The view state the divider between the video and audio groups changes. */
export interface SplitState {
  videoH: number;
  collapsed: Record<string, boolean>;
  trackHeights: Record<string, number>;
}

/**
 * The divider's state after it has moved `dy` px from where the drag began. It moves the video tracks that are open
 * at the start, so a track folded by hand stays folded. If every video track was folded, the divider is the group
 * handle and pulling it out opens them all. Pulled below the minimum, the moved tracks fold together. `videoH` is
 * the height the moved tracks take when open; the caller keeps the last open height while the group is folded.
 */
export function splitDrag(start: SplitState, videoIds: string[], dy: number) {
  const open = videoIds.filter((id) => !start.collapsed[id]);
  const wholeGroupFolded = videoIds.length > 0 && open.length === 0;
  const moved = wholeGroupFolded ? videoIds : open;
  // Each moved track takes its share of the pointer movement, so the line moves exactly as far as the pointer.
  const raw = (wholeGroupFolded ? COLLAPSED_PX : clamp(start.videoH, TRACK_MIN_PX, TRACK_MAX_PX)) + dy / Math.max(1, moved.length);
  const folded = raw < TRACK_MIN_PX;
  const collapsed = { ...start.collapsed };
  for (const id of videoIds) {
    // Pulling back out restores each track's state from the start, so a track folded by hand stays folded.
    collapsed[id] = folded || (!wholeGroupFolded && !!start.collapsed[id]);
  }
  const trackHeights = { ...start.trackHeights };
  for (const id of moved) delete trackHeights[id];
  return { folded, videoH: Math.round(clamp(raw, TRACK_MIN_PX, TRACK_MAX_PX)), collapsed, trackHeights };
}

const AUDIO_CLASSES = ["k-voice", "k-music", "k-sfx"];

/**
 * The ids plus every clip in their link groups, when linked selection is on. A video and the sound taken from it
 * select, move in time, copy and delete as one. Off, the ids are returned as they are.
 */
export function linkedIds(clips: Clip[], ids: string[], linked = true): string[] {
  const out = new Set(ids);
  if (!linked) return [...out];
  const groups = new Set(ids.map((id) => clips.find((c) => c.id === id)?.link).filter((g): g is string => !!g));
  for (const c of clips) if (c.link && groups.has(c.link)) out.add(c.id);
  return [...out];
}

const KIND_ORDER: Record<MediaItem["kind"], number> = { video: 0, audio: 1, image: 2 };

/**
 * The media in the order the pool shows it. `desc` reverses that: for import order, newest first.
 * `uses` counts the timeline clips each item backs, for the clips order. Ties fall back to name, so the order
 * never depends on the import sequence alone.
 */
export function sortMedia(media: MediaItem[], sort: MediaSort, desc: boolean, uses: Map<string, number> = new Map()): MediaItem[] {
  const imported = new Map(media.map((m, i) => [m.id, i]));
  const byName = (a: MediaItem, b: MediaItem) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
  const cmp = (a: MediaItem, b: MediaItem): number => {
    if (sort === "name") return byName(a, b);
    if (sort === "length") return a.duration - b.duration || byName(a, b);
    if (sort === "type") return KIND_ORDER[a.kind] - KIND_ORDER[b.kind] || byName(a, b);
    if (sort === "clips") return (uses.get(a.id) ?? 0) - (uses.get(b.id) ?? 0) || byName(a, b);
    return imported.get(a.id)! - imported.get(b.id)!;
  };
  const sorted = [...media].sort(cmp);
  return desc ? sorted.reverse() : sorted;
}

/** The media selection after a click on one item. A plain click replaces it; Ctrl or Shift toggles the item in. */
export function pickMedia(current: string[], id: string, extend: boolean): string[] {
  if (!extend) return [id];
  return current.includes(id) ? current.filter((x) => x !== id) : [...current, id];
}

/** The clips a cut at frame `at` would split: those the frame falls strictly inside, on tracks that are not locked. */
export function cutTargets(clips: Clip[], tracks: Track[], at: number): Clip[] {
  const locked = new Set(tracks.filter((t) => t.locked).map((t) => t.id));
  return clips.filter((c) => at > c.start && at < clipEnd(c) && !locked.has(c.track));
}

export function clipClass(c: Clip, tracks: Track[]): string {
  if (c.kind === "caption") return "k-cap";
  if (c.kind === "graphic") return "k-gfx";
  if (c.kind === "audio") {
    const i = tracks.filter((t) => t.kind === "audio").findIndex((t) => t.id === c.track);
    return AUDIO_CLASSES[Math.max(0, i) % AUDIO_CLASSES.length];
  }
  return "k-video";
}

/** Frames shown on the timeline: the content plus room to work after it. */
export function timelineFrames(p: Project): number {
  const fps = fpsOf(p);
  const end = p.sequence.clips.reduce((m, c) => Math.max(m, clipEnd(c)), 0);
  return Math.round(Math.max(end, 60 * fps) + 30 * fps);
}
