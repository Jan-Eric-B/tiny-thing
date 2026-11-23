# 07: Robust connection

**What to build:** The connection heals itself. The Tray core tracks No link, Link and Connected; a Windows `LinkWatcher` reports the Device's USB network appearing and disappearing; heartbeats every second detect a dead connection after 3 seconds; the Tray core reconnects every 1-2 seconds while in Link, including after the computer sleeps and wakes. The tray icon shows the state, and the Device shows a calm "not connected" hint.

**Blocked by:** 02

**Status:** open

- [ ] Seam 1 tests with fake clock and fake `LinkWatcher`: Connected drops to Link after 3 s of silence; reconnects when the Device daemon returns; starting order of the two sides does not matter; No link stops retrying until the link appears again
- [ ] No state requires a user action to recover
- [ ] Windows `LinkWatcher` adapter based on network interface events, with the Simulator treated as an always-present link
- [ ] Tray icon (and its tooltip) shows No link, Link or Connected
- [ ] Device app shows the "not connected" hint while not Connected (seam 3 test)
- [ ] Verified by hand: killing and restarting the Simulator, and a sleep/wake cycle of the computer
