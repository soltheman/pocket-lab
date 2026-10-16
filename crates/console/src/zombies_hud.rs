//! Stand-ins for the parts of the BO1 zombies HUD that come from BO1's menus, which this
//! runtime does not present yet: points and game over (the round is BO1's own chalk HUD
//! element). The zombies scripts' HUD feed sends them as client dvars (`ui_zm_points`,
//! `ui_zm_gameover`) and this draws them. Nothing shows until a feed arrives, so other
//! maps are untouched.

use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use ui::UiLayer;

const POINTS_DVAR: &str = "ui_zm_points";
/// The rounds survived, once the game is over (empty while it is not).
const GAME_OVER_DVAR: &str = "ui_zm_gameover";
/// Clear of the notch / Dynamic Island in landscape, as the touch controls.
const EDGE: f32 = 56.0;

#[derive(Component)]
pub(crate) struct ZombiesHudRoot;

#[derive(Component)]
pub(crate) struct ZombiesPoints;

#[derive(Component)]
pub(crate) struct ZombiesGameOver;

#[derive(Component)]
pub(crate) struct ZombiesSurvived;

#[derive(Component)]
pub(crate) struct ZombiesHint;

/// The row the hint sits in: under the crosshair, or above it while the touch buttons
/// fill the lower right of the screen.
#[derive(Component)]
pub(crate) struct ZombiesHintRow;

const HINT_TOP: f32 = 58.0;
const HINT_TOP_TOUCH: f32 = 24.0;

/// A hint as the zombies HUD shows it: no `^N` colour codes, and the touch USE button
/// named in place of a keyboard key.
fn hint_line(text: &str, bind: &str, touch: bool) -> String {
    let mut line = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
            continue;
        }
        line.push(c);
    }
    if touch && !bind.is_empty() {
        line = line.replace(bind, "USE");
    }
    line
}

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
            // The round is BO1's own chalk HUD element now (bottom left).
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
            // The use hint ("Hold USE for Weapon [500]..."), under the crosshair.
            root.spawn((
                ZombiesHintRow,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: percent(HINT_TOP),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ))
                .with_children(|row| {
                    row.spawn((
                        ZombiesHint,
                        Text::new(""),
                        TextFont {
                            font: font.clone().into(),
                            font_size: FontSize::Px(22.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.95, 0.95)),
                        TextShadow::default(),
                    ));
                });
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
    mut points: Query<&mut Text, (With<ZombiesPoints>, Without<ZombiesSurvived>)>,
    mut survived: Query<&mut Text, (With<ZombiesSurvived>, Without<ZombiesPoints>)>,
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
            Some((value(POINTS_DVAR), value(GAME_OVER_DVAR)))
        });
    let in_game = screen.is_some_and(|s| *s == frame::AppScreen::InGame);
    let Some((Some(score), game_over)) = dvars.filter(|_| in_game) else {
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
}

/// The use hint, when the IW4 HUD could not draw it (a BO1-only install has no IW4 menus
/// or font); hidden with the rest of the zombies HUD.
pub(crate) fn update_zombies_hint(
    hint: Option<Res<hud::UseHintText>>,
    touch: Option<Res<crate::touch_controls::TouchGameplay>>,
    mut text: Query<(&mut Text, &mut TextColor), With<ZombiesHint>>,
    mut row: Query<&mut Node, With<ZombiesHintRow>>,
) {
    let Ok((mut text, mut color)) = text.single_mut() else {
        return;
    };
    if let Ok(mut row) = row.single_mut() {
        let buttons = touch.as_deref().is_some_and(|touch| touch.owns_touches);
        let top = percent(if buttons { HINT_TOP_TOUCH } else { HINT_TOP });
        if row.top != top {
            row.top = top;
        }
    }
    let line = hint.as_deref().filter(|hint| !hint.drawn).and_then(|hint| {
        let touch = touch.as_deref().is_some_and(|touch| touch.enabled);
        Some((
            hint_line(hint.text.as_deref()?, &hint.bind, touch),
            hint.alpha,
        ))
    });
    let (line, alpha) = line.unwrap_or_default();
    if text.0 != line {
        text.0 = line;
    }
    color.0.set_alpha(alpha);
}
