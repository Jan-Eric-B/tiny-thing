# Device daemon and Device app are separate programs

The Device runs two programs instead of one: the Device daemon (in TinyThing OS, owns the connection, Device updates, Confirm and brightness) and the Device app (on the separate app partition, draws the screen only). A single program would be simpler, but a bad App update could then crash the only thing able to receive a fix, leaving a Flash in recovery mode as the only way out. With the split, the Device daemon rolls back to the previous Device app when a new one crashes and stays reachable for the Tray app.
