use pulseflow_governor::{
    authority::{authority_contract, transition, AuthorityAction, AuthorityState},
    config::Config,
    model::{RuntimeState, RuntimeTuning},
};

#[test]
fn promotion_chain_cannot_skip_gates() {
    let discovered = transition(AuthorityState::Observation, AuthorityAction::Discover).unwrap();
    let connected = transition(discovered, AuthorityAction::Connect).unwrap();
    let verified = transition(connected, AuthorityAction::Verify).unwrap();
    let active = transition(verified, AuthorityAction::Enable).unwrap();
    assert_eq!(active, AuthorityState::Active);
    assert!(transition(AuthorityState::Observation, AuthorityAction::Enable).is_err());
    assert!(transition(AuthorityState::Connected, AuthorityAction::Enable).is_err());
}

#[test]
fn pause_resume_and_fault_rollbacks_are_explicit() {
    assert_eq!(
        transition(AuthorityState::Active, AuthorityAction::Pause).unwrap(),
        AuthorityState::Paused
    );
    assert_eq!(
        transition(AuthorityState::Paused, AuthorityAction::Resume).unwrap(),
        AuthorityState::Active
    );
    let fault = authority_contract(AuthorityState::Faulted);
    assert_eq!(fault.color, "red");
    assert_eq!(fault.rollback, AuthorityState::Observation);
    assert!(!fault.allowed_actions.contains(&AuthorityAction::Enable));
}

#[test]
fn process_target_does_not_bind_agent_or_auto_activate() {
    let config = Config::default();
    let state = RuntimeState::new(
        Some(4242),
        "test-process".into(),
        true,
        RuntimeTuning::from(&config.control),
        60,
    );
    assert_eq!(state.authority_state, AuthorityState::Connected);
    assert!(!state.governor_active);
    assert!(!state.agent_bound);
}
