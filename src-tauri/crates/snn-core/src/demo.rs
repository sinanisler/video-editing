use crate::model::*;

/// Test fixture: a sequence shaped like the opening of a "Minecraft ama…" video, with no media behind it.
pub fn demo_project() -> Project {
    let mut p = Project::new("minecraft-ama-ep01");
    let fps = p.settings.clone();
    let mut add = |track: &str, start: f64, dur: f64, name: &str, kind: ClipKind| -> usize {
        let id = p.new_id("c");
        p.sequence.clips.push(Clip {
            id,
            track: track.into(),
            start: fps.frames(start),
            duration: fps.frames(dur),
            source_in: 0,
            name: name.into(),
            kind,
            media: None,
            text: None,
            template: None,
            props: Default::default(),
            effects: vec![],
            transition_in: None,
            transition_out: None,
            last_actor: None,
            link: None,
        });
        p.sequence.clips.len() - 1
    };

    use ClipKind::*;
    add("V1", 0.0, 4.0, "Teaser · OP armor", Video);
    add("V1", 4.0, 2.5, "Teaser · 1000 enchant", Video);
    add("V1", 6.5, 2.5, "Teaser · explosion", Video);
    add("V1", 9.0, 3.0, "Teaser · one block", Video);
    let world = add("V1", 12.0, 6.0, "World start", Video);
    add("V1", 18.0, 22.0, "First craft", Video);
    add("V1", 40.0, 50.0, "First loop · mine", Video);
    let village = add("V1", 90.0, 60.0, "Village", Video);
    add("V2", 18.0, 34.0, "Facecam", Video);
    let flashback = add("V2", 55.0, 5.0, "Flashback", Video);
    let graphics = [
        add("V3", 0.5, 3.5, "Title card", Graphic),
        add("V3", 12.5, 3.0, "Subscribe card", Graphic),
        add("V3", 18.0, 70.0, "Counter", Graphic),
        add("V3", 90.0, 3.0, "Chapter card", Graphic),
    ];
    add("A1", 0.0, 12.0, "voice_01.wav", Audio);
    add("A1", 12.0, 78.0, "voice_01.wav", Audio);
    add("A1", 90.0, 60.0, "voice_01.wav", Audio);
    add("A2", 0.0, 150.0, "upbeat_loop.mp3", Audio);
    for (at, name) in [(4.0, "whoosh"), (6.5, "whoosh"), (9.0, "whoosh"), (12.5, "pop"), (18.0, "ding")] {
        add("A3", at, 1.0, name, Audio);
    }

    let half = fps.frames(0.5);
    let clips = &mut p.sequence.clips;
    clips[world].transition_in = Some(Transition { name: "Dip to black".into(), duration: half });
    clips[village].transition_in = Some(Transition { name: "Cross dissolve".into(), duration: half });
    clips[flashback].effects.push("Greyscale".into());
    for (i, (template, text)) in graphics.into_iter().zip([
        ("title", "MINECRAFT AMA"),
        ("subscribe", "Abone ol"),
        ("counter", "GÜN 1"),
        ("chapter", "2. Adım: Köyü Bul"),
    ]) {
        clips[i].template = Some(template.into());
        clips[i].text = Some(text.into());
    }

    for (at, label) in [(12.0, "Hook ends"), (90.0, "Chapter 2")] {
        let id = p.new_id("m");
        p.sequence.markers.push(Marker { id, at: fps.frames(at), label: label.into(), actor: None });
    }
    p
}
