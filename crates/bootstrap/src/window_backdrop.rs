//! A black backdrop behind the game's view (iOS).
//!
//! Until the renderer presents its first frame, and around any frame it has not
//! presented yet, UIKit shows what is behind the Metal layer: the window and the root
//! view, which UIKit leaves without a colour. Players saw a pink flash there before the
//! loading screen. The window and its root view are painted black as soon as they
//! exist; every other platform registers nothing.
use bevy::prelude::*;

pub(crate) fn register(app: &mut App) {
    #[cfg(target_os = "ios")]
    app.insert_non_send(ios::MainThread)
        .add_systems(First, ios::paint_backdrop);
    #[cfg(not(target_os = "ios"))]
    let _ = app;
}

#[cfg(target_os = "ios")]
mod ios {
    use bevy::prelude::*;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    /// NonSend marker: UIKit is touched on the main thread only.
    pub(super) struct MainThread;

    /// Frames to keep looking for a window before giving up.
    const TRIES: u32 = 600;

    pub(super) fn paint_backdrop(_main: NonSend<MainThread>, mut tries: Local<u32>) {
        if *tries >= TRIES {
            return;
        }
        *tries += 1;
        // SAFETY: plain UIKit messages with their documented signatures, on the main
        // thread (NonSend): -[UIApplication windows] is an NSArray of UIWindow, and
        // -setBackgroundColor: takes a UIColor.
        let painted = unsafe {
            let app: Option<Retained<AnyObject>> =
                msg_send![class!(UIApplication), sharedApplication];
            let Some(app) = app else {
                return;
            };
            let windows: Option<Retained<AnyObject>> = msg_send![&*app, windows];
            let Some(windows) = windows else {
                return;
            };
            let count: usize = msg_send![&*windows, count];
            let black: Option<Retained<AnyObject>> = msg_send![class!(UIColor), blackColor];
            let Some(black) = black else {
                return;
            };
            for index in 0..count {
                let window: Option<Retained<AnyObject>> =
                    msg_send![&*windows, objectAtIndex: index];
                let Some(window) = window else {
                    continue;
                };
                let _: () = msg_send![&*window, setBackgroundColor: &*black];
                let controller: Option<Retained<AnyObject>> =
                    msg_send![&*window, rootViewController];
                if let Some(controller) = controller {
                    let view: Option<Retained<AnyObject>> = msg_send![&*controller, view];
                    if let Some(view) = view {
                        let _: () = msg_send![&*view, setBackgroundColor: &*black];
                    }
                }
            }
            count
        };
        if painted > 0 {
            *tries = TRIES;
            diag::boot_crumb(&format!("window backdrop: black ({painted} window(s))"));
        }
    }
}
