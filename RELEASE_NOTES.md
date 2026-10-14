# Pocket Lab 0.1.0

First iPhone test build. It opens straight into a walkable map from your own Black Ops files; there are no enemies, weapons or HUD yet.

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
