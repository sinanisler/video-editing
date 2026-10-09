import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { aiState, aiTranscribe, mediaAudioStreams } from "../ai";
import type { AiState, AudioStream } from "../ai";
import { addCaptions } from "../captions";
import { captionsForClip, clipSpeed } from "../subtitles";
import { core, project, toast, ui } from "../store";
import type { Clip, MediaItem } from "../types";
import { fpsOf } from "../util";

/** Seconds of sound in each part sent to the subtitle model. Matches PART_SECONDS in snn-core's speech.rs. */
const PART_SECONDS = 600;

/** What the app reports while it works: encoding the audio, then sending each part. */
interface Progress {
  stage: "encoding" | "sending";
  done: number;
  total: number;
}

/**
 * The Subtitles tab: writes subtitles from the sound of the selected clip, when the user presses Generate.
 * The sound is encoded first, then sent in parts. Only the part of the source the clip plays is used, and only its sound.
 */
export function SubtitlesTab() {
  const state = core.use((s) => s.state)!;
  const selection = ui.use((s) => s.selection);
  const p = state.project;
  if (selection.length !== 1) {
    return (
      <div className="empty">
        <b>Select one clip</b>
        <p>Subtitles are written from the sound of a single clip. Select a video or audio clip on the timeline.</p>
      </div>
    );
  }
  const clip = p.sequence.clips.find((c) => c.id === selection[0]);
  const media = p.media.find((m) => m.id === clip?.media);
  if (!clip || !media || (clip.kind !== "video" && clip.kind !== "audio")) {
    return (
      <div className="empty">
        <b>No sound to write from</b>
        <p>Subtitles come from a video or audio clip. The selected clip is a title or a caption.</p>
      </div>
    );
  }
  return <SubtitlesClip key={clip.id} clip={clip} media={media} fps={fpsOf(p)} />;
}

function SubtitlesClip({ clip, media, fps }: { clip: Clip; media: MediaItem; fps: number }) {
  const [ai, setAi] = useState<AiState | null>(null);
  const [streams, setStreams] = useState<AudioStream[] | null>(null);
  const [track, setTrack] = useState(0);
  /** Null mixes every channel of the track down to mono. */
  const [channel, setChannel] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    aiState().then(setAi, (e) => setError(String(e)));
    mediaAudioStreams(media.id).then(setStreams, (e) => setError(String(e)));
  }, [media.id]);

  const chosen = streams?.find((s) => s.index === track) ?? streams?.[0];
  const names = chosen?.channel_names ?? [];
  // The part of the source the clip plays: from its in point, for its length at its speed.
  const speed = clipSpeed(clip);
  const from = clip.source_in / fps;
  const to = (clip.source_in + clip.duration * speed) / fps;
  const parts = Math.max(1, Math.ceil((to - from) / PART_SECONDS));
  const ready = !!ai?.has_key && !!ai.whisper_model && !!chosen && !busy;

  /** The clip still sits where it did when the request went out, so the subtitles line up with it. */
  const unchangedSince = () => {
    const now = project()?.sequence.clips.find((c) => c.id === clip.id);
    return !!now && now.start === clip.start && now.duration === clip.duration && now.source_in === clip.source_in && now.media === clip.media && clipSpeed(now) === speed;
  };

  const generate = async () => {
    if (!chosen) return;
    setBusy(true);
    setError("");
    setProgress(null);
    let stop: (() => void) | undefined;
    try {
      stop = await listen<Progress>("subtitles:progress", (e) => setProgress(e.payload));
      const lines = await aiTranscribe(media.id, chosen.index, channel, from, to);
      if (!unchangedSince()) {
        setError("The clip moved or changed while its subtitles were written, so none were added. Generate them again.");
        return;
      }
      const cues = captionsForClip(clip, lines, fps);
      if (!cues.length) {
        toast("No speech found in this clip");
        return;
      }
      const result = await addCaptions(cues);
      if (result) toast(`Added ${result.added} subtitles${result.skipped ? `, skipped ${result.skipped} that overlap` : ""}`);
    } catch (e) {
      setError(String(e));
    } finally {
      stop?.();
      setBusy(false);
      setProgress(null);
    }
  };

  return (
    <div className="insp">
      <h4>Subtitles</h4>
      <div className="row">
        <span className="lbl">Clip</span>
        <span className="dim">{clip.name}</span>
      </div>

      {!streams && !error && <p className="hint">Reading the sound tracks…</p>}
      {streams?.length === 0 && <p className="hint err">This file has no sound track, so there is nothing to write subtitles from.</p>}

      {streams && streams.length > 1 && (
        <div className="row">
          <span className="lbl">Sound track</span>
          <select
            className="span"
            data-tip="Which sound track of this file the subtitles are written from"
            value={chosen?.index ?? 0}
            onChange={(e) => {
              setTrack(+e.target.value);
              setChannel(null);
            }}
          >
            {streams.map((s) => (
              <option key={s.index} value={s.index}>
                {`Track ${s.index + 1}${s.layout ? ` · ${s.layout}` : ""}`}
              </option>
            ))}
          </select>
        </div>
      )}

      {names.length > 1 && (
        <div className="row">
          <span className="lbl">Channel</span>
          <select
            className="span"
            data-tip="Which channel of the sound track the subtitles are written from. Mixed uses all of them."
            value={channel ?? ""}
            onChange={(e) => setChannel(e.target.value === "" ? null : +e.target.value)}
          >
            <option value="">All channels mixed</option>
            {names.map((name, i) => (
              <option key={i} value={i}>
                {name}
              </option>
            ))}
          </select>
        </div>
      )}

      {chosen && (
        <p className="hint">
          Encodes about {((to - from) / 60).toFixed(1)} min of this sound as {parts} small mono MP3 {parts === 1 ? "part" : "parts"} of up to{" "}
          {PART_SECONDS / 60} minutes, then sends {parts === 1 ? "it" : "them"} to {ai?.whisper_model ?? "the subtitle model"}. The picture is not sent.
          The model is set in AI Settings.
        </p>
      )}
      {ai && !ai.has_key && <p className="hint err">Add your OpenRouter key in AI Settings first.</p>}

      <div className="line">
        <button
          className="primary"
          disabled={!ready}
          data-tip="Encode this sound, send it to the subtitle model, and add the subtitles to the Captions track. One undo step."
          onClick={() => void generate()}
        >
          {busy ? "Making subtitles…" : "Generate subtitles"}
        </button>
      </div>
      {busy && <p className="hint">{progressText(progress)}</p>}
      {error && <p className="hint err">{error}</p>}
      <p className="hint note">
        Subtitles go on the first unlocked Captions track as one undo step, split by that track's settings. Click its header to change them. Nothing is
        sent until you press Generate.
      </p>
    </div>
  );
}

/** The line under the button while the subtitles are made. */
function progressText(progress: Progress | null): string {
  if (!progress) return "Preparing the audio…";
  if (progress.stage === "encoding") return `Encoding audio, part ${progress.done} of ${progress.total}`;
  return `Sending part ${progress.done} of ${progress.total} to the model`;
}
