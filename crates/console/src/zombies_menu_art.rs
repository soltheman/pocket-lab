//! The zombies menu's atmosphere, in Black Ops' own art read from the install: the
//! zombie lobby's blood splash behind the title (`menu_zom_lobby_blood_big01`), the Group
//! 935 emblem (`zombie_logo_c`) faint behind it, embers rising (`fxt_debris_fire_ember_01`),
//! old-film scratches and grain over everything (`film_scratch`, `menu_tv_noise`), a dark
//! vignette, and the map's picture slowly pushing in.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use bevy::ui::FocusPolicy;
use bevy::ui::widget::NodeImageMode;

use crate::zombies_menu::{MenuBackground, ZombiesMenuStart};

/// How a picture is turned into what the menu draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// As stored (tinted when drawn).
    Plain,
    /// White with dark scratches: the scratches become a dark, see-through layer.
    Scratches,
    /// Coloured noise: a faint grey grain.
    Grain,
}

const ART: [(&str, Look); 5] = [
    ("menu_zom_lobby_blood_big01", Look::Plain),
    ("zombie_logo_c", Look::Plain),
    ("fxt_debris_fire_ember_01", Look::Plain),
    ("film_scratch", Look::Scratches),
    ("menu_tv_noise", Look::Grain),
];
const SPLATTER: usize = 0;
const LOGO: usize = 1;
const EMBER: usize = 2;
const SCRATCH: usize = 3;
const GRAIN: usize = 4;

const EMBERS: usize = 26;

/// Which picture a node shows once it is decoded, and its tint.
#[derive(Component, Clone, Copy)]
pub(crate) struct MenuArtNode {
    art: usize,
    tint: Color,
    stretch: bool,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct MenuEmber(f32);

#[derive(Component)]
pub(crate) struct MenuScratch;

#[derive(Component)]
pub(crate) struct MenuGrain;

#[derive(Component)]
pub(crate) struct MenuSplatter;

#[derive(Resource, Default)]
pub(crate) struct MenuArt {
    started: bool,
    tasks: Vec<(usize, Task<Option<(u32, u32, Vec<u8>)>>)>,
    images: [Option<Handle<Image>>; ART.len()],
}

fn turn(look: Look, (width, height, mut rgba): (u32, u32, Vec<u8>)) -> (u32, u32, Vec<u8>) {
    for pixel in rgba.chunks_exact_mut(4) {
        let lum = (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3;
        match look {
            Look::Plain => {}
            Look::Scratches => {
                let dark = 255 - lum.min(255) as u8;
                pixel.copy_from_slice(&[10, 6, 4, dark]);
            }
            Look::Grain => {
                let grey = lum as u8;
                pixel.copy_from_slice(&[grey, grey, grey, 255]);
            }
        }
    }
    (width, height, rgba)
}

/// A full-screen layer.
fn layer() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: percent(100),
        height: percent(100),
        ..default()
    }
}

/// Behind the menu's text: the dark wash, the 935 emblem, the embers. `s` is the screen
/// scale.
pub(crate) fn spawn_backdrop(root: &mut ChildSpawnerCommands, s: f32) {
    // BO1's menus sit on a dark wash, darkest under the text on the left and along
    // the bottom.
    root.spawn((
        layer(),
        BackgroundGradient(vec![
            Gradient::Linear(LinearGradient::to_right(vec![
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.82), 0.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.45), 32.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 55.0),
            ])),
            Gradient::Linear(LinearGradient::to_top(vec![
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.75), 0.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 32.0),
            ])),
        ]),
        FocusPolicy::Pass,
    ));
    // Group 935's emblem, faint, behind the title.
    root.spawn((
        MenuArtNode {
            art: LOGO,
            tint: Color::srgba(0.75, 0.68, 0.6, 0.07),
            stretch: false,
        },
        Node {
            position_type: PositionType::Absolute,
            left: px(-40.0 * s),
            top: percent(4),
            width: px(330.0 * s),
            height: px(330.0 * s),
            ..default()
        },
        FocusPolicy::Pass,
    ));
    // The blood splash BO1's zombie lobby puts behind its title.
    root.spawn((
        MenuSplatter,
        MenuArtNode {
            art: SPLATTER,
            tint: Color::srgba(0.42, 0.01, 0.01, 0.9),
            stretch: false,
        },
        Node {
            position_type: PositionType::Absolute,
            left: px(10.0 * s),
            top: percent(6),
            width: px(330.0 * s),
            height: px(250.0 * s),
            ..default()
        },
        UiTransform::from_rotation(Rot2::degrees(-8.0)),
        FocusPolicy::Pass,
    ));
    for index in 0..EMBERS {
        // A spread of seeds in 0..1, so the embers don't line up.
        let seed = (index as f32 * 0.618_034).fract();
        let size = (3.0 + 7.0 * ((index * 7 % 11) as f32 / 10.0)) * s;
        root.spawn((
            MenuEmber(seed),
            MenuArtNode {
                art: EMBER,
                tint: Color::srgba(1.0, 0.55, 0.2, 0.0),
                stretch: true,
            },
            Node {
                position_type: PositionType::Absolute,
                width: px(size),
                height: px(size),
                ..default()
            },
            FocusPolicy::Pass,
        ));
    }
}

/// Over everything: the vignette, film scratches and grain.
pub(crate) fn spawn_film(root: &mut ChildSpawnerCommands) {
    root.spawn((
        layer(),
        BackgroundGradient(vec![Gradient::Radial(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 60.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.6), 100.0),
            ],
        ))]),
        FocusPolicy::Pass,
    ));
    root.spawn((
        MenuScratch,
        MenuArtNode {
            art: SCRATCH,
            tint: Color::srgba(1.0, 1.0, 1.0, 0.35),
            stretch: true,
        },
        Node {
            position_type: PositionType::Absolute,
            left: percent(-10),
            top: percent(-10),
            width: percent(120),
            height: percent(120),
            ..default()
        },
        FocusPolicy::Pass,
    ));
    root.spawn((
        MenuGrain,
        MenuArtNode {
            art: GRAIN,
            tint: Color::srgba(1.0, 1.0, 1.0, 0.035),
            stretch: false,
        },
        Node {
            position_type: PositionType::Absolute,
            left: percent(-20),
            top: percent(-20),
            width: percent(140),
            height: percent(140),
            ..default()
        },
        FocusPolicy::Pass,
    ));
}

/// `IW4L_CINEMATIC=1` (recording the menu's map flybys): every HUD layer stays hidden.
pub(crate) fn hide_hud_for_cinematic(mut layers: Query<(&ui::UiLayer, &mut Visibility)>) {
    if !diag::cinematic() {
        return;
    }
    for (layer, mut visibility) in &mut layers {
        if matches!(layer, ui::UiLayer::Hud) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Starts decoding the art once the menu is up (from the install the map comes from).
pub(crate) fn load_menu_art(start: Option<Res<ZombiesMenuStart>>, mut art: ResMut<MenuArt>) {
    if art.started {
        return;
    }
    let Some((zone, _)) = start.as_deref().and_then(|start| start.preview.clone()) else {
        return;
    };
    art.started = true;
    let Ok(main) = asset_transport::game_root_for_zone(&zone).map(|root| root.join("main")) else {
        return;
    };
    for (index, (name, look)) in ART.iter().enumerate() {
        let main = main.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            asset_material::decode_ui_image_from_main(&main, name)
                .ok()
                .flatten()
                .map(|picture| turn(*look, picture))
        });
        art.tasks.push((index, task));
    }
}

/// Pictures as they decode go onto the nodes waiting for them.
pub(crate) fn apply_menu_art(
    mut commands: Commands,
    mut art: ResMut<MenuArt>,
    mut images: ResMut<Assets<Image>>,
    nodes: Query<(Entity, &MenuArtNode), Without<ImageNode>>,
) {
    let mut done = Vec::new();
    art.tasks.retain_mut(|(index, task)| match future::block_on(future::poll_once(task)) {
        Some(result) => {
            done.push((*index, result));
            false
        }
        None => true,
    });
    for (index, result) in done {
        let Some((width, height, rgba)) = result else {
            diag::info!(Ui, "zombies menu: no {} in this install", ART[index].0);
            continue;
        };
        let mut image = Image::new(
            bevy::render::render_resource::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            rgba,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        image.sampler = bevy::image::ImageSampler::linear();
        art.images[index] = Some(images.add(image));
    }
    for (entity, node) in &nodes {
        if let Some(handle) = art.images[node.art].clone() {
            let mut image = ImageNode::new(handle).with_color(node.tint);
            if node.stretch {
                image.image_mode = NodeImageMode::Stretch;
            }
            commands.entity(entity).insert(image);
        }
    }
}

/// The picture pushes in and out slowly; embers drift up and flicker; the film's
/// scratches jump and its grain crawls; the splash breathes.
#[allow(clippy::type_complexity)]
pub(crate) fn animate_menu_atmosphere(
    time: Res<Time>,
    mut background: Query<&mut UiTransform, (With<MenuBackground>, Without<MenuSplatter>)>,
    mut embers: Query<(&MenuEmber, &mut Node, Option<&mut ImageNode>)>,
    mut scratch: Query<
        (&mut Node, Option<&mut ImageNode>),
        (With<MenuScratch>, Without<MenuEmber>, Without<MenuGrain>),
    >,
    mut grain: Query<
        &mut Node,
        (With<MenuGrain>, Without<MenuEmber>, Without<MenuScratch>),
    >,
    mut splatter: Query<&mut UiTransform, (With<MenuSplatter>, Without<MenuBackground>)>,
    mut frame: Local<u32>,
) {
    let t = time.elapsed_secs();
    *frame = frame.wrapping_add(1);
    if let Ok(mut transform) = background.single_mut() {
        let zoom = 1.03 + 0.02 * (t / 14.0).sin();
        transform.scale = Vec2::splat(zoom);
    }
    if let Ok(mut transform) = splatter.single_mut() {
        transform.scale = Vec2::splat(1.0 + 0.015 * (t * 0.8).sin());
    }
    for (ember, mut node, image) in &mut embers {
        let seed = ember.0;
        // Each ember rises over 9 to 16 s from below the screen, swaying.
        let period = 9.0 + 7.0 * seed;
        let phase = ((t / period) + seed).fract();
        let top = 108.0 - phase * 118.0;
        let left = 8.0 + 90.0 * seed + 3.0 * (t * (0.6 + seed) + seed * 9.0).sin();
        node.left = percent(left);
        node.top = percent(top);
        if let Some(mut image) = image {
            let fade = (phase * 6.0).min(1.0) * ((1.0 - phase) * 3.0).min(1.0);
            let flicker = 0.65 + 0.35 * (t * (5.0 + 7.0 * seed) + seed * 31.0).sin();
            image.color = image.color.with_alpha(0.85 * fade * flicker);
        }
    }
    // Old film: the scratches jump about a few times a second, now and then brighter.
    if let Ok((mut node, image)) = scratch.single_mut()
        && *frame % 5 == 0
    {
        let jump = |k: f32| ((t * k).sin() * 43_758.547).fract();
        node.left = percent(-10.0 + 10.0 * jump(12.9));
        node.top = percent(-10.0 + 10.0 * jump(78.2));
        if let Some(mut image) = image {
            image.color = image.color.with_alpha(0.1 + 0.18 * jump(37.7));
        }
    }
    if let Ok(mut node) = grain.single_mut() {
        let jump = |k: f32| ((t * k + *frame as f32).sin() * 43_758.547).fract();
        node.left = percent(-20.0 + 20.0 * jump(3.1));
        node.top = percent(-20.0 + 20.0 * jump(5.7));
    }
}
