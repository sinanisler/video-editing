import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";
import { clampPlayhead, handleKey, timelineApi } from "./commands";
import { addMediaClips, importPaths } from "./media";
import { Overlays, StatusBar, TopBar } from "./components/Chrome";
import { LeftPanel } from "./components/LeftPanel";
import { Preview } from "./components/Preview";
import { RightPanel } from "./components/RightPanel";
import { Timeline } from "./components/Timeline";
import { initExport } from "./export";
import { core, init, ui } from "./store";

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

export function App() {
  const state = core.use((s) => s.state);
  const [error, setError] = useState("");

  useEffect(() => {
    init()
      .then(initExport)
      .catch((e) => setError(String(e)));
    window.addEventListener("keydown", handleKey);
    const drops = listenForFileDrops();
    return () => {
      window.removeEventListener("keydown", handleKey);
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
    <div id="app">
      <TopBar />
      <div id="main">
        <LeftPanel />
        <Preview />
        <RightPanel />
      </div>
      <Timeline />
      <StatusBar />
      <Overlays />
      <DragFeedback />
    </div>
  );
}
