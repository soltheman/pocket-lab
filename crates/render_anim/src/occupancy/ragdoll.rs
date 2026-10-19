//! The script models the server ragdolls (BO1's dying zombies): each is started from
//! the pose it is drawn in, stepped with the others on the world's collision, and drawn
//! (and its tags bolted) in the ragdoll's pose instead of its animation's.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::anim::ragdoll::{
    MAX_SIMULATING, PoseBone, REACTIVATION_CUTOFF, Ragdoll, RagdollFrame, RagdollHit, RagdollRig,
    RagdollStart, RagdollState, RagdollTrace, STEP_MS, compose_pose,
};
use crate::occupancy::dyn_ent_phys::DynEntPhysClip;
use crate::occupancy::script_model::ScriptModelDobjs;
use render_scene::{FpvLens, WorldScriptModelInstance};

/// What the world's collision a ragdoll hits: what is solid (BO1's physics world),
/// not the player or AI clip.
const RAGDOLL_CONTENTS: u32 = 1;
/// BO1's explosion event pushes at most the four ragdolls nearest it.
const EXPLOSION_BODIES: usize = 4;
/// At most this many steps a frame (a long hitch does not make the bodies run on).
const MAX_STEPS: i32 = 4;

/// The ragdoll pose a script model is drawn in this frame.
#[derive(Clone, Debug)]
pub(crate) struct RagdollPose {
    /// The skeleton's bones, in the world.
    pub world: Vec<Mat4>,
    /// Where the body is (its pelvis), for where it is drawn and lit from.
    pub root: Vec3,
    /// Its pose's revision: new each frame it moves, kept while it lies still (so a
    /// still body's skinning is kept).
    pub pose_revision: u32,
}

impl RagdollPose {
    /// Where it is drawn from: its pelvis, turned as the entity is.
    pub fn world_from_local(&self, transform: &Transform) -> Mat4 {
        Mat4::from_rotation_translation(transform.rotation, self.root)
    }

    /// The bones relative to `world_from_local`.
    pub fn local_bones(&self, world_from_local: Mat4) -> Vec<Mat4> {
        let inverse = world_from_local.inverse();
        self.world.iter().map(|bone| inverse * *bone).collect()
    }
}

enum Phase {
    /// The skeleton has no ragdoll (a hellhound's): it keeps its animation.
    Unrigged,
    /// Its first frame: the pose it starts from, kept to see how it moves by the next
    /// (BO1's velocity capture).
    Capture {
        joints: [Vec3; 17],
        at_ms: i32,
    },
    Live(Box<Ragdoll>),
}

struct Slot {
    start_ms: i32,
    phase: Phase,
    rig: Option<RagdollRig>,
    start_rotations: [Quat; 12],
    start_scales: [Vec3; 12],
    launches: usize,
    order: u64,
    /// The pose it froze in, once idle.
    frozen: Option<(Vec<Mat4>, Vec3, u32)>,
}

#[derive(Resource, Default)]
pub(crate) struct ScriptModelRagdolls {
    slots: HashMap<u16, Slot>,
    last_ms: Option<i32>,
    accum_ms: i32,
    last_blast_ms: Option<i32>,
    next_order: u64,
    next_revision: u32,
    poses: HashMap<u16, RagdollPose>,
    scratch: Vec<Mat4>,
}

impl ScriptModelRagdolls {
    pub fn pose(&self, entnum: u16) -> Option<&RagdollPose> {
        self.poses.get(&entnum)
    }

    fn revision(&mut self) -> u32 {
        self.next_revision = self.next_revision.wrapping_add(1) & 0x7fff_ffff;
        0x8000_0000 | self.next_revision
    }

    pub fn running(&self) -> usize {
        self.slots
            .values()
            .filter(
                |slot| matches!(&slot.phase, Phase::Live(r) if r.state() == RagdollState::Running),
            )
            .count()
    }
}

struct ClipTrace<'a>(Option<&'a asset_world::ClipCollision>);

impl RagdollTrace for ClipTrace<'_> {
    fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> RagdollHit {
        let Some(clip) = self.0 else {
            return RagdollHit::clear(to);
        };
        let hit = clip.sweep_box(
            from.to_array(),
            to.to_array(),
            [-radius; 3],
            [radius; 3],
            RAGDOLL_CONTENTS,
        );
        RagdollHit {
            fraction: hit.fraction,
            endpos: Vec3::from_array(hit.endpos),
            normal: Vec3::from_array(hit.normal),
            startsolid: hit.startsolid || hit.allsolid,
        }
    }
}

fn joint_positions(rig: &RagdollRig, anim: &[Mat4]) -> Option<[Vec3; 17]> {
    let mut joints = [Vec3::ZERO; 17];
    for (slot, &bone) in joints.iter_mut().zip(&rig.joints) {
        *slot = anim.get(bone)?.w_axis.truncate();
    }
    Some(joints)
}

/// Steps the ragdolls the snapshot names and works out each one's pose this frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step_script_model_ragdolls(
    owners: Query<(&WorldScriptModelInstance, &Transform, &Visibility)>,
    persist: Res<ScriptModelDobjs>,
    xanims: Option<Res<assets::PreparedXAnims>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    clock: Option<Res<net::FrameClock>>,
    clip: Res<DynEntPhysClip>,
    cameras: Query<&GlobalTransform, With<FpvLens>>,
    mut ragdolls: ResMut<ScriptModelRagdolls>,
) {
    let ragdolls = &mut *ragdolls;
    ragdolls.poses.clear();
    let Some(meta) = presented
        .as_ref()
        .and_then(|presented| presented.snapshot())
        .map(|snapshot| &snapshot.meta.ragdolls)
    else {
        ragdolls.slots.clear();
        return;
    };
    let Some(now) = clock
        .as_ref()
        .filter(|clock| clock.started())
        .map(|clock| clock.time())
    else {
        return;
    };
    if meta.bodies.is_empty() && ragdolls.slots.is_empty() {
        ragdolls.last_ms = Some(now);
        ragdolls.last_blast_ms = meta.blasts.iter().map(|b| b.at_ms).max();
        return;
    }
    let trace = ClipTrace(clip.0.as_deref());
    // Bodies gone, or a new body under an old number, are dropped.
    ragdolls.slots.retain(|entnum, slot| {
        meta.body(*entnum)
            .is_some_and(|body| body.start_ms == slot.start_ms)
    });
    let elapsed = ragdolls
        .last_ms
        .map_or(0, |last| (now - last).clamp(0, 250));
    ragdolls.last_ms = Some(now);
    ragdolls.accum_ms += elapsed;
    let steps = (ragdolls.accum_ms / STEP_MS).min(MAX_STEPS);
    ragdolls.accum_ms = (ragdolls.accum_ms - steps * STEP_MS).min(STEP_MS);

    // Each body's animation pose now: what a new one starts from, and what the bones its
    // ragdoll does not drive keep following.
    let mut anims: HashMap<u16, (Vec<Mat4>, std::sync::Arc<xmodel_runtime::DObj>)> = HashMap::new();
    for (owner, transform, visibility) in &owners {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some(entnum) = owner.gentity_number else {
            continue;
        };
        let Some(body) = meta.body(entnum) else {
            continue;
        };
        if ragdolls
            .slots
            .get(&entnum)
            .is_some_and(|slot| slot.frozen.is_some() || matches!(slot.phase, Phase::Unrigged))
        {
            continue;
        }
        let Some(slot_dobj) = owner
            .authority_owner
            .and_then(|owner| owner.script_model())
            .and_then(|id| persist.by_id.get(&id.to_wire()))
        else {
            continue;
        };
        let request = owner
            .dobj_state
            .resolve_request(|name| {
                let xanims = &xanims.as_ref()?.0;
                [
                    asset_core::AssetNamespace::Iw4,
                    asset_core::AssetNamespace::T5,
                    asset_core::AssetNamespace::Iw5,
                    asset_core::AssetNamespace::T6,
                ]
                .into_iter()
                .find_map(|namespace| xanims.clip(namespace, name))
            })
            .unwrap_or_else(|_| xmodel_runtime::DObjPoseRequest::bind_pose());
        let Ok(anim) = xmodel_runtime::pose_dobj(&slot_dobj.dobj, &request, transform.to_matrix())
        else {
            continue;
        };
        let dobj = slot_dobj.dobj.clone();
        if !ragdolls.slots.contains_key(&entnum) {
            let rig = RagdollRig::find(|name| dobj.find(name));
            let phase = match rig.as_ref().and_then(|rig| joint_positions(rig, &anim)) {
                Some(joints) => Phase::Capture { joints, at_ms: now },
                None => Phase::Unrigged,
            };
            let order = ragdolls.next_order;
            ragdolls.next_order += 1;
            ragdolls.slots.insert(
                entnum,
                Slot {
                    start_ms: body.start_ms,
                    phase,
                    rig,
                    start_rotations: [Quat::IDENTITY; 12],
                    start_scales: [Vec3::ONE; 12],
                    launches: 0,
                    order,
                    frozen: None,
                },
            );
        }
        anims.insert(entnum, (anim, dobj));
    }

    // A captured body starts at its second frame, moving as its pose moved; with
    // `MAX_SIMULATING` already running, the one running longest is frozen for it.
    let starting: Vec<u16> = ragdolls
        .slots
        .iter()
        .filter(|(entnum, slot)| {
            matches!(slot.phase, Phase::Capture { at_ms, .. } if now > at_ms)
                && anims.contains_key(entnum)
        })
        .map(|(entnum, _)| *entnum)
        .collect();
    for entnum in starting {
        if ragdolls.running() >= MAX_SIMULATING
            && let Some(oldest) = ragdolls
                .slots
                .iter()
                .filter(|(_, slot)| {
                    matches!(&slot.phase, Phase::Live(r) if r.state() == RagdollState::Running)
                })
                .min_by_key(|(_, slot)| slot.order)
                .map(|(entnum, _)| *entnum)
            && let Some(Phase::Live(ragdoll)) =
                ragdolls.slots.get_mut(&oldest).map(|slot| &mut slot.phase)
        {
            ragdoll.freeze();
        }
        let (anim, _) = &anims[&entnum];
        let Some(slot) = ragdolls.slots.get_mut(&entnum) else {
            continue;
        };
        let (Phase::Capture { joints, at_ms }, Some(rig)) = (&slot.phase, slot.rig.as_ref()) else {
            continue;
        };
        let Some(now_joints) = joint_positions(rig, anim) else {
            continue;
        };
        let mut bone_rotations = [Quat::IDENTITY; 12];
        for (b, rotation) in bone_rotations.iter_mut().enumerate() {
            let (scale, turn, _) = anim[rig.start_bone(b)].to_scale_rotation_translation();
            *rotation = turn;
            slot.start_rotations[b] = turn;
            slot.start_scales[b] = scale;
        }
        let start = RagdollStart {
            before: *joints,
            now: now_joints,
            capture_s: (now - at_ms) as f32 / 1000.0,
            bone_rotations,
        };
        slot.phase = match Ragdoll::new(&start, &trace) {
            Some(ragdoll) => Phase::Live(Box::new(ragdoll)),
            None => Phase::Unrigged,
        };
    }

    // Launches not yet given (BO1's launch events), each once.
    for (entnum, slot) in ragdolls.slots.iter_mut() {
        let Phase::Live(ragdoll) = &mut slot.phase else {
            continue;
        };
        let Some(body) = meta.body(*entnum) else {
            continue;
        };
        for launch in body.launches.iter().skip(slot.launches) {
            ragdoll.launch(Vec3::from_array(launch.force), launch.hitloc);
            slot.frozen = None;
        }
        slot.launches = slot.launches.max(body.launches.len());
    }

    // Explosions: the four ragdolls nearest each, near enough the player.
    let eye = cameras.single().ok().map(GlobalTransform::translation);
    let last_blast = ragdolls.last_blast_ms;
    for blast in meta
        .blasts
        .iter()
        .filter(|blast| last_blast.is_none_or(|last| blast.at_ms > last))
    {
        let origin = Vec3::from_array(blast.origin);
        let mut near: Vec<(f32, u16)> = ragdolls
            .slots
            .iter()
            .filter_map(|(entnum, slot)| match &slot.phase {
                Phase::Live(ragdoll) => Some((ragdoll.root().distance_squared(origin), *entnum)),
                _ => None,
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, entnum) in near.into_iter().take(EXPLOSION_BODIES) {
            let running = ragdolls.running();
            let Some(slot) = ragdolls.slots.get_mut(&entnum) else {
                continue;
            };
            let Phase::Live(ragdoll) = &mut slot.phase else {
                continue;
            };
            if eye.is_some_and(|eye| ragdoll.torso().distance(eye) > REACTIVATION_CUTOFF) {
                continue;
            }
            if ragdoll.state() == RagdollState::Idle && running >= MAX_SIMULATING {
                continue;
            }
            ragdoll.explode(origin, blast.radius);
            if ragdoll.state() == RagdollState::Running {
                slot.frozen = None;
            }
        }
    }
    if let Some(latest) = meta.blasts.iter().map(|blast| blast.at_ms).max() {
        ragdolls.last_blast_ms = Some(latest.max(last_blast.unwrap_or(i32::MIN)));
    }

    for _ in 0..steps {
        for slot in ragdolls.slots.values_mut() {
            if let Phase::Live(ragdoll) = &mut slot.phase {
                ragdoll.step(&trace);
            }
        }
    }

    // The poses: an idle body keeps the one it froze in.
    let alpha = ragdolls.accum_ms as f32 / STEP_MS as f32;
    let entnums: Vec<u16> = ragdolls.slots.keys().copied().collect();
    for entnum in entnums {
        let revision = ragdolls.revision();
        let mut scratch = std::mem::take(&mut ragdolls.scratch);
        let Some(slot) = ragdolls.slots.get_mut(&entnum) else {
            continue;
        };
        if let Some((world, root, revision)) = &slot.frozen {
            ragdolls.poses.insert(
                entnum,
                RagdollPose {
                    world: world.clone(),
                    root: *root,
                    pose_revision: *revision,
                },
            );
            ragdolls.scratch = scratch;
            continue;
        }
        let (Phase::Live(ragdoll), Some(rig), Some((anim, dobj))) =
            (&slot.phase, slot.rig.as_ref(), anims.get(&entnum))
        else {
            ragdolls.scratch = scratch;
            continue;
        };
        let idle = ragdoll.state() == RagdollState::Idle;
        let frame: RagdollFrame = ragdoll.frame(if idle { 1.0 } else { alpha });
        let bones: Vec<PoseBone> = dobj
            .bones
            .iter()
            .map(|bone| PoseBone {
                parent: bone.parent,
                bind_rotation: bone.bind_rotation,
            })
            .collect();
        compose_pose(
            rig,
            &frame,
            &slot.start_rotations,
            &slot.start_scales,
            &bones,
            &dobj.duplicates,
            anim,
            ragdoll.running_ms(),
            &mut scratch,
        );
        let root = frame.joints[0];
        if idle {
            slot.frozen = Some((scratch.clone(), root, revision));
        }
        ragdolls.poses.insert(
            entnum,
            RagdollPose {
                world: scratch.clone(),
                root,
                pose_revision: revision,
            },
        );
        ragdolls.scratch = scratch;
    }
}
