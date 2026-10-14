//! Frame pacing on iOS, all switches in `iw4l-env.txt`, read once at startup:
//!
//! - `IW4L_FRAME_PACE=off|0|30|40|60` (default off): hold each present to the
//!   given rate with `presentDrawable:afterMinimumDuration:` in the vendored
//!   wgpu-hal. Off is the plain present.
//! - `IW4L_DT_SNAP=0|1` (default 0, acts only with a pace): advance game time
//!   on the 120 Hz vsync grid (see `diag::frame_pacing::PaceClock`) through
//!   `TimeUpdateStrategy::ManualInstant`, instead of Bevy's sample taken after
//!   the previous present. Without a pace the time update stays automatic.
//! - `IW4L_FRAME_STATS=0|1` (default 1): install the hal's present hooks and log
//!   `frame pacing:` every 128 presents (`render_gpu::diag::frame_pacing`).
//!   0 leaves the hal without hooks.
//!
//! Every other platform registers nothing: the hooks only exist on the phone.
use bevy::prelude::*;

pub(crate) fn register(app: &mut App) {
    #[cfg(target_os = "ios")]
    ios::register(app);
    #[cfg(not(target_os = "ios"))]
    let _ = app;
}

#[cfg(target_os = "ios")]
mod ios {
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    use bevy::prelude::*;
    use bevy::time::{TimeSystems, TimeUpdateStrategy};
    use diag::frame_pacing::{FramePace, PaceClock, lead_ns};
    use render_gpu::diag::frame_pacing as stats;
    use wgpu_hal::metal::pacing;

    pub(super) fn register(app: &mut App) {
        let pace = FramePace::get();
        pacing::MIN_PRESENT_US.store(pace.min_present_us(), Ordering::Relaxed);
        if pace.stats {
            // Once per process. A second app keeps these hooks, which feed the
            // same statics.
            let _ = pacing::HOOKS.set(pacing::Hooks {
                submitted: stats::on_submitted,
                presented: stats::on_presented,
            });
            stats::register(app, pace);
            let paced_us = pacing::MIN_PRESENT_US.load(Ordering::Relaxed);
            app.add_systems(Last, log_gpu_errors).add_systems(
                Last,
                move |trial: Local<GpuTrial>, screen: Option<Res<frame::AppScreen>>| {
                    gpu_trial(trial, screen, paced_us)
                },
            );
        }
        app.add_systems(Last, log_gpu_memory);
        if pace.snap_active() {
            app.insert_resource(PaceTime(PaceClock::new(pace.hz)))
                .add_systems(First, pace_time.before(TimeSystems));
        }
        diag::info!(
            Launch,
            "{}",
            pace.boot_line(crate::plugins::frame_latency())
        );
    }

    /// A GPU command buffer that failed leaves its frame's drawable showing what it
    /// last held (an older frame, or black). Logged as the count grows: the first
    /// failure, then at most once a second.
    fn log_gpu_errors(mut logged: Local<(u32, Option<Instant>)>) {
        let count = pacing::GPU_ERRORS.load(Ordering::Relaxed);
        if count == logged.0 || logged.1.is_some_and(|at| at.elapsed().as_secs() < 1) {
            return;
        }
        let code = pacing::LAST_GPU_ERROR.load(Ordering::Relaxed);
        let text = pacing::LAST_GPU_ERROR_TEXT
            .lock()
            .ok()
            .and_then(|last| last.clone())
            .unwrap_or_default();
        let first = logged.0 == 0;
        let ok = pacing::GPU_OK.load(Ordering::Relaxed);
        diag::warn!(
            World,
            "gpu: {count} command buffer(s) failed so far, {ok} completed; last MTLCommandBufferError code {code} in {text}"
        );
        for (samples, kind) in [(&pacing::FAILED_SAMPLES, "failed"), (&pacing::OK_SAMPLES, "completed")] {
            if let Ok(mut samples) = samples.lock() {
                for line in samples.drain(..) {
                    diag::warn!(World, "gpu: {kind} batch {line}");
                }
            }
        }
        if first {
            diag::warn!(
                World,
                "gpu memory at the first failure: {} MiB held by the device ({})",
                pacing::GPU_ALLOCATED_AT_FIRST_ERROR.load(Ordering::Relaxed) >> 20,
                gpu_memory_line()
            );
        }
        *logged = (count, Some(Instant::now()));
    }

    fn gpu_memory_line() -> String {
        let label = pacing::BINDLESS_MAX_LABEL
            .lock()
            .ok()
            .and_then(|label| label.clone())
            .unwrap_or_default();
        format!(
            "now {} MiB, peak {} MiB, Metal's recommended working set {} MiB; {} bind group(s) with texture arrays, the largest {} resources ({label:?}); most command buffers queued at once {}",
            pacing::GPU_ALLOCATED.load(Ordering::Relaxed) >> 20,
            pacing::GPU_ALLOCATED_PEAK.load(Ordering::Relaxed) >> 20,
            pacing::GPU_WORKING_SET.load(Ordering::Relaxed) >> 20,
            pacing::BINDLESS_GROUPS.load(Ordering::Relaxed),
            pacing::BINDLESS_MAX.load(Ordering::Relaxed),
            pacing::IN_FLIGHT_MAX.load(Ordering::Relaxed),
        )
    }

    /// Every graphics resource of 16 MiB or more as it is made, and what the GPU
    /// device holds in total: once it grows by 64 MiB, and at most every 5 seconds.
    fn log_gpu_memory(mut logged: Local<(u64, Option<Instant>)>) {
        if let Ok(mut big) = pacing::BIG_ALLOCATIONS.lock() {
            for line in big.drain(..) {
                diag::info!(World, "gpu: big allocation {line}");
            }
        }
        let held = pacing::GPU_ALLOCATED.load(Ordering::Relaxed);
        if held.abs_diff(logged.0) < 64 << 20
            || logged.1.is_some_and(|at| at.elapsed().as_secs() < 5)
        {
            return;
        }
        diag::info!(World, "gpu memory: {}", gpu_memory_line());
        *logged = (held, Some(Instant::now()));
    }

    /// The submission changes tried in a match, each for [`TRIAL_SECS`]: their name,
    /// `pacing::MODE` bits, and whether presents stay paced.
    const TRIALS: [(&str, u32, bool); 6] = [
        ("as before", 0, true),
        ("present pacing off", 0, false),
        ("empty batches not sent", pacing::SKIP_EMPTY, true),
        ("each batch sent alone", pacing::ONE_BY_ONE, true),
        ("one frame on the GPU at a time", pacing::ONE_FRAME, true),
        (
            "all of the above",
            pacing::SKIP_EMPTY | pacing::ONE_BY_ONE | pacing::ONE_FRAME,
            false,
        ),
    ];
    const TRIAL_SECS: u64 = 10;
    /// Seconds in the match before the first trial, so loading is over.
    const TRIAL_SETTLE_SECS: u64 = 8;

    #[derive(Default)]
    struct GpuTrial {
        in_match: Option<Instant>,
        /// The trial running, when it started, and the counters then.
        running: Option<(usize, Instant, u32, u32, u32)>,
        /// (trial, failed, completed) for each finished trial.
        results: Vec<(usize, u32, u32)>,
        done: bool,
    }

    fn apply_trial(index: usize, paced_us: u32) {
        let (_, mode, paced) = TRIALS[index];
        pacing::MODE.store(mode, Ordering::Relaxed);
        pacing::MIN_PRESENT_US.store(if paced { paced_us } else { 0 }, Ordering::Relaxed);
    }

    /// Once in a match, runs each of [`TRIALS`] for a while and logs how many command
    /// buffers failed under it; then keeps the one with the fewest failures.
    fn gpu_trial(
        mut trial: Local<GpuTrial>,
        screen: Option<Res<frame::AppScreen>>,
        paced_us: u32,
    ) {
        if trial.done {
            return;
        }
        let in_match = matches!(
            screen.as_deref(),
            Some(frame::AppScreen::ClassSelect | frame::AppScreen::InGame)
        );
        if !in_match {
            return;
        }
        let now = Instant::now();
        let entered = *trial.in_match.get_or_insert(now);
        let counters = || {
            (
                pacing::GPU_ERRORS.load(Ordering::Relaxed),
                pacing::GPU_OK.load(Ordering::Relaxed),
                pacing::SKIPPED.load(Ordering::Relaxed),
            )
        };
        let Some((index, started, failed0, ok0, skipped0)) = trial.running else {
            if now.duration_since(entered).as_secs() >= TRIAL_SETTLE_SECS {
                let (failed, ok, skipped) = counters();
                apply_trial(0, paced_us);
                pacing::FAILED_SAMPLE_BUDGET.store(4, Ordering::Relaxed);
                trial.running = Some((0, now, failed, ok, skipped));
                diag::warn!(World, "gpu test: trying {}", TRIALS[0].0);
            }
            return;
        };
        if now.duration_since(started).as_secs() < TRIAL_SECS {
            return;
        }
        let (failed, ok, skipped) = counters();
        let (failed, ok, skipped) = (failed - failed0, ok - ok0, skipped - skipped0);
        diag::warn!(
            World,
            "gpu test: {}: {failed} batch(es) failed, {ok} completed, {skipped} empty not sent ({}% failed)",
            TRIALS[index].0,
            if failed + ok == 0 { 0 } else { 100 * failed / (failed + ok) }
        );
        trial.results.push((index, failed, ok));
        let next = index + 1;
        if next < TRIALS.len() {
            apply_trial(next, paced_us);
            pacing::FAILED_SAMPLE_BUDGET.store(4, Ordering::Relaxed);
            let (failed, ok, skipped) = counters();
            trial.running = Some((next, Instant::now(), failed, ok, skipped));
            diag::warn!(World, "gpu test: trying {}", TRIALS[next].0);
            return;
        }
        // Fewest failed per completed; ties go to the earlier (cheaper) one.
        let rate = |&(_, failed, ok): &(usize, u32, u32)| {
            u64::from(failed) * 1_000_000 / u64::from((failed + ok).max(1))
        };
        let best = trial
            .results
            .iter()
            .min_by_key(|result| rate(result))
            .map_or(0, |&(index, ..)| index);
        apply_trial(best, paced_us);
        trial.running = None;
        trial.done = true;
        diag::warn!(World, "gpu test: done; keeping {}", TRIALS[best].0);
    }

    #[derive(Resource)]
    struct PaceTime(PaceClock);

    fn pace_time(mut clock: ResMut<PaceTime>, mut strategy: ResMut<TimeUpdateStrategy>) {
        let now = Instant::now();
        let at = clock.0.next(now);
        stats::FRAME_LEAD_NS.store(lead_ns(at, now), Ordering::Relaxed);
        *strategy = TimeUpdateStrategy::ManualInstant(at);
    }
}
