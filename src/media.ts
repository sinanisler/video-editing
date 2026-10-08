import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { exec, execBatch, project, toast, ui } from "./store";
import type { MediaItem, MediaKind, Track } from "./types";
import { clipEnd, fpsOf } from "./util";

const EXTENSIONS: Record<MediaKind, string[]> = {
  video: ["mp4", "mov", "m4v", "mkv", "webm"],
  audio: ["mp3", "wav", "m4a", "aac", "ogg", "flac"],
  image: ["png", "jpg", "jpeg", "webp", "gif"],
};
export const MEDIA_EXTENSIONS = Object.values(EXTENSIONS).flat();
const IMAGE_SECONDS = 5;

export const assetUrl = (path: string) => convertFileSrc(path);

export function kindOf(path: string): MediaKind | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (Object.keys(EXTENSIONS) as MediaKind[]).find((k) => EXTENSIONS[k].includes(ext)) ?? null;
}

/** Reads length and size by letting the webview open the file, the same decoder the preview uses. */
function probe(path: string, kind: MediaKind): Promise<{ seconds: number; width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const fail = () => reject(new Error("this app cannot decode it"));
    const timer = window.setTimeout(fail, 20000);
    const done = (seconds: number, width: number, height: number) => {
      window.clearTimeout(timer);
      Number.isFinite(seconds) ? resolve({ seconds, width, height }) : fail();
    };
    if (kind === "image") {
      const img = new Image();
      img.onload = () => done(0, img.naturalWidth, img.naturalHeight);
      img.onerror = fail;
      img.src = assetUrl(path);
      return;
    }
    const el = document.createElement(kind);
    el.preload = "metadata";
    el.onloadedmetadata = () => done(el.duration, (el as HTMLVideoElement).videoWidth ?? 0, (el as HTMLVideoElement).videoHeight ?? 0);
    el.onerror = fail;
    el.src = assetUrl(path);
  });
}

/** Adds files to the media pool and returns the pool items, including ones that were already there. */
export async function importPaths(paths: string[]): Promise<MediaItem[]> {
  const p = project();
  if (!p) return [];
  const fps = fpsOf(p);
  const items: MediaItem[] = [];
  for (const path of paths) {
    const name = path.split(/[\\/]/).pop() ?? path;
    const existing = project()?.media.find((m) => m.path === path);
    if (existing) {
      items.push(existing);
      continue;
    }
    const kind = kindOf(path);
    if (!kind) {
      toast(`${name}: not a supported video, audio or image file`);
      continue;
    }
    try {
      await invoke("allow_asset", { path });
      const info = await probe(path, kind);
      // Only MP4-family files can be read for a sound track here. Anything else is assumed to have one.
      const hasAudio = kind === "image" ? false : kind === "audio" ? true : (await invoke<boolean | null>("probe_audio", { path })) ?? true;
      const result = await exec("media.import", {
        path,
        kind,
        duration: Math.round(info.seconds * fps),
        width: info.width,
        height: info.height,
        has_audio: hasAudio,
      });
      if (result) items.push(result.media as MediaItem);
    } catch (e) {
      toast(`${name}: ${e instanceof Error ? e.message : e}`);
    }
  }
  if (items.length) ui.set({ leftTab: "media" });
  return items;
}

const overlaps = (track: string, start: number, duration: number) =>
  project()?.sequence.clips.find((c) => c.track === track && start < clipEnd(c) && c.start < start + duration);

/**
 * Places a video's sound on the first unlocked audio track that is free for its length, links it to the video
 * clip, and mutes the video so the sound is not heard twice. The video clip has to exist first, so this runs as a
 * second undo step. The two stay linked: they select and move in time together, but either can change track alone.
 */
async function linkAudio(m: MediaItem, clip: string, start: number, duration: number) {
  const p = project();
  if (!p) return;
  const track = p.sequence.tracks.find((t) => t.kind === "audio" && !t.locked && !overlaps(t.id, start, duration));
  if (!track) return toast(`No free audio track for the sound of ${m.name}`);
  await execBatch([
    { tool: "timeline.add_clip", args: { track: track.id, start, duration, name: m.name, kind: "audio", media: m.id, link_to: clip } },
    { tool: "clip.set_property", args: { clip, key: "mute", value: true } },
  ]);
}

/**
 * Puts a pool item on the timeline at `frame`. Uses `track` when the item fits there, otherwise
 * the first suitable track that is free at that spot. If nothing is free the clip slides right
 * past whatever is in the way. Returns where it landed.
 */
export async function addMediaClip(
  m: MediaItem,
  frame: number,
  track?: string | null,
): Promise<{ track: string; end: number } | null> {
  const p = project();
  if (!p) return null;
  const duration = m.kind === "image" ? Math.round(IMAGE_SECONDS * fpsOf(p)) : m.duration;
  const wanted: Track["kind"] = m.kind === "audio" ? "audio" : "video";
  const usable = p.sequence.tracks.filter((t) => t.kind === wanted && !t.locked);
  // Video fills from the main (bottom) track upwards, audio from the top down.
  const order = wanted === "video" ? [...usable].reverse() : usable;
  const target =
    usable.find((t) => t.id === track) ?? order.find((t) => !overlaps(t.id, frame, duration)) ?? order[0];
  if (!target) {
    toast(`No unlocked ${wanted} track`);
    return null;
  }
  let start = Math.max(0, frame);
  for (let hit = overlaps(target.id, start, duration); hit; hit = overlaps(target.id, start, duration)) start = clipEnd(hit);
  const result = await exec("timeline.add_clip", {
    track: target.id,
    start,
    duration,
    name: m.name,
    kind: wanted === "audio" ? "audio" : "video",
    media: m.id,
  });
  if (!result) return null;
  ui.set({ selection: [result.clip as string] });
  // A video with sound gets its sound as a linked audio clip, and its own sound is muted so it is not heard twice.
  if (m.kind === "video" && m.has_audio) await linkAudio(m, result.clip as string, start, duration);
  return { track: target.id, end: start + duration };
}

/** Adds several items one after another, starting at `frame`. */
export async function addMediaClips(items: MediaItem[], frame: number, track?: string | null) {
  let at = frame;
  let on = track;
  for (const m of items) {
    const placed = await addMediaClip(m, at, on);
    if (placed && (m.kind === "audio") === (items[0].kind === "audio")) {
      at = placed.end;
      on = placed.track;
    }
  }
}
