# 03: Playback controls

**What to build:** Controlling Now Playing from the Device. Dial press and button 2 toggle play/pause, buttons 1 and 3 go to previous and next, and on-screen buttons do the same by touch. The Device app shows the result immediately and accepts the next state from the Tray app as the truth.

**Blocked by:** 01

**Status:** done

- [x] Input actions defined in `protocol` (play/pause, previous, next)
- [x] Seam 1 test: each action sent from the Device side arrives as the matching `MediaSource` call
- [x] Seam 3 test (Slint headless backend): dial press, buttons 1-3 and the on-screen buttons produce the right actions
- [x] Optimistic play/pause state on the Device, corrected by the next Now Playing state
- [x] In the Simulator, keyboard keys stand in for dial and buttons
- [x] Works against real Windows playback when combined with 02 (verified by hand)

## Comments

**2026-11-23, implementation.**

- Protocol (version 2): `InputAction { PlayPause, Previous, Next }`, sent as `AppMessage::Input` (Device app to Device daemon) and `DeviceMessage::Input` (Device daemon to Tray app). `NowPlaying` gained `playback: PlaybackState { Playing, Paused }` end to end. Seek (06) and Volume (04) become further `InputAction` variants.
- `MediaSource` has `toggle_play_pause`, `previous` and `next`, each `async` returning `Result<(), ControlError>` (`NoMediaSession`, `Refused`, `TimedOut`, `Platform`). The trait uses `impl Future + Send` returns, so `Session::run` is now generic (`&M`, no `dyn`) and its future borrows the source; `FakeMediaSource::stop()` replaces dropping the source in the session-end test. The session carries out input actions one after the other; a failed one is logged as a warning and the session goes on. `FakeMediaSource` records calls (`calls()`, `MediaCall`) and can be made to fail (`fail_controls`).
- Device daemon: input actions go from the Device app link to the Tray app link over a `broadcast` channel that each Tray app connection subscribes to on accept. A press while no Tray app is connected is dropped (logged), never replayed to a later connection. The Device app likewise drops presses that queued up while its local socket was down.
- Windows adapter: the controls call `TryTogglePlayPauseAsync`, `TrySkipPreviousAsync` and `TrySkipNextAsync` on the session that is current at that moment, with a 2 s limit (cancelled in Windows when it runs out); `false` from Windows is `Refused`. Playback status: only `Playing` counts as playing, every other status (paused, stopped, opened, closed, changing) shows as paused (unit tested in `windows_media/now_playing.rs`). The read-with-timeout-and-cancel code now lives in `windows_media/call.rs` for reads and controls. `windows-future` 0.3.2 is a direct dependency now (the version `windows` 0.62.2 uses; its `IAsyncOperation` is not re-exported).
- Device app: physical inputs reach Rust as a Slint `DeviceInput` (dial press, buttons 1-3) and on-screen controls as `TouchControl`; `input.rs` maps both to `InputAction` (one place for the v1.1 remapping). The optimistic state is the pure `PlayPause` (`play_pause.rs`, unit tested): a toggle shows the other state at once, the next Now Playing replaces it whatever it says, and `Screen` ends it after `OPTIMISTIC_HOLD` (2 s) with a Slint timer when no Now Playing comes (player ignored the command, Windows publishes nothing because nothing changed, or no Tray app is connected). Each toggle restarts the hold. `screen::show` became `Screen::new(window, send)` plus `screen.show(message)`; in `main` the `Screen` lives in a `slint::spawn_local` task on the UI thread, fed by a channel from the local socket thread. The on-screen controls show only while something plays; the dial and buttons always send.
- Simulator keys: Enter = dial press (play/pause), 1 = button 1 (previous), 2 = button 2 (play/pause), 3 = button 3 (next); a held key acts once. Documented in `device-app --help` and in `ui/app.slint`. Enter and 1-4 are also what the Car Thing's input driver reportedly emits, so the same mapping may work on the Device; unverified until 12.
- Dial rotation (Volume, 04) is not implemented.

Verified by hand (debug builds, Windows 11 24H2 LTSC): device-daemon, device-app and the Tray app, with the 02 test page extended with Media Session `play`, `pause`, `previoustrack` and `nexttrack` handlers that show the page state in the Edge window title.

- Enter in the Simulator paused the Edge test page (window title went from `[playing]` to `[paused]`).
- With the test page paused, Windows made the TIDAL desktop app (already playing on this machine) the current Media session, so the following presses went to TIDAL: 3 and 3 skipped to the next song twice, 1 went back one, 2 paused it, and a click on the on-screen Play resumed it. Each showed up as the matching Now Playing change within about 50 ms to 1 s (Tray app log). So all three controls work against a real Windows player, through keys and touch.
- Against the fake `MediaSource` (`fake-now-playing` example): every key and every on-screen control arrived as the right action; clicking Pause showed Play at once and fell back to Pause after 2 s, since the fake does not change state. With no Tray app connected, the Device daemon logged and dropped the presses.

Open question for the product: following `GetCurrentSession` means that after pausing one app, Windows may make another playing app current, and the next press (for example play/pause to resume) goes to that other app. That is what happened with TIDAL above. Worth deciding whether the Tray core should stick to the session it last controlled for a while.

**2026-11-23, review fixes.**

- Optimistic play/pause: a Now Playing ends the optimistic state only when its playback differs from the one reported at the toggle (the player acted); a new title (or, from 06, a new position) with the same playback keeps it until the hold runs out. `OPTIMISTIC_HOLD` is now 3 s, so it outlasts the Windows adapter's 2 s `CONTROL_TIMEOUT` plus the Now Playing round trip; both doc comments name the dependency. `PlaybackState::toggled` moved from `protocol` into `play_pause.rs`, and its unit tests are gone (Seam 3 covers them).
- Tray core: the session keeps reading the Device while a control runs. Input actions go to a control worker through a channel with room for one, so one control runs and one waits; a press beyond that is dropped (debug log). The receive loop yields after each hand-over so presses that arrive in one read are not mistaken for presses during a busy control. `NoMediaSession` is logged at debug, other control errors at warn.
- `MediaSource` has one `control(TransportControl)` instead of three methods (`TransportControl { TogglePlayPause, Previous, Next }`, from `InputAction` via `From`); the fake records `TransportControl` (no more `MediaCall`) and can stall controls (`stall_controls`). `Session::run` is a plain `async fn`. The Device daemon's unreachable `InputsClosed` error is gone.
- New tests: Seam 1 for a stalled control (the session still notices the Device going away; only one stale press runs after the stall), a press before any session is not delivered later, and after a newer Tray app connection only the new session gets presses; device-app `daemon_link` for dropping presses queued while not connected; Seam 3 for a same-playback Now Playing keeping the optimistic state.
