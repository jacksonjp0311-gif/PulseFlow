use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityState {
    Observation,
    Discovered,
    Connected,
    Verified,
    Active,
    Paused,
    Faulted,
    Disconnected,
}

impl Default for AuthorityState {
    fn default() -> Self {
        Self::Observation
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityAction {
    Discover,
    Connect,
    Verify,
    Enable,
    Pause,
    Resume,
    Disconnect,
    Fault,
    Recover,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthorityContract {
    pub state: AuthorityState,
    pub allowed_actions: Vec<AuthorityAction>,
    pub prohibited_actions: Vec<AuthorityAction>,
    pub header_label: &'static str,
    pub interlink_label: &'static str,
    pub governor_label: &'static str,
    pub process_qos_label: &'static str,
    pub center_core_label: &'static str,
    pub color: &'static str,
    pub evidence_requirement: &'static str,
    pub persistence: &'static str,
    pub rollback: AuthorityState,
}

const ACTIONS: [AuthorityAction; 9] = [
    AuthorityAction::Discover,
    AuthorityAction::Connect,
    AuthorityAction::Verify,
    AuthorityAction::Enable,
    AuthorityAction::Pause,
    AuthorityAction::Resume,
    AuthorityAction::Disconnect,
    AuthorityAction::Fault,
    AuthorityAction::Recover,
];

fn contract(
    state: AuthorityState,
    allowed_actions: Vec<AuthorityAction>,
    labels: (&'static str, &'static str, &'static str, &'static str),
    color: &'static str,
    evidence_requirement: &'static str,
    persistence: &'static str,
    rollback: AuthorityState,
) -> AuthorityContract {
    let prohibited_actions = ACTIONS
        .into_iter()
        .filter(|action| !allowed_actions.contains(action))
        .collect();
    AuthorityContract {
        state,
        allowed_actions,
        prohibited_actions,
        header_label: labels.0,
        interlink_label: labels.0,
        governor_label: labels.1,
        process_qos_label: labels.2,
        center_core_label: labels.3,
        color,
        evidence_requirement,
        persistence,
        rollback,
    }
}

pub fn authority_contract(state: AuthorityState) -> AuthorityContract {
    use AuthorityAction::*;
    use AuthorityState::*;
    match state {
        Observation => contract(
            state,
            vec![Discover, Fault],
            ("OBSERVATION LINK", "MONITOR ONLY", "OFF", "MONITOR ONLY"),
            "amber",
            "fresh host telemetry",
            "generic observation segment",
            Observation,
        ),
        Discovered => contract(
            state,
            vec![Discover, Connect, Fault],
            ("TARGET DISCOVERED", "MONITOR ONLY", "OFF", "MONITOR ONLY"),
            "amber",
            "live process discovery result",
            "generic observation segment",
            Observation,
        ),
        Connected => contract(
            state,
            vec![Verify, Disconnect, Fault],
            ("CONNECTED · VERIFY REQUIRED", "WAIT", "OFF", "MONITOR ONLY"),
            "amber",
            "PID and executable identity captured",
            "target segment, unverified",
            Observation,
        ),
        Verified => contract(
            state,
            vec![Enable, Disconnect, Fault],
            ("VERIFIED CONNECTED", "READY", "READY", "MONITOR ONLY"),
            "green",
            "fresh identity and governor capability receipt",
            "target segment, verified",
            Connected,
        ),
        Active => contract(
            state,
            vec![Pause, Disconnect, Fault],
            (
                "VERIFIED ACTIVE",
                "POLICY ACTIVE",
                "ACTIVE",
                "SELECTED MODE",
            ),
            "green",
            "successful applied-QoS receipt and fresh telemetry",
            "target segment with control output",
            Verified,
        ),
        Paused => contract(
            state,
            vec![Resume, Disconnect, Fault],
            ("VERIFIED PAUSED", "PAUSED", "READY", "MONITOR ONLY"),
            "amber",
            "prior verification remains fresh",
            "target segment, applied modulation zero",
            Verified,
        ),
        Faulted => contract(
            state,
            vec![Recover, Disconnect],
            ("AUTHORITY FAULT", "FAULT", "OFF", "MONITOR ONLY"),
            "red",
            "failed invariant and last valid state",
            "observation plus failure receipt",
            Observation,
        ),
        Disconnected => contract(
            state,
            vec![Discover, Fault],
            ("OBSERVATION LINK", "MONITOR ONLY", "OFF", "MONITOR ONLY"),
            "amber",
            "disconnect receipt",
            "new generic observation segment",
            Observation,
        ),
    }
}

pub fn transition(
    current: AuthorityState,
    action: AuthorityAction,
) -> Result<AuthorityState, String> {
    use AuthorityAction::*;
    use AuthorityState::*;
    let next = match (current, action) {
        (Observation | Disconnected, Discover) => Discovered,
        (Discovered, Discover) => Discovered,
        (Discovered, Connect) => Connected,
        (Connected, Verify) => Verified,
        (Verified, Enable) => Active,
        (Active, Pause) => Paused,
        (Paused, Resume) => Active,
        (Connected | Verified | Active | Paused | Faulted, Disconnect) => Disconnected,
        (
            Observation | Discovered | Connected | Verified | Active | Paused | Disconnected,
            Fault,
        ) => Faulted,
        (Faulted, Recover) => Observation,
        _ => {
            return Err(format!(
                "transition {:?} is prohibited while authority is {:?}",
                action, current
            ))
        }
    };
    Ok(next)
}
