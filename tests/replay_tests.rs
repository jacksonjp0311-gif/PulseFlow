use pulseflow_governor::{
    config::Config,
    model::{ObservationFrame, RuntimeTuning},
    replay::run_replay,
};

#[test]
fn empty_replay_is_well_defined() {
    let config = Config::default();
    let tuning = RuntimeTuning::from(&config.control);
    let report = run_replay("empty", &[] as &[ObservationFrame], &config, &tuning);
    assert_eq!(report.samples_replayed, 0);
    assert_eq!(report.candidate_prediction_rmse, 0.0);
    assert!(report.note.contains("does not claim"));
}
