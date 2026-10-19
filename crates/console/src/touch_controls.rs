//! On-screen touch controls for phones. A finger put down on the stick side of the screen
//! becomes a floating stick that moves the player; dragging anywhere else turns the view;
//! buttons press gameplay inputs. Holding fire or aim and dragging the same finger also
//! turns, so you can aim while shooting.
//!
//! The buttons follow the chosen layout (CLASSIC, LEFT FIRE with a second fire button by
//! the stick, COMPACT, SOUTHPAW mirrored, or CUSTOM), draw icons or text, and take the
//! options' size and opacity. In a match the options' EDIT BUTTONS turns on edit mode:
//! drag a button to move it, RESET or DONE; DONE saves the positions as the CUSTOM layout.
//!
//! The layer is on by default on iOS and off elsewhere; `IW4L_TOUCH=1` turns it on (and
//! `IW4L_TOUCH=0` off) on any platform. It owns touches only in a match with no menu or
//! console open, and only then does the iOS touch-as-mouse bridge stand aside.
//!
//! `touch <id> <down|move|up> <x> <y>` injects a touch for scripted tests.

use std::collections::{BTreeMap, BTreeSet};

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;
use frame::{AppScreen, GameSettings};
use ui::UiLayer;

use crate::game_text::{GameFont, GameText, GameTextMut};
use crate::{ConsoleCommand, ConsoleRegistry};
use crate::{ConsoleSettings, ConsoleState};

pub const TOUCH_ENV: &str = "IW4L_TOUCH";

/// Room kept free at the left and right edges for the notch / Dynamic Island in landscape.
const SAFE_EDGE: f32 = 48.0;
/// The stick lives in this share of the screen's width (on its side), below `STICK_TOP`.
const STICK_ZONE_WIDTH: f32 = 0.42;
const STICK_TOP: f32 = 0.18;
const STICK_RADIUS: f32 = 64.0;
const STICK_KNOB_RADIUS: f32 = 26.0;
const STICK_DEADZONE: f32 = 0.12;
/// Pushing past the ring by this much, mostly forward, sprints.
const SPRINT_REACH: f32 = 1.3;
const SPRINT_FORWARD: f32 = 0.5;
/// Mouse counts per dragged logical pixel; the mouse path then applies sensitivity,
/// FOV and ADS scaling.
const LOOK_SCALE: f32 = 1.0;
/// Touch buttons press inputs on key numbers no keyboard, mouse or pad uses (those stop
/// at 205), one per button, so each keeps its own down state.
const FIRST_TOUCH_KEYNUM: i32 = 206;

#[derive(Clone, Copy, Debug)]
struct ButtonSpec {
    /// Its text, and its key in the saved custom layout.
    label: &'static str,
    key: &'static str,
    command: &'static str,
    icon: &'static [u8],
    /// A finger that pressed this button also turns the view when it drags.
    looks: bool,
}

const ICON_FIRE: &[u8] = include_bytes!("../assets/touch/fire.png");

#[rustfmt::skip]
const BUTTONS: [ButtonSpec; 10] = [
    ButtonSpec { label: "FIRE", key: "fire", command: "+attack", icon: ICON_FIRE, looks: true },
    ButtonSpec { label: "AIM", key: "aim", command: "+speed_throw", icon: include_bytes!("../assets/touch/aim.png"), looks: true },
    ButtonSpec { label: "RELOAD", key: "reload", command: "+reload", icon: include_bytes!("../assets/touch/reload.png"), looks: false },
    ButtonSpec { label: "USE", key: "use", command: "+activate", icon: include_bytes!("../assets/touch/use.png"), looks: false },
    ButtonSpec { label: "JUMP", key: "jump", command: "+gostand", icon: include_bytes!("../assets/touch/jump.png"), looks: false },
    ButtonSpec { label: "CROUCH", key: "crouch", command: "togglecrouch", icon: include_bytes!("../assets/touch/crouch.png"), looks: false },
    ButtonSpec { label: "KNIFE", key: "knife", command: "+melee", icon: include_bytes!("../assets/touch/knife.png"), looks: false },
    ButtonSpec { label: "NADE", key: "nade", command: "+frag", icon: include_bytes!("../assets/touch/nade.png"), looks: false },
    ButtonSpec { label: "SWAP", key: "swap", command: "weapnext", icon: include_bytes!("../assets/touch/swap.png"), looks: false },
    // LEFT FIRE's second fire button, by the stick.
    ButtonSpec { label: "FIRE", key: "fire2", command: "+attack", icon: ICON_FIRE, looks: true },
];
const BUTTON_COUNT: usize = BUTTONS.len();
const SPRINT_COMMAND: &str = "+breath_sprint";
const SPRINT_KEYNUM: i32 = FIRST_TOUCH_KEYNUM + BUTTON_COUNT as i32;

/// Where a layout puts a button: its centre in logical pixels from the right edge (after
/// `SAFE_EDGE`), or from the left edge when `left`, and from the bottom; and its radius.
#[derive(Clone, Copy, Debug)]
struct Seat {
    x: f32,
    bottom: f32,
    radius: f32,
    left: bool,
}

const fn seat(x: f32, bottom: f32, radius: f32) -> Option<Seat> {
    Some(Seat { x, bottom, radius, left: false })
}

/// The layout the game shipped with; the others are made from it.
#[rustfmt::skip]
const CLASSIC: [Option<Seat>; BUTTON_COUNT] = [
    seat(130.0, 125.0, 48.0), // FIRE
    seat(240.0, 70.0, 34.0),  // AIM
    seat(245.0, 168.0, 28.0), // RELOAD
    seat(130.0, 238.0, 30.0), // USE
    seat(40.0, 200.0, 30.0),  // JUMP
    seat(40.0, 110.0, 28.0),  // CROUCH
    seat(325.0, 60.0, 26.0),  // KNIFE
    seat(330.0, 150.0, 26.0), // NADE
    None,                     // SWAP: a tap on the weapon readout instead
    None,                     // second FIRE
];

/// A layout's seats: LEFT FIRE adds a fire button above the stick, COMPACT pulls them in
/// and shrinks them, SOUTHPAW mirrors them (the stick goes right).
fn preset(layout: u8) -> [Option<Seat>; BUTTON_COUNT] {
    let mut seats = CLASSIC;
    match layout {
        GameSettings::TOUCH_LAYOUT_LEFT_FIRE => {
            seats[9] = Some(Seat {
                x: 70.0,
                bottom: 245.0,
                radius: 36.0,
                left: true,
            });
        }
        GameSettings::TOUCH_LAYOUT_COMPACT => {
            for seat in seats.iter_mut().flatten() {
                seat.x = 20.0 + (seat.x - 20.0) * 0.82;
                seat.bottom = 22.0 + (seat.bottom - 22.0) * 0.82;
                seat.radius *= 0.88;
            }
        }
        GameSettings::TOUCH_LAYOUT_SOUTHPAW => {
            for seat in seats.iter_mut().flatten() {
                seat.left = !seat.left;
            }
        }
        _ => {}
    }
    seats
}

/// A button as drawn and hit this frame: its centre (logical pixels) and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    centre: Vec2,
    radius: f32,
}

/// The buttons' size as set (BUTTON SIZE), held down so a layout's highest button still
/// fits under the top of the screen.
/// How tall the layouts are fitted as (logical points up from the bottom): where the
/// SWAP button's top was, so the buttons keep their size now that SWAP is a tap on the
/// weapon readout.
const LAYOUT_TOP: f32 = 316.0;

fn button_scale(wanted: f32, size: Vec2, seats: &[Option<Seat>]) -> f32 {
    let tallest = seats
        .iter()
        .flatten()
        .map(|seat| seat.bottom + seat.radius)
        .fold(LAYOUT_TOP, f32::max);
    wanted.min((size.y - 8.0) / tallest).max(0.5)
}

fn place_seat(seat: &Seat, size: Vec2, scale: f32) -> Placed {
    let x = if seat.left {
        SAFE_EDGE + seat.x * scale
    } else {
        size.x - SAFE_EDGE - seat.x * scale
    };
    Placed {
        centre: Vec2::new(x, size.y - seat.bottom * scale),
        radius: seat.radius * scale,
    }
}

/// The saved custom layout: each button's centre as shares of the screen.
fn parse_custom(saved: &str) -> [Option<Vec2>; BUTTON_COUNT] {
    let mut at = [None; BUTTON_COUNT];
    for entry in saved.split(';') {
        let Some((key, xy)) = entry.split_once(':') else {
            continue;
        };
        let Some((x, y)) = xy.split_once(',') else {
            continue;
        };
        let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) else {
            continue;
        };
        if let Some(index) = BUTTONS.iter().position(|spec| spec.key == key.trim()) {
            at[index] = Some(Vec2::new(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
        }
    }
    at
}

fn format_custom(at: &[Option<Vec2>; BUTTON_COUNT]) -> String {
    at.iter()
        .zip(BUTTONS.iter())
        .filter_map(|(at, spec)| at.map(|at| format!("{}:{:.4},{:.4}", spec.key, at.x, at.y)))
        .collect::<Vec<_>>()
        .join(";")
}

/// Where every button of the chosen layout is this frame (None: not in the layout).
/// `editing` overrides the positions while the player drags them about.
fn placements(
    settings: &GameSettings,
    size: Vec2,
    editing: Option<&[Option<Vec2>; BUTTON_COUNT]>,
) -> [Option<Placed>; BUTTON_COUNT] {
    let custom = settings.touch_layout == GameSettings::TOUCH_LAYOUT_CUSTOM;
    let seats = if custom {
        CLASSIC
    } else {
        preset(settings.touch_layout)
    };
    let scale = button_scale(settings.touch_button_scale, size, &seats);
    let saved = editing
        .copied()
        .or_else(|| custom.then(|| parse_custom(&settings.touch_custom)));
    std::array::from_fn(|index| {
        let seat = seats[index].as_ref();
        match saved.as_ref().and_then(|saved| saved[index]) {
            Some(at) => {
                let radius = CLASSIC[index].or(preset(GameSettings::TOUCH_LAYOUT_LEFT_FIRE)[index]);
                radius.map(|seat| Placed {
                    centre: Vec2::new(at.x * size.x, at.y * size.y),
                    radius: seat.radius * scale,
                })
            }
            None if saved.is_some() && editing.is_some() => None,
            None => seat.map(|seat| place_seat(seat, size, scale)),
        }
    })
}

fn button_at(pos: Vec2, placed: &[Option<Placed>; BUTTON_COUNT]) -> Option<usize> {
    // A little grace around each button: fingers are bigger than the drawing. The
    // nearest wins where two graces meet.
    placed
        .iter()
        .enumerate()
        .filter_map(|(index, placed)| placed.map(|placed| (index, placed)))
        .filter(|(_, placed)| pos.distance(placed.centre) <= placed.radius + 8.0)
        .min_by(|a, b| {
            pos.distance(a.1.centre)
                .total_cmp(&pos.distance(b.1.centre))
        })
        .map(|(index, _)| index)
}

/// The pause button (top left) that opens the options: centre and radius.
const MENU_BUTTON: (f32, f32, f32) = (SAFE_EDGE + 26.0, 30.0, 20.0);

fn on_menu_button(pos: Vec2) -> bool {
    let (x, y, radius) = MENU_BUTTON;
    pos.distance(Vec2::new(x, y)) <= radius + 10.0
}

/// The stick's side: the left, or the right for SOUTHPAW.
fn stick_on_right(settings: &GameSettings) -> bool {
    settings.touch_layout == GameSettings::TOUCH_LAYOUT_SOUTHPAW
}

fn in_stick_zone(pos: Vec2, size: Vec2, right: bool) -> bool {
    let x = if right { size.x - pos.x } else { pos.x };
    x >= SAFE_EDGE * 0.5 && x < size.x * STICK_ZONE_WIDTH && pos.y > size.y * STICK_TOP
}

/// Stick displacement → (forward, right) with a radial deadzone, and whether it sprints.
fn stick_axes(stick: &Stick) -> ([f32; 2], bool) {
    let delta = stick.at - stick.origin;
    let reach = delta.length() / STICK_RADIUS;
    if reach <= STICK_DEADZONE {
        return ([0.0, 0.0], false);
    }
    let scaled = ((reach.min(1.0) - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).clamp(0.0, 1.0);
    let dir = delta / delta.length();
    let forward = -dir.y * scaled;
    let right = dir.x * scaled;
    let sprint = reach >= SPRINT_REACH && -dir.y >= SPRINT_FORWARD;
    ([forward, right], sprint)
}

/// Whether the layer is on, and whether it owns touches this frame.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TouchGameplay {
    pub enabled: bool,
    pub owns_touches: bool,
}

/// What the touch layer asks of this frame's usercmd.
#[derive(Resource, Debug, Default)]
pub(crate) struct TouchFrame {
    /// Forward, right; each in -1..=1.
    pub move_axes: [f32; 2],
    /// Logical pixels dragged this frame by looking fingers.
    pub look_px: [f32; 2],
    /// Inputs held now, as (command id, key number).
    pub held: BTreeSet<(u32, i32)>,
}

#[derive(Clone, Copy, Debug)]
struct Stick {
    id: u64,
    origin: Vec2,
    at: Vec2,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct TouchState {
    stick: Option<Stick>,
    /// Last position of every finger that turns the view.
    looks: BTreeMap<u64, Vec2>,
    /// Button index held by each finger.
    buttons: BTreeMap<u64, usize>,
    /// AIM in toggle mode: aiming until AIM is pressed again.
    aim_latched: bool,
    /// BO1's use prompt is up: the USE button shows.
    use_available: bool,
}

impl TouchState {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn button_held(&self, index: usize) -> bool {
        self.buttons.values().any(|&b| b == index)
            || (self.aim_latched && BUTTONS[index].key == AIM_KEY)
    }
}

/// The AIM button's key in `BUTTONS`.
const AIM_KEY: &str = "aim";
const USE_KEY: &str = "use";
const SWAP_KEY: &str = "swap";

/// Where the weapon name and ammo are drawn (logical pixels): a tap there switches guns
/// (SWAP has no button of its own). Set by the zombies HUD as it places the readout.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct WeaponTapZone(pub Option<Rect>);

/// USE shows only while there is something to use (BO1's use prompt is up).
fn use_available(hint: Option<&hud::UseHintText>) -> bool {
    hint.is_some_and(|hint| hint.text.is_some() && hint.alpha > 0.0)
}

fn index_of(key: &str) -> usize {
    BUTTONS.iter().position(|b| b.key == key).expect("a touch button key")
}

/// Edit mode (EDIT BUTTONS in the options, in a match): the buttons' positions while the
/// player drags them, as shares of the screen, and the button a finger holds.
#[derive(Resource, Debug, Default)]
pub struct TouchEditing {
    pub active: bool,
    at: Option<[Option<Vec2>; BUTTON_COUNT]>,
    /// (finger, button, the finger's offset from the button's centre).
    grab: Option<(u64, usize, Vec2)>,
}

impl TouchEditing {
    /// Edit mode on, starting from where the buttons are.
    pub fn begin(&mut self) {
        *self = Self {
            active: true,
            ..Self::default()
        };
    }
}

/// The edit banner's two pills, RESET and DONE: centre and half size.
fn edit_pills(size: Vec2) -> [(Vec2, Vec2); 2] {
    let half = Vec2::new(58.0, 18.0);
    [
        (Vec2::new(size.x * 0.5 - 70.0, 74.0), half),
        (Vec2::new(size.x * 0.5 + 70.0, 74.0), half),
    ]
}

fn on_pill(pos: Vec2, pill: (Vec2, Vec2)) -> bool {
    let d = (pos - pill.0).abs();
    d.x <= pill.1.x + 6.0 && d.y <= pill.1.y + 6.0
}

fn enabled_from_env() -> bool {
    match std::env::var(TOUCH_ENV).ok().as_deref().map(str::trim) {
        Some("1" | "on" | "true") => true,
        Some("0" | "off" | "false") => false,
        _ => cfg!(target_os = "ios"),
    }
}

pub(crate) fn update_touch_ownership(
    screen: Option<Res<AppScreen>>,
    console: Res<ConsoleState>,
    menus: Option<Res<hud::ScriptMenus>>,
    options: Option<Res<crate::zombies_menu::ZombiesOptionsOpen>>,
    mut editing: ResMut<TouchEditing>,
    mut gameplay: ResMut<TouchGameplay>,
) {
    let in_game = screen.as_ref().is_some_and(|s| **s == AppScreen::InGame);
    let menu =
        menus.is_some_and(|m| m.captures_input()) || options.is_some_and(|options| options.0);
    let owns = gameplay.enabled && in_game && !console.open && !menu;
    if !in_game && editing.active {
        *editing = TouchEditing::default();
    }
    if owns != gameplay.owns_touches && gameplay.enabled {
        diag::info!(
            Console,
            "touch controls {}: screen={:?} console_open={} menu_captures={menu}",
            if owns { "active" } else { "paused" },
            screen.as_deref(),
            console.open
        );
    }
    gameplay.owns_touches = owns;
}

/// Edit mode's touches: a finger on a button carries it; RESET puts the CLASSIC layout
/// back; DONE saves the positions as the CUSTOM layout and ends edit mode.
fn read_touch_edit(
    touches: &mut MessageReader<TouchInput>,
    size: Vec2,
    settings: &mut GameSettings,
    editing: &mut TouchEditing,
) {
    let mut at = editing.at.unwrap_or_else(|| {
        // Start from where the buttons are now.
        placements(settings, size, None).map(|placed| {
            placed.map(|placed| Vec2::new(placed.centre.x / size.x, placed.centre.y / size.y))
        })
    });
    for touch in touches.read() {
        let id = touch.id;
        match touch.phase {
            TouchPhase::Started => {
                let [reset, done] = edit_pills(size);
                if on_pill(touch.position, done) {
                    settings.touch_custom = format_custom(&at);
                    settings.touch_layout = GameSettings::TOUCH_LAYOUT_CUSTOM;
                    settings.touch();
                    diag::info!(Console, "touch: custom layout saved: {}", settings.touch_custom);
                    *editing = TouchEditing::default();
                    return;
                }
                if on_pill(touch.position, reset) {
                    let classic = GameSettings {
                        touch_layout: GameSettings::TOUCH_LAYOUT_CLASSIC,
                        ..settings.clone()
                    };
                    at = placements(&classic, size, None).map(|placed| {
                        placed.map(|p| Vec2::new(p.centre.x / size.x, p.centre.y / size.y))
                    });
                    continue;
                }
                let placed = placements(settings, size, Some(&at));
                if let Some(index) = button_at(touch.position, &placed)
                    && let Some(placed) = placed[index]
                {
                    editing.grab = Some((id, index, touch.position - placed.centre));
                }
            }
            TouchPhase::Moved => {
                if let Some((finger, index, offset)) = editing.grab
                    && finger == id
                {
                    let centre = (touch.position - offset).clamp(Vec2::splat(24.0), size - 24.0);
                    at[index] = Some(Vec2::new(centre.x / size.x, centre.y / size.y));
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if editing.grab.is_some_and(|(finger, ..)| finger == id) {
                    editing.grab = None;
                }
            }
        }
    }
    editing.at = Some(at);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_touch_controls(
    mut touches: MessageReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    gameplay: Res<TouchGameplay>,
    mut settings: ResMut<GameSettings>,
    mut options: ResMut<crate::zombies_menu::ZombiesOptionsOpen>,
    mut editing: ResMut<TouchEditing>,
    mut state: ResMut<TouchState>,
    mut frame: ResMut<TouchFrame>,
    hint: Option<Res<hud::UseHintText>>,
    weapon_tap: Res<WeaponTapZone>,
) {
    frame.look_px = [0.0; 2];
    if !gameplay.owns_touches {
        touches.clear();
        state.clear();
        frame.move_axes = [0.0; 2];
        frame.held.clear();
        return;
    }
    let Ok(window) = windows.single() else {
        touches.clear();
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    if editing.active {
        state.clear();
        frame.move_axes = [0.0; 2];
        frame.held.clear();
        read_touch_edit(&mut touches, size, &mut settings, &mut editing);
        return;
    }
    let mut placed = placements(&settings, size, None);
    state.use_available = use_available(hint.as_deref());
    if !state.use_available {
        placed[index_of(USE_KEY)] = None;
    }
    let stick_right = stick_on_right(&settings);
    let mut look = Vec2::ZERO;
    for touch in touches.read() {
        let id = touch.id;
        match touch.phase {
            TouchPhase::Started => {
                if on_menu_button(touch.position) {
                    // The options take the screen; this layer lets go of every finger.
                    options.0 = true;
                    state.clear();
                    frame.move_axes = [0.0; 2];
                    frame.held.clear();
                    return;
                }
                let on_weapon = weapon_tap.0.is_some_and(|zone| zone.contains(touch.position));
                if let Some(index) = button_at(touch.position, &placed)
                    .or(on_weapon.then(|| index_of(SWAP_KEY)))
                {
                    if std::env::var_os("IW4L_TOUCH_DEBUG").is_some() {
                        diag::info!(Console, "touch: finger {id} on {}", BUTTONS[index].label);
                    }
                    state.buttons.insert(id, index);
                    if BUTTONS[index].key == AIM_KEY && settings.touch_aim_toggle {
                        state.aim_latched = !state.aim_latched;
                    }
                    if BUTTONS[index].looks {
                        state.looks.insert(id, touch.position);
                    }
                } else if state.stick.is_none() && in_stick_zone(touch.position, size, stick_right)
                {
                    state.stick = Some(Stick {
                        id,
                        origin: touch.position,
                        at: touch.position,
                    });
                } else {
                    state.looks.insert(id, touch.position);
                }
            }
            TouchPhase::Moved => {
                if let Some(stick) = state.stick.as_mut().filter(|s| s.id == id) {
                    stick.at = touch.position;
                }
                if let Some(last) = state.looks.get_mut(&id) {
                    look += touch.position - *last;
                    *last = touch.position;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if state.stick.is_some_and(|s| s.id == id) {
                    state.stick = None;
                }
                state.looks.remove(&id);
                state.buttons.remove(&id);
            }
        }
    }
    let (axes, sprint) = state.stick.as_ref().map_or(([0.0; 2], false), stick_axes);
    frame.move_axes = axes;
    frame.look_px = [look.x * LOOK_SCALE, look.y * LOOK_SCALE];
    frame.held.clear();
    // AIM in toggle mode holds aim from one press to the next; any other button, and
    // AIM in hold mode, while a finger is on it.
    let toggle = settings.touch_aim_toggle;
    if !toggle {
        state.aim_latched = false;
    }
    for index in 0..BUTTON_COUNT {
        let held = if toggle && BUTTONS[index].key == AIM_KEY {
            state.aim_latched
        } else {
            state.buttons.values().any(|&b| b == index)
        };
        if !held {
            continue;
        }
        if let Some(id) = input_iw4::command_id_from_name(BUTTONS[index].command) {
            frame.held.insert((id, FIRST_TOUCH_KEYNUM + index as i32));
        }
    }
    if sprint && let Some(id) = input_iw4::command_id_from_name(SPRINT_COMMAND) {
        frame.held.insert((id, SPRINT_KEYNUM));
    }
}

#[derive(Component)]
pub(crate) struct TouchOverlayRoot;

#[derive(Component)]
pub(crate) struct TouchButtonNode(usize);

#[derive(Component)]
pub(crate) struct TouchButtonLabel(usize);

#[derive(Component)]
pub(crate) struct TouchButtonIcon(usize);

#[derive(Component)]
pub(crate) struct TouchStickBase;

#[derive(Component)]
pub(crate) struct TouchStickKnob;

/// Where the stick rests when no thumb is on it: a faint ring showing where to put one.
#[derive(Component)]
pub(crate) struct TouchStickHint;

#[derive(Component)]
pub(crate) struct TouchMenuButton;

/// Edit mode's banner (its line and the RESET / DONE pills).
#[derive(Component)]
pub(crate) struct TouchEditBanner;

#[derive(Component)]
pub(crate) struct TouchEditPill(usize);

/// The icon sets, decoded once: PERK (drawn in the style of BO1's perk-machine logos),
/// BO1 (its own HUD pictures where it has one), CLASSIC (the first drawn set), and the
/// weathered metal badges the icons sit on.
#[derive(Resource)]
pub(crate) struct TouchIcons {
    perk: [Handle<Image>; BUTTON_COUNT],
    classic: [Handle<Image>; BUTTON_COUNT],
    bo1: [Handle<Image>; BUTTON_COUNT],
    badge: Handle<Image>,
    badge_fire: Handle<Image>,
}

impl TouchIcons {
    fn set(&self, style: u8) -> &[Handle<Image>; BUTTON_COUNT] {
        match style {
            GameSettings::TOUCH_ICONS_BO1 => &self.bo1,
            GameSettings::TOUCH_ICONS_CLASSIC => &self.classic,
            _ => &self.perk,
        }
    }
}

/// The PERK set's picture for a button.
fn perk_icon(key: &str) -> Option<&'static [u8]> {
    Some(match key {
        "fire" | "fire2" => include_bytes!("../assets/touch/perk_fire.png"),
        "aim" => include_bytes!("../assets/touch/perk_aim.png"),
        "reload" => include_bytes!("../assets/touch/perk_reload.png"),
        "use" => include_bytes!("../assets/touch/perk_use.png"),
        "jump" => include_bytes!("../assets/touch/perk_jump.png"),
        "crouch" => include_bytes!("../assets/touch/perk_crouch.png"),
        "knife" => include_bytes!("../assets/touch/perk_knife.png"),
        "nade" => include_bytes!("../assets/touch/perk_nade.png"),
        _ => return None,
    })
}

/// Dark glass with a light rim, as BO1's console prompts; the fire button tinted red;
/// a held button lit in BO1's menu orange. Alphas are scaled by BUTTON OPACITY.
/// BO1's HUD look: dark warm glass, a worn off-white rim, FIRE in blood red; a held
/// button glows the menus' amber.
const IDLE_FILL: Color = Color::srgba(0.04, 0.03, 0.02, 0.46);
const FIRE_FILL: Color = Color::srgba(0.42, 0.03, 0.02, 0.5);
const HELD_FILL: Color = Color::srgba(1.0, 0.62, 0.15, 0.5);
const RIM: Color = Color::srgba(0.92, 0.86, 0.74, 0.4);
const FIRE_RIM: Color = Color::srgba(0.85, 0.12, 0.06, 0.8);
const HELD_RIM: Color = Color::srgba(1.0, 0.75, 0.35, 0.9);
const LABEL: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const SPRINT_KNOB: Color = Color::srgba(1.0, 0.62, 0.15, 0.7);
const KNOB: Color = Color::srgba(1.0, 1.0, 1.0, 0.38);
/// The resting stick's centre: from its edge (after `SAFE_EDGE`) and the bottom.
const STICK_HINT: Vec2 = Vec2::new(110.0, 105.0);

fn faded(color: Color, opacity: f32) -> Color {
    color.with_alpha(color.alpha() * opacity)
}

fn circle(radius: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: px(radius * 2.0),
        height: px(radius * 2.0),
        border: UiRect::all(px(2)),
        border_radius: BorderRadius::MAX,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn label_px(radius: f32) -> f32 {
    (radius * if radius >= 40.0 { 0.32 } else { 0.4 }).clamp(9.0, 22.0).round()
}

/// Decodes the icons once (PNG, built into the game).
pub(crate) fn load_touch_icons(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    icons: Option<ResMut<TouchIcons>>,
    fonts: Option<Res<crate::game_text::GameFonts>>,
    mut from_game: Local<bool>,
) {
    if let Some(mut icons) = icons {
        // BO1's own HUD pictures, once its fonts are read: the knife, the grenade and a
        // round for FIRE (both fire buttons) over the classic icons, for the BO1 set.
        if !*from_game && let Some(fonts) = fonts {
            *from_game = true;
            for (index, spec) in BUTTONS.iter().enumerate() {
                let key = if spec.key == "fire2" { "fire" } else { spec.key };
                if let Some(handle) = fonts.button_icons.get(key) {
                    icons.bo1[index] = handle.clone();
                }
            }
        }
        return;
    }
    let mut decode = |png: &[u8]| {
        let image = Image::from_buffer(
            png,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            bevy::image::ImageSampler::linear(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap_or_default();
        images.add(image)
    };
    let classic = BUTTONS.map(|spec| decode(spec.icon));
    let perk = std::array::from_fn(|index| match perk_icon(BUTTONS[index].key) {
        Some(png) => decode(png),
        None => classic[index].clone(),
    });
    commands.insert_resource(TouchIcons {
        perk,
        bo1: classic.clone(),
        classic,
        badge: decode(include_bytes!("../assets/touch/badge.png")),
        badge_fire: decode(include_bytes!("../assets/touch/badge_fire.png")),
    });
}

pub(crate) fn spawn_touch_overlay(commands: &mut Commands, enabled: bool) {
    if !enabled {
        return;
    }
    commands
        .spawn((
            TouchOverlayRoot,
            UiLayer::Hud,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                ..default()
            },
            FocusPolicy::Pass,
            GlobalZIndex(18_000),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            for (index, spec) in BUTTONS.iter().enumerate() {
                root.spawn((
                    TouchButtonNode(index),
                    circle(30.0),
                    BackgroundColor(IDLE_FILL),
                    BorderColor::all(RIM),
                    FocusPolicy::Pass,
                    Visibility::Hidden,
                ))
                .with_children(|button| {
                    button.spawn((
                        TouchButtonLabel(index),
                        GameText::new(GameFont::Normal, 11.0, LABEL, spec.label),
                    ));
                    button.spawn((
                        TouchButtonIcon(index),
                        Node {
                            width: percent(62),
                            height: percent(62),
                            display: Display::None,
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ));
                });
            }
            let (x, y, radius) = MENU_BUTTON;
            root.spawn((
                TouchMenuButton,
                Node {
                    left: px(x - radius),
                    top: px(y - radius),
                    column_gap: px(4),
                    ..circle(radius)
                },
                BackgroundColor(IDLE_FILL),
                BorderColor::all(RIM),
                FocusPolicy::Pass,
            ))
            .with_children(|pause| {
                // A pause sign: two bars.
                for _ in 0..2 {
                    pause.spawn((
                        Node {
                            width: px(4),
                            height: px(14),
                            ..default()
                        },
                        BackgroundColor(LABEL),
                    ));
                }
            });
            root.spawn((
                TouchStickHint,
                circle(STICK_RADIUS),
                BackgroundColor(Color::NONE),
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.16)),
                FocusPolicy::Pass,
            ))
            .with_child((
                Node {
                    width: px(STICK_KNOB_RADIUS * 2.0),
                    height: px(STICK_KNOB_RADIUS * 2.0),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
            ));
            root.spawn((
                TouchStickBase,
                circle(STICK_RADIUS),
                BackgroundColor(IDLE_FILL),
                BorderColor::all(RIM),
                FocusPolicy::Pass,
                Visibility::Hidden,
            ));
            root.spawn((
                TouchStickKnob,
                circle(STICK_KNOB_RADIUS),
                BackgroundColor(KNOB),
                BorderColor::all(RIM),
                FocusPolicy::Pass,
                Visibility::Hidden,
            ));
            // Edit mode: a line of help at the top and the RESET / DONE pills under it.
            root.spawn((
                TouchEditBanner,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                FocusPolicy::Pass,
                Visibility::Hidden,
            ))
            .with_children(|banner| {
                banner
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        top: px(16),
                        width: percent(100),
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_child(GameText::new(
                        GameFont::Normal,
                        16.0,
                        LABEL,
                        "DRAG A BUTTON TO MOVE IT",
                    ));
                for (index, label) in ["RESET", "DONE"].into_iter().enumerate() {
                    banner
                        .spawn((
                            TouchEditPill(index),
                            Node {
                                position_type: PositionType::Absolute,
                                width: px(116),
                                height: px(36),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(px(2)),
                                border_radius: BorderRadius::all(px(18)),
                                ..default()
                            },
                            BackgroundColor(if index == 1 {
                                Color::srgba(1.0, 0.62, 0.15, 0.55)
                            } else {
                                IDLE_FILL
                            }),
                            BorderColor::all(RIM),
                            FocusPolicy::Pass,
                        ))
                        .with_child(GameText::new(GameFont::Normal, 15.0, LABEL, label));
                }
            });
        });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn update_touch_overlay(
    mut commands: Commands,
    gameplay: Res<TouchGameplay>,
    state: Res<TouchState>,
    editing: Res<TouchEditing>,
    settings: Res<GameSettings>,
    icons: Option<Res<TouchIcons>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<TouchOverlayRoot>>,
    mut buttons: Query<
        (
            Entity,
            &TouchButtonNode,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Visibility,
            Option<&mut ImageNode>,
        ),
        (
            Without<TouchButtonIcon>,
            Without<TouchStickBase>,
            Without<TouchStickKnob>,
            Without<TouchStickHint>,
            Without<TouchOverlayRoot>,
            Without<TouchEditBanner>,
        ),
    >,
    mut labels: Query<
        (&TouchButtonLabel, &mut GameText, &mut Node),
        Without<TouchButtonNode>,
    >,
    mut glyphs: Query<
        (Entity, &TouchButtonIcon, &mut Node, Option<&mut ImageNode>),
        (Without<TouchButtonNode>, Without<TouchButtonLabel>),
    >,
    mut base: Query<
        (&mut Node, &mut Visibility, &mut BackgroundColor, &mut BorderColor),
        (
            With<TouchStickBase>,
            Without<TouchStickKnob>,
            Without<TouchOverlayRoot>,
            Without<TouchButtonNode>,
            Without<TouchButtonLabel>,
            Without<TouchButtonIcon>,
        ),
    >,
    mut knob: Query<
        (&mut Node, &mut Visibility, &mut BackgroundColor),
        (
            With<TouchStickKnob>,
            Without<TouchStickBase>,
            Without<TouchOverlayRoot>,
            Without<TouchButtonNode>,
            Without<TouchButtonLabel>,
            Without<TouchButtonIcon>,
        ),
    >,
    mut hint: Query<
        (&mut Node, &mut Visibility),
        (
            With<TouchStickHint>,
            Without<TouchStickBase>,
            Without<TouchStickKnob>,
            Without<TouchOverlayRoot>,
            Without<TouchButtonNode>,
            Without<TouchButtonLabel>,
            Without<TouchButtonIcon>,
        ),
    >,
    mut banner: Query<
        &mut Visibility,
        (
            With<TouchEditBanner>,
            Without<TouchOverlayRoot>,
            Without<TouchButtonNode>,
            Without<TouchStickHint>,
            Without<TouchStickBase>,
            Without<TouchStickKnob>,
        ),
    >,
    mut pills: Query<
        (&TouchEditPill, &mut Node),
        (
            Without<TouchButtonNode>,
            Without<TouchButtonLabel>,
            Without<TouchButtonIcon>,
            Without<TouchStickBase>,
            Without<TouchStickKnob>,
            Without<TouchStickHint>,
        ),
    >,
) {
    let Ok(mut root) = root.single_mut() else {
        return;
    };
    let show = if gameplay.owns_touches {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    root.set_if_neq(show);
    let size = windows
        .single()
        .map_or(Vec2::new(844.0, 390.0), |w| Vec2::new(w.width(), w.height()));
    let edit = editing.active.then_some(editing.at.as_ref()).flatten();
    let mut placed = placements(&settings, size, edit);
    if !editing.active && !state.use_available {
        placed[index_of(USE_KEY)] = None;
    }
    let opacity = if editing.active {
        1.0
    } else {
        settings.touch_opacity
    };
    let grabbed = editing.grab.map(|(_, index, _)| index);
    let style = settings.touch_icon_style;
    let badges = style != GameSettings::TOUCH_ICONS_TEXT && icons.is_some();
    for (entity, button, mut node, mut fill, mut rim, mut visibility, badge) in &mut buttons {
        let Some(at) = placed[button.0] else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let want = (
            px(at.centre.x - at.radius),
            px(at.centre.y - at.radius),
            px(at.radius * 2.0),
        );
        if node.left != want.0 || node.top != want.1 || node.width != want.2 {
            node.left = want.0;
            node.top = want.1;
            node.right = Val::Auto;
            node.bottom = Val::Auto;
            node.width = want.2;
            node.height = want.2;
        }
        let held = state.button_held(button.0) || grabbed == Some(button.0);
        let idle = if BUTTONS[button.0].command == "+attack" {
            FIRE_FILL
        } else {
            IDLE_FILL
        };
        let fire = BUTTONS[button.0].command == "+attack";
        // Icons sit on weathered metal badges (the badge is the button's picture; a held
        // one glows amber at the rim); text buttons keep the dark glass circle.
        let want_fill = if badges {
            Color::NONE
        } else if held {
            HELD_FILL
        } else {
            faded(idle, opacity)
        };
        if fill.0 != want_fill {
            fill.0 = want_fill;
        }
        let rim_color = if fire { FIRE_RIM } else { RIM };
        let want_rim = BorderColor::all(if held {
            HELD_RIM
        } else if badges {
            Color::NONE
        } else {
            faded(rim_color, opacity)
        });
        if *rim != want_rim {
            *rim = want_rim;
        }
        let badge_tint = if !badges {
            Color::NONE
        } else if held {
            Color::srgba(1.0, 0.8, 0.55, opacity.max(0.6))
        } else {
            faded(Color::WHITE, opacity.max(0.45))
        };
        match (badge, icons.as_deref()) {
            (Some(mut image), Some(icons)) => {
                let handle = if fire { &icons.badge_fire } else { &icons.badge };
                if image.image != *handle {
                    image.image = handle.clone();
                }
                if image.color != badge_tint {
                    image.color = badge_tint;
                }
            }
            (None, Some(icons)) => {
                let handle = if fire { &icons.badge_fire } else { &icons.badge };
                commands
                    .entity(entity)
                    .insert(ImageNode::new(handle.clone()).with_color(badge_tint));
            }
            _ => {}
        }
    }
    let icons_on = style != GameSettings::TOUCH_ICONS_TEXT && icons.is_some();
    for (label, mut text, mut node) in &mut labels {
        let radius = placed[label.0].map_or(30.0, |placed| placed.radius);
        text.set_size(label_px(radius));
        text.set_color(faded(LABEL, opacity.max(0.6)));
        let display = if icons_on { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
    }
    for (entity, glyph, mut node, image) in &mut glyphs {
        let display = if icons_on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let tint = faded(Color::WHITE, opacity.max(0.6));
        match (image, icons.as_deref()) {
            (Some(mut image), icons) => {
                if image.color != tint {
                    image.color = tint;
                }
                if let Some(icons) = icons
                    && image.image != icons.set(style)[glyph.0]
                {
                    image.image = icons.set(style)[glyph.0].clone();
                }
            }
            (None, Some(icons)) => {
                commands
                    .entity(entity)
                    .insert(ImageNode::new(icons.set(style)[glyph.0].clone()).with_color(tint));
            }
            (None, None) => {}
        }
    }
    let stick = state.stick;
    let stick_right = stick_on_right(&settings);
    if let Ok((mut node, mut vis)) = hint.single_mut() {
        vis.set_if_neq(if stick.is_some() || editing.active {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        let x = if stick_right {
            size.x - SAFE_EDGE - STICK_HINT.x
        } else {
            SAFE_EDGE + STICK_HINT.x
        };
        let want = (px(x - STICK_RADIUS), px(size.y - STICK_HINT.y - STICK_RADIUS));
        if node.left != want.0 || node.top != want.1 {
            node.left = want.0;
            node.top = want.1;
        }
    }
    let sprinting = stick.as_ref().is_some_and(|stick| stick_axes(stick).1);
    if let Ok((mut node, mut vis, mut fill, mut rim)) = base.single_mut() {
        match stick {
            Some(stick) => {
                node.left = px(stick.origin.x - STICK_RADIUS);
                node.top = px(stick.origin.y - STICK_RADIUS);
                vis.set_if_neq(Visibility::Inherited);
                fill.0 = faded(IDLE_FILL, opacity);
                *rim = BorderColor::all(if sprinting {
                    HELD_RIM
                } else {
                    faded(RIM, opacity)
                });
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    if let Ok((mut node, mut vis, mut fill)) = knob.single_mut() {
        match stick {
            Some(stick) => {
                let delta = stick.at - stick.origin;
                let clamped = delta.clamp_length_max(STICK_RADIUS);
                let at = stick.origin + clamped;
                node.left = px(at.x - STICK_KNOB_RADIUS);
                node.top = px(at.y - STICK_KNOB_RADIUS);
                vis.set_if_neq(Visibility::Inherited);
                fill.0 = if sprinting { SPRINT_KNOB } else { KNOB };
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    if let Ok(mut vis) = banner.single_mut() {
        vis.set_if_neq(if editing.active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let spots = edit_pills(size);
    for (pill, mut node) in &mut pills {
        let (centre, half) = spots[pill.0];
        let want = (px(centre.x - half.x), px(centre.y - half.y));
        if node.left != want.0 || node.top != want.1 {
            node.left = want.0;
            node.top = want.1;
        }
    }
}

pub(crate) fn register_touch_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("touch").is_none() {
        registry.register(
            crate::CommandSpec::new("touch")
                .usage("touch <id> <down|move|up> <x> <y> — inject a touch (logical pixels)"),
        );
    }
}

pub(crate) fn handle_touch_commands(
    mut events: MessageReader<ConsoleCommand>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut touches: MessageWriter<TouchInput>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
) {
    for command in events.read() {
        if command.name != "touch" {
            continue;
        }
        let parsed = match command.args.as_slice() {
            [id, phase, x, y] => id
                .parse::<u64>()
                .ok()
                .zip(match phase.as_str() {
                    "down" => Some(TouchPhase::Started),
                    "move" => Some(TouchPhase::Moved),
                    "up" => Some(TouchPhase::Ended),
                    _ => None,
                })
                .zip(x.parse::<f32>().ok().zip(y.parse::<f32>().ok())),
            _ => None,
        };
        let (Some(((id, phase), (x, y))), Ok(window)) = (parsed, windows.single()) else {
            console.echo(
                "usage: touch <id> <down|move|up> <x> <y>",
                settings.log_capacity,
            );
            continue;
        };
        touches.write(TouchInput {
            phase,
            position: Vec2::new(x, y),
            window,
            force: None,
            id,
        });
    }
}

pub(crate) fn touch_gameplay_resource() -> TouchGameplay {
    TouchGameplay {
        enabled: enabled_from_env(),
        owns_touches: false,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Vec2 = Vec2::new(844.0, 390.0);

    fn with_layout(layout: u8, scale: f32) -> GameSettings {
        GameSettings {
            touch_layout: layout,
            touch_button_scale: scale,
            ..GameSettings::default()
        }
    }

    #[test]
    fn every_preset_fits_the_screen_without_overlaps() {
        for layout in 0..GameSettings::TOUCH_LAYOUT_CUSTOM {
            for scale in [0.75, 1.0, 1.35] {
                let settings = with_layout(layout, scale);
                let placed = placements(&settings, PHONE, None);
                let right = stick_on_right(&settings);
                for (i, a) in placed.iter().enumerate() {
                    let Some(a) = a else { continue };
                    let label = BUTTONS[i].key;
                    assert!(a.centre.x - a.radius >= 0.0 && a.centre.x + a.radius <= PHONE.x, "{label} off the side in layout {layout}");
                    assert!(a.centre.y - a.radius >= 0.0 && a.centre.y + a.radius <= PHONE.y, "{label} off the top or bottom in layout {layout}");
                    if BUTTONS[i].key != "fire2" {
                        assert!(!in_stick_zone(a.centre, PHONE, right), "{label} in the stick zone in layout {layout}");
                    }
                    for (j, b) in placed.iter().enumerate().skip(i + 1) {
                        let Some(b) = b else { continue };
                        assert!(
                            a.centre.distance(b.centre) >= a.radius + b.radius - 0.5,
                            "{label} overlaps {} in layout {layout} at size {scale}",
                            BUTTONS[j].key
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn left_fire_has_a_second_fire_button_and_classic_does_not() {
        let classic = placements(&with_layout(GameSettings::TOUCH_LAYOUT_CLASSIC, 1.0), PHONE, None);
        let left = placements(&with_layout(GameSettings::TOUCH_LAYOUT_LEFT_FIRE, 1.0), PHONE, None);
        assert!(classic[9].is_none());
        let second = left[9].expect("second fire button");
        assert!(second.centre.x < PHONE.x * 0.5);
        assert_eq!(button_at(second.centre, &left), Some(9));
    }

    #[test]
    fn southpaw_mirrors_buttons_and_stick() {
        let settings = with_layout(GameSettings::TOUCH_LAYOUT_SOUTHPAW, 1.0);
        let placed = placements(&settings, PHONE, None);
        assert!(placed[0].expect("fire").centre.x < PHONE.x * 0.5);
        assert!(in_stick_zone(Vec2::new(PHONE.x - 150.0, 300.0), PHONE, true));
        assert!(!in_stick_zone(Vec2::new(150.0, 300.0), PHONE, true));
    }

    #[test]
    fn custom_layout_round_trips_and_places_buttons() {
        let mut at = [None; BUTTON_COUNT];
        at[0] = Some(Vec2::new(0.5, 0.5));
        at[4] = Some(Vec2::new(0.9, 0.2));
        let saved = format_custom(&at);
        let back = parse_custom(&saved);
        assert!((back[0].unwrap() - Vec2::new(0.5, 0.5)).length() < 1e-3);
        assert!((back[4].unwrap() - Vec2::new(0.9, 0.2)).length() < 1e-3);
        let settings = GameSettings {
            touch_layout: GameSettings::TOUCH_LAYOUT_CUSTOM,
            touch_custom: saved,
            ..GameSettings::default()
        };
        let placed = placements(&settings, PHONE, None);
        assert!((placed[0].unwrap().centre - Vec2::new(422.0, 195.0)).length() < 0.5);
        // A button the saved layout leaves out keeps its CLASSIC place.
        assert!(placed[1].is_some());
        assert!(placed[9].is_none());
    }

    #[test]
    fn button_commands_are_known_inputs() {
        for spec in BUTTONS {
            assert!(input_iw4::command_id_from_name(spec.command).is_some(), "{}", spec.command);
        }
        assert!(input_iw4::command_id_from_name(SPRINT_COMMAND).is_some());
        assert!(SPRINT_KEYNUM < input_iw4::KEY_COUNT as i32);
    }

    #[test]
    fn icons_decode() {
        for spec in BUTTONS {
            let image = Image::from_buffer(
                spec.icon,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                true,
                bevy::image::ImageSampler::linear(),
                bevy::asset::RenderAssetUsages::default(),
            );
            assert!(image.is_ok(), "{} icon", spec.key);
        }
    }

    #[test]
    fn stick_pushed_up_walks_forward_and_far_up_sprints() {
        let origin = Vec2::new(150.0, 250.0);
        let walk = Stick { id: 1, origin, at: origin + Vec2::new(0.0, -STICK_RADIUS) };
        let (axes, sprint) = stick_axes(&walk);
        assert!((axes[0] - 1.0).abs() < 1e-4 && axes[1].abs() < 1e-4 && !sprint);
        let run = Stick { id: 1, origin, at: origin + Vec2::new(0.0, -STICK_RADIUS * 1.5) };
        assert!(stick_axes(&run).1);
        let strafe = Stick { id: 1, origin, at: origin + Vec2::new(STICK_RADIUS * 1.5, 0.0) };
        let (axes, sprint) = stick_axes(&strafe);
        assert!(axes[1] > 0.99 && !sprint);
        let rest = Stick { id: 1, origin, at: origin + Vec2::new(3.0, 3.0) };
        assert_eq!(stick_axes(&rest).0, [0.0, 0.0]);
    }
}
