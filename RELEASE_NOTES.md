# Bo1ZPocket 0.3.31

Smoother on the phone, every touch button does its job, and co-op over Wi-Fi.

**New in 0.3.31**
- **New icon:** the Call of Duty: Black Ops Zombies logo you sent, on black.
- **No more red screen at spawn.** Just before you spawn, Black Ops sets your max health to 0. That got rounded up to 1, so you spawned with 1 health and the screen stayed blood-red while it crept back up. Now you spawn with full health.
- **Getting hit turns the screen red.** It uses Black Ops' own blood-edged overlay: one zombie hit leaves it plainly red (one more and you go down), and it fades as you heal. Before, a hit showed nothing, because the overlay used images a Black Ops install doesn't have.
- **Head shots count:**
  - Each gun now does its own Black Ops damage for where it hits (for example the M1911 ×3.5 to the head, the MP40 ×4). Before, a head shot did body damage.
  - Black Ops' scripts now get the real hit location (head, neck, torso, arm, hand, leg, foot). So head-shot kills pay head-shot points, and heads and limbs pop the way Black Ops decides. As in Black Ops, the starting M1911 never pops heads.
- **Pause actually pauses.** In a solo game, the pause/options menu (and button edit mode) now freezes the game, as in Black Ops. Tapping in the menu no longer fires your gun.
- **Aim button toggles.** One tap aims, the next tap stops; the button stays lit while you aim. You can still drag on it to look around. To get the old hold-to-aim back, set OPTIONS → BUTTONS → AIM BUTTON to HOLD.
- **A zombie behind you growls.** Black Ops plays a special growl when a zombie is close behind you, about once every 5 s at most. It never played here, because the script checks whether each zombie is a dog using a value the game itself has to provide, and we didn't. It plays now.
- **Zombies no longer get stuck at the foyer window** (the one with "Hold USE to Rebuild Barrier" next to the stairs).
  - **What happened:** they cut a corner towards the window and walked into the wall beside it, until Black Ops' failsafe killed them.
  - **Now:** they walk round the corner first.
  - **Test, 3 min standing in the foyer:** 21 of 26 zombies got inside (before: 14 of 21) and none stood stuck (before: 4–6). The rest were still at their windows or just spawned.
- **The black behind the windows is how the map is.** There's nothing to draw out there: Kino has no scenery behind those boards, only the ground zombies walk on.
- **Checked on PC:**
  - spawn and hit screens;
  - head-shot damage, points and head pops;
  - pause in solo, and menu taps not firing;
  - aim toggle on and off;
  - the behind-you growl playing;
  - the window test above, and a doors-open run across the whole map with no zombies stuck.

  Not tried on an iPhone yet.
- **Still need from you:** where zombies walked over a boundary they shouldn't. Tell me the spot and what they crossed, and I'll fix it.

**New in 0.3.30**
- **New name: Bo1ZPocket.** It's the same app: it installs over Pocket Lab and keeps your games folder and settings. In the Files app the folder now shows as Bo1ZPocket.
- **Black Ops' own font for all the text.** That covers the points, ammo and weapon name, the buy and door hints ("Hold USE to Open Door [Cost: 750]"), game over, the main menu, the options and the touch-button labels.
  - It's read from your Black Ops install (its font sheet and font tables), so nothing is bundled.
  - If the font can't be read, the old one is used.
- **Smoother zombies.** Their movement looked rough for two reasons:
  - **Animation stepped.** It only moved forward with each update from the game, 20 times a second, so walks and swings stepped. Now it moves forward every frame.
  - **Position overshot.** The game guessed where each zombie was heading, so it overshot whenever one turned and then snapped back. Now each zombie is drawn between its last two known positions, one update behind, so its path is smooth.

  The swings already ran at Black Ops' speed. They looked too fast because they stepped at 20 frames a second.
- **Blood when you shoot zombies:**
  - It sprayed sideways off the hit box. Now it sprays back toward you, as in Black Ops.
  - Black Ops' blood effect has a near version and a far version (a big burst meant to be seen from across the map). Both played at every distance, so up close a big red haze showed beside the zombie. Now each plays only at its own distance, as in Black Ops. The same goes for every other effect with near and far versions (smoke, dust, light glows).
- **Checked on PC:**
  - the font on the HUD, hints, menu and options at the iPhone's screen size and pixel density;
  - zombies drawn smoothly between updates (position and animation every frame);
  - the blood's direction and the near/far split, in screenshots.

  Not tried on an iPhone yet.
- **Not right yet:** up close the blood is now a quick, small puff, and its colour is still lighter than Black Ops' dark red. Tell me how it feels.

**New in 0.3.29**
- **Zombies walk like Black Ops zombies.** Black Ops moves a zombie by its walk animation itself, so its feet stay planted. Here every walker moved at one fixed speed, faster than any of its walks, so their feet slid along the floor. Now each zombie uses the walk Black Ops' script picks for it and moves at that walk's own pace, step by step. Kino's walkers go about half the old speed; some shamble even slower, as in Black Ops.
- **No more instant hits.** A zombie that got close damaged you that same instant. Now it swings like in Black Ops:
  - it uses one of Black Ops' attack animations (standing swipes, or walking and running ones that keep coming);
  - the hit lands at the swipe's strike, and only if you're still within reach and in front of it;
  - backing off makes it miss, and you have time to knife it first.
- **Points on a blood stain**, bottom right as in Black Ops (top right with the touch buttons). It uses Black Ops' own score smear, coloured blood red.
- **Ambience.** Kino's room tone now plays during a game. Black Ops starts it from client scripts this game doesn't run.
- **Touch buttons: layouts, icons, and moving them yourself.** In OPTIONS → BUTTONS:
  - **Button layout:**
    - CLASSIC, as before;
    - LEFT FIRE adds a second fire button above the move stick;
    - COMPACT pulls the buttons in and makes them smaller;
    - SOUTHPAW mirrors everything, with the buttons on the left and the stick on the right;
    - CUSTOM is your own.
  - **Button style:** new icons (bullet, scope, reload arrow, hand, jump and crouch arrows, knife, grenade, swap) or the old text.
  - **Edit** (in a game): drag any button where you want it, then DONE. RESET goes back to CLASSIC.

  Size and opacity work with every layout.
- The options are now on two pages, GAMEPLAY and BUTTONS.
- **Checked on PC:**
  - every layout and both styles at an iPhone's screen size;
  - edit mode moving and saving a button;
  - zombies' walk paces read from the animations;
  - their swings landing on the strike;
  - the blood-stain points;
  - Kino's room tone starting;
  - a 3-minute game with no zombie stuck.

  Not tried on an iPhone yet.
- Not done yet: when you knife a zombie slightly out of reach, Black Ops lunges you onto it. Here the lunge still only goes for other players.

**New in 0.3.28**
- **A main menu.** The app now opens on a zombies menu in Black Ops' style instead of dropping you straight into Kino:
  - Kino's loading-screen art drifts behind a dark fade, under ZOMBIES and KINO DER TOTEN.
  - SOLO loads the map; OPTIONS opens the options.
  - With co-op set up the button says CO-OP. Joining someone else's room still goes straight in.
- **Options**, from the menu or, in a game, from the new pause button at the top left:
  - look sensitivity (5.0 as before);
  - field of view;
  - volume;
  - invert look;
  - button size and opacity for the touch controls.

  Use the – and + buttons, or tap or drag a bar. Your settings are saved. The game keeps running while the options are open; there is no pause yet.
- **Points and ammo.** The points move to Black Ops' place, with a new ammo counter: weapon name, the clip large and the reserve beside it ("8 / 32"), the clip red when low. With the touch buttons on, they sit along the top right, clear of the buttons.
- **Touch controls**:
  - dark buttons that light up orange while pressed, and a red FIRE button;
  - a faint ring where the move stick rests;
  - the stick turns orange while you sprint;
  - size and opacity follow the options.
- **Blood.** Shooting a zombie now plays Black Ops' own blood effects, bigger for head shots and kills. Bullets used to make the dust and sparks a wall makes.
- **Zombies that rise out of the ground** (Kino's theater, once the power is on) now climb out and come for you. They used to stand where they spawned. In a 3-minute test with the power on, 22 of 24 zombies got into play (before: 11 of 18). Two still got stuck at one theater window.
- **Starting ammo.** Black Ops counts a weapon's starting ammo in reserve, on top of a full clip. The game counted the clip in it, so every Black Ops weapon started a clip short. The pistol now starts with 8 / 32, not 8 / 24.
- **New icon**: glowing zombie eyes over a red chalk round tally.
- **Checked on PC:**
  - menu, options and the pause button at an iPhone's screen size;
  - blood on hits;
  - the risers;
  - starting ammo;
  - a 3-minute game in the foyer with no zombie stuck.

  Not tried on an iPhone yet.

**New in 0.3.27**
- Zombies move more like Black Ops zombies:
  - **No more floating through windows and off ledges.** Black Ops moves a zombie through a window, off a ledge or over a wall along the path built into that climb or jump animation. This game slid it in a straight line while a generic climb animation played, so zombies glided through window sills and floated down from ledges. Each window and drop now plays the animation Kino's map gives it (the window climb for the zombie's walk, run or sprint speed; jumps down 40, 72, 120 and 190), and the zombie follows that animation's own path up onto the sill and down to the floor. The game wasn't reading that path out of Black Ops' animation files at all; it does now.
  - **No more gliding over things to get unstuck.** When a zombie couldn't move, the game slid it straight to where it was headed, through whatever was in the way. Testing found what was catching them:
    - landing from a window 1 or 2 units into the floor. They now land standing on it.
    - Kino's closed stage curtain. Black Ops keeps zombies from routing through it until the power opens it. Zombies here took that route anyway and glided through the curtain.
    - Before, a 3-minute test with every door open had 18 of those slides. Now: none with every door open and the power on, and none in 3 minutes in the foyer with the doors shut.
  - **No zigzag.** Zombies walked to each point of Kino's path grid in turn and turned sharply at every one. They now head for the furthest point ahead they can walk straight to, and turn gradually. A shortcut that walks a zombie into a wall is never tried again.
  - **Smoother changes between animations.** Going from walking to attacking or climbing blends over 0.2 seconds, as in Black Ops, instead of snapping to the new pose.
- The knife works: melee now hurts zombies (one stab kills a round 1 zombie, as in Black Ops).
- The crosshair turns red when it's on a zombie.
- Coming out of a sprint uses Black Ops' own timing for each weapon (0.3 seconds for the pistol). The game wasn't reading Black Ops' sprint timings at all.
- Checked on PC:
  - 3 minutes in the foyer with the doors shut: 38 zombies, none stuck, 32 window climbs, all with Black Ops' climb animations, none sliding;
  - close-up screenshots of zombies climbing in through a foyer window;
  - zombies jumping off the stage into the theater: they step to the edge, then drop along the jump's curve and land on the floor (before: a straight slide down through the air);
  - knifing round 1 zombies kills them;
  - the red crosshair on a zombie.
- Found while testing, not fixed yet: once the power is on, the zombies that climb up out of the ground in Kino's theater stay where they spawn. The Black Ops script that raises them fails here. That's next.

**New in 0.3.26**
- First part of Black Ops' own HUD. The round counter is now Black Ops' dark red chalk tally, bottom left: one mark per round up to five, then a crossed five plus more marks up to round 10. The old red number at the top left is gone. Perks you buy show their Black Ops icon above the chalk.
  - Why they were missing: Black Ops draws these from its scripts with images that live in its own archives. The HUD only knew how to find Multiplayer's (MW2's) images, and it stretched images to the height of text (the chalk carries a giant text size for later rounds).
  - On iPhone the chalk and perk icons stay clear of the rounded corner and the Dynamic Island.
  - Checked on PC: rounds 1, 4 and 6 show the right chalk; buying Jugger-Nog shows its icon and takes 2,500 points.
- Not yet Black Ops' own: the points (still the yellow number top right), the ammo counter, weapon name and the round number after round 10. Those come from Black Ops' menu files and fonts, which are next.

**New in 0.3.25**
- Each character's own arms in first person. Black Ops gives each Kino character their own: Dempsey (player 1) the POW arms, Nikolai (player 2) the prisoner arms, Takeo (player 3) the NVA arms, Richtofen (player 4) the hazmat arms. Black Ops' weapons only carry stand-in hands (the grey ones you've had). The script's choice now reaches your phone, and every weapon is held in those arms. Checked on PC: Dempsey's arms on the pistol and the MP5K, including the MP5K's first-raise animation.
- Not done yet: Black Ops' own HUD (next).

**New in 0.3.24**
- Zombies have eyes again, glowing orange like in Black Ops:
  - **No more see-through heads.** The eyeballs are in the zombie head models, but their shader needs a view matrix the renderer never supplied, so every eye was skipped and you saw through the sockets. It's supplied now.
  - **The glow.** Black Ops lights each zombie's eyes when it spawns and turns them off when it dies or loses its head. The game ignored that, and the eye bone lives on the head, which is a separate model, so the effect also had nowhere to attach. Effects now find bones on attached models, and they follow the animated pose instead of a stiff default pose, so the glow stays on the eyes as the zombie moves.
  - Checked on PC with close-up screenshots of zombies attacking: solid eyes, orange glow on them. A 3-minute zombie test is unchanged: none stuck.

**New in 0.3.23**
- The starting pistol no longer shoots zombies' limbs off. Black Ops never takes a limb off with the M1911 and checks that by its weapon name, `m1911_zm`. This game reported every Black Ops weapon with a multiplayer `_mp` ending (`m1911_zm_mp`), so the check never matched. Weapons now go by their real names, which also fixes any other Black Ops script that compares weapon names. Checked on PC with a test player shooting zombies:
  - starting pistol, before: 29 hits, 6 limbs off; now: 24 hits, none;
  - MP5K wall gun, now: 16 hits, 4 limbs off (as in Black Ops, where other guns still do).

**New in 0.3.22**
- Zombie pathfinding, mostly for once doors are open:
  - **Zombies follow their spawner's instructions.** Each place zombies spawn tells Black Ops how they should come in, and none of it ever reached the zombie:
    - Half of Kino's 50 spawn points (back rooms, balconies, stage) send their zombies straight at you instead of to a window. Those zombies went looking for a window anyway. Where their room has no way to one, they stood still until Black Ops removed them about a minute later.
    - The two spawn points by the lobby name the window their zombies use. Their zombies instead picked one of the 3 nearest windows at random, so upstairs zombies sometimes went for a downstairs window. Following that instruction needed a path-point lookup the game didn't have.
  - **Zombies no longer fall through the back stairwell.** Its steps are a ramp that only stops players and AI, and the game only applied the player part, so zombies dropped into the room below, where there are no paths. They now climb the stairs to their window.
  - **Zombies no longer bounce at the top of the east stairs.** On their way to the theater, some zombies headed straight for a point at the foot of the stairs, hit the rail, and went back and forth.
- Checked on PC, 3 minutes per test, player unkillable so zombies keep coming:
  - every door forced open, player at the start: 29 of 30 zombies got inside and none got stuck (the last had spawned 14 seconds before the end). In 0.3.21: 23 of 36.
  - every door forced open, player in the theater: none stuck; 6 of 7 crossed the map to the player.
  - doors closed, player at the start and at the foot of the lobby stairs: no zombie stuck, same as 0.3.21.
- Not checked yet: the theater's own spawn points, including two where zombies climb out of the ground. In my tests the theater never switched on as an area, because the player was moved there rather than walking in. They now get their instructions like the others, but I haven't seen them spawn.

**New in 0.3.21**
- The low walls under Kino's boarded windows show again. They're the splintered wooden panels zombies climb over. Black Ops keeps those two models in the map's language file (`en_zombie_theater`), and the map itself only refers to them by name. The game never looked there for models, so all 12 were missing and left a gap down to the floor. They now load from that file, are solid like the rest of the map, and zombies still climb in over them. Checked on PC:
  - the lobby window shows its wall where it showed a black gap before;
  - two 3-minute tests (player at the start, then right next to that window): no zombie got stuck, and as many came in through that window as before.
- Not fixed yet: perk bottles and the teddy bear in the mystery box are missing the same way. They aren't in the language file, and where they are is next. Zombies that lose their legs don't crawl.

**New in 0.3.20**
- Zombies no longer T-pose when they lose a limb. Since 0.3.19, Black Ops' limb-loss script runs to the end (before, it stopped early). It swaps the zombie's body for a damaged one, and this game treated the swap as a brand-new model with no animation. The animation now carries on through the swap. Checked on PC: zombies that lost an arm keep walking and reaching.
- Not fixed yet: zombies that lose their legs should switch to crawling and don't.

**New in 0.3.19**
- Fewer of Black Ops' scripts stop partway through. Each of these used to stop the script that ran into it:
  - Sounds that tell the script when they've finished (voice lines waiting their turn) played, then stopped their script. They now report back about 2 seconds later.
  - Asking whether a player has a weapon this game has no definition for (the zombie-kill voice lines ask about the sickle) stopped the script instead of answering no.
  - Comparing two lists, and a dozen engine calls this game has nothing to show for yet: the teleporter's "transported" screen, rope collision, a wall gun's hidden parts, head gibs, vision sets, saving, and similar.
- Checked on PC: those script errors no longer appear in a match. Each effect isn't checked on screen one by one; the teleporter in particular still needs a play-through.

**New in 0.3.18**
- Power-ups drop. When a zombie dies, Black Ops' script finds the floor under it with a ground check this game didn't have yet. The script stopped there every time, so nothing ever dropped. Checked on PC: an Insta-Kill skull, a Max Ammo and a Double Points dropped inside the lobby, each looked like the real thing, and walking onto the Double Points picked it up and doubled the points.

**New in 0.3.17**
- Zombies no longer get stranded outside windows before they come in:
  - A zombie within a few steps of its spot at a window walked straight at it, even with a wall in between. At one lobby window that spot is round the corner of the frame, so it pressed into the wall for good. Now it only walks straight when nothing is in the way.
  - A zombie upstairs, right above a ground-floor window, counted as having arrived at it. It then tried to tear boards 240 units below it. Arriving now needs it on the same floor. Black Ops then sees the window is out of reach and sends it to another.
  Checked on PC with two 3-minute tests (player at the start and at the foot of the lobby stairs): no zombie was stuck anywhere, against 10 in 0.3.15.

**New in 0.3.16**
- Zombies coming down the lobby stairs no longer get stuck halfway, walking up and down on the spot. Halfway down, the nearest path point is a dead end that only leads back up to the landing. Each time a zombie re-planned (once a second) it started from that point, so it turned back up, then down again on the next plan. A zombie that is inside now starts its route from whichever point makes the whole way shortest.
  Checked on PC, 3 minutes with the player at the foot of those stairs: zombies stuck for 30 seconds or more went from 10 (6 on the stairs) to 2, and none on the stairs. Those 2 were outside one lobby window that zombies sometimes can't get through; that was already happening before and is next.

**New in 0.3.15**
- Zombies no longer plan their way through closed doors. Black Ops marks every closed door and debris pile as blocking the paths through it, and unblocks them when you buy it; the game ignored both, so zombies walked into closed doors and kept pressing against them. Checked on PC: the door pieces block between 2 and 27 path links each at the start of a match.
- Zombies use the map's own jump-down and climb-up spots (54 in Kino), going up then across or across then down instead of walking into the ledge.
- A zombie stuck against a rail or ledge it could hop over no longer gets refused because of walls that only block players.
- Not yet seen on screen: a zombie using a jump or climb spot (none needed one in a 2-minute test standing at the start).

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
