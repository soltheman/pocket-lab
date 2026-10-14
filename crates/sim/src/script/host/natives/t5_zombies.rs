//! BO1 zombies builtins the engine does not implement yet (generated with the catalog
//! from `gsc_census` over Kino der Toten's scripts). Each answers `unavailable`: a
//! recovered runtime error, logged once per call site and counted per name, so the
//! scripts install and run around it. A real implementation registered anywhere else
//! wins; only names still unbound get a stub.
use bevy_ecs::prelude::World;

use crate::script::Namespace::{Function, Method};
use crate::script::host::arrays::new_array;
use crate::script::host::registry::{NativeRegistry, unavailable};
use crate::script::{Runtime, Value};

/// Every player the scripts know, connected or still connecting, in client order.
fn player_objects(world: &World) -> Vec<Value> {
    world
        .resource::<Runtime>()
        .players
        .values()
        .map(|slot| Value::Object(slot.object))
        .collect()
}

/// The builtins implemented so far; `register` stubs the rest.
fn register_implemented(registry: &mut NativeRegistry) {
    registry.register(Function, "getplayers", |world, _, _| {
        let players = player_objects(world);
        new_array(world, players)
    });
    // Solo or local co-op: everyone expected is connected once their slot exists.
    registry.register(Function, "getnumconnectedplayers", |world, _, _| {
        Ok(Value::Int(player_objects(world).len() as i32))
    });
    registry.register(Function, "getnumexpectedplayers", |world, _, _| {
        Ok(Value::Int(player_objects(world).len() as i32))
    });
    // AI: the living actors, optionally one team's (getaiarray("axis")).
    registry.register(Function, "getaiarray", |world, _, args| {
        let team = match args.first() {
            Some(Value::String(team)) if team.to_string() != "all" => Some(team.to_string()),
            _ => None,
        };
        let actors = crate::script::host::actors::living(world, team.as_deref());
        new_array(world, actors)
    });
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let team = match args.first() {
            Some(Value::String(team)) if team.to_string() != "all" => Some(team.to_string()),
            _ => None,
        };
        let actors = crate::script::host::actors::living(world, team.as_deref());
        new_array(world, actors)
    });
    for name in ["codespawnerspawn", "codespawnerforcespawn"] {
        registry.register(Method, name, |world, receiver, args| {
            crate::script::host::actors::spawn_from_spawner(world, receiver, args)
        });
    }
    registry.register(Method, "finishactordamage", |world, receiver, args| {
        crate::script::host::actors::finish_damage(world, receiver, args)
    });
    // Spawners are the map's `actor_*` entities.
    registry.register(Function, "getspawnerarray", |world, _, _| {
        let runtime = world.resource::<Runtime>();
        let spawners: Vec<Value> = runtime
            .entities
            .iter()
            .filter(|(_, entity)| entity.classname.starts_with("actor_"))
            .map(|(id, _)| Value::Object(*id))
            .collect();
        new_array(world, spawners)
    });
    // wait_network_frame() waits on snapshot acknowledgements while remote clients exist;
    // the listen host's own players are local, so it waits a tenth of a second instead.
    registry.register(Function, "numremoteclients", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "setsaveddvar", |world, _, args| {
        let name = crate::script::host::args::string(args, 0)?;
        let value = match args.get(1) {
            Some(Value::Int(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => s.to_string(),
            _ => String::new(),
        };
        super::iw4::set_dvar(world, &name, &value);
        Ok(Value::Undefined)
    });
    // Players are the only sentients until actors exist.
    registry.register(Function, "issentient", |world, _, args| {
        let Some(Value::Object(id)) = args.first() else {
            return Ok(Value::Int(0));
        };
        let is_player = world
            .resource::<Runtime>()
            .players
            .values()
            .any(|slot| slot.object == *id);
        let sentient = is_player || crate::script::host::actors::is_actor(world, *id);
        Ok(Value::Int(i32::from(sentient)))
    });
}

macro_rules! stubs {
    ($registry:expr; $($namespace:ident $name:literal),* $(,)?) => {$(
        $registry.register_absent($namespace, $name, |world, _, _| stub(world, $name));
    )*};
}

/// A yes/no question (is_, has_, can_, may_...) answers no, so a check the runtime cannot
/// answer does not take its `if` (a failed condition falls into the body); anything
/// else is `unavailable`. Both count the call.
fn stub(world: &mut World, name: &'static str) -> Result<Value, String> {
    let predicate = ["is", "has", "can", "may", "should"]
        .iter()
        .any(|prefix| name.starts_with(prefix));
    if predicate {
        let mut runtime = world.resource_mut::<Runtime>();
        let calls = runtime.unsupported.entry(name).or_default();
        *calls = calls.saturating_add(1);
        return Ok(Value::Int(0));
    }
    unavailable(world, name, "BO1 zombies builtin not implemented yet")
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    register_implemented(registry);
    stubs! { registry;
        Function "absangleclamp180",
        Function "activateclientexploder",
        Method "addaieventlistener",
        Method "addvehicletocompass",
        Method "adsbuttonpressed",
        Method "allowads",
        Method "allowcrouch",
        Method "allowedstances",
        Method "allowjump",
        Method "allowlean",
        Method "allowmelee",
        Method "allowpickupweapons",
        Method "allowpitchangle",
        Method "allowprone",
        Method "allowsprint",
        Method "allowstand",
        Method "animcustom",
        Method "animmode",
        Method "animscripted",
        Function "asin",
        Function "badplace_arc",
        Function "badplace_cylinder",
        Method "bloodimpact",
        Function "box",
        Function "bulletspread",
        Method "calcpathlength",
        Method "canattackenemynode",
        Method "cansee",
        Method "canshoot",
        Method "canuseturret",
        Method "changefontscaleovertime",
        Function "changelevel",
        Method "checkgrenadethrow",
        Method "checkgrenadethrowpos",
        Function "circle",
        Method "clearanim",
        Method "clearenemy",
        Method "clearfixednodesafevolume",
        Method "cleargoalvolume",
        Method "clearlookatent",
        Function "codeplayloopedfx",
        Function "codespawn",
        Method "codespawnerforcespawn",
        Method "codespawnerspawn",
        Function "codespawnfx",
        Function "codespawnturret",
        Function "codespawnvehicle",
        Function "commitsave",
        Function "createdynentandlaunch",
        Function "createthreatbiasgroup",
        Function "deactivateclientexploder",
        Function "disablegrenadesuicide",
        Method "disablegunnerfiring",
        Method "disableinvulnerability",
        Function "distance2dsquared",
        Method "dontinterpolate",
        Method "drivepath",
        Method "dropweapon",
        Method "enablehealthshield",
        Method "enableinvulnerability",
        Method "enterprone",
        Function "entsearch",
        Method "exitprone",
        Method "findbestcovernode",
        Function "findpath",
        Method "findreacquiredirectpath",
        Method "findreacquirenode",
        Method "findreacquireproximatepath",
        Method "flagenemyunattackable",
        Function "forcelevelend",
        Method "freevehicle",
        Function "getaiarray",
        Function "getaispeciesarray",
        Method "getaivelocity",
        Function "getallvehiclenodes",
        Function "getangledelta",
        Method "getanglestolikelyenemypath",
        Method "getanimtime",
        Function "getanynodearray",
        Method "getattachmodelname",
        Method "getattachpos",
        Method "getattachsize",
        Method "getattachtagname",
        Method "getcentroid",
        Function "getcinematictimeremaining",
        Method "getclosestenemysqdist",
        Method "getcurrentweaponclipammo",
        Method "getdebugeye",
        Function "getdifficulty",
        Method "geteyeapprox",
        Method "getflashbangedstrength",
        Method "getfractionmaxammo",
        Method "getgunneranimpitch",
        Method "gethitenttype",
        Method "getlightintensity",
        Method "getlinkedent",
        Method "getmotionangle",
        Function "getmovedelta",
        Method "getnegotiationendnode",
        Function "getnode",
        Function "getnotetracksindelta",
        Function "getnumconnectedplayers",
        Function "getnumexpectedplayers",
        Function "getpartname",
        Method "getperfectinfo",
        Function "getpersistentprofilevar",
        Function "getplayers",
        Method "getplayerviewheight",
        Method "getreacquirenode",
        Function "getsnapshotindexarray",
        Function "getspawnerarray",
        Function "getstartangles",
        Function "getstartorigin",
        Function "getthreatbias",
        Method "getthreatbiasgroup",
        Method "getturret",
        Method "getturretowner",
        Method "getturrettarget",
        Function "getvehiclenodearray",
        Method "getvehicleowner",
        Method "getvisionsetnaked",
        Function "getwaterheight",
        Function "getweaponaccuracy",
        Function "getweaponclipmodel",
        Method "getweaponforwarddir",
        Method "getweaponmuzzlepoint",
        Method "getweaponrenderoptions",
        Method "gib",
        Method "givegamerpicture",
        Function "groundtrace",
        Function "hascollectible",
        Method "haseyes",
        Method "hidepart",
        Method "hideviewmodel",
        Function "iscoopepd",
        Method "isingoal",
        Method "isknownenemyinradius",
        Method "isknownenemyinvolume",
        Function "ismp",
        Function "isnodeoccupied",
        Method "isnotarget",
        Function "issaverecentlyloaded",
        Function "issavesuccessful",
        Function "issentient",
        Method "isstanceallowed",
        Method "issuppressed",
        Function "isturretactive",
        Function "isturretfiring",
        Method "isturretready",
        Function "isvec",
        Method "isvehicleusable",
        Method "iswaitingonsound",
        Function "isweapondetonationtimed",
        Method "joltbody",
        Function "lerpfloat",
        Method "localtoworldcoords",
        Method "lookatentity",
        Method "magicgrenade",
        Method "magicgrenademanual",
        Method "makefakeai",
        Method "maketurretusable",
        Method "makevehicleusable",
        Method "maymovetopoint",
        Method "melee",
        Function "missionfailed",
        Function "missionsuccess",
        Function "modelhasphyspreset",
        Method "movex",
        Method "movez",
        Method "nearnode",
        Function "numremoteclients",
        Function "oktospawn",
        Function "pause3dcinematic",
        Function "physicsexplosioncylinder",
        Method "playerknockback",
        Method "playerlinkto",
        Method "playerlinktoabsolute",
        Method "playerlinktodelta",
        Function "playerpositionvalid",
        Method "playersetgroundreferenceent",
        Method "playweapondeatheffects",
        Function "prefetchlevel",
        Method "pushplayer",
        Method "reacquiremove",
        Method "reacquirestep",
        Function "recordline",
        Function "refreshhudammocounter",
        Method "reloadbuttonpressed",
        Method "removevehiclefromcompass",
        Function "reportclientdisconnected",
        Function "reportmtu",
        Method "resetadswidthandlerp",
        Method "resetmissiledetonationtime",
        Method "restoredefaultdroppitch",
        Method "resumespeed",
        Method "reviveplayer",
        Function "ropesetflag",
        Function "savegame",
        Function "savegamenocommit",
        Method "setadswidthandlerp",
        Function "setailimit",
        Method "setanimknob",
        Method "setanimknoball",
        Method "setanimknoballrestart",
        Method "setanimknoblimited",
        Method "setanimknobrestart",
        Method "setanimlimited",
        Method "setanimrestart",
        Method "setanimtime",
        Method "setautopickup",
        Method "setblur",
        Method "setbrake",
        Method "setclock",
        Method "setconvergencetime",
        Function "setculldist",
        Method "setdeathcontents",
        Function "setdebugangles",
        Function "setdebugorigin",
        Method "setdoublevision",
        Method "setelectrified",
        Method "setengagementmaxdist",
        Method "setengagementmindist",
        Method "setenginevolume",
        Method "setentitytarget",
        Function "setexpfog",
        Method "setexploderid",
        Method "setfixednodesafevolume",
        Method "setflaggedanimknob",
        Method "setflaggedanimknoball",
        Method "setflaggedanimknoballrestart",
        Method "setflaggedanimknobrestart",
        Method "setflaggedanimlimited",
        Method "setflaggedanimrestart",
        Method "setgoalentity",
        Method "setgoalvolume",
        Method "setlightintensity",
        Method "setlookatent",
        Method "setmaxhealth",
        Function "setmissiondvar",
        Function "setpersistentprofilevar",
        Method "setphysparams",
        Method "setproneanimnodes",
        Method "setredactfx",
        Method "setruntopos",
        Function "setsaveddvar",
        Method "setscripthintstring",
        Method "setshadowhint",
        Method "setsoundblend",
        Method "setspawnerteam",
        Method "setstance",
        Method "setswitchnode",
        Method "settalktospecies",
        Method "setteamforentity",
        Function "setthreatbias",
        Method "setthreatbiasgroup",
        Method "settransported",
        Method "setturretignoregoals",
        Function "setuinextlevel",
        Method "setvehicleattachments",
        Method "setvehiclelookattext",
        Method "setvolfog",
        Method "setwaitspeed",
        Method "shoot",
        Method "showpart",
        Method "showviewmodel",
        Function "snapshotacknowledged",
        Function "soundexists",
        Function "splitviewallowed",
        Function "start3dcinematic",
        Method "startactorreact",
        Method "startcameratween",
        Method "startfadingblur",
        Method "startrevive",
        Function "stop3dcinematic",
        Function "stopallrumbles",
        Method "stopanimscripted",
        Method "stoprevive",
        Method "stopshoot",
        Method "stopsound",
        Method "stopsounds",
        Method "stopuseturret",
        Method "throwgrenade",
        Method "trackscriptstate",
        Method "transmittargetname",
        Function "updategamerprofile",
        Method "updateplayersightaccuracy",
        Method "updateprone",
        Method "uploadscore",
        Method "useby",
        Method "usecovernode",
        Method "usereacquirenode",
        Method "useturret",
        Method "useweaponhidetags",
        Method "vibrate",
        Method "viewkick",
        Method "visionsetlaststand",
        Method "visionsetnaked",
        Function "watersimenable",
        Function "weapondualwieldweaponname",
        Function "weaponisboltaction",
        Function "weaponisgasweapon",
        Function "weaponmountable",
        Function "weapontype",
    }
}
