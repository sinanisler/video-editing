// Run: node --test --experimental-strip-types dev/linked.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { linkedIds } from "../src/util.ts";

const clip = (id: string, kind: "video" | "audio", media: string, start: number, duration: number) =>
  ({ id, kind, media, start, duration }) as never;

describe("linked clips", () => {
  const video = clip("v", "video", "m1", 0, 100);
  const sound = clip("a", "audio", "m1", 0, 100);
  const other = clip("x", "audio", "m2", 0, 100);
  const shifted = clip("s", "audio", "m1", 10, 100);

  it("pairs a video with the audio clip taken from it", () => {
    assert.deepEqual(linkedIds([video, sound, other], ["v"]).sort(), ["a", "v"]);
  });

  it("does not pair clips from other media, or once one has been moved", () => {
    assert.deepEqual(linkedIds([video, other, shifted], ["v"]), ["v"]);
  });

  it("leaves an unlinked selection alone", () => {
    assert.deepEqual(linkedIds([video, sound], []), []);
  });
});
