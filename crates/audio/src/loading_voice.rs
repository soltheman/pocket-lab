//! BO1's loading-screen voice: Richtofen over Kino's loading screen.
//!
//! BO1 plays a short Bink video while a zombies map loads (`main/video/
//! <map>_load.bik`): the map's loading art with a voice track over it. The art is
//! already the loading screen's picture; the voice comes from that video's audio,
//! converted once on the PC by `scripts/stage-bo1-for-iphone.ps1` into
//! `main/video/<map>_load.wav` (PCM16). While the map loads the track plays as
//! menu-scope music; when the map comes up it fades out.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use asset_core::AssetNamespace;
use assets::LoadingPreviewSource;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{AppScreen, ClientSet};

use crate::backend::AudioScope;
use crate::clip_store::{ClipKey, MediaService};
use crate::media::PcmBuffer;
use crate::menu_music::{one_track_bank, parse_pcm16_wav};
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};

const VOICE_ALIAS: &str = "loading_voice";
const VOICE_GAIN: f32 = 0.9;
/// Its own key, apart from the menu music (object 1) and MW2's menu sources.
const VOICE_SLOT: u32 = 5;
const VOICE_OBJECT: u64 = 2;
/// How long the voice takes to fade once the map is up.
const FADE_SECONDS: f32 = 1.5;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<LoadingVoice>()
        .add_systems(Update, play_loading_voice.in_set(ClientSet::Effects));
}

#[derive(Resource, Default)]
pub(crate) struct LoadingVoice {
    /// The load request the voice was looked up for, so each load looks once.
    request: Option<u64>,
    reading: Option<Task<Result<PcmBuffer, String>>>,
    fade_left: Option<f32>,
    version: u64,
    pub source: Option<DesiredSource>,
}

impl LoadingVoice {
    fn stop(&mut self) {
        self.reading = None;
        self.fade_left = None;
        self.source = None;
    }
}

/// `<game>/main/video/<map>_load.wav` beside the zone the load reads from.
fn voice_path(zone_ff: &Path, map_name: &str) -> Option<PathBuf> {
    let map = map_name.rsplit(':').next().unwrap_or(map_name);
    if !map.starts_with("zombie_") {
        return None;
    }
    let game = zone_ff.ancestors().find(|dir| dir.join("main").is_dir())?;
    Some(game.join("main").join("video").join(format!("{map}_load.wav")))
}

fn read_voice(path: &Path) -> Result<PcmBuffer, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("`{}`: {error}", path.display()))?;
    let wav = parse_pcm16_wav(&bytes)
        .ok_or_else(|| format!("`{}` is not a PCM16 WAV", path.display()))?;
    PcmBuffer::from_zone(Arc::from(&bytes[wav.data]), 16, wav.channels, wav.rate)
        .map_err(|error| format!("`{}`: {error:?}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn play_loading_voice(
    mut voice: ResMut<LoadingVoice>,
    screen: Option<Res<AppScreen>>,
    source: Option<Res<LoadingPreviewSource>>,
    silent: Option<Res<crate::AudioSilent>>,
    runtime: Res<crate::AudioRuntime>,
    time: Res<Time<Real>>,
) {
    let voice = &mut *voice;
    if silent.is_some() {
        voice.stop();
        return;
    }
    let loading = screen.is_some_and(|screen| matches!(*screen, AppScreen::Loading));

    // A new load looks for its map's voice once.
    if loading
        && let Some(source) = source.as_deref()
        && voice.request != Some(source.request_id)
    {
        voice.stop();
        voice.request = Some(source.request_id);
        if let Some(path) = voice_path(&source.path, &source.map_name) {
            if path.is_file() {
                let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                voice.reading = Some(pool.spawn(async move { read_voice(&path) }));
            } else {
                diag::info!(
                    Audio,
                    "audio: no loading voice at `{}` — the loading screen stays silent (typed gap)",
                    path.display()
                );
            }
        }
    }

    if let Some(task) = &mut voice.reading
        && let Some(read) = future::block_on(future::poll_once(task))
    {
        voice.reading = None;
        match read {
            // Only worth starting while the load is still up.
            Ok(pcm) if loading => {
                let seconds =
                    (pcm.len() / usize::from(pcm.channels())) as f32 / pcm.rate() as f32;
                let bank = Arc::new(one_track_bank(VOICE_ALIAS, pcm.channels(), pcm.rate(), false));
                let media = MediaService::resident(Arc::clone(&bank), ClipKey::Loaded(0), pcm);
                let cue = runtime.source_cue_with_media(
                    SourceCueRequest {
                        bank,
                        namespace: AssetNamespace::T5,
                        alias: VOICE_ALIAS.to_owned(),
                        emitter: None,
                        scope: AudioScope::Menu,
                        epoch: 0,
                        group: None,
                    },
                    media,
                );
                if let Some(cue) = cue {
                    voice.version = voice.version.wrapping_add(1);
                    voice.source = Some(DesiredSource {
                        key: SourceKey {
                            scope: AudioScope::Menu,
                            epoch: 0,
                            object: VOICE_OBJECT,
                            slot: VOICE_SLOT,
                        },
                        version: voice.version,
                        cue,
                        origin_inches: None,
                        start_frame: runtime.audio_frame(),
                        gain: VOICE_GAIN,
                        rate: 1.0,
                        audible: true,
                    });
                    diag::info!(Audio, "audio: loading voice playing ({seconds:.1}s)");
                }
            }
            Ok(_) => {}
            Err(reason) => {
                diag::warn!(Audio, "audio: loading voice unavailable: {reason} (typed gap)");
            }
        }
    }

    // The map is up: fade the voice out, then let its memory go.
    if !loading && voice.source.is_some() {
        let left = voice.fade_left.get_or_insert(FADE_SECONDS);
        *left -= time.delta_secs();
        if *left <= 0.0 {
            voice.stop();
        } else if let Some(source) = voice.source.as_mut() {
            source.gain = VOICE_GAIN * (*left / FADE_SECONDS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_sits_in_the_installs_video_folder() {
        let root = std::env::temp_dir().join("iw4l_loading_voice_test");
        let game = root.join("Call of Duty Black Ops");
        std::fs::create_dir_all(game.join("main")).unwrap();
        std::fs::create_dir_all(game.join("zone").join("Common")).unwrap();
        let zone = game.join("zone").join("Common").join("zombie_theater.ff");
        assert_eq!(
            voice_path(&zone, "t5:zombie_theater"),
            Some(game.join("main").join("video").join("zombie_theater_load.wav"))
        );
        assert_eq!(voice_path(&zone, "mp_rust"), None);
    }
}
