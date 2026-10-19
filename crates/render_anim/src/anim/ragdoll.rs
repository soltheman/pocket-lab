//! BO1's ragdolls, run on the client as BO1 runs them (`Ragdoll_*`): `startRagdoll`
//! turns a dying body into one, built from the pose it is drawn in and moving as that
//! pose moved over its last frame; it falls under the physics gravity, its limbs held by
//! BO1's joint limits and its own self-collision pairs, onto the world's collision, and
//! goes idle (frozen where it lies) once it settles or after `ragdoll_max_life`.
//! Launches (`launchRagdoll`) and explosions push it as BO1's do.
//!
//! BO1 makes a rigid body per bone of its ragdoll definition (`ragdoll.cfg`, def 1)
//! jointed by hinges and swivels. Here each joint is a particle, the torso one rigid
//! cluster (BO1's spine joints allow 6 degrees) and each limb bone a distance between
//! two particles, with the bone's swing about its joint held to the same limits
//! (position-based dynamics): a few bodies, one short collision sweep per joint a step.

use bevy::math::{Mat3, Mat4, Quat, Vec3};

/// BO1's `phys_gravity`.
pub const GRAVITY: f32 = -800.0;
/// BO1's `ragdoll_max_life`: a ragdoll still moving after this goes idle anyway.
pub const MAX_LIFE_MS: i32 = 9000;
/// BO1's `ragdoll_max_simulating`.
pub const MAX_SIMULATING: usize = 16;
/// BO1's `ragdoll_explode_force` and `ragdoll_explode_upbias`.
pub const EXPLODE_FORCE: f32 = 18000.0;
pub const EXPLODE_UPBIAS: f32 = 0.8;
/// BO1's `ragdoll_bullet_force` (what a `launchRagdoll` force is scaled by).
pub const BULLET_FORCE: f32 = 500.0;
/// BO1's `ragdoll_reactivation_cutoff`: an explosion wakes ragdolls only this near the
/// player.
pub const REACTIVATION_CUTOFF: f32 = 1200.0;
/// BO1's `ragdoll_jointlerp_time`: the joints' friction eases off over this long.
const JOINT_LERP_MS: f32 = 3000.0;
/// BO1's `ragdoll_self_collision_scale`.
const SELF_COLLISION_SCALE: f32 = 1.2;
/// BO1 goes idle once more than three quarters of the bodies have slept this long
/// (`Ragdoll_CheckIdle`), or when the body falls faster than 2500 on average.
const IDLE_STABLE_MS: i32 = 250;
const IDLE_FALL_SPEED: f32 = -2500.0;
/// BO1's start velocities: at most 600, and at most 50 upward
/// (`Ragdoll_EstimateInitialVelocities`).
const START_SPEED_MAX: f32 = 600.0;
const START_UP_MAX: f32 = 50.0;

/// The physics step (BO1's `ragdoll_fps` is 20; twice that keeps fast falls tight).
pub const STEP_MS: i32 = 25;
const ITERATIONS: usize = 4;
/// A body slower than this is asleep.
const SLEEP_SPEED: f32 = 6.0;
/// The joints' friction (BO1's per-axis joint friction, eased from full to a tenth over
/// `JOINT_LERP_MS`), as the share of a limb's swing speed lost a second.
const JOINT_DAMPING: f32 = 7.0;
/// What a joint pushed past its limit is moved back by in one pass.
const LIMIT_STIFFNESS: f32 = 0.6;
/// Self-collision that starts overlapping (an arm across the chest) is pushed out this
/// fast.
const SELF_RELAX_SPEED: f32 = 30.0;
const CONTACT_SLOP: f32 = 0.125;
/// A contact plane is dropped once the joint has moved this far along it.
const CONTACT_REACH: f32 = 6.0;
/// A joint on the floor slower than this loses most of its speed each step.
const REST_SPEED: f32 = 20.0;
const REST_DAMPING: f32 = 0.3;
/// A body with this many joints on the floor, none faster than this, comes to rest.
const REST_CONTACTS: usize = 3;
const REST_BODY_SPEED: f32 = 40.0;

/// The joints, as the particles the bones run between (BO1's ragdoll bone names).
pub const JOINTS: [&str; 17] = [
    "j_mainroot",
    "j_spinelower",
    "j_spineupper",
    "j_neck",
    "j_head",
    "j_shoulder_le",
    "j_elbow_le",
    "j_wrist_le",
    "j_shoulder_ri",
    "j_elbow_ri",
    "j_wrist_ri",
    "j_hip_le",
    "j_knee_le",
    "j_ankle_le",
    "j_hip_ri",
    "j_knee_ri",
    "j_ankle_ri",
];
const HEAD: usize = 4;
/// The particles of the rigid torso: the spine, neck, shoulders and hips.
const TORSO: [usize; 8] = [0, 1, 2, 3, 5, 8, 11, 14];

#[derive(Clone, Copy, Debug)]
enum Limit {
    /// One of the three spine bones, held by the torso.
    Torso,
    /// BO1's swivel: limits about the bone's y and z axes (degrees; its twist about x is
    /// not modelled).
    Swivel { y: (f32, f32), z: (f32, f32) },
    /// BO1's hinge about the bone's z axis (degrees), BO1's bend negative.
    Hinge { z: (f32, f32) },
}

#[derive(Clone, Copy, Debug)]
struct BoneDef {
    p0: usize,
    p1: usize,
    radius: f32,
    mass: f32,
    friction: f32,
    parent: usize,
    limit: Limit,
}

const fn bone(
    p0: usize,
    p1: usize,
    radius: f32,
    mass: f32,
    friction: f32,
    parent: usize,
    limit: Limit,
) -> BoneDef {
    BoneDef {
        p0,
        p1,
        radius,
        mass,
        friction,
        parent,
        limit,
    }
}

/// BO1's ragdoll definition 1 (`ragdoll.cfg`: `ragdoll_bone`, `ragdoll_joint` and
/// `ragdoll_limit`): radius, mass, friction, parent bone and joint limits.
const BONES: [BoneDef; 12] = [
    bone(0, 1, 4.5, 20.0, 0.7, usize::MAX, Limit::Torso),
    bone(1, 2, 4.5, 20.0, 0.7, 0, Limit::Torso),
    bone(2, 3, 4.5, 20.0, 0.7, 1, Limit::Torso),
    bone(
        3,
        4,
        3.6,
        5.0,
        0.9,
        2,
        Limit::Swivel {
            y: (-30.0, 30.0),
            z: (-10.0, 30.0),
        },
    ),
    bone(
        5,
        6,
        2.8,
        8.0,
        0.7,
        2,
        Limit::Swivel {
            y: (-90.0, 35.0),
            z: (-110.0, 16.0),
        },
    ),
    bone(6, 7, 2.2, 6.8, 0.7, 4, Limit::Hinge { z: (-100.0, 2.0) }),
    bone(
        8,
        9,
        2.8,
        8.0,
        0.7,
        2,
        Limit::Swivel {
            y: (-35.0, 90.0),
            z: (-110.0, 16.0),
        },
    ),
    bone(9, 10, 2.2, 6.8, 0.7, 6, Limit::Hinge { z: (-100.0, 2.0) }),
    bone(
        11,
        12,
        3.4,
        12.0,
        0.7,
        0,
        Limit::Swivel {
            y: (-30.0, 30.0),
            z: (-4.0, 100.0),
        },
    ),
    bone(12, 13, 3.1, 10.0, 0.7, 8, Limit::Hinge { z: (-120.0, 3.5) }),
    bone(
        14,
        15,
        3.4,
        12.0,
        0.7,
        0,
        Limit::Swivel {
            y: (-30.0, 30.0),
            z: (-4.0, 100.0),
        },
    ),
    bone(
        15,
        16,
        3.1,
        10.0,
        0.7,
        10,
        Limit::Hinge { z: (-120.0, 3.5) },
    ),
];

/// BO1's self-collision pairs for definition 1 (`ragdoll_selfpair`).
const SELF_PAIRS: [(usize, usize); 33] = [
    (0, 4),
    (1, 4),
    (2, 4),
    (8, 4),
    (9, 4),
    (11, 4),
    (0, 5),
    (1, 5),
    (2, 5),
    (3, 5),
    (7, 5),
    (8, 5),
    (9, 5),
    (10, 5),
    (11, 5),
    (0, 6),
    (1, 6),
    (2, 6),
    (8, 6),
    (9, 6),
    (10, 6),
    (0, 7),
    (1, 7),
    (2, 7),
    (3, 7),
    (8, 7),
    (9, 7),
    (10, 7),
    (11, 7),
    (10, 8),
    (11, 8),
    (10, 9),
    (11, 9),
];

/// BO1's base-lerp bones (`ragdoll_baselerp_bone`): the head, hands and feet ease from
/// the animation's pose to the model's base pose over these times.
pub const LERP_BONES: [(&str, i32); 5] = [
    ("j_head", 2000),
    ("j_wrist_le", 1000),
    ("j_wrist_ri", 1000),
    ("j_ankle_le", 1000),
    ("j_ankle_ri", 1000),
];

/// BO1's ragdoll bone for a hit location (`MapHitLocationToRagdollBoneName`), as the
/// joint the bone ends at; `launchRagdoll` pushes the bone ending there.
fn hitloc_joint(hitloc: u8) -> Option<usize> {
    Some(match hitloc {
        1 | 2 => 4,    // helmet, head: j_head
        3 => 3,        // neck
        4 => 2,        // torso_upper: j_spineupper
        5 => 1,        // torso_lower: j_spinelower
        6 => 9,        // right_arm_upper: j_elbow_ri
        7 => 6,        // left_arm_upper: j_elbow_le
        8 | 10 => 10,  // right_arm_lower, right_hand: j_wrist_ri
        9 | 11 => 7,   // left_arm_lower, left_hand: j_wrist_le
        12 => 15,      // right_leg_upper: j_knee_ri
        13 => 12,      // left_leg_upper: j_knee_le
        14 | 16 => 16, // right_leg_lower, right_foot: j_ankle_ri
        15 | 17 => 13, // left_leg_lower, left_foot: j_ankle_le
        _ => return None,
    })
}

/// A box sweep through the world's collision, for the joints.
pub trait RagdollTrace {
    fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> RagdollHit;
}

#[derive(Clone, Copy, Debug)]
pub struct RagdollHit {
    pub fraction: f32,
    pub endpos: Vec3,
    pub normal: Vec3,
    pub startsolid: bool,
}

impl RagdollHit {
    pub fn clear(to: Vec3) -> Self {
        Self {
            fraction: 1.0,
            endpos: to,
            normal: Vec3::Z,
            startsolid: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Contact {
    normal: Vec3,
    point: Vec3,
}

#[derive(Clone, Copy, Debug, Default)]
struct Particle {
    x: Vec3,
    prev: Vec3,
    v: Vec3,
    inv_mass: f32,
    radius: f32,
    friction: f32,
    contact: Option<Contact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RagdollState {
    Running,
    Idle,
}

/// The pose a ragdoll starts from: each joint where the animation had it over the last
/// frame and now, and each bone's orientation now (the bone's own axes, x along it).
pub struct RagdollStart {
    pub before: [Vec3; 17],
    pub now: [Vec3; 17],
    pub capture_s: f32,
    pub bone_rotations: [Quat; 12],
}

/// Where the ragdoll's bodies are: each joint, and each bone's rotation from its start.
#[derive(Clone, Copy, Debug)]
pub struct RagdollFrame {
    pub joints: [Vec3; 17],
    pub delta: [Quat; 12],
}

#[derive(Clone, Debug)]
pub struct Ragdoll {
    particles: [Particle; 17],
    state: RagdollState,
    torso_rest: [Vec3; 8],
    torso_q: Quat,
    torso_q_prev: Quat,
    start_dir: [Vec3; 12],
    length: [f32; 12],
    /// Each limb bone's frame at the start (x along the bone), for its joint limits.
    frame0: [Quat; 12],
    limits: [Limit; 12],
    self_allowed: [f32; 33],
    running_ms: i32,
    stable_ms: i32,
    pending: Vec<(Vec3, Option<usize>)>,
}

fn parent(b: usize) -> Option<usize> {
    (BONES[b].parent != usize::MAX).then_some(BONES[b].parent)
}

fn deg(range: (f32, f32)) -> (f32, f32) {
    (range.0.to_radians(), range.1.to_radians())
}

impl Ragdoll {
    /// A ragdoll from the pose it starts in, moving as that pose moved (BO1's velocity
    /// capture), the joints out of the world kept on the near side of it (its tunnel
    /// test). `None` with a degenerate pose.
    pub fn new(start: &RagdollStart, trace: &impl RagdollTrace) -> Option<Self> {
        let mut particles = [Particle::default(); 17];
        let mut mass = [0.0f32; 17];
        let mut radius = [0.0f32; 17];
        let mut friction = [0.0f32; 17];
        for def in &BONES {
            for p in [def.p0, def.p1] {
                mass[p] += def.mass * 0.5;
                radius[p] = radius[p].max(def.radius);
                friction[p] = friction[p].max(def.friction);
            }
        }
        // BO1 runs the head's capsule 2 past the head joint: the skull.
        radius[HEAD] += 2.0;
        let dt = start.capture_s.max(0.001);
        let root = start.now[0];
        for (i, particle) in particles.iter_mut().enumerate() {
            if !start.now[i].is_finite() || !start.before[i].is_finite() {
                return None;
            }
            let mut v = (start.now[i] - start.before[i]) / dt;
            if v.length() > START_SPEED_MAX {
                v = v.normalize() * START_SPEED_MAX;
            }
            v.z = v.z.min(START_UP_MAX);
            // Kept on the root's side of a wall or floor it would start through.
            let mut x = start.now[i];
            if i != 0 {
                let hit = trace.sweep(root, x, radius[i].min(2.0));
                if !hit.startsolid && hit.fraction < 1.0 {
                    x = hit.endpos + hit.normal * CONTACT_SLOP;
                }
            }
            // A joint the animation lays closer to the floor than its body's radius
            // starts lifted clear of it.
            x = clear_of(trace, x, radius[i]).unwrap_or(x);
            *particle = Particle {
                x,
                prev: x,
                v,
                inv_mass: 1.0 / mass[i].max(0.1),
                radius: radius[i],
                friction: friction[i],
                contact: None,
            };
        }
        let centroid = torso_centroid(&particles);
        let torso_rest = TORSO.map(|p| particles[p].x - centroid);
        let mut start_dir = [Vec3::X; 12];
        let mut length = [0.0f32; 12];
        let mut frame0 = [Quat::IDENTITY; 12];
        let mut limits = [Limit::Torso; 12];
        // The body's front, for which way the hips and knees bend.
        let left = particles[5].x - particles[8].x + particles[11].x - particles[14].x;
        let up = particles[3].x - particles[0].x;
        let forward = left.cross(up).normalize_or_zero();
        for (b, def) in BONES.iter().enumerate() {
            let d = particles[def.p1].x - particles[def.p0].x;
            length[b] = d.length();
            if length[b] < 0.01 {
                return None;
            }
            start_dir[b] = d / length[b];
            // The bone's axes, x along it: a mirrored bone (BO1's right side) has its x
            // the other way, turned round z as BO1 turns it.
            let mut frame = start.bone_rotations[b].normalize();
            if (frame * Vec3::X).dot(start_dir[b]) < 0.0 {
                frame *= Quat::from_rotation_z(std::f32::consts::PI);
            }
            frame = Quat::from_rotation_arc((frame * Vec3::X).normalize(), start_dir[b]) * frame;
            frame0[b] = frame;
            limits[b] = match def.limit {
                Limit::Hinge { z } => {
                    // BO1's hinge bends one way: which way that is in this bone's frame
                    // comes from how bent it starts (or, straight, from the body's
                    // front: knees bend back, elbows forward).
                    let upper = parent(b).map_or(Vec3::X, |pb| {
                        frame.inverse() * (particles[BONES[pb].p1].x - particles[BONES[pb].p0].x)
                    });
                    // The turn about z that took the upper bone's line onto this one,
                    // and which way about z bends it further.
                    let bent = -upper.y.atan2(upper.x);
                    let toward = frame * Vec3::Y;
                    let front = if b == 9 || b == 11 { -forward } else { forward };
                    let bend = if bent.abs() > 10f32.to_radians() {
                        bent.signum()
                    } else if toward.dot(front) > 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let z = if bend < 0.0 { z } else { (-z.1, -z.0) };
                    Limit::Hinge { z: deg(z) }
                }
                Limit::Swivel { y, z } => {
                    // BO1's hip swings forward to its larger limit.
                    let z = if (b == 8 || b == 10) && (frame * Vec3::Y).dot(forward) < 0.0 {
                        (-z.1, -z.0)
                    } else {
                        z
                    };
                    Limit::Swivel {
                        y: deg(y),
                        z: deg(z),
                    }
                }
                Limit::Torso => Limit::Torso,
            };
        }
        let mut ragdoll = Self {
            particles,
            state: RagdollState::Running,
            torso_rest,
            torso_q: Quat::IDENTITY,
            torso_q_prev: Quat::IDENTITY,
            start_dir,
            length,
            frame0,
            limits,
            self_allowed: [0.0; 33],
            running_ms: 0,
            stable_ms: 0,
            pending: Vec::new(),
        };
        for (pair, &(a, b)) in SELF_PAIRS.iter().enumerate() {
            let want = (BONES[a].radius + BONES[b].radius) * SELF_COLLISION_SCALE;
            let (d, _, _) = ragdoll.segment_gap(a, b);
            ragdoll.self_allowed[pair] = want.min(d);
        }
        Some(ragdoll)
    }

    pub fn state(&self) -> RagdollState {
        self.state
    }

    pub fn running_ms(&self) -> i32 {
        self.running_ms
    }

    /// Where the pelvis is.
    pub fn root(&self) -> Vec3 {
        self.particles[0].x
    }

    /// The torso's middle, where BO1 measures an explosion from.
    pub fn torso(&self) -> Vec3 {
        torso_centroid(&self.particles)
    }

    /// BO1's `Ragdoll_Launch`: a force spread over every bone, or all of it on the bone a
    /// hit location names. It lands at the next step (BO1 stores it until the body runs).
    pub fn launch(&mut self, force: Vec3, hitloc: u8) {
        match hitloc_joint(hitloc) {
            Some(joint) => self.pending.push((force * BULLET_FORCE, Some(joint))),
            None => self
                .pending
                .push((force * (BULLET_FORCE / BONES.len() as f32), None)),
        }
        self.wake();
    }

    /// BO1's `Ragdoll_ExplosionEvent` on this body: each bone within `radius` pushed out
    /// from `origin` (biased up), harder the nearer it is; it runs a full life again.
    pub fn explode(&mut self, origin: Vec3, radius: f32) {
        let outer = radius * radius;
        if (self.torso() - origin).length_squared() >= outer {
            return;
        }
        self.wake();
        for (b, def) in BONES.iter().enumerate() {
            let center = (self.particles[def.p0].x + self.particles[def.p1].x) * 0.5;
            let delta = center - origin;
            let dist = delta.length_squared();
            if dist >= outer {
                continue;
            }
            let scale = (outer - dist) / outer;
            let mut dir = delta.normalize_or(Vec3::Z);
            dir.z += EXPLODE_UPBIAS;
            let force = dir.normalize_or(Vec3::Z) * (scale * EXPLODE_FORCE / BONES.len() as f32);
            self.push_bone(b, force);
        }
        self.running_ms = 0;
    }

    fn wake(&mut self) {
        if self.state == RagdollState::Idle {
            self.state = RagdollState::Running;
            self.stable_ms = 0;
            for particle in &mut self.particles {
                particle.v = Vec3::ZERO;
                particle.prev = particle.x;
            }
            self.torso_q_prev = self.torso_q;
        }
    }

    /// Freezes it where it is (BO1's idle body: no physics, posed as it lies).
    pub fn freeze(&mut self) {
        self.state = RagdollState::Idle;
        for particle in &mut self.particles {
            particle.prev = particle.x;
            particle.v = Vec3::ZERO;
        }
        self.torso_q_prev = self.torso_q;
    }

    /// An impulse on one bone, half on each end (BO1's bone mass splits half and half).
    fn push_bone(&mut self, b: usize, impulse: Vec3) {
        for p in [BONES[b].p0, BONES[b].p1] {
            let particle = &mut self.particles[p];
            particle.v += impulse * 0.5 * particle.inv_mass;
        }
    }

    /// One physics step of `STEP_MS`.
    pub fn step(&mut self, trace: &impl RagdollTrace) {
        if self.state == RagdollState::Idle {
            return;
        }
        let dt = STEP_MS as f32 / 1000.0;
        for (force, joint) in std::mem::take(&mut self.pending) {
            match joint {
                Some(joint) => {
                    for b in 0..BONES.len() {
                        if BONES[b].p1 == joint {
                            self.push_bone(b, force);
                        }
                    }
                }
                None => {
                    for b in 0..BONES.len() {
                        self.push_bone(b, force);
                    }
                }
            }
        }
        // BO1 eases the joints' friction from full to a tenth over its first seconds.
        let ease = (1.0 - (self.running_ms as f32 / JOINT_LERP_MS).min(1.0)).powi(2) * 0.9 + 0.1;
        let keep = (1.0 - JOINT_DAMPING * ease * dt).clamp(0.0, 1.0);
        for b in 3..BONES.len() {
            let (p0, p1) = (BONES[b].p0, BONES[b].p1);
            let base = self.particles[p0].v;
            let child = &mut self.particles[p1];
            child.v = base + (child.v - base) * keep;
        }
        let start: [Vec3; 17] = self.particles.map(|p| p.x);
        self.torso_q_prev = self.torso_q;
        for particle in &mut self.particles {
            particle.prev = particle.x;
            particle.v.z += GRAVITY * dt;
            particle.x += particle.v * dt;
        }
        let predicted: [Vec3; 17] = self.particles.map(|p| p.x);
        let relax = SELF_RELAX_SPEED * dt;
        for allowed in self.self_allowed.iter_mut().enumerate() {
            let (pair, allowed) = allowed;
            let (a, b) = SELF_PAIRS[pair];
            let want = (BONES[a].radius + BONES[b].radius) * SELF_COLLISION_SCALE;
            *allowed = (*allowed + relax).min(want);
        }
        for _ in 0..ITERATIONS {
            self.solve_torso();
            self.solve_limbs();
            self.solve_self_collision();
            self.solve_contacts();
        }
        // The world: each joint swept from where it was to where it went.
        for (i, particle) in self.particles.iter_mut().enumerate() {
            let from = start[i];
            let to = particle.x;
            if (to - from).length_squared() < 1e-4 {
                continue;
            }
            let hit = trace.sweep(from, to, particle.radius);
            if hit.startsolid {
                // Sunk into the world: lifted out where it is clear, else held.
                particle.x = clear_of(trace, from, particle.radius).unwrap_or(from);
                particle.v = Vec3::ZERO;
                continue;
            }
            if hit.fraction < 1.0 {
                particle.x = hit.endpos + hit.normal * CONTACT_SLOP;
                particle.contact = Some(Contact {
                    normal: hit.normal,
                    point: particle.x,
                });
            }
        }
        // Velocities from the motion, less what the floor took (BO1's ragdolls do not
        // bounce) and its friction.
        let mut zvel = 0.0;
        for (i, particle) in self.particles.iter_mut().enumerate() {
            let mut v = (particle.x - start[i]) / dt;
            if let Some(contact) = particle.contact {
                let n = contact.normal;
                let vn = v.dot(n);
                let pushed = ((particle.x - predicted[i]).dot(n) / dt).max(0.0);
                let mut tangent = v - n * vn;
                let speed = tangent.length();
                if speed > 0.0 {
                    let lose = (particle.friction * (pushed + (-vn).max(0.0))).min(speed);
                    tangent *= (speed - lose) / speed;
                }
                v = tangent + n * vn.max(0.0);
                // A joint creeping along the floor comes to rest (BO1's physics puts
                // slow bodies in contact to sleep).
                if v.length() < REST_SPEED {
                    v *= REST_DAMPING;
                }
                let along = particle.x - contact.point;
                if along.dot(n) > particle.radius
                    || (along - n * along.dot(n)).length() > CONTACT_REACH
                {
                    particle.contact = None;
                }
            }
            particle.v = v;
            zvel += v.z;
        }
        // A body lying on the floor and barely moving is let come to rest as a whole (the
        // joints off the floor drift with the torso that lies on it).
        let touching = self
            .particles
            .iter()
            .filter(|p| p.contact.is_some())
            .count();
        let fastest = self
            .particles
            .iter()
            .map(|p| p.v.length())
            .fold(0.0, f32::max);
        if touching >= REST_CONTACTS && fastest < REST_BODY_SPEED {
            for particle in &mut self.particles {
                particle.v *= REST_DAMPING;
            }
        }
        self.running_ms += STEP_MS;
        // BO1's idle test: falling for good, out of life, or settled.
        zvel /= self.particles.len() as f32;
        let asleep = BONES
            .iter()
            .filter(|def| {
                self.particles[def.p0].v.length() < SLEEP_SPEED
                    && self.particles[def.p1].v.length() < SLEEP_SPEED
            })
            .count();
        if asleep as f32 > BONES.len() as f32 * 0.75 {
            self.stable_ms += STEP_MS;
        } else {
            self.stable_ms = 0;
        }
        if zvel < IDLE_FALL_SPEED
            || self.running_ms > MAX_LIFE_MS
            || self.stable_ms > IDLE_STABLE_MS
        {
            self.freeze();
        }
    }

    fn solve_torso(&mut self) {
        let c = torso_centroid(&self.particles);
        let mut a = Mat3::ZERO;
        for (k, &p) in TORSO.iter().enumerate() {
            let m = 1.0 / self.particles[p].inv_mass;
            let d = self.particles[p].x - c;
            let r = self.torso_rest[k];
            a += Mat3::from_cols(r * d.x, r * d.y, r * d.z).transpose() * m;
        }
        self.torso_q = extract_rotation(&a, self.torso_q);
        for (k, &p) in TORSO.iter().enumerate() {
            self.particles[p].x = c + self.torso_q * self.torso_rest[k];
        }
    }

    /// Each limb bone's rotation from its start: the torso's for the spine, and for a
    /// limb its parent's carried on by the shortest turn onto where it points now.
    fn rotations(&self, joints: &[Vec3; 17], torso: Quat) -> [Quat; 12] {
        let mut q = [torso; 12];
        for b in 3..BONES.len() {
            let pb = BONES[b].parent;
            let was = q[pb] * self.start_dir[b];
            let now = (joints[BONES[b].p1] - joints[BONES[b].p0]).normalize_or(was);
            q[b] = (Quat::from_rotation_arc(was, now) * q[pb]).normalize();
        }
        q
    }

    fn solve_limbs(&mut self) {
        let mut q = [self.torso_q; 12];
        for b in 3..BONES.len() {
            let def = BONES[b];
            let (p0, p1) = (def.p0, def.p1);
            // The bone's length.
            let (w0, w1) = (self.particles[p0].inv_mass, self.particles[p1].inv_mass);
            let d = self.particles[p1].x - self.particles[p0].x;
            let len = d.length();
            if len > 1e-4 {
                let err = (len - self.length[b]) / len / (w0 + w1);
                self.particles[p0].x += d * (err * w0);
                self.particles[p1].x -= d * (err * w1);
            }
            // Its swing about its joint, within BO1's limits (in the bone's start frame,
            // carried along by its parent).
            let pb = def.parent;
            let frame = q[pb] * self.frame0[b];
            let joint = self.particles[p0].x;
            let dir = (self.particles[p1].x - joint).normalize_or(frame * Vec3::X);
            let local = frame.inverse() * dir;
            let (lo_y, hi_y, lo_z, hi_z) = match self.limits[b] {
                Limit::Swivel { y, z } => (y.0, y.1, z.0, z.1),
                Limit::Hinge { z } => (0.0, 0.0, z.0, z.1),
                Limit::Torso => continue,
            };
            let ay = (-local.z).clamp(-1.0, 1.0).asin();
            let az = local.y.atan2(local.x);
            let (cy, cz) = (ay.clamp(lo_y, hi_y), az.clamp(lo_z, hi_z));
            if cy != ay || cz != az {
                let held = Vec3::new(cy.cos() * cz.cos(), cy.cos() * cz.sin(), -cy.sin());
                let target = joint + frame * held * self.length[b];
                let p = &mut self.particles[p1];
                p.x += (target - p.x) * LIMIT_STIFFNESS;
            }
            let was = q[pb] * self.start_dir[b];
            let now = (self.particles[p1].x - self.particles[p0].x).normalize_or(was);
            q[b] = (Quat::from_rotation_arc(was, now) * q[pb]).normalize();
        }
    }

    /// The closest the two bones' capsules come: the gap, and where along each.
    fn segment_gap(&self, a: usize, b: usize) -> (f32, f32, f32) {
        let (a0, a1) = (self.particles[BONES[a].p0].x, self.particles[BONES[a].p1].x);
        let (b0, b1) = (self.particles[BONES[b].p0].x, self.particles[BONES[b].p1].x);
        let (s, t) = self.pair_closest(a, b);
        let pa = a0.lerp(a1, s);
        let pb = b0.lerp(b1, t);
        (pa.distance(pb), s, t)
    }

    /// Where along each of two bones their capsules come closest. A limb's half at the
    /// torso is left out: it is jointed there, and its capsule meets the torso's (and
    /// the other hip's) at the joint whatever the pose.
    fn pair_closest(&self, a: usize, b: usize) -> (f32, f32) {
        let from = |bone: usize| {
            if TORSO.contains(&BONES[bone].p0) && bone >= 3 {
                0.5
            } else {
                0.0
            }
        };
        let (fa, fb) = (from(a), from(b));
        let (a0, a1) = (self.particles[BONES[a].p0].x, self.particles[BONES[a].p1].x);
        let (b0, b1) = (self.particles[BONES[b].p0].x, self.particles[BONES[b].p1].x);
        let (s, t) = closest_on_segments(a0.lerp(a1, fa), a1, b0.lerp(b1, fb), b1);
        (fa + (1.0 - fa) * s, fb + (1.0 - fb) * t)
    }

    fn solve_self_collision(&mut self) {
        for (pair, &(a, b)) in SELF_PAIRS.iter().enumerate() {
            let allowed = self.self_allowed[pair];
            let (a0, a1) = (BONES[a].p0, BONES[a].p1);
            let (b0, b1) = (BONES[b].p0, BONES[b].p1);
            let (s, t) = self.pair_closest(a, b);
            let pa = self.particles[a0].x.lerp(self.particles[a1].x, s);
            let pb = self.particles[b0].x.lerp(self.particles[b1].x, t);
            let d = pb - pa;
            let dist = d.length();
            if dist >= allowed || dist < 1e-4 {
                continue;
            }
            let n = d / dist;
            let weights = [
                (a0, 1.0 - s, -1.0),
                (a1, s, -1.0),
                (b0, 1.0 - t, 1.0),
                (b1, t, 1.0),
            ];
            let total: f32 = weights
                .iter()
                .map(|&(p, w, _)| w * w * self.particles[p].inv_mass)
                .sum();
            if total <= 0.0 {
                continue;
            }
            let lambda = (allowed - dist) / total;
            for (p, w, side) in weights {
                let particle = &mut self.particles[p];
                particle.x += n * (side * lambda * w * particle.inv_mass);
            }
        }
    }

    fn solve_contacts(&mut self) {
        for particle in &mut self.particles {
            if let Some(contact) = particle.contact {
                let depth = (particle.x - contact.point).dot(contact.normal);
                if depth < 0.0 {
                    particle.x -= contact.normal * depth;
                }
            }
        }
    }

    /// Where it is drawn `alpha` of the way from its last step to its latest.
    pub fn frame(&self, alpha: f32) -> RagdollFrame {
        let alpha = alpha.clamp(0.0, 1.0);
        let joints = self.particles.map(|p| p.prev.lerp(p.x, alpha));
        let torso = self.torso_q_prev.slerp(self.torso_q, alpha);
        RagdollFrame {
            joints,
            delta: self.rotations(&joints, torso),
        }
    }
}

/// Where a joint of `radius` at `x` is clear of the world: there, or lifted by the least
/// of a few heights up to just over its radius.
fn clear_of(trace: &impl RagdollTrace, x: Vec3, radius: f32) -> Option<Vec3> {
    let lifts = [0.0, 0.5, 1.0, 2.0, 3.0, 4.5, radius + 1.0];
    lifts
        .into_iter()
        .filter(|lift| *lift <= radius + 1.0)
        .map(|lift| x + Vec3::Z * lift)
        .find(|at| !trace.sweep(*at, *at, radius).startsolid)
}

fn torso_centroid(particles: &[Particle; 17]) -> Vec3 {
    let mut sum = Vec3::ZERO;
    let mut mass = 0.0;
    for &p in &TORSO {
        let m = 1.0 / particles[p].inv_mass;
        sum += particles[p].x * m;
        mass += m;
    }
    sum / mass
}

/// The rotation part of `a` (Müller et al.'s iteration, from the last one found).
fn extract_rotation(a: &Mat3, mut q: Quat) -> Quat {
    for _ in 0..8 {
        let r = Mat3::from_quat(q);
        let omega = r.x_axis.cross(a.x_axis) + r.y_axis.cross(a.y_axis) + r.z_axis.cross(a.z_axis);
        let denom =
            (r.x_axis.dot(a.x_axis) + r.y_axis.dot(a.y_axis) + r.z_axis.dot(a.z_axis)).abs() + 1e-9;
        let omega = omega / denom;
        let w = omega.length();
        if w < 1e-7 {
            break;
        }
        q = (Quat::from_axis_angle(omega / w, w) * q).normalize();
    }
    q
}

/// The parameters (0..1 along each) of the closest points of two segments.
fn closest_on_segments(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (f32, f32) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.length_squared();
    let e = d2.length_squared();
    let f = d2.dot(r);
    if a <= 1e-8 && e <= 1e-8 {
        return (0.0, 0.0);
    }
    if a <= 1e-8 {
        return (0.0, (f / e).clamp(0.0, 1.0));
    }
    let c = d1.dot(r);
    if e <= 1e-8 {
        return ((-c / a).clamp(0.0, 1.0), 0.0);
    }
    let b = d1.dot(d2);
    let denom = a * e - b * b;
    let mut s = if denom > 1e-8 {
        ((b * f - c * e) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    (s, t)
}

/// Which bones of a skeleton the ragdoll drives, found by name.
#[derive(Clone, Debug)]
pub struct RagdollRig {
    /// The skeleton bone at each joint.
    pub joints: [usize; 17],
    /// The bones eased to their base pose, with how long that takes.
    pub lerp: Vec<(usize, i32)>,
}

impl RagdollRig {
    /// The rig of a skeleton with every ragdoll joint, `None` otherwise (a hellhound).
    pub fn find(find: impl Fn(&str) -> Option<usize>) -> Option<Self> {
        let mut joints = [0usize; 17];
        for (slot, name) in joints.iter_mut().zip(JOINTS) {
            *slot = find(name)?;
        }
        let lerp = LERP_BONES
            .iter()
            .filter_map(|(name, ms)| Some((find(name)?, *ms)))
            .collect();
        Some(Self { joints, lerp })
    }

    /// Each bone's start: the skeleton bone at its first joint.
    pub fn start_bone(&self, b: usize) -> usize {
        self.joints[BONES[b].p0]
    }
}

/// One skeleton bone's place in the ragdoll's pose: the parent it hangs from, and the
/// model's base rotation for it (for the eased bones).
#[derive(Clone, Copy, Debug)]
pub struct PoseBone {
    pub parent: Option<usize>,
    pub bind_rotation: Quat,
}

/// The skeleton's pose (world matrices) with the ragdoll's bodies in it: each bone at
/// the start of a ragdoll bone turned as that bone has turned since the start, the
/// eased bones blended toward the model's base pose, and every other bone kept where the
/// animation holds it relative to its parent (BO1 keeps animating the bones its ragdoll
/// does not drive).
#[allow(clippy::too_many_arguments)]
pub fn compose_pose(
    rig: &RagdollRig,
    frame: &RagdollFrame,
    start_rotations: &[Quat; 12],
    start_scales: &[Vec3; 12],
    bones: &[PoseBone],
    duplicates: &[(usize, usize)],
    anim: &[Mat4],
    running_ms: i32,
    out: &mut Vec<Mat4>,
) {
    out.clear();
    out.resize(bones.len(), Mat4::IDENTITY);
    let mut driven: Vec<Option<usize>> = vec![None; bones.len()];
    for b in 0..BONES.len() {
        if let Some(slot) = driven.get_mut(rig.start_bone(b)) {
            *slot = Some(b);
        }
    }
    let world_of = |b: usize| {
        Mat4::from_scale_rotation_translation(
            start_scales[b],
            frame.delta[b] * start_rotations[b],
            frame.joints[BONES[b].p0],
        )
    };
    let root_bone = rig.start_bone(0);
    let root_shift = anim
        .get(root_bone)
        .map_or(Mat4::IDENTITY, |a| world_of(0) * a.inverse());
    let duplicate_of = |i: usize| duplicates.iter().find(|(d, _)| *d == i).map(|(_, s)| *s);
    for i in 0..bones.len() {
        let Some(a) = anim.get(i).copied() else {
            continue;
        };
        out[i] = if let Some(b) = driven[i] {
            world_of(b)
        } else if let Some(source) = duplicate_of(i).filter(|s| *s < i) {
            out[source] * anim[source].inverse() * a
        } else if let Some(parent) = bones[i].parent.filter(|p| *p < i) {
            let local = anim[parent].inverse() * a;
            let eased = rig
                .lerp
                .iter()
                .find(|(bone, _)| *bone == i)
                .map(|&(_, ms)| (running_ms as f32 / ms.max(1) as f32).clamp(0.0, 1.0));
            match eased {
                Some(t) if t > 0.0 => {
                    let (scale, rotation, translation) = local.to_scale_rotation_translation();
                    let rotation = rotation.slerp(bones[i].bind_rotation, t);
                    out[parent]
                        * Mat4::from_scale_rotation_translation(scale, rotation, translation)
                }
                _ => out[parent] * local,
            }
        } else {
            root_shift * a
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor at z = 0 and nothing else.
    struct Floor;

    impl RagdollTrace for Floor {
        fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> RagdollHit {
            let lo = |p: Vec3| p.z - radius;
            if lo(from) < 0.0 {
                return RagdollHit {
                    fraction: 0.0,
                    endpos: from,
                    normal: Vec3::Z,
                    startsolid: true,
                };
            }
            if lo(to) >= 0.0 {
                return RagdollHit::clear(to);
            }
            let f = lo(from) / (lo(from) - lo(to));
            RagdollHit {
                fraction: f,
                endpos: from.lerp(to, f),
                normal: Vec3::Z,
                startsolid: false,
            }
        }
    }

    /// A body standing upright with its feet `height` above the floor, facing +x
    /// (left is +y).
    fn standing(height: f32) -> [Vec3; 17] {
        let h = height;
        [
            Vec3::new(0.0, 0.0, h + 40.0),
            Vec3::new(0.0, 0.0, h + 46.0),
            Vec3::new(0.0, 0.0, h + 54.0),
            Vec3::new(0.0, 0.0, h + 62.0),
            Vec3::new(0.0, 0.0, h + 66.0),
            Vec3::new(0.0, 7.0, h + 59.0),
            Vec3::new(0.0, 8.0, h + 47.0),
            Vec3::new(2.0, 8.0, h + 36.0),
            Vec3::new(0.0, -7.0, h + 59.0),
            Vec3::new(0.0, -8.0, h + 47.0),
            Vec3::new(2.0, -8.0, h + 36.0),
            Vec3::new(0.0, 4.0, h + 38.0),
            Vec3::new(1.0, 4.0, h + 20.0),
            Vec3::new(0.0, 4.0, h + 3.0),
            Vec3::new(0.0, -4.0, h + 38.0),
            Vec3::new(1.0, -4.0, h + 20.0),
            Vec3::new(0.0, -4.0, h + 3.0),
        ]
    }

    /// Each bone's axes with x along it, y toward the body's front.
    fn axes(joints: &[Vec3; 17]) -> [Quat; 12] {
        std::array::from_fn(|b| {
            let x = (joints[BONES[b].p1] - joints[BONES[b].p0]).normalize();
            let side = if x.dot(Vec3::X).abs() > 0.9 {
                Vec3::Y
            } else {
                Vec3::X
            };
            let z = x.cross(side).normalize();
            let y = z.cross(x);
            Quat::from_mat3(&Mat3::from_cols(x, y, z))
        })
    }

    fn start(joints: [Vec3; 17], push: Vec3) -> RagdollStart {
        RagdollStart {
            before: joints.map(|p| p - push * 0.05),
            now: joints,
            capture_s: 0.05,
            bone_rotations: axes(&joints),
        }
    }

    #[test]
    fn falls_onto_the_floor_and_goes_idle() {
        let mut ragdoll = Ragdoll::new(&start(standing(10.0), Vec3::new(60.0, 0.0, 0.0)), &Floor)
            .expect("a ragdoll");
        let mut steps = 0;
        while ragdoll.state() == RagdollState::Running && steps < 2000 {
            ragdoll.step(&Floor);
            steps += 1;
            for p in &ragdoll.particles {
                assert!(p.x.is_finite());
                assert!(
                    p.x.z - p.radius > -1.0,
                    "a joint went through the floor: {p:?}"
                );
            }
        }
        assert_eq!(ragdoll.state(), RagdollState::Idle);
        assert!(ragdoll.running_ms() <= MAX_LIFE_MS + STEP_MS);
        // It lies down: the head no longer above the pelvis by a standing height.
        let frame = ragdoll.frame(1.0);
        assert!(
            frame.joints[HEAD].z < 20.0,
            "head at {}",
            frame.joints[HEAD].z
        );
        // Its bones keep their lengths.
        for (b, def) in BONES.iter().enumerate() {
            let len = frame.joints[def.p1].distance(frame.joints[def.p0]);
            assert!(
                (len - ragdoll.length[b]).abs() < 1.5,
                "bone {b}: {len} vs {}",
                ragdoll.length[b]
            );
        }
    }

    #[test]
    fn a_launch_pushes_the_body() {
        let mut ragdoll =
            Ragdoll::new(&start(standing(0.0), Vec3::ZERO), &Floor).expect("a ragdoll");
        let before = ragdoll.torso();
        ragdoll.launch(Vec3::new(0.0, 60.0, 12.0), 0);
        for _ in 0..8 {
            ragdoll.step(&Floor);
        }
        assert!(ragdoll.torso().y - before.y > 5.0);
    }

    #[test]
    fn an_explosion_wakes_an_idle_body() {
        let mut ragdoll =
            Ragdoll::new(&start(standing(0.0), Vec3::ZERO), &Floor).expect("a ragdoll");
        ragdoll.freeze();
        let before = ragdoll.torso();
        ragdoll.explode(before - Vec3::new(60.0, 0.0, 20.0), 256.0);
        assert_eq!(ragdoll.state(), RagdollState::Running);
        for _ in 0..8 {
            ragdoll.step(&Floor);
        }
        assert!(ragdoll.torso().x - before.x > 5.0);
        // Too far: nothing.
        let mut far = Ragdoll::new(&start(standing(0.0), Vec3::ZERO), &Floor).expect("a ragdoll");
        far.freeze();
        far.explode(Vec3::new(1000.0, 0.0, 0.0), 256.0);
        assert_eq!(far.state(), RagdollState::Idle);
    }

    #[test]
    fn closest_points_of_crossing_segments() {
        let (s, t) = closest_on_segments(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        );
        assert!((s - 0.5).abs() < 1e-5 && (t - 0.5).abs() < 1e-5);
    }

    #[test]
    fn rotation_extraction_finds_a_turn() {
        let turn = Quat::from_rotation_z(0.7) * Quat::from_rotation_x(0.3);
        let rest = [
            Vec3::X,
            Vec3::Y * 2.0,
            Vec3::Z * 3.0,
            Vec3::new(1.0, 1.0, 0.0),
        ];
        let mut a = Mat3::ZERO;
        for r in rest {
            let d = turn * r;
            a += Mat3::from_cols(r * d.x, r * d.y, r * d.z).transpose();
        }
        // From a cold start it closes in; warm-started from the last (as each step's
        // torso is) it lands on it.
        let found = extract_rotation(&a, Quat::IDENTITY);
        assert!(found.angle_between(turn) < 0.05);
        let found = extract_rotation(&a, found);
        assert!(found.angle_between(turn) < 1e-3);
    }
}
