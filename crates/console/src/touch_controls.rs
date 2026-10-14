//! On-screen touch controls for phones. A finger put down on the left of the screen
//! becomes a floating stick that moves the player; dragging anywhere else turns the
//! view; buttons on the right press gameplay inputs. Holding fire and dragging the same
//! finger also turns, so you can aim while shooting.
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
use frame::AppScreen;
use ui::UiLayer;

use crate::{ConsoleSettings, ConsoleState};
use crate::{ConsoleCommand, ConsoleRegistry};

pub const TOUCH_ENV: &str = "IW4L_TOUCH";

/// Room kept free at the left and right edges for the notch / Dynamic Island in landscape.
const SAFE_EDGE: f32 = 48.0;
/// The stick lives in this share of the screen's width, below `STICK_TOP`.
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
    label: &'static str,
    command: &'static str,
    /// Centre, in logical pixels from the right edge (after `SAFE_EDGE`) and the bottom.
    right: f32,
    bottom: f32,
    radius: f32,
    /// A finger that pressed this button also turns the view when it drags.
    looks: bool,
}

const BUTTONS: [ButtonSpec; 9] = [
    ButtonSpec { label: "FIRE", command: "+attack", right: 130.0, bottom: 125.0, radius: 48.0, looks: true },
    ButtonSpec { label: "AIM", command: "+speed_throw", right: 240.0, bottom: 70.0, radius: 34.0, looks: true },
    ButtonSpec { label: "RELOAD", command: "+reload", right: 245.0, bottom: 168.0, radius: 28.0, looks: false },
    ButtonSpec { label: "USE", command: "+activate", right: 130.0, bottom: 238.0, radius: 30.0, looks: false },
    ButtonSpec { label: "JUMP", command: "+gostand", right: 40.0, bottom: 200.0, radius: 30.0, looks: false },
    ButtonSpec { label: "CROUCH", command: "togglecrouch", right: 40.0, bottom: 110.0, radius: 28.0, looks: false },
    ButtonSpec { label: "KNIFE", command: "+melee", right: 325.0, bottom: 60.0, radius: 26.0, looks: false },
    ButtonSpec { label: "NADE", command: "+frag", right: 330.0, bottom: 150.0, radius: 26.0, looks: false },
    ButtonSpec { label: "SWAP", command: "weapnext", right: 40.0, bottom: 290.0, radius: 26.0, looks: false },
];
const SPRINT_COMMAND: &str = "+breath_sprint";
const SPRINT_KEYNUM: i32 = FIRST_TOUCH_KEYNUM + BUTTONS.len() as i32;

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
}

impl TouchState {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn button_held(&self, index: usize) -> bool {
        self.buttons.values().any(|&b| b == index)
    }
}

fn enabled_from_env() -> bool {
    match std::env::var(TOUCH_ENV).ok().as_deref().map(str::trim) {
        Some("1" | "on" | "true") => true,
        Some("0" | "off" | "false") => false,
        _ => cfg!(target_os = "ios"),
    }
}

fn button_centre(spec: &ButtonSpec, size: Vec2) -> Vec2 {
    Vec2::new(size.x - SAFE_EDGE - spec.right, size.y - spec.bottom)
}

fn button_at(pos: Vec2, size: Vec2) -> Option<usize> {
    BUTTONS.iter().position(|spec| {
        // A little grace around each button: fingers are bigger than the drawing.
        pos.distance(button_centre(spec, size)) <= spec.radius + 8.0
    })
}

fn in_stick_zone(pos: Vec2, size: Vec2) -> bool {
    pos.x >= SAFE_EDGE * 0.5 && pos.x < size.x * STICK_ZONE_WIDTH && pos.y > size.y * STICK_TOP
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

pub(crate) fn update_touch_ownership(
    screen: Option<Res<AppScreen>>,
    console: Res<ConsoleState>,
    menus: Option<Res<hud::ScriptMenus>>,
    mut gameplay: ResMut<TouchGameplay>,
) {
    let in_game = screen.as_ref().is_some_and(|s| **s == AppScreen::InGame);
    let menu = menus.is_some_and(|m| m.captures_input());
    let owns = gameplay.enabled && in_game && !console.open && !menu;
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

pub(crate) fn read_touch_controls(
    mut touches: MessageReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    gameplay: Res<TouchGameplay>,
    mut state: ResMut<TouchState>,
    mut frame: ResMut<TouchFrame>,
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
    let mut look = Vec2::ZERO;
    for touch in touches.read() {
        let id = touch.id;
        match touch.phase {
            TouchPhase::Started => {
                if let Some(index) = button_at(touch.position, size) {
                    if std::env::var_os("IW4L_TOUCH_DEBUG").is_some() {
                        diag::info!(Console, "touch: finger {id} on {}", BUTTONS[index].label);
                    }
                    state.buttons.insert(id, index);
                    if BUTTONS[index].looks {
                        state.looks.insert(id, touch.position);
                    }
                } else if state.stick.is_none() && in_stick_zone(touch.position, size) {
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
    for &index in state.buttons.values() {
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
pub(crate) struct TouchStickBase;

#[derive(Component)]
pub(crate) struct TouchStickKnob;

const IDLE_FILL: Color = Color::srgba(1.0, 1.0, 1.0, 0.14);
const HELD_FILL: Color = Color::srgba(1.0, 0.86, 0.45, 0.42);
const RIM: Color = Color::srgba(1.0, 1.0, 1.0, 0.42);
const LABEL: Color = Color::srgba(1.0, 1.0, 1.0, 0.85);

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

pub(crate) fn spawn_touch_overlay(commands: &mut Commands, font: Handle<Font>, enabled: bool) {
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
                    Node {
                        right: px(SAFE_EDGE + spec.right - spec.radius),
                        bottom: px(spec.bottom - spec.radius),
                        ..circle(spec.radius)
                    },
                    BackgroundColor(IDLE_FILL),
                    BorderColor::all(RIM),
                    FocusPolicy::Pass,
                ))
                .with_child((
                    Text::new(spec.label),
                    TextFont {
                        font: font.clone().into(),
                        font_size: FontSize::Px(if spec.radius >= 40.0 { 15.0 } else { 11.0 }),
                        ..default()
                    },
                    TextColor(LABEL),
                ));
            }
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
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.38)),
                BorderColor::all(RIM),
                FocusPolicy::Pass,
                Visibility::Hidden,
            ));
        });
}

pub(crate) fn update_touch_overlay(
    gameplay: Res<TouchGameplay>,
    state: Res<TouchState>,
    mut root: Query<&mut Visibility, With<TouchOverlayRoot>>,
    mut buttons: Query<(&TouchButtonNode, &mut BackgroundColor)>,
    mut base: Query<
        (&mut Node, &mut Visibility),
        (With<TouchStickBase>, Without<TouchStickKnob>, Without<TouchOverlayRoot>),
    >,
    mut knob: Query<
        (&mut Node, &mut Visibility),
        (With<TouchStickKnob>, Without<TouchStickBase>, Without<TouchOverlayRoot>),
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
    for (node, mut fill) in &mut buttons {
        let want = if state.button_held(node.0) { HELD_FILL } else { IDLE_FILL };
        if fill.0 != want {
            fill.0 = want;
        }
    }
    let stick = state.stick;
    if let Ok((mut node, mut vis)) = base.single_mut() {
        match stick {
            Some(stick) => {
                node.left = px(stick.origin.x - STICK_RADIUS);
                node.top = px(stick.origin.y - STICK_RADIUS);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
    if let Ok((mut node, mut vis)) = knob.single_mut() {
        match stick {
            Some(stick) => {
                let delta = stick.at - stick.origin;
                let clamped = delta.clamp_length_max(STICK_RADIUS);
                let at = stick.origin + clamped;
                node.left = px(at.x - STICK_KNOB_RADIUS);
                node.top = px(at.y - STICK_KNOB_RADIUS);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
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

    #[test]
    fn buttons_do_not_overlap_and_fit_the_screen() {
        for (i, a) in BUTTONS.iter().enumerate() {
            let ca = button_centre(a, PHONE);
            assert!(ca.x + a.radius <= PHONE.x - SAFE_EDGE + 1.0, "{} past the edge", a.label);
            assert!(ca.y - a.radius >= 0.0 && ca.y + a.radius <= PHONE.y, "{} off screen", a.label);
            assert!(!in_stick_zone(ca, PHONE), "{} sits in the stick zone", a.label);
            for b in &BUTTONS[i + 1..] {
                let cb = button_centre(b, PHONE);
                assert!(
                    ca.distance(cb) >= a.radius + b.radius,
                    "{} overlaps {}",
                    a.label,
                    b.label
                );
            }
        }
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
