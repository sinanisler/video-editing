import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { cancelExport, exportOptions, startExport } from "../export";
import type { ExportOptions, ExportSettings, ExportStatus } from "../export";
import { core, exec, toast, ui } from "../store";
import { fpsOf, mmss } from "../util";

const FRAME_RATES = [24, 25, 30, 50, 60];
const QUALITIES = [
  ["high", "High · for uploading"],
  ["standard", "Standard"],
  ["small", "Small file"],
] as const;
const ENCODER_NAMES: Record<string, string> = {
  h264_nvenc: "Graphics card (NVIDIA)",
  h264_qsv: "Graphics card (Intel)",
  h264_amf: "Graphics card (AMD)",
  libx264: "Software (slower, best quality)",
};

const close = () => ui.set({ overlay: null });

function Header({ title }: { title: string }) {
  return (
    <header>
      <span>{title}</span>
      <button className="ghost" data-tip="Close" onClick={close}>✕</button>
    </header>
  );
}

function Progress({ status }: { status: ExportStatus }) {
  const part = status.frames_total ? status.frames_done / status.frames_total : 0;
  const left = part > 0.02 ? (status.seconds / part) * (1 - part) : null;
  const running = status.state === "preparing" || status.state === "rendering";
  return (
    <div className="body export">
      <div className="exportfile" data-tip={status.path}>{status.path}</div>
      <div className="bar">
        <i style={{ width: `${(status.state === "done" ? 1 : part) * 100}%` }} className={status.state} />
      </div>
      <div className="exportline">
        {status.state === "preparing" && <span>{status.message || "Getting ready"}…</span>}
        {status.state === "rendering" && (
          <span>
            {Math.round(part * 100)}% · frame {status.frames_done} of {status.frames_total} · {mmss(status.seconds)} so far
            {left !== null && ` · about ${mmss(left)} left`}
          </span>
        )}
        {status.state === "done" && <span>Done in {mmss(status.seconds)}.</span>}
        {status.state === "cancelled" && <span>Cancelled. Nothing was saved.</span>}
        {status.state === "failed" && <span className="bad">{status.message}</span>}
      </div>
      {status.warnings.map((w) => (
        <div key={w} className="exportwarn">{w}</div>
      ))}
      <div className="actions">
        {running ? (
          <button data-tip="Stop rendering. The file is not saved." onClick={() => cancelExport()}>Cancel export</button>
        ) : (
          <button data-tip="Go back to the export settings to make another file" onClick={() => core.set({ exportStatus: { ...status, state: "idle" } })}>New export</button>
        )}
        <button
          className="primary"
          data-tip={running ? "Close this window. The export keeps going; the status bar shows its progress." : "Close this window"}
          onClick={close}
        >
          {running ? "Keep working" : "Close"}
        </button>
      </div>
    </div>
  );
}

export function ExportDialog() {
  const status = core.use((s) => s.exportStatus);
  const project = core.use((s) => s.state)!.project;
  const [options, setOptions] = useState<ExportOptions | null>(null);
  const [preset, setPreset] = useState("project");
  const [settings, setSettings] = useState<ExportSettings | null>(null);

  useEffect(() => {
    exportOptions().then((o) => {
      const first = o.presets[0];
      setOptions(o);
      setSettings({ path: o.path, width: first.width, height: first.height, fps: first.fps, quality: first.quality, encoder: "", range: "whole", audio: "first" });
    }, (e) => toast(String(e)));
  }, []);

  if (status && status.state !== "idle") {
    return (
      <div className="modal">
        <Header title="Export" />
        <Progress status={status} />
      </div>
    );
  }
  if (!options || !settings) {
    return (
      <div className="modal">
        <Header title="Export" />
        <div className="body">Checking what this computer can encode…</div>
      </div>
    );
  }

  const change = (patch: Partial<ExportSettings>) => setSettings({ ...settings, ...patch });
  const choose = (id: string) => {
    const p = options.presets.find((x) => x.id === id);
    setPreset(id);
    if (p) change({ width: p.width, height: p.height, fps: p.fps, quality: p.quality });
  };
  const browse = async () => {
    const path = await save({ defaultPath: settings.path, filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv"] }] });
    if (path) change({ path });
  };
  const rates = [...new Set([...FRAME_RATES, fpsOf(project), settings.fps])].sort((a, b) => a - b);
  const sizes = [...new Map(options.presets.map((p) => [`${p.width}×${p.height}`, p])).values()];
  const hint = options.presets.find((p) => p.id === preset)?.hint;

  return (
    <div className="modal">
      <Header title="Export" />
      <div className="body export">
        <div className="row">
          <span className="lbl">Preset</span>
          <select className="span" data-tip="A ready-made size, frame rate and quality. You can still change each setting below." value={preset} onChange={(e) => choose(e.target.value)}>
            {options.presets.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </div>
        {hint && <div className="exporthint">{hint}</div>}
        <div className="row">
          <span className="lbl">Save as</span>
          <input type="text" className="path" data-tip="Full path of the video file to write" value={settings.path} onChange={(e) => change({ path: e.target.value })} />
          <button data-tip="Choose where to save the file and what to name it" onClick={browse}>Browse…</button>
        </div>
        <div className="row">
          <span className="lbl">Size</span>
          <select
            className="span"
            data-tip="Picture width × height in pixels. Larger is sharper and makes a bigger file."
            value={`${settings.width}×${settings.height}`}
            onChange={(e) => {
              const p = sizes.find((x) => `${x.width}×${x.height}` === e.target.value);
              if (p) change({ width: p.width, height: p.height });
            }}
          >
            {sizes.map((p) => (
              <option key={`${p.width}×${p.height}`}>{`${p.width}×${p.height}`}</option>
            ))}
          </select>
        </div>
        <div className="row">
          <span className="lbl">Frame rate</span>
          <select className="span" data-tip="Frames per second. Match the project unless the video must play at another rate." value={settings.fps} onChange={(e) => change({ fps: Number(e.target.value) })}>
            {rates.map((r) => (
              <option key={r} value={r}>{r} fps{r === fpsOf(project) ? " · project" : ""}</option>
            ))}
          </select>
        </div>
        <div className="row">
          <span className="lbl">Quality</span>
          <select className="span" data-tip="How finely the picture is compressed. Higher quality gives a bigger file and takes longer." value={settings.quality} onChange={(e) => change({ quality: e.target.value as ExportSettings["quality"] })}>
            {QUALITIES.map(([id, label]) => (
              <option key={id} value={id}>{label}</option>
            ))}
          </select>
        </div>
        <div className="row">
          <span className="lbl">Encoder</span>
          <select className="span" data-tip="Which part of this computer does the encoding. A graphics card is faster; software gives the best quality for its size." value={settings.encoder} onChange={(e) => change({ encoder: e.target.value })}>
            <option value="">Automatic{options.encoders[0] ? ` · ${ENCODER_NAMES[options.encoders[0]] ?? options.encoders[0]}` : ""}</option>
            {options.encoders.map((e) => (
              <option key={e} value={e}>{ENCODER_NAMES[e] ?? e}</option>
            ))}
          </select>
        </div>
        <div className="row">
          <span className="lbl">Range</span>
          <select className="span" data-tip="Export the whole timeline, or only the part between the in and out markers (I and O)" value={settings.range} onChange={(e) => change({ range: e.target.value as ExportSettings["range"] })}>
            <option value="whole">Whole timeline</option>
            <option value="in_out" disabled={!options.has_range}>Between in and out points</option>
          </select>
        </div>
        <div className="row">
          <span className="lbl">Sound</span>
          <select className="span" data-tip="Which audio tracks go into the file" value={settings.audio} onChange={(e) => change({ audio: e.target.value as ExportSettings["audio"] })}>
            <option value="first">First audio track of each file · as previewed</option>
            <option value="all">All audio tracks of each file, mixed</option>
          </select>
        </div>
        {options.encoders.length === 0 && <div className="exportwarn">FFmpeg was not found. Install it with: winget install Gyan.FFmpeg</div>}
        <div className="actions">
          <button data-tip="Close without exporting" onClick={close}>Cancel</button>
          <button
            className="primary"
            data-tip="Render the timeline to a video file with these settings"
            disabled={!options.encoders.length}
            onClick={() => startExport(settings)}
          >
            Export
          </button>
        </div>
      </div>
    </div>
  );
}

/** Picture size and frame rate of the project. */
export function ProjectDialog() {
  const s = core.use((x) => x.state)!.project.settings;
  const [width, setWidth] = useState(s.width);
  const [height, setHeight] = useState(s.height);
  const [fps, setFps] = useState(s.fps_num / s.fps_den);
  const rates = [...new Set([...FRAME_RATES, fps])].sort((a, b) => a - b);
  const apply = async () => {
    if (await exec("project.set_settings", { width, height, fps })) close();
  };
  return (
    <div className="modal">
      <Header title="Project settings" />
      <div className="body export">
        <div className="row">
          <span className="lbl">Width</span>
          <input type="number" className="span" data-tip="Picture width in pixels" min={16} step={2} value={width} onChange={(e) => setWidth(Number(e.target.value))} />
        </div>
        <div className="row">
          <span className="lbl">Height</span>
          <input type="number" className="span" data-tip="Picture height in pixels" min={16} step={2} value={height} onChange={(e) => setHeight(Number(e.target.value))} />
        </div>
        <div className="row">
          <span className="lbl">Frame rate</span>
          <select className="span" data-tip="Frames per second for the whole project" value={fps} onChange={(e) => setFps(Number(e.target.value))}>
            {rates.map((r) => (
              <option key={r} value={r}>{r} fps</option>
            ))}
          </select>
        </div>
        <div className="exporthint">A new frame rate keeps every clip at the same moment. It can be undone.</div>
        <div className="actions">
          <button data-tip="Close without changing the project" onClick={close}>Cancel</button>
          <button className="primary" data-tip="Change the project's picture size and frame rate" onClick={apply}>Apply</button>
        </div>
      </div>
    </div>
  );
}

/** A small running total in the status bar; click to open the export window. */
export function ExportBadge() {
  const status = core.use((s) => s.exportStatus);
  if (!status || status.state === "idle") return null;
  const part = status.frames_total ? Math.round((status.frames_done / status.frames_total) * 100) : 0;
  const label = { preparing: "Export: getting ready", rendering: `Exporting ${part}%`, done: "Export done", failed: "Export failed", cancelled: "Export cancelled", idle: "" }[status.state];
  return (
    <span className={`exportbadge ${status.state}`} data-tip="Open the export window" onClick={() => ui.set({ overlay: "export" })}>
      {label}
    </span>
  );
}
