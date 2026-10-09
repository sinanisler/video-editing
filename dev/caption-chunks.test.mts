// Run: node --test --experimental-strip-types dev/caption-chunks.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { chunkCues, chunkRuleOf } from "../src/subtitles.ts";

const cue = (text: string, start: number, end: number) => ({ start, end, text });

describe("chunkRuleOf", () => {
  it("reads the limits from a captions track, where a missing, zero or negative one is off", () => {
    assert.deepEqual(chunkRuleOf(undefined), { words: 0, chars: 0, seconds: 0 });
    assert.deepEqual(chunkRuleOf({ chunk_words: 3.7, chunk_chars: 0, chunk_seconds: -1 }), { words: 3, chars: 0, seconds: 0 });
    assert.deepEqual(chunkRuleOf({ chunk_chars: 42, chunk_seconds: 2.5 }), { words: 0, chars: 42, seconds: 2.5 });
  });
});

describe("chunkCues", () => {
  it("leaves cues as they are while every limit is off", () => {
    const cues = [cue("one two\nthree", 0, 2)];
    assert.equal(chunkCues(cues, chunkRuleOf({})), cues);
    assert.equal(chunkCues(cues, chunkRuleOf({ chunk_words: 0, chunk_chars: -3, chunk_seconds: 0 })), cues);
  });

  it("splits into groups of at most the word limit", () => {
    const parts = chunkCues([cue("one two three four five six seven", 0, 7)], chunkRuleOf({ chunk_words: 3 }));
    assert.deepEqual(
      parts.map((p) => p.text),
      ["one two three", "four five six", "seven"],
    );
  });

  it("shares a cue's time between its words by their length, from the cue's start to its end", () => {
    // "a" has 1 letter, "bbb" 3 and "cc" 2. Over 8 seconds, "a bbb" covers 4 of the 6 letters.
    const [first, second] = chunkCues([cue("a bbb cc", 10, 18)], chunkRuleOf({ chunk_words: 2 }));
    assert.equal(first.start, 10);
    assert.ok(Math.abs(first.end - (10 + (8 * 4) / 6)) < 1e-9, `first ends at ${first.end}`);
    assert.ok(Math.abs(second.start - first.end) < 1e-9);
    assert.equal(second.end, 18);
  });

  it("keeps each group within the character limit, counting the spaces between words", () => {
    const parts = chunkCues([cue("alpha beta gamma delta", 0, 4)], chunkRuleOf({ chunk_chars: 10 }));
    assert.deepEqual(
      parts.map((p) => p.text),
      ["alpha beta", "gamma", "delta"],
    );
  });

  it("keeps each group within the time limit", () => {
    // Ten words of equal length, one second each: a group of three ends at exactly three seconds, which is still allowed.
    const words = Array.from({ length: 10 }, (_, i) => `w${i}`).join(" ");
    const parts = chunkCues([cue(words, 0, 10)], chunkRuleOf({ chunk_seconds: 3 }));
    assert.deepEqual(
      parts.map((p) => p.text.split(" ").length),
      [3, 3, 3, 1],
    );
    assert.equal(parts[0].end, 3);
  });

  it("leaves a word longer than the limit whole, and keeps the next word in its own group", () => {
    const parts = chunkCues([cue("extraordinary ok", 0, 2)], chunkRuleOf({ chunk_chars: 4 }));
    assert.deepEqual(
      parts.map((p) => p.text),
      ["extraordinary", "ok"],
    );
  });

  it("keeps a cue that already fits as one cue", () => {
    const [only] = chunkCues([cue("short and sweet", 3, 5)], chunkRuleOf({ chunk_words: 5 }));
    assert.deepEqual(only, { start: 3, end: 5, text: "short and sweet" });
  });
});
