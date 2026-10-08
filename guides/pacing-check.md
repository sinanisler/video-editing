# Pacing check: measure the edit before handing it over

Checks a finished or half-finished edit against the pace of the studied videos and lists what to fix. It changes nothing by itself.

## Measure

Call `timeline_get_pace`. It counts picture changes: every frame where a clip starts on a visible video track, which covers cuts, cards and insets. It returns the count for each minute, the count for the first 30 seconds, and the longest stretches with no change.

The study behind the targets counted camera movement as a change too, so its numbers run a little high. Being slightly under a target is fine.

## Targets

| Where | Target | Act when |
|---|---|---|
| First 30 seconds | 10 to 15 changes | under 8 |
| First minute | about 23 | under 15 |
| Every later minute | 15 to 25 | under 10, unless it is a fight |
| Longest stretch without a change | under 10 s in the opening, under 20 s later | longer, unless it is a fight or a real reaction |
| Whole video | 15 to 25 minutes | far outside what the user asked for |

Pace must not fall after the opening. In the study the first minute averaged 23 changes and minutes two to five stayed at 18 to 19: the fast start is the rhythm of the whole video, not an intro effect.

## Structure, minute by minute

Go through the timeline in order and check:

- 0–5 s: the shot and the caption show what the title and thumbnail promise, and make sense with the sound off.
- 4–12 s: three or four short teaser clips.
- 12–25 s: a subscribe card of about 3 seconds.
- By 0:45: the first loop has started.
- Every 45 to 60 seconds after that: a new item, mechanic, place or result. Use the inventory to judge this; the timeline alone cannot tell you.
- Every new stage: a chapter card or a counter change.
- No five-minute stretch of one kind of scene.
- No sponsor in the first minute.
- The open question from the opening is not answered before the final scene.
- The final scene delivers the result the opening promised, and the video ends within about 20 seconds of it.

## Fix

- Slow minute with dead footage: more cuts (`dead-air`).
- Slow minute with good footage, or talk over a view where nothing moves: add a picture change that carries information, such as an inset of what is being talked about, a counter update, a zoom punch on the reaction, a greyscale inset recalling an earlier moment, or a chapter card. Do not add cuts that only raise the count.
- Too fast to follow (over about 45 changes a minute outside the teaser): let the reactions and tooltips breathe.

## Report

Give the user a short table: minute, changes, verdict. Then list what you fixed, what you marked for them, and what you could not judge without seeing or hearing the footage.
