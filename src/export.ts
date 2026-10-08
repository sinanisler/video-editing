// Starts exports and follows their progress. The Rust side renders; this side draws the text
// clips first, because they are styled with the window's CSS. See `exportText.tsx`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { blobBase64, renderText } from "./exportText";
import { core, toast } from "./store";

export type Quality = "high" | "standard" | "small";

export interface ExportSettings {
  path: string;
  width: number;
  height: number;
  fps: number;
  quality: Quality;
  /** Empty for the best encoder the machine has. */
  encoder: string;
  range: "whole" | "in_out";
  audio: "first" | "all";
}

export interface ExportPreset {
  id: string;
  label: string;
  hint: string;
  width: number;
  height: number;
  fps: number;
  quality: Quality;
}

export interface ExportOptions {
  presets: ExportPreset[];
  encoders: string[];
  /** A suggested output file. */
  path: string;
  has_range: boolean;
}

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
