import { execBatch, project, toast } from "./store";
import { chunkCues, chunkRuleOf, parseSubtitles, toSrt, toVtt } from "./subtitles";
import type { Cue } from "./subtitles";
import type { Clip } from "./types";
import { clipEnd, fpsOf } from "./util";

/** Opens the webview's own file picker, so no file-system plugin is needed. Resolves to null if cancelled. */
export function pickSubtitleFile(): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".srt,.vtt";
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/**
 * The commands that put each cue on a captions track as a caption. Cue times are seconds on the timeline.
 * A cue that would overlap one of `taken` is skipped. A cue that overlaps the one before it starts when that one ends.
 */
function placeCues(track: string, cues: Cue[], fps: number, taken: Clip[]) {
  const commands: { tool: string; args: Record<string, unknown> }[] = [];
  let skipped = 0;
  let previousEnd = 0;
  for (const cue of cues) {
    const start = Math.max(Math.round(cue.start * fps), previousEnd);
    const end = Math.round(cue.end * fps);
    const duration = end - start;
    if (duration < 1 || taken.some((c) => start < clipEnd(c) && c.start < end)) {
      skipped++;
      continue;
    }
    commands.push({
      tool: "timeline.add_clip",
      args: { track, start, duration, name: cue.text.split("\n")[0].slice(0, 40), kind: "caption", text: cue.text },
    });
    previousEnd = end;
  }
  return { commands, skipped };
}

/**
 * Adds each cue as a caption clip on the first unlocked captions track, as one undo step. The track's chunk settings split
 * long cues first. A cue that would overlap a caption already there is skipped.
 * Resolves to how many were added and skipped, or null when nothing was added.
 */
export async function addCaptions(cues: Cue[]): Promise<{ added: number; skipped: number } | null> {
  const p = project();
  if (!p) return null;
  const track = p.sequence.tracks.find((t) => t.kind === "caption" && !t.locked);
  if (!track) {
    toast("No unlocked captions track to put the captions on");
    return null;
  }
  const pieces = chunkCues(cues, chunkRuleOf(track.props));
  const { commands, skipped } = placeCues(track.id, pieces, fpsOf(p), p.sequence.clips.filter((c) => c.track === track.id));
  if (!commands.length) {
    toast(`None of the ${pieces.length} captions fit on the captions track`);
    return null;
  }
  const done = await execBatch(commands);
  return done ? { added: commands.length, skipped } : null;
}

/**
 * Splits the captions on a captions track that are longer than its chunk settings allow, as one undo step. A caption that
 * already fits keeps its own settings. One that is split takes the track's, so its own settings are lost.
 */
export async function splitCaptions(track: string) {
  const p = project();
  if (!p) return;
  const rule = chunkRuleOf(p.sequence.tracks.find((t) => t.id === track)?.props);
  if (!rule.words && !rule.chars && !rule.seconds) return toast("Set a maximum number of words, characters or seconds first");
  const own = p.sequence.clips.filter((c) => c.track === track && c.kind === "caption").sort((a, b) => a.start - b.start);
  if (!own.length) return toast("No captions on this track to split");
  const fps = fpsOf(p);
  const split: Clip[] = [];
  const pieces: Cue[] = [];
  for (const c of own) {
    const parts = chunkCues([{ start: c.start / fps, end: clipEnd(c) / fps, text: c.text ?? c.name }], rule);
    if (parts.length > 1) {
      split.push(c);
      pieces.push(...parts);
    }
  }
  if (!split.length) return toast("Every caption on this track already fits its settings");
  const commands = [
    ...split.map((c) => ({ tool: "timeline.delete_clip", args: { clip: c.id } })),
    ...placeCues(track, pieces, fps, []).commands,
  ];
  if (await execBatch(commands)) toast(`Split ${split.length} captions into ${pieces.length}`);
}

/** Adds the captions in a subtitle file. */
export async function importSubtitles(file: File) {
  const cues = parseSubtitles(await file.text());
  if (!cues.length) return toast(`No subtitles found in ${file.name}`);
  const result = await addCaptions(cues);
  if (result) toast(`Imported ${result.added} subtitles${result.skipped ? `, skipped ${result.skipped} that overlap` : ""}`);
}


/** Saves the captions track as an SRT or WebVTT file through a browser download. */
export function exportSubtitles(format: "srt" | "vtt") {
  const p = project();
  if (!p) return;
  const captionTracks = new Set(p.sequence.tracks.filter((t) => t.kind === "caption").map((t) => t.id));
  const clips = p.sequence.clips.filter((c) => captionTracks.has(c.track)).sort((a, b) => a.start - b.start);
  if (!clips.length) return toast("No captions to export");
  const fps = fpsOf(p);
  const cues: Cue[] = clips.map((c) => ({ start: c.start / fps, end: clipEnd(c) / fps, text: c.text ?? c.name }));
  const url = URL.createObjectURL(new Blob([format === "srt" ? toSrt(cues) : toVtt(cues)], { type: "text/plain;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `${p.name}.${format}`;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  toast(`Exported ${clips.length} captions as .${format}`);
}
