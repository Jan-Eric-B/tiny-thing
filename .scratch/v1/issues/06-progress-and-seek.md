# 06: Progress and seek

**What to build:** The Now Playing screen shows position and duration with a progress bar that moves smoothly between updates, and tapping the progress bar seeks. Position travels with a timestamp so the Device can advance it locally while playing and freeze it while paused.

**Blocked by:** 01

**Status:** open

- [ ] Now Playing carries duration, position and the time the position was taken
- [ ] Seam 3 test: with a fake clock, the progress advances while playing and stays still while paused
- [ ] Seam 1 test: a seek action from the Device arrives as a `MediaSource` seek to the right position
- [ ] Seam 3 test: tapping the progress bar at a point produces a seek to the matching position
- [ ] Nothing playing (no title) shows no progress bar at all
- [ ] Media sessions that cannot seek ignore the tap without error
