// Run: node --test --experimental-strip-types dev/captions.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { captionsForClip, clipSpeed } from "../src/subtitles.ts";
import type { Clip } from "../src/types.ts";

const fps = 30;
/** A clip on V1 starting at 2 s and lasting 10 s, showing its source from 10 s on. */
const clip = (props: Record<string, unknown> = {}): Clip =>
  ({ id: "c1", track: "V1", start: 60, duration: 300, source_in: 300, name: "a", kind: "video", media: "m1", text: null, props, effects: [] }) as unknown as Clip;

describe("clipSpeed", () => {
  it("is 1 when the clip has no speed, and a factor otherwise", () => {
    assert.equal(clipSpeed(clip()), 1);
    assert.equal(clipSpeed(clip({ speed: 200 })), 2);
    assert.equal(clipSpeed(clip({ speed: 50 })), 0.5);
  });
});

describe("captionsForClip", () => {
  it("places a line from the source at the same offset within the clip", () => {
    // Source 12 s to 13.5 s is 2 s to 3.5 s into the clip, which starts at 2 s on the timeline.
    const cues = captionsForClip(clip(), [{ start: 12, end: 13.5, text: "hello" }], fps);
    assert.deepEqual(cues, [{ start: 4, end: 5.5, text: "hello" }]);
  });

  it("follows the clip's speed: at 2x a source second is half a timeline second", () => {
    // The clip shows source 10 s to 30 s in 10 timeline seconds. Source 12 s to 13 s is 1 s into the clip.
    const cues = captionsForClip(clip({ speed: 200 }), [{ start: 12, end: 13, text: "fast" }], fps);
    assert.deepEqual(cues, [{ start: 3, end: 3.5, text: "fast" }]);
  });

  it("drops lines the clip does not show, and cuts a line that runs past its end", () => {
    // The clip shows source 10 s to 20 s, which is timeline 2 s to 12 s.
    const lines = [
      { start: 0, end: 5, text: "before the in point" },
      { start: 25, end: 30, text: "after the out point" },
      { start: 19.5, end: 21, text: "runs past the end" },
    ];
    const cues = captionsForClip(clip(), lines, fps);
    assert.deepEqual(cues, [{ start: 11.5, end: 12, text: "runs past the end" }]);
  });

  it("does not start a line before the clip's start on the timeline", () => {
    // Source 9 s to 11 s: the part from 10 s on is inside the clip, so it starts at the clip's first frame (2 s) and ends 1 s in (3 s).
    const cues = captionsForClip(clip(), [{ start: 9, end: 11, text: "straddles the in point" }], fps);
    assert.deepEqual(cues, [{ start: 2, end: 3, text: "straddles the in point" }]);
  });
});
