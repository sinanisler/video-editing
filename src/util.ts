import type { Clip, Project, Track } from "./types";

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

export function trackHeight(t: Track): number {
  return t.kind === "caption" ? 36 : t.kind === "audio" ? 42 : 46;
}

const AUDIO_CLASSES = ["k-voice", "k-music", "k-sfx"];

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
