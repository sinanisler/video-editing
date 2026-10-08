# Dead air: cut what loses the viewer

Removes the stretches of a raw recording where nothing happens. Do this pass first, before music, cards or captions are on the timeline.

## What to cut

- Silence. When in doubt, remove it; a gap loses viewers faster than a jump cut.
- Walking, sailing or digging with nothing happening and nothing said.
- Waiting: loading screens, smelting, searching through chests or menus without a purpose.
- Repeated attempts. Keep the best one, and the first failure if it is funny.
- Setup talk before the action starts.
- Anything said twice, and explanations longer than the thing needs. Show it working instead.
- Scenes off the promise of the title. A side errand can be full of talk and still be why people leave; "but first" is the phrase to listen for.

Be hardest on the first half. Viewers who have stayed ten minutes mostly keep watching, so a stretch that would be cut at minute 2 can sometimes stay at minute 15.

## What to keep

- Deaths, mistakes and surprised reactions. They are a pacing device in this format: at least 5 of the 30 studied videos show a death screen in the first 5 minutes.
- The second or two **after** a surprise. The reaction is the content; do not cut it off.
- Fights. A real fight holds attention as one long take, so do not chop it to raise the cut count.
- About 2 seconds of each item tooltip or recipe screen. It shows the rule without explaining it.
- The sentence that states the next goal. Each turn of the loop starts with it.

## How to cut

1. Start from the silences `audio_get_activity` reports on the voice, and the stretches the contact sheets showed as travel, waiting or menus.
2. **Look at each one before cutting it.** A sheet over the stretch tells a quiet fight from an empty walk. Keep the first; cut the second.
3. Turn each dead stretch into a `[from, to)` range. The tools report source frames; a clip that starts at timeline frame `start` showing source frame `source_in` puts source frame `s` at `start + (s - source_in)`.
4. Leave room around speech: start the cut about 0.15 s after the silence begins and end it about 0.15 s before it ends, so no word is clipped.
5. Skip ranges shorter than about 0.7 s unless they are true silence between two sentences.
6. Send all ranges in one `batch_apply` of `timeline_ripple_delete_range`, **last range first**.
7. Read back the track and check the total length dropped by the sum of the ranges.

If travel has to be seen (the viewer must know how far it was), keep 2 to 4 seconds of it instead of all of it, or speed that piece up: split it out, shorten it with `timeline_trim_clip`, set `speed`, then close the gap it leaves. Mark it for the user to check, because sped-up sound may need covering.

## How much

Raw recordings of 90 to 150 minutes usually end as 15 to 25. Expect to remove most of the recording. After this pass, no stretch longer than about 10 seconds should be without a picture change, a new piece of information or a reaction, except a fight.

Afterwards, look at the result: `frames_contact_sheet` without `media` shows the timeline as it now plays. Do not cut a stretch you have not looked at.
