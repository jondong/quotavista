//! Application lifecycle policy: what closing, waking, and quitting do.
//!
//! The decisions live here, away from the Tauri plumbing that observes the
//! platform, so they can be tested without a window server. [`crate::platform`]
//! detects capabilities, this module turns a detected capability plus the
//! persisted settings into behaviour, and the `run` loop in the crate root only
//! translates platform signals into these calls.
//!
//! Two rules from the requirements drive everything below:
//!
//! * Tray availability is a detected capability, not an assumption about a
//!   desktop environment. When no usable tray exists, closing the main window
//!   exits the application instead of leaving a collector the user can no
//!   longer reach.
//! * With automatic refresh off, only an explicit user action may contact a
//!   provider. Waking the application is not an explicit user action.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use quotavista_contracts::{LocalId, SafeError, TrayAvailabilityDto};
use quotavista_core::storage::{Repository, WriteContext};
use quotavista_infrastructure::refresh::{Admission, Outcome};

use crate::{publish_change, storage_error, AppState, MAX_REFRESH_STEPS};

/// What closing the main window does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseDecision {
    /// A usable tray exists, so the window hides and the collector stays
    /// reachable through it.
    HideToTray,
    /// No usable tray: the application exits rather than leaving a process the
    /// user can no longer reach.
    ExitApplication,
}

/// Decide what closing the main window does from the tray capability that was
/// actually detected at start-up.
pub fn close_decision(tray: TrayAvailabilityDto) -> CloseDecision {
    match tray {
        TrayAvailabilityDto::Available => CloseDecision::HideToTray,
        TrayAvailabilityDto::Unavailable => CloseDecision::ExitApplication,
    }
}

/// The persisted refresh setting that governs whether anything may contact a
/// provider on its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshMode {
    /// Automatic refresh is on with this interval in minutes.
    Automatic(u8),
    /// Automatic refresh is off: only an explicit user action may query.
    Off,
}

impl RefreshMode {
    /// Read the mode from the persisted settings. `None` means off, which is
    /// what the prototype ships with.
    pub fn from_minutes(minutes: Option<u8>) -> Self {
        match minutes {
            Some(minutes) => Self::Automatic(minutes),
            None => Self::Off,
        }
    }
}

/// What waking the application may do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeDecision {
    /// One merged refresh is permitted, still subject to the coordinator's own
    /// interval and cooldown rules.
    MergePermittedRefresh,
    /// Nothing may be initiated; the application stays idle.
    StayIdle,
}

/// Decide what a wake may do from the persisted refresh mode.
pub fn wake_decision(mode: RefreshMode) -> WakeDecision {
    match mode {
        RefreshMode::Automatic(_) => WakeDecision::MergePermittedRefresh,
        RefreshMode::Off => WakeDecision::StayIdle,
    }
}

/// The platform signal that woke the application.
///
/// Tauri 2.12 emits no desktop suspend, resume, or wake event: the `Suspended`
/// and `Resumed` window events exist on mobile targets only. Wake therefore
/// comes from two independent places - the reactivation signals a window server
/// does emit, and [`ResumeMonitor`], which detects the suspend gap itself by
/// comparing clocks. The monitor is what covers a resume while the application
/// stays hidden in the tray, where no window event fires at all. Each signal is
/// named in the report so the native targets can be compared with each other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeSource {
    /// The main window regained focus.
    WindowFocused,
    /// macOS: the application was reactivated while no window was visible.
    ReopenWithoutVisibleWindow,
    /// A second launch attempt handed over to the running instance.
    SingleInstanceActivation,
    /// A clock comparison showed the process had been suspended.
    ///
    /// The only source that does not depend on window activation, so it is what
    /// makes "refresh after wake" work while the main window stays hidden.
    ResumeAfterSuspend,
}

/// Minimum spacing between two wakes that are acted on.
pub const WAKE_GATE_INTERVAL: Duration = Duration::from_secs(5);

/// Debounce for reactivation signals.
///
/// These signals arrive in bursts: showing a hidden window raises a focus
/// event, and a single-instance handover raises both a focus event and a
/// reactivation. The coordinator would coalesce the resulting refreshes anyway,
/// but the gate keeps "one user action, one wake" explicit instead of relying
/// on a downstream side effect.
#[derive(Debug)]
pub struct WakeGate {
    last: Option<Instant>,
    min_interval: Duration,
}

impl WakeGate {
    /// A gate that has not admitted anything yet.
    pub fn new(min_interval: Duration) -> Self {
        Self {
            last: None,
            min_interval,
        }
    }

    /// Whether one more wake may be acted on.
    ///
    /// The clock is monotonic, so a wall-clock jump can neither suppress wakes
    /// forever nor let a burst through. A reading that somehow goes backwards
    /// is treated as an immediate repeat and refused.
    pub fn admit(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last)
                if now.checked_duration_since(last).unwrap_or_default() < self.min_interval =>
            {
                false
            }
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// Raised once by quit, and read by anything that could still start or continue
/// lifecycle work.
///
/// Without it a wake spawned just before the user chose Quit would take the
/// refresh lock that shutdown needs, so quitting would wait for a bounded pump
/// loop nobody wants any more. Raising the flag first is what keeps the Quit
/// action reachable while refresh work is in flight.
#[derive(Debug, Default)]
pub struct ShutdownSignal(AtomicBool);

impl ShutdownSignal {
    /// A signal that has not been raised.
    pub fn new() -> Self {
        Self::default()
    }

    /// Tell every lifecycle path to stop starting work.
    pub fn raise(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether quitting has begun.
    pub fn raised(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How often the resume monitor samples the clocks.
///
/// Short enough that a resume is noticed soon after it happens, long enough that
/// sampling costs nothing next to the refresh work it may trigger.
pub const RESUME_TICK: Duration = Duration::from_secs(30);

/// A clock advance of at least this much counts as a suspend gap.
///
/// Four ticks, so scheduler jitter, a busy event loop, or a paused debugger
/// cannot trigger a refresh. A real sleep is minutes to hours.
pub const RESUME_THRESHOLD: Duration = Duration::from_secs(120);

/// One reading of the two clocks the resume monitor compares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResumeSample {
    /// Monotonic reading, as an offset from a process-wide base.
    pub monotonic: Duration,
    /// Wall-clock reading since the Unix epoch.
    pub wall: Duration,
}

/// The process-wide base for monotonic readings, so a sample is a plain
/// `Duration` that a test can construct without waiting for anything.
fn monotonic_base() -> &'static Instant {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    BASE.get_or_init(Instant::now)
}

/// Take one sample now.
pub fn now_sample() -> ResumeSample {
    ResumeSample {
        monotonic: monotonic_base().elapsed(),
        wall: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default(),
    }
}

/// Detects a system suspend by comparing two clock readings.
///
/// Tauri offers no desktop resume event, and reactivation cannot be the whole
/// story: a machine can sleep and wake with the main window hidden in the tray
/// and never gain focus, so the permitted post-wake refresh would never run.
/// This monitor is the desktop resume signal. It samples on a timer and looks
/// for a gap far larger than the tick, which is what a suspend leaves behind.
///
/// Both clocks are compared and either one advancing is enough. Rust's `Instant`
/// is `mach_absolute_time` on macOS, which does not advance during sleep, but
/// `CLOCK_MONOTONIC` on Linux, whose behaviour across suspend varies by kernel;
/// the wall clock always advances. Accepting either signal means the monitor
/// never has to assume which platform it is on. A wall-clock step (NTP, a manual
/// change) is at worst one extra wake, and the refresh mode plus the
/// coordinator's own interval still govern whether that wake does anything.
#[derive(Debug)]
pub struct ResumeMonitor {
    tick: Duration,
    threshold: Duration,
    previous: Option<ResumeSample>,
}

impl ResumeMonitor {
    /// A monitor that has not sampled yet.
    pub fn new(tick: Duration, threshold: Duration) -> Self {
        Self {
            tick,
            threshold,
            previous: None,
        }
    }

    /// How often [`ResumeMonitor::observe`] should be called.
    pub fn tick(&self) -> Duration {
        self.tick
    }

    /// Record one sample and report a suspend gap if the clocks show one.
    ///
    /// The first sample only establishes a baseline. Saturating subtraction makes
    /// a clock that reads backwards - a wall-clock step back, or a monotonic base
    /// from a different process - look like no time at all, so it can never
    /// trigger a refresh on its own.
    pub fn observe(&mut self, sample: ResumeSample) -> Option<Duration> {
        let previous = self.previous.replace(sample)?;
        let monotonic = sample.monotonic.saturating_sub(previous.monotonic);
        let wall = sample.wall.saturating_sub(previous.wall);
        let advanced = monotonic.max(wall);
        (advanced >= self.threshold).then_some(advanced)
    }
}

/// Act on one resume sample.
///
/// When the monitor sees a suspend gap this runs exactly the gated wake path
/// every other resume signal uses, so refresh-Off still starts nothing and a
/// burst of samples still produces one merged refresh. `None` means no gap was
/// detected, or the wake gate swallowed it.
pub async fn observe_resume(
    state: &AppState,
    gate: &std::sync::Mutex<WakeGate>,
    monitor: &mut ResumeMonitor,
    sample: ResumeSample,
) -> Result<Option<WakeReport>, SafeError> {
    if monitor.observe(sample).is_none() {
        return Ok(None);
    }
    handle_gated_wake(state, gate, WakeSource::ResumeAfterSuspend).await
}

/// Sample the clocks until the application is quitting.
///
/// Runs on the composition runtime, never on the event-loop thread, so a slow
/// provider or a contended database cannot freeze window interaction.
pub async fn drive_resume_monitor(
    state: &AppState,
    gate: &std::sync::Mutex<WakeGate>,
    monitor: ResumeMonitor,
) {
    let mut monitor = monitor;
    let tick = monitor.tick();
    loop {
        tokio::time::sleep(tick).await;
        if state.shutdown_signal.raised() {
            return;
        }
        match observe_resume(state, gate, &mut monitor, now_sample()).await {
            Ok(Some(report)) => eprintln!("quotavista: {}", report.diagnostic()),
            Ok(None) => {}
            Err(error) => eprintln!("quotavista: resume refresh skipped: {}", error.message()),
        }
    }
}

/// What one wake did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WakeReport {
    /// The signal that woke the application.
    pub source: WakeSource,
    /// The refresh mode read from the persisted settings.
    pub mode: RefreshMode,
    /// Whether any refresh work was permitted at all.
    pub permitted: bool,
    /// Coordinator job ids the wake merged into; empty when nothing was due.
    pub launched: Vec<LocalId>,
    /// Connections that were not due, or that the coordinator refused.
    pub refused: usize,
}

impl WakeReport {
    /// One-line diagnostic for a wake. It names the signal, the persisted mode,
    /// and counts only: never a connection id, alias, or account.
    pub fn diagnostic(&self) -> String {
        format!(
            "wake {:?} mode {:?} permitted {} merged {} not-due {}",
            self.source,
            self.mode,
            self.permitted,
            self.launched.len(),
            self.refused
        )
    }
}

/// Merge the refresh work one wake permits.
///
/// With automatic refresh off this returns before the coordinator is touched at
/// all, so an Off application never initiates an implicit query: no job is
/// created, no provider is reachable, and no revision moves. With automatic
/// refresh on the wake asks the coordinator for one merged refresh and the
/// coordinator still decides per connection whether its interval has elapsed,
/// so a wake never replays missed timers.
pub async fn handle_wake(state: &AppState, source: WakeSource) -> Result<WakeReport, SafeError> {
    let settings = state.repository.settings().await.map_err(storage_error)?;
    let mode = RefreshMode::from_minutes(settings.refresh_minutes);
    if wake_decision(mode) == WakeDecision::StayIdle {
        return Ok(WakeReport {
            source,
            mode,
            permitted: false,
            launched: Vec::new(),
            refused: 0,
        });
    }

    let connections = state
        .repository
        .connections()
        .await
        .map_err(storage_error)?;
    let requests: Vec<(String, WriteContext)> = connections
        .iter()
        .map(|connection| {
            (
                connection.id.clone(),
                WriteContext {
                    connection_id: connection.id.clone(),
                    generation: connection.generation,
                },
            )
        })
        .collect();
    let request_refs: Vec<(&str, WriteContext)> = requests
        .iter()
        .map(|(id, context)| (id.as_str(), context.clone()))
        .collect();

    let revision_before = state
        .repository
        .data_revision()
        .await
        .map_err(storage_error)?;

    // Job id paired with the connection it belongs to, so a committed job can
    // name the scope its change event covers.
    let mut admitted: Vec<(String, String)> = Vec::new();
    let mut refused = 0usize;
    let mut affected: Vec<LocalId> = Vec::new();
    {
        let mut refresh = state.refresh.lock().await;
        let admissions = refresh.tick(&request_refs).await;
        for (admission, (connection_id, _)) in admissions.iter().zip(requests.iter()) {
            match admission {
                Admission::Admitted { job } | Admission::Coalesced { job } => {
                    state
                        .jobs
                        .lock()
                        .await
                        .insert(job.clone(), connection_id.clone());
                    admitted.push((job.clone(), connection_id.clone()));
                }
                Admission::CooldownBlocked { .. } | Admission::Refused => refused += 1,
            }
        }

        // Drive the merged work far enough that a committed revision can be
        // announced. The bound only stops a provider that never terminates;
        // whatever it leaves in flight is what quitting then cancels.
        for _ in 0..MAX_REFRESH_STEPS {
            // Quit wins over finishing. Breaking here releases the lock so
            // shutdown can cancel instead of waiting for the remaining steps.
            if state.shutdown_signal.raised() {
                break;
            }
            let running = admitted
                .iter()
                .any(|(job, _)| refresh.outcome(job).is_none());
            if !running {
                break;
            }
            refresh.pump().await;
        }
        for (job, connection_id) in &admitted {
            if matches!(refresh.outcome(job), Some(Outcome::Completed { .. })) {
                if let Ok(local_id) = LocalId::try_from(connection_id.clone()) {
                    affected.push(local_id);
                }
            }
        }
    }

    publish_change(state, revision_before, affected).await?;

    let launched = admitted
        .into_iter()
        .filter_map(|(job, _)| LocalId::try_from(job).ok())
        .collect();
    Ok(WakeReport {
        source,
        mode,
        permitted: true,
        launched,
        refused,
    })
}

/// Apply the wake gate and, unless the signal is part of a burst, merge the
/// permitted refresh work. `None` means the signal was not acted on: either the
/// gate swallowed it as part of a burst, or quitting has already begun and
/// starting work would only create something shutdown has to cancel.
pub async fn handle_gated_wake(
    state: &AppState,
    gate: &std::sync::Mutex<WakeGate>,
    source: WakeSource,
) -> Result<Option<WakeReport>, SafeError> {
    // Checked before the gate so a wake that arrives while quitting neither
    // consumes the gate's slot nor competes with shutdown for the refresh lock.
    if state.shutdown_signal.raised() {
        return Ok(None);
    }
    let admitted = gate
        .lock()
        .expect("the wake gate never stays poisoned")
        .admit(Instant::now());
    if !admitted {
        return Ok(None);
    }
    handle_wake(state, source).await.map(Some)
}

/// What quitting finished.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    /// Coordinator jobs that were cancelled instead of being left in flight.
    pub cancelled: Vec<String>,
    /// Jobs still occupying a concurrency slot afterwards; always zero.
    pub running_after: usize,
    /// Jobs still waiting for a slot afterwards; always zero.
    pub pending_after: usize,
}

/// Cancel in-flight work, then leave committed persistence durable.
///
/// Quitting never abandons a half-applied write. Every live coordinator job is
/// moved to a terminal outcome, so a job that already committed its current
/// values keeps them and a job that was still staging history abandons its
/// candidate and leaves the published partition untouched. The step drives no
/// pump, so it cannot commit anything new and announces no change event: every
/// commit is announced by the command path that made it.
///
/// The first thing it does is raise [`ShutdownSignal`], so a wake that is still
/// running lets go of the refresh lock instead of making the user's Quit action
/// wait for the rest of a bounded pump loop.
pub async fn shutdown(state: &AppState) -> ShutdownReport {
    // Raised before anything else: a concurrently spawned wake stops asking for
    // the refresh lock instead of racing the cancellation below, and the resume
    // monitor returns at its next sample.
    state.shutdown_signal.raise();
    let report = {
        let mut refresh = state.refresh.lock().await;
        let cancelled: Vec<String> = refresh
            .job_ids()
            .into_iter()
            .filter_map(|job| refresh.cancel(&job).map(|_| job))
            .collect();
        ShutdownReport {
            cancelled,
            running_after: refresh.running(),
            pending_after: refresh.pending(),
        }
    };

    // Each committed transaction is already durable on disk; closing the pools
    // only makes sure no write is still in flight when the process exits.
    state.repository.close().await;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use quotavista_contracts::ErrorCode;
    use quotavista_core::domain::Lifecycle;
    use quotavista_core::storage::Settings;
    use quotavista_providers::simulated::SimulatedAdapter;
    use quotavista_providers::synthetic_providers;

    use crate::tests::{fixture, seed_connection, seed_connection_with_id, CONNECTION_ID};
    use crate::{build_with_provider, ChangeSink, RecordingChangeSink, SecretChoice};

    /// Turn automatic refresh on. The prototype ships with it off, so every
    /// wake test states which mode it is exercising.
    async fn set_automatic_refresh(state: &AppState, minutes: u8) {
        let current = state.repository.settings().await.expect("settings");
        let updated = Settings {
            refresh_minutes: Some(minutes),
            ..current
        };
        state
            .repository
            .save_settings(&updated)
            .await
            .expect("settings must save");
    }

    /// A composition on a caller-owned root, so a test can rebuild it after a
    /// shutdown and observe what survived.
    async fn composition_at(root: std::path::PathBuf) -> (AppState, Arc<RecordingChangeSink>) {
        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        let state = build_with_provider(
            root,
            changes,
            SecretChoice::SessionOnly,
            SimulatedAdapter::new(synthetic_providers()[0]),
        )
        .await
        .expect("the composition must initialise");
        (state, sink)
    }

    #[test]
    fn closing_the_main_window_exits_when_no_tray_was_detected() {
        // Without a usable tray there is no way back into a hidden collector,
        // so the window close has to be the application exit.
        assert_eq!(
            close_decision(TrayAvailabilityDto::Unavailable),
            CloseDecision::ExitApplication
        );
        assert_eq!(
            close_decision(TrayAvailabilityDto::Available),
            CloseDecision::HideToTray
        );
    }

    #[test]
    fn wake_is_permitted_only_when_automatic_refresh_is_on() {
        assert_eq!(RefreshMode::from_minutes(None), RefreshMode::Off);
        assert_eq!(wake_decision(RefreshMode::Off), WakeDecision::StayIdle);
        for minutes in [1u8, 5, 15] {
            assert_eq!(
                RefreshMode::from_minutes(Some(minutes)),
                RefreshMode::Automatic(minutes)
            );
            assert_eq!(
                wake_decision(RefreshMode::Automatic(minutes)),
                WakeDecision::MergePermittedRefresh
            );
        }
    }

    #[test]
    fn the_wake_gate_swallows_a_burst_of_reactivation_signals() {
        let mut gate = WakeGate::new(Duration::from_secs(5));
        let start = Instant::now();
        assert!(gate.admit(start), "the first wake is always acted on");
        assert!(!gate.admit(start), "an immediate repeat is a burst");
        assert!(!gate.admit(start + Duration::from_secs(4)));
        assert!(
            gate.admit(start + Duration::from_secs(5)),
            "once the interval elapsed the next wake counts again"
        );
        assert!(!gate.admit(start + Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn refresh_off_never_initiates_an_implicit_query_on_wake() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let settings = state.repository.settings().await.expect("settings");
        assert_eq!(
            settings.refresh_minutes, None,
            "the prototype ships with automatic refresh off"
        );
        let revision = state.repository.data_revision().await.expect("revision");

        let report = handle_wake(&state, WakeSource::WindowFocused)
            .await
            .expect("an idle wake must not fail");

        assert_eq!(report.mode, RefreshMode::Off);
        assert!(!report.permitted);
        assert!(report.launched.is_empty());
        assert_eq!(report.refused, 0, "nothing was offered to the coordinator");
        // The diagnostic a native run prints names counts only.
        assert!(!report.diagnostic().contains(CONNECTION_ID));

        // No job exists, so no provider call was reachable.
        {
            let refresh = state.refresh.lock().await;
            assert!(refresh.job_ids().is_empty());
            assert_eq!(refresh.running(), 0);
            assert_eq!(refresh.pending(), 0);
        }
        assert_eq!(
            state.repository.data_revision().await.expect("revision"),
            revision,
            "an idle wake must not move the store revision"
        );
        assert!(sink.events().is_empty(), "an idle wake announces nothing");
    }

    #[tokio::test]
    async fn the_coordinator_also_refuses_a_scheduled_refresh_while_off() {
        // Second layer: even a caller that reaches the coordinator directly is
        // refused while automatic refresh is off.
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let context = WriteContext {
            connection_id: CONNECTION_ID.to_owned(),
            generation: 0,
        };

        let mut refresh = state.refresh.lock().await;
        let admissions = refresh.tick(&[(CONNECTION_ID, context)]).await;
        assert_eq!(admissions, vec![Admission::Refused]);
        assert!(refresh.job_ids().is_empty());
    }

    #[tokio::test]
    async fn wake_merges_one_refresh_and_a_second_wake_adds_nothing() {
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        set_automatic_refresh(&state, 1).await;
        let revision = state.repository.data_revision().await.expect("revision");

        let first = handle_wake(&state, WakeSource::WindowFocused)
            .await
            .expect("wake");
        assert_eq!(first.mode, RefreshMode::Automatic(1));
        assert!(first.permitted);
        assert_eq!(
            first.launched.len(),
            1,
            "the merged refresh admitted one job"
        );
        assert_eq!(first.refused, 0);

        let committed = state.repository.data_revision().await.expect("revision");
        assert!(committed > revision, "the merged refresh must commit");
        assert_eq!(
            sink.events().len(),
            1,
            "the wake announces the commit it made"
        );

        // A second wake inside the same interval is not due, so it merges
        // nothing instead of replaying a missed timer.
        let second = handle_wake(&state, WakeSource::ReopenWithoutVisibleWindow)
            .await
            .expect("wake");
        assert!(second.permitted, "automatic refresh is still on");
        assert!(second.launched.is_empty());
        assert_eq!(second.refused, 1);
        assert_eq!(
            sink.events().len(),
            1,
            "a wake that commits nothing announces nothing"
        );
        assert_eq!(
            state.repository.data_revision().await.expect("revision"),
            committed
        );
    }

    #[tokio::test]
    async fn the_wake_gate_turns_a_reactivation_burst_into_one_wake() {
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        set_automatic_refresh(&state, 1).await;
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));

        let first = handle_gated_wake(&state, &gate, WakeSource::WindowFocused)
            .await
            .expect("wake");
        let report = first.expect("the first signal is acted on");
        assert_eq!(report.launched.len(), 1);

        let second = handle_gated_wake(&state, &gate, WakeSource::SingleInstanceActivation)
            .await
            .expect("wake");
        assert!(
            second.is_none(),
            "a burst must not reach the coordinator again"
        );
    }

    #[tokio::test]
    async fn quit_cancels_the_work_a_wake_left_in_flight() {
        let (_dir, state, sink) = fixture().await;
        // More connections than the concurrency budget can carry inside the
        // step bound, so the wake necessarily leaves jobs in flight.
        for index in 0..24 {
            seed_connection_with_id(
                &state,
                &format!("synthetic-connection-{index}"),
                Lifecycle::Active,
            )
            .await;
        }
        set_automatic_refresh(&state, 1).await;

        let report = handle_wake(&state, WakeSource::SingleInstanceActivation)
            .await
            .expect("wake");
        assert_eq!(report.launched.len(), 24);
        let live = {
            let refresh = state.refresh.lock().await;
            refresh.running() + refresh.pending()
        };
        assert!(live > 0, "the step bound must leave work in flight");

        let events_before = sink.events().len();

        let quit = shutdown(&state).await;
        assert!(
            !quit.cancelled.is_empty(),
            "in-flight jobs must be cancelled, not abandoned"
        );
        assert_eq!(quit.running_after, 0);
        assert_eq!(quit.pending_after, 0);

        // Quitting announced nothing of its own: every commit is announced by
        // the command path that made it. The pools are closed by now, so the
        // restart test below is what proves the committed data survived.
        assert_eq!(sink.events().len(), events_before);

        // Every job the wake admitted now has a terminal outcome, so nothing is
        // left half-applied for a later run to trip over.
        let refresh = state.refresh.lock().await;
        for job in refresh.job_ids() {
            assert!(
                refresh.outcome(&job).is_some(),
                "job {job} must be terminal after quit"
            );
        }
    }

    #[tokio::test]
    async fn quitting_twice_is_safe() {
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;

        let first = shutdown(&state).await;
        assert!(first.cancelled.is_empty(), "nothing was in flight");
        let second = shutdown(&state).await;
        assert_eq!(second, first);
    }

    #[tokio::test]
    async fn quit_leaves_committed_data_readable_after_a_restart() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().to_path_buf();
        let committed;
        {
            let (state, _sink) = composition_at(root.clone()).await;
            seed_connection(&state, Lifecycle::Active).await;
            set_automatic_refresh(&state, 1).await;
            let report = handle_wake(&state, WakeSource::WindowFocused)
                .await
                .expect("wake");
            assert_eq!(report.launched.len(), 1);
            committed = state.repository.data_revision().await.expect("revision");
            assert!(committed > 0, "the refresh must commit before quitting");
            let quit = shutdown(&state).await;
            assert_eq!(quit.running_after, 0);
        }

        // Rebuilding on the same root re-runs journal recovery before the
        // scheduler is opened, and the committed data is still there. The
        // session-only credential store is empty again, which is the documented
        // session-only restart behaviour.
        let (state, sink) = composition_at(root).await;
        let connections = state.repository.connections().await.expect("connections");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].id, CONNECTION_ID);
        assert_eq!(
            state.repository.data_revision().await.expect("revision"),
            committed,
            "a restart reads the committed revision back"
        );
        assert!(
            sink.events().is_empty(),
            "recovery announces nothing by itself"
        );
    }

    #[tokio::test]
    async fn a_corrupt_lifecycle_journal_blocks_the_composition_before_scheduling() {
        // Recovery ordering is a precondition, not a follow-up step: when the
        // journal cannot be resolved the composition aborts, so no refresh
        // scheduler is ever opened behind an unrecovered store.
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().to_path_buf();
        let journal = root
            .join("journal")
            .join(quotavista_infrastructure::lifecycle::JOURNAL_NAME);
        std::fs::create_dir_all(journal.parent().expect("journal dir")).expect("journal dir");
        std::fs::write(&journal, "{not a journal").expect("journal write");

        let sink = Arc::new(RecordingChangeSink::new());
        let changes: Arc<dyn ChangeSink> = sink.clone();
        // `AppState` is not `Debug`, so the failure is matched rather than
        // unwrapped with `expect_err`.
        let error = match build_with_provider(
            root,
            changes,
            SecretChoice::SessionOnly,
            SimulatedAdapter::new(synthetic_providers()[0]),
        )
        .await
        {
            Ok(_) => panic!("a corrupt journal must abort the composition"),
            Err(error) => error,
        };
        assert_eq!(error.code(), ErrorCode::InternalError);

        // The unreadable journal is left for inspection rather than silently
        // rewritten, and nothing was announced.
        assert_eq!(
            std::fs::read_to_string(&journal).expect("journal"),
            "{not a journal"
        );
        assert!(sink.events().is_empty());
    }

    /// Place a resume sample anywhere on either clock, so the detection logic
    /// can be tested without sleeping the machine.
    fn sample(monotonic_secs: u64, wall_secs: u64) -> ResumeSample {
        ResumeSample {
            monotonic: Duration::from_secs(monotonic_secs),
            wall: Duration::from_secs(wall_secs),
        }
    }

    #[test]
    fn a_suspend_gap_is_detected_from_either_clock() {
        // Which clock keeps counting across a suspend is platform-dependent, so
        // the monitor accepts either signal rather than assuming one.
        let tick = Duration::from_secs(30);
        let threshold = Duration::from_secs(120);

        // macOS: `Instant` is `mach_absolute_time`, which stops during sleep,
        // while the wall clock keeps running.
        let mut wall_only = ResumeMonitor::new(tick, threshold);
        assert!(wall_only.observe(sample(0, 1_000)).is_none());
        assert_eq!(
            wall_only.observe(sample(30, 4_600)),
            Some(Duration::from_secs(3_600)),
            "a wall-clock advance past the threshold is a resume"
        );

        // A platform whose monotonic clock does count suspend time sees both
        // move; the larger advance is the one reported.
        let mut both = ResumeMonitor::new(tick, threshold);
        assert!(both.observe(sample(0, 1_000)).is_none());
        assert_eq!(
            both.observe(sample(3_630, 4_630)),
            Some(Duration::from_secs(3_630)),
            "a monotonic advance past the threshold is a resume too"
        );
    }

    #[test]
    fn the_first_resume_sample_only_establishes_a_baseline() {
        let mut monitor = ResumeMonitor::new(Duration::from_secs(30), Duration::from_secs(120));
        // No previous sample means no elapsed span, so starting the monitor -
        // including starting it long after boot - cannot report a wake on its
        // own and trigger a refresh nobody asked for.
        assert_eq!(monitor.observe(sample(90_000, 1_700_000_000)), None);
    }

    #[test]
    fn ordinary_ticks_and_a_backwards_clock_are_not_a_resume() {
        let mut monitor = ResumeMonitor::new(Duration::from_secs(30), Duration::from_secs(120));
        assert!(monitor.observe(sample(0, 1_000)).is_none());
        assert_eq!(
            monitor.observe(sample(30, 1_030)),
            None,
            "one tick is normal"
        );
        assert_eq!(monitor.observe(sample(60, 1_060)), None);
        // A wall clock stepped backwards (NTP correction, a manual change) must
        // never read as elapsed time; saturating subtraction makes it no time at
        // all, so a clock anomaly cannot start a refresh.
        assert_eq!(monitor.observe(sample(90, 500)), None);
        // The baseline moved with it, so the next ordinary tick is normal too.
        assert_eq!(monitor.observe(sample(120, 530)), None);
    }

    #[tokio::test]
    async fn a_detected_resume_merges_one_refresh_without_window_activation() {
        // This is the path that works while the main window stays hidden in the
        // tray: no focus transition, no reopen, no second instance - only the
        // clock gap the monitor measures.
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        set_automatic_refresh(&state, 1).await;
        let revision = state.repository.data_revision().await.expect("revision");
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));
        let mut monitor = ResumeMonitor::new(Duration::from_secs(30), Duration::from_secs(120));

        let baseline = observe_resume(&state, &gate, &mut monitor, sample(0, 1_000))
            .await
            .expect("resume");
        assert!(baseline.is_none(), "a baseline sample is not a wake");
        assert!(sink.events().is_empty());

        let resumed = observe_resume(&state, &gate, &mut monitor, sample(30, 4_600))
            .await
            .expect("resume");
        let report = resumed.expect("a gap past the threshold is a wake");
        assert_eq!(report.source, WakeSource::ResumeAfterSuspend);
        assert_eq!(report.mode, RefreshMode::Automatic(1));
        assert!(report.permitted);
        assert_eq!(report.launched.len(), 1, "the resume merged one refresh");
        assert!(
            state.repository.data_revision().await.expect("revision") > revision,
            "the merged refresh committed"
        );
        assert_eq!(sink.events().len(), 1, "the wake announces its own commit");

        // A second gap inside the same refresh interval merges nothing: the
        // resume path reuses the gate and the coordinator, so deduplication and
        // the interval rule both survive the change of trigger.
        let again = observe_resume(&state, &gate, &mut monitor, sample(60, 8_200))
            .await
            .expect("resume");
        assert!(
            again.is_none(),
            "the gate still deduplicates a resume burst"
        );
        assert_eq!(sink.events().len(), 1);
    }

    #[tokio::test]
    async fn a_detected_resume_starts_nothing_while_refresh_is_off() {
        // Off gating has to survive the new trigger: a real suspend gap is
        // exactly the case where an implicit query would be most tempting.
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        let revision = state.repository.data_revision().await.expect("revision");
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));
        let mut monitor = ResumeMonitor::new(Duration::from_secs(30), Duration::from_secs(120));

        observe_resume(&state, &gate, &mut monitor, sample(0, 1_000))
            .await
            .expect("resume");
        let report = observe_resume(&state, &gate, &mut monitor, sample(30, 4_600))
            .await
            .expect("resume")
            .expect("the gap is detected, so the gated wake runs and reports");

        assert_eq!(report.mode, RefreshMode::Off);
        assert!(!report.permitted);
        assert!(report.launched.is_empty());
        assert_eq!(report.refused, 0, "nothing was offered to the coordinator");
        assert!(!report.diagnostic().contains(CONNECTION_ID));
        {
            let refresh = state.refresh.lock().await;
            assert!(
                refresh.job_ids().is_empty(),
                "no job means no provider call"
            );
        }
        assert_eq!(
            state.repository.data_revision().await.expect("revision"),
            revision,
            "an off-mode resume must not move the store revision"
        );
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn a_resume_detected_after_quit_starts_no_work() {
        // Quit raises its signal before it cancels, so a resume that lands in
        // the same instant neither competes for the refresh lock nor creates
        // work that shutdown would immediately have to cancel. The pools are
        // already closed here, which is what makes the check meaningful: an
        // inert wake never touches them.
        let (_dir, state, sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        set_automatic_refresh(&state, 1).await;
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));

        let quit = shutdown(&state).await;
        assert!(state.shutdown_signal.raised());
        assert_eq!(quit.running_after, 0);

        let mut monitor = ResumeMonitor::new(Duration::from_secs(30), Duration::from_secs(120));
        observe_resume(&state, &gate, &mut monitor, sample(0, 1_000))
            .await
            .expect("resume");
        let after = observe_resume(&state, &gate, &mut monitor, sample(30, 4_600))
            .await
            .expect("resume");
        assert!(after.is_none(), "a wake after quit must not start work");
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn every_wake_source_is_inert_once_quit_was_raised() {
        let (_dir, state, _sink) = fixture().await;
        seed_connection(&state, Lifecycle::Active).await;
        set_automatic_refresh(&state, 1).await;
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));
        shutdown(&state).await;

        for source in [
            WakeSource::WindowFocused,
            WakeSource::ReopenWithoutVisibleWindow,
            WakeSource::SingleInstanceActivation,
            WakeSource::ResumeAfterSuspend,
        ] {
            let woke = handle_gated_wake(&state, &gate, source)
                .await
                .expect("wake");
            assert!(woke.is_none(), "{source:?} must not start work after quit");
        }
    }

    #[tokio::test]
    async fn the_pump_loop_releases_the_lock_when_quit_is_raised() {
        // What keeps the user's Quit action reachable is not the step bound but
        // the signal check at the top of every iteration. Raising it before the
        // call exercises that check deterministically, including on the first
        // pass, so not one pump step runs.
        let (_dir, state, sink) = fixture().await;
        for index in 0..24 {
            seed_connection_with_id(
                &state,
                &format!("synthetic-connection-{index}"),
                Lifecycle::Active,
            )
            .await;
        }
        set_automatic_refresh(&state, 1).await;
        state.shutdown_signal.raise();

        let report = handle_wake(&state, WakeSource::WindowFocused)
            .await
            .expect("wake");
        assert!(
            report.permitted,
            "automatic refresh was on when the wake ran"
        );
        {
            let refresh = state.refresh.lock().await;
            // Admission promotes its own first `MAX_CONCURRENCY` jobs, so those
            // are already running before the loop starts. The loop's first act
            // would advance them. Seeing exactly the admission-time split, and
            // no job with a terminal outcome, proves it broke before its first
            // pump step rather than running to the step bound.
            assert_eq!(refresh.running(), MAX_CONCURRENCY);
            assert_eq!(refresh.pending(), 24 - MAX_CONCURRENCY);
            assert!(
                refresh
                    .job_ids()
                    .iter()
                    .all(|job| refresh.outcome(job).is_none()),
                "not one job reached a terminal outcome"
            );
        }
        assert!(
            sink.events().is_empty(),
            "nothing committed, so nothing was announced"
        );

        // The jobs the wake admitted are still cancelled rather than left
        // half-applied, which is what quit is for.
        let quit = shutdown(&state).await;
        assert_eq!(quit.cancelled.len(), 24);
        assert_eq!(quit.running_after, 0);
        assert_eq!(quit.pending_after, 0);
    }

    #[tokio::test]
    async fn the_resume_monitor_stops_itself_once_quit_was_raised() {
        let (_dir, state, _sink) = fixture().await;
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));
        let monitor = ResumeMonitor::new(Duration::from_millis(10), Duration::from_secs(120));
        state.shutdown_signal.raise();

        // The monitor only returns when quitting has begun, so this completing
        // at all is the assertion.
        tokio::time::timeout(
            Duration::from_secs(5),
            drive_resume_monitor(&state, &gate, monitor),
        )
        .await
        .expect("the monitor must stop after quit");
    }

    #[tokio::test]
    async fn the_resume_monitor_keeps_sampling_until_quit() {
        let (_dir, state, _sink) = fixture().await;
        let gate = std::sync::Mutex::new(WakeGate::new(WAKE_GATE_INTERVAL));
        let monitor = ResumeMonitor::new(Duration::from_millis(10), Duration::from_secs(120));

        // An unraised signal must keep the loop alive for the whole application
        // lifetime; only the timeout ending the future stops it here.
        let outcome = tokio::time::timeout(
            Duration::from_millis(100),
            drive_resume_monitor(&state, &gate, monitor),
        )
        .await;
        assert!(outcome.is_err(), "the monitor must not stop on its own");
    }
}
