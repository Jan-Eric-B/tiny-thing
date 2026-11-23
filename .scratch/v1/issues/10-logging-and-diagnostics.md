# 10: Logging and diagnostics

**What to build:** Logs that can never grow without bound, and one button to collect them. The Tray app writes rolling log files (at most 5 files of 2 MB, level info); the Device keeps its logs in RAM (at most 8 MB) and keeps only crash reports across reboots. "Save diagnostics" in the settings window fetches the Device logs over the connection and saves them with the Tray app logs as one archive.

**Blocked by:** 09

**Status:** open

- [ ] Tray app log rotation and size cap (test that writing far past the cap keeps total size bounded)
- [ ] Device daemon and Device app log caps
- [ ] Seam 1 test: Save diagnostics returns an archive containing both sides' logs; with No link it still saves the Tray app logs and says the Device part is missing
- [ ] Dev builds log every protocol message in readable form; release builds do not
- [ ] Save diagnostics button lets me choose where the archive goes
