import { execBatch, project, toast } from "./store";
import { parseSubtitles, toSrt, toVtt } from "./subtitles";
import type { Cue } from "./subtitles";
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
 * Adds each subtitle as a caption clip on the first unlocked captions track, as one undo step.
 * A cue that would overlap a caption already there is skipped. A cue that overlaps the one before it starts when that one ends.
 */
export async function importSubtitles(file: File) {
  const p = project();
  if (!p) return;
  const track = p.sequence.tracks.find((t) => t.kind === "caption" && !t.locked);
  if (!track) return toast("No unlocked captions track to put the subtitles on");
  const fps = fpsOf(p);
  const cues = parseSubtitles(await file.text());
  if (!cues.length) return toast(`No subtitles found in ${file.name}`);

  const existing = p.sequence.clips.filter((c) => c.track === track.id);
  const commands: { tool: string; args: Record<string, unknown> }[] = [];
  let skipped = 0;
  let previousEnd = 0;
  for (const cue of cues) {
    const start = Math.max(Math.round(cue.start * fps), previousEnd);
    const end = Math.round(cue.end * fps);
    const duration = end - start;
    if (duration < 1 || existing.some((c) => start < clipEnd(c) && c.start < end)) {
      skipped++;
      continue;
    }
    commands.push({
      tool: "timeline.add_clip",
      args: { track: track.id, start, duration, name: cue.text.split("\n")[0].slice(0, 40), kind: "caption", text: cue.text },
    });
    previousEnd = end;
  }
  if (!commands.length) return toast(`None of the ${cues.length} subtitles fit on the captions track`);
  const done = await execBatch(commands);
  if (done) toast(`Imported ${commands.length} subtitles${skipped ? `, skipped ${skipped} that overlap` : ""}`);
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
