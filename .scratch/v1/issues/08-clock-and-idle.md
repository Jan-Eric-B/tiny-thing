# 08: Clock and idle

**What to build:** The Device shows a large clock when nothing has played for two minutes or when the back button selects it. The Tray core sends time and time zone on every connect; the Device keeps the clock running when disconnected, and shows no clock at all before it has been Connected once since boot.

**Blocked by:** 01

**Status:** open

- [ ] Time and time zone sent after every Handshake
- [ ] Seam 3 tests with a fake clock: clock screen after 2 minutes with nothing playing; back to Now Playing when something plays; back button toggles; no clock before the first Connected since boot; clock keeps running after the connection drops
- [ ] Clock formatting follows the computer's 12/24 h setting
- [ ] Daylight-saving and time-zone changes on the computer reach the Device on the next update
