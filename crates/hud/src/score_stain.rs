//! BO1's blood smears behind each zombies player's points (`scorebar_zom_1..4`, drawn by
//! the engine's score ownerdraw, so no menu or script names them), loaded from the
//! install's IWDs for a HUD that draws the points itself.

use asset_core::AssetNamespace;
use bevy::prelude::*;

use crate::images::HudImages;

/// One smear per player slot; None until it has loaded.
#[derive(Resource, Default, Clone, Debug)]
pub struct ScoreStains(pub [Option<Handle<Image>>; 4]);

/// Tries to load the smears now and then (the games root arrives after startup) until
/// they are all in, or a minute of tries has passed.
pub(crate) fn publish_score_stains(
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut stains: ResMut<ScoreStains>,
    mut tick: Local<u32>,
) {
    if stains.0.iter().all(Option::is_some) || *tick > 60 * 60 {
        return;
    }
    *tick += 1;
    if *tick % 30 != 1 {
        return;
    }
    for (slot, stain) in stains.0.iter_mut().enumerate() {
        if stain.is_none() {
            *stain = hud_images.get(
                AssetNamespace::T5,
                &format!("scorebar_zom_{}", slot + 1),
                &mut images,
            );
            if stain.is_some() {
                diag::info!(Ui, "hud: score smear scorebar_zom_{} loaded", slot + 1);
            }
        }
    }
}
