use serde_json::Value;
use std::collections::BTreeSet;

const HTML: &str = include_str!("../web/index.html");
const SERVER: &str = include_str!("../src/server.rs");
const UI_CONTRACT: &str = include_str!("../schemas/ui-actions.json");
const OBSERVATION_SCHEMA: &str = include_str!("../schemas/observation-frame.schema.json");
const DIRECTIVE_SCHEMA: &str = include_str!("../schemas/agent-directive.schema.json");
const ADAPTIVE_SCHEMA: &str = include_str!("../schemas/adaptive-suggestion.schema.json");
const API_CONTRACT: &str = include_str!("../schemas/pulseflow-api.contract.json");

fn attribute_values(document: &str, attribute: &str) -> BTreeSet<String> {
    let needle = format!("{attribute}=\"");
    let mut values = BTreeSet::new();
    let mut rest = document;
    while let Some(start) = rest.find(&needle) {
        rest = &rest[start + needle.len()..];
        let Some(end) = rest.find('"') else { break };
        values.insert(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    values
}

#[test]
fn every_declared_button_has_a_handler_and_contract_entry() {
    let contract: Value = serde_json::from_str(UI_CONTRACT).expect("valid UI contract JSON");
    let actions = contract["actions"].as_array().expect("actions array");
    let contract_ids: BTreeSet<String> = actions
        .iter()
        .map(|action| action["id"].as_str().expect("action id").to_string())
        .collect();
    let html_ids = attribute_values(HTML, "data-action");
    assert_eq!(
        html_ids, contract_ids,
        "HTML actions and contract actions diverged"
    );

    for action in actions {
        let id = action["id"].as_str().expect("action id");
        assert!(
            HTML.contains(&format!("\"{id}\":")),
            "ACTION_HANDLERS is missing {id}"
        );
        if action["kind"].as_str() == Some("http") {
            let route = action["route"].as_str().expect("HTTP route");
            assert!(
                SERVER.contains(route),
                "server route marker missing for {id}: {route}"
            );
        }
    }
}

#[test]
fn every_tab_has_exactly_one_view() {
    let tabs = attribute_values(HTML, "data-tab");
    let views = attribute_values(HTML, "data-view");
    assert_eq!(tabs, views);
}

#[test]
fn every_live_control_exists_in_the_document() {
    let contract: Value = serde_json::from_str(UI_CONTRACT).expect("valid UI contract JSON");
    for control in contract["live_controls"]
        .as_array()
        .expect("controls array")
    {
        let id = control.as_str().expect("control id");
        assert!(
            HTML.contains(&format!("id=\"{id}\"")),
            "missing control {id}"
        );
    }
}

#[test]
fn machine_readable_contracts_are_valid_json() {
    for document in [
        OBSERVATION_SCHEMA,
        DIRECTIVE_SCHEMA,
        ADAPTIVE_SCHEMA,
        API_CONTRACT,
    ] {
        let value: Value = serde_json::from_str(document).expect("valid contract JSON");
        assert!(value.is_object());
    }
}

#[test]
fn causal_outcome_alignment_is_visible_in_code_and_schema() {
    let model = include_str!("../src/model.rs");
    assert!(model.contains("pending_next_interval"));
    assert!(model.contains("next_interval"));
    assert!(OBSERVATION_SCHEMA.contains("next_interval"));
}

#[test]
fn system_interlink_attestation_is_truth_derived() {
    for marker in [
        "header-system-link",
        "interlink-panel",
        "deriveSystemLink",
        "governor_supported",
        "governor_active",
        "applied_qos",
        "latest_frame",
        "VERIFIED ACTIVE",
        "OBSERVATION LINK",
        "LINK FRACTURE",
        "coherence-scope",
        "computeOscillationDamping",
        "process-select",
        "interlink-connect",
        "interlink-baseline",
        "interlink-enable",
        "interlink-disconnect",
        "/api/interlink/verify",
        "/api/processes",
        "pulseflow.interlink.v1",
        "core-reactor",
    ] {
        assert!(
            HTML.contains(marker),
            "system interlink marker missing: {marker}"
        );
    }
}

#[test]
fn application_icon_is_installation_and_web_bound() {
    for marker in [
        "/favicon.ico",
        "/site.webmanifest",
        "pulseflow-governor-64.png",
        "pulseflow-governor-192.png",
        "pulseflow-governor-512.png",
    ] {
        assert!(HTML.contains(marker), "dashboard is missing {marker}");
        assert!(SERVER.contains(marker), "server is missing route {marker}");
    }
    let installer = include_str!("../scripts/Install-PulseFlow.ps1");
    for marker in [
        "pulseflow-governor.ico",
        "IconLocation",
        "PulseFlow Governor.lnk",
        "Launch-PulseFlow.ps1",
        "Test-PulseFlowReady",
        "installation.json",
    ] {
        assert!(installer.contains(marker), "installer is missing {marker}");
    }
    let build_script = include_str!("../build.rs");
    assert!(build_script.contains("set_icon(\"assets/icons/pulseflow-governor.ico\")"));
}

#[test]
fn dynamic_interlink_is_runtime_rebindable() {
    let model = include_str!("../src/model.rs");
    let main = include_str!("../src/main.rs");
    assert!(model.contains("target_revision"));
    assert!(main.contains("observed_target_revision"));
    assert!(main.contains("TelemetryCollector::new(target_pid)"));
    assert!(main.contains("ProcessGovernor::new(target_pid"));
}
