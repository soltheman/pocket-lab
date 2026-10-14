# Pocket Lab 0.3.0

Zombies. Kino der Toten now plays a real round 1: zombies spawn outside, tear the boards off the windows, climb in and come for you.

**New**
- Zombies spawn from Black Ops' own round scripts, fully textured, with their walk, attack and death animations.
- They go to a window, rip the boards off, climb through, then chase you over the map's own AI paths.
- Shooting them works and pays like Black Ops: 10 points a hit, 60 for the kill.
- They hurt. Solo, going down ends the game: "GAME OVER - You Survived N Rounds" over the theater, then the map starts over by itself after about 10 seconds.
- What you can buy is spelled out when you look at it: "Hold USE to buy Olympia [Cost: 500]".

**Known problems**
- Zombies sometimes get stuck on a ledge or behind a door that is still closed.
- Doors, power, perks and the mystery box are untested; many sounds are missing.
- No co-op yet.

**From 0.2.0**
- Spawn at Kino's start with the M1911; points and round on screen; wall buys (the Olympia by the start costs 500).

**Setup**
1. Install `Pocket.Lab.ipa` over the existing Pocket Lab (do not delete the app: that deletes the game files).
2. First time only: copy the staged `Call of Duty Black Ops` folder (`scripts/stage-bo1-for-iphone.ps1`) into Pocket Lab's folder (Apple Devices → Files → Pocket Lab).
3. Open Pocket Lab and stay in the app until the map shows. The first load after this update re-plans the map's navigation once, so it can take a little longer.

Problem reports: `iw4l-boot.log` from the app's folder.
