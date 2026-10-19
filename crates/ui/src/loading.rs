use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};

use crate::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};

pub use asset_transport::LoadProgress;
pub use assets::{LoadingPreviewSource, LoadingScreen};

use crate::load_table::{RowState, format_elapsed, project, total_elapsed};

#[derive(Component)]
pub(crate) struct LoadingRoot;

/// What the spawned overlay was built for: a different title, or the same map
/// now known to be BO1 zombies, is a different screen.
#[derive(Component)]
pub(crate) struct LoadingOverlayTitle {
    title: String,
    look: LoadingLook,
}

/// Which loading screen a load gets. BO1 zombies maps get BO1's own: the
/// loadscreen picture over the whole screen, the map's name bottom-left and the
/// 935 emblem turning in the corner, with no stage table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadingLook {
    Generic,
    Bo1Zombies,
}

impl LoadingLook {
    /// The map's zone name is the reliable signal (`zombie_theater`, with or
    /// without a `t5:` prefix); the launch labels BO1 zombies loads `ZOMBIES`,
    /// which covers a screen that arrives before its preview source.
    pub(crate) fn for_load(screen: &LoadingScreen, source: Option<&LoadingPreviewSource>) -> Self {
        let zone_says = source
            .is_some_and(|source| asset_transport::t5_zombies_map_name(&source.map_name).is_some());
        let label_says = screen.mode_label().trim().eq_ignore_ascii_case("zombies");
        if zone_says || label_says {
            Self::Bo1Zombies
        } else {
            Self::Generic
        }
    }
}

/// BO1 writes where a map is under its name on the loading screen. Zone names
/// match without their `t5:` prefix; a map not listed gets no line.
const BO1_MAP_LOCATIONS: &[(&str, &str)] = &[("zombie_theater", "GERMANY")];

pub(crate) fn bo1_map_location(map_name: &str) -> Option<&'static str> {
    let (_, stem) = asset_transport::split_zone_key(map_name);
    BO1_MAP_LOCATIONS
        .iter()
        .find(|(zone, _)| zone.eq_ignore_ascii_case(stem))
        .map(|(_, line)| *line)
}

/// The BO1 overlay's picture box: sized every frame to a 16:9 frame covering
/// the screen, because BO1 stores its loadscreens square for a 16:9 display.
#[derive(Component)]
pub(crate) struct LoadingPicture;

/// The 935 emblem turning in the corner while the map loads.
#[derive(Component)]
pub(crate) struct LoadingEmblem;

/// Where the BO1 overlay says a load failed, since it has no stage table to
/// say it in.
#[derive(Component)]
pub(crate) struct LoadingFailureText;

/// Black over the first frames of the map after a BO1 load, fading out so the
/// world is never seen before gameplay has drawn it. Not part of the overlay:
/// it outlives the dismissal and despawns itself.
#[derive(Component)]
pub(crate) struct LoadingFadeCover {
    frames: u32,
    elapsed: Duration,
}

/// `zombie_logo_c`, decoded once per install and kept for every later load.
#[derive(Resource, Default)]
pub(crate) struct LoadingEmblemCache {
    image: Option<Handle<Image>>,
    tried: Option<std::path::PathBuf>,
    task: Option<Task<Option<(u32, u32, Vec<u8>)>>>,
}

#[derive(Component)]
pub(crate) struct LoadingCamera;

#[derive(Component)]
pub(crate) struct OverlayUiCamera;

fn spawn_overlay_ui_camera(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 200,

            clear_color: ClearColorConfig::Custom(Color::NONE),
            // The screen image is cleared only by the first camera to write it in a
            // frame; behind the world camera this clear never applies. On a frame the
            // world camera did not write the screen, the UI would otherwise blend over
            // whatever the image last held: an older frame (a doubled HUD) or, on Apple
            // GPUs, uninitialised memory (magenta). Black instead.
            output_mode: bevy::camera::CameraOutputMode::Write {
                blend_state: None,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
            },
            ..default()
        },
        // Flat UI: the default 4x MSAA is a full-resolution multisampled
        // target and resolve every frame for nothing visible.
        bevy::render::view::Msaa::Off,
        crate::UiCamera,
        OverlayUiCamera,
    ));
}

pub(crate) fn dismiss_loading_overlay(
    commands: &mut Commands,
    chrome: impl IntoIterator<Item = Entity>,
    overlay_cams: impl IntoIterator<Item = Entity>,
    spawn_console_camera: bool,
) {
    for entity in chrome {
        commands.entity(entity).try_despawn();
    }
    let mut overlay_n = 0usize;
    for entity in overlay_cams {
        overlay_n += 1;
        if !spawn_console_camera {
            commands.entity(entity).try_despawn();
        }
    }
    commands.remove_resource::<LoadingScreen>();
    commands.remove_resource::<LoadingPreviewTask>();
    commands.remove_resource::<LoadingPreviewSource>();
    if spawn_console_camera && overlay_n == 0 {
        spawn_overlay_ui_camera(commands);
    }
}

#[derive(Component)]
pub(crate) struct LoadingLetter(usize);

#[derive(Component)]
pub(crate) struct LoadingStatusList;

#[derive(Component)]
pub(crate) struct LoadingStatusRow;

#[derive(Component)]
pub(crate) struct LoadingStatusCaption;

#[derive(Component)]
pub(crate) struct LoadingStatusCount;

#[derive(Component)]
pub(crate) struct LoadingStatusTime;

#[derive(Resource)]
pub(crate) struct LoadingPreviewTask {
    identity: PreviewIdentity,
    task: Task<Option<(u32, u32, Vec<u8>)>>,
}

#[derive(Clone, PartialEq, Eq)]
struct PreviewIdentity {
    request_id: u64,
    map_name: String,
}

impl PreviewIdentity {
    fn matches(&self, source: &LoadingPreviewSource) -> bool {
        self.request_id == source.request_id && self.map_name == source.map_name
    }
}

const LETTER_STEP: Duration = Duration::from_millis(140);
const LETTER_FONT_PX: f32 = 56.0;
const LETTER_FONT_MIN: f32 = 28.0;
const MODE_FONT_PX: f32 = 22.0;
const STATUS_FONT_PX: f32 = 14.0;
const STATUS_NUMBER_COL_PX: f32 = 64.0;
const STATUS_NAME_GAP_PX: f32 = 28.0;
const STATUS_UNIT_COL_PX: f32 = 124.0;
/// Every row is the same height, whether it says anything or not.
const STATUS_ROW_PX: f32 = 20.0;
const STATUS_SLOTS: usize = 6;
/// The table is redrawn about ten times a second; the letters keep their own
/// per-frame animation.
const STATUS_TABLE_PERIOD: Duration = Duration::from_millis(100);
const LOADING_CLEAR: Color = Color::srgb(0.08, 0.09, 0.12);
const STATUS_TOTAL: Color = Color::srgb(0.92, 0.93, 0.95);
const STATUS_FAILED: Color = Color::srgb(1.0, 0.35, 0.25);

// The BO1 look is sized in viewport height (`Vh`), so it keeps its proportions
// from a 390-pt phone up: 1 Vh is 3.9 pt there.
/// The map's name: about 40 pt at 390 pt tall.
const BO1_TITLE_VH: f32 = 10.0;
/// The location line under it: about 16 pt.
const BO1_LOCATION_VH: f32 = 4.1;
/// The emblem: 64 pt.
const BO1_EMBLEM_VH: f32 = 64.0 / 390.0 * 100.0;
const BO1_EDGE_VH: f32 = 6.0;
/// BO1's loading indicator turns slowly, about a revolution every ten seconds.
const BO1_EMBLEM_DEG_PER_SEC: f32 = 35.0;
const BO1_EMBLEM_TINT: Color = Color::srgba(0.93, 0.88, 0.78, 0.55);
const BO1_TITLE_COLOR: Color = Color::srgb(0.96, 0.94, 0.88);
const BO1_LOCATION_COLOR: Color = Color::srgb(0.6, 0.6, 0.58);
const BO1_PICTURE_ASPECT: f32 = 16.0 / 9.0;
/// Frames of the map kept fully black before the fade starts: the first ones
/// after a load are the camera and HUD settling.
const FADE_HOLD_FRAMES: u32 = 3;
const FADE_DURATION: Duration = Duration::from_millis(800);
/// One slow frame (the first in-game ones often are) must not eat the fade.
const FADE_MAX_STEP: Duration = Duration::from_millis(33);
/// Above the world and the loading overlay, under the zombies menus and HUD.
const FADE_Z: i32 = 19_000;

fn loading_text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(1.5),
        color: Color::linear_rgba(0., 0., 0., 0.75),
        ..default()
    }
}

/// BO1's loading text sits on a picture, not a plain backdrop: a heavier shadow.
fn bo1_text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(2.0),
        color: Color::linear_rgba(0., 0., 0., 0.9),
        ..default()
    }
}

fn game_text_font_vh(font: &Handle<Font>, size_vh: f32) -> TextFont {
    TextFont {
        font_size: bevy::text::FontSize::Vh(size_vh),
        ..game_text_font(font, 0.0)
    }
}

pub(crate) fn spawn_loading_screen(
    mut commands: Commands,
    screen: Option<Res<LoadingScreen>>,
    font: Option<Res<GameUiFont>>,
    zone_ff: Option<Res<LoadingPreviewSource>>,
    existing: Query<(Entity, Option<&LoadingOverlayTitle>), With<LoadingRoot>>,
    cameras: Query<Entity, With<LoadingCamera>>,
    leftover_overlay: Query<Entity, With<OverlayUiCamera>>,
    fade_covers: Query<Entity, With<LoadingFadeCover>>,
    inflight_preview: Option<Res<LoadingPreviewTask>>,
    mut emblem: ResMut<LoadingEmblemCache>,
) {
    let Some(screen) = screen else {
        return;
    };
    let title = screen.title();
    let look = LoadingLook::for_load(&screen, zone_ff.as_deref());
    let title_ok = existing
        .iter()
        .any(|(_, spawned)| spawned.is_some_and(|s| s.title == title && s.look == look));
    if title_ok && leftover_overlay.is_empty() {
        return;
    }
    let Some(font) = font else {
        diag::warn!(Ui, "loading: GameUiFont missing; loading screen skipped");
        return;
    };
    let font = font.0.clone();

    let stale: Vec<Entity> = existing
        .iter()
        .map(|(entity, _)| entity)
        .chain(cameras.iter())
        .chain(leftover_overlay.iter())
        // A new load's screen replaces the last one's fade, not the other way round.
        .chain(fade_covers.iter())
        .collect();
    for entity in stale {
        commands.entity(entity).try_despawn();
    }

    commands.spawn((
        Camera2d,
        Camera {
            order: 100,
            clear_color: ClearColorConfig::Custom(LOADING_CLEAR),
            ..default()
        },
        bevy::render::view::Msaa::Off,
        crate::UiCamera,
        LoadingCamera,
    ));

    let root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),

                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                // The BO1 picture is a 16:9 frame covering the screen, so it is
                // taller or wider than the root on most phones.
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(match look {
                LoadingLook::Generic => LOADING_CLEAR,
                LoadingLook::Bo1Zombies => Color::BLACK,
            }),
            GlobalZIndex(10_000),
            UiLayer::Loading,
            UiLayerVisibility,
            Visibility::Inherited,
            LoadingRoot,
            LoadingOverlayTitle {
                title: title.to_owned(),
                look,
            },
        ))
        .id();
    match look {
        LoadingLook::Generic => spawn_generic_chrome(&mut commands, root, &screen, &font),
        LoadingLook::Bo1Zombies => {
            let location = zone_ff
                .as_deref()
                .and_then(|source| bo1_map_location(&source.map_name));
            spawn_bo1_chrome(&mut commands, root, &screen, &font, location);
            if let Some(source) = zone_ff.as_deref() {
                begin_emblem_decode(&mut emblem, &source.path);
            }
        }
    }

    diag::info!(
        Ui,
        "loading: screen spawned (title=`{}`, mode=`{}`, look={look:?})",
        screen.title(),
        screen.mode_label()
    );
    if let Some(source) = zone_ff {
        let same = inflight_preview
            .as_deref()
            .is_some_and(|task| task.identity.matches(&source));
        if !same {
            begin_loading_preview_decode(
                &mut commands,
                source.path.clone(),
                source.map_name.clone(),
                source.request_id,
                screen.progress.clone(),
            );
        }
    }
}

/// The engine's own screen: the title's letters lighting in turn, the mode, and
/// the stage table on the right.
fn spawn_generic_chrome(
    commands: &mut Commands,
    root: Entity,
    screen: &LoadingScreen,
    font: &Handle<Font>,
) {
    commands.entity(root).with_children(|root| {
        root.spawn(Node {
            width: Val::Percent(78.0),
            margin: UiRect {
                bottom: Val::Percent(10.0),
                ..default()
            },
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(48.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: Val::Percent(48.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|left| {
                left.spawn((
                    Text::default(),
                    TextLayout::justify(Justify::Left),
                    loading_text_shadow(),
                ))
                .with_children(|text| {
                    let letter_px = letter_font_px(screen.title());
                    for (index, letter) in screen.title().chars().enumerate() {
                        text.spawn((
                            TextSpan::new(letter.to_string()),
                            game_text_font(font, letter_px),
                            TextColor(Color::srgb(0.42, 0.44, 0.47)),
                            LoadingLetter(index),
                        ));
                    }
                });
                left.spawn((
                    Text::new(screen.mode_label().to_owned()),
                    game_text_font(font, MODE_FONT_PX),
                    TextColor(Color::srgb(0.92, 0.93, 0.95)),
                    TextLayout::justify(Justify::Left),
                    loading_text_shadow(),
                ));
            });
            row.spawn(Node {
                width: Val::Percent(48.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::FlexEnd,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|right| {
                right.spawn((
                    LoadingStatusList,
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Stretch,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                ));
            });
        });
    });
}

/// BO1's zombies loading screen: the loadscreen picture over everything, a dark
/// wash along the bottom, the map's name and where it is bottom-left, and the
/// 935 emblem turning bottom-right. The stage table is not shown; the load's
/// progress is still tracked the same way.
fn spawn_bo1_chrome(
    commands: &mut Commands,
    root: Entity,
    screen: &LoadingScreen,
    font: &Handle<Font>,
    location: Option<&str>,
) {
    commands.entity(root).with_children(|root| {
        root.spawn((
            LoadingPicture,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ));
        // Without it the text would sit on whatever the picture has at the bottom.
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                height: Val::Percent(45.0),
                ..default()
            },
            BackgroundGradient(vec![Gradient::Linear(LinearGradient::to_top(vec![
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.8), 0.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.45), 45.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 100.0),
            ]))]),
        ));
        root.spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Vh(BO1_EDGE_VH),
            bottom: Val::Vh(BO1_EDGE_VH),
            max_width: Val::Percent(70.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Vh(0.6),
            ..default()
        })
        .with_children(|left| {
            left.spawn((
                Text::new(screen.title().to_uppercase()),
                game_text_font_vh(font, BO1_TITLE_VH),
                TextColor(BO1_TITLE_COLOR),
                TextLayout::justify(Justify::Left),
                bo1_text_shadow(),
            ));
            if let Some(location) = location {
                left.spawn((
                    Text::new(location.to_owned()),
                    game_text_font_vh(font, BO1_LOCATION_VH),
                    TextColor(BO1_LOCATION_COLOR),
                    TextLayout::justify(Justify::Left),
                    bo1_text_shadow(),
                ));
            }
            left.spawn((
                LoadingFailureText,
                Text::new(""),
                game_text_font_vh(font, BO1_LOCATION_VH),
                TextColor(STATUS_FAILED),
                TextLayout::justify(Justify::Left),
                bo1_text_shadow(),
            ));
        });
        root.spawn((
            LoadingEmblem,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Vh(BO1_EDGE_VH),
                bottom: Val::Vh(BO1_EDGE_VH),
                width: Val::Vh(BO1_EMBLEM_VH),
                height: Val::Vh(BO1_EMBLEM_VH),
                ..default()
            },
            UiTransform::default(),
        ));
    });
}

/// The emblem comes from the install the map is loaded from, decoded off the
/// main thread like the preview. One decode per install: later loads reuse it.
fn begin_emblem_decode(cache: &mut LoadingEmblemCache, zone_ff: &std::path::Path) {
    if cache.image.is_some() {
        return;
    }
    let Ok(main) = asset_transport::game_root_for_zone(zone_ff).map(|root| root.join("main"))
    else {
        return;
    };
    if cache.tried.as_ref() == Some(&main) {
        return;
    }
    cache.tried = Some(main.clone());
    cache.task = Some(AsyncComputeTaskPool::get().spawn(async move {
        match asset_material::decode_ui_image_from_main(&main, "zombie_logo_c") {
            Ok(image) => image,
            Err(error) => {
                diag::warn!(Ui, "loading: emblem: {error}");
                None
            }
        }
    }));
}

/// The BO1 overlay's per-frame work: the picture kept a 16:9 frame covering the
/// screen, the emblem put on once decoded and turned, a failure written out.
pub(crate) fn update_bo1_loading(
    mut commands: Commands,
    screen: Option<Res<LoadingScreen>>,
    mut emblem: ResMut<LoadingEmblemCache>,
    mut images: ResMut<Assets<Image>>,
    roots: Query<&ComputedNode, With<LoadingRoot>>,
    mut pictures: Query<&mut Node, With<LoadingPicture>>,
    mut emblems: Query<(Entity, &mut UiTransform, Has<ImageNode>), With<LoadingEmblem>>,
    mut failures: Query<&mut Text, With<LoadingFailureText>>,
) {
    if let Some(task) = emblem.task.as_mut()
        && let Some(decoded) = future::block_on(future::poll_once(task))
    {
        emblem.task = None;
        match decoded {
            Some((width, height, pixels)) => {
                emblem.image = Some(images.add(ui_image(width, height, pixels)));
                diag::info!(Ui, "loading: 935 emblem ready ({width}x{height})");
            }
            None => diag::info!(Ui, "loading: no zombie_logo_c in this install (no emblem)"),
        }
    }
    let Some(screen) = screen else {
        return;
    };

    if let Ok(root) = roots.single() {
        let size = root.size() * root.inverse_scale_factor();
        let (width, height) = cover_16_9(size);
        let left = Val::Px((size.x - width) * 0.5);
        let top = Val::Px((size.y - height) * 0.5);
        for mut node in &mut pictures {
            if node.width != Val::Px(width)
                || node.height != Val::Px(height)
                || node.left != left
                || node.top != top
            {
                node.width = Val::Px(width);
                node.height = Val::Px(height);
                node.left = left;
                node.top = top;
            }
        }
    }

    let turn = Rot2::degrees(screen.elapsed().as_secs_f32() * BO1_EMBLEM_DEG_PER_SEC);
    for (entity, mut transform, has_image) in &mut emblems {
        transform.rotation = turn;
        if !has_image && let Some(image) = emblem.image.clone() {
            commands.entity(entity).insert(
                ImageNode::new(image)
                    .with_color(BO1_EMBLEM_TINT)
                    .with_mode(bevy::ui::widget::NodeImageMode::Stretch),
            );
        }
    }

    let failure = screen
        .failure()
        .map(|reason| format!("Map load failed — {reason}"))
        .unwrap_or_default();
    for mut text in &mut failures {
        set_text(&mut text, &failure);
    }
}

/// The size of a 16:9 frame that covers `screen`: as wide as a wider screen, as
/// tall as a taller one, overflowing (and clipped) the other way.
fn cover_16_9(screen: Vec2) -> (f32, f32) {
    if screen.y <= 0.0 || screen.x <= 0.0 {
        return (screen.x.max(0.0), screen.y.max(0.0));
    }
    if screen.x / screen.y >= BO1_PICTURE_ASPECT {
        (screen.x, screen.x / BO1_PICTURE_ASPECT)
    } else {
        (screen.y * BO1_PICTURE_ASPECT, screen.y)
    }
}

/// Puts black over the map as a BO1 load ends (see [`LoadingFadeCover`]).
fn spawn_fade_cover(commands: &mut Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::BLACK),
        GlobalZIndex(FADE_Z),
        // Gameplay input is live from the first frame; the cover only darkens it.
        bevy::ui::FocusPolicy::Pass,
        LoadingFadeCover {
            frames: 0,
            elapsed: Duration::ZERO,
        },
    ));
}

/// Holds the cover black for the first few frames of the map, then fades it out
/// and despawns it.
pub(crate) fn fade_loading_cover(
    mut commands: Commands,
    time: Res<Time>,
    mut covers: Query<(Entity, &mut LoadingFadeCover, &mut BackgroundColor)>,
) {
    for (entity, mut cover, mut color) in &mut covers {
        if cover.frames < FADE_HOLD_FRAMES {
            cover.frames += 1;
            continue;
        }
        cover.elapsed += time.delta().min(FADE_MAX_STEP);
        let alpha = fade_alpha(cover.elapsed);
        if alpha <= 0.0 {
            commands.entity(entity).try_despawn();
        } else {
            color.0 = Color::BLACK.with_alpha(alpha);
        }
    }
}

fn fade_alpha(elapsed: Duration) -> f32 {
    (1.0 - elapsed.as_secs_f32() / FADE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

fn ui_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    let size = bevy::render::render_resource::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new(
        size,
        bevy::render::render_resource::TextureDimension::D2,
        pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

fn begin_loading_preview_decode(
    commands: &mut Commands,
    zone_ff: std::path::PathBuf,
    map_name: String,
    request_id: u64,
    progress: LoadProgress,
) {
    let identity = PreviewIdentity {
        request_id,
        map_name: map_name.clone(),
    };

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let stage = progress.begin(asset_transport::StageId::Preview, None);
        let decoded = match asset_material::decode_map_preview(&zone_ff, &map_name) {
            Ok(decoded) => decoded,
            Err(error) => {
                diag::warn!(Ui, "loading: preview index: {error}");
                stage.fail();
                return None;
            }
        };
        // A zone with no loadscreen is not a failure — the branch simply had
        // nothing to decode.
        match decoded.is_some() {
            true => stage.done(),
            false => stage.skip(),
        }
        decoded
    });
    commands.insert_resource(LoadingPreviewTask { identity, task });
}

pub(crate) fn poll_loading_preview(
    mut commands: Commands,
    mut task: Option<ResMut<LoadingPreviewTask>>,
    mut loading: Option<ResMut<LoadingScreen>>,
    source: Option<Res<LoadingPreviewSource>>,
    mut images: ResMut<Assets<Image>>,
    loading_root: Query<Entity, With<LoadingRoot>>,
    pictures: Query<Entity, With<LoadingPicture>>,
) {
    let Some(task) = task.as_deref_mut() else {
        return;
    };
    let Some(decoded) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    let identity = task.identity.clone();
    commands.remove_resource::<LoadingPreviewTask>();
    let Some(source) = source.as_deref() else {
        return;
    };
    if !identity.matches(source) {
        return;
    }
    let Some(loading) = loading.as_deref_mut() else {
        return;
    };
    // The BO1 look has a box of its own for the picture; the generic one shows
    // it on the root.
    let target = pictures.single().or(loading_root.single());
    match (decoded, target) {
        (Some((width, height, pixels)), Ok(target)) => {
            let handle = images.add(ui_image(width, height, pixels));
            commands
                .entity(target)
                .insert(ImageNode::new(handle).with_mode(bevy::ui::widget::NodeImageMode::Stretch));
            loading.mark_preview_ready();
            diag::info!(Ui, "loading: map preview ready ({width}x{height})");
        }
        (None, _) => {
            diag::info!(Ui, "loading: no map preview in IWD (keeping solid color)");
        }
        (_, Err(_)) => {}
    }
}

pub(crate) fn update_loading_screen(
    mut commands: Commands,
    time: Res<Time>,
    font: Option<Res<GameUiFont>>,
    mut screen: Option<ResMut<LoadingScreen>>,
    load: Option<Res<assets::MapLoadProcess>>,
    mut next_table: Local<Option<std::time::Instant>>,
    mut shown: Local<StatusTable>,
    mut letters: Query<(&LoadingLetter, &mut TextColor), Without<LoadingStatusCell>>,
    list: Query<(Entity, Option<&Children>), With<LoadingStatusList>>,
    mut cells: Query<(&LoadingStatusCell, &mut Text, &mut TextColor), Without<LoadingLetter>>,
    roots: Query<Entity, Or<(With<LoadingRoot>, With<LoadingCamera>)>>,
    looks: Query<&LoadingOverlayTitle, With<LoadingRoot>>,
) {
    let Some(screen) = screen.as_deref_mut() else {
        return;
    };

    screen.tick_elapsed(time.delta());

    let count = letters.iter().count().max(1);
    let step = (screen.elapsed().as_millis() / LETTER_STEP.as_millis()) as usize;
    let active = ping_pong(step, count);
    let inactive = if screen.preview_ready() {
        Color::srgb(0.96, 0.96, 0.97)
    } else {
        Color::srgb(0.42, 0.44, 0.47)
    };
    for (letter, mut color) in &mut letters {
        color.0 = if letter.0 == active {
            Color::srgb(1.0, 0.62, 0.12)
        } else {
            inactive
        };
    }

    if screen.is_complete() {
        // Dropping the overlay at once shows the map raw for a moment before
        // gameplay has drawn it; BO1 fades in from black instead.
        if looks
            .iter()
            .any(|spawned| spawned.look == LoadingLook::Bo1Zombies)
        {
            spawn_fade_cover(&mut commands);
        }
        dismiss_loading_overlay(&mut commands, roots.iter(), std::iter::empty(), true);
        diag::info!(Ui, "loading: screen dismissed");
        return;
    }

    let now = std::time::Instant::now();
    if next_table.is_some_and(|due| now < due) {
        return;
    }
    *next_table = Some(now + STATUS_TABLE_PERIOD);

    // The process is the load; the screen is one way of watching it. When both
    // are here the process wins, because it is the one a new request replaces.
    let progress = load
        .as_deref()
        .map(|process| &process.progress)
        .unwrap_or(&screen.progress);
    let snapshot = progress.snapshot();
    let table = project(&snapshot);

    let (Ok((list_entity, children)), Some(font)) = (list.single(), font.as_ref()) else {
        return;
    };
    if children.is_none_or(|children| children.is_empty())
        || shown.request != Some(snapshot.request_id)
    {
        for entity in shown.drain() {
            commands.entity(entity).try_despawn();
        }
        shown.request = Some(snapshot.request_id);
        let mut children = Vec::with_capacity(2 + STATUS_SLOTS);
        // Replacing children only detaches old rows, and a detached row stays on
        // screen as its own UI root.
        let head = spawn_metric_row(&mut commands, &font.0, StatusLine::Head);
        let spacer = spawn_spacer_row(&mut commands);
        shown.head = Some(head);
        shown.spacer = Some(spacer);
        children.extend([head, spacer]);
        for slot in 0..STATUS_SLOTS {
            let entity = spawn_metric_row(&mut commands, &font.0, StatusLine::Slot(slot));
            shown.slots.push((None, entity));
            children.push(entity);
        }
        commands.entity(list_entity).replace_children(&children);
    }

    for row in table.running.iter().chain(table.ended.iter()) {
        if shown.slots.iter().any(|(id, _)| *id == Some(row.id)) {
            continue;
        }
        let free = shown.slots.iter().position(|(id, _)| id.is_none());
        let slot = free.or_else(|| {
            shown.slots.iter().position(|(id, _)| {
                id.and_then(|id| table.row(id))
                    .is_some_and(|held| matches!(held.state, RowState::Done | RowState::Reused))
            })
        });
        let Some(slot) = slot else { continue };
        shown.slots[slot].0 = Some(row.id);
    }

    // Stages overlap, so this is shorter than their column added up.
    let elapsed = format_elapsed(total_elapsed(&snapshot));
    let (head_name, head_paint) = match screen.failure() {
        Some(reason) => (format!("Map load failed — {reason}"), STATUS_FAILED),
        None => ("Loading".to_owned(), STATUS_TOTAL),
    };

    for (cell, mut text, mut color) in &mut cells {
        let (value, name, paint) = match cell.line {
            StatusLine::Head => (Some(&elapsed), head_name.as_str(), head_paint),
            StatusLine::Slot(slot) => {
                match shown
                    .slots
                    .get(slot)
                    .and_then(|(id, _)| *id)
                    .and_then(|id| table.row(id))
                {
                    Some(row) => (Some(&row.value), row.name.as_str(), row.state.color()),
                    None => (None, "", STATUS_TOTAL),
                }
            }
        };
        set_text(
            &mut text,
            match cell.column {
                StatusColumn::Name => name,
                StatusColumn::Number => value.map_or("", |metric| metric.number.as_str()),
                StatusColumn::Unit => value.map_or("", |metric| metric.unit.as_str()),
            },
        );
        color.0 = paint;
    }
}

/// Rewriting a `Text` that already says this would re-lay out the row.
fn set_text(text: &mut Text, value: &str) {
    if text.0 != value {
        text.0.clear();
        text.0.push_str(value);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusColumn {
    Name,
    Number,
    Unit,
}

#[derive(Default)]
pub(crate) struct StatusTable {
    /// A second request is a different load and gets a table of its own.
    request: Option<u64>,
    head: Option<Entity>,
    spacer: Option<Entity>,
    slots: Vec<(Option<asset_transport::StageId>, Entity)>,
}

impl StatusTable {
    fn drain(&mut self) -> Vec<Entity> {
        self.head
            .take()
            .into_iter()
            .chain(self.spacer.take())
            .chain(self.slots.drain(..).map(|(_, entity)| entity))
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusLine {
    Head,
    Slot(usize),
}

#[derive(Component)]
pub(crate) struct LoadingStatusCell {
    line: StatusLine,
    column: StatusColumn,
}

fn status_row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Px(STATUS_ROW_PX),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        ..default()
    }
}

fn spawn_spacer_row(commands: &mut Commands) -> Entity {
    commands.spawn((LoadingStatusRow, status_row_node())).id()
}

fn spawn_metric_row(commands: &mut Commands, font: &Handle<Font>, line: StatusLine) -> Entity {
    let entity = commands.spawn((LoadingStatusRow, status_row_node())).id();
    commands.entity(entity).with_children(|parent| {
        parent
            .spawn(Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                overflow: Overflow::clip_x(),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::right(Val::Px(STATUS_NAME_GAP_PX)),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusCaption,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Name,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
        parent
            .spawn(status_cell_node(
                STATUS_NUMBER_COL_PX,
                JustifyContent::FlexEnd,
            ))
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusCount,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Number,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
        parent
            .spawn(Node {
                padding: UiRect::left(Val::Px(6.0)),
                ..status_cell_node(STATUS_UNIT_COL_PX, JustifyContent::FlexStart)
            })
            .with_children(|cell| {
                cell.spawn((
                    LoadingStatusTime,
                    LoadingStatusCell {
                        line,
                        column: StatusColumn::Unit,
                    },
                    Text::new(""),
                    game_text_font(font, STATUS_FONT_PX),
                    TextColor(STATUS_TOTAL),
                    status_text_layout(),
                    loading_text_shadow(),
                ));
            });
    });
    entity
}

/// A wrapped cell is a taller box in a row of fixed height, and a taller box
/// centres its first line higher than its neighbours'.
fn status_text_layout() -> TextLayout {
    TextLayout::new(Justify::Left, bevy::text::LineBreak::NoWrap)
}

fn status_cell_node(width: f32, justify: JustifyContent) -> Node {
    Node {
        width: Val::Px(width),
        min_width: Val::Px(width),
        flex_shrink: 0.0,
        justify_content: justify,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn ping_pong(step: usize, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let period = 2 * (count - 1);
    let phase = step % period;
    if phase < count { phase } else { period - phase }
}

fn letter_font_px(title: &str) -> f32 {
    let n = title.chars().count().max(1) as f32;
    (LETTER_FONT_PX * 10.0 / n).clamp(LETTER_FONT_MIN, LETTER_FONT_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(title: &str, mode: &str) -> LoadingScreen {
        LoadingScreen::new(LoadProgress::default(), title, mode)
    }

    fn source(map_name: &str) -> LoadingPreviewSource {
        LoadingPreviewSource {
            path: std::path::PathBuf::from("zone/english/x.ff"),
            map_name: map_name.to_owned(),
            request_id: 0,
        }
    }

    #[test]
    fn bo1_zombies_maps_get_the_bo1_look() {
        let kino = screen("Kino der Toten", "Free-for-all");
        assert_eq!(
            LoadingLook::for_load(&kino, Some(&source("zombie_theater"))),
            LoadingLook::Bo1Zombies
        );
        assert_eq!(
            LoadingLook::for_load(&kino, Some(&source("t5:zombie_theater"))),
            LoadingLook::Bo1Zombies
        );
        // The launch's label covers a screen whose preview source is not in yet.
        assert_eq!(
            LoadingLook::for_load(&screen("Kino der Toten", "ZOMBIES"), None),
            LoadingLook::Bo1Zombies
        );
    }

    #[test]
    fn other_maps_keep_the_generic_look() {
        let rust = screen("mp_rust", "Free-for-all");
        assert_eq!(
            LoadingLook::for_load(&rust, Some(&source("mp_rust"))),
            LoadingLook::Generic
        );
        assert_eq!(LoadingLook::for_load(&rust, None), LoadingLook::Generic);
    }

    #[test]
    fn location_line_is_per_map() {
        assert_eq!(bo1_map_location("zombie_theater"), Some("GERMANY"));
        assert_eq!(bo1_map_location("t5:ZOMBIE_THEATER"), Some("GERMANY"));
        assert_eq!(bo1_map_location("zombie_unknown"), None);
    }

    #[test]
    fn picture_covers_the_screen_at_16_9() {
        // An iPhone in landscape is wider than 16:9: full width, overflowing height.
        let (w, h) = cover_16_9(Vec2::new(844.0, 390.0));
        assert_eq!(w, 844.0);
        assert!((h - 844.0 * 9.0 / 16.0).abs() < 1e-3 && h >= 390.0);
        // A 4:3 screen: full height, overflowing width.
        let (w, h) = cover_16_9(Vec2::new(1024.0, 768.0));
        assert_eq!(h, 768.0);
        assert!((w - 768.0 * 16.0 / 9.0).abs() < 1e-3 && w >= 1024.0);
    }

    #[test]
    fn fade_runs_from_black_to_clear() {
        assert_eq!(fade_alpha(Duration::ZERO), 1.0);
        assert!((fade_alpha(FADE_DURATION / 2) - 0.5).abs() < 1e-3);
        assert_eq!(fade_alpha(FADE_DURATION), 0.0);
        assert_eq!(fade_alpha(FADE_DURATION * 2), 0.0);
    }
}
