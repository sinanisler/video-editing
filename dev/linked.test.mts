// Run: node --test --experimental-strip-types dev/linked.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { linkedIds } from "../src/util.ts";

const clip = (id: string, link: string | null = null, start = 0) => ({ id, link, start, duration: 100 }) as never;

describe("linked clips", () => {
  const video = clip("v", "L1");
  const sound = clip("a", "L1");
  const other = clip("x");
  const elsewhere = clip("e", "L2");

  it("selects the whole group of a clicked clip", () => {
    assert.deepEqual(linkedIds([video, sound, other], ["v"]).sort(), ["a", "v"]);
    assert.deepEqual(linkedIds([video, sound, other], ["a"]).sort(), ["a", "v"]);
  });

  it("keeps a group together wherever its clips are in time", () => {
    const movedSound = clip("a", "L1", 40);
    assert.deepEqual(linkedIds([video, movedSound], ["v"]).sort(), ["a", "v"]);
  });

  it("does not add clips without a link, or from another group", () => {
    assert.deepEqual(linkedIds([video, other, elsewhere], ["v"]), ["v"]);
    assert.deepEqual(linkedIds([video, sound, other], ["x"]), ["x"]);
  });

  it("takes just the clicked clips when linked selection is off", () => {
    assert.deepEqual(linkedIds([video, sound], ["v"], false), ["v"]);
  });

  it("leaves an empty selection alone", () => {
    assert.deepEqual(linkedIds([video, sound], []), []);
  });
});
