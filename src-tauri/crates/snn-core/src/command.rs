use crate::library::{effect_names, is_effect, is_transition, transition_names, validate_property};
use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Every edit the editor supports. The serialized form is `{"tool": "...", "args": {...}}`,
/// and the tool names are the MCP tool names.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args")]
pub enum Command {
    #[serde(rename = "project.new")]
    NewProject { name: String },

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
    },
    #[serde(rename = "timeline.move_clip")]
    MoveClip {
        clip: String,
        start: Frame,
        #[serde(default)]
        track: Option<String>,
    },
    /// Shifts several clips by the same number of frames, keeping their tracks. All move or none does.
    #[serde(rename = "timeline.move_clips")]
    MoveClips { clips: Vec<String>, delta: Frame },
    /// Copies clips, with their properties and effects, `delta` frames later (or earlier) on the same tracks.
    #[serde(rename = "timeline.duplicate_clips")]
    DuplicateClips { clips: Vec<String>, delta: Frame },
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
    if (m.kind == MediaKind::Audio) != (kind == ClipKind::Audio) {
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

/// Splits the clip at `at` and returns the id of the new right-hand part.
fn split_at(p: &mut Project, clip: &str, at: Frame, actor: &str) -> String {
    let id = p.new_id("c");
    let left = p.clip_mut(clip).expect("caller checked the clip exists");
    let mut right = left.clone();
    right.id = id.clone();
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

/// Applies one command. On error the project may be half-changed; [`crate::Editor`] restores it.
pub fn apply(p: &mut Project, cmd: &Command, actor: &str) -> R<Value> {
    match cmd {
        Command::NewProject { name } => {
            *p = Project::new(name);
            Ok(json!({ "project": name }))
        }

        Command::ImportMedia { path, name, kind, duration, width, height } => {
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

        Command::AddClip { track, start, duration, name, kind, media, source_in, text, template } => {
            let t = unlocked(p, track)?;
            if !kind.fits(t.kind) {
                return Err(format!("a {kind:?} clip cannot go on {:?} track {track}", t.kind).to_lowercase());
            }
            check_span(*start, *duration)?;
            check_source(p, media, *kind, *source_in, *duration, 1.0)?;
            check_free(p, track, *start, *duration, "")?;
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
        Command::MoveClips { clips, delta } => {
            if clips.is_empty() {
                return Err("no clips given".into());
            }
            for id in clips {
                let c = editable(p, id)?;
                check_span(c.start + delta, c.duration)?;
                // The clips travel together, so they only need to clear the ones staying behind.
                let blocker = p.sequence.clips.iter().find(|o| {
                    o.track == c.track && !clips.contains(&o.id) && c.start + delta < o.end() && o.start < c.end() + delta
                });
                if let Some(o) = blocker {
                    return Err(format!("{} would overlap clip {} ({}) on track {}", id, o.id, o.name, o.track));
                }
            }
            for c in p.sequence.clips.iter_mut().filter(|c| clips.contains(&c.id)) {
                c.start += delta;
                c.last_actor = Some(actor.into());
            }
            Ok(json!({ "moved": clips, "delta": delta }))
        }
        Command::DuplicateClips { clips, delta } => {
            if clips.is_empty() {
                return Err("no clips given".into());
            }
            let mut created = vec![];
            for id in clips {
                let mut copy = editable(p, id)?;
                copy.start += delta;
                check_span(copy.start, copy.duration)?;
                check_free(p, &copy.track, copy.start, copy.duration, "")?;
                copy.id = p.new_id("c");
                copy.last_actor = Some(actor.into());
                created.push(copy.id.clone());
                p.sequence.clips.push(copy);
            }
            Ok(json!({ "created": created }))
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
            let created: Vec<String> = targets.iter().map(|id| split_at(p, id, *at, actor)).collect();
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
            let (from, to, len) = (*from, *to, to - from);
            let open: Vec<String> =
                p.sequence.tracks.iter().filter(|t| !t.locked).map(|t| t.id.clone()).collect();
            let locked = p.sequence.tracks.len() - open.len();
            for edge in [from, to] {
                let hit: Vec<String> = p
                    .sequence
                    .clips
                    .iter()
                    .filter(|c| open.contains(&c.track) && c.contains_inner(edge))
                    .map(|c| c.id.clone())
                    .collect();
                for id in hit {
                    split_at(p, &id, edge, actor);
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
            Ok(json!({ "removed_frames": len, "clips_removed": removed, "locked_tracks_skipped": locked }))
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
            tracks.insert(at, Track { id: id.clone(), name, kind: *kind, locked: false, muted: false, hidden: false });
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
