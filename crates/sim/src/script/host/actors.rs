//! BO1 AI actors (zombies). An `actor_*` spawner's `CodeSpawnerSpawn` makes a script
//! model with the spawner's model and runs the engine's actor scripts on it (aitype, init
//! animscript). BO1's own scripts then drive it through the AI natives here: a goal
//! (`setGoalPos`/`Node`/`Entity`, answered with `goal`), `animScripted` (its notetracks
//! and `end`), and the window climb its pathing would do. Each tick it walks straight at
//! its goal with monster collision (sliding along walls) and, once in the playable area,
//! swings at players in reach. Damage goes through BO1's `CodeCallback_ActorDamage`, whose
//! scripts score the hit and call `finishActorDamage`; at zero health it runs
//! `CodeCallback_ActorKilled`, the `death` notify and the death animscript, whose
//! `startRagdoll` hands the body to the client's ragdoll. The body then stays on the floor
//! in one of BO1's `ai_corpseCount` corpse slots, and is taken away a few seconds later.

use bevy_ecs::prelude::World;

use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Runtime, Value};

const ACTOR_DAMAGE: &str = "maps/_callbacksetup::codecallback_actordamage";
const ACTOR_KILLED: &str = "maps/_callbacksetup::codecallback_actorkilled";
const MODEL_ANIM_SCRIPTED: &str = "iw4l_zombies/hud::model_anim_scripted";
const NOTIFY_AFTER: &str = "iw4l_zombies/hud::notify_after";

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
const DEATH_CLIPS: [&str; 2] = ["ai_zombie_death_v1", "ai_zombie_death_v2"];
/// BO1's swings (`level._zombie_melee`), and the ones a walker or a runner adds, which
/// keep it stepping forward (`_zombie_walk_melee`, `_zombie_run_melee`).
const MELEE_CLIPS: [&str; 6] = [
    "ai_zombie_attack_v2",
    "ai_zombie_attack_v4",
    "ai_zombie_attack_v6",
    "ai_zombie_attack_v1",
    "ai_zombie_attack_forward_v1",
    "ai_zombie_attack_forward_v2",
];
const WALK_MELEE_CLIPS: [&str; 4] = [
    "ai_zombie_walk_attack_v1",
    "ai_zombie_walk_attack_v2",
    "ai_zombie_walk_attack_v3",
    "ai_zombie_walk_attack_v4",
];
const RUN_MELEE_CLIPS: [&str; 3] = [
    "ai_zombie_run_attack_v1",
    "ai_zombie_run_attack_v2",
    "ai_zombie_run_attack_v3",
];
/// BO1's hellhound clips (`anim.dogAnims["zombie"]` in animscripts/zombie_dog_init).
const DOG_RUN_CLIPS: [&str; 1] = ["zombie_dog_run"];
const DOG_IDLE: &str = "zombie_dog_idle";
const DOG_BITE: &str = "zombie_dog_run_attack";
/// By BO1's `getAnimDirection(damageyaw)`: front, right, back, left.
const DOG_DEATH_CLIPS: [&str; 4] = [
    "zombie_dog_death_front",
    "zombie_dog_death_hit_right",
    "zombie_dog_death_hit_back",
    "zombie_dog_death_hit_left",
];
const DOG_JUMP_DOWN_CLIPS: [(f32, &str); 3] = [
    (40.0, "zombie_dog_traverse_down_40"),
    (126.0, "zombie_dog_traverse_down_126"),
    (190.0, "zombie_dog_traverse_down_190"),
];
const DOG_JUMP_UP_CLIPS: [(f32, &str); 2] = [
    (40.0, "zombie_dog_traverse_up_40"),
    (80.0, "zombie_dog_traverse_up_80"),
];
const DOG_MANTLE_CLIPS: [&str; 1] = ["zombie_dog_run_jump_window_40"];
/// A hellhound bites a player within its `meleeAttackDist` (BO1 sets 64, and
/// `meleeDamage` 40, on its dogs); its bite clip stays in place.
const DOG_MELEE_ATTACK_DIST: f32 = 64.0;
const DOG_BITE_DAMAGE: i32 = 40;
/// A Nova crawler's idles (`quad_prespawn`'s `idleAnimOverrideArray`).
const QUAD_IDLE_CLIPS: [&str; 2] = ["ai_zombie_quad_idle", "ai_zombie_quad_idle_2"];
/// A crawler's traversals (`animscripts/traverse/*`'s `_quad` branches): its own drop
/// for `zombie_jump_down_120`, a zombie's wall hop for mantles, and the wall crawl's
/// intro down a wall.
const QUAD_JUMP_DOWN_120: &str = "ai_zombie_quad_jump_down_128";
const QUAD_MANTLE: &str = "ai_zombie_traverse_v3";
const QUAD_WALL_CRAWL: &str = "ai_zombie_quad_crawl_sprint_wall_intro";
/// How tall each kind stands, for touch aim assist's target boxes.
const ZOMBIE_HEIGHT: f32 = 64.0;
const DOG_HEIGHT: f32 = 32.0;
const QUAD_HEIGHT: f32 = 28.0;
/// BO1's least push, a frame (`ai` push: 1.5 units each 50 ms frame).
const PUSH_MIN_STEP: f32 = 1.5;
/// Where it stops walking at a player (from feet to feet, horizontally).
const REACH: f32 = 52.0;
/// BO1 swings at a player this close (`anim.meleeRange`), and its hit lands only if the
/// player is still within it (to the edge of their body) when the swing's "fire" note
/// comes; BO1's `ai_meleeRange`.
const MELEE_RANGE: f32 = 64.0;
/// A player's collision half-width: BO1's melee box stops at the body's edge.
const PLAYER_RADIUS: f32 = 15.0;
/// A swing that steps forward stops short of this (it does not walk into the player).
const MELEE_CLOSEST: f32 = 30.0;
const HIT_DAMAGE: i32 = 50;
/// The collision cube's half size; its centre sits this far above the feet.
const RADIUS: f32 = 15.0;
/// A killed actor stays an actor this long (its death animscript), then its body becomes
/// a corpse.
const CORPSE_MS: i64 = 1500;
/// A body is taken away this long after its death. BO1 keeps a body until its
/// `ai_corpseCount` slot is wanted for a newer one; ours also go once they have lain
/// a while, so the floor does not stay strewn with them.
const CORPSE_STAY_MS: i64 = 6500;
/// A body is never taken away sooner than this after its ragdoll started, so it does not
/// vanish mid-fall (a ragdoll comes to rest in a second or two).
const CORPSE_SETTLE_MS: i64 = 4000;
/// How many bodies of the dead stay on the floor: BO1's `ai_corpseCount` (5, its
/// default and the menu's "medium").
const AI_CORPSE_COUNT: usize = 5;
const DEFAULT_HEALTH: i32 = 150;

/// `animscripts/traverse/zombie_mantle_over_40` (windows, low walls) by
/// `zombie_move_speed`, and a crawler's.
const MANTLE_WALK_CLIPS: [&str; 2] = ["ai_zombie_traverse_v1", "ai_zombie_traverse_v2"];
const MANTLE_RUN_CLIPS: [&str; 1] = ["ai_zombie_traverse_v5"];
const MANTLE_SPRINT_CLIPS: [&str; 2] = ["ai_zombie_traverse_v6", "ai_zombie_traverse_v7"];
const MANTLE_CRAWL_CLIPS: [&str; 2] = ["ai_zombie_traverse_crawl_v1", "ai_zombie_traverse_v4"];
/// BO1's jumps down by the drop they were made for (`animscripts/traverse/
/// zombie_jump_down_<height>`), a zombie's and a crawler's. A map's zones hold only some:
/// a jump plays the nearest one loaded, scaled to its drop.
const JUMP_DOWN_CLIPS: [(f32, &str); 10] = [
    (40.0, "ai_zombie_jump_down_40"),
    (72.0, "ai_zombie_jump_down_72"),
    (96.0, "ai_zombie_jump_down_96"),
    (120.0, "ai_zombie_jump_down_120"),
    (127.0, "ai_zombie_jump_down_127"),
    (184.0, "ai_zombie_jump_down_184"),
    (190.0, "ai_zombie_jump_down_190"),
    (222.0, "ai_zombie_jump_down_222"),
    (240.0, "ai_zombie_jump_down_240"),
    (286.0, "ai_zombie_jump_down_286"),
];
const CRAWL_JUMP_DOWN_CLIPS: [(f32, &str); 9] = [
    (72.0, "ai_zombie_crawl_jump_down_72"),
    (97.0, "ai_zombie_crawl_jump_down_97"),
    (120.0, "ai_zombie_crawl_jump_down_120"),
    (127.0, "ai_zombie_crawl_jump_down_127"),
    (184.0, "ai_zombie_crawl_jump_down_184"),
    (189.0, "ai_zombie_crawl_jump_down_189"),
    (222.0, "ai_zombie_crawl_jump_down_222"),
    (240.0, "ai_zombie_crawl_jump_down_240"),
    (286.0, "ai_zombie_crawl_jump_down_286"),
];
/// Jumps up (`zombie_jump_up_222`) by the climb they were made for.
const JUMP_UP_CLIPS: [(f32, &str); 2] = [
    (222.0, "ai_zombie_jump_up_222"),
    (98.0, "ai_zombie_jump_up_2_climb"),
];
const CRAWL_JUMP_UP_CLIPS: [(f32, &str); 2] = [
    (222.0, "ai_zombie_crawl_jump_up_222"),
    (97.0, "ai_zombie_crawl_jump_up_2_climb"),
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

/// An `animScripted` clip: the actor holds its place while the notetracks fire, or with
/// root motion (a rise out of the ground) moves along it from where the clip started.
#[derive(Clone, Debug)]
struct Scripted {
    notify: std::sync::Arc<str>,
    start_ms: i64,
    length_ms: i64,
    notes: std::sync::Arc<[crate::AnimNote]>,
    fired: usize,
    /// The clip's root motion, the origin it starts from and the yaw it plays facing.
    motion: Option<(std::sync::Arc<[[f32; 3]]>, [f32; 3], f32)>,
}

/// A swing at a player: its clip, when it started and how long it runs, and when (shares
/// of the clip) its "fire" notes come, where BO1's `melee()` lands the hit.
#[derive(Clone, Debug)]
struct Swing {
    clip: &'static str,
    start_ms: i64,
    length_ms: i64,
    fires: Vec<f32>,
    fired: usize,
    target: crate::ClientId,
}

/// A clip name a script handed over, as the `&'static str` an actor's state keeps: each
/// distinct name is kept once (a map uses a few dozen).
fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(known) = names.iter().find(|known| known.eq_ignore_ascii_case(name)) {
        return known;
    }
    let kept: &'static str = Box::leak(name.to_ascii_lowercase().into_boxed_str());
    names.push(kept);
    kept
}

/// How fast a clip's root motion carries the body along (units a second) at `phase`
/// (0..1) through it, over a short stretch so one sample doesn't jitter: BO1 moves a
/// zombie by its clip, so its feet stay planted.
fn root_speed(world: &mut World, clip: &str, phase: f32) -> Option<f32> {
    let frame = FrameWorld::from_world(world);
    let samples = frame.anim_delta(clip)?;
    let length = frame
        .script_model_anim(clip)
        .filter(|timing| timing.frequency > 0.0)
        .map(|timing| 1.0 / timing.frequency)?;
    let last = samples.len().checked_sub(1).filter(|last| *last >= 2)?;
    let at = phase.rem_euclid(1.0) * last as f32;
    let i = (at as usize).min(last - 1);
    let (lo, hi) = (i.saturating_sub(1), (i + 2).min(last));
    let dx = samples[hi][0] - samples[lo][0];
    let dy = samples[hi][1] - samples[lo][1];
    let span = (hi - lo) as f32 / last as f32 * length;
    (span > 0.0).then(|| (dx * dx + dy * dy).sqrt() / span)
}

/// Where root motion `samples` (sampled evenly over a clip) has moved it `t` (0..1)
/// through the clip, from where the clip started.
fn motion_sample(samples: &[[f32; 3]], t: f32) -> [f32; 3] {
    let last = samples.len() - 1;
    if last == 0 {
        return [0.0; 3];
    }
    let at = t.clamp(0.0, 1.0) * last as f32;
    let i = (at as usize).min(last - 1);
    let f = at - i as f32;
    std::array::from_fn(|k| {
        let a = samples[i][k] - samples[0][k];
        let b = samples[i + 1][k] - samples[0][k];
        a + (b - a) * f
    })
}

/// Carried through a window: from where it stood to where it lands inside.
#[derive(Clone, Debug)]
struct Traverse {
    from: [f32; 3],
    to: [f32; 3],
    start_ms: i64,
    ms: i64,
    /// Through its window (BO1 waits on the traverse notifies), not a traversal link.
    window: bool,
    /// Over a ledge (a traversal link): up before across when climbing, across
    /// before down when dropping, so it doesn't pass through the ledge's corner.
    ledge: bool,
    /// Carried by its clip's root motion (sampled over the clip), played facing this yaw
    /// (radians): BO1 plays a traverse animscript's clip from the link's start node and
    /// lets the clip move it. What the clip falls short of `to` is made up along the way.
    motion: Option<(std::sync::Arc<[[f32; 3]]>, f32)>,
}

impl Traverse {
    /// Where it is `t` (0..1) of the way through.
    fn at(&self, t: f32) -> [f32; 3] {
        if let Some((samples, yaw)) = &self.motion
            && samples.len() >= 2
        {
            return self.along_motion(samples, *yaw, t);
        }
        let mut at: [f32; 3] =
            std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * t);
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

    /// Where the clip's root motion has it `t` of the way through: the clip's delta
    /// turned to `yaw`, its height scaled to the link's when the clip climbs or drops the
    /// same way, and the rest of the gap to `to` closed as the clip moves across.
    fn along_motion(&self, samples: &[[f32; 3]], yaw: f32, t: f32) -> [f32; 3] {
        let last = samples.len() - 1;
        let sample = |t: f32| motion_sample(samples, t);
        let (sin, cos) = yaw.sin_cos();
        let turn = |v: [f32; 3]| [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos, v[2]];
        let flat_len =
            |a: [f32; 3], b: [f32; 3]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        // How far across the clip has come by `t`: the share of the gap made up by then.
        let path: f32 = (0..last)
            .map(|i| flat_len(samples[i], samples[i + 1]))
            .sum();
        let across = if path > 8.0 {
            let whole = t.clamp(0.0, 1.0) * last as f32;
            let full = whole as usize;
            let mut done: f32 = (0..full.min(last))
                .map(|i| flat_len(samples[i], samples[i + 1]))
                .sum();
            if full < last {
                done += flat_len(samples[full], samples[full + 1]) * (whole - full as f32);
            }
            (done / path).clamp(0.0, 1.0)
        } else {
            t.clamp(0.0, 1.0)
        };
        let now = turn(sample(t));
        let end = turn(sample(1.0));
        let rise = self.to[2] - self.from[2];
        let z = if end[2].abs() > 16.0
            && rise * end[2] > 0.0
            && (0.5..2.0).contains(&(rise / end[2]))
        {
            self.from[2] + now[2] * (rise / end[2])
        } else {
            self.from[2] + now[2] + (rise - end[2]) * across
        };
        [
            self.from[0] + now[0] + (self.to[0] - self.from[0] - end[0]) * across,
            self.from[1] + now[1] + (self.to[1] - self.from[1] - end[1]) * across,
            z,
        ]
    }
}

/// How long a traversal from `from` to `to` takes.
fn ledge_ms(from: [f32; 3], to: [f32; 3]) -> i64 {
    let rise = (to[2] - from[2]).abs();
    (500.0 + flat_distance(from, to) * 3.0 + rise * 4.0).clamp(500.0, 2500.0) as i64
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Actor {
    /// The swing in progress (BO1's `MeleeCombat`), if any.
    swing: Option<Swing>,
    /// When the body's clip started, for where it is in its cycle.
    clip_start_ms: i64,
    fall: f32,
    died_ms: Option<i64>,
    /// The death clip its death animscript started (`setFlaggedAnimKnobAll("deathanim",
    /// ...)`), once dead.
    death_clip: Option<&'static str>,
    /// The clip the body plays now.
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
    /// Where it faces and walks, turning toward where it steers at a limited rate.
    heading: Option<f32>,
    /// When to look ahead along its route again (`LOOKAHEAD`).
    lookahead_ms: i64,
    /// BO1's look-ahead amount (the area a cut across the path may leave out): 0 until
    /// first used, then cut down where a cut is blocked and grown back as it walks.
    lookahead_amount: f32,
    /// The look-ahead point last worked out, for which next node (kept `LOOKAHEAD_MS`).
    lookahead_point: Option<([f32; 3], [f32; 3])>,
    /// Route nodes it got stuck heading straight for: no route starts at them again until
    /// it reaches a node.
    blocked_cuts: Vec<[f32; 3]>,
    /// The route node it is closing on round a corner, the nearest it has come to it and
    /// since when (`NO_CLOSER_MS`).
    closing: Option<([f32; 3], f32, i64)>,
    /// The height it spawned at: far below that it has fallen out of the map.
    home_z: f32,
    /// Where it left the floor, while it falls (IW4L_ACTOR_DEBUG logs big drops).
    airborne_from: Option<[f32; 3]>,
}

impl Actor {
    /// Killed: its body is on its way out.
    pub(crate) fn is_dead(&self) -> bool {
        self.died_ms.is_some()
    }
}

/// How far above or below its goal an actor may be and still be at it.
const GOAL_HEIGHT: f32 = 80.0;

/// No headway for this long and the actor plans its route again, from a node it can walk
/// onto.
const STUCK_MS: i64 = 800;
/// After it gets stuck it follows its route node by node (no look-ahead) this long.
const STUCK_DIRECT_MS: i64 = 5000;
/// Fallen this far below its spawn: out of the map, so it dies.
const FELL_OUT: f32 = 1500.0;

/// Nothing solid between two points (the actor itself aside), by what stops an actor's
/// walk: BO1's AI mask (0x2820011, as its path look-ahead checks a point with). Monster
/// clip (the AI-only walls along Kino's banisters and ledges) stops it; player clip
/// doesn't. Without monster clip, zombies cut over stair rails to the next flight.
fn clear_between(world: &mut World, object: u64, a: [f32; 3], b: [f32; 3]) -> bool {
    const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
    const CONTENTS_MONSTERCLIP: u32 = 0x0002_0000;
    let ignore = super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
    matches!(
        super::natives::engine::entity_trace(
            world,
            a,
            b,
            (crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP)
                | CONTENTS_MONSTERCLIP,
            ignore,
        ),
        crate::bullet_collision::TraceOutcome::Miss { .. }
    )
}

/// Nothing of the world between two points (bodies aside): what a zombie's swing at a
/// player has to get past.
fn no_wall_between(world: &mut World, object: u64, a: [f32; 3], b: [f32; 3]) -> bool {
    const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
    const CONTENTS_BODY: u32 = 0x0200_0000;
    let ignore = super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
    matches!(
        super::natives::engine::entity_trace(
            world,
            a,
            b,
            crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP & !CONTENTS_BODY,
            ignore,
        ),
        crate::bullet_collision::TraceOutcome::Miss { .. }
    )
}

/// Floor an actor walks on (monster clip too: the ramp over a flight of stairs) within a
/// step of `at`'s height.
fn floor_near(world: &mut World, object: u64, at: [f32; 3]) -> bool {
    const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
    const CONTENTS_MONSTERCLIP: u32 = 0x0002_0000;
    let ignore = super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
    matches!(
        super::natives::engine::entity_trace(
            world,
            [at[0], at[1], at[2] + 24.0],
            [at[0], at[1], at[2] - 24.0],
            (crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP)
                | CONTENTS_MONSTERCLIP,
            ignore,
        ),
        crate::bullet_collision::TraceOutcome::Hit { .. }
    )
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
        near.first()
            .map(|&node| vec![(node, cost(node))])
            .unwrap_or_default()
    } else {
        seen
    }
}

/// The nodes a route may start at for an actor at `at`, as `route_starts` (inside) or
/// `graph_end` (on the way to its window) pick them, kept to those it can walk straight
/// onto (`walk_onto`) and not one it just got stuck heading for (`blocked`). A node in
/// sight past a door jamb or a stair's newel post is no start: heading for it the body
/// catches on the corner. When none is walkable, the nodes in sight as before.
fn walkable_starts(
    world: &mut World,
    object: u64,
    nav: &crate::actor_nav::ActorNav,
    at: [f32; 3],
    inside: bool,
    blocked: &[[f32; 3]],
) -> Vec<(usize, f32)> {
    let open = |node: usize| !blocked.contains(&nav.nodes[node]);
    if inside {
        let seen = route_starts(world, object, nav, at);
        let walkable: Vec<(usize, f32)> = seen
            .iter()
            .copied()
            .filter(|&(node, _)| open(node) && walk_onto(world, object, at, nav.nodes[node]))
            .collect();
        return if walkable.is_empty() { seen } else { walkable };
    }
    let near = nav.nearest_k(at, 6);
    let mut fallback = None;
    for node in near.iter().copied() {
        let n = nav.nodes[node];
        if !clear_between(
            world,
            object,
            [at[0], at[1], at[2] + 40.0],
            [n[0], n[1], n[2] + 40.0],
        ) {
            continue;
        }
        fallback.get_or_insert(node);
        if open(node) && walk_onto(world, object, at, n) {
            return vec![(node, 0.0)];
        }
    }
    fallback
        .or_else(|| near.first().copied())
        .map(|node| vec![(node, 0.0)])
        .unwrap_or_default()
}

/// Whether an actor at `at` could walk straight onto the floor under route node `node`.
fn walk_onto(world: &mut World, object: u64, at: [f32; 3], node: [f32; 3]) -> bool {
    if flat_distance(at, node) < 1.0 {
        return true;
    }
    // Nodes sit about 32 over the floor (more over a clip ramp on stairs).
    let floor = floor_below(world, object, node).unwrap_or(node[2] - 32.0);
    body_clear(world, object, at, node, (floor - at[2]).clamp(-64.0, 64.0))
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
    // In sight for its whole body (`body_clear`), not just lines through its middle: a low
    // wall between (the one along Kino's foyer window, outside) is clear at chest height,
    // and a wall's end by a spawn closet's way out clear of a line past it; walking
    // straight at the spot kept zombies against them.
    let close = flat_distance(origin, target) < 96.0
        && (origin[2] - target[2]).abs() < 40.0
        && body_clear(world, object, origin, target, target[2] - origin[2]);
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
        let starts = walkable_starts(world, object, &nav, origin, inside, &state.blocked_cuts);
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
        // Nearer on the map is not nearer by the way when the next node is out of sight:
        // at the top of a flight of stairs, the node at its foot is behind the rail.
        // Outside, keep it too when the next node is round a wall from here: the nearest
        // node can be the corner a zombie has to go by (Kino's foyer window), and heading
        // past it took it straight into the wall.
        let keep_entry = state.route.len() > 1
            && if inside {
                state.route_traverse.get(1).copied().unwrap_or(false)
                    || flat_distance(origin, state.route[1])
                        >= flat_distance(state.route[0], state.route[1])
                    || !walk_onto(world, object, origin, state.route[1])
            } else {
                let next = state.route[1];
                !walk_onto(world, object, origin, next)
            };
        if state.route.len() > 1 && !keep_entry {
            state.route.remove(0);
            state.route_traverse.remove(0);
        }
    }
    while let Some(next) = state.route.first() {
        // Nodes sit about 32 above the floor, more over the clip ramp on a flight of
        // stairs (Kino's back stairwell: 62), so one up to the goal height counts.
        let rise = next[2] - origin[2];
        let near = flat_distance(origin, *next);
        // Near enough, unless the way on is round a corner from here: then right up to it
        // first, or it cuts the corner into the wall (Kino's foyer window) or catches the
        // newel post at the top of a flight. A traversal's start is reached as it is.
        // Or as near as its body gets: a node closer to a rail or wall than its width (on
        // the turn of a curved flight) can't be stood on, and it would slide round it.
        let reached = near < NODE_REACHED
            && (near < NODE_REACHED_CORNER
                || state.route_traverse.get(1).copied().unwrap_or(false)
                || {
                    let on = state.route.get(1).copied().unwrap_or(target);
                    walk_onto(world, object, origin, on)
                }
                || match state.closing {
                    Some((node, best, since)) if node == *next => {
                        if near < best - 1.0 {
                            state.closing = Some((node, near, now));
                            false
                        } else {
                            now - since >= NO_CLOSER_MS
                        }
                    }
                    _ => {
                        state.closing = Some((*next, near, now));
                        false
                    }
                });
        if reached && rise < GOAL_HEIGHT && rise > -48.0 {
            let begin = *next;
            state.blocked_cuts.clear();
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
                let node = traverse_node(world, begin);
                let yaw = node.as_ref().map_or_else(
                    || (end[1] - origin[1]).atan2(end[0] - origin[0]),
                    |(_, yaw, _)| *yaw,
                );
                let script = node.as_ref().map(|(script, ..)| script.as_str());
                let clip = traverse_clip(world, object, script, origin, end, now);
                start_traverse(world, object, state, origin, end, yaw, clip, now, false);
                return end;
            }
        } else {
            break;
        }
    }
    // Worked out again every `LOOKAHEAD_MS`, or when the route moves on; none while it
    // goes node by node after getting stuck.
    if now < state.lookahead_ms {
        match (state.lookahead_point, state.route.first()) {
            (Some((point, for_node)), Some(&next)) if for_node == next => return point,
            (None, Some(&next)) => return next,
            _ => {}
        }
    }
    let point = path_lookahead(world, object, state, origin, target)
        .or_else(|| state.route.first().copied())
        .unwrap_or(target);
    state.lookahead_ms = now + LOOKAHEAD_MS;
    state.lookahead_point = state.route.first().map(|&next| (point, next));
    point
}

/// BO1's look-ahead amount: at most this much area may be cut off the path (square
/// units), never less than the least.
const LOOKAHEAD_AREA_MAX: f32 = 65536.0;
const LOOKAHEAD_AREA_MIN: f32 = 1024.0;

/// Where to steer, as BO1 follows a path (`Path_CalcLookahead` and its helpers, read
/// from its executable): a point on the path itself, as far along as the area between
/// the straight way to it and the path stays under the look-ahead amount, so it cuts a
/// corner only that much; and only if the straight way there is clear (BO1's AI mask,
/// monster clip included), else with the amount cut down. Never past a traversal's start.
/// Passed nodes go first.
fn path_lookahead(
    world: &mut World,
    object: u64,
    state: &mut Actor,
    origin: [f32; 3],
    target: [f32; 3],
) -> Option<[f32; 3]> {
    // Passed nodes: it is beyond one, toward the next, and near it.
    while state.route.len() >= 2
        && !state.route_traverse.get(1).copied().unwrap_or(false)
    {
        let (a, b) = (state.route[0], state.route[1]);
        let along = (b[0] - a[0]) * (origin[0] - a[0]) + (b[1] - a[1]) * (origin[1] - a[1]);
        if along > 0.0
            && flat_distance(origin, a) < 96.0
            && body_clear(world, object, origin, b, 0.0)
        {
            state.route.remove(0);
            state.route_traverse.remove(0);
            // BO1 grows the amount back as legs are done (by three quarters of their area).
            state.lookahead_amount = (state.lookahead_amount * 2.0).min(LOOKAHEAD_AREA_MAX);
        } else {
            break;
        }
    }
    let first = *state.route.first()?;
    // The path from here: its nodes up to a traversal's start, then the goal.
    let mut path = vec![origin];
    let mut cut_short = false;
    for (k, node) in state.route.iter().enumerate() {
        path.push(*node);
        if state.route_traverse.get(k + 1).copied().unwrap_or(false) {
            cut_short = true;
            break;
        }
    }
    if !cut_short {
        path.push(target);
    }
    let floor = first[2];
    let mut amount = if state.lookahead_amount > 0.0 {
        state.lookahead_amount
    } else {
        LOOKAHEAD_AREA_MAX
    };
    while amount >= LOOKAHEAD_AREA_MIN {
        let point = area_lookahead(&path, origin, amount);
        // Nodes sit about 32 over the floor; the goal is on it.
        let rise = if point == target {
            target[2] - origin[2]
        } else {
            point[2] - floor
        };
        let on_floor = rise.abs() <= 24.0
            || [1.0 / 3.0, 2.0 / 3.0].into_iter().all(|f: f32| {
                let at = [
                    origin[0] + (point[0] - origin[0]) * f,
                    origin[1] + (point[1] - origin[1]) * f,
                    origin[2] + rise * f,
                ];
                floor_near(world, object, at)
            });
        if flat_distance(origin, point) < 1.0
            || on_floor && body_clear(world, object, origin, point, rise.clamp(-64.0, 64.0))
        {
            state.lookahead_amount = amount;
            return Some(point);
        }
        amount *= 0.5;
    }
    // Nothing clear to cut to: the next node, as the route has it.
    state.lookahead_amount = LOOKAHEAD_AREA_MIN;
    Some(first)
}

/// The point along `path` (from `origin`) where the area between the path and the
/// straight way from `origin` reaches `amount`: each leg adds its length times how far
/// off its line `origin` stands. The path's end when it never does.
fn area_lookahead(path: &[[f32; 3]], origin: [f32; 3], amount: f32) -> [f32; 3] {
    let mut area = 0.0f32;
    for leg in path.windows(2) {
        let (a, b) = (leg[0], leg[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if length < 0.001 {
            continue;
        }
        let dir = [d[0] / length, d[1] / length];
        let off = (dir[0] * (a[1] - origin[1]) - dir[1] * (a[0] - origin[0])).abs();
        area += length * off;
        if area >= amount && off > 0.001 {
            let back = ((area - amount) / off).min(length);
            let t = back / length;
            return [
                b[0] - d[0] * t,
                b[1] - d[1] * t,
                b[2] - (b[2] - a[2]) * t,
            ];
        }
    }
    *path.last().unwrap_or(&origin)
}

const LOOKAHEAD_MS: i64 = 150;


/// Whether an actor could walk straight from `origin` to `to` (rising `rise` on the way):
/// its body swept along the way, above what its walk steps up (`STEP_HEIGHT`), clears what
/// its walk collides with (monster clip included). A line through its middle, or one per
/// shoulder, misses a door jamb or a stair's newel post its body still catches, more so on
/// a slant, where the box is wider than its shoulders.
fn body_clear(world: &mut World, object: u64, origin: [f32; 3], to: [f32; 3], rise: f32) -> bool {
    let mins = [-RADIUS, -RADIUS, STEP_HEIGHT + 1.0];
    let maxs = [RADIUS, RADIUS, STEP_HEIGHT + 1.0 + 2.0 * RADIUS];
    let end = [to[0], to[1], origin[2] + rise];
    let (fraction, inside) = crate::step::actor_box_sweep(world, object, origin, end, mins, maxs);
    if inside {
        // Already touching something at that height (a low ceiling, a rail it leans on):
        // by lines at knee and chest across its width instead.
        let along = [to[0] - origin[0], to[1] - origin[1]];
        let length = (along[0] * along[0] + along[1] * along[1])
            .sqrt()
            .max(0.001);
        let side = [-along[1] / length * RADIUS, along[0] / length * RADIUS];
        return [20.0, 40.0].into_iter().all(|height| {
            [-1.0, 0.0, 1.0].into_iter().all(|offset: f32| {
                let (dx, dy) = (side[0] * offset, side[1] * offset);
                clear_between(
                    world,
                    object,
                    [origin[0] + dx, origin[1] + dy, origin[2] + height],
                    [to[0] + dx, to[1] + dy, origin[2] + height + rise],
                )
            })
        });
    }
    fraction >= 1.0
}

/// How high an actor's walk steps up (its slide's step).
const STEP_HEIGHT: f32 = 18.0;

/// How near a route node an actor has to come before leaving it for the next, when the
/// way on from where it stands is round a corner.
const NODE_REACHED_CORNER: f32 = 8.0;
/// Within reach of a route node and no nearer to it for this long: as near as it gets.
const NO_CLOSER_MS: i64 = 600;

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
/// `animScripted` on a script model (Kino's stage curtains): the clip plays on the model
/// and, once through, the model gets the notify with "end", as BO1's
/// `waittill("<notify>")` expects.
fn model_anim_scripted(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let notify = match args.first() {
        Some(Value::String(name)) => Value::String(name.clone()),
        _ => return Err("animScripted takes a notify name".into()),
    };
    let clip = match args.get(3) {
        Some(Value::Animation { name, .. }) => name.to_string(),
        _ => return Err("animScripted takes an animation".into()),
    };
    let object = super::natives::engine::entity_id(world, receiver)?;
    if let Some(entity) = world.resource_mut::<Runtime>().entities.get_mut(&object) {
        entity.anim_op = Some(Some(clip.as_str().into()));
    }
    let seconds = FrameWorld::from_world(world)
        .script_model_anim(&clip)
        .filter(|timing| timing.frequency > 0.0)
        .map_or(1.0, |timing| 1.0 / timing.frequency);
    let now = super::players::now_ms(world);
    run_now(
        world,
        MODEL_ANIM_SCRIPTED,
        receiver.clone(),
        vec![notify, Value::Float(seconds)],
        now,
    )
    .map_err(|error| format!("animScripted on a model: {error:?}"))?;
    Ok(Value::Undefined)
}

pub(crate) fn anim_scripted(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let object = super::natives::engine::entity_id(world, receiver)?;
    if !is_actor(world, object) {
        return model_anim_scripted(world, receiver, args);
    }
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
    let samples = frame.anim_delta(&clip);
    let (start, yaw) = {
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(origin) = vector_arg(args, 1) {
            runtime.set_object_field(object, "origin", Value::Vector(origin));
        }
        if let Some(angles) = vector_arg(args, 2) {
            runtime.set_object_field(object, "angles", Value::Vector([0.0, angles[1], 0.0]));
        }
        (
            vec_field(&mut runtime, object, "origin"),
            vec_field(&mut runtime, object, "angles")[1].to_radians(),
        )
    };
    let motion = samples.filter(|s| s.len() >= 2).map(|s| (s, start, yaw));
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
    if let Some(entity) = runtime.entities.get_mut(&object) {
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
            motion,
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

/// The map's traverse node (`node_negotiation_begin`) a link starts at near `at`: its
/// animscript, the yaw (radians) BO1 faces the zombie to (`OrientMode "face angle"`) and
/// where its end node (the one it targets) stands.
fn traverse_node(world: &mut World, at: [f32; 3]) -> Option<(String, f32, Option<[f32; 3]>)> {
    let mut runtime = world.resource_mut::<Runtime>();
    let nodes = runtime.engine.map_nodes.clone();
    let mut best: Option<(f32, String, f32, u64)> = None;
    for &node in &nodes {
        let Value::String(kind) = runtime.object_field(node, "type") else {
            continue;
        };
        if kind.to_string() != "Begin" {
            continue;
        }
        let Value::String(script) = runtime.object_field(node, "animscript") else {
            continue;
        };
        let origin = vec_field(&mut runtime, node, "origin");
        let away = flat_distance(origin, at);
        if away > 64.0 || (origin[2] - at[2]).abs() > 64.0 {
            continue;
        }
        if best.as_ref().is_none_or(|(was, ..)| away < *was) {
            let yaw = vec_field(&mut runtime, node, "angles")[1].to_radians();
            best = Some((away, script.to_string(), yaw, node));
        }
    }
    let (_, script, yaw, node) = best?;
    let end = match runtime.object_field(node, "target") {
        Value::String(target) => {
            let target = target.to_string();
            nodes.iter().copied().find(|&other| {
                matches!(runtime.object_field(other, "targetname"), Value::String(name) if name.to_string() == target)
            })
        }
        _ => None,
    }
    .map(|end| vec_field(&mut runtime, end, "origin"));
    Some((script, yaw, end))
}

/// The clip a zombie traverses from `from` to `to` with: what the link's animscript plays
/// (else what its height calls for), for its gait and whether it still has legs.
fn traverse_clip(
    world: &mut World,
    object: u64,
    script: Option<&str>,
    from: [f32; 3],
    to: [f32; 3],
    now: i64,
) -> &'static str {
    let (legs, speed) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let legs = match runtime.object_field(object, "has_legs") {
            Value::Undefined => true,
            legs => truthy(&legs),
        };
        let speed = match runtime.object_field(object, "zombie_move_speed") {
            Value::String(speed) => speed.to_string(),
            _ => String::new(),
        };
        (legs, speed)
    };
    let drop = from[2] - to[2];
    let script = script.unwrap_or("");
    // A crawler's own, where BO1's traversal scripts give it one.
    if is_quad(&mut world.resource_mut::<Runtime>(), object) {
        if script == "zombie_jump_down_120" && clip_length(world, QUAD_JUMP_DOWN_120).is_some() {
            return QUAD_JUMP_DOWN_120;
        }
        if script.starts_with("zombie_wall_crawl")
            && clip_length(world, QUAD_WALL_CRAWL).is_some()
        {
            return QUAD_WALL_CRAWL;
        }
        if (script.starts_with("zombie_mantle") || (script.is_empty() && drop.abs() <= 30.0))
            && clip_length(world, QUAD_MANTLE).is_some()
        {
            return QUAD_MANTLE;
        }
    }
    // A hellhound has its own (`anim.dogAnims["zombie"].traverse`): up, down, or over.
    if is_dog(&mut world.resource_mut::<Runtime>(), object) {
        let sets: &[(f32, &'static str)] = if drop < -20.0 {
            &DOG_JUMP_UP_CLIPS
        } else if drop > 20.0 {
            &DOG_JUMP_DOWN_CLIPS
        } else {
            &[]
        };
        return nearest_clip(world, &[sets], drop.abs()).unwrap_or(DOG_MANTLE_CLIPS[0]);
    }
    let crawl_first = |crawl: &'static [(f32, &'static str)],
                       walk: &'static [(f32, &'static str)]| {
        if legs { vec![walk] } else { vec![crawl, walk] }
    };
    if (script.starts_with("zombie_jump_up") || (script.is_empty() && drop < -100.0))
        && let Some(clip) = nearest_clip(
            world,
            &crawl_first(&CRAWL_JUMP_UP_CLIPS, &JUMP_UP_CLIPS),
            -drop,
        )
    {
        return clip;
    }
    if script.starts_with("zombie_jump_down")
        || script.starts_with("zombie_wall_crawl")
        || (script.is_empty() && drop > 30.0)
    {
        let height = script
            .strip_prefix("zombie_jump_down_")
            .and_then(|height| height.parse::<f32>().ok())
            .unwrap_or(drop);
        if let Some(clip) = nearest_clip(
            world,
            &crawl_first(&CRAWL_JUMP_DOWN_CLIPS, &JUMP_DOWN_CLIPS),
            height,
        ) {
            return clip;
        }
    }
    let clips: &[&'static str] = if !legs {
        &MANTLE_CRAWL_CLIPS
    } else {
        match speed.as_str() {
            "sprint" => &MANTLE_SPRINT_CLIPS,
            "run" => &MANTLE_RUN_CLIPS,
            _ => &MANTLE_WALK_CLIPS,
        }
    };
    // BO1 picks one of the set at random each time.
    clips[(object as usize).wrapping_add((now / 50) as usize) % clips.len()]
}

/// Of the first of `sets` with a clip loaded, the one made for the height nearest `height`.
fn nearest_clip(
    world: &mut World,
    sets: &[&'static [(f32, &'static str)]],
    height: f32,
) -> Option<&'static str> {
    let frame = FrameWorld::from_world(world);
    sets.iter().find_map(|set| {
        set.iter()
            .filter(|(_, clip)| frame.anim_delta(clip).is_some())
            .min_by(|a, b| (a.0 - height).abs().total_cmp(&(b.0 - height).abs()))
            .map(|(_, clip)| *clip)
    })
}

/// Where it can stand near `at` when its collision box there is inside something (it
/// landed on a sill's edge or a bump in the floor): a little higher, else a little further
/// along `yaw`. None when it is clear at `at` already, or nothing near is.
fn clear_of_solid(world: &mut World, object: u64, at: [f32; 3], yaw: f32) -> Option<[f32; 3]> {
    let center = [0.0, 0.0, RADIUS + 1.0];
    if !crate::step::actor_box_solid(world, object, at, center, RADIUS) {
        return None;
    }
    for ahead in [0.0f32, 4.0, 8.0, 16.0, 24.0] {
        for up in [2.0f32, 4.0, 8.0, 12.0, 18.0] {
            let spot = [
                at[0] + yaw.cos() * ahead,
                at[1] + yaw.sin() * ahead,
                at[2] + up,
            ];
            if !crate::step::actor_box_solid(world, object, spot, center, RADIUS) {
                if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
                    diag::info!(
                        Sim,
                        "actor debug: {object} inside something at {:?}; stands clear {ahead} ahead {up} up",
                        at.map(|v| v as i32)
                    );
                }
                return Some(spot);
            }
        }
    }
    None
}

/// The floor under `at` within a step above and a drop below.
fn floor_below(world: &mut World, object: u64, at: [f32; 3]) -> Option<f32> {
    const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
    const CONTENTS_MONSTERCLIP: u32 = 0x0002_0000;
    let ignore = super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
    match super::natives::engine::entity_trace(
        world,
        [at[0], at[1], at[2] + 8.0],
        [at[0], at[1], at[2] - 96.0],
        (crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP) | CONTENTS_MONSTERCLIP,
        ignore,
    ) {
        crate::bullet_collision::TraceOutcome::Hit { end, .. } => Some(end[2]),
        _ => None,
    }
}

/// Carry it from `from` to `to` by `clip` (played facing `yaw`, radians) as BO1's
/// `DoTraverse` does: no gravity or collision, its root motion moving it, for the clip's
/// length. A clip without root motion moves it over the ledge in a straight line.
#[allow(clippy::too_many_arguments)]
fn start_traverse(
    world: &mut World,
    object: u64,
    state: &mut Actor,
    from: [f32; 3],
    to: [f32; 3],
    yaw: f32,
    clip: &'static str,
    now: i64,
    window: bool,
) {
    let to = floor_below(world, object, to).map_or(to, |z| [to[0], to[1], z]);
    // A clip that stays level (a mantle) carries it over a big climb or drop only in
    // a slant through the air: over the ledge instead.
    let motion = FrameWorld::from_world(world)
        .anim_delta(clip)
        .filter(|samples| {
            let height = samples[samples.len() - 1][2] - samples[0][2];
            (to[2] - from[2]).abs() < 40.0 || height.abs() > 16.0
        });
    let ms = clip_length(world, clip)
        .map(|seconds| (seconds * 1000.0) as i64)
        .filter(|ms| *ms > 100)
        .unwrap_or(if window {
            TRAVERSE_MS
        } else {
            ledge_ms(from, to)
        });
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        diag::info!(
            Sim,
            "actor debug: {object} traverses {:?} -> {:?} with {clip} ({ms} ms, root motion {})",
            from.map(|v| v as i32),
            to.map(|v| v as i32),
            motion.is_some()
        );
    }
    state.traverse = Some(Traverse {
        from,
        to,
        start_ms: now,
        ms,
        window,
        ledge: !window || motion.is_some() || (to[2] - from[2]).abs() > 40.0,
        motion: motion.map(|samples| (samples, yaw)),
    });
    state.heading = Some(yaw);
    play(world, object, state, clip);
}

/// The gait BO1's scripts asked for (`self.zombie_move_speed`): its speed and clips.
/// The clip it moves with: the one BO1's script picked for it (`self.run_combatanim`,
/// a random walk, run or sprint of its gait), else one of the gait's.
fn gait(
    runtime: &mut Runtime,
    object: u64,
) -> (f32, &'static [&'static str], Option<&'static str>) {
    if is_dog(runtime, object) {
        return (SPRINT_SPEED, &DOG_RUN_CLIPS, None);
    }
    let speed = match runtime.object_field(object, "zombie_move_speed") {
        Value::String(speed) => speed.to_string(),
        _ => String::new(),
    };
    let chosen = match runtime.object_field(object, "run_combatanim") {
        Value::Animation { name, .. } => Some(intern(&name.to_string())),
        _ => None,
    };
    let (speed, clips): (f32, &'static [&'static str]) = match speed.as_str() {
        "sprint" => (SPRINT_SPEED, &SPRINT_CLIPS),
        "run" => (RUN_SPEED, &RUN_CLIPS),
        _ => (WALK_SPEED, &WALK_CLIPS),
    };
    (speed, clips, chosen)
}

/// Whether it walks (BO1's `zombie_move_speed`), for the swings a walker adds.
fn walks(runtime: &mut Runtime, object: u64) -> bool {
    !matches!(runtime.object_field(object, "zombie_move_speed"), Value::String(speed) if matches!(&*speed.to_string(), "run" | "sprint"))
}

/// Start `clip` on the actor's body unless it already plays it.
fn play(world: &mut World, object: u64, state: &mut Actor, clip: &'static str) {
    if state.clip == Some(clip) {
        return;
    }
    static UNTIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let timing = FrameWorld::from_world(world).script_model_anim(clip);
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        let pace = root_speed(world, clip, 0.0)
            .zip(root_speed(world, clip, 0.5))
            .map(|(a, b)| (a + b) * 0.5);
        diag::info!(
            Sim,
            "actor debug: {object} plays {clip} {timing:?} pace {pace:?}"
        );
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
    state.clip = Some(clip);
    entity.anim_op = Some(Some(clip.into()));
    state.clip_start_ms = super::players::now_ms(world);
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
    // The spawner's script keys and `target` go to the actor it spawns, as the engine
    // spawns an AI from its spawner's key/values: BO1 reads them on the zombie
    // (`script_string` "zombie_chaser" skips the windows, "riser" climbs out of the
    // ground, `target` names the window it heads for). Also the `zone_name` BO1's zone
    // manager writes on each spawner: a riser climbs out at that zone's rise spots.
    let inherited: Vec<(u32, Value)> = runtime
        .objects
        .get(&spawner)
        .map(|fields| {
            fields
                .iter()
                .filter(|(symbol, _)| {
                    super::restart::symbol_name(&runtime, **symbol).is_some_and(|name| {
                        name.starts_with("script_") || matches!(&*name, "target" | "zone_name")
                    })
                })
                .map(|(symbol, value)| (*symbol, value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let id = runtime.create_entity(EntityKind::Actor, &classname)?;
    if let Some(fields) = runtime.objects.get_mut(&id) {
        fields.extend(inherited);
    }
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
    runtime.set_object_field(id, "damageyaw", Value::Int(0));
    runtime.set_object_field(id, "damagetaken", Value::Int(0));
    // The engine's dog flag: `zombie_behind_vox` (the growl of a zombie close behind you)
    // and others test it on every AI, so an unset one made them skip every zombie.
    runtime.set_object_field(id, "isdog", Value::Int(i32::from(kind == "dog")));
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
    let hitloc = weapon_iw4::HITLOC_NAMES[usize::from(hitloc_for_bone(part))];
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
        // And `Actor_Damage`'s: the amount, the hit's direction and its yaw against the
        // actor's facing (`AngleNormalize180(vectoyaw(dir) - angles[1])`), which BO1's
        // death and pain scripts pick their clips by.
        runtime.set_object_field(object, "damagetaken", Value::Int(amount));
        if let Some(Value::Vector(dir)) = args.get(7) {
            runtime.set_object_field(object, "damagedir", Value::Vector(*dir));
            if dir[0] != 0.0 || dir[1] != 0.0 {
                let facing = vec_field(&mut runtime, object, "angles")[1];
                let yaw = damage_yaw(*dir, facing);
                runtime.set_object_field(object, "damageyaw", Value::Int(yaw as i32));
            }
        }
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

/// `AngleNormalize180(vectoyaw(dir) - facing)`: where a hit came from against an actor's
/// facing, BO1's `damageyaw` (beyond 135 either way it struck the front).
fn damage_yaw(dir: [f32; 3], facing: f32) -> f32 {
    let along = dir[1].atan2(dir[0]).to_degrees();
    (along - facing + 540.0).rem_euclid(360.0) - 180.0
}

/// BO1's dog death by where the killing hit came from (`getAnimDirection(damageyaw)`):
/// the hit's direction (`direction`, else from `point`) against the dog's facing.
fn dog_death_clip(world: &mut World, object: u64, args: &[Value]) -> &'static str {
    let mut runtime = world.resource_mut::<Runtime>();
    let at = vec_field(&mut runtime, object, "origin");
    let facing = vec_field(&mut runtime, object, "angles")[1];
    let direction = match (args.get(7), args.get(6)) {
        (Some(Value::Vector(d)), _) if d[0] != 0.0 || d[1] != 0.0 => *d,
        (_, Some(Value::Vector(point))) => [at[0] - point[0], at[1] - point[1], 0.0],
        _ => return DOG_DEATH_CLIPS[0],
    };
    let yaw = damage_yaw(direction, facing);
    if !(-135.0..=135.0).contains(&yaw) {
        DOG_DEATH_CLIPS[0]
    } else if yaw > 45.0 {
        DOG_DEATH_CLIPS[1]
    } else if yaw > -45.0 {
        DOG_DEATH_CLIPS[2]
    } else {
        DOG_DEATH_CLIPS[3]
    }
}

/// `setFlaggedAnimKnobAll(flag, anim, ...)` and its kin on a dead zombie: BO1's death
/// animscript plays the death clip itself (`self.deathanim`, which the spawn picked at
/// random from `level._zombie_deaths`, a crawler's or a gibbed zombie's own, a
/// traversal's). The body plays it, and its notetracks go out as `flag` notifies at their
/// times, "end" last, for the script's `DoNoteTracks` (its `start_ragdoll` note, or
/// `waitForRagdoll` at 35% of the clip, starts the ragdoll). Anything else is the actor's
/// own think's, as before.
pub(crate) fn death_anim(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let Value::Object(object) = receiver else {
        return Ok(Value::Undefined);
    };
    let object = *object;
    let (Some(flag @ Value::String(_)), Some(Value::Animation { name, .. })) =
        (args.first(), args.get(1))
    else {
        return Ok(Value::Undefined);
    };
    let dead = world
        .resource::<Runtime>()
        .actors
        .get(&object)
        .is_some_and(Actor::is_dead);
    if !dead || is_dog(&mut world.resource_mut::<Runtime>(), object) {
        return Ok(Value::Undefined);
    }
    let clip = intern(&name.to_string());
    start_death_clip(world, object, clip, flag.clone(), true);
    Ok(Value::Undefined)
}

/// Plays `clip` as the dead actor's death clip; with `notes`, its notetracks and "end" go
/// out as `flag` notifies.
fn start_death_clip(world: &mut World, object: u64, clip: &'static str, flag: Value, notes: bool) {
    let Some(mut state) = world.resource::<Runtime>().actors.get(&object).cloned() else {
        return;
    };
    // A restart of the clip it plays starts it over.
    state.clip = None;
    play(world, object, &mut state, clip);
    state.death_clip = Some(clip);
    if let Some(slot) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
        slot.clip = state.clip;
        slot.clip_start_ms = state.clip_start_ms;
        slot.death_clip = state.death_clip;
    }
    let seconds = clip_length(world, clip).unwrap_or(1.0);
    let mut timed: Vec<(Value, f32)> = Vec::new();
    if notes {
        timed.extend(
            clip_notes(world, clip)
                .iter()
                .filter(|note| !note.name.eq_ignore_ascii_case("end"))
                .map(|note| (Value::string(&note.name), note.time * seconds)),
        );
    }
    timed.push((Value::string("end"), seconds));
    let now = super::players::now_ms(world);
    for (note, at) in timed {
        let _ = run_now(
            world,
            NOTIFY_AFTER,
            Value::Object(object),
            vec![flag.clone(), note, Value::Float(at.max(0.05))],
            now,
        );
    }
}

/// A death clip for a zombie whose death animscript played none (its `deathFunction`
/// dealt with the death): its `deathanim`, else one of `level._zombie_deaths` for its
/// kind, else one of ours.
fn fallback_death_clip(world: &mut World, object: u64) -> &'static str {
    let (chosen, kind) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let chosen = match runtime.object_field(object, "deathanim") {
            Value::Animation { name, .. } => Some(intern(&name.to_string())),
            _ => None,
        };
        (chosen, animname(&mut runtime, object))
    };
    if let Some(clip) = chosen {
        return clip;
    }
    let listed = kind
        .map(|kind| level_clips(world, "_zombie_deaths", &kind, None))
        .unwrap_or_default();
    let mut roll =
        (super::players::now_ms(world) as u64) ^ object.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    roll = (roll ^ (roll >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    roll ^= roll >> 27;
    if listed.is_empty() {
        DEATH_CLIPS[(roll % DEATH_CLIPS.len() as u64) as usize]
    } else {
        listed[(roll % listed.len() as u64) as usize]
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
    let dog = is_dog(&mut world.resource_mut::<Runtime>(), object);
    if dog && let Some(mut state) = world.resource::<Runtime>().actors.get(&object).cloned() {
        // A hellhound's death by where the hit came from, as its death animscript picks.
        let clip = dog_death_clip(world, object, args);
        play(world, object, &mut state, clip);
        if let Some(slot) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
            *slot = state;
        }
        let seconds = clip_length(world, clip)
            .unwrap_or(1.0)
            .min((CORPSE_MS - 100) as f32 / 1000.0);
        let _ = run_now(
            world,
            NOTIFY_AFTER,
            Value::Object(object),
            vec![
                Value::string("deathanim"),
                Value::string("end"),
                Value::Float(seconds),
            ],
            now,
        );
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
    // The death animscript: a zombie's `deathFunction` hands out the kill points there,
    // then it plays the death clip (`death_anim`).
    let dog_script = world
        .resource::<Runtime>()
        .entities
        .get(&object)
        .is_some_and(|entity| entity.classname.contains("dog"));
    let death = if dog_script {
        "animscripts/zombie_dog_death::main"
    } else {
        "animscripts/zombie_death::main"
    };
    let _ = run_now(world, death, Value::Object(object), Vec::new(), now);
    if dog {
        return;
    }
    let played = world
        .resource::<Runtime>()
        .actors
        .get(&object)
        .is_some_and(|actor| actor.death_clip.is_some());
    if !played {
        let clip = fallback_death_clip(world, object);
        if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
            diag::info!(
                Sim,
                "actor debug: {object} death animscript played no clip; {clip}"
            );
        }
        start_death_clip(world, object, clip, Value::string("deathanim"), false);
    }
}

/// The engine's dog flag, which the spawn sets from the aitype.
fn is_dog(runtime: &mut Runtime, object: u64) -> bool {
    truthy(&runtime.object_field(object, "isdog"))
}

/// Its `animname` (BO1's key into its clip tables: "zombie", "quad_zombie", ...).
fn animname(runtime: &mut Runtime, object: u64) -> Option<String> {
    match runtime.object_field(object, "animname") {
        Value::String(name) => Some(name.to_string()),
        _ => None,
    }
}

fn is_quad(runtime: &mut Runtime, object: u64) -> bool {
    animname(runtime, object).as_deref() == Some("quad_zombie")
}

/// `array[key]` of a script array.
fn array_entry(runtime: &Runtime, array: &Value, key: crate::script::ArrayKey) -> Value {
    match array {
        Value::Array(id) => runtime
            .arrays
            .get(id)
            .and_then(|entries| entries.get(&key))
            .cloned()
            .unwrap_or(Value::Undefined),
        _ => Value::Undefined,
    }
}

/// The clips a BO1 level table lists for an animname: every entry of
/// `level.<table>[animname]`, or just `level.<table>[animname][key]`. Only clips loaded.
fn level_clips(
    world: &mut World,
    table: &str,
    animname: &str,
    key: Option<&str>,
) -> Vec<&'static str> {
    let names: Vec<String> = {
        let mut runtime = world.resource_mut::<Runtime>();
        let table = runtime.object_field(0, table);
        let runtime = &*runtime;
        let set = array_entry(
            runtime,
            &table,
            crate::script::ArrayKey::String(animname.into()),
        );
        let values = match key {
            Some(key) => vec![array_entry(
                runtime,
                &set,
                crate::script::ArrayKey::String(key.into()),
            )],
            None => match &set {
                Value::Array(id) => runtime
                    .arrays
                    .get(id)
                    .map(|entries| entries.values().cloned().collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
        };
        values
            .into_iter()
            .filter_map(|value| match value {
                Value::Animation { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect()
    };
    names
        .iter()
        .map(|name| intern(name))
        .filter(|clip| clip_length(world, clip).is_some())
        .collect()
}

/// Whether a script value is a hellhound.
pub(crate) fn is_dog_value(world: &mut World, value: &Value) -> bool {
    match value {
        Value::Object(object) => is_dog(&mut world.resource_mut::<Runtime>(), *object),
        _ => false,
    }
}

/// A float field, or `default` when the scripts have not set it.
fn float_field(runtime: &mut Runtime, object: u64, name: &str, default: f32) -> f32 {
    match runtime.object_field(object, name) {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => default,
    }
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
fn window_climb(runtime: &mut Runtime, object: u64) -> Option<([f32; 3], f32)> {
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
    Some((to, yaw))
}

/// BO1's actors are solid to each other (read from its executable): one that walks into a
/// `pushable` actor shoves it straight away at its own pace, at least 1.5 units a frame;
/// where that is blocked it tries aside, 0.75 as far, twice. An actor that is not
/// pushable (in a scripted clip or a traversal: BO1 clears `pushable` for those) stops
/// the walker at its edge. Bodies of the dead are not solid.
fn push_actors(world: &mut World, object: u64, from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let step = flat_distance(from, to);
    if step < 0.001 {
        return to;
    }
    let others: Vec<(u64, [f32; 3], bool)> = {
        let mut runtime = world.resource_mut::<Runtime>();
        let ids: Vec<(u64, bool)> = runtime
            .actors
            .iter()
            .filter(|(id, actor)| **id != object && actor.died_ms.is_none())
            .map(|(id, actor)| (*id, actor.scripted.is_none() && actor.traverse.is_none()))
            .collect();
        let mut others = Vec::with_capacity(ids.len());
        for (id, free) in ids {
            if !runtime.live(&id) {
                continue;
            }
            let at = vec_field(&mut runtime, id, "origin");
            let pushable =
                free && !matches!(runtime.object_field(id, "pushable"), Value::Int(0));
            others.push((id, at, pushable));
        }
        others
    };
    // Players are solid to the AI too (BO1 body-blocks zombies): a walker stops at a
    // player's edge rather than passing through.
    let frame = FrameWorld::from_world(world);
    let players: Vec<[f32; 3]> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| {
            frame
                .client_meta(*id)
                .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
        })
        .filter_map(|id| frame.player(id).map(|ps| ps.origin))
        .collect();
    for &at in &players {
        let apart = flat_distance(to, at);
        if apart < RADIUS + PLAYER_RADIUS
            && (at[2] - to[2]).abs() < GOAL_HEIGHT
            && flat_distance(from, at) >= apart
        {
            return from;
        }
    }
    let shove = step.max(PUSH_MIN_STEP);
    let motion = [(to[0] - from[0]) / step, (to[1] - from[1]) / step];
    for &(other, at, pushable) in &others {
        let apart = flat_distance(to, at);
        if apart >= RADIUS * 2.0 || (at[2] - to[2]).abs() >= GOAL_HEIGHT {
            continue;
        }
        // Moving away from it already: no push, no block.
        if flat_distance(from, at) < apart {
            continue;
        }
        let mut crowd: Vec<[f32; 3]> = others_at(&others, other);
        crowd.extend(players.iter().copied());
        if pushable && shove_actor(world, other, at, to, motion, shove, &crowd) {
            continue;
        }
        return from;
    }
    to
}

/// Where the living actors other than `skip` stand.
fn others_at(others: &[(u64, [f32; 3], bool)], skip: u64) -> Vec<[f32; 3]> {
    others
        .iter()
        .filter(|(id, _, _)| *id != skip)
        .map(|(_, at, _)| *at)
        .collect()
}

/// Moves `other` (at `at`) away from a walker now at `by`, as BO1's push: straight away,
/// else to the side the walker passes it on, 0.75 as far each try. False when all are
/// blocked.
fn shove_actor(
    world: &mut World,
    other: u64,
    at: [f32; 3],
    by: [f32; 3],
    motion: [f32; 2],
    shove: f32,
    crowd: &[[f32; 3]],
) -> bool {
    let mut away = [at[0] - by[0], at[1] - by[1]];
    let length = (away[0] * away[0] + away[1] * away[1]).sqrt();
    away = if length < 0.001 {
        // Right on top of it: out ahead of the walker.
        motion
    } else {
        [away[0] / length, away[1] / length]
    };
    // The side the walker passes it on (BO1 picks by the sign of the cross product).
    let side = if motion[0] * away[1] - motion[1] * away[0] >= 0.0 {
        [-away[1], away[0]]
    } else {
        [away[1], -away[0]]
    };
    let center = [0.0, 0.0, RADIUS + 1.0];
    let mut distance = shove;
    for direction in [away, side, side] {
        let spot = [
            at[0] + direction[0] * distance,
            at[1] + direction[1] * distance,
            at[2],
        ];
        let clear_of_crowd = crowd.iter().all(|c| {
            flat_distance(spot, *c) >= RADIUS * 2.0 || (c[2] - spot[2]).abs() >= GOAL_HEIGHT
        });
        if clear_of_crowd && !crate::step::actor_box_solid(world, other, spot, center, RADIUS) {
            world
                .resource_mut::<Runtime>()
                .set_object_field(other, "origin", Value::Vector(spot));
            return true;
        }
        distance *= 0.75;
    }
    false
}

/// Hellhounds take turns on a player: one starts a bite at most every 200 ms
/// (`shouldWaitInCombatIdle` / `setNextDogAttackAllowTime(200)`). True claims the turn.
fn dog_may_bite(world: &mut World, client: crate::ClientId, now: i64) -> bool {
    let mut runtime = world.resource_mut::<Runtime>();
    let Some(player) = runtime.players.get(&client.0).map(|slot| slot.object) else {
        return true;
    };
    if let Value::Int(allow) = runtime.object_field(player, "dogattackallowtime")
        && now < i64::from(allow)
    {
        return false;
    }
    runtime.set_object_field(player, "dogattackallowtime", Value::Int((now + 200) as i32));
    true
}

/// The player BO1's scripts set as this actor's `favoriteenemy`, when it is one it can go
/// for now.
fn favorite_enemy(
    world: &mut World,
    object: u64,
    targets: &[(crate::ClientId, [f32; 3])],
) -> Option<(crate::ClientId, [f32; 3])> {
    let mut runtime = world.resource_mut::<Runtime>();
    let Value::Object(enemy) = runtime.object_field(object, "favoriteenemy") else {
        return None;
    };
    let client = runtime
        .players
        .iter()
        .find(|(_, slot)| slot.object == enemy)
        .map(|(client, _)| *client)?;
    targets
        .iter()
        .find(|(id, _)| id.0 == client)
        .copied()
}

/// Each tick: play out scripted clips and window climbs, walk every other living actor at
/// its goal (or the nearest living player when its scripts set none) and swing at players
/// in reach; turn the dead into corpses and take away the bodies whose time is up.
pub(crate) fn advance(world: &mut World) {
    remove_old_corpses(world);
    if world.resource::<Runtime>().actors.is_empty() {
        return;
    }
    super::natives::t5::close_dynamic_paths(world);
    let now = super::players::now_ms(world);
    let targets: Vec<(crate::ClientId, [f32; 3])> = {
        let runtime = world.resource::<Runtime>();
        // A player BO1 sets `ignoreme` on (downed, waiting for a revive) is left alone.
        let clients: Vec<u32> = runtime
            .players
            .iter()
            .filter(|(_, slot)| {
                !runtime
                    .peek_field(slot.object, "ignoreme")
                    .is_some_and(truthy)
            })
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
        let died = world
            .resource_mut::<Runtime>()
            .actors
            .remove(&object)
            .and_then(|actor| actor.died_ms)
            .unwrap_or(now);
        become_corpse(world, object, died);
    }
}

/// A dead actor's body on the floor, in one of the `ai_corpseCount` slots.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ActorCorpse {
    pub(crate) object: u64,
    died_ms: i64,
}

/// When a body goes: `CORPSE_STAY_MS` after its death, but not before its ragdoll (if it
/// has one) has had `CORPSE_SETTLE_MS` to come to rest.
fn corpse_due(died_ms: i64, ragdoll_ms: Option<i64>) -> i64 {
    let stay = died_ms + CORPSE_STAY_MS;
    ragdoll_ms.map_or(stay, |start| stay.max(start + CORPSE_SETTLE_MS))
}

/// Deletes the bodies that have lain long enough (`corpse_due`).
fn remove_old_corpses(world: &mut World) {
    if world.resource::<Runtime>().actor_corpses.is_empty() {
        return;
    }
    let now = super::players::now_ms(world);
    let mut runtime = world.resource_mut::<Runtime>();
    let due: Vec<u64> = runtime
        .actor_corpses
        .iter()
        .filter(|corpse| {
            let ragdoll = runtime
                .ragdolls
                .get(&corpse.object)
                .map(|mark| i64::from(mark.start_ms));
            now >= corpse_due(corpse.died_ms, ragdoll)
        })
        .map(|corpse| corpse.object)
        .collect();
    for object in due {
        if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
            let died = runtime
                .actor_corpses
                .iter()
                .find(|corpse| corpse.object == object)
                .map_or(now, |corpse| corpse.died_ms);
            diag::info!(
                Sim,
                "actor debug: {object} body removed {} ms after its death",
                now - died
            );
        }
        runtime
            .actor_corpses
            .retain(|corpse| corpse.object != object);
        runtime.ragdolls.remove(&object);
        if runtime.live(&object) && !runtime.pending_deletes.contains(&object) {
            runtime.pending_deletes.push(object);
        }
    }
}

/// `startRagdoll`'s mark on a body (BO1 makes its trajectory a ragdoll one): since when,
/// and the launches (`launchRagdoll`) sent to it.
#[derive(Clone, Debug, Default)]
pub(crate) struct RagdollMark {
    start_ms: i32,
    launches: Vec<crate::RagdollLaunch>,
}

/// `self startRagdoll()`: from now the client runs the body as a ragdoll. A second call
/// changes nothing (BO1 leaves a ragdoll trajectory alone).
pub(crate) fn start_ragdoll(world: &mut World, object: u64) {
    let now = super::players::now_ms(world) as i32;
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() && !is_ragdoll(world, object) {
        let runtime = world.resource::<Runtime>();
        let died = runtime
            .actors
            .get(&object)
            .and_then(|actor| actor.died_ms)
            .or_else(|| {
                runtime
                    .actor_corpses
                    .iter()
                    .find(|corpse| corpse.object == object)
                    .map(|corpse| corpse.died_ms)
            });
        diag::info!(
            Sim,
            "actor debug: {object} ragdoll starts {:?} ms after its death",
            died.map(|died| i64::from(now) - died)
        );
    }
    world
        .resource_mut::<Runtime>()
        .ragdolls
        .entry(object)
        .or_insert_with(|| RagdollMark {
            start_ms: now,
            launches: Vec::new(),
        });
}

pub(crate) fn is_ragdoll(world: &World, object: u64) -> bool {
    world.resource::<Runtime>().ragdolls.contains_key(&object)
}

/// `self launchRagdoll(force, hitloc)`: BO1 sends the launch only to a ragdoll and the
/// client keeps two of them.
pub(crate) fn launch_ragdoll(world: &mut World, object: u64, force: [f32; 3], hitloc: u8) {
    if let Some(mark) = world.resource_mut::<Runtime>().ragdolls.get_mut(&object)
        && mark.launches.len() < crate::RAGDOLL_MAX_LAUNCHES
    {
        mark.launches.push(crate::RagdollLaunch { force, hitloc });
    }
}

/// BO1's actor becomes a corpse once its death animscript is done (`G_CorpseFromActor`):
/// the body stays where it fell, no longer solid, in one of `ai_corpseCount` slots. With
/// every slot taken, the body farthest behind the player goes (else the farthest off to
/// the side, else the farthest), as `G_GetFreeActorCorpseIndex` picks.
fn become_corpse(world: &mut World, object: u64, died_ms: i64) {
    let view = {
        let runtime = world.resource::<Runtime>();
        let clients: Vec<u32> = runtime.players.keys().copied().collect();
        let frame = FrameWorld::from_world(world);
        clients.into_iter().find_map(|client| {
            let ps = frame.player(crate::ClientId(client))?;
            let mut eye = ps.origin;
            eye[2] += ps.view_height_current;
            let (forward, _, _) = math_iw4::angle_vectors(ps.viewangles);
            Some((eye, forward))
        })
    };
    let mut runtime = world.resource_mut::<Runtime>();
    let kept: Vec<ActorCorpse> = runtime
        .actor_corpses
        .iter()
        .copied()
        .filter(|corpse| {
            let id = &corpse.object;
            runtime.live(id) && !runtime.pending_deletes.contains(id) && *id != object
        })
        .collect();
    runtime.actor_corpses = kept;
    if runtime.actor_corpses.len() >= AI_CORPSE_COUNT {
        let corpses: Vec<(u64, [f32; 3])> = runtime
            .actor_corpses
            .iter()
            .map(|corpse| {
                let at = match runtime.peek_field(corpse.object, "origin") {
                    Some(Value::Vector(at)) => *at,
                    _ => [0.0; 3],
                };
                (corpse.object, at)
            })
            .collect();
        if let Some(freed) = corpse_to_free(&corpses, view) {
            runtime
                .actor_corpses
                .retain(|corpse| corpse.object != freed);
            runtime.ragdolls.remove(&freed);
            if !runtime.pending_deletes.contains(&freed) {
                runtime.pending_deletes.push(freed);
            }
        }
    }
    runtime.actor_corpses.push(ActorCorpse { object, died_ms });
    if let Some(entity) = runtime.entities.get_mut(&object) {
        entity.solid = false;
    }
}

/// Which body `G_GetFreeActorCorpseIndex` frees, seen from the player's eye along their
/// flat view: the farthest behind, else the farthest more than 45 degrees off the view,
/// else the farthest. No player: from the origin, looking along +y.
fn corpse_to_free(corpses: &[(u64, [f32; 3])], view: Option<([f32; 3], [f32; 3])>) -> Option<u64> {
    let (eye, mut dir, flat) = match view {
        Some((eye, forward)) => (eye, forward, true),
        None => ([0.0; 3], [0.0, 1.0, 0.0], true),
    };
    let mut flat = flat;
    dir[2] = 0.0;
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    if len > 0.0 {
        dir = [dir[0] / len, dir[1] / len, 0.0];
    } else if let Some((_, forward)) = view {
        dir = forward;
        flat = false;
    }
    let mut behind: Option<(f32, u64)> = None;
    let mut aside: Option<(f32, u64)> = None;
    let mut farthest: Option<(f32, u64)> = None;
    for &(id, at) in corpses {
        let mut delta = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
        if flat {
            delta[2] = 0.0;
        }
        let dist_sq = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
        let dot = delta[0] * dir[0] + delta[1] * dir[1] + delta[2] * dir[2];
        let better = |best: Option<(f32, u64)>| best.is_none_or(|(d, _)| dist_sq > d);
        if dot < 0.0 && better(behind) {
            behind = Some((dist_sq, id));
        }
        if 0.5 * dist_sq >= dot * dot && better(aside) {
            aside = Some((dist_sq, id));
        }
        if better(farthest) {
            farthest = Some((dist_sq, id));
        }
    }
    behind.or(aside).or(farthest).map(|(_, id)| id)
}

/// The bodies ragdolling, for the clients: by entity number, with when each started and
/// its launches. Marks on bodies gone are dropped.
pub(crate) fn publish_ragdolls(world: &mut World) {
    let now = super::players::now_ms(world) as i32;
    let bodies = {
        let mut runtime = world.resource_mut::<Runtime>();
        let gone: Vec<u64> = runtime
            .ragdolls
            .keys()
            .copied()
            .filter(|id| !runtime.live(id))
            .collect();
        for id in gone {
            runtime.ragdolls.remove(&id);
        }
        runtime
            .ragdolls
            .iter()
            .filter_map(|(object, mark)| {
                let entity = runtime.entities.get(object)?;
                Some(crate::RagdollBodyRecord {
                    entnum: u16::try_from(entity.number).ok()?,
                    start_ms: mark.start_ms,
                    launches: mark.launches.clone(),
                })
            })
            .collect()
    };
    FrameWorld::from_world(world)
        .ragdolls_mut()
        .set_bodies(bodies, now);
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
        if let Some((samples, start, yaw)) = &scripted.motion {
            let moved = motion_sample(samples, at);
            let (sin, cos) = yaw.sin_cos();
            let at = [
                start[0] + moved[0] * cos - moved[1] * sin,
                start[1] + moved[0] * sin + moved[1] * cos,
                start[2] + moved[2],
            ];
            world
                .resource_mut::<Runtime>()
                .set_object_field(object, "origin", Value::Vector(at));
        }
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
        let climb = window_climb(&mut world.resource_mut::<Runtime>(), object);
        if let Some((hop, yaw)) = climb {
            // The window's own mantle link (its `zombie_mantle_over_40` node by the
            // window) when one leads in the window's way, else a hop past the window.
            let node = traverse_node(world, origin);
            let (to, yaw) = match &node {
                Some((script, node_yaw, Some(end)))
                    if script.starts_with("zombie_mantle")
                        && flat_distance(*end, hop) < WINDOW_HOP
                        && angle_between(*node_yaw, yaw).abs() < 0.8 =>
                {
                    (*end, *node_yaw)
                }
                _ => (hop, yaw),
            };
            let script = node.as_ref().map(|(script, ..)| script.as_str());
            let clip = traverse_clip(
                world,
                object,
                script.filter(|s| s.starts_with("zombie_mantle")),
                origin,
                origin,
                now,
            );
            start_traverse(world, object, state, origin, to, yaw, clip, now, true);
            state.climbed = true;
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
        if std::env::var_os("IW4L_ACTOR_TRACE").is_some()
            && now % 100 < i64::from(crate::MATCH_TICK_MS)
        {
            diag::info!(
                Sim,
                "actor trace: {object} {:.1} {:.1} {:.1} traverse",
                at[0],
                at[1],
                at[2]
            );
        }
        let yaw = climb.motion.as_ref().map_or_else(
            || (climb.to[1] - climb.from[1]).atan2(climb.to[0] - climb.from[0]),
            |(_, yaw)| *yaw,
        );
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
            state.heading = Some(yaw);
            // Landed with its collision box inside something: it couldn't walk off.
            if let Some(spot) = clear_of_solid(world, object, at, yaw) {
                world.resource_mut::<Runtime>().set_object_field(
                    object,
                    "origin",
                    Value::Vector(spot),
                );
            }
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
    // A hellhound in play runs at its enemy (`favoriteenemy`, else the nearest player)
    // whatever its goal: BO1's scripts only ever give a dog its spawn spot as a goal, and
    // the engine's dog chases its enemy.
    let (dog, in_play) = {
        let mut runtime = world.resource_mut::<Runtime>();
        (
            is_dog(&mut runtime, object),
            !truthy(&runtime.object_field(object, "ignoreall")),
        )
    };
    let nearest = if dog {
        favorite_enemy(world, object, &targets).or(nearest)
    } else {
        nearest
    };
    let chasing = dog && in_play;
    let goal = match &state.goal {
        _ if chasing => None,
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
    // A player within BO1's melee range starts a swing (`MeleeCombat`); the hit lands, or
    // misses, at the swing's "fire" notes (after the move, below). A dog bites from its
    // `meleeAttackDist`.
    let reach = if dog {
        float_field(
            &mut world.resource_mut::<Runtime>(),
            object,
            "meleeattackdist",
            DOG_MELEE_ATTACK_DIST,
        )
    } else {
        MELEE_RANGE
    };
    if state.swing.is_none()
        && in_play
        && let Some((client, _)) = nearest.filter(|(_, at)| {
            flat_distance(origin, *at) <= reach && (at[2] - origin[2]).abs() < 72.0
        })
        && (!dog || dog_may_bite(world, client, now))
    {
        start_swing(world, object, state, client, now);
    }
    let attacking = state.swing.is_some();

    let (speed, clips, chosen) = gait(&mut world.resource_mut::<Runtime>(), object);
    let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
    let mut yaw = None;
    let mut walk = [0.0, 0.0];
    let mut steering = None;
    if let Some(swing) = state.swing.clone() {
        // It turns to face whom it swings at, and a walking or running swing carries it
        // on with the clip's own step, short of walking into them.
        let at = targets
            .iter()
            .find(|(client, _)| *client == swing.target)
            .map_or(origin, |(_, at)| *at);
        let toward = (at[1] - origin[1]).atan2(at[0] - origin[0]);
        let heading = turn_toward(
            state.heading.unwrap_or(toward),
            toward,
            ATTACK_TURN_RATE * dt,
        );
        state.heading = Some(heading);
        yaw = Some(heading.to_degrees());
        let phase = (now - swing.start_ms) as f32 / swing.length_ms.max(1) as f32;
        let room = flat_distance(origin, at) - MELEE_CLOSEST;
        if room > 0.0
            && let Some(step) = root_speed(world, swing.clip, phase.min(0.999))
        {
            let step = step.min(room / dt);
            walk = [heading.cos() * step, heading.sin() * step];
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
            // As BO1's actors move along a path (read from its executable): the walk turns
            // toward the look-ahead at no more than `ai_turnRate` (220 degrees a second)
            // and keeps its full pace; within 24 units of where it steers it heads there
            // straight (round a window frame's corner, onto its spot).
            let toward = to[1].atan2(to[0]);
            let was = state.heading.unwrap_or(toward);
            let heading = if leg <= DIRECT_STEER {
                toward
            } else {
                turn_toward(was, toward, TURN_RATE * dt)
            };
            state.heading = Some(heading);
            // The clip BO1 picked for it, and the pace its root motion sets at this point
            // of its cycle (each step's surge), so its feet don't slide.
            let clip = chosen
                .filter(|clip| clip_length(world, clip).is_some())
                .unwrap_or_else(|| pick(clips, object));
            play(world, object, state, clip);
            let length = clip_length(world, clip).unwrap_or(1.0).max(0.05);
            let phase = (now - state.clip_start_ms) as f32 / 1000.0 / length;
            let pace = root_speed(world, clip, phase).unwrap_or(speed);
            let step = pace.min(leg / dt);
            walk = [heading.cos() * step, heading.sin() * step];
            yaw = Some(heading.to_degrees());
        } else {
            let idle = if dog {
                DOG_IDLE
            } else if is_quad(&mut world.resource_mut::<Runtime>(), object) {
                pick(&QUAD_IDLE_CLIPS, object)
            } else {
                "ai_zombie_idle_v1"
            };
            play(world, object, state, idle);
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
    let mut slid = crate::step::actor_slide(world, object, origin, &mut slide);
    let wanted = (walk[0] * walk[0] + walk[1] * walk[1]).sqrt() * dt;
    // Its body caught square on a corner (a door jamb, the newel post at the top of a
    // flight), where sliding along it goes nowhere: a step aside, toward where it steers,
    // round the corner. A walk like any other, against the same walls and rails.
    if wanted > 1.0
        && flat_distance(origin, slid) < wanted * 0.5
        && let Some(via) = steering
        && let Some((aside, velocity)) = step_aside(world, object, origin, walk, state.fall, via)
    {
        slid = aside;
        slide.velocity = velocity;
    }
    state.fall = slide.velocity[2];
    let moved = push_actors(world, object, origin, slid);
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
    // No headway on a walk even aside: plan the route again from a node it can walk onto.
    if wanted > 1.0 && flat_distance(origin, slid) < wanted * 0.25 {
        let since = *state.stuck_since.get_or_insert(now);
        if now - since >= STUCK_MS {
            // IW4L_ACTOR_DEBUG: what stops it, along its walk at knee, waist and chest.
            if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
                const CONTENTS_PLAYERCLIP: u32 = 0x0001_0000;
                const CONTENTS_MONSTERCLIP: u32 = 0x0002_0000;
                let mask = (crate::bullet_collision::MASK_PLAYER_SOLID & !CONTENTS_PLAYERCLIP)
                    | CONTENTS_MONSTERCLIP;
                let length = (walk[0] * walk[0] + walk[1] * walk[1]).sqrt().max(1.0);
                let ahead = [walk[0] / length * 32.0, walk[1] / length * 32.0];
                let ignore =
                    super::natives::engine::trace_ignore(world, Some(&Value::Object(object)));
                let sides = [
                    ahead,
                    [-ahead[0], -ahead[1]],
                    [-ahead[1], ahead[0]],
                    [ahead[1], -ahead[0]],
                ];
                let hits: Vec<String> = [8.0f32, 24.0, 48.0]
                    .iter()
                    .flat_map(|height| sides.iter().map(move |side| (*height, *side)))
                    .map(|(height, side)| {
                        let start = [origin[0], origin[1], origin[2] + height];
                        let end = [start[0] + side[0], start[1] + side[1], start[2]];
                        match super::natives::engine::entity_trace(
                            world,
                            start,
                            end,
                            mask,
                            ignore.clone(),
                        ) {
                            crate::bullet_collision::TraceOutcome::Miss { .. } => {
                                format!("{height}{side:?}: clear")
                            }
                            crate::bullet_collision::TraceOutcome::Hit {
                                fraction,
                                collider,
                                ..
                            } if let Some(model) = match collider {
                                crate::ColliderId::EntityLinkedBrush { owner, .. }
                                | crate::ColliderId::EntityDObjBone { owner, .. } => {
                                    owner.script_model()
                                }
                                _ => None,
                            } =>
                            {
                                let mut runtime = world.resource_mut::<Runtime>();
                                let id = runtime
                                    .entities
                                    .iter()
                                    .find(|(_, entity)| entity.presence == Some(model))
                                    .map(|(id, entity)| (*id, entity.classname.to_string()));
                                let what = id.map(|(id, classname)| {
                                    let field = |runtime: &mut Runtime, name: &str| match runtime
                                        .object_field(id, name)
                                    {
                                        Value::String(text) => text.to_string(),
                                        Value::Vector(v) => format!("{:?}", v.map(|c| c as i32)),
                                        _ => String::new(),
                                    };
                                    format!(
                                        "{classname} {} {} at {} model {}",
                                        field(&mut runtime, "targetname"),
                                        field(&mut runtime, "script_noteworthy"),
                                        field(&mut runtime, "origin"),
                                        field(&mut runtime, "model")
                                    )
                                });
                                format!("{height}{side:?}: hit {what:?} at {:.0}", fraction * 32.0)
                            }
                            crate::bullet_collision::TraceOutcome::Hit {
                                fraction,
                                collider,
                                ..
                            } => {
                                format!(
                                    "{height}{side:?}: hit {collider:?} at {:.0}",
                                    fraction * 32.0
                                )
                            }
                            other => format!("{height}{side:?}: {other:?}"),
                        }
                    })
                    .collect();
                diag::info!(
                    Sim,
                    "actor debug: {object} stuck at {:?} walking {:?}: {}",
                    origin.map(|v| v as i32),
                    ahead.map(|v| v as i32),
                    hits.join(", ")
                );
            }
            state.stuck_since = None;
            state.route.clear();
            state.replan_ms = 0;
            // Node by node for a while: a look-ahead past a step it can't walk up would
            // only steer it back into the step.
            state.lookahead_ms = now + STUCK_DIRECT_MS;
            // Its next node, with the least look-ahead (as BO1 trims a blocked one).
            state.lookahead_point = None;
            state.lookahead_amount = LOOKAHEAD_AREA_MIN;
            // The new route starts at a node it can walk onto, not the one it was heading
            // for. It stays on the floor: BO1's AI leave it only at traversals.
            if let Some(via) = steering {
                if state.blocked_cuts.len() >= 8 {
                    state.blocked_cuts.remove(0);
                }
                state.blocked_cuts.push(via);
            }
        }
    } else {
        state.stuck_since = None;
    }
    // IW4L_ACTOR_DEBUG: a walk that left the floor and came down far below (off a ledge
    // or over a rail; BO1's AI only leave the floor at traversals).
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        if state.fall < -150.0 {
            state.airborne_from.get_or_insert(origin);
        } else if state.fall > -1.0
            && let Some(from) = state.airborne_from.take()
            && from[2] - moved[2] > 40.0
        {
            diag::info!(
                Sim,
                "actor debug: {object} dropped {:.0} from {:?} to {:?}",
                from[2] - moved[2],
                from.map(|v| v as i32),
                moved.map(|v| v as i32)
            );
        }
    }
    // IW4L_ACTOR_TRACE=1: every actor's position ten times a second (path shape).
    if std::env::var_os("IW4L_ACTOR_TRACE").is_some() && now % 100 < i64::from(crate::MATCH_TICK_MS)
    {
        diag::info!(
            Sim,
            "actor trace: {object} {:.1} {:.1} {:.1} {}",
            moved[0],
            moved[1],
            moved[2],
            attacking
        );
    }
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some()
        && now % 2000 < i64::from(crate::MATCH_TICK_MS)
    {
        diag::info!(
            Sim,
            "actor debug: {object} at [{:.0}, {:.0}, {:.0}] headed {headed:?} goal {:?} reached {} in play {in_play} attacking {attacking} via {:?} route {:?}",
            moved[0],
            moved[1],
            moved[2],
            state.goal,
            state.goal_reached,
            steering.map(|v| v.map(|c| c as i32)),
            state
                .route
                .iter()
                .take(3)
                .map(|n| n.map(|c| c as i32))
                .collect::<Vec<_>>(),
        );
        // IW4L_ACTOR_FLOOR=1: what is under and ahead of it, per kind of collision.
        if std::env::var_os("IW4L_ACTOR_FLOOR").is_some() {
            let mut rows = Vec::new();
            for (what, mask) in [
                ("solid", crate::bullet_collision::CONTENTS_SOLID),
                ("playerclip", 0x0001_0000),
                ("monsterclip", 0x0002_0000),
                ("player", crate::bullet_collision::MASK_PLAYER_SOLID),
            ] {
                let down = super::natives::engine::entity_trace(
                    world,
                    [moved[0], moved[1], moved[2] + 100.0],
                    [moved[0], moved[1], moved[2] - 100.0],
                    mask,
                    Default::default(),
                );
                rows.push(format!("{what} {down:?}"));
            }
            diag::info!(
                Sim,
                "actor floor: {object} at {:?}: {}",
                moved.map(|v| v as i32),
                rows.join(" | ")
            );
        }
    }
    {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.set_object_field(object, "origin", Value::Vector(moved));
        if let Some(yaw) = yaw {
            runtime.set_object_field(object, "angles", Value::Vector([0.0, yaw, 0.0]));
        }
    }
    // The swing's "fire" notes: BO1's `melee()` hits whom it swings at only if they are
    // still in range and in front of it then; backing off (or killing it first) dodges.
    let Some(swing) = state.swing.as_mut() else {
        return;
    };
    let share = (now - swing.start_ms) as f32 / swing.length_ms.max(1) as f32;
    let mut hits = Vec::new();
    while swing.fired < swing.fires.len() && swing.fires[swing.fired] <= share {
        swing.fired += 1;
        let Some(&(client, at)) = targets.iter().find(|(client, _)| *client == swing.target) else {
            continue;
        };
        let facing = state.heading.map_or(0.0, |heading| {
            angle_between(heading, (at[1] - moved[1]).atan2(at[0] - moved[0]))
        });
        // As BO1's `melee()` (read from its executable): a box `ai_meleeWidth` 20 wide
        // swept `ai_meleeRange` (64) from the zombie straight at its enemy, so it reaches
        // the edge of the player's body, whichever way it faces, unless a wall is between.
        let distance = flat_distance(moved, at);
        let short = (distance - PLAYER_RADIUS).max(0.0) / distance.max(0.001);
        let near_edge = [
            moved[0] + (at[0] - moved[0]) * short,
            moved[1] + (at[1] - moved[1]) * short,
            moved[2] + 40.0,
        ];
        let connects = distance <= MELEE_RANGE + PLAYER_RADIUS
            && (at[2] - moved[2]).abs() < 72.0
            && no_wall_between(world, object, [moved[0], moved[1], moved[2] + 40.0], near_edge);
        if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
            diag::info!(
                Sim,
                "actor debug: {object} swing {} {} at {:.0} units, {:.0} degrees off",
                swing.clip,
                if connects { "hits" } else { "misses" },
                flat_distance(moved, at),
                facing.to_degrees()
            );
        }
        if connects {
            hits.push(client);
        }
    }
    if now - swing.start_ms >= swing.length_ms {
        state.swing = None;
    }
    let damage = if is_dog(&mut world.resource_mut::<Runtime>(), object) {
        float_field(
            &mut world.resource_mut::<Runtime>(),
            object,
            "meleedamage",
            DOG_BITE_DAMAGE as f32,
        ) as i32
    } else {
        HIT_DAMAGE
    };
    for client in hits {
        super::players::actor_hit_player(world, client, object, damage, moved);
    }
}

/// The walk `walk` turned aside, nearest the straight way first and at most about a right
/// angle off it, when that gets it on: it moves at least half its step, not away from
/// `via`, and keeps a floor under it. Where it slides to and the slide's velocity after.
fn step_aside(
    world: &mut World,
    object: u64,
    origin: [f32; 3],
    walk: [f32; 2],
    fall: f32,
    via: [f32; 3],
) -> Option<([f32; 3], [f32; 3])> {
    let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
    let step = (walk[0] * walk[0] + walk[1] * walk[1]).sqrt() * dt;
    let toward = [via[0] - origin[0], via[1] - origin[1]];
    for angle in [0.5f32, -0.5, 1.0, -1.0, 1.5, -1.5] {
        let (sin, cos) = angle.sin_cos();
        let mut slide = super::mechanics::Slide {
            center: [0.0, 0.0, RADIUS + 1.0],
            radius: RADIUS,
            velocity: [
                walk[0] * cos - walk[1] * sin,
                walk[0] * sin + walk[1] * cos,
                fall,
            ],
        };
        let at = crate::step::actor_slide(world, object, origin, &mut slide);
        let went = [at[0] - origin[0], at[1] - origin[1]];
        let away = went[0] * toward[0] + went[1] * toward[1] < 0.0;
        if away || flat_distance(origin, at) < step * 0.5 {
            continue;
        }
        if floor_below(world, object, at).is_none_or(|floor| at[2] - floor > 24.0) {
            continue;
        }
        return Some((at, slide.velocity));
    }
    None
}

/// Starts a swing at `target`: one of BO1's swings for its gait (a walker's or runner's
/// that steps on, or a standing one), its "fire" notes from the clip.
fn start_swing(
    world: &mut World,
    object: u64,
    state: &mut Actor,
    target: crate::ClientId,
    now: i64,
) {
    let dog = is_dog(&mut world.resource_mut::<Runtime>(), object);
    let walker = walks(&mut world.resource_mut::<Runtime>(), object);
    let moving: &[&'static str] = if dog {
        &[]
    } else if walker {
        &WALK_MELEE_CLIPS
    } else {
        &RUN_MELEE_CLIPS
    };
    // A crawler swings from its own set (`level._zombie_melee["quad_zombie"]`).
    let quad_swings = if is_quad(&mut world.resource_mut::<Runtime>(), object) {
        level_clips(world, "_zombie_melee", "quad_zombie", None)
    } else {
        Vec::new()
    };
    let swings: &[&'static str] = if dog {
        &[DOG_BITE]
    } else if !quad_swings.is_empty() {
        &quad_swings
    } else {
        &MELEE_CLIPS
    };
    let moving: &[&'static str] = if quad_swings.is_empty() { moving } else { &[] };
    let choices: Vec<&'static str> = swings
        .iter()
        .chain(moving)
        .copied()
        .filter(|clip| clip_length(world, clip).is_some())
        .collect();
    if choices.is_empty() {
        return;
    }
    // BO1 picks one at random each swing (mixed from the actor and the time, so swings
    // that end on a beat don't fall into a cycle).
    let mut roll = object ^ (now as u64).rotate_left(17);
    roll = (roll ^ (roll >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    roll = (roll ^ (roll >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    roll ^= roll >> 31;
    let clip = choices[(roll % choices.len() as u64) as usize];
    // A dog's bite runs 1.2 to 1.6 seconds (`attack_time = 1.2 + randomfloat(0.4)`) and
    // lands at its "dog_melee" note.
    let length_ms = if dog {
        1200 + (roll >> 8) as i64 % 400
    } else {
        clip_length(world, clip).map_or(1200, |seconds| (seconds * 1000.0) as i64)
    };
    let note = if dog { "dog_melee" } else { "fire" };
    let mut fires: Vec<f32> = clip_notes(world, clip)
        .iter()
        .filter(|n| n.name.eq_ignore_ascii_case(note))
        .map(|note| note.time)
        .collect();
    if dog {
        // Its notes are shares of the clip; the bite's own length runs on from them.
        let clip_ms = clip_length(world, clip).map_or(length_ms as f32, |s| s * 1000.0);
        for share in &mut fires {
            *share *= clip_ms / length_ms as f32;
        }
    }
    if fires.is_empty() {
        fires.push(0.45);
    }
    fires.sort_by(f32::total_cmp);
    if std::env::var_os("IW4L_ACTOR_DEBUG").is_some() {
        diag::info!(
            Sim,
            "actor debug: {object} swings {clip} ({length_ms} ms, fire at {fires:?})"
        );
    }
    // Swings play once: the same clip twice running starts again.
    state.clip = None;
    play(world, object, state, clip);
    state.swing = Some(Swing {
        clip,
        start_ms: now,
        length_ms: length_ms.max(200),
        fires,
        fired: 0,
        target,
    });
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Whether each player's crosshair is on an enemy actor (a BO1 zombie): the engine turns
/// it red there. A trace along the view, as a bullet would go; the client reads the
/// client dvar `iw4l_crosshair_enemy` ("1" on one), written only when it changes.
pub(crate) fn crosshair_targets(world: &mut World) {
    const DVAR: &str = "iw4l_crosshair_enemy";
    const RANGE: f32 = 8192.0;
    let clients: Vec<u32> = world
        .resource::<Runtime>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        let id = crate::ClientId(client);
        let (view, shown) = {
            let frame = FrameWorld::from_world(world);
            let view = frame.player(id).map(|ps| {
                let eye = [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ];
                (eye, ps.viewangles)
            });
            let shown = frame.client_meta(id).and_then(|meta| {
                meta.client_dvars
                    .iter()
                    .find(|(name, _)| name == DVAR)
                    .map(|(_, value)| value == "1")
            });
            (view, shown)
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let enemy = view.is_some_and(|(eye, angles)| {
            let Some(model) = crate::combat::aimed_script_model(
                &FrameWorld::from_world(world),
                tick,
                id,
                eye,
                angles,
                RANGE,
            ) else {
                return false;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let target = runtime
                .entities
                .iter()
                .find(|(_, entity)| entity.presence == Some(model))
                .map(|(object, _)| *object);
            target.is_some_and(|object| {
                runtime.actors.contains_key(&object)
                    && runtime.object_field(object, "team") == Value::string("axis")
            })
        });
        if shown != Some(enemy) {
            super::natives::player::publish_client_dvar(
                world,
                client,
                DVAR,
                if enemy { "1" } else { "0" }.to_owned(),
            );
        }
    }
}

/// How fast an actor turns while it walks (BO1's `ai_turnRate` default, 220 degrees a
/// second), and while it swings at a player (radians/s).
const TURN_RATE: f32 = 220.0 * std::f32::consts::PI / 180.0;
const ATTACK_TURN_RATE: f32 = 600.0 * std::f32::consts::PI / 180.0;
/// Within this of where it steers, an actor heads there straight (BO1: a look-ahead of
/// 24 units or less is followed without the turn rate).
const DIRECT_STEER: f32 = 24.0;

/// The signed angle from `from` to `to`, in (-pi, pi].
fn angle_between(from: f32, to: f32) -> f32 {
    let mut delta = (to - from) % std::f32::consts::TAU;
    if delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    } else if delta <= -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

/// `from` turned toward `to` by at most `max_step` radians.
fn turn_toward(from: f32, to: f32, max_step: f32) -> f32 {
    let delta = angle_between(from, to);
    from + delta.clamp(-max_step, max_step)
}

/// BO1's hit location (an index into `HITLOC_NAMES`) for a bone of the zombie skeleton,
/// as the engine reports it to `actor_damage` and scales a weapon's damage by: the head
/// and neck for head shots, the limbs for BO1's gibs, the torso for the rest.
pub(crate) fn hitloc_for_bone(bone: &str) -> u8 {
    let bone = bone.to_ascii_lowercase();
    let side = |right: u8, left: u8| {
        if bone.ends_with("_le") || bone.contains("_le_") {
            left
        } else {
            right
        }
    };
    let has = |part: &str| bone.contains(part);
    if has("helmet") {
        1
    } else if has("head") || has("eye") || has("jaw") || has("brow") || has("cheek") {
        2
    } else if has("neck") {
        3
    } else if has("shoulder") {
        side(6, 7)
    } else if has("elbow") {
        side(8, 9)
    } else if has("wrist") || has("finger") || has("thumb") || has("hand") {
        side(10, 11)
    } else if has("hip") {
        side(12, 13)
    } else if has("knee") {
        side(14, 15)
    } else if has("ankle") || has("ball") || has("toe") || has("foot") {
        side(16, 17)
    } else if has("spinelower") || has("mainroot") || has("pelvis") || has("spine1") {
        5
    } else {
        4
    }
}

#[cfg(test)]
mod corpse_tests {
    use super::corpse_to_free;

    #[test]
    fn frees_the_body_farthest_behind_then_aside_then_farthest() {
        // The player at the origin looking along +x.
        let view = Some(([0.0, 0.0, 60.0], [1.0, 0.0, 0.0]));
        let ahead_far = (1, [900.0, 0.0, 0.0]);
        let aside = (2, [10.0, 300.0, 0.0]);
        let behind_near = (3, [-50.0, 0.0, 0.0]);
        let behind_far = (4, [-400.0, 10.0, 0.0]);
        assert_eq!(
            corpse_to_free(&[ahead_far, aside, behind_near, behind_far], view),
            Some(4)
        );
        assert_eq!(corpse_to_free(&[ahead_far, aside], view), Some(2));
        assert_eq!(corpse_to_free(&[ahead_far], view), Some(1));
        assert_eq!(corpse_to_free(&[], view), None);
    }

    #[test]
    fn a_body_goes_after_its_stay_and_not_while_its_ragdoll_settles() {
        use super::{CORPSE_SETTLE_MS, CORPSE_STAY_MS, corpse_due};
        assert_eq!(corpse_due(1000, None), 1000 + CORPSE_STAY_MS);
        assert_eq!(corpse_due(1000, Some(1500)), 1000 + CORPSE_STAY_MS);
        // A ragdoll started late (a long death clip) gets its time to come to rest.
        let late = 1000 + CORPSE_STAY_MS - 1000;
        assert_eq!(corpse_due(1000, Some(late)), late + CORPSE_SETTLE_MS);
    }

    #[test]
    fn damage_yaw_is_the_hit_against_the_facing() {
        use super::damage_yaw;
        // Facing +x, shot from the front: the bullet travels along -x.
        assert_eq!(damage_yaw([-1.0, 0.0, 0.0], 0.0).abs(), 180.0);
        // From behind.
        assert_eq!(damage_yaw([1.0, 0.0, 0.0], 0.0), 0.0);
        // Travelling along +y while facing +x: 90.
        assert!((damage_yaw([0.0, 1.0, 0.0], 0.0) - 90.0).abs() < 1e-3);
        assert!((damage_yaw([0.0, 1.0, 0.0], 170.0) + 80.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod hitloc_tests {
    use super::hitloc_for_bone;

    #[test]
    fn zombie_bones_name_bo1_locations() {
        let name = |bone| weapon_iw4::HITLOC_NAMES[usize::from(hitloc_for_bone(bone))];
        assert_eq!(name("j_head"), "head");
        assert_eq!(name("J_Neck"), "neck");
        assert_eq!(name("j_spineupper"), "torso_upper");
        assert_eq!(name("j_spinelower"), "torso_lower");
        assert_eq!(name("j_shoulder_le"), "left_arm_upper");
        assert_eq!(name("j_elbow_ri"), "right_arm_lower");
        assert_eq!(name("j_wrist_le"), "left_hand");
        assert_eq!(name("j_knee_ri"), "right_leg_lower");
        assert_eq!(name("j_ankle_le"), "left_foot");
        assert_eq!(name("j_hip_ri"), "right_leg_upper");
    }
}

/// Where every living zombie stands, for the players' melee lunge (BO1 lunges a knife
/// onto an AI just out of reach as it does onto a player); the client picks from these.
pub(crate) fn publish_melee_targets(world: &mut World) {
    let at: Vec<[f32; 4]> = {
        let runtime = world.resource::<Runtime>();
        let truthy_field = |object: u64, name: &str| runtime.peek_field(object, name).is_some_and(truthy);
        runtime
            .actors
            .iter()
            .filter(|(_, actor)| !actor.is_dead())
            .filter_map(|(object, _)| match runtime.peek_field(*object, "origin") {
                Some(Value::Vector(v)) => {
                    // How tall it stands: a hellhound and a Nova crawler are low.
                    let quad = matches!(
                        runtime.peek_field(*object, "animname"),
                        Some(Value::String(name)) if &**name == "quad_zombie"
                    );
                    let height = if truthy_field(*object, "isdog") {
                        DOG_HEIGHT
                    } else if quad {
                        QUAD_HEIGHT
                    } else {
                        ZOMBIE_HEIGHT
                    };
                    Some([v[0], v[1], v[2], height])
                }
                _ => None,
            })
            .collect()
    };
    FrameWorld::from_world(world).set_actor_melee_targets(at);
}
