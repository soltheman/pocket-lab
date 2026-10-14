//! BO1 AI actors, first cut (zombies). An `actor_*` spawner's `CodeSpawnerSpawn` makes a
//! script model with the spawner's model; each tick it walks at the nearest living player
//! with monster collision, and in reach it hits them on an interval. Damage to it goes
//! through BO1's `CodeCallback_ActorDamage`, whose scripts score the hit and call
//! `finishActorDamage`; that takes the health and, at zero, runs
//! `CodeCallback_ActorKilled` and the `death` notify. The body is deleted a little later.
//!
//! No pathing (it walks straight, sliding along walls), no animation and no AI natives
//! (goals, animscripts) yet: the actor's own think runs here, not in its scripts.

use bevy_ecs::prelude::World;

use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Runtime, Value};

const ACTOR_DAMAGE: &str = "maps/_callbacksetup::codecallback_actordamage";
const ACTOR_KILLED: &str = "maps/_callbacksetup::codecallback_actorkilled";

/// Walk speed, units per second (BO1's early-round zombies shamble near this).
const SPEED: f32 = 80.0;
/// Close enough to swing at a player (from feet to feet, horizontally).
const REACH: f32 = 52.0;
const HIT_DAMAGE: i32 = 50;
const HIT_INTERVAL_MS: i64 = 1250;
/// The collision cube's half size; its centre sits this far above the feet.
const RADIUS: f32 = 15.0;
/// A killed actor's body stays this long before it is deleted.
const CORPSE_MS: i64 = 1500;
const DEFAULT_HEALTH: i32 = 150;

#[derive(Clone, Debug, Default)]
pub(crate) struct Actor {
    next_hit_ms: i64,
    fall: f32,
    died_ms: Option<i64>,
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
    if let Some(Value::String(targetname)) = args.get(1) {
        runtime.set_object_field(id, "targetname", Value::String(targetname.clone()));
    }
    let entity = runtime.entities.get_mut(&id).unwrap();
    entity.presence = Some(presence);
    entity.can_damage = true;
    if let Some(number) = number {
        entity.number = number;
    }
    runtime.actors.insert(id, Actor::default());
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
    let (before, after) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let before = int_field(&mut runtime, object, "health");
        let after = before.saturating_sub(amount.max(0));
        runtime.set_object_field(object, "health", Value::Int(after));
        (before, after)
    };
    raise(
        world,
        Value::Object(object),
        "damage",
        vec![Value::Int(amount), attacker.clone()],
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
}

/// Each tick: walk every living actor at the nearest living player and swing in reach;
/// delete bodies whose time is up.
pub(crate) fn advance(world: &mut World) {
    if world.resource::<Runtime>().actors.is_empty() {
        return;
    }
    let now = super::players::now_ms(world);
    let targets: Vec<(crate::ClientId, [f32; 3])> = {
        let runtime = world.resource::<Runtime>();
        let clients: Vec<u32> = runtime.players.keys().copied().collect();
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
        let origin = vec_field(&mut world.resource_mut::<Runtime>(), object, "origin");
        let nearest = targets
            .iter()
            .min_by(|a, b| flat_distance(origin, a.1).total_cmp(&flat_distance(origin, b.1)));
        let Some(&(client, target)) = nearest else {
            continue;
        };
        let to = [target[0] - origin[0], target[1] - origin[1]];
        let distance = (to[0] * to[0] + to[1] * to[1]).sqrt();
        let yaw = to[1].atan2(to[0]).to_degrees();
        let walk = if distance > REACH {
            [to[0] / distance * SPEED, to[1] / distance * SPEED]
        } else {
            [0.0, 0.0]
        };
        let mut slide = super::mechanics::Slide {
            center: [0.0, 0.0, RADIUS + 1.0],
            radius: RADIUS,
            velocity: [walk[0], walk[1], state.fall],
        };
        let moved = crate::step::actor_slide(world, object, origin, &mut slide);
        state.fall = slide.velocity[2];
        if std::env::var_os("IW4L_ACTOR_DEBUG").is_some()
            && now % 2000 < i64::from(crate::MATCH_TICK_MS)
        {
            diag::info!(
                Sim,
                "actor debug: {object} at [{:.0}, {:.0}, {:.0}] -> client {} at {distance:.0}",
                moved[0],
                moved[1],
                moved[2],
                client.0
            );
        }
        {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(object, "origin", Value::Vector(moved));
            runtime.set_object_field(object, "angles", Value::Vector([0.0, yaw, 0.0]));
        }
        if distance <= REACH && (target[2] - moved[2]).abs() < 72.0 && now >= state.next_hit_ms {
            state.next_hit_ms = now + HIT_INTERVAL_MS;
            super::players::actor_hit_player(world, client, object, HIT_DAMAGE, moved);
        }
        if let Some(slot) = world.resource_mut::<Runtime>().actors.get_mut(&object) {
            *slot = state;
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

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
