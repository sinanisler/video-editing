/** One subtitle: when it shows, in seconds, and what it says. */
export interface Cue {
  start: number;
  end: number;
  text: string;
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
