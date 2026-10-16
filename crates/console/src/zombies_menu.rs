//! The front of a Black Ops-only install, in the look of BO1's zombies menu: the map's
//! loading-screen picture slowly drifting behind a dark fade, ZOMBIES over the map's name
//! on the left, and SOLO (CO-OP when the game hosts a co-op room) and OPTIONS under it.
//! The map starts loading only once SOLO is pressed.
//!
//! The options panel (look sensitivity, field of view, volume, invert look, and the touch
//! buttons' size and opacity) opens from here and, in a match, from the touch layer's
//! menu button. Its changes go straight into the saved settings.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;
use frame::{AppScreen, GameSettings};
use ui::{GameUiFont, UiLayer};

use crate::game_text::{GameFont, GameText, GameTextMut};

/// The match a BO1-only install starts once SOLO is pressed: what the launch would have
/// inserted at once.
pub struct ZombiesMenuLoad {
    pub request: assets::MatchLoadRequest,
    pub loading: assets::LoadingScreen,
    pub preview: Option<assets::LoadingPreviewSource>,
}

/// Present while the menu waits for SOLO.
#[derive(Resource)]
pub struct ZombiesMenuStart {
    pub load: Option<ZombiesMenuLoad>,
    /// SOLO, or CO-OP when the game hosts a co-op room.
    pub play_label: String,
    /// The map's name under the title ("KINO DER TOTEN").
    pub map_title: String,
    /// The map's zone and name, to decode its loading-screen picture for the background.
    pub preview: Option<(PathBuf, String)>,
}

/// The options panel is open (from the menu or a match).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZombiesOptionsOpen(pub bool);

const RED: Color = Color::srgb(0.78, 0.07, 0.04);
const WHITE: Color = Color::srgb(0.93, 0.92, 0.88);
const GREY: Color = Color::srgb(0.62, 0.61, 0.58);
const LABEL: Color = Color::srgb(0.8, 0.79, 0.75);
const HOT: Color = Color::srgb(1.0, 0.66, 0.18);
const PANEL: Color = Color::srgba(0.03, 0.03, 0.03, 0.94);
const BAR: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Sizes at a 390-point-tall screen (an iPhone in landscape), scaled with the screen.
const REFERENCE_HEIGHT: f32 = 390.0;
/// Clear of the notch / Dynamic Island in landscape.
const EDGE: f32 = 56.0;

#[derive(Component)]
pub(crate) struct MenuRoot;

#[derive(Component)]
pub(crate) struct MenuBackground;

#[derive(Component)]
pub(crate) struct MenuTitle;

/// The title and entries: hidden under the options panel.
#[derive(Component)]
pub(crate) struct MenuColumn;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MenuAction {
    Play,
    Options,
}

#[derive(Resource)]
pub(crate) struct MenuPreviewTask(Task<Option<(u32, u32, Vec<u8>)>>);

#[derive(Component)]
pub(crate) struct OptionsRoot;

#[derive(Component)]
pub(crate) struct OptionsDone;

#[derive(Component, Clone, Copy)]
pub(crate) struct OptionBar(Setting);

#[derive(Component, Clone, Copy)]
pub(crate) struct OptionFill(Setting);

#[derive(Component, Clone, Copy)]
pub(crate) struct OptionValue(Setting);

#[derive(Component, Clone, Copy)]
pub(crate) struct OptionStep(Setting, f32);

/// A row of the options panel, shown on its page.
#[derive(Component, Clone, Copy)]
pub(crate) struct OptionRow(OptionsTab);

/// A page's tab at the top of the options panel.
#[derive(Component, Clone, Copy)]
pub(crate) struct OptionTabButton(OptionsTab);

/// EDIT (the touch buttons' positions), on the BUTTONS page; only in a match.
#[derive(Component)]
pub(crate) struct OptionsEdit;

#[derive(Component)]
pub(crate) struct OptionsEditLabel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    Sensitivity,
    FieldOfView,
    Volume,
    InvertLook,
    Layout,
    Style,
    ButtonSize,
    ButtonOpacity,
}

/// The options panel's pages.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptionsTab {
    #[default]
    Gameplay,
    Buttons,
}

const LAYOUT_NAMES: [&str; 5] = ["CLASSIC", "LEFT FIRE", "COMPACT", "SOUTHPAW", "CUSTOM"];

impl Setting {
    const ALL: [Setting; 8] = [
        Setting::Sensitivity,
        Setting::FieldOfView,
        Setting::Volume,
        Setting::InvertLook,
        Setting::Layout,
        Setting::Style,
        Setting::ButtonSize,
        Setting::ButtonOpacity,
    ];

    fn tab(self) -> OptionsTab {
        match self {
            Setting::Sensitivity | Setting::FieldOfView | Setting::Volume | Setting::InvertLook => {
                OptionsTab::Gameplay
            }
            Setting::Layout | Setting::Style | Setting::ButtonSize | Setting::ButtonOpacity => {
                OptionsTab::Buttons
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Setting::Sensitivity => "LOOK SENSITIVITY",
            Setting::FieldOfView => "FIELD OF VIEW",
            Setting::Volume => "VOLUME",
            Setting::InvertLook => "INVERT LOOK",
            Setting::Layout => "BUTTON LAYOUT",
            Setting::Style => "BUTTON STYLE",
            Setting::ButtonSize => "BUTTON SIZE",
            Setting::ButtonOpacity => "BUTTON OPACITY",
        }
    }

    /// Lowest, highest and step.
    fn range(self) -> (f32, f32, f32) {
        match self {
            Setting::Sensitivity => (0.5, 20.0, 0.5),
            Setting::FieldOfView => (GameSettings::FOV_MIN, 110.0, 1.0),
            Setting::Volume => (0.0, 1.0, 0.05),
            Setting::InvertLook | Setting::Style => (0.0, 1.0, 1.0),
            Setting::Layout => (0.0, f32::from(GameSettings::TOUCH_LAYOUT_CUSTOM), 1.0),
            Setting::ButtonSize => (0.75, 1.35, 0.05),
            Setting::ButtonOpacity => (0.3, 1.0, 0.05),
        }
    }

    /// Steps that go round (the last layout's + comes back to the first) and toggles.
    fn cycles(self) -> bool {
        matches!(self, Setting::InvertLook | Setting::Style | Setting::Layout)
    }

    fn get(self, settings: &GameSettings) -> f32 {
        match self {
            Setting::Sensitivity => settings.sensitivity,
            Setting::FieldOfView => settings.fov,
            Setting::Volume => settings.master_volume,
            Setting::InvertLook => f32::from(u8::from(settings.invert_mouse)),
            Setting::Layout => f32::from(settings.touch_layout),
            Setting::Style => f32::from(u8::from(settings.touch_icons)),
            Setting::ButtonSize => settings.touch_button_scale,
            Setting::ButtonOpacity => settings.touch_opacity,
        }
    }

    fn set(self, settings: &mut GameSettings, value: f32) {
        let (low, high, step) = self.range();
        let value = (((value - low) / step).round() * step + low).clamp(low, high);
        match self {
            Setting::Sensitivity => settings.sensitivity = value,
            Setting::FieldOfView => settings.fov = value,
            Setting::Volume => settings.master_volume = value,
            Setting::InvertLook => settings.invert_mouse = value >= 0.5,
            Setting::Layout => settings.touch_layout = value as u8,
            Setting::Style => settings.touch_icons = value >= 0.5,
            Setting::ButtonSize => settings.touch_button_scale = value,
            Setting::ButtonOpacity => settings.touch_opacity = value,
        }
    }

    fn shown(self, value: f32) -> String {
        match self {
            Setting::Sensitivity => format!("{value:.1}"),
            Setting::FieldOfView => format!("{value:.0}"),
            Setting::InvertLook => if value >= 0.5 { "ON" } else { "OFF" }.to_owned(),
            Setting::Style => if value >= 0.5 { "ICONS" } else { "TEXT" }.to_owned(),
            Setting::Layout => LAYOUT_NAMES
                .get(value.round() as usize)
                .copied()
                .unwrap_or("CLASSIC")
                .to_owned(),
            Setting::Volume | Setting::ButtonSize | Setting::ButtonOpacity => {
                format!("{:.0}%", value * 100.0)
            }
        }
    }

    /// How far along its bar (0..1) a value sits.
    fn share(self, value: f32) -> f32 {
        let (low, high, _) = self.range();
        ((value - low) / (high - low)).clamp(0.0, 1.0)
    }
}

fn screen_scale(window: &Window) -> f32 {
    (window.height().max(1.0) / REFERENCE_HEIGHT).clamp(0.8, 2.5)
}

/// A line in BO1's fonts: bigFont for titles and entries, normalFont for the rest; the UI
/// font (the menu's `font`) until those are read.
fn text(_font: &Handle<Font>, size: f32, color: Color, line: impl Into<String>) -> GameText {
    let font = if size >= 20.0 { GameFont::Big } else { GameFont::Normal };
    GameText::new(font, size.round(), color, line)
}

/// Builds the menu once the window and font are there, and starts decoding the map's
/// picture for its background.
pub(crate) fn spawn_zombies_menu(
    mut commands: Commands,
    start: Option<Res<ZombiesMenuStart>>,
    screen: Res<AppScreen>,
    font: Option<Res<GameUiFont>>,
    window: Query<&Window, With<PrimaryWindow>>,
    existing: Query<(), With<MenuRoot>>,
) {
    let Some(start) = start.filter(|start| start.load.is_some()) else {
        return;
    };
    if *screen != AppScreen::MainMenu || !existing.is_empty() {
        return;
    }
    let (Some(font), Ok(window)) = (font, window.single()) else {
        return;
    };
    let font = font.0.clone();
    let s = screen_scale(window);
    if let Some((zone, map)) = start.preview.clone() {
        let task = AsyncComputeTaskPool::get().spawn(async move {
            asset_material::decode_map_preview(&zone, &map)
                .ok()
                .flatten()
        });
        commands.insert_resource(MenuPreviewTask(task));
    }
    diag::info!(Ui, "zombies menu: waiting for {}", start.play_label);
    commands
        .spawn((
            MenuRoot,
            UiLayer::Shell,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(16_000),
        ))
        .with_children(|root| {
            // The picture: a square wider than the screen, cropped top and bottom.
            root.spawn((
                MenuBackground,
                Node {
                    width: percent(104),
                    aspect_ratio: Some(1.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ));
            // BO1's menus sit on a dark wash, darkest under the text on the left.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundGradient(vec![Gradient::Linear(LinearGradient::to_right(vec![
                    ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.94), 0.0),
                    ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.78), 38.0),
                    ColorStop::percent(Color::srgba(0.02, 0.0, 0.0, 0.35), 100.0),
                ]))]),
            ));
            root.spawn((
                MenuColumn,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(EDGE + 8.0 * s),
                    top: px(0),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    row_gap: px(5.0 * s),
                    ..default()
                },
            ))
            .with_children(|column| {
                column.spawn(text(&font, 13.0 * s, GREY, "BLACK OPS"));
                column.spawn((MenuTitle, text(&font, 54.0 * s, RED, "ZOMBIES")));
                column.spawn(text(&font, 15.0 * s, WHITE, start.map_title.to_uppercase()));
                // A thin red rule under the title, as BO1's headers have.
                column.spawn((
                    Node {
                        width: px(180.0 * s),
                        height: px(2.0 * s),
                        margin: UiRect::vertical(px(14.0 * s)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.66, 0.06, 0.03, 0.8)),
                ));
                for (action, label) in [
                    (MenuAction::Play, start.play_label.as_str()),
                    (MenuAction::Options, "OPTIONS"),
                ] {
                    column
                        .spawn((
                            action,
                            Button,
                            Node {
                                padding: UiRect::axes(px(12.0 * s), px(6.0 * s)),
                                margin: UiRect::bottom(px(4.0 * s)),
                                border: UiRect::left(px(3.0 * s)),
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                        ))
                        .with_child(text(&font, 26.0 * s, WHITE, label));
                }
            });
        });
}

/// The background picture once decoded; a slow drift and a film-like flicker of the title.
pub(crate) fn animate_zombies_menu(
    mut commands: Commands,
    time: Res<Time>,
    task: Option<ResMut<MenuPreviewTask>>,
    mut images: ResMut<Assets<Image>>,
    mut background: Query<(Entity, &mut Node), With<MenuBackground>>,
    mut title: Query<&mut GameText, With<MenuTitle>>,
    mut column: Query<&mut Visibility, With<MenuColumn>>,
    options: Res<ZombiesOptionsOpen>,
) {
    if let Ok(mut visibility) = column.single_mut() {
        visibility.set_if_neq(if options.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    if let Some(mut task) = task
        && let Some(decoded) = future::block_on(future::poll_once(&mut task.0))
    {
        commands.remove_resource::<MenuPreviewTask>();
        if let (Some((width, height, pixels)), Ok((entity, _))) = (decoded, background.single()) {
            let mut image = Image::new(
                bevy::render::render_resource::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                pixels,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            image.sampler = bevy::image::ImageSampler::linear();
            commands
                .entity(entity)
                .insert(ImageNode::new(images.add(image)));
            diag::info!(Ui, "zombies menu: background {width}x{height}");
        }
    }
    let t = time.elapsed_secs();
    if let Ok((_, mut node)) = background.single_mut() {
        let want = percent(106.0 + 4.0 * (t / 9.0).sin());
        if node.width != want {
            node.width = want;
        }
    }
    if let Ok(mut title) = title.single_mut() {
        // Mostly steady, with the odd quick dip, like old film.
        let dip = ((t * 7.3).sin() * (t * 2.9).sin()).max(0.0).powi(8);
        title.set_color(RED.with_alpha(1.0 - 0.45 * dip));
    }
}

/// SOLO starts the map's load (the launch's own request, loading screen and preview);
/// OPTIONS opens the panel. A pressed entry lights up as BO1's do.
#[allow(clippy::type_complexity)]
pub(crate) fn zombies_menu_buttons(
    mut commands: Commands,
    mut start: Option<ResMut<ZombiesMenuStart>>,
    mut screen: ResMut<AppScreen>,
    mut options: ResMut<ZombiesOptionsOpen>,
    mut buttons: Query<
        (
            &MenuAction,
            &Interaction,
            &Children,
            &mut BorderColor,
            &mut BackgroundColor,
        ),
        Changed<Interaction>,
    >,
    mut labels: Query<&mut GameText>,
    roots: Query<Entity, With<MenuRoot>>,
    mut scripted: ResMut<MenuCommand>,
    mut tests: ScriptedOptions,
) {
    let command = scripted.0.take();
    let mut words = command.as_deref().unwrap_or("").split_whitespace();
    let mut pressed = match words.next() {
        Some("play") => Some(MenuAction::Play),
        Some("options") => Some(MenuAction::Options),
        Some("done") => {
            options.0 = false;
            None
        }
        Some("tab_buttons") => {
            *tests.tab = OptionsTab::Buttons;
            None
        }
        Some("tab_gameplay") => {
            *tests.tab = OptionsTab::Gameplay;
            None
        }
        Some("edit") => {
            options.0 = false;
            tests.editing.begin();
            None
        }
        Some("layout") => {
            if let Some(layout) = words.next().and_then(|n| n.parse::<u8>().ok()) {
                tests.settings.touch_layout = layout;
                tests.settings.sanitize();
                tests.settings.touch();
            }
            None
        }
        Some("style") => {
            tests.settings.touch_icons = words.next() != Some("text");
            tests.settings.touch();
            None
        }
        _ => None,
    };
    for (action, interaction, children, mut border, mut fill) in &mut buttons {
        let hot = matches!(interaction, Interaction::Pressed | Interaction::Hovered);
        *border = BorderColor::all(if hot { HOT } else { Color::NONE });
        fill.0 = if hot {
            Color::srgba(1.0, 0.66, 0.18, 0.12)
        } else {
            Color::NONE
        };
        for child in children.iter() {
            if let Ok(mut label) = labels.get_mut(child) {
                label.set_color(if hot { HOT } else { WHITE });
            }
        }
        if *interaction == Interaction::Pressed && !options.0 {
            pressed = Some(*action);
        }
    }
    match pressed {
        Some(MenuAction::Options) => options.0 = true,
        Some(MenuAction::Play) => {
            let Some(load) = start.as_mut().and_then(|start| start.load.take()) else {
                return;
            };
            diag::info!(Ui, "zombies menu: starting {}", load.request.zone);
            options.0 = false;
            commands.insert_resource(load.request);
            commands.insert_resource(load.loading);
            if let Some(preview) = load.preview {
                commands.insert_resource(preview);
            }
            commands.remove_resource::<ZombiesMenuStart>();
            commands.remove_resource::<MenuPreviewTask>();
            *screen = AppScreen::Loading;
            for root in &roots {
                commands.entity(root).try_despawn();
            }
        }
        None => {}
    }
}

/// The options panel, built the first time it opens, shown while it is open.
pub(crate) fn show_zombies_options(
    mut commands: Commands,
    open: Res<ZombiesOptionsOpen>,
    font: Option<Res<GameUiFont>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<OptionsRoot>>,
) {
    if let Ok(mut visibility) = root.single_mut() {
        visibility.set_if_neq(if open.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        return;
    }
    if !open.0 {
        return;
    }
    let (Some(font), Ok(window)) = (font, window.single()) else {
        return;
    };
    let font = font.0.clone();
    let s = screen_scale(window);
    let row_height = 34.0 * s;
    commands
        .spawn((
            OptionsRoot,
            UiLayer::Overlay,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            FocusPolicy::Block,
            GlobalZIndex(21_000),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(88),
                    max_width: px(640.0 * s),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(18.0 * s), px(10.0 * s)),
                    border: UiRect::top(px(3.0 * s)),
                    row_gap: px(2.0 * s),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(RED),
            ))
            .with_children(|panel| {
                // OPTIONS, and the pages' tabs on its right.
                panel
                    .spawn(Node {
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        margin: UiRect::bottom(px(4.0 * s)),
                        ..default()
                    })
                    .with_children(|top| {
                        top.spawn(text(&font, 22.0 * s, WHITE, "OPTIONS"));
                        top.spawn(Node {
                            column_gap: px(6.0 * s),
                            ..default()
                        })
                        .with_children(|tabs| {
                            for (tab, label) in [
                                (OptionsTab::Gameplay, "GAMEPLAY"),
                                (OptionsTab::Buttons, "BUTTONS"),
                            ] {
                                tabs.spawn((
                                    OptionTabButton(tab),
                                    Button,
                                    Node {
                                        padding: UiRect::axes(px(12.0 * s), px(4.0 * s)),
                                        border: UiRect::bottom(px(2.0 * s)),
                                        ..default()
                                    },
                                    BorderColor::all(Color::NONE),
                                    BackgroundColor(Color::NONE),
                                ))
                                .with_child(text(
                                    &font,
                                    14.0 * s,
                                    WHITE,
                                    label,
                                ));
                            }
                        });
                    });
                // EDIT on the BUTTONS page: drag the touch buttons about (in a match).
                panel
                    .spawn((
                        OptionRow(OptionsTab::Buttons),
                        Node {
                            height: px(row_height),
                            align_items: AlignItems::Center,
                            column_gap: px(8.0 * s),
                            ..default()
                        },
                    ))
                    .with_children(|row| {
                        row.spawn(Node {
                            width: percent(36),
                            ..default()
                        })
                        .with_child(text(
                            &font,
                            13.0 * s,
                            LABEL,
                            "MOVE BUTTONS",
                        ));
                        row.spawn((
                            OptionsEdit,
                            Button,
                            Node {
                                padding: UiRect::axes(px(16.0 * s), px(4.0 * s)),
                                border: UiRect::all(px(1.0)),
                                ..default()
                            },
                            BorderColor::all(HOT),
                            BackgroundColor(Color::srgba(1.0, 0.66, 0.18, 0.14)),
                        ))
                        .with_child((OptionsEditLabel, text(&font, 14.0 * s, WHITE, "EDIT")));
                    });
                for setting in Setting::ALL {
                    panel
                        .spawn((
                            OptionRow(setting.tab()),
                            Node {
                                height: px(row_height),
                                align_items: AlignItems::Center,
                                column_gap: px(8.0 * s),
                                ..default()
                            },
                        ))
                        .with_children(|row| {
                            row.spawn(Node {
                                width: percent(36),
                                ..default()
                            })
                            .with_child(text(
                                &font,
                                13.0 * s,
                                LABEL,
                                setting.label(),
                            ));
                            let step = |row: &mut ChildSpawnerCommands, sign: f32, label: &str| {
                                row.spawn((
                                    OptionStep(setting, sign),
                                    Button,
                                    Node {
                                        width: px(30.0 * s),
                                        height: px(26.0 * s),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        border: UiRect::all(px(1.0)),
                                        ..default()
                                    },
                                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
                                ))
                                .with_child(text(
                                    &font,
                                    16.0 * s,
                                    WHITE,
                                    label,
                                ));
                            };
                            step(row, -1.0, "-");
                            row.spawn((
                                OptionBar(setting),
                                Button,
                                RelativeCursorPosition::default(),
                                Node {
                                    flex_grow: 1.0,
                                    height: px(10.0 * s),
                                    ..default()
                                },
                                BackgroundColor(BAR),
                            ))
                            .with_child((
                                OptionFill(setting),
                                Node {
                                    width: percent(50),
                                    height: percent(100),
                                    ..default()
                                },
                                BackgroundColor(HOT),
                                FocusPolicy::Pass,
                            ));
                            step(row, 1.0, "+");
                            row.spawn(Node {
                                width: px(54.0 * s),
                                justify_content: JustifyContent::FlexEnd,
                                ..default()
                            })
                            .with_child((OptionValue(setting), text(&font, 14.0 * s, WHITE, "")));
                        });
                }
                panel
                    .spawn(Node {
                        justify_content: JustifyContent::FlexEnd,
                        margin: UiRect::top(px(6.0 * s)),
                        ..default()
                    })
                    .with_children(|bottom| {
                        bottom
                            .spawn((
                                OptionsDone,
                                Button,
                                Node {
                                    padding: UiRect::axes(px(22.0 * s), px(6.0 * s)),
                                    border: UiRect::all(px(1.0)),
                                    ..default()
                                },
                                BorderColor::all(HOT),
                                BackgroundColor(Color::srgba(1.0, 0.66, 0.18, 0.14)),
                            ))
                            .with_child(text(&font, 18.0 * s, WHITE, "DONE"));
                    });
            });
        });
}

/// Steps, bar presses (and drags) and DONE; then the bars and values follow the settings.
#[allow(clippy::type_complexity)]
pub(crate) fn zombies_options_input(
    mut open: ResMut<ZombiesOptionsOpen>,
    mut settings: ResMut<GameSettings>,
    steps: Query<(&OptionStep, &Interaction), Changed<Interaction>>,
    bars: Query<(&OptionBar, &Interaction, &RelativeCursorPosition)>,
    done: Query<&Interaction, (With<OptionsDone>, Changed<Interaction>)>,
    mut fills: Query<(&OptionFill, &mut Node)>,
    mut values: Query<(&OptionValue, &mut GameText)>,
) {
    if !open.0 {
        return;
    }
    let mut changed = false;
    for (step, interaction) in &steps {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let OptionStep(setting, sign) = *step;
        let (low, high, size) = setting.range();
        let now = setting.get(&settings);
        // Toggles flip; the layouts go round; the rest step and stop at their ends.
        let next = if setting.cycles() {
            let count = ((high - low) / size).round() + 1.0;
            low + ((now - low) / size + sign).rem_euclid(count) * size
        } else {
            now + sign * size
        };
        setting.set(&mut settings, next);
        changed = true;
    }
    for (bar, interaction, cursor) in &bars {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(at) = cursor.normalized else {
            continue;
        };
        let setting = bar.0;
        let (low, high, _) = setting.range();
        let share = (at.x + 0.5).clamp(0.0, 1.0);
        let value = low + (high - low) * share;
        if (setting.get(&settings) - value).abs() > f32::EPSILON {
            setting.set(&mut settings, value);
            changed = true;
        }
    }
    if changed {
        settings.sanitize();
        settings.touch();
    }
    if done
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        open.0 = false;
    }
    for (fill, mut node) in &mut fills {
        let want = percent(100.0 * fill.0.share(fill.0.get(&settings)));
        if node.width != want {
            node.width = want;
        }
    }
    for (value, mut text) in &mut values {
        text.set_text(&value.0.shown(value.0.get(&settings)));
    }
}

/// A scripted press for tests: `zm_menu play|options|done`.
#[derive(Resource, Default, Debug)]
pub(crate) struct MenuCommand(Option<String>);

pub(crate) fn register_zombies_menu_commands(registry: &mut crate::ConsoleRegistry) {
    if registry.resolve("zm_menu").is_none() {
        registry.register(
            crate::CommandSpec::new("zm_menu")
                .usage("zm_menu <play|options|done|tab_buttons|tab_gameplay|edit|layout N|style icons|text> — tests"),
        );
    }
}

pub(crate) fn handle_zombies_menu_commands(
    mut events: MessageReader<crate::ConsoleCommand>,
    mut pending: ResMut<MenuCommand>,
) {
    for command in events.read() {
        if command.name == "zm_menu" {
            pending.0 = Some(command.args.join(" "));
        }
    }
}

/// The options' pages: a tab shows its rows; EDIT (in a match, with the touch buttons on)
/// closes the panel and puts the touch layer into edit mode.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn zombies_options_pages(
    mut open: ResMut<ZombiesOptionsOpen>,
    mut tab: ResMut<OptionsTab>,
    screen: Res<AppScreen>,
    touch: Option<Res<crate::touch_controls::TouchGameplay>>,
    mut editing: ResMut<crate::touch_controls::TouchEditing>,
    tabs: Query<(&OptionTabButton, &Interaction), Changed<Interaction>>,
    mut tab_looks: Query<(&OptionTabButton, &mut BorderColor)>,
    edit: Query<&Interaction, (With<OptionsEdit>, Changed<Interaction>)>,
    mut edit_label: Query<&mut GameText, With<OptionsEditLabel>>,
    mut rows: Query<(&OptionRow, &mut Node)>,
) {
    if !open.0 {
        return;
    }
    for (button, interaction) in &tabs {
        if *interaction == Interaction::Pressed {
            *tab = button.0;
        }
    }
    for (button, mut border) in &mut tab_looks {
        let want = BorderColor::all(if button.0 == *tab { HOT } else { Color::NONE });
        if *border != want {
            *border = want;
        }
    }
    for (row, mut node) in &mut rows {
        let display = if row.0 == *tab {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    let can_edit =
        *screen == AppScreen::InGame && touch.as_deref().is_some_and(|touch| touch.enabled);
    if let Ok(mut label) = edit_label.single_mut() {
        label.set_text(if can_edit { "EDIT" } else { "IN A MATCH" });
    }
    if can_edit
        && edit
            .iter()
            .any(|interaction| *interaction == Interaction::Pressed)
    {
        open.0 = false;
        editing.begin();
    }
}

/// What the scripted `zm_menu` presses reach besides the menu: the options' page, edit
/// mode and the button settings.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScriptedOptions<'w> {
    tab: ResMut<'w, OptionsTab>,
    editing: ResMut<'w, crate::touch_controls::TouchEditing>,
    settings: ResMut<'w, GameSettings>,
}
