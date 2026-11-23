# 14: OS update and Confirm

**What to build:** OS updates over the USB network. The Tray app delivers a signed SWUpdate package, but only while nothing is playing; the Device daemon writes it into the inactive Slot and reboots into it. The Device daemon Confirms the booted Slot once the Device app has run and drawn for about 30 seconds, regardless of whether the Tray app is reachable; an unconfirmed Slot is abandoned after three failed boots.

**Blocked by:** 13

**Status:** open

- [ ] yocto-superbird's boot-time "slot good" marking replaced by the Device daemon's Confirm
- [ ] Seam 2 tests (fake boot environment holding Slot counters): Confirm after a healthy run and not before; no Confirm when the Device app keeps crashing; unsigned package refused
- [ ] Seam 1 test: an OS update waits while something is playing and proceeds when playback stops
- [ ] Active Slot reported in the Handshake and shown in the settings window
- [ ] Verified on the real Device: a good OS update switches Slot; a deliberately broken one falls back to the previous Slot
