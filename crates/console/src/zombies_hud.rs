//! The BO1 zombies HUD: points, the round and game over. BO1 draws them with script HUD
//! elements and its own menus, which this runtime does not present; the zombies scripts'
//! HUD feed sends them as client dvars (`ui_zm_points`, `ui_zm_round`, `ui_zm_gameover`)
//! and this draws them. Nothing shows until a feed arrives, so other maps are untouched.

use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use ui::UiLayer;

const POINTS_DVAR: &str = "ui_zm_points";
const ROUND_DVAR: &str = "ui_zm_round";
/// The rounds survived, once the game is over (empty while it is not).
const GAME_OVER_DVAR: &str = "ui_zm_gameover";
/// Clear of the notch / Dynamic Island in landscape, as the touch controls.
const EDGE: f32 = 56.0;

#[derive(Component)]
pub(crate) struct ZombiesHudRoot;

#[derive(Component)]
pub(crate) struct ZombiesPoints;

#[derive(Component)]
pub(crate) struct ZombiesRound;

#[derive(Component)]
pub(crate) struct ZombiesGameOver;

#[derive(Component)]
pub(crate) struct ZombiesSurvived;

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
                    font: font.clone().into(),
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::srgb(0.98, 0.86, 0.33)),
                TextShadow::default(),
            ));
            // Game over, centred: BO1's red title and the rounds survived under it.
            root.spawn((
                ZombiesGameOver,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: percent(32),
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(12),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|over| {
                over.spawn((
                    Text::new("GAME OVER"),
                    TextFont {
                        font: font.clone().into(),
                        font_size: FontSize::Px(54.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.75, 0.06, 0.04)),
                    TextShadow::default(),
                ));
                over.spawn((
                    ZombiesSurvived,
                    Text::new(""),
                    TextFont {
                        font: font.into(),
                        font_size: FontSize::Px(26.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.92, 0.92, 0.92)),
                    TextShadow::default(),
                ));
            });
        });
}

pub(crate) fn update_zombies_hud(
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    screen: Option<Res<frame::AppScreen>>,
    mut root: Query<&mut Visibility, (With<ZombiesHudRoot>, Without<ZombiesGameOver>)>,
    mut over: Query<&mut Visibility, (With<ZombiesGameOver>, Without<ZombiesHudRoot>)>,
    mut points: Query<
        &mut Text,
        (
            With<ZombiesPoints>,
            Without<ZombiesRound>,
            Without<ZombiesSurvived>,
        ),
    >,
    mut round: Query<
        &mut Text,
        (
            With<ZombiesRound>,
            Without<ZombiesPoints>,
            Without<ZombiesSurvived>,
        ),
    >,
    mut survived: Query<
        &mut Text,
        (
            With<ZombiesSurvived>,
            Without<ZombiesPoints>,
            Without<ZombiesRound>,
        ),
    >,
) {
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    let dvars = presented
        .as_deref()
        .zip(local.as_deref())
        .and_then(|(presented, local)| {
            let meta = presented.snapshot()?.meta.for_client(local.0)?;
            let value = |name: &str| {
                meta.client_dvars
                    .iter()
                    .rev()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.to_string())
            };
            Some((value(POINTS_DVAR), value(ROUND_DVAR), value(GAME_OVER_DVAR)))
        });
    let in_game = screen.is_some_and(|s| *s == frame::AppScreen::InGame);
    let Some((Some(score), round_number, game_over)) = dvars.filter(|_| in_game) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let rounds = game_over.filter(|rounds| !rounds.is_empty());
    if let Ok(mut shown) = over.single_mut() {
        shown.set_if_neq(if rounds.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if let Some(rounds) = rounds {
        let line = if rounds == "1" {
            "You Survived 1 Round".to_owned()
        } else {
            format!("You Survived {rounds} Rounds")
        };
        if let Ok(mut text) = survived.single_mut()
            && text.0 != line
        {
            text.0 = line;
        }
    }
    if let Ok(mut text) = points.single_mut()
        && text.0 != score
    {
        text.0 = score;
    }
    let round_text = round_number.filter(|r| r != "0").unwrap_or_default();
    if let Ok(mut text) = round.single_mut()
        && text.0 != round_text
    {
        text.0 = round_text;
    }
}
