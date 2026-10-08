import type React from "react";
import { exec, ui } from "../store";
import type { MenuItem } from "../store";
import type { Sequence, Track } from "../types";
import { clamp, COLLAPSED_PX, TRACK_MAX_PX, TRACK_MIN_PX } from "../util";
import { openMenu } from "./ContextMenu";
import { exportSubtitles, importSubtitles, pickSubtitleFile } from "../captions";

/** Colours a track can be given. A track's colour shows as a stripe on its header and on its lane. */
export const TRACK_COLOURS = ["#3566c9", "#2b9167", "#1d8598", "#7556cf", "#b84a88", "#bf6d27", "#a8892a", "#878d9c"];

/** The two groups are separated by a divider; this is the track that starts the audio group. */
export const SPLITTER_PX = 8;

function TrackName({ t }: { t: Track }) {
  const renaming = ui.use((s) => s.renaming === t.id);
  if (!renaming) {
    return (
      <span className="n" data-tip="Double-click to rename. Right-click for more options." onDoubleClick={() => ui.set({ renaming: t.id })}>
        {t.name}
      </span>
    );
  }
  return (
    <input
      className="n"
      data-tip="New track name. Press Enter to save."
      autoFocus
      defaultValue={t.name}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      onBlur={(e) => {
        ui.set({ renaming: null });
        const name = e.target.value.trim();
        if (name && name !== t.name) exec("timeline.rename_track", { track: t.id, name });
      }}
    />
  );
}

/** Drag the bottom edge of a header to change that track's height. */
function startResize(e: React.PointerEvent, t: Track, height: number) {
  if (e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  const y0 = e.clientY;
  const move = (ev: PointerEvent) => {
    const next = clamp(Math.round(height + ev.clientY - y0), TRACK_MIN_PX, TRACK_MAX_PX);
    ui.set({ trackHeights: { ...ui.get().trackHeights, [t.id]: next } });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/**
 * Drag the divider between the video and audio groups. The line follows the pointer: the video tracks above it
 * grow or shrink by the same amount, and the audio tracks below keep their size and move with it. Pulling the
 * video group below its minimum collapses it, and pulling it back out expands it again.
 */
function startSplit(e: React.PointerEvent, tracks: Track[]) {
  if (e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  const videos = tracks.filter((t) => t.kind === "video");
  const n = Math.max(1, videos.length);
  const start = ui.get();
  // A group that starts collapsed resumes from its collapsed height, so the first pixels of drag expand it smoothly.
  const base = videos.length && videos.every((t) => start.collapsed[t.id]) ? COLLAPSED_PX : clamp(start.videoH, TRACK_MIN_PX, TRACK_MAX_PX);
  const y0 = e.clientY;
  const move = (ev: PointerEvent) => {
    // Each video track takes its share of the pointer movement, so the line moves exactly as far as the pointer.
    const raw = base + (ev.clientY - y0) / n;
    const collapsed = raw < TRACK_MIN_PX;
    const s = ui.get();
    const next = { ...s.collapsed };
    const heights = { ...s.trackHeights };
    for (const t of videos) {
      next[t.id] = collapsed;
      delete heights[t.id];
    }
    ui.set({
      // A collapsed group keeps its last expanded height, so pulling it back out restores that size.
      videoH: collapsed ? s.videoH : Math.round(clamp(raw, TRACK_MIN_PX, TRACK_MAX_PX)),
      collapsed: next,
      trackHeights: heights,
    });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/** The divider row. It appears in both the headers column and the lanes, so the two stay aligned. */
export function Splitter({ tracks }: { tracks: Track[] }) {
  return (
    <div
      className="split"
      data-tip="Drag up or down to give video or audio tracks more height"
      onPointerDown={(e) => startSplit(e, tracks)}
    />
  );
}

/** Right-click options for one track. Also used by right-clicking its lane. */
export function trackMenu(t: Track, seq: Sequence): MenuItem[] {
  const s = ui.get();
  const clipIds = seq.clips.filter((c) => c.track === t.id).map((c) => c.id);
  const collapsed = !!s.collapsed[t.id];
  const flag = t.kind === "audio" ? "muted" : "hidden";
  const on = t[flag];
  return [
    { label: "Rename", action: () => ui.set({ renaming: t.id }) },
    {
      label: "Colour",
      swatches: [null, ...TRACK_COLOURS],
      current: s.trackColors[t.id],
      onPick: (colour) => {
        const next = { ...ui.get().trackColors };
        if (colour) next[t.id] = colour;
        else delete next[t.id];
        ui.set({ trackColors: next });
      },
    },
    "sep",
    {
      label: "Paste here",
      hint: "Ctrl+V",
      checked: s.targetTrack === t.id,
      action: () => ui.set({ targetTrack: t.id }),
    },
    { label: "Select all clips on track", disabled: clipIds.length === 0, action: () => ui.set({ selection: clipIds }) },
    "sep",
    { label: t.locked ? "Unlock track" : "Lock track", checked: t.locked, action: () => exec("timeline.set_track_state", { track: t.id, locked: !t.locked }) },
    {
      label: t.kind === "audio" ? "Mute track" : "Hide track",
      checked: on,
      action: () => exec("timeline.set_track_state", { track: t.id, [flag]: !on }),
    },
    {
      label: collapsed ? "Expand track" : "Collapse track",
      action: () => ui.set({ collapsed: { ...ui.get().collapsed, [t.id]: !collapsed } }),
    },
    {
      label: "Reset height",
      disabled: !s.trackHeights[t.id],
      action: () => {
        const next = { ...ui.get().trackHeights };
        delete next[t.id];
        ui.set({ trackHeights: next });
      },
    },
    "sep",
    ...(t.kind === "caption"
      ? ([
          { label: "Import subtitles…", action: () => pickSubtitleFile().then((file) => file && importSubtitles(file)) },
          { label: "Export as SRT", action: () => exportSubtitles("srt") },
          { label: "Export as WebVTT", action: () => exportSubtitles("vtt") },
          "sep",
        ] as MenuItem[])
      : []),
    { label: "Add video track", action: () => exec("timeline.add_track", { kind: "video" }) },
    { label: "Add audio track", action: () => exec("timeline.add_track", { kind: "audio" }) },
    { label: "Remove track", disabled: clipIds.length > 0, danger: true, action: () => exec("timeline.remove_track", { track: t.id }) },
  ];
}

export function openTrackMenu(e: React.MouseEvent, t: Track, seq: Sequence) {
  e.preventDefault();
  e.stopPropagation();
  ui.set({ targetTrack: t.id });
  openMenu(e.clientX, e.clientY, trackMenu(t, seq));
}

/** One row of the headers column: fold button, id, name, colour, lock and mute or hide, and a resize grip. */
export function TrackHead({ t, seq, height }: { t: Track; seq: Sequence; height: number }) {
  const collapsed = ui.use((s) => !!s.collapsed[t.id]);
  const target = ui.use((s) => s.targetTrack === t.id);
  const colour = ui.use((s) => s.trackColors[t.id]);
  const flag = t.kind === "audio" ? "muted" : "hidden";
  const hasClips = seq.clips.some((c) => c.track === t.id);
  const mini = collapsed;
  return (
    <div
      className={`hd ${target ? "target" : ""} ${mini ? "mini" : ""}`}
      style={{ height, boxShadow: colour ? `inset 3px 0 0 ${colour}` : undefined }}
      data-tip={`${t.name}: click to ${collapsed ? "expand" : "collapse"}. Double-click the name to rename. Right-click for options.`}
      onClick={(e) => {
        // Buttons, the name and the height grip keep their own clicks.
        if ((e.target as HTMLElement).closest("button, input, .grip, .n")) return;
        ui.set({ collapsed: { ...ui.get().collapsed, [t.id]: !collapsed } });
      }}
      onContextMenu={(e) => openTrackMenu(e, t, seq)}
    >
      <span className="fold" aria-hidden="true">{collapsed ? "▸" : "▾"}</span>
      <b>{t.id}</b>
      <TrackName t={t} />
      {!hasClips && (
        <button className="rm" data-tip="Remove this empty track" onClick={() => exec("timeline.remove_track", { track: t.id })}>
          ✕
        </button>
      )}
      <button className={t.locked ? "on" : ""} data-tip="Lock track" onClick={() => exec("timeline.set_track_state", { track: t.id, locked: !t.locked })}>
        L
      </button>
      <button
        className={t[flag] ? "on" : ""}
        data-tip={flag === "muted" ? "Mute track" : "Hide track"}
        onClick={() => exec("timeline.set_track_state", { track: t.id, [flag]: !t[flag] })}
      >
        {flag === "muted" ? "M" : "H"}
      </button>
      {!mini && (
        <i
          className="grip"
          data-tip="Drag to change this track's height. Double-click to reset it."
          onPointerDown={(e) => startResize(e, t, height)}
          onDoubleClick={() => {
            const next = { ...ui.get().trackHeights };
            delete next[t.id];
            ui.set({ trackHeights: next });
          }}
        />
      )}
    </div>
  );
}

