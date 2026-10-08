# Cold open: the first 90 seconds

Builds the opening from the best moments of the recording, so the viewer knows what the video is and why to stay before the slow start of the real session. Viewers decide in the first 90 seconds; the opening gets the most care.

## The shape

Times are from the start of the video. Convert to frames with the project frame rate.

| Time | Part | Picture | Edit |
|---|---|---|---|
| 0–4 s | Promise | The single wildest result, one shot | Title card over it (three words at most), zoom punch, the rule in one spoken or captioned sentence |
| 4–12 s | Teaser | 3 or 4 peaks | 1 to 2 s each, whoosh on each cut, strongest last |
| 12–18 s | Subscribe | The player at the start of the session | Subscribe card for about 3 s, one sentence at most |
| 18–45 s | Into the game | First steps, first item, first screen that shows the rule | Cut every empty second; tooltip or recipe held about 2 s |
| 45–90 s | First full loop | The rule working for the first time: first reward, first surprise, first danger or death | Counter appears; picture changes every 8 to 10 s at the slowest |

Targets: 10 to 15 picture changes in the first 30 seconds, about 23 in the first minute.

## Choosing the shots

- Take them from the **Peaks** list of the inventory, as source positions.
- The promise shot shows a result, not a setup: the boss enraged, the reward on screen, the huge number.
- Teaser shots differ from each other: a fight, a loss, a reward, a place. Four clips of the same fight read as one.
- Leave one question open. Show the boss waking, not the boss dying. Show the death, not what was lost.
- The teaser is a **copy**. The same moments stay in the body, where they pay off.
- If the user recorded a separate promise scene, use it for 0–4 s.

## Building it

1. Add up the length of the promise and teaser shots. The subscribe card sits over the first seconds of the body, so it needs no room of its own.
2. Shift the whole body right by that length: `timeline_move_clips` with every clip id and a positive `delta`. Markers do not move with it; re-place them.
3. Add each shot on the gameplay track with `timeline_add_clip`: `media`, `source_in` at the peak, the short `duration`. One `batch_apply`.
4. Add the title card and subscribe card on the top video track, captions on the caption track.
5. Add `Zoom punch` on the promise shot and `Whoosh zoom` transitions between teaser shots.
6. Tighten 18–90 s of the body with more cuts until it meets the targets.

## Never

- Open on bare gameplay with no text.
- Explain the rule later than 30 seconds in.
- Talk through setup before anything has happened.
- Put a sponsor in the first minute.
- Make the subscribe call longer than one sentence.

If the inventory has no real peak, say so. A cold open made of ordinary shots promises nothing; ask the user for a better moment or a recorded promise scene.
