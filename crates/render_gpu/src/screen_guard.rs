//! Counts frames on which a camera that should write the screen image first did not.
//!
//! Cameras stack on the window: the world camera copies its image over the whole
//! screen (it clears first), then the UI camera blends its image on top. When the world
//! camera's copy does not run, the UI camera is the first to touch the screen; it then
//! clears to black (see `spawn_overlay_ui_camera` in the ui crate) where it used to
//! blend over the image's old contents: an older frame (a doubled HUD) or, on Apple
//! GPUs, uninitialised memory (magenta). This only records that it happened.
use bevy::render::camera::ExtractedCamera;
use bevy::core_pipeline::Core2d;
use bevy::core_pipeline::upscaling::upscaling;
use bevy::prelude::*;
use bevy::render::renderer::ViewQuery;
use bevy::render::view::ViewTarget;

pub(crate) fn register(render_app: &mut SubApp) {
    render_app.add_systems(Core2d, note_unwritten_screen.before(upscaling));
}

/// Frames counted between log lines, after the first.
const LOG_EVERY: u32 = 300;

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
