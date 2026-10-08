import { importMedia, timelineApi } from "../commands";
import { addMediaClip, addMediaClips, assetUrl } from "../media";
import { core, exec, execBatch, project, toast, ui } from "../store";
import type { MediaItem, MediaSort, Project } from "../types";
import type { MenuItem } from "../store";
import { clipEnd, fpsOf, pickMedia, sortMedia, timecode } from "../util";
import { openMenu } from "./ContextMenu";
import { Tabs } from "./Tabs";
import { importSubtitles, pickSubtitleFile } from "../captions";

async function chooseSubtitles() {
  const file = await pickSubtitleFile();
  if (file) await importSubtitles(file);
}

/** What the import dialog accepts. Shown in the Import tooltip and under the media list. */
const FORMATS = [
  ["Video", "MP4, MOV, M4V, MKV, WebM"],
  ["Audio", "MP3, WAV, M4A, AAC, OGG, FLAC"],
  ["Images", "PNG, JPG, WebP, GIF"],
] as const;
const SUBTITLE_TIP = "Import an SRT or WebVTT subtitle file as captions on the captions track, at the times written in the file.";
const IMPORT_TIP = `Import video, audio and image files.\n${FORMATS.map(([kind, list]) => `${kind}: ${list}`).join("\n")}`;
/** What a media item does, shown when it is hovered. */
const ITEM_TIP = "Drag onto the timeline, or double-click to add it at the playhead.\nRight-click for more options.";

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

/**
 * The media an action on `m` applies to: the whole selection when `m` is part of it, otherwise just `m`.
 * Listed in the order the pool shows them.
 */
function targetsOf(m: MediaItem): MediaItem[] {
  const p = project();
  const s = ui.get();
  if (!p || !s.mediaSel.includes(m.id)) return [m];
  return sortMedia(p.media, s.mediaSort, s.mediaDesc).filter((x) => s.mediaSel.includes(x.id));
}

/**
 * Pointer-based drag to the timeline. HTML drag and drop is unavailable while the window accepts file drops.
 * A click without a drag selects the item; Ctrl or Shift adds it to the selection instead.
 */
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
    if (!dragging) {
      ui.set({ mediaSel: pickMedia(ui.get().mediaSel, m.id, ev.ctrlKey || ev.metaKey || ev.shiftKey) });
      return;
    }
    ui.set({ mediaDrag: null });
    const target = timelineApi.dropTarget(ev.clientX, ev.clientY);
    // Dragging one of several selected items brings the whole selection along, in pool order.
    if (target) addMediaClips(targetsOf(m), target.frame, target.track);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/**
 * Drag across empty space in the media panel to select the items the box touches. Ctrl or Shift keeps the
 * current selection and adds to it; a plain click on empty space clears the selection.
 */
function startMediaMarquee(e: React.PointerEvent) {
  if (e.button !== 0 || (e.target as HTMLElement).closest(".mi, button, input, select, .mediabar")) return;
  const x0 = e.clientX;
  const y0 = e.clientY;
  const extend = e.ctrlKey || e.metaKey || e.shiftKey;
  const base = extend ? ui.get().mediaSel : [];
  let moved = false;
  const move = (ev: PointerEvent) => {
    if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 4) return;
    moved = true;
    const [l, r] = [Math.min(x0, ev.clientX), Math.max(x0, ev.clientX)];
    const [t, b] = [Math.min(y0, ev.clientY), Math.max(y0, ev.clientY)];
    ui.set({ mediaMarquee: { x: l, y: t, w: r - l, h: b - t } });
    const hit = [...document.querySelectorAll<HTMLElement>("#left .mi")]
      .filter((el) => {
        const q = el.getBoundingClientRect();
        return q.left < r && q.right > l && q.top < b && q.bottom > t;
      })
      .map((el) => el.dataset.mid!);
    ui.set({ mediaSel: [...new Set([...base, ...hit])] });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    ui.set({ mediaMarquee: null });
    if (!moved && !extend) ui.set({ mediaSel: [] });
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

async function copyPath(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    toast("Path copied");
  } catch (e) {
    toast(`Could not copy the path: ${e}`);
  }
}

/** Right-click on the media panel, away from any item: importing, and actions on the whole pool. */
function poolMenu(p: Project): MenuItem[] {
  const used = new Set(p.sequence.clips.map((c) => c.media));
  const unused = p.media.filter((m) => !used.has(m.id));
  const grid = ui.get().mediaView === "grid";
  return [
    { label: "Import files…", hint: "Ctrl+I", action: importMedia },
    { label: "Import subtitles…", action: chooseSubtitles },
    "sep",
    { label: grid ? "Show as list" : "Show as grid", action: () => ui.set({ mediaView: grid ? "list" : "grid" }) },
    { label: "Select all media", disabled: p.media.length === 0, action: () => ui.set({ mediaSel: p.media.map((m) => m.id) }) },
    "sep",
    {
      label: unused.length ? `Remove ${unused.length} unused media ${unused.length === 1 ? "item" : "items"}` : "Remove unused media",
      disabled: unused.length === 0,
      // Red only while it can be used: a disabled row would otherwise look like a live danger.
      danger: unused.length > 0,
      action: () => execBatch(unused.map((m) => ({ tool: "media.remove", args: { media: m.id } }))),
    },
  ];
}

/**
 * Right-click on one media item, or on a selection. Removing is refused while clips on the timeline still use an
 * item, so that row is shown as off and says why.
 */
function mediaMenu(items: MediaItem[], p: Project): MenuItem[] {
  const n = items.length;
  const ids = new Set(items.map((m) => m.id));
  const clips = p.sequence.clips.filter((c) => c.media && ids.has(c.media));
  const used = items.filter((m) => p.sequence.clips.some((c) => c.media === m.id));
  const removeLabel = used.length
    ? n === 1
      ? `Remove from project (used by ${clips.length} ${clips.length === 1 ? "clip" : "clips"})`
      : `Remove from project (${used.length} of them ${used.length === 1 ? "is" : "are"} used by clips)`
    : n === 1
      ? "Remove from project"
      : `Remove ${n} items from project`;
  return [
    {
      label: n > 1 ? `Add ${n} items to timeline at playhead` : "Add to timeline at playhead",
      hint: n === 1 ? "Double-click" : undefined,
      action: () => addMediaClips(items, ui.get().frame),
    },
    { label: "Select its clips on the timeline", disabled: clips.length === 0, action: () => ui.set({ selection: clips.map((c) => c.id) }) },
    { label: n > 1 ? "Copy file paths" : "Copy file path", action: () => copyPath(items.map((m) => m.path).join("\n")) },
    "sep",
    {
      label: removeLabel,
      disabled: used.length > 0,
      danger: used.length === 0,
      action: () => execBatch(items.map((m) => ({ tool: "media.remove", args: { media: m.id } }))),
    },
  ];
}

function openPoolMenu(e: React.MouseEvent) {
  const p = project();
  if (!p) return;
  e.preventDefault();
  openMenu(e.clientX, e.clientY, poolMenu(p));
}

function openMediaMenu(e: React.MouseEvent, m: MediaItem) {
  const p = project();
  if (!p) return;
  e.preventDefault();
  // The item's menu replaces the panel's, so the panel does not open its own on top.
  e.stopPropagation();
  // Right-clicking an item outside the selection selects just that item; inside it, the menu acts on the selection.
  if (!ui.get().mediaSel.includes(m.id)) ui.set({ mediaSel: [m.id] });
  openMenu(e.clientX, e.clientY, mediaMenu(targetsOf(m), p));
}

/** Sort orders the bar offers. The button beside the menu says what each direction means. */
const SORTS: { id: MediaSort; label: string; ways: [string, string] }[] = [
  { id: "imported", label: "Import order", ways: ["Oldest first", "Newest first"] },
  { id: "name", label: "Name", ways: ["A to Z", "Z to A"] },
  { id: "length", label: "Length", ways: ["Shortest first", "Longest first"] },
  { id: "type", label: "Type", ways: ["Video first", "Images first"] },
];

/** The bar at the top of the media panel: layout, size, order and how many items are selected. */
function MediaBar({ count, selected }: { count: number; selected: number }) {
  const view = ui.use((s) => s.mediaView);
  const tile = ui.use((s) => s.mediaTile);
  const sort = ui.use((s) => s.mediaSort);
  const desc = ui.use((s) => s.mediaDesc);
  const way = SORTS.find((s) => s.id === sort)!.ways[desc ? 1 : 0];
  return (
    <div className="mediabar">
      <div className="viewtoggle">
        <button className={view === "grid" ? "on" : ""} data-tip="Thumbnails in a grid" onClick={() => ui.set({ mediaView: "grid" })}>▦</button>
        <button className={view === "list" ? "on" : ""} data-tip="A list with type, length and how many clips use each item" onClick={() => ui.set({ mediaView: "list" })}>☰</button>
      </div>
      <select value={sort} data-tip="Order the media by" onChange={(e) => ui.set({ mediaSort: e.target.value as MediaSort })}>
        {SORTS.map((s) => (
          <option key={s.id} value={s.id}>{s.label}</option>
        ))}
      </select>
      <button data-tip="Reverse the order" onClick={() => ui.set({ mediaDesc: !desc })}>{way}</button>
      {view === "grid" && (
        <input
          type="range"
          min={96}
          max={240}
          step={2}
          value={tile}
          data-tip="Thumbnail size. Drag right for bigger tiles, left for more per row."
          onChange={(e) => ui.set({ mediaTile: +e.target.value })}
        />
      )}
      <span className="muted">
        {count} {count === 1 ? "item" : "items"}
        {selected > 0 && `, ${selected} selected`}
      </span>
    </div>
  );
}

/** The line under each name: length or image, and the picture size. */
function details(m: MediaItem, fps: number): string {
  return `${m.kind === "image" ? "image" : timecode(m.duration, fps)}${m.width > 0 ? ` · ${m.width}×${m.height}` : ""}`;
}

function MediaPool() {
  const p = core.use((s) => s.state)!.project;
  const fps = fpsOf(p);
  const view = ui.use((s) => s.mediaView);
  const tile = ui.use((s) => s.mediaTile);
  const sort = ui.use((s) => s.mediaSort);
  const desc = ui.use((s) => s.mediaDesc);
  const sel = ui.use((s) => s.mediaSel);
  if (p.media.length === 0) {
    return (
      <div className="empty">
        <b>No media yet</b>
        <p>Choose files with the button below, or drop them onto this panel. Right-click for more options.</p>
        <button className="primary wide" data-tip={IMPORT_TIP} onClick={importMedia}>Import files…</button>
        <button className="wide" style={{ marginTop: 6 }} data-tip={SUBTITLE_TIP} onClick={chooseSubtitles}>Import subtitles…</button>
      </div>
    );
  }
  const items = sortMedia(p.media, sort, desc);
  const uses = new Map<string, number>();
  for (const c of p.sequence.clips) if (c.media) uses.set(c.media, (uses.get(c.media) ?? 0) + 1);
  const chosen = items.filter((m) => sel.includes(m.id)).length;
  const list = view === "list";
  return (
    <>
      <MediaBar count={items.length} selected={chosen} />
      <div className={list ? "medialist" : "media"} style={list ? undefined : { gridTemplateColumns: `repeat(auto-fill, minmax(${tile}px, 1fr))` }}>
        {items.map((m) => {
          const n = uses.get(m.id) ?? 0;
          return (
            <div
              key={m.id}
              data-mid={m.id}
              className={`mi ${list ? "row" : ""} ${sel.includes(m.id) ? "sel" : ""}`}
              data-tip={`${m.path}\n${ITEM_TIP}`}
              onPointerDown={(e) => startMediaDrag(e, m)}
              onDoubleClick={() => addMediaClip(m, ui.get().frame)}
              onContextMenu={(e) => openMediaMenu(e, m)}
            >
              <Thumb m={m} />
              <button className="rm" data-tip="Remove from project. Clips already on the timeline stay." onClick={() => exec("media.remove", { media: m.id })}>✕</button>
              <div className="info">
                <div className="nm">{m.name}</div>
                <div className="meta">
                  {list ? `${m.kind === "image" ? "" : `${m.kind} · `}${details(m, fps)} · ${n} ${n === 1 ? "clip" : "clips"}` : details(m, fps)}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </>
  );
}

export function LeftPanel() {
  const tab = ui.use((s) => s.leftTab);
  const lib = core.use((s) => s.library)!;
  const edge = ui.use((s) => s.transitionEdge);
  const marquee = ui.use((s) => s.mediaMarquee);
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
      <div
        className="pane"
        onContextMenu={tab === "media" ? openPoolMenu : undefined}
        onPointerDown={tab === "media" ? startMediaMarquee : undefined}
      >
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
      {marquee && <div className="mselect" style={{ left: marquee.x, top: marquee.y, width: marquee.w, height: marquee.h }} />}
    </div>
  );
}
