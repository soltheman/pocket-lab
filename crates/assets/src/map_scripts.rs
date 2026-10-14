pub(crate) const T6_EQUIPMENT: &str = include_str!("t6_equipment.gsc.txt");

/// BO1 zombies HUD feed: BO1 draws points and the round through script HUD elements and
/// its own menus, which this runtime does not present, so each player's points and the
/// round number go out as client dvars (`ui_zm_points`, `ui_zm_round`) for the client's
/// zombies HUD to draw.
pub(crate) const ZOMBIES_HUD: (&str, &str) = (
    "iw4l_zombies/hud",
    r#"main()
{
    level thread hud_feed();
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
