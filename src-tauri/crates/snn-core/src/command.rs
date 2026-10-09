use crate::export::rescale_frames;
use crate::library::{effect_names, is_effect, is_transition, transition_names, validate_caption_track_property, validate_property};
use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Every edit the editor supports. The serialized form is `{"tool": "...", "args": {...}}`,
/// and the tool names are the MCP tool names.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args")]
pub enum Command {
    #[serde(rename = "project.new")]
    NewProject { name: String },

    /// Changes the picture size or the frame rate. A new frame rate keeps every clip at the same
    /// moment in time, so frame numbers change.
    #[serde(rename = "project.set_settings")]
    SetSettings {
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        fps: Option<f64>,
    },

    /// Registers a file in the media pool. Importing the same path twice returns the existing item.
    #[serde(rename = "media.import")]
    ImportMedia {
        path: String,
        #[serde(default)]
        name: Option<String>,
        kind: MediaKind,
        duration: Frame,
        #[serde(default)]
        width: u32,
        #[serde(default)]
        height: u32,
        #[serde(default)]
        has_audio: bool,
    },
    /// Removes an item from the pool. Fails while clips still use it.
    #[serde(rename = "media.remove")]
    RemoveMedia { media: String },

    #[serde(rename = "timeline.add_clip")]
    AddClip {
        track: String,
        start: Frame,
        duration: Frame,
        name: String,
        kind: ClipKind,
        #[serde(default)]
        media: Option<String>,
        /// First source frame to show, for clips backed by media.
        #[serde(default)]
        source_in: Frame,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        template: Option<String>,
        /// Clip to link the new clip with. The two then select and move together in time.
        #[serde(default)]
        link_to: Option<String>,
    },
    #[serde(rename = "timeline.move_clip")]
    MoveClip {
        clip: String,
        start: Frame,
        #[serde(default)]
        track: Option<String>,
    },
    /// Shifts several clips by the same number of frames. `tracks` moves some of them to another track too, and
    /// the rest keep theirs. All move or none does.
    #[serde(rename = "timeline.move_clips")]
    MoveClips {
        clips: Vec<String>,
        delta: Frame,
        #[serde(default)]
        tracks: BTreeMap<String, String>,
        /// Also shifts the markers that sit over the moved clips, from the first one's start to the last one's end.
        #[serde(default)]
        markers: bool,
    },
    /// Copies clips, with their properties and effects, `delta` frames later (or earlier) on the same tracks.
    /// Copies of linked clips are linked to each other, not to the originals.
    #[serde(rename = "timeline.duplicate_clips")]
    DuplicateClips { clips: Vec<String>, delta: Frame },
    /// Links the clips into one group, replacing any links they had. Needs at least two.
    #[serde(rename = "timeline.link")]
    Link { clips: Vec<String> },
    /// Breaks the links of every clip in the groups the given clips belong to.
    #[serde(rename = "timeline.unlink")]
    Unlink { clips: Vec<String> },
    #[serde(rename = "timeline.trim_clip")]
    TrimClip { clip: String, start: Frame, duration: Frame },
    /// Splits the given clips at `at`, or every clip under `at` on unlocked tracks when `clips` is omitted.
    #[serde(rename = "timeline.split_clip")]
    SplitClip {
        at: Frame,
        #[serde(default)]
        clips: Option<Vec<String>>,
    },
    #[serde(rename = "timeline.delete_clip")]
    DeleteClip { clip: String },
    /// Deletes the clip and closes the gap on its track.
    #[serde(rename = "timeline.ripple_delete_clip")]
    RippleDeleteClip { clip: String },
    /// Removes `[from, to)` from every unlocked track and shifts everything after it left.
    #[serde(rename = "timeline.ripple_delete_range")]
    RippleDeleteRange { from: Frame, to: Frame },
    /// Removes several spans in one step. Every span is given in the timeline as it is before the call, in any
    /// order; overlapping spans are joined. With `media`, the spans are source frames of that file instead, and
    /// whatever part of them is on the timeline is removed, wherever the cuts so far have moved it.
    #[serde(rename = "timeline.ripple_delete_ranges")]
    RippleDeleteRanges {
        ranges: Vec<Span>,
        #[serde(default)]
        media: Option<String>,
    },
    #[serde(rename = "timeline.add_marker")]
    AddMarker { at: Frame, label: String },
    #[serde(rename = "timeline.remove_marker")]
    RemoveMarker { marker: String },
    #[serde(rename = "timeline.set_range")]
    SetRange {
        #[serde(default)]
        in_point: Option<Frame>,
        #[serde(default)]
        out_point: Option<Frame>,
    },
    /// Adds a track. Video tracks go on top of the video stack, audio tracks at the bottom.
    #[serde(rename = "timeline.add_track")]
    AddTrack {
        kind: TrackKind,
        #[serde(default)]
        name: Option<String>,
    },
    /// Removes a track that has no clips on it.
    #[serde(rename = "timeline.remove_track")]
    RemoveTrack { track: String },
    #[serde(rename = "timeline.rename_track")]
    RenameTrack { track: String, name: String },
    #[serde(rename = "timeline.set_track_state")]
    SetTrackState {
        track: String,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        muted: Option<bool>,
        #[serde(default)]
        hidden: Option<bool>,
    },
    /// Sets one subtitle setting of a captions track, such as `chunk_words` or `color`. The keys are the
    /// `caption_track_keys` of `library_list`. Null takes the setting away, which brings the default back.
    #[serde(rename = "timeline.set_track_property")]
    SetTrackProperty { track: String, key: String, value: Value },

    /// `name` and `text` set those fields; any other key goes into the clip's property bag. A null value removes the key.
    #[serde(rename = "clip.set_property")]
    SetProperty { clip: String, key: String, value: Value },
    #[serde(rename = "clip.add_effect")]
    AddEffect { clip: String, effect: String },
    #[serde(rename = "clip.remove_effect")]
    RemoveEffect { clip: String, effect: String },
    /// Sets the transition at the clip's start. Omit `transition` to go back to a plain cut.
    #[serde(rename = "clip.set_transition")]
    SetTransition {
        clip: String,
        #[serde(default)]
        transition: Option<String>,
        #[serde(default)]
        duration: Option<Frame>,
        /// Which end of the clip: `in` (the default) or `out`.
        #[serde(default)]
        edge: Edge,
    },
}

/// Frames `from` up to, but not including, `to`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub from: Frame,
    pub to: Frame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    #[default]
    In,
    Out,
}

impl Command {
    pub fn tool(&self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v["tool"].as_str().map(String::from)).unwrap_or_default()
    }

    pub fn args(&self) -> Value {
        serde_json::to_value(self).map(|v| v["args"].clone()).unwrap_or(Value::Null)
    }
}

type R<T> = Result<T, String>;

/// About 92 hours at 30 fps. Guards against typos and runaway agent arguments.
const MAX_TIMELINE_FRAMES: Frame = 10_000_000;
const MAX_TRACKS_PER_KIND: usize = 99;

fn unlocked<'a>(p: &'a Project, track: &str) -> R<&'a Track> {
    let t = p.track(track).ok_or_else(|| format!("no track {track}"))?;
    if t.locked {
        return Err(format!("track {track} is locked"));
    }
    Ok(t)
}

/// The clip, after checking that it exists and its track is unlocked.
fn editable(p: &Project, clip: &str) -> R<Clip> {
    let c = p.clip(clip).ok_or_else(|| format!("no clip {clip}"))?;
    unlocked(p, &c.track)?;
    Ok(c.clone())
}

fn check_free(p: &Project, track: &str, start: Frame, duration: Frame, ignore: &str) -> R<()> {
    for c in &p.sequence.clips {
        if c.track == track && c.id != ignore && start < c.end() && c.start < start + duration {
            return Err(format!("would overlap clip {} ({}) on track {track}", c.id, c.name));
        }
    }
    Ok(())
}

fn check_span(start: Frame, duration: Frame) -> R<()> {
    if start < 0 {
        return Err("start cannot be before frame 0".into());
    }
    if duration < 1 {
        return Err("duration must be at least 1 frame".into());
    }
    if start + duration > MAX_TIMELINE_FRAMES {
        return Err(format!("the timeline cannot extend past frame {MAX_TIMELINE_FRAMES}"));
    }
    Ok(())
}

/// Checks that a clip backed by `media` stays inside the source and matches its kind.
fn check_source(p: &Project, media: &Option<String>, kind: ClipKind, source_in: Frame, duration: Frame, speed: f64) -> R<()> {
    let Some(id) = media else { return Ok(()) };
    let m = p.media(id).ok_or_else(|| format!("no media {id}"))?;
    // An audio clip may take its sound from a video file, which is how an imported video's sound gets its own track.
    if kind == ClipKind::Audio && m.kind == MediaKind::Video {
        if !m.has_audio {
            return Err(format!("{} has no sound track", m.name).to_lowercase());
        }
    } else if (m.kind == MediaKind::Audio) != (kind == ClipKind::Audio) {
        return Err(format!("{} is {:?} media and cannot back a {kind:?} clip", m.name, m.kind).to_lowercase());
    }
    if source_in < 0 {
        return Err("source_in cannot be negative".into());
    }
    // A faster clip uses up more of its source per timeline frame.
    let used = (duration as f64 * speed).ceil() as Frame;
    if m.kind != MediaKind::Image && source_in + used > m.duration {
        return Err(format!("clip would run past the end of {} ({} frames long)", m.name, m.duration));
    }
    Ok(())
}

/// The link for a copy or a split part of a clip in group `old`. Every part of the same group gets the same new
/// link, so the parts stay together and are not joined to the originals.
fn fresh_link(p: &mut Project, map: &mut BTreeMap<String, String>, old: &str) -> String {
    if let Some(link) = map.get(old) {
        return link.clone();
    }
    let link = p.new_id("L");
    map.insert(old.to_string(), link.clone());
    link
}

/// The clip's link, creating one if it has none, so another clip can join it.
fn link_of(p: &mut Project, clip: &str) -> String {
    if let Some(link) = p.clip(clip).and_then(|c| c.link.clone()) {
        return link;
    }
    let link = p.new_id("L");
    p.clip_mut(clip).expect("caller checked the clip exists").link = Some(link.clone());
    link
}

/// Splits the clip at `at` and returns the id of the new right-hand part.
fn split_at(p: &mut Project, clip: &str, at: Frame, actor: &str, links: &mut BTreeMap<String, String>) -> String {
    let link = p.clip(clip).and_then(|c| c.link.clone()).map(|old| fresh_link(p, links, &old));
    let id = p.new_id("c");
    let left = p.clip_mut(clip).expect("caller checked the clip exists");
    let mut right = left.clone();
    right.id = id.clone();
    right.link = link;
    right.start = at;
    right.duration = left.end() - at;
    right.source_in += ((at - left.start) as f64 * left.speed()).round() as Frame;
    right.transition_in = None;
    left.transition_out = None;
    right.last_actor = Some(actor.into());
    left.duration = at - left.start;
    p.sequence.clips.push(right);
    id
}

/// Removes `[from, to)` from every unlocked track and closes the gap. Returns how many clips went and how many
/// tracks were locked and so left alone.
fn ripple_delete(p: &mut Project, from: Frame, to: Frame, actor: &str) -> (usize, usize) {
    let len = to - from;
    let open: Vec<String> = p.sequence.tracks.iter().filter(|t| !t.locked).map(|t| t.id.clone()).collect();
    let locked = p.sequence.tracks.len() - open.len();
    for edge in [from, to] {
        let hit: Vec<String> = p.sequence.clips.iter().filter(|c| open.contains(&c.track) && c.contains_inner(edge)).map(|c| c.id.clone()).collect();
        let mut links = BTreeMap::new();
        for id in hit {
            split_at(p, &id, edge, actor, &mut links);
        }
    }
    let before = p.sequence.clips.len();
    p.sequence.clips.retain(|c| !(open.contains(&c.track) && c.start >= from && c.end() <= to));
    let removed = before - p.sequence.clips.len();
    for c in p.sequence.clips.iter_mut().filter(|c| open.contains(&c.track) && c.start >= to) {
        c.start -= len;
    }
    p.sequence.markers.retain(|m| m.at < from || m.at >= to);
    for m in p.sequence.markers.iter_mut().filter(|m| m.at >= to) {
        m.at -= len;
    }
    p.sequence.in_point = None;
    p.sequence.out_point = None;
    (removed, locked)
}

/// Where the source frames `[from, to)` of a file are on the timeline now: one span for each clip that shows part
/// of them, on any track. A clip played faster covers its source in fewer timeline frames.
pub(crate) fn source_on_timeline(p: &Project, media: &str, from: Frame, to: Frame) -> Vec<(String, Span)> {
    let mut found = vec![];
    for c in p.sequence.clips.iter().filter(|c| c.media.as_deref() == Some(media)) {
        let speed = c.speed();
        let source_end = c.source_in as f64 + c.duration as f64 * speed;
        let (a, b) = ((from as f64).max(c.source_in as f64), (to as f64).min(source_end));
        if b <= a {
            continue;
        }
        let at = |s: f64| c.start + ((s - c.source_in as f64) / speed).round() as Frame;
        let span = Span { from: at(a).clamp(c.start, c.end()), to: at(b).clamp(c.start, c.end()) };
        if span.to > span.from {
            found.push((c.id.clone(), span));
        }
    }
    found.sort_by_key(|(_, s)| (s.from, s.to));
    found
}

/// Sorts spans and joins the ones that overlap or touch.
fn joined(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|s| (s.from, s.to));
    let mut out: Vec<Span> = vec![];
    for s in spans {
        match out.last_mut() {
            Some(last) if s.from <= last.to => last.to = last.to.max(s.to),
            _ => out.push(s),
        }
    }
    out
}

/// Applies one command. On error the project may be half-changed; [`crate::Editor`] restores it.
pub fn apply(p: &mut Project, cmd: &Command, actor: &str) -> R<Value> {
    match cmd {
        Command::NewProject { name } => {
            *p = Project::new(name);
            Ok(json!({ "project": name }))
        }

        Command::SetSettings { width, height, fps } => {
            let (w, h) = (width.unwrap_or(p.settings.width), height.unwrap_or(p.settings.height));
            if w < 16 || h < 16 || w > 7680 || h > 7680 || w % 2 != 0 || h % 2 != 0 {
                return Err("width and height must be even numbers between 16 and 7680".into());
            }
            if let Some(fps) = fps {
                if !(1.0..=240.0).contains(fps) {
                    return Err("the frame rate must be between 1 and 240".into());
                }
                // 23.976, 29.97 and 59.94 are really 24000, 30000 and 60000 over 1001.
                let whole = fps.round();
                let (num, den) = if (fps - whole).abs() < 0.001 {
                    (whole as u32, 1)
                } else if (fps - (fps * 1.001).round() / 1.001).abs() < 0.01 {
                    ((fps * 1.001).round() as u32 * 1000, 1001)
                } else {
                    ((fps * 1000.0).round() as u32, 1000)
                };
                if (num, den) != (p.settings.fps_num, p.settings.fps_den) {
                    rescale_frames(p, num, den);
                }
            }
            p.settings.width = w;
            p.settings.height = h;
            let s = &p.settings;
            Ok(json!({ "width": s.width, "height": s.height, "fps": s.fps_num as f64 / s.fps_den as f64, "duration_frames": p.duration() }))
        }

        Command::ImportMedia { path, name, kind, duration, width, height, has_audio } => {
            if let Some(existing) = p.media.iter().find(|m| &m.path == path) {
                return Ok(json!({ "media": existing, "already_imported": true }));
            }
            if *kind != MediaKind::Image && *duration < 1 {
                return Err("media must be at least 1 frame long".into());
            }
            let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
            let item = MediaItem {
                id: p.new_id("media"),
                name: name.clone().unwrap_or(file_name),
                path: path.clone(),
                kind: *kind,
                duration: *duration,
                width: *width,
                height: *height,
                has_audio: *has_audio,
            };
            p.media.push(item.clone());
            Ok(json!({ "media": item }))
        }
        Command::RemoveMedia { media } => {
            p.media(media).ok_or_else(|| format!("no media {media}"))?;
            let users = p.sequence.clips.iter().filter(|c| c.media.as_ref() == Some(media)).count();
            if users > 0 {
                return Err(format!("{users} clip(s) on the timeline still use this media"));
            }
            p.media.retain(|m| &m.id != media);
            Ok(json!({ "removed": media }))
        }

        Command::AddClip { track, start, duration, name, kind, media, source_in, text, template, link_to } => {
            let t = unlocked(p, track)?;
            if !kind.fits(t.kind) {
                return Err(format!("a {kind:?} clip cannot go on {:?} track {track}", t.kind).to_lowercase());
            }
            check_span(*start, *duration)?;
            check_source(p, media, *kind, *source_in, *duration, 1.0)?;
            check_free(p, track, *start, *duration, "")?;
            if let Some(other) = link_to {
                p.clip(other).ok_or_else(|| format!("no clip {other} to link with"))?;
            }
            let link = link_to.as_ref().map(|other| link_of(p, other));
            let id = p.new_id("c");
            p.sequence.clips.push(Clip {
                id: id.clone(),
                track: track.clone(),
                start: *start,
                duration: *duration,
                source_in: *source_in,
                name: name.clone(),
                kind: *kind,
                media: media.clone(),
                text: text.clone(),
                template: template.clone(),
                props: Default::default(),
                effects: vec![],
                transition_in: None,
                transition_out: None,
                last_actor: Some(actor.into()),
                link,
            });
            Ok(json!({ "clip": id }))
        }
        Command::MoveClip { clip, start, track } => {
            let c = editable(p, clip)?;
            let target = track.clone().unwrap_or_else(|| c.track.clone());
            let t = unlocked(p, &target)?;
            if !c.kind.fits(t.kind) {
                return Err(format!("clip {clip} cannot go on track {target}"));
            }
            check_span(*start, c.duration)?;
            check_free(p, &target, *start, c.duration, clip)?;
            let c = p.clip_mut(clip).unwrap();
            c.start = *start;
            c.track = target.clone();
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "track": target, "start": start }))
        }
        Command::MoveClips { clips, delta, tracks, markers } => {
            if clips.is_empty() {
                return Err("no clips given".into());
            }
            if let Some(stray) = tracks.keys().find(|id| !clips.contains(*id)) {
                return Err(format!("clip {stray} is given a track but is not in the list of clips to move"));
            }
            // Where each clip lands: its track, and the frames it covers there.
            let mut landing: Vec<(String, String, Frame, Frame)> = vec![];
            for id in clips {
                let c = editable(p, id)?;
                let track = tracks.get(id).cloned().unwrap_or_else(|| c.track.clone());
                let t = unlocked(p, &track)?;
                if !c.kind.fits(t.kind) {
                    return Err(format!("clip {id} cannot go on track {track}"));
                }
                let start = c.start + delta;
                check_span(start, c.duration)?;
                landing.push((id.clone(), track, start, start + c.duration));
            }
            for (i, (id, track, start, end)) in landing.iter().enumerate() {
                // Clips that stay where they are must not be in the way; moved clips must not collide with each other.
                let blocker = p
                    .sequence
                    .clips
                    .iter()
                    .find(|o| o.track == *track && !clips.contains(&o.id) && *start < o.end() && o.start < *end);
                if let Some(o) = blocker {
                    return Err(format!("{} would overlap clip {} ({}) on track {}", id, o.id, o.name, o.track));
                }
                let other = landing.iter().enumerate().find(|(j, l)| *j != i && l.1 == *track && *start < l.3 && l.2 < *end);
                if let Some((_, l)) = other {
                    return Err(format!("{} would overlap {} on track {}", id, l.0, track));
                }
            }
            // The stretch of timeline the clips covered before they moved; markers over it belong to them.
            let covered = (landing.iter().map(|l| l.2 - delta).min().unwrap_or(0), landing.iter().map(|l| l.3 - delta).max().unwrap_or(0));
            for c in p.sequence.clips.iter_mut().filter(|c| clips.contains(&c.id)) {
                c.start += delta;
                if let Some(track) = tracks.get(&c.id) {
                    c.track = track.clone();
                }
                c.last_actor = Some(actor.into());
            }
            let mut markers_moved = 0;
            if *markers {
                for m in p.sequence.markers.iter_mut().filter(|m| m.at >= covered.0 && m.at < covered.1) {
                    m.at += delta;
                    markers_moved += 1;
                }
            }
            Ok(json!({ "moved": clips, "delta": delta, "tracks": tracks, "markers_moved": markers_moved }))
        }
        Command::DuplicateClips { clips, delta } => {
            if clips.is_empty() {
                return Err("no clips given".into());
            }
            let mut created = vec![];
            let mut links = BTreeMap::new();
            for id in clips {
                let mut copy = editable(p, id)?;
                copy.start += delta;
                check_span(copy.start, copy.duration)?;
                check_free(p, &copy.track, copy.start, copy.duration, "")?;
                copy.id = p.new_id("c");
                copy.link = copy.link.take().map(|old| fresh_link(p, &mut links, &old));
                copy.last_actor = Some(actor.into());
                created.push(copy.id.clone());
                p.sequence.clips.push(copy);
            }
            Ok(json!({ "created": created }))
        }
        Command::Link { clips } => {
            for id in clips {
                editable(p, id)?;
            }
            let found = p.sequence.clips.iter().filter(|c| clips.contains(&c.id)).count();
            if found < 2 {
                return Err("linking needs at least two clips".into());
            }
            let link = p.new_id("L");
            for c in p.sequence.clips.iter_mut().filter(|c| clips.contains(&c.id)) {
                c.link = Some(link.clone());
                c.last_actor = Some(actor.into());
            }
            Ok(json!({ "linked": clips, "link": link }))
        }
        Command::Unlink { clips } => {
            let mut groups: Vec<String> = vec![];
            for id in clips {
                if let Some(link) = editable(p, id)?.link {
                    if !groups.contains(&link) {
                        groups.push(link);
                    }
                }
            }
            // Clips that are not linked at all are fine to leave alone, so this is a no-op rather than an error.
            let members = |c: &&Clip| c.link.as_ref().is_some_and(|l| groups.contains(l));
            for c in p.sequence.clips.iter().filter(members) {
                unlocked(p, &c.track)?;
            }
            let mut unlinked = vec![];
            for c in p.sequence.clips.iter_mut().filter(|c| c.link.as_ref().is_some_and(|l| groups.contains(l))) {
                c.link = None;
                c.last_actor = Some(actor.into());
                unlinked.push(c.id.clone());
            }
            Ok(json!({ "unlinked": unlinked }))
        }
        Command::TrimClip { clip, start, duration } => {
            let c = editable(p, clip)?;
            check_span(*start, *duration)?;
            let source_in = c.source_in + ((start - c.start) as f64 * c.speed()).round() as Frame;
            if c.media.is_some() && source_in < 0 {
                return Err(format!("clip {clip} has no source media before its current start"));
            }
            check_source(p, &c.media, c.kind, source_in.max(0), *duration, c.speed())?;
            check_free(p, &c.track, *start, *duration, clip)?;
            let c = p.clip_mut(clip).unwrap();
            c.start = *start;
            c.duration = *duration;
            c.source_in = source_in.max(0);
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "start": start, "duration": duration }))
        }
        Command::SplitClip { at, clips } => {
            let targets: Vec<String> = match clips {
                Some(ids) => {
                    for id in ids {
                        if !editable(p, id)?.contains_inner(*at) {
                            return Err(format!("frame {at} is not inside clip {id}"));
                        }
                    }
                    ids.clone()
                }
                None => p
                    .sequence
                    .clips
                    .iter()
                    .filter(|c| c.contains_inner(*at) && p.track(&c.track).is_some_and(|t| !t.locked))
                    .map(|c| c.id.clone())
                    .collect(),
            };
            if targets.is_empty() {
                return Err(format!("no clip to split at frame {at}"));
            }
            let mut links = BTreeMap::new();
            let created: Vec<String> = targets.iter().map(|id| split_at(p, id, *at, actor, &mut links)).collect();
            Ok(json!({ "split": targets, "created": created }))
        }
        Command::DeleteClip { clip } => {
            editable(p, clip)?;
            p.sequence.clips.retain(|c| &c.id != clip);
            Ok(json!({ "deleted": clip }))
        }
        Command::RippleDeleteClip { clip } => {
            let gone = editable(p, clip)?;
            p.sequence.clips.retain(|c| &c.id != clip);
            for c in p.sequence.clips.iter_mut().filter(|c| c.track == gone.track && c.start >= gone.end()) {
                c.start -= gone.duration;
            }
            Ok(json!({ "deleted": clip, "shifted_by": -gone.duration }))
        }
        Command::RippleDeleteRange { from, to } => {
            if *from < 0 || to <= from {
                return Err("range needs 0 <= from < to".into());
            }
            let (removed, locked) = ripple_delete(p, *from, *to, actor);
            Ok(json!({ "removed_frames": to - from, "clips_removed": removed, "locked_tracks_skipped": locked }))
        }
        Command::RippleDeleteRanges { ranges, media } => {
            if ranges.is_empty() {
                return Err("no ranges given".into());
            }
            if let Some(bad) = ranges.iter().find(|r| r.from < 0 || r.to <= r.from) {
                return Err(format!("range {} to {} needs 0 <= from < to", bad.from, bad.to));
            }
            let mut not_on_timeline = vec![];
            let spans = match media {
                None => ranges.clone(),
                Some(id) => {
                    p.media(id).ok_or_else(|| format!("no media {id}"))?;
                    let mut spans = vec![];
                    for r in ranges {
                        let places = source_on_timeline(p, id, r.from, r.to);
                        if places.is_empty() {
                            not_on_timeline.push(*r);
                        }
                        spans.extend(places.into_iter().map(|(_, span)| span));
                    }
                    if spans.is_empty() {
                        return Err(format!("none of those source frames of {id} is on the timeline"));
                    }
                    spans
                }
            };
            let spans = joined(spans);
            let (mut clips_removed, mut locked) = (0, 0);
            // Last first, so removing one span does not move the ones still to come.
            for s in spans.iter().rev() {
                let (removed, skipped) = ripple_delete(p, s.from, s.to, actor);
                clips_removed += removed;
                locked = skipped;
            }
            let removed_frames: Frame = spans.iter().map(|s| s.to - s.from).sum();
            let mut result = json!({
                "ranges_removed": spans.len(),
                "removed_frames": removed_frames,
                "clips_removed": clips_removed,
                "locked_tracks_skipped": locked,
                "duration_frames": p.duration(),
            });
            if media.is_some() {
                result["timeline_ranges_removed"] = json!(spans);
                result["source_ranges_not_on_timeline"] = json!(not_on_timeline);
            }
            Ok(result)
        }
        Command::AddMarker { at, label } => {
            if *at < 0 {
                return Err("marker cannot be before frame 0".into());
            }
            let id = p.new_id("m");
            p.sequence.markers.push(Marker { id: id.clone(), at: *at, label: label.clone(), actor: Some(actor.into()) });
            Ok(json!({ "marker": id }))
        }
        Command::RemoveMarker { marker } => {
            let before = p.sequence.markers.len();
            p.sequence.markers.retain(|m| &m.id != marker);
            if p.sequence.markers.len() == before {
                return Err(format!("no marker {marker}"));
            }
            Ok(json!({ "removed": marker }))
        }
        Command::SetRange { in_point, out_point } => {
            if let (Some(i), Some(o)) = (in_point, out_point) {
                if o <= i {
                    return Err("out point must be after in point".into());
                }
            }
            p.sequence.in_point = *in_point;
            p.sequence.out_point = *out_point;
            Ok(json!({ "in_point": in_point, "out_point": out_point }))
        }
        Command::AddTrack { kind, name } => {
            let (prefix, label) = match kind {
                TrackKind::Video => ("V", "Video"),
                TrackKind::Audio => ("A", "Audio"),
                TrackKind::Caption => ("T", "Captions"),
            };
            let tracks = &mut p.sequence.tracks;
            if tracks.iter().filter(|t| t.kind == *kind).count() >= MAX_TRACKS_PER_KIND {
                return Err(format!("a sequence can have at most {MAX_TRACKS_PER_KIND} {label} tracks").to_lowercase());
            }
            let n = tracks.iter().filter_map(|t| t.id.strip_prefix(prefix)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
            let id = format!("{prefix}{n}");
            // Top of the timeline first: captions, then video from highest to lowest, then audio.
            let at = match kind {
                TrackKind::Caption => 0,
                TrackKind::Video => tracks.iter().position(|t| t.kind != TrackKind::Caption).unwrap_or(tracks.len()),
                TrackKind::Audio => tracks.len(),
            };
            let name = name.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| format!("{label} {n}"));
            tracks.insert(at, Track { id: id.clone(), name, kind: *kind, locked: false, muted: false, hidden: false, props: BTreeMap::new() });
            Ok(json!({ "track": id }))
        }
        Command::RemoveTrack { track } => {
            p.track(track).ok_or_else(|| format!("no track {track}"))?;
            let clips = p.sequence.clips.iter().filter(|c| &c.track == track).count();
            if clips > 0 {
                return Err(format!("track {track} still has {clips} clip(s); move or delete them first"));
            }
            p.sequence.tracks.retain(|t| &t.id != track);
            Ok(json!({ "removed": track }))
        }
        Command::RenameTrack { track, name } => {
            if name.trim().is_empty() {
                return Err("a track name cannot be empty".into());
            }
            let t = p.sequence.tracks.iter_mut().find(|t| &t.id == track).ok_or_else(|| format!("no track {track}"))?;
            t.name = name.trim().to_string();
            Ok(json!({ "track": track, "name": t.name }))
        }
        Command::SetTrackState { track, locked, muted, hidden } => {
            let t = p
                .sequence
                .tracks
                .iter_mut()
                .find(|t| &t.id == track)
                .ok_or_else(|| format!("no track {track}"))?;
            t.locked = locked.unwrap_or(t.locked);
            t.muted = muted.unwrap_or(t.muted);
            t.hidden = hidden.unwrap_or(t.hidden);
            Ok(json!({ "track": track, "locked": t.locked, "muted": t.muted, "hidden": t.hidden }))
        }
        Command::SetTrackProperty { track, key, value } => {
            let t = p.sequence.tracks.iter_mut().find(|t| &t.id == track).ok_or_else(|| format!("no track {track}"))?;
            if t.kind != TrackKind::Caption {
                return Err(format!("track {track} is not a captions track; only captions tracks have subtitle settings"));
            }
            validate_caption_track_property(key, value)?;
            if value.is_null() {
                t.props.remove(key);
            } else {
                t.props.insert(key.clone(), value.clone());
            }
            Ok(json!({ "track": track, "key": key }))
        }

        Command::SetProperty { clip, key, value } => {
            let current = editable(p, clip)?;
            if !matches!(key.as_str(), "name" | "text") {
                validate_property(key, value)?;
            }
            if key == "speed" {
                let speed = value.as_f64().unwrap_or(100.0) / 100.0;
                check_source(p, &current.media, current.kind, current.source_in, current.duration, speed)
                    .map_err(|e| format!("{e}; shorten the clip before speeding it up"))?;
            }
            let c = p.clip_mut(clip).unwrap();
            match key.as_str() {
                "name" => c.name = value.as_str().ok_or("name must be a string")?.into(),
                "text" => c.text = value.as_str().map(String::from),
                _ if value.is_null() => {
                    c.props.remove(key);
                }
                _ => {
                    c.props.insert(key.clone(), value.clone());
                }
            }
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "key": key }))
        }
        Command::AddEffect { clip, effect } => {
            editable(p, clip)?;
            if !is_effect(effect) {
                return Err(format!("unknown effect {effect}; available: {}", effect_names()));
            }
            let c = p.clip_mut(clip).unwrap();
            if !c.effects.contains(effect) {
                c.effects.push(effect.clone());
            }
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "effects": c.effects }))
        }
        Command::RemoveEffect { clip, effect } => {
            editable(p, clip)?;
            let c = p.clip_mut(clip).unwrap();
            c.effects.retain(|e| e != effect);
            c.props.remove(&format!("fx.{effect}"));
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "effects": c.effects }))
        }
        Command::SetTransition { clip, transition, duration, edge } => {
            editable(p, clip)?;
            let name = transition.as_deref().filter(|n| *n != "Cut");
            if let Some(n) = name {
                if !is_transition(n) {
                    return Err(format!("unknown transition {n}; available: {}", transition_names()));
                }
            }
            let half_second = p.settings.frames(0.5);
            let c = p.clip_mut(clip).unwrap();
            let length = duration.unwrap_or(half_second).clamp(1, c.duration);
            let value = name.map(|n| Transition { name: n.into(), duration: length });
            match edge {
                Edge::In => c.transition_in = value,
                Edge::Out => c.transition_out = value,
            }
            c.last_actor = Some(actor.into());
            Ok(json!({ "clip": clip, "edge": edge, "transition": name }))
        }
    }
}
