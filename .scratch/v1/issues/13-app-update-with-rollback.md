# 13: App update with rollback

**What to build:** When the Handshake shows that the Device app differs from the one bundled in the Tray app, the Tray app sends a signed Device app package; the Device daemon checks the signature, installs it beside the current one, starts it, and rolls back to the previous Device app if the new one crashes on start. The Device stays reachable throughout (ADR-0004).

**Blocked by:** 12

**Status:** open

- [ ] Device app package format with a signature over its content
- [ ] Seam 2 tests (fake file system and fake process supervision): valid package installed and started; package with a bad signature or truncated content refused and the current Device app kept; new Device app crashing on start leads to rollback and a report to the Tray app
- [ ] Seam 1 test: a version mismatch in the Handshake triggers exactly one App update
- [ ] Progress of the App update visible in the tray icon tooltip
- [ ] Verified on the real Device, including a deliberately crashing build
