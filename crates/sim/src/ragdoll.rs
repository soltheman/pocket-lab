//! What the server tells a client about its ragdolls. BO1 simulates ragdolls on the
//! client only: the server's `startRagdoll` turns the entity's trajectory into a ragdoll
//! one, `launchRagdoll` sends a launch event (a force and the hit location it struck),
//! and the client's explosion events push the ragdolls near a blast. The client builds
//! the body from the pose it is drawn in and runs it from there.

/// BO1 keeps two launches per ragdoll (`storedLaunchCount < 2`) and drops the rest.
pub const RAGDOLL_MAX_LAUNCHES: usize = 2;

/// How long a blast stays in the snapshots, so a client that misses one snapshot
/// still sees it.
pub const RAGDOLL_BLAST_KEEP_MS: i32 = 500;

/// `launchRagdoll(force, hitloc)`: BO1's force (scaled on the client by
/// `ragdoll_bullet_force`) and the hit location index it is pushed at (0, `none`, for
/// the whole body).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RagdollLaunch {
    pub force: [f32; 3],
    pub hitloc: u8,
}

/// A body the server turned into a ragdoll: the entity, when (`startRagdoll`), and the
/// launches sent to it since.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RagdollBodyRecord {
    pub entnum: u16,
    pub start_ms: i32,
    pub launches: Vec<RagdollLaunch>,
}

/// An explosion, for BO1's `Ragdoll_ExplosionEvent` on the ragdolls near it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RagdollBlast {
    pub origin: [f32; 3],
    pub radius: f32,
    pub at_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RagdollMeta {
    pub bodies: Vec<RagdollBodyRecord>,
    pub blasts: Vec<RagdollBlast>,
}

impl RagdollMeta {
    pub fn body(&self, entnum: u16) -> Option<&RagdollBodyRecord> {
        self.bodies.iter().find(|body| body.entnum == entnum)
    }

    /// An explosion this tick; the ones older than `RAGDOLL_BLAST_KEEP_MS` go.
    pub(crate) fn note_blast(&mut self, origin: [f32; 3], radius: f32, at_ms: i32) {
        if radius <= 0.0 {
            return;
        }
        self.blasts
            .retain(|blast| at_ms.wrapping_sub(blast.at_ms) < RAGDOLL_BLAST_KEEP_MS);
        self.blasts.push(RagdollBlast {
            origin,
            radius,
            at_ms,
        });
    }

    pub(crate) fn set_bodies(&mut self, bodies: Vec<RagdollBodyRecord>, now_ms: i32) {
        self.bodies = bodies;
        self.blasts
            .retain(|blast| now_ms.wrapping_sub(blast.at_ms) < RAGDOLL_BLAST_KEEP_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blasts_age_out() {
        let mut meta = RagdollMeta::default();
        meta.note_blast([0.0; 3], 256.0, 1000);
        meta.note_blast([0.0; 3], 0.0, 1000);
        assert_eq!(meta.blasts.len(), 1);
        meta.set_bodies(Vec::new(), 1000 + RAGDOLL_BLAST_KEEP_MS - 50);
        assert_eq!(meta.blasts.len(), 1);
        meta.set_bodies(Vec::new(), 1000 + RAGDOLL_BLAST_KEEP_MS);
        assert!(meta.blasts.is_empty());
    }
}
