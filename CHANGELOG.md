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
- GPLv3 license.

[Unreleased]: https://github.com/Jan-Eric-B/tiny-thing/commits/main
