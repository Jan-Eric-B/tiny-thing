# 16: Releases and self-update

**What to build:** Tagging a version produces a GitHub Release with a per-user Windows installer that bundles the matching Device app. The Tray app checks GitHub Releases on start and every 24 hours (unless turned off in settings), downloads in the background, and installs on my click or at the next start; afterwards the App update from ticket 13 brings the Device to the same version.

**Blocked by:** 09, 13

**Status:** open

- [ ] CI release workflow on a version tag: builds the Tray app, cross-compiles and signs the Device app package, bundles it, publishes the installer and updater metadata as a GitHub Release
- [ ] Installer is per-user, needs no admin rights, unsigned for Windows code signing; README explains the SmartScreen warning
- [ ] Update checks on start and every 24 h; "check updates" toggle and "check now" in the settings window
- [ ] Download in the background; install on click or at next start; never mid-use without consent
- [ ] Release notes taken from the CHANGELOG section of that version; Unreleased moved into the version section
- [ ] Verified end to end: install an older release, publish a newer one, the Tray app and then the Device update themselves
