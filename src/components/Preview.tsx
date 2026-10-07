import { useEffect, useRef } from "react";
import { runCommand } from "../commands";
import { clipGain, clipLook, outgoingLook, presence, reader, textLook, transitionLook } from "../look";
import { assetUrl } from "../media";
import { core, ui } from "../store";
import type { Clip, MediaItem } from "../types";
import { clipEnd, fpsOf, hue, timecode } from "../util";

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

function Graphic({ c }: { c: Clip }) {
  const text = c.text ?? c.name;
  switch (c.template) {
    case "title":
      return <div className="g-title">{text}</div>;
    case "subscribe":
      return (
        <div className="g-sub">
          <i />
          channel<b>{text}</b>
        </div>
      );
    case "counter":
      return <div className="g-count">{text}</div>;
    case "chapter":
      return <div className="g-chap">{text}</div>;
    case "lower":
      return <div className="g-lower">{text}</div>;
    default:
      return <div className="g-plain">{text}</div>;
  }
}

interface Shown {
  c: Clip;
  /** Timeline frame to draw the clip at. */
  frame: number;
  /** A previous clip held on its last frame under an incoming transition: its name and progress. */
  leaving?: { name: string; t: number };
}

function Stage() {
  const p = core.use((s) => s.state)!.project;
  const lib = core.use((s) => s.library)!;
  const frame = ui.use((s) => s.frame);
  const live = ui.use((s) => s.live);
  const safeArea = ui.use((s) => s.safeArea);
  const fps = fpsOf(p);
  const { tracks, clips } = p.sequence;
  const at = (track: string) => clips.find((c) => c.track === track && frame >= c.start && frame < clipEnd(c));
  const media = (c: Clip) => p.media.find((x) => x.id === c.media);

  // Bottom track first, so higher tracks draw on top. Hidden tracks are left out.
  const shown: Shown[] = [];
  for (const t of [...tracks].reverse()) {
    if (t.kind === "audio" || t.hidden) continue;
    const c = at(t.id);
    if (!c) continue;
    const pr = presence(c, frame);
    const previous = pr.edge === "in" ? clips.find((x) => x.track === t.id && clipEnd(x) === c.start) : undefined;
    if (previous && pr.name) shown.push({ c: previous, frame: clipEnd(previous) - 1, leaving: { name: pr.name, t: pr.t } });
    shown.push({ c, frame });
  }
  const sounding = tracks.filter((t) => t.kind === "audio" && !t.muted).map((t) => at(t.id)).filter((c): c is Clip => !!c);

  return (
    <div id="stage">
      {shown.map(({ c, frame: f, leaving }) => {
        const r = reader(c, lib, live);
        const look = clipLook(c, f, p, r);
        const pr = presence(c, f);
        const hasPrevious = clips.some((x) => x.track === c.track && clipEnd(x) === c.start);
        const tr = leaving ? { style: outgoingLook(leaving.name, leaving.t) } : transitionLook(pr, hasPrevious, f);
        const m = media(c);
        const isText = c.kind === "graphic" || c.kind === "caption";
        return (
          <div key={c.id} className="layer" style={look.outer}>
            <div className="layer" style={tr.style}>
              <div className="layer" style={look.frame}>
                <div className="cropbox" style={look.crop}>
                  <div className="content" style={{ ...look.content, ...(isText ? textLook(r) : null) }}>
                    {c.kind === "graphic" ? (
                      <Graphic c={c} />
                    ) : c.kind === "caption" ? (
                      <div className="g-caption">{c.text}</div>
                    ) : m?.kind === "image" ? (
                      <img src={assetUrl(m.path)} />
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
      })}
      {sounding.map((c) => {
        const m = media(c);
        return m && <Media key={c.id} c={c} m={m} fps={fps} frame={frame} frozen={false} gain={clipGain(c, frame, fps, reader(c, lib, live))} />;
      })}
      {safeArea && <div id="safe" />}
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
      <span id="tc">{timecode(frame, fps)}</span>
      <span className="mono dim">/ {timecode(end, fps)}</span>
      <span className="sp" />
      <button title="Go to start (Home)" onClick={() => runCommand("home")}>⏮</button>
      <button title="Back one frame (←)" onClick={() => runCommand("back")}>◀</button>
      <button className="primary" title="Play / pause (Space)" onClick={() => runCommand("play")}>
        {playing ? "❚❚" : "▶"}
      </button>
      <button title="Forward one frame (→)" onClick={() => runCommand("fwd")}>▶</button>
      <button title="Go to end (End)" onClick={() => runCommand("end")}>⏭</button>
      <span className="mono dim rate">{rate}×</span>
      <span className="sp" />
      <button title="Mark in (I)" onClick={() => runCommand("in")}>I</button>
      <button title="Mark out (O)" onClick={() => runCommand("out")}>O</button>
      <button className={safeArea ? "on" : ""} title="Safe area guides" onClick={() => ui.set({ safeArea: !safeArea })}>
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
