//! Embedded Cortex agent: multi-provider chat interlinked with live PulseFlow state.
//!
//! API keys live in `state/agent-secrets.json` (gitignored under state/).
//! Provider calls use the OpenAI-compatible chat/completions protocol so SpaceXAI/xAI,
//! OpenAI, OpenRouter, Groq, DeepSeek, and Ollama share one code path.

use crate::{
    config::Config,
    model::{now_ms, RuntimeState},
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Duration,
};

const SECRETS_FILE: &str = "state/agent-secrets.json";
const MAX_TOOL_ROUNDS: usize = 4;
const MAX_HISTORY_MESSAGES: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub default_model: String,
    pub env_hint: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSecrets {
    #[serde(default)]
    pub active_provider: String,
    #[serde(default)]
    pub active_model: String,
    /// provider_id -> api key (never returned in full to the UI)
    #[serde(default)]
    pub keys: HashMap<String, String>,
    /// optional per-provider model override
    #[serde(default)]
    pub models: HashMap<String, String>,
    /// optional custom base URL overrides
    #[serde(default)]
    pub base_urls: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
}

#[derive(Debug, Deserialize)]
pub struct AgentConfigPatch {
    #[serde(default)]
    pub active_provider: Option<String>,
    #[serde(default)]
    pub active_model: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Provider the api_key applies to (defaults to active_provider)
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// In-process chat transcript for the embedded panel (session-local).
pub struct AgentSession {
    history: Mutex<Vec<ChatMessage>>,
}

impl Default for AgentSession {
    fn default() -> Self {
        Self {
            history: Mutex::new(Vec::new()),
        }
    }
}

impl AgentSession {
    pub fn clear(&self) {
        if let Ok(mut h) = self.history.lock() {
            h.clear();
        }
    }

    pub fn snapshot(&self) -> Vec<ChatMessage> {
        self.history
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    fn push(&self, msg: ChatMessage) {
        if let Ok(mut h) = self.history.lock() {
            h.push(msg);
            let overflow = h.len().saturating_sub(MAX_HISTORY_MESSAGES);
            if overflow > 0 {
                h.drain(0..overflow);
            }
        }
    }
}

pub fn catalog() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec {
            id: "spacexai".into(),
            label: "SpaceXAI / Grok (xAI)".into(),
            base_url: "https://api.x.ai/v1".into(),
            default_model: "grok-4-1-fast-reasoning".into(),
            env_hint: "XAI_API_KEY".into(),
            notes: "Default. OpenAI-compatible. api.x.ai".into(),
        },
        ProviderSpec {
            id: "openai".into(),
            label: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-4.1-mini".into(),
            env_hint: "OPENAI_API_KEY".into(),
            notes: "Official OpenAI chat completions.".into(),
        },
        ProviderSpec {
            id: "openrouter".into(),
            label: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            default_model: "openai/gpt-4.1-mini".into(),
            env_hint: "OPENROUTER_API_KEY".into(),
            notes: "Multi-model router.".into(),
        },
        ProviderSpec {
            id: "nemotron_free".into(),
            label: "NVIDIA Nemotron Ultra 550B (free)".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            default_model: "nvidia/nemotron-3-ultra-550b-a55b:free".into(),
            env_hint: "OPENROUTER_API_KEY".into(),
            notes: "OpenRouter free tier · nvidia/nemotron-3-ultra-550b-a55b:free".into(),
        },
        ProviderSpec {
            id: "anthropic".into(),
            label: "Anthropic (OpenAI-compat bridge)".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            default_model: "claude-sonnet-4-20250514".into(),
            env_hint: "ANTHROPIC_API_KEY".into(),
            notes: "Requires OpenAI-compatible gateway if native Messages API only.".into(),
        },
        ProviderSpec {
            id: "groq".into(),
            label: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            default_model: "llama-3.3-70b-versatile".into(),
            env_hint: "GROQ_API_KEY".into(),
            notes: "Fast OpenAI-compatible inference.".into(),
        },
        ProviderSpec {
            id: "deepseek".into(),
            label: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            env_hint: "DEEPSEEK_API_KEY".into(),
            notes: "OpenAI-compatible DeepSeek API.".into(),
        },
        ProviderSpec {
            id: "ollama".into(),
            label: "Ollama (local)".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            default_model: "llama3.2".into(),
            env_hint: "(none — local)".into(),
            notes: "Local models; leave API key blank or set ollama.".into(),
        },
        ProviderSpec {
            id: "custom".into(),
            label: "Custom OpenAI-compatible".into(),
            base_url: "http://127.0.0.1:8080/v1".into(),
            default_model: "local-model".into(),
            env_hint: "CUSTOM_API_KEY".into(),
            notes: "Set base URL + model in settings.".into(),
        },
    ]
}

pub fn secrets_path() -> PathBuf {
    PathBuf::from(SECRETS_FILE)
}

pub fn load_secrets() -> AgentSecrets {
    let path = secrets_path();
    if !path.exists() {
        return AgentSecrets {
            active_provider: "spacexai".into(),
            active_model: String::new(),
            keys: HashMap::new(),
            models: HashMap::new(),
            base_urls: HashMap::new(),
        };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_secrets(secrets: &AgentSecrets) -> Result<(), String> {
    let path = secrets_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create secrets dir: {e}"))?;
    }
    let text = serde_json::to_string_pretty(secrets).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("write secrets: {e}"))
}

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 8 {
        return "••••••••".into();
    }
    format!(
        "{}…{}",
        &trimmed[..4],
        &trimmed[trimmed.len().saturating_sub(4)..]
    )
}

fn resolve_key(provider: &str, secrets: &AgentSecrets) -> Option<String> {
    if let Some(k) = secrets.keys.get(provider) {
        let t = k.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let env_names: &[&str] = match provider {
        "spacexai" => &["XAI_API_KEY", "SPACEXAI_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "openrouter" | "nemotron_free" => &["OPENROUTER_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY"],
        "groq" => &["GROQ_API_KEY"],
        "deepseek" => &["DEEPSEEK_API_KEY"],
        "ollama" => &["OLLAMA_API_KEY"],
        "custom" => &["CUSTOM_API_KEY", "OPENAI_API_KEY"],
        _ => &[],
    };
    // Nemotron free rides OpenRouter keys — fall back to a saved openrouter key.
    if provider == "nemotron_free" {
        if let Some(k) = secrets.keys.get("openrouter") {
            let t = k.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    for name in env_names {
        if let Ok(v) = std::env::var(name) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    if provider == "ollama" {
        return Some("ollama".into());
    }
    None
}

fn provider_spec(id: &str) -> Option<ProviderSpec> {
    catalog().into_iter().find(|p| p.id == id)
}

fn resolve_base_url(provider: &str, secrets: &AgentSecrets) -> String {
    if let Some(u) = secrets.base_urls.get(provider) {
        let t = u.trim();
        if !t.is_empty() {
            return t.trim_end_matches('/').to_string();
        }
    }
    provider_spec(provider)
        .map(|p| p.base_url)
        .unwrap_or_else(|| "https://api.x.ai/v1".into())
}

fn resolve_model(provider: &str, secrets: &AgentSecrets, override_model: Option<&str>) -> String {
    if let Some(m) = override_model {
        let t = m.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    if !secrets.active_model.trim().is_empty() && secrets.active_provider == provider {
        return secrets.active_model.trim().to_string();
    }
    if let Some(m) = secrets.models.get(provider) {
        let t = m.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    provider_spec(provider)
        .map(|p| p.default_model)
        .unwrap_or_else(|| "grok-4-1-fast-reasoning".into())
}

pub fn public_config() -> Value {
    let secrets = load_secrets();
    let providers: Vec<Value> = catalog()
        .into_iter()
        .map(|p| {
            let has_key = resolve_key(&p.id, &secrets).is_some();
            let masked = secrets
                .keys
                .get(&p.id)
                .map(|k| mask_key(k))
                .unwrap_or_default();
            json!({
                "id": p.id,
                "label": p.label,
                "base_url": secrets.base_urls.get(&p.id).cloned().unwrap_or(p.base_url),
                "default_model": secrets.models.get(&p.id).cloned().unwrap_or(p.default_model),
                "env_hint": p.env_hint,
                "notes": p.notes,
                "has_key": has_key,
                "key_preview": masked,
            })
        })
        .collect();
    let active = if secrets.active_provider.is_empty() {
        "spacexai".to_string()
    } else {
        secrets.active_provider.clone()
    };
    let model = resolve_model(&active, &secrets, None);
    json!({
        "schema_version": "pulseflow.agent-config.v1",
        "active_provider": active,
        "active_model": model,
        "providers": providers,
        "tools": tool_catalog_public(),
        "secrets_path": SECRETS_FILE,
    })
}

pub fn apply_config_patch(patch: AgentConfigPatch) -> Result<Value, String> {
    let mut secrets = load_secrets();
    if let Some(p) = patch.active_provider {
        if provider_spec(&p).is_none() {
            return Err(format!("unknown provider: {p}"));
        }
        secrets.active_provider = p;
    }
    if secrets.active_provider.is_empty() {
        secrets.active_provider = "spacexai".into();
    }
    let key_provider = patch
        .provider
        .clone()
        .unwrap_or_else(|| secrets.active_provider.clone());
    if let Some(key) = patch.api_key {
        let t = key.trim();
        if t.is_empty() {
            secrets.keys.remove(&key_provider);
        } else {
            secrets.keys.insert(key_provider.clone(), t.to_string());
            // Share OpenRouter credentials with the free Nemotron preset.
            if key_provider == "nemotron_free" || key_provider == "openrouter" {
                secrets
                    .keys
                    .insert("openrouter".into(), t.to_string());
                secrets
                    .keys
                    .insert("nemotron_free".into(), t.to_string());
            }
        }
    }
    if let Some(model) = patch.active_model {
        let t = model.trim().to_string();
        secrets.active_model = t.clone();
        if !t.is_empty() {
            secrets.models.insert(secrets.active_provider.clone(), t);
        }
    }
    if let Some(base) = patch.base_url {
        let t = base.trim().to_string();
        if t.is_empty() {
            secrets.base_urls.remove(&key_provider);
        } else {
            secrets.base_urls.insert(key_provider, t);
        }
    }
    save_secrets(&secrets)?;
    Ok(public_config())
}

fn tool_catalog_public() -> Value {
    json!([
        {"name": "get_system_status", "purpose": "Live host telemetry, QoS, mesh, authority."},
        {"name": "rehydrate_cortex", "purpose": "Rebuild memory: metrics, regime, futurist, learning blobs."},
        {"name": "list_sessions", "purpose": "List recorded sessions and sample counts."},
        {"name": "get_session_summary", "purpose": "Summarize one session from storage."},
        {"name": "get_directive", "purpose": "Current agent resource directive."},
        {"name": "get_events_tail", "purpose": "Recent ledger events."},
        {"name": "aria_overview", "purpose": "Read ARIA lattice / verify-before-promote contract."},
        {"name": "aria_manifest_status", "purpose": "Check MANIFEST.json presence and high-level fields."},
        {"name": "enable_pulse_mesh", "purpose": "Arm whole-system Pulse Mesh (mutation)."},
        {"name": "disconnect_governance", "purpose": "Disconnect mesh/process authority (mutation)."},
        {"name": "set_mode", "purpose": "Set quiet/balanced/performance mode (mutation)."},
        {"name": "list_processes", "purpose": "Discover top controllable processes."}
    ])
}

fn tools_openai_schema() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "get_system_status",
                "description": "Read live PulseFlow runtime status: CPU/RAM/GPU, stress, QoS, mesh, authority, coherence.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "rehydrate_cortex",
                "description": "Rehydrate the agent cortex from live metrics, regime arbiter, futurist foresight, and recent learning-dataset memory.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_sessions",
                "description": "List saved observation sessions with sample counts and targets.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_session_summary",
                "description": "Load a compact summary of one session by id.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"}
                    },
                    "required": ["session_id"],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_directive",
                "description": "Return the current resource-homeostasis agent directive.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_events_tail",
                "description": "Return the last N ledger events.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                    },
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "aria_overview",
                "description": "Read the ARIA verify-before-promote lattice and connection identity for this repo.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "aria_manifest_status",
                "description": "Check whether MANIFEST.json exists and return key top-level fields.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "enable_pulse_mesh",
                "description": "Enable whole-system Pulse Mesh: host pressure drives Eco on top user processes. No single PID required.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "disconnect_governance",
                "description": "Disconnect process/mesh authority; host observation remains live.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "set_mode",
                "description": "Set operating mode: quiet, balanced, or performance.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": {"type": "string", "enum": ["quiet", "balanced", "performance"]}
                    },
                    "required": ["mode"],
                    "additionalProperties": false
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_processes",
                "description": "List top controllable host processes by CPU/memory for interlink decisions.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                    },
                    "additionalProperties": false
                }
            }
        }
    ])
}

fn system_prompt(cortex: &Value) -> String {
    format!(
        r#"You are the PulseFlow Cortex Agent — an embedded operator copilot interlinked with this local Windows PulseFlow Governor.

## Identity
- You sit next to Condition · O-plane in the modulation HUD.
- You are wired into live host telemetry, authority lattice, Pulse Mesh, sessions, Futurist foresight, and ARIA verify-before-promote discipline.
- You do NOT control clocks, voltages, firmware, or fan curves. Governance is bounded process QoS (Eco / Normal / ThermalProtect) and observation.

## Cortex rehydration
On non-trivial questions, call `rehydrate_cortex` and/or `get_system_status` before answering so you speak from plant truth, not guesswork.
Cortex snapshot available at conversation start (may be slightly stale — refresh with tools):
{cortex}

## Memory
- Observation sessions are JSONL plant memory (X, A, R, Y).
- Learning datasets / graph blobs are compacted long-term memory after Learn All.
- Use list_sessions / get_session_summary when the operator asks about past runs.
- Oscillation coherence lives under O-plane: how consistently raw load follows filtered control.

## ARIA
- ARIA means verify-before-promote: discover → orient → verify contracts → compile/test → smoke → promote only on zero failures.
- Use aria_overview and aria_manifest_status when asked about promotion, install integrity, or lattice status.
- Never claim a promotion succeeded without tool evidence.

## Tools
- Prefer tools for status, sessions, ARIA, mesh enable, mode changes.
- Mutation tools (enable_pulse_mesh, disconnect_governance, set_mode) change the live plant — confirm intent briefly in prose after calling them.
- Be concise, operational, and honest about uncertainty.

## Style
- Short paragraphs or tight bullets.
- Name concrete metrics (RAM%, Eco duty, coherence, mesh targets) when relevant.
- Default provider preference is SpaceXAI/Grok when discussing model choice, but honor the operator's configured provider.
"#
    )
}

pub fn build_cortex_snapshot(state: &RuntimeState, config: &Config) -> Value {
    let m = &state.metrics;
    let c = &state.control;
    let t = &state.telemetry;
    let learning = storage::list_learning_datasets(&config.storage.directory)
        .unwrap_or_default()
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    let sessions = storage::list_sessions(&config.storage.directory)
        .unwrap_or_default()
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    json!({
        "timestamp_ms": now_ms(),
        "session_id": state.session_id,
        "authority_state": state.authority_state,
        "mesh_mode": state.mesh_mode,
        "mesh_targets": state.mesh_targets,
        "mesh_note": state.mesh_note,
        "governor_active": state.governor_active,
        "mode": state.mode,
        "target_label": state.target_label,
        "target_pid": state.target_pid,
        "machine": {
            "cpu_percent": t.cpu_percent,
            "ram_percent": t.memory_percent,
            "ram_used_gb": t.memory_used_gb,
            "ram_total_gb": t.memory_total_gb,
            "gpu_percent": t.gpu.as_ref().map(|g| g.utilization_percent),
            "gpu_temp_c": t.gpu.as_ref().map(|g| g.temperature_c),
        },
        "control": {
            "raw_stress": c.raw_stress,
            "filtered_stress": c.filtered_stress,
            "error": c.error,
            "residue": c.residue,
            "requested_qos": c.requested_qos,
            "applied_qos": c.applied_qos,
            "control_authority": c.control_authority,
            "applied_modulation": c.applied_modulation,
            "phase": c.phase,
        },
        "o_plane": {
            "oscillation_coherence": m.oscillation_coherence,
            "flow_stability": m.flow_stability,
            "turbulence_state": m.turbulence_state,
            "homeostatic_slack": m.homeostatic_slack,
            "ecosystem_pressure": m.ecosystem_pressure,
            "pressure_transduction": m.pressure_transduction,
            "prediction_rmse": m.prediction_rmse,
            "envelope_zone": m.envelope_zone,
            "condition_margin": m.condition_margin,
        },
        "regime": state.regime,
        "futurist": state.futurist,
        "directive": state.directive,
        "system_form_factor": m.system_form_factor,
        "recent_sessions": sessions,
        "learning_memory_heads": learning,
        "form_factor_profile": {
            "known_as": state.system_profile.known_as,
            "form_factor": state.system_profile.form_factor.as_str(),
            "eco_ram_enter_percent": state.system_profile.eco_ram_enter_percent,
        }
    })
}

fn execute_tool(
    name: &str,
    args: &Value,
    state: &std::sync::Arc<std::sync::RwLock<RuntimeState>>,
    config: &Config,
) -> Value {
    match name {
        "get_system_status" => match state.read() {
            Ok(locked) => json!({
                "ok": true,
                "status": {
                    "session_id": locked.session_id,
                    "authority_state": locked.authority_state,
                    "mesh_mode": locked.mesh_mode,
                    "mesh_targets": locked.mesh_targets,
                    "mesh_note": locked.mesh_note,
                    "governor_active": locked.governor_active,
                    "mode": locked.mode,
                    "target_label": locked.target_label,
                    "target_pid": locked.target_pid,
                    "cpu_percent": locked.telemetry.cpu_percent,
                    "ram_percent": locked.telemetry.memory_percent,
                    "filtered_stress": locked.control.filtered_stress,
                    "applied_qos": locked.control.applied_qos,
                    "coherence": locked.metrics.oscillation_coherence,
                    "stability": locked.metrics.flow_stability,
                    "turbulence": locked.metrics.turbulence_state,
                    "slack": locked.metrics.homeostatic_slack,
                    "eco_pressure": locked.metrics.ecosystem_pressure,
                    "zone": locked.metrics.envelope_zone,
                    "regime": locked.regime.regime_code,
                }
            }),
            Err(_) => json!({"ok": false, "error": "state lock poisoned"}),
        },
        "rehydrate_cortex" => match state.read() {
            Ok(locked) => json!({"ok": true, "cortex": build_cortex_snapshot(&locked, config)}),
            Err(_) => json!({"ok": false, "error": "state lock poisoned"}),
        },
        "list_sessions" => match storage::list_sessions(&config.storage.directory) {
            Ok(sessions) => json!({"ok": true, "sessions": sessions}),
            Err(e) => json!({"ok": false, "error": e}),
        },
        "get_session_summary" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if session_id.is_empty() {
                return json!({"ok": false, "error": "session_id required"});
            }
            match storage::read_session_frames(&config.storage.directory, session_id, 10_000) {
                Ok(frames) => {
                    let summary = crate::analytics::summarize_session(
                        session_id,
                        &frames,
                        config.analytics.epsilon,
                    );
                    json!({"ok": true, "samples": frames.len(), "summary": summary})
                }
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "get_directive" => match state.read() {
            Ok(locked) => json!({"ok": true, "directive": locked.directive}),
            Err(_) => json!({"ok": false, "error": "state lock poisoned"}),
        },
        "get_events_tail" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
            match storage::read_event_tail(&config.event_ledger_path, limit.clamp(1, 50)) {
                Ok(events) => json!({"ok": true, "events": events}),
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        "aria_overview" => json!({
            "ok": true,
            "aria": read_aria_bundle()
        }),
        "aria_manifest_status" => json!({
            "ok": true,
            "manifest": read_manifest_status()
        }),
        "enable_pulse_mesh" => match state.write() {
            Ok(mut locked) => {
                if !locked.governor_supported {
                    return json!({"ok": false, "error": "Pulse Mesh requires Windows process QoS"});
                }
                locked.mesh_mode = true;
                locked.mesh_note = "Pulse Mesh armed by Cortex agent.".into();
                locked.mesh_targets = 0;
                locked.target_pid = None;
                locked.target_label = "pulse-mesh".into();
                locked.target_identity = None;
                locked.governor_active = true;
                locked.authority_state = crate::authority::AuthorityState::Active;
                locked.last_valid_authority_state = crate::authority::AuthorityState::Active;
                locked.failed_invariant = None;
                locked.recording = true;
                locked.target_revision = locked.target_revision.saturating_add(1);
                let _ = locked.begin_new_session();
                let _ = locked.push_event(
                    "interlink",
                    "Pulse Mesh enabled by Cortex agent (whole-system Eco).",
                );
                json!({
                    "ok": true,
                    "mesh_mode": true,
                    "session_id": locked.session_id,
                    "message": "Pulse Mesh armed. Host pressure will drive Eco on top processes."
                })
            }
            Err(_) => json!({"ok": false, "error": "state lock poisoned"}),
        },
        "disconnect_governance" => match state.write() {
            Ok(mut locked) => {
                locked.mesh_mode = false;
                locked.mesh_note = String::new();
                locked.mesh_targets = 0;
                locked.target_pid = None;
                locked.target_label = "system-monitor".into();
                locked.target_identity = None;
                locked.target_revision = locked.target_revision.saturating_add(1);
                locked.governor_active = false;
                locked.authority_state = crate::authority::AuthorityState::Observation;
                locked.last_valid_authority_state = crate::authority::AuthorityState::Observation;
                locked.verification_receipt = None;
                locked.failed_invariant = None;
                let _ = locked.push_event(
                    "interlink",
                    "Governance disconnected by Cortex agent; observation remains live.",
                );
                json!({"ok": true, "message": "Disconnected. Host telemetry continues."})
            }
            Err(_) => json!({"ok": false, "error": "state lock poisoned"}),
        },
        "set_mode" => {
            let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("");
            let parsed = match mode {
                "quiet" => Some(crate::model::OperatingMode::Quiet),
                "balanced" => Some(crate::model::OperatingMode::Balanced),
                "performance" => Some(crate::model::OperatingMode::Performance),
                _ => None,
            };
            match (parsed, state.write()) {
                (Some(mode), Ok(mut locked)) => {
                    locked.mode = mode;
                    let _ = locked.push_event("mode", format!("Mode set to {mode:?} by Cortex agent."));
                    json!({"ok": true, "mode": mode})
                }
                (None, _) => json!({"ok": false, "error": "mode must be quiet|balanced|performance"}),
                (_, Err(_)) => json!({"ok": false, "error": "state lock poisoned"}),
            }
        }
        "list_processes" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
            json!({"ok": true, "processes": list_top_processes(limit.clamp(1, 20))})
        }
        other => json!({"ok": false, "error": format!("unknown tool: {other}")}),
    }
}

fn read_aria_bundle() -> Value {
    let aria = read_text_capped(Path::new("aria/pulseflow.aria"), 8_000);
    let connect = read_json_capped(Path::new("aria/ARIA-CONNECT.json"), 8_000);
    let readme = read_text_capped(Path::new("aria/README.md"), 4_000);
    json!({
        "lattice": aria,
        "connection": connect,
        "readme_excerpt": readme,
        "discipline": "verify-before-promote: compile, test, smoke, receipt, then promote only on zero failures"
    })
}

fn read_manifest_status() -> Value {
    let path = Path::new("MANIFEST.json");
    if !path.exists() {
        return json!({"present": false});
    }
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) => json!({
                "present": true,
                "keys": v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()),
                "version": v.get("version"),
                "package": v.get("package"),
                "generated_at": v.get("generated_at").or_else(|| v.get("created_at")),
            }),
            Err(e) => json!({"present": true, "parse_error": e.to_string()}),
        },
        Err(e) => json!({"present": false, "error": e.to_string()}),
    }
}

fn read_text_capped(path: &Path, max: usize) -> Value {
    match fs::read_to_string(path) {
        Ok(text) => {
            if text.len() <= max {
                json!(text)
            } else {
                json!(format!("{}…", &text[..max]))
            }
        }
        Err(e) => json!(format!("(unavailable: {e})")),
    }
}

fn read_json_capped(path: &Path, max: usize) -> Value {
    match fs::read_to_string(path) {
        Ok(text) => {
            let slice = if text.len() > max {
                &text[..max]
            } else {
                &text
            };
            serde_json::from_str(slice).unwrap_or_else(|_| json!({"raw": slice}))
        }
        Err(e) => json!({"error": e.to_string()}),
    }
}

fn list_top_processes(limit: usize) -> Value {
    use sysinfo::{PidExt, ProcessExt, System, SystemExt};
    let mut system = System::new();
    system.refresh_processes();
    let self_pid = std::process::id();
    let mut rows: Vec<Value> = system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let id = pid.as_u32();
            if id == 0 || id == self_pid {
                return None;
            }
            let name = process.name().to_string();
            let cpu = process.cpu_usage() as f64;
            let mem_mb = process.memory() as f64 / 1024.0 / 1024.0;
            if cpu < 0.5 && mem_mb < 80.0 {
                return None;
            }
            Some(json!({
                "pid": id,
                "name": name,
                "cpu_percent": cpu,
                "memory_mb": mem_mb,
            }))
        })
        .collect();
    rows.sort_by(|a, b| {
        let sa = a["memory_mb"].as_f64().unwrap_or(0.0) * 0.6
            + a["cpu_percent"].as_f64().unwrap_or(0.0) * 8.0;
        let sb = b["memory_mb"].as_f64().unwrap_or(0.0) * 0.6
            + b["cpu_percent"].as_f64().unwrap_or(0.0) * 8.0;
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);
    json!(rows)
}

/// Run one operator chat turn with tool loop.
pub fn chat(
    req: ChatRequest,
    session: &AgentSession,
    state: &std::sync::Arc<std::sync::RwLock<RuntimeState>>,
    config: &Config,
) -> Result<Value, String> {
    let message = req.message.trim();
    if message.is_empty() {
        return Err("message is empty".into());
    }
    let secrets = load_secrets();
    let provider = req
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(if secrets.active_provider.is_empty() {
            "spacexai"
        } else {
            secrets.active_provider.as_str()
        })
        .to_string();
    if provider_spec(&provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }
    let model = resolve_model(&provider, &secrets, req.model.as_deref());
    let api_key = resolve_key(&provider, &secrets).ok_or_else(|| {
        format!(
            "No API key for provider '{provider}'. Save a key in agent settings or set the env var."
        )
    })?;
    let base_url = resolve_base_url(&provider, &secrets);

    let cortex = state
        .read()
        .map(|locked| build_cortex_snapshot(&locked, config))
        .unwrap_or_else(|_| json!({"error": "state unavailable"}));

    // Seed history: prefer server session, else client-provided
    let mut messages: Vec<Value> = vec![json!({
        "role": "system",
        "content": system_prompt(&cortex)
    })];

    let prior = {
        let snap = session.snapshot();
        if snap.is_empty() {
            req.history.clone()
        } else {
            snap
        }
    };
    for msg in prior {
        if msg.role == "system" {
            continue;
        }
        let mut obj = json!({
            "role": msg.role,
            "content": msg.content,
        });
        if let Some(tc) = msg.tool_calls {
            obj["tool_calls"] = tc;
        }
        if let Some(id) = msg.tool_call_id {
            obj["tool_call_id"] = json!(id);
        }
        if let Some(name) = msg.name {
            obj["name"] = json!(name);
        }
        messages.push(obj);
    }

    let user_msg = ChatMessage {
        role: "user".into(),
        content: message.to_string(),
        name: None,
        tool_call_id: None,
        tool_calls: None,
    };
    session.push(user_msg.clone());
    messages.push(json!({"role": "user", "content": message}));

    let mut tool_trace: Vec<Value> = Vec::new();
    let mut final_text = String::new();
    let mut rounds = 0usize;

    loop {
        rounds += 1;
        if rounds > MAX_TOOL_ROUNDS {
            if final_text.is_empty() {
                final_text = "Tool loop limit reached. Partial context is available in the tool trace.".into();
            }
            break;
        }

        let body = json!({
            "model": model,
            "messages": messages,
            "tools": tools_openai_schema(),
            "tool_choice": "auto",
            "temperature": 0.4,
        });

        let response = openai_chat_completions(&base_url, &api_key, &body)?;
        let choice = response
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| format!("provider response missing choices: {response}"))?;

        let content = choice
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_calls = choice.get("tool_calls").cloned().unwrap_or(Value::Null);

        messages.push(choice.clone());

        if tool_calls.is_null()
            || tool_calls.as_array().map(|a| a.is_empty()).unwrap_or(true)
        {
            final_text = content;
            session.push(ChatMessage {
                role: "assistant".into(),
                content: final_text.clone(),
                name: None,
                tool_call_id: None,
                tool_calls: None,
            });
            break;
        }

        if !content.is_empty() {
            // Intermediate assistant narration with tool calls
            session.push(ChatMessage {
                role: "assistant".into(),
                content: content.clone(),
                name: None,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
            });
        } else {
            session.push(ChatMessage {
                role: "assistant".into(),
                content: String::new(),
                name: None,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
            });
        }

        let calls = tool_calls.as_array().cloned().unwrap_or_default();
        for call in calls {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("tool_call")
                .to_string();
            let name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arg_str = call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let args: Value = serde_json::from_str(arg_str).unwrap_or_else(|_| json!({}));
            let result = execute_tool(&name, &args, state, config);
            tool_trace.push(json!({
                "id": id,
                "name": name,
                "arguments": args,
                "result": result,
            }));
            let result_text = serde_json::to_string(&tool_trace.last().unwrap()["result"])
                .unwrap_or_else(|_| "{}".into());
            let tool_msg = json!({
                "role": "tool",
                "tool_call_id": id,
                "content": result_text,
            });
            messages.push(tool_msg);
            session.push(ChatMessage {
                role: "tool".into(),
                content: result_text,
                name: Some(name),
                tool_call_id: Some(
                    call.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool_call")
                        .to_string(),
                ),
                tool_calls: None,
            });
        }
    }

    Ok(json!({
        "schema_version": "pulseflow.agent-chat.v1",
        "provider": provider,
        "model": model,
        "reply": final_text,
        "tool_trace": tool_trace,
        "rounds": rounds,
        "timestamp_ms": now_ms(),
    }))
}

/// OpenAI-compatible chat completions via PowerShell (Windows-native TLS, no extra crates).
fn openai_chat_completions(base_url: &str, api_key: &str, body: &Value) -> Result<Value, String> {
    let url = format!(
        "{}/chat/completions",
        base_url.trim_end_matches('/')
    );
    let body_text = serde_json::to_string(body).map_err(|e| e.to_string())?;
    // Write body to temp file to avoid command-line length limits
    let tmp = std::env::temp_dir().join(format!("pulseflow-agent-{}.json", now_ms()));
    fs::write(&tmp, &body_text).map_err(|e| format!("temp body write: {e}"))?;
    let tmp_out = std::env::temp_dir().join(format!("pulseflow-agent-out-{}.json", now_ms()));

    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
try {{
  $headers = @{{
    'Authorization' = 'Bearer {key}'
    'Content-Type' = 'application/json'
    'HTTP-Referer' = 'http://127.0.0.1:8791'
    'X-Title' = 'PulseFlow Cortex Agent'
  }}
  $body = Get-Content -Raw -LiteralPath '{body_path}'
  $resp = Invoke-RestMethod -Method Post -Uri '{url}' -Headers $headers -Body $body -TimeoutSec 120
  $resp | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath '{out_path}' -Encoding utf8
  exit 0
}} catch {{
  $_.Exception.Message | Set-Content -LiteralPath '{out_path}' -Encoding utf8
  exit 1
}}
"#,
        key = escape_ps_single(api_key),
        body_path = escape_ps_single(&tmp.to_string_lossy()),
        url = escape_ps_single(&url),
        out_path = escape_ps_single(&tmp_out.to_string_lossy()),
    );

    let result = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output();

    let _ = fs::remove_file(&tmp);
    let output = result.map_err(|e| format!("powershell spawn failed: {e}"))?;
    let out_text = fs::read_to_string(&tmp_out).unwrap_or_default();
    let _ = fs::remove_file(&tmp_out);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "provider HTTP failed: {} {}",
            out_text.trim(),
            stderr.trim()
        ));
    }
    if out_text.trim().is_empty() {
        return Err("provider returned empty body".into());
    }
    serde_json::from_str(&out_text).map_err(|e| {
        format!(
            "provider JSON parse error: {e}; body starts: {}",
            out_text.chars().take(200).collect::<String>()
        )
    })
}

fn escape_ps_single(s: &str) -> String {
    s.replace('\'', "''")
}

#[allow(dead_code)]
pub fn http_timeout() -> Duration {
    Duration::from_secs(120)
}
