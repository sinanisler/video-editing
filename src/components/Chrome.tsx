import { useEffect, useRef, useState } from "react";
import { COMMANDS, runCommand } from "../commands";
import { core, ui } from "../store";
import { fpsOf } from "../util";
import { ExportBadge, ExportDialog, ProjectDialog } from "./Export";

export function TopBar() {
  const s = core.use((x) => x.state)!;
  const mcp = core.use((x) => x.mcp);
  return (
    <div id="top">
      <span className="brand">
        snn<i>-editor</i>
      </span>
      <button className="ghost" title="New project (Ctrl+N)" onClick={() => runCommand("new")}>New</button>
      <button className="ghost" title="Open project (Ctrl+O)" onClick={() => runCommand("open")}>Open</button>
      <button className="ghost" title="Save project (Ctrl+S)" onClick={() => runCommand("save")}>Save</button>
      <button className="ghost" title="Export video (Ctrl+E)" onClick={() => runCommand("export")}>Export</button>
      <button className="ghost" title="Keyboard shortcuts (?)" onClick={() => runCommand("shortcuts")}>Shortcuts</button>
      <span className="proj" title={s.path ?? "Not saved to a file yet"}>
        <b>{s.project.name}</b> · {s.project.sequence.name}
        {s.dirty && " · unsaved"}
      </span>
      <button className="ghost" disabled={!s.can_undo} title={`Undo ${s.undo_label ?? ""} (Ctrl+Z)`} onClick={() => runCommand("undo")}>
        ↶
      </button>
      <button className="ghost" disabled={!s.can_redo} title={`Redo ${s.redo_label ?? ""} (Ctrl+Shift+Z)`} onClick={() => runCommand("redo")}>
        ↷
      </button>
      <button className="ghost" onClick={() => runCommand("palette")}>
        Commands <kbd>Ctrl K</kbd>
      </button>
      <span className="pill" title="MCP server status" onClick={() => ui.set({ rightTab: "agent" })}>
        <span className={mcp?.running ? "dot" : "dot off"} />
        MCP · {mcp?.running ? "on" : "off"}
      </span>
    </div>
  );
}

export function StatusBar() {
  const s = core.use((x) => x.state)!;
  const { settings, sequence } = s.project;
  return (
    <div id="status">
      <span>{s.dirty ? "○ Unsaved changes" : s.path ? "● Saved" : "● No changes"}</span>
      <span>History: {s.history_len}</span>
      <span>{sequence.clips.length} clips</span>
      <ExportBadge />
      <span className="sp" />
      <span className="link" title="Project settings" onClick={() => runCommand("projectsettings")}>
        {settings.width}×{settings.height} · {fpsOf(s.project)} fps
      </span>
      <span>
        Press <kbd>?</kbd> for shortcuts
      </span>
    </div>
  );
}

const Keys = ({ combo }: { combo: string }) => (
  <span>
    {/* Split on "+" except a trailing one, which is the plus key itself. */}
    {combo.split(/\+(?!$)/).map((k, i) => (
      <kbd key={i}>{k}</kbd>
    ))}
  </span>
);

function Palette() {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const matches = COMMANDS.filter((c) => c.title.toLowerCase().includes(query.toLowerCase()));
  const current = Math.min(index, Math.max(0, matches.length - 1));
  const run = (i: number) => {
    ui.set({ overlay: null });
    matches[i]?.run();
  };
  return (
    <div className="modal">
      <input
        id="palq"
        ref={input}
        placeholder="Type a command…"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setIndex(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") setIndex(Math.min(current + 1, matches.length - 1));
          else if (e.key === "ArrowUp") setIndex(Math.max(current - 1, 0));
          else if (e.key === "Enter") run(current);
          else return;
          e.preventDefault();
        }}
      />
      <div className="pallist">
        {matches.map((c, i) => (
          <div key={c.id} className={`pi ${i === current ? "cur" : ""}`} onClick={() => run(i)}>
            <span>{c.title}</span>
            {c.keys && <Keys combo={c.keys[0]} />}
          </div>
        ))}
      </div>
    </div>
  );
}

function Shortcuts() {
  return (
    <div className="modal wide">
      <header>
        <span>Keyboard shortcuts</span>
        <button className="ghost" onClick={() => ui.set({ overlay: null })}>✕</button>
      </header>
      <div className="body sc">
        {COMMANDS.filter((c) => c.keys).map((c) => (
          <div key={c.id}>
            <span>{c.title}</span>
            <Keys combo={c.keys![0]} />
          </div>
        ))}
      </div>
    </div>
  );
}

export function Overlays() {
  const overlay = ui.use((s) => s.overlay);
  const toast = ui.use((s) => s.toast);
  return (
    <>
      {overlay && (
        <div className="ov" onPointerDown={(e) => e.target === e.currentTarget && ui.set({ overlay: null })}>
          {overlay === "palette" ? <Palette /> : overlay === "export" ? <ExportDialog /> : overlay === "project" ? <ProjectDialog /> : <Shortcuts />}
        </div>
      )}
      <div id="toast" className={toast ? "show" : ""}>{toast}</div>
    </>
  );
}
