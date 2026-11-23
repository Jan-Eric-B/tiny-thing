# 17: First-run guidance

**What to build:** The Tray app recognises which state a plugged-in Car Thing is in from its USB identity and guides me: old firmware (Spotify stock or Thing Labs) leads to instructions for entering recovery mode; recovery mode leads to the backup reminder and the exact Flash command for the pinned flashthing-cli; a TinyThing Device simply connects.

**Blocked by:** 09, 11

**Status:** open

- [ ] USB identity detection for the three states, behind an interface with a fake for tests
- [ ] Tests: each state leads to the right guidance; switching states while the guidance is open updates it
- [ ] Guidance shown in the settings window and announced by a tray notification
- [ ] Flash command and driver steps match what ticket 11 established
- [ ] Verified by hand with the Car Thing in recovery mode
