# Dead air: cut what loses the viewer

Removes the stretches of a raw recording where nothing happens. Do this pass first, before music, cards or captions are on the timeline.

## What to cut

- Silence. When in doubt, remove it; a gap loses viewers faster than a jump cut.
- Walking, sailing or digging with nothing happening and nothing said.
- Waiting: loading screens, smelting, searching through chests or menus without a purpose.
- Repeated attempts. Keep the best one, and the first failure if it is funny.
- Setup talk before the action starts.
- Anything said twice.

## What to keep

- Deaths, mistakes and surprised reactions. They are a pacing device in this format: at least 5 of the 30 studied videos show a death screen in the first 5 minutes.
- The second or two **after** a surprise. The reaction is the content; do not cut it off.
- Fights. A real fight holds attention as one long take, so do not chop it to raise the cut count.
- About 2 seconds of each item tooltip or recipe screen. It shows the rule without explaining it.
- The sentence that states the next goal. Each turn of the loop starts with it.

## How to cut

1. Work from the inventory. Turn each dead stretch into a `[from, to)` range in timeline frames.
2. Leave room around speech: start the cut about 0.15 s after the last word and end it about 0.15 s before the next. Never cut inside a word.
3. Skip ranges shorter than about 0.7 s unless they are true silence between two sentences.
4. Send all ranges in one `batch_apply` of `timeline_ripple_delete_range`, **last range first**.
5. Read back the track and check the total length dropped by the sum of the ranges.

If travel has to be seen (the viewer must know how far it was), keep 2 to 4 seconds of it instead of all of it, or speed that piece up: split it out, shorten it with `timeline_trim_clip`, set `speed`, then close the gap it leaves. Mark it for the user to check, because sped-up sound may need covering.

## How much

Raw recordings of 90 to 150 minutes usually end as 15 to 25. Expect to remove most of the recording. After this pass, no stretch longer than about 10 seconds should be without a picture change, a new piece of information or a reaction, except a fight.

Do not cut a stretch you have no inventory for. Mark it and ask.
