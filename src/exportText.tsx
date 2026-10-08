// Draws cards and captions for an export. FFmpeg renders the footage, but text is styled with this
// app's CSS, so the window draws it: each distinct picture of a track's text becomes a transparent
// PNG, made from the same component the preview uses, and the export lays those over the video.

import { renderToStaticMarkup } from "react-dom/server";
import { LayerView, shownAt } from "./components/Preview";
import { presence } from "./look";
import type { Clip, Library, Project } from "./types";
import { clipEnd } from "./util";

/** x, y, width, height of the part of a picture that has anything in it. */
type Rect = [number, number, number, number];

export interface TextSpan {
  from: number;
  to: number;
  image: string;
  /** Lets the export lay only this part over the video, which is much faster than a whole frame. */
  rect: Rect;
}

export interface TextTrack {
  track: string;
  spans: TextSpan[];
}

const isText = (c: Clip) => c.kind === "graphic" || c.kind === "caption";

/** The style rules the stage needs, gathered from the page so the pictures follow the app's stylesheet. */
function stageCss(): string {
  const wanted = /#stage|\.g-|\.layer|:root/;
  const rules: string[] = [];
  for (const sheet of Array.from(document.styleSheets)) {
    try {
      for (const rule of Array.from(sheet.cssRules)) {
        if (rule instanceof CSSStyleRule && wanted.test(rule.selectorText)) rules.push(rule.cssText);
      }
    } catch {
      // A stylesheet from another origin cannot be read; the app has none of its own there.
    }
  }
  return rules.join("\n");
}

/** The smallest box around every pixel that is not fully see-through. */
function inkRect(ctx: CanvasRenderingContext2D, width: number, height: number): Rect {
  const pixels = new Uint32Array(ctx.getImageData(0, 0, width, height).data.buffer);
  let [left, top, right, bottom] = [width, height, -1, -1];
  for (let y = 0, i = 0; y < height; y++) {
    for (let x = 0; x < width; x++, i++) {
      // Alpha is the top byte on every machine the app runs on.
      if (pixels[i] >>> 24) {
        if (x < left) left = x;
        if (x > right) right = x;
        if (y < top) top = y;
        bottom = y;
      }
    }
  }
  return right < 0 ? [0, 0, 2, 2] : [left, top, right - left + 1, bottom - top + 1];
}

/** Turns stage markup into PNG bytes by drawing it as an SVG picture on a canvas. */
async function rasterise(markup: string, css: string, width: number, height: number, canvas: HTMLCanvasElement): Promise<{ png: Blob; rect: Rect }> {
  const holder = document.createElement("div");
  holder.innerHTML = markup;
  const body = Array.from(holder.childNodes, (n) => new XMLSerializer().serializeToString(n)).join("");
  // The page's line height is a ratio; the computed value is in pixels and would not grow with the text.
  const page = getComputedStyle(document.body);
  const font = `font-family:${page.fontFamily.replace(/"/g, "'")};line-height:${parseFloat(page.lineHeight) / parseFloat(page.fontSize) || 1.4}`;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">` +
    `<style><![CDATA[${css}]]></style>` +
    `<foreignObject width="100%" height="100%">` +
    `<div xmlns="http://www.w3.org/1999/xhtml" style="position:relative;width:${width}px;height:${height}px;${font};color:#fff">` +
    `<div id="stage" style="background:none;outline:none">${body}</div></div></foreignObject></svg>`;
  const img = new Image();
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await img.decode();
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  ctx.clearRect(0, 0, width, height);
  ctx.drawImage(img, 0, 0, width, height);
  const rect = inkRect(ctx, width, height);
  return new Promise((resolve, reject) => canvas.toBlob((png) => (png ? resolve({ png, rect }) : reject(new Error("the text could not be drawn"))), "image/png"));
}

export async function blobBase64(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/**
 * Works out which text picture shows on which frames of each track between `from` and `to`, and
 * hands every distinct picture to `save`. A clip that holds still is drawn once; one that moves
 * (a transition, or shake) is drawn frame by frame.
 */
export async function renderText(
  p: Project,
  lib: Library,
  size: { width: number; height: number },
  range: { from: number; to: number },
  save: (name: string, png: Blob) => Promise<void>,
  progress: (done: number, total: number) => void,
): Promise<TextTrack[]> {
  const { clips, tracks } = p.sequence;
  const canvas = document.createElement("canvas");
  canvas.width = size.width;
  canvas.height = size.height;
  const css = stageCss();
  await save("blank.png", (await rasterise("", css, size.width, size.height, canvas)).png);

  /** The text of one track at one frame, as markup; empty when the track shows no text there. */
  const markupAt = (track: string, frame: number) =>
    shownAt(p, frame)
      .filter((s) => s.c.track === track && isText(s.c))
      .map((s) => renderToStaticMarkup(<LayerView s={s} p={p} lib={lib} live={null} />))
      .join("");

  // First pass: what to show where, as markup. Second pass: draw each distinct markup once.
  const plans: { track: string; spans: { from: number; to: number; markup: string }[] }[] = [];
  for (const t of tracks) {
    if (t.kind === "audio" || t.hidden) continue;
    const own = clips.filter((c) => c.track === t.id).sort((a, b) => a.start - b.start);
    if (!own.some(isText)) continue;
    const spans: { from: number; to: number; markup: string }[] = [];
    const add = (from: number, to: number, markup: string) => {
      const last = spans[spans.length - 1];
      if (!markup || to <= from) return;
      if (last && last.to === from && last.markup === markup) last.to = to;
      else spans.push({ from, to, markup });
    };
    for (const c of own) {
      const from = Math.max(c.start, range.from);
      // A clip that only matters for the text leaving under it is looked at for the length of its way in.
      const to = Math.min(isText(c) ? clipEnd(c) : c.start + (c.transition_in?.duration ?? 0), clipEnd(c), range.to);
      // A text clip also shows while the next clip's transition comes in over it, which the next clip's frames cover.
      const follows = own.some((x) => isText(x) && clipEnd(x) === c.start) && c.transition_in;
      if (!isText(c) && !follows) continue;
      let f = from;
      while (f < to) {
        const still = isText(c) && !presence(c, f).name && !c.effects.includes("Shake");
        if (!still) {
          add(f, f + 1, markupAt(t.id, f));
          f += 1;
          continue;
        }
        // Holds still until its way out begins.
        const until = Math.min(to, c.transition_out ? Math.max(f + 1, clipEnd(c) - c.transition_out.duration) : to);
        add(f, until, markupAt(t.id, f));
        f = until;
      }
    }
    if (spans.length) plans.push({ track: t.id, spans });
  }

  const names = new Map<string, { image: string; rect: Rect }>();
  const distinct = new Set(plans.flatMap((x) => x.spans.map((s) => s.markup))).size;
  const out: TextTrack[] = [];
  for (const plan of plans) {
    const spans: TextSpan[] = [];
    for (const s of plan.spans) {
      let drawn = names.get(s.markup);
      if (!drawn) {
        const { png, rect } = await rasterise(s.markup, css, size.width, size.height, canvas);
        drawn = { image: `text-${names.size + 1}.png`, rect };
        names.set(s.markup, drawn);
        await save(drawn.image, png);
        progress(names.size, distinct);
      }
      spans.push({ from: s.from, to: s.to, ...drawn });
    }
    out.push({ track: plan.track, spans });
  }
  return out;
}
