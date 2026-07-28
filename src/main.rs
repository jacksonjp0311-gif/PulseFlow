use pulseflow_governor::{
    adaptive,
    analytics::AnalyticsEngine,
    authority::AuthorityState,
    config::Config,
    controller::PulseController,
    futurist,
    governor::{self, platform_governor_supported, ProcessGovernor},
    model::{now_ms, ObservationFrame, QosLevel, RuntimeEvent, RuntimeState, RuntimeTuning},
    policy::{self, MemoryGuard},
    regime::RegimeArbiter,
    server,
    storage::{append_event, FrameRecorder},
    system,
    telemetry::TelemetryCollector,
};
use std::{
    env,
    process::{Child, Command},
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("◆ PULSEFLOW FAULT · {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config_path =
        env::var("PULSEFLOW_CONFIG").unwrap_or_else(|_| "config/pulseflow.json".into());
    let config = Config::load(&config_path)?;
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg.eq_ignore_ascii_case("learn"))
    {
        return run_learn_batch(&config);
    }
    let (target_pid, target_label, _child) = resolve_target(&args)?;
    let supported = platform_governor_supported();
    let state = Arc::new(RwLock::new(RuntimeState::new(
        target_pid,
        target_label.clone(),
        supported,
        RuntimeTuning::from(&config.control),
        config.storage.recent_history_capacity,
    )));

    print_banner(&config, target_pid, &target_label, supported);
    if let Ok(locked) = state.read() {
        if let Some(event) = locked.events.first() {
            let _ = append_event(&config.event_ledger_path, event);
        }
    }

    let worker_state = Arc::clone(&state);
    let worker_config = config.clone();
    thread::spawn(move || run_control_loop(worker_state, worker_config));

    let bind = config.bind.clone();
    server::serve(&bind, state, config)
}

fn run_learn_batch(config: &Config) -> Result<(), String> {
    use pulseflow_governor::{analytics, futurist, storage};
    let sessions = storage::list_sessions(&config.storage.directory)?;
    let mut compacted = 0u64;
    let mut freed = 0u64;
    let profile = system::probe(platform_governor_supported(), false);
    for session in sessions {
        if session.samples == 0 {
            continue;
        }
        let frames = match storage::read_session_frames(
            &config.storage.directory,
            &session.session_id,
            usize::MAX,
        ) {
            Ok(frames) if !frames.is_empty() => frames,
            _ => continue,
        };
        let mut summary =
            analytics::summarize_session(&session.session_id, &frames, config.analytics.epsilon);
        let calibration =
            futurist::calibrate_session(&session.session_id, &frames, config.analytics.epsilon);
        summary.futurist_skill_improvement = calibration.skill.relative_improvement;
        summary.system_form_factor = profile.form_factor.as_str().into();
        let points = analytics::learning_graph_points(&frames, 240);
        let options = storage::CompactOptions {
            system_form_factor: profile.form_factor.as_str().into(),
            system_known_as: profile.known_as.clone(),
            futurist_skill_mae_h5: calibration.skill.mae_h5,
            futurist_skill_improvement: calibration.skill.relative_improvement,
            futurist_beats_persist: calibration.skill.beats_persist,
        };
        match storage::compact_session_with_options(
            &config.storage.directory,
            &session.session_id,
            summary,
            points,
            options,
        ) {
            Ok(receipt) => {
                compacted += 1;
                freed = freed.saturating_add(receipt.freed_bytes);
                println!(
                    "◆ learned {} → graph blob (freed {} bytes, futurist Δ={:.0}%)",
                    receipt.session_id,
                    receipt.freed_bytes,
                    calibration.skill.relative_improvement * 100.0
                );
            }
            Err(error) => eprintln!("◇ skip {}: {error}", session.session_id),
        }
    }
    println!("◆ learn complete · {compacted} blob(s) · freed {freed} bytes");
    Ok(())
}

fn resolve_target(args: &[String]) -> Result<(Option<u32>, String, Option<Child>), String> {
    if args.is_empty() || args[0].eq_ignore_ascii_case("serve") {
        return Ok((None, "system-monitor".into(), None));
    }

    if args[0].eq_ignore_ascii_case("attach") {
        let pid = args
            .get(1)
            .ok_or("usage: pulseflow-governor attach <pid>")?
            .parse::<u32>()
            .map_err(|_| "attach PID must be an unsigned integer")?;
        return Ok((Some(pid), format!("pid-{pid}"), None));
    }

    if args[0].eq_ignore_ascii_case("run") {
        let separator = args.iter().position(|arg| arg == "--").unwrap_or(1);
        let separator_is_marker = args.get(separator).is_some_and(|arg| arg == "--");
        let command_index = separator + if separator_is_marker { 1 } else { 0 };
        let executable = args
            .get(command_index)
            .ok_or("usage: pulseflow-governor run -- <program> [arguments...]")?;
        let child = Command::new(executable)
            .args(&args[command_index + 1..])
            .spawn()
            .map_err(|error| format!("cannot launch {executable}: {error}"))?;
        let pid = child.id();
        let label = std::path::Path::new(executable)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("workload")
            .to_string();
        return Ok((Some(pid), label, Some(child)));
    }

    Err("unknown command; use serve, learn, attach <pid>, or run -- <program> [args]".into())
}

fn run_control_loop(state: Arc<RwLock<RuntimeState>>, config: Config) {
    let mut target_pid = state.read().ok().and_then(|locked| locked.target_pid);
    let mut monitor_only = target_pid.is_none() || !platform_governor_supported();
    let mut telemetry = TelemetryCollector::new(target_pid);
    let mut controller =
        PulseController::new(config.control.clone(), config.weights.clone(), monitor_only);
    let mut governor = ProcessGovernor::new(target_pid, config.governor.clone());
    let mut analytics = AnalyticsEngine::new(config.analytics.clone());
    let interval = Duration::from_millis(config.sample_interval_ms.max(250));
    let dt = interval.as_secs_f64();
    let mut observed_target_revision = 0u64;
    let mut observed_tuning_revision = 0u64;
    let mut observed_reset_revision = 0u64;
    let mut sequence = 0u64;
    let mut recorder = open_current_recorder(&state, &config);
    let mut pending_frame: Option<(ObservationFrame, bool)> = None;
    let mut profile_refresh_counter = 0u64;
    let mut regime_arbiter = RegimeArbiter::default();
    let mut last_regime_code = String::new();
    // Initial adaptive profile — know thy system.
    {
        let initial = system::probe(platform_governor_supported(), false);
        let weights = system::renormalize_weights(&initial.adaptive_weights, false, false);
        controller.update_weights(weights);
        controller.set_eco_ram_enter_percent(initial.eco_ram_enter_percent);
        if let Ok(mut locked) = state.write() {
            locked.system_profile = initial;
            let message = format!(
                "Know thy system: {} ({})",
                locked.system_profile.known_as, locked.system_profile.adaptation_reason
            );
            let event = locked.push_event("system_profile", message);
            let _ = append_event(&config.event_ledger_path, &event);
        }
    }

    loop {
        let snapshot = match state.read() {
            Ok(locked) => (
                locked.telemetry.io.clone(),
                locked.mode,
                locked.governor_active,
                locked.recording,
                locked.learning_stage,
                locked.tuning.clone(),
                locked.tuning_revision,
                locked.reset_revision,
                locked.target_pid,
                locked.target_revision,
                locked.authority_state,
                locked.verification_receipt.as_ref().is_some_and(|receipt| {
                    now_ms().saturating_sub(receipt.timestamp_ms) <= 300_000
                }),
                locked.mesh_mode,
                locked.session_id.clone(),
                locked.experiment_id.clone(),
                locked.epoch_revision,
                locked.epoch_reason.clone(),
                locked.metrics.clone(),
                locked.adaptive_suggestion.clone(),
            ),
            Err(_) => break,
        };
        let (
            io,
            mode,
            active,
            recording,
            stage,
            tuning,
            tuning_revision,
            reset_revision,
            requested_target_pid,
            target_revision,
            authority_state,
            verification_fresh,
            mesh_mode,
            session_id,
            experiment_id,
            epoch_revision,
            epoch_reason,
            previous_metrics,
            previous_adaptive_suggestion,
        ) = snapshot;

        if target_revision != observed_target_revision {
            target_pid = requested_target_pid;
            monitor_only = target_pid.is_none() || !platform_governor_supported();
            telemetry = TelemetryCollector::new(target_pid);
            let mut next = config.control.clone();
            tuning.apply_to(&mut next);
            let (weights, eco_ram) = state
                .read()
                .ok()
                .map(|locked| {
                    (
                        locked.system_profile.adaptive_weights.clone(),
                        locked.system_profile.eco_ram_enter_percent,
                    )
                })
                .unwrap_or_else(|| (config.weights.clone(), 88.0));
            controller = PulseController::new(next, weights, monitor_only);
            controller.set_eco_ram_enter_percent(eco_ram);
            governor = ProcessGovernor::new(target_pid, config.governor.clone());
            analytics.reset();
            pending_frame = None;
            sequence = 0;
            observed_target_revision = target_revision;
        }

        if tuning_revision != observed_tuning_revision {
            let mut next = config.control.clone();
            tuning.apply_to(&mut next);
            controller.update_config(next);
            observed_tuning_revision = tuning_revision;
        }

        if reset_revision != observed_reset_revision {
            if let Some(current) = recorder.as_mut() {
                let _ = current.finalize();
            }
            recorder = open_current_recorder(&state, &config);
            controller.reset(monitor_only);
            analytics.reset();
            sequence = 0;
            pending_frame = None;
            observed_reset_revision = reset_revision;
        }

        sequence = sequence.saturating_add(1);
        let sample = telemetry.sample(io, config.signal_stale_after_ms);
        let target_alive = requested_target_pid.is_none()
            || sample.process.as_ref().is_some_and(|process| process.alive);
        // Single-process path requires verified Active + live PID.
        // Pulse Mesh path governs host-wide without one PID attachment.
        let effective_active = active
            && authority_state == AuthorityState::Active
            && (mesh_mode || (target_alive && verification_fresh));

        // Refresh host profile periodically so weights track the live plant.
        profile_refresh_counter = profile_refresh_counter.saturating_add(1);
        let has_gpu = sample.gpu.is_some();
        if profile_refresh_counter == 1 || profile_refresh_counter % 30 == 0 {
            let profile = system::probe(platform_governor_supported(), has_gpu);
            let weights = system::renormalize_weights(
                &profile.adaptive_weights,
                has_gpu,
                sample.io_signal_fresh,
            );
            controller.update_weights(weights);
            controller.set_eco_ram_enter_percent(profile.eco_ram_enter_percent);
            if let Ok(mut locked) = state.write() {
                locked.system_profile = profile;
            }
        }

        let mut control = controller.step(
            &sample,
            mode,
            dt,
            effective_active,
            config.governor.thermal_guard_c,
            config.governor.thermal_release_c,
        );
        let mut mesh_note = String::new();
        let mut mesh_targets = 0u32;
        let apply = if effective_active && mesh_mode {
            // Whole-system mesh: Eco/ThermalProtect lands on top pressure processes.
            let (count, note) = governor::apply_mesh_qos(control.requested_qos, 4);
            mesh_targets = count;
            mesh_note = note;
            control.applied_qos = control.requested_qos;
            // Synthetic apply result for logging.
            pulseflow_governor::governor::ApplyResult {
                changed: count > 0,
                applied: control.requested_qos,
                message: mesh_note.clone(),
            }
        } else if effective_active {
            governor.apply(control.requested_qos)
        } else {
            governor.apply(QosLevel::MonitorOnly)
        };
        control.applied_qos = apply.applied;
        control.control_authority = if mesh_mode && effective_active {
            1.0
        } else if matches!(
            authority_state,
            AuthorityState::Verified | AuthorityState::Active | AuthorityState::Paused
        ) {
            1.0
        } else {
            0.0
        };
        if apply.applied == QosLevel::MonitorOnly && !mesh_mode {
            control.controller_effort = 0.0;
            control.applied_modulation = 0.0;
            control.modulation = 0.0;
        }

        let (memory_guard, form_factor_label) = state
            .read()
            .ok()
            .map(|locked| {
                (
                    MemoryGuard::from(&locked.system_profile),
                    locked.system_profile.form_factor.as_str().to_string(),
                )
            })
            .unwrap_or_else(|| (MemoryGuard::default(), "desktop".into()));

        // Rolling series for futurist multi-horizon foresight.
        let (stress_series, ram_series, eco_series) = state
            .read()
            .ok()
            .map(|locked| {
                let stress: Vec<f64> = locked
                    .history
                    .iter()
                    .map(|frame| frame.controller.filtered_stress)
                    .chain(std::iter::once(control.filtered_stress))
                    .collect();
                let ram: Vec<f64> = locked
                    .history
                    .iter()
                    .map(|frame| frame.machine.ram_percent)
                    .chain(std::iter::once(sample.memory_percent))
                    .collect();
                let eco: Vec<f64> = locked
                    .history
                    .iter()
                    .map(|frame| frame.metrics.ecosystem_pressure)
                    .chain(std::iter::once(previous_metrics.ecosystem_pressure))
                    .collect();
                (stress, ram, eco)
            })
            .unwrap_or_default();
        let mut futurist_snapshot = if stress_series.len() >= 8 {
            futurist::snapshot_from_series(
                &stress_series,
                &ram_series,
                &eco_series,
                sample.gpu.as_ref().and_then(|gpu| gpu.temperature_c),
                config.analytics.pressure_limit,
                config.analytics.epsilon,
            )
        } else {
            futurist::bootstrap_from_telemetry(&sample, config.analytics.pressure_limit)
        };
        if let Some(skill) = state
            .read()
            .ok()
            .map(|locked| locked.futurist.skill.clone())
        {
            if skill.samples_scored > 0 {
                futurist_snapshot.calibrated = skill.beats_persist;
                futurist_snapshot.skill = skill;
            }
        }

        let failed_invariant = state
            .read()
            .ok()
            .and_then(|locked| locked.failed_invariant.clone())
            .is_some();
        let verification_ok = state
            .read()
            .ok()
            .and_then(|locked| locked.verification_receipt.clone())
            .map(|receipt| receipt.success)
            .unwrap_or(true);
        let session_samples = state
            .read()
            .ok()
            .map(|locked| locked.session_samples)
            .unwrap_or(0);
        let regime_decision = regime_arbiter.decide(
            &sample,
            &control,
            &previous_metrics,
            authority_state,
            verification_fresh,
            verification_ok,
            failed_invariant,
            session_samples,
        );

        let mut directive = policy::recommend_with_guard(
            &sample,
            &control,
            &previous_metrics,
            stage,
            &config.agent_policy,
            memory_guard,
            &futurist_snapshot.envelope,
        );
        // GCMT hard regimes force shadow / contract without widening authority.
        use pulseflow_governor::regime::MemoryRegime;
        match regime_decision.regime {
            MemoryRegime::Abstain | MemoryRegime::Quarantine | MemoryRegime::Rollback => {
                directive.shadow_only = true;
                directive.allow_background_memory_work = false;
                directive.recommended_concurrency = directive.recommended_concurrency.min(1);
                directive.model_route = "efficient".into();
                directive.reason = format!(
                    "{} · regime {}",
                    directive.reason, regime_decision.regime_label
                );
            }
            MemoryRegime::Evidence | MemoryRegime::Reanchor => {
                directive.shadow_only = true;
                directive.reason = format!(
                    "{} · regime {} suggests evidence/re-anchor before promotion.",
                    directive.reason, regime_decision.regime_label
                );
            }
            MemoryRegime::Local => {}
        }
        let current_frame = ObservationFrame::new(
            session_id,
            experiment_id,
            epoch_revision,
            epoch_reason,
            tuning_revision,
            sequence,
            sample.clone(),
            control.clone(),
            mode,
            effective_active,
            stage,
            directive.clone(),
            previous_adaptive_suggestion,
            dt,
        );

        let mut finalized_frame = None;
        let mut latest_metrics = previous_metrics;
        let mut latest_adaptive_suggestion = None;
        let mut adaptive_tuning_to_apply = None;
        let mut bytes_written = 0u64;
        let mut storage_fault = None;

        if let Some((mut frame, should_record)) = pending_frame.take() {
            frame.finalize_outcome(&sample);
            let horizon_seconds = (frame.outcome.horizon_ms as f64 / 1_000.0).clamp(0.001, 60.0);
            latest_metrics = analytics.update(&frame, horizon_seconds);
            latest_metrics.futurist_envelope = futurist_snapshot.envelope.clone();
            latest_metrics.system_form_factor = form_factor_label.clone();
            latest_metrics.envelope_zone = regime_decision.zone_code.clone();
            latest_metrics.memory_regime = regime_decision.regime_code.clone();
            latest_metrics.memory_regime_label = regime_decision.regime_label.clone();
            latest_metrics.continuation_debt = regime_decision.continuation_debt;
            latest_metrics.condition_drift = regime_decision.condition.drift;
            latest_metrics.condition_legitimacy = regime_decision.condition.legitimacy;
            latest_metrics.condition_integrity = regime_decision.condition.integrity;
            latest_metrics.condition_freshness = regime_decision.condition.freshness;
            latest_metrics.condition_margin = regime_decision.condition.margin;
            latest_metrics.regime_reason = regime_decision.reason.clone();
            if let Some(channel) = futurist_snapshot
                .channels
                .iter()
                .find(|channel| channel.channel == "stress")
            {
                latest_metrics.futurist_stress_h5 = channel
                    .horizons
                    .iter()
                    .find(|horizon| horizon.horizon_samples == 5)
                    .map(|horizon| horizon.forecast);
            }
            if let Some(channel) = futurist_snapshot
                .channels
                .iter()
                .find(|channel| channel.channel == "ram")
            {
                latest_metrics.futurist_ram_h5 = channel
                    .horizons
                    .iter()
                    .find(|horizon| horizon.horizon_samples == 5)
                    .map(|horizon| horizon.forecast);
            }
            frame.metrics = latest_metrics.clone();
            let suggestion =
                adaptive::recommend(&tuning, &latest_metrics, stage, &config.agent_policy);
            if suggestion.applied {
                adaptive_tuning_to_apply = suggestion.proposed_tuning.clone();
            }
            latest_adaptive_suggestion = Some(suggestion);

            if should_record {
                match recorder.as_mut() {
                    Some(current) => match current.append(&frame) {
                        Ok(bytes) => bytes_written = bytes,
                        Err(error) => storage_fault = Some(error),
                    },
                    None => storage_fault = Some("observation recorder is unavailable".into()),
                }
            }
            finalized_frame = Some(frame);
        }

        pending_frame = Some((current_frame, recording && config.storage.enabled));

        let mut ledger_events: Vec<RuntimeEvent> = Vec::new();
        if let Ok(mut locked) = state.write() {
            if locked.authority_state == AuthorityState::Active
                && !locked.mesh_mode
                && !target_alive
            {
                locked.governor_active = false;
                locked.last_valid_authority_state = AuthorityState::Verified;
                locked.authority_state = AuthorityState::Faulted;
                locked.failed_invariant = Some("target_process_exited".into());
                ledger_events.push(locked.push_event(
                    "authority_fault",
                    "Target process exited; governance stopped and observation remains live.",
                ));
            } else if locked.authority_state == AuthorityState::Active
                && !locked.mesh_mode
                && !verification_fresh
            {
                locked.governor_active = false;
                locked.last_valid_authority_state = AuthorityState::Connected;
                locked.authority_state = AuthorityState::Faulted;
                locked.failed_invariant = Some("verification_expired".into());
                ledger_events.push(locked.push_event(
                    "authority_fault",
                    "Verification expired; governance stopped pending rediscovery and verification.",
                ));
            }
            if mesh_mode {
                locked.mesh_note = mesh_note.clone();
                locked.mesh_targets = mesh_targets;
            }
            locked.telemetry = sample;
            locked.control = control;
            locked.metrics = latest_metrics;
            locked.futurist = futurist_snapshot.clone();
            locked.regime = regime_decision.clone();
            locked.directive = directive;
            if regime_decision.regime_code != last_regime_code && !last_regime_code.is_empty() {
                ledger_events.push(locked.push_event(
                    "regime_switch",
                    format!(
                        "{} → {} ({}); debt={:.2}; zone={}",
                        last_regime_code,
                        regime_decision.regime_code,
                        regime_decision.regime_label,
                        regime_decision.continuation_debt,
                        regime_decision.zone_code
                    ),
                ));
            }
            last_regime_code = regime_decision.regime_code.clone();
            if let Some(suggestion) = latest_adaptive_suggestion {
                locked.adaptive_suggestion = suggestion;
            }
            if let Some(next_tuning) = adaptive_tuning_to_apply {
                locked.tuning = next_tuning;
                locked.tuning_revision = locked.tuning_revision.saturating_add(1);
                ledger_events.push(locked.push_event(
                    "adaptive_tuning",
                    "A bounded controller update was applied at an evidence checkpoint.",
                ));
            }
            locked.live_sequence = sequence;
            locked.session_samples =
                locked
                    .session_samples
                    .saturating_add(if bytes_written > 0 { 1 } else { 0 });
            locked.session_bytes = locked.session_bytes.saturating_add(bytes_written);
            if let Some(frame) = finalized_frame {
                locked.push_frame(frame, config.storage.recent_history_capacity);
            }
            if apply.changed {
                ledger_events.push(locked.push_event("qos", apply.message));
            }
            if let Some(error) = storage_fault {
                locked.recording = false;
                ledger_events.push(locked.push_event("storage_fault", error));
            }
            if locked
                .telemetry
                .process
                .as_ref()
                .is_some_and(|process| !process.alive)
            {
                locked.governor_active = false;
            }
        }
        for event in ledger_events {
            let _ = append_event(&config.event_ledger_path, &event);
        }
        thread::sleep(interval);
    }
}

fn open_current_recorder(
    state: &Arc<RwLock<RuntimeState>>,
    config: &Config,
) -> Option<FrameRecorder> {
    if !config.storage.enabled {
        return None;
    }
    let (session_id, target_label) = state
        .read()
        .ok()
        .map(|locked| (locked.session_id.clone(), locked.target_label.clone()))?;
    match FrameRecorder::open(
        &config.storage.directory,
        &session_id,
        &target_label,
        config.storage.metadata_flush_every_samples,
    ) {
        Ok(recorder) => Some(recorder),
        Err(error) => {
            if let Ok(mut locked) = state.write() {
                locked.recording = false;
                let event = locked.push_event("storage_fault", error);
                let _ = append_event(&config.event_ledger_path, &event);
            }
            None
        }
    }
}

fn print_banner(config: &Config, target_pid: Option<u32>, target_label: &str, supported: bool) {
    println!(
        "◆  PULSEFLOW GOVERNOR · SYSTEM INTERLINK v{}",
        env!("CARGO_PKG_VERSION")
    );
    println!("│");
    println!("├─ ◈  initialize telemetry lattice      ACTIVE");
    println!(
        "├─ ◆  console                           http://{}",
        config.bind
    );
    println!("├─ ◆  target                            {target_label}");
    println!(
        "├─ ◆  pid                               {}",
        target_pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "none".into())
    );
    println!(
        "├─ ◆  process modulation                {}",
        if target_pid.is_some() && supported {
            "ACTIVE"
        } else {
            "MONITOR ONLY"
        }
    );
    println!(
        "├─ ◆  frame recording                   {}",
        if config.storage.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        }
    );
    println!("├─ ◆  adaptive profile                  know-thy-system active");
    println!("├─ ◆  futurist governor                 multi-horizon advisory");
    println!("└─ ◆  safety                            clocks / voltage / fans untouched");
}
