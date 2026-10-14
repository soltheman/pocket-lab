use crate::script::source::SourceResolver;

/// BO1 zombies (the singleplayer engine's realm): the level script is `maps/<map>` and the
/// code callbacks live in `maps/_callbacksetup`. Install runs the struct initializer
/// itself, before the map's structs exist, as for IW4.
pub struct ZombiesStartup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}

pub const ZOMBIES_CALLBACKS: &str = "maps/_callbacksetup";
const ZOMBIES_HUD: &str = "iw4l_zombies/hud";
/// The animscripts the engine runs on every actor: init when it spawns, death when it dies,
/// for zombies and for dogs.
pub const ACTOR_ANIMSCRIPTS: &[&str] = &[
    "animscripts/zombie_init",
    "animscripts/zombie_dog_init",
    "animscripts/zombie_death",
    "animscripts/zombie_dog_death",
];

impl ZombiesStartup {
    pub fn new(resolver: &impl SourceResolver, map: &str) -> Option<Self> {
        let level = format!("maps/{map}");
        if resolver.read_bytes(&level).is_err() || resolver.read_bytes(ZOMBIES_CALLBACKS).is_err() {
            return None;
        }
        let mut startup = Self {
            roots: vec![
                "codescripts/delete".to_owned(),
                "codescripts/struct".to_owned(),
                ZOMBIES_CALLBACKS.to_owned(),
                level.clone(),
            ],
            entries: vec![
                format!("{level}::main"),
                format!("{ZOMBIES_CALLBACKS}::codecallback_startgametype"),
            ],
        };
        // The animscripts the engine runs on the actors it spawns.
        for script in ACTOR_ANIMSCRIPTS {
            if resolver.read_bytes(script).is_ok() {
                startup.roots.push((*script).to_owned());
            }
        }
        // The runtime's own HUD feed, when the walk installed it.
        if resolver.read_bytes(ZOMBIES_HUD).is_ok() {
            startup.roots.push(ZOMBIES_HUD.to_owned());
            startup.entries.push(format!("{ZOMBIES_HUD}::main"));
        }
        Some(startup)
    }
}
