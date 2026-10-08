// Run: node --test --experimental-strip-types dev/split.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { COLLAPSED_PX, splitDrag, type SplitState } from "../src/util.ts";

const videos = ["V5", "V4", "V3", "V2", "V1"];
const folded = (ids: string[]) => Object.fromEntries(ids.map((id) => [id, true]));
const all = (value: boolean) => Object.fromEntries(videos.map((id) => [id, value]));

// The screenshot: V3 and V2 folded by hand, the other video tracks open at 46 px.
const handFolded = (over: Partial<SplitState> = {}): SplitState => ({
  videoH: 46,
  collapsed: folded(["V3", "V2"]),
  trackHeights: {},
  ...over,
});

describe("divider with tracks folded by hand", () => {
  it("leaves the folded tracks folded when pulled down", () => {
    const r = splitDrag(handFolded(), videos, 5);
    assert.equal(r.folded, false);
    assert.deepEqual(r.collapsed, { V5: false, V4: false, V3: true, V2: true, V1: false });
  });

  it("moves the line exactly as far as the pointer, counting only the open tracks", () => {
    const up = splitDrag(handFolded(), videos, 30);
    assert.equal(3 * (up.videoH - 46), 30);
    const down = splitDrag(handFolded(), videos, -12);
    assert.equal(3 * (down.videoH - 46), -12);
  });

  it("folds the open tracks together when pulled below the minimum, and the folded ones stay folded", () => {
    const r = splitDrag(handFolded(), videos, -80);
    assert.equal(r.folded, true);
    assert.deepEqual(r.collapsed, all(true));
  });

  it("opens the open tracks again when pulled back out, and the hand folds stay", () => {
    splitDrag(handFolded(), videos, -80);
    const back = splitDrag(handFolded(), videos, -20);
    assert.equal(back.folded, false);
    assert.deepEqual(back.collapsed, { V5: false, V4: false, V3: true, V2: true, V1: false });
  });

  it("clears custom heights only on the tracks the divider moves", () => {
    const r = splitDrag(handFolded({ trackHeights: { V5: 70, V3: 60 } }), videos, 10);
    assert.deepEqual(r.trackHeights, { V3: 60 });
  });
});

describe("divider with no folds", () => {
  it("moves all the video tracks together", () => {
    const r = splitDrag(handFolded({ collapsed: {} }), videos, 25);
    assert.equal(r.videoH, 51);
    assert.deepEqual(r.collapsed, all(false));
  });
});

describe("divider with the whole video group folded", () => {
  const allFolded = handFolded({ collapsed: folded(videos) });

  it("stays folded until the pointer has moved far enough", () => {
    assert.equal(splitDrag(allFolded, videos, 20).folded, true);
  });

  it("opens every track when pulled out, and the line follows the pointer from the folded line", () => {
    const r = splitDrag(allFolded, videos, 30);
    assert.equal(r.folded, false);
    assert.deepEqual(r.collapsed, all(false));
    assert.equal(r.videoH, 24);
    assert.equal(videos.length * (r.videoH - COLLAPSED_PX), 30);
  });
});

describe("divider with no video tracks", () => {
  it("only changes the height it would give an open track", () => {
    const r = splitDrag({ videoH: 46, collapsed: {}, trackHeights: {} }, [], 10);
    assert.equal(r.folded, false);
    assert.equal(r.videoH, 56);
    assert.deepEqual(r.collapsed, {});
  });
});
