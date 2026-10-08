use crate::demo::demo_project;
use crate::*;
use serde_json::json;

fn cmd(v: serde_json::Value) -> Command {
    serde_json::from_value(v).expect("valid command json")
}

fn demo() -> Editor {
    Editor::new(demo_project())
}

fn clips_on<'a>(e: &'a Editor, track: &str) -> Vec<&'a Clip> {
    let mut v: Vec<_> = e.project().sequence.clips.iter().filter(|c| c.track == track).collect();
    v.sort_by_key(|c| c.start);
    v
}

fn assert_no_overlaps(e: &Editor) {
    for t in &e.project().sequence.tracks {
        let v = clips_on(e, &t.id);
        for w in v.windows(2) {
            assert!(w[0].end() <= w[1].start, "{} overlaps {} on {}", w[0].id, w[1].id, t.id);
        }
    }
}

#[test]
fn commands_use_mcp_tool_names() {
    let c = cmd(json!({"tool": "timeline.split_clip", "args": {"at": 60}}));
    assert_eq!(c.tool(), "timeline.split_clip");
    assert_eq!(c.args()["at"], 60);
}

#[test]
fn split_keeps_total_length_and_source_offset() {
    let mut e = demo();
    let before = clips_on(&e, "V1").len();
    // 20 s at 30 fps sits inside "First craft" (18 s to 40 s).
    e.execute("user", cmd(json!({"tool": "timeline.split_clip", "args": {"at": 600}}))).unwrap();
    let v1 = clips_on(&e, "V1");
    assert_eq!(v1.len(), before + 1);
    let right = v1.iter().find(|c| c.start == 600).unwrap();
    assert_eq!(right.source_in, 60);
    assert_eq!(right.end(), 1200);
    assert_no_overlaps(&e);
}

#[test]
fn overlap_is_rejected_and_project_unchanged() {
    let mut e = demo();
    let before = e.project().clone();
    let first = clips_on(&e, "V1")[0].id.clone();
    let err = e
        .execute("user", cmd(json!({"tool": "timeline.move_clip", "args": {"clip": first, "start": 150}})))
        .unwrap_err();
    assert!(err.contains("overlap"), "{err}");
    assert_eq!(e.project(), &before);
    assert!(!e.state().can_undo);
    assert!(!e.state().log[0].ok);
}

#[test]
fn ripple_delete_range_closes_the_gap_on_every_track() {
    let mut e = demo();
    let end = e.project().duration();
    // Remove 31 s to 35 s: 120 frames.
    e.execute("claude", cmd(json!({"tool": "timeline.ripple_delete_range", "args": {"from": 930, "to": 1050}})))
        .unwrap();
    assert_eq!(e.project().duration(), end - 120);
    assert_no_overlaps(&e);
    let chapter = e.project().sequence.markers.iter().find(|m| m.label == "Chapter 2").unwrap();
    assert_eq!(chapter.at, 2700 - 120);
    assert_eq!(e.state().log[0].actor, "claude");
}

#[test]
fn locked_tracks_are_left_alone() {
    let mut e = demo();
    e.execute("user", cmd(json!({"tool": "timeline.set_track_state", "args": {"track": "A2", "locked": true}})))
        .unwrap();
    let music = clips_on(&e, "A2")[0].clone();
    e.execute("user", cmd(json!({"tool": "timeline.ripple_delete_range", "args": {"from": 930, "to": 1050}})))
        .unwrap();
    assert_eq!(clips_on(&e, "A2")[0], &music);
    let err = e
        .execute("user", cmd(json!({"tool": "timeline.delete_clip", "args": {"clip": music.id}})))
        .unwrap_err();
    assert!(err.contains("locked"), "{err}");
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let mut e = demo();
    let start = e.project().clone();
    e.execute("user", cmd(json!({"tool": "timeline.split_clip", "args": {"at": 600}}))).unwrap();
    let id = clips_on(&e, "V1")[0].id.clone();
    e.execute("user", cmd(json!({"tool": "timeline.ripple_delete_clip", "args": {"clip": id}}))).unwrap();
    let after = e.project().clone();
    e.undo("user").unwrap();
    e.undo("user").unwrap();
    assert_eq!(e.project(), &start);
    assert!(e.undo("user").is_err());
    e.redo("user").unwrap();
    e.redo("user").unwrap();
    assert_eq!(e.project(), &after);
}

#[test]
fn clip_kind_must_match_track_kind() {
    let mut e = demo();
    let err = e
        .execute(
            "user",
            cmd(json!({"tool": "timeline.add_clip", "args":
                {"track": "A3", "start": 3000, "duration": 30, "name": "Title", "kind": "graphic"}})),
        )
        .unwrap_err();
    assert!(err.contains("cannot go on"), "{err}");
}

#[test]
fn properties_effects_and_transitions() {
    let mut e = demo();
    let id = clips_on(&e, "V1")[1].id.clone();
    for c in [
        json!({"tool": "clip.set_property", "args": {"clip": id, "key": "zoom_x", "value": 1.2}}),
        json!({"tool": "clip.add_effect", "args": {"clip": id, "effect": "Zoom punch"}}),
        json!({"tool": "clip.add_effect", "args": {"clip": id, "effect": "Zoom punch"}}),
        json!({"tool": "clip.set_transition", "args": {"clip": id, "transition": "Whoosh zoom"}}),
    ] {
        e.execute("user", cmd(c)).unwrap();
    }
    let c = e.project().clip(&id).unwrap();
    assert_eq!(c.props["zoom_x"], 1.2);
    assert_eq!(c.effects, vec!["Zoom punch"]);
    assert_eq!(c.transition_in.as_ref().unwrap().duration, 15);
}

#[test]
fn save_and_open_round_trip() {
    let mut e = demo();
    let path = std::env::temp_dir().join(format!("snn-core-test-{}.snn", std::process::id()));
    e.save(Some(&path)).unwrap();
    let mut other = Editor::default();
    other.open(&path).unwrap();
    assert_eq!(other.project(), e.project());
    assert!(!other.state().dirty);
    std::fs::remove_file(path).ok();
}

#[test]
fn media_clips_stay_inside_their_source() {
    let mut e = Editor::default();
    let import = json!({"tool": "media.import", "args":
        {"path": "C:/clips/run.mp4", "kind": "video", "duration": 300, "width": 1920, "height": 1080}});
    let r = e.execute("user", cmd(import.clone())).unwrap();
    let id = r["media"]["id"].as_str().unwrap().to_string();
    assert_eq!(r["media"]["name"], "run.mp4");
    // The same path again gives back the same item.
    assert_eq!(e.execute("user", cmd(import)).unwrap()["media"]["id"], id.as_str());
    assert_eq!(e.project().media.len(), 1);

    let add = |start: i64, duration: i64, source_in: i64| {
        cmd(json!({"tool": "timeline.add_clip", "args": {"track": "V1", "start": start, "duration": duration,
            "name": "run", "kind": "video", "media": id, "source_in": source_in}}))
    };
    assert!(e.execute("user", add(0, 301, 0)).unwrap_err().contains("past the end"));
    let clip = e.execute("user", add(0, 200, 50)).unwrap()["clip"].as_str().unwrap().to_string();
    // 50 + 260 frames is more than the 300 the file has.
    let trim = cmd(json!({"tool": "timeline.trim_clip", "args": {"clip": clip, "start": 0, "duration": 260}}));
    assert!(e.execute("user", trim).unwrap_err().contains("past the end"));
    let audio = cmd(json!({"tool": "timeline.add_clip", "args": {"track": "A1", "start": 0, "duration": 10,
        "name": "run", "kind": "audio", "media": id}}));
    // This file was imported without a sound track, so it cannot back an audio clip.
    assert!(e.execute("user", audio).unwrap_err().contains("no sound track"));
    let remove = cmd(json!({"tool": "media.remove", "args": {"media": id}}));
    assert!(e.execute("user", remove).unwrap_err().contains("still use"));
}

/// Every catalog tool runs with its example arguments against the fixture. A tool whose name or
/// schema drifts from the command layer fails here with "invalid arguments" or "unknown tool".
#[test]
fn every_catalog_tool_is_wired_up() {
    for tool in catalog() {
        assert!(tool.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{}", tool.name);
        assert_eq!(tool.schema["type"], "object", "{}", tool.name);
        let mut e = demo();
        let media = json!({"path": "C:/clips/other.mp4", "kind": "video", "duration": 300});
        e.call_tool("test", "media_import", media).unwrap();
        e.call_tool("test", "timeline_add_marker", json!({"at": 1, "label": "x"})).unwrap();
        e.call_tool("test", "history_undo", json!({})).unwrap();
        let result = e.call_tool("agent", tool.name, tool.example.clone());
        if let Err(err) = &result {
            assert!(!err.contains("invalid arguments") && !err.contains("unknown tool"), "{}: {err}", tool.name);
        }
        // These examples are valid against the fixture and must succeed outright.
        // The analysis tools need FFmpeg and a real file; `analysis_reads_a_real_file` covers them.
        if !matches!(tool.name, "media_remove" | "timeline_remove_marker") && !crate::is_analysis(tool.name) && !crate::is_export(tool.name) {
            assert!(result.is_ok(), "{}: {:?}", tool.name, result);
        }
        assert_eq!(e.state().log[0].actor, "agent");
    }
}

#[test]
fn guides_are_listed_and_readable() {
    let mut e = demo();
    let listed = e.call_tool("agent", "guide_list", json!({})).unwrap();
    let names: Vec<&str> = listed["guides"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"retention-edit"));
    for g in crate::guides() {
        assert!(g.text.starts_with("# ") && !g.title.is_empty() && !g.description.is_empty(), "{}", g.name);
        assert_eq!(e.call_tool("agent", "guide_get", json!({"name": g.name})).unwrap(), json!(g.text));
        // A guide must not send an agent to a tool that does not exist.
        for word in g.text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let prefixes = ["timeline_", "clip_", "project_", "history_", "media_", "batch_", "view_", "guide_", "library_", "frames_", "audio_", "export_"];
            if prefixes.iter().any(|p| word.starts_with(p) && word.len() > p.len()) {
                assert!(catalog().iter().any(|t| t.name == word), "{} mentions unknown tool {word}", g.name);
            }
        }
    }
    assert!(e.call_tool("agent", "guide_get", json!({"name": "nope"})).unwrap_err().contains("retention-edit"));
}

#[test]
fn base64_reads_back_what_it_wrote() {
    for bytes in [&b""[..], b"f", b"fo", b"foo", b"foobar", &[0, 255, 254, 3, 128]] {
        assert_eq!(crate::unbase64(&crate::base64(bytes)).unwrap(), bytes);
    }
    assert!(crate::unbase64("not base64!").is_err());
}

#[test]
fn base64_matches_the_standard() {
    assert_eq!(crate::base64(b""), "");
    assert_eq!(crate::base64(b"f"), "Zg==");
    assert_eq!(crate::base64(b"fo"), "Zm8=");
    assert_eq!(crate::base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(crate::base64(&[0xff, 0xfe, 0xfd]), "//79");
}

#[test]
fn analysis_arguments_are_checked_against_the_project() {
    let mut e = Editor::default();
    e.call_tool("t", "media_import", json!({"path": "C:/a.mp4", "kind": "video", "duration": 9000, "width": 1920, "height": 1080})).unwrap();
    let media = e.project().media[0].id.clone();
    let err = |e: &Editor, tool: &str, args: serde_json::Value| e.analysis_job(tool, &args).unwrap_err();
    assert!(err(&e, "frames_get", json!({"at": 10})).contains("no video clip is visible"));
    assert!(err(&e, "frames_get", json!({"media": media, "at": 9000})).contains("9000 frames long"));
    assert!(err(&e, "frames_get", json!({"media": "nope", "at": 1})).contains("no media"));
    assert!(err(&e, "frames_contact_sheet", json!({"media": media, "from": 50, "to": 50})).contains("nothing to show"));
    assert!(err(&e, "audio_get_activity", json!({})).contains("media"));

    // On the timeline, a frame is found through the clip: its source offset and its speed.
    e.call_tool("t", "timeline_add_clip", json!({"track": "V1", "start": 100, "duration": 300, "name": "x", "kind": "video", "media": media, "source_in": 3000})).unwrap();
    let Job::Frame { shot, .. } = e.analysis_job("frames_get", &json!({"at": 130})).unwrap() else { panic!() };
    assert_eq!(format!("{shot:?}"), r#"Shot { path: "C:/a.mp4", seconds: 101.0, label: "T 0m04s f130" }"#);
    let Job::Sheet { shots, columns, tile, .. } = e.analysis_job("frames_contact_sheet", &json!({"count": 4})).unwrap() else { panic!() };
    // Four samples across frames 0 to 400; the first falls before the clip and is left out.
    assert_eq!((shots.len(), columns, tile), (3, 3, (320, 180)));
}

/// Runs the analysis tools on a generated file. Skipped when FFmpeg is not installed.
#[test]
fn analysis_reads_a_real_file() {
    let dir = std::env::temp_dir().join(format!("snn-analysis-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("çekim ğü.mp4").display().to_string();
    // 20 seconds of test pattern; a steady tone that drops out from 6 s to 10 s and peaks at 14 s.
    let volume = "volume='if(between(t,6,10),0,if(between(t,14,15),1,0.2))':eval=frame";
    let made = std::process::Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=20"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=20", "-af", volume])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", &path])
        .status();
    if !made.is_ok_and(|s| s.success()) {
        eprintln!("skipped: FFmpeg is not on the PATH");
        return;
    }
    let mut e = Editor::default();
    let media = e.call_tool("t", "media_import", json!({"path": path})).unwrap()["media"]["id"].as_str().unwrap().to_string();
    let jpeg = |a: crate::Analysis| a.jpeg.filter(|j| j.starts_with(&[0xff, 0xd8])).expect("a JPEG");

    let one = e.analysis_job("frames_get", &json!({"media": media, "at": 300, "width": 320})).unwrap().run().unwrap();
    assert!(jpeg(one).len() > 1000);
    let sheet = e.analysis_job("frames_contact_sheet", &json!({"media": media, "count": 8, "columns": 4})).unwrap().run().unwrap();
    assert_eq!((sheet.info["tiles"].clone(), sheet.info["frames_between_tiles"].clone()), (json!(8), json!(75)));
    assert!(jpeg(sheet).len() > 5000);

    let sound = e.call_tool("agent", "audio_get_activity", json!({"media": media})).unwrap();
    let silences = sound["silences"].as_array().unwrap();
    assert_eq!(silences.len(), 1, "{sound}");
    let (from, to) = (silences[0]["from"].as_i64().unwrap(), silences[0]["to"].as_i64().unwrap());
    assert!((from - 180).abs() <= 3 && (to - 300).abs() <= 3, "{sound}");
    let loudest = sound["loudest"].as_array().unwrap().iter().max_by(|a, b| a["lufs"].as_f64().unwrap().total_cmp(&b["lufs"].as_f64().unwrap())).unwrap();
    assert!((420..=460).contains(&loudest["at"].as_i64().unwrap()), "{sound}");
    assert_eq!(e.state().log[0].tool, "audio.get_activity");

    // A window in the middle reports frames of the whole file, not of the window.
    let part = e.call_tool("agent", "audio_get_activity", json!({"media": media, "from": 150, "to": 450})).unwrap();
    assert!((part["silences"][0]["from"].as_i64().unwrap() - 180).abs() <= 3, "{part}");
    assert!(e.call_tool("agent", "audio_get_activity", json!({"media": media, "audio_stream": 3})).unwrap_err().contains("1 audio stream"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pace_counts_picture_changes_per_minute() {
    let mut e = Editor::default();
    e.call_tool("t", "media_import", json!({"path": "C:/a.mp4", "kind": "video", "duration": 9000})).unwrap();
    let media = e.project().media[0].id.clone();
    let mut add = |track: &str, start: i64, duration: i64, kind: &str| {
        let mut args = json!({"track": track, "start": start, "duration": duration, "name": "x", "kind": kind});
        match kind {
            "video" => args["media"] = json!(media),
            _ => args["text"] = json!("x"),
        }
        e.call_tool("t", "timeline_add_clip", args).unwrap();
    };
    // 30 fps: two cuts in the first 30 seconds, a card later in the first minute, one cut in the second minute.
    add("V1", 0, 300, "video");
    add("V1", 300, 300, "video");
    add("V1", 600, 1500, "video");
    add("V1", 2100, 1500, "video");
    add("V3", 1200, 90, "graphic");
    add("T1", 10, 20, "caption");
    let pace = e.call_tool("t", "timeline_get_pace", json!({})).unwrap();
    assert_eq!(pace["changes"], 4);
    assert_eq!(pace["per_minute"], json!([3, 1]));
    assert_eq!(pace["first_30_seconds"], 2);
    assert_eq!(pace["longest_without_change"][0], json!({"from": 2100, "to": 3600, "seconds": 50.0}));
}

#[test]
fn batch_is_atomic_and_one_undo_step() {
    let mut e = demo();
    let before = e.project().clone();
    let bad = json!({"commands": [
        {"tool": "timeline_add_marker", "args": {"at": 5, "label": "kept?"}},
        {"tool": "timeline_delete_clip", "args": {"clip": "nope"}},
    ]});
    let err = e.call_tool("agent", "batch_apply", bad).unwrap_err();
    assert!(err.contains("command 1") && err.contains("nothing was changed"), "{err}");
    assert_eq!(e.project(), &before);

    let good = json!({"commands": [
        {"tool": "timeline_add_marker", "args": {"at": 5, "label": "a"}},
        {"tool": "timeline_ripple_delete_range", "args": {"from": 930, "to": 1050}},
    ]});
    e.call_tool("agent", "batch_apply", good).unwrap();
    assert_ne!(e.project(), &before);
    e.undo("user").unwrap();
    assert_eq!(e.project(), &before);
}

#[test]
fn reads_filter_by_range_and_track() {
    let mut e = demo();
    let all = e.call_tool("agent", "timeline_get_state", json!({})).unwrap();
    assert_eq!(all["clips"].as_array().unwrap().len(), e.project().sequence.clips.len());
    let some = e.call_tool("agent", "timeline_get_state", json!({"from": 0, "to": 120, "track": "V1"})).unwrap();
    assert_eq!(some["clips"].as_array().unwrap().len(), 1);
    assert_eq!(e.call_tool("agent", "project_get_info", json!({})).unwrap()["fps"], 30.0);
    assert!(!e.state().can_undo, "reads must not create undo steps");
}

#[test]
fn mp4_probe_reads_length_and_size() {
    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v
    }
    let mut mvhd = vec![0u8; 100];
    mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&7500u32.to_be_bytes());
    let mut tkhd = vec![0u8; 84];
    tkhd[76..80].copy_from_slice(&(1280u32 << 16).to_be_bytes());
    tkhd[80..84].copy_from_slice(&(720u32 << 16).to_be_bytes());
    let moov = [bx(b"mvhd", &mvhd), bx(b"trak", &bx(b"tkhd", &tkhd))].concat();
    let file = [bx(b"ftyp", b"isom"), bx(b"mdat", &[0u8; 32]), bx(b"moov", &moov)].concat();

    let path = std::env::temp_dir().join(format!("snn-core-probe-{}.mp4", std::process::id()));
    std::fs::write(&path, file).unwrap();
    let probe = probe_mp4(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(probe, Probe { kind: MediaKind::Video, seconds: 12.5, width: 1280, height: 720, has_audio: false });
}

/// A box with a 4-character type and a payload, for building small MP4 files by hand in tests.
fn mp4_box(kind: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(payload);
    v
}

/// A track with a handler of the given type, the way MP4 stores audio (soun) and video (vide).
fn mp4_track(handler: &[u8; 4]) -> Vec<u8> {
    let mut hdlr = vec![0u8; 24];
    hdlr[8..12].copy_from_slice(handler);
    mp4_box(b"trak", &mp4_box(b"mdia", &mp4_box(b"hdlr", &hdlr)))
}

#[test]
fn probe_sees_a_sound_track() {
    let mut mvhd = vec![0u8; 100];
    mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&6000u32.to_be_bytes());
    let mut video = mp4_track(b"vide");
    video.extend(mp4_track(b"soun"));
    let mut moov = mp4_box(b"mvhd", &mvhd);
    moov.extend(video);
    let file = [mp4_box(b"ftyp", b"isom"), mp4_box(b"moov", &moov)].concat();
    let path = std::env::temp_dir().join(format!("snn-core-audio-{}.mp4", std::process::id()));
    std::fs::write(&path, file).unwrap();
    let probe = probe_mp4(&path);
    std::fs::remove_file(&path).ok();
    let probe = probe.unwrap();
    assert!(probe.has_audio, "a soun handler means the file has sound");
}

#[test]
fn probe_reads_the_real_mrbeast_file_when_present() {
    // Documents/MrBeast-being-a-SnapYT.App.mp4 on the development machine. Skipped where it is absent.
    let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) else { return };
    let path = std::path::Path::new(&home).join("Documents").join("MrBeast-being-a-SnapYT.App.mp4");
    if !path.is_file() {
        return;
    }
    let probe = probe_mp4(&path).unwrap();
    assert_eq!(probe.kind, MediaKind::Video);
    assert_eq!((probe.width, probe.height), (640, 360));
    assert!((probe.seconds - 594.08).abs() < 0.05, "length was {}", probe.seconds);
    assert!(probe.has_audio, "this file has an AAC track");
}

#[test]
fn import_records_whether_the_file_has_sound() {
    let mut e = Editor::default();
    let out = e
        .execute("user", cmd(json!({"tool": "media.import", "args": {"path": "C:/clips/a.mp4", "kind": "video", "duration": 300, "has_audio": true}})))
        .unwrap();
    assert_eq!(out["media"]["has_audio"], json!(true));
    let out = e
        .execute("user", cmd(json!({"tool": "media.import", "args": {"path": "C:/clips/b.mp4", "kind": "video", "duration": 300}})))
        .unwrap();
    assert_eq!(out["media"]["has_audio"], json!(false), "older callers that omit it get false");
}

#[test]
fn mute_is_a_sound_property_that_defaults_to_off() {
    let lib = library();
    let mute = lib.properties.iter().find(|p| p.key == "mute").expect("mute is in the library");
    assert_eq!(mute.applies, crate::library::Applies::Sound);
    assert!(matches!(mute.kind, crate::library::PropKind::Bool { default: false }));
}

#[test]
fn tracks_can_be_added_renamed_and_removed() {
    let mut e = Editor::default();
    let names: Vec<_> = e.project().sequence.tracks.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, ["Captions", "Video 3", "Video 2", "Video 1", "Audio 1", "Audio 2", "Audio 3"]);
    let v = e.execute("user", cmd(json!({"tool": "timeline.add_track", "args": {"kind": "video"}}))).unwrap();
    let a = e.execute("user", cmd(json!({"tool": "timeline.add_track", "args": {"kind": "audio"}}))).unwrap();
    assert_eq!((v["track"].as_str(), a["track"].as_str()), (Some("V4"), Some("A4")));
    let ids: Vec<_> = e.project().sequence.tracks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids, ["T1", "V4", "V3", "V2", "V1", "A1", "A2", "A3", "A4"]);
    e.execute("user", cmd(json!({"tool": "timeline.rename_track", "args": {"track": "V4", "name": "Titles"}}))).unwrap();
    assert_eq!(e.project().track("V4").unwrap().name, "Titles");
    let clip = json!({"tool": "timeline.add_clip", "args": {"track": "V4", "start": 0, "duration": 30, "name": "t", "kind": "graphic"}});
    e.execute("user", cmd(clip)).unwrap();
    let remove = cmd(json!({"tool": "timeline.remove_track", "args": {"track": "V4"}}));
    assert!(e.execute("user", remove.clone()).unwrap_err().contains("still has 1 clip"));
    e.undo("user").unwrap();
    e.execute("user", remove).unwrap();
    assert!(e.project().track("V4").is_none());
}

#[test]
fn clips_move_and_duplicate_as_a_group() {
    let mut e = demo();
    let v1: Vec<String> = clips_on(&e, "V1").iter().map(|c| c.id.clone()).collect();
    // Shifting every V1 clip by one second works although each lands where its neighbour was.
    e.execute("user", cmd(json!({"tool": "timeline.move_clips", "args": {"clips": v1, "delta": 30}}))).unwrap();
    assert_eq!(clips_on(&e, "V1")[0].start, 30);
    assert_no_overlaps(&e);
    // Moving only the first one a second later would land on the second.
    let first = clips_on(&e, "V1")[0].id.clone();
    let err = e
        .execute("user", cmd(json!({"tool": "timeline.move_clips", "args": {"clips": [first.clone()], "delta": 30}})))
        .unwrap_err();
    assert!(err.contains("overlap"), "{err}");

    e.execute("user", cmd(json!({"tool": "clip.add_effect", "args": {"clip": first, "effect": "Blur"}}))).unwrap();
    let r = e
        .execute("user", cmd(json!({"tool": "timeline.duplicate_clips", "args": {"clips": [first], "delta": 6000}})))
        .unwrap();
    let copy = e.project().clip(r["created"][0].as_str().unwrap()).unwrap();
    assert_eq!((copy.start, copy.effects.clone()), (6030, vec!["Blur".to_string()]));
    assert_no_overlaps(&e);
}

#[test]
fn library_names_are_enforced() {
    let mut e = demo();
    let id = clips_on(&e, "V1")[0].id.clone();
    let set = |key: &str, value: serde_json::Value| cmd(json!({"tool": "clip.set_property", "args": {"clip": id, "key": key, "value": value}}));
    assert!(e.execute("user", set("scale", json!(120))).unwrap_err().contains("unknown property"));
    assert!(e.execute("user", set("opacity", json!(150))).unwrap_err().contains("between 0 and 100"));
    assert!(e.execute("user", set("blend", json!("Sparkle"))).unwrap_err().contains("one of"));
    assert!(e.execute("user", set("color", json!("red"))).unwrap_err().contains("colour like"));
    assert!(e.execute("user", set("fx.Nope", json!(10))).unwrap_err().contains("unknown effect"));
    e.execute("user", set("fx.Blur", json!(80))).unwrap();
    e.execute("user", set("flip_h", json!(true))).unwrap();
    e.execute("user", set("opacity", json!(null))).unwrap();
    let bad_fx = cmd(json!({"tool": "clip.add_effect", "args": {"clip": id, "effect": "Speed ramp"}}));
    assert!(e.execute("user", bad_fx).unwrap_err().contains("unknown effect"));
    let bad_tr = cmd(json!({"tool": "clip.set_transition", "args": {"clip": id, "transition": "Slide"}}));
    assert!(e.execute("user", bad_tr).unwrap_err().contains("unknown transition"));

    // Every library entry is accepted by the commands that take it.
    let lib = library();
    for fx in &lib.effects {
        e.execute("user", cmd(json!({"tool": "clip.add_effect", "args": {"clip": id, "effect": fx.name}}))).unwrap();
    }
    for (i, t) in lib.transitions.iter().enumerate() {
        let edge = if i % 2 == 0 { "in" } else { "out" };
        e.execute("user", cmd(json!({"tool": "clip.set_transition", "args": {"clip": id, "transition": t.name, "edge": edge}}))).unwrap();
    }
    let c = e.project().clip(&id).unwrap();
    assert_eq!(c.effects.len(), lib.effects.len());
    assert!(c.transition_in.is_some() && c.transition_out.is_some());
}

#[test]
fn speed_uses_more_source() {
    let mut e = Editor::default();
    let media = json!({"path": "C:/clips/run.mp4", "kind": "video", "duration": 300});
    let m = e.call_tool("user", "media_import", media).unwrap()["media"]["id"].as_str().unwrap().to_string();
    let add = json!({"track": "V1", "start": 0, "duration": 200, "name": "run", "kind": "video", "media": m});
    let id = e.call_tool("user", "timeline_add_clip", add).unwrap()["clip"].as_str().unwrap().to_string();
    let speed = |v: i64| json!({"clip": id, "key": "speed", "value": v});
    // 200 timeline frames at 200% would need 400 source frames; the file has 300.
    assert!(e.call_tool("user", "clip_set_property", speed(200)).unwrap_err().contains("shorten the clip"));
    e.call_tool("user", "clip_set_property", speed(150)).unwrap();
    let right = e.call_tool("user", "timeline_split_clip", json!({"at": 100})).unwrap()["created"][0].as_str().unwrap().to_string();
    assert_eq!(e.project().clip(&right).unwrap().source_in, 150);
}

/// Export tests: they make small source files, render a project, and read pixels back.
mod export {
    use crate::*;
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::AtomicBool;

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];
    const GREEN: [u8; 3] = [0, 255, 0];

    struct Bench {
        dir: PathBuf,
        e: Editor,
    }

    fn ffmpeg(args: &[&str]) -> bool {
        Command::new(ffmpeg_path()).args(["-loglevel", "error", "-y"]).args(args).status().is_ok_and(|s| s.success())
    }

    impl Bench {
        /// A 640×360 project at 30 fps with a red file, a blue file, and one that is red for a second and then blue.
        /// `None` when FFmpeg is not installed.
        fn new(name: &str) -> Option<Bench> {
            let dir = std::env::temp_dir().join(format!("snn-export-test-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = |n: &str| dir.join(n).display().to_string();
            let encode = ["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"];
            for colour in ["red", "blue"] {
                let picture = format!("color=c={colour}:s=640x360:r=30:d=30");
                let made = ffmpeg(&[&["-f", "lavfi", "-i", &picture, "-f", "lavfi", "-i", "sine=frequency=440:duration=30"], &encode[..], &[&file(&format!("{colour}.mp4"))]].concat());
                if !made {
                    eprintln!("skipped: FFmpeg is not installed");
                    return None;
                }
            }
            let steps = "color=c=red:s=640x360:r=30:d=1[a];color=c=blue:s=640x360:r=30:d=3[b];[a][b]concat=n=2:v=1:a=0";
            assert!(ffmpeg(&[&["-f", "lavfi", "-i", steps, "-f", "lavfi", "-i", "sine=frequency=440:duration=4"], &encode[..], &[&file("steps.mp4")]].concat()));
            // What the window would hand over for text: an empty picture and one with a green box in the middle.
            assert!(ffmpeg(&["-f", "lavfi", "-i", "color=c=black@0:s=640x360,format=rgba", "-frames:v", "1", &file("blank.png")]));
            assert!(ffmpeg(&["-f", "lavfi", "-i", "color=c=black@0:s=640x360,format=rgba,drawbox=x=270:y=130:w=100:h=100:color=lime:t=fill:replace=1", "-frames:v", "1", &file("text.png")]));
            let mut e = Editor::default();
            e.call_tool("t", "project_set_settings", json!({"width": 640, "height": 360})).unwrap();
            for n in ["red.mp4", "blue.mp4", "steps.mp4"] {
                e.call_tool("t", "media_import", json!({"path": file(n)})).unwrap();
            }
            Some(Bench { dir, e })
        }

        fn clip(&mut self, track: &str, start: i64, duration: i64, media: &str, source_in: i64) -> String {
            let id = self.e.project().media.iter().find(|m| m.name == media).unwrap().id.clone();
            let args = json!({"track": track, "start": start, "duration": duration, "name": media, "kind": "video", "media": id, "source_in": source_in});
            self.e.call_tool("t", "timeline_add_clip", args).unwrap()["clip"].as_str().unwrap().to_string()
        }

        fn set(&mut self, clip: &str, key: &str, value: Value) {
            self.e.call_tool("t", "clip_set_property", json!({"clip": clip, "key": key, "value": value})).unwrap();
        }

        fn transition(&mut self, clip: &str, name: &str, edge: &str) {
            self.e.call_tool("t", "clip_set_transition", json!({"clip": clip, "transition": name, "duration": 20, "edge": edge})).unwrap();
        }

        fn export(&self, name: &str, fps: f64, text: &[TextTrack]) -> (PathBuf, Plan) {
            self.export_with(name, fps, text, "libx264", |_| {})
        }

        /// Renders with `encoder`, after `change` has set what the test needs.
        fn export_with(&self, name: &str, fps: f64, text: &[TextTrack], encoder: &str, change: impl FnOnce(&mut ExportSettings)) -> (PathBuf, Plan) {
            let path = self.dir.join(name);
            let mut settings = ExportSettings { quality: Quality::High, ..ExportSettings::new(path.display().to_string(), 640, 360, fps) };
            change(&mut settings);
            let plan = plan_export(self.e.project(), &settings, encoder, text, &|m| probe_streams(&m.path), &self.dir).unwrap();
            let mut last = 0;
            render(&plan, &AtomicBool::new(false), &mut |n| last = n).unwrap();
            assert_eq!(last, plan.total_frames);
            (path, plan)
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    /// The colour at a point of one frame of a rendered file.
    fn pixel(file: &Path, frame: i64, x: usize, y: usize) -> [u8; 3] {
        let pick = format!("select=eq(n\\,{frame})");
        let out = Command::new(ffmpeg_path())
            .args(["-loglevel", "error", "-i", &file.display().to_string(), "-vf", &pick, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap();
        assert_eq!(out.stdout.len(), 640 * 360 * 3, "frame {frame} of {} could not be read", file.display());
        let i = (y * 640 + x) * 3;
        [out.stdout[i], out.stdout[i + 1], out.stdout[i + 2]]
    }

    #[track_caller]
    fn expect(file: &Path, frame: i64, x: usize, y: usize, want: [u8; 3]) {
        let got = pixel(file, frame, x, y);
        let near = got.iter().zip(want).all(|(a, b)| (*a as i32 - b as i32).abs() <= 40);
        assert!(near, "frame {frame} at {x},{y}: got {got:?}, expected about {want:?}");
    }

    /// Frames of picture and seconds of sound in a rendered file.
    fn length(file: &Path) -> (i64, f64) {
        let read = |stream: &str, key: &str| {
            let out = Command::new(ffmpeg_path()).args(["-i", &file.display().to_string(), "-map", stream, "-f", "null", "-"]).output().unwrap();
            let log = String::from_utf8_lossy(&out.stderr).into_owned();
            log.rsplit(key).next().and_then(|s| s.split_whitespace().next().map(str::to_string)).unwrap_or_default()
        };
        let seconds = read("0:a", "time=").split(':').filter_map(|p| p.parse::<f64>().ok()).fold(0.0, |total, p| total * 60.0 + p);
        (read("0:v", "frame=").parse().unwrap_or(0), seconds)
    }

    #[test]
    fn cuts_speed_and_frame_rate() {
        let Some(mut b) = Bench::new("cuts") else { return };
        b.clip("V1", 0, 30, "red.mp4", 0);
        b.clip("V1", 30, 30, "blue.mp4", 100);
        // Twice as fast: 30 timeline frames use 60 of the file, which turns blue at its frame 30.
        let fast = b.clip("V1", 60, 30, "steps.mp4", 0);
        b.set(&fast, "speed", json!(200));
        let (file, plan) = b.export("cuts.mp4", 30.0, &[]);
        assert_eq!((plan.total_frames, plan.chunks.len()), (90, 1));
        let (frames, seconds) = length(&file);
        assert_eq!(frames, 90);
        assert!((seconds - 3.0).abs() < 0.1, "{seconds}");
        expect(&file, 10, 320, 180, RED);
        expect(&file, 29, 320, 180, RED);
        expect(&file, 30, 320, 180, BLUE);
        expect(&file, 65, 320, 180, RED);
        expect(&file, 85, 320, 180, BLUE);

        // The same timeline at 60 fps has twice the frames and cuts at the same moments.
        let (file, plan) = b.export("cuts60.mp4", 60.0, &[]);
        assert_eq!((plan.total_frames, length(&file).0), (180, 180));
        expect(&file, 59, 320, 180, RED);
        expect(&file, 60, 320, 180, BLUE);
    }

    #[test]
    fn long_timelines_are_rendered_in_chunks_without_a_seam() {
        let Some(mut b) = Bench::new("chunks") else { return };
        b.clip("V1", 0, 700, "red.mp4", 0);
        b.clip("V1", 700, 500, "blue.mp4", 0);
        let (file, plan) = b.export("long.mp4", 30.0, &[]);
        assert_eq!((plan.chunks.len(), plan.total_frames), (2, 1200));
        let (frames, seconds) = length(&file);
        assert_eq!(frames, 1200);
        assert!((seconds - 40.0).abs() < 0.1, "{seconds}");
        // The chunks meet at frame 600; the cut is at 700.
        for (frame, colour) in [(599, RED), (600, RED), (601, RED), (699, RED), (700, BLUE), (1199, BLUE)] {
            expect(&file, frame, 320, 180, colour);
        }
    }

    #[test]
    fn layers_are_placed_cropped_and_tinted_like_the_preview() {
        let Some(mut b) = Bench::new("look") else { return };
        b.clip("V1", 0, 150, "red.mp4", 0);
        // Half size, moved 160 px right: covers x 320..640, y 90..270.
        let inset = b.clip("V2", 0, 30, "blue.mp4", 0);
        // The same, turned a quarter: covers x 390..570, y 20..340.
        let turned = b.clip("V2", 30, 30, "blue.mp4", 0);
        for clip in [&inset, &turned] {
            for (key, value) in [("zoom_x", json!(0.5)), ("zoom_y", json!(0.5)), ("pos_x", json!(160))] {
                b.set(clip, key, value);
            }
        }
        b.set(&turned, "rotation", json!(90));
        // Left half cropped away.
        let cropped = b.clip("V2", 60, 30, "blue.mp4", 0);
        b.set(&cropped, "crop_left", json!(320));
        // Half see-through.
        let faint = b.clip("V2", 90, 30, "blue.mp4", 0);
        b.set(&faint, "opacity", json!(50));
        // Grey, and moved up so the red below still shows.
        let grey = b.clip("V2", 120, 30, "blue.mp4", 0);
        b.e.call_tool("t", "clip_add_effect", json!({"clip": grey, "effect": "Greyscale"})).unwrap();
        b.set(&grey, "pos_y", json!(180));

        let (file, plan) = b.export("look.mp4", 30.0, &[]);
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        expect(&file, 10, 480, 180, BLUE);
        expect(&file, 10, 300, 180, RED);
        expect(&file, 10, 480, 60, RED);
        expect(&file, 40, 480, 60, BLUE);
        expect(&file, 40, 370, 180, RED);
        expect(&file, 40, 585, 180, RED);
        expect(&file, 70, 160, 180, RED);
        expect(&file, 70, 480, 180, BLUE);
        expect(&file, 100, 320, 180, [128, 0, 128]);
        // CSS greyscale of pure blue is 7% grey.
        expect(&file, 130, 320, 90, [18, 18, 18]);
        expect(&file, 130, 320, 270, RED);
    }

    #[test]
    fn transitions_text_and_blending() {
        let Some(mut b) = Bench::new("moves") else { return };
        b.clip("V1", 0, 30, "red.mp4", 0);
        let dissolved = b.clip("V1", 30, 30, "blue.mp4", 0);
        b.transition(&dissolved, "Cross dissolve", "in");
        let pushed = b.clip("V1", 60, 30, "red.mp4", 0);
        b.transition(&pushed, "Push left", "in");
        let wiped = b.clip("V1", 90, 30, "blue.mp4", 0);
        b.transition(&wiped, "Wipe right", "out");
        // Screen blending red over blue gives magenta.
        b.clip("V1", 120, 30, "blue.mp4", 0);
        let screened = b.clip("V2", 120, 30, "red.mp4", 0);
        b.set(&screened, "blend", json!("Screen"));
        // The second span says where its pixels are, as the window does; the first leaves it to the whole frame.
        let spans = vec![TextSpan { from: 10, to: 20, image: "text.png".into(), rect: None }, TextSpan { from: 100, to: 130, image: "text.png".into(), rect: Some([269, 131, 101, 99]) }];
        let text = [TextTrack { track: "V3".into(), spans }];

        let (file, _) = b.export("moves.mp4", 30.0, &text);
        // Halfway through the dissolve, the old picture is still under the new one.
        expect(&file, 40, 320, 180, [128, 0, 128]);
        expect(&file, 55, 320, 180, BLUE);
        // Halfway through the push the old blue fills the left half and the new red the right.
        expect(&file, 70, 100, 180, BLUE);
        expect(&file, 70, 540, 180, RED);
        expect(&file, 85, 100, 180, RED);
        // A wipe on the way out leaves black behind it from the right.
        expect(&file, 95, 600, 180, BLUE);
        expect(&file, 110, 100, 180, BLUE);
        expect(&file, 110, 540, 180, [0, 0, 0]);
        expect(&file, 135, 100, 180, [255, 0, 255]);
        // Text shows exactly on its frames, over everything.
        expect(&file, 9, 320, 180, RED);
        expect(&file, 10, 320, 180, GREEN);
        expect(&file, 19, 320, 180, GREEN);
        expect(&file, 20, 320, 180, RED);
        expect(&file, 125, 320, 180, GREEN);
        expect(&file, 125, 100, 180, [255, 0, 255]);
    }

    #[test]
    fn settings_are_checked_and_a_new_frame_rate_keeps_time() {
        let mut e = Editor::default();
        e.call_tool("t", "media_import", json!({"path": "C:/a.mp4", "kind": "video", "duration": 900})).unwrap();
        let media = e.project().media[0].id.clone();
        for (start, source) in [(0, 0), (45, 300)] {
            e.call_tool("t", "timeline_add_clip", json!({"track": "V1", "start": start, "duration": 45, "name": "x", "kind": "video", "media": media, "source_in": source})).unwrap();
        }
        e.call_tool("t", "timeline_add_marker", json!({"at": 30, "label": "m"})).unwrap();
        let after = e.call_tool("t", "project_set_settings", json!({"fps": 60})).unwrap();
        assert_eq!((after["fps"].clone(), after["duration_frames"].clone()), (json!(60.0), json!(180)));
        let p = e.project();
        let spans: Vec<(i64, i64, i64)> = p.sequence.clips.iter().map(|c| (c.start, c.duration, c.source_in)).collect();
        assert_eq!(spans, [(0, 90, 0), (90, 90, 600)]);
        assert_eq!((p.media[0].duration, p.sequence.markers[0].at), (1800, 60));
        e.call_tool("t", "history_undo", json!({})).unwrap();
        assert_eq!(e.project().duration(), 90);
        e.call_tool("t", "project_set_settings", json!({"fps": 29.97})).unwrap();
        assert_eq!((e.project().settings.fps_num, e.project().settings.fps_den), (30000, 1001));
        assert!(e.call_tool("t", "project_set_settings", json!({"width": 641})).is_err());

        let s = |path: &str, width, height| ExportSettings { quality: Quality::Standard, ..ExportSettings::new(path, width, height, 30.0) };
        let p = e.project();
        assert!(check_export(p, &s("out.mp4", 1920, 1080)).unwrap_err().contains("full path"));
        assert!(check_export(p, &s("C:/out.avi", 1920, 1080)).unwrap_err().contains(".mp4"));
        assert!(check_export(p, &s("C:/out.mp4", 1080, 1920)).unwrap_err().contains("different shape"));
        assert!(check_export(p, &s("C:/out.mp4", 1280, 720)).unwrap_err().contains("is missing"));
        assert!(check_export(&Project::new("empty"), &s("C:/out.mp4", 1280, 720)).unwrap_err().contains("nothing on the timeline"));
        let sizes: Vec<(u32, u32)> = export_presets(p).iter().map(|x| (x.width, x.height)).collect();
        assert_eq!(sizes, [(1920, 1080), (1920, 1080), (1920, 1080), (2560, 1440), (2560, 1440), (3840, 2160), (3840, 2160), (1920, 1080), (1280, 720)]);
    }

    /// The YouTube presets carry YouTube's recommended bitrates, in H.264 with variable bitrate.
    #[test]
    fn youtube_presets_use_youtubes_bitrates() {
        let presets = export_presets(&Project::new("presets"));
        let find = |id: &str| presets.iter().find(|x| x.id == id).unwrap().clone();
        let p30 = find("youtube_1080p30");
        assert_eq!((p30.codec, p30.rate, p30.bitrate_kbps, p30.max_kbps), (Codec::H264, RateMode::Vbr, 8_000, 12_000));
        assert_eq!(find("youtube_1080p60").bitrate_kbps, 12_000);
        assert_eq!(find("youtube_1440p60").bitrate_kbps, 24_000);
        assert_eq!(find("youtube_2160p30").bitrate_kbps, 40_000);
        assert_eq!(find("draft_720p").rate, RateMode::Quality);
    }

    /// Each encoder's own names for the rate modes, the keyframe rule, the B-frames and the H.265 tag.
    #[test]
    fn encoder_arguments_follow_the_settings() {
        use crate::export::encoder_args;
        let has = |args: &[String], pair: &[&str]| args.windows(pair.len()).any(|w| w.iter().map(String::as_str).eq(pair.iter().copied()));
        let mut s = ExportSettings::new("C:/out.mp4", 1920, 1080, 30.0);
        s.rate = RateMode::Vbr;
        s.bitrate_kbps = 8_000;
        s.max_kbps = 12_000;
        let vbr = encoder_args("libx264", &s).unwrap();
        assert!(has(&vbr, &["-b:v", "8000k"]) && has(&vbr, &["-maxrate", "12000k"]) && has(&vbr, &["-bufsize", "24000k"]), "{vbr:?}");
        assert!(has(&vbr, &["-g", "15"]) && has(&vbr, &["-bf", "2"]), "half the frame rate and two B-frames: {vbr:?}");
        s.keyframe_frames = 60;
        assert!(has(&encoder_args("libx264", &s).unwrap(), &["-g", "60"]));
        s.rate = RateMode::Cbr;
        let cbr = encoder_args("h264_nvenc", &s).unwrap();
        assert!(has(&cbr, &["-rc", "cbr"]) && has(&cbr, &["-b:v", "8000k"]), "{cbr:?}");
        s.codec = Codec::Hevc;
        let hevc = encoder_args("hevc_nvenc", &s).unwrap();
        assert!(has(&hevc, &["-tag:v", "hvc1"]), "{hevc:?}");
        s.rate = RateMode::Quality;
        s.quality = Quality::High;
        assert!(has(&encoder_args("libx265", &s).unwrap(), &["-crf", "22"]));
        assert!(encoder_args("vp9", &s).is_err());
    }

    /// Settings that cannot make a sensible file are refused before anything is rendered.
    #[test]
    fn export_settings_are_checked() {
        let mut e = Editor::default();
        e.call_tool("t", "project_set_settings", json!({"width": 640, "height": 360})).unwrap();
        let base = ExportSettings::new("C:/out.mp4", 640, 360, 30.0);
        let refused = |change: fn(&mut ExportSettings)| {
            let mut s = base.clone();
            change(&mut s);
            check_export(e.project(), &s).unwrap_err()
        };
        assert!(refused(|s| s.encoder = "hevc_nvenc".into()).contains("makes H.265"));
        assert!(refused(|s| s.encoder = "vp9".into()).contains("unknown encoder"));
        assert!(refused(|s| {
            s.rate = RateMode::Vbr;
            s.bitrate_kbps = 50;
        })
        .contains("between 100"));
        assert!(refused(|s| {
            s.rate = RateMode::Vbr;
            s.bitrate_kbps = 8_000;
            s.max_kbps = 4_000;
        })
        .contains("maximum bitrate"));
        assert!(refused(|s| s.two_pass = true).contains("variable bitrate"));
        assert!(refused(|s| {
            s.rate = RateMode::Vbr;
            s.two_pass = true;
            s.encoder = "h264_nvenc".into();
        })
        .contains("software encoder"));
        assert!(refused(|s| s.b_frames = 5).contains("B-frames"));
        assert!(refused(|s| s.audio_kbps = 10).contains("audio bitrate"));
    }

    /// Bitrate exports land near their target. The sound is 384 kbps AAC, which adds to the file.
    #[test]
    fn bitrate_exports_land_near_their_target() {
        let Some(mut b) = Bench::new("bitrate") else { return };
        b.clip("V1", 0, 60, "steps.mp4", 0);
        let (cbr, _) = b.export_with("cbr.mp4", 30.0, &[], "libx264", |s| {
            s.rate = RateMode::Cbr;
            s.bitrate_kbps = 2_000;
        });
        let (two, _) = b.export_with("two.mp4", 30.0, &[], "libx264", |s| {
            s.rate = RateMode::Vbr;
            s.two_pass = true;
            s.bitrate_kbps = 2_000;
            s.max_kbps = 3_000;
        });
        // Two seconds of picture at the target, plus two seconds of sound.
        let expected = (2_000.0 + 384.0) * 2.0 * 1000.0 / 8.0;
        let cbr_size = std::fs::metadata(&cbr).unwrap().len() as f64;
        assert!((cbr_size / expected - 1.0).abs() < 0.2, "constant bitrate wrote {cbr_size} bytes, expected about {expected}");
        let two_size = std::fs::metadata(&two).unwrap().len() as f64;
        assert!(two_size < (3_000.0 + 384.0) * 2.0 * 1000.0 / 8.0 * 1.2, "two passes wrote {two_size} bytes, above the maximum");
    }

    /// An H.265 export is an MP4 tagged hvc1, so Apple's players read it, and it passes the playback check.
    #[test]
    fn h265_exports_play() {
        let Some(mut b) = Bench::new("hevc") else { return };
        b.clip("V1", 0, 30, "red.mp4", 0);
        let (file, _) = b.export_with("hevc.mp4", 30.0, &[], "libx265", |s| s.codec = Codec::Hevc);
        let out = Command::new(ffmpeg_path()).args(["-hide_banner", "-i", &file.display().to_string()]).output().unwrap();
        let log = String::from_utf8_lossy(&out.stderr);
        assert!(log.contains("Video: hevc") && log.contains("hvc1"), "{log}");
    }

    /// What FFmpeg reports about a file: its streams, and the encoder that wrote it.
    fn ffmpeg_report(file: &Path) -> String {
        let out = Command::new(ffmpeg_path()).args(["-hide_banner", "-i", &file.display().to_string()]).output().unwrap();
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// The graphics card's encoder makes the file, not the processor. The file records the encoder that wrote it, so the
    /// test checks that. It is skipped unless this FFmpeg has a working NVENC encoder for the driver on this computer.
    #[test]
    fn exports_use_the_graphics_card_encoder() {
        let Some(mut b) = Bench::new("nvenc") else { return };
        let found = crate::available_encoders();
        b.clip("V1", 0, 60, "steps.mp4", 0);
        if found.iter().any(|e| e == "h264_nvenc") {
            let (file, _) = b.export_with("nvenc-h264.mp4", 30.0, &[], "h264_nvenc", |s| {
                s.rate = RateMode::Vbr;
                s.bitrate_kbps = 2_000;
                s.max_kbps = 3_000;
            });
            let report = ffmpeg_report(&file);
            assert!(report.contains("h264_nvenc"), "the picture was not written by the card's encoder: {report}");
        } else {
            eprintln!("skipped H.264 on the card: no working h264_nvenc here");
        }
        if found.iter().any(|e| e == "hevc_nvenc") {
            let (file, _) = b.export_with("nvenc-hevc.mp4", 30.0, &[], "hevc_nvenc", |s| {
                s.codec = Codec::Hevc;
                s.rate = RateMode::Cbr;
                s.bitrate_kbps = 2_000;
            });
            let report = ffmpeg_report(&file);
            assert!(report.contains("hevc_nvenc") && report.contains("hvc1"), "{report}");
        } else {
            eprintln!("skipped H.265 on the card: no working hevc_nvenc here");
        }
    }

    /// Optional: a 20-second cut from a real recording, set with SNN_SAMPLE_VIDEO to its path. It uses the card's encoder
    /// when there is one, and the processor's otherwise. Without the variable the test does nothing.
    #[test]
    fn real_footage_exports() {
        let Some(sample) = std::env::var_os("SNN_SAMPLE_VIDEO").map(PathBuf::from).filter(|p| p.is_file()) else { return };
        let Some(mut b) = Bench::new("footage") else { return };
        b.e.call_tool("t", "media_import", json!({"path": sample.display().to_string()})).unwrap();
        let name = b.e.project().media.last().unwrap().name.clone();
        // Twenty seconds, starting one minute into the recording.
        b.clip("V1", 0, 600, &name, 1_800);
        let encoder = if crate::available_encoders().iter().any(|e| e == "h264_nvenc") { "h264_nvenc" } else { "libx264" };
        let (file, _) = b.export_with("footage.mp4", 30.0, &[], encoder, |s| {
            s.rate = RateMode::Vbr;
            s.bitrate_kbps = 8_000;
            s.max_kbps = 12_000;
        });
        let report = ffmpeg_report(&file);
        assert!(report.contains(encoder), "{report}");
        eprintln!("real footage exported with {encoder}: {} bytes", std::fs::metadata(&file).unwrap().len());
    }

    /// The check that FFmpeg can export must agree with its version: 7.0 and newer have the `-/filter_complex` option.
    #[test]
    fn export_support_matches_the_build() {
        let version = ffmpeg_version();
        if version.is_empty() {
            eprintln!("skipped: FFmpeg is not installed");
            return;
        }
        let major: u32 = version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
        assert_eq!(supports_export(), major >= 7, "FFmpeg {version}");
    }

    /// Every transition and effect in the library must come out as filters FFmpeg accepts.
    #[test]
    fn everything_in_the_library_renders() {
        let Some(mut b) = Bench::new("library") else { return };
        let lib = library();
        let mut at = 0;
        for (i, t) in lib.transitions.iter().enumerate() {
            let clip = b.clip("V1", at, 16, if i % 2 == 0 { "red.mp4" } else { "blue.mp4" }, 0);
            b.e.call_tool("t", "clip_set_transition", json!({"clip": clip, "transition": t.name, "duration": 6})).unwrap();
            b.e.call_tool("t", "clip_set_transition", json!({"clip": clip, "transition": t.name, "duration": 6, "edge": "out"})).unwrap();
            at += 16;
        }
        let transitions_end = at;
        for fx in &lib.effects {
            let clip = b.clip("V1", at, 6, "steps.mp4", 20);
            b.e.call_tool("t", "clip_add_effect", json!({"clip": clip, "effect": fx.name})).unwrap();
            at += 6;
        }
        // An inset with everything on it at once, tilted, flipped and faded.
        let inset = b.clip("V2", transitions_end, 30, "blue.mp4", 0);
        for fx in ["Rounded corners", "Border", "Drop shadow", "Vignette", "Shake", "Zoom punch"] {
            b.e.call_tool("t", "clip_add_effect", json!({"clip": inset, "effect": fx})).unwrap();
        }
        let props = [("zoom_x", json!(0.4)), ("zoom_y", json!(0.4)), ("pos_x", json!(-150)), ("pos_y", json!(60)), ("pitch", json!(20)), ("yaw", json!(-25))];
        let more = [("flip_h", json!(true)), ("anchor_x", json!(40)), ("crop_top", json!(30)), ("opacity", json!(80)), ("blend", json!("Hue"))];
        let sound = [("volume", json!(-6)), ("fade_in", json!(0.3)), ("fade_out", json!(0.3)), ("speed", json!(50))];
        for (key, value) in props.into_iter().chain(more).chain(sound) {
            b.set(&inset, key, value);
        }
        let (file, plan) = b.export("library.mp4", 30.0, &[]);
        assert_eq!(length(&file).0, at);
        assert_eq!(plan.warnings, ["Blend mode Hue is drawn as Normal in the export."]);
        // The glitch and whoosh clips are back to a plain picture once their transitions are over.
        expect(&file, 8, 320, 180, RED);
    }
}

#[test]
fn a_video_with_sound_can_back_an_audio_clip_and_a_silent_one_cannot() {
    let mut e = Editor::default();
    let media = |e: &mut Editor, path: &str, has_audio: bool| {
        let out = e
            .execute("user", cmd(json!({"tool": "media.import", "args": {"path": path, "kind": "video", "duration": 300, "has_audio": has_audio}})))
            .unwrap();
        out["media"]["id"].as_str().unwrap().to_string()
    };
    let loud = media(&mut e, "C:/clips/loud.mp4", true);
    let silent = media(&mut e, "C:/clips/silent.mp4", false);
    let audio_track = e.project().sequence.tracks.iter().find(|t| t.kind == TrackKind::Audio).unwrap().id.clone();
    let add = |media: &str| {
        cmd(json!({"tool": "timeline.add_clip", "args": {"track": audio_track, "start": 0, "duration": 100, "name": "x", "kind": "audio", "media": media}}))
    };
    assert!(e.execute("user", add(&loud)).is_ok(), "the sound of a video with sound must be placeable");
    let err = e.execute("user", add(&silent)).unwrap_err();
    assert!(err.contains("no sound track"), "got: {err}");
}

fn add_clip(e: &mut Editor, track: &str, start: i64, duration: i64, kind: &str, link_to: Option<&str>) -> String {
    let mut args = json!({"track": track, "start": start, "duration": duration, "name": kind, "kind": kind});
    if let Some(other) = link_to {
        args["link_to"] = json!(other);
    }
    let r = e.execute("user", cmd(json!({"tool": "timeline.add_clip", "args": args}))).unwrap();
    r["clip"].as_str().unwrap().to_string()
}

/// A video on V1 and the sound taken from it on A1, linked, both 0 to 30.
fn pair(e: &mut Editor) -> (String, String) {
    let v = add_clip(e, "V1", 0, 30, "video", None);
    let a = add_clip(e, "A1", 0, 30, "audio", Some(&v));
    (v, a)
}

fn link_of(e: &Editor, id: &str) -> Option<String> {
    e.project().clip(id).unwrap().link.clone()
}

#[test]
fn a_linked_pair_moves_in_time_and_each_half_can_change_track() {
    let mut e = Editor::default();
    let (v, a) = pair(&mut e);
    assert!(link_of(&e, &v).is_some() && link_of(&e, &v) == link_of(&e, &a));

    // Dragging the video up to V2 moves the pair in time; the sound stays on its own track.
    let moved = json!({"clips": [v, a], "delta": 60, "tracks": {v.clone(): "V2"}});
    e.execute("user", cmd(json!({"tool": "timeline.move_clips", "args": moved}))).unwrap();
    let (vc, ac) = (e.project().clip(&v).unwrap().clone(), e.project().clip(&a).unwrap().clone());
    assert_eq!((vc.track.as_str(), vc.start), ("V2", 60));
    assert_eq!((ac.track.as_str(), ac.start), ("A1", 60));
    assert_eq!(vc.link, ac.link, "the link survives a track change");
    assert_no_overlaps(&e);

    // Moving the sound alone to another audio track keeps it linked to the video.
    e.execute("user", cmd(json!({"tool": "timeline.move_clip", "args": {"clip": a, "start": 60, "track": "A3"}}))).unwrap();
    assert_eq!(e.project().clip(&a).unwrap().track, "A3");
    assert_eq!(link_of(&e, &v), link_of(&e, &a));

    // The whole move is one undo step, and the track change goes with it.
    e.undo("user").unwrap();
    assert_eq!(e.project().clip(&a).unwrap().track, "A1");
}

#[test]
fn a_track_change_must_fit_the_clip_and_be_free() {
    let mut e = Editor::default();
    let (v, a) = pair(&mut e);
    add_clip(&mut e, "V2", 60, 30, "video", None);
    let try_move = |e: &mut Editor, clips: serde_json::Value, delta: i64, tracks: serde_json::Value| {
        e.execute("user", cmd(json!({"tool": "timeline.move_clips", "args": {"clips": clips, "delta": delta, "tracks": tracks}})))
    };
    // Sound cannot go on a video track.
    assert!(try_move(&mut e, json!([v, a]), 0, json!({ a.clone(): "V2" })).unwrap_err().contains("cannot go on track V2"));
    // The video would land on the clip already on V2 at frame 60.
    assert!(try_move(&mut e, json!([v, a]), 60, json!({ v.clone(): "V2" })).unwrap_err().contains("overlap"));
    // A track can only be given for a clip that is being moved.
    assert!(try_move(&mut e, json!([a]), 0, json!({ v.clone(): "V2" })).unwrap_err().contains("not in the list"));
    assert_eq!(e.project().clip(&v).unwrap().track, "V1", "a failed move changes nothing");
}

#[test]
fn clips_can_swap_tracks_in_one_move() {
    let mut e = Editor::default();
    let x = add_clip(&mut e, "V1", 0, 30, "video", None);
    let y = add_clip(&mut e, "V2", 0, 30, "video", None);
    // Each lands where the other was. Checking against the old places would refuse this.
    let swap = json!({"clips": [x, y], "delta": 0, "tracks": {x.clone(): "V2", y.clone(): "V1"}});
    e.execute("user", cmd(json!({"tool": "timeline.move_clips", "args": swap}))).unwrap();
    assert_eq!(e.project().clip(&x).unwrap().track, "V2");
    assert_eq!(e.project().clip(&y).unwrap().track, "V1");
    // Two moved clips that end up on the same spot are refused.
    let clash = json!({"clips": [x, y], "delta": 0, "tracks": {x.clone(): "V1", y.clone(): "V1"}});
    assert!(e.execute("user", cmd(json!({"tool": "timeline.move_clips", "args": clash}))).unwrap_err().contains("overlap"));
}

#[test]
fn link_and_unlink_group_clips_and_unlinking_nothing_is_harmless() {
    let mut e = Editor::default();
    let v = add_clip(&mut e, "V1", 0, 30, "video", None);
    let a = add_clip(&mut e, "A1", 0, 30, "audio", None);
    let link = |ids: &[&str]| cmd(json!({"tool": "timeline.link", "args": {"clips": ids}}));
    assert!(e.execute("user", link(&[v.as_str()])).unwrap_err().contains("at least two"));
    e.execute("user", link(&[v.as_str(), a.as_str()])).unwrap();
    assert!(link_of(&e, &v).is_some() && link_of(&e, &v) == link_of(&e, &a));

    // Unlinking one clip breaks the whole group; both then stand alone.
    let unlinked = e.execute("user", cmd(json!({"tool": "timeline.unlink", "args": {"clips": [a]}}))).unwrap();
    assert_eq!(unlinked["unlinked"].as_array().unwrap().len(), 2);
    assert!(link_of(&e, &v).is_none() && link_of(&e, &a).is_none());

    // Nothing is linked any more, so unlinking again changes nothing and is not an error.
    let again = e.execute("user", cmd(json!({"tool": "timeline.unlink", "args": {"clips": [a]}}))).unwrap();
    assert_eq!(again["unlinked"], json!([]));
}

#[test]
fn a_new_clip_can_join_an_existing_link() {
    let mut e = Editor::default();
    let (v, a) = pair(&mut e);
    let second = add_clip(&mut e, "A2", 0, 30, "audio", Some(&v));
    assert_eq!(link_of(&e, &second), link_of(&e, &v));
    assert_eq!(link_of(&e, &a), link_of(&e, &v), "the existing partner stays linked");
    let err = e
        .execute("user", cmd(json!({"tool": "timeline.add_clip", "args": {"track": "A3", "start": 0, "duration": 5, "name": "x", "kind": "audio", "link_to": "c99"}})))
        .unwrap_err();
    assert!(err.contains("no clip c99"), "{err}");
}

#[test]
fn split_and_copy_give_each_part_its_own_pair() {
    let mut e = Editor::default();
    let (v, a) = pair(&mut e);
    let group = link_of(&e, &v).unwrap();

    // Cutting both halves at frame 10 makes two new pairs: the left halves keep the old one, the right halves share a new one.
    let cut = e.execute("user", cmd(json!({"tool": "timeline.split_clip", "args": {"at": 10, "clips": [v, a]}}))).unwrap();
    let created: Vec<String> = cut["created"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
    assert_eq!(link_of(&e, &v), Some(group.clone()));
    assert_eq!(link_of(&e, &a), Some(group.clone()));
    let right = link_of(&e, &created[0]).unwrap();
    assert_ne!(right, group);
    assert_eq!(link_of(&e, &created[1]), Some(right.clone()));

    // Copies of the left halves form a pair of their own, not a pair with the originals.
    let copy = e.execute("user", cmd(json!({"tool": "timeline.duplicate_clips", "args": {"clips": [v, a], "delta": 100}}))).unwrap();
    let copies: Vec<String> = copy["created"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
    let copied = link_of(&e, &copies[0]).unwrap();
    assert_eq!(link_of(&e, &copies[1]), Some(copied.clone()));
    assert!(copied != group && copied != right);
}

#[test]
fn projects_saved_before_links_pair_video_with_its_sound() {
    let media = MediaItem {
        id: "media1".into(),
        name: "a.mp4".into(),
        path: "C:/a.mp4".into(),
        kind: MediaKind::Video,
        duration: 300,
        width: 1920,
        height: 1080,
        has_audio: true,
    };
    let clip = |id: &str, track: &str, start: i64, duration: i64, kind: ClipKind| Clip {
        id: id.into(),
        track: track.into(),
        start,
        duration,
        source_in: 0,
        name: id.into(),
        kind,
        media: Some("media1".into()),
        text: None,
        template: None,
        props: Default::default(),
        effects: vec![],
        transition_in: None,
        transition_out: None,
        last_actor: None,
        link: None,
    };
    let mut old = Project::new("old");
    old.version = 1;
    old.media.push(media);
    old.sequence.clips = vec![
        clip("v1", "V1", 0, 100, ClipKind::Video),
        clip("a1", "A1", 0, 100, ClipKind::Audio),
        // The sound was moved on its own in the old version, so it no longer matches and is not paired.
        clip("v2", "V2", 200, 50, ClipKind::Video),
        clip("a2", "A2", 210, 50, ClipKind::Audio),
    ];
    // Written as a version-1 file, which has no link field in its clips.
    let mut value = serde_json::to_value(&old).unwrap();
    for c in value["sequence"]["clips"].as_array_mut().unwrap() {
        c.as_object_mut().unwrap().remove("link");
    }
    let path = std::env::temp_dir().join(format!("snn-core-legacy-{}.snn", std::process::id()));
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

    let mut e = Editor::default();
    e.open(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(e.project().version, PROJECT_VERSION);
    assert!(link_of(&e, "v1").is_some() && link_of(&e, "v1") == link_of(&e, "a1"));
    assert!(link_of(&e, "v2").is_none() && link_of(&e, "a2").is_none());
}

/// The caption read tool lists caption text in time order, with frames and seconds, and can be limited to a span.
#[test]
fn captions_read_lists_caption_text_in_time_order() {
    let mut e = Editor::default();
    e.call_tool("t", "timeline_add_clip", json!({"track": "T1", "start": 60, "duration": 30, "name": "second", "kind": "caption", "text": "Second line"})).unwrap();
    e.call_tool("t", "timeline_add_clip", json!({"track": "T1", "start": 0, "duration": 30, "name": "first", "kind": "caption", "text": "First line"})).unwrap();
    let all = e.call_tool("t", "captions_read", json!({})).unwrap();
    let captions = all["captions"].as_array().unwrap();
    assert_eq!(captions.len(), 2);
    assert_eq!(captions[0]["text"], json!("First line"));
    assert_eq!(captions[1]["text"], json!("Second line"));
    assert_eq!(captions[1]["start_frame"], json!(60));
    assert_eq!(captions[1]["end_frame"], json!(90));
    let fps = all["fps"].as_f64().unwrap();
    assert!((captions[1]["start_seconds"].as_f64().unwrap() - 60.0 / fps).abs() < 1e-9);
    let span = e.call_tool("t", "captions_read", json!({"from": 50, "to": 100})).unwrap();
    assert_eq!(span["captions"].as_array().unwrap().len(), 1);
    assert_eq!(span["captions"][0]["text"], json!("Second line"));
}
