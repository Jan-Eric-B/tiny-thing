# 02: Real Now Playing on Windows

**What to build:** The Tray app runs as a real Windows tray application (Tauri 2, icon and a Quit menu item, no window) and reads Now Playing from the Windows system media transport controls through a Windows `MediaSource` adapter. Whatever Tidal, Spotify or a browser plays shows up in the Simulator, and it follows the session Windows marks as current.

**Blocked by:** 01

**Status:** in-progress

- [x] Windows `MediaSource` adapter behind the existing interface; the Tray core is unchanged apart from wiring
- [x] Title and artist of the current Media session appear in the Simulator within a second of a song change
- [x] Switching which app plays switches what the Simulator shows
- [x] A Media session without a title counts as nothing playing
- [x] Tray icon in the notification area with Quit; no taskbar entry and no window
- [x] Idle Tray app memory and CPU measured and noted in the ticket (baseline for later)
- [ ] Verified by hand with Tidal and with a browser

## Comments

**2026-11-23, implementation.** Everything is built and verified with a browser. Left open: the check with Tidal (not available here). Tick the last box, and set the status to done, after playing something in Tidal and in a browser and switching between them.

- Where it lives: the `tray` crate, as planned in 01. The library is still the Tray core (unchanged; no UI, no platform code). The new binary (`src/app/`) is the Tray app: the Tauri shell (`shell.rs`), the Windows `MediaSource` adapter (`windows_media.rs`) and the wiring between them and a Tray core `Session` (`forwarding.rs`). Run it with `cargo run -p tray` (`--device` for another Device daemon address; default is localhost for the Simulator).
- Linux CI: `tauri`, `tauri-build` and `windows` are `cfg(windows)` dependencies, and the binary is a stub on other systems that says the Tray app runs on Windows only so far. So Linux builds, lints and tests the Tray core without Tauri's system libraries (webkit2gtk and friends) and CI needs no new apt packages. When the Linux adapters arrive (v2), the gate moves and CI gets those packages. I could not run a Linux build locally (no Linux target or WSL here), so the first push is the real check.
- The Windows adapter is event driven, no polling: it wakes on `CurrentSessionChanged` and `SessionsChanged` of the session manager and on `MediaPropertiesChanged` and `PlaybackInfoChanged` of the current session, then reads the current session once. Wake-ups during a read fold into one more read. It follows `GetCurrentSession` only (choosing a session by hand is out of scope). A failed or slow read (2 s timeout) is logged and keeps the last Now Playing; the next event reads again. The WinRT path has no unwrap or expect of its own (windows-future still unwraps internally: an `IAsyncInfo` cast that cannot fail, and its waker mutex).
- Mapping rules (unit tested, `windows_media/now_playing.rs`): no current session or a blank title is nothing playing; surrounding whitespace is dropped; an empty artist falls back to the album artist; a title without any artist still shows.
- No window: `app.windows` is empty in `tauri.conf.json`. The process does own Tao's 16x16 event-target window, but it is a transparent tool window (`WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE`), so no taskbar or Alt+Tab entry. Release builds use the Windows subsystem, so no console either. Windows puts new tray icons in the hidden-icons overflow by default.
- Device daemon not reachable: the Tray app logs the error and stays in the notification area so it can be quit; same when the session ends. It needs a restart until reconnecting arrives in 07.
- The icon (`icons/icon.ico`) is a generated placeholder: a dark device body with a teal screen and a light dial.

Verified by hand (release builds, Windows 11 24H2 LTSC): device-daemon, device-app and the Tray app, with a test page in two separate Edge instances (own profiles) that plays a near-silent tone and changes its Media Session title every 10 seconds.

- The Simulator showed the page's title and artist. Song changes reached the Tray app 44 to 120 ms after the 10-second mark at which the page changes them (Tray app log timestamps; the page itself checks every 100 ms).
- Starting the second instance switched the Simulator to it; closing it switched back to the first; closing that too showed nothing playing.
- Tray icon present (found via UI Automation in the overflow); its menu shows Quit, and Quit ends the process with exit code 0.
- Idle baseline (release build, connected, no Media session, 30 s sampled every second with `Get-Process`): CPU time 0 ms (0.000 % of one core), working set 23.0 MB, private bytes 4.2 to 4.3 MB, 23 threads, 283 handles.

Not covered: Tidal, Spotify, and a session that keeps an empty title (covered by the unit test only). Some players clear the title briefly during a song change, which shows as a short moment of nothing playing; if that flickers on the Device, debounce it in the Tray core later.

**2026-11-23, review fixes.** These supersede the bullets above where they differ.

- Crate split: the Tray core and the Tray app no longer share a crate, so the Tray core cannot reach Tauri or `windows`, and building it no longer compiles Tauri. `crates/tray-core` is the library (code unchanged, with its Seam 1 tests, `FakeMediaSource` and the fake-now-playing example); `crates/tray-app` is the binary (`cargo run -p tray-app`). Linux CI keeps the same strategy, now contained in `tray-app`: all its dependencies are `cfg(windows)` and its `main` is a one-line stub elsewhere, so `ci.yml` is unchanged. `tauri-build` is an ungated build dependency and `build.rs` checks `CARGO_CFG_TARGET_OS`, because build scripts run on the host (Linux compiles tauri-build, which needs no system libraries).
- Failed reads retry: a failed or timed-out read keeps the last Now Playing and reads again after 0.5 s, 1 s, 2 s, then every 5 s until a read succeeds; a change event still reads at once (`windows_media/retry.rs`, unit tested). A timed-out read is cancelled in Windows (`Cancel` on the `IAsyncOperation`; windows-future's future does not cancel on drop).
- Clean Quit: the shell builds the Tauri app, adds the tray icon and starts forwarding outside a setup closure, then runs with `App::run_return`. Errors reach `main` as `Result`s instead of a panic in setup, and on Quit forwarding stops and removes its Windows event handlers (waiting at most 2 s) before `main` returns Tauri's exit code. Before, Tauri's `run` ended the process inside the event loop, so none of this ran.
- Why forwarding stopped shows in the tray icon's tooltip, for example "TinyThing: Device not reachable, restart to retry" (also: connection to the Device lost, Handshake with the Device failed, cannot read Now Playing from Windows, Now Playing from Windows stopped, and stopped after an internal error when the forwarding task panics). Still no reconnecting (07) and no log file (10).
- The followed Media session is compared by COM identity (`IUnknown` pointers, cast with `?`) instead of `PartialEq`, which unwraps a cast.
- `tauri.conf.json` no longer sets `security.csp` to null (that is the default), so 09 does not inherit what looks like a deliberate CSP opt-out.

Re-verified by hand (debug build with console, same Edge test page): the Simulator showed the page's title and artist and followed switches between two Edge instances and back to nothing playing. "following the current Media session" was logged once per app switch (3 switches over 9 song changes), so `GetCurrentSession` returns the same object for the same session. Quit from the tray menu exited with code 0, also after forwarding had stopped. Killing the Device daemon set the tooltip to "connection to the Device lost"; starting without it, to "Device not reachable". Idle memory and CPU were not measured again: the split changes no runtime code, and retries run only after failed reads.

**2026-11-24, nothing-playing debounce.** Done, as decided: the Tray core holds back nothing playing, for every `MediaSource`, so the Windows adapter is unchanged.

- When Now Playing goes from something to nothing, the session waits `NOTHING_PLAYING_HOLD` (500 ms, `crates/tray-core/src/session.rs`) before telling the Device. A title within that time goes out at once and drops the nothing held back, so the Device never sees the gap. Nothing after nothing changes nothing (no repeated "nothing" messages any more); the first Now Playing of a session goes out at once, nothing included. The logic is one small state (`Told`) and one timer branch in the session's send loop.
- Artwork: the Tray core's Artwork follows what the Device was told, so while nothing is held back, the Device can still get the Artwork of the song it shows; a title that comes back with the same Artwork keeps the Artwork already prepared. Volume and Artwork answers go out during the hold as before. Optimistic play/pause on the Device is unaffected: only "nothing" is delayed, and any Now Playing with a title (with its playback) goes out at once.
- Seam 1 tests in `crates/tray-core/tests/now_playing.rs` run on a paused Tokio clock moved by hand: a gap shorter than the hold never reaches the Device app stand-in; nothing that lasts reaches it exactly after the hold; a new title during the hold goes out at once and nothing follows later. Since Tokio moves a paused clock to the next timer whenever the runtime waits, also for loopback I/O, the tests keep the clock still with a running blocking task that lets go after the tests' patience in real time (so a broken test fails instead of hanging).
