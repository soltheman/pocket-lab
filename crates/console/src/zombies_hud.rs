//! The BO1 zombies HUD: points and the round. BO1 draws them with script HUD elements and
//! its own menus, which this runtime does not present; the zombies scripts' HUD feed sends
//! them as client dvars (`ui_zm_points`, `ui_zm_round`) and this draws them. Nothing shows
//! until a feed arrives, so other maps are untouched.

use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use ui::UiLayer;

const POINTS_DVAR: &str = "ui_zm_points";
const ROUND_DVAR: &str = "ui_zm_round";
/// Clear of the notch / Dynamic Island in landscape, as the touch controls.
const EDGE: f32 = 56.0;

#[derive(Component)]
pub(crate) struct ZombiesHudRoot;

#[derive(Component)]
pub(crate) struct ZombiesPoints;

#[derive(Component)]
pub(crate) struct ZombiesRound;

pub(crate) fn spawn_zombies_hud(commands: &mut Commands, font: Handle<Font>) {
    commands
        .spawn((
            ZombiesHudRoot,
            UiLayer::Hud,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                ..default()
            },
            bevy::ui::FocusPolicy::Pass,
            GlobalZIndex(17_000),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            // The round, chalk red, top left.
            root.spawn((
                ZombiesRound,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(EDGE),
                    top: px(10),
                    ..default()
                },
                Text::new(""),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(34.0),
                    ..default()
                },
                TextColor(Color::srgb(0.62, 0.05, 0.03)),
                TextShadow::default(),
            ));
            // Points, top right, in the yellow of BO1's score.
            root.spawn((
                ZombiesPoints,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(EDGE),
                    top: px(12),
                    ..default()
                },
                Text::new(""),
                TextFont {
                    font: font.into(),
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::srgb(0.98, 0.86, 0.33)),
                TextShadow::default(),
            ));
        });
}

pub(crate) fn update_zombies_hud(
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    screen: Option<Res<frame::AppScreen>>,
    mut root: Query<&mut Visibility, With<ZombiesHudRoot>>,
    mut points: Query<&mut Text, (With<ZombiesPoints>, Without<ZombiesRound>)>,
    mut round: Query<&mut Text, (With<ZombiesRound>, Without<ZombiesPoints>)>,
) {
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    let dvars = presented.as_deref().zip(local.as_deref()).and_then(|(presented, local)| {
        let meta = presented.snapshot()?.meta.for_client(local.0)?;
        let value = |name: &str| {
            meta.client_dvars
                .iter()
                .rev()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.to_string())
        };
        Some((value(POINTS_DVAR), value(ROUND_DVAR)))
    });
    let in_game = screen.is_some_and(|s| *s == frame::AppScreen::InGame);
    let Some((Some(score), round_number)) = dvars.filter(|_| in_game) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    if let Ok(mut text) = points.single_mut()
        && text.0 != score
    {
        text.0 = score;
    }
    let round_text = round_number
        .filter(|r| r != "0")
        .unwrap_or_default();
    if let Ok(mut text) = round.single_mut()
        && text.0 != round_text
    {
        text.0 = round_text;
    }
}
