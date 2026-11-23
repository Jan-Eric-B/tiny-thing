# 12: TinyThing on the real Device

**What to build:** The Device daemon and the Device app run on the real Device from the TinyThing OS image. The Device app draws straight to the display, the real dial, buttons and touchscreen drive the same actions as in the Simulator, the Device daemon controls brightness, and the Tray app on Windows connects to it over the USB network and shows Now Playing on the real screen.

**Blocked by:** 01, 03, 04, 11

**Status:** open

- [ ] Device daemon and Device app built for the Device and included in the image (Device app on the app partition), started and supervised by the system
- [ ] Device daemon listens only on the USB network interface
- [ ] Dial turn, dial press, buttons 1-4, back button and touch mapped to the existing actions
- [ ] Brightness control from the Device daemon works on the real backlight
- [ ] The Tray app finds and connects to the real Device with no configuration
- [ ] Unplug and replug of the Device recovers by itself (verified by hand)
- [ ] Boot time to the first drawn screen and Device memory use measured and noted
