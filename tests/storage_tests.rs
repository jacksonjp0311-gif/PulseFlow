use pulseflow_governor::{
    model::{ObservationFrame, SessionSummary},
    storage::{compact_session, list_sessions, read_session_frames, FrameRecorder},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("pulseflow-test-{nonce}"))
}

fn frame() -> ObservationFrame {
    serde_json::from_value(serde_json::json!({
        "schema_version":"pulseflow.observation.v1","session_id":"unit-session","sequence":1,"timestamp_ms":1000,
        "workload":{"source":"test","agent":"test","task_type":"test","model":"test","context_tokens":0,"input_queue":0,"output_queue":0,"busy":false,"signal_fresh":false},
        "machine":{"cpu_percent":10.0,"ram_percent":20.0,"ram_used_gb":1.0,"ram_total_gb":8.0,"gpu_percent":null,"gpu_temperature_c":null,"gpu_power_w":null,"gpu_memory_used_mb":null,"gpu_memory_total_mb":null,"process_cpu_percent":null,"process_memory_mb":null,"process_alive":null},
        "controller":{"raw_stress":0.1,"filtered_stress":0.1,"setpoint":0.66,"error":0.56,"integral":0.0,"derivative":0.0,"predicted_stress":0.1,"residue":0.0,"residue_memory":0.0,"modulation":0.7,"jitter":0.0,"phase":"hold","reason":"test","requested_qos":"monitor_only","applied_qos":"monitor_only","transition_count":0},
        "action":{"mode":"balanced","governor_active":false,"requested_qos":"monitor_only","applied_qos":"monitor_only","modulation_authority":0.7,"learning_stage":"recorder","directive":{"authority":0.7,"recommended_concurrency":1,"recommended_batch_size":1,"allow_background_memory_work":false,"model_route":"balanced","token_budget_scale":0.8,"retrieval_depth_scale":0.8,"shadow_only":true,"reason":"test"}},
        "residue":{"predicted_stress":0.1,"observed_stress":0.1,"residue":0.0,"residue_memory":0.0,"squared_prediction_error":0.0},
        "outcome":{"observed_at_ms":2000,"horizon_ms":1000,"alignment":"next_interval","latency_ms":0.0,"tokens_per_second":0.0,"completed_units":0,"success":null,"estimated_tokens_this_interval":0.0},
        "metrics":{}
    })).expect("valid frame")
}

#[test]
fn recorder_round_trips_jsonl_and_metadata() {
    let directory = temp_directory();
    let mut recorder =
        FrameRecorder::open(&directory, "unit-session", "unit", 1).expect("open recorder");
    recorder.append(&frame()).expect("append frame");
    recorder.finalize().expect("finalize");

    let frames = read_session_frames(&directory, "unit-session", 10).expect("read frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].sequence, 1);
    assert_eq!(frames[0].schema_version, "pulseflow.observation.v3");
    assert_eq!(frames[0].controller.applied_modulation, 0.0);

    let sessions = list_sessions(&directory).expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].samples, 1);
    assert!(sessions[0].bytes > 0);

    fs::remove_dir_all(directory).ok();
}

#[test]
fn compaction_preserves_receipt_before_deleting_raw_session() {
    let directory = temp_directory();
    let mut recorder =
        FrameRecorder::open(&directory, "compact-session", "unit", 1).expect("open recorder");
    let mut observation = frame();
    observation.session_id = "compact-session".into();
    recorder.append(&observation).expect("append frame");
    recorder.finalize().expect("finalize");
    drop(recorder);

    let receipt = compact_session(
        &directory,
        "compact-session",
        SessionSummary {
            session_id: "compact-session".into(),
            samples: 1,
            ..SessionSummary::default()
        },
    )
    .expect("compact session");

    assert!(receipt.raw_deleted);
    assert!(receipt.freed_bytes > 0);
    assert!(!directory.join("compact-session.jsonl").exists());
    assert!(!directory.join("compact-session.meta.json").exists());
    assert!(directory
        .join("analysis-receipts")
        .join("compact-session.analysis.json")
        .exists());

    fs::remove_dir_all(directory).ok();
}
