# 11: TinyThing OS dev image boots

**What to build:** A TinyThing OS dev image built locally and flashed onto the real Car Thing. It boots, appears on Windows as a USB network (CDC-NCM) without extra drivers, and is reachable over SSH with a key. This settles whether the yocto-superbird hardware layer can be built without Chromium.

**Blocked by:** None (can start immediately).

**Status:** open

- [ ] WSL2 with Ubuntu, a container runtime, kas and just set up on the PC; free disk space checked first
- [ ] yocto-superbird forked to the Jan-Eric-B account and pinned by commit (ADR-0001)
- [ ] `meta-tinything` layer with a minimal dev image: no browser, no compositor, no Bluetooth, no audio; build uses `rm_work` and the shared sstate mirror; actual disk use recorded
- [ ] Chromium layer question answered and, if needed, patched in the fork (recorded as a note or ADR)
- [ ] Full backup of the current Car Thing taken with superbird-tool before flashing, stored outside the repo
- [ ] Flashed with flashthing-cli (pinned version); Windows recovery-mode driver steps written down
- [ ] Device boots, Windows shows the USB network, SSH by key works, root login without a key does not
- [ ] Boot time to a shell measured and noted
