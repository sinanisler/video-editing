import type { SpeechCue } from "./ai";
import type { Clip } from "./types";

/** One subtitle: when it shows, in seconds, and what it says. */
export interface Cue {
  start: number;
  end: number;
  text: string;
}

/** Playback speed as a factor, where 1 is normal. Matches the preview and the export. */
export const clipSpeed = (clip: Clip) => (typeof clip.props.speed === "number" ? clip.props.speed / 100 : 1);

/**
 * Places lines of speech from a clip's source on the timeline, using the clip's in point, speed and position.
 * Lines outside the clip are dropped, and lines that run past its end are cut at the end, so no caption runs past the picture it belongs to.
 * The lines are in seconds of the source file; the cues come back in seconds of the timeline.
 */
export function captionsForClip(clip: Clip, lines: SpeechCue[], fps: number): Cue[] {
  const speed = clipSpeed(clip);
  const toTimeline = (seconds: number) => (clip.start + (seconds * fps - clip.source_in) / speed) / fps;
  const from = clip.start / fps;
  const to = (clip.start + clip.duration) / fps;
  return lines
    .map((line) => ({ start: Math.max(toTimeline(line.start), from), end: Math.min(toTimeline(line.end), to), text: line.text }))
    .filter((cue) => cue.end > cue.start);
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", nbsp: " ", quot: '"', apos: "'" };

/** Removes markup such as <i>, <c.yellow> and inline timestamps, and decodes the common entities. */
function plain(text: string): string {
  return text
    .replace(/<[^>]*>/g, "")
    .replace(/&(amp|lt|gt|nbsp|quot|apos);/g, (_, name: string) => ENTITIES[name])
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .join("\n");
}

/** `HH:MM:SS,mmm` (SRT), `HH:MM:SS.mmm` (WebVTT), or `MM:SS.mmm` (WebVTT without hours). */
function seconds(stamp: string): number | null {
  const m = stamp.match(/^(?:(\d+):)?(\d{1,2}):(\d{2})[,.](\d{1,3})$/);
  if (!m) return null;
  const [, hours = "0", minutes, secs, millis] = m;
  return Number(hours) * 3600 + Number(minutes) * 60 + Number(secs) + Number(millis.padEnd(3, "0")) / 1000;
}

/**
 * Reads SRT or WebVTT. Index numbers, the WEBVTT header, NOTE and STYLE blocks and cue settings are skipped.
 * Cues that end before they start, or have no text, are dropped.
 */
export function parseSubtitles(source: string): Cue[] {
  const lines = source.replace(/^﻿/, "").replace(/\r\n?/g, "\n").split("\n");
  const cues: Cue[] = [];
  let i = 0;
  while (i < lines.length) {
    const arrow = lines[i].match(/^\s*(\S+)\s+-->\s+(\S+)/);
    if (!arrow) {
      i++;
      continue;
    }
    const start = seconds(arrow[1]);
    const end = seconds(arrow[2]);
    i++;
    const body: string[] = [];
    while (i < lines.length && lines[i].trim() !== "") body.push(lines[i++]);
    const text = plain(body.join("\n"));
    if (start !== null && end !== null && end > start && text) cues.push({ start, end, text });
  }
  return cues;
}

const pad = (n: number, width: number) => String(n).padStart(width, "0");

/** Formats seconds as `HH:MM:SS` plus a separator and milliseconds. */
function stamp(at: number, separator: "," | "."): string {
  const ms = Math.round(at * 1000);
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${pad(h, 2)}:${pad(m, 2)}:${pad(s, 2)}${separator}${pad(ms % 1000, 3)}`;
}

export function toSrt(cues: Cue[]): string {
  return cues.map((c, i) => `${i + 1}\n${stamp(c.start, ",")} --> ${stamp(c.end, ",")}\n${c.text}\n`).join("\n");
}

export function toVtt(cues: Cue[]): string {
  return `WEBVTT\n\n${cues.map((c) => `${stamp(c.start, ".")} --> ${stamp(c.end, ".")}\n${c.text}\n`).join("\n")}`;
}

/**
 * How long a subtitle may be, from a captions track's settings. Each limit is off at 0: with all three off, a subtitle
 * stays as the subtitle model gave it.
 */
export interface ChunkRule {
  /** Most words in one subtitle. */
  words: number;
  /** Most characters in one subtitle, counting the spaces between its words. */
  chars: number;
  /** Longest a subtitle may run, in seconds. */
  seconds: number;
}

/** The chunk limits a captions track's settings give. A limit that is missing, zero or negative is off. */
export function chunkRuleOf(props: Record<string, unknown> | undefined): ChunkRule {
  const limit = (key: string) => {
    const value = props?.[key];
    return typeof value === "number" && value > 0 ? value : 0;
  };
  return { words: Math.floor(limit("chunk_words")), chars: Math.floor(limit("chunk_chars")), seconds: limit("chunk_seconds") };
}

/**
 * Splits each cue into shorter ones that keep within the rule. The model times whole lines rather than words, so the
 * words of a cue share its time in proportion to their length. A word longer than a limit stays whole.
 */
export function chunkCues(cues: Cue[], rule: ChunkRule): Cue[] {
  if (!rule.words && !rule.chars && !rule.seconds) return cues;
  const out: Cue[] = [];
  for (const cue of cues) {
    const words = cue.text.split(/\s+/).filter(Boolean);
    const total = words.reduce((n, w) => n + w.length, 0);
    const span = cue.end - cue.start;
    let letters = 0;
    const timed = words.map((word) => {
      const start = cue.start + (span * letters) / total;
      letters += word.length;
      return { word, start, end: cue.start + (span * letters) / total };
    });
    let group: typeof timed = [];
    let chars = 0;
    const flush = () => {
      if (!group.length) return;
      out.push({ start: group[0].start, end: group[group.length - 1].end, text: group.map((t) => t.word).join(" ") });
      group = [];
      chars = 0;
    };
    for (const t of timed) {
      const grown = group.length ? chars + 1 + t.word.length : t.word.length;
      const full =
        group.length > 0 &&
        ((rule.words > 0 && group.length >= rule.words) ||
          (rule.chars > 0 && grown > rule.chars) ||
          (rule.seconds > 0 && t.end - group[0].start > rule.seconds));
      if (full) flush();
      chars = group.length ? chars + 1 + t.word.length : t.word.length;
      group.push(t);
    }
    flush();
  }
  return out;
}
