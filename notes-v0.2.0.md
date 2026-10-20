Unreal Engine downloads work again.

- Since October 20, Cloudflare blocks unrealengine.com for anything that doesn't look like a web browser, so uehub couldn't refresh the engine list or install an engine ("Epic answered 403 Forbidden"). uehub now connects the way a browser does, and both work again, in the app and in the command line.
- If Cloudflare blocks uehub again some day, the error says so, instead of blaming your Epic account.

Which file to download:

- Debian, Ubuntu, Linux Mint, Pop!_OS: `uehub_0.2.0_amd64.deb`
- Fedora, openSUSE: `uehub-0.2.0-1.x86_64.rpm`
- Any other distribution, Arch Linux included until the AUR package is out: `uehub_0.2.0_amd64.AppImage` (make it executable, then run it)

If you already have uehub 0.1.0, it offers this update on its own. The `.sig` files and `latest.json` are for that updater; you don't need to download them.

The AppImage includes unmodified libraries from Ubuntu 22.04 (GTK, WebKitGTK and others) so that it runs everywhere. Each keeps its own license: their copyright files are inside the AppImage, in `usr/share/doc`, and their source code is available from Ubuntu at https://launchpad.net/ubuntu/jammy.

uehub is not affiliated with Epic Games.
