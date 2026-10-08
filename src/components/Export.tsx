import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { cancelExport, codecOf, exportOptions, softwareFor, startExport } from "../export";
import type { Codec, ExportOptions, ExportPreset, ExportSettings, ExportStatus, RateMode } from "../export";
import { core, exec, toast, ui } from "../store";
import { fpsOf, mmss, timelineFrames } from "../util";

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
  hevc_nvenc: "Graphics card (NVIDIA)",
  hevc_qsv: "Graphics card (Intel)",
  hevc_amf: "Graphics card (AMD)",
  libx265: "Software (slower, best quality)",
};
const CODECS: [Codec, string, string][] = [
  ["h264", "H.264", "Plays everywhere. What YouTube asks for."],
  ["hevc", "H.265 (HEVC)", "Smaller files at the same quality. Not every player reads it."],
];
const AUDIO_RATES = [128, 192, 256, 320, 384];
/** Settings a preset fills in. Changing any of them makes the recipe custom. */
const RECIPE_KEYS: string[] = ["width", "height", "fps", "quality", "codec", "rate", "bitrate_kbps", "max_kbps", "two_pass", "keyframe_frames", "b_frames", "audio_kbps"];

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

/** The settings a preset sets. Everything else (file, range, sound source, encoder) stays as the user left it. */
const recipeOf = (p: ExportPreset): Partial<ExportSettings> => ({
  width: p.width,
  height: p.height,
  fps: p.fps,
  quality: p.quality,
  codec: p.codec,
  rate: p.rate,
  bitrate_kbps: p.bitrate_kbps,
  max_kbps: p.max_kbps,
  two_pass: false,
  keyframe_frames: 0,
  b_frames: 2,
  audio_kbps: 384,
});

const isSoftwareEncoder = (encoder: string) => encoder === "libx264" || encoder === "libx265";
const mbps = (kbps: number) => +(kbps / 1000).toFixed(1);

/** YouTube's recommended SDR bitrate in kilobits a second, for a picture height and frame rate. */
function youtubeKbps(height: number, fps: number): number | undefined {
  const pairs: Record<number, [number, number]> = { 1080: [8000, 12000], 1440: [16000, 24000], 2160: [40000, 60000] };
  const pair = pairs[height];
  return pair && (fps >= 48 ? pair[1] : pair[0]);
}

function containerName(path: string): string {
  const ending = path.split(".").pop()?.toLowerCase();
  if (ending === "mp4") return "MP4";
  if (ending === "mov") return "QuickTime (MOV)";
  if (ending === "mkv") return "Matroska (MKV)";
  return "not known; end the name with .mp4, .mov or .mkv";
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
      setSettings({ ...recipeOf(first), path: o.path, encoder: "", range: "whole", audio: "first" } as ExportSettings);
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

  const change = (patch: Partial<ExportSettings>) => {
    const next = { ...settings, ...patch };
    // Two passes run on the processor, so choosing a graphics-card encoder turns them off.
    if (patch.encoder !== undefined && !isSoftwareEncoder(patch.encoder)) next.two_pass = false;
    setSettings(next);
    if (Object.keys(patch).some((key) => RECIPE_KEYS.includes(key))) setPreset("custom");
  };
  const choose = (id: string) => {
    const p = options.presets.find((x) => x.id === id);
    if (!p) return;
    setPreset(id);
    setSettings({ ...settings, ...recipeOf(p), encoder: codecOf(settings.encoder) === p.codec ? settings.encoder : "" } as ExportSettings);
  };
  const browse = async () => {
    const path = await save({ defaultPath: settings.path, filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv"] }] });
    if (path) change({ path });
  };

  // What this computer can do for the chosen codec.
  const codec = settings.codec;
  const working = options.encoders.filter((e) => codecOf(e) === codec);
  const broken = options.unavailable.filter((u) => codecOf(u.encoder) === codec);
  const software = softwareFor(codec);
  const codecLabel = codec === "hevc" ? "H.265" : "H.264";
  const encoderWorks = settings.encoder ? working.includes(settings.encoder) : working.length > 0;
  const twoPassAllowed = settings.rate === "vbr" && (settings.encoder ? isSoftwareEncoder(settings.encoder) : options.encoders.includes(software));

  const rates = [...new Set([...FRAME_RATES, fpsOf(project), settings.fps])].sort((a, b) => a - b);
  const sizes = [...new Map(options.presets.map((p) => [`${p.width}×${p.height}`, p])).values()];
  const hint = options.presets.find((p) => p.id === preset)?.hint;
  const youtube = youtubeKbps(settings.height, settings.fps);

  // Length of what will be exported, and the file size a bitrate implies.
  const seq = project.sequence;
  const frames = settings.range === "in_out" ? (seq.out_point ?? timelineFrames(project)) - (seq.in_point ?? 0) : timelineFrames(project);
  const seconds = Math.max(0, frames) / fpsOf(project);
  const megabytes = settings.rate === "quality" ? null : ((settings.bitrate_kbps + settings.audio_kbps) * 1000 * seconds) / 8 / 1e6;
  const rateText =
    settings.rate === "quality"
      ? `quality ${QUALITIES.find(([id]) => id === settings.quality)?.[1] ?? settings.quality}`
      : settings.rate === "vbr"
        ? `variable bitrate, target ${mbps(settings.bitrate_kbps)} Mbps, up to ${mbps(settings.max_kbps)} Mbps`
        : `constant bitrate ${mbps(settings.bitrate_kbps)} Mbps`;

  const problems: string[] = [];
  if (!options.ffmpeg.supports_export) {
    problems.push(
      options.ffmpeg.version
        ? `This FFmpeg (${options.ffmpeg.version}) is too old for exports; FFmpeg 7.0 or newer is needed. Install it with: ${options.ffmpeg.install}`
        : `FFmpeg was not found. Install it with: ${options.ffmpeg.install}`,
    );
  } else if (!encoderWorks) {
    const why = broken.map((u) => `${ENCODER_NAMES[u.encoder]}: ${u.reason}`).join(" ");
    problems.push(`No ${codecLabel} encoder works on this computer. ${why}`);
  }
  if (settings.two_pass && settings.rate !== "vbr") problems.push("Two passes need variable bitrate.");
  if (settings.two_pass && !twoPassAllowed) problems.push(`Two passes run on the processor, and ${software} does not work here.`);
  if (settings.rate !== "quality" && (settings.bitrate_kbps < 100 || settings.bitrate_kbps > 200000)) problems.push("The target bitrate must be between 0.1 and 200 Mbps.");
  if (settings.rate === "vbr" && settings.max_kbps < settings.bitrate_kbps) problems.push("The maximum bitrate must be at least the target.");

  return (
    <div className="modal">
      <Header title="Export" />
      <div className="body export">
        <div className="exportsec">Preset</div>
        <div className="row">
          <span className="lbl">Preset</span>
          <select className="span" data-tip="A ready-made recipe for a platform or a quality. Changing any setting below makes it a custom recipe." value={preset} onChange={(e) => choose(e.target.value)}>
            {preset === "custom" && <option value="custom">Custom</option>}
            {options.presets.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </div>
        {hint && <div className="exporthint">{hint}</div>}

        <div className="exportsec">File</div>
        <div className="row">
          <span className="lbl">Save as</span>
          <input type="text" className="path" data-tip="Full path of the video file to write" value={settings.path} onChange={(e) => change({ path: e.target.value })} />
          <button data-tip="Choose where to save the file and what to name it" onClick={browse}>Browse…</button>
        </div>
        <div className="exporthint">Container: {containerName(settings.path)}</div>

        <div className="exportsec">Video</div>
        <div className="row">
          <span className="lbl">Codec</span>
          <select className="span" data-tip="The compression format. H.264 plays everywhere; H.265 makes smaller files." value={codec} onChange={(e) => change({ codec: e.target.value as Codec, encoder: settings.encoder && codecOf(settings.encoder) !== e.target.value ? "" : settings.encoder })}>
            {CODECS.map(([id, label]) => (
              <option key={id} value={id}>{label}</option>
            ))}
          </select>
        </div>
        <div className="exporthint">{CODECS.find(([id]) => id === codec)?.[2]}</div>
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
          <span className="lbl">Encoder</span>
          <select className="span" data-tip="Which part of this computer does the encoding. A graphics card is faster; software gives the best quality for its size." value={settings.encoder} onChange={(e) => change({ encoder: e.target.value })}>
            <option value="">Automatic{working[0] ? ` · ${ENCODER_NAMES[working[0]]}` : " · none works here"}</option>
            {working.map((e) => (
              <option key={e} value={e}>{ENCODER_NAMES[e] ?? e}</option>
            ))}
            {broken.map((u) => (
              <option key={u.encoder} value={u.encoder} disabled>{ENCODER_NAMES[u.encoder] ?? u.encoder} · not available</option>
            ))}
          </select>
        </div>
        {broken.map((u) => (
          <div key={u.encoder} className="exporthint">{ENCODER_NAMES[u.encoder] ?? u.encoder} is not available: {u.reason}</div>
        ))}
        <div className="row">
          <span className="lbl">Rate control</span>
          <select className="span" data-tip="Quality keeps one quality and lets the file size follow the picture. Variable bitrate aims at a target size and rises for busy scenes. Constant bitrate is the same every second." value={settings.rate} onChange={(e) => change({ rate: e.target.value as RateMode })}>
            <option value="quality">Quality · one quality for the whole picture</option>
            <option value="vbr">Variable bitrate · a target and a maximum</option>
            <option value="cbr">Constant bitrate · the same every second</option>
          </select>
        </div>
        {settings.rate === "quality" && (
          <div className="row">
            <span className="lbl">Quality</span>
            <select className="span" data-tip="How finely the picture is compressed. Higher quality gives a bigger file and takes longer." value={settings.quality} onChange={(e) => change({ quality: e.target.value as ExportSettings["quality"] })}>
              {QUALITIES.map(([id, label]) => (
                <option key={id} value={id}>{label}</option>
              ))}
            </select>
          </div>
        )}
        {settings.rate !== "quality" && (
          <>
            <div className="row">
              <span className="lbl">Target (Mbps)</span>
              <input type="number" className="span" min={0.1} max={200} step={0.5} data-tip="The bitrate the file aims at, in megabits a second" value={mbps(settings.bitrate_kbps)} onChange={(e) => change({ bitrate_kbps: Math.round(Number(e.target.value) * 1000) })} />
            </div>
            {youtube !== undefined && (
              <div className="exporthint">YouTube recommends {mbps(youtube)} Mbps for {settings.height}p at {settings.fps >= 48 ? "48–60" : "24–30"} fps.</div>
            )}
          </>
        )}
        {settings.rate === "vbr" && (
          <div className="row">
            <span className="lbl">Maximum (Mbps)</span>
            <input type="number" className="span" min={0.1} max={200} step={0.5} data-tip="The highest bitrate any part of the picture may use" value={mbps(settings.max_kbps)} onChange={(e) => change({ max_kbps: Math.round(Number(e.target.value) * 1000) })} />
          </div>
        )}
        <div className="row">
          <span className="lbl">Two passes</span>
          <input type="checkbox" checked={settings.two_pass} disabled={!twoPassAllowed} data-tip="Measure the picture first, then encode, so the bitrate is spread evenly. Slower. Software encoders only." onChange={(e) => change({ two_pass: e.target.checked })} />
        </div>
        <div className="exporthint">
          {settings.rate !== "vbr"
            ? "Two passes need variable bitrate."
            : settings.encoder && !isSoftwareEncoder(settings.encoder)
              ? "Turned off for a graphics-card encoder, which works in one pass."
              : "Slower, but spreads the bitrate evenly over the file."}
        </div>
        <div className="row">
          <span className="lbl">Keyframe every</span>
          <input type="number" className="span" min={0} max={1000} step={1} data-tip="Frames between keyframes. 0 means half the frame rate, which YouTube asks for." value={settings.keyframe_frames} onChange={(e) => change({ keyframe_frames: Math.max(0, Math.round(Number(e.target.value))) })} />
        </div>
        <div className="exporthint">
          {settings.keyframe_frames > 0 ? `A keyframe every ${settings.keyframe_frames} frames.` : `Automatic: every ${Math.max(1, Math.round(settings.fps / 2))} frames, half the frame rate.`}
        </div>
        <div className="row">
          <span className="lbl">B-frames</span>
          <select className="span" data-tip="Frames predicted from both the past and the future. More saves space; YouTube asks for 2." value={settings.b_frames} onChange={(e) => change({ b_frames: Number(e.target.value) })}>
            {[0, 1, 2, 3, 4].map((n) => (
              <option key={n} value={n}>{n === 2 ? "2 · YouTube's choice" : n === 0 ? "0 · none" : String(n)}</option>
            ))}
          </select>
        </div>

        <div className="exportsec">Audio</div>
        <div className="row">
          <span className="lbl">Sound</span>
          <select className="span" data-tip="Which audio tracks go into the file" value={settings.audio} onChange={(e) => change({ audio: e.target.value as ExportSettings["audio"] })}>
            <option value="first">First audio track of each file · as previewed</option>
            <option value="all">All audio tracks of each file, mixed</option>
          </select>
        </div>
        <div className="row">
          <span className="lbl">Bitrate</span>
          <select className="span" data-tip="How much data each second of sound takes. Stereo AAC at 384 kbps is YouTube's choice." value={settings.audio_kbps} onChange={(e) => change({ audio_kbps: Number(e.target.value) })}>
            {AUDIO_RATES.map((r) => (
              <option key={r} value={r}>{r} kbps{r === 384 ? " · YouTube's stereo choice" : ""}</option>
            ))}
          </select>
        </div>

        <div className="exportsec">Range</div>
        <div className="row">
          <span className="lbl">Range</span>
          <select className="span" data-tip="Export the whole timeline, or only the part between the in and out markers (I and O)" value={settings.range} onChange={(e) => change({ range: e.target.value as ExportSettings["range"] })}>
            <option value="whole">Whole timeline</option>
            <option value="in_out" disabled={!options.has_range}>Between in and out points</option>
          </select>
        </div>

        <div className="exportsec">Summary</div>
        <div className="exportsummary">
          <div>
            {settings.width}×{settings.height} · {settings.fps} fps · {codecLabel} · {rateText} · sound {settings.audio_kbps} kbps AAC
          </div>
          <div>
            {mmss(seconds)} long · {megabytes === null ? "file size depends on the picture" : `about ${megabytes < 1000 ? `${Math.round(megabytes)} MB` : `${(megabytes / 1000).toFixed(2)} GB`}`}
          </div>
        </div>
        {problems.map((p) => (
          <div key={p} className="exportwarn">{p}</div>
        ))}
        <div className="exporthint">FFmpeg in use: {options.ffmpeg.version || "not found"}</div>
        <div className="actions">
          <button data-tip="Close without exporting" onClick={close}>Cancel</button>
          <button
            className="primary"
            data-tip={problems.length ? problems[0] : "Render the timeline to a video file with these settings"}
            disabled={problems.length > 0}
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
