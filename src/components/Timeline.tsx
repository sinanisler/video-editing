import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { runCommand, splitAt, timelineApi } from "../commands";
import { core, exec, seek, toast, ui } from "../store";
import type { Clip, Project, Sequence } from "../types";
import { clamp, clipClass, clipEnd, fits, fpsOf, linkedIds, MAX_PPS, MIN_PPS, mmss, timecode, timelineFrames, trackHeight } from "../util";
import { openTrackMenu, Splitter, SPLITTER_PX, TrackHead } from "./Tracks";
import { openClipMenu } from "./ClipMenu";
import { filmstripKey, filmstripSize, frameUrl, requestFilmstrip } from "../filmstrip";
import type { MediaItem } from "../types";

/** Small frames along a video clip, one per tile, so the picture shows what is where on the timeline. */
function Filmstrip({ c, m, ppf, fps, lane }: { c: Clip; m: MediaItem; ppf: number; fps: number; lane: number }) {
  // Ask for frames sized for this track on screen, counting the screen's pixel density.
  const size = filmstripSize(lane * (window.devicePixelRatio || 1));
  const k = filmstripKey(m.id, size);
  const strip = ui.use((s) => s.filmstrips[k]);
  useEffect(() => requestFilmstrip(m, size), [m, size]);
  if (typeof strip !== "object") return null;
  // Tiles sit on the timeline at the time their frame shows. A tile is never narrower than the picture is wide, so
  // frames are never squeezed: zoomed in, each half second gets a tile; zoomed out, several frames share one.
  const pixelsPerSecond = ppf * fps;
  const minTile = Math.max(8, lane * ((m.width || 16) / (m.height || 9)));
  const steps = Math.max(1, Math.ceil(minTile / (strip.interval * pixelsPerSecond)));
  const span = steps * strip.interval;
  const tiles = Math.ceil(c.duration / fps / span);
  return (
    <div className="filmstrip">
      {Array.from({ length: tiles }, (_, i) => (
        <img
          key={i}
          src={frameUrl(strip, c.source_in / fps + i * span)}
          style={{ left: i * span * pixelsPerSecond, width: span * pixelsPerSecond }}
          draggable={false}
          alt=""
        />
      ))}
    </div>
  );
}

const SNAP_PX = 8;
const DRAG_PX = 4;
const PACE_WINDOW_S = 10;
const PACE_COLOURS = ["#3a2020", "#3d3520", "#2c4a34", "#2f7d50"];

interface Ghost {
  start: number;
  duration: number;
  track: string;
}
type Ghosts = Record<string, Ghost>;
interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

type Drag =
  | { mode: "seek" }
  | { mode: "marquee"; x0: number; y0: number; additive: string[]; moved: boolean }
  | {
      mode: "move" | "trimL" | "trimR";
      /** The clip under the pointer; snapping follows it. */
      lead: Clip;
      /** Every clip that moves, lead included. */
      group: Clip[];
      grabFrame: number;
      moved: boolean;
      /** Clicking a clip that is part of a multi-selection without dragging selects just that clip. */
      collapse: boolean;
      delta: number;
      ghosts: Ghosts;
    };

function Playhead({ ppf, scroller }: { ppf: number; scroller: React.RefObject<HTMLDivElement | null> }) {
  const frame = ui.use((s) => s.frame);
  const playing = ui.use((s) => s.playing);
  const x = frame * ppf;
  useEffect(() => {
    const el = scroller.current;
    if (!el || !playing) return;
    if (x < el.scrollLeft || x > el.scrollLeft + el.clientWidth - 40) el.scrollLeft = x - el.clientWidth / 4;
  }, [x, playing, scroller]);
  return <div id="playhead" style={{ left: x }} />;
}

function Toolbar({ selected, fps }: { selected: Clip[]; fps: number }) {
  const tool = ui.use((s) => s.tool);
  const snap = ui.use((s) => s.snap);
  const pps = ui.use((s) => s.pps);
  const showPace = ui.use((s) => s.showPace);
  const one = selected.length === 1 ? selected[0] : null;
  return (
    <div id="tlbar">
      <button className={tool === "select" ? "on" : ""} data-tip="Selection tool (V)" onClick={() => runCommand("select")}>
        ↖ Select
      </button>
      <button className={tool === "razor" ? "on" : ""} data-tip="Razor tool (C)" onClick={() => runCommand("razor")}>
        ✂ Razor
      </button>
      <span className="sep" />
      <button data-tip="Split at playhead (Ctrl+B)" onClick={() => runCommand("split")}>Split</button>
      <button data-tip="Delete (Del)" onClick={() => runCommand("delete")}>Delete</button>
      <button data-tip="Ripple delete (Shift+Del)" onClick={() => runCommand("ripple")}>Ripple delete</button>
      <button data-tip="Duplicate (Ctrl+D)" onClick={() => runCommand("duplicate")}>Duplicate</button>
      <button data-tip="Add marker (M)" onClick={() => runCommand("marker")}>◆ Marker</button>
      <span className="sep" />
      <button className={snap ? "on" : ""} data-tip="Snapping (N)" onClick={() => runCommand("snap")}>⌁ Snap</button>
      <button
        className={showPace ? "on" : ""}
        data-tip="Pace lane: cuts per 10 seconds on the lowest video track"
        onClick={() => ui.set({ showPace: !showPace })}
      >
        ▥ Pace
      </button>
      <span className="sp" />
      <span className="muted">
        {one && `${one.name} · ${(one.start / fps).toFixed(2)}s → ${(clipEnd(one) / fps).toFixed(2)}s`}
        {selected.length > 1 && `${selected.length} clips selected`}
      </span>
      <span className="sep" />
      <button data-tip="Zoom out (-)" onClick={() => runCommand("zoomout")}>−</button>
      {/* Logarithmic, so each bit of travel zooms by the same factor at any scale. */}
      <input
        className="zoom"
        type="range"
        data-tip="Zoom (Ctrl + mouse wheel)"
        min={Math.log(MIN_PPS)}
        max={Math.log(MAX_PPS)}
        step={0.01}
        value={Math.log(pps)}
        onChange={(e) => ui.set({ pps: Math.exp(+e.target.value) })}
      />
      <button data-tip="Zoom in (+)" onClick={() => runCommand("zoomin")}>+</button>
      <button data-tip="Fit timeline (Shift+Z)" onClick={() => runCommand("fit")}>Fit</button>
    </div>
  );
}

function PaceLane({ seq, fps, ppf, total }: { seq: Sequence; fps: number; ppf: number; total: number }) {
  const lowest = [...seq.tracks].reverse().find((t) => t.kind === "video")?.id;
  const cuts = seq.clips.filter((c) => c.track === lowest).map((c) => c.start);
  const win = PACE_WINDOW_S * fps;
  if (win * ppf < 6) {
    return (
      <div className="lane ana" style={{ height: 24 }}>
        <span className="lanehint">Zoom in to see pace</span>
      </div>
    );
  }
  const cells = [];
  for (let f = 0; f < total; f += win) {
    const n = cuts.filter((x) => x >= f && x < f + win).length;
    cells.push(
      <div
        key={f}
        className="cell"
        style={{ left: f * ppf + 1, width: win * ppf - 2, background: PACE_COLOURS[Math.min(n, 3)] }}
        data-tip={`${mmss(f / fps)}–${mmss((f + win) / fps)}: ${n} cut${n === 1 ? "" : "s"}`}
      >
        {n}
      </div>,
    );
  }
  return <div className="lane ana" style={{ height: 24 }}>{cells}</div>;
}

/**
 * The picture at the playhead, from the top-most video clip there, shown just above the playhead so you can see
 * exactly which frame it is. It uses the same cached frames as the clips.
 */
function PlayheadPreview({ p, ppf, fps }: { p: Project; ppf: number; fps: number }) {
  const frame = ui.use((s) => s.frame);
  const PREVIEW_PX = 60;
  const dpr = window.devicePixelRatio || 1;
  // Tracks are listed top first, so the first video track with a clip here is the one on top.
  const topVideo = p.sequence.tracks.filter((t) => t.kind === "video").map((t) => t.id);
  const clip = topVideo
    .map((track) => p.sequence.clips.find((c) => c.track === track && frame >= c.start && frame < clipEnd(c)))
    .find((c) => c?.media);
  const m = clip?.media ? p.media.find((x) => x.id === clip.media) : undefined;
  const size = filmstripSize(PREVIEW_PX * dpr);
  const k = m ? filmstripKey(m.id, size) : "";
  const strip = ui.use((s) => (k ? s.filmstrips[k] : undefined));
  useEffect(() => {
    if (m) requestFilmstrip(m, size);
  }, [m, size]);
  if (!clip || !m || typeof strip !== "object") return null;
  const width = Math.round(PREVIEW_PX * ((m.width || 16) / (m.height || 9)));
  const seconds = (clip.source_in + (frame - clip.start)) / fps;
  return (
    <div className="phprev" style={{ left: frame * ppf, width: width + 2 }}>
      <img src={frameUrl(strip, seconds)} style={{ width, height: PREVIEW_PX }} draggable={false} alt="" />
      <span>{timecode(frame, fps)}</span>
    </div>
  );
}

export function Timeline() {
  const state = core.use((s) => s.state)!;
  const pps = ui.use((s) => s.pps);
  const tool = ui.use((s) => s.tool);
  const snap = ui.use((s) => s.snap);
  const selection = ui.use((s) => s.selection);
  const showPace = ui.use((s) => s.showPace);
  const videoH = ui.use((s) => s.videoH);
  const audioH = ui.use((s) => s.audioH);
  const heights = ui.use((s) => s.trackHeights);
  const collapsed = ui.use((s) => s.collapsed);
  const colours = ui.use((s) => s.trackColors);
  const clipColours = ui.use((s) => s.clipColors);
  const targetId = ui.use((s) => s.targetTrack);
  const layout = { videoH, audioH, heights, collapsed };
  const headsW = ui.use((s) => s.headsW);

  const p = state.project;
  const seq = p.sequence;
  const fps = fpsOf(p);
  const ppf = pps / fps;
  const total = timelineFrames(p);

  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const pendingScroll = useRef<number | null>(null);
  const [ghosts, setGhosts] = useState<Ghosts>({});
  const [snapX, setSnapX] = useState<number | null>(null);
  const [marquee, setMarquee] = useState<Box | null>(null);
  /** Where a razor click would cut, shown as a line while the razor tool is equipped. */
  const [cutFrame, setCutFrame] = useState<number | null>(null);

  // Window-level pointer handlers read the latest values from here.
  const live = useRef({ seq, ppf, total, snap, fps });
  live.current = { seq, ppf, total, snap, fps };

  const frameAt = (clientX: number) => {
    const { ppf, total } = live.current;
    const left = content.current?.getBoundingClientRect().left ?? 0;
    return clamp(Math.round((clientX - left) / ppf), 0, total);
  };

  /** Nearest snap target within reach of `f`, ignoring the clips being dragged. */
  const snapTarget = (f: number, skip: string[] = []): number | null => {
    const { seq, ppf, snap } = live.current;
    if (!snap) return null;
    const points = [0, ui.get().frame, ...seq.markers.map((m) => m.at)];
    for (const c of seq.clips) if (!skip.includes(c.id)) points.push(c.start, clipEnd(c));
    let best: number | null = null;
    for (const pt of points) {
      if (Math.abs(pt - f) * ppf < SNAP_PX && (best === null || Math.abs(pt - f) < Math.abs(best - f))) best = pt;
    }
    return best;
  };

  const fit = () => {
    const el = scroller.current;
    if (!el) return;
    const { total, fps } = live.current;
    ui.set({ pps: clamp((el.clientWidth - 30) / (total / fps), MIN_PPS, MAX_PPS) });
    el.scrollLeft = 0;
  };
  timelineApi.fit = fit;
  timelineApi.dropTarget = (x, y) => {
    const box = scroller.current?.getBoundingClientRect();
    if (!box || x < box.left || x > box.right || y < box.top || y > box.bottom) return null;
    const lane = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>("[data-track]");
    const at = frameAt(x);
    return { track: lane?.dataset.track ?? null, frame: snapTarget(at) ?? at };
  };
  useEffect(fit, []);

  useLayoutEffect(() => {
    if (pendingScroll.current !== null && scroller.current) scroller.current.scrollLeft = pendingScroll.current;
    pendingScroll.current = null;
  }, [pps]);

  useEffect(() => {
    const el = scroller.current!;
    const onWheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.altKey) {
        e.preventDefault();
        const { ppf, fps } = live.current;
        const rect = el.getBoundingClientRect();
        const anchor = (e.clientX - rect.left + el.scrollLeft) / ppf;
        const next = clamp(ui.get().pps * (e.deltaY < 0 ? 1.15 : 0.87), MIN_PPS, MAX_PPS);
        pendingScroll.current = (anchor * next) / fps - (e.clientX - rect.left);
        ui.set({ pps: next });
      } else if (!e.shiftKey && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
        e.preventDefault();
        el.scrollLeft += e.deltaY;
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      if (d.mode === "seek") return seek(frameAt(e.clientX));

      if (d.mode === "marquee") {
        if (!d.moved && Math.hypot(e.clientX - d.x0, e.clientY - d.y0) < DRAG_PX) return;
        d.moved = true;
        const el = content.current!;
        const origin = el.getBoundingClientRect();
        const [l, r] = [Math.min(d.x0, e.clientX), Math.max(d.x0, e.clientX)];
        const [t, b] = [Math.min(d.y0, e.clientY), Math.max(d.y0, e.clientY)];
        setMarquee({ x: l - origin.left, y: t - origin.top, w: r - l, h: b - t });
        const hit = [...el.querySelectorAll<HTMLElement>(".clip")]
          .filter((c) => {
            const q = c.getBoundingClientRect();
            return q.left < r && q.right > l && q.top < b && q.bottom > t;
          })
          .map((c) => c.dataset.id!);
        ui.set({ selection: [...new Set([...d.additive, ...hit])] });
        return;
      }

      const { seq, ppf } = live.current;
      let delta = frameAt(e.clientX) - d.grabFrame;
      if (!d.moved && Math.abs(delta * ppf) < DRAG_PX) return;
      d.moved = true;
      const { lead } = d;
      const ids = d.group.map((c) => c.id);
      const next: Ghosts = {};
      let snapped: number | null = null;

      if (d.mode === "move") {
        const head = snapTarget(lead.start + delta, ids);
        const tail = head === null ? snapTarget(clipEnd(lead) + delta, ids) : null;
        if (head !== null) delta = (snapped = head) - lead.start;
        else if (tail !== null) delta = (snapped = tail) - clipEnd(lead);
        delta = Math.max(delta, -Math.min(...d.group.map((c) => c.start)));
        for (const c of d.group) next[c.id] = { start: c.start + delta, duration: c.duration, track: c.track };
        // A single clip can also change track; a group keeps its layout.
        if (d.group.length === 1) {
          const lane = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>(
            "[data-track]",
          );
          const target = seq.tracks.find((t) => t.id === lane?.dataset.track);
          if (target && !target.locked && fits(lead.kind, target.kind)) next[lead.id].track = target.id;
        }
      } else if (d.mode === "trimL") {
        snapped = snapTarget(lead.start + delta, ids);
        const start = clamp(snapped ?? lead.start + delta, 0, clipEnd(lead) - 1);
        next[lead.id] = { start, duration: clipEnd(lead) - start, track: lead.track };
      } else {
        snapped = snapTarget(clipEnd(lead) + delta, ids);
        next[lead.id] = { start: lead.start, duration: Math.max(1, (snapped ?? clipEnd(lead) + delta) - lead.start), track: lead.track };
      }
      d.delta = delta;
      d.ghosts = next;
      setGhosts(next);
      setSnapX(snapped === null ? null : snapped * ppf);
    };

    const onUp = (e: PointerEvent) => {
      const d = drag.current;
      drag.current = null;
      setSnapX(null);
      setMarquee(null);
      if (!d || d.mode === "seek") return;
      if (d.mode === "marquee") {
        // A plain click on empty timeline deselects and moves the playhead there.
        if (!d.moved) {
          ui.set({ selection: d.additive });
          seek(frameAt(e.clientX));
        }
        return;
      }
      if (!d.moved) {
        if (d.collapse) ui.set({ selection: [d.lead.id] });
        return;
      }
      const g = d.ghosts[d.lead.id];
      const done =
        d.mode !== "move"
          ? exec("timeline.trim_clip", { clip: d.lead.id, start: g.start, duration: g.duration })
          : d.group.length === 1
            ? exec("timeline.move_clip", { clip: d.lead.id, start: g.start, track: g.track })
            : exec("timeline.move_clips", { clips: d.group.map((c) => c.id), delta: d.delta });
      // Keep the ghosts until the core answers, so the clips do not jump back for a frame.
      done.finally(() => setGhosts({}));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, []);

  const onLanesDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    // Any press on a lane makes that track the paste target.
    const lane = target.closest<HTMLElement>("[data-track]");
    if (lane?.dataset.track) ui.set({ targetTrack: lane.dataset.track });
    const el = target.closest<HTMLElement>(".clip");
    const c = seq.clips.find((x) => x.id === el?.dataset.id);
    const extend = e.ctrlKey || e.metaKey || e.shiftKey;
    // Razor tool, or Alt held with any tool: a click cuts at the pointer, no playhead needed.
    // Shift cuts every unlocked track at that frame, as Premiere's Shift+razor does.
    if (tool === "razor" || e.altKey) {
      const at = snapTarget(frameAt(e.clientX), c ? [c.id] : []) ?? frameAt(e.clientX);
      if (e.shiftKey) return exec("timeline.split_clip", { at });
      if (!c) return;
      ui.set({ selection: [c.id] });
      splitAt(at, c.id);
      return;
    }
    if (!c) {
      drag.current = { mode: "marquee", x0: e.clientX, y0: e.clientY, additive: extend ? selection : [], moved: false };
      return;
    }
    const at = frameAt(e.clientX);
    if (extend) {
      // Ctrl or Shift click adds the clip to the selection, or takes it out.
      ui.set({ selection: selection.includes(c.id) ? selection.filter((id) => id !== c.id) : [...selection, c.id] });
      return;
    }
    const mode = target.classList.contains("hl") ? "trimL" : target.classList.contains("hr") ? "trimR" : "move";
    const keepGroup = mode === "move" && selection.includes(c.id) && selection.length > 1;
    const ids = linkedIds(seq.clips, keepGroup ? selection : [c.id]);
    if (!keepGroup) ui.set({ selection: ids, rightTab: "inspector" });
    const locked = (id: string) => seq.tracks.find((t) => t.id === id)?.locked;
    if (locked(c.track)) return toast(`Track ${c.track} is locked`);
    const group = seq.clips.filter((x) => ids.includes(x.id) && !locked(x.track));
    drag.current = { mode, lead: c, group, grabFrame: at, moved: false, collapse: keepGroup, delta: 0, ghosts: {} };
  };

  /** Drag the top edge of the timeline to make it taller or shorter. */
  const startTimelineResize = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const y0 = e.clientY;
    const h0 = ui.get().timelineH;
    const move = (ev: PointerEvent) => ui.set({ timelineH: clamp(Math.round(h0 - (ev.clientY - y0)), 160, window.innerHeight - 260) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Drag the right edge of the track headers column to change its width. */
  const startHeadsResize = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const x0 = e.clientX;
    const w0 = ui.get().headsW;
    const move = (ev: PointerEvent) => ui.set({ headsW: clamp(Math.round(w0 + ev.clientX - x0), 120, 420) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const onRulerDown = (e: React.PointerEvent) => {
    drag.current = { mode: "seek" };
    seek(frameAt(e.clientX));
  };

  const clips = seq.clips.map((c) => (ghosts[c.id] ? { ...c, ...ghosts[c.id] } : c));
  const stepS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 14400].find((s) => s * pps >= 80) ?? 28800;
  const ticks = [];
  for (let s = 0; s * fps <= total; s += stepS) ticks.push(s);
  // The divider sits above the first audio track, when there are video tracks above it.
  const firstAudio = seq.tracks.findIndex((t) => t.kind === "audio");
  const splitIndex = firstAudio > 0 ? firstAudio : -1;
  const range = seq.in_point !== null && seq.out_point !== null && seq.out_point > seq.in_point;

  return (
    <div id="tl" className={tool === "razor" ? "razor" : ""}>
      <i
        className="pgrip h"
        data-tip="Drag up or down to make the timeline taller or shorter. Double-click to reset."
        onPointerDown={startTimelineResize}
        onDoubleClick={() => ui.set({ timelineH: Math.round(window.innerHeight * 0.46) })}
      />
      <Toolbar selected={seq.clips.filter((c) => selection.includes(c.id))} fps={fps} />
      <div id="tlbody" style={{ "--heads": `${headsW}px` } as React.CSSProperties}>
        <div id="heads">
          <i className="colgrip" data-tip="Drag to make the track name column wider or narrower" onPointerDown={startHeadsResize} />
          <div className="hd corner" style={{ height: 28 }}>
            <button data-tip="Add a video track" onClick={() => exec("timeline.add_track", { kind: "video" })}>+ Video</button>
            <button data-tip="Add an audio track" onClick={() => exec("timeline.add_track", { kind: "audio" })}>+ Audio</button>
          </div>
          {showPace && (
            <div className="hd ana" style={{ height: 24 }} data-tip="Cuts on the lowest video track per 10 seconds">
              Pace · cuts / 10 s
            </div>
          )}
          {seq.tracks.map((t, i) => (
            <Fragment key={t.id}>
              {i === splitIndex && <div className="splitpad" style={{ height: SPLITTER_PX }}><Splitter tracks={seq.tracks} /></div>}
              <TrackHead t={t} seq={seq} height={trackHeight(t, layout)} />
            </Fragment>
          ))}
        </div>

        <div id="scroll" ref={scroller}>
          <div id="content" ref={content} style={{ width: total * ppf + 40 }}>
            <div id="ruler" data-tip="Click or drag to move the playhead. Markers: double-click one to remove it." onPointerDown={onRulerDown}>
              {range && (
                <div className="range" style={{ left: seq.in_point! * ppf, width: (seq.out_point! - seq.in_point!) * ppf }} />
              )}
              {!range && seq.in_point !== null && <div className="edge" style={{ left: seq.in_point * ppf }} />}
              {!range && seq.out_point !== null && <div className="edge" style={{ left: seq.out_point * ppf }} />}
              {ticks.map((s) => (
                <div key={s} className="tick" style={{ left: s * pps }}>{mmss(s)}</div>
              ))}
              {seq.markers.map((m) => (
                <div
                  key={m.id}
                  className={`marker ${m.actor && m.actor !== "user" ? "agent" : ""}`}
                  style={{ left: m.at * ppf }}
                  data-tip={`${m.label} · double-click to remove`}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={() => seek(m.at)}
                  onDoubleClick={() => exec("timeline.remove_marker", { marker: m.id })}
                />
              ))}
            </div>

            <div
              id="lanes"
              data-tip="Drag across empty space to select several clips. Click to select one. Drag a clip to move it."
              onPointerDown={onLanesDown}
              onPointerMove={(e) => tool === "razor" && setCutFrame(snapTarget(frameAt(e.clientX)) ?? frameAt(e.clientX))}
              onPointerLeave={() => setCutFrame(null)}
            >
              {showPace && <PaceLane seq={seq} fps={fps} ppf={ppf} total={total} />}
              {seq.tracks.map((t, i) => (
                <Fragment key={t.id}>
                {i === splitIndex && <div className="splitpad" style={{ height: SPLITTER_PX }}><Splitter tracks={seq.tracks} /></div>}
                <div
                  data-track={t.id}
                  className={`lane ${t.locked ? "locked" : ""} ${t.muted || t.hidden ? "dim" : ""} ${layout.collapsed[t.id] ? "mini" : ""} ${targetId === t.id ? "target" : ""}`}
                  style={{ height: trackHeight(t, layout), boxShadow: colours[t.id] ? `inset 3px 0 0 ${colours[t.id]}` : undefined }}
                  onContextMenu={(e) => openTrackMenu(e, t, seq)}
                >
                  {clips
                    .filter((c) => c.track === t.id)
                    .map((c) => {
                      const byAgent = c.last_actor && c.last_actor !== "user";
                      return (
                        <div
                          key={c.id}
                          data-id={c.id}
                          className={`clip ${clipClass(c, seq.tracks)} ${selection.includes(c.id) ? "sel" : ""}`}
                          style={{
                            left: c.start * ppf,
                            width: Math.max(3, c.duration * ppf - 1),
                            background: colours[c.track],
                            boxShadow: clipColours[c.id] ? `inset 5px 0 0 ${clipColours[c.id]}` : undefined,
                          }}
                          data-tip={byAgent ? `${c.name} · last changed by ${c.last_actor}` : c.name}
                          onContextMenu={(e) => openClipMenu(e, c)}
                        >
                          {c.kind === "video" && c.media && (() => {
                            const m = p.media.find((x) => x.id === c.media);
                            return m ? <Filmstrip c={c} m={m} ppf={ppf} fps={fps} lane={trackHeight(t, layout) - 4} /> : null;
                          })()}
                          {c.transition_in && <span className="tr" data-tip={c.transition_in.name} />}
                          <span className="h hl" data-tip="Drag to trim the start of the clip" />
                          <span className="nm" style={c.transition_in ? { paddingLeft: 16 } : undefined}>
                            {(c.kind === "graphic" || c.kind === "caption") && c.text ? c.text : c.name}
                          </span>
                          <span className="bd">
                            {c.effects.length > 0 && <i>fx {c.effects.length}</i>}
                            {byAgent && <i className="ag">✦</i>}
                          </span>
                          {c.transition_out && <span className="tr out" data-tip={c.transition_out.name} />}
                          <span className="h hr" data-tip="Drag to trim the end of the clip" />
                        </div>
                      );
                    })}
                </div>
                </Fragment>
              ))}
            </div>

            {marquee && <div id="marquee" style={{ left: marquee.x, top: marquee.y, width: marquee.w, height: marquee.h }} />}
            {snapX !== null && <div id="snapline" style={{ left: snapX }} />}
            {tool === "razor" && cutFrame !== null && (
              <div className="cutline" style={{ left: cutFrame * ppf }} data-label={timecode(cutFrame, fps)} />
            )}
            <Playhead ppf={ppf} scroller={scroller} />
            <PlayheadPreview p={p} ppf={ppf} fps={fps} />
          </div>
        </div>
      </div>
    </div>
  );
}
