use pulseflow_governor::{
    analytics::{
        compare_sessions, forecast_trajectory, mean, oscillation_coherence, percentile, stddev,
        summarize_session,
    },
    model::{ObservationFrame, SessionSummary},
};

fn frame(sequence: u64, timestamp_ms: u128, tps: f64, temp: f64, residue: f64) -> ObservationFrame {
    let mut value: ObservationFrame = serde_json::from_value(serde_json::json!({
        "schema_version":"pulseflow.observation.v1",
        "session_id":"test-session",
        "sequence":sequence,
        "timestamp_ms":timestamp_ms,
        "workload":{"source":"test","agent":"test","task_type":"test","model":"test","context_tokens":0,"input_queue":1,"output_queue":1,"busy":true,"signal_fresh":true},
        "machine":{"cpu_percent":50.0,"ram_percent":40.0,"ram_used_gb":8.0,"ram_total_gb":16.0,"gpu_percent":60.0,"gpu_temperature_c":temp,"gpu_power_w":40.0,"gpu_memory_used_mb":1000.0,"gpu_memory_total_mb":4000.0,"process_cpu_percent":20.0,"process_memory_mb":500.0,"process_alive":true},
        "controller":{"raw_stress":0.6,"filtered_stress":0.58,"setpoint":0.66,"error":0.08,"integral":0.0,"derivative":0.0,"predicted_stress":0.57,"residue":residue,"residue_memory":residue,"modulation":0.7,"jitter":0.02,"phase":"hold","reason":"test","requested_qos":"normal","applied_qos":"normal","transition_count":0},
        "action":{"mode":"balanced","governor_active":true,"requested_qos":"normal","applied_qos":"normal","modulation_authority":0.7,"learning_stage":"recorder","directive":{"authority":0.7,"recommended_concurrency":2,"recommended_batch_size":4,"allow_background_memory_work":true,"model_route":"balanced","token_budget_scale":0.8,"retrieval_depth_scale":0.8,"shadow_only":true,"reason":"test"}},
        "residue":{"predicted_stress":0.57,"observed_stress":0.6,"residue":residue,"residue_memory":residue,"squared_prediction_error":residue*residue},
        "outcome":{"observed_at_ms":timestamp_ms+1000,"horizon_ms":1000,"alignment":"next_interval","latency_ms":100.0,"tokens_per_second":tps,"completed_units":1,"success":true,"estimated_tokens_this_interval":tps},
        "metrics":{}
    })).expect("valid frame");
    value.metrics = Default::default();
    value
}

#[test]
fn coherence_handles_known_signal_classes() {
    let identical: Vec<f64> = (0..64).map(|index| (index as f64 * 0.2).sin()).collect();
    let shifted: Vec<f64> = (0..64)
        .map(|index| ((index + 5) as f64 * 0.2).sin())
        .collect();
    let noisy: Vec<f64> = identical
        .iter()
        .enumerate()
        .map(|(index, value)| value + if index % 2 == 0 { 0.03 } else { -0.03 })
        .collect();
    let unrelated: Vec<f64> = (0..64)
        .map(|index| ((index * 17 % 31) as f64 / 15.0) - 1.0)
        .collect();
    assert_eq!(
        oscillation_coherence(&identical, &identical, 1e-9),
        Some(1.0)
    );
    assert!(oscillation_coherence(&identical, &noisy, 1e-9).unwrap() > 0.85);
    assert!(oscillation_coherence(&identical, &shifted, 1e-9).unwrap() < 0.9);
    assert!(oscillation_coherence(&identical, &unrelated, 1e-9).unwrap() < 0.75);
    assert_eq!(oscillation_coherence(&[1.0; 8], &[1.0; 8], 1e-9), Some(1.0));
    assert_eq!(oscillation_coherence(&[], &[], 1e-9), None);
}

#[test]
fn futurist_forecast_is_bounded_and_evidence_gated() {
    assert_eq!(forecast_trajectory(&[0.2; 7], 5, 1e-9).0, None);
    let rising: Vec<f64> = (0..60).map(|index| 0.2 + index as f64 * 0.005).collect();
    let (forecast, trend, confidence) = forecast_trajectory(&rising, 5, 1e-9);
    assert!((0.0..=1.0).contains(&forecast.unwrap()));
    assert!(trend > 0.0);
    assert!((0.0..=1.0).contains(&confidence));
}

#[test]
fn elementary_statistics_are_stable() {
    assert_eq!(mean(&[1.0, 2.0, 3.0]), 2.0);
    assert!((stddev(&[1.0, 2.0, 3.0]) - 0.8164965809).abs() < 1e-9);
    assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.95), 4.0);
}

#[test]
fn session_summary_integrates_energy_and_tokens() {
    let frames = vec![
        frame(1, 1_000, 20.0, 70.0, 0.10),
        frame(2, 2_000, 30.0, 72.0, 0.05),
        frame(3, 3_000, 40.0, 68.0, -0.02),
    ];
    let summary = summarize_session("test-session", &frames, 1e-6);
    assert_eq!(summary.samples, 3);
    assert!((summary.average_tokens_per_second - 30.0).abs() < 1e-9);
    assert!((summary.total_gpu_energy_joules.unwrap() - 120.0).abs() < 1e-9);
    assert!((summary.estimated_tokens - 90.0).abs() < 1e-9);
    assert!((summary.energy_per_token_joules.unwrap() - 4.0 / 3.0).abs() < 1e-9);
    assert!(summary.prediction_rmse > 0.0);
}

#[test]
fn comparison_enforces_minimum_evidence_boundary() {
    let baseline = SessionSummary {
        session_id: "baseline".into(),
        samples: 10,
        duration_seconds: 10.0,
        average_tokens_per_second: 20.0,
        ..SessionSummary::default()
    };
    let candidate = SessionSummary {
        session_id: "candidate".into(),
        samples: 10,
        duration_seconds: 10.0,
        average_tokens_per_second: 22.0,
        ..SessionSummary::default()
    };
    let report = compare_sessions(baseline, candidate);
    assert!(!report.comparable);
    assert!((report.intervention_value_tokens_per_second - 2.0).abs() < 1e-9);
    assert_eq!(report.throughput_delta_percent, Some(10.0));
}

#[test]
fn homeostatic_slack_contracts_when_ecosystem_pressure_accumulates() {
    let stable: Vec<ObservationFrame> = (0..20)
        .map(|index| frame(index + 1, 1_000 + index as u128 * 1_000, 20.0, 50.0, 0.01))
        .collect();
    let rising: Vec<ObservationFrame> = (0..20)
        .map(|index| {
            let mut observation = frame(index + 1, 1_000 + index as u128 * 1_000, 20.0, 50.0, 0.01);
            observation.machine.ram_percent = 40.0 + index as f64 * 2.5;
            observation.machine.ram_used_gb = 8.0 + index as f64 * 0.4;
            observation
        })
        .collect();

    let stable_summary = summarize_session("stable", &stable, 1e-6);
    let rising_summary = summarize_session("rising", &rising, 1e-6);

    assert!((0.0..=1.0).contains(&stable_summary.homeostatic_slack));
    assert!((0.0..=1.0).contains(&rising_summary.homeostatic_slack));
    assert!(rising_summary.homeostatic_slack < stable_summary.homeostatic_slack);
    assert!(rising_summary.pressure_momentum_per_minute > 0.0);
    assert!(rising_summary.latent_pressure > stable_summary.latent_pressure);
    assert!(rising_summary.target_memory_share.is_some());
}

#[test]
fn vector_pressure_exposes_cross_resource_transduction() {
    let frames: Vec<ObservationFrame> = (0..20)
        .map(|index| {
            let mut observation = frame(index + 1, 1_000 + index as u128 * 1_000, 20.0, 50.0, 0.0);
            observation.machine.cpu_percent = 80.0 - index as f64 * 2.0;
            observation.machine.ram_percent = 40.0 + index as f64 * 2.0;
            observation
        })
        .collect();
    let summary = summarize_session("transduction", &frames, 1e-6);
    assert!(summary.vector_accumulation > 0.0);
    assert!(summary.vector_dissipation > 0.0);
    assert!(summary.pressure_transduction > 0.0);
    assert!(summary.latent_pressure > 0.0);
    assert!(summary.resource_momentum_per_minute["cpu"] < 0.0);
    assert!(summary.resource_momentum_per_minute["ram"] > 0.0);
    assert!(summary.net_vector_pressure.abs() < 1e-9);
}
