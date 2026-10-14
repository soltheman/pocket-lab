use crate::script::source::SourceResolver;

/// BO1 zombies (the singleplayer engine's realm): the level script is `maps/<map>` and the
/// code callbacks live in `maps/_callbacksetup`. Install runs the struct initializer
/// itself, before the map's structs exist, as for IW4.
pub struct ZombiesStartup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}

pub const ZOMBIES_CALLBACKS: &str = "maps/_callbacksetup";

impl ZombiesStartup {
    pub fn new(resolver: &impl SourceResolver, map: &str) -> Option<Self> {
        let level = format!("maps/{map}");
        if resolver.read_bytes(&level).is_err() || resolver.read_bytes(ZOMBIES_CALLBACKS).is_err() {
            return None;
        }
        Some(Self {
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
        })
    }
}
