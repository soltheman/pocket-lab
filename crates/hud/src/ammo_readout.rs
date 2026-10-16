//! The local player's ammo, clip and weapon name each frame, for a HUD that draws them
//! itself where the IW4 menus are not installed (a BO1-only install's zombies HUD). The
//! IW4 weapon bar computes the same from the same state but only with its menus.

use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::PM_TYPE_DEAD;
use weapon_iw4::get_viewmodel_weapon_index;

use crate::ammo::weaponbar_ammo;
use crate::gaps::HudPresentationGaps;
use crate::weapon_name::localized_weapon_name;

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct AmmoReadout {
    /// Rounds in the clip, when the weapon has one.
    pub clip: Option<i32>,
    /// Rounds in reserve, when the weapon has a reserve.
    pub stock: Option<i32>,
    /// A full clip: the clip counts as low under a third of it.
    pub clip_size: i32,
    /// The weapon's display name (localized).
    pub weapon: String,
    /// Nothing to show: no weapon, dead, or the game hides the ammo HUD.
    pub hidden: bool,
}

pub(crate) fn publish_ammo_readout(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut out: ResMut<AmmoReadout>,
) {
    let next = presented
        .player(local.0)
        .filter(|ps| ps.pm_type != PM_TYPE_DEAD)
        .and_then(|ps| {
            let weapons = weapons.as_deref()?;
            let meta = presented
                .snapshot()
                .and_then(|s| s.meta.for_client(local.0));
            let viewmodel = get_viewmodel_weapon_index(ps);
            if viewmodel == 0 {
                return None;
            }
            let ammo = weaponbar_ammo(ps, weapons, meta);
            let weapon =
                localized_weapon_name(viewmodel, Some(weapons), strings.as_deref(), &mut gaps)
                    .unwrap_or_default();
            Some(AmmoReadout {
                clip: ammo.as_ref().and_then(|a| a.clip),
                stock: ammo.as_ref().and_then(|a| a.stock),
                clip_size: ammo.as_ref().map_or(0, |a| a.clip_size),
                weapon,
                hidden: ammo.is_none(),
            })
        })
        .unwrap_or(AmmoReadout {
            hidden: true,
            ..default()
        });
    if *out != next {
        *out = next;
    }
}
