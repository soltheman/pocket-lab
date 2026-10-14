# Pocket Lab 0.3.1

Kino der Toten plays like Black Ops zombies now: zombies tear in through the windows and come for you, and the map's buyables work.

**New in 0.3.1**
- Sound: Black Ops' own zombie sounds play (zombie groans, the round start music, the box, perks, purchases). Earlier builds looked for them in the wrong place.
- Perks: buying one plays the drink and then hands your gun back (in 0.3.0 the bottle never went away).

**What works**
- Zombies spawn from Black Ops' own round scripts, rip the boards off the windows, climb in and chase you over the map's own AI paths, with their walk, attack and death animations.
- Shooting them pays like Black Ops: 10 points a hit, 60 for the kill. Clearing a round starts the next one.
- Everything you can buy says so when you look at it ("Hold USE to buy Olympia [Cost: 500]"): wall guns, doors and debris, the power switch, perks (Jugger-Nog tested) and the mystery box.
- Going down solo ends the game: "GAME OVER - You Survived N Rounds", then the map starts over by itself after about 10 seconds.

**Known problems**
- Zombies sometimes get stuck on a ledge or behind a door that is still closed.
- The mystery box does not show the weapons spinning before it gives you one.
- No co-op yet.

**Setup**
1. Install `Pocket.Lab.ipa` over the existing Pocket Lab (do not delete the app: that deletes the game files).
2. First time only: copy the staged `Call of Duty Black Ops` folder (`scripts/stage-bo1-for-iphone.ps1`) into Pocket Lab's folder (Apple Devices → Files → Pocket Lab).
3. Open Pocket Lab and stay in the app until the map shows. The first load after 0.3.x re-plans the map's navigation once, so it can take a little longer.

Problem reports: `iw4l-boot.log` from the app's folder.
