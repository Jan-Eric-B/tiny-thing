# 05: Artwork, Backdrop and Accent

**What to build:** The Now Playing screen with Artwork, Backdrop and Accent. The Tray core turns the Media session's image into Artwork at the exact Device pixel size, a Backdrop and an Accent color, identifies the Artwork by a hash of its content, and sends an image only when the Device does not already have it. The Device app lays out the Artwork beside white text on the Backdrop and never rescales images.

**Blocked by:** 01

**Status:** open

- [ ] Downscaling with Lanczos3 in linear light; upscaling with Mitchell in sRGB, capped at 2x, smaller drawing beyond that
- [ ] Non-square Artwork (16:9 thumbnails) is shown whole and undistorted
- [ ] Backdrop: shrink to about 32 px (box), enlarge (Gaussian), darken until its brightest pixel has at least 4.5:1 contrast to white
- [ ] Accent: color from the Artwork adjusted to at least 4.5:1 contrast against the Backdrop
- [ ] Seam 1 tests: received images have the exact expected size; the Artwork id is stable for identical content and changes when only the image changes (the Tidal case: new title first, new image a moment later); Backdrop and Accent contrast hold for a set of test images including very bright, very dark and mixed covers
- [ ] The Device keeps the last few images and the Tray core does not resend them
- [ ] Images travel lossless
- [ ] Text is white and never drawn over the Artwork
