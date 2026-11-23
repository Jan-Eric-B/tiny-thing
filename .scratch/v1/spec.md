# TinyThing v1

Status: open

## Problem Statement

I use a Spotify Car Thing on my desk as a display and controller for whatever plays on my Windows PC (Tidal, Spotify, browser audio). Today it runs DeskThing, which I had to patch heavily and which still lets me down:

- The connection between computer and Car Thing is unreliable, at boot and during use, and when it drops it often does not come back without me intervening.
- The DeskThing server is a heavy Electron app that I keep maximized just to watch and rescue the connection, instead of it living quietly in the background.
- The Car Thing runs Spotify's 2018 firmware with Chromium 69 and a web app, which is slow to boot, memory hungry and full of workarounds.
- Installing and updating the Car Thing side is a manual, error-prone adb procedure (two Slots, a hand-written manifest), and the two halves drift apart in version.
- The code has had basic quality problems (unbounded logs, the cover art of the previous song shown for the current one, text that becomes unreadable on some covers).

DeskThing is an app platform; I use one feature of it. I want a small, reliable product built for exactly that feature.

## Solution

TinyThing: a Tray app on the computer and TinyThing OS with its own software on the Device.

- The Device shows Now Playing (Artwork, title, artist, progress) and controls it: play/pause, previous, next and Volume with the dial, the buttons and the touchscreen. When nothing plays it shows a large clock; when the computer is not reachable it says so calmly.
- The Tray app sits in the notification area, starts with the computer, uses almost no resources, shows the connection state in its icon, and opens a small settings window only on demand.
- Connection is automatic: the Tray app notices the Device's USB network appear and connects within seconds, and recovers by itself after every drop, sleep or reboot on either side.
- TinyThing OS is a minimal Linux image built for the Car Thing, with a native Device app instead of a browser, so it boots fast and leaves most of its memory free.
- Updates are hands-off: the Tray app updates itself from GitHub Releases (checks can be turned off) and keeps the Device on the matching version by itself, safely, with automatic rollback.
- Windows first; Linux and macOS later behind the same interfaces.

## User Stories

### Seeing what plays

1. As a listener, I want the Device to show the title and artist of Now Playing, so that I can see what is playing without switching windows.
2. As a listener, I want the Device to show the Artwork of Now Playing, so that I recognise the music at a glance.
3. As a listener, I want the Artwork to change exactly when the song changes, never showing the previous song's Artwork, so that the screen is always right (the Tidal timing problem).
4. As a listener, I want non-square Artwork such as 16:9 video thumbnails to be shown whole and undistorted, so that videos look as good as music.
5. As a listener, I want small Artwork not to be blown up into a blurry mess, so that low-resolution covers still look clean.
6. As a listener, I want a Backdrop behind the content that matches the Artwork, so that the screen feels tied to what plays.
7. As a listener, I want all text to be readable on every cover, so that I never have white text on a bright area again.
8. As a listener, I want an Accent color from the Artwork on the progress bar and icons, so that the screen is not monotonous while staying legible.
9. As a listener, I want to see the playback progress and duration, so that I know how far into the song I am.
10. As a listener, I want the progress to move smoothly between updates from the computer, so that it does not jump.
11. As a listener, I want the Device to follow whichever app the operating system marks as current, so that I do not have to choose a source.
12. As a listener, I want TinyThing to show nothing playing when the session has no title, so that idle placeholders never animate a fake progress bar.

### Controlling playback

13. As a listener, I want to press the dial to play or pause, so that the most common action is one press.
14. As a listener, I want to turn the dial to change the Volume in small steps, so that I can adjust loudness from the Device.
15. As a listener, I want the Device to show the Volume level briefly while I turn the dial, so that I know where it is.
16. As a listener, I want buttons 1, 2 and 3 to be previous, play/pause and next, so that I can skip without looking.
17. As a listener, I want the back button to switch between the Now Playing screen and the clock screen, so that I can choose what the Device shows.
18. As a listener, I want on-screen buttons for previous, play/pause and next, so that I can use touch as well.
19. As a listener, I want to tap the progress bar to seek, so that I can jump within a song.
20. As a listener, I want my input to take effect immediately on the Device and be corrected only if the computer disagrees, so that controls feel instant.
21. As a listener, I want Volume changes made on the computer to show up on the Device, so that both stay in sync.

### Idle and offline

22. As a desk user, I want a large clock when nothing has played for two minutes, so that the Device stays useful while idle.
23. As a desk user, I want the clock to show the computer's time and time zone, so that it is correct even though the Device has no internet.
24. As a desk user, I want the Device to show no clock rather than a wrong one before it has ever been Connected since boot, so that it never misleads me.
25. As a desk user, I want the clock to keep running when the connection drops, together with a small "not connected" hint, so that a drop is visible but not alarming.
26. As a desk user, I want optional dimming at night and screen-off after a long No link period, off by default, so that the Device does not glow all night if I choose.
27. As a desk user, I want to set the Device brightness from the settings, so that it suits my room.

### Connection

28. As a desk user, I want the Device and the Tray app to connect by themselves after either one starts, in any order, so that I never have to click "connect".
29. As a desk user, I want the connection to come back within a few seconds after the Device drops off USB and returns, so that hardware hiccups heal themselves.
30. As a desk user, I want the connection to recover after the computer sleeps and wakes, so that I do not have to replug anything in the morning.
31. As a desk user, I want the Tray icon to show No link, Link or Connected, so that I can see the state without opening anything.
32. As a desk user, I want no firewall prompt or rule for TinyThing, so that setup is clean.
33. As a desk user, I want the Device to reach the screen the moment it is Connected after boot, even if the computer was still starting, so that the boot order never matters.

### Tray app

34. As a desk user, I want TinyThing to live only in the notification area, so that it never occupies my taskbar or screen.
35. As a desk user, I want TinyThing to start with my login, with an option to turn that off, so that it is just there.
36. As a desk user, I want the Tray app to use very little memory and CPU when idle, so that it never affects my other apps.
37. As a desk user, I want a settings window that shows the connection state, Device versions and active Slot, so that I can see what is going on.
38. As a desk user, I want the settings window to free its resources when I close it, so that opening it once does not cost memory forever.
39. As a desk user, I want a "Save diagnostics" button that saves Tray app and Device logs into one archive, so that I can troubleshoot or report a problem in one step.
40. As a desk user, I want logs to have a fixed size limit on both sides, so that they never fill my disk or the Device.

### Installing and first run

41. As a new user, I want a per-user installer that needs no admin rights, so that installing is quick and harmless.
42. As a new user, I want the Tray app to recognise a Car Thing still on old firmware and tell me how to put it into recovery mode, so that I know the next step.
43. As a new user, I want the Tray app to recognise a Car Thing in recovery mode and show me the exact Flash command, so that the first Flash is not guesswork.
44. As a new user, I want to be reminded to back up the Car Thing before the first Flash, so that I can always go back.
45. As a new user, I want the Tray app to recognise a Device running TinyThing OS and connect without further steps, so that setup ends there.
46. As a new user, I want Windows' SmartScreen warning to be explained in the install instructions, so that I know it is expected for an unsigned app.

### Updates

47. As a desk user, I want the Tray app to check GitHub Releases on start and every 24 hours, so that I get fixes without looking for them.
48. As a desk user, I want to turn update checks off, so that nothing changes unless I want it to.
49. As a desk user, I want updates downloaded in the background and installed on my click or at the next start, so that an update never interrupts me.
50. As a desk user, I want the Device to receive an App update automatically when its version does not match the Tray app, so that the two halves never drift apart.
51. As a desk user, I want OS updates to install only while nothing is playing, so that the Device's reboot does not interrupt my music.
52. As a desk user, I want a broken App update to be rolled back automatically, so that a bad release cannot leave the Device dead.
53. As a desk user, I want a broken OS update to fall back to the previous Slot automatically, so that a bad image cannot brick the Device.
54. As a desk user, I want the Device to accept only Device updates signed with the TinyThing release key, so that a corrupt or foreign image is refused.

### Developer (me)

55. As the developer, I want to run the Device app as a Simulator window on my computer, so that I can build and test the screen without flashing.
56. As the developer, I want the Tray app to connect to the Simulator exactly as to a real Device, so that the whole flow can be exercised without hardware.
57. As the developer, I want dev images with SSH by key, so that I can inspect a real Device, while release images have no SSH at all.
58. As the developer, I want CI to build and test the Tray app and the Device software on every push, so that regressions show up immediately.
59. As the developer, I want TinyThing OS built locally from a pinned yocto-superbird commit, so that builds are reproducible.
60. As the developer, I want a changelog entry per user-visible change, so that release notes write themselves.

## Implementation Decisions

The decisions and their reasons that are expensive to reverse are recorded as ADRs 0001-0005; the vocabulary is in the glossary.

### Modules

- **protocol** (shared crate): every message between the Tray app and the Device daemon, the framing, and the Handshake. Deep module: callers send and receive typed messages, never bytes.
- **Tray core** (library inside the Tray app, no UI): connection state machine, Now Playing tracking, Artwork pipeline, update orchestration for the Device, settings and logging. Owns all logic so that it is testable without Tauri, Windows or a Device.
- **Platform adapters** (inside the Tray app): `MediaSource` (Now Playing plus transport commands), `VolumeControl` (master Volume get/set/observe), `LinkWatcher` (notices the Device's USB network appearing and disappearing) and a clock. v1 implements Windows (system media transport controls, Core Audio). The Tray core sees only the interfaces.
- **Tray shell** (Tauri 2): tray icon and menu, the settings window (Svelte 5 + TypeScript, created on open, destroyed on close), autostart, self-update.
- **Device daemon**: listens on the USB network interface only, runs the Handshake and heartbeat, forwards state to the Device app and input from it, applies App and OS updates, supervises the Device app, performs Confirm, controls brightness, keeps Device logs in RAM.
- **Device app** (Rust + Slint): draws the screens and turns dial, button and touch input into actions sent to the Device daemon over a local socket. Holds no connection or update logic. Runs on the Device (drawing straight to the display) and as the Simulator.
- **TinyThing OS** (`meta-tinything` layer on the pinned `meta-superbird` hardware layer): minimal image with no browser, compositor, Bluetooth or audio stack; A/B Slots via SWUpdate; a separate app partition for the Device app; SSH only in dev images.

### Protocol

- Plain TCP, each message length-prefixed and serialized with postcard. Dev builds log every message in readable form.
- The Device daemon listens on a fixed address on its USB network interface; the Tray app connects (ADR-0003). No pairing or password.
- Handshake first: protocol version, Tray app version, Device app version, TinyThing OS version, active Slot. A Device app version different from the one bundled in the Tray app triggers an App update right after the Handshake.
- Heartbeat every 1 s in both directions; a connection with 3 s of silence is dead.
- Tray app to Device: Now Playing (title, artist, duration, position with a timestamp, playing or paused, Artwork id), Artwork and Backdrop images, Accent color, Volume level, time and time zone, settings that affect the Device (brightness, dimming, idle rules), update payloads.
- Device to Tray app: input actions (play/pause, previous, next, seek to position, Volume step or level), Device status (versions, Slot, health), log bundles on request.
- Artwork is identified by a hash of its content, never by title or artist. The Device keeps the last few images; the Tray app sends an image only when the Device does not have it.

### Connection states

- No link: the Device's USB network is not present. Link: it is present but no session is up. Connected: the Handshake has completed and heartbeats flow.
- The Tray app reacts to `LinkWatcher` events and otherwise retries every 1-2 s while in Link. A dead connection drops to Link or No link, never to an error state that needs a user.

### Artwork pipeline (in the Tray core)

- Scale once, to the exact pixel size the Device draws; the Device never rescales.
- Downscaling: Lanczos3 in linear light (sRGB converted to linear and back).
- Upscaling: Mitchell in sRGB, at most 2x; beyond that the Artwork is drawn smaller instead.
- Backdrop: shrink to about 32 px with a box filter, enlarge with Gaussian, then darken until its brightest pixel has at least 4.5:1 contrast against white text.
- Accent: a color taken from the Artwork, adjusted until it has at least 4.5:1 contrast against the Backdrop.
- Images travel lossless (QOI or PNG).
- Text is never drawn over the Artwork itself, and is always white.

### Device screens and input

- Screens: Now Playing; Clock (after 2 minutes with nothing playing, or chosen with the back button); Not connected hint layered over the Clock. No clock before the first time since boot that the Device is Connected.
- Input: dial turn = Volume ±2 %; dial press = play/pause; buttons 1/2/3 = previous / play-pause / next; button 4 unassigned; back button = toggle Now Playing / Clock; touch = on-screen controls and seek on the progress bar. Input is applied optimistically on the Device and corrected by the next state from the Tray app.

### Updates

- Tray app: Tauri updater against GitHub Releases, signed with the TinyThing updater key; check on start and every 24 h unless turned off; download in the background, install on click or at the next start. The installer is per-user, unsigned for Windows code signing.
- The Device app is bundled inside each Tray app release and is never downloaded separately.
- App update: the Tray app streams the signed Device app package; the Device daemon stores it next to the current one, starts it, and rolls back to the previous Device app if the new one crashes on start.
- OS update: a signed SWUpdate package written into the inactive Slot, applied automatically only while nothing is playing, followed by a reboot.
- Confirm: the Device daemon Confirms the booted Slot after the Device app has run and drawn for about 30 seconds, independent of whether the Tray app is reachable. An unconfirmed Slot falls back after three failed boots.

### First run and Flash

- The Tray app identifies three Car Thing states by their USB identity: old firmware (Spotify stock or Thing Labs), recovery mode, TinyThing Device. It shows guidance for the first two.
- v1 Flash uses flashthing-cli, pinned to an exact version, run by the user following the guidance; built-in Flash comes later. A full backup with superbird-tool is recommended before the first Flash.

### Settings window (v1)

Connection state and Device info (versions, active Slot); brightness, dimming and screen-off rules; autostart on/off; update checks on/off and "check now"; Save diagnostics; about.

### Logging

- Tray app: rolling files, at most 5 files of 2 MB, level info.
- Device: logs in RAM only, capped at 8 MB; only crash reports survive a reboot.
- Save diagnostics: one archive with Tray app logs and the Device logs fetched over the connection.

### Platforms

v1 implements the platform adapters for Windows. Linux (MPRIS, PipeWire/PulseAudio, network interface events) follows in v2; macOS is best effort later because access to Now Playing is restricted there.

### Repository

kebab-case names throughout; Rust workspace with one folder per crate; the TinyThing OS layer and its build configuration in their own top-level folder; GPLv3; Keep a Changelog 1.1.0 with Semantic Versioning.

## Testing Decisions

Good tests here exercise behaviour through the highest public seam and assert what a user or the other side of the wire would observe, never internal structure. Time, the operating system and hardware are faked at the edges; everything between the edges is real. Tests are written first, one behaviour at a time.

There are three seams:

1. **Tray core and Device daemon, connected for real.** Both run in the test process over a real local TCP connection. Faked edges: `MediaSource`, `VolumeControl`, `LinkWatcher` and the clock on the Tray side, and a recording stand-in for the Device app on the Device side. This seam covers the protocol and Handshake (including version mismatch triggering an App update), the connection states, heartbeat timeout and reconnect, Now Playing forwarding, the Artwork pipeline as received by the Device (exact size, 2x cap, Artwork id stable per content and changed on content change, Backdrop contrast guarantee for every test image, Accent contrast), input round trips (a button action arrives as the right `MediaSource` or `VolumeControl` call), time and clock rules, and Save diagnostics.
2. **Device daemon updates.** The Device daemon's update, supervision and Confirm behaviour against a fake file system, fake process supervision and a fake boot environment holding the Slot counters. Covers: App update installed and started, rollback when the new Device app crashes, unsigned or corrupt packages refused, OS update only while nothing plays, Confirm after a healthy run and not before, fallback after three unconfirmed boots.
3. **Device app.** Slint's headless testing backend: given a state from the Device daemon (playing, paused, idle for 2 minutes, No link, never Connected since boot), the right screen and elements are shown; dial, button and touch input produce the right actions.

The Windows adapters, the Tauri shell, TinyThing OS and a real Device are verified by hand (and the Simulator end to end), not by automated tests.

There is no prior art in the repo yet; these suites set the pattern.

## Out of Scope

- Weather, app shortcuts on button 4 and remapping of buttons and dial (v1.1, sharing one action model).
- Flashing from inside the Tray app, and installing the Windows recovery-mode USB driver from it.
- Linux and macOS platform adapters.
- More than one Device per computer, and pairing.
- Choosing a Media session by hand instead of following the operating system's current one.
- Per-app volume.
- Light themes, black text, or other text colors.
- Paid Windows code signing.
- Bluetooth, Wi-Fi, audio output and microphones on the Device.
- Building TinyThing OS in CI.
- Any mechanism that fixes the Car Thing dropping off USB; TinyThing only recovers from it.

## Further Notes

- The Car Thing's drops off the USB bus are a hardware problem (most likely power). A powered USB hub is the recommended remedy alongside TinyThing.
- The yocto-superbird hardware layer currently expects its Chromium layer to be present; whether it must also be built is checked when TinyThing OS is first set up, and the fork carries a patch if needed.
- flashthing is pre-1.0, untested on Windows by its author and has no eMMC backup; pin an exact version.
- The macOS Now Playing restriction should be verified before macOS work starts.
