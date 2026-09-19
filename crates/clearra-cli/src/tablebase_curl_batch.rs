//! SRP: one finite native curl process owns one bounded set of immutable Range
//! transfers. It may reuse/multiplex connections, emits each completed span as
//! soon as curl finishes it, and knows no graph or product semantics.

use super::{
    hex,
    http_range::{content_range, HttpReply},
    transport::{append_public_https_transfer, public_https_parallel_command},
    Artifact, Result, FILES, REPOSITORY,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ExitStatus},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::JoinHandle,
    time::Duration,
};

const MAX_LOGICAL: usize = 16;
const MAX_GAP_BYTES: u64 = 4_096;
const MAX_RANGE_BYTES: u64 = 65_536;
const RECEIPT_PREFIX: &str = "CLEARRA-PC4-HTTP-V1";

#[derive(Clone, Debug)]
pub(super) struct NativeRangeDemand {
    pub(super) role: usize,
    pub(super) lookup_session: u64,
    pub(super) request_id: u64,
    pub(super) artifact: Artifact,
    pub(super) offset: u64,
    pub(super) length: u64,
}

pub(super) struct NativeRangeAdmission {
    pub(super) lookup_session: u64,
    pub(super) request_id: u64,
    pub(super) offset: u64,
    pub(super) length: u64,
    pub(super) total: u64,
    pub(super) bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Projection {
    lookup_session: u64,
    request_id: u64,
    offset: u64,
    length: u64,
}

#[derive(Clone, Debug)]
struct Transfer {
    role: usize,
    artifact: Artifact,
    offset: u64,
    length: u64,
    projections: Vec<Projection>,
}

#[derive(Debug)]
pub(super) struct NativeCurlPlan {
    transfers: Vec<Transfer>,
}

impl NativeCurlPlan {
    pub fn new(demands: Vec<NativeRangeDemand>) -> Result<Self> {
        if demands.is_empty() || demands.len() > MAX_LOGICAL {
            return Err("pc4_online_batch_invalid");
        }
        let mut by_role = BTreeMap::<usize, Vec<NativeRangeDemand>>::new();
        for demand in demands {
            if demand.role >= FILES.len()
                || demand.artifact.path != FILES[demand.role]
                || demand.length == 0
                || demand.length > MAX_RANGE_BYTES
                || demand
                    .offset
                    .checked_add(demand.length)
                    .is_none_or(|end| end > demand.artifact.size)
                || demand.lookup_session == 0
                || demand.request_id == 0
            {
                return Err("pc4_online_batch_invalid");
            }
            by_role.entry(demand.role).or_default().push(demand);
        }

        let mut transfers: Vec<Transfer> = Vec::new();
        for (role, mut demands) in by_role {
            demands.sort_unstable_by_key(|d| (d.offset, d.length, d.lookup_session, d.request_id));
            let first = demands.first().ok_or("pc4_online_batch_invalid")?;
            if demands.iter().any(|d| {
                d.artifact.path != first.artifact.path
                    || d.artifact.size != first.artifact.size
                    || d.artifact.digest != first.artifact.digest
            }) {
                return Err("pc4_online_artifact_identity_mismatch");
            }
            for demand in demands {
                let end = demand.offset + demand.length;
                let append = transfers.last_mut().filter(|transfer| {
                    transfer.role == role
                        && transfer.artifact.path == demand.artifact.path
                        && demand.offset
                            <= transfer
                                .offset
                                .saturating_add(transfer.length)
                                .saturating_add(MAX_GAP_BYTES)
                        && end.max(transfer.offset + transfer.length) - transfer.offset
                            <= MAX_RANGE_BYTES
                });
                let projection = Projection {
                    lookup_session: demand.lookup_session,
                    request_id: demand.request_id,
                    offset: demand.offset,
                    length: demand.length,
                };
                if let Some(transfer) = append {
                    transfer.length = end.max(transfer.offset + transfer.length) - transfer.offset;
                    transfer.projections.push(projection);
                } else {
                    transfers.push(Transfer {
                        role,
                        artifact: demand.artifact,
                        offset: demand.offset,
                        length: demand.length,
                        projections: vec![projection],
                    });
                }
            }
        }
        if transfers.is_empty() || transfers.len() > MAX_LOGICAL {
            return Err("pc4_online_batch_invalid");
        }
        Ok(Self { transfers })
    }

    pub fn reservations(&self) -> Vec<(usize, u64, u64)> {
        self.transfers
            .iter()
            .map(|transfer| (transfer.role, transfer.offset, transfer.length))
            .collect()
    }

    pub fn spawn(self, revision: &str) -> Result<NativeCurlBatch> {
        if !hex(revision, 40) {
            return Err("pc4_online_identity_invalid");
        }
        let scratch = Scratch::create(self.transfers.len())?;
        let mut command = self.command(revision, &scratch)?;
        let mut child = command
            .spawn()
            .map_err(|_| "pc4_online_transport_unavailable")?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("pc4_online_transport_unavailable");
            }
        };
        let transfer_count = self.transfers.len();
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let event = line
                    .map_err(|_| "pc4_online_transport_interrupted")
                    .and_then(|line| parse_receipt(&line, transfer_count));
                let failed = event.is_err();
                if sender.send(ReaderEvent::Receipt(event)).is_err() || failed {
                    return;
                }
            }
            let _ = sender.send(ReaderEvent::Finished);
        });
        Ok(NativeCurlBatch {
            child,
            reader: Some(reader),
            receiver,
            scratch,
            transfers: self.transfers,
            completed: vec![false; transfer_count],
            remaining: transfer_count,
            reader_finished: false,
            exit: None,
        })
    }

    fn command(&self, revision: &str, scratch: &Scratch) -> Result<std::process::Command> {
        if !hex(revision, 40) || scratch.body_paths.len() != self.transfers.len() {
            return Err("pc4_online_identity_invalid");
        }
        let mut command = public_https_parallel_command();
        for (index, transfer) in self.transfers.iter().enumerate() {
            if index != 0 {
                command.arg("--next");
            }
            let url = format!(
                "https://huggingface.co/datasets/{REPOSITORY}/resolve/{revision}/{}",
                transfer.artifact.path
            );
            append_public_https_transfer(&mut command, &url, transfer.length, 30)?;
            command
                .arg("--range")
                .arg(format!(
                    "{}-{}",
                    transfer.offset,
                    transfer.offset + transfer.length - 1
                ))
                .arg("--output")
                .arg(&scratch.body_paths[index])
                .arg("--write-out")
                .arg(format!(
                    "{RECEIPT_PREFIX}|{index}|%{{http_code}}|%header{{content-range}}\n"
                ));
        }
        Ok(command)
    }
}

pub(super) enum NativeCurlPoll {
    Pending,
    Admissions(Vec<NativeRangeAdmission>),
    Finished,
}

pub(super) struct NativeCurlBatch {
    child: Child,
    reader: Option<JoinHandle<()>>,
    receiver: Receiver<ReaderEvent>,
    scratch: Scratch,
    transfers: Vec<Transfer>,
    completed: Vec<bool>,
    remaining: usize,
    reader_finished: bool,
    exit: Option<ExitStatus>,
}

impl NativeCurlBatch {
    pub fn poll(&mut self, wait: bool) -> Result<NativeCurlPoll> {
        let event = if wait {
            match self.receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => Some(event),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.reader_finished = true;
                    None
                }
            }
        } else {
            match self.receiver.try_recv() {
                Ok(event) => Some(event),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    self.reader_finished = true;
                    None
                }
            }
        };
        if let Some(event) = event {
            match event {
                ReaderEvent::Receipt(result) => {
                    let receipt = result?;
                    return self.complete(receipt).map(NativeCurlPoll::Admissions);
                }
                ReaderEvent::Finished => self.reader_finished = true,
            }
        }
        if self.exit.is_none() {
            self.exit = self
                .child
                .try_wait()
                .map_err(|_| "pc4_online_transport_interrupted")?;
        }
        if let Some(status) = self.exit.as_ref() {
            if !status.success() {
                // `try_wait` may observe process exit just before the stdout
                // reader publishes a final HTTP receipt. Let the reader drain
                // first so 200/429/416 keep their precise fail-closed reason.
                if !self.reader_finished {
                    return Ok(NativeCurlPoll::Pending);
                }
                return Err("pc4_online_transport_interrupted");
            }
            if self.reader_finished && self.remaining != 0 {
                return Err("pc4_online_response_receipt_missing");
            }
            if self.reader_finished && self.remaining == 0 {
                return Ok(NativeCurlPoll::Finished);
            }
        }
        Ok(NativeCurlPoll::Pending)
    }

    fn complete(&mut self, receipt: Receipt) -> Result<Vec<NativeRangeAdmission>> {
        if receipt.index >= self.transfers.len() || self.completed[receipt.index] {
            return Err("pc4_online_response_receipt_invalid");
        }
        let transfer = &self.transfers[receipt.index];
        validate_receipt(transfer, &receipt)?;
        let body_path = &self.scratch.body_paths[receipt.index];
        let metadata =
            fs::symlink_metadata(body_path).map_err(|_| "pc4_online_transport_interrupted")?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("pc4_online_transport_interrupted");
        }
        if metadata.len() > transfer.length {
            return Err("pc4_online_response_too_large");
        }
        if metadata.len() < transfer.length {
            return Err("pc4_online_truncated_range");
        }
        let bytes = fs::read(body_path).map_err(|_| "pc4_online_transport_interrupted")?;
        let bytes = HttpReply {
            status: receipt.status,
            content_range: receipt.content_range,
            bytes,
        }
        .validate(&transfer.artifact, transfer.offset, transfer.length)?;
        let mut admissions = Vec::with_capacity(transfer.projections.len());
        for projection in &transfer.projections {
            let begin = usize::try_from(projection.offset - transfer.offset)
                .map_err(|_| "pc4_online_response_too_large")?;
            let length =
                usize::try_from(projection.length).map_err(|_| "pc4_online_response_too_large")?;
            admissions.push(NativeRangeAdmission {
                lookup_session: projection.lookup_session,
                request_id: projection.request_id,
                offset: projection.offset,
                length: projection.length,
                total: transfer.artifact.size,
                bytes: bytes[begin..begin + length].to_vec(),
            });
        }
        self.completed[receipt.index] = true;
        self.remaining -= 1;
        let _ = fs::remove_file(body_path);
        Ok(admissions)
    }
}

fn validate_receipt(transfer: &Transfer, receipt: &Receipt) -> Result<()> {
    match receipt.status {
        206 => {}
        200 => return Err("pc4_online_whole_content_rejected"),
        429 => return Err("pc4_online_rate_limited"),
        416 => return Err("pc4_online_range_unsatisfiable"),
        _ => return Err("pc4_online_range_response_invalid"),
    }
    if receipt.content_range
        != content_range(transfer.offset, transfer.length, transfer.artifact.size)
    {
        return Err("pc4_online_content_range_mismatch");
    }
    Ok(())
}

impl Drop for NativeCurlBatch {
    fn drop(&mut self) {
        if self.exit.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

enum ReaderEvent {
    Receipt(Result<Receipt>),
    Finished,
}

struct Receipt {
    index: usize,
    status: u16,
    content_range: String,
}

fn parse_receipt(line: &str, transfer_count: usize) -> Result<Receipt> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut fields = line.split('|');
    if fields.next() != Some(RECEIPT_PREFIX) {
        return Err("pc4_online_response_receipt_invalid");
    }
    let index = fields
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index < transfer_count)
        .ok_or("pc4_online_response_receipt_invalid")?;
    let status = fields
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or("pc4_online_response_receipt_invalid")?;
    let content_range = fields
        .next()
        .filter(|value| value.len() <= 128 && value.is_ascii())
        .ok_or("pc4_online_response_receipt_invalid")?;
    if fields.next().is_some() {
        return Err("pc4_online_response_receipt_invalid");
    }
    Ok(Receipt {
        index,
        status,
        content_range: content_range.to_owned(),
    })
}

struct Scratch {
    directory: PathBuf,
    body_paths: Vec<PathBuf>,
}

impl Scratch {
    fn create(count: usize) -> Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..4 {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random).map_err(|_| "pc4_online_transport_unavailable")?;
            let name = format!(
                "clearra-pc4-http-{}",
                random
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            let directory = base.join(name);
            match fs::create_dir(&directory) {
                Ok(()) => {
                    if restrict_directory(&directory).is_err() {
                        let _ = fs::remove_dir(&directory);
                        return Err("pc4_online_transport_unavailable");
                    }
                    let body_paths = (0..count)
                        .map(|index| directory.join(format!("body-{index}.bin")))
                        .collect();
                    return Ok(Self {
                        directory,
                        body_paths,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("pc4_online_transport_unavailable"),
            }
        }
        Err("pc4_online_transport_unavailable")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for path in &self.body_paths {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "pc4_online_transport_unavailable")
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(role: usize) -> Artifact {
        Artifact {
            path: FILES[role],
            size: 131_072,
            digest: format!("{role:x}").repeat(64),
        }
    }

    fn demand(role: usize, session: u64, offset: u64, length: u64) -> NativeRangeDemand {
        NativeRangeDemand {
            role,
            lookup_session: session,
            request_id: 1,
            artifact: artifact(role),
            offset,
            length,
        }
    }

    #[test]
    fn native_batch_merges_only_bounded_ranges_from_one_immutable_artifact() {
        let plan = NativeCurlPlan::new(vec![
            demand(2, 1, 100, 12),
            demand(2, 2, 124, 12),
            demand(2, 3, 70_000, 12),
            demand(1, 4, 100, 8),
        ])
        .unwrap();
        assert_eq!(
            plan.reservations(),
            vec![(1, 100, 8), (2, 100, 36), (2, 70_000, 12)]
        );
        assert_eq!(plan.transfers[1].projections.len(), 2);
        assert!(NativeCurlPlan::new(Vec::new()).is_err());
        assert!(NativeCurlPlan::new(vec![demand(2, 1, 0, 12); 17]).is_err());
    }

    #[test]
    fn native_batch_rejects_cross_generation_aliases_and_oversized_spans() {
        let mut changed = demand(2, 2, 120, 12);
        changed.artifact.digest = "f".repeat(64);
        assert_eq!(
            NativeCurlPlan::new(vec![demand(2, 1, 100, 12), changed]).unwrap_err(),
            "pc4_online_artifact_identity_mismatch"
        );
        let mut invalid = demand(2, 1, 0, 12);
        invalid.length = 65_537;
        assert_eq!(
            NativeCurlPlan::new(vec![invalid]).unwrap_err(),
            "pc4_online_batch_invalid"
        );
    }

    #[test]
    fn native_batch_receipts_are_indexed_and_strict() {
        let receipt = parse_receipt("CLEARRA-PC4-HTTP-V1|2|206|bytes 100-111/131072\r", 3).unwrap();
        assert_eq!((receipt.index, receipt.status), (2, 206));
        assert_eq!(receipt.content_range, "bytes 100-111/131072");
        for value in [
            "bad|2|206|bytes 100-111/131072",
            "CLEARRA-PC4-HTTP-V1|3|206|bytes 100-111/131072",
            "CLEARRA-PC4-HTTP-V1|2|bad|bytes 100-111/131072",
            "CLEARRA-PC4-HTTP-V1|2|206|range|extra",
        ] {
            assert!(parse_receipt(value, 3).is_err());
        }
    }

    #[test]
    fn native_batch_classifies_http_status_before_opening_a_body() {
        let plan = NativeCurlPlan::new(vec![demand(2, 1, 100, 12)]).unwrap();
        let transfer = &plan.transfers[0];
        let receipt = |status, range: &str| Receipt {
            index: 0,
            status,
            content_range: range.to_owned(),
        };
        let valid = content_range(100, 12, 131_072);
        assert!(validate_receipt(transfer, &receipt(206, &valid)).is_ok());
        assert_eq!(
            validate_receipt(transfer, &receipt(200, "")).unwrap_err(),
            "pc4_online_whole_content_rejected"
        );
        assert_eq!(
            validate_receipt(transfer, &receipt(429, "")).unwrap_err(),
            "pc4_online_rate_limited"
        );
        assert_eq!(
            validate_receipt(transfer, &receipt(416, "")).unwrap_err(),
            "pc4_online_range_unsatisfiable"
        );
        assert_eq!(
            validate_receipt(transfer, &receipt(206, "bytes 99-110/131072")).unwrap_err(),
            "pc4_online_content_range_mismatch"
        );
    }

    #[test]
    fn native_batch_command_repeats_secure_local_options_without_retry_or_http3() {
        let plan =
            NativeCurlPlan::new(vec![demand(2, 1, 0, 12), demand(2, 2, 70_000, 12)]).unwrap();
        let scratch = Scratch::create(plan.transfers.len()).unwrap();
        let revision = "a".repeat(40);
        let command = plan.command(&revision, &scratch).unwrap();
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(arguments.first().map(String::as_str), Some("-q"));
        assert_eq!(
            arguments.iter().filter(|value| *value == "--url").count(),
            2
        );
        assert_eq!(
            arguments
                .iter()
                .filter(|value| *value == "--write-out")
                .count(),
            2
        );
        assert_eq!(
            arguments
                .iter()
                .filter(|value| *value == "--no-buffer")
                .count(),
            2
        );
        assert_eq!(
            arguments.iter().filter(|value| *value == "--next").count(),
            1
        );
        assert!(arguments
            .windows(2)
            .any(|pair| pair[0] == "--parallel-max" && pair[1] == "4"));
        assert!(!arguments
            .iter()
            .any(|value| matches!(value.as_str(), "--retry" | "--http3" | "--config")));
        assert_eq!(
            arguments
                .iter()
                .filter(|value| value.contains(&format!("/resolve/{revision}/graph.bin")))
                .count(),
            2
        );
    }
}
