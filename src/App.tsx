import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";
import { clampPlayhead, handleKey, timelineApi } from "./commands";
import { addMediaClips, importPaths } from "./media";
import { Overlays, StatusBar, TopBar } from "./components/Chrome";
import { LeftPanel } from "./components/LeftPanel";
import { Preview } from "./components/Preview";
import { RightPanel } from "./components/RightPanel";
import { Timeline } from "./components/Timeline";
import { Tooltip } from "./components/Tooltip";
import { ContextMenu } from "./components/ContextMenu";
import { initExport } from "./export";
import { core, init, ui } from "./store";
import { clamp } from "./util";

/** Files dragged in from outside the app: import them, and place them if they landed on the timeline. */
function listenForFileDrops() {
  return getCurrentWebview().onDragDropEvent(async ({ payload }) => {
    if (payload.type === "enter" || payload.type === "over") return ui.set({ fileDrag: true });
    ui.set({ fileDrag: false });
    if (payload.type !== "drop") return;
    // Tauri reports the drop point in physical pixels.
    const x = payload.position.x / window.devicePixelRatio;
    const y = payload.position.y / window.devicePixelRatio;
    const target = timelineApi.dropTarget(x, y);
    const items = await importPaths(payload.paths);
    if (target && items.length) await addMediaClips(items, target.frame, target.track);
  });
}

function DragFeedback() {
  const fileDrag = ui.use((s) => s.fileDrag);
  const drag = ui.use((s) => s.mediaDrag);
  return (
    <>
      {fileDrag && <div id="dropveil">Drop to import · on the timeline to place it there</div>}
      {drag && (
        <div id="dragghost" style={{ left: drag.x, top: drag.y }}>
          {drag.name}
        </div>
      )}
    </>
  );
}

/** Drags a panel edge. `sign` is +1 when moving the pointer right makes the panel wider, -1 when it makes it narrower. */
function startPanelResize(e: React.PointerEvent, key: "leftW" | "rightW", sign: 1 | -1) {
  if (e.button !== 0) return;
  e.preventDefault();
  const x0 = e.clientX;
  const w0 = ui.get()[key];
  const move = (ev: PointerEvent) => ui.set({ [key]: clamp(Math.round(w0 + sign * (ev.clientX - x0)), 220, 560) });
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

export function App() {
  const state = core.use((s) => s.state);
  const timelineH = ui.use((s) => s.timelineH);
  const leftW = ui.use((s) => s.leftW);
  const rightW = ui.use((s) => s.rightW);
  const [error, setError] = useState("");

  useEffect(() => {
    init()
      .then(initExport)
      .catch((e) => setError(String(e)));
    window.addEventListener("keydown", handleKey);
    // The browser menu (Reload, Print, Inspect) has no place in the editor. Text fields keep theirs for copy and paste.
    const noMenu = (e: MouseEvent) => !/^(input|textarea)$/i.test((e.target as HTMLElement).tagName) && e.preventDefault();
    window.addEventListener("contextmenu", noMenu);
    const drops = listenForFileDrops();
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("contextmenu", noMenu);
      drops.then((stop) => stop());
    };
  }, []);

  // An undo or an agent edit can shorten the timeline under the playhead.
  useEffect(clampPlayhead, [state]);

  if (!state) {
    return (
      <div className="boot">
        {error ? `Could not reach the editor core: ${error}. This page only works inside the snn-editor app.` : "Starting…"}
      </div>
    );
  }
  return (
    <div id="app" style={{ gridTemplateRows: `42px 1fr ${timelineH}px 30px` }}>
      <TopBar />
      <div id="main" style={{ gridTemplateColumns: `${leftW}px 1fr ${rightW}px` }}>
        <i className="pgrip v" data-tip="Drag to make the media panel wider or narrower. Double-click to reset." style={{ left: leftW - 3 }} onPointerDown={(e) => startPanelResize(e, "leftW", 1)} onDoubleClick={() => ui.set({ leftW: 330 })} />
        <i className="pgrip v" data-tip="Drag to make the inspector wider or narrower. Double-click to reset." style={{ left: rightW - 3 }} onPointerDown={(e) => startPanelResize(e, "rightW", -1)} onDoubleClick={() => ui.set({ rightW: 370 })} />
        <LeftPanel />
        <Preview />
        <RightPanel />
      </div>
      <Timeline />
      <StatusBar />
      <Overlays />
      <DragFeedback />
      <Tooltip />
      <ContextMenu />
    </div>
  );
}
