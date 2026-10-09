// The AI chat's calls to the Rust side. The OpenRouter key never comes back to the window: only whether one is saved.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AiState {
  has_key: boolean;
  agent_model: string | null;
  whisper_model: string | null;
  /** The FFmpeg the app found: empty version when there is none. The frame, sound and export tools need it. */
  ffmpeg?: { version: string; supports_export: boolean; install: string };
}

export interface AiModel {
  id: string;
  name: string;
  context_length: number | null;
  /** Whether the model can call tools, which the chat needs. */
  tools: boolean;
  input: string[];
  output: string[];
  /** USD per token, as OpenRouter sends them. */
  prompt_price: string;
  completion_price: string;
}

/** A message in OpenAI chat format, as OpenRouter takes and returns them. */
export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string | null;
  tool_calls?: { id: string; type: string; function: { name: string; arguments: string } }[];
  tool_call_id?: string;
}

export interface ChatTurn {
  /** The model's messages and tool results from this turn, in order. */
  messages: ChatMessage[];
  /** Each tool call the model made, and whether it worked. */
  calls: { name: string; ok: boolean }[];
  /** Set when the turn ended before the model was done: the user stopped it, it hit the step limit, or a request failed. */
  stopped?: "cancelled" | "limit" | "failed";
}

/** What the chat is doing now: which step of the turn, and the tool it is running (null while the model thinks). */
export interface ChatProgress {
  round: number;
  tool: string | null;
}

/** One sound track of a media file. `channel_names` is empty when FFmpeg names a layout this app does not know. */
export interface AudioStream {
  index: number;
  layout: string;
  channels: number;
  channel_names: string[];
}

/** A line of speech, in seconds from the start of the media file. */
export interface SpeechCue {
  start: number;
  end: number;
  text: string;
}

/** The models the app starts with. Same ids as DEFAULT_CHAT_MODEL and DEFAULT_SUBTITLE_MODEL in src-tauri/src/ai.rs; change both. */
export const DEFAULT_MODELS = { agent_model: "~anthropic/claude-haiku-latest", whisper_model: "openai/whisper-large-v3" } as const;

/** An editing guide agents can read. `source` is "built-in" or the file it was read from. */
export interface GuideInfo {
  name: string;
  title: string;
  description: string;
  source: string;
}

/** The guides agents are offered, and the folder the user's own guides go in (null when it could not be made). */
export const guidesInfo = () => invoke<{ folder: string | null; guides: GuideInfo[] } | null>("guides_info");

export const aiState = () => invoke<AiState>("ai_state");
export const aiSetKey = (key: string) => invoke<void>("ai_set_key", { key });
export const aiClearKey = () => invoke<void>("ai_clear_key");
export const aiSetModel = (role: "agent" | "whisper", model: string | null) => invoke<void>("ai_set_model", { role, model });
export const aiModels = () => invoke<{ models: AiModel[] }>("ai_models");
export const aiChat = (messages: ChatMessage[]) => invoke<ChatTurn>("ai_chat", { messages });
/** Stops the chat after the step it is on. What it did stays on the timeline. */
export const aiCancel = () => invoke<void>("ai_cancel");
/** Calls `on` with each step of a running chat turn. Returns a function that stops listening. */
export const onChatProgress = (on: (p: ChatProgress) => void) => listen<ChatProgress>("ai:progress", (e) => on(e.payload));

/**
 * What is still missing before the chat can do its whole job, most important first. `blocking` items stop the chat
 * from working at all; the others only take tools away from it.
 */
export function setupGaps(state: AiState | null): { text: string; blocking: boolean }[] {
  if (!state) return [];
  const gaps: { text: string; blocking: boolean }[] = [];
  if (!state.has_key) gaps.push({ text: "Add your OpenRouter key in AI Settings. The chat cannot start without it.", blocking: true });
  if (!state.agent_model) gaps.push({ text: "Choose a chat model in AI Settings.", blocking: true });
  if (state.ffmpeg && !state.ffmpeg.version) {
    gaps.push({ text: `FFmpeg was not found, so the AI cannot look at frames, measure sound or export. Install it with: ${state.ffmpeg.install}`, blocking: false });
  } else if (state.ffmpeg && !state.ffmpeg.supports_export) {
    gaps.push({ text: `This FFmpeg (${state.ffmpeg.version}) is too old to export. Install a newer one with: ${state.ffmpeg.install}`, blocking: false });
  }
  if (!state.whisper_model) gaps.push({ text: "No subtitle model is chosen in AI Settings, so the AI cannot read what is said.", blocking: false });
  return gaps;
}
export const mediaAudioStreams = (media: string) => invoke<AudioStream[]>("media_audio_streams", { media });
/** Writes the lines of speech for `from` to `to` (seconds in the media) from one track, or one channel of it. Null channel mixes them. */
export const aiTranscribe = (media: string, track: number, channel: number | null, from: number, to: number) =>
  invoke<SpeechCue[]>("ai_transcribe", { media, track, channel, from, to });

/** Prices as dollars per million tokens, for the picker. */
export const perMillion = (perToken: string) => {
  const value = Number(perToken) * 1_000_000;
  return Number.isFinite(value) ? `$${value.toFixed(value < 1 ? 3 : 2)} per 1M tokens` : "price unknown";
};
