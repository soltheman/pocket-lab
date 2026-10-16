//! The parts of the BO1 zombies HUD that come from BO1's menus, which this runtime does
//! not present: points, ammo and game over (the round is BO1's own chalk HUD element). The
//! zombies scripts' HUD feed sends points and game over as client dvars (`ui_zm_points`,
//! `ui_zm_gameover`); the ammo comes from the HUD's readout. Nothing shows until a feed
//! arrives, so other maps are untouched.

use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use ui::UiLayer;

use crate::game_text::{GameFont, GameText, GameTextMut};

const POINTS_DVAR: &str = "ui_zm_points";
/// The rounds survived, once the game is over (empty while it is not).
const GAME_OVER_DVAR: &str = "ui_zm_gameover";
/// Clear of the notch / Dynamic Island in landscape, as the touch controls.
const EDGE: f32 = 56.0;

#[derive(Component)]
pub(crate) struct ZombiesHudRoot;

#[derive(Component)]
pub(crate) struct ZombiesPoints;

/// The points' box: BO1's place for it (bottom right, above the ammo) or, with the touch
/// buttons over the bottom right, the top right.
#[derive(Component)]
pub(crate) struct ZombiesScoreBlock;

/// The weapon name over the clip and reserve, BO1's bottom right.
#[derive(Component)]
pub(crate) struct ZombiesAmmoBlock;

#[derive(Component)]
pub(crate) struct ZombiesWeaponName;

#[derive(Component)]
pub(crate) struct ZombiesClip;

#[derive(Component)]
pub(crate) struct ZombiesStock;

/// The text sizes at a 390-point-tall screen (an iPhone in landscape); scaled with the
/// screen's height.
const POINTS_PX: f32 = 25.0;
const CLIP_PX: f32 = 30.0;
const STOCK_PX: f32 = 18.0;
const WEAPON_PX: f32 = 13.0;
const REFERENCE_HEIGHT: f32 = 390.0;
/// BO1's score sits 100 of 480 virtual units above the bottom (`score_y` in
/// `_zombiemode_score`, "Location from hud.menu").
const BO1_SCORE_Y: f32 = 100.0 / 480.0;
const SCORE_WHITE: Color = Color::srgb(0.96, 0.96, 0.92);
const AMMO_WHITE: Color = Color::srgb(0.93, 0.93, 0.9);
const AMMO_GREY: Color = Color::srgb(0.7, 0.7, 0.68);
const AMMO_LOW: Color = Color::srgb(0.85, 0.12, 0.08);

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
const HINT_PX: f32 = 22.0;

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

pub(crate) fn spawn_zombies_hud(commands: &mut Commands) {
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
            // The round is BO1's own chalk HUD element now (bottom left). Points, then the
            // ammo, on the right; `place_zombies_hud` puts them where the screen has room.
            root.spawn((
                ZombiesScoreBlock,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(EDGE),
                    bottom: px(80),
                    padding: UiRect::axes(px(10), px(2)),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ))
            .with_children(|block| {
                block.spawn((
                    ZombiesPoints,
                    GameText::new(GameFont::Big, POINTS_PX, SCORE_WHITE, ""),
                ));
            });
            root.spawn((
                ZombiesAmmoBlock,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(EDGE),
                    bottom: px(12),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|block| {
                block.spawn((
                    ZombiesWeaponName,
                    GameText::new(GameFont::Small, WEAPON_PX, AMMO_GREY, ""),
                ));
                block
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::FlexEnd,
                        column_gap: px(6),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            ZombiesClip,
                            GameText::new(GameFont::Big, CLIP_PX, AMMO_WHITE, ""),
                        ));
                        row.spawn((
                            ZombiesStock,
                            GameText::new(GameFont::Big, STOCK_PX, AMMO_GREY, ""),
                        ));
                    });
            });
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
                    GameText::new(GameFont::Normal, HINT_PX, Color::srgb(0.95, 0.95, 0.95), ""),
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
                over.spawn(GameText::new(
                    GameFont::Big,
                    54.0,
                    Color::srgb(0.75, 0.06, 0.04),
                    "GAME OVER",
                ));
                over.spawn((
                    ZombiesSurvived,
                    GameText::new(GameFont::Normal, 26.0, Color::srgb(0.92, 0.92, 0.92), ""),
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
    mut points: Query<&mut GameText, (With<ZombiesPoints>, Without<ZombiesSurvived>)>,
    mut survived: Query<&mut GameText, (With<ZombiesSurvived>, Without<ZombiesPoints>)>,
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
        if let Ok(mut text) = survived.single_mut() {
            text.set_text(&line);
        }
    }
    if let Ok(mut text) = points.single_mut() {
        text.set_text(&score);
    }
}

/// The points and ammo where BO1 has them (right side, points 100/480 up, ammo under them)
/// or, while the touch buttons fill the bottom right, stacked at the top right; text sized
/// with the screen; the ammo from the HUD's readout, the clip red once it runs low.
#[allow(clippy::type_complexity)]
pub(crate) fn place_zombies_hud(
    touch: Option<Res<crate::touch_controls::TouchGameplay>>,
    readout: Option<Res<hud::AmmoReadout>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut score: Query<&mut Node, (With<ZombiesScoreBlock>, Without<ZombiesAmmoBlock>)>,
    mut ammo: Query<
        (&mut Node, &mut Visibility),
        (With<ZombiesAmmoBlock>, Without<ZombiesScoreBlock>),
    >,
    mut texts: ParamSet<(
        Query<&mut GameText, With<ZombiesPoints>>,
        Query<&mut GameText, With<ZombiesWeaponName>>,
        Query<&mut GameText, With<ZombiesClip>>,
        Query<&mut GameText, With<ZombiesStock>>,
    )>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let height = window.height().max(1.0);
    let scale = (height / REFERENCE_HEIGHT).clamp(0.8, 2.5);
    let buttons = touch.as_deref().is_some_and(|touch| touch.enabled);
    let size = |px: f32| (px * scale).round();
    let (score_at, ammo_at) = if buttons {
        // Top right, clear of the Dynamic Island: points in the corner and the ammo left
        // of them, both above the touch buttons (SWAP rises to 74 points from the top).
        let top = 8.0 * scale;
        (
            (px(EDGE), None, Some(px(top))),
            (px(EDGE + 170.0 * scale), None, Some(px(top))),
        )
    } else {
        let right = px(height * 0.05);
        (
            (right, Some(px(height * BO1_SCORE_Y)), None),
            (right, Some(px(height * 0.03)), None),
        )
    };
    let apply = |node: &mut Node, (right, bottom, top): (Val, Option<Val>, Option<Val>)| {
        let bottom = bottom.unwrap_or(Val::Auto);
        let top = top.unwrap_or(Val::Auto);
        if node.right != right || node.bottom != bottom || node.top != top {
            node.right = right;
            node.bottom = bottom;
            node.top = top;
        }
    };
    if let Ok(mut node) = score.single_mut() {
        apply(&mut node, score_at);
    }
    if let Ok(mut text) = texts.p0().single_mut() {
        text.set_size(size(POINTS_PX));
    }
    let Ok((mut node, mut visibility)) = ammo.single_mut() else {
        return;
    };
    apply(&mut node, ammo_at);
    let readout = readout.as_deref().filter(|r| !r.hidden && r.clip.is_some());
    let Some(readout) = readout else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    if let Ok(mut text) = texts.p1().single_mut() {
        text.set_size(size(WEAPON_PX));
        text.set_text(&readout.weapon.to_uppercase());
    }
    let clip = readout.clip.unwrap_or(0);
    if let Ok(mut text) = texts.p2().single_mut() {
        text.set_size(size(CLIP_PX));
        text.set_text(&clip.to_string());
        let low = readout.clip_size > 0 && clip * 3 <= readout.clip_size;
        text.set_color(if low { AMMO_LOW } else { AMMO_WHITE });
    }
    if let Ok(mut text) = texts.p3().single_mut() {
        text.set_size(size(STOCK_PX));
        let shown = readout
            .stock
            .map(|stock| format!("/ {stock}"))
            .unwrap_or_default();
        text.set_text(&shown);
    }
}

/// BO1's blood smear behind the points (`scorebar_zom_<slot>`) once it has loaded: the
/// points sit on it, as BO1 draws them; the dark plate stands in until then.
pub(crate) fn stain_zombies_score(
    mut commands: Commands,
    stains: Option<Res<hud::ScoreStains>>,
    local: Option<Res<LocalPresentClient>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut block: Query<
        (Entity, &mut Node, &mut BackgroundColor, Option<&ImageNode>),
        With<ZombiesScoreBlock>,
    >,
) {
    let Ok((entity, mut node, mut plate, image)) = block.single_mut() else {
        return;
    };
    let slot = local.as_deref().map_or(0, |local| local.0.0 as usize % 4);
    let Some(stain) = stains.as_deref().and_then(|stains| stains.0[slot].clone()) else {
        return;
    };
    if image.is_none_or(|image| image.image != stain) {
        // BO1's smear is white; the engine tints it. Blood red here, as asked for.
        commands
            .entity(entity)
            .insert(ImageNode::new(stain).with_color(Color::srgba(0.55, 0.03, 0.02, 0.92)));
        plate.0 = Color::NONE;
    }
    let scale = window.single().map_or(1.0, |window| {
        (window.height() / REFERENCE_HEIGHT).clamp(0.8, 2.5)
    });
    // The smear is 8:1; drawn at 4:1 it keeps its shape well enough and fits by the ammo.
    let (width, height) = (px(STAIN_WIDTH * scale), px(STAIN_HEIGHT * scale));
    if node.width != width || node.height != height {
        node.width = width;
        node.height = height;
        node.justify_content = JustifyContent::Center;
        node.align_items = AlignItems::Center;
        node.padding = UiRect::ZERO;
    }
}

const STAIN_WIDTH: f32 = 200.0;
const STAIN_HEIGHT: f32 = 66.0;

/// The use hint, when the IW4 HUD could not draw it (a BO1-only install has no IW4 menus
/// or font); hidden with the rest of the zombies HUD.
pub(crate) fn update_zombies_hint(
    hint: Option<Res<hud::UseHintText>>,
    touch: Option<Res<crate::touch_controls::TouchGameplay>>,
    mut text: Query<&mut GameText, With<ZombiesHint>>,
    mut row: Query<&mut Node, With<ZombiesHintRow>>,
) {
    let Ok(mut text) = text.single_mut() else {
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
    text.set_text(&line);
    let color = text.color.with_alpha(alpha);
    text.set_color(color);
}
