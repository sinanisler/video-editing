// Starts exports and follows their progress. The Rust side renders; this side draws the text
// clips first, because they are styled with the window's CSS. See `exportText.tsx`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { blobBase64, renderText } from "./exportText";
import { core, toast } from "./store";

export type Quality = "high" | "standard" | "small";
export type Codec = "h264" | "hevc";
/** quality: one quality for the whole picture. vbr: a target that rises and falls, up to a maximum. cbr: the same bitrate every second. */
export type RateMode = "quality" | "vbr" | "cbr";

export interface ExportSettings {
  path: string;
  width: number;
  height: number;
  fps: number;
  quality: Quality;
  /** Empty for the best encoder for the codec that the machine has. */
  encoder: string;
  range: "whole" | "in_out";
  audio: "first" | "all";
  codec: Codec;
  rate: RateMode;
  /** Target video bitrate in kilobits a second, for vbr and cbr. */
  bitrate_kbps: number;
  /** Highest bitrate in kilobits a second, for vbr. */
  max_kbps: number;
  two_pass: boolean;
  /** Frames between keyframes; 0 for half the frame rate. */
  keyframe_frames: number;
  b_frames: number;
  /** Stereo AAC bitrate in kilobits a second. */
  audio_kbps: number;
}

export interface ExportPreset {
  id: string;
  label: string;
  hint: string;
  width: number;
  height: number;
  fps: number;
  quality: Quality;
  codec: Codec;
  rate: RateMode;
  bitrate_kbps: number;
  max_kbps: number;
}

export interface ExportOptions {
  presets: ExportPreset[];
  /** Encoders that work on this machine, best first. */
  encoders: string[];
  /** Encoders that do not work here, with FFmpeg's reason. */
  unavailable: { encoder: string; reason: string }[];
  /** A suggested output file. */
  path: string;
  has_range: boolean;
  /** The FFmpeg this app runs: its version, whether it can export at all, and the command that installs the one it needs. */
  ffmpeg: { version: string; supports_export: boolean; install: string };
}

/** The codec an encoder makes. Mirrors `codec_of` in the Rust core, which is what decides. */
export function codecOf(encoder: string): Codec | null {
  if (["h264_nvenc", "h264_qsv", "h264_amf", "libx264"].includes(encoder)) return "h264";
  if (["hevc_nvenc", "hevc_qsv", "hevc_amf", "libx265"].includes(encoder)) return "hevc";
  return null;
}

/** The software encoder for a codec, which two passes use. */
export const softwareFor = (codec: Codec): string => (codec === "hevc" ? "libx265" : "libx264");

export interface ExportStatus {
  state: "idle" | "preparing" | "rendering" | "done" | "failed" | "cancelled";
  job: number;
  path: string;
  encoder: string;
  frames_done: number;
  frames_total: number;
  seconds: number;
  message: string;
  warnings: string[];
}

export const exportOptions = () => invoke<ExportOptions>("export_options");
export const cancelExport = () => invoke<ExportStatus>("export_cancel");

/**
 * Runs an export from start to finish: reserve the job, draw the text, hand over to FFmpeg.
 * `job` is set when an agent asked for the export and the job already exists.
 */
export async function startExport(settings: ExportSettings, job?: number): Promise<boolean> {
  const { state, library } = core.get();
  if (!state || !library) return false;
  let begun: { job: number; from: number; to: number; width: number; height: number };
  try {
    begun = await invoke("export_begin", { settings, job: job ?? null });
  } catch (e) {
    toast(String(e));
    return false;
  }
  try {
    const size = { width: begun.width, height: begun.height };
    const save = async (name: string, png: Blob) => invoke<void>("export_image", { job: begun.job, name, data: await blobBase64(png) });
    const drawing = (done: number, total: number) => {
      const current = core.get().exportStatus;
      if (current?.job === begun.job) core.set({ exportStatus: { ...current, message: `Drawing text ${done} of ${total}` } });
    };
    const text = await renderText(state.project, library, size, { from: begun.from, to: begun.to }, save, drawing);
    await invoke("export_run", { job: begun.job, text });
    return true;
  } catch (e) {
    await invoke("export_fail", { job: begun.job, message: `The export could not start: ${e}` });
    return false;
  }
}

let listening = false;

export async function initExport() {
  // React runs effects twice in development; two listeners would start every requested export twice.
  if (listening) return;
  listening = true;
  await listen<ExportStatus>("export:status", ({ payload }) => {
    const before = core.get().exportStatus;
    core.set({ exportStatus: payload });
    if (payload.state !== before?.state) {
      if (payload.state === "done") toast(`Exported ${payload.path}`);
      if (payload.state === "failed") toast(`Export failed: ${payload.message}`);
    }
  });
  // An agent asked for an export over MCP.
  await listen<{ job: number; settings: ExportSettings }>("export:request", ({ payload }) => {
    void startExport(payload.settings, payload.job);
  });
  core.set({ exportStatus: await invoke<ExportStatus>("export_status") });
}
