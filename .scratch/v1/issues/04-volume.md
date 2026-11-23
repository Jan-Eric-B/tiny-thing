# 04: Volume

**What to build:** Turning the dial changes the computer's master Volume in 2 % steps, the Device briefly shows the Volume level, and Volume changes made on the computer show up on the Device. Includes the Windows `VolumeControl` adapter (Core Audio, default output device).

**Blocked by:** 01

**Status:** open

- [ ] `VolumeControl` interface (get, set, observe) with a fake for tests and a Windows adapter
- [ ] Seam 1 test: dial steps arrive as Volume changes, clamped to 0-100 %
- [ ] Seam 1 test: a Volume change on the computer reaches the Device
- [ ] Seam 3 test: dial turn produces Volume step actions and shows the Volume indicator, which hides again after a short time
- [ ] Mute state is shown when the computer is muted
- [ ] Switching the Windows default output device keeps Volume working (verified by hand)
