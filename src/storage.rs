use crate::model::{
    now_ms, safe_session_id, stable_hash, ObservationFrame, RuntimeEvent, SessionInfo,
    SessionSummary,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub struct FrameRecorder {
    directory: PathBuf,
    session: SessionInfo,
    file: File,
    metadata_flush_every_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCompactionReceipt {
    pub schema_version: String,
    pub session_id: String,
    pub compacted_at_ms: u128,
    pub raw_checksum: String,
    pub freed_bytes: u64,
    pub raw_deleted: bool,
    pub summary: SessionSummary,
}

impl FrameRecorder {
    pub fn open(
        directory: impl AsRef<Path>,
        session_id: &str,
        target_label: &str,
        metadata_flush_every_samples: u64,
    ) -> Result<Self, String> {
        let session_id = safe_session_id(session_id).ok_or("unsafe session id")?;
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = directory.join(format!("{session_id}.jsonl"));
        let existing_bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        let session = SessionInfo {
            session_id,
            path: path.to_string_lossy().into_owned(),
            samples: 0,
            bytes: existing_bytes,
            started_at_ms: now_ms(),
            last_sample_at_ms: 0,
            target_label: target_label.to_string(),
            schema_version: "pulseflow.observation.v3".into(),
        };
        let mut recorder = Self {
            directory,
            session,
            file,
            metadata_flush_every_samples: metadata_flush_every_samples.max(1),
        };
        recorder.write_metadata()?;
        Ok(recorder)
    }

    pub fn session(&self) -> &SessionInfo {
        &self.session
    }

    pub fn append(&mut self, frame: &ObservationFrame) -> Result<u64, String> {
        let line = serde_json::to_string(frame).map_err(|error| error.to_string())?;
        writeln!(self.file, "{line}").map_err(|error| error.to_string())?;
        self.file.flush().map_err(|error| error.to_string())?;
        let bytes = (line.as_bytes().len() + 1) as u64;
        self.session.samples = self.session.samples.saturating_add(1);
        self.session.bytes = self.session.bytes.saturating_add(bytes);
        self.session.last_sample_at_ms = frame.timestamp_ms;
        if self.session.samples % self.metadata_flush_every_samples == 0 {
            self.write_metadata()?;
        }
        Ok(bytes)
    }

    pub fn finalize(&mut self) -> Result<(), String> {
        self.file.flush().map_err(|error| error.to_string())?;
        self.write_metadata()
    }

    fn metadata_path(&self) -> PathBuf {
        self.directory
            .join(format!("{}.meta.json", self.session.session_id))
    }

    fn write_metadata(&mut self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.session).map_err(|error| error.to_string())?;
        fs::write(self.metadata_path(), bytes).map_err(|error| error.to_string())
    }
}

pub fn append_event(path: impl AsRef<Path>, event: &RuntimeEvent) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let line = serde_json::to_string(event).map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| error.to_string())
}

pub fn list_sessions(directory: impl AsRef<Path>) -> Result<Vec<SessionInfo>, String> {
    let directory = directory.as_ref();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json")
            || !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".meta.json"))
        {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        if let Ok(session) = serde_json::from_str::<SessionInfo>(&text) {
            sessions.push(session);
        }
    }
    sessions.sort_by(|left, right| right.last_sample_at_ms.cmp(&left.last_sample_at_ms));
    Ok(sessions)
}

pub fn read_session_frames(
    directory: impl AsRef<Path>,
    session_id: &str,
    limit: usize,
) -> Result<Vec<ObservationFrame>, String> {
    let safe = safe_session_id(session_id).ok_or("unsafe session id")?;
    let path = directory.as_ref().join(format!("{safe}.jsonl"));
    let file =
        File::open(&path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut frames = VecDeque::with_capacity(limit.max(1).min(4_096));
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let frame: ObservationFrame = serde_json::from_str(&line).map_err(|error| {
            format!(
                "invalid observation frame in {} at line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        let frame = validate_and_migrate_frame(frame).map_err(|error| {
            format!(
                "rejected observation frame in {} at line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        frames.push_back(frame);
        while frames.len() > limit.max(1) {
            frames.pop_front();
        }
    }
    Ok(frames.into_iter().collect())
}

pub fn validate_and_migrate_frame(mut frame: ObservationFrame) -> Result<ObservationFrame, String> {
    match frame.schema_version.as_str() {
        "pulseflow.observation.v1" => {
            // V1 used `modulation` as ambiguous controller capacity. It cannot
            // prove applied effort, so migration deliberately records zero.
            frame.controller.control_authority = 0.0;
            frame.controller.capacity_signal = 0.0;
            frame.controller.controller_effort = 0.0;
            frame.controller.applied_modulation = 0.0;
            frame.controller.modulation = 0.0;
            frame.action.modulation_authority = 0.0;
            frame.action.applied_modulation = 0.0;
            frame.schema_version = "pulseflow.observation.v3".into();
        }
        "pulseflow.observation.v2" => {
            frame.controller.capacity_signal = 0.0;
            frame.controller.controller_effort = frame.controller.applied_modulation;
            frame.schema_version = "pulseflow.observation.v3".into();
        }
        "pulseflow.observation.v3" => {}
        other => return Err(format!("unsupported schema_version {other}")),
    }
    if safe_session_id(&frame.session_id).is_none() {
        return Err("unsafe session_id".into());
    }
    if frame.sequence == 0 {
        return Err("sequence must be at least 1".into());
    }
    let bounded = [
        ("raw_stress", frame.controller.raw_stress),
        ("filtered_stress", frame.controller.filtered_stress),
        ("capacity_signal", frame.controller.capacity_signal),
        ("control_authority", frame.controller.control_authority),
        ("controller_effort", frame.controller.controller_effort),
        ("applied_modulation", frame.controller.applied_modulation),
    ];
    for (name, value) in bounded {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(format!("{name} must be finite and within [0,1]"));
        }
    }
    if !frame.machine.cpu_percent.is_finite()
        || !(0.0..=100.0).contains(&frame.machine.cpu_percent)
        || !frame.machine.ram_percent.is_finite()
        || !(0.0..=100.0).contains(&frame.machine.ram_percent)
    {
        return Err("host percentages must be finite and within [0,100]".into());
    }
    Ok(frame)
}

pub fn read_session_bytes(
    directory: impl AsRef<Path>,
    session_id: &str,
) -> Result<Vec<u8>, String> {
    let safe = safe_session_id(session_id).ok_or("unsafe session id")?;
    let path = directory.as_ref().join(format!("{safe}.jsonl"));
    fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

pub fn compact_session(
    directory: impl AsRef<Path>,
    session_id: &str,
    summary: SessionSummary,
) -> Result<SessionCompactionReceipt, String> {
    let safe = safe_session_id(session_id).ok_or("unsafe session id")?;
    let directory = directory.as_ref();
    let raw_path = directory.join(format!("{safe}.jsonl"));
    let metadata_path = directory.join(format!("{safe}.meta.json"));
    let raw = fs::read(&raw_path)
        .map_err(|error| format!("cannot read {}: {error}", raw_path.display()))?;
    let metadata_bytes = fs::metadata(&metadata_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let receipt = SessionCompactionReceipt {
        schema_version: "pulseflow.compaction.v1".into(),
        session_id: safe.clone(),
        compacted_at_ms: now_ms(),
        raw_checksum: stable_hash(&raw),
        freed_bytes: raw.len() as u64 + metadata_bytes,
        raw_deleted: true,
        summary,
    };
    let receipt_directory = directory.join("analysis-receipts");
    fs::create_dir_all(&receipt_directory).map_err(|error| error.to_string())?;
    let receipt_path = receipt_directory.join(format!("{safe}.analysis.json"));
    let receipt_bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
    fs::write(&receipt_path, receipt_bytes)
        .map_err(|error| format!("cannot write {}: {error}", receipt_path.display()))?;
    fs::remove_file(&raw_path)
        .map_err(|error| format!("cannot delete {}: {error}", raw_path.display()))?;
    if metadata_path.exists() {
        fs::remove_file(&metadata_path)
            .map_err(|error| format!("cannot delete {}: {error}", metadata_path.display()))?;
    }
    Ok(receipt)
}

pub fn read_event_tail(path: impl AsRef<Path>, limit: usize) -> Result<Vec<RuntimeEvent>, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut events = VecDeque::with_capacity(limit.max(1));
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| error.to_string())?;
        if let Ok(event) = serde_json::from_str::<RuntimeEvent>(&line) {
            events.push_back(event);
            while events.len() > limit.max(1) {
                events.pop_front();
            }
        }
    }
    Ok(events.into_iter().collect())
}
