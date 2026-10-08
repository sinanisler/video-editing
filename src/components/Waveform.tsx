import { useEffect } from "react";
import { ui } from "../store";
import type { MediaItem } from "../types";
import type { Clip } from "../types";
import { requestWave } from "../waveform";
import type { Wave } from "../waveform";

const COLUMNS_MAX = 4000;
const VIEW_H = 100;

/**
 * The sound of one audio clip, drawn as a single vector path. Only the part on screen is drawn, with one column
 * per pixel, so zooming and scrolling only recompute the visible columns.
 */
export function Waveform({ c, m, ppf, fps, lane }: { c: Clip; m: MediaItem; ppf: number; fps: number; lane: number }) {
  const wave = ui.use((s) => s.waves[m.id]);
  const scrollX = ui.use((s) => s.scrollX);
  const viewW = ui.use((s) => s.viewW);
  useEffect(() => requestWave(m), [m]);
  if (!wave || typeof wave !== "object" || !viewW) return null;

  const pixelsPerSecond = ppf * fps;
  const clipX = c.start * ppf;
  const clipW = c.duration * ppf;
  const from = Math.max(0, scrollX - clipX);
  const to = Math.min(clipW, scrollX + viewW - clipX);
  if (to <= from) return null;
  const columns = Math.min(COLUMNS_MAX, Math.ceil(to - from));
  const columnPx = (to - from) / columns;
  const d = pathFor(wave, c.source_in / fps, pixelsPerSecond, from, columnPx, columns);
  return (
    <svg className="wave" style={{ left: from, width: to - from, height: lane }} viewBox={`0 0 ${columns} ${VIEW_H}`} preserveAspectRatio="none">
      <path d={d} />
    </svg>
  );
}

/** Outline of the sound: the top edge left to right, then the bottom edge back. */
function pathFor(wave: Wave, sourceStart: number, pixelsPerSecond: number, from: number, columnPx: number, columns: number): string {
  const blocksPerSecond = wave.rate / wave.block;
  const mid = VIEW_H / 2;
  const scale = mid / 128;
  const top: string[] = [];
  const bottom: string[] = [];
  for (let j = 0; j < columns; j++) {
    const t0 = sourceStart + (from + j * columnPx) / pixelsPerSecond;
    const t1 = t0 + columnPx / pixelsPerSecond;
    const b0 = Math.min(wave.blocks - 1, Math.max(0, Math.floor(t0 * blocksPerSecond)));
    const b1 = Math.min(wave.blocks, Math.max(b0 + 1, Math.ceil(t1 * blocksPerSecond)));
    let lo = 127;
    let hi = -128;
    for (let b = b0; b < b1; b++) {
      const mn = wave.data[2 * b];
      const mx = wave.data[2 * b + 1];
      if (mn < lo) lo = mn;
      if (mx > hi) hi = mx;
    }
    if (hi < lo) lo = hi = 0;
    top.push(`${j} ${(mid - hi * scale).toFixed(1)}`);
    bottom.push(`${j} ${(mid - lo * scale).toFixed(1)}`);
  }
  return `M${top.join(" L")} L${bottom.reverse().join(" L")} Z`;
}
