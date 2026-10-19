pub(crate) const T6_EQUIPMENT: &str = include_str!("t6_equipment.gsc.txt");

/// BO1 zombies HUD feed: BO1 draws points and the round through script HUD elements and
/// its own menus, which this runtime does not present, so each player's points and the
/// round number go out as client dvars (`ui_zm_points`, `ui_zm_round`) for the client's
/// zombies HUD to draw.
pub(crate) const ZOMBIES_HUD: (&str, &str) = (
    "iw4l_zombies/hud",
    r#"main()
{
    // BO1 zombies has no prematch; this is what moves the match to playing (entity
    // thinks, model animation and shot history all wait for it).
    level notify("prematch_over");
    level thread hud_feed();
    level thread game_over();
    level thread room_tone();
    level thread place_joiners();
    if (getdvar("iw4l_zm_watch") == "1")
        level thread watch_zombie();
    if (getdvar("iw4l_zm_watch") == "aim")
        level thread aim_at_zombies();
    if (getdvar("iw4l_zm_watch") == "die")
        level thread down_first_player();
    if (getdvar("iw4l_zm_watch") == "buy")
        level thread face_wall_buy();
    if (getdvar("iw4l_zm_watch") == "door")
        level thread face_door();
    if (getdvar("iw4l_zm_watch") == "power")
        level thread face_power_switch();
    if (getdvar("iw4l_zm_watch") == "window")
        level thread face_window();
    if (getdvar("iw4l_zm_watch") == "tour")
        level thread tour();
    if (getdvar("iw4l_zm_watch") == "at")
        level thread stand_at();
    if (getdvar("iw4l_zm_watch") == "box")
        level thread face_box();
    if (getdvar("iw4l_zm_watch") == "perk")
        level thread face_juggernog();
    if (getdvar("iw4l_zm_watch") == "nuke")
        level thread kill_all_zombies();
    if (getdvar("iw4l_zm_watch") == "volumes")
        level thread log_player_volumes();
    if (getdvar("iw4l_zm_watch") == "revive")
        level thread revive_test();
    if (getdvar("iw4l_zm_watch") == "link")
        level thread link_teleporter();
    if (getdvar("iw4l_zm_watch") == "powerup")
        level thread drop_powerup();
    if (getdvar("iw4l_zm_watch") == "dogs")
        level thread spawn_dogs();
    if (getdvar("iw4l_zm_watch") == "audit")
        level thread audit_rounds();
    if (getdvar("iw4l_zm_watch") == "reel")
        level thread camera_reel();
}

// `animScripted` on a script model (Kino's stage curtains): the engine plays the clip,
// and when it is through the model is notified as BO1's engine does.
model_anim_scripted(note, seconds)
{
    self endon("death");
    wait seconds;
    self notify(note, "end");
}

// A notify the engine sends a little later (a death clip's "end" note).
notify_after(note, value, seconds)
{
    wait seconds;
    if (isdefined(self))
        self notify(note, value);
}

// The map's room tone, which BO1 plays from its client scripts (`<map>_amb.csc`:
// `setAmbientRoomTone` on the room it starts in), as a looping 2D ambience.
room_tone()
{
    wait 1;
    tone = undefined;
    switch (tolower(getdvar("mapname")))
    {
    case "zombie_theater":
        tone = "amb_theater_bg";
        break;
    }
    if (isdefined(tone))
        ambientplay(tone, 3);
}

// Debug (IW4L_ZM_WATCH=1): every new zombie is put a few metres in front of
// the first player, so a screenshot shows one.
watch_zombie()
{
    for (;;)
    {
        wait 0.5;
        players = getplayers();
        zombies = getaiarray("axis");
        if (!players.size)
            continue;
        p = players[0];
        for (i = 0; i < zombies.size; i++)
        {
            if (isdefined(zombies[i].iw4l_watched))
                continue;
            zombies[i].iw4l_watched = true;
            forward = anglestoforward(p getplayerangles());
            zombies[i].origin = p.origin + (forward[0] * 300, forward[1] * 300, 0) + (i * 20, 0, 0);
        }
    }
}

// Debug (IW4L_ZM_WATCH=buy): the first player stands at the nearest wall buy, facing it.
face_wall_buy()
{
    wait 8;
    players = getplayers();
    buys = getentarray("weapon_upgrade", "targetname");
    if (!players.size || !buys.size)
        return;
    p = players[0];
    buy = buys[0];
    for (i = 1; i < buys.size; i++)
    {
        if (distancesquared(buys[i].origin, p.origin) < distancesquared(buy.origin, p.origin))
            buy = buys[i];
    }
    away = vectornormalize(p.origin - buy.origin);
    p setorigin(buy.origin + (away[0] * 48, away[1] * 48, 0) - (0, 0, 40));
    // Aim once landed: the trigger is a plate on the wall, a few units high.
    wait 1;
    p setplayerangles(vectortoangles(buy.origin - p geteye()));
}

// Debug (IW4L_ZM_WATCH=door): the first player gets 10000 points and stands at the
// nearest door or debris, facing it.
face_door()
{
    wait 8;
    players = getplayers();
    all = getentarray("zombie_door", "targetname");
    debris = getentarray("zombie_debris", "targetname");
    for (i = 0; i < debris.size; i++)
        all[all.size] = debris[i];
    // Ones bought with points (not the doors the power opens).
    doors = [];
    for (i = 0; i < all.size; i++)
    {
        if (!isdefined(all[i].script_noteworthy) || all[i].script_noteworthy != "electric_door")
            doors[doors.size] = all[i];
    }
    if (!players.size || !doors.size)
        return;
    p = players[0];
    p.score = 10000;
    door = doors[0];
    for (i = 1; i < doors.size; i++)
    {
        if (distancesquared(doors[i].origin, p.origin) < distancesquared(door.origin, p.origin))
            door = doors[i];
    }
    away = vectornormalize((p.origin[0] - door.origin[0], p.origin[1] - door.origin[1], 0));
    p setorigin((door.origin[0], door.origin[1], p.origin[2]) + (away[0] * 24, away[1] * 24, 0));
    p setplayerangles(vectortoangles(door.origin - (p.origin + (0, 0, 60))));
    wait 1;
    logprint("door debug: player " + p.origin + " touching the door trigger: " + p istouching(door));
    for (pass = 0; pass < 4; pass++)
    {
        pieces = getentarray(door.target, "targetname");
        // IW4L_ZM_LOOK=leaves: once bought, turn to the door to see it open.
        if (pass == 2 && getdvar("iw4l_zm_look") == "leaves" && pieces.size)
            p setplayerangles(vectortoangles(pieces[0].origin - (p.origin + (0, 0, 60))));
        logprint("door debug pass " + pass + ": " + pieces.size + " pieces for " + door.target);
        for (i = 0; i < pieces.size; i++)
        {
            model = "";
            if (isdefined(pieces[i].model))
                model = pieces[i].model;
            style = "";
            if (isdefined(pieces[i].script_string))
                style = pieces[i].script_string;
            logprint("  " + pieces[i].classname + " " + model + " " + style + " at " + pieces[i].origin + " angles " + pieces[i].angles);
        }
        wait 5;
    }
}

// Debug (IW4L_ZM_WATCH=at): the first player stands at IW4L_ZM_X/Y/Z looking along
// IW4L_ZM_YAW/PITCH, kept there, and ignored by zombies. IW4L_ZM_LOOK=bait: zombies come
// for it instead, and it can't go down.
stand_at()
{
    wait 8;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    // IW4L_ZM_LOOK=shoot: bait that also turns to the nearest zombie's chest with its
    // ammo kept full (a test script holds fire), and logs each zombie that loses a limb.
    shoot = getdvar("iw4l_zm_look") == "shoot";
    bait = getdvar("iw4l_zm_look") == "bait" || getdvar("iw4l_zm_look") == "lunge" || shoot;
    if (bait)
        p setmaxhealth(100000);
    else
        p.ignoreme = true;
    at = (getdvarfloat("iw4l_zm_x"), getdvarfloat("iw4l_zm_y"), getdvarfloat("iw4l_zm_z"));
    look = (getdvarfloat("iw4l_zm_pitch"), getdvarfloat("iw4l_zm_yaw"), 0);
    p setorigin(at);
    p setplayerangles(look);
    // IW4L_ZM_OPEN=1: every door and debris pile the player could buy opens (Black Ops'
    // own forced unlock) and the power goes on, so zombies come from the whole map.
    if (getdvar("iw4l_zm_open") == "1")
    {
        all = getentarray("zombie_door", "targetname");
        debris = getentarray("zombie_debris", "targetname");
        for (i = 0; i < debris.size; i++)
            all[all.size] = debris[i];
        for (i = 0; i < all.size; i++)
            all[i] notify("trigger", p, true);
        logprint("open debug: forced " + all.size + " doors and debris open");
        // The power too: it opens the electric doors and (Kino) the stage curtains, the
        // only ways into the theater.
        switch_trigger = getent("use_elec_switch", "targetname");
        if (isdefined(switch_trigger))
        {
            switch_trigger notify("trigger", p);
            logprint("open debug: power switch thrown");
        }
    }
    wait 1;
    logprint("stand debug: player " + p.origin + " looking " + look);
    // IW4L_ZM_LOOK=probe: what a trace along the look direction hits, at knee, waist and
    // chest height (where a zombie walking that way would be stopped).
    if (getdvar("iw4l_zm_look") == "probe")
    {
        forward = anglestoforward((0, look[1], 0));
        for (h = 20; h <= 60; h += 20)
        {
            start = at + (0, 0, h);
            end = start + (forward[0] * 200, forward[1] * 200, 0);
            t = bullettrace(start, end, false, p);
            what = "world";
            if (isdefined(t["entity"]))
            {
                what = t["entity"].classname;
                if (isdefined(t["entity"].targetname))
                    what = what + " " + t["entity"].targetname;
            }
            logprint("probe debug: height " + h + " fraction " + t["fraction"] + " hit " + t["position"] + " " + what);
        }
    }
    if (shoot)
    {
        gun = getdvar("iw4l_zm_gun");
        if (gun != "")
        {
            p giveweapon(gun);
            p switchtoweapon(gun);
            wait 2;
        }
        level thread aim_at_zombies();
        logprint("shoot debug: weapon " + p getcurrentweapon() + " has m1911_zm " + p hasweapon("m1911_zm"));
    }
    // IW4L_ZM_LOOK=lunge: free to move, facing the nearest zombie and logging the gap to
    // it (feet to feet), to see a knife lunge stop at a zombie's edge.
    if (getdvar("iw4l_zm_look") == "lunge")
    {
        for (;;)
        {
            wait 0.1;
            p.health = 100000;
            zombies = getaispeciesarray("axis", "all");
            nearest = undefined;
            for (i = 0; i < zombies.size; i++)
                if (!isdefined(nearest) || distance2d(zombies[i].origin, p.origin) < distance2d(nearest.origin, p.origin))
                    nearest = zombies[i];
            if (isdefined(nearest))
            {
                p setplayerangles(vectortoangles(nearest.origin + (0, 0, 40) - p geteye()));
                logprint("lunge debug: gap " + distance2d(nearest.origin, p.origin) + " player " + p.origin + " zombie " + nearest getentitynumber() + " " + nearest.origin);
            }
        }
    }
    for (;;)
    {
        wait 0.5;
        p setorigin(at);
        if (!shoot)
            p setplayerangles(look);
        if (bait)
            p.health = 100000;
        // IW4L_ZM_INSTA=1: Insta-Kill stays on.
        if (getdvar("iw4l_zm_insta") == "1")
        {
            level.zombie_vars["zombie_insta_kill"] = 1;
            if (!isdefined(p.iw4l_last_score) || p.iw4l_last_score != p.score)
                logprint("insta debug: score " + p.score);
            p.iw4l_last_score = p.score;
        }
        if (shoot)
        {
            weapon = p getcurrentweapon();
            p givemaxammo(weapon);
            zombies = getaiarray("axis");
            for (i = 0; i < zombies.size; i++)
            {
                if (isdefined(zombies[i].a) && isdefined(zombies[i].a.gib_ref) && !isdefined(zombies[i].iw4l_gib_logged))
                {
                    zombies[i].iw4l_gib_logged = true;
                    logprint("shoot debug: zombie lost " + zombies[i].a.gib_ref + " holding " + weapon);
                }
                if (isdefined(zombies[i].head_gibbed) && zombies[i].head_gibbed && !isdefined(zombies[i].iw4l_head_logged))
                {
                    zombies[i].iw4l_head_logged = true;
                    logprint("shoot debug: zombie lost its head to " + weapon);
                }
            }
        }
    }
}

// Debug (IW4L_ZM_WATCH=tour): with every door open, the first player (ignored by
// zombies) visits each "x y z" of IW4L_ZM_TOUR (","-separated), looking each way for a
// second, so one run draws the whole map.
tour()
{
    wait 8;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    p.ignoreme = true;
    all = getentarray("zombie_door", "targetname");
    debris = getentarray("zombie_debris", "targetname");
    for (i = 0; i < debris.size; i++)
        all[all.size] = debris[i];
    for (i = 0; i < all.size; i++)
        all[i] notify("trigger", p, true);
    wait 2;
    spots = strtok(getdvar("iw4l_zm_tour"), ",");
    for (i = 0; i < spots.size; i++)
    {
        xyz = strtok(spots[i], " ");
        at = (int(xyz[0]), int(xyz[1]), int(xyz[2]));
        logprint("tour debug: spot " + i + " at " + at);
        for (yaw = 0; yaw < 360; yaw += 90)
        {
            p setorigin(at);
            p setplayerangles((10, yaw, 0));
            wait 1;
        }
    }
    logprint("tour debug: done");
}

// Debug (IW4L_ZM_WATCH=window): the first player stands back from the nearest boarded
// window, facing it.
face_window()
{
    wait 8;
    players = getplayers();
    windows = common_scripts\utility::getstructarray("exterior_goal", "targetname");
    if (!players.size || !windows.size)
        return;
    p = players[0];
    w = windows[0];
    for (i = 1; i < windows.size; i++)
    {
        if (distancesquared(windows[i].origin, p.origin) < distancesquared(w.origin, p.origin))
            w = windows[i];
    }
    away = vectornormalize((p.origin[0] - w.origin[0], p.origin[1] - w.origin[1], 0));
    p setorigin((w.origin[0], w.origin[1], p.origin[2]) + (away[0] * 96, away[1] * 96, 0));
    p setplayerangles(vectortoangles(w.origin - (p.origin + (0, 0, 60))));
    wait 1;
    logprint("window debug: player " + p.origin + " window " + w.origin);
}

// Debug (IW4L_ZM_WATCH=power): the first player stands at the power switch, facing it.
face_power_switch()
{
    wait 8;
    players = getplayers();
    switch_trigger = getent("use_elec_switch", "targetname");
    if (!players.size || !isdefined(switch_trigger))
        return;
    p = players[0];
    target = switch_trigger.origin;
    away = vectornormalize((p.origin[0] - target[0], p.origin[1] - target[1], 0));
    p setorigin((target[0], target[1], target[2] - 40) + (away[0] * 48, away[1] * 48, 0));
    p setplayerangles(vectortoangles(target - (p.origin + (0, 0, 60))));
    logprint("power debug: switch at " + target + ", player at " + p.origin);
}

// Debug (IW4L_ZM_WATCH=box): the first player gets 10000 points and stands at the mystery
// box where it starts, facing it.
face_box()
{
    wait 8;
    players = getplayers();
    if (!players.size || !isdefined(level.chests) || !isdefined(level.chest_index))
        return;
    p = players[0];
    p.score = 10000;
    box = level.chests[level.chest_index];
    away = anglestoright(box.angles);
    stand = (box.origin[0], box.origin[1], p.origin[2]) + (away[0] * 56, away[1] * 56, 0);
    if (distancesquared(stand, p.origin) > distancesquared(stand - (away[0] * 112, away[1] * 112, 0), p.origin))
        stand = stand - (away[0] * 112, away[1] * 112, 0);
    p setorigin((stand[0], stand[1], box.origin[2] - 10));
    p setplayerangles(vectortoangles(box.origin - (p.origin + (0, 0, 60))));
}

// Debug (IW4L_ZM_WATCH=perk): the power goes on, then the first player gets 10000 points
// and stands at Juggernog, facing it.
face_juggernog()
{
    wait 8;
    players = getplayers();
    switch_trigger = getent("use_elec_switch", "targetname");
    if (!players.size || !isdefined(switch_trigger))
        return;
    p = players[0];
    switch_trigger notify("trigger", p);
    wait 8;
    machines = getentarray("zombie_vending", "targetname");
    for (i = 0; i < machines.size; i++)
    {
        if (!isdefined(machines[i].script_noteworthy) || machines[i].script_noteworthy != "specialty_armorvest")
            continue;
        p.score = 10000;
        target = machines[i].origin;
        away = vectornormalize((p.origin[0] - target[0], p.origin[1] - target[1], 0));
        p setorigin((target[0], target[1], target[2] - 40) + (away[0] * 48, away[1] * 48, 0));
        p setplayerangles(vectortoangles(target - (p.origin + (0, 0, 60))));
        wait 1;
        logprint("perk debug: player at Juggernog " + p.origin);
        return;
    }
}

// Debug (IW4L_ZM_WATCH=revive, co-op): once a second player is in, the first player goes
// down and the second stands beside them, facing them (hold USE on the second client,
// whose window has focus when both run on one PC, to revive).
revive_test()
{
    for (waited = 0; ; waited++)
    {
        wait 1;
        players = getplayers();
        if (players.size >= 2)
            break;
        if (waited % 2 == 0 && players.size)
            logprint("revive test: waiting for a second player; first player use " + players[0] usebuttonpressed());
    }
    level thread kill_all_zombies();
    wait 8;
    players = getplayers();
    victim = players[0];
    reviver = players[1];
    victim dodamage(victim.health + 100, victim.origin);
    wait 2;
    reviver setorigin(victim.origin + (30, 0, 0));
    wait 1;
    reviver setplayerangles(vectortoangles(victim.origin - reviver geteye()));
    for (i = 0; i < 15; i++)
    {
        touching = 0;
        if (isdefined(victim.revivetrigger))
        {
            touching = reviver istouching(victim.revivetrigger);
            if (i == 0)
                logprint("revive test: radius dvar " + getdvarint("revive_trigger_radius") + " trigger at " + victim.revivetrigger.origin + " victim at " + victim.origin + " reviver at " + reviver.origin);
        }
        sight = sighttracepassed(reviver.origin + (0, 0, 50), victim.origin + (0, 0, 30), false, undefined);
        bullet = bullettracepassed(reviver.origin + (0, 0, 50), victim.origin + (0, 0, 30), false, undefined);
        logprint("revive test t+" + i + ": victim down " + victim maps\_laststand::player_is_in_laststand() + " health " + victim.health + " state " + victim.sessionstate + " | reviver can " + reviver maps\_laststand::can_revive(victim) + " touching " + touching + " facing " + reviver maps\_laststand::is_facing(victim) + " sight " + sight + " bullet " + bullet + " use " + reviver usebuttonpressed() + " at " + reviver.origin);
        wait 1;
    }
    // Left down (no one holds USE), BO1 bleeds the player out after 45 s into a
    // spectator, back at the next round.
    for (i = 15; i < 120; i += 5)
    {
        wait 5;
        round = 0;
        if (isdefined(level.round_number))
            round = level.round_number;
        logprint("revive test t+" + i + ": victim down " + victim maps\_laststand::player_is_in_laststand() + " state " + victim.sessionstate + " round " + round);
    }
}

// Debug (IW4L_ZM_WATCH=volumes): logs which playable-area volumes the first player touches.
log_player_volumes()
{
    wait 10;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    volumes = getentarray("player_volume", "script_noteworthy");
    logprint("volumes: " + volumes.size + " player volumes; player at " + p.origin);
    for (i = 0; i < volumes.size; i++)
    {
        name = "";
        if (isdefined(volumes[i].targetname))
            name = volumes[i].targetname;
        model = "";
        if (isdefined(volumes[i].model))
            model = volumes[i].model;
        logprint("  " + volumes[i].classname + " " + name + " " + model + " at " + volumes[i].origin + " touching " + p istouching(volumes[i]));
    }
}

// Debug (IW4L_ZM_WATCH=nuke): every three seconds the first player kills every zombie.
kill_all_zombies()
{
    for (;;)
    {
        wait 3;
        players = getplayers();
        if (!players.size)
            continue;
        zombies = getaiarray("axis");
        // IW4L_ZM_LOOK=near: only the ones that got in and reached the player (a power-up
        // dropped outside the playable area is thrown away).
        near = getdvar("iw4l_zm_look") == "near";
        // IW4L_ZM_LOOK=gib: zombies that reach the player lose an arm (as a body shot can
        // do), are looked at, and are left alive.
        gib = getdvar("iw4l_zm_look") == "gib";
        for (i = 0; i < zombies.size; i++)
        {
            if ((near || gib) && distancesquared(zombies[i].origin, players[0].origin) > 200 * 200)
                continue;
            if (gib)
            {
                if (!isdefined(zombies[i].iw4l_gibbed))
                {
                    zombies[i].iw4l_gibbed = true;
                    zombies[i].a.gib_ref = "right_arm";
                    zombies[i] thread animscripts\zombie_death::do_gib();
                    logprint("gib debug: zombie at " + zombies[i].origin + " loses its right arm");
                }
                players[0] setplayerangles(vectortoangles(zombies[i].origin + (0, 0, 40) - players[0] geteye()));
                players[0] setmaxhealth(100000);
                players[0].health = 100000;
                continue;
            }
            zombies[i] dodamage(zombies[i].health + 100, zombies[i].origin, players[0]);
        }
        if (near)
            players[0].health = players[0].maxhealth;
        // Power-ups on the ground now, and BO1's drop bookkeeping.
        models = getentarray("script_model", "classname");
        dropped = "";
        for (i = 0; i < models.size; i++)
        {
            if (isdefined(models[i].powerup_name))
            {
                dropped = dropped + " " + models[i].powerup_name + "@" + models[i].origin;
                // Look at it for a screenshot, then (a round of the loop later) step onto it.
                if (near && isdefined(models[i].iw4l_seen))
                    players[0] setorigin(models[i].origin - (0, 0, 40));
                else if (near)
                    players[0] setplayerangles(vectortoangles(models[i].origin - players[0] geteye()));
                models[i].iw4l_seen = true;
            }
        }
        if (near)
            logprint("nuke debug: insta kill " + level.zombie_vars["zombie_insta_kill"] + ", point scalar " + level.zombie_vars["zombie_point_scalar"]);
        logprint("nuke debug: killed " + zombies.size + ", score " + players[0].score + ", drops this round " + level.powerup_drop_count + ", on the ground:" + dropped);
    }
}

// Debug (IW4L_ZM_WATCH=die): the first player takes a killing blow after 35 seconds.
down_first_player()
{
    wait 35;
    players = getplayers();
    if (players.size)
        players[0] dodamage(players[0].health + 100, players[0].origin);
}

// Debug (IW4L_ZM_WATCH=aim): the first player also keeps aiming at the
// nearest zombie's chest.
aim_at_zombies()
{
    for (;;)
    {
        wait 0.05;
        players = getplayers();
        zombies = getaiarray("axis");
        if (!players.size || !zombies.size)
            continue;
        p = players[0];
        z = zombies[0];
        for (i = 1; i < zombies.size; i++)
        {
            if (distancesquared(zombies[i].origin, p.origin) < distancesquared(z.origin, p.origin))
                z = zombies[i];
        }
        // IW4L_ZM_AIM=head: at its head, for head shots.
        target = z.origin + (0, 0, 48);
        if (getdvar("iw4l_zm_aim") == "head")
            target = z gettagorigin("j_head");
        p setplayerangles(vectortoangles(target - (p.origin + (0, 0, 60))));
    }
}

// Debug (IW4L_ZM_WATCH=powerup): a Max Ammo drops in front of the first player, who walks
// onto it a few seconds later (to see the glow go with the pick-up).
// Debug (IW4L_ZM_WATCH=link, Kino): the power goes on and the first player (10000
// points) stands at the teleporter's core, then its pad, then on the pad, each time
// logging where it is so a test can press USE; the flags show what each use did.
link_teleporter()
{
    wait 8;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    p.ignoreme = true;
    p.score = 10000;
    switch_trigger = getent("use_elec_switch", "targetname");
    if (isdefined(switch_trigger))
        switch_trigger notify("trigger", p);
    wait 6;
    core = getent("trigger_teleport_pad_0", "targetname");
    if (!isdefined(core))
    {
        logprint("link debug: no core");
        return;
    }
    pad = getent(core.target, "targetname");
    logprint("link debug: core at " + core.origin + " pad at " + pad.origin + " power " + level.flag["power_on"]);
    p setorigin(core.origin + (0, 0, -40));
    wait 1;
    logprint("link debug: at core, touching " + p istouching(core));
    wait 4;
    logprint("link debug: core_linked " + level.flag["core_linked"]);
    p setorigin(pad.origin + (0, 0, -40));
    wait 1;
    logprint("link debug: at pad, touching " + p istouching(pad));
    wait 4;
    logprint("link debug: teleporter_linked " + level.flag["teleporter_linked"]);
    p setorigin(core.origin + (0, 0, -40));
    wait 1;
    logprint("link debug: on the pad again " + p.origin);
    wait 12;
    logprint("link debug: after teleport use, player at " + p.origin + " score " + p.score);
}

drop_powerup()
{
    wait 10;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    forward = anglestoforward((0, p.angles[1], 0));
    spot = p.origin + (forward[0] * 120, forward[1] * 120, 0);
    level thread maps\_zombiemode_powerups::specific_powerup_drop("full_ammo", spot);
    logprint("powerup debug: dropped at " + spot);
    wait 4;
    p setorigin(spot);
    logprint("powerup debug: grabbed");
    // Back off and look at the spot: nothing of the drop should be left there.
    wait 1;
    p setorigin(spot - (forward[0] * 150, forward[1] * 150, 0));
    p setplayerangles((20, p.angles[1], 0));
    logprint("powerup debug: looking back");
}

// Debug (IW4L_ZM_WATCH=dogs): four hellhounds through Black Ops' own special_dog_spawn
// (as its power-ups and teleporters do). The first player faces the nearest one, its
// health topped up, and each dog's position is logged every 2 seconds.
spawn_dogs()
{
    wait 10;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    // IW4L_ZM_LOOK=round: a real dog round instead, next round, with round 1's zombies
    // killed off.
    if (getdvar("iw4l_zm_look") == "round")
    {
        level.next_dog_round = level.round_number + 1;
        logprint("dog debug: dog round next (round " + level.next_dog_round + ")");
        while (!isdefined(level.flag) || !isdefined(level.flag["dog_round"]) || !level.flag["dog_round"])
        {
            zombies = getaispeciesarray("axis", "zombie");
            for (i = 0; i < zombies.size; i++)
                zombies[i] dodamage(zombies[i].health + 10, zombies[i].origin);
            wait 1;
        }
        logprint("dog debug: spawning (dog round " + level.round_number + ")");
    }
    else
    {
        level thread maps\_zombiemode_ai_dogs::special_dog_spawn(undefined, 4);
        logprint("dog debug: spawning 4");
    }
    for (tick = 0;; tick++)
    {
        wait 0.1;
        p.health = 100;
        dogs = getaispeciesarray("all", "zombie_dog");
        nearest = undefined;
        for (i = 0; i < dogs.size; i++)
        {
            if (!isdefined(dogs[i].isdog) || !dogs[i].isdog)
                continue;
            if (!isdefined(nearest) || distance(dogs[i].origin, p.origin) < distance(nearest.origin, p.origin))
                nearest = dogs[i];
            if (tick % 20 == 0)
                logprint("dog debug: " + dogs[i] getentitynumber() + " at " + dogs[i].origin + " player " + distance(dogs[i].origin, p.origin));
        }
        if (isdefined(nearest))
            p setplayerangles(vectortoangles(nearest.origin + (0, 0, 20) - p geteye()));
        // IW4L_ZM_LOOK=kill: after 15 seconds the player kills the nearest dog each second.
        if (getdvar("iw4l_zm_look") == "kill" && tick >= 150 && tick % 10 == 0 && isdefined(nearest))
        {
            logprint("dog debug: killing " + nearest getentitynumber());
            nearest dodamage(nearest.health + 10, p.origin, p);
        }
    }
}

// Debug (IW4L_ZM_WATCH=audit): every door, debris pile and the power open, the first
// player kept alive, and each round's zombies killed by the player a few seconds after
// they come, so a run walks through rounds (dog rounds too) and logs what breaks.
audit_rounds()
{
    wait 8;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    all = getentarray("zombie_door", "targetname");
    debris = getentarray("zombie_debris", "targetname");
    for (i = 0; i < debris.size; i++)
        all[all.size] = debris[i];
    for (i = 0; i < all.size; i++)
        all[i] notify("trigger", p, true);
    switch_trigger = getent("use_elec_switch", "targetname");
    if (isdefined(switch_trigger))
        switch_trigger notify("trigger", p);
    logprint("audit debug: opened " + all.size + " doors and debris, power on");
    level thread audit_round_starts();
    last = -1;
    // IW4L_ZM_LOOK=hurt: after the first hit every zombie dies, and the player's health
    // is logged as BO1's regen brings it back.
    if (getdvar("iw4l_zm_look") == "hurt")
    {
        while (p.health >= p.maxhealth)
            wait 0.05;
        hit = gettime();
        ai = getaispeciesarray("axis", "all");
        for (i = 0; i < ai.size; i++)
            ai[i] dodamage(ai[i].health + 10, ai[i].origin);
        for (t = 0; t < 120; t++)
        {
            logprint("audit debug: hurt " + (gettime() - hit) + " ms health " + p.health + " / " + p.maxhealth);
            wait 0.1;
        }
        return;
    }
    for (;;)
    {
        p.health = 100;
        if (level.round_number != last)
        {
            last = level.round_number;
            logprint("audit debug: round " + last + " points " + p.score);
        }
        ai = getaispeciesarray("axis", "all");
        // IW4L_ZM_LOOK=quad: no killing; the player faces the nearest Nova crawler.
        if (getdvar("iw4l_zm_look") == "quad")
        {
            nearest = undefined;
            for (i = 0; i < ai.size; i++)
                if (ai[i].classname == "actor_zombie_quad" && (!isdefined(nearest) || distance(ai[i].origin, p.origin) < distance(nearest.origin, p.origin)))
                    nearest = ai[i];
            // Other zombies die as they come, so the rounds reach the crawler waves.
            if (!isdefined(level.audit_cull) || gettime() - level.audit_cull > 3000)
            {
                level.audit_cull = gettime();
                for (i = 0; i < ai.size; i++)
                    if (ai[i].classname != "actor_zombie_quad" && isalive(ai[i]) && distance(ai[i].origin, p.origin) < 2000)
                        ai[i] dodamage(ai[i].health + 10, ai[i].origin, p);
            }
            if (isdefined(nearest))
            {
                p setplayerangles(vectortoangles(nearest.origin + (0, 0, 20) - p geteye()));
                // After a while, one crawler killed every 3 seconds (its death and gas).
                if (!isdefined(level.audit_quad_seen))
                    level.audit_quad_seen = gettime();
                if (gettime() - level.audit_quad_seen > 20000 && (!isdefined(level.audit_quad_kill) || gettime() - level.audit_quad_kill > 3000))
                {
                    level.audit_quad_kill = gettime();
                    logprint("audit debug: killing crawler " + nearest getentitynumber() + " at " + nearest.origin);
                    nearest dodamage(nearest.health + 10, p.origin, p, p, 0, "MOD_PISTOL_BULLET");
                }
            }
            wait 0.1;
            continue;
        }
        for (i = 0; i < ai.size; i++)
            if (isalive(ai[i]) && distance(ai[i].origin, p.origin) < 2000)
                ai[i] dodamage(ai[i].health + 10, ai[i].origin, p);
        wait 3;
    }
}

audit_round_starts()
{
    a = 27;
    logprint("audit debug: math " + int(a * 0.5) + " " + (a * 0.5) + " " + int(13.5) + " " + int(a * 0.3) + " " + int(a * 0.7) + " " + (a * 0.25));
    level.round_number = 3;
    logprint("audit debug: branch r3 " + maps\_zombiemode::default_max_zombie_func(27) + " first " + level.first_round);
    level.round_number = 1;
    for (;;)
    {
        level waittill("start_of_round");
        zkeys = getarraykeys(level.zones);
        line = "";
        for (z = 0; z < zkeys.size; z++)
        {
            zone = level.zones[zkeys[z]];
            line = line + zkeys[z] + "=" + zone.is_enabled + zone.is_active + zone.is_occupied + "/" + zone.spawners.size + " ";
        }
        players = getplayers();
        logprint("audit debug: zones " + line + "player " + players[0].origin);
        for (i = 0; i < 4; i++)
        {
            logprint("audit debug: start of round " + level.round_number + " total " + level.zombie_total + " intermission " + level.intermission + " spawns " + level.enemy_spawns.size);
            wait 0.05;
        }
    }
}

// Debug (IW4L_ZM_WATCH=reel, with IW4L_CINEMATIC=1): the camera flies a few slow dolly
// shots through the map for the menu's background recording: the power on and every
// door open (the lights up), the player ignored by zombies and kept alive. Each shot
// eases from its start to its end; "reel debug: shot N" marks each start.
// IW4L_ZM_SHOT=n holds shot n's start (to frame it).
camera_reel()
{
    wait 8;
    players = getplayers();
    if (!players.size)
        return;
    p = players[0];
    p.ignoreme = true;
    all = getentarray("zombie_door", "targetname");
    debris = getentarray("zombie_debris", "targetname");
    for (i = 0; i < debris.size; i++)
        all[all.size] = debris[i];
    for (i = 0; i < all.size; i++)
        all[i] notify("trigger", p, true);
    switch_trigger = getent("use_elec_switch", "targetname");
    if (isdefined(switch_trigger))
        switch_trigger notify("trigger", p);
    // Shots: from, to (eye positions), from angles, to angles, seconds.
    from = [];
    to = [];
    a = [];
    b = [];
    secs = [];
    // The foyer: drifting past the ticket booth toward the stairs.
    from[0] = (-300, -1450, 150); to[0] = (100, -1400, 160); a[0] = (2, 60, 0); b[0] = (4, 105, 0); secs[0] = 9;
    // The auditorium from the back rows, pushing toward the stage.
    from[1] = (0, -150, 150); to[1] = (0, 250, 110); a[1] = (5, 90, 0); b[1] = (3, 90, 0); secs[1] = 9;
    // From the stage, out over the seats toward the balcony.
    from[2] = (120, 640, 190); to[2] = (-120, 520, 210); a[2] = (4, -110, 0); b[2] = (6, -70, 0); secs[2] = 10;
    // The stage from the side aisle, the screen and curtains past the seats.
    from[3] = (-520, 160, 215); to[3] = (-420, 360, 215); a[3] = (9, 40, 0); b[3] = (9, 55, 0); secs[3] = 9;
    only = getdvar("iw4l_zm_shot");
    if (only != "")
    {
        n = int(only);
        p setorigin(from[n] - (0, 0, 60));
        p setplayerangles(a[n]);
        logprint("reel debug: holding shot " + n);
        for (;;)
        {
            p setorigin(from[n] - (0, 0, 60));
            p setplayerangles(a[n]);
            wait 0.05;
        }
    }
    for (;;)
    {
        for (n = 0; n < from.size; n++)
        {
            logprint("reel debug: shot " + n);
            // Held still a moment first, so the map is drawn in before it rolls.
            for (i = 0; i < 50; i++)
            {
                p setorigin(from[n] - (0, 0, 60));
                p setplayerangles(a[n]);
                wait 0.05;
            }
            logprint("reel debug: rolling " + n);
            steps = int(secs[n] * 20);
            for (i = 0; i <= steps; i++)
            {
                // Ease in and out.
                t = i / steps;
                t = t * t * (3 - 2 * t);
                at = from[n] + (to[n] - from[n]) * t;
                look = a[n] + (b[n] - a[n]) * t;
                p setorigin(at - (0, 0, 60));
                p setplayerangles(look);
                wait 0.05;
            }
        }
        logprint("reel debug: loop done");
    }
}

// Co-op joiners: BO1 places every player on its own start point once, when all of
// them have connected from the lobby. Here players join a match already running, and
// they spawn where the first player started; BO1 kills two players standing within 18
// units of each other, so a player who spawns on top of another moves to a free
// start point (which is also where BO1 respawns them between rounds).
place_joiners()
{
    level thread note_intro_done();
    for (;;)
    {
        players = getplayers();
        for (i = 0; i < players.size; i++)
        {
            p = players[i];
            if (p.sessionstate != "playing" || isdefined(p.iw4l_placed))
                continue;
            p.iw4l_placed = true;
            // BO1 freezes each player's first spawn and unfreezes everyone once, when
            // the opening fade ends: a player joining after that stays frozen.
            if (isdefined(level.iw4l_intro_done))
                p thread unfreeze_joiner();
            if (!crowded(p.origin, p))
                continue;
            structs = common_scripts\utility::getstructarray("initial_spawn_points", "targetname");
            for (s = 0; s < structs.size; s++)
            {
                if (crowded(structs[s].origin, p))
                    continue;
                p setorigin(structs[s].origin);
                if (isdefined(structs[s].angles))
                    p setplayerangles(structs[s].angles);
                p.spectator_respawn = structs[s];
                logprint("joiner " + p getentitynumber() + " placed at start point " + s + " " + structs[s].origin);
                break;
            }
        }
        wait 0.05;
    }
}

note_intro_done()
{
    level waittill("fade_in_complete");
    level.iw4l_intro_done = true;
}

unfreeze_joiner()
{
    self endon("disconnect");
    wait 1;
    self freezecontrols(false);
}

crowded(origin, self_player)
{
    players = getplayers();
    for (i = 0; i < players.size; i++)
    {
        if (players[i] == self_player || players[i].sessionstate != "playing")
            continue;
        if (distance2d(players[i].origin, origin) < 48 && abs(players[i].origin[2] - origin[2]) < 72)
            return true;
    }
    return false;
}

// Game over: the HUD shows the rounds survived, then the map starts over (BO1 would
// leave the level for its menus).
game_over()
{
    level waittill("end_game");
    rounds = 1;
    if (isdefined(level.round_number))
        rounds = level.round_number;
    players = getplayers();
    for (i = 0; i < players.size; i++)
        players[i] setclientdvar("ui_zm_gameover", "" + rounds);
    wait 10;
    map_restart(false);
}

hud_feed()
{
    for (;;)
    {
        round = 0;
        if (isdefined(level.round_number))
            round = level.round_number;
        players = getplayers();
        for (i = 0; i < players.size; i++)
        {
            p = players[i];
            // Engine fields BO1's player checks read before anything sets them: without a
            // team, the zombies' hits count as friendly fire and do nothing.
            if (!isdefined(p.ignoreme))
                p.ignoreme = false;
            // Counted by BO1's last stand (each down, each revive) and its game-over stats.
            if (!isdefined(p.downs))
                p.downs = 0;
            if (!isdefined(p.revives))
                p.revives = 0;
            if (!isdefined(p.team) || p.team == "")
                p.team = "allies";
            if (!isdefined(p.iw4l_hud_over))
            {
                p.iw4l_hud_over = true;
                p setclientdvar("ui_zm_gameover", "");
            }
            score = 0;
            if (isdefined(p.score))
                score = p.score;
            if (!isdefined(p.iw4l_hud_score) || p.iw4l_hud_score != score)
            {
                p.iw4l_hud_score = score;
                p setclientdvar("ui_zm_points", "" + score);
            }
            if (!isdefined(p.iw4l_hud_round) || p.iw4l_hud_round != round)
            {
                p.iw4l_hud_round = round;
                p setclientdvar("ui_zm_round", "" + round);
            }
        }
        wait 0.1;
    }
}
"#,
);

/// Free roam for an install without MW2's gametype scripts: every player who connects is
/// spawned at the map's first start point, and nothing else runs.
pub(crate) const FREE_ROAM: [(&str, &str); 4] = [
    ("codescripts/delete", "main()\n{\n}\n"),
    (
        "codescripts/struct",
        r#"initstructs()
{
    level.struct = [];
}

createstruct()
{
    s = spawnstruct();
    level.struct[level.struct.size] = s;
    return s;
}
"#,
    ),
    ("maps/mp/gametypes/dm", "main()\n{\n}\n"),
    (
        "maps/mp/gametypes/_callbacksetup",
        r#"codecallback_startgametype()
{
}

codecallback_playerconnect()
{
    waittillframeend;
    start = free_roam_start();
    self.sessionstate = "playing";
    self spawn(start.origin, start.angles);
}

codecallback_playerdisconnect()
{
}

codecallback_playerdamage(inflictor, attacker, damage, flags, meansofdeath, weapon, point, dir, hitloc, timeoffset)
{
}

codecallback_playerkilled(inflictor, attacker, damage, meansofdeath, weapon, dir, hitloc, timeoffset, deathanimduration)
{
}

codecallback_playerlaststand(inflictor, attacker, damage, meansofdeath, weapon, dir, hitloc, timeoffset, deathanimduration)
{
}

codecallback_vehicledamage(inflictor, attacker, damage, flags, meansofdeath, weapon, point, dir, hitloc, timeoffset, partname, modelname, tagname)
{
}

// Zombies maps start players at their `initial_spawn_points` structs; other maps at
// their first player start of a known classname.
free_roam_start()
{
    for (i = 0; i < level.struct.size; i++)
    {
        s = level.struct[i];
        if (isdefined(s.targetname) && s.targetname == "initial_spawn_points" && isdefined(s.origin))
            return free_roam_point(s.origin, s.angles);
    }
    classnames = [];
    classnames[0] = "info_player_start";
    classnames[1] = "mp_dm_spawn";
    classnames[2] = "mp_tdm_spawn";
    classnames[3] = "mp_global_intermission";
    for (i = 0; i < classnames.size; i++)
    {
        ents = getentarray(classnames[i], "classname");
        if (ents.size > 0)
            return free_roam_point(ents[0].origin, ents[0].angles);
    }
    return free_roam_point((0, 0, 0), undefined);
}

free_roam_point(origin, angles)
{
    p = spawnstruct();
    p.origin = origin;
    p.angles = (0, 0, 0);
    if (isdefined(angles))
        p.angles = angles;
    return p;
}
"#,
    ),
];

pub(crate) const RADIATION: &str = r#"main()
{
    waittillframeend;
    level.radiation_busy = true;
    level.radiation_open = false;
    level.radiation_doors = [];
    level.radiation_doors[0] = getent("big_door1_clip", "targetname");
    level.radiation_doors[1] = getent("big_door2_clip", "targetname");
    switches = [];
    switches[0] = getent("switch_trigger1", "targetname");
    switches[1] = getent("switch_trigger2", "targetname");
    if (!isdefined(level.radiation_doors[0]) || !isdefined(level.radiation_doors[1])
        || !isdefined(switches[0]) || !isdefined(switches[1]))
        return;
    models = [];
    models[0] = getent("big_door1", "targetname");
    models[1] = getent("big_door2", "targetname");
    for (i = 0; i < 2; i++)
    {
        if (isdefined(models[i]))
            models[i] linkto(level.radiation_doors[i]);
        switches[i] usetriggerrequirelookat();
        switches[i] setcursorhint("HINT_ACTIVATE");
        switches[i] sethintstring(&"MP_HOLD_TO_OPERATE_DOORS");
        switches[i] thread watch_switch();
    }
    level.radiation_switches = switches;
    if (!isdefined(level.gameended) || !level.gameended)
    {
        level waittill("prematch_over");
        level.radiation_busy = false;
        thread operate_doors();
    }
}

watch_switch()
{
    for (;;)
    {
        self waittill("trigger", player);
        if (!level.radiation_busy)
        {
            if (isdefined(player))
                player playsound("t5:evt_hydraulic_switch");
            thread operate_doors();
        }
    }
}

operate_doors()
{
    if (level.radiation_busy)
        return;
    level.radiation_busy = true;
    for (i = 0; i < 2; i++)
        level.radiation_switches[i] makeunusable();
    turn = 123;
    if (level.radiation_open)
        turn = -turn;
    duration = getdvarfloat("scr_d1_time");
    if (duration <= 0)
        duration = 8;
    cooldown = getdvarfloat("scr_door_cooldown");
    if (cooldown <= 0)
        cooldown = 20;
    level.radiation_doors[0] rotateroll(turn, duration, duration * .6, duration * .4);
    level.radiation_doors[1] rotateroll(-turn, duration, duration * .7, duration * .3);
    level.radiation_doors[0] playloopsound("evt_hydraulic_loop");
    level.radiation_doors[0] playsound("t5:evt_hydraulic_start");
    level.radiation_doors[0] waittill("rotatedone");
    level.radiation_doors[0] stoploopsound();
    level.radiation_open = !level.radiation_open;
    if (level.radiation_open)
        level.radiation_doors[0] playsound("t5:evt_hydraulic_open");
    else
        level.radiation_doors[0] playsound("t5:evt_hydraulic_close");
    wait cooldown;
    level.radiation_busy = false;
    for (i = 0; i < 2; i++)
        level.radiation_switches[i] makeusable();
}
"#;

pub(crate) const DESTRUCTIBLES: &str = r#"main()
{
    level.globalCarsDestroyed = 0;
}

broken(event, attacker)
{
    if (event == "destructible_car_fire" || event == "destructible_barrel_fire")
        self thread ignite(attacker);
    else if (event == "destructible_car_explosion" || event == "destructible_barrel_explosion" || event == "explode")
        self explode(attacker);
}

ignite(attacker)
{
    self endon("wrecked");
    wait randomintrange(7, 10);
    self thread explode(attacker);
}

explode(attacker)
{
    if (isdefined(self.wrecked) && self.wrecked)
        return;
    self.wrecked = true;
    self notify("wrecked");
    self damagepiece(20000, -1, attacker);
    self radiusdamage(self.origin, 256, 300, 75, attacker, "MOD_EXPLOSIVE");
    earthquake(.5, .5, self.origin, 800);
    level.globalCarsDestroyed++;
}

break_after(piece, stage, duration, amount, attacker)
{
    wait duration;
    if (self piecestage(piece, stage))
        self damagepiece(amount, piece, attacker);
}
"#;
