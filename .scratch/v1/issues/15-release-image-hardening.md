# 15: Release image hardening

**What to build:** A release variant of TinyThing OS that has no SSH at all, accepts only Device updates signed with the TinyThing release key, and is otherwise identical to the dev image. The keys for Device updates and for the Tray app updater are generated, kept outside the repo, and backed up.

**Blocked by:** 14

**Status:** open

- [ ] Release image builds without SSH; dev image keeps SSH by key
- [ ] Release key pair for Device updates generated; public key built into the image; private key stored outside the repo with a documented backup location
- [ ] Tray app updater key generated and stored the same way
- [ ] A Device update signed with a different key is refused by the release image (verified on the real Device)
- [ ] Documented how to switch a Device between dev and release images
