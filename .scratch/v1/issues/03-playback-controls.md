# 03: Playback controls

**What to build:** Controlling Now Playing from the Device. Dial press and button 2 toggle play/pause, buttons 1 and 3 go to previous and next, and on-screen buttons do the same by touch. The Device app shows the result immediately and accepts the next state from the Tray app as the truth.

**Blocked by:** 01

**Status:** open

- [ ] Input actions defined in `protocol` (play/pause, previous, next)
- [ ] Seam 1 test: each action sent from the Device side arrives as the matching `MediaSource` call
- [ ] Seam 3 test (Slint headless backend): dial press, buttons 1-3 and the on-screen buttons produce the right actions
- [ ] Optimistic play/pause state on the Device, corrected by the next Now Playing state
- [ ] In the Simulator, keyboard keys stand in for dial and buttons
- [ ] Works against real Windows playback when combined with 02 (verified by hand)
