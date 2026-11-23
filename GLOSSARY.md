# TinyThing

TinyThing turns a Spotify Car Thing into a small desk display and controller for whatever is playing on one computer. It consists of a tray app on the computer and its own system image and software on the Car Thing.

## Language

### Parts

**Device**:
The one Car Thing paired with a computer. There is exactly one Device per computer.
_Avoid_: Client, CarThing (in prose), Superbird, thing

**Tray app**:
The TinyThing program on the computer. It lives in the notification area and has no window of its own except settings.
_Avoid_: Server, host app, desktop app, DeskThing

**Device daemon**:
The always-running TinyThing background program on the Device. It owns the connection to the Tray app, Device updates, Confirm and screen brightness, and keeps the Device reachable when the Device app fails.
_Avoid_: Agent, service, server

**Device app**:
The native TinyThing program that draws the Device's screen. It gets its data from the Device daemon.
_Avoid_: Client, webapp, UI

**Simulator**:
The Device app running as an ordinary window on the computer, standing in for a real Device.
_Avoid_: Emulator, preview

### Connection

**No link**:
The computer sees no USB network connection to the Device.
_Avoid_: Disconnected, offline

**Link**:
The USB network connection to the Device is up, but no TinyThing session is running over it.
_Avoid_: Connecting, half-connected

**Connected**:
The Tray app and the Device daemon have completed the handshake and are exchanging heartbeats.
_Avoid_: Online, paired

**Handshake**:
The first exchange of a connection, in which both sides state their protocol and software versions. A version mismatch triggers an App update.

### Getting software onto the Device

**Flash**:
Writing a complete system image to the Device while it is in its USB recovery mode. Needed only for the first setup or to recover a broken Device.
_Avoid_: Install, burn

**Device update**:
Replacing software on a running Device over its USB network connection, without recovery mode. Either an App update or an OS update.
_Avoid_: Push, deploy, flash

**App update**:
A Device update that replaces only the Device app.

**OS update**:
A Device update that writes a new TinyThing OS into the inactive Slot and switches to it on the next boot.

**TinyThing OS**:
The system image TinyThing ships for the Device, built on the community hardware support for the Car Thing.
_Avoid_: Firmware (for the whole image), ROM

**Slot**:
One of the two places on the Device that each hold a complete TinyThing OS. The Device boots one and keeps the other as fallback.
_Avoid_: Partition (for the whole slot), A/B side

**Confirm**:
Marking a freshly booted Slot as good once the Device app has run healthily for a while. An unconfirmed Slot is abandoned after three failed boots and the other Slot is used again.
_Avoid_: Mark good, commit

**Diagnostics**:
One archive holding the recent logs of the Tray app and the Device, saved on request for troubleshooting.
_Avoid_: Logs (for the archive), bug report

### Media

**Media session**:
What the operating system reports as currently playing in some app (title, artist, Artwork, playback state), together with its transport controls.
_Avoid_: Song, track (for the whole session), player

**Now Playing**:
The Media session the operating system marks as current. TinyThing shows and controls only this one and plays no audio itself.
_Avoid_: Local audio, current song

**Artwork**:
The image the Media session supplies for what is playing. It can have any aspect ratio (square covers, 16:9 video thumbnails).
_Avoid_: Album cover, thumbnail

**Backdrop**:
A blurred, darkened version of the Artwork that fills the screen area the Artwork itself does not cover. Its brightness is capped so white text on it is always readable.
_Avoid_: Background, glow

**Accent**:
A color taken from the Artwork and adjusted until it stands out clearly on the Backdrop, used for the progress bar and icons.
_Avoid_: Theme color, glow color

**Volume**:
The computer's master output volume. It is not the volume of an individual app.
_Avoid_: App volume, session volume
