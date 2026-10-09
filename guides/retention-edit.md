# Retention edit: raw recording to a video people keep watching

The full method for turning a long raw gameplay recording into a 15 to 25 minute YouTube video. It calls on the other guides for each pass: `dead-air`, `cold-open` and `pacing-check`. Read the channel guide too if there is one.

## Where the numbers come from

They come from a frame-by-frame study of 30 Turkish Minecraft videos (first 5 minutes of each) and an earlier set of 60. The study measured picture changes, cards and structure. It did not measure sound, and it had no view counts for the 30, so the numbers describe what successful channels do, not what is proven to cause views. Treat them as targets. A real fight or a real reaction beats any target.

The principles marked **(MrBeast)** come from two interview compilations in which he explains how he holds viewers. They are his stated practice, not measurements, and his videos are big challenge videos, not gameplay. Only the parts that carry over to any video are used here.

## The five principles

1. **The video keeps the promise of its title and thumbnail, at once.** The viewer clicked on one thing. The first 5 seconds show that exact thing, then the video gives more than was promised. Anything else in those seconds reads as a bait and they leave. (MrBeast)
2. **The opening decides the video.** Losing 20% of viewers in the first 30 seconds instead of 35% carries through to the last minute. Spend the most care there, and more on the first half than the second: the longer someone has watched, the more likely they stay, so the back half mainly has to avoid a truly boring stretch. (MrBeast)
3. **One open question runs to the end.** State early what is at stake and what the viewer will find out, and do not answer it before the final scene. A viewer waiting for a result sits through a slow moment. (MrBeast)
4. **Stay on the promise.** No "but first". A lively scene that has nothing to do with the title is still a reason to leave. (MrBeast)
5. **Never the same picture for long.** About 10 seconds of talk over one unchanging view loses people; the same talk with the picture changing every 3 seconds holds them. (MrBeast, and the measured pace of the 30 videos agrees.)

## Look before you cut

You can see the footage, measure its sound, and read what is said. Build an inventory of the recording before the first edit, in this order.

1. **Sound first, because it is cheap.** `audio_get_activity` on the whole file. If the file has more than one audio stream, the microphone is usually on its own; analyse that stream, because voice mixed with game sound is almost never silent. Silences on the voice are candidates for cutting. The loudest moments, well above `typical_lufs`, are shouts and laughter: candidates for the opening.
2. **A wide look.** `frames_contact_sheet` over the recording in stretches of about 10 minutes at 48 tiles, one tile every 12 seconds or so. Note what each stretch is: menu, travel, fight, building, death, new place. This is where a long recording turns into a list of scenes.
3. **A close look where it matters.** Around each loud moment, each scene change and each cut you plan, ask for a sheet over a few seconds. For on-screen text (a tooltip, a boss name, a death message) use `frames_get`, which is large enough to read.
4. **What is said, where it matters.** `speech_transcribe` returns each line of speech with the source frames it is heard at. It costs the user money and takes about a minute for each ten minutes of sound, so do not run it over a whole two-hour recording unasked: transcribe the stretches you are going to keep or cut on (the loud moments, the scenes for the opening, a stretch where the cut must fall between sentences), or ask the user first whether to transcribe everything. Give it the number of the audio stream when the microphone is on its own stream. It fails when no OpenRouter key or speech model is set in the app's AI Settings; tell the user and carry on from pictures and sound.
5. **Ask the user for what the footage cannot tell you**: the title and payoff, and which of two similar attempts was the good one.

A sheet costs about as much to read as a few pages of text. A two-hour recording is around a dozen wide sheets; do not sample it second by second.

Never guess what happens in a stretch you have not looked at. A silence on the voice track can be the tensest moment of a fight, so look at the frames of a silence before cutting it.

Record every moment as a **source position** (media id and source frame), not a timeline frame. Timeline frames move with every cut; source positions never do. Do not convert between the two by hand: `timeline_locate` with `at` gives the source frame under a timeline frame, and with `media` and `source` it gives where that source frame is on the timeline now, or says it has been cut out. `frames_get` on the timeline reports both numbers for the frame it shows.

Sort the inventory into four lists:

- **Peaks**: the wildest results, boss moments, big wins. The cold open is built from these.
- **Turns**: each goal, new item or mechanic, first try, result. The body is built from these.
- **Keep as is**: deaths, mistakes, real reactions, fights.
- **Dead**: silence, walking, waiting, menus with no purpose, repeated attempts.

## Before the first edit

- Ask the user for the planned **title and thumbnail**, and for the **payoff**: the result the viewer waits for. Write both down in one sentence each. Every pass is judged against them. If the user has not decided, ask before building the opening; an opening cannot keep a promise nobody has made.
- `project_get_info` for the frame rate, then `timeline_get_state`. All tool times are frames: frame = seconds × fps, rounded, and a frame is the finest step there is.
- Save before you start: `project_save`. If the project has no file yet (`file` is null in `project_get_info`), ask the user where to save it, because a long pass is lost if the app closes. Save again after each pass.
- Every tool result ends with a `context` block: the user's playhead, their selection, and what anyone else changed since your last call. When the user says "this clip" or "here", that is what they mean. When it lists changes by others, your frame numbers and clip ids may be stale: read the timeline again before the next edit.
- Name the tracks: gameplay on the lowest video track; insets above it; cards and counters on top; voice, music and sound effects on separate audio tracks.
- Do not start if the timeline already has music, cards or captions laid against the picture and you plan to cut time out. See the order below.

## Order of passes

The order matters because removing time moves everything after it, on every unlocked track.

1. **Dead air** (`dead-air`). Cut first, while the timeline is only the recording.
2. **Shape the body.** The body runs on a loop of 40 to 60 seconds: goal, item or recipe, first try, result. Keep each turn, cut what sits between turns. Keep the best of repeated attempts. Something new every 45 to 60 seconds. Never five minutes of one kind of scene. Aim for a final length of 15 to 25 minutes, or what the user asked for. Cut scenes that do not serve the title's promise, however lively. Hold the payoff for the final scene and end soon after it; a long goodbye after the result is where the last viewers leave.
3. **Cold open** (`cold-open`). Build it in front of the body from copies of the peaks.
4. **Cards and counters.** A chapter card or counter update at each new stage. A counter stays on screen while its number matters. A second subscribe card around 1:25 is common. A sponsor segment, if any, goes after the first small peak (about 3:30), never in the first minute.
5. **Captions.** One to three words at a time, large, white, thick outline, bottom centre. Take the words and their timing from `speech_transcribe` on the stretch you are captioning, and place each caption with `timeline_locate`, since the lines come back in source frames. Check names and game terms against the picture: a speech model mishears them.
6. **Accents.** A zoom punch on a reaction or a reveal, a whoosh transition on teaser cuts, a small greyscale inset for a flashback two to four times a video. When the player talks for more than about 5 seconds over a view where nothing changes, put what they are talking about on screen: an inset of the item, the boss or the earlier moment. Hold an item tooltip or recipe screen about 2 seconds, then cut to the item in use. Cut a death screen tight and keep the reaction after it.
7. **Pacing check** (`pacing-check`). Measure, fix the slow minutes, report.
8. **Hand over.** Put a marker wherever you were unsure, show the user the opening with `view_set`, and say in plain words what you changed and what you could not check.
9. **Export, when the user asks for the file.** `export_start` with the `youtube_1080p60` preset for gameplay recorded at 60 fps, then `export_get_status` until it is done. Use `draft_720p` for a quick copy to review. Pass `audio: "all"` when the recording keeps the microphone on its own audio stream, or the voice will be missing. Report any warnings the status lists.

## After the video is published

The retention graph in YouTube Studio shows where viewers left. When the user brings those times, find what was on the timeline at each one, name the cause (slow stretch, off-topic scene, promise answered too early, long talk over one view), and add it to the channel guide under a "Do not repeat" heading. MrBeast's version of this: go through your last 50 videos, write down where everyone clicked off, and stop doing those things.

## Rules for the tools

- Group related edits in `batch_apply`: one undo step for the user, and the undo history only holds 200 steps.
- To remove several ranges, use `timeline_ripple_delete_ranges` with all of them in one call. They are given as the timeline is before the call, in any order, and the tool allows for each removal shifting the rest. With `media` the ranges are source frames, which is what `audio_get_activity` and `speech_transcribe` report, so nothing has to be converted. Only when you send single `timeline_ripple_delete_range` calls yourself must they go **last to first**.
- Ripple deletes cut every unlocked track. Lock a track to protect it, and remember a locked track then falls out of sync with the rest.
- `timeline_move_clips` leaves markers where they are unless you pass `markers: true`. Pass it when shifting the body to make room for an intro.
- Read with `from`/`to` once the timeline has many pieces; the whole state gets large.
- `history_undo` undoes the newest change, whoever made it. Check `history_list` first and only undo your own.
- Never call `project_new`.
- The exported file is the reference for how an edit looks and sounds. The preview is an interim one: it can hitch at cuts and plays only the first audio stream of a file.
- `frames_get` and `frames_contact_sheet` show footage without effects, cards or captions. To check those, `frames_render` returns one timeline frame as the finished video shows it. It takes a few seconds a frame, so use it on the frames that matter: each card, each inset, the first frame of the video. For motion (a transition, a zoom punch), export a draft of the range with `range: "in_out"` and ask the user to look.
- After a pass, `audio_get_activity` without `media` measures the edit as it now plays, in timeline frames. Use it to find the silences a pass left behind.
