// Run: node --test --experimental-strip-types dev/caption-look.test.mts
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { inherit, reader, textLook, withLive } from "../src/look.ts";
import type { Clip, Library, Project, Track } from "../src/types.ts";

/** The properties these tests read, with the defaults the library gives them. */
const lib = {
  properties: [
    { key: "font", default: "Segoe UI" },
    { key: "font_size", default: 100 },
    { key: "bold", default: true },
    { key: "color", default: "#ffffff" },
    { key: "stroke_color", default: "#000000" },
    { key: "stroke_size", default: 5 },
    { key: "shadow", default: true },
    { key: "tracking", default: 0 },
    { key: "uppercase", default: false },
    { key: "align", default: "Centre" },
    { key: "line_width", default: 90 },
    { key: "bg_color", default: "#000000" },
    { key: "bg_opacity", default: 0 },
  ],
  effects: [],
  transitions: [],
  caption_track_keys: [],
} as unknown as Library;

const caption = (props: Record<string, unknown> = {}): Clip =>
  ({ id: "k1", track: "T1", start: 0, duration: 30, source_in: 0, name: "hi", kind: "caption", media: null, text: "hi", template: null, props, effects: [] }) as unknown as Clip;

const captionTrack = (props?: Record<string, unknown>): Track => ({ id: "T1", name: "Captions", kind: "caption", locked: false, muted: false, hidden: false, props });

describe("caption settings", () => {
  it("gives a caption its track's settings, with its own values on top", () => {
    const c = inherit(caption({ font_size: 120 }), [captionTrack({ align: "Left", font_size: 80 })]);
    assert.equal(c.props.align, "Left");
    assert.equal(c.props.font_size, 120);
  });

  it("returns other clips, and captions on a track with no settings, unchanged", () => {
    const video = { ...caption(), kind: "video" } as unknown as Clip;
    assert.equal(inherit(video, [captionTrack({ align: "Left" })]), video);
    const bare = caption();
    assert.equal(inherit(bare, [captionTrack()]), bare);
  });

  it("shows a dragged track setting on the captions, and leaves the project as it was", () => {
    const p = { settings: { width: 1920, height: 1080, fps_num: 30, fps_den: 1 }, media: [], sequence: { name: "s", tracks: [captionTrack({ font_size: 80 })], clips: [], markers: [], in_point: null, out_point: null } } as unknown as Project;
    const shown = withLive(p, { clip: "T1", values: { font_size: 150 } });
    assert.equal(shown.sequence.tracks[0].props?.font_size, 150);
    assert.equal(p.sequence.tracks[0].props?.font_size, 80);
    assert.equal(withLive(p, null), p);
    assert.equal(withLive(p, { clip: "k1", values: { font_size: 1 } }), p, "a clip being dragged does not change the track");
  });
});

describe("caption look", () => {
  it("aligns, sizes the line and capitalises a caption from its settings", () => {
    const look = textLook(reader(caption({ align: "Right", line_width: 70, uppercase: true }), lib, null)) as Record<string, unknown>;
    assert.equal(look["--align"], "right");
    assert.equal(look["--lw"], "70%");
    assert.equal(look.textTransform, "uppercase");
  });

  it("draws no box while its opacity is 0, and a translucent one otherwise", () => {
    const none = textLook(reader(caption(), lib, null)) as Record<string, unknown>;
    assert.equal(none["--bg"], "transparent");
    assert.equal(none["--bgpad"], "0");
    const box = textLook(reader(caption({ bg_color: "#102030", bg_opacity: 50 }), lib, null)) as Record<string, unknown>;
    assert.equal(box["--bg"], "rgba(16, 32, 48, 0.5)");
    assert.equal(box["--bgpad"], "0.12em 0.3em");
  });

  it("leaves capitals off and centres the text unless a caption says otherwise", () => {
    const look = textLook(reader(caption(), lib, null)) as Record<string, unknown>;
    assert.equal(look.textTransform, undefined);
    assert.equal(look["--align"], "center");
    assert.equal(look["--lw"], "90%");
  });
});
