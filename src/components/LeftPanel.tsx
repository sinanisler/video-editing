import { importMedia, timelineApi } from "../commands";
import { addMediaClip, assetUrl } from "../media";
import { core, exec, execBatch, project, toast, ui } from "../store";
import type { MediaItem } from "../types";
import { clipEnd, fpsOf, timecode } from "../util";
import { Tabs } from "./Tabs";

/** What the import dialog accepts. Shown in the Import tooltip and under the media list. */
const FORMATS = [
  ["Video", "MP4, MOV, M4V, MKV, WebM"],
  ["Audio", "MP3, WAV, M4A, AAC, OGG, FLAC"],
  ["Images", "PNG, JPG, WebP, GIF"],
] as const;
const IMPORT_TIP = `Import video, audio and image files.\n${FORMATS.map(([kind, list]) => `${kind}: ${list}`).join("\n")}`;

export const TEXT_TEMPLATES = [
  { id: "title", name: "Title card", text: "TITLE", hint: "Big title in the centre" },
  { id: "subscribe", name: "Subscribe card", text: "Subscribe", hint: "Avatar and button card" },
  { id: "counter", name: "Counter", text: "1", hint: "Number or label badge in the corner" },
  { id: "chapter", name: "Chapter card", text: "Chapter 1", hint: "Section divider" },
  { id: "lower", name: "Lower third", text: "Name", hint: "Name or handle label" },
];

function Tile(props: { name: string; hint: string; tip: string; colour: string; onClick: () => void }) {
  return (
    <div className="tile" data-tip={props.tip} onClick={props.onClick}>
      <span className="ic" style={{ background: props.colour }}>{props.name[0]}</span>
      <span>
        {props.name}
        <small>{props.hint}</small>
      </span>
    </div>
  );
}

/** Adds a 3 second graphic at the playhead on the highest video track that is free there. */
function addGraphic(t: (typeof TEXT_TEMPLATES)[number]) {
  const p = project();
  if (!p) return;
  const start = ui.get().frame;
  const duration = Math.round(3 * fpsOf(p));
  const free = p.sequence.tracks.find(
    (tr) =>
      tr.kind === "video" &&
      !tr.locked &&
      !p.sequence.clips.some((c) => c.track === tr.id && start < clipEnd(c) && c.start < start + duration),
  );
  if (!free) return toast("No free video track at the playhead");
  exec("timeline.add_clip", { track: free.id, start, duration, name: t.name, kind: "graphic", template: t.id, text: t.text }).then(
    (r) => r && ui.set({ selection: [r.clip as string], rightTab: "inspector" }),
  );
}

/** Applies one command to every selected clip, as a single undo step. */
function onSelection(tool: string, args: Record<string, unknown>) {
  const clips = ui.get().selection;
  if (clips.length) execBatch(clips.map((clip) => ({ tool, args: { clip, ...args } })));
  else toast("Select a clip first");
}

function Thumb({ m }: { m: MediaItem }) {
  if (m.kind === "audio") return <div className="th note">♪</div>;
  if (m.kind === "image") return <img className="th" src={assetUrl(m.path)} draggable={false} />;
  // The fragment makes the element show a frame one second in instead of a black first frame.
  return <video className="th" src={`${assetUrl(m.path)}#t=1`} preload="metadata" muted />;
}

/** Pointer-based drag to the timeline. HTML drag and drop is unavailable while the window accepts file drops. */
function startMediaDrag(e: React.PointerEvent, m: MediaItem) {
  if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
  const origin = { x: e.clientX, y: e.clientY };
  let dragging = false;
  const move = (ev: PointerEvent) => {
    if (!dragging && Math.hypot(ev.clientX - origin.x, ev.clientY - origin.y) < 5) return;
    dragging = true;
    ui.set({ mediaDrag: { id: m.id, name: m.name, x: ev.clientX, y: ev.clientY } });
  };
  const up = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (!dragging) return;
    ui.set({ mediaDrag: null });
    const target = timelineApi.dropTarget(ev.clientX, ev.clientY);
    if (target) addMediaClip(m, target.frame, target.track);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function MediaPool() {
  const p = core.use((s) => s.state)!.project;
  const fps = fpsOf(p);
  const importButton = <button className="primary wide" data-tip={IMPORT_TIP} onClick={importMedia}>Import files…</button>;
  return (
    <>
      {p.media.length === 0 ? (
        <div className="empty">
          <b>No media yet</b>
          <p>Choose files with the button below, or drag them from a folder onto this panel or straight onto the timeline.</p>
          {importButton}
        </div>
      ) : (
        <>
          {importButton}
          <p className="hint">Drag onto the timeline, or double-click to add at the playhead.</p>
          <div className="media">
            {p.media.map((m) => (
              <div
                key={m.id}
                className="mi"
                data-tip={m.path}
                onPointerDown={(e) => startMediaDrag(e, m)}
                onDoubleClick={() => addMediaClip(m, ui.get().frame)}
              >
                <Thumb m={m} />
                <button className="rm" data-tip="Remove from project. Clips already on the timeline stay." onClick={() => exec("media.remove", { media: m.id })}>✕</button>
                <div className="nm">{m.name}</div>
                <div className="meta">
                  {m.kind === "image" ? "image" : timecode(m.duration, fps)}
                  {m.width > 0 && ` · ${m.width}×${m.height}`}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </>
  );
}

export function LeftPanel() {
  const tab = ui.use((s) => s.leftTab);
  const lib = core.use((s) => s.library)!;
  const edge = ui.use((s) => s.transitionEdge);
  return (
    <div className="panel" id="left">
      <Tabs
        value={tab}
        onChange={(leftTab) => ui.set({ leftTab })}
        tabs={[
          ["media", "Media", "Import files and place them on the timeline"],
          ["text", "Text", "Title cards, counters and lower thirds"],
          ["transitions", "Transitions", "Blends between clips, at their start or end"],
          ["effects", "Effects", "Looks and filters for the selected clips"],
        ]}
      />
      <div className="pane">
        {tab === "media" && <MediaPool />}
        {tab === "text" && (
          <>
            <p className="hint">Click to add at the playhead.</p>
            {TEXT_TEMPLATES.map((t) => (
              <Tile key={t.id} name={t.name} hint={t.hint} tip={`Add a 3 second ${t.name.toLowerCase()} at the playhead, on the top free video track.`} colour="var(--k-gfx)" onClick={() => addGraphic(t)} />
            ))}
          </>
        )}
        {tab === "transitions" && (
          <>
            <p className="hint">Select one or more clips, then click a transition.</p>
            <div className="seg">
              <button className={edge === "in" ? "on" : ""} data-tip="Put the transition where each selected clip starts" onClick={() => ui.set({ transitionEdge: "in" })}>At clip start</button>
              <button className={edge === "out" ? "on" : ""} data-tip="Put the transition where each selected clip ends" onClick={() => ui.set({ transitionEdge: "out" })}>At clip end</button>
            </div>
            {[{ name: "Cut", hint: "Remove the transition" }, ...lib.transitions].map((t) => (
              <Tile
                key={t.name}
                name={t.name}
                hint={t.hint}
                tip={`${t.name === "Cut" ? "Remove the transition from" : `Apply ${t.name} to`} the selected clips at ${edge === "in" ? "their start" : "their end"}.`}
                colour="var(--k-pip)"
                onClick={() => onSelection("clip.set_transition", { edge, transition: t.name === "Cut" ? null : t.name })}
              />
            ))}
          </>
        )}
        {tab === "effects" && (
          <>
            <p className="hint">Select one or more clips, then click an effect. Set its strength in the Inspector.</p>
            {lib.effects.map((t) => (
              <Tile
                key={t.name}
                name={t.name}
                hint={t.hint}
                tip={`Apply ${t.name} to the selected clips. Set its strength in the Inspector.`}
                colour="var(--k-video)"
                onClick={() => onSelection("clip.add_effect", { effect: t.name })}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}
