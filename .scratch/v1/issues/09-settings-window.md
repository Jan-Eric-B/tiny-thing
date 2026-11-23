# 09: Settings window

**What to build:** A settings window opened from the tray icon (Svelte 5 + TypeScript), created on open and destroyed on close. It shows the connection state and Device info (versions, active Slot) and lets me set autostart at login, Device brightness, night dimming and screen-off after a long No link period (both off by default). Settings persist and settings that affect the Device are sent to it.

**Blocked by:** 07, 08

**Status:** open

- [ ] Window opens from the tray menu and frees its WebView when closed (memory checked before and after)
- [ ] Connection state and Device info update live while the window is open
- [ ] Autostart toggle registers or removes per-user login start
- [ ] Brightness, dimming and screen-off settings are saved and reach the Device (seam 1 test); the Simulator reflects brightness visibly
- [ ] Seam 3 test: dimming and screen-off rules applied with a fake clock
- [ ] Settings survive a restart of the Tray app
