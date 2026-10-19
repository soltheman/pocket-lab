use std::path::{Component, Path, PathBuf};

use bevy::prelude::*;
use frame::LaunchIdentity;
use render::diag::capture::{CaptureQueue, CaptureRequest};

use crate::{ConsoleCommand, ConsoleLine, ConsoleSettings, ConsoleState};

pub(crate) fn route_capture_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    identity: Option<Res<LaunchIdentity>>,
    mut capture: Option<ResMut<CaptureQueue>>,
    mut recording: ResMut<FrameRecording>,
    mut exit: MessageWriter<AppExit>,
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "record" => {
                let (Some(name), Some(seconds)) = (
                    cmd.args.first(),
                    cmd.args.get(1).and_then(|s| s.parse::<f32>().ok()),
                ) else {
                    echo("usage: record <name> <seconds>".into(), console, line);
                    continue;
                };
                let Some(identity) = identity.as_ref() else {
                    echo("record: launch identity missing".into(), console, line);
                    continue;
                };
                let dir = identity.artifacts.join("screenshots").join(name);
                if let Err(error) = std::fs::create_dir_all(&dir) {
                    echo(format!("record: create {}: {error}", dir.display()), console, line);
                    continue;
                }
                *recording = FrameRecording {
                    dir: Some(dir.clone()),
                    frames: (seconds * RECORD_FPS).round() as u32,
                    next: 0,
                    next_at: None,
                };
                echo(format!("record: {} frames to {}", recording.frames, dir.display()), console, line);
            }
            "screenshot" => {
                if cmd.args.len() > 1 {
                    echo("usage: screenshot [name]".into(), console, line);
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "screenshot: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let path = match screenshot_path(
                    &identity.artifacts,
                    &identity.zone,
                    cmd.args.first().map(String::as_str),
                ) {
                    Ok(path) => path,
                    Err(error) => {
                        echo(format!("screenshot: {error}"), console, line);
                        continue;
                    }
                };
                if let Some(parent) = path.parent()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    echo(
                        format!("screenshot: create {}: {error}", parent.display()),
                        console,
                        line,
                    );
                    continue;
                }
                let Some(queue) = capture.as_deref_mut() else {
                    echo(
                        "screenshot: render capture queue missing (no RenderPlugin)".into(),
                        console,
                        line,
                    );
                    continue;
                };

                queue.push(CaptureRequest {
                    path: path.clone(),
                    exit_after_capture: false,
                });
                echo(
                    format!(
                        "screenshot: queued {} ({} waiting)",
                        path.display(),
                        queue.pending()
                    ),
                    console,
                    line,
                );
            }

            "exit" | "quit" => {
                let (queued, writing) = capture
                    .as_deref()
                    .map(CaptureQueue::owed_at_exit)
                    .unwrap_or((0, 0));
                render::diag::capture::exit_is_user_quit();
                diag::lifecycle_boundary("quit_requested", "");
                if queued + writing > 0 {
                    echo(
                        format!(
                            "quit: leaving {queued} queued and {writing} unfinished screenshot(s) behind"
                        ),
                        console,
                        line,
                    );
                }
                exit.write(AppExit::Success);
            }

            "finish_run" => {
                diag::lifecycle_boundary("quit_requested", " via=finish_run");
                let owed_shots = capture
                    .as_deref_mut()
                    .is_some_and(CaptureQueue::exit_after_drained);
                if owed_shots {
                    echo(
                        "finish_run: waiting for queued screenshots".into(),
                        console,
                        line,
                    );
                } else {
                    exit.write(AppExit::Success);
                }
            }
            _ => {}
        }
    }
}

/// Frames a second `record` captures, in game time.
const RECORD_FPS: f32 = 20.0;

/// `record <name> <seconds>`: the rendered frames (no other window can get in), one
/// every 1/20 s of game time, as `screenshots/<name>/frame_00000.png`...; for the menu's
/// map flybys. Game time keeps the motion even when capturing slows the game.
#[derive(Resource, Default)]
pub(crate) struct FrameRecording {
    dir: Option<PathBuf>,
    frames: u32,
    next: u32,
    next_at: Option<f64>,
}

pub(crate) fn record_frames(
    time: Res<Time<Virtual>>,
    mut recording: ResMut<FrameRecording>,
    capture: Option<ResMut<CaptureQueue>>,
) {
    let Some(dir) = recording.dir.clone() else {
        return;
    };
    let Some(mut capture) = capture else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let due = *recording.next_at.get_or_insert(now);
    if now < due {
        return;
    }
    capture.push(CaptureRequest {
        path: dir.join(format!("frame_{:05}.png", recording.next)),
        exit_after_capture: false,
    });
    recording.next += 1;
    recording.next_at = Some(due + 1.0 / f64::from(RECORD_FPS));
    if recording.next >= recording.frames {
        diag::info!(Console, "record: done, {} frames in {}", recording.next, dir.display());
        recording.dir = None;
    }
}

fn screenshot_path(artifacts: &Path, zone: &str, name: Option<&str>) -> Result<PathBuf, String> {
    let name = name.unwrap_or(zone);
    let relative = Path::new(name);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("name must be relative to iw4l-artifacts/screenshots".into());
    }
    if let Some(extension) = relative.extension()
        && !extension.to_string_lossy().eq_ignore_ascii_case("png")
    {
        return Err("name must use a .png extension".into());
    }
    let mut path = artifacts.join("screenshots").join(relative);
    if path.extension().is_none() {
        path.set_extension("png");
    }
    Ok(path)
}
