The first release of uehub: your Fab library and Unreal Engine, on Linux.

- Sign in with your Epic Games account, browse your Fab library, and add an asset to an Unreal project in one click. uehub picks the version that matches the project's engine and puts plugins where they belong.
- Downloads go through a queue that survives pauses, lost connections and restarts.
- uehub tells you when an asset you added has an update, and removing an asset deletes the files it added, and only those.
- It installs the prebuilt Unreal Engine releases Epic publishes for Linux, registers them for your projects, and tells you when a new patch is out.
- English and French, notifications when a download ends, and a log that is safe to attach to a bug report.

Which file to download:

- Debian, Ubuntu, Linux Mint, Pop!_OS: `uehub_0.1.0_amd64.deb`
- Fedora, openSUSE: `uehub-0.1.0-1.x86_64.rpm`
- Any other distribution, Arch Linux included until the AUR package is out: `uehub_0.1.0_amd64.AppImage` (make it executable, then run it)

These three update themselves from now on: uehub checks for new releases and installs them once you agree. The `.sig` files and `latest.json` are for that updater; you don't need to download them.

The AppImage includes unmodified libraries from Ubuntu 22.04 (GTK, WebKitGTK and others) so that it runs everywhere. Each keeps its own license: their copyright files are inside the AppImage, in `usr/share/doc`, and their source code is available from Ubuntu at https://launchpad.net/ubuntu/jammy.

uehub is not affiliated with Epic Games.
