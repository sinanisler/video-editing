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
    assert!(e.execute("user", audio).unwrap_err().contains("cannot back"));
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
        if !matches!(tool.name, "media_remove" | "timeline_remove_marker") {
            assert!(result.is_ok(), "{}: {:?}", tool.name, result);
        }
        assert_eq!(e.state().log[0].actor, "agent");
    }
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
    assert_eq!(probe, Probe { kind: MediaKind::Video, seconds: 12.5, width: 1280, height: 720 });
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
