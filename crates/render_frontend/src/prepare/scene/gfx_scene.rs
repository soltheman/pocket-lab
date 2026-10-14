pub use render_scene::{
    GfxSceneAdd, GfxSceneClear, HostGfxScene, SceneEntSkinInput, SceneEntSkinInputs,
    SceneEntSkinModel, SceneEntSkinPending, SceneEntSkinPendingModel, SceneEntSurfaceCache,
    ScriptMoverBmodelClaim, SpotShadowEntityOriginTrack, SpotShadowSceneOccupancy,
    expand_scene_ent_pending, gfx_scene_spot_shadow_dobj_slots, gfx_scene_spot_shadow_model_slots,
    hide_part_bits_from_tags, occupy_add_bmodel, occupy_add_dobj, occupy_add_dobj_fx,
    occupy_script_brushes, scene_quat_from_angles, scene_quat_from_viewmodel_axes,
    store_scene_ent_pending,
};
pub(crate) use render_scene::{clear_host_gfx_scene, snapshot_spot_shadow_occupancy};

use bevy::prelude::*;
use render_scene::AddDObjPose;

pub(crate) fn apply_anim_dobj_scene_submissions(
    mut submissions: MessageReader<
        crate::adapters::anim::scene_submission::AnimDObjSceneSubmission,
    >,
    mut scene: ResMut<HostGfxScene>,
    mut skin_inputs: ResMut<SceneEntSkinInputs>,
) {
    for submission in submissions.read() {
        apply_anim_dobj_scene_submission(submission, &mut scene, &mut skin_inputs);
    }
}

fn apply_anim_dobj_scene_submission(
    submission: &crate::adapters::anim::scene_submission::AnimDObjSceneSubmission,
    scene: &mut HostGfxScene,
    skin_inputs: &mut SceneEntSkinInputs,
) {
    occupy_add_dobj_fx(
        &mut scene.scene,
        submission.occupy_model_n as usize,
        submission.has_tree,
        submission.render_fx_flags,
        AddDObjPose {
            origin: submission.origin,
            lighting_origin: submission.lighting_origin,
            radius: submission.radius,
            entnum: submission.entnum,
            quat: submission.quat,
        },
    );
    if submission.store_skin {
        store_scene_ent_pending(
            &mut scene.scene,
            skin_inputs,
            submission.entnum,
            submission
                .models
                .iter()
                .map(|model| SceneEntSkinPendingModel {
                    lod: model.lod,
                    bone_count: model.bone_count,
                    skel: std::sync::Arc::clone(&model.skel),
                })
                .collect(),
            submission.hide_part_bits,
        );
    }
}

pub(crate) fn occupy_script_brush_scene(
    world: Option<Res<crate::prepare::scene::world::WorldScene>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    runtimes: Query<(&net::CEntity, &net::CEntityRuntime)>,
    mut gfx_scene: ResMut<HostGfxScene>,
    mut census_log: Local<Option<std::time::Instant>>,
) {
    let Some(world) = world else {
        return;
    };
    let models = world
        .cull
        .as_ref()
        .map(|cull| cull.brush_models.as_slice())
        .unwrap_or(&[]);
    let at_time = presented
        .as_ref()
        .and_then(|value| value.snapshot())
        .map(|snapshot| net::ServerTime::from_tick(snapshot.tick).ms())
        .unwrap_or(0);
    let mut live = Vec::new();
    let mut census = [0u32; 5];
    for (cent, runtime) in &runtimes {
        let entnum = u32::from(cent.number());
        let dobj_present = gfx_scene.scene.scene_ent_live(entnum);
        if runtime.next_state.e_type == entity_iw4::ET_SCRIPTMOVER {
            census[0] += 1;
            census[1] += u32::from(runtime.next_state.solid == entity_iw4::SCRIPT_MOVER_BMODEL_SOLID);
            census[2] += u32::from(dobj_present);
            census[3] +=
                u32::from(runtime.next_state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0);
            census[4] += u32::from(runtime.next_state.index != 0);
        }
        let hidden = !dobj_present
            && runtime.next_state.e_type == entity_iw4::ET_SCRIPTMOVER
            && runtime.next_state.solid == entity_iw4::SCRIPT_MOVER_BMODEL_SOLID
            && runtime.next_state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0;
        if !hidden
            && !entity_iw4::script_mover_add_bmodel(
                runtime.next_state.e_type,
                runtime.next_state.e_flags,
                runtime.next_state.solid,
                dobj_present,
            )
        {
            continue;
        }
        let Ok(model_index) = u32::try_from(runtime.next_state.index) else {
            continue;
        };
        let origin = entity_iw4::evaluate_trajectory(&runtime.current.pos, at_time);
        let angles = entity_iw4::evaluate_trajectory(&runtime.current.apos, at_time);
        live.push(ScriptMoverBmodelClaim {
            model_index,
            origin,
            angles,
            entnum,
            hidden,
        });
    }
    if census_log.is_none_or(|at| at.elapsed().as_secs() >= 30) {
        *census_log = Some(std::time::Instant::now());
        diag::info!(
            World,
            "script movers on the client: {} (bmodel solid {}, with a model {}, nodraw {}, nonzero index {}); {} live brush claims",
            census[0],
            census[1],
            census[2],
            census[3],
            census[4],
            live.len()
        );
    }
    occupy_script_brushes(
        &mut gfx_scene.scene,
        models,
        &world.script_brush_models,
        &live,
    );
}
