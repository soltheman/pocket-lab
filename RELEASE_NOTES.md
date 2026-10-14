# Pocket Lab 0.3.3

Smoother on the phone, every touch button does its job, and co-op over Wi-Fi.

**New in 0.3.3**
- Smoother: the game now runs at a locked 30 frames per second with its clock in step with the screen. Before, some frames moved the game ~1 ms and others ~65 ms, which made the view shake while you moved and looked around.
- Buttons:
  - SWAP no longer pulls out your grenade as if it were a gun.
  - NADE throws a grenade. You start with none, as in Black Ops, and get 2 to 4 at the end of each round (that top-up never happened before).
  - KNIFE plays the knife slash.
- Prompts: "Hold USE to ..." now shows for the thing you're facing (wall guns) or standing at (doors, debris). The "You must turn on the power first!" prompt no longer sits on screen at the start.
- Co-op over Wi-Fi (see below):
  - When you go down, you're downed, not dead. A teammate stands over you and holds USE for about 3 seconds to revive you. Zombies leave downed players alone.
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
