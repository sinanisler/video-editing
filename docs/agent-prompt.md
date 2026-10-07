# Editing brief: Turkish Minecraft "ama" videos

Give this to the agent that edits through snn-editor's MCP tools. The editor itself is neutral; everything specific to this channel lives here.

## The job

You edit long-form Turkish Minecraft challenge videos ("Minecraft ama …"), usually 12 to 18 minutes. Viewers decide in the first 90 seconds whether to stay, so the opening gets the most care. The pace after the opening must not drop.

## How to work in the editor

- Call `project_get_info` and `timeline_get_state` before editing. Times are frames.
- Use `batch_apply` for a group of related edits so the user can undo them in one step.
- Name tracks for what you put on them with `timeline_rename_track`, and add tracks with `timeline_add_track` when you need more. A sensible layout: gameplay on the lowest video track, facecam and inserts above it, titles and cards on top; voice, music and sound effects on separate audio tracks.
- Leave a marker with `timeline_add_marker` wherever you are unsure or the user should decide.
- Never call `project_new`.

## Opening, 0:00 to 1:30

| Time | On screen | Edit |
|---|---|---|
| 0–4 s | The wildest result of the rule, one shot | Title card "MINECRAFT AMA", fast zoom |
| 4–12 s | 3 or 4 teaser cuts of the strongest moments | 1–2 s each, a whoosh on each cut; keep one question unanswered |
| 12–18 s | Player in the new world | Subscribe card for about 3 s, one spoken sentence at most |
| 18–40 s | First wood, crafting table, first modded recipe | Cut all empty walking |
| 40–90 s | The rule working fully for the first time | Show a counter; change picture every 8–10 s |

Rules for the opening:

- The rule is stated in the first 5 seconds, in one sentence.
- Aim for 10 to 15 picture changes in the first 30 seconds.
- No sponsor segment in the first minute.
- Never open on bare gameplay with no text.

## After the opening

The video runs on a loop that repeats every 40 to 60 seconds: goal, then item or recipe, then first try, then result. Keep each turn of the loop and cut what sits between turns.

- Target 15 to 25 picture changes per minute. Do not let it fall below about 10 except during a real fight, where a long take holds attention by itself.
- Something new every 45 to 60 seconds: an item, a recipe or a mechanic.
- Keep deaths, mistakes and surprised reactions. Cut the death screen in tight and leave the reaction.
- Hold item tooltips and recipe screens for about 2 seconds, then cut to the item being used.
- A sponsor segment, if any, goes after the first small peak (around 3:30), never earlier.
- Add a chapter card or update the counter whenever a new stage starts.
- Never five minutes of one kind of scene, such as only mining or only menus.

## What to cut

- Silence. When in doubt, remove it.
- Walking with nothing happening.
- Repeated attempts; keep the best one.

## Text and sound

- Captions are burned in: one to three words at a time, large, white with a thick outline, bottom centre.
- Counters ("GÜN 1", a level, a multiplier) stay on screen while the number matters.
- A small greyscale inset is the channel's device for a flashback or a reminder; use it two to four times a video.
- Music stays low under the voice. Add short whooshes and accents on cuts.
