// Turns a clip's properties, effects and transitions into CSS for the interim preview.
// Everything the library defines must be handled here; `unimplemented` reports what is not.

import type { CSSProperties } from "react";
import type { Clip, Library, PropDef, Project } from "./types";

/** Values being dragged in the inspector, shown before they are committed. */
export interface Live {
  clip: string;
  values: Record<string, unknown>;
}

export interface Reader {
  n: (key: string) => number;
  b: (key: string) => boolean;
  s: (key: string) => string;
  /** Strength of an effect, 0 to 100, or null when the clip does not have it. */
  fx: (name: string) => number | null;
  /** True when the clip stores its own value for the key. */
  isSet: (key: string) => boolean;
}

export function reader(c: Clip, lib: Library, live: Live | null): Reader {
  const defs = new Map<string, PropDef>(lib.properties.map((p) => [p.key, p]));
  const raw = (key: string): unknown => (live?.clip === c.id && key in live.values ? live.values[key] : c.props[key]);
  const get = (key: string): unknown => raw(key) ?? defs.get(key)?.default;
  return {
    n: (key) => Number(get(key) ?? 0),
    b: (key) => Boolean(get(key)),
    s: (key) => String(get(key) ?? ""),
    fx: (name) => {
      if (!c.effects.includes(name)) return null;
      const v = raw(`fx.${name}`);
      return typeof v === "number" ? v : (lib.effects.find((e) => e.name === name)?.amount ?? 50);
    },
    isSet: (key) => c.props[key] !== undefined && c.props[key] !== null,
  };
}

const BLEND: Record<string, CSSProperties["mixBlendMode"]> = {
  Normal: "normal",
  Add: "plus-lighter",
  Multiply: "multiply",
  Screen: "screen",
  Overlay: "overlay",
  Darken: "darken",
  Lighten: "lighten",
  "Soft Light": "soft-light",
  "Hard Light": "hard-light",
  Difference: "difference",
  Exclusion: "exclusion",
  "Color Dodge": "color-dodge",
  "Color Burn": "color-burn",
  Hue: "hue",
  Saturation: "saturation",
  Color: "color",
  Luminosity: "luminosity",
};

/** Effects drawn through CSS filters on the picture: name → filter for a 0–100 amount. */
const FILTERS: Record<string, (a: number) => string> = {
  Greyscale: (a) => `grayscale(${a / 100})`,
  Sepia: (a) => `sepia(${a / 100})`,
  Invert: (a) => `invert(${a / 100})`,
  Blur: (a) => `blur(${a * 0.04}cqh)`,
  Brightness: (a) => `brightness(${a / 50})`,
  Contrast: (a) => `contrast(${a / 50})`,
  Saturation: (a) => `saturate(${a / 50})`,
  "Hue shift": (a) => `hue-rotate(${a * 3.6}deg)`,
};
/** Effects handled elsewhere in `clipLook`. */
const OTHER_EFFECTS = ["Vignette", "Zoom punch", "Shake", "Rounded corners", "Border", "Drop shadow"];

export interface ClipLook {
  /** Moves, scales and rotates the whole layer. */
  frame: CSSProperties;
  /** The visible window after cropping. */
  crop: CSSProperties;
  /** The picture itself, counter-shifted so cropping does not move it. */
  content: CSSProperties;
  /** Opacity and blend mode against the layers below. */
  outer: CSSProperties;
  vignette: number;
  border: string | null;
}

export function clipLook(c: Clip, frame: number, p: Project, r: Reader): ClipLook {
  const { width: W, height: H } = p.settings;
  const punch = 1 + ((r.fx("Zoom punch") ?? 0) / 100) * 0.4;
  const shake = (r.fx("Shake") ?? 0) / 100;
  const dx = (r.n("pos_x") / W) * 100 + Math.sin(frame * 1.7) * shake * 2;
  const dy = (-r.n("pos_y") / H) * 100 + Math.cos(frame * 2.3) * shake * 1.2;
  const tilt = r.n("pitch") || r.n("yaw") ? `perspective(160cqh) rotateX(${r.n("pitch")}deg) rotateY(${r.n("yaw")}deg) ` : "";
  const sx = r.n("zoom_x") * punch * (r.b("flip_h") ? -1 : 1);
  const sy = r.n("zoom_y") * punch * (r.b("flip_v") ? -1 : 1);
  const shadow = r.fx("Drop shadow");

  const l = (r.n("crop_left") / W) * 100;
  const rt = (r.n("crop_right") / W) * 100;
  const t = (r.n("crop_top") / H) * 100;
  const b = (r.n("crop_bottom") / H) * 100;
  const round = r.fx("Rounded corners");
  const border = r.fx("Border");
  const filters = c.effects.filter((e) => FILTERS[e]).map((e) => FILTERS[e](r.fx(e) ?? 0));

  return {
    frame: {
      transform: `translate(${dx}cqw, ${dy}cqh) ${tilt}rotate(${r.n("rotation")}deg) scale(${sx}, ${sy})`,
      transformOrigin: `calc(50% + ${(r.n("anchor_x") / W) * 100}cqw) calc(50% - ${(r.n("anchor_y") / H) * 100}cqh)`,
      filter: shadow ? `drop-shadow(0 1cqh ${0.5 + shadow * 0.04}cqh rgba(0,0,0,${0.3 + shadow / 150}))` : undefined,
    },
    crop: {
      inset: `${t}cqh ${rt}cqw ${b}cqh ${l}cqw`,
      borderRadius: round ? `${round * 0.25}cqh` : undefined,
    },
    content: { left: `${-l}cqw`, top: `${-t}cqh`, filter: filters.join(" ") || undefined },
    outer: { opacity: r.n("opacity") / 100, mixBlendMode: BLEND[r.s("blend")] ?? "normal" },
    vignette: (r.fx("Vignette") ?? 0) / 100,
    border: border ? `inset 0 0 0 ${0.2 + border * 0.02}cqh #fff` : null,
  };
}

/** CSS variables and font settings for graphics and captions. */
export function textLook(r: Reader): CSSProperties {
  return {
    fontFamily: `"${r.s("font")}", sans-serif`,
    fontWeight: r.b("bold") ? 900 : 400,
    letterSpacing: `${r.n("tracking") / 100}em`,
    "--fs": r.n("font_size") / 100,
    "--txt": r.s("color"),
    "--stroke": `${r.n("stroke_size") * 0.12}cqh ${r.s("stroke_color")}`,
    "--shadow": r.b("shadow") ? "0 0.8cqh 0 #000a" : "none",
  } as CSSProperties;
}

export interface TransitionLook {
  style: CSSProperties;
  /** A flat colour over the layer, for dips. */
  overlay?: { color: string; opacity: number };
}

const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);
/** Cheap repeatable noise in 0..1 from a frame number. */
const noise = (n: number) => {
  const x = Math.sin(n * 127.1) * 43758.5453;
  return x - Math.floor(x);
};
const slide = (x: number, y: number) => (e: number) => ({ style: { transform: `translate(${x * (1 - e)}cqw, ${y * (1 - e)}cqh)` } });
const wipe = (side: 0 | 1 | 2 | 3) => (e: number) => {
  const inset = ["0", "0", "0", "0"];
  inset[side] = `${(1 - e) * 100}%`;
  return { style: { clipPath: `inset(${inset.join(" ")})` } };
};
const dip = (color: string) => (_e: number, t: number, hasPrevious: boolean) => ({
  // With a previous clip under it, the layer appears at the darkest point; otherwise it simply fades up.
  style: { opacity: hasPrevious && t < 0.5 ? 0 : 1 },
  overlay: { color, opacity: hasPrevious ? 1 - Math.abs(2 * t - 1) : 1 - t },
});

/**
 * How a transition draws the clip at presence `t`: 0 is fully absent, 1 fully there.
 * `e` is `t` eased. The same function serves the start of a clip and, mirrored, its end.
 */
const TRANSITIONS: Record<string, (e: number, t: number, hasPrevious: boolean, frame: number) => TransitionLook> = {
  "Cross dissolve": (e) => ({ style: { opacity: e } }),
  "Blur dissolve": (e) => ({ style: { opacity: e, filter: `blur(${(1 - e) * 3}cqh)` } }),
  "Dip to black": dip("#000"),
  "Dip to white": dip("#fff"),
  "Slide left": slide(100, 0),
  "Slide right": slide(-100, 0),
  "Slide up": slide(0, 100),
  "Slide down": slide(0, -100),
  "Push left": slide(100, 0),
  "Push right": slide(-100, 0),
  "Wipe left": wipe(3),
  "Wipe right": wipe(1),
  "Wipe up": wipe(0),
  "Wipe down": wipe(2),
  Iris: (e) => ({ style: { clipPath: `circle(${e * 75}% at 50% 50%)` } }),
  "Zoom in": (e) => ({ style: { transform: `scale(${0.2 + 0.8 * e})`, opacity: Math.min(1, e * 2) } }),
  "Zoom out": (e) => ({ style: { transform: `scale(${1 + (1 - e) * 0.8})`, opacity: Math.min(1, e * 2) } }),
  "Whoosh zoom": (e) => ({
    style: { transform: `scale(${1 + (1 - e) * 1.5})`, filter: `blur(${(1 - e) * 4}cqh)`, opacity: Math.min(1, e * 3) },
  }),
  Spin: (e) => ({ style: { transform: `rotate(${(1 - e) * -180}deg) scale(${0.3 + 0.7 * e})`, opacity: Math.min(1, e * 2) } }),
  Glitch: (_e, t, _p, frame) => ({
    style: {
      transform: `translate(${(noise(frame) - 0.5) * 12 * (1 - t)}cqw, ${(noise(frame + 9) - 0.5) * 4 * (1 - t)}cqh)`,
      filter: `hue-rotate(${noise(frame + 3) * 180 * (1 - t)}deg) saturate(${1 + 2 * (1 - t)})`,
      opacity: noise(frame + 5) < (1 - t) * 0.5 ? 0.25 : 1,
    },
  }),
};

/** Where the previous clip goes while the next one comes in. Only pushes move it. */
const OUTGOING: Record<string, (e: number) => CSSProperties> = {
  "Push left": (e) => ({ transform: `translate(${-100 * e}cqw, 0)` }),
  "Push right": (e) => ({ transform: `translate(${100 * e}cqw, 0)` }),
};

export interface Presence {
  /** 0..1, or 1 when no transition is running. */
  t: number;
  name: string | null;
  edge: "in" | "out" | null;
}

/** Which transition, if any, is running on the clip at `frame`, and how far along it is. */
export function presence(c: Clip, frame: number): Presence {
  const into = c.transition_in;
  if (into && frame < c.start + into.duration) {
    return { t: (frame - c.start + 0.5) / into.duration, name: into.name, edge: "in" };
  }
  const out = c.transition_out;
  if (out && frame >= c.start + c.duration - out.duration) {
    return { t: (c.start + c.duration - frame - 0.5) / out.duration, name: out.name, edge: "out" };
  }
  return { t: 1, name: null, edge: null };
}

export function transitionLook(pr: Presence, hasPrevious: boolean, frame: number): TransitionLook {
  const fn = pr.name ? TRANSITIONS[pr.name] : undefined;
  return fn ? fn(ease(pr.t), pr.t, hasPrevious && pr.edge === "in", frame) : { style: {} };
}

export function outgoingLook(name: string, t: number): CSSProperties {
  return OUTGOING[name]?.(ease(t)) ?? {};
}

/** Loudness of a clip at `frame` as 0..1: volume, fades, and any transition used as an audio fade. */
export function clipGain(c: Clip, frame: number, fps: number, r: Reader): number {
  if (r.b("mute")) return 0;
  let gain = 10 ** (r.n("volume") / 20);
  const fadeIn = r.n("fade_in") * fps;
  const fadeOut = r.n("fade_out") * fps;
  if (fadeIn > 0) gain *= Math.min(1, (frame - c.start) / fadeIn);
  if (fadeOut > 0) gain *= Math.min(1, (c.start + c.duration - frame) / fadeOut);
  return Math.max(0, Math.min(1, gain * Math.min(1, presence(c, frame).t)));
}

/** Library entries the preview has no drawing code for. Should be empty. */
export function unimplemented(lib: Library): string[] {
  return [
    ...lib.effects.filter((e) => !FILTERS[e.name] && !OTHER_EFFECTS.includes(e.name)).map((e) => `effect ${e.name}`),
    ...lib.transitions.filter((t) => !TRANSITIONS[t.name]).map((t) => `transition ${t.name}`),
    ...lib.properties.filter((p) => p.type === "choice" && p.key === "blend").flatMap((p) => (p.type === "choice" ? p.options : []).filter((o) => !BLEND[o]).map((o) => `blend mode ${o}`)),
  ];
}
