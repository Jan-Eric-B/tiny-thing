# 01: Walking skeleton

**What to build:** The thinnest end-to-end path through TinyThing. A Rust workspace with the `protocol` crate, the Tray core, the Device daemon and the Device app. The Tray core connects to the Device daemon over TCP, runs the Handshake, and forwards the title and artist from a fake `MediaSource`; the Device daemon passes them to the Device app, which shows them in the Simulator window. CI builds and tests the workspace on every push. This ticket establishes test seam 1 (Tray core and Device daemon connected for real, faked edges) and the local socket between Device daemon and Device app.

**Blocked by:** None (can start immediately).

**Status:** open

- [ ] Cargo workspace with one kebab-case folder per crate (protocol, tray, device-daemon, device-app), GPLv3 license file
- [ ] `protocol`: length-prefixed postcard framing and a Handshake carrying protocol and software versions; every message round-trips losslessly (test)
- [ ] Seam 1 test: fake `MediaSource` reports a title and artist, the recording Device app stand-in receives them
- [ ] Seam 1 test: a protocol version mismatch in the Handshake is reported, not silently ignored
- [ ] Device daemon listens; Tray core connects (ADR-0003)
- [ ] Running the Tray core with a fake `MediaSource` plus the Simulator shows the title and artist on screen
- [ ] CI (GitHub Actions) builds and runs all tests on Windows and Linux
- [ ] CHANGELOG entry under Unreleased
