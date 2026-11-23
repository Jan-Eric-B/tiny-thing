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
- GPLv3 license.

[Unreleased]: https://github.com/Jan-Eric-B/tiny-thing/commits/main
