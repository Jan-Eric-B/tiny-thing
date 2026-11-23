# Own system image on yocto-superbird's hardware layer only

TinyThing ships its own TinyThing OS instead of running on the Thing Labs 8.9.2 firmware (Spotify's 2018 kernel, Chromium 69, RNDIS networking that macOS lacks) or on Nocturne. It uses only the `meta-superbird` hardware layer from yocto-superbird, pinned by commit in a TinyThing fork, plus an own `meta-tinything` layer with its own image. Writing an own BSP was rejected: it would mean redoing the kernel patches, the U-Boot port and the A/B logic for no gain, while the image on top is where TinyThing can be better than existing projects (no browser, no compositor, no Bluetooth).

## Consequences

- yocto-superbird is effectively maintained by one person and has no API stability promise; the pin and the fork protect against upstream changes and disappearance. Nocturne depending on the same layer makes upstream fixes likely.
- The hardware layer currently expects the Chromium layer to be present; if it must be built, the fork carries a patch to drop it.
- yocto-superbird marks a Slot good on boot without a health check; TinyThing replaces this with its own Confirm.
- First setup needs a Flash in USB recovery mode (flashthing, pinned to an exact version). flashthing cannot back up the eMMC, so a backup with superbird-tool comes before the first Flash.
