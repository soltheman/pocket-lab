//! BO1 AI actors (zombies). An `actor_*` spawner's `CodeSpawnerSpawn` makes a script
//! model with the spawner's model and runs the engine's actor scripts on it (aitype, init
//! animscript). BO1's own scripts then drive it through the AI natives here: a goal
//! (`setGoalPos`/`Node`/`Entity`, answered with `goal`), `animScripted` (its notetracks
//! and `end`), and the window climb its pathing would do. Each tick it walks straight at
//! its goal with monster collision (sliding along walls) and, once in the playable area,
//! swings at players in reach. Damage goes through BO1's `CodeCallback_ActorDamage`, whose
//! scripts score the hit and call `finishActorDamage`; at zero health it runs
//! `CodeCallback_ActorKilled`, the `death` notify and the death animscript. The body is
//! deleted a little later.

use bevy_ecs::prelude::World;

use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Runtime, Value};

const ACTOR_DAMAGE: &str = "maps/_callbacksetup::codecallback_actordamage";
const ACTOR_KILLED: &str = "maps/_callbacksetup::codecallback_actorkilled";

/// Ground speeds, units per second, for BO1's `zombie_move_speed` (walk, run, sprint):
/// about what each gait's clips cover.
const WALK_SPEED: f32 = 55.0;
const RUN_SPEED: f32 = 115.0;
const SPRINT_SPEED: f32 = 175.0;
const WALK_CLIPS: [&str; 8] = [
    "ai_zombie_walk_v1",
    "ai_zombie_walk_v2",
    "ai_zombie_walk_v3",
    "ai_zombie_walk_v4",
    "ai_zombie_walk_v6",
    "ai_zombie_walk_v7",
    "ai_zombie_walk_v8",
    "ai_zombie_walk_v9",
];
const RUN_CLIPS: [&str; 4] = [
    "ai_zombie_run_v1",
    "ai_zombie_run_v2",
    "ai_zombie_run_v3",
    "ai_zombie_run_v4",
];
const SPRINT_CLIPS: [&str; 2] = ["ai_zombie_sprint_v1", "ai_zombie_sprint_v2"];
const ATTACK_CLIPS: [&str; 4] = [
    "ai_zombie_attack_v1",
    "ai_zombie_attack_v2",
    "ai_zombie_attack_v4",
    "ai_zombie_attack_v6",
];
const DEATH_CLIPS: [&str; 2] = ["ai_zombie_death_v1", "ai_zombie_death_v2"];
/// Close enough to swing at a player (from feet to feet, horizontally).
const REACH: f32 = 52.0;
const HIT_DAMAGE: i32 = 50;
const HIT_INTERVAL_MS: i64 = 1250;
/// The collision cube's half size; its centre sits this far above the feet.
const RADIUS: f32 = 15.0;
/// A killed actor's body stays this long before it is deleted.
const CORPSE_MS: i64 = 1500;
const DEFAULT_HEALTH: i32 = 150;

/// BO1's window climb clips.
const TRAVERSE_CLIPS: [&str; 4] = [
    "ai_zombie_traverse_v1",
    "ai_zombie_traverse_v2",
    "ai_zombie_traverse_v3",
    "ai_zombie_traverse_v4",
];
/// How far past its exterior goal (along the goal's facing) a zombie lands inside.
const WINDOW_HOP: f32 = 110.0;
const TRAVERSE_MS: i64 = 1400;
const DEFAULT_GOAL_RADIUS: f32 = 32.0;

#[derive(Clone, Debug)]
enum Goal {
    Pos([f32; 3]),
    Entity(u64),
}

/// An `animScripted` clip: the actor holds its place while the notetracks fire.
#[derive(Clone, Debug)]
struct Scripted {
    notify: std::sync::Arc<str>,
    start_ms: i64,
    length_ms: i64,
    notes: std::sync::Arc<[crate::AnimNote]>,
    fired: usize,
}

/// Carried through a window: from where it stood to where it lands inside.
#[derive(Clone, Debug)]
struct Traverse {
    from: [f32; 3],
    to: [f32; 3],
    start_ms: i64,
    ms: i64,
    /// Through its window (BO1 waits on the traverse notifies), not an unsticking hop.
    window: bool,
    /// Over a ledge (a traversal link or a hop): up before across when climbing, across
    /// before down when dropping, so it doesn't pass through the ledge's corner.
    ledge: bool,
}

impl Traverse {
    /// Where it is `t` (0..1) of the way through.
    fn at(&self, t: f32) -> [f32; 3] {
        let mut at: [f32; 3] = std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * t);
        if self.ledge {
            let rise = self.to[2] - self.from[2];
            let share = if rise > 0.0 {
                (t / 0.4).min(1.0)
            } else {
                ((t - 0.5) / 0.5).max(0.0)
            };
            at[2] = self.from[2] + rise * share;
        }
        at
    }
}

/// How long a traversal (or hop) from `from` to `to` takes.
fn ledge_ms(from: [f32; 3], to: [f32; 3]) -> i64 {
    let rise = (to[2] - from[2]).abs();
    (500.0 + flat_distance(from, to) * 3.0 + rise * 4.0).clamp(500.0, 2500.0) as i64
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Actor {
    next_hit_ms: i64,
    fall: f32,
    died_ms: Option<i64>,
    /// The clip the body plays now (dogs play none yet).
    clip: Option<&'static str>,
    goal: Option<Goal>,
    goal_yaw: Option<f32>,
    goal_reached: bool,
    scripted: Option<Scripted>,
    traverse: Option<Traverse>,
    climbed: bool,
    /// The walk-graph route toward `routed_to`, next node first, and when to plan again.
    route: Vec<[f32; 3]>,
    /// Per route node: reaching it from the node before is a traversal (jump, climb).
    route_traverse: Vec<bool>,
    routed_to: Option<[f32; 3]>,
    replan_ms: i64,
    /// Since when its walk has made no headway, and where it was steering.
    stuck_since: Option<i64>,
    /// The height it spawned at: far below that it has fallen out of the map.
    home_z: f32,
}

/// How far above or below its goal an actor may be and still be at it.
const GOAL_HEIGHT: f32 = 80.0;

/// No headway for this long and the actor hops to where it steers, when that is in sight
/// and near (a rail, a ledge, a step too tall for the walk).
const STUCK_MS: i64 = 800;
const HOP_REACH: f32 = 260.0;
/// Fallen this far below its spawn: out of the map, so it dies.
const FELL_OUT: f32 = 1500.0;

/// Nothing solid between two points (the actor itself aside), by what stops an actor's
/// walk: player clip (rails, ledge edges kept from players) doesn't.
fn clear_between(world: &mut World, object: u64, a: [f32; 3], b: [f32; 3]) -> bool {
    const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
    let ignore = super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
    matches!(
        super::natives::engine::entity_trace(
            world,
            a,
            b,
            crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP,
            ignore,
        ),
        crate::bullet_collision::TraceOutcome::Miss { .. }
    )
}

/// Nothing solid between two points at chest height, straight across or over the top
/// (up from the lower one, across at the higher one's height, down: a ledge or a drop).
fn in_sight(world: &mut World, object: u64, from: [f32; 3], to: [f32; 3]) -> bool {
    let mut clear = |a: [f32; 3], b: [f32; 3]| clear_between(world, object, a, b);
    let lift = |p: [f32; 3]| [p[0], p[1], p[2] + 40.0];
    if clear(lift(from), lift(to)) {
        return true;
    }
    let top = from[2].max(to[2]) + 40.0;
    let over_from = [from[0], from[1], top];
    let over_to = [to[0], to[1], top];
    clear(lift(from), over_from) && clear(over_from, over_to) && clear(over_to, lift(to))
}

/// Plan again this often, or as soon as the goal moves this far.
const REPLAN_MS: i64 = 1000;
const REPLAN_MOVE: f32 = 96.0;
/// A route node counts as reached this close (horizontally, and within a step of height).
const NODE_REACHED: f32 = 28.0;

/// The graph actors route over: the map's own path nodes, else the baked walk graph.
pub(crate) fn nav(world: &mut World) -> Option<std::sync::Arc<crate::actor_nav::ActorNav>> {
    FrameWorld::from_world(world).ai_nav().or_else(|| {
        world
            .get_resource::<crate::actor_nav::BakedActorNav>()
            .map(|baked| baked.0.clone())
    })
}

/// The graph node a route starts or ends at for `at`: the nearest in sight of it (the
/// nearest of all can be on the far side of a wall), else the nearest.
fn graph_end(
    world: &mut World,
    object: u64,
    nav: &crate::actor_nav::ActorNav,
    at: [f32; 3],
) -> Option<usize> {
    let near = nav.nearest_k(at, 6);
    near.iter()
        .copied()
        .find(|&node| {
            let node = nav.nodes[node];
            clear_between(
                world,
                object,
                [at[0], at[1], at[2] + 40.0],
                [node[0], node[1], node[2] + 40.0],
            )
        })
        .or_else(|| near.first().copied())
}

/// The nodes a route may start at for an actor at `at`: the nearest few in sight of it,
/// each costing the way onto it (height counted double: another floor is further than
/// it looks), else the nearest.
fn route_starts(
    world: &mut World,
    object: u64,
    nav: &crate::actor_nav::ActorNav,
    at: [f32; 3],
) -> Vec<(usize, f32)> {
    let near = nav.nearest_k(at, 6);
    let cost = |node: usize| {
        let n = nav.nodes[node];
        let rise = 2.0 * (n[2] - at[2]);
        (flat_distance(at, n).powi(2) + rise * rise).sqrt()
    };
    let seen: Vec<(usize, f32)> = near
        .iter()
        .copied()
        .filter(|&node| {
            let n = nav.nodes[node];
            clear_between(
                world,
                object,
                [at[0], at[1], at[2] + 40.0],
                [n[0], n[1], n[2] + 40.0],
            )
        })
        .map(|node| (node, cost(node)))
        .collect();
    if seen.is_empty() {
        near.first().map(|&node| vec![(node, cost(node))]).unwrap_or_default()
    } else {
        seen
    }
}

/// Where to steer this tick toward `target`: the next node of a walk-graph route when the
/// map has one and both ends are on it, else straight at the target.
fn steer_point(
    world: &mut World,
    object: u64,
    state: &mut Actor,
    origin: [f32; 3],
    target: [f32; 3],
    now: i64,
) -> [f32; 3] {
    let Some(nav) = nav(world) else {
        return target;
    };
    // Near and in plain sight: straight at it. Near behind a wall (a window's attack spot
    // round the corner of its frame), still by the route, or it walks into the wall.
    let close = flat_distance(origin, target) < 96.0
        && (origin[2] - target[2]).abs() < 40.0
        && clear_between(
            world,
            object,
            [origin[0], origin[1], origin[2] + 40.0],
            [target[0], target[1], target[2] + 40.0],
        );
    if close || nav.is_empty() {
        state.route.clear();
        state.route_traverse.clear();
        return target;
    }
    let moved = state
        .routed_to
        .is_none_or(|was| flat_distance(was, target) > REPLAN_MOVE);
    if moved || now >= state.replan_ms {
        // Inside, the route joins where the whole way is shortest (see `route_starts`);
        // on the way to its window, at the nearest node as before (window entry relies on it).
        let inside = truthy(
            &world
                .resource_mut::<Runtime>()
                .object_field(object, "completed_emerging_into_playable_area"),
        );
        let starts = if inside {
            route_starts(world, object, &nav, origin)
        } else {
            graph_end(world, object, &nav, origin)
                .map(|node| vec![(node, 0.0)])
                .unwrap_or_default()
        };
        let goal = graph_end(world, object, &nav, target);
        // Around closed doors and debris (their `disconnectpaths`).
        let blocks = &world.resource::<Runtime>().nav_blocks;
        let nodes = goal
            .and_then(|goal| nav.route_nodes_from(&starts, goal, blocks))
            .unwrap_or_default();
        state.route = nodes.iter().map(|&node| nav.nodes[node]).collect();
        state.route_traverse = std::iter::once(false)
            .chain(
                nodes
                    .windows(2)
                    .map(|pair| nav.traversals.contains(&(pair[0] as u32, pair[1] as u32))),
            )
            .collect();
        if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
            diag::info!(
                Sim,
                "actor debug: route {:?} -> {:?}: {} nodes ({} in graph; start {:?} reaches {:?}, goal {:?} reaches {:?})",
                origin.map(|v| v as i32),
                target.map(|v| v as i32),
                state.route.len(),
                nav.nodes.len(),
                nav.nearest(origin).map(|n| nav.nodes[n].map(|v| v as i32)),
                nav.nearest(origin).map(|n| nav.reach_count(n)),
                nav.nearest(target).map(|n| nav.nodes[n].map(|v| v as i32)),
                nav.nearest(target).map(|n| nav.reach_count(n)),
            );
        }
        state.routed_to = Some(target);
        state.replan_ms = now + REPLAN_MS;
        // No route to the goal: the engine's `bad_path`, on which BO1 picks another window
        // (zombie_assure_node) or falls back to following the player's breadcrumbs.
        if state.route.is_empty() {
            raise(world, Value::Object(object), "bad_path", Vec::new());
        }
        // The first node is where the route joins. Outside, it is where the actor stands:
        // skip it. Inside, skip it once the actor is nearer the next node than it is
        // (passed it), unless a traversal starts there: it has to get onto it first.
        let keep_entry = inside
            && state.route.len() > 1
            && (state.route_traverse.get(1).copied().unwrap_or(false)
                || flat_distance(origin, state.route[1])
                    >= flat_distance(state.route[0], state.route[1]));
        if state.route.len() > 1 && !keep_entry {
            state.route.remove(0);
            state.route_traverse.remove(0);
        }
    }
    while let Some(next) = state.route.first() {
        if flat_distance(origin, *next) < NODE_REACHED && (origin[2] - next[2]).abs() < 48.0 {
            state.route.remove(0);
            if !state.route_traverse.is_empty() {
                state.route_traverse.remove(0);
            }
            // At a traversal's start: jump down, climb up or mantle to its end, as the
            // map's traverse animscripts would carry it.
            // Only once inside: getting in through its window is the window climb's.
            let inside = truthy(
                &world
                    .resource_mut::<Runtime>()
                    .object_field(object, "completed_emerging_into_playable_area"),
            );
            if inside
                && state.route_traverse.first().copied().unwrap_or(false)
                && let Some(&end) = state.route.first()
            {
                state.route.remove(0);
                state.route_traverse.remove(0);
                state.traverse = Some(Traverse {
                    from: origin,
                    to: end,
                    start_ms: now,
                    ms: ledge_ms(origin, end),
                    window: false,
                    ledge: true,
                });
                play(world, object, state, pick(&TRAVERSE_CLIPS, object));
                if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
                    diag::info!(
                        Sim,
                        "actor debug: {object} traverses {:?} -> {:?}",
                        origin.map(|v| v as i32),
                        end.map(|v| v as i32)
                    );
                }
                return end;
            }
        } else {
            break;
        }
    }
    state.route.first().copied().unwrap_or(target)
}

fn actor_id(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let object = super::natives::engine::entity_id(world, receiver)?;
    if is_actor(world, object) {
        Ok(object)
    } else {
        Err("receiver is not an actor".into())
    }
}

fn with_actor(world: &mut World, object: u64, change: impl FnOnce(&mut Actor)) {
    if let Some(actor) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
        change(actor);
    }
}

fn vector_arg(args: &[Value], index: usize) -> Option<[f32; 3]> {
    match args.get(index) {
        Some(Value::Vector(v)) => Some(*v),
        _ => None,
    }
}

/// `setGoalPos(origin, [angles])`, `setGoalNode(node)` and `setGoalEntity(ent)`.
pub(crate) fn set_goal(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    how: &str,
) -> Result<Value, String> {
    let object = actor_id(world, receiver)?;
    let goal = match how {
        "pos" => Goal::Pos(vector_arg(args, 0).ok_or("setGoalPos takes an origin")?),
        "node" => {
            let node = match args.first() {
                Some(Value::Object(node)) => *node,
                _ => return Err("setGoalNode takes a node".into()),
            };
            Goal::Pos(vec_field(
                &mut world.resource_mut::<Runtime>(),
                node,
                "origin",
            ))
        }
        _ => match args.first() {
            Some(Value::Object(target)) => Goal::Entity(*target),
            _ => return Err("setGoalEntity takes an entity".into()),
        },
    };
    let yaw = vector_arg(args, 1).map(|angles| angles[1]);
    with_actor(world, object, |actor| {
        actor.goal = Some(goal);
        actor.goal_yaw = yaw;
        actor.goal_reached = false;
    });
    Ok(Value::Undefined)
}

/// `animScripted(notify, origin, angles, anim, ...)`: the clip plays from that spot, its
/// notetracks go out as `notify` with the notetrack's name, and `end` last.
pub(crate) fn anim_scripted(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let object = actor_id(world, receiver)?;
    let notify: std::sync::Arc<str> = match args.first() {
        Some(Value::String(name)) => name.to_string().into(),
        _ => return Err("animScripted takes a notify name".into()),
    };
    let clip = match args.get(3) {
        Some(Value::Animation { name, .. }) => name.to_string(),
        _ => return Err("animScripted takes an animation".into()),
    };
    let now = super::players::now_ms(world);
    let frame = FrameWorld::from_world(world);
    let length_ms = frame
        .script_model_anim(&clip)
        .filter(|timing| timing.frequency > 0.0)
        .map_or(1000, |timing| (1000.0 / timing.frequency) as i64);
    let notes = frame.anim_notes(&clip);
    {
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(origin) = vector_arg(args, 1) {
            runtime.set_object_field(object, "origin", Value::Vector(origin));
        }
        if let Some(angles) = vector_arg(args, 2) {
            runtime.set_object_field(object, "angles", Value::Vector([0.0, angles[1], 0.0]));
        }
    }
    let previous = world
        .resource::<Runtime>()
        .actors
        .get(&object)
        .and_then(|actor| actor.scripted.clone());
    if let Some(previous) = previous {
        raise(
            world,
            Value::Object(object),
            &previous.notify,
            vec![Value::string("end")],
        );
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let dog = runtime
        .entities
        .get(&object)
        .is_some_and(|entity| entity.classname.contains("dog"));
    if let Some(entity) = runtime.entities.get_mut(&object).filter(|_| !dog) {
        entity.anim_op = Some(Some(clip.as_str().into()));
    }
    if let Some(actor) = runtime.actors.get_mut(&object) {
        actor.clip = None;
        actor.scripted = Some(Scripted {
            notify,
            start_ms: now,
            length_ms: length_ms.max(50),
            notes,
            fired: 0,
        });
    }
    Ok(Value::Undefined)
}

/// `stopAnimScripted()`: the clip ends now.
pub(crate) fn stop_anim_scripted(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let object = actor_id(world, receiver)?;
    let scripted = world
        .resource_mut::<Runtime>()
        .actors
        .get_mut(&object)
        .and_then(|actor| actor.scripted.take());
    if let Some(scripted) = scripted {
        raise(
            world,
            Value::Object(object),
            &scripted.notify,
            vec![Value::string("end")],
        );
    }
    Ok(Value::Undefined)
}

/// `forceTeleport(origin, [angles])`.
pub(crate) fn force_teleport(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let object = actor_id(world, receiver)?;
    let origin = vector_arg(args, 0).ok_or("forceTeleport takes an origin")?;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    if let Some(angles) = vector_arg(args, 1) {
        runtime.set_object_field(object, "angles", Value::Vector([0.0, angles[1], 0.0]));
    }
    Ok(Value::Undefined)
}

/// `calcPathLength(origin)`: the walk-graph route's length to there, or the straight
/// distance where the graph has no route.
pub(crate) fn path_length(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let object = actor_id(world, receiver)?;
    let to = vector_arg(args, 0).ok_or("calcPathLength takes an origin")?;
    let from = vec_field(&mut world.resource_mut::<Runtime>(), object, "origin");
    let straight =
        ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
    let routed = nav(world).and_then(|nav| nav.route(from, to)).map(|route| {
        route
            .windows(2)
            .map(|pair| {
                (0..3)
                    .map(|i| (pair[1][i] - pair[0][i]).powi(2))
                    .sum::<f32>()
                    .sqrt()
            })
            .sum::<f32>()
    });
    Ok(Value::Float(routed.unwrap_or(straight).max(1.0)))
}

/// An animation's length and notetracks, for the notetrack queries on clips the player
/// tree does not hold.
pub(crate) fn clip_length(world: &mut World, name: &str) -> Option<f32> {
    FrameWorld::from_world(world)
        .script_model_anim(name)
        .filter(|timing| timing.frequency > 0.0)
        .map(|timing| 1.0 / timing.frequency)
}

pub(crate) fn clip_notes(world: &mut World, name: &str) -> std::sync::Arc<[crate::AnimNote]> {
    FrameWorld::from_world(world).anim_notes(name)
}

/// One of a set of clips, the same one for an actor every time.
fn pick(clips: &[&'static str], object: u64) -> &'static str {
    clips[(object as usize) % clips.len()]
}

/// The gait BO1's scripts asked for (`self.zombie_move_speed`): its speed and clips.
fn gait(runtime: &mut Runtime, object: u64) -> (f32, &'static [&'static str]) {
    let speed = match runtime.object_field(object, "zombie_move_speed") {
        Value::String(speed) => speed.to_string(),
        _ => String::new(),
    };
    match speed.as_str() {
        "sprint" => (SPRINT_SPEED, &SPRINT_CLIPS),
        "run" => (RUN_SPEED, &RUN_CLIPS),
        _ => (WALK_SPEED, &WALK_CLIPS),
    }
}

/// Start `clip` on the actor's body unless it already plays it.
fn play(world: &mut World, object: u64, state: &mut Actor, clip: &'static str) {
    if state.clip == Some(clip) {
        return;
    }
    static UNTIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let timing = FrameWorld::from_world(world).script_model_anim(clip);
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        diag::info!(Sim, "actor debug: {object} plays {clip} {timing:?}");
    }
    if timing.is_none() && !UNTIMED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        diag::warn!(
            Sim,
            "actor: clip {clip} has no timing in the anim table; it holds its first frame"
        );
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let Some(entity) = runtime.entities.get_mut(&object) else {
        return;
    };
    if entity.classname.contains("dog") {
        return;
    }
    state.clip = Some(clip);
    entity.anim_op = Some(Some(clip.into()));
}

fn vec_field(runtime: &mut Runtime, object: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(object, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn int_field(runtime: &mut Runtime, object: u64, name: &str) -> i32 {
    match runtime.object_field(object, name) {
        Value::Int(n) => n,
        Value::Float(f) => f as i32,
        _ => 0,
    }
}

/// `spawner CodeSpawnerSpawn([noEnemyInfo], [targetname])` and `CodeSpawnerForceSpawn`.
pub(crate) fn spawn_from_spawner(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let spawner = super::natives::engine::entity_id(world, receiver)?;
    let (classname, origin, angles, model) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let classname = runtime.entities[&spawner].classname.to_string();
        let origin = vec_field(&mut runtime, spawner, "origin");
        let angles = vec_field(&mut runtime, spawner, "angles");
        let model = match runtime.object_field(spawner, "model") {
            Value::String(model) => model.to_string(),
            _ => String::new(),
        };
        (classname, origin, angles, model)
    };
    if !classname.starts_with("actor_") {
        return Err(format!("{classname} is not an actor spawner"));
    }
    let presence = super::presence::spawn_presence(world, origin)?;
    let number = FrameWorld::from_world(world).gentity_number(presence);
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.create_entity(EntityKind::Actor, &classname)?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0, angles[1], 0.0]));
    runtime.set_object_field(id, "model", Value::string(&model));
    runtime.set_object_field(id, "health", Value::Int(DEFAULT_HEALTH));
    runtime.set_object_field(id, "maxhealth", Value::Int(DEFAULT_HEALTH));
    runtime.set_object_field(id, "team", Value::string("axis"));
    let kind = if classname.contains("dog") {
        "dog"
    } else {
        "zombie"
    };
    runtime.set_object_field(id, "type", Value::string(kind));
    // Engine-owned actor fields the scripts read before they ever set them.
    runtime.set_object_field(id, "delayeddeath", Value::Int(0));
    runtime.set_object_field(id, "allowpain", Value::Int(1));
    runtime.set_object_field(id, "ignoreall", Value::Int(0));
    runtime.set_object_field(id, "ignoreme", Value::Int(0));
    runtime.set_object_field(id, "damagelocation", Value::string("none"));
    if let Some(Value::String(targetname)) = args.get(1) {
        runtime.set_object_field(id, "targetname", Value::String(targetname.clone()));
    }
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.presence = Some(presence);
    entity.can_damage = true;
    if let Some(number) = number {
        entity.number = number;
    }
    runtime.actors.insert(
        id,
        Actor {
            home_z: origin[2],
            ..Actor::default()
        },
    );
    let living = runtime.actors.len();
    drop(runtime);
    diag::info!(
        Sim,
        "actor: spawned {classname} ({model}) at [{:.0}, {:.0}, {:.0}]; {living} actors",
        origin[0],
        origin[1],
        origin[2]
    );
    // The aitype's main sets what its scripts read (team, type, health, weapon) and the
    // character's models, as the engine runs it on every actor it spawns.
    let aitype = format!(
        "aitype/{}::main",
        classname.strip_prefix("actor_").unwrap_or(&classname)
    );
    let now = super::players::now_ms(world);
    let _ = run_now(world, &aitype, Value::Object(id), Vec::new(), now);
    // Then its init animscript, which builds the `self.a` state the AI scripts read.
    let init = if kind == "dog" {
        "animscripts/zombie_dog_init::main"
    } else {
        "animscripts/zombie_init::main"
    };
    let _ = run_now(world, init, Value::Object(id), Vec::new(), now);
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        let mut runtime = world.resource_mut::<Runtime>();
        let model = runtime.object_field(id, "model");
        let attachments = runtime.entities[&id].attachments.clone();
        drop(runtime);
        let mut has = |name: &str| {
            FrameWorld::from_world(world)
                .model_capability(name)
                .is_some_and(|c| c.is_some())
        };
        let body = match &model {
            Value::String(m) => m.to_string(),
            _ => String::new(),
        };
        diag::info!(
            Sim,
            "actor debug: model {body} capability={} attachments={attachments:?}",
            has(&body)
        );
    }
    raise(
        world,
        Value::Object(spawner),
        "spawned",
        vec![Value::Object(id)],
    );
    Ok(Value::Object(id))
}

/// Living actors, optionally only one team's (`getaiarray("axis")`).
pub(crate) fn living(world: &mut World, team: Option<&str>) -> Vec<Value> {
    let mut runtime = world.resource_mut::<Runtime>();
    let ids: Vec<u64> = runtime
        .actors
        .iter()
        .filter(|(_, actor)| actor.died_ms.is_none())
        .map(|(id, _)| *id)
        .filter(|id| runtime.live(id))
        .collect();
    ids.into_iter()
        .filter(|id| {
            team.is_none_or(|team| {
                matches!(runtime.object_field(*id, "team"), Value::String(t) if t.to_string() == team)
            })
        })
        .map(Value::Object)
        .collect()
}

pub(crate) fn is_actor(world: &World, object: u64) -> bool {
    world.resource::<Runtime>().actors.contains_key(&object)
}

/// Bullet, explosive or melee damage to an actor: BO1's actor damage callback decides
/// (it scores the hit and calls `finishActorDamage`).
pub(crate) fn damage(
    world: &mut World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
    attacker: Value,
    weapon: &str,
    part: &str,
) {
    if world
        .resource::<Runtime>()
        .actors
        .get(&object)
        .is_none_or(|actor| actor.died_ms.is_some())
    {
        return;
    }
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        diag::info!(
            Sim,
            "actor debug: {object} hit for {} by {weapon} ({}) at {part:?}",
            hit.amount,
            hit.means
        );
    }
    let hitloc = if part.starts_with("j_head") || part.starts_with("j_neck") {
        "head"
    } else {
        "torso_upper"
    };
    let args = vec![
        attacker.clone(),
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::string(weapon),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string(hitloc),
        Value::Int(0),
        Value::Int(0),
    ];
    let now = super::players::now_ms(world);
    if run_now(
        world,
        ACTOR_DAMAGE,
        Value::Object(object),
        args.clone(),
        now,
    )
    .is_err()
    {
        // No callback to decide: take the damage as given.
        finish(world, object, &args);
    }
}

/// `actor finishActorDamage(inflictor, attacker, damage, flags, means, weapon, point, dir,
/// hitloc, modelIndex, timeOffset)`.
pub(crate) fn finish_damage(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let object = super::natives::engine::entity_id(world, receiver)?;
    if !is_actor(world, object) {
        return Err("finishActorDamage: receiver is not an actor".into());
    }
    finish(world, object, args);
    Ok(Value::Undefined)
}

fn finish(world: &mut World, object: u64, args: &[Value]) {
    let amount = match args.get(2) {
        Some(Value::Int(n)) => *n,
        Some(Value::Float(f)) => *f as i32,
        _ => 0,
    };
    let attacker = args.get(1).cloned().unwrap_or(Value::Undefined);
    let get = |i: usize| args.get(i).cloned().unwrap_or(Value::Undefined);
    let (before, after) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let before = int_field(&mut runtime, object, "health");
        let after = before.saturating_sub(amount.max(0));
        runtime.set_object_field(object, "health", Value::Int(after));
        // What the engine records of the last hit, for the damage scripts.
        runtime.set_object_field(object, "damagelocation", get(8));
        runtime.set_object_field(object, "damageweapon", get(5));
        runtime.set_object_field(object, "damagemod", get(4));
        (before, after)
    };
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        diag::info!(Sim, "actor debug: {object} health {before} -> {after}");
    }
    // `damage`: amount, attacker, direction, point, means, model, tag, part, flags.
    raise(
        world,
        Value::Object(object),
        "damage",
        vec![
            Value::Int(amount),
            attacker.clone(),
            get(7),
            get(6),
            get(4),
            Value::string(""),
            Value::string(""),
            Value::string(""),
            get(3),
        ],
    );
    if before > 0 && after <= 0 {
        kill(world, object, args, attacker);
    }
}

fn kill(world: &mut World, object: u64, args: &[Value], attacker: Value) {
    let now = super::players::now_ms(world);
    if let Some(actor) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
        if actor.died_ms.is_some() {
            return;
        }
        actor.died_ms = Some(now);
    }
    // The killer, as the engine leaves it on a dead actor (`zombie.attacker`).
    world
        .resource_mut::<Runtime>()
        .set_object_field(object, "attacker", attacker.clone());
    if let Some(mut state) = world.resource::<Runtime>().actors.get(&object).cloned() {
        play(world, object, &mut state, pick(&DEATH_CLIPS, object));
        if let Some(slot) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
            *slot = state;
        }
    }
    let get = |i: usize| args.get(i).cloned().unwrap_or(Value::Undefined);
    let killed = vec![
        get(0),
        attacker.clone(),
        get(2),
        get(4),
        get(5),
        get(7),
        get(8),
        Value::Int(0),
    ];
    let _ = run_now(world, ACTOR_KILLED, Value::Object(object), killed, now);
    raise(world, Value::Object(object), "death", vec![attacker]);
    // The death animscript: a zombie's `deathFunction` hands out the kill points there.
    let dog = world
        .resource::<Runtime>()
        .entities
        .get(&object)
        .is_some_and(|entity| entity.classname.contains("dog"));
    let death = if dog {
        "animscripts/zombie_dog_death::main"
    } else {
        "animscripts/zombie_death::main"
    };
    let _ = run_now(world, death, Value::Object(object), Vec::new(), now);
}

/// GSC truth: a defined, non-zero value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Int(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::Undefined => false,
        _ => true,
    }
}

/// A zombie that has torn its window open climbs in: BO1 sets `got_to_entrance` at the
/// window and clears `ignoreall` once the boards are down, then waits for its pathing to
/// carry it over the sill (`zombie_start_traverse` / `zombie_end_traverse`).
fn window_climb(runtime: &mut Runtime, object: u64, origin: [f32; 3]) -> Option<Traverse> {
    if !truthy(&runtime.object_field(object, "got_to_entrance"))
        || truthy(&runtime.object_field(object, "ignoreall"))
        || truthy(&runtime.object_field(object, "completed_emerging_into_playable_area"))
    {
        return None;
    }
    let Value::Object(node) = runtime.object_field(object, "first_node") else {
        return None;
    };
    let at = vec_field(runtime, node, "origin");
    let yaw = vec_field(runtime, node, "angles")[1].to_radians();
    let to = [
        at[0] + yaw.cos() * WINDOW_HOP,
        at[1] + yaw.sin() * WINDOW_HOP,
        at[2],
    ];
    Some(Traverse {
        from: origin,
        to,
        start_ms: 0,
        ms: TRAVERSE_MS,
        window: true,
        ledge: false,
    })
}

/// Each tick: play out scripted clips and window climbs, walk every other living actor at
/// its goal (or the nearest living player when its scripts set none) and swing at players
/// in reach; delete bodies whose time is up.
pub(crate) fn advance(world: &mut World) {
    if world.resource::<Runtime>().actors.is_empty() {
        return;
    }
    let now = super::players::now_ms(world);
    let targets: Vec<(crate::ClientId, [f32; 3])> = {
        let runtime = world.resource::<Runtime>();
        // A player BO1 sets `ignoreme` on (downed, waiting for a revive) is left alone.
        let clients: Vec<u32> = runtime
            .players
            .iter()
            .filter(|(_, slot)| !runtime.peek_field(slot.object, "ignoreme").is_some_and(truthy))
            .map(|(client, _)| *client)
            .collect();
        let frame = FrameWorld::from_world(world);
        clients
            .into_iter()
            .map(crate::ClientId)
            .filter(|id| {
                frame
                    .client_meta(*id)
                    .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
            })
            .filter_map(|id| Some((id, frame.player(id)?.origin)))
            .collect()
    };
    let actors: Vec<u64> = world.resource::<Runtime>().actors.keys().copied().collect();
    let mut expired = Vec::new();
    for object in actors {
        let state = world.resource::<Runtime>().actors.get(&object).cloned();
        let Some(mut state) = state else { continue };
        if !world.resource::<Runtime>().live(&object) {
            world.resource_mut::<Runtime>().actors.remove(&object);
            continue;
        }
        if let Some(died) = state.died_ms {
            if now - died >= CORPSE_MS {
                expired.push(object);
            }
            continue;
        }
        think(world, object, &mut state, &targets, now);
        if let Some(slot) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
            // A death during the think (it fell out, a script killed it) stands.
            let died = slot.died_ms;
            *slot = state;
            slot.died_ms = slot.died_ms.or(died);
        }
    }
    for object in expired {
        world.resource_mut::<Runtime>().actors.remove(&object);
        let mut runtime = world.resource_mut::<Runtime>();
        if !runtime.pending_deletes.contains(&object) {
            runtime.pending_deletes.push(object);
        }
    }
}

fn think(
    world: &mut World,
    object: u64,
    state: &mut Actor,
    targets: &[(crate::ClientId, [f32; 3])],
    now: i64,
) {
    if let Some(scripted) = state.scripted.as_mut() {
        let elapsed = now - scripted.start_ms;
        let at = elapsed as f32 / scripted.length_ms as f32;
        let mut fire = Vec::new();
        while let Some(note) = scripted
            .notes
            .get(scripted.fired)
            .filter(|note| note.time <= at)
        {
            if &*note.name != "end" {
                fire.push(note.name.clone());
            }
            scripted.fired += 1;
        }
        let notify = scripted.notify.clone();
        let done = elapsed >= scripted.length_ms;
        for note in fire {
            raise(
                world,
                Value::Object(object),
                &notify,
                vec![Value::String(note.into())],
            );
        }
        if done {
            state.scripted = None;
            raise(
                world,
                Value::Object(object),
                &notify,
                vec![Value::string("end")],
            );
        }
        return;
    }
    let origin = vec_field(&mut world.resource_mut::<Runtime>(), object, "origin");
    if state.traverse.is_none() && !state.climbed {
        let climb = window_climb(&mut world.resource_mut::<Runtime>(), object, origin);
        if let Some(mut climb) = climb {
            climb.start_ms = now;
            state.traverse = Some(climb);
            state.climbed = true;
            play(world, object, state, pick(&TRAVERSE_CLIPS, object));
            raise(
                world,
                Value::Object(object),
                "zombie_start_traverse",
                Vec::new(),
            );
        }
    }
    if let Some(climb) = state.traverse.clone() {
        let t = ((now - climb.start_ms) as f32 / climb.ms.max(1) as f32).clamp(0.0, 1.0);
        let at = climb.at(t);
        let yaw = (climb.to[1] - climb.from[1]).atan2(climb.to[0] - climb.from[0]);
        {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(object, "origin", Value::Vector(at));
            runtime.set_object_field(
                object,
                "angles",
                Value::Vector([0.0, yaw.to_degrees(), 0.0]),
            );
        }
        if t >= 1.0 {
            state.traverse = None;
            state.fall = 0.0;
            state.stuck_since = None;
            if climb.window {
                raise(
                    world,
                    Value::Object(object),
                    "zombie_end_traverse",
                    Vec::new(),
                );
            }
        }
        return;
    }

    // Where it is headed: its goal, else (no scripts steering it) the nearest player.
    let nearest = targets
        .iter()
        .min_by(|a, b| flat_distance(origin, a.1).total_cmp(&flat_distance(origin, b.1)))
        .copied();
    let goal = match &state.goal {
        Some(Goal::Pos(at)) => Some(*at),
        Some(Goal::Entity(target)) => {
            let target = *target;
            let mut runtime = world.resource_mut::<Runtime>();
            runtime
                .live(&target)
                .then(|| vec_field(&mut runtime, target, "origin"))
        }
        None => None,
    };
    let headed = goal.or(nearest.map(|(_, at)| at));
    let radius = match world
        .resource_mut::<Runtime>()
        .object_field(object, "goalradius")
    {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => DEFAULT_GOAL_RADIUS,
    }
    .max(1.0);

    // In the playable area (`ignoreall` cleared), a player in reach gets swung at.
    let in_play = !truthy(
        &world
            .resource_mut::<Runtime>()
            .object_field(object, "ignoreall"),
    );
    let attacking = in_play
        && nearest.is_some_and(|(_, at)| {
            flat_distance(origin, at) <= REACH && (at[2] - origin[2]).abs() < 72.0
        });

    let (speed, clips) = gait(&mut world.resource_mut::<Runtime>(), object);
    let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
    let mut yaw = None;
    let mut walk = [0.0, 0.0];
    let mut steering = None;
    if let Some((_, at)) = nearest.filter(|_| attacking) {
        yaw = Some((at[1] - origin[1]).atan2(at[0] - origin[0]).to_degrees());
        if !state.clip.is_some_and(|clip| ATTACK_CLIPS.contains(&clip)) {
            play(world, object, state, pick(&ATTACK_CLIPS, object));
        }
    } else if let Some(at) = headed {
        let to = [at[0] - origin[0], at[1] - origin[1]];
        let distance = (to[0] * to[0] + to[1] * to[1]).sqrt();
        let arrive = if goal.is_some() { radius } else { REACH };
        // On its goal's floor too (the engine's goal height): a zombie upstairs right
        // above its window has not reached it; it walks on and, when no route leads
        // there, raises `bad_path` so BO1 picks another window.
        let level = (at[2] - origin[2]).abs() <= GOAL_HEIGHT;
        if distance > arrive || !level {
            let via = steer_point(world, object, state, origin, at, now);
            steering = Some(via);
            let to = [via[0] - origin[0], via[1] - origin[1]];
            let leg = (to[0] * to[0] + to[1] * to[1]).sqrt().max(0.001);
            let step = speed.min(leg / dt);
            walk = [to[0] / leg * step, to[1] / leg * step];
            yaw = Some(to[1].atan2(to[0]).to_degrees());
            play(world, object, state, pick(clips, object));
        } else {
            play(world, object, state, "ai_zombie_idle_v1");
            if goal.is_some() && !state.goal_reached {
                state.goal_reached = true;
                raise(world, Value::Object(object), "goal", Vec::new());
                if let Some(goal_yaw) = state.goal_yaw {
                    yaw = Some(goal_yaw);
                    raise(world, Value::Object(object), "orientdone", Vec::new());
                }
            }
        }
    }
    let mut slide = super::mechanics::Slide {
        center: [0.0, 0.0, RADIUS + 1.0],
        radius: RADIUS,
        velocity: [walk[0], walk[1], state.fall],
    };
    let moved = crate::step::actor_slide(world, object, origin, &mut slide);
    state.fall = slide.velocity[2];
    if moved[2] < state.home_z - FELL_OUT {
        diag::warn!(
            Sim,
            "actor: {object} fell out of the map at [{:.0}, {:.0}, {:.0}]; it dies",
            moved[0],
            moved[1],
            moved[2]
        );
        world
            .resource_mut::<Runtime>()
            .set_object_field(object, "health", Value::Int(0));
        kill(world, object, &[], Value::Undefined);
        return;
    }
    // No headway on a walk: hop to where it steers if that is near and in sight (a rail,
    // a ledge, a step too tall), else plan the route again.
    let wanted = (walk[0] * walk[0] + walk[1] * walk[1]).sqrt() * dt;
    if wanted > 1.0 && flat_distance(origin, moved) < wanted * 0.25 {
        let since = *state.stuck_since.get_or_insert(now);
        if now - since >= STUCK_MS {
            state.stuck_since = None;
            state.route.clear();
            state.replan_ms = 0;
            if let Some(via) = steering.filter(|via| {
                flat_distance(moved, *via) < HOP_REACH && (via[2] - moved[2]).abs() < 300.0
            }) && in_sight(world, object, moved, via)
            {
                let ms = ((flat_distance(moved, via) / speed) * 1000.0) as i64;
                state.traverse = Some(Traverse {
                    from: moved,
                    to: via,
                    start_ms: now,
                    ms: ms.clamp(400, 1500),
                    window: false,
                    ledge: true,
                });
            }
        }
    } else {
        state.stuck_since = None;
    }
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some()
        && now % 2000 < i64::from(crate::MATCH_TICK_MS)
    {
        diag::info!(
            Sim,
            "actor debug: {object} at [{:.0}, {:.0}, {:.0}] headed {headed:?} goal {:?} reached {} in play {in_play} attacking {attacking}",
            moved[0],
            moved[1],
            moved[2],
            state.goal,
            state.goal_reached,
        );
    }
    {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.set_object_field(object, "origin", Value::Vector(moved));
        if let Some(yaw) = yaw {
            runtime.set_object_field(object, "angles", Value::Vector([0.0, yaw, 0.0]));
        }
    }
    if let Some((client, _)) = nearest.filter(|_| attacking && now >= state.next_hit_ms) {
        state.next_hit_ms = now + HIT_INTERVAL_MS;
        // Attack clips play once: each swing starts one again.
        state.clip = None;
        play(
            world,
            object,
            state,
            pick(&ATTACK_CLIPS, object ^ now as u64),
        );
        super::players::actor_hit_player(world, client, object, HIT_DAMAGE, moved);
    }
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
