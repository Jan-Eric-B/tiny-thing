# Native Device app in Rust and Slint, no browser on the Device

The Device has 512 MB of RAM, and a browser would take most of it and most of the boot time, while low resource use is a core requirement. So the Device app is native Rust with Slint drawing directly to the display, sharing its language and message types with the Tray app, and running as an ordinary window on the computer as the Simulator. A web app in a newer Chromium (as Nocturne and bridgething do) was rejected despite faster UI iteration.

## Consequences

- Slint is free on embedded devices only under GPLv3, so TinyThing is licensed GPLv3.
- Slint cannot blur images, so the Tray app prepares the Backdrop.
