//! Sound for speech recognition: which sound tracks and channels a file has, a small mono MP3 of one channel of one
//! stretch of it, and the lines a transcription service returns. Only the sound is decoded, so the picture is never read
//! and the file is never sent whole.

use crate::analysis::{failure, run};
use serde::Serialize;
use serde_json::Value;

/// How long each part of the sound is, in seconds. Upstream providers time out after 60 seconds per request.
pub const CHUNK_SECONDS: f64 = 60.0;

/// One sound track of a file, as FFmpeg lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioStream {
    /// Counts sound tracks from 0, the way `-map 0:a:N` does.
    pub index: usize,
    /// FFmpeg's name for the channel layout, such as "stereo" or "5.1".
    pub layout: String,
    /// How many channels the track has. 0 when the layout is one this does not know.
    pub channels: usize,
    /// What each channel is called, in order. Empty when the count is unknown.
    pub channel_names: Vec<String>,
}

/// One line of speech and the seconds it is heard, from the start of the file.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpeechCue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// The sound tracks of a file. A file with no sound returns an empty list.
pub fn audio_streams(path: &str) -> Result<Vec<AudioStream>, String> {
    // Naming no output makes FFmpeg list the file's streams and stop, with an error that is expected here.
    let out = run(vec!["-i".into(), path.into()])?;
    let log = String::from_utf8_lossy(&out.stderr);
    if !log.contains("Input #0") {
        return Err(failure(&out));
    }
    Ok(parse_audio_streams(&log))
}

/// Reads the audio lines of FFmpeg's stream list, such as `Stream #0:1(und): Audio: aac, 48000 Hz, stereo, fltp`.
fn parse_audio_streams(log: &str) -> Vec<AudioStream> {
    log.lines()
        .filter(|line| line.trim_start().starts_with("Stream #0:") && line.contains("Audio:"))
        .enumerate()
        .map(|(index, line)| {
            let details = line.split_once("Audio:").map_or("", |(_, rest)| rest);
            let fields: Vec<&str> = details.split(',').map(str::trim).collect();
            // The layout is the field after the sample rate.
            let layout = fields.iter().position(|field| field.ends_with(" Hz")).and_then(|i| fields.get(i + 1)).copied().unwrap_or_default().to_string();
            let channels = channel_count(&layout);
            AudioStream { index, layout, channels, channel_names: channel_names(channels) }
        })
        .collect()
}

/// How many channels a layout has. Layouts FFmpeg names as "N channels" give N.
fn channel_count(layout: &str) -> usize {
    match layout {
        "mono" => 1,
        "stereo" => 2,
        "2.1" | "3.0" | "3.0(front)" | "3.0(back)" => 3,
        "3.1" | "4.0" | "quad" | "quad(side)" => 4,
        "4.1" | "5.0" | "5.0(side)" => 5,
        "5.1" | "5.1(side)" | "6.0" | "6.0(front)" | "hexagonal" => 6,
        "6.1" | "6.1(back)" | "6.1(front)" | "7.0" | "7.0(front)" => 7,
        "7.1" | "7.1(wide)" | "7.1(wide-side)" | "octagonal" => 8,
        other => other.strip_suffix(" channels").and_then(|n| n.parse().ok()).unwrap_or(0),
    }
}

/// The names the picker shows for each channel. Stereo gets left and right; other layouts are numbered.
fn channel_names(channels: usize) -> Vec<String> {
    match channels {
        0 => vec![],
        1 => vec!["Mono".into()],
        2 => vec!["Left".into(), "Right".into()],
        n => (1..=n).map(|i| format!("Channel {i}")).collect(),
    }
}

/// The parts the sound from `from` to `to` (seconds) is cut into: (start, length) pairs of at most [`CHUNK_SECONDS`].
pub fn chunk_spans(from: f64, to: f64) -> Vec<(f64, f64)> {
    let mut spans = vec![];
    let mut start = from;
    while start < to {
        let length = (to - start).min(CHUNK_SECONDS);
        spans.push((start, length));
        start += length;
    }
    spans
}

/// A mono MP3 of one stretch of one sound track: 16 kHz and 32 kbit/s, enough for speech and small enough to send.
/// `channel` picks one channel of the track; None mixes all of them down to mono. The picture is not decoded.
pub fn speech_audio(path: &str, start: f64, length: f64, track: usize, channel: Option<usize>) -> Result<Vec<u8>, String> {
    let mut args: Vec<String> = vec![
        "-ss".into(),
        format!("{start:.3}"),
        "-t".into(),
        format!("{length:.3}"),
        "-i".into(),
        path.into(),
        "-map".into(),
        format!("0:a:{track}"),
        "-vn".into(),
    ];
    if let Some(channel) = channel {
        args.extend(["-af".into(), format!("pan=mono|c0=c{channel}")]);
    }
    args.extend(["-ac", "1", "-ar", "16000", "-c:a", "libmp3lame", "-b:a", "32k", "-f", "mp3", "pipe:1"].map(String::from));
    let out = run(args)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(failure(&out));
    }
    Ok(out.stdout)
}

/// Reads a `verbose_json` transcript. `offset` is where the audio it came from starts in the file, so the times line up.
pub fn parse_speech(answer: &Value, offset: f64) -> Result<Vec<SpeechCue>, String> {
    let segments = answer["segments"]
        .as_array()
        .ok_or("the subtitle model sent no timings, so no captions can be placed. Choose another subtitle model in AI Settings")?;
    Ok(segments
        .iter()
        .filter_map(|segment| {
            let text = segment["text"].as_str()?.trim();
            let (start, end) = (segment["start"].as_f64()?, segment["end"].as_f64()?);
            (!text.is_empty() && end > start).then(|| SpeechCue { start: offset + start, end: offset + end, text: text.to_string() })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    const LOG: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'clip.mp4':
  Duration: 00:00:03.00, start: 0.000000, bitrate: 200 kb/s
  Stream #0:0[0x1](und): Video: h264 (High), yuv420p, 1920x1080, 30 fps
  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)
    Metadata:
  Stream #0:2: Audio: aac (LC), 48000 Hz, mono, fltp, 64 kb/s
  Stream #0:3: Audio: pcm_s16le ([1][0][0][0] / 0x0001), 48000 Hz, 2 channels, s16, 1536 kb/s
  Stream #0:4: Audio: opus, 48000 Hz, 5.1, fltp
  Stream #0:5: Audio: mystery, 44100 Hz, some-new-layout, s16";

    #[test]
    fn reads_each_sound_track_and_its_channels() {
        let streams = parse_audio_streams(LOG);
        let summary: Vec<(usize, &str, usize)> = streams.iter().map(|s| (s.index, s.layout.as_str(), s.channels)).collect();
        assert_eq!(summary, vec![(0, "stereo", 2), (1, "mono", 1), (2, "2 channels", 2), (3, "5.1", 6), (4, "some-new-layout", 0)]);
        assert_eq!(streams[0].channel_names, ["Left", "Right"]);
        assert_eq!(streams[1].channel_names, ["Mono"]);
        assert_eq!(streams[3].channel_names.len(), 6);
        assert!(streams[4].channel_names.is_empty(), "an unknown layout offers no channel choice");
    }

    #[test]
    fn a_file_without_sound_has_no_tracks() {
        assert!(parse_audio_streams("Input #0, mov,mp4 from 'silent.mp4':\n  Stream #0:0: Video: h264, 1920x1080").is_empty());
    }

    #[test]
    fn the_sound_is_cut_into_parts_of_at_most_a_minute() {
        assert_eq!(chunk_spans(0.0, 150.0), vec![(0.0, 60.0), (60.0, 60.0), (120.0, 30.0)]);
        assert_eq!(chunk_spans(10.0, 20.0), vec![(10.0, 10.0)]);
        assert!(chunk_spans(5.0, 5.0).is_empty());
    }

    #[test]
    fn each_line_keeps_its_time_in_the_file() {
        let answer = json!({
            "text": "hello there general kenobi",
            "segments": [
                { "id": 0, "start": 0.5, "end": 2.0, "text": " Hello there. " },
                { "id": 1, "start": 2.5, "end": 2.5, "text": "empty span" },
                { "id": 2, "start": 3.0, "end": 4.25, "text": "" },
                { "id": 3, "start": 4.5, "end": 6.0, "text": "General Kenobi" }
            ]
        });
        let cues = parse_speech(&answer, 60.0).unwrap();
        assert_eq!(
            cues,
            vec![
                SpeechCue { start: 60.5, end: 62.0, text: "Hello there.".into() },
                SpeechCue { start: 64.5, end: 66.0, text: "General Kenobi".into() },
            ]
        );
    }

    #[test]
    fn a_reply_without_timings_is_an_error_not_silent_captions() {
        let err = parse_speech(&json!({ "text": "hello" }), 0.0).unwrap_err();
        assert!(err.contains("no timings"), "{err}");
    }

    fn ffmpeg_works() -> bool {
        run(vec!["-version".into()]).is_ok_and(|out| out.status.success())
    }

    /// The mean loudness of an MP3 in dB, as FFmpeg's volumedetect measures it.
    fn mean_db(mp3: &[u8]) -> f64 {
        let path = std::env::temp_dir().join(format!("snn-speech-check-{}.mp3", std::process::id()));
        std::fs::write(&path, mp3).unwrap();
        let out = run(vec!["-i".into(), path.display().to_string(), "-af".into(), "volumedetect".into(), "-f".into(), "null".into(), "-".into()]).unwrap();
        std::fs::remove_file(&path).ok();
        let log = String::from_utf8_lossy(&out.stderr).into_owned();
        let text = log.lines().find_map(|l| l.split("mean_volume:").nth(1)).map(|rest| rest.trim().trim_end_matches(" dB").to_string());
        // Digital silence is reported as -inf.
        text.expect("volumedetect reports a mean").parse().unwrap_or(f64::NEG_INFINITY)
    }

    #[test]
    fn one_channel_of_one_track_is_what_gets_extracted() {
        if !ffmpeg_works() {
            eprintln!("skipped: FFmpeg is not installed");
            return;
        }
        // Track 0 is stereo with a tone on the right only. Track 1 is mono with a second tone.
        let path: PathBuf = std::env::temp_dir().join(format!("snn-speech-{}.mp4", std::process::id()));
        let made = run(
            [
                "-y", "-f", "lavfi", "-i", "sine=frequency=880:duration=3", "-f", "lavfi", "-i", "sine=frequency=220:duration=3",
                "-filter_complex", "[0:a]pan=stereo|c0=0*c0|c1=c0[left]", "-map", "[left]", "-map", "1:a", "-c:a", "aac",
                &path.display().to_string(),
            ]
            .map(String::from)
            .into(),
        )
        .unwrap();
        assert!(made.status.success(), "{}", failure(&made));
        let file = path.display().to_string();
        let streams = audio_streams(&file).unwrap();
        assert_eq!(streams.iter().map(|s| s.channels).collect::<Vec<_>>(), vec![2, 1]);

        let left = mean_db(&speech_audio(&file, 0.0, 2.0, 0, Some(0)).unwrap());
        let right = mean_db(&speech_audio(&file, 0.0, 2.0, 0, Some(1)).unwrap());
        let mixed = mean_db(&speech_audio(&file, 0.0, 2.0, 0, None).unwrap());
        let second_track = mean_db(&speech_audio(&file, 0.0, 2.0, 1, None).unwrap());
        std::fs::remove_file(&path).ok();
        assert!(left < -60.0, "the left channel is silent, measured {left} dB");
        assert!(right > -30.0, "the right channel carries the tone, measured {right} dB");
        assert!(left < mixed && mixed < right, "a mix sits between the two, measured {mixed} dB");
        assert!(second_track > -30.0, "track 1 is the 220 Hz tone, measured {second_track} dB");
    }
}
