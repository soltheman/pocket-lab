//! Keeps a frame from reaching the screen without the world in it.
//!
//! Cameras stack on the window: the world camera copies its image over the whole
//! screen image (Bevy's `upscaling`, which clears first), then the UI camera blends its
//! image on top. Bevy's `upscaling` claims the screen image (using up its clear) before
//! it looks up its copy pipeline, and when that pipeline is not ready (its shader still
//! loading, or creation failed) it returns without writing anything; only on macOS does
//! it still begin a pass. The image then reaches the screen holding whatever it last
//! held: a frame from a few presents ago (the phone showed old and new frames
//! alternating, and the HUD drawn twice) or, on Apple GPUs, uninitialised memory
//! (magenta). The guard clears the image to black on such a frame and logs why.
//!
//! It also counts frames where the world camera did not claim the screen image at all;
//! the UI camera then clears it (see `spawn_overlay_ui_camera` in the ui crate).
use bevy::camera::{CameraOutputMode, CompositingSpace};
use bevy::core_pipeline::Core2d;
use bevy::core_pipeline::Core3d;
use bevy::core_pipeline::blit::{BlitPipeline, BlitPipelineKey};
use bevy::core_pipeline::upscaling::upscaling;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_resource::{
    BlendState, CachedPipelineState, LoadOp, Operations, PipelineCache, RenderPassColorAttachment,
    RenderPassDescriptor, SpecializedRenderPipelines, StoreOp,
};
use bevy::render::renderer::{RenderContext, ViewQuery};
use bevy::render::view::ViewTarget;
use bevy::render::{Render, RenderSystems};

pub(crate) fn register(render_app: &mut SubApp) {
    render_app
        .add_systems(Render, log_camera_stack.in_set(RenderSystems::Prepare))
        .add_systems(Core3d, clear_when_world_copy_missing.after(upscaling))
        .add_systems(Core2d, note_unwritten_screen.before(upscaling));
}

/// The cameras that draw this frame, in the order they write their windows, logged
/// whenever that list changes: a camera ahead of the world camera makes Bevy blend the
/// world over the screen image instead of replacing it.
fn log_camera_stack(
    cameras: Query<(&ExtractedCamera, Has<Camera3d>, Has<Camera2d>)>,
    mut last: Local<String>,
) {
    let mut rows: Vec<(isize, usize, &str, String)> = cameras
        .iter()
        .map(|(camera, is_3d, is_2d)| {
            let kind = match (is_3d, is_2d) {
                (true, _) => "3d",
                (_, true) => "2d",
                _ => "other",
            };
            let output = match camera.output_mode {
                CameraOutputMode::Skip => "skip".to_owned(),
                CameraOutputMode::Write { blend_state, .. } => match blend_state {
                    None => "auto".to_owned(),
                    Some(BlendState::REPLACE) => "replace".to_owned(),
                    Some(BlendState::ALPHA_BLENDING) => "alpha".to_owned(),
                    Some(_) => "custom blend".to_owned(),
                },
            };
            (camera.order, camera.sorted_camera_index_for_target, kind, output)
        })
        .collect();
    rows.sort();
    let line = rows
        .iter()
        .map(|(order, index, kind, output)| format!("{kind} order {order} #{index} {output}"))
        .collect::<Vec<_>>()
        .join(", ");
    if *last != line {
        diag::info!(World, "camera stack: [{line}]");
        *last = line;
    }
}

/// Frames counted between log lines, after the first.
const LOG_EVERY: u32 = 300;

fn clear_when_world_copy_missing(
    view: ViewQuery<(&ViewTarget, Option<&ExtractedCamera>)>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    blit: Res<BlitPipeline>,
    mut ctx: RenderContext,
    mut missed: Local<u32>,
) {
    let (target, camera) = view.into_inner();
    let Some(camera) = camera else {
        return;
    };
    let CameraOutputMode::Write { blend_state, .. } = camera.output_mode else {
        return;
    };
    // Bevy's own rule: unset, the first camera on a window replaces the screen image and
    // the ones after it alpha-blend. Only a replacing copy leaves stale contents behind
    // when it does not run; a missing blend only drops that camera's layer.
    let blend_state = match blend_state {
        None if camera.sorted_camera_index_for_target > 0 => Some(BlendState::ALPHA_BLENDING),
        other => other,
    };
    if !matches!(blend_state, None | Some(BlendState::REPLACE)) {
        return;
    }
    let (Some(out), Some(target_format)) = (target.out_texture(), target.out_texture_view_format())
    else {
        return;
    };
    // The key Bevy's `prepare_view_upscaling_pipelines` specializes for this view: an
    // already specialized key returns its cached pipeline id.
    let key = BlitPipelineKey {
        target_format,
        blend_state,
        samples: 1,
        source_space: target.compositing_space,
    };
    let id = pipelines.specialize(&pipeline_cache, &blit, key);
    let reason = match pipeline_cache.get_render_pipeline_state(id) {
        CachedPipelineState::Ok(_) => return,
        CachedPipelineState::Queued => "queued (its shader has not loaded)".to_owned(),
        CachedPipelineState::Creating(_) => "still compiling".to_owned(),
        CachedPipelineState::Err(error) => format!("failed: {error}"),
    };
    {
        let _pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("screen_guard_clear"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: out,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(LinearRgba::BLACK.into()),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    *missed += 1;
    if *missed == 1 || *missed % LOG_EVERY == 0 {
        diag::warn!(
            World,
            "screen guard: {} frame(s) where the world camera's copy to the screen did not run; its pipeline is {reason} (format {target_format:?}, source space {:?}); cleared to black",
            *missed,
            target.compositing_space.unwrap_or(CompositingSpace::Linear)
        );
    }
}

fn note_unwritten_screen(
    view: ViewQuery<(&ViewTarget, Option<&ExtractedCamera>)>,
    mut missed: Local<u32>,
) {
    let (target, camera) = view.into_inner();
    let Some(camera) = camera else {
        return;
    };
    // The first camera on its window writes the screen by design.
    if camera.sorted_camera_index_for_target == 0 || target.needs_present() {
        return;
    }
    *missed += 1;
    if *missed == 1 || *missed % LOG_EVERY == 0 {
        diag::warn!(
            World,
            "screen guard: {} frame(s) where the world camera did not write the screen; the UI camera cleared it to black",
            *missed
        );
    }
}
