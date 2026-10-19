//! BO1's zombie music on the BO1-only main menu.
//!
//! A BO1-only install has no MW2 menu zone, so the frontend bank never builds
//! (`FrontendAudioUnavailable`) and the menu has no music of its own. BO1's
//! music tracks, though, are plain streamed files in the install's IWD
//! archives — no zone has to be loaded to reach them. The menu reads the chosen
//! track out of the T5 archives the frontend walk already opened, decodes it
//! off the main thread with the decoders the match's streamed clips use, and
//! loops it as menu-scope music.

use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::sync::Arc;

use asset_audio::{CapturedAlias, CapturedSound, LoadedSoundPcm, SoundCatalog, ZoneGame};
use asset_core::AssetNamespace;
use assets::NamespaceSoundIwd;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{AppScreen, ClientSet};

use crate::backend::AudioScope;
use crate::clip_store::{ClipKey, MediaService, decode_streamed_bytes};
use crate::frontend::FrontendAudioUnavailable;
use crate::media::PcmBuffer;
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};

/// The menu's tracks as `(label, IWD path)`. `MenuMusicChoice(n)` plays entry
/// `n - 1`; 0 is off.
pub const MENU_TRACKS: &[(&str, &str)] = &[
    (
        "KINO",
        "sound/mus/zombie_theatre/mus_theatre_underscore_l.wav",
    ),
    ("115", "sound/mus/zombie/theater/115.wav"),
    ("ABRACADAVRE", "sound/mus/zombie/cosmo/abracadavre.wav"),
    (
        "NOT READY TO DIE",
        "sound/mus/zombie/coast/a7x_not_ready_to_die.wav",
    ),
    ("PAREIDOLIA", "sound/mus/zombie/temple/mus_pareidolia.wav"),
    ("COMING HOME", "sound/mus/zombie/moon/coming_home.wav"),
];

/// Which of `MENU_TRACKS` the BO1-only main menu loops: `n` plays entry
/// `n - 1`, 0 (or anything past the list) is off. Set it from the menu's
/// settings; a change stops the current track and starts the new one.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuMusicChoice(pub u8);

impl Default for MenuMusicChoice {
    fn default() -> Self {
        Self(1)
    }
}

impl MenuMusicChoice {
    /// The `(label, IWD path)` this choice plays, or `None` when it is off.
    pub fn track(self) -> Option<(&'static str, &'static str)> {
        MENU_TRACKS
            .get(usize::from(self.0).checked_sub(1)?)
            .copied()
    }
}

/// The alias every track's one-alias bank files it under.
const MENU_MUSIC_ALIAS: &str = "menu_music";
/// The MW2 menu music's gain and slot: on a BO1-only menu this is the menu
/// music, so it sits in the mix where that track would.
const MENU_MUSIC_GAIN: f32 = 0.55;
const MENU_MUSIC_SLOT: u32 = 4;
/// `MenuSources` owns object 0 of the menu slot and can still hold an MW2
/// alias request nothing resolves; a separate object keeps the two keys apart.
const MENU_MUSIC_OBJECT: u64 = 1;
/// Decoded tracks kept resident: only the one playing. Even as 16-bit PCM a long
/// track is tens of MB (Abracadavre about 70), too much to keep a spare on a
/// phone; they are all let go when a match starts.
const RESIDENT_TRACKS: usize = 1;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<MenuMusicChoice>()
        .init_resource::<MenuMusic>()
        .add_systems(Update, play_bo1_menu_music.in_set(ClientSet::Effects));
}

struct ResidentTrack {
    choice: u8,
    bank: Arc<SoundCatalog>,
    media: MediaService,
}

/// The BO1-only menu's music source and the tracks behind it.
#[derive(Resource, Default)]
pub(crate) struct MenuMusic {
    resident: VecDeque<ResidentTrack>,
    /// One track read at a time; a choice made meanwhile waits its turn.
    reading: Option<(u8, Task<Result<PcmBuffer, String>>)>,
    /// Choices whose file is missing or unreadable; not retried this process.
    failed: HashSet<u8>,
    playing: Option<u8>,
    next_version: u64,
    pub source: Option<DesiredSource>,
}

impl MenuMusic {
    fn stop(&mut self) {
        if self.source.take().is_some() {
            diag::info!(Audio, "audio: menu music stopped");
        }
        self.playing = None;
    }
}

fn play_bo1_menu_music(
    mut music: ResMut<MenuMusic>,
    choice: Res<MenuMusicChoice>,
    screen: Option<Res<AppScreen>>,
    unavailable: Option<Res<FrontendAudioUnavailable>>,
    silent: Option<Res<crate::AudioSilent>>,
    runtime: Res<crate::AudioRuntime>,
) {
    let music = &mut *music;
    if let Some((read_choice, task)) = &mut music.reading
        && let Some(read) = future::block_on(future::poll_once(task))
    {
        let read_choice = *read_choice;
        music.reading = None;
        let label = MenuMusicChoice(read_choice)
            .track()
            .map_or("?", |(label, _)| label);
        match read {
            Ok(pcm) => {
                diag::info!(
                    Audio,
                    "audio: menu music `{label}` loaded ({} ch, {} Hz, {:.1}s)",
                    pcm.channels(),
                    pcm.rate(),
                    seconds(&pcm)
                );
                let bank = Arc::new(menu_music_bank(pcm.channels(), pcm.rate()));
                let media = MediaService::resident(Arc::clone(&bank), ClipKey::Loaded(0), pcm);
                if music.resident.len() == RESIDENT_TRACKS {
                    music.resident.pop_front();
                }
                music.resident.push_back(ResidentTrack {
                    choice: read_choice,
                    bank,
                    media,
                });
            }
            Err(reason) => {
                diag::warn!(
                    Audio,
                    "audio: menu music `{label}` unavailable: {reason} — the menu stays silent (typed gap)"
                );
                music.failed.insert(read_choice);
            }
        }
    }

    let on_menu = unavailable.is_some()
        && silent.is_none()
        && screen.is_some_and(|screen| matches!(*screen, AppScreen::MainMenu));
    // Off the menu (loading or playing a map) the tracks give their memory back.
    if !on_menu && (!music.resident.is_empty() || music.reading.is_some()) {
        music.stop();
        music.resident.clear();
        music.reading = None;
    }
    let wanted = choice.track().filter(|_| on_menu);
    if wanted.is_none() || music.playing.is_some_and(|playing| playing != choice.0) {
        music.stop();
    }
    let (Some((label, path)), Some(unavailable)) = (wanted, unavailable) else {
        return;
    };
    if music.source.is_some() || music.failed.contains(&choice.0) {
        return;
    }
    let Some(at) = music
        .resident
        .iter()
        .position(|track| track.choice == choice.0)
    else {
        if music.reading.is_none() {
            let iwd = Arc::clone(&unavailable.iwd);
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            music.reading = Some((choice.0, pool.spawn(async move { read_track(&iwd, path) })));
        }
        return;
    };
    // Most recently played last, so the eviction above drops the older one.
    let track = music.resident.remove(at).expect("position is in range");
    let cue = runtime.source_cue_with_media(
        SourceCueRequest {
            bank: Arc::clone(&track.bank),
            namespace: AssetNamespace::T5,
            alias: MENU_MUSIC_ALIAS.to_owned(),
            emitter: None,
            scope: AudioScope::Menu,
            epoch: 0,
            group: None,
        },
        track.media.clone(),
    );
    music.resident.push_back(track);
    let Some(cue) = cue else {
        return;
    };
    // A fresh version each start: the publisher refuses a version it has
    // already retired, and a new one restarts the track from the top.
    music.next_version = music
        .next_version
        .checked_add(1)
        .expect("source version exhausted");
    music.source = Some(DesiredSource {
        key: SourceKey {
            scope: AudioScope::Menu,
            epoch: 0,
            object: MENU_MUSIC_OBJECT,
            slot: MENU_MUSIC_SLOT,
        },
        version: music.next_version,
        cue,
        origin_inches: None,
        start_frame: runtime.audio_frame(),
        gain: MENU_MUSIC_GAIN,
        rate: 1.0,
        audible: true,
    });
    music.playing = Some(choice.0);
    diag::info!(Audio, "audio: menu music playing `{label}`");
}

/// Read one track out of the T5 archives. A PCM16 WAV plays its data bytes in
/// place; anything else goes through the streamed-clip decoders.
fn read_track(iwd: &NamespaceSoundIwd, path: &str) -> Result<PcmBuffer, String> {
    // The archive index is keyed below `sound/`, the way aliases name files.
    let relative = path.strip_prefix("sound/").unwrap_or(path);
    if iwd.index(AssetNamespace::T5).is_none() {
        return Err("no T5 sound archives in this install".to_owned());
    }
    let bytes = match iwd.read_sound(AssetNamespace::T5, relative) {
        Some(Ok(bytes)) => bytes,
        Some(Err(error)) => return Err(format!("IWD read `{path}` failed: {error}")),
        None => return Err(format!("`{path}` is in no T5 IWD")),
    };
    if let Some(wav) = parse_pcm16_wav(&bytes) {
        return PcmBuffer::from_zone(Arc::from(&bytes[wav.data]), 16, wav.channels, wav.rate)
            .map_err(|error| format!("`{path}`: {error:?}"));
    }
    // Kept as 16-bit PCM: half the decoder's f32 size for minutes of music.
    let decoded = decode_streamed_bytes(AssetNamespace::T5, &bytes)
        .map_err(|error| format!("`{path}` decode refused: {error:?}"))?;
    let pcm16 = decoded.pcm16_bytes();
    let (channels, rate) = (decoded.channels(), decoded.rate());
    drop(decoded);
    PcmBuffer::from_zone(pcm16, 16, channels, rate).map_err(|error| format!("`{path}`: {error:?}"))
}

fn seconds(pcm: &PcmBuffer) -> f64 {
    (pcm.len() / usize::from(pcm.channels())) as f64 / f64::from(pcm.rate())
}

#[derive(Debug, PartialEq, Eq)]
struct WavPcm16 {
    channels: u16,
    rate: u32,
    data: Range<usize>,
}

/// The fmt and data chunks of a mono or stereo PCM16 WAV, or `None` for any
/// other file, which is then left to the general decoders.
fn parse_pcm16_wav(bytes: &[u8]) -> Option<WavPcm16> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut format = None;
    let mut data = None;
    let mut at = 12usize;
    while let Some(header) = bytes.get(at..at.checked_add(8)?) {
        let size = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let body = at + 8;
        let end = body.checked_add(size)?;
        let chunk = bytes.get(body..end)?;
        match &header[..4] {
            b"fmt " if chunk.len() >= 16 => {
                let field = |offset: usize| u16::from_le_bytes([chunk[offset], chunk[offset + 1]]);
                let rate = u32::from_le_bytes(chunk[4..8].try_into().ok()?);
                format = Some((field(0), field(2), rate, field(14)));
            }
            b"data" => data = Some(body..end),
            _ => {}
        }
        at = end.checked_add(size & 1)?;
    }
    let (tag, channels, rate, bits) = format?;
    let data = data?;
    (tag == 1
        && bits == 16
        && PcmBuffer::validate_geometry(channels, rate).is_ok()
        && !data.is_empty()
        && data.len().is_multiple_of(usize::from(channels) * 2))
    .then_some(WavPcm16 {
        channels,
        rate,
        data,
    })
}

/// A one-alias T5 bank naming a resident track, so it plays as an ordinary
/// source cue: the control thread resolves the alias, picks variant 0 and asks
/// the track's resident service for `Loaded(0)`. The loaded entry carries no
/// bytes — the service already holds the PCM, and nothing decodes this bank.
fn menu_music_bank(channels: u16, rate: u32) -> SoundCatalog {
    let mut bank = SoundCatalog::default();
    bank.set_capture_game(ZoneGame::T5);
    let mut loaded = LoadedSoundPcm::captured(
        MENU_MUSIC_ALIAS,
        asset_audio::MSS_PCM,
        rate,
        i32::from(channels),
        Vec::new(),
        Vec::new(),
    );
    loaded.game = ZoneGame::T5;
    bank.ingest_loaded(loaded);
    bank.ingest_sound(CapturedSound {
        name: MENU_MUSIC_ALIAS.to_owned(),
        aliases: vec![CapturedAlias {
            alias_name: MENU_MUSIC_ALIAS.to_owned(),
            loaded_name: Some(MENU_MUSIC_ALIAS.to_owned()),
            file_type: Some(1),
            vol_min: 1.0,
            vol_max: 1.0,
            pitch_min: 1.0,
            pitch_max: 1.0,
            probability: 1.0,
            // Looping, 2D (no 3D bit), mixer group 0: unity gain, no falloff.
            flags: Some(asset_iw4::SND_ALIAS_FLAG_LOOPING),
            looping: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    });
    bank.finalize();
    bank
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(channels: u16, rate: u32, samples: &[i16]) -> Vec<u8> {
        let data = u32::try_from(samples.len() * 2).expect("small");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn pcm16_wav_data_plays_in_place() {
        let samples: Vec<i16> = (0..2000).map(|n| (n * 37 - 30000) as i16).collect();
        let bytes = wav(2, 44100, &samples);
        let parsed = parse_pcm16_wav(&bytes).expect("pcm16 wav");
        assert_eq!((parsed.channels, parsed.rate), (2, 44100));
        assert_eq!(parsed.data, 44..bytes.len());
        let pcm =
            PcmBuffer::from_zone(Arc::from(&bytes[parsed.data]), 16, 2, 44100).expect("zone pcm");
        assert_eq!(pcm.len(), samples.len());

        let decoded = decode_streamed_bytes(AssetNamespace::T5, &bytes).expect("riff decoder");
        assert_eq!(
            (decoded.channels(), decoded.rate(), decoded.len()),
            (2, 44100, samples.len())
        );
    }

    #[test]
    fn other_wavs_are_left_to_the_decoders() {
        let bytes = wav(1, 48000, &[1, 2, 3, 4]);
        assert!(parse_pcm16_wav(&bytes[..bytes.len() - 1]).is_none());
        assert!(parse_pcm16_wav(&bytes[..40]).is_none());
        assert!(parse_pcm16_wav(b"RIFF\0\0\0\0WAVE").is_none());
        let mut eight_bit = bytes.clone();
        eight_bit[34] = 8;
        assert!(parse_pcm16_wav(&eight_bit).is_none());
        let mut adpcm = bytes;
        adpcm[20] = 0x11;
        assert!(parse_pcm16_wav(&adpcm).is_none());
    }

    #[test]
    fn choice_maps_onto_the_track_list() {
        assert_eq!(
            MenuMusicChoice::default().track().map(|t| t.0),
            Some("KINO")
        );
        assert_eq!(MenuMusicChoice(0).track(), None);
        assert_eq!(
            MenuMusicChoice(MENU_TRACKS.len() as u8)
                .track()
                .map(|t| t.0),
            Some("COMING HOME")
        );
        assert_eq!(MenuMusicChoice(MENU_TRACKS.len() as u8 + 1).track(), None);
        assert!(
            MENU_TRACKS
                .iter()
                .all(|(_, path)| path.starts_with("sound/"))
        );
    }

    #[test]
    fn track_bank_resolves_to_its_resident_clip() {
        let bank = menu_music_bank(2, 48000);
        assert_eq!(
            crate::clip_store::clip_key_for_variant(
                &bank,
                AssetNamespace::T5,
                MENU_MUSIC_ALIAS,
                None,
                0
            ),
            Some(ClipKey::Loaded(0))
        );
        let index = bank
            .index_in(AssetNamespace::T5, MENU_MUSIC_ALIAS)
            .expect("alias");
        let policy = bank.playback_policy(index, 0).expect("policy");
        assert_eq!(policy.namespace, AssetNamespace::T5);
        assert!(policy.spatial.is_none(), "menu music is 2D");
        assert!(policy.composition.unsupported.is_empty());
        assert_eq!(policy.volume(0.5).ok(), Some(1.0));
        assert_eq!(policy.pitch(0.5), 1.0);

        let bank = Arc::new(bank);
        let pcm = PcmBuffer::from_zone(Arc::from(&[0u8, 1, 2, 3][..]), 16, 2, 48000).expect("pcm");
        let media = MediaService::resident(Arc::clone(&bank), ClipKey::Loaded(0), pcm);
        assert_eq!(media.bank_revision(), bank.revision());
        assert!(matches!(media.ready(&ClipKey::Loaded(0)), Some(Ok(_))));
        assert!(!media.request(ClipKey::Loaded(1)));
    }
}
