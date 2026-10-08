import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ui } from "./store";
import type { MediaItem } from "./types";

/**
 * A file's sound as 8-bit min and max pairs, one pair per `block` samples at `rate` samples a second.
 * `data` holds min, max, min, max, ...
 */
export interface Wave {
  data: Int8Array;
  blocks: number;
  rate: number;
  block: number;
}

export type WaveState = Wave | "pending" | "failed";

const setState = (id: string, state: WaveState) => ui.set({ waves: { ...ui.get().waves, [id]: state } });

/** Starts reading a file's sound, once. Only media with a sound track has one. */
export function requestWave(m: MediaItem) {
  const hasSound = m.kind === "audio" || (m.kind === "video" && m.has_audio);
  if (!hasSound || ui.get().waves[m.id]) return;
  setState(m.id, "pending");
  invoke<{ file: string; blocks: number; rate: number; block: number }>("waveform", { path: m.path })
    .then(async (info) => {
      const bytes = await (await fetch(convertFileSrc(info.file))).arrayBuffer();
      setState(m.id, { data: new Int8Array(bytes), blocks: info.blocks, rate: info.rate, block: info.block });
    })
    .catch(() => setState(m.id, "failed"));
}
