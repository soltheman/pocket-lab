# Pocket Lab 0.3.14

Smoother on the phone, every touch button does its job, and co-op over Wi-Fi.

**New in 0.3.14**
- Doors, debris and the boards on the windows now show. They are pieces of level geometry Black Ops' scripts move and hide, and none had ever been drawn, for three reasons:
  - the game never read them from the map file;
  - the marker saying "this object is one of those pieces" was dropped on its way from the game to the renderer;
  - the GPU code placed every piece of level geometry where the map was built, ignoring where the scripts had moved it.
  Checked on PC: the foyer door, the doorway under the lobby arch and a boarded window all show. Each piece is drawn where the game currently has it and not at all once hidden, so boards a zombie tears off should disappear and a bought door should swing open. That part isn't checked on screen yet.
- Co-op: every player needs 0.3.14; it can't join a game run by an older version.

**New in 0.3.13**
- Likely fix for the shaking, flashing and doubled HUD. 0.3.12's self-test separated the batches of GPU work that really fail from the ones that only failed alongside them. The failing ones carry nothing but GPU timestamps. A built-in timing tool was writing timestamps into the GPU's work around every rendering step, every frame, and the iPhone rejects that work with "Insufficient Memory". Each rejection also took the rest of that frame's work with it, which showed as old, black or doubled frames. The timing tool no longer uses GPU timestamps on iPhone.
- If GPU work still fails 8 seconds into a match, the self-test from 0.3.12 runs again and keeps the best setting. Otherwise it doesn't run.

**New in 0.3.12**
- A self-test for the shaking. 0.3.11's log shows about 11 of the roughly 41 batches of GPU work the game sends each frame failing with "Insufficient Memory", even batches with nothing in them. So the problem is in how the work is handed to the GPU, not in what is drawn. Starting 8 seconds into a match, the game tries five changes to that hand-off for 10 seconds each (no pacing; not sending empty batches; sending each batch on its own; one frame on the GPU at a time; all of them together), logs how many batches fail under each, then keeps the best one. The picture may stutter or slow down during that minute.

**New in 0.3.11**
- Still no fix for the shaking. 0.3.10's log ruled out graphics memory: the graphics chip held 102 MB of its 5.4 GB limit when the "Insufficient Memory" failures began. So one batch of GPU work is going over some per-batch limit. The log now describes each failed batch (its drawing passes and screen size, number of draws and triangles, texture uploads, bound resources, how many batches were queued) next to batches that worked.

**New in 0.3.10**
- No fix yet for the shaking and black/pink frames. This build finds their cause. In 0.3.9's log, the iPhone's graphics chip rejects almost every frame with "Insufficient Memory", starting on the loading screen while the game used under 1 GB of the 3.3 GB iOS allows. The log now records how much memory the graphics chip itself holds, Metal's limit for it, and every graphics allocation of 16 MB or more by name and size.

**New in 0.3.9**
- The log names which part of a frame the GPU rejects, with Metal's own error text.

**New in 0.3.8**
- About 1.2 GB less memory, measured rather than guessed: each of the game's ~2,000 shader programs kept a 600 KB parsed copy of its source alive (23x the source) after the GPU no longer needed it. The graphics library held it for the life of every pipeline; on iPhone it now lets go once the pipeline is built.
- The log now counts GPU frames that failed, and why (out of memory, timeout and so on). A failed frame is what shows an older frame again, or black: the shaking you see.

**New in 0.3.7**
- Fixes the old frames showing up, stacked stick knobs, and doubled text (most likely also the "shaky" look). The 3D view now always replaces the whole screen each frame. Before, on the phone it could be blended over the screen instead, so older frames and the HUD from several frames back showed through.
- The log now lists which cameras draw the screen, and notes any frame where the 3D view failed to copy to the screen, with the reason.
- Still to do: memory. The game still uses about 3 GB on an iPhone 16e. It hasn't crashed in any of the logs, but it's close to the limit.

**New in 0.3.6**
- Another big memory cut: the game kept the full source of every compiled shader (around 1.2 GB on the iPhone) after it no longer needed it.
- No more doubled HUD (two hint texts, two stick knobs on top of each other) or pink frames: on a frame where the 3D view wasn't drawn, the buttons and text were drawn over an older frame or leftover memory. That frame is now black, and the log counts how often it happens.

**New in 0.3.5**
- Much less memory, and the end of the black and pink flashes mid-game they most likely caused. Your 0.3.3 session ran within about 0.1 GB of the iPhone's limit. Two things were eating it:
  - With sound on, the game retried building a menu sound bank that Black Ops doesn't have about every 2 seconds, re-indexing all 29,000 sound files each time. Now it tries once.
  - Kino loaded a bot path map it doesn't use (zombies follow Black Ops' own paths). That cost up to 2.5 minutes of loading and about 1.4 GB. It's skipped now.
- Loading Kino should be much faster.

**New in 0.3.4**
- No pink flash before the loading screen: the screen behind the game is now black while the game starts.

**New in 0.3.3**
- Smoother: the game now runs at a locked 30 frames per second with its clock in step with the screen. Before, some frames moved the game ~1 ms and others ~65 ms, which made the view shake while you moved and looked around.
- Buttons:
  - SWAP no longer pulls out your grenade as if it were a gun.
  - NADE throws a grenade. You start with none, as in Black Ops, and get 2 to 4 at the end of each round (that top-up never happened before).
  - KNIFE plays the knife slash.
- Prompts: "Hold USE to ..." now shows for the thing you're facing (wall guns) or standing at (doors, debris). The "You must turn on the power first!" prompt no longer sits on screen at the start.
- Co-op over Wi-Fi (see below):
  - When you go down, you're downed, not dead. A teammate stands over you and holds USE for about 3 seconds to revive you. Zombies leave downed players alone. If nobody revives you, you bleed out after about 45 seconds, watch as a spectator, and come back at the start of the next round.
  - A player who joins lands on their own start point and can move. Before, Black Ops' own rule killed both players for standing in the same spot.
  - A host that stalls for a moment no longer drops the other player.
- Jugger-Nog now actually raises your health.

**Co-op (up to 4 players on the same Wi-Fi)**
1. On a Windows PC on the same Wi-Fi, run `Pocket-Lab-Co-op-Server.exe` (from this release). The first time, Windows may ask to allow it on private networks; allow it. Leave the window open.
2. It makes a file called `Pocket Lab Co-op.iw4l-server` in a `co-op` folder next to it. Copy that file to every phone that will play: Apple Devices → Files → Pocket Lab, next to the `Games` folder (or AirDrop it and save it to On My iPhone → Pocket Lab).
3. Open Pocket Lab on the first phone. The first time, iPhone asks to let it find devices on your local network: allow it, then close Pocket Lab and open it again (that first try plays solo). With no game running yet, it hosts one.
4. Open Pocket Lab on the next phone (same "allow, then reopen" the first time). It finds that game and joins it.
5. To play from the PC instead of a second phone: run `scripts\play-kino-on-pc.cmd` from the source folder, with the co-op file next to `target\play\iw4l.exe`.

Without the file, Pocket Lab plays solo as before. If the PC server isn't running, it waits about 4 seconds, then plays solo.

**What works**
- Zombies spawn from Black Ops' own round scripts, rip the boards off the windows, climb in and chase you over the map's own AI paths, with their walk, attack and death animations.
- Shooting them pays like Black Ops: 10 points a hit, 60 for the kill. Clearing a round starts the next one.
- Wall guns, doors and debris, the power switch, perks and the mystery box can all be bought.
- Going down solo ends the game: "GAME OVER - You Survived N Rounds", then the map starts over by itself after about 10 seconds.

**Known problems**
- Zombies sometimes get stuck on a ledge or behind a door that is still closed.
- The mystery box does not show the weapons spinning before it gives you one.
- Co-op has been tested with two copies of the game on one PC (joining, playing, going down and being revived), not yet on two phones.
- The "revived" voice line doesn't play.

**Setup**
1. Install `Pocket.Lab.ipa` over the existing Pocket Lab (do not delete the app: that deletes the game files).
2. First time only: copy the staged `Call of Duty Black Ops` folder (`scripts/stage-bo1-for-iphone.ps1`) into Pocket Lab's folder (Apple Devices → Files → Pocket Lab).
3. Open Pocket Lab and stay in the app until the map shows.

Problem reports: `iw4l-boot.log` from the app's folder.
