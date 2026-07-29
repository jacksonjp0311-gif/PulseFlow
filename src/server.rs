use crate::{
    agent_chat::{self, AgentConfigPatch, AgentSession, ChatRequest},
    analytics,
    authority::{transition, AuthorityAction, AuthorityState},
    config::Config,
    futurist,
    model::{
        now_ms, stable_hash, EvidenceReceipt, IoSignal, LearningStage, OperatingMode, QosLevel,
        RuntimeEvent, RuntimeState, RuntimeTuning, SignalInput, TargetIdentity, TuningPatch,
    },
    replay, storage, system,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};
use sysinfo::{Pid, PidExt, ProcessExt, System, SystemExt};

const INDEX: &str = include_str!("../web/index.html");
const ICON_64: &[u8] = include_bytes!("../assets/icons/pulseflow-governor-64.png");
const ICON_192: &[u8] = include_bytes!("../assets/icons/pulseflow-governor-192.png");
const ICON_512: &[u8] = include_bytes!("../assets/icons/pulseflow-governor-512.png");
const ICON_ICO: &[u8] = include_bytes!("../assets/icons/pulseflow-governor.ico");
const WEB_MANIFEST: &str = r##"{
  "name": "PulseFlow Governor",
  "short_name": "PulseFlow",
  "description": "Pulse feedback workload governor and observation laboratory.",
  "start_url": "/",
  "scope": "/",
  "display": "standalone",
  "background_color": "#0a0a0a",
  "theme_color": "#0a0a0a",
  "icons": [
    {"src":"/assets/icons/pulseflow-governor-192.png","sizes":"192x192","type":"image/png"},
    {"src":"/assets/icons/pulseflow-governor-512.png","sizes":"512x512","type":"image/png"}
  ]
}"##;

#[derive(Debug, Deserialize)]
struct ModeInput {
    mode: OperatingMode,
}

#[derive(Debug, Deserialize)]
struct ControlInput {
    command: String,
}

#[derive(Debug, Deserialize)]
struct RecordingInput {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct StageInput {
    stage: LearningStage,
}

#[derive(Debug, Deserialize)]
struct ReplayInput {
    session_id: String,
    tuning: Option<RuntimeTuning>,
}

#[derive(Debug, Deserialize)]
struct CompareInput {
    baseline_session_id: String,
    candidate_session_id: String,
}

#[derive(Debug, Deserialize)]
struct InterlinkConnectInput {
    pid: u32,
}

#[derive(Debug, Deserialize)]
struct CompactSessionInput {
    session_id: String,
    confirm_delete_raw: bool,
}

#[derive(Debug, Deserialize)]
struct LearnSessionsInput {
    confirm_delete_raw: bool,
    /// When true, compact every inactive session with at least one frame.
    #[serde(default = "default_true")]
    all_inactive: bool,
    /// Optional minimum raw bytes before auto-delete (default 256 KiB).
    #[serde(default = "default_min_raw_bytes")]
    min_raw_bytes: u64,
}

fn default_true() -> bool {
    true
}
fn default_min_raw_bytes() -> u64 {
    256 * 1024
}

#[derive(Debug, Clone, Serialize)]
struct ProcessCandidate {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    executable: String,
    cpu_percent: f64,
    memory_mb: f64,
    controllable: bool,
}

struct Request<'a> {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: &'a [u8],
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    extra_headers: Vec<(String, String)>,
}

impl Response {
    fn json<T: serde::Serialize>(status: u16, value: &T) -> Result<Self, String> {
        Ok(Self {
            status,
            content_type: "application/json; charset=utf-8",
            body: serde_json::to_vec(value).map_err(|error| error.to_string())?,
            extra_headers: Vec::new(),
        })
    }
}

pub fn serve(bind: &str, state: Arc<RwLock<RuntimeState>>, config: Config) -> Result<(), String> {
    let listener = TcpListener::bind(bind).map_err(|error| format!("bind {bind}: {error}"))?;
    let config = Arc::new(config);
    let agent_session = Arc::new(AgentSession::default());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                let config = Arc::clone(&config);
                let agent_session = Arc::clone(&agent_session);
                thread::spawn(move || {
                    let _ = handle_connection(stream, state, config, agent_session);
                });
            }
            Err(error) => eprintln!("◆ HTTP accept warning: {error}"),
        }
    }
    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    state: Arc<RwLock<RuntimeState>>,
    config: Arc<Config>,
    agent_session: Arc<AgentSession>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;
    let bytes = read_request(&mut stream)?;
    let request = parse_request(&bytes)?;
    // Agent chat may take a long provider round-trip; keep the socket alive for the write.
    if request.path.starts_with("/api/agent/") {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(180)));
    }
    let response = match dispatch(request, state, &config, agent_session) {
        Ok(response) => response,
        Err(error) => Response::json(400, &json!({ "error": error }))?,
    };
    respond(&mut stream, response)
}

fn dispatch(
    request: Request<'_>,
    state: Arc<RwLock<RuntimeState>>,
    config: &Config,
    agent_session: Arc<AgentSession>,
) -> Result<Response, String> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Ok(Response {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: INDEX.as_bytes().to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/favicon.ico") => Ok(Response {
            status: 200,
            content_type: "image/x-icon",
            body: ICON_ICO.to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/assets/icons/pulseflow-governor-64.png") => Ok(Response {
            status: 200,
            content_type: "image/png",
            body: ICON_64.to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/assets/icons/pulseflow-governor-192.png") => Ok(Response {
            status: 200,
            content_type: "image/png",
            body: ICON_192.to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/assets/icons/pulseflow-governor-512.png") => Ok(Response {
            status: 200,
            content_type: "image/png",
            body: ICON_512.to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/site.webmanifest") => Ok(Response {
            status: 200,
            content_type: "application/manifest+json; charset=utf-8",
            body: WEB_MANIFEST.as_bytes().to_vec(),
            extra_headers: Vec::new(),
        }),
        ("GET", "/api/status") => {
            let snapshot = state.read().map_err(|_| "state lock poisoned")?.clone();
            Response::json(200, &snapshot)
        }
        ("GET", "/api/history") => {
            let maximum = config.storage.maximum_query_samples.max(1);
            let limit = query_limit(&request.query, 600, maximum);
            let locked = state.read().map_err(|_| "state lock poisoned")?;
            let start = locked.history.len().saturating_sub(limit);
            let frames: Vec<_> = locked.history.iter().skip(start).cloned().collect();
            Response::json(
                200,
                &json!({
                    "session_id": locked.session_id,
                    "frames": frames
                }),
            )
        }
        ("GET", "/api/sessions") => {
            let sessions = storage::list_sessions(&config.storage.directory)?;
            Response::json(200, &sessions)
        }
        ("GET", "/api/learning/iterations") => {
            let datasets = storage::list_learning_datasets(&config.storage.directory)?;
            Response::json(200, &datasets)
        }
        ("GET", "/api/system") => {
            let profile = state
                .read()
                .map_err(|_| "state lock poisoned")?
                .system_profile
                .clone();
            Response::json(200, &profile)
        }
        ("GET", "/api/futurist") => {
            let snapshot = state
                .read()
                .map_err(|_| "state lock poisoned")?
                .futurist
                .clone();
            Response::json(200, &snapshot)
        }
        ("POST", "/api/system/refresh") => {
            let (governor_supported, has_gpu) = {
                let locked = state.read().map_err(|_| "state lock poisoned")?;
                (locked.governor_supported, locked.telemetry.gpu.is_some())
            };
            let profile = system::probe(governor_supported, has_gpu);
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            locked.system_profile = profile.clone();
            let event = locked.push_event(
                "system_profile",
                format!("System profile refreshed: {}", profile.known_as),
            );
            persist_event(config, &event);
            Response::json(200, &profile)
        }
        ("GET", "/api/ledger/tail") => {
            let limit = query_limit(&request.query, 100, 2_000);
            let events = storage::read_event_tail(&config.event_ledger_path, limit)?;
            Response::json(200, &events)
        }
        ("GET", "/api/directive") => {
            let directive = state
                .read()
                .map_err(|_| "state lock poisoned")?
                .directive
                .clone();
            Response::json(200, &directive)
        }
        ("GET", "/api/config") => Response::json(200, config),
        ("GET", "/api/agent/config") => Response::json(200, &agent_chat::public_config()),
        ("POST", "/api/agent/config") => {
            validate_json_content_type(&request.headers)?;
            let patch: AgentConfigPatch = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid agent config JSON: {error}"))?;
            let report = agent_chat::apply_config_patch(patch)?;
            if let Ok(mut locked) = state.write() {
                let event = locked.push_event(
                    "agent",
                    "Cortex agent provider settings updated (keys stored locally under state/).",
                );
                persist_event(config, &event);
            }
            Response::json(200, &report)
        }
        ("POST", "/api/agent/chat") => {
            validate_json_content_type(&request.headers)?;
            let input: ChatRequest = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid agent chat JSON: {error}"))?;
            let reply = agent_chat::chat(input, &agent_session, &state, config)?;
            if let Ok(mut locked) = state.write() {
                locked.agent_bound = true;
                let event = locked.push_event(
                    "agent",
                    format!(
                        "Cortex chat via {} / {}.",
                        reply
                            .get("provider")
                            .and_then(|v| v.as_str())
                            .unwrap_or("provider"),
                        reply
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("model")
                    ),
                );
                persist_event(config, &event);
            }
            Response::json(200, &reply)
        }
        ("POST", "/api/agent/clear") => {
            agent_session.clear();
            Response::json(200, &json!({ "cleared": true }))
        }
        ("GET", "/api/agent/cortex") => {
            let locked = state.read().map_err(|_| "state lock poisoned")?;
            Response::json(200, &agent_chat::build_cortex_snapshot(&locked, config))
        }
        ("GET", "/api/processes") => {
            let processes = list_processes();
            if let Ok(mut locked) = state.write() {
                locked.discovered_pids = processes
                    .iter()
                    .filter(|candidate| candidate.controllable)
                    .map(|candidate| candidate.pid)
                    .collect();
                if matches!(
                    locked.authority_state,
                    AuthorityState::Observation | AuthorityState::Disconnected
                ) && !locked.discovered_pids.is_empty()
                {
                    locked.authority_state = AuthorityState::Discovered;
                    locked.last_valid_authority_state = AuthorityState::Discovered;
                }
            }
            Response::json(200, &processes)
        }
        ("GET", "/api/interlink/handshake") => Response::json(
            200,
            &json!({
                "protocol": "pulseflow.interlink.v1",
                "transport": "HTTP/1.1 localhost",
                "authority": "process-scoped",
                "phases": ["discover", "select", "connect", "verify", "enable"],
                "telemetry_channel": "/api/status",
                "process_discovery": "/api/processes",
                "agent_signal_channel": "/api/signal",
                "agent_directive_channel": "/api/directive",
                "initial_authority": "none",
                "remote_network_exposure": false
            }),
        ),
        ("GET", "/api/interlink/verify") => {
            let locked = state.read().map_err(|_| "state lock poisoned")?;
            Response::json(200, &interlink_report(&locked))
        }
        ("POST", "/api/interlink/verify") => {
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if locked.authority_state != AuthorityState::Connected {
                return Err(format!(
                    "verification requires connected authority, current state is {:?}",
                    locked.authority_state
                ));
            }
            let identity = locked
                .target_identity
                .clone()
                .ok_or("connected target identity is missing")?;
            let candidate = find_process(identity.pid)
                .ok_or_else(|| format!("target PID {} exited before verification", identity.pid))?;
            let observed_hash = stable_hash(candidate.executable.as_bytes());
            if observed_hash != identity.executable_path_hash
                || candidate.name != identity.executable
            {
                locked.authority_state = AuthorityState::Faulted;
                locked.failed_invariant = Some("pid_identity_changed".into());
                return Err(
                    "PID identity changed before verification; rediscovery is required".into(),
                );
            }
            let previous = locked.authority_state;
            let resulting = transition(previous, AuthorityAction::Verify)?;
            locked.authority_state = resulting;
            locked.last_valid_authority_state = resulting;
            locked.failed_invariant = None;
            let receipt = make_receipt(
                &locked,
                config,
                "VERIFY",
                previous,
                resulting,
                vec![
                    "pid_alive".into(),
                    "executable_identity_match".into(),
                    "governor_supported".into(),
                ],
                "READY",
                true,
                None,
            );
            let event = locked.push_receipt(receipt);
            persist_event(config, &event);
            Response::json(200, &interlink_report(&locked))
        }
        ("POST", "/api/interlink/connect") => {
            validate_json_content_type(&request.headers)?;
            let input: InterlinkConnectInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid interlink JSON: {error}"))?;
            let candidate = find_process(input.pid)
                .ok_or_else(|| format!("PID {} is not visible or has already exited", input.pid))?;
            if !candidate.controllable {
                return Err(
                    "PulseFlow cannot attach to its own runtime or a protected system PID".into(),
                );
            }
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if !locked.discovered_pids.contains(&candidate.pid) {
                return Err(
                    "The target must be returned by process discovery before connect".into(),
                );
            }
            if !matches!(
                locked.authority_state,
                AuthorityState::Discovered | AuthorityState::Disconnected
            ) {
                return Err(format!(
                    "connect is prohibited while authority is {:?}",
                    locked.authority_state
                ));
            }
            let previous = AuthorityState::Discovered;
            let resulting = transition(previous, AuthorityAction::Connect)?;
            locked.target_pid = Some(candidate.pid);
            locked.target_label = candidate.name.clone();
            locked.target_identity = Some(TargetIdentity {
                pid: candidate.pid,
                executable: candidate.name.clone(),
                executable_path_hash: stable_hash(candidate.executable.as_bytes()),
                connected_at_ms: now_ms(),
            });
            locked.target_revision = locked.target_revision.saturating_add(1);
            locked.governor_active = false;
            locked.authority_state = resulting;
            locked.last_valid_authority_state = resulting;
            locked.verification_receipt = None;
            locked.failed_invariant = None;
            locked.recording = true;
            let session_event = locked.begin_new_epoch("baseline");
            persist_event(config, &session_event);
            let event = locked.push_event(
                "interlink",
                format!(
                    "Process {} (PID {}) connected; authority remains paused until explicitly enabled.",
                    candidate.name, candidate.pid
                ),
            );
            persist_event(config, &event);
            let receipt = make_receipt(
                &locked,
                config,
                "CONNECT",
                previous,
                resulting,
                vec![
                    "discovery_membership".into(),
                    "pid_alive".into(),
                    "identity_captured".into(),
                ],
                "MONITOR_ONLY",
                true,
                None,
            );
            let receipt_event = locked.push_receipt(receipt);
            persist_event(config, &receipt_event);
            Response::json(
                200,
                &json!({
                    "connected": true,
                    "target": candidate,
                    "verification": interlink_report(&locked)
                }),
            )
        }
        ("POST", "/api/interlink/disconnect") => {
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            let prior = locked.target_pid;
            let previous = locked.authority_state;
            let mesh_was_on = locked.mesh_mode;
            if !mesh_was_on
                && !matches!(
                    previous,
                    AuthorityState::Connected
                        | AuthorityState::Verified
                        | AuthorityState::Active
                        | AuthorityState::Paused
                        | AuthorityState::Faulted
                )
            {
                return Err(
                    "disconnect requires an existing target, mesh, or authority link".into(),
                );
            }
            let receipt_target = locked.target_identity.clone();
            let resulting = transition(previous, AuthorityAction::Disconnect)?;
            locked.target_pid = None;
            locked.target_label = "system-monitor".into();
            locked.target_identity = None;
            locked.target_revision = locked.target_revision.saturating_add(1);
            locked.mesh_mode = false;
            locked.mesh_note = String::new();
            locked.mesh_targets = 0;
            locked.governor_active = false;
            locked.authority_state = resulting;
            locked.last_valid_authority_state = AuthorityState::Observation;
            locked.verification_receipt = None;
            locked.failed_invariant = None;
            locked.recording = true;
            let session_event = locked.begin_new_epoch("governance_enabled");
            persist_event(config, &session_event);
            let event = locked.push_event(
                "interlink",
                format!(
                    "Process/mesh link disconnected from {:?}; system observation remains live.",
                    prior
                ),
            );
            persist_event(config, &event);
            let receipt = EvidenceReceipt::new(
                &locked.session_id,
                receipt_target.as_ref(),
                "DISCONNECT",
                previous,
                resulting,
                vec!["control_stopped".into(), "host_observation_retained".into()],
                "MONITOR_ONLY",
                config_hash(config),
                true,
                None,
                locked.last_receipt_hash.clone(),
            );
            let receipt_event = locked.push_receipt(receipt);
            persist_event(config, &receipt_event);
            Response::json(200, &interlink_report(&locked))
        }
        ("POST", "/api/interlink/baseline") => {
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if !matches!(
                locked.authority_state,
                AuthorityState::Verified | AuthorityState::Paused
            ) {
                return Err("Verify the target before capturing a baseline".into());
            }
            locked.governor_active = false;
            locked.recording = true;
            let session_event = locked.begin_new_session();
            persist_event(config, &session_event);
            let event = locked.push_event(
                "experiment",
                "Baseline session started with process modulation paused.",
            );
            persist_event(config, &event);
            Response::json(
                200,
                &json!({
                    "session_id": locked.session_id,
                    "verification": interlink_report(&locked)
                }),
            )
        }
        ("POST", "/api/interlink/enable") => {
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if !matches!(
                locked.authority_state,
                AuthorityState::Verified | AuthorityState::Paused
            ) {
                return Err(format!(
                    "enable requires verified or paused authority, current state is {:?}",
                    locked.authority_state
                ));
            }
            let pid = locked
                .target_pid
                .ok_or("Connect a target process before enabling PulseFlow")?;
            if !locked.governor_supported {
                return Err("The active process governor is supported only on Windows".into());
            }
            if find_process(pid).is_none() {
                return Err(format!("Target PID {pid} is no longer alive"));
            }
            let previous = locked.authority_state;
            let action = if previous == AuthorityState::Paused {
                AuthorityAction::Resume
            } else {
                AuthorityAction::Enable
            };
            let resulting = transition(previous, action)?;
            locked.mesh_mode = false;
            locked.mesh_note = String::new();
            locked.mesh_targets = 0;
            locked.governor_active = true;
            locked.authority_state = resulting;
            locked.last_valid_authority_state = resulting;
            locked.recording = true;
            let session_event = locked.begin_new_session();
            persist_event(config, &session_event);
            let event = locked.push_event(
                "interlink",
                format!("PulseFlow enabled for PID {pid}; bounded process QoS modulation armed."),
            );
            persist_event(config, &event);
            let receipt = make_receipt(
                &locked,
                config,
                if action == AuthorityAction::Resume {
                    "RESUME"
                } else {
                    "ENABLE"
                },
                previous,
                resulting,
                vec![
                    "verification_receipt_present".into(),
                    "pid_alive".into(),
                    "governor_supported".into(),
                ],
                "ARMED_PENDING_FIRST_APPLY",
                true,
                None,
            );
            let receipt_event = locked.push_receipt(receipt);
            persist_event(config, &receipt_event);
            Response::json(
                200,
                &json!({
                    "session_id": locked.session_id,
                    "verification": interlink_report(&locked)
                }),
            )
        }
        ("POST", "/api/interlink/mesh") => {
            // Whole-system Pulse Mesh: no single-PID attach required.
            // Host telemetry drives Eco; top pressure processes receive bounded QoS.
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if !locked.governor_supported {
                return Err("Pulse Mesh process QoS requires Windows".into());
            }
            let previous = locked.authority_state;
            locked.mesh_mode = true;
            locked.mesh_note = "Pulse Mesh armed for whole-system host pressure.".into();
            locked.mesh_targets = 0;
            locked.target_pid = None;
            locked.target_label = "pulse-mesh".into();
            locked.target_identity = None;
            locked.governor_active = true;
            locked.authority_state = AuthorityState::Active;
            locked.last_valid_authority_state = AuthorityState::Active;
            locked.failed_invariant = None;
            locked.recording = true;
            locked.target_revision = locked.target_revision.saturating_add(1);
            let session_event = locked.begin_new_session();
            persist_event(config, &session_event);
            let event = locked.push_event(
                "interlink",
                "Pulse Mesh enabled: whole-system observation + bounded multi-process Eco under host pressure.",
            );
            persist_event(config, &event);
            let receipt = make_receipt(
                &locked,
                config,
                "MESH_ENABLE",
                previous,
                AuthorityState::Active,
                vec![
                    "mesh_mode".into(),
                    "no_single_pid_required".into(),
                    "host_pressure_drives_qos".into(),
                    "top_process_eco_bounded".into(),
                ],
                "MESH_ARMED",
                true,
                None,
            );
            let receipt_event = locked.push_receipt(receipt);
            persist_event(config, &receipt_event);
            Response::json(
                200,
                &json!({
                    "session_id": locked.session_id,
                    "mesh_mode": true,
                    "verification": interlink_report(&locked)
                }),
            )
        }
        ("POST", "/api/signal") => {
            validate_json_content_type(&request.headers)?;
            let input: SignalInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid signal JSON: {error}"))?;
            let io = IoSignal::from(input);
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            locked.telemetry.io = io.clone();
            let event = locked.push_event(
                "signal",
                format!("Agent signal received from {}.", io.source),
            );
            persist_event(config, &event);
            Response::json(202, &io)
        }
        ("POST", "/api/mode") => {
            validate_json_content_type(&request.headers)?;
            let input: ModeInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid mode JSON: {error}"))?;
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if locked.mode == input.mode {
                return Response::json(200, &locked.mode);
            }
            locked.mode = input.mode;
            locked.recording = true;
            let epoch_event =
                locked.begin_new_epoch(format!("mode_{:?}", input.mode).to_lowercase());
            persist_event(config, &epoch_event);
            let event =
                locked.push_event("mode", format!("Operating mode set to {:?}.", input.mode));
            persist_event(config, &event);
            Response::json(200, &locked.mode)
        }
        ("POST", "/api/control") => {
            validate_json_content_type(&request.headers)?;
            let input: ControlInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid control JSON: {error}"))?;
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            let authority_before = locked.authority_state;
            let event = match input.command.as_str() {
                "start" => {
                    if locked.authority_state != AuthorityState::Paused {
                        return Err("start is only valid as resume from paused authority".into());
                    }
                    locked.authority_state =
                        transition(locked.authority_state, AuthorityAction::Resume)?;
                    locked.governor_active = true;
                    locked.recording = true;
                    let message = if locked.governor_active {
                        "Process modulation and frame recording started."
                    } else {
                        "Frame recording started in monitor-only mode."
                    };
                    locked.push_event("control", message)
                }
                "pause" => {
                    if locked.authority_state != AuthorityState::Active {
                        return Err("pause requires active governance".into());
                    }
                    locked.governor_active = false;
                    locked.authority_state =
                        transition(locked.authority_state, AuthorityAction::Pause)?;
                    locked.push_event(
                        "control",
                        "Process modulation paused; observation remains live.",
                    )
                }
                "reset" => {
                    let event = locked.begin_new_epoch("operator_reset");
                    locked.recording = true;
                    event
                }
                _ => return Err("control command must be start, pause, or reset".into()),
            };
            persist_event(config, &event);
            if matches!(input.command.as_str(), "start" | "pause") {
                let receipt = make_receipt(
                    &locked,
                    config,
                    if input.command == "start" {
                        "RESUME"
                    } else {
                        "PAUSE"
                    },
                    authority_before,
                    locked.authority_state,
                    vec!["state_gate_passed".into(), "observation_retained".into()],
                    if locked.governor_active {
                        "ARMED"
                    } else {
                        "MONITOR_ONLY"
                    },
                    true,
                    None,
                );
                let receipt_event = locked.push_receipt(receipt);
                persist_event(config, &receipt_event);
            }
            Response::json(200, &*locked)
        }
        ("POST", "/api/recording") => {
            validate_json_content_type(&request.headers)?;
            let input: RecordingInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid recording JSON: {error}"))?;
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            locked.recording = input.enabled;
            let event = locked.push_event(
                "recording",
                if input.enabled {
                    "Observation-frame recording enabled."
                } else {
                    "Observation-frame recording paused."
                },
            );
            persist_event(config, &event);
            Response::json(200, &json!({ "recording": locked.recording }))
        }
        ("POST", "/api/session/new") => {
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            let event = locked.begin_new_epoch("manual_session");
            locked.recording = true;
            persist_event(config, &event);
            Response::json(201, &json!({ "session_id": locked.session_id }))
        }
        ("POST", "/api/learning-stage") => {
            validate_json_content_type(&request.headers)?;
            let input: StageInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid learning-stage JSON: {error}"))?;
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            if locked.learning_stage == input.stage {
                return Response::json(200, &locked.learning_stage);
            }
            locked.learning_stage = input.stage;
            locked.recording = true;
            let epoch_event =
                locked.begin_new_epoch(format!("learning_stage_{:?}", input.stage).to_lowercase());
            persist_event(config, &epoch_event);
            let suffix = if input.stage == LearningStage::BoundedAdaptive
                && !config.agent_policy.allow_bounded_adaptation
            {
                " Bounded gain writes remain disabled by configuration; recommendations stay shadow-only."
            } else {
                ""
            };
            let event = locked.push_event(
                "learning_stage",
                format!("Learning stage set to {:?}.{suffix}", input.stage),
            );
            persist_event(config, &event);
            Response::json(200, &locked.learning_stage)
        }
        ("POST", "/api/tuning") => {
            validate_json_content_type(&request.headers)?;
            let patch: TuningPatch = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid tuning JSON: {error}"))?;
            let mut locked = state.write().map_err(|_| "state lock poisoned")?;
            locked.tuning.apply_patch(patch);
            locked.tuning_revision = locked.tuning_revision.saturating_add(1);
            locked.recording = true;
            let tuning_revision = locked.tuning_revision;
            let epoch_event = locked.begin_new_epoch(format!("tuning_revision_{tuning_revision}"));
            persist_event(config, &epoch_event);
            let event = locked.push_event("tuning", "Bounded runtime controller tuning updated.");
            persist_event(config, &event);
            Response::json(200, &locked.tuning)
        }
        ("POST", "/api/session/compact") => {
            validate_json_content_type(&request.headers)?;
            let input: CompactSessionInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid compaction JSON: {error}"))?;
            if !input.confirm_delete_raw {
                return Err("confirm_delete_raw must be true".into());
            }
            let current_session = state
                .read()
                .map_err(|_| "state lock poisoned")?
                .session_id
                .clone();
            if input.session_id == current_session {
                return Err(
                    "The active session cannot be compacted. Start a new session first.".into(),
                );
            }
            let receipt = compact_one_session(state.clone(), config, &input.session_id)?;
            Response::json(200, &receipt)
        }
        ("POST", "/api/session/learn") => {
            validate_json_content_type(&request.headers)?;
            let input: LearnSessionsInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid learn JSON: {error}"))?;
            if !input.confirm_delete_raw {
                return Err("confirm_delete_raw must be true".into());
            }
            let current_session = state
                .read()
                .map_err(|_| "state lock poisoned")?
                .session_id
                .clone();
            let sessions = storage::list_sessions(&config.storage.directory)?;
            let mut receipts = Vec::new();
            let mut total_freed = 0u64;
            for session in sessions {
                if session.session_id == current_session {
                    continue;
                }
                if session.samples == 0 {
                    continue;
                }
                if !input.all_inactive && session.bytes < input.min_raw_bytes {
                    continue;
                }
                match compact_one_session(state.clone(), config, &session.session_id) {
                    Ok(receipt) => {
                        total_freed = total_freed.saturating_add(receipt.freed_bytes);
                        receipts.push(receipt);
                    }
                    Err(error) => {
                        if let Ok(mut locked) = state.write() {
                            let event = locked.push_event(
                                "session_learn_fault",
                                format!("{}: {error}", session.session_id),
                            );
                            persist_event(config, &event);
                        }
                    }
                }
            }
            if let Ok(mut locked) = state.write() {
                let event = locked.push_event(
                    "session_learn",
                    format!(
                        "Learned {} session(s) into graph blobs; freed {} bytes of raw JSONL.",
                        receipts.len(),
                        total_freed
                    ),
                );
                persist_event(config, &event);
            }
            Response::json(
                200,
                &json!({
                    "schema_version": "pulseflow.learn-batch.v1",
                    "compacted": receipts.len(),
                    "freed_bytes": total_freed,
                    "receipts": receipts,
                }),
            )
        }
        ("POST", "/api/replay") => {
            validate_json_content_type(&request.headers)?;
            let input: ReplayInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid replay JSON: {error}"))?;
            let frames = storage::read_session_frames(
                &config.storage.directory,
                &input.session_id,
                config.storage.maximum_query_samples,
            )?;
            let tuning = match input.tuning {
                Some(tuning) => tuning,
                None => state
                    .read()
                    .map_err(|_| "state lock poisoned")?
                    .tuning
                    .clone(),
            };
            let report = replay::run_replay(&input.session_id, &frames, config, &tuning);
            if let Ok(mut locked) = state.write() {
                let event = locked.push_event(
                    "replay",
                    format!(
                        "Controller replay completed for {} frames from {}.",
                        frames.len(),
                        input.session_id
                    ),
                );
                persist_event(config, &event);
            }
            Response::json(200, &report)
        }
        ("POST", "/api/compare") => {
            validate_json_content_type(&request.headers)?;
            let input: CompareInput = serde_json::from_slice(request.body)
                .map_err(|error| format!("invalid comparison JSON: {error}"))?;
            let baseline_frames = storage::read_session_frames(
                &config.storage.directory,
                &input.baseline_session_id,
                config.storage.maximum_query_samples,
            )?;
            let candidate_frames = storage::read_session_frames(
                &config.storage.directory,
                &input.candidate_session_id,
                config.storage.maximum_query_samples,
            )?;
            let baseline = analytics::summarize_session(
                &input.baseline_session_id,
                &baseline_frames,
                config.analytics.epsilon,
            );
            let candidate = analytics::summarize_session(
                &input.candidate_session_id,
                &candidate_frames,
                config.analytics.epsilon,
            );
            let report = analytics::compare_sessions(baseline, candidate);
            if let Ok(mut locked) = state.write() {
                let event = locked.push_event(
                    "comparison",
                    format!(
                        "Compared baseline {} with candidate {}.",
                        input.baseline_session_id, input.candidate_session_id
                    ),
                );
                persist_event(config, &event);
            }
            Response::json(200, &report)
        }
        ("GET", "/health") => Ok(Response {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body: b"ok".to_vec(),
            extra_headers: Vec::new(),
        }),
        _ => dynamic_route(&request, state, config),
    }
}

fn compact_one_session(
    state: Arc<RwLock<RuntimeState>>,
    config: &Config,
    session_id: &str,
) -> Result<storage::SessionCompactionReceipt, String> {
    let frames = storage::read_session_frames(&config.storage.directory, session_id, usize::MAX)?;
    if frames.is_empty() {
        return Err("Cannot compact a session with no validated observation frames.".into());
    }
    let mut summary = analytics::summarize_session(session_id, &frames, config.analytics.epsilon);
    let calibration = futurist::calibrate_session(session_id, &frames, config.analytics.epsilon);
    summary.futurist_skill_improvement = calibration.skill.relative_improvement;
    summary.futurist_envelope = if calibration.skill.beats_persist {
        "calibrated".into()
    } else {
        "hold".into()
    };
    let (form_factor, known_as) = state
        .read()
        .map(|locked| {
            (
                locked.system_profile.form_factor.as_str().to_string(),
                locked.system_profile.known_as.clone(),
            )
        })
        .unwrap_or_else(|_| ("desktop".into(), "unknown".into()));
    summary.system_form_factor = form_factor.clone();
    let points = analytics::learning_graph_points(&frames, 240);
    let options = storage::CompactOptions {
        system_form_factor: form_factor,
        system_known_as: known_as,
        futurist_skill_mae_h5: calibration.skill.mae_h5,
        futurist_skill_improvement: calibration.skill.relative_improvement,
        futurist_beats_persist: calibration.skill.beats_persist,
    };
    let receipt = storage::compact_session_with_options(
        &config.storage.directory,
        session_id,
        summary,
        points,
        options,
    )?;
    if let Ok(mut locked) = state.write() {
        if calibration.skill.samples_scored > locked.futurist.skill.samples_scored {
            locked.futurist.skill = calibration.skill.clone();
            locked.futurist.calibrated = calibration.skill.beats_persist;
        }
        let event = locked.push_event(
            "session_compacted",
            format!(
                "Session {} → graph blob; freed {} bytes; futurist Δ={:.0}% vs persist.",
                receipt.session_id,
                receipt.freed_bytes,
                calibration.skill.relative_improvement * 100.0
            ),
        );
        persist_event(config, &event);
    }
    // Persist calibration receipt beside analysis receipts.
    let calib_dir = std::path::Path::new(&config.storage.directory).join("analysis-receipts");
    let _ = std::fs::create_dir_all(&calib_dir);
    let calib_path = calib_dir.join(format!("{session_id}.futurist.json"));
    let _ = std::fs::write(
        calib_path,
        serde_json::to_vec_pretty(&calibration).unwrap_or_default(),
    );
    Ok(receipt)
}

fn dynamic_route(
    request: &Request<'_>,
    _state: Arc<RwLock<RuntimeState>>,
    config: &Config,
) -> Result<Response, String> {
    if request.method == "GET" {
        if let Some(iteration_id) = request.path.strip_prefix("/api/learning/dataset/") {
            let dataset = storage::read_learning_dataset(&config.storage.directory, iteration_id)?;
            return Response::json(200, &dataset);
        }
        if let Some(session_id) = request.path.strip_prefix("/api/summary/") {
            let frames = storage::read_session_frames(
                &config.storage.directory,
                session_id,
                config.storage.maximum_query_samples,
            )?;
            let summary =
                analytics::summarize_session(session_id, &frames, config.analytics.epsilon);
            return Response::json(200, &summary);
        }
        if let Some(session_id) = request.path.strip_prefix("/api/session/") {
            let limit = query_limit(
                &request.query,
                config.storage.maximum_query_samples,
                config.storage.maximum_query_samples,
            );
            let frames =
                storage::read_session_frames(&config.storage.directory, session_id, limit)?;
            return Response::json(200, &json!({ "session_id": session_id, "frames": frames }));
        }
        if let Some(session_id) = request.path.strip_prefix("/api/export/") {
            let bytes = storage::read_session_bytes(&config.storage.directory, session_id)?;
            return Ok(Response {
                status: 200,
                content_type: "application/x-ndjson; charset=utf-8",
                body: bytes,
                extra_headers: vec![(
                    "Content-Disposition".into(),
                    format!("attachment; filename=\"{session_id}.jsonl\""),
                )],
            });
        }
    }
    Response::json(404, &json!({ "error": "not found" }))
}

fn list_processes() -> Vec<ProcessCandidate> {
    let mut system = System::new_all();
    system.refresh_processes();
    let own_pid = std::process::id();
    let mut processes: Vec<ProcessCandidate> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let pid = pid.as_u32();
            ProcessCandidate {
                pid,
                parent_pid: process.parent().map(|parent| parent.as_u32()),
                name: process.name().to_string(),
                executable: process.exe().to_string_lossy().to_string(),
                cpu_percent: process.cpu_usage() as f64,
                memory_mb: process.memory() as f64 / 1024.0 / 1024.0,
                controllable: pid > 4 && pid != own_pid,
            }
        })
        .collect();
    processes.sort_by(|left, right| {
        right
            .cpu_percent
            .partial_cmp(&left.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    processes.truncate(400);
    processes
}

fn find_process(pid: u32) -> Option<ProcessCandidate> {
    let mut system = System::new_all();
    system.refresh_process(Pid::from_u32(pid));
    let process = system.process(Pid::from_u32(pid))?;
    let own_pid = std::process::id();
    Some(ProcessCandidate {
        pid,
        parent_pid: process.parent().map(|parent| parent.as_u32()),
        name: process.name().to_string(),
        executable: process.exe().to_string_lossy().to_string(),
        cpu_percent: process.cpu_usage() as f64,
        memory_mb: process.memory() as f64 / 1024.0 / 1024.0,
        controllable: pid > 4 && pid != own_pid,
    })
}

fn interlink_report(state: &RuntimeState) -> serde_json::Value {
    let now = now_ms();
    let frame_age_ms = state
        .latest_frame
        .as_ref()
        .map(|frame| now.saturating_sub(frame.timestamp_ms));
    let telemetry_live = frame_age_ms.is_some_and(|age| age <= 5_000);
    let current_candidate = state.target_pid.and_then(find_process);
    let target_alive = match (&state.target_identity, &current_candidate) {
        (Some(identity), Some(candidate)) => {
            candidate.controllable
                && candidate.name == identity.executable
                && stable_hash(candidate.executable.as_bytes()) == identity.executable_path_hash
        }
        (None, _) => false,
        _ => false,
    };
    let verification_fresh = state
        .verification_receipt
        .as_ref()
        .is_some_and(|receipt| now.saturating_sub(receipt.timestamp_ms) <= 300_000);
    // Mesh is host-scoped: armed Active counts as live process channel even while QoS is MonitorOnly.
    // Single-PID path still requires applied Eco/Thermal + live verified target.
    let process_qos_active = state.authority_state == AuthorityState::Active
        && state.governor_active
        && (state.mesh_mode
            || (state.control.applied_qos != QosLevel::MonitorOnly
                && target_alive
                && verification_fresh));
    let agent_channel_live = state.agent_bound && state.telemetry.io_signal_fresh;
    let contract = state.authority_contract();
    let state_label = match state.authority_state {
        AuthorityState::Observation => "observation",
        AuthorityState::Discovered => "discovered",
        AuthorityState::Connected => "connected",
        AuthorityState::Verified => "verified",
        AuthorityState::Active if process_qos_active => "active",
        AuthorityState::Active => "activating",
        AuthorityState::Paused => "paused",
        AuthorityState::Faulted => "faulted",
        AuthorityState::Disconnected => "disconnected",
    };
    let authority_scope = if state.mesh_mode {
        "host-mesh"
    } else if state.target_pid.is_some() {
        "process-scoped"
    } else {
        "observation-only"
    };
    json!({
        "schema_version": "pulseflow.interlink-verification.v1",
        "verification_id": state.verification_receipt.as_ref().map(|receipt| receipt.receipt_id.clone()),
        "verified_at_ms": now,
        "state": state_label,
        "authority_state": state.authority_state,
        "authority_contract": contract,
        "api_connected": true,
        "telemetry_live": telemetry_live,
        "frame_age_ms": frame_age_ms,
        "target_pid": state.target_pid,
        "target_label": state.target_label.clone(),
        "target_alive": target_alive,
        "verification_fresh": verification_fresh,
        "governor_supported": state.governor_supported,
        "governor_armed": state.governor_active,
        "mesh_mode": state.mesh_mode,
        "mesh_targets": state.mesh_targets,
        "mesh_note": state.mesh_note,
        "requested_qos": state.control.requested_qos,
        "applied_qos": state.control.applied_qos,
        "process_qos_active": process_qos_active,
        "agent_signal_live": state.telemetry.io_signal_fresh,
        "agent_bound": state.agent_bound,
        "channels": {
            "host_telemetry": true,
            "process_qos": process_qos_active,
            "agent_adapter": agent_channel_live
        },
        "authority": authority_scope,
        "capacity_signal": state.control.capacity_signal,
        "control_authority": state.control.control_authority,
        "controller_effort": state.control.controller_effort,
        "applied_modulation": state.control.applied_modulation,
        "failed_invariant": state.failed_invariant,
        "evidence": state.verification_receipt,
        "limits": ["no clocks", "no voltage", "no firmware", "no fan-curve writes"]
    })
}

fn config_hash(config: &Config) -> String {
    serde_json::to_vec(config)
        .map(|bytes| stable_hash(&bytes))
        .unwrap_or_else(|_| "config-unavailable".into())
}

#[allow(clippy::too_many_arguments)]
fn make_receipt(
    state: &RuntimeState,
    config: &Config,
    requested_transition: &str,
    previous_state: AuthorityState,
    resulting_state: AuthorityState,
    verification_checks: Vec<String>,
    qos_action_result: &str,
    success: bool,
    failure_reason: Option<String>,
) -> EvidenceReceipt {
    EvidenceReceipt::new(
        &state.session_id,
        state.target_identity.as_ref(),
        requested_transition,
        previous_state,
        resulting_state,
        verification_checks,
        qos_action_result,
        config_hash(config),
        success,
        failure_reason,
        state.last_receipt_hash.clone(),
    )
}

fn persist_event(config: &Config, event: &RuntimeEvent) {
    if let Err(error) = storage::append_event(&config.event_ledger_path, event) {
        eprintln!("◆ event-ledger warning: {error}");
    }
}

fn query_limit(query: &HashMap<String, String>, default: usize, maximum: usize) -> usize {
    query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, maximum.max(1))
}

fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(8_192);
    let mut buffer = [0u8; 8_192];
    let mut content_length = 0usize;
    let mut header_end = None;

    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
        if data.len() > 1_048_576 {
            return Err("request too large".into());
        }
        if header_end.is_none() {
            header_end = find_header_end(&data);
            if let Some(end) = header_end {
                let header_text = String::from_utf8_lossy(&data[..end]);
                content_length = parse_content_length(&header_text).unwrap_or_default();
            }
        }
        if let Some(end) = header_end {
            if data.len() >= end + 4 + content_length {
                break;
            }
        }
    }
    Ok(data)
}

fn parse_request(data: &[u8]) -> Result<Request<'_>, String> {
    let end = find_header_end(data).ok_or("malformed HTTP request")?;
    let head = String::from_utf8_lossy(&data[..end]);
    let mut lines = head.lines();
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_string();
    let raw_target = parts.next().ok_or("missing path")?;
    let (path, query) = match raw_target.split_once('?') {
        Some((path, query)) => (path.to_string(), parse_query(query)),
        None => (raw_target.to_string(), HashMap::new()),
    };
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let body_start = end + 4;
    let body = if body_start <= data.len() {
        &data[body_start..]
    } else {
        &[]
    };
    Ok(Request {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

fn validate_json_content_type(headers: &HashMap<String, String>) -> Result<(), String> {
    let content_type = headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or("");
    if content_type.starts_with("application/json") {
        Ok(())
    } else {
        Err("Content-Type must be application/json".into())
    }
}

fn respond(stream: &mut TcpStream, response: Response) -> Result<(), String> {
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let mut header = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    for (name, value) in response.extra_headers {
        header.push_str(&format!("{name}: {value}\r\n"));
    }
    header.push_str("\r\n");
    stream
        .write_all(header.as_bytes())
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&response.body)
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}
