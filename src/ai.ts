// The AI chat's calls to the Rust side. The OpenRouter key never comes back to the window: only whether one is saved.

import { invoke } from "@tauri-apps/api/core";

export interface AiState {
  has_key: boolean;
  agent_model: string | null;
  whisper_model: string | null;
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

export const aiState = () => invoke<AiState>("ai_state");
export const aiSetKey = (key: string) => invoke<void>("ai_set_key", { key });
export const aiClearKey = () => invoke<void>("ai_clear_key");
export const aiSetModel = (role: "agent" | "whisper", model: string | null) => invoke<void>("ai_set_model", { role, model });
export const aiModels = () => invoke<{ models: AiModel[] }>("ai_models");
export const aiChat = (messages: ChatMessage[]) => invoke<ChatTurn>("ai_chat", { messages });
export const mediaAudioStreams = (media: string) => invoke<AudioStream[]>("media_audio_streams", { media });
/** Writes the lines of speech for `from` to `to` (seconds in the media) from one track, or one channel of it. Null channel mixes them. */
export const aiTranscribe = (media: string, track: number, channel: number | null, from: number, to: number) =>
  invoke<SpeechCue[]>("ai_transcribe", { media, track, channel, from, to });

/** Prices as dollars per million tokens, for the picker. */
export const perMillion = (perToken: string) => {
  const value = Number(perToken) * 1_000_000;
  return Number.isFinite(value) ? `$${value.toFixed(value < 1 ? 3 : 2)} per 1M tokens` : "price unknown";
};
