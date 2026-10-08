use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Timeline time, counted in frames of the sequence frame rate. Never a float.
pub type Frame = i64;

pub const PROJECT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl Settings {
    /// Frames in `seconds`, rounded to the nearest frame.
    pub fn frames(&self, seconds: f64) -> Frame {
        (seconds * self.fps_num as f64 / self.fps_den as f64).round() as Frame
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
    Caption,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Video,
    Graphic,
    Caption,
    Audio,
}

impl ClipKind {
    pub fn fits(self, track: TrackKind) -> bool {
        matches!(
            (self, track),
            (ClipKind::Video | ClipKind::Graphic, TrackKind::Video)
                | (ClipKind::Audio, TrackKind::Audio)
                | (ClipKind::Caption, TrackKind::Caption)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub duration: Frame,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub track: String,
    pub start: Frame,
    pub duration: Frame,
    /// First frame of the source media that this clip shows.
    #[serde(default)]
    pub source_in: Frame,
    pub name: String,
    pub kind: ClipKind,
    #[serde(default)]
    pub media: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// Graphic template id, for example `title` or `subscribe`.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub props: BTreeMap<String, Value>,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub transition_in: Option<Transition>,
    #[serde(default)]
    pub transition_out: Option<Transition>,
    /// Who changed this clip last: `user` or an agent name.
    #[serde(default)]
    pub last_actor: Option<String>,
}

impl Clip {
    pub fn end(&self) -> Frame {
        self.start + self.duration
    }

    /// Playback speed as a factor: 1.0 is normal, 2.0 uses source frames twice as fast.
    pub fn speed(&self) -> f64 {
        self.props.get("speed").and_then(Value::as_f64).unwrap_or(100.0) / 100.0
    }

    /// True when `at` falls strictly inside the clip, so a split there leaves two non-empty parts.
    pub fn contains_inner(&self, at: Frame) -> bool {
        at > self.start && at < self.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub id: String,
    pub at: Frame,
    pub label: String,
    #[serde(default)]
    pub actor: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: MediaKind,
    /// Length in sequence frames. Zero for images, which can be any length on the timeline.
    pub duration: Frame,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Whether the file has a sound track. Older projects were saved without it, so it reads as false.
    #[serde(default)]
    pub has_audio: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    /// Top of the timeline first.
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub in_point: Option<Frame>,
    #[serde(default)]
    pub out_point: Option<Frame>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub settings: Settings,
    pub media: Vec<MediaItem>,
    pub sequence: Sequence,
    pub next_id: u64,
}

fn track(id: &str, name: &str, kind: TrackKind) -> Track {
    Track { id: id.into(), name: name.into(), kind, locked: false, muted: false, hidden: false }
}

impl Project {
    pub fn new(name: &str) -> Self {
        Project {
            version: PROJECT_VERSION,
            name: name.into(),
            settings: Settings { width: 1920, height: 1080, fps_num: 30, fps_den: 1 },
            media: vec![],
            sequence: Sequence {
                name: "Sequence 01".into(),
                tracks: vec![
                    track("T1", "Captions", TrackKind::Caption),
                    track("V3", "Video 3", TrackKind::Video),
                    track("V2", "Video 2", TrackKind::Video),
                    track("V1", "Video 1", TrackKind::Video),
                    track("A1", "Audio 1", TrackKind::Audio),
                    track("A2", "Audio 2", TrackKind::Audio),
                    track("A3", "Audio 3", TrackKind::Audio),
                ],
                clips: vec![],
                markers: vec![],
                in_point: None,
                out_point: None,
            },
            next_id: 1,
        }
    }

    pub fn new_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}{}", self.next_id);
        self.next_id += 1;
        id
    }

    pub fn track(&self, id: &str) -> Option<&Track> {
        self.sequence.tracks.iter().find(|t| t.id == id)
    }

    pub fn media(&self, id: &str) -> Option<&MediaItem> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn clip(&self, id: &str) -> Option<&Clip> {
        self.sequence.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: &str) -> Option<&mut Clip> {
        self.sequence.clips.iter_mut().find(|c| c.id == id)
    }

    /// End of the last clip.
    pub fn duration(&self) -> Frame {
        self.sequence.clips.iter().map(Clip::end).max().unwrap_or(0)
    }
}
