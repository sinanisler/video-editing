// Run: node --test --experimental-strip-types dev/volume.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { volumeAt, withVolumePoint } from "../src/volume.ts";

describe("volume line", () => {
  it("uses the clip's own volume when there are no points", () => {
    assert.equal(volumeAt([], -6, 100), -6);
  });

  it("holds the first and last levels outside the points", () => {
    const points = [
      { at: 10, db: -3 },
      { at: 20, db: -9 },
    ];
    assert.equal(volumeAt(points, 0, 0), -3);
    assert.equal(volumeAt(points, 0, 50), -9);
  });

  it("draws a straight line in dB between two points", () => {
    const points = [
      { at: 0, db: 0 },
      { at: 100, db: -20 },
    ];
    assert.equal(volumeAt(points, 0, 25), -5);
    assert.equal(volumeAt(points, 0, 100), -20);
  });

  it("uses the segment that holds the frame when there are several points", () => {
    const points = [
      { at: 0, db: 0 },
      { at: 50, db: -10 },
      { at: 100, db: 0 },
    ];
    assert.equal(volumeAt(points, 0, 75), -5);
    assert.equal(volumeAt(points, 0, 50), -10);
  });

  it("keeps points in time order and replaces one already at that frame", () => {
    let points = withVolumePoint([], { at: 50, db: -6 });
    points = withVolumePoint(points, { at: 10, db: -2 });
    points = withVolumePoint(points, { at: 50, db: -12 });
    assert.deepEqual(points, [
      { at: 10, db: -2 },
      { at: 50, db: -12 },
    ]);
  });
});
