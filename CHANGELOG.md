# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project glossary and architecture decision records.
- Walking skeleton: the Tray core connects to the Device daemon over TCP, runs the Handshake (protocol and software versions, mismatches reported) and forwards the title and artist of Now Playing; the Device daemon passes them to the Device app, which shows them in the Simulator window.
- The Device daemon lets a new connection from the Tray app or the Device app replace the previous one, so a connection left half-open (USB unplugged, computer asleep) no longer blocks reconnecting. A Handshake that gets no answer gives up after 5 seconds instead of hanging.
- Continuous integration builds, lints and tests the workspace on Windows and Linux.
- The Tray app runs on Windows as a notification area icon with a Quit item and no window. It shows Now Playing from the Windows media controls on the Device: the title and artist of the Media session Windows marks as current, following song changes and switches between apps as they happen. A Media session without a title counts as nothing playing. When Windows fails to report Now Playing, the Tray app tries again after a short delay. When forwarding stops (for example the Device is not reachable), the tray icon's tooltip says why.
- Playback controls on the Device: the dial press and button 2 play or pause Now Playing, buttons 1 and 3 go to the previous and next song, and on-screen controls do the same by touch. The Device shows play or pause at once and keeps showing it until the Tray app reports that the player has played or paused (a new song alone does not end it); if that does not happen within 3 seconds it shows the last reported state again. In the Simulator, Enter stands in for the dial press and the keys 1, 2 and 3 for the buttons. Now Playing now carries whether it is playing or paused (protocol version 2). A press while no Tray app is connected is dropped rather than carried out later, and so is a press made while the player is still busy with one press and another is already waiting.
- Volume on the Device: turning the dial changes the computer's master Volume in 2 % steps, from 0 to 100 %, on the Windows default output device. The Device shows the Volume level while the dial turns or when the Volume changes on the computer, and hides it again 1.5 seconds after the last change. While the computer is muted, the Device shows a muted speaker; turning the dial unmutes it and changes the level, like the Windows volume keys. A slow player does not hold up Volume steps. When Windows fails to report the Volume, the Tray app keeps showing the last one and tries again after a short delay; Now Playing goes on either way. In the Simulator, the right and left arrow keys turn the dial, and so does the mouse wheel, one step per notch. The Tray app sends the Volume and the Device sends Volume steps (protocol version 3).
- GPLv3 license.

[Unreleased]: https://github.com/Jan-Eric-B/tiny-thing/commits/main
