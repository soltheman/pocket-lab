use bevy::prelude::*;
use render_fx::FxDumpRequest;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

pub(crate) fn register_debug_fx_commands(registry: &mut ConsoleRegistry) {
    if registry.resolve("fx_dump").is_none() {
        registry.register(
            crate::CommandSpec::new("fx_dump")
                .usage("fx_dump [radius] — log nearby CodeMesh sprites (needs a live world)"),
        );
    }
    if registry.resolve("fx_play").is_none() {
        registry.register(
            crate::CommandSpec::new("fx_play")
                .usage("fx_play <effect> [distance] — play an effect in front of the camera"),
        );
    }
}

/// `fx_play <effect> [distance]`: queues the effect for the FX presenter.
pub(crate) fn route_fx_play_command(
    mut events: MessageReader<ConsoleCommand>,
    request: Option<ResMut<render_fx::FxPlayRequest>>,
) {
    let Some(mut request) = request else {
        return;
    };
    for cmd in events.read() {
        if cmd.name != "fx_play" {
            continue;
        }
        let Some(name) = cmd.args.first() else {
            continue;
        };
        let distance = cmd
            .args
            .get(1)
            .and_then(|d| d.parse::<f32>().ok())
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(40.0);
        request.pending = Some((name.clone(), distance));
    }
}

pub(crate) fn route_debug_fx_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut dump: Option<ResMut<FxDumpRequest>>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        if cmd.name != "fx_dump" {
            continue;
        }
        let Some(dump) = dump.as_deref_mut() else {
            echo(
                "fx_dump: render FX dump request missing (no RenderPlugin)".into(),
                &mut console,
                &mut line,
            );
            continue;
        };
        match cmd.args.as_slice() {
            [] => {}
            [radius] => match radius.parse::<f32>() {
                Ok(value) if value.is_finite() && value > 0.0 => dump.radius = value,
                _ => {
                    echo(
                        format!("usage: fx_dump [radius] (got `{radius}`)"),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
            },
            _ => {
                echo("usage: fx_dump [radius]".into(), &mut console, &mut line);
                continue;
            }
        }
        dump.pending = true;
        echo(
            format!(
                "fx_dump: queued radius={:.0} (next Effects tick)",
                dump.radius
            ),
            &mut console,
            &mut line,
        );
    }
}
