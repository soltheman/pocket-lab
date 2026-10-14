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
    if (getdvar("iw4l_zm_watch") == "box")
        level thread face_box();
    if (getdvar("iw4l_zm_watch") == "perk")
        level thread face_juggernog();
    if (getdvar("iw4l_zm_watch") == "nuke")
        level thread kill_all_zombies();
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
    p setplayerangles(vectortoangles(buy.origin - (p.origin + (0, 0, 60))));
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
    p setorigin((door.origin[0], door.origin[1], p.origin[2]) + (away[0] * 64, away[1] * 64, 0));
    p setplayerangles(vectortoangles(door.origin - (p.origin + (0, 0, 60))));
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
        return;
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
        for (i = 0; i < zombies.size; i++)
            zombies[i] dodamage(zombies[i].health + 100, zombies[i].origin, players[0]);
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
        p setplayerangles(vectortoangles(z.origin + (0, 0, 48) - (p.origin + (0, 0, 60))));
    }
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
