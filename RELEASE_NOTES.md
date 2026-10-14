# Pocket Lab 0.1.3

Fixes screens drawing too big on the phone: the app now opens fullscreen at the screen's own size (a windowed iOS window takes the requested 1280x720 as its frame), and compiles graphics three at a time instead of one (the first load on an iPhone 16e was over three minutes), and names the map on the loading screen. If the Black Ops folder is missing the app still closes, but `iw4l-boot.log` now says where the folder goes. It opens straight into a walkable map from your own Black Ops files; there are no enemies, weapons or HUD yet.

**Setup**
1. Install `Pocket Lab.ipa` with SideStore and open the app once (it makes its folders, then stops at a "games not found" message or a black screen).
2. Copy the staged `Call of Duty Black Ops` folder (`scripts/stage-bo1-for-iphone.ps1`, about 6.9 GB) into **On My iPhone → Pocket Lab → Games** (Files app, or Apple Devices / iTunes file sharing on Windows).
3. Open Pocket Lab again. The first load is slow while graphics are prepared.

**Controls**
- Left side: a stick appears under your finger. Push past the ring to sprint.
- Anywhere else: drag to look.
- Right side: FIRE, AIM, RELOAD, USE, JUMP, CROUCH, KNIFE, NADE, SWAP. Holding FIRE or AIM and dragging also turns.
- A paired controller works too.

**Known**
- No zombies, weapons, HUD or menus yet.
- Some surfaces draw wrong and about 60 props are missing.
- Problem reports: send `iw4l-boot.log`, `iw4l-boot.prev.log` and `iw4l-memory-settings.txt` from the app's folder.
