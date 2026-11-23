# 01: Walking skeleton

**What to build:** The thinnest end-to-end path through TinyThing. A Rust workspace with the `protocol` crate, the Tray core, the Device daemon and the Device app. The Tray core connects to the Device daemon over TCP, runs the Handshake, and forwards the title and artist from a fake `MediaSource`; the Device daemon passes them to the Device app, which shows them in the Simulator window. CI builds and tests the workspace on every push. This ticket establishes test seam 1 (Tray core and Device daemon connected for real, faked edges) and the local socket between Device daemon and Device app.

**Blocked by:** None (can start immediately).

**Status:** in-progress

- [x] Cargo workspace with one kebab-case folder per crate (protocol, tray, device-daemon, device-app), GPLv3 license file
- [x] `protocol`: length-prefixed postcard framing and a Handshake carrying protocol and software versions; every message round-trips losslessly (test)
- [x] Seam 1 test: fake `MediaSource` reports a title and artist, the recording Device app stand-in receives them
- [x] Seam 1 test: a protocol version mismatch in the Handshake is reported, not silently ignored
- [x] Device daemon listens; Tray core connects (ADR-0003)
- [x] Running the Tray core with a fake `MediaSource` plus the Simulator shows the title and artist on screen
- [ ] CI (GitHub Actions) builds and runs all tests on Windows and Linux
- [x] CHANGELOG entry under Unreleased

## Comments

**2026-11-23, implementation.** Everything is built except that CI has not run yet. Tick the CI box, and set the status to done, once the first push shows a green run on both runners. The Linux job could not be tried locally (no WSL distribution here); its apt package list comes from Slint's build docs.

- Crates live in `crates/<name>` (protocol, tray, device-daemon, device-app). The `tray` library is the Tray core; the Tauri shell will go into the same crate in 02.
- Wire format: a 4-byte big-endian length, then the postcard payload. Frames are capped at 8 MiB (`MAX_FRAME_LEN`).
- The Handshake is symmetric: each side sends its own `{protocol_version, software_version}` first, then reads the other side's. Only the protocol version is guaranteed to decode across releases, because it must stay the Handshake's first field. So a mismatch is reported even when the rest of the Handshake has changed shape. The Tray core returns `ConnectError::Handshake(HandshakeError::ProtocolMismatch { ours, theirs })`. The Device daemon logs the mismatch, closes the connection and keeps listening.
- The Device daemon to Device app local socket is TCP on loopback (port 47801), using the same framing and `protocol::local::{DaemonMessage, AppMessage}`, with no Handshake. The Device daemon keeps the latest state in a `watch` channel, so a Device app that connects late or restarts gets the current Now Playing straight away.
- `MediaSource::now_playing()` returns a `watch::Receiver<Option<NowPlaying>>`. In 03, transport commands become extra trait methods. `DeviceMessage` and `AppMessage` are empty enums until input actions arrive in 03. Their receive functions then turn into loops, and they already run as long-lived futures beside the sender so that neither is cancelled mid-frame.
- The Seam 1 tests live in `crates/tray/tests` and use `device-daemon` as a dev-dependency, with the recording Device app stand-in in `tests/support`. There is also a first Seam 3 test (Slint testing backend) in `crates/device-app/tests/screen.rs`. It needs Slint debug info, which `build.rs` turns on for non-release builds only.
- Running the Simulator by hand: `cargo run -p device-daemon`, `cargo run -p device-app`, `cargo run -p tray --example fake-now-playing -- --title "..." --artist "..."`. Tested on Windows: the window showed the title and artist.
- License: `GPL-3.0-only`, to match Slint's GPL option. Switch to `-or-later` if that is preferred.
- Left for later tickets: heartbeat and reconnect from the Tray side (07; the Device app does retry its local socket every 500 ms), and Device app, TinyThing OS and Slot versions in the Handshake (12/13). The local socket has no version check, but an App update can ship a Device app newer than the Device daemon's local protocol, so 13 needs one.

**2026-11-23, review fixes.** Newest connection wins on both Device daemon listeners (one shared accept loop in `device-daemon/src/newest_connection.rs`), and the Handshake times out after `HANDSHAKE_TIMEOUT` (5 s) on both sides. Framing runs on tokio-util's `FramedRead`/`FramedWrite` with an own codec, so `recv` is cancel-safe, and the Handshake shares the message reader and writer. `SessionError::DeviceClosed` is now `DeviceGone` (also covers resets and broken pipes). `SoftwareVersion` is built from `CARGO_PKG_VERSION_MAJOR/MINOR/PATCH`. All links share one send-loop plus receive-loop shape.
