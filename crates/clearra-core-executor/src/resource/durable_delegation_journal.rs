//! Crash-recoverable authority for producer-to-worker delegation.
//! SRP rationale: this module has one change reason: crash-recoverable delegation authority and its state transitions.
//!
//! This is deliberately separate from the in-process resource lease. A resource
//! lease answers whether work may run on this process; this journal answers
//! whether an executable payload may cross a producer/worker boundary. The
//! payload itself is never journaled. Only stable identity, digests, budget,
//! fencing token, and the state transition are persisted.

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const OFFER_ACCEPT_TIMEOUT_MS: u64 = 10_000;
pub const WORKER_HEARTBEAT_INTERVAL_MS: u64 = 5_000;
pub const PERSISTED_RENEWAL_INTERVAL_MS: u64 = 30_000;
pub const ACTIVE_LEASE_EXPIRY_MS: u64 = 120_000;
pub const TERMINAL_TOMBSTONE_RETENTION_MS: u64 = 24 * 60 * 60 * 1_000;

const SCHEMA: &str = "clearra.delegation-journal.v1";
const HEADER_SCHEMA: &str = "clearra.delegation-journal-header.v1";
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DelegationBudget {
    pub compute_units: u32,
    pub memory_bytes: u64,
}

impl DelegationBudget {
    pub const fn new(compute_units: u32, memory_bytes: u64) -> Option<Self> {
        if compute_units == 0 {
            None
        } else {
            Some(Self {
                compute_units,
                memory_bytes,
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationIdentity {
    pub job_id: String,
    pub task_id: String,
    pub coordinator_id: String,
    pub payload_sha256: String,
    pub request_sha256: String,
}

impl DelegationIdentity {
    pub fn new(
        job_id: impl Into<String>,
        task_id: impl Into<String>,
        coordinator_id: impl Into<String>,
        payload_sha256: impl Into<String>,
        request_sha256: impl Into<String>,
    ) -> Result<Self, DurableDelegationError> {
        let value = Self {
            job_id: job_id.into(),
            task_id: task_id.into(),
            coordinator_id: coordinator_id.into(),
            payload_sha256: payload_sha256.into(),
            request_sha256: request_sha256.into(),
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), DurableDelegationError> {
        if self.job_id.is_empty() || self.task_id.is_empty() || self.coordinator_id.is_empty() {
            return Err(DurableDelegationError::InvalidIdentity);
        }
        if !is_sha256(&self.payload_sha256) || !is_sha256(&self.request_sha256) {
            return Err(DurableDelegationError::InvalidIdentity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DelegationPhase {
    Prepared,
    Offered,
    Accepted,
    Published,
    Running,
    Renewed,
    ResultSealed,
    ResultApplied,
    Completed,
    Revoked,
    Expired,
    Cancelled,
    FailedClosed,
}

impl DelegationPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Offered => "offered",
            Self::Accepted => "accepted",
            Self::Published => "published",
            Self::Running => "running",
            Self::Renewed => "renewed",
            Self::ResultSealed => "result-sealed",
            Self::ResultApplied => "result-applied",
            Self::Completed => "completed",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
            Self::FailedClosed => "failed-closed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "offered" => Self::Offered,
            "accepted" => Self::Accepted,
            "published" => Self::Published,
            "running" => Self::Running,
            "renewed" => Self::Renewed,
            "result-sealed" => Self::ResultSealed,
            "result-applied" => Self::ResultApplied,
            "completed" => Self::Completed,
            "revoked" => Self::Revoked,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            "failed-closed" => Self::FailedClosed,
            _ => return None,
        })
    }

    const fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Revoked | Self::Expired | Self::Cancelled | Self::FailedClosed
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationEvent {
    pub sequence: u64,
    pub identity: DelegationIdentity,
    pub budget: DelegationBudget,
    pub phase: DelegationPhase,
    pub fencing_token: u64,
    pub worker_id: Option<String>,
    pub reservation_sha256: Option<String>,
    pub result_sha256: Option<String>,
    pub worker_reply_sha256: Option<String>,
    pub timestamp_unix_ms: u64,
    pub reason: Option<String>,
    pub previous_event_sha256: String,
    pub event_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct DelegationEventDraft {
    identity: DelegationIdentity,
    budget: DelegationBudget,
    phase: DelegationPhase,
    fencing_token: u64,
    worker_id: Option<String>,
    reservation_sha256: Option<String>,
    result_sha256: Option<String>,
    worker_reply_sha256: Option<String>,
    timestamp_unix_ms: u64,
    reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationToken {
    pub job_id: String,
    pub task_id: String,
    pub fencing_token: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableDelegationPermit {
    pub job_id: String,
    pub task_id: String,
    pub worker_id: String,
    pub payload_sha256: String,
    pub request_sha256: String,
    pub fencing_token: u64,
    pub publication_sequence: u64,
    pub publication_sha256: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultApplicationDecision {
    ApplyOnce,
    AlreadyApplied,
}

#[derive(Debug)]
pub enum DelegationJournalError {
    Io(std::io::Error),
    Corrupt {
        line: usize,
        reason: String,
    },
    Quarantined {
        path: PathBuf,
        reason: String,
    },
    WriterAlreadyActive {
        path: PathBuf,
    },
    HeadChanged {
        expected: String,
        actual: Option<String>,
    },
    InjectedFailure,
}

impl fmt::Display for DelegationJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "delegation journal I/O failed: {error}"),
            Self::Corrupt { line, reason } => {
                write!(
                    formatter,
                    "delegation journal line {line} is corrupt: {reason}"
                )
            }
            Self::Quarantined { path, reason } => write!(
                formatter,
                "delegation journal is quarantined at {}: {reason}",
                path.display()
            ),
            Self::WriterAlreadyActive { path } => write!(
                formatter,
                "delegation journal already has an active writer: {}",
                path.display()
            ),
            Self::HeadChanged { expected, actual } => write!(
                formatter,
                "delegation journal head changed: expected {expected}, found {}",
                actual.as_deref().unwrap_or("<empty>")
            ),
            Self::InjectedFailure => formatter.write_str("delegation journal injected failure"),
        }
    }
}

impl Error for DelegationJournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Corrupt { .. }
            | Self::Quarantined { .. }
            | Self::WriterAlreadyActive { .. }
            | Self::HeadChanged { .. }
            | Self::InjectedFailure => None,
        }
    }
}

impl From<std::io::Error> for DelegationJournalError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub enum DurableDelegationError {
    Journal(DelegationJournalError),
    InvalidIdentity,
    UnknownDelegation,
    StaleFence,
    InvalidTransition {
        from: DelegationPhase,
        to: DelegationPhase,
    },
    OfferExpired,
    LeaseExpired,
    IdentityAlreadyUsed,
    ResultDigestMismatch,
    SequenceExhausted,
    FencingTokenExhausted,
}

impl fmt::Display for DurableDelegationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Journal(error) => error.fmt(formatter),
            Self::InvalidIdentity => formatter.write_str("invalid delegation identity"),
            Self::UnknownDelegation => formatter.write_str("unknown delegation"),
            Self::StaleFence => formatter.write_str("stale delegation fencing token"),
            Self::InvalidTransition { from, to } => write!(
                formatter,
                "invalid delegation transition {} -> {}",
                from.as_str(),
                to.as_str()
            ),
            Self::OfferExpired => formatter.write_str("delegation offer expired"),
            Self::LeaseExpired => formatter.write_str("delegation lease expired"),
            Self::IdentityAlreadyUsed => formatter.write_str("delegation identity already used"),
            Self::ResultDigestMismatch => {
                formatter.write_str("delegation result digest does not match the sealed result")
            }
            Self::SequenceExhausted => formatter.write_str("delegation sequence exhausted"),
            Self::FencingTokenExhausted => {
                formatter.write_str("delegation fencing token exhausted")
            }
        }
    }
}

impl Error for DurableDelegationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Journal(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DelegationJournalError> for DurableDelegationError {
    fn from(value: DelegationJournalError) -> Self {
        Self::Journal(value)
    }
}

pub trait DelegationJournal {
    fn records(&self) -> &[DelegationEvent];
    fn quarantine_reason(&self) -> Option<&str>;
    fn quarantine(&mut self, reason: &str) -> Result<(), DelegationJournalError>;
    fn append(
        &mut self,
        draft: DelegationEventDraft,
    ) -> Result<DelegationEvent, DelegationJournalError>;
    fn reset_if_head(&mut self, expected_event_sha256: &str) -> Result<(), DelegationJournalError>;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryDelegationJournal {
    records: Vec<DelegationEvent>,
    fail_next_append: bool,
    fail_next_reset: bool,
    quarantine_reason: Option<String>,
}

impl MemoryDelegationJournal {
    pub fn fail_next_append(&mut self) {
        self.fail_next_append = true;
    }

    pub fn fail_next_reset(&mut self) {
        self.fail_next_reset = true;
    }
}

impl DelegationJournal for MemoryDelegationJournal {
    fn records(&self) -> &[DelegationEvent] {
        &self.records
    }

    fn quarantine_reason(&self) -> Option<&str> {
        self.quarantine_reason.as_deref()
    }

    fn quarantine(&mut self, reason: &str) -> Result<(), DelegationJournalError> {
        self.quarantine_reason = Some(reason.to_owned());
        Ok(())
    }

    fn append(
        &mut self,
        draft: DelegationEventDraft,
    ) -> Result<DelegationEvent, DelegationJournalError> {
        if let Some(reason) = &self.quarantine_reason {
            return Err(DelegationJournalError::Corrupt {
                line: 0,
                reason: format!("journal is quarantined: {reason}"),
            });
        }
        if self.fail_next_append {
            self.fail_next_append = false;
            return Err(DelegationJournalError::InjectedFailure);
        }
        let event = build_event(&self.records, draft)?;
        self.records.push(event.clone());
        Ok(event)
    }

    fn reset_if_head(&mut self, expected_event_sha256: &str) -> Result<(), DelegationJournalError> {
        if let Some(reason) = &self.quarantine_reason {
            return Err(DelegationJournalError::Corrupt {
                line: 0,
                reason: format!("journal is quarantined: {reason}"),
            });
        }
        if self.fail_next_reset {
            self.fail_next_reset = false;
            return Err(DelegationJournalError::InjectedFailure);
        }
        ensure_expected_head(&self.records, expected_event_sha256)?;
        self.records.clear();
        Ok(())
    }
}

#[derive(Debug)]
pub struct NativeJsonlDelegationJournal {
    path: PathBuf,
    file: Option<File>,
    records: Vec<DelegationEvent>,
    quarantine_reason: Option<String>,
    job_id: Option<String>,
    header_len: u64,
}

impl NativeJsonlDelegationJournal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DelegationJournalError> {
        Self::open_bound(path.as_ref().to_path_buf(), None)
    }

    fn open_bound(
        path: PathBuf,
        expected_job_id: Option<&str>,
    ) -> Result<Self, DelegationJournalError> {
        let marker = quarantine_marker_path(&path);
        if marker.exists() {
            return Err(DelegationJournalError::Quarantined {
                path: marker,
                reason: "a prior recovery quarantined this journal".to_owned(),
            });
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(DelegationJournalError::Corrupt {
                    line: 0,
                    reason: "journal path is a symbolic link".to_owned(),
                });
            }
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(DelegationJournalError::WriterAlreadyActive { path });
            }
            Err(TryLockError::Error(error)) => return Err(DelegationJournalError::Io(error)),
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let header_len = if let Some(job_id) = expected_job_id {
            if bytes.is_empty() {
                let header = canonical_native_header(job_id);
                file.write_all(header.as_bytes())?;
                file.flush()?;
                file.sync_all()?;
                bytes.extend_from_slice(header.as_bytes());
                header.len()
            } else {
                match parse_native_header(&bytes, job_id) {
                    Ok(value) => value,
                    Err(DelegationJournalError::Corrupt { line, reason }) => {
                        drop(file);
                        let combined = format!("line {line}: {reason}");
                        let quarantine = quarantine_native_path(&path, &bytes, &combined)?;
                        return Err(DelegationJournalError::Quarantined {
                            path: quarantine,
                            reason: combined,
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
        } else {
            0
        };
        let event_bytes = &bytes[header_len..];
        let (records, valid_event_len) = match recover_jsonl(event_bytes) {
            Ok(recovered) => recovered,
            Err(DelegationJournalError::Corrupt { line, reason }) => {
                drop(file);
                let combined = format!("line {line}: {reason}");
                let quarantine = quarantine_native_path(&path, &bytes, &combined)?;
                return Err(DelegationJournalError::Quarantined {
                    path: quarantine,
                    reason: combined,
                });
            }
            Err(error) => return Err(error),
        };
        if let Some(job_id) = expected_job_id {
            if records.iter().any(|event| event.identity.job_id != job_id) {
                drop(file);
                let combined = "journal event job_id does not match its durable header".to_owned();
                let quarantine = quarantine_native_path(&path, &bytes, &combined)?;
                return Err(DelegationJournalError::Quarantined {
                    path: quarantine,
                    reason: combined,
                });
            }
        }
        let valid_len = header_len.checked_add(valid_event_len).ok_or_else(|| {
            DelegationJournalError::Corrupt {
                line: 0,
                reason: "journal length exhausted".to_owned(),
            }
        })?;
        if valid_len < bytes.len() {
            file.set_len(valid_len as u64)?;
            file.sync_all()?;
        }
        file.seek(SeekFrom::End(0))?;
        Ok(Self {
            path,
            file: Some(file),
            records,
            quarantine_reason: None,
            job_id: expected_job_id.map(str::to_owned),
            header_len: u64::try_from(header_len).map_err(|_| DelegationJournalError::Corrupt {
                line: 0,
                reason: "journal header length exceeds u64".to_owned(),
            })?,
        })
    }

    pub fn open_for_job(
        root: impl AsRef<Path>,
        job_id: &str,
    ) -> Result<Self, DelegationJournalError> {
        if !is_uuid(job_id) {
            return Err(DelegationJournalError::Corrupt {
                line: 0,
                reason: "job_id must be a canonical lowercase UUID".to_owned(),
            });
        }
        let filename = format!("{}.jsonl", sha256_hex(job_id.as_bytes()));
        Self::open_bound(root.as_ref().join(filename), Some(job_id))
    }

    /// Reopens an existing header-bound job journal during process-start
    /// recovery. The durable header, not the hashed filename, supplies the job
    /// identity. `open_bound` parses the header again after taking the writer
    /// lock, so a raced replacement cannot change the recovered identity.
    pub fn open_existing_job_for_recovery(
        path: impl AsRef<Path>,
    ) -> Result<Self, DelegationJournalError> {
        let path = path.as_ref().to_path_buf();
        let marker = quarantine_marker_path(&path);
        if marker.exists() {
            return Err(DelegationJournalError::Quarantined {
                path: marker,
                reason: "a prior recovery quarantined this journal".to_owned(),
            });
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(DelegationJournalError::Corrupt {
                    line: 0,
                    reason: "journal path is a symbolic link".to_owned(),
                });
            }
        }
        let bytes = fs::read(&path)?;
        let job_id = match parse_native_header_job_id(&bytes) {
            Ok(job_id) => job_id,
            Err(DelegationJournalError::Corrupt { line, reason }) => {
                let combined = format!("line {line}: {reason}");
                let quarantine = quarantine_native_path(&path, &bytes, &combined)?;
                return Err(DelegationJournalError::Quarantined {
                    path: quarantine,
                    reason: combined,
                });
            }
            Err(error) => return Err(error),
        };
        Self::open_bound(path, Some(&job_id))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn job_id(&self) -> Option<&str> {
        self.job_id.as_deref()
    }
}

impl DelegationJournal for NativeJsonlDelegationJournal {
    fn records(&self) -> &[DelegationEvent] {
        &self.records
    }

    fn quarantine_reason(&self) -> Option<&str> {
        self.quarantine_reason.as_deref()
    }

    fn quarantine(&mut self, reason: &str) -> Result<(), DelegationJournalError> {
        if self.quarantine_reason.is_some() {
            return Ok(());
        }
        if let Some(file) = self.file.take() {
            file.sync_all()?;
            drop(file);
        }
        let bytes = fs::read(&self.path)?;
        let quarantine = quarantine_native_path(&self.path, &bytes, reason)?;
        self.quarantine_reason = Some(reason.to_owned());
        self.path = quarantine;
        Ok(())
    }

    fn append(
        &mut self,
        draft: DelegationEventDraft,
    ) -> Result<DelegationEvent, DelegationJournalError> {
        if let Some(reason) = &self.quarantine_reason {
            return Err(DelegationJournalError::Corrupt {
                line: 0,
                reason: format!("journal is quarantined: {reason}"),
            });
        }
        if self
            .job_id
            .as_deref()
            .is_some_and(|job_id| draft.identity.job_id != job_id)
        {
            return Err(DelegationJournalError::Corrupt {
                line: self.records.len().saturating_add(1),
                reason: "delegation job_id does not match the durable journal header".to_owned(),
            });
        }
        let event = build_event(&self.records, draft)?;
        let line = canonical_event_json(&event);
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| DelegationJournalError::Corrupt {
                line: 0,
                reason: "journal writer is unavailable".to_owned(),
            })?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        self.records.push(event.clone());
        Ok(event)
    }

    fn reset_if_head(&mut self, expected_event_sha256: &str) -> Result<(), DelegationJournalError> {
        if let Some(reason) = &self.quarantine_reason {
            return Err(DelegationJournalError::Corrupt {
                line: 0,
                reason: format!("journal is quarantined: {reason}"),
            });
        }
        ensure_expected_head(&self.records, expected_event_sha256)?;
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| DelegationJournalError::Corrupt {
                line: 0,
                reason: "journal writer is unavailable".to_owned(),
            })?;
        file.set_len(self.header_len)?;
        file.seek(SeekFrom::Start(self.header_len))?;
        file.flush()?;
        file.sync_all()?;
        self.records.clear();
        Ok(())
    }
}

fn ensure_expected_head(
    records: &[DelegationEvent],
    expected_event_sha256: &str,
) -> Result<(), DelegationJournalError> {
    let actual = records.last().map(|event| event.event_sha256.clone());
    if actual.as_deref() != Some(expected_event_sha256) {
        return Err(DelegationJournalError::HeadChanged {
            expected: expected_event_sha256.to_owned(),
            actual,
        });
    }
    Ok(())
}

fn quarantine_marker_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".quarantined");
    PathBuf::from(value)
}

fn canonical_native_header(job_id: &str) -> String {
    format!(
        "{{\"schema\":{},\"job_id\":{}}}\n",
        json_string(HEADER_SCHEMA),
        json_string(job_id)
    )
}

fn parse_native_header(
    bytes: &[u8],
    expected_job_id: &str,
) -> Result<usize, DelegationJournalError> {
    let (job_id, header_len) = parse_native_header_parts(bytes)?;
    if job_id != expected_job_id {
        return Err(DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header belongs to another job".to_owned(),
        });
    }
    Ok(header_len)
}

fn parse_native_header_job_id(bytes: &[u8]) -> Result<String, DelegationJournalError> {
    parse_native_header_parts(bytes).map(|(job_id, _)| job_id)
}

fn parse_native_header_parts(bytes: &[u8]) -> Result<(String, usize), DelegationJournalError> {
    let newline = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header is torn".to_owned(),
        })?;
    let header_len = newline
        .checked_add(1)
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header length exhausted".to_owned(),
        })?;
    let line =
        std::str::from_utf8(&bytes[..newline]).map_err(|_| DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header is not UTF-8".to_owned(),
        })?;
    let value: Value =
        serde_json::from_str(line).map_err(|error| DelegationJournalError::Corrupt {
            line: 1,
            reason: error.to_string(),
        })?;
    let object = value
        .as_object()
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header is not an object".to_owned(),
        })?;
    if object.len() != 2 || string_field(object, "schema", 1)? != HEADER_SCHEMA {
        return Err(DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header schema or field set mismatch".to_owned(),
        });
    }
    let job_id = string_field(object, "job_id", 1)?.to_owned();
    if !is_uuid(&job_id)
        || bytes.get(..header_len) != Some(canonical_native_header(&job_id).as_bytes())
    {
        return Err(DelegationJournalError::Corrupt {
            line: 1,
            reason: "durable journal header is non-canonical".to_owned(),
        });
    }
    Ok((job_id, header_len))
}

fn quarantine_native_path(
    path: &Path,
    bytes: &[u8],
    reason: &str,
) -> Result<PathBuf, DelegationJournalError> {
    let digest = sha256_hex(bytes);
    let mut ordinal = 0_u64;
    let quarantine = loop {
        let suffix = if ordinal == 0 {
            format!(".quarantine-{digest}")
        } else {
            format!(".quarantine-{digest}-{ordinal}")
        };
        let mut value = path.as_os_str().to_owned();
        value.push(suffix);
        let candidate = PathBuf::from(value);
        if !candidate.exists() {
            break candidate;
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| DelegationJournalError::Corrupt {
                line: 0,
                reason: "quarantine path space exhausted".to_owned(),
            })?;
    };
    fs::rename(path, &quarantine)?;
    let marker = quarantine_marker_path(path);
    let marker_body = format!(
        "{{\"schema\":{},\"quarantine_path\":{},\"reason\":{}}}\n",
        json_string("clearra.delegation-journal-quarantine.v1"),
        json_string(&quarantine.to_string_lossy()),
        json_string(reason)
    );
    let marker_result = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker)
        .and_then(|mut file| {
            file.write_all(marker_body.as_bytes())?;
            file.flush()?;
            file.sync_all()
        });
    if let Err(error) = marker_result {
        let _ = fs::rename(&quarantine, path);
        return Err(DelegationJournalError::Io(error));
    }
    Ok(quarantine)
}

pub fn default_native_delegation_journal_root() -> Result<PathBuf, DelegationJournalError> {
    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("LOCALAPPDATA").ok_or_else(|| DelegationJournalError::Corrupt {
            line: 0,
            reason: "LOCALAPPDATA is unavailable".to_owned(),
        })?;
        return Ok(PathBuf::from(base)
            .join("Clearra")
            .join("state")
            .join("delegation-journal")
            .join("v1"));
    }
    #[cfg(target_os = "macos")]
    {
        let base = env::var_os("HOME").ok_or_else(|| DelegationJournalError::Corrupt {
            line: 0,
            reason: "HOME is unavailable".to_owned(),
        })?;
        return Ok(PathBuf::from(base)
            .join("Library")
            .join("Application Support")
            .join("Clearra")
            .join("state")
            .join("delegation-journal")
            .join("v1"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
            .ok_or_else(|| DelegationJournalError::Corrupt {
                line: 0,
                reason: "XDG_STATE_HOME and HOME are unavailable".to_owned(),
            })?;
        return Ok(base.join("clearra/delegation-journal/v1"));
    }
    #[allow(unreachable_code)]
    Err(DelegationJournalError::Corrupt {
        line: 0,
        reason: "unsupported native journal platform".to_owned(),
    })
}

#[derive(Clone, Debug)]
struct DelegationState {
    identity: DelegationIdentity,
    budget: DelegationBudget,
    phase: DelegationPhase,
    fencing_token: u64,
    worker_id: Option<String>,
    reservation_sha256: Option<String>,
    result_sha256: Option<String>,
    worker_reply_sha256: Option<String>,
    offered_at_unix_ms: Option<u64>,
    last_heartbeat_unix_ms: u64,
    last_persisted_renewal_unix_ms: u64,
    terminal_at_unix_ms: Option<u64>,
}

#[derive(Debug)]
pub struct DurableDelegationAuthority<J: DelegationJournal> {
    journal: J,
    states: BTreeMap<(String, String), DelegationState>,
    next_fencing_token: u64,
}

impl<J: DelegationJournal> DurableDelegationAuthority<J> {
    pub fn recover(journal: J) -> Result<Self, DurableDelegationError> {
        let mut authority = Self {
            journal,
            states: BTreeMap::new(),
            next_fencing_token: 0,
        };
        if let Some(reason) = authority.journal.quarantine_reason() {
            return Err(DurableDelegationError::Journal(
                DelegationJournalError::Corrupt {
                    line: 0,
                    reason: format!("journal is quarantined: {reason}"),
                },
            ));
        }
        let records = authority.journal.records().to_vec();
        for event in records {
            if let Err(error) = authority.replay(event) {
                let reason = error.to_string();
                let _ = authority.journal.quarantine(&reason);
                return Err(error);
            }
        }
        if let Some(state) = authority
            .states
            .values()
            .find(|state| !state.phase.terminal())
        {
            let reason = format!(
                "process restart found unresolved delegation {}/{} in phase {}; same-identity resume is forbidden",
                state.identity.job_id,
                state.identity.task_id,
                state.phase.as_str()
            );
            let _ = authority.journal.quarantine(&reason);
            return Err(DurableDelegationError::Journal(
                DelegationJournalError::Corrupt { line: 0, reason },
            ));
        }
        Ok(authority)
    }

    pub fn journal(&self) -> &J {
        &self.journal
    }

    pub fn journal_mut(&mut self) -> &mut J {
        &mut self.journal
    }

    pub fn prepare(
        &mut self,
        identity: DelegationIdentity,
        budget: DelegationBudget,
        now_unix_ms: u64,
    ) -> Result<DelegationToken, DurableDelegationError> {
        identity.validate()?;
        let key = (identity.job_id.clone(), identity.task_id.clone());
        if self.states.contains_key(&key) {
            return Err(DurableDelegationError::IdentityAlreadyUsed);
        }
        let fencing_token = self
            .next_fencing_token
            .checked_add(1)
            .ok_or(DurableDelegationError::FencingTokenExhausted)?;
        let event = self.append(DelegationEventDraft {
            identity: identity.clone(),
            budget,
            phase: DelegationPhase::Prepared,
            fencing_token,
            worker_id: None,
            reservation_sha256: None,
            result_sha256: None,
            worker_reply_sha256: None,
            timestamp_unix_ms: now_unix_ms,
            reason: None,
        })?;
        self.next_fencing_token = fencing_token;
        self.states.insert(
            key,
            DelegationState {
                identity: event.identity,
                budget,
                phase: DelegationPhase::Prepared,
                fencing_token,
                worker_id: None,
                reservation_sha256: None,
                result_sha256: None,
                worker_reply_sha256: None,
                offered_at_unix_ms: None,
                last_heartbeat_unix_ms: now_unix_ms,
                last_persisted_renewal_unix_ms: now_unix_ms,
                terminal_at_unix_ms: None,
            },
        );
        Ok(DelegationToken {
            job_id: identity.job_id,
            task_id: identity.task_id,
            fencing_token,
        })
    }

    pub fn offered(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.transition(
            token,
            DelegationPhase::Offered,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        self.state_mut(token)?.offered_at_unix_ms = Some(now_unix_ms);
        Ok(())
    }

    pub fn accepted(
        &mut self,
        token: &DelegationToken,
        worker_id: impl Into<String>,
        reservation_sha256: impl Into<String>,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        let worker_id = worker_id.into();
        let reservation_sha256 = reservation_sha256.into();
        if !is_worker_id(&worker_id) || !is_sha256(&reservation_sha256) {
            return Err(DurableDelegationError::InvalidIdentity);
        }
        let offered_at = self.state(token)?.offered_at_unix_ms.ok_or(
            DurableDelegationError::InvalidTransition {
                from: self.state(token)?.phase,
                to: DelegationPhase::Accepted,
            },
        )?;
        if now_unix_ms.saturating_sub(offered_at) > OFFER_ACCEPT_TIMEOUT_MS {
            return Err(DurableDelegationError::OfferExpired);
        }
        self.transition(
            token,
            DelegationPhase::Accepted,
            now_unix_ms,
            Some(worker_id.clone()),
            Some(reservation_sha256.clone()),
            None,
            None,
            None,
        )?;
        let state = self.state_mut(token)?;
        state.worker_id = Some(worker_id);
        state.reservation_sha256 = Some(reservation_sha256);
        Ok(())
    }

    pub fn publish(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<ExecutableDelegationPermit, DurableDelegationError> {
        let event = self.transition(
            token,
            DelegationPhase::Published,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        let state = self.state_mut(token)?;
        state.last_heartbeat_unix_ms = now_unix_ms;
        state.last_persisted_renewal_unix_ms = now_unix_ms;
        Ok(ExecutableDelegationPermit {
            job_id: state.identity.job_id.clone(),
            task_id: state.identity.task_id.clone(),
            worker_id: state
                .worker_id
                .clone()
                .ok_or(DurableDelegationError::InvalidIdentity)?,
            payload_sha256: state.identity.payload_sha256.clone(),
            request_sha256: state.identity.request_sha256.clone(),
            fencing_token: state.fencing_token,
            publication_sequence: event.sequence,
            publication_sha256: event.event_sha256,
            expires_at_unix_ms: now_unix_ms
                .checked_add(ACTIVE_LEASE_EXPIRY_MS)
                .ok_or(DurableDelegationError::SequenceExhausted)?,
        })
    }

    pub fn running(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.ensure_live(token, now_unix_ms)?;
        self.transition(
            token,
            DelegationPhase::Running,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        self.state_mut(token)?.last_heartbeat_unix_ms = now_unix_ms;
        Ok(())
    }

    pub fn heartbeat(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<bool, DurableDelegationError> {
        self.ensure_live(token, now_unix_ms)?;
        let persist = now_unix_ms.saturating_sub(self.state(token)?.last_persisted_renewal_unix_ms)
            >= PERSISTED_RENEWAL_INTERVAL_MS;
        self.state_mut(token)?.last_heartbeat_unix_ms = now_unix_ms;
        if persist {
            self.transition(
                token,
                DelegationPhase::Renewed,
                now_unix_ms,
                None,
                None,
                None,
                None,
                None,
            )?;
            self.state_mut(token)?.last_persisted_renewal_unix_ms = now_unix_ms;
        }
        Ok(persist)
    }

    pub fn result_sealed(
        &mut self,
        token: &DelegationToken,
        result_sha256: impl Into<String>,
        worker_reply_sha256: impl Into<String>,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.ensure_live(token, now_unix_ms)?;
        let result_sha256 = result_sha256.into();
        let worker_reply_sha256 = worker_reply_sha256.into();
        if !is_sha256(&result_sha256) || !is_sha256(&worker_reply_sha256) {
            return Err(DurableDelegationError::InvalidIdentity);
        }
        let state = self.state(token)?;
        if matches!(
            state.phase,
            DelegationPhase::ResultSealed
                | DelegationPhase::ResultApplied
                | DelegationPhase::Completed
        ) {
            return if state.result_sha256.as_deref() == Some(result_sha256.as_str())
                && state.worker_reply_sha256.as_deref() == Some(worker_reply_sha256.as_str())
            {
                Ok(())
            } else {
                Err(DurableDelegationError::ResultDigestMismatch)
            };
        }
        self.transition(
            token,
            DelegationPhase::ResultSealed,
            now_unix_ms,
            None,
            None,
            Some(result_sha256.clone()),
            Some(worker_reply_sha256.clone()),
            None,
        )?;
        let state = self.state_mut(token)?;
        state.result_sha256 = Some(result_sha256);
        state.worker_reply_sha256 = Some(worker_reply_sha256);
        Ok(())
    }

    /// Returns whether the caller owns the single in-process merger application
    /// for the already sealed immutable result. A restart never reaches this
    /// method with a live `ResultSealed` state because recovery quarantines every
    /// unresolved nonterminal delegation.
    pub fn result_application_decision(
        &self,
        token: &DelegationToken,
        result_sha256: &str,
    ) -> Result<ResultApplicationDecision, DurableDelegationError> {
        if !is_sha256(result_sha256) {
            return Err(DurableDelegationError::InvalidIdentity);
        }
        let state = self.state(token)?;
        if state.result_sha256.as_deref() != Some(result_sha256) {
            return Err(DurableDelegationError::ResultDigestMismatch);
        }
        match state.phase {
            DelegationPhase::ResultSealed => Ok(ResultApplicationDecision::ApplyOnce),
            DelegationPhase::ResultApplied | DelegationPhase::Completed => {
                Ok(ResultApplicationDecision::AlreadyApplied)
            }
            phase => Err(DurableDelegationError::InvalidTransition {
                from: phase,
                to: DelegationPhase::ResultApplied,
            }),
        }
    }

    pub fn result_applied(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        if matches!(
            self.state(token)?.phase,
            DelegationPhase::ResultApplied | DelegationPhase::Completed
        ) {
            return Ok(());
        }
        self.transition(
            token,
            DelegationPhase::ResultApplied,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        Ok(())
    }

    pub fn complete(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        if self.state(token)?.phase == DelegationPhase::Completed {
            return Ok(());
        }
        self.transition(
            token,
            DelegationPhase::Completed,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        Ok(())
    }

    pub fn fail_closed(
        &mut self,
        token: &DelegationToken,
        reason: impl Into<String>,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.transition(
            token,
            DelegationPhase::FailedClosed,
            now_unix_ms,
            None,
            None,
            None,
            None,
            Some(reason.into()),
        )?;
        Ok(())
    }

    pub fn revoke(
        &mut self,
        token: &DelegationToken,
        reason: impl Into<String>,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.transition(
            token,
            DelegationPhase::Revoked,
            now_unix_ms,
            None,
            None,
            None,
            None,
            Some(reason.into()),
        )?;
        Ok(())
    }

    pub fn cancel(
        &mut self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        self.transition(
            token,
            DelegationPhase::Cancelled,
            now_unix_ms,
            None,
            None,
            None,
            None,
            None,
        )?;
        Ok(())
    }

    pub fn expire_stale(&mut self, now_unix_ms: u64) -> Result<usize, DurableDelegationError> {
        let stale: Vec<_> = self
            .states
            .values()
            .filter(|state| {
                matches!(
                    state.phase,
                    DelegationPhase::Published
                        | DelegationPhase::Running
                        | DelegationPhase::Renewed
                ) && now_unix_ms.saturating_sub(state.last_heartbeat_unix_ms)
                    > ACTIVE_LEASE_EXPIRY_MS
            })
            .map(|state| DelegationToken {
                job_id: state.identity.job_id.clone(),
                task_id: state.identity.task_id.clone(),
                fencing_token: state.fencing_token,
            })
            .collect();
        for token in &stale {
            self.transition(
                token,
                DelegationPhase::Expired,
                now_unix_ms,
                None,
                None,
                None,
                None,
                Some("heartbeat lease expired".to_owned()),
            )?;
        }
        Ok(stale.len())
    }

    pub fn phase(
        &self,
        token: &DelegationToken,
    ) -> Result<DelegationPhase, DurableDelegationError> {
        Ok(self.state(token)?.phase)
    }

    pub fn retained_terminal_count(&self, now_unix_ms: u64) -> usize {
        self.states
            .values()
            .filter(|state| {
                state.terminal_at_unix_ms.is_some_and(|ended| {
                    now_unix_ms.saturating_sub(ended) <= TERMINAL_TOMBSTONE_RETENTION_MS
                })
            })
            .count()
    }

    /// Durably releases dedupe tombstones only when every recovered state is a
    /// terminal state strictly older than the retention interval. The checked
    /// journal head is the compare-and-reset fence: a changed head leaves both
    /// the journal and the in-memory authority untouched.
    pub fn compact_expired_terminal_tombstones(
        &mut self,
        now_unix_ms: u64,
    ) -> Result<bool, DurableDelegationError> {
        if self.states.is_empty()
            || self.states.values().any(|state| {
                !state.phase.terminal()
                    || state.terminal_at_unix_ms.is_none_or(|ended| {
                        now_unix_ms.saturating_sub(ended) <= TERMINAL_TOMBSTONE_RETENTION_MS
                    })
            })
        {
            return Ok(false);
        }
        let expected_head = self
            .journal
            .records()
            .last()
            .map(|event| event.event_sha256.clone())
            .ok_or_else(|| {
                DurableDelegationError::Journal(DelegationJournalError::Corrupt {
                    line: 0,
                    reason: "terminal delegation states have no journal head".to_owned(),
                })
            })?;
        self.journal.reset_if_head(&expected_head)?;
        self.states.clear();
        // A fully expired journal defines a new fencing epoch. This is safe
        // only after the 24-hour dedupe window and keeps live/recovered
        // authorities identical to a fresh recovery from the durable reset.
        self.next_fencing_token = 0;
        Ok(true)
    }

    fn ensure_live(
        &self,
        token: &DelegationToken,
        now_unix_ms: u64,
    ) -> Result<(), DurableDelegationError> {
        let state = self.state(token)?;
        if now_unix_ms.saturating_sub(state.last_heartbeat_unix_ms) > ACTIVE_LEASE_EXPIRY_MS {
            return Err(DurableDelegationError::LeaseExpired);
        }
        Ok(())
    }

    fn state(&self, token: &DelegationToken) -> Result<&DelegationState, DurableDelegationError> {
        let state = self
            .states
            .get(&(token.job_id.clone(), token.task_id.clone()))
            .ok_or(DurableDelegationError::UnknownDelegation)?;
        if state.fencing_token != token.fencing_token {
            return Err(DurableDelegationError::StaleFence);
        }
        Ok(state)
    }

    fn state_mut(
        &mut self,
        token: &DelegationToken,
    ) -> Result<&mut DelegationState, DurableDelegationError> {
        let state = self
            .states
            .get_mut(&(token.job_id.clone(), token.task_id.clone()))
            .ok_or(DurableDelegationError::UnknownDelegation)?;
        if state.fencing_token != token.fencing_token {
            return Err(DurableDelegationError::StaleFence);
        }
        Ok(state)
    }

    fn transition(
        &mut self,
        token: &DelegationToken,
        to: DelegationPhase,
        now_unix_ms: u64,
        worker_id: Option<String>,
        reservation_sha256: Option<String>,
        result_sha256: Option<String>,
        worker_reply_sha256: Option<String>,
        reason: Option<String>,
    ) -> Result<DelegationEvent, DurableDelegationError> {
        let state = self.state(token)?.clone();
        if !valid_transition(state.phase, to) {
            return Err(DurableDelegationError::InvalidTransition {
                from: state.phase,
                to,
            });
        }
        let event = self.append(DelegationEventDraft {
            identity: state.identity,
            budget: state.budget,
            phase: to,
            fencing_token: token.fencing_token,
            worker_id: worker_id.or(state.worker_id),
            reservation_sha256: reservation_sha256.or(state.reservation_sha256),
            result_sha256: result_sha256.or(state.result_sha256),
            worker_reply_sha256: worker_reply_sha256.or(state.worker_reply_sha256),
            timestamp_unix_ms: now_unix_ms,
            reason,
        })?;
        let state = self.state_mut(token)?;
        state.phase = to;
        if to.terminal() {
            state.terminal_at_unix_ms = Some(now_unix_ms);
        }
        Ok(event)
    }

    fn append(
        &mut self,
        draft: DelegationEventDraft,
    ) -> Result<DelegationEvent, DurableDelegationError> {
        self.journal.append(draft).map_err(Into::into)
    }

    fn replay(&mut self, event: DelegationEvent) -> Result<(), DurableDelegationError> {
        validate_replayed_event(&event)?;
        let key = (
            event.identity.job_id.clone(),
            event.identity.task_id.clone(),
        );
        if event.phase == DelegationPhase::Prepared {
            if self.states.contains_key(&key) {
                return Err(DurableDelegationError::IdentityAlreadyUsed);
            }
            if event.fencing_token <= self.next_fencing_token {
                return Err(replay_corrupt(
                    &event,
                    "prepared fencing tokens are not strictly increasing",
                ));
            }
            self.next_fencing_token = event.fencing_token;
            self.states.insert(
                key,
                DelegationState {
                    identity: event.identity,
                    budget: event.budget,
                    phase: event.phase,
                    fencing_token: event.fencing_token,
                    worker_id: None,
                    reservation_sha256: None,
                    result_sha256: None,
                    worker_reply_sha256: None,
                    offered_at_unix_ms: None,
                    last_heartbeat_unix_ms: event.timestamp_unix_ms,
                    last_persisted_renewal_unix_ms: event.timestamp_unix_ms,
                    terminal_at_unix_ms: None,
                },
            );
            return Ok(());
        }
        let state = self
            .states
            .get_mut(&key)
            .ok_or(DurableDelegationError::UnknownDelegation)?;
        if state.fencing_token != event.fencing_token
            || state.identity != event.identity
            || state.budget != event.budget
        {
            return Err(DurableDelegationError::StaleFence);
        }
        if !valid_transition(state.phase, event.phase) {
            return Err(DurableDelegationError::InvalidTransition {
                from: state.phase,
                to: event.phase,
            });
        }
        match (&state.worker_id, &state.reservation_sha256) {
            (None, None) if event.phase == DelegationPhase::Accepted => {}
            (None, None) if event.worker_id.is_none() && event.reservation_sha256.is_none() => {}
            (Some(worker_id), Some(reservation_sha256))
                if event.worker_id.as_ref() == Some(worker_id)
                    && event.reservation_sha256.as_ref() == Some(reservation_sha256) => {}
            _ => {
                return Err(replay_corrupt(
                    &event,
                    "worker reservation changed outside the accepted transition",
                ));
            }
        }
        match (&state.result_sha256, &state.worker_reply_sha256) {
            (None, None) if event.phase == DelegationPhase::ResultSealed => {}
            (None, None)
                if event.result_sha256.is_none() && event.worker_reply_sha256.is_none() => {}
            (Some(result_sha256), Some(worker_reply_sha256))
                if event.result_sha256.as_ref() == Some(result_sha256)
                    && event.worker_reply_sha256.as_ref() == Some(worker_reply_sha256) => {}
            _ => {
                return Err(replay_corrupt(
                    &event,
                    "sealed result identity changed outside the result-sealed transition",
                ));
            }
        }
        state.phase = event.phase;
        state.worker_id = event.worker_id;
        state.reservation_sha256 = event.reservation_sha256;
        state.result_sha256 = event.result_sha256;
        state.worker_reply_sha256 = event.worker_reply_sha256;
        if event.phase == DelegationPhase::Offered {
            state.offered_at_unix_ms = Some(event.timestamp_unix_ms);
        }
        if matches!(
            event.phase,
            DelegationPhase::Published | DelegationPhase::Running | DelegationPhase::Renewed
        ) {
            state.last_heartbeat_unix_ms = event.timestamp_unix_ms;
        }
        if matches!(
            event.phase,
            DelegationPhase::Published | DelegationPhase::Renewed
        ) {
            state.last_persisted_renewal_unix_ms = event.timestamp_unix_ms;
        }
        if event.phase.terminal() {
            state.terminal_at_unix_ms = Some(event.timestamp_unix_ms);
        }
        Ok(())
    }
}

fn validate_replayed_event(event: &DelegationEvent) -> Result<(), DurableDelegationError> {
    event
        .identity
        .validate()
        .map_err(|_| replay_corrupt(event, "invalid identity"))?;
    if event.budget.compute_units == 0 {
        return Err(replay_corrupt(event, "compute budget must be positive"));
    }
    if event.fencing_token == 0 {
        return Err(replay_corrupt(event, "fencing token must be positive"));
    }
    if event.worker_id.is_some() != event.reservation_sha256.is_some() {
        return Err(replay_corrupt(
            event,
            "worker identity and reservation digest must appear together",
        ));
    }
    if event
        .worker_id
        .as_deref()
        .is_some_and(|value| !is_worker_id(value))
    {
        return Err(replay_corrupt(
            event,
            "worker identity must be a job-local positive integer",
        ));
    }
    if event.result_sha256.is_some() != event.worker_reply_sha256.is_some() {
        return Err(replay_corrupt(
            event,
            "normalized result and worker reply digests must appear together",
        ));
    }
    match event.phase {
        DelegationPhase::Prepared | DelegationPhase::Offered => {
            if event.worker_id.is_some() {
                return Err(replay_corrupt(
                    event,
                    "worker reservation is forbidden before acceptance",
                ));
            }
            if event.result_sha256.is_some() {
                return Err(replay_corrupt(
                    event,
                    "result identity is forbidden before result sealing",
                ));
            }
        }
        DelegationPhase::Accepted
        | DelegationPhase::Published
        | DelegationPhase::Running
        | DelegationPhase::Renewed => {
            if event.worker_id.is_none() {
                return Err(replay_corrupt(
                    event,
                    "accepted delegation is missing its worker reservation",
                ));
            }
            if event.result_sha256.is_some() {
                return Err(replay_corrupt(
                    event,
                    "result identity is forbidden before result sealing",
                ));
            }
        }
        DelegationPhase::ResultSealed
        | DelegationPhase::ResultApplied
        | DelegationPhase::Completed => {
            if event.worker_id.is_none() {
                return Err(replay_corrupt(
                    event,
                    "accepted delegation is missing its worker reservation",
                ));
            }
            if event.result_sha256.is_none() {
                return Err(replay_corrupt(
                    event,
                    "sealed delegation is missing its result identity",
                ));
            }
        }
        DelegationPhase::Revoked
        | DelegationPhase::Expired
        | DelegationPhase::Cancelled
        | DelegationPhase::FailedClosed => {}
    }
    Ok(())
}

fn replay_corrupt(event: &DelegationEvent, reason: &str) -> DurableDelegationError {
    DurableDelegationError::Journal(DelegationJournalError::Corrupt {
        line: usize::try_from(event.sequence).unwrap_or(usize::MAX),
        reason: reason.to_owned(),
    })
}

fn valid_transition(from: DelegationPhase, to: DelegationPhase) -> bool {
    if from.terminal() {
        return false;
    }
    if matches!(
        to,
        DelegationPhase::Revoked
            | DelegationPhase::Expired
            | DelegationPhase::Cancelled
            | DelegationPhase::FailedClosed
    ) {
        return true;
    }
    matches!(
        (from, to),
        (DelegationPhase::Prepared, DelegationPhase::Offered)
            | (DelegationPhase::Offered, DelegationPhase::Accepted)
            | (DelegationPhase::Accepted, DelegationPhase::Published)
            | (DelegationPhase::Published, DelegationPhase::Running)
            | (DelegationPhase::Running, DelegationPhase::Renewed)
            | (DelegationPhase::Renewed, DelegationPhase::Renewed)
            | (DelegationPhase::Running, DelegationPhase::ResultSealed)
            | (DelegationPhase::Renewed, DelegationPhase::ResultSealed)
            | (
                DelegationPhase::ResultSealed,
                DelegationPhase::ResultApplied
            )
            | (DelegationPhase::ResultApplied, DelegationPhase::Completed)
    )
}

fn build_event(
    records: &[DelegationEvent],
    draft: DelegationEventDraft,
) -> Result<DelegationEvent, DelegationJournalError> {
    let sequence = u64::try_from(records.len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line: records.len().saturating_add(1),
            reason: "sequence exhausted".to_owned(),
        })?;
    let previous_event_sha256 = records.last().map_or_else(
        || ZERO_SHA256.to_owned(),
        |event| event.event_sha256.clone(),
    );
    let material = canonical_hash_material(sequence, &draft, &previous_event_sha256);
    let event_sha256 = sha256_hex(material.as_bytes());
    Ok(DelegationEvent {
        sequence,
        identity: draft.identity,
        budget: draft.budget,
        phase: draft.phase,
        fencing_token: draft.fencing_token,
        worker_id: draft.worker_id,
        reservation_sha256: draft.reservation_sha256,
        result_sha256: draft.result_sha256,
        worker_reply_sha256: draft.worker_reply_sha256,
        timestamp_unix_ms: draft.timestamp_unix_ms,
        reason: draft.reason,
        previous_event_sha256,
        event_sha256,
    })
}

fn canonical_hash_material(
    sequence: u64,
    draft: &DelegationEventDraft,
    previous_event_sha256: &str,
) -> String {
    format!(
        concat!(
            "{{\"schema\":{},\"sequence\":{},\"job_id\":{},\"task_id\":{},",
            "\"coordinator_id\":{},\"payload_sha256\":{},\"request_sha256\":{},",
            "\"compute_units\":{},\"memory_bytes\":{},\"phase\":{},",
            "\"fencing_token\":{},\"worker_id\":{},\"reservation_sha256\":{},",
            "\"result_sha256\":{},\"worker_reply_sha256\":{},",
            "\"timestamp_unix_ms\":{},\"reason\":{},\"previous_event_sha256\":{}}}"
        ),
        json_string(SCHEMA),
        json_string(&sequence.to_string()),
        json_string(&draft.identity.job_id),
        json_string(&draft.identity.task_id),
        json_string(&draft.identity.coordinator_id),
        json_string(&draft.identity.payload_sha256),
        json_string(&draft.identity.request_sha256),
        json_string(&draft.budget.compute_units.to_string()),
        json_string(&draft.budget.memory_bytes.to_string()),
        json_string(draft.phase.as_str()),
        json_string(&draft.fencing_token.to_string()),
        optional_json_string(draft.worker_id.as_deref()),
        optional_json_string(draft.reservation_sha256.as_deref()),
        optional_json_string(draft.result_sha256.as_deref()),
        optional_json_string(draft.worker_reply_sha256.as_deref()),
        json_string(&draft.timestamp_unix_ms.to_string()),
        optional_json_string(draft.reason.as_deref()),
        json_string(previous_event_sha256),
    )
}

fn canonical_event_json(event: &DelegationEvent) -> String {
    let draft = DelegationEventDraft {
        identity: event.identity.clone(),
        budget: event.budget,
        phase: event.phase,
        fencing_token: event.fencing_token,
        worker_id: event.worker_id.clone(),
        reservation_sha256: event.reservation_sha256.clone(),
        result_sha256: event.result_sha256.clone(),
        worker_reply_sha256: event.worker_reply_sha256.clone(),
        timestamp_unix_ms: event.timestamp_unix_ms,
        reason: event.reason.clone(),
    };
    let material = canonical_hash_material(event.sequence, &draft, &event.previous_event_sha256);
    format!(
        "{},\"event_sha256\":{}}}",
        material.trim_end_matches('}'),
        json_string(&event.event_sha256)
    )
}

fn recover_jsonl(bytes: &[u8]) -> Result<(Vec<DelegationEvent>, usize), DelegationJournalError> {
    let mut records = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let remainder = &bytes[offset..];
        let Some(relative_end) = remainder.iter().position(|byte| *byte == b'\n') else {
            // A non-newline-terminated final append is not acknowledged durable
            // state. It is the only torn-write shape that may be truncated.
            return Ok((records, offset));
        };
        let end = offset + relative_end;
        let line_number = records.len() + 1;
        let line = std::str::from_utf8(&bytes[offset..end]).map_err(|_| {
            DelegationJournalError::Corrupt {
                line: line_number,
                reason: "invalid UTF-8".to_owned(),
            }
        })?;
        let event = parse_event(line, line_number)?;
        let expected_sequence = u64::try_from(records.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| DelegationJournalError::Corrupt {
                line: line_number,
                reason: "sequence exhausted".to_owned(),
            })?;
        if event.sequence != expected_sequence {
            return Err(DelegationJournalError::Corrupt {
                line: line_number,
                reason: "sequence gap".to_owned(),
            });
        }
        let expected_previous = records
            .last()
            .map_or(ZERO_SHA256, |prior: &DelegationEvent| {
                prior.event_sha256.as_str()
            });
        if event.previous_event_sha256 != expected_previous {
            return Err(DelegationJournalError::Corrupt {
                line: line_number,
                reason: "hash-chain mismatch".to_owned(),
            });
        }
        records.push(event);
        offset = end + 1;
    }
    Ok((records, offset))
}

fn parse_event(line: &str, line_number: usize) -> Result<DelegationEvent, DelegationJournalError> {
    let value: Value =
        serde_json::from_str(line).map_err(|error| DelegationJournalError::Corrupt {
            line: line_number,
            reason: error.to_string(),
        })?;
    let object = value
        .as_object()
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line: line_number,
            reason: "record is not an object".to_owned(),
        })?;
    if object.len() != 19 || string_field(object, "schema", line_number)? != SCHEMA {
        return Err(DelegationJournalError::Corrupt {
            line: line_number,
            reason: "schema or field set mismatch".to_owned(),
        });
    }
    let sequence = decimal_field(object, "sequence", line_number)?;
    let phase_text = string_field(object, "phase", line_number)?;
    let phase =
        DelegationPhase::parse(phase_text).ok_or_else(|| DelegationJournalError::Corrupt {
            line: line_number,
            reason: "unknown phase".to_owned(),
        })?;
    let compute_units = u32::try_from(decimal_field(object, "compute_units", line_number)?)
        .map_err(|_| DelegationJournalError::Corrupt {
            line: line_number,
            reason: "compute_units out of range".to_owned(),
        })?;
    let identity = DelegationIdentity {
        job_id: string_field(object, "job_id", line_number)?.to_owned(),
        task_id: string_field(object, "task_id", line_number)?.to_owned(),
        coordinator_id: string_field(object, "coordinator_id", line_number)?.to_owned(),
        payload_sha256: string_field(object, "payload_sha256", line_number)?.to_owned(),
        request_sha256: string_field(object, "request_sha256", line_number)?.to_owned(),
    };
    identity
        .validate()
        .map_err(|_| DelegationJournalError::Corrupt {
            line: line_number,
            reason: "invalid identity".to_owned(),
        })?;
    let previous_event_sha256 =
        string_field(object, "previous_event_sha256", line_number)?.to_owned();
    let event_sha256 = string_field(object, "event_sha256", line_number)?.to_owned();
    if !is_sha256(&previous_event_sha256) || !is_sha256(&event_sha256) {
        return Err(DelegationJournalError::Corrupt {
            line: line_number,
            reason: "invalid event digest".to_owned(),
        });
    }
    let draft = DelegationEventDraft {
        identity: identity.clone(),
        budget: DelegationBudget {
            compute_units,
            memory_bytes: decimal_field(object, "memory_bytes", line_number)?,
        },
        phase,
        fencing_token: decimal_field(object, "fencing_token", line_number)?,
        worker_id: optional_string_field(object, "worker_id", line_number)?,
        reservation_sha256: optional_string_field(object, "reservation_sha256", line_number)?,
        result_sha256: optional_string_field(object, "result_sha256", line_number)?,
        worker_reply_sha256: optional_string_field(object, "worker_reply_sha256", line_number)?,
        timestamp_unix_ms: decimal_field(object, "timestamp_unix_ms", line_number)?,
        reason: optional_string_field(object, "reason", line_number)?,
    };
    if draft
        .reservation_sha256
        .as_deref()
        .is_some_and(|digest| !is_sha256(digest))
    {
        return Err(DelegationJournalError::Corrupt {
            line: line_number,
            reason: "invalid reservation digest".to_owned(),
        });
    }
    if draft
        .result_sha256
        .as_deref()
        .is_some_and(|digest| !is_sha256(digest))
        || draft
            .worker_reply_sha256
            .as_deref()
            .is_some_and(|digest| !is_sha256(digest))
    {
        return Err(DelegationJournalError::Corrupt {
            line: line_number,
            reason: "invalid sealed result digest".to_owned(),
        });
    }
    let actual =
        sha256_hex(canonical_hash_material(sequence, &draft, &previous_event_sha256).as_bytes());
    if actual != event_sha256 {
        return Err(DelegationJournalError::Corrupt {
            line: line_number,
            reason: "event digest mismatch".to_owned(),
        });
    }
    Ok(DelegationEvent {
        sequence,
        identity,
        budget: draft.budget,
        phase,
        fencing_token: draft.fencing_token,
        worker_id: draft.worker_id,
        reservation_sha256: draft.reservation_sha256,
        result_sha256: draft.result_sha256,
        worker_reply_sha256: draft.worker_reply_sha256,
        timestamp_unix_ms: draft.timestamp_unix_ms,
        reason: draft.reason,
        previous_event_sha256,
        event_sha256,
    })
}

fn string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    line: usize,
) -> Result<&'a str, DelegationJournalError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| DelegationJournalError::Corrupt {
            line,
            reason: format!("missing string field {key}"),
        })
}

fn optional_string_field(
    object: &serde_json::Map<String, Value>,
    key: &str,
    line: usize,
) -> Result<Option<String>, DelegationJournalError> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(DelegationJournalError::Corrupt {
            line,
            reason: format!("invalid optional string field {key}"),
        }),
    }
}

fn decimal_field(
    object: &serde_json::Map<String, Value>,
    key: &str,
    line: usize,
) -> Result<u64, DelegationJournalError> {
    let value = string_field(object, key, line)?;
    if value != "0"
        && (value.starts_with('0') || !value.as_bytes().iter().all(|byte| byte.is_ascii_digit()))
    {
        return Err(DelegationJournalError::Corrupt {
            line,
            reason: format!("non-canonical decimal field {key}"),
        });
    }
    value.parse().map_err(|_| DelegationJournalError::Corrupt {
        line,
        reason: format!("invalid decimal field {key}"),
    })
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization is infallible")
}

fn optional_json_string(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.as_bytes().iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
        }
    })
}

fn is_worker_id(value: &str) -> bool {
    value
        .parse::<u32>()
        .is_ok_and(|worker_id| worker_id > 0 && worker_id.to_string() == value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn identity(task: &str) -> DelegationIdentity {
        DelegationIdentity::new(
            "job-1",
            task,
            "coordinator-1",
            "11".repeat(32),
            "22".repeat(32),
        )
        .expect("identity")
    }

    fn budget() -> DelegationBudget {
        DelegationBudget::new(2, 4096).expect("budget")
    }

    fn accepted_authority(
        task: &str,
    ) -> (
        DurableDelegationAuthority<MemoryDelegationJournal>,
        DelegationToken,
    ) {
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("authority");
        let token = authority
            .prepare(identity(task), budget(), 100)
            .expect("prepare");
        authority.offered(&token, 101).expect("offer");
        authority
            .accepted(&token, "1", "33".repeat(32), 102)
            .expect("accept");
        (authority, token)
    }

    #[test]
    fn executable_permit_requires_durable_published_ack() {
        let (mut authority, token) = accepted_authority("publish-failure");
        authority.journal_mut().fail_next_append();
        assert!(matches!(
            authority.publish(&token, 103),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::InjectedFailure
            ))
        ));
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Accepted
        );

        let permit = authority.publish(&token, 104).expect("published permit");
        assert_eq!(permit.publication_sequence, 4);
        assert_eq!(permit.fencing_token, token.fencing_token);
        assert_eq!(authority.journal().records().len(), 4);
    }

    #[test]
    fn every_nonterminal_ack_failure_preserves_the_prior_authority_phase() {
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("authority");
        authority.journal_mut().fail_next_append();
        assert!(matches!(
            authority.prepare(identity("all-ack-points"), budget(), 0),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::InjectedFailure
            ))
        ));
        assert!(authority.journal().records().is_empty());
        let token = authority
            .prepare(identity("all-ack-points"), budget(), 0)
            .expect("prepare retry");
        assert_eq!(token.fencing_token, 1);

        authority.journal_mut().fail_next_append();
        assert!(authority.offered(&token, 1).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Prepared
        );
        authority.offered(&token, 1).expect("offer retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.accepted(&token, "1", "33".repeat(32), 2).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Offered
        );
        authority
            .accepted(&token, "1", "33".repeat(32), 2)
            .expect("accept retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.publish(&token, 3).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Accepted
        );
        authority.publish(&token, 3).expect("publish retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.running(&token, 4).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Published
        );
        authority.running(&token, 4).expect("running retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.heartbeat(&token, 30_003).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Running
        );
        assert!(authority.heartbeat(&token, 30_003).expect("renew retry"));

        authority.journal_mut().fail_next_append();
        assert!(authority
            .result_sealed(&token, "44".repeat(32), "55".repeat(32), 30_004)
            .is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Renewed
        );
        authority
            .result_sealed(&token, "44".repeat(32), "55".repeat(32), 30_004)
            .expect("sealed retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.result_applied(&token, 30_005).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::ResultSealed
        );
        authority
            .result_applied(&token, 30_005)
            .expect("applied retry");

        authority.journal_mut().fail_next_append();
        assert!(authority.complete(&token, 30_006).is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::ResultApplied
        );
        authority.complete(&token, 30_006).expect("complete retry");
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Completed
        );
    }

    #[test]
    fn revoked_terminal_is_durable_and_replayable() {
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("authority");
        let token = authority
            .prepare(identity("revoke"), budget(), 100)
            .expect("prepare");
        authority.journal_mut().fail_next_append();
        assert!(authority
            .revoke(&token, "coordinator shutdown", 101)
            .is_err());
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Prepared
        );
        authority
            .revoke(&token, "coordinator shutdown", 101)
            .expect("revoke retry");
        let recovered = DurableDelegationAuthority::recover(authority.journal().clone())
            .expect("recover revoked");
        assert_eq!(
            recovered.phase(&token).expect("phase"),
            DelegationPhase::Revoked
        );
    }

    #[test]
    fn canonical_prepared_event_matches_cross_runtime_kat() {
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("authority");
        authority
            .prepare(identity("kat"), budget(), 100)
            .expect("prepare");
        assert_eq!(
            authority.journal().records()[0].event_sha256,
            "2ebdba8445ca1d4d33fd71a0e4fed883555333c4d83201b89ea51f3c6963025a"
        );
    }

    #[test]
    fn offer_timeout_and_stale_fence_fail_closed() {
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("authority");
        let token = authority
            .prepare(identity("timeout"), budget(), 0)
            .expect("prepare");
        authority.offered(&token, 1).expect("offer");
        assert!(matches!(
            authority.accepted(
                &token,
                "1",
                "33".repeat(32),
                1 + OFFER_ACCEPT_TIMEOUT_MS + 1
            ),
            Err(DurableDelegationError::OfferExpired)
        ));
        let stale = DelegationToken {
            fencing_token: token.fencing_token + 1,
            ..token
        };
        assert!(matches!(
            authority.phase(&stale),
            Err(DurableDelegationError::StaleFence)
        ));
    }

    #[test]
    fn heartbeat_persists_at_thirty_seconds_and_expires_after_one_twenty() {
        let (mut authority, token) = accepted_authority("renew");
        authority.publish(&token, 200).expect("publish");
        authority.running(&token, 201).expect("running");
        assert!(!authority
            .heartbeat(&token, 29_999)
            .expect("volatile heartbeat"));
        assert!(authority
            .heartbeat(&token, 30_200)
            .expect("durable heartbeat"));
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Renewed
        );
        assert_eq!(
            authority
                .expire_stale(150_200)
                .expect("not expired at boundary"),
            0
        );
        assert_eq!(authority.expire_stale(150_201).expect("expired"), 1);
        assert_eq!(
            authority.phase(&token).expect("phase"),
            DelegationPhase::Expired
        );
    }

    #[test]
    fn complete_sequence_replays_with_same_terminal_state() {
        let (mut authority, token) = accepted_authority("complete");
        authority.publish(&token, 103).expect("publish");
        authority.running(&token, 104).expect("running");
        authority
            .result_sealed(&token, "44".repeat(32), "55".repeat(32), 105)
            .expect("sealed");
        authority.result_applied(&token, 106).expect("applied");
        authority.complete(&token, 107).expect("completed");
        let recovered = DurableDelegationAuthority::recover(authority.journal().clone())
            .expect("recovered authority");
        assert_eq!(
            recovered.phase(&token).expect("phase"),
            DelegationPhase::Completed
        );
        assert_eq!(recovered.retained_terminal_count(107), 1);
        assert_eq!(
            recovered.retained_terminal_count(107 + TERMINAL_TOMBSTONE_RETENTION_MS),
            1
        );
        assert_eq!(
            recovered.retained_terminal_count(108 + TERMINAL_TOMBSTONE_RETENTION_MS),
            0
        );
        assert_eq!(recovered.journal().records().len(), 8);
    }

    #[test]
    fn sealed_result_identity_is_idempotent_and_application_is_decided_once() {
        let (mut authority, token) = accepted_authority("result-dedupe");
        authority.publish(&token, 103).expect("publish");
        authority.running(&token, 104).expect("running");
        let result_sha256 = "44".repeat(32);
        let worker_reply_sha256 = "55".repeat(32);
        authority
            .result_sealed(
                &token,
                result_sha256.clone(),
                worker_reply_sha256.clone(),
                105,
            )
            .expect("seal");
        let sealed_event_count = authority.journal().records().len();
        authority
            .result_sealed(&token, result_sha256.clone(), worker_reply_sha256, 106)
            .expect("same sealed identity is idempotent");
        assert_eq!(authority.journal().records().len(), sealed_event_count);
        assert!(matches!(
            authority.result_sealed(&token, "66".repeat(32), "77".repeat(32), 106),
            Err(DurableDelegationError::ResultDigestMismatch)
        ));
        assert_eq!(
            authority
                .result_application_decision(&token, &result_sha256)
                .expect("application decision"),
            ResultApplicationDecision::ApplyOnce
        );
        authority.result_applied(&token, 107).expect("applied ACK");
        assert_eq!(
            authority
                .result_application_decision(&token, &result_sha256)
                .expect("dedupe decision"),
            ResultApplicationDecision::AlreadyApplied
        );
        let applied_event_count = authority.journal().records().len();
        authority
            .result_applied(&token, 108)
            .expect("duplicate applied ACK");
        assert_eq!(authority.journal().records().len(), applied_event_count);
    }

    #[test]
    fn unresolved_restart_is_quarantined_without_same_identity_resume() {
        let (mut authority, token) = accepted_authority("restart-quarantine");
        authority.publish(&token, 103).expect("publish");
        authority.running(&token, 104).expect("running");
        let journal = authority.journal().clone();
        assert!(matches!(
            DurableDelegationAuthority::recover(journal),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::Corrupt { line: 0, .. }
            ))
        ));
    }

    #[test]
    fn native_unresolved_restart_moves_journal_to_read_only_quarantine() {
        let root = unique_temp_dir("unresolved-restart");
        let path = root.join("journal.jsonl");
        {
            let journal = NativeJsonlDelegationJournal::open(&path).expect("open");
            let mut authority = DurableDelegationAuthority::recover(journal).expect("authority");
            authority
                .prepare(identity("native-unresolved"), budget(), 100)
                .expect("prepare");
        }
        let journal = NativeJsonlDelegationJournal::open(&path).expect("reopen live journal");
        assert!(matches!(
            DurableDelegationAuthority::recover(journal),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::Corrupt { line: 0, .. }
            ))
        ));
        assert!(!path.exists());
        assert!(quarantine_marker_path(&path).exists());
        assert!(matches!(
            NativeJsonlDelegationJournal::open(&path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn tombstone_compaction_is_strictly_after_retention_and_ack_atomic() {
        let (mut authority, token) = accepted_authority("compact");
        authority.publish(&token, 103).expect("publish");
        authority.running(&token, 104).expect("running");
        authority
            .result_sealed(&token, "44".repeat(32), "55".repeat(32), 105)
            .expect("sealed");
        authority.result_applied(&token, 106).expect("applied");
        authority.complete(&token, 107).expect("completed");

        assert!(!authority
            .compact_expired_terminal_tombstones(107 + TERMINAL_TOMBSTONE_RETENTION_MS)
            .expect("retained at boundary"));
        assert_eq!(authority.journal().records().len(), 8);

        authority.journal_mut().fail_next_reset();
        assert!(matches!(
            authority.compact_expired_terminal_tombstones(108 + TERMINAL_TOMBSTONE_RETENTION_MS),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::InjectedFailure
            ))
        ));
        assert_eq!(
            authority.phase(&token).expect("terminal state retained"),
            DelegationPhase::Completed
        );
        assert_eq!(authority.journal().records().len(), 8);

        assert!(authority
            .compact_expired_terminal_tombstones(108 + TERMINAL_TOMBSTONE_RETENTION_MS)
            .expect("durable reset"));
        assert!(authority.journal().records().is_empty());
        assert!(matches!(
            authority.phase(&token),
            Err(DurableDelegationError::UnknownDelegation)
        ));
        let reused = authority
            .prepare(
                identity("compact"),
                budget(),
                109 + TERMINAL_TOMBSTONE_RETENTION_MS,
            )
            .expect("identity becomes reusable only after durable expiry");
        assert_eq!(reused.fencing_token, 1);
    }

    #[test]
    fn tombstone_compaction_refuses_live_or_mixed_state_and_changed_head() {
        let (mut authority, token) = accepted_authority("live");
        authority.publish(&token, 103).expect("publish");
        assert!(!authority
            .compact_expired_terminal_tombstones(u64::MAX)
            .expect("live state is never compacted"));
        assert!(!authority.journal().records().is_empty());

        let mut journal = authority.journal().clone();
        let actual = journal
            .records()
            .last()
            .expect("journal head")
            .event_sha256
            .clone();
        assert!(matches!(
            journal.reset_if_head(&"ff".repeat(32)),
            Err(DelegationJournalError::HeadChanged {
                expected,
                actual: Some(found)
            }) if expected == "ff".repeat(32) && found == actual
        ));
        assert!(!journal.records().is_empty());
    }

    #[test]
    fn native_tombstone_compaction_syncs_an_empty_recoverable_journal() {
        let root = unique_temp_dir("native-compaction");
        let path = root.join("journal.jsonl");
        {
            let journal = NativeJsonlDelegationJournal::open(&path).expect("open");
            let mut authority = DurableDelegationAuthority::recover(journal).expect("authority");
            let token = authority
                .prepare(identity("native-compaction"), budget(), 100)
                .expect("prepare");
            authority.cancel(&token, 101).expect("cancel");
            assert!(authority
                .compact_expired_terminal_tombstones(102 + TERMINAL_TOMBSTONE_RETENTION_MS)
                .expect("compact"));
            assert_eq!(fs::metadata(&path).expect("metadata").len(), 0);
        }
        let recovered = NativeJsonlDelegationJournal::open(&path).expect("reopen empty journal");
        assert!(recovered.records().is_empty());
        DurableDelegationAuthority::recover(recovered).expect("recover reset journal");
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn native_job_journal_durably_binds_uuid_header_and_retains_it_on_reset() {
        let root = unique_temp_dir("job-header");
        let job_id = "018f0f25-6f8a-7c1d-9b20-8b85c4d9e001";
        let path;
        {
            let journal = NativeJsonlDelegationJournal::open_for_job(&root, job_id)
                .expect("open job journal");
            assert_eq!(journal.job_id(), Some(job_id));
            path = journal.path().to_path_buf();
            let mut authority = DurableDelegationAuthority::recover(journal).expect("authority");
            let mut owned_identity = identity("job-header");
            owned_identity.job_id = job_id.to_owned();
            let token = authority
                .prepare(owned_identity, budget(), 100)
                .expect("prepare");
            authority.cancel(&token, 101).expect("cancel");
            assert!(authority
                .compact_expired_terminal_tombstones(102 + TERMINAL_TOMBSTONE_RETENTION_MS,)
                .expect("compact"));
        }
        let expected_header = canonical_native_header(job_id);
        assert_eq!(
            fs::read(&path).expect("read header"),
            expected_header.as_bytes()
        );
        let reopened =
            NativeJsonlDelegationJournal::open_for_job(&root, job_id).expect("reopen job journal");
        assert!(reopened.records().is_empty());
        assert_eq!(reopened.job_id(), Some(job_id));
        let reopened_path = reopened.path().to_path_buf();
        drop(reopened);
        let recovered =
            NativeJsonlDelegationJournal::open_existing_job_for_recovery(&reopened_path)
                .expect("startup recovery reopens the header-bound identity");
        assert_eq!(recovered.job_id(), Some(job_id));
        assert!(recovered.records().is_empty());
        drop(recovered);
        assert!(matches!(
            NativeJsonlDelegationJournal::open_for_job(&root, "not-a-uuid"),
            Err(DelegationJournalError::Corrupt { line: 0, .. })
        ));
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn existing_job_recovery_quarantines_bad_header_and_honors_its_marker() {
        let root = unique_temp_dir("existing-job-bad-header");
        let path = root.join("bad.jsonl");
        fs::write(&path, b"{\"schema\":\"not-a-durable-header\"}\n")
            .expect("write bad durable header");

        assert!(matches!(
            NativeJsonlDelegationJournal::open_existing_job_for_recovery(&path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        assert!(!path.exists());
        assert!(quarantine_marker_path(&path).exists());
        assert!(matches!(
            NativeJsonlDelegationJournal::open_existing_job_for_recovery(&path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn semantic_recovery_failure_quarantines_native_journal() {
        let root = unique_temp_dir("semantic-quarantine");
        let path = root.join("journal.jsonl");
        let mut journal = MemoryDelegationJournal::default();
        let mut authority =
            DurableDelegationAuthority::recover(journal.clone()).expect("authority");
        authority
            .prepare(identity("semantic-corruption"), budget(), 100)
            .expect("prepare");
        journal = authority.journal().clone();
        let event = journal.records.get_mut(0).expect("prepared event");
        event.worker_id = Some("unexpected-worker".to_owned());
        event.reservation_sha256 = Some("33".repeat(32));
        let draft = DelegationEventDraft {
            identity: event.identity.clone(),
            budget: event.budget,
            phase: event.phase,
            fencing_token: event.fencing_token,
            worker_id: event.worker_id.clone(),
            reservation_sha256: event.reservation_sha256.clone(),
            result_sha256: event.result_sha256.clone(),
            worker_reply_sha256: event.worker_reply_sha256.clone(),
            timestamp_unix_ms: event.timestamp_unix_ms,
            reason: event.reason.clone(),
        };
        event.event_sha256 = sha256_hex(
            canonical_hash_material(event.sequence, &draft, &event.previous_event_sha256)
                .as_bytes(),
        );
        fs::write(
            &path,
            format!("{}\n", canonical_event_json(event)).as_bytes(),
        )
        .expect("write semantically corrupt journal");

        let native = NativeJsonlDelegationJournal::open(&path).expect("parse valid hash chain");
        assert!(matches!(
            DurableDelegationAuthority::recover(native),
            Err(DurableDelegationError::Journal(
                DelegationJournalError::Corrupt { .. }
            ))
        ));
        assert!(!path.exists());
        assert!(quarantine_marker_path(&path).exists());
        assert!(matches!(
            NativeJsonlDelegationJournal::open(&path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn native_jsonl_recovers_torn_final_append_and_rejects_middle_corruption() {
        let root = unique_temp_dir("jsonl");
        let path = root.join("journal.jsonl");
        {
            let journal = NativeJsonlDelegationJournal::open(&path).expect("open");
            let mut authority = DurableDelegationAuthority::recover(journal).expect("authority");
            let token = authority
                .prepare(identity("file"), budget(), 10)
                .expect("prepare");
            authority.offered(&token, 11).expect("offer");
        }
        let valid = fs::read(&path).expect("read valid journal");
        let valid_len = valid.len();
        {
            let mut append = OpenOptions::new().append(true).open(&path).expect("append");
            append
                .write_all(b"{\"schema\":")
                .expect("write torn append");
        }
        let recovered = NativeJsonlDelegationJournal::open(&path).expect("recover torn append");
        assert_eq!(recovered.records().len(), 2);
        assert_eq!(
            fs::metadata(&path).expect("metadata").len(),
            valid_len as u64
        );
        drop(recovered);

        let mut corrupt = valid;
        let first_payload = corrupt
            .iter()
            .position(|byte| *byte == b'1')
            .expect("digit in first line");
        corrupt[first_payload] = b'9';
        fs::write(&path, corrupt).expect("write corrupt journal");
        let quarantine = match NativeJsonlDelegationJournal::open(&path) {
            Err(DelegationJournalError::Quarantined { path, .. }) => path,
            other => panic!("expected quarantine, found {other:?}"),
        };
        assert!(!path.exists());
        assert!(quarantine.exists());
        assert!(quarantine_marker_path(&path).exists());
        assert!(matches!(
            NativeJsonlDelegationJournal::open(&path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    #[test]
    fn native_journal_allows_only_one_writer_for_a_job_path() {
        let root = unique_temp_dir("single-writer");
        let path = root.join("journal.jsonl");
        let first = NativeJsonlDelegationJournal::open(&path).expect("first writer");
        assert!(matches!(
            NativeJsonlDelegationJournal::open(&path),
            Err(DelegationJournalError::WriterAlreadyActive { path: locked }) if locked == path
        ));
        drop(first);
        NativeJsonlDelegationJournal::open(&path).expect("lock released with writer");
        fs::remove_dir_all(root).expect("remove temp journal");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "clearra-delegation-journal-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp directory");
        path
    }
}
