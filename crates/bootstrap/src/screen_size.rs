//! Keep the primary window at the size of its native window (iOS).
//!
//! Bevy sizes a new window from the requested resolution times the scale factor and
//! learns the real size only from a resize event. iOS hands the app its screen and sends
//! none, so a 1280x720 request stayed 1280x720 logical (3840x2160 pixels at 3x) on a
//! 2532x1170 iPhone 16e: everything drew too big and only the top-left of each screen
//! showed. Other platforms register nothing.
use bevy::prelude::*;

pub(crate) fn register(app: &mut App) {
    #[cfg(target_os = "ios")]
    app.add_systems(First, follow_native_window);
    #[cfg(not(target_os = "ios"))]
    let _ = app;
}

#[cfg(target_os = "ios")]
fn follow_native_window(
    _main: bevy::ecs::system::NonSendMarker,
    mut windows: Query<(Entity, &mut Window), With<bevy::window::PrimaryWindow>>,
    mut logged: Local<Option<(u32, u32, u32)>>,
) {
    let Ok((entity, mut window)) = windows.single_mut() else {
        return;
    };
    let native = bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        windows.get_window(entity).map(|native| {
            // outer: the whole screen the view and its Metal layer cover, where touches
            // land. winit's inner size on iOS is the safe area (750x369 pt of an 844x390
            // pt iPhone 16e), which put the touch layout and the drawn one apart.
            let size = native.outer_size();
            (size.width, size.height, native.scale_factor() as f32)
        })
    });
    let Some((width, height, scale)) = native else {
        return;
    };
    if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let resolution = &window.resolution;
    if resolution.physical_width() == width
        && resolution.physical_height() == height
        && resolution.scale_factor() == scale
    {
        return;
    }
    window.resolution.set_scale_factor(scale);
    window.resolution.set_physical_resolution(width, height);
    let key = (width, height, scale.to_bits());
    if *logged != Some(key) {
        *logged = Some(key);
        diag::boot_note(&format!(
            "screen: window follows the native window {width}x{height} px at {scale}x ({}x{} pt)",
            (width as f32 / scale).round(),
            (height as f32 / scale).round()
        ));
    }
}
