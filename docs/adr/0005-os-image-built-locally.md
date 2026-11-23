# TinyThing OS is built locally, not in CI

A Yocto build of TinyThing OS needs tens of GB of disk, while GitHub's standard runners have 14 GB, and no comparable project (Nocturne included) builds its image in CI. So TinyThing OS is built locally in WSL2 with kas, and CI builds only the Tray app and the Device app (cross-compiled for the Device). Because App updates go to a separate partition, most releases do not need a new TinyThing OS.
