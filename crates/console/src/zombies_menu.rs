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
    Coop,
    Settings,
}

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
    Brightness,
    Shadows,
    FrameRate,
    Textures,
    Sensitivity,
    FieldOfView,
    Volume,
    InvertLook,
    AimAssist,
    MenuMusic,
    Layout,
    Style,
    AimMode,
    ButtonSize,
    ButtonOpacity,
}

/// The options panel's pages.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptionsTab {
    Graphics,
    #[default]
    Controls,
    Buttons,
    Audio,
}

impl OptionsTab {
    const ALL: [(OptionsTab, &'static str); 4] = [
        (OptionsTab::Graphics, "GRAPHICS"),
        (OptionsTab::Controls, "CONTROLS"),
        (OptionsTab::Buttons, "BUTTONS"),
        (OptionsTab::Audio, "AUDIO"),
    ];
}

/// The phone-only graphics choices that take effect when the app next starts: kept in
/// `Documents/iw4l-env.txt` (the file the launcher reads before graphics start, as the
/// IW4 Pocket options do). Index 0 is Auto (no line).
type Choices = [(&'static str, Option<&'static str>)];
const SHADOW_CHOICES: [(&str, Option<&str>); 3] = [("AUTO", None), ("OFF", Some("0")), ("ON", Some("1"))];
const PACE_CHOICES: [(&str, Option<&str>); 4] = [
    ("AUTO", None),
    ("30 FPS", Some("30")),
    ("40 FPS", Some("40")),
    ("60 FPS", Some("60")),
];
const TEXTURE_CHOICES: [(&str, Option<&str>); 4] = [
    ("AUTO", None),
    ("LOW", Some("256")),
    ("MEDIUM", Some("512")),
    ("HIGH", Some("1024")),
];

/// The launch-time graphics choices as the menu shows them (indexes into the tables above).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct LaunchGraphics {
    shadows: u8,
    pace: u8,
    textures: u8,
}

fn launch_env_path() -> Option<PathBuf> {
    if !cfg!(target_os = "ios") {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Documents").join("iw4l-env.txt"))
}

fn choice_index(choices: &Choices, value: Option<&str>) -> u8 {
    choices
        .iter()
        .position(|(_, env)| *env == value)
        .unwrap_or(0) as u8
}

impl LaunchGraphics {
    pub(crate) fn load() -> Self {
        let text = launch_env_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let value = |env: &str| diag::ios_env::env_file_value(&text, env);
        Self {
            shadows: choice_index(&SHADOW_CHOICES, value(diag::memory_settings::SHADOWS_ENV).as_deref()),
            pace: choice_index(&PACE_CHOICES, value(diag::frame_pacing::FRAME_PACE_ENV).as_deref()),
            textures: choice_index(
                &TEXTURE_CHOICES,
                value(diag::memory_settings::TEXTURE_CAP_ENV).as_deref(),
            ),
        }
    }

    fn save(&self) {
        let Some(path) = launch_env_path() else {
            return;
        };
        let pick = |choices: &Choices, i: u8| choices.get(usize::from(i)).and_then(|(_, env)| *env);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = diag::ios_env::merge_env_lines(
            &existing,
            &[
                (diag::memory_settings::SHADOWS_ENV, pick(&SHADOW_CHOICES, self.shadows)),
                (diag::frame_pacing::FRAME_PACE_ENV, pick(&PACE_CHOICES, self.pace)),
                (diag::memory_settings::TEXTURE_CAP_ENV, pick(&TEXTURE_CHOICES, self.textures)),
            ],
        );
        if updated != existing
            && let Err(error) = std::fs::write(&path, updated)
        {
            diag::warn!(Ui, "zombies menu: could not save graphics choices: {error}");
        }
    }
}

const LAYOUT_NAMES: [&str; 5] = ["CLASSIC", "LEFT FIRE", "COMPACT", "SOUTHPAW", "CUSTOM"];

impl Setting {
    const ALL: [Setting; 15] = [
        Setting::Brightness,
        Setting::Shadows,
        Setting::FrameRate,
        Setting::Textures,
        Setting::Sensitivity,
        Setting::FieldOfView,
        Setting::Volume,
        Setting::InvertLook,
        Setting::AimAssist,
        Setting::Layout,
        Setting::Style,
        Setting::AimMode,
        Setting::ButtonSize,
        Setting::ButtonOpacity,
        Setting::MenuMusic,
    ];

    fn tab(self) -> OptionsTab {
        match self {
            Setting::Brightness | Setting::Shadows | Setting::FrameRate | Setting::Textures => {
                OptionsTab::Graphics
            }
            Setting::Volume | Setting::MenuMusic => OptionsTab::Audio,
            Setting::Sensitivity
            | Setting::FieldOfView
            | Setting::InvertLook
            | Setting::AimAssist => OptionsTab::Controls,
            Setting::Layout
            | Setting::Style
            | Setting::AimMode
            | Setting::ButtonSize
            | Setting::ButtonOpacity => {
                OptionsTab::Buttons
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Setting::Brightness => "BRIGHTNESS",
            Setting::Shadows => "SHADOWS",
            Setting::FrameRate => "FRAME RATE",
            Setting::Textures => "TEXTURE DETAIL",
            Setting::Sensitivity => "LOOK SENSITIVITY",
            Setting::FieldOfView => "FIELD OF VIEW",
            Setting::Volume => "VOLUME",
            Setting::InvertLook => "INVERT LOOK",
            Setting::Layout => "BUTTON LAYOUT",
            Setting::Style => "BUTTON STYLE",
            Setting::AimMode => "AIM BUTTON",
            Setting::AimAssist => "AIM ASSIST",
            Setting::MenuMusic => "MENU MUSIC",
            Setting::ButtonSize => "BUTTON SIZE",
            Setting::ButtonOpacity => "BUTTON OPACITY",
        }
    }

    /// Lowest, highest and step.
    fn range(self) -> (f32, f32, f32) {
        match self {
            Setting::Brightness => (-0.2, 0.2, 0.02),
            Setting::Shadows => (0.0, 2.0, 1.0),
            Setting::FrameRate | Setting::Textures => (0.0, 3.0, 1.0),
            Setting::MenuMusic => (0.0, audio::MENU_TRACKS.len() as f32, 1.0),
            Setting::Sensitivity => (0.5, 20.0, 0.5),
            Setting::FieldOfView => (GameSettings::FOV_MIN, 110.0, 1.0),
            Setting::Volume => (0.0, 1.0, 0.05),
            Setting::InvertLook | Setting::Style | Setting::AimMode | Setting::AimAssist => {
                (0.0, 1.0, 1.0)
            }
            Setting::Layout => (0.0, f32::from(GameSettings::TOUCH_LAYOUT_CUSTOM), 1.0),
            Setting::ButtonSize => (0.75, 1.35, 0.05),
            Setting::ButtonOpacity => (0.3, 1.0, 0.05),
        }
    }

    /// Steps that go round (the last layout's + comes back to the first) and toggles.
    fn cycles(self) -> bool {
        matches!(
            self,
            Setting::InvertLook
                | Setting::Style
                | Setting::AimMode
                | Setting::AimAssist
                | Setting::Layout
                | Setting::Shadows
                | Setting::FrameRate
                | Setting::Textures
                | Setting::MenuMusic
        )
    }

    fn get(self, settings: &GameSettings, launch: &LaunchGraphics) -> f32 {
        match self {
            Setting::Brightness => settings.brightness,
            Setting::Shadows => f32::from(launch.shadows),
            Setting::FrameRate => f32::from(launch.pace),
            Setting::Textures => f32::from(launch.textures),
            Setting::MenuMusic => f32::from(settings.menu_music),
            Setting::Sensitivity => settings.sensitivity,
            Setting::FieldOfView => settings.fov,
            Setting::Volume => settings.master_volume,
            Setting::InvertLook => f32::from(u8::from(settings.invert_mouse)),
            Setting::Layout => f32::from(settings.touch_layout),
            Setting::Style => f32::from(u8::from(settings.touch_icons)),
            Setting::AimMode => f32::from(u8::from(settings.touch_aim_toggle)),
            Setting::AimAssist => f32::from(u8::from(settings.touch_aim_assist)),
            Setting::ButtonSize => settings.touch_button_scale,
            Setting::ButtonOpacity => settings.touch_opacity,
        }
    }

    fn set(self, settings: &mut GameSettings, launch: &mut LaunchGraphics, value: f32) {
        let (low, high, step) = self.range();
        let value = (((value - low) / step).round() * step + low).clamp(low, high);
        match self {
            Setting::Brightness => settings.brightness = value,
            Setting::Shadows => launch.shadows = value as u8,
            Setting::FrameRate => launch.pace = value as u8,
            Setting::Textures => launch.textures = value as u8,
            Setting::MenuMusic => settings.menu_music = value as u8,
            Setting::Sensitivity => settings.sensitivity = value,
            Setting::FieldOfView => settings.fov = value,
            Setting::Volume => settings.master_volume = value,
            Setting::InvertLook => settings.invert_mouse = value >= 0.5,
            Setting::Layout => settings.touch_layout = value as u8,
            Setting::Style => settings.touch_icons = value >= 0.5,
            Setting::AimMode => settings.touch_aim_toggle = value >= 0.5,
            Setting::AimAssist => settings.touch_aim_assist = value >= 0.5,
            Setting::ButtonSize => settings.touch_button_scale = value,
            Setting::ButtonOpacity => settings.touch_opacity = value,
        }
    }

    fn shown(self, value: f32) -> String {
        let named = |choices: &Choices| {
            choices
                .get(value.round() as usize)
                .map_or("AUTO", |(name, _)| *name)
                .to_owned()
        };
        match self {
            Setting::Brightness => format!("{:+.0}", value * 100.0),
            Setting::Shadows => named(&SHADOW_CHOICES),
            Setting::FrameRate => named(&PACE_CHOICES),
            Setting::Textures => named(&TEXTURE_CHOICES),
            Setting::MenuMusic => match value.round() as usize {
                0 => "OFF".to_owned(),
                n => audio::MENU_TRACKS
                    .get(n - 1)
                    .map_or("OFF", |(label, _)| *label)
                    .to_owned(),
            },
            Setting::Sensitivity => format!("{value:.1}"),
            Setting::FieldOfView => format!("{value:.0}"),
            Setting::InvertLook => if value >= 0.5 { "ON" } else { "OFF" }.to_owned(),
            Setting::Style => if value >= 0.5 { "ICONS" } else { "TEXT" }.to_owned(),
            Setting::AimMode => if value >= 0.5 { "TOGGLE" } else { "HOLD" }.to_owned(),
            Setting::AimAssist => if value >= 0.5 { "ON" } else { "OFF" }.to_owned(),
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

/// BO1's zombie maps as the menu lists them: the art key its loading-screen picture is
/// found by (`loadscreen_<key>`) and the name shown. The first is the map this install
/// plays; the rest show locked until their files are there.
struct MenuMap {
    art: &'static str,
    title: &'static str,
}

const LOCKED_MAPS: [MenuMap; 2] = [
    MenuMap {
        art: "zombie_pentagon",
        title: "\"FIVE\"",
    },
    MenuMap {
        art: "zombie_deadops",
        title: "DEAD OPS ARCADE",
    },
];
const MAP_COUNT: usize = 1 + LOCKED_MAPS.len();

/// A map card at the bottom right: its picture and, when locked, a dark veil over it.
#[derive(Component, Clone, Copy)]
pub(crate) struct MapCard(usize);

#[derive(Component, Clone, Copy)]
pub(crate) struct MapCardImage(usize);

/// The selected map's name under ZOMBIES.
#[derive(Component)]
pub(crate) struct MenuMapName;

/// A short line under the entries ("NOT INSTALLED", "COMING SOON").
#[derive(Component)]
pub(crate) struct MenuNotice;

/// Which map is picked, the decoded pictures, and the notice's time left.
#[derive(Resource, Default)]
pub(crate) struct MenuMaps {
    selected: usize,
    titles: Vec<String>,
    images: [Option<Handle<Image>>; MAP_COUNT],
    tasks: Vec<(usize, Task<Option<(u32, u32, Vec<u8>)>>)>,
    shown_background: Option<usize>,
    notice_left: f32,
}

/// The co-op page is open (a placeholder for setting co-op up from the menu).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoopOpen(pub bool);

#[derive(Component)]
pub(crate) struct CoopRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoopButton {
    Host,
    Join,
    Back,
}

#[derive(Component)]
pub(crate) struct CoopNote;

/// An entry in BO1's style: a left bar that lights, the label in bigFont.
fn entry(column: &mut ChildSpawnerCommands, font: &Handle<Font>, s: f32, action: MenuAction, label: &str) {
    column
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(px(12.0 * s), px(5.0 * s)),
                margin: UiRect::bottom(px(3.0 * s)),
                border: UiRect::left(px(3.0 * s)),
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
        ))
        .with_child(text(font, 25.0 * s, WHITE, label));
}

/// Builds the menu once the window and font are there: the selected map's picture
/// drifting behind a dark wash, ZOMBIES and the entries on the left, the map cards at
/// the bottom right. The maps' pictures are decoded in the background.
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
    let mut maps = MenuMaps {
        titles: std::iter::once(start.map_title.to_uppercase())
            .chain(LOCKED_MAPS.iter().map(|map| map.title.to_owned()))
            .collect(),
        ..default()
    };
    if let Some((zone, map)) = start.preview.clone() {
        let arts = std::iter::once(map).chain(LOCKED_MAPS.iter().map(|m| m.art.to_owned()));
        for (index, art) in arts.enumerate() {
            let zone = zone.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                asset_material::decode_map_preview(&zone, &art).ok().flatten()
            });
            maps.tasks.push((index, task));
        }
    }
    let play_label = if start.play_label == "SOLO" {
        "SOLO".to_owned()
    } else {
        format!("PLAY {}", start.play_label)
    };
    commands.insert_resource(maps);
    diag::info!(Ui, "zombies menu: waiting for {}", start.play_label);
    let card_w = 150.0 * s;
    let card_h = card_w * 9.0 / 16.0;
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
            // BO1's menus sit on a dark wash, darkest under the text on the left and
            // along the bottom under the cards.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundGradient(vec![
                    Gradient::Linear(LinearGradient::to_right(vec![
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.95), 0.0),
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.78), 36.0),
                        ColorStop::percent(Color::srgba(0.03, 0.0, 0.0, 0.3), 100.0),
                    ])),
                    Gradient::Linear(LinearGradient::to_top(vec![
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.85), 0.0),
                        ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 45.0),
                    ])),
                ]),
            ));
            // A thin red line along the top, as BO1's lobby frames have.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: px(2.0 * s),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.66, 0.06, 0.03, 0.85)),
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
                    row_gap: px(4.0 * s),
                    ..default()
                },
            ))
            .with_children(|column| {
                column.spawn(text(&font, 13.0 * s, GREY, "BLACK OPS"));
                column.spawn((MenuTitle, text(&font, 54.0 * s, RED, "ZOMBIES")));
                column.spawn((
                    MenuMapName,
                    text(&font, 15.0 * s, WHITE, start.map_title.to_uppercase()),
                ));
                // A thin red rule under the title, as BO1's headers have.
                column.spawn((
                    Node {
                        width: px(180.0 * s),
                        height: px(2.0 * s),
                        margin: UiRect::vertical(px(12.0 * s)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.66, 0.06, 0.03, 0.8)),
                ));
                entry(column, &font, s, MenuAction::Play, &play_label);
                entry(column, &font, s, MenuAction::Coop, "CO-OP");
                entry(column, &font, s, MenuAction::Settings, "SETTINGS");
                column.spawn((MenuNotice, text(&font, 13.0 * s, HOT, "")));
            });
            // The map cards, bottom right, under SELECT MAP.
            root.spawn((
                MenuColumn,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(EDGE),
                    bottom: px(18.0 * s),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6.0 * s),
                    ..default()
                },
            ))
            .with_children(|cards| {
                cards.spawn(text(&font, 13.0 * s, GREY, "SELECT MAP"));
                cards
                    .spawn(Node {
                        column_gap: px(10.0 * s),
                        ..default()
                    })
                    .with_children(|row| {
                        for index in 0..MAP_COUNT {
                            let locked = index > 0;
                            row.spawn((
                                MapCard(index),
                                Button,
                                Node {
                                    width: px(card_w),
                                    height: px(card_h),
                                    border: UiRect::all(px(2.0 * s)),
                                    overflow: Overflow::clip(),
                                    ..default()
                                },
                                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                                BackgroundColor(Color::srgb(0.06, 0.05, 0.05)),
                            ))
                            .with_children(|card| {
                                // The square picture across the card's width, cropped
                                // top and bottom.
                                card.spawn((
                                    MapCardImage(index),
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(0),
                                        top: px(-(card_w - card_h) * 0.5),
                                        width: px(card_w),
                                        height: px(card_w),
                                        ..default()
                                    },
                                    FocusPolicy::Pass,
                                ));
                                if locked {
                                    card.spawn((
                                        Node {
                                            position_type: PositionType::Absolute,
                                            width: percent(100),
                                            height: percent(100),
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                                        FocusPolicy::Pass,
                                    ))
                                    .with_child(text(&font, 11.0 * s, GREY, "NOT INSTALLED"));
                                }
                                // The name on a dark strip along the bottom.
                                card.spawn((
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(0),
                                        bottom: px(0),
                                        width: percent(100),
                                        padding: UiRect::axes(px(6.0 * s), px(2.0 * s)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
                                    FocusPolicy::Pass,
                                ))
                                .with_child(text(
                                    &font,
                                    11.0 * s,
                                    if locked { GREY } else { WHITE },
                                    if index == 0 {
                                        start.map_title.to_uppercase()
                                    } else {
                                        LOCKED_MAPS[index - 1].title.to_owned()
                                    },
                                ));
                            });
                        }
                    });
            });
        });
}

fn image_from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Image {
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
    image
}

/// The pictures as they decode; the background follows the selected map's, drifting
/// slowly; the title flickers like old film; the notice fades out.
#[allow(clippy::too_many_arguments)]
pub(crate) fn animate_zombies_menu(
    mut commands: Commands,
    time: Res<Time>,
    maps: Option<ResMut<MenuMaps>>,
    mut images: ResMut<Assets<Image>>,
    mut background: Query<(Entity, &mut Node), With<MenuBackground>>,
    card_images: Query<(Entity, &MapCardImage)>,
    mut title: Query<&mut GameText, (With<MenuTitle>, Without<MenuNotice>)>,
    mut notice: Query<&mut GameText, (With<MenuNotice>, Without<MenuTitle>)>,
    mut column: Query<&mut Visibility, With<MenuColumn>>,
    options: Res<ZombiesOptionsOpen>,
    coop: Res<CoopOpen>,
) {
    for mut visibility in &mut column {
        visibility.set_if_neq(if options.0 || coop.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    let Some(mut maps) = maps else {
        return;
    };
    let mut decoded = Vec::new();
    maps.tasks.retain_mut(|(index, task)| {
        match future::block_on(future::poll_once(task)) {
            Some(result) => {
                decoded.push((*index, result));
                false
            }
            None => true,
        }
    });
    for (index, result) in decoded {
        let Some((width, height, pixels)) = result else {
            diag::info!(Ui, "zombies menu: no picture for map card {index}");
            continue;
        };
        let handle = images.add(image_from_rgba(width, height, pixels));
        for (entity, card) in &card_images {
            if card.0 == index {
                commands.entity(entity).insert(ImageNode::new(handle.clone()));
            }
        }
        maps.images[index] = Some(handle);
        diag::info!(Ui, "zombies menu: map card {index} picture {width}x{height}");
    }
    let selected = maps.selected;
    if maps.shown_background != Some(selected)
        && let Some(handle) = maps.images[selected].clone()
        && let Ok((entity, _)) = background.single()
    {
        commands.entity(entity).insert(ImageNode::new(handle));
        maps.shown_background = Some(selected);
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
    if maps.notice_left > 0.0 {
        maps.notice_left -= time.delta_secs();
        if let Ok(mut notice) = notice.single_mut() {
            let alpha = maps.notice_left.clamp(0.0, 1.0);
            notice.set_color(HOT.with_alpha(alpha));
        }
    }
}

/// A card picks its map: the name under ZOMBIES and the background follow, and the
/// selected card's frame turns red.
pub(crate) fn zombies_menu_cards(
    maps: Option<ResMut<MenuMaps>>,
    cards: Query<(&MapCard, &Interaction), Changed<Interaction>>,
    mut frames: Query<(&MapCard, &mut BorderColor)>,
    mut name: Query<&mut GameText, With<MenuMapName>>,
) {
    let Some(mut maps) = maps else {
        return;
    };
    for (card, interaction) in &cards {
        if *interaction == Interaction::Pressed && maps.selected != card.0 {
            maps.selected = card.0;
            if let Ok(mut name) = name.single_mut()
                && let Some(title) = maps.titles.get(card.0)
            {
                name.set_text(title);
            }
        }
    }
    for (card, mut border) in &mut frames {
        let want = BorderColor::all(if card.0 == maps.selected {
            RED
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.25)
        });
        if *border != want {
            *border = want;
        }
    }
}

/// SOLO starts the selected map's load (the launch's own request, loading screen and
/// preview), or says the map is not installed; CO-OP and SETTINGS open their pages. A
/// pressed entry lights up as BO1's do.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn zombies_menu_buttons(
    mut commands: Commands,
    mut start: Option<ResMut<ZombiesMenuStart>>,
    mut maps: Option<ResMut<MenuMaps>>,
    mut screen: ResMut<AppScreen>,
    mut options: ResMut<ZombiesOptionsOpen>,
    mut coop: ResMut<CoopOpen>,
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
    mut labels: Query<&mut GameText, Without<MenuNotice>>,
    mut notice: Query<&mut GameText, With<MenuNotice>>,
    roots: Query<Entity, With<MenuRoot>>,
    mut scripted: ResMut<MenuCommand>,
    mut tests: ScriptedOptions,
) {
    let command = scripted.0.take();
    let mut words = command.as_deref().unwrap_or("").split_whitespace();
    let mut pressed = match words.next() {
        Some("play") => Some(MenuAction::Play),
        Some("options" | "settings") => Some(MenuAction::Settings),
        Some("coop") => Some(MenuAction::Coop),
        Some("map") => {
            if let (Some(maps), Some(index)) = (
                maps.as_mut(),
                words.next().and_then(|n| n.parse::<usize>().ok()),
            ) {
                maps.selected = index.min(MAP_COUNT - 1);
            }
            None
        }
        Some("done") => {
            options.0 = false;
            coop.0 = false;
            None
        }
        Some("tab_buttons") => {
            *tests.tab = OptionsTab::Buttons;
            None
        }
        Some("tab_gameplay" | "tab_controls") => {
            *tests.tab = OptionsTab::Controls;
            None
        }
        Some("tab_graphics") => {
            *tests.tab = OptionsTab::Graphics;
            None
        }
        Some("tab_audio") => {
            *tests.tab = OptionsTab::Audio;
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
        Some("music") => {
            if let Some(track) = words.next().and_then(|n| n.parse::<u8>().ok()) {
                tests.settings.menu_music = track;
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
        if *interaction == Interaction::Pressed && !options.0 && !coop.0 {
            pressed = Some(*action);
        }
    }
    let mut say = |line: &str, maps: &mut Option<ResMut<MenuMaps>>| {
        if let Ok(mut notice) = notice.single_mut() {
            notice.set_text(line);
            notice.set_color(HOT);
        }
        if let Some(maps) = maps.as_mut() {
            maps.notice_left = 3.0;
        }
    };
    match pressed {
        Some(MenuAction::Settings) => options.0 = true,
        Some(MenuAction::Coop) => coop.0 = true,
        Some(MenuAction::Play) => {
            if maps.as_ref().is_some_and(|maps| maps.selected != 0) {
                say("NOT INSTALLED: COPY THIS MAP'S FILES TO PLAY IT", &mut maps);
                return;
            }
            let Some(load) = start.as_mut().and_then(|start| start.load.take()) else {
                return;
            };
            diag::info!(Ui, "zombies menu: starting {}", load.request.zone);
            options.0 = false;
            coop.0 = false;
            commands.insert_resource(load.request);
            commands.insert_resource(load.loading);
            if let Some(preview) = load.preview {
                commands.insert_resource(preview);
            }
            commands.remove_resource::<ZombiesMenuStart>();
            commands.remove_resource::<MenuMaps>();
            *screen = AppScreen::Loading;
            for root in &roots {
                commands.entity(root).try_despawn();
            }
        }
        None => {}
    }
}

/// The co-op page: a placeholder in BO1's lobby look (four player slots, HOST and JOIN)
/// until co-op is set up from here; the buttons say it's coming.
pub(crate) fn show_coop_page(
    mut commands: Commands,
    open: Res<CoopOpen>,
    font: Option<Res<GameUiFont>>,
    settings: Res<GameSettings>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<CoopRoot>>,
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
    let you = if settings.player_name.trim().is_empty() {
        "YOU".to_owned()
    } else {
        settings.player_name.trim().to_uppercase()
    };
    let button = |parent: &mut ChildSpawnerCommands, which: CoopButton, label: &str| {
        parent
            .spawn((
                which,
                Button,
                Node {
                    padding: UiRect::axes(px(22.0 * s), px(6.0 * s)),
                    border: UiRect::all(px(1.0)),
                    ..default()
                },
                BorderColor::all(HOT),
                BackgroundColor(Color::srgba(1.0, 0.66, 0.18, 0.14)),
            ))
            .with_child(text(&font, 18.0 * s, WHITE, label));
    };
    commands
        .spawn((
            CoopRoot,
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
                    width: percent(80),
                    max_width: px(600.0 * s),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(18.0 * s), px(12.0 * s)),
                    border: UiRect::top(px(3.0 * s)),
                    row_gap: px(6.0 * s),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(RED),
            ))
            .with_children(|panel| {
                panel.spawn(text(&font, 22.0 * s, WHITE, "CO-OP"));
                panel.spawn(text(
                    &font,
                    13.0 * s,
                    GREY,
                    "UP TO 4 PLAYERS ON THE SAME WI-FI",
                ));
                // The lobby's four player slots.
                for slot in 0..4 {
                    panel
                        .spawn((
                            Node {
                                height: px(26.0 * s),
                                align_items: AlignItems::Center,
                                padding: UiRect::horizontal(px(10.0 * s)),
                                border: UiRect::left(px(3.0 * s)),
                                column_gap: px(10.0 * s),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
                            BorderColor::all(if slot == 0 { RED } else { Color::NONE }),
                        ))
                        .with_children(|row| {
                            row.spawn(text(&font, 13.0 * s, GREY, format!("{}", slot + 1)));
                            if slot == 0 {
                                row.spawn(text(&font, 15.0 * s, WHITE, you.clone()));
                            } else {
                                row.spawn(text(&font, 15.0 * s, GREY, "WAITING FOR PLAYER"));
                            }
                        });
                }
                panel.spawn((CoopNote, text(&font, 13.0 * s, HOT, "")));
                panel
                    .spawn(Node {
                        column_gap: px(10.0 * s),
                        justify_content: JustifyContent::FlexEnd,
                        margin: UiRect::top(px(4.0 * s)),
                        ..default()
                    })
                    .with_children(|bottom| {
                        button(bottom, CoopButton::Host, "HOST");
                        button(bottom, CoopButton::Join, "JOIN");
                        button(bottom, CoopButton::Back, "BACK");
                    });
            });
        });
}

pub(crate) fn coop_page_input(
    mut open: ResMut<CoopOpen>,
    buttons: Query<(&CoopButton, &Interaction), Changed<Interaction>>,
    mut note: Query<&mut GameText, With<CoopNote>>,
) {
    if !open.0 {
        return;
    }
    for (button, interaction) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            CoopButton::Back => open.0 = false,
            CoopButton::Host | CoopButton::Join => {
                if let Ok(mut note) = note.single_mut() {
                    note.set_text("COMING SOON: CO-OP FROM THE MENU IS NOT READY YET");
                }
            }
        }
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
                            for (tab, label) in OptionsTab::ALL {
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
                    .spawn((
                        OptionRow(OptionsTab::Graphics),
                        Node {
                            height: px(row_height * 0.8),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_child(text(
                        &font,
                        12.0 * s,
                        GREY,
                        "SHADOWS, FRAME RATE AND TEXTURES APPLY THE NEXT TIME YOU OPEN THE APP",
                    ));
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
    mut launch: Local<Option<LaunchGraphics>>,
    steps: Query<(&OptionStep, &Interaction), Changed<Interaction>>,
    bars: Query<(&OptionBar, &Interaction, &RelativeCursorPosition)>,
    done: Query<&Interaction, (With<OptionsDone>, Changed<Interaction>)>,
    mut fills: Query<(&OptionFill, &mut Node)>,
    mut values: Query<(&OptionValue, &mut GameText)>,
) {
    if !open.0 {
        return;
    }
    let launch = launch.get_or_insert_with(LaunchGraphics::load);
    let before = *launch;
    let mut changed = false;
    for (step, interaction) in &steps {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let OptionStep(setting, sign) = *step;
        let (low, high, size) = setting.range();
        let now = setting.get(&settings, launch);
        // Toggles flip; the layouts go round; the rest step and stop at their ends.
        let next = if setting.cycles() {
            let count = ((high - low) / size).round() + 1.0;
            low + ((now - low) / size + sign).rem_euclid(count) * size
        } else {
            now + sign * size
        };
        setting.set(&mut settings, launch, next);
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
        if (setting.get(&settings, launch) - value).abs() > f32::EPSILON {
            setting.set(&mut settings, launch, value);
            changed = true;
        }
    }
    if changed {
        settings.sanitize();
        settings.touch();
    }
    if *launch != before {
        launch.save();
    }
    if done
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        open.0 = false;
    }
    for (fill, mut node) in &mut fills {
        let want = percent(100.0 * fill.0.share(fill.0.get(&settings, launch)));
        if node.width != want {
            node.width = want;
        }
    }
    for (value, mut text) in &mut values {
        text.set_text(&value.0.shown(value.0.get(&settings, launch)));
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

/// The saved MENU MUSIC choice drives the track the BO1 main menu loops.
pub(crate) fn sync_menu_music(
    settings: Res<GameSettings>,
    choice: Option<ResMut<audio::MenuMusicChoice>>,
) {
    if let Some(mut choice) = choice
        && choice.0 != settings.menu_music
    {
        choice.0 = settings.menu_music;
    }
}

/// Black Ops' solo pause: while the options panel (the pause menu) or the touch buttons'
/// edit mode is up in a solo match, the game's clock stops, so zombies wait. A hosted
/// co-op room keeps running, as Black Ops' co-op does.
pub(crate) fn pause_solo_match(
    open: Res<ZombiesOptionsOpen>,
    editing: Res<crate::touch_controls::TouchEditing>,
    screen: Res<AppScreen>,
    role: Option<Res<frame::RuntimeRole>>,
    hub: Option<Res<net::UdpAuthorityHub>>,
    mut time: ResMut<Time<Virtual>>,
    mut paused_here: Local<bool>,
) {
    let solo = role.is_some_and(|role| *role == frame::RuntimeRole::Listen) && hub.is_none();
    let want = solo && *screen == AppScreen::InGame && (open.0 || editing.active);
    if want && !*paused_here {
        time.pause();
        *paused_here = true;
        diag::info!(Ui, "zombies menu: solo match paused");
    } else if !want && *paused_here {
        time.unpause();
        *paused_here = false;
        diag::info!(Ui, "zombies menu: solo match resumed");
    }
}
