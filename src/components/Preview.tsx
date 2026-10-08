import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { runCommand } from "../commands";
import { clipGain, clipLook, outgoingLook, presence, reader, textLook, transitionLook } from "../look";
import { assetUrl } from "../media";
import { commitProps, core, previewProps, setPlaying, toast, ui } from "../store";
import type { Live } from "../look";
import type { Clip, Library, MediaItem, Project } from "../types";
import { clamp, clipEnd, fpsOf, hue, timecode } from "../util";

const SNAP_PX = 8;

/**
 * Plays one clip's file with a media element and keeps it in step with the playhead.
 * This is the interim player: each clip is its own element, so a cut can hitch for a moment
 * while the next one starts. The WebCodecs compositor replaces it.
 */
function Media(props: { c: Clip; m: MediaItem; fps: number; frame: number; frozen: boolean; gain: number }) {
  const { c, m, fps, frame, frozen, gain } = props;
  const ref = useRef<HTMLMediaElement>(null);
  const playing = ui.use((s) => s.playing);
  const rate = ui.use((s) => s.rate);
  const speed = typeof c.props.speed === "number" ? c.props.speed / 100 : 1;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const t = (c.source_in + (frame - c.start) * speed) / fps;
    el.volume = gain;
    if (playing && rate > 0 && !frozen) {
      const wanted = Math.min(16, rate * speed);
      if (el.playbackRate !== wanted) el.playbackRate = wanted;
      if (el.paused) {
        el.currentTime = t;
        el.play().catch(() => {});
      } else if (Math.abs(el.currentTime - t) > 0.3) {
        el.currentTime = t;
      }
    } else {
      // Paused, scrubbing, playing backwards or held on its last frame: show exactly this frame.
      if (!el.paused) el.pause();
      if (Math.abs(el.currentTime - t) > 0.5 / fps) el.currentTime = t;
    }
  });

  const src = assetUrl(m.path);
  if (m.kind === "audio") return <audio ref={ref as React.RefObject<HTMLAudioElement>} src={src} preload="auto" />;
  return <video ref={ref as React.RefObject<HTMLVideoElement>} src={src} preload="auto" />;
}

/** Text clips. The element marked `data-hit` is what the pointer grabs and what the selection box hugs. */
function Graphic({ c }: { c: Clip }) {
  const text = c.text ?? c.name;
  const hit = { "data-hit": c.id };
  switch (c.template) {
    case "subscribe":
      return (
        <div className="g-center">
          <div className="g-sub" {...hit}>
            <i />
            channel<b>{text}</b>
          </div>
        </div>
      );
    case "counter":
      return <div className="g-count" {...hit}>{text}</div>;
    case "chapter":
      return <div className="g-chap" {...hit}>{text}</div>;
    case "lower":
      return <div className="g-lower" {...hit}>{text}</div>;
    default:
      return (
        <div className="g-center">
          <span className={c.template === "title" ? "g-title" : "g-plain"} {...hit}>{text}</span>
        </div>
      );
  }
}

export interface Shown {
  c: Clip;
  /** Timeline frame to draw the clip at. */
  frame: number;
  /** A previous clip held on its last frame under an incoming transition: its name and progress. */
  leaving?: { name: string; t: number };
}

/** What is on screen at `frame`: bottom track first, so higher tracks draw on top. Hidden tracks are left out. */
export function shownAt(p: Project, frame: number): Shown[] {
  const { tracks, clips } = p.sequence;
  const shown: Shown[] = [];
  for (const t of [...tracks].reverse()) {
    if (t.kind === "audio" || t.hidden) continue;
    const c = clips.find((x) => x.track === t.id && frame >= x.start && frame < clipEnd(x));
    if (!c) continue;
    const pr = presence(c, frame);
    const previous = pr.edge === "in" ? clips.find((x) => x.track === t.id && clipEnd(x) === c.start) : undefined;
    if (previous && pr.name) shown.push({ c: previous, frame: clipEnd(previous) - 1, leaving: { name: pr.name, t: pr.t } });
    shown.push({ c, frame });
  }
  return shown;
}

/** One clip drawn on the stage. The export draws text clips with this same component, so they come out as they look here. */
export function LayerView({ s, p, lib, live }: { s: Shown; p: Project; lib: Library; live: Live | null }) {
  const { c, frame: f, leaving } = s;
  const fps = fpsOf(p);
  const r = reader(c, lib, live);
  const look = clipLook(c, f, p, r);
  const pr = presence(c, f);
  const hasPrevious = p.sequence.clips.some((x) => x.track === c.track && clipEnd(x) === c.start);
  const tr = leaving ? { style: outgoingLook(leaving.name, leaving.t) } : transitionLook(pr, hasPrevious, f);
  const m = p.media.find((x) => x.id === c.media);
  const isText = c.kind === "graphic" || c.kind === "caption";
  return (
    <div className="layer" style={look.outer}>
      <div className="layer" style={tr.style}>
        <div className="layer" data-frame="1" style={look.frame}>
          {/* Pictures are grabbed by their crop window; text by the text itself. */}
          <div className="cropbox" style={look.crop} {...(isText || leaving ? null : { "data-hit": c.id })}>
            <div className="content" style={{ ...look.content, ...(isText ? textLook(r) : null) }}>
              {c.kind === "graphic" ? (
                <Graphic c={c} />
              ) : c.kind === "caption" ? (
                <div className="g-caption">
                  <span data-hit={c.id}>{c.text}</span>
                </div>
              ) : m?.kind === "image" ? (
                <img src={assetUrl(m.path)} draggable={false} />
              ) : m ? (
                <Media c={c} m={m} fps={fps} frame={f} frozen={!!leaving} gain={leaving ? 0 : clipGain(c, f, fps, r)} />
              ) : (
                <div className="missing" style={{ background: `linear-gradient(135deg,hsl(${hue(c.name)} 45% 28%),hsl(${(hue(c.name) + 50) % 360} 50% 14%))` }}>
                  {c.name}
                  <small>no media</small>
                </div>
              )}
            </div>
            {look.vignette > 0 && <div className="vignette" style={{ opacity: look.vignette }} />}
            {look.border && <div className="edgeline" style={{ boxShadow: look.border }} />}
          </div>
        </div>
      </div>
      {tr.overlay && <div className="layer" style={{ background: tr.overlay.color, opacity: tr.overlay.opacity }} />}
    </div>
  );
}

/** Where the selected clip's grab box sits inside its (untransformed) frame, as percentages of the stage, so it survives a resize. */
interface Box {
  id: string;
  l: number;
  t: number;
  w: number;
  h: number;
}

function Stage() {
  const p = core.use((s) => s.state)!.project;
  const lib = core.use((s) => s.library)!;
  const frame = ui.use((s) => s.frame);
  const live = ui.use((s) => s.live);
  const selection = ui.use((s) => s.selection);
  const safeArea = ui.use((s) => s.safeArea);
  const stage = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<Box | null>(null);
  const [guides, setGuides] = useState({ v: false, h: false });

  const fps = fpsOf(p);
  const { width: W, height: H } = p.settings;
  const { tracks, clips } = p.sequence;
  const at = (track: string) => clips.find((c) => c.track === track && frame >= c.start && frame < clipEnd(c));
  const media = (c: Clip) => p.media.find((x) => x.id === c.media);

  const shown = shownAt(p, frame);
  const sounding = tracks.filter((t) => t.kind === "audio" && !t.muted).map((t) => at(t.id)).filter((c): c is Clip => !!c);

  // The transform box shows for a single selected clip that is on screen now.
  const picked = selection.length === 1 ? shown.find((s) => s.c.id === selection[0] && !s.leaving)?.c : undefined;

  // Measure the grab box from layout, which ignores transforms; the box is then drawn under the same transform.
  useLayoutEffect(() => {
    const el = picked && stage.current?.querySelector<HTMLElement>(`[data-hit="${picked.id}"]`);
    if (!el || !picked) {
      if (box) setBox(null);
      return;
    }
    let l = 0;
    let t = 0;
    for (let n: HTMLElement | null = el; n && !n.dataset.frame; n = n.offsetParent as HTMLElement | null) {
      l += n.offsetLeft;
      t += n.offsetTop;
    }
    const sw = stage.current!.clientWidth / 100;
    const sh = stage.current!.clientHeight / 100;
    const next = { id: picked.id, l: l / sw, t: t / sh, w: el.offsetWidth / sw, h: el.offsetHeight / sh };
    const same = box && box.id === next.id && (["l", "t", "w", "h"] as const).every((k) => Math.abs(next[k] - box[k]) < 0.05);
    if (!same) setBox(next);
  });

  /** Click to select, then drag the layer to move it, a corner to scale it, or the top knob to rotate it. */
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0 || !stage.current) return;
    const target = e.target as HTMLElement;
    const handle = target.dataset.handle;
    const id = handle ? picked?.id : target.closest<HTMLElement>("[data-hit]")?.dataset.hit;
    const c = clips.find((x) => x.id === id);
    if (!c) return ui.set({ selection: [] });
    if (tracks.find((t) => t.id === c.track)?.locked) return toast(`Track ${c.track} is locked`);
    e.preventDefault();
    setPlaying(false);
    ui.set({ selection: [c.id], rightTab: "inspector" });

    const rect = stage.current.getBoundingClientRect();
    const r = reader(c, lib, null);
    const from = { x: r.n("pos_x"), y: r.n("pos_y"), zx: r.n("zoom_x"), zy: r.n("zoom_y"), rot: r.n("rotation") };
    // The point the clip scales and rotates around, on screen.
    const pivot = {
      x: rect.left + rect.width / 2 + ((from.x + r.n("anchor_x")) / W) * rect.width,
      y: rect.top + rect.height / 2 - ((from.y + r.n("anchor_y")) / H) * rect.height,
    };
    const start = { x: e.clientX, y: e.clientY };
    const angle = (x: number, y: number) => (Math.atan2(y - pivot.y, x - pivot.x) * 180) / Math.PI;
    let values: Record<string, unknown> | null = null;

    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - start.x;
      const dy = ev.clientY - start.y;
      if (!values && Math.hypot(dx, dy) < 3) return;
      if (handle === "rot") {
        let rot = from.rot + angle(ev.clientX, ev.clientY) - angle(start.x, start.y);
        if (ev.shiftKey) rot = Math.round(rot / 15) * 15;
        // Keep within the property's range without a visible jump.
        while (rot > 360) rot -= 360;
        while (rot < -360) rot += 360;
        values = { rotation: Math.round(rot * 10) / 10 };
      } else if (handle) {
        const grow = Math.hypot(ev.clientX - pivot.x, ev.clientY - pivot.y) / Math.max(1, Math.hypot(start.x - pivot.x, start.y - pivot.y));
        const zoom = (z: number) => clamp(Math.round(z * grow * 100) / 100, 0.01, 10);
        values = { zoom_x: zoom(from.zx), zoom_y: zoom(from.zy) };
      } else {
        // Shift keeps the move on one axis.
        let x = from.x + ((ev.shiftKey && Math.abs(dy) > Math.abs(dx) ? 0 : dx) / rect.width) * W;
        let y = from.y - ((ev.shiftKey && Math.abs(dx) >= Math.abs(dy) ? 0 : dy) / rect.height) * H;
        const reach = (SNAP_PX / rect.width) * W;
        const snapX = Math.abs(x) < reach;
        const snapY = Math.abs(y) < reach;
        if (snapX) x = 0;
        if (snapY) y = 0;
        setGuides({ v: snapX, h: snapY });
        values = { pos_x: Math.round(x), pos_y: Math.round(y) };
      }
      previewProps(c.id, values);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setGuides({ v: false, h: false });
      if (values) commitProps(c.id, values);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const pickedLook = picked && clipLook(picked, frame, p, reader(picked, lib, live));
  const pickedReader = picked && reader(picked, lib, live);
  const punch = pickedReader ? 1 + ((pickedReader.fx("Zoom punch") ?? 0) / 100) * 0.4 : 1;
  const inverse = (zoom: number) => 1 / Math.max(0.01, Math.abs(zoom * punch));

  return (
    <div id="stagebox" onPointerDown={onPointerDown}>
      <div id="stage" ref={stage}>
        {shown.map((s) => (
          <LayerView key={s.c.id} s={s} p={p} lib={lib} live={live} />
        ))}
        {sounding.map((c) => {
          const m = media(c);
          return m && <Media key={c.id} c={c} m={m} fps={fps} frame={frame} frozen={false} gain={clipGain(c, frame, fps, reader(c, lib, live))} />;
        })}
        {safeArea && <div id="safe" />}
      </div>

      {/* Drawn outside the clipped stage so handles stay reachable when a layer fills or leaves the frame. */}
      <div id="gizmos">
        {guides.v && <div className="guide v" />}
        {guides.h && <div className="guide h" />}
        {picked && pickedLook && pickedReader && box?.id === picked.id && (
          <div className="layer" style={{ transform: pickedLook.frame.transform, transformOrigin: pickedLook.frame.transformOrigin }}>
            <div
              className="gizmo"
              style={
                {
                  left: `${box.l}%`,
                  top: `${box.t}%`,
                  width: `${box.w}%`,
                  height: `${box.h}%`,
                  "--ix": inverse(pickedReader.n("zoom_x")),
                  "--iy": inverse(pickedReader.n("zoom_y")),
                } as React.CSSProperties
              }
            >
              <i className="edge" />
              {["tl", "tr", "bl", "br"].map((h) => (
                <i key={h} className={`hnd ${h}`} data-handle={h} data-tip="Drag to scale" />
              ))}
              <i className="hnd rot" data-handle="rot" data-tip="Drag to rotate (Shift: 15° steps)" />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function Transport() {
  const p = core.use((s) => s.state)!.project;
  const frame = ui.use((s) => s.frame);
  const playing = ui.use((s) => s.playing);
  const rate = ui.use((s) => s.rate);
  const safeArea = ui.use((s) => s.safeArea);
  const fps = fpsOf(p);
  const end = p.sequence.clips.reduce((m, c) => Math.max(m, clipEnd(c)), 0);
  return (
    <div id="transport">
      <span id="tc" data-tip="Playhead position: hours, minutes, seconds and frames">{timecode(frame, fps)}</span>
      <span className="mono dim" data-tip="Length of the edit: where the last clip ends">/ {timecode(end, fps)}</span>
      <span className="sp" />
      <button data-tip="Go to start (Home)" onClick={() => runCommand("home")}>⏮</button>
      <button data-tip="Back one frame (←)" onClick={() => runCommand("back")}>◀</button>
      <button className="primary" data-tip="Play / pause (Space)" onClick={() => runCommand("play")}>
        {playing ? "❚❚" : "▶"}
      </button>
      <button data-tip="Forward one frame (→)" onClick={() => runCommand("fwd")}>▶</button>
      <button data-tip="Go to end (End)" onClick={() => runCommand("end")}>⏭</button>
      <span className="mono dim rate" data-tip="Playback speed">{rate}×</span>
      <span className="sp" />
      <button data-tip="Mark in (I)" onClick={() => runCommand("in")}>I</button>
      <button data-tip="Mark out (O)" onClick={() => runCommand("out")}>O</button>
      <button className={safeArea ? "on" : ""} data-tip="Safe area guides" onClick={() => ui.set({ safeArea: !safeArea })}>
        ▢
      </button>
    </div>
  );
}

export function Preview() {
  return (
    <div id="center">
      <div id="stagewrap">
        <Stage />
      </div>
      <Transport />
    </div>
  );
}
