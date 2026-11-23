# 02: Real Now Playing on Windows

**What to build:** The Tray app runs as a real Windows tray application (Tauri 2, icon and a Quit menu item, no window) and reads Now Playing from the Windows system media transport controls through a Windows `MediaSource` adapter. Whatever Tidal, Spotify or a browser plays shows up in the Simulator, and it follows the session Windows marks as current.

**Blocked by:** 01

**Status:** open

- [ ] Windows `MediaSource` adapter behind the existing interface; the Tray core is unchanged apart from wiring
- [ ] Title and artist of the current Media session appear in the Simulator within a second of a song change
- [ ] Switching which app plays switches what the Simulator shows
- [ ] A Media session without a title counts as nothing playing
- [ ] Tray icon in the notification area with Quit; no taskbar entry and no window
- [ ] Idle Tray app memory and CPU measured and noted in the ticket (baseline for later)
- [ ] Verified by hand with Tidal and with a browser
