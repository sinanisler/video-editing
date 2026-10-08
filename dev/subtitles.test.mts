// Run: node --test --experimental-strip-types dev/subtitles.test.mts
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { parseSubtitles, toSrt, toVtt } from "../src/subtitles.ts";

const here = dirname(fileURLToPath(import.meta.url));
const example = (name: string) => readFileSync(join(here, "subtitles", name), "utf8");
const expected = JSON.parse(example("expected.json"));

describe("SRT parsing", () => {
  it("reads times with commas and skips index numbers", () => {
    const cues = parseSubtitles("1\n00:00:01,500 --> 00:00:03,250\nHello\n");
    assert.deepEqual(cues, [{ start: 1.5, end: 3.25, text: "Hello" }]);
  });

  it("accepts a byte order mark and Windows line endings", () => {
    const cues = parseSubtitles("﻿1\r\n00:00:01,000 --> 00:00:02,000\r\nOne\r\nTwo\r\n");
    assert.deepEqual(cues, [{ start: 1, end: 2, text: "One\nTwo" }]);
  });

  it("reads hours and milliseconds past 999", () => {
    const [cue] = parseSubtitles("1\n01:02:03,4 --> 01:02:04,45\nLong\n");
    assert.equal(cue.start, 3723.4);
    assert.equal(cue.end, 3724.45);
  });

  it("drops zero-length, backwards and empty cues", () => {
    const cues = parseSubtitles(example("tags.srt"));
    assert.deepEqual(
      cues.map((c) => c.text),
      expected["tags.srt"].texts,
    );
  });
});

describe("markup", () => {
  it("removes tags and decodes common entities", () => {
    const [cue] = parseSubtitles("1\n00:00:00,000 --> 00:00:01,000\n<i>Tom</i> &amp; Jerry &lt;3\n");
    assert.equal(cue.text, "Tom & Jerry <3");
  });

  it("keeps the text of colour and bold tags, dropping the markup", () => {
    const cues = parseSubtitles(example("tags.srt"));
    assert.equal(cues[0].text, "Yellow line");
    assert.equal(cues[1].text, "Bold and underlined");
  });
});

describe("WebVTT parsing", () => {
  it("skips the header, NOTE and STYLE blocks and cue settings", () => {
    const cues = parseSubtitles(example("styled.vtt"));
    assert.deepEqual(
      cues.map((c) => c.text),
      expected["styled.vtt"].texts,
    );
  });

  it("reads minutes-and-seconds times without hours", () => {
    const [cue] = parseSubtitles("WEBVTT\n\n00:05.000 --> 00:07.500\nShort\n");
    assert.deepEqual(cue, { start: 5, end: 7.5, text: "Short" });
  });

  it("reads a cue with an identifier line before its times", () => {
    const [cue] = parseSubtitles("WEBVTT\n\nintro\n00:00:04.000 --> 00:00:05.500\nok\n");
    assert.deepEqual(cue, { start: 4, end: 5.5, text: "ok" });
  });
});

describe("example files", () => {
  it("basic.srt has four cues and the last one starts past a minute", () => {
    const cues = parseSubtitles(example("basic.srt"));
    assert.equal(cues.length, expected["basic.srt"].cues);
    assert.deepEqual(cues[0], expected["basic.srt"].first);
    assert.equal(cues.at(-1)!.start, expected["basic.srt"].lastStart);
    assert.equal(cues[2].text, "Tom & Jerry <3");
  });

  it("styled.vtt has four cues", () => {
    assert.equal(parseSubtitles(example("styled.vtt")).length, expected["styled.vtt"].cues);
  });
});

describe("writing", () => {
  it("writes SRT with numbered cues, comma times and blank lines between", () => {
    const text = toSrt([
      { start: 1.5, end: 3.25, text: "A" },
      { start: 62, end: 64, text: "B\nC" },
    ]);
    assert.equal(text, "1\n00:00:01,500 --> 00:00:03,250\nA\n\n2\n00:01:02,000 --> 00:01:04,000\nB\nC\n");
  });

  it("writes WebVTT with a header and dot times", () => {
    const text = toVtt([{ start: 3723.4, end: 3724.45, text: "Long" }]);
    assert.equal(text, "WEBVTT\n\n01:02:03.400 --> 01:02:04.450\nLong\n");
  });

  it("round-trips through the parser", () => {
    const cues = parseSubtitles(example("basic.srt"));
    assert.deepEqual(parseSubtitles(toSrt(cues)), cues);
    assert.deepEqual(parseSubtitles(toVtt(cues)), cues);
  });
});
