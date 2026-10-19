//! BO1's menu navigation sounds, for the zombies menus and the in-match pause
//! and options menus.
//!
//! A BO1-only install has no MW2 menu zone, so the frontend bank never builds
//! and the menus' `UiPlaySound` aliases (MW2 names) have nothing to resolve.
//! BO1's own `uin_navigation_*` aliases are defined in `code_post_gfx.ff`, and
//! every one the menus use is a loaded sound inside that zone (a few KB of
//! ADPCM each), not a streamed IWD file. So the zone's sound assets are walked
//! once at startup on a background task, the clips the menus use are decoded
//! there, and the result stays resident for the whole process: the zone's
//! small bank plus a `MediaService::resident_clips` holding the decoded PCM.
//! A `UiSound` then plays as an ordinary one-shot cue against that bank, so it
//! sounds the same on the menu and in a match whatever bank the match holds.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use asset_audio::SoundCatalog;
use asset_core::AssetNamespace;
use asset_transport::GamesRoot;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{ClientSet, LaunchIdentity};

use crate::clip_store::{CueFeedback, MediaService, clip_keys_for_alias, prepare_clip_now};
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::start::{StartDecision, StartDecisions, StartOutcome};
use crate::{SND_ENT_LOCAL, SoundClass};

/// One of BO1's menu navigation sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiSoundKind {
    /// A button pressed (`uin_navigation_click`).
    Click,
    /// Focus moved onto a button (`uin_navigation_over`).
    Over,
    /// Backing out of a menu (`uin_navigation_backout`).
    Back,
    /// A small menu or popup opening (`uin_navigation_menu_sm_open`).
    Open,
    /// A small menu or popup closing (`uin_navigation_menu_sm_close`).
    Close,
    /// An options slider moved (`uin_options_slider`).
    Slider,
}

impl UiSoundKind {
    pub const ALL: [Self; 6] = [
        Self::Click,
        Self::Over,
        Self::Back,
        Self::Open,
        Self::Close,
        Self::Slider,
    ];

    /// The BO1 (T5) sound alias this kind plays.
    pub fn alias(self) -> &'static str {
        match self {
            Self::Click => "uin_navigation_click",
            Self::Over => "uin_navigation_over",
            Self::Back => "uin_navigation_backout",
            Self::Open => "uin_navigation_menu_sm_open",
            Self::Close => "uin_navigation_menu_sm_close",
            Self::Slider => "uin_options_slider",
        }
    }
}

/// Play one BO1 menu sound once, non-positional, under the master volume. A
/// no-op (logged once) when the install has no BO1 menu sounds or they are
/// still loading.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiSound(pub UiSoundKind);

impl From<UiSoundKind> for UiSound {
    fn from(kind: UiSoundKind) -> Self {
        Self(kind)
    }
}

/// BO1's `code_post_gfx`: it defines the `uin_*` aliases and carries their PCM.
const UI_SOUND_ZONE: &str = "t5:code_post_gfx";

/// The zone's bank and the decoded clips every `UiSoundKind` (and its
/// secondary layers) resolves to.
struct UiClips {
    bank: Arc<SoundCatalog>,
    media: MediaService,
    /// Kinds whose alias or clip the zone did not yield; they stay silent.
    missing: HashSet<UiSoundKind>,
}

enum UiSoundState {
    /// Waiting for a games root.
    Idle,
    Loading(Task<Result<UiClips, String>>),
    Ready(UiClips),
    Unavailable,
}

#[derive(Resource)]
pub(crate) struct UiSounds {
    state: UiSoundState,
    /// Each drop reason is logged once: a menu sends these on every focus move.
    logged_loading: bool,
    logged_unavailable: bool,
    /// The slider's alias runs most of a second with a second layer; a drag
    /// sends a change every frame, so a new one waits for the last to end.
    slider: Option<CueHandle>,
}

impl Default for UiSounds {
    fn default() -> Self {
        Self {
            state: UiSoundState::Idle,
            logged_loading: false,
            logged_unavailable: false,
            slider: None,
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<UiSounds>()
        .add_message::<UiSound>()
        .add_systems(Update, play_ui_sounds.in_set(ClientSet::Effects));
}

#[allow(clippy::too_many_arguments)]
fn play_ui_sounds(
    mut requests: MessageReader<UiSound>,
    mut sounds: ResMut<UiSounds>,
    identity: Option<Res<LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    runtime: Res<crate::AudioRuntime>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    epoch: Res<crate::backend::MatchEpoch>,
) {
    let sounds = &mut *sounds;
    if silent.is_some() {
        requests.clear();
        return;
    }
    if matches!(sounds.state, UiSoundState::Idle)
        && let Some(identity) = identity
        && !identity.games_root.as_os_str().is_empty()
    {
        let games = GamesRoot(identity.games_root.clone());
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        sounds.state = UiSoundState::Loading(pool.spawn(async move { load_ui_clips(&games) }));
    }
    if let UiSoundState::Loading(task) = &mut sounds.state
        && let Some(loaded) = future::block_on(future::poll_once(task))
    {
        sounds.state = match loaded {
            Ok(clips) => UiSoundState::Ready(clips),
            Err(reason) => {
                diag::warn!(
                    Audio,
                    "audio: ui sounds unavailable: {reason} — UiSound is a no-op (typed gap)"
                );
                UiSoundState::Unavailable
            }
        };
    }

    // One of each kind a frame: a focus sweep or a slider drag can send many.
    let mut kinds = Vec::new();
    for UiSound(kind) in requests.read() {
        if !kinds.contains(kind) {
            kinds.push(*kind);
        }
    }
    let Some(&first) = kinds.first() else {
        return;
    };
    let clips = match &sounds.state {
        UiSoundState::Ready(clips) => clips,
        UiSoundState::Idle | UiSoundState::Loading(_) => {
            if !std::mem::replace(&mut sounds.logged_loading, true) {
                diag::info!(
                    Audio,
                    "audio: UiSound {first:?} dropped — BO1 menu sounds are still loading"
                );
            }
            return;
        }
        UiSoundState::Unavailable => {
            if !std::mem::replace(&mut sounds.logged_unavailable, true) {
                diag::warn!(
                    Audio,
                    "audio: UiSound {first:?} dropped — no BO1 menu sounds (typed gap)"
                );
            }
            return;
        }
    };
    for kind in kinds {
        if clips.missing.contains(&kind) {
            continue;
        }
        if kind == UiSoundKind::Slider && sounds.slider.as_ref().is_some_and(CueHandle::active) {
            continue;
        }
        let alias = kind.alias();
        let handle = runtime.trigger_cue_with_media(
            CueTrigger {
                event: None,
                bank: Arc::clone(&clips.bank),
                namespace: AssetNamespace::T5,
                alias: alias.to_owned(),
                bound: None,
                origin_inches: None,
                emitter: Some(SND_ENT_LOCAL),
                class: SoundClass::Ui,
                epoch: epoch.0,
                pitch_scale: 1.0,
                fallbacks: Vec::new(),
            },
            clips.media.clone(),
        );
        if kind == UiSoundKind::Slider {
            sounds.slider = Some(CueHandle(Arc::clone(&handle.0)));
        }
        pending.push(handle);
        decisions.record(StartDecision {
            event: None,
            namespace: AssetNamespace::T5,
            alias: alias.to_owned(),
            variant: None,
            loaded_binding_origin: None,
            outcome: StartOutcome::Pending,
            secondary: None,
            detail: None,
        });
    }
}

/// Walk BO1's `code_post_gfx` for its sounds and decode every clip the menu
/// kinds reach, secondary layers included. Runs on a background task.
fn load_ui_clips(games: &GamesRoot) -> Result<UiClips, String> {
    let started = Instant::now();
    let zone = asset_transport::discover::find_zone_file(games, UI_SOUND_ZONE)?;
    let mut catalog = asset_audio::load_sound_catalog_t5(&zone.path)?;
    prepare_ui_bank(&mut catalog);
    let bank = Arc::new(catalog);
    let (clips, missing) = decode_ui_clips(&bank);
    if clips.is_empty() {
        return Err(format!(
            "`{}` yielded none of the menu aliases",
            zone.path.display()
        ));
    }
    for kind in &missing {
        diag::warn!(
            Audio,
            "audio: ui sound `{}` not playable from `{}` — {kind:?} stays silent (typed gap)",
            kind.alias(),
            zone.path.display()
        );
    }
    let frames: usize = clips
        .iter()
        .map(|(_, pcm)| pcm.len() / usize::from(pcm.channels()))
        .sum();
    diag::info!(
        Audio,
        "audio: ui sounds resident from `{}` — {} clips, {frames} frames, {} of {} kinds, {:.0} ms",
        zone.path.display(),
        clips.len(),
        UiSoundKind::ALL.len() - missing.len(),
        UiSoundKind::ALL.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(UiClips {
        media: MediaService::resident_clips(Arc::clone(&bank), clips),
        bank,
        missing,
    })
}

/// Fit the zone's catalog for playing on its own. Its rawfiles are dropped
/// (nothing plays them). Its mixer groups are dropped too: the match banks
/// carry none, so every match sound plays at its alias volume, and the UI
/// group's -11 dB would otherwise leave the menu sounds that far under the
/// rest of the mix (and under the menu music). Without them each alias plays
/// at its own authored volume (-6 dB for the navigation sounds).
fn prepare_ui_bank(catalog: &mut SoundCatalog) {
    catalog.rawfiles.clear();
    catalog.ingest_mixer_groups(AssetNamespace::T5, Vec::new());
    catalog.publish();
}

type DecodedClips = Vec<(crate::clip_store::ClipKey, crate::media::PcmBuffer)>;

/// Decode the clips of every kind's alias (all variants and secondary
/// layers). A kind is missing when its alias is absent or any clip it needs
/// does not decode.
fn decode_ui_clips(bank: &SoundCatalog) -> (DecodedClips, HashSet<UiSoundKind>) {
    let mut clips: DecodedClips = Vec::new();
    let mut missing = HashSet::new();
    for kind in UiSoundKind::ALL {
        let alias = kind.alias();
        let keys = match bank.index_in(AssetNamespace::T5, alias) {
            Some(_) => clip_keys_for_alias(bank, AssetNamespace::T5, alias),
            None => Vec::new(),
        };
        if keys.is_empty() {
            missing.insert(kind);
            continue;
        }
        for key in keys {
            if clips.iter().any(|(have, _)| *have == key) {
                continue;
            }
            match prepare_clip_now(bank, None, &key).1 {
                Ok(pcm) => clips.push((key, pcm)),
                Err(error) => {
                    diag::warn!(
                        Audio,
                        "audio: ui sound `{alias}` clip {key:?} did not decode: {error:?}"
                    );
                    missing.insert(kind);
                }
            }
        }
    }
    (clips, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset_audio::{CapturedAlias, CapturedSound, LoadedSoundPcm, ZoneGame};

    use crate::clip_store::{ClipKey, clip_key_for_variant};
    use crate::media::PcmBuffer;

    /// A T5 bank shaped like `code_post_gfx`'s menu sounds: a loaded clip per
    /// alias, the slider with a secondary layer, all in mixer group 18.
    fn ui_bank(aliases: &[(&str, Option<&str>)]) -> SoundCatalog {
        let mut bank = SoundCatalog::default();
        bank.set_capture_game(ZoneGame::T5);
        bank.ingest_mixer_groups(
            AssetNamespace::T5,
            (0..32)
                .map(|_| asset_audio::MixerGroup {
                    parent: -1,
                    attenuation: 0.5,
                })
                .collect(),
        );
        for (alias, secondary) in aliases {
            let file = format!("sound\\uin\\{alias}.wav");
            let mut loaded = LoadedSoundPcm::captured(
                file.clone(),
                asset_audio::MSS_PCM,
                48000,
                1,
                vec![0, 0, 0x10, 0, 0, 0x20],
                Vec::new(),
            );
            loaded.game = ZoneGame::T5;
            bank.ingest_loaded(loaded);
            bank.ingest_sound(CapturedSound {
                name: (*alias).to_owned(),
                aliases: vec![CapturedAlias {
                    alias_name: (*alias).to_owned(),
                    loaded_name: Some(file),
                    secondary: secondary.map(str::to_owned),
                    file_type: Some(1),
                    vol_min: 0.5,
                    vol_max: 0.5,
                    pitch_min: 1.0,
                    pitch_max: 1.0,
                    probability: 1.0,
                    // 2D, not looping, mixer group 18 (BO1's UI flags).
                    flags: Some(0x1212_4008),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        bank.finalize();
        bank
    }

    fn full_bank() -> SoundCatalog {
        let mut aliases: Vec<(&str, Option<&str>)> = UiSoundKind::ALL
            .iter()
            .map(|kind| (kind.alias(), None))
            .collect();
        aliases[5].1 = Some("uin_options_slider_rs");
        aliases.push(("uin_options_slider_rs", None));
        ui_bank(&aliases)
    }

    #[test]
    fn kinds_name_distinct_bo1_aliases() {
        let aliases: HashSet<_> = UiSoundKind::ALL.iter().map(|kind| kind.alias()).collect();
        assert_eq!(aliases.len(), UiSoundKind::ALL.len());
        assert!(aliases.iter().all(|alias| alias.starts_with("uin_")));
        assert_eq!(UiSound::from(UiSoundKind::Over), UiSound(UiSoundKind::Over));
    }

    #[test]
    fn every_kind_and_the_slider_layer_decode_resident() {
        let bank = full_bank();
        let (clips, missing) = decode_ui_clips(&bank);
        assert!(missing.is_empty(), "missing {missing:?}");
        // Six kinds plus the slider's secondary layer.
        assert_eq!(clips.len(), 7);
        let rs = clip_key_for_variant(&bank, AssetNamespace::T5, "uin_options_slider_rs", None, 0)
            .expect("secondary clip");
        assert!(clips.iter().any(|(key, _)| *key == rs));

        let bank = Arc::new(bank);
        let media = MediaService::resident_clips(Arc::clone(&bank), clips);
        assert_eq!(media.bank_revision(), bank.revision());
        for kind in UiSoundKind::ALL {
            let key = clip_key_for_variant(&bank, AssetNamespace::T5, kind.alias(), None, 0)
                .expect("clip key");
            assert!(matches!(media.ready(&key), Some(Ok(_))), "{kind:?}");
        }
        assert!(!media.request(ClipKey::Loaded(99)));
    }

    #[test]
    fn a_missing_alias_silences_only_its_kind() {
        let bank = ui_bank(&[
            ("uin_navigation_click", None),
            ("uin_navigation_over", None),
        ]);
        let (clips, missing) = decode_ui_clips(&bank);
        assert_eq!(clips.len(), 2);
        assert_eq!(
            missing,
            HashSet::from([
                UiSoundKind::Back,
                UiSoundKind::Open,
                UiSoundKind::Close,
                UiSoundKind::Slider
            ])
        );
    }

    #[test]
    fn ui_bank_plays_at_the_alias_volume_without_the_mixer_group() {
        let mut bank = full_bank();
        let index = bank
            .index_in(AssetNamespace::T5, "uin_navigation_click")
            .expect("alias");
        let grouped = bank.playback_policy(index, 0).expect("policy").volume(0.5);
        assert_eq!(grouped.ok(), Some(0.25));
        prepare_ui_bank(&mut bank);
        let policy = bank.playback_policy(index, 0).expect("policy");
        assert_eq!(policy.volume(0.5).ok(), Some(0.5));
        assert!(policy.spatial.is_none(), "menu sounds are 2D");
        assert!(!policy.authored_looping.unwrap_or(false));
        assert!(policy.composition.unsupported.is_empty());
    }

    /// The cue path end to end, without a device: a kind triggered against the
    /// resident bank starts from its resident clip whatever store the runtime
    /// holds (here none at all).
    #[test]
    fn a_kind_starts_from_the_resident_clips() {
        let mut bank = full_bank();
        prepare_ui_bank(&mut bank);
        let (clips, _) = decode_ui_clips(&bank);
        let bank = Arc::new(bank);
        let media = MediaService::resident_clips(Arc::clone(&bank), clips);
        let runtime = crate::AudioRuntime::new(false);
        let handle = runtime.trigger_cue_with_media(
            CueTrigger {
                event: None,
                bank: Arc::clone(&bank),
                namespace: AssetNamespace::T5,
                alias: UiSoundKind::Slider.alias().to_owned(),
                bound: None,
                origin_inches: None,
                emitter: Some(SND_ENT_LOCAL),
                class: SoundClass::Ui,
                epoch: 0,
                pitch_scale: 1.0,
                fallbacks: Vec::new(),
            },
            media,
        );
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let decision = loop {
            if let Some(decision) = handle.completion() {
                break decision;
            }
            assert!(Instant::now() < deadline, "cue never completed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(decision.outcome, StartOutcome::Submitted, "{decision:?}");
        let (layer, outcome) = decision.secondary.expect("slider layer");
        assert_eq!(layer, "uin_options_slider_rs");
        assert_eq!(outcome, StartOutcome::Submitted);
    }

    #[test]
    fn resident_pcm_matches_the_loaded_clip() {
        let bank = full_bank();
        let (clips, _) = decode_ui_clips(&bank);
        let pcm: &PcmBuffer = &clips[0].1;
        assert_eq!((pcm.channels(), pcm.rate(), pcm.len()), (1, 48000, 3));
    }

    /// Reads the real BO1 install named by `IW4L_GAMES` (run with
    /// `--ignored`): every kind must come out of `code_post_gfx`.
    #[test]
    #[ignore = "needs a BO1 install in IW4L_GAMES"]
    fn bo1_install_yields_every_menu_sound() {
        let root = std::env::var_os("IW4L_GAMES").expect("IW4L_GAMES");
        let clips = load_ui_clips(&GamesRoot(root.into())).expect("ui sounds");
        assert!(clips.missing.is_empty(), "missing {:?}", clips.missing);
        for kind in UiSoundKind::ALL {
            let key = clip_key_for_variant(&clips.bank, AssetNamespace::T5, kind.alias(), None, 0)
                .expect("clip key");
            let Some(Ok(pcm)) = clips.media.ready(&key) else {
                panic!("{kind:?} not resident");
            };
            let index = clips
                .bank
                .index_in(AssetNamespace::T5, kind.alias())
                .expect("alias");
            let volume = clips
                .bank
                .playback_policy(index, 0)
                .expect("policy")
                .volume(0.5);
            let peak = pcm
                .pcm16_bytes()
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
                .max()
                .unwrap_or(0);
            println!(
                "{kind:?} `{}`: {} ch {} Hz {:.3}s peak {peak} volume {volume:?}",
                kind.alias(),
                pcm.channels(),
                pcm.rate(),
                (pcm.len() / usize::from(pcm.channels())) as f64 / f64::from(pcm.rate())
            );
            assert!(peak > 1000, "{kind:?} decoded to near silence");
        }
    }
}
