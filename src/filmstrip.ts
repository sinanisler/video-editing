import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ui } from "./store";
import type { MediaItem } from "./types";

/**
 * Frame height the cache is made in: 240 pixels. Tracks are at most 120 px tall, which is 240 physical pixels on a
 * 2x screen, so one size covers every track without stretching.
 */
export const FILMSTRIP_SIZES = [240];
/** One frame every quarter second of video, so the frame under the playhead is within an eighth of a second of its time. */
export const FILMSTRIP_INTERVAL = 0.25;

type Strip = { dir: string; count: number; interval: number };

/** The cache size for a track that is `pixels` tall on screen. */
export function filmstripSize(pixels: number): number {
  return FILMSTRIP_SIZES.find((s) => s >= pixels) ?? FILMSTRIP_SIZES[FILMSTRIP_SIZES.length - 1];
}

const key = (id: string, height: number) => `${id}@${height}`;
const setState = (k: string, state: Strip | "pending" | "failed") =>
  ui.set({ filmstrips: { ...ui.get().filmstrips, [k]: state } });

/** Starts making the frames for a video at one size, once. Images and audio need none. */
export function requestFilmstrip(m: MediaItem, height: number) {
  const k = key(m.id, height);
  if (m.kind !== "video" || ui.get().filmstrips[k]) return;
  setState(k, "pending");
  invoke<Strip>("filmstrip", { path: m.path, height, interval: FILMSTRIP_INTERVAL }).then(
    (strip) => setState(k, strip),
    () => setState(k, "failed"),
  );
}

export const filmstripKey = key;

/** The image for the frame at `seconds` into the source. */
export function frameUrl(strip: Strip, seconds: number): string {
  const i = Math.min(strip.count - 1, Math.max(0, Math.round(seconds / strip.interval)));
  return convertFileSrc(`${strip.dir}/${String(i + 1).padStart(5, "0")}.jpg`);
}
