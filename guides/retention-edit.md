# Retention edit: raw recording to a video people keep watching

The full method for turning a long raw gameplay recording into a 15 to 25 minute YouTube video. It calls on the other guides for each pass: `dead-air`, `cold-open` and `pacing-check`. Read the channel guide too if there is one.

## Where the numbers come from

They come from a frame-by-frame study of 30 Turkish Minecraft videos (first 5 minutes of each) and an earlier set of 60. The study measured picture changes, cards and structure. It did not measure sound, and it had no view counts for the 30, so the numbers describe what successful channels do, not what is proven to cause views. Treat them as targets. A real fight or a real reaction beats any target.

## What you can and cannot know

You can read the timeline. You cannot watch the footage or hear it unless `tools/list` shows analysis tools (transcript, silence, frames). So before judging content, get an inventory from one of these, best first:

1. Analysis tools, if the server offers them.
2. A list from the user: time ranges with a word each, such as `12:40–13:05 boss enrage`, `31:10 death`, `40:00–46:30 walking, nothing`.
3. Markers the user dropped while watching. Ask what they mean if the labels do not say.

Never invent what happens in a part of the recording. If you have no inventory for a stretch, leave it alone and put a marker on it.

Record every moment as a **source position** (media id and source frame), not a timeline frame. Timeline frames move with every cut; source positions never do. For a timeline frame inside a clip: `source = clip.source_in + (frame - clip.start) × speed / 100`.

Sort the inventory into four lists:

- **Peaks**: the wildest results, boss moments, big wins. The cold open is built from these.
- **Turns**: each goal, new item or mechanic, first try, result. The body is built from these.
- **Keep as is**: deaths, mistakes, real reactions, fights.
- **Dead**: silence, walking, waiting, menus with no purpose, repeated attempts.

## Before the first edit

- `project_get_info` for the frame rate, then `timeline_get_state`. All tool times are frames.
- Ask the user to save the project. There is no save tool, and a long pass is lost if the app closes.
- Name the tracks: gameplay on the lowest video track; insets above it; cards and counters on top; voice, music and sound effects on separate audio tracks.
- Do not start if the timeline already has music, cards or captions laid against the picture and you plan to cut time out. See the order below.

## Order of passes

The order matters because removing time moves everything after it, on every unlocked track.

1. **Dead air** (`dead-air`). Cut first, while the timeline is only the recording.
2. **Shape the body.** The body runs on a loop of 40 to 60 seconds: goal, item or recipe, first try, result. Keep each turn, cut what sits between turns. Keep the best of repeated attempts. Something new every 45 to 60 seconds. Never five minutes of one kind of scene. Aim for a final length of 15 to 25 minutes, or what the user asked for.
3. **Cold open** (`cold-open`). Build it in front of the body from copies of the peaks.
4. **Cards and counters.** A chapter card or counter update at each new stage. A counter stays on screen while its number matters. A second subscribe card around 1:25 is common. A sponsor segment, if any, goes after the first small peak (about 3:30), never in the first minute.
5. **Captions.** One to three words at a time, large, white, thick outline, bottom centre. Without a transcript you can only caption lines the user gives you.
6. **Accents.** A zoom punch on a reaction or a reveal, a whoosh transition on teaser cuts, a small greyscale inset for a flashback two to four times a video. Hold an item tooltip or recipe screen about 2 seconds, then cut to the item in use. Cut a death screen tight and keep the reaction after it.
7. **Pacing check** (`pacing-check`). Measure, fix the slow minutes, report.
8. **Hand over.** Put a marker wherever you were unsure, show the user the opening with `view_set`, and say in plain words what you changed and what you could not check.

## Rules for the tools

- Group related edits in `batch_apply`: one undo step for the user, and the undo history only holds 200 steps.
- When removing several ranges, send them **last to first**. Each removal shifts everything after it, so earlier-first makes every later range wrong.
- `timeline_ripple_delete_range` cuts every unlocked track. Lock a track to protect it, and remember a locked track then falls out of sync with the rest.
- `timeline_move_clips` does not move markers. After shifting the body to make room for an intro, re-place the markers.
- Read with `from`/`to` once the timeline has many pieces; the whole state gets large.
- `history_undo` undoes the newest change, whoever made it. Check `history_list` first and only undo your own.
- Never call `project_new`.
- Effects, speed and transitions are stored exactly, but the preview is an interim one. Tell the user to look at anything that depends on how an effect renders.
