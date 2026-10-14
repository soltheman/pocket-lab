# Pocket Lab 0.1.4

Touch buttons now answer where they are drawn, and the game runs two frames in flight on every iPhone (smoother, up to 60 fps).

**What changed**
- Touch buttons: the game laid itself out for the screen's safe area (750x369 pt on an iPhone 16e) while drawing and taking touches over the whole 844x390 pt screen, so presses could land on the wrong button. It now uses the whole screen.
- Smoother movement: the phone was held at 30 fps with each frame waiting ~26 ms for the next one; two frames in flight let it overlap work.
- Still from 0.1.3: fullscreen at the screen's own size; the match starts once the first view's graphics are ready (the rest load in the background, so a few surfaces may pop in).

**Setup**
1. Install `Pocket.Lab.ipa` over the existing Pocket Lab (do not delete the app: that deletes the game files).
2. First time only: copy the staged `Call of Duty Black Ops` folder (`scripts/stage-bo1-for-iphone.ps1`, about 6.9 GB) into Pocket Lab's folder (Apple Devices → Files → Pocket Lab).
3. Open Pocket Lab and stay in the app until the map shows.

**Controls**
- Left side: a stick appears under your finger. Push past the ring to sprint.
- Anywhere else: drag to look.
- Right side: FIRE, AIM, RELOAD, USE, JUMP, CROUCH, KNIFE, NADE, SWAP. There is no weapon yet, so only JUMP and CROUCH do something; JUMP while crouched stands up first.

**Known**
- No zombies, weapons, HUD or menus yet.
- Problem reports: `iw4l-boot.log` from the app's folder.
