// Tests against a real file: MrBeast-being-a-SnapYT.App.mp4 in Documents.
// Run: node --test --experimental-strip-types dev/media-file.test.mts
// Override the file with SNN_TEST_VIDEO=<path>, and the ffprobe binary with FFPROBE=<path>.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { parseSubtitles, toSrt } from "../src/subtitles.ts";

const VIDEO = process.env.SNN_TEST_VIDEO ?? join(homedir(), "Documents", "MrBeast-being-a-SnapYT.App.mp4");
const FFPROBE = process.env.FFPROBE ?? "ffprobe";
const ready = existsSync(VIDEO);

interface Probe {
  duration: number;
  streams: { codec_type: string; codec_name: string; width?: number; height?: number; r_frame_rate: string }[];
}

function probe(): Probe {
  const out = execFileSync(FFPROBE, ["-hide_banner", "-v", "error", "-show_entries", "format=duration:stream=codec_type,codec_name,width,height,r_frame_rate", "-of", "json", VIDEO], {
    encoding: "utf8",
  });
  const json = JSON.parse(out);
  return { duration: Number(json.format.duration), streams: json.streams };
}

const skip = ready ? false : `missing ${VIDEO}`;
let info: Probe | null = null;
const get = (): Probe => (info ??= probe());

describe("MrBeast-being-a-SnapYT.App.mp4", { skip }, () => {
  it("has an H.264 video stream at 640x360", () => {
    const video = get().streams.find((s) => s.codec_type === "video");
    assert.ok(video, "no video stream");
    assert.equal(video.codec_name, "h264");
    assert.equal(video.width, 640);
    assert.equal(video.height, 360);
  });

  it("runs at 30000/1001 fps (29.97)", () => {
    const video = get().streams.find((s) => s.codec_type === "video")!;
    assert.equal(video.r_frame_rate, "30000/1001");
  });

  it("has an AAC audio stream, which the editor does not yet detect on import", () => {
    // Documents what the editor must eventually read from the file. MediaItem has no audio flag yet.
    const audio = get().streams.find((s) => s.codec_type === "audio");
    assert.ok(audio, "no audio stream");
    assert.equal(audio.codec_name, "aac");
  });

  it("lasts 594.08 seconds (9 minutes 54 seconds)", () => {
    assert.ok(Math.abs(get().duration - 594.08) < 0.05, `duration was ${get().duration}`);
  });

  it("gives whole frames for every 10 second caption step, all inside the video", () => {
    const fps = 30000 / 1001;
    const frames = Math.round(get().duration * fps);
    const cues = [];
    for (let at = 0; at + 5 <= get().duration; at += 10) cues.push({ start: at, end: at + 5, text: `Caption at ${at}s` });
    const srt = toSrt(cues);
    const back = parseSubtitles(srt);
    assert.equal(back.length, cues.length);
    assert.ok(back.every((c) => c.end <= get().duration));
    // Each caption's frame range falls inside the video's frame count.
    for (const c of back) assert.ok(Math.round(c.end * fps) <= frames, `cue ends at frame ${Math.round(c.end * fps)} of ${frames}`);
  });

  it("writes an SRT covering the whole video that round-trips exactly", () => {
    const cues = [];
    for (let at = 0; at < 60; at += 3) cues.push({ start: at, end: at + 2.5, text: "Tick" });
    assert.deepEqual(parseSubtitles(toSrt(cues)), cues);
  });
});
