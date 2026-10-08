// Run: node --test --experimental-strip-types dev/media-sort.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { pickMedia, sortMedia } from "../src/util.ts";

const item = (id: string, name: string, kind: "video" | "audio" | "image", duration: number) =>
  ({ id, name, kind, duration, path: `C:/${name}`, width: 0, height: 0, has_audio: false }) as never;

describe("media order", () => {
  // Imported in this order: m1 first.
  const pool = [item("m1", "beta.mp4", "video", 300), item("m2", "Alpha.wav", "audio", 100), item("m3", "logo.png", "image", 0), item("m4", "clip10.mp4", "video", 300), item("m5", "clip2.mp4", "video", 50)];
  const ids = (list: { id: string }[]) => list.map((m) => m.id);

  it("keeps import order, oldest first, and reverses it for newest first", () => {
    assert.deepEqual(ids(sortMedia(pool, "imported", false)), ["m1", "m2", "m3", "m4", "m5"]);
    assert.deepEqual(ids(sortMedia(pool, "imported", true)), ["m5", "m4", "m3", "m2", "m1"]);
  });

  it("sorts by name without caring about case, and numbers in names count as numbers", () => {
    assert.deepEqual(ids(sortMedia(pool, "name", false)), ["m2", "m1", "m5", "m4", "m3"]);
    assert.deepEqual(ids(sortMedia(pool, "name", true)), ["m3", "m4", "m5", "m1", "m2"]);
  });

  it("sorts by length, with the name breaking ties", () => {
    // Images have no length, so they come first; the two 300-frame videos are told apart by name.
    assert.deepEqual(ids(sortMedia(pool, "length", false)), ["m3", "m5", "m2", "m1", "m4"]);
    assert.deepEqual(ids(sortMedia(pool, "length", true)), ["m4", "m1", "m2", "m5", "m3"]);
  });

  it("sorts by kind: video, then audio, then images", () => {
    // Videos first, ordered by name among themselves; then the one audio file; then the image.
    assert.deepEqual(ids(sortMedia(pool, "type", false)), ["m1", "m5", "m4", "m2", "m3"]);
  });

  it("sorts by how many clips use each item, unused first, with the name breaking ties", () => {
    const uses = new Map([["m4", 2], ["m1", 1]]);
    assert.deepEqual(ids(sortMedia(pool, "clips", false, uses)), ["m2", "m5", "m3", "m1", "m4"]);
    assert.deepEqual(ids(sortMedia(pool, "clips", true, uses)), ["m4", "m1", "m3", "m5", "m2"]);
  });

  it("does not change the list it was given", () => {
    const before = ids(pool);
    sortMedia(pool, "name", true);
    assert.deepEqual(ids(pool), before);
  });
});

describe("media selection", () => {
  it("a plain click selects only the item clicked", () => {
    assert.deepEqual(pickMedia(["a", "b"], "c", false), ["c"]);
  });

  it("Ctrl or Shift adds the item, and adds nothing twice", () => {
    assert.deepEqual(pickMedia(["a"], "b", true), ["a", "b"]);
    assert.deepEqual(pickMedia(["a", "b"], "a", true), ["b"]);
  });
});
