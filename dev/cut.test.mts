// Run: node --test --experimental-strip-types dev/cut.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { cutTargets } from "../src/util.ts";

const clip = (id: string, track: string, start: number, duration: number) => ({ id, track, start, duration }) as never;
const track = (id: string, locked = false) => ({ id, locked }) as never;

describe("cut targets", () => {
  const tracks = [track("V1"), track("V2"), track("A1", true)];
  const clips = [clip("v1", "V1", 0, 100), clip("v2", "V2", 50, 100), clip("a1", "A1", 0, 100), clip("v3", "V1", 100, 50)];

  it("takes the clips the frame falls strictly inside", () => {
    assert.deepEqual(cutTargets(clips, tracks, 60).map((c) => c.id), ["v1", "v2"]);
  });

  it("leaves out clips that start or end exactly at the frame, since a cut there leaves an empty part", () => {
    assert.deepEqual(cutTargets(clips, tracks, 100).map((c) => c.id), ["v2"]);
    assert.deepEqual(cutTargets(clips, tracks, 0).map((c) => c.id), []);
  });

  it("leaves out clips on locked tracks", () => {
    assert.deepEqual(cutTargets(clips, tracks, 30).map((c) => c.id), ["v1"]);
  });

  it("returns nothing when no clip is under the frame", () => {
    assert.deepEqual(cutTargets(clips, tracks, 500), []);
  });
});
