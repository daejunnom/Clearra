//! SRP rationale: this module's single change reason is one finite native curl
//! process owning one bounded set of immutable Range
//! transfers. It may reuse/multiplex connections, emits each completed span as
//! soon as curl finishes it, and knows no graph or product semantics.

#[cfg(not(feature = "native-pc4-libcurl"))]
use super::transport::{append_public_https_transfer, public_https_parallel_command};
use super::{
    hex,
    http_range::{content_range, HttpReply},
    Artifact, Result, FILES, REPOSITORY,
};
use std::{collections::BTreeMap, time::Duration};

#[cfg(not(feature = "native-pc4-libcurl"))]
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ExitStatus},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::JoinHandle,
};

#[cfg(feature = "native-pc4-libcurl")]
use curl::{
    easy::{Easy, HttpVersion},
    multi::{EasyHandle, Multi},
};
#[cfg(feature = "native-pc4-libcurl")]
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_LOGICAL: usize = 16;
#[cfg(feature = "native-pc4-libcurl")]
const MAX_ACTIVE_TRANSFERS: usize = 4;
const MAX_GAP_BYTES: u64 = 4_096;
const MAX_RANGE_BYTES: u64 = 65_536;
#[cfg(not(feature = "native-pc4-libcurl"))]
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

    #[cfg(feature = "native-pc4-libcurl")]
    fn logical_count(&self) -> usize {
        self.transfers
            .iter()
            .map(|transfer| transfer.projections.len())
            .sum()
    }

    #[cfg(not(feature = "native-pc4-libcurl"))]
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

    #[cfg(not(feature = "native-pc4-libcurl"))]
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

#[cfg(not(feature = "native-pc4-libcurl"))]
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

#[cfg(not(feature = "native-pc4-libcurl"))]
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
        validate_receipt(transfer, receipt.status, &receipt.content_range)?;
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

fn validate_receipt(transfer: &Transfer, status: u16, received_content_range: &str) -> Result<()> {
    match status {
        206 => {}
        200 => return Err("pc4_online_whole_content_rejected"),
        429 => return Err("pc4_online_rate_limited"),
        416 => return Err("pc4_online_range_unsatisfiable"),
        408 | 425 | 500 | 502 | 503 | 504 => return Err("pc4_online_dataset_unavailable"),
        _ => return Err("pc4_online_range_response_invalid"),
    }
    if received_content_range
        != content_range(transfer.offset, transfer.length, transfer.artifact.size)
    {
        return Err("pc4_online_content_range_mismatch");
    }
    Ok(())
}

/// One native online execution's connection owner for the opt-in HTTP/2 A/B.
///
/// CPU lookup continuations submit immutable byte demands; this owner keeps
/// one libcurl connection cache, admits the first completed transfer and lets
/// callers add more handles while older transfers are still running. It is
/// intentionally not enabled by default until build/runtime closure and the
/// full-search A/B have passed on both release targets.
#[cfg(feature = "native-pc4-libcurl")]
pub(super) struct NativeCurlPool {
    revision: String,
    multi: Multi,
    queued: VecDeque<LibcurlPending>,
    active: BTreeMap<usize, LibcurlActive>,
    identities: BTreeMap<(u64, u64), DemandIdentity>,
    next_token: usize,
    next_scalar_request_id: u64,
    observation: NativeCurlTransportObservation,
}

#[cfg(feature = "native-pc4-libcurl")]
struct LibcurlPending {
    easy: Easy,
    transfer: Transfer,
    collector: Arc<Mutex<LibcurlCollector>>,
}

#[cfg(feature = "native-pc4-libcurl")]
struct LibcurlActive {
    handle: EasyHandle,
    transfer: Transfer,
    collector: Arc<Mutex<LibcurlCollector>>,
}

#[cfg(feature = "native-pc4-libcurl")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct DemandIdentity {
    role: usize,
    path: &'static str,
    digest: String,
    offset: u64,
    length: u64,
}

#[cfg(feature = "native-pc4-libcurl")]
#[derive(Debug)]
struct LibcurlCollector {
    limit: usize,
    body: Vec<u8>,
    content_range: Option<String>,
    status_line: Option<String>,
    oversized: bool,
}

/// Monotonic, non-authoritative transport evidence for the opt-in live A/B.
///
/// A failed getinfo call is counted but never changes Range admission. These
/// counters therefore diagnose connection reuse without becoming product
/// authority or introducing a new search failure mode.
#[cfg(feature = "native-pc4-libcurl")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct NativeCurlTransportObservation {
    pub(super) completed_transfers: u64,
    pub(super) connection_samples: u64,
    pub(super) new_connections: u64,
    pub(super) reused_connection_transfers: u64,
    pub(super) http2_transfers: u64,
    pub(super) redirects: u64,
    pub(super) info_failures: u64,
    pub(super) name_lookup_time: Duration,
    pub(super) connect_time: Duration,
    pub(super) tls_time: Duration,
    pub(super) start_transfer_time: Duration,
    pub(super) total_time: Duration,
}

#[cfg(feature = "native-pc4-libcurl")]
impl NativeCurlTransportObservation {
    #[cfg(test)]
    pub(super) fn delta_since(self, previous: Self) -> Option<Self> {
        Some(Self {
            completed_transfers: self
                .completed_transfers
                .checked_sub(previous.completed_transfers)?,
            connection_samples: self
                .connection_samples
                .checked_sub(previous.connection_samples)?,
            new_connections: self.new_connections.checked_sub(previous.new_connections)?,
            reused_connection_transfers: self
                .reused_connection_transfers
                .checked_sub(previous.reused_connection_transfers)?,
            http2_transfers: self.http2_transfers.checked_sub(previous.http2_transfers)?,
            redirects: self.redirects.checked_sub(previous.redirects)?,
            info_failures: self.info_failures.checked_sub(previous.info_failures)?,
            name_lookup_time: self
                .name_lookup_time
                .checked_sub(previous.name_lookup_time)?,
            connect_time: self.connect_time.checked_sub(previous.connect_time)?,
            tls_time: self.tls_time.checked_sub(previous.tls_time)?,
            start_transfer_time: self
                .start_transfer_time
                .checked_sub(previous.start_transfer_time)?,
            total_time: self.total_time.checked_sub(previous.total_time)?,
        })
    }

    fn record(&mut self, sample: NativeCurlTransferObservation) {
        self.completed_transfers = self.completed_transfers.saturating_add(1);
        self.info_failures = self.info_failures.saturating_add(sample.info_failures);
        if let Some(new_connections) = sample.new_connections {
            self.connection_samples = self.connection_samples.saturating_add(1);
            self.new_connections = self.new_connections.saturating_add(new_connections);
            if new_connections == 0 {
                self.reused_connection_transfers =
                    self.reused_connection_transfers.saturating_add(1);
            }
        }
        self.http2_transfers = self.http2_transfers.saturating_add(u64::from(sample.http2));
        self.redirects = self
            .redirects
            .saturating_add(sample.redirects.unwrap_or_default());
        self.name_lookup_time = self
            .name_lookup_time
            .saturating_add(sample.name_lookup_time.unwrap_or_default());
        self.connect_time = self
            .connect_time
            .saturating_add(sample.connect_time.unwrap_or_default());
        self.tls_time = self
            .tls_time
            .saturating_add(sample.tls_time.unwrap_or_default());
        self.start_transfer_time = self
            .start_transfer_time
            .saturating_add(sample.start_transfer_time.unwrap_or_default());
        self.total_time = self
            .total_time
            .saturating_add(sample.total_time.unwrap_or_default());
    }
}

#[cfg(feature = "native-pc4-libcurl")]
#[derive(Clone, Copy, Debug, Default)]
struct NativeCurlTransferObservation {
    new_connections: Option<u64>,
    redirects: Option<u64>,
    http2: bool,
    info_failures: u64,
    name_lookup_time: Option<Duration>,
    connect_time: Option<Duration>,
    tls_time: Option<Duration>,
    start_transfer_time: Option<Duration>,
    total_time: Option<Duration>,
}

#[cfg(feature = "native-pc4-libcurl")]
impl NativeCurlTransferObservation {
    fn read(handle: &EasyHandle, final_status_line: Option<&str>) -> Self {
        let mut info_failures = 0;
        let new_connections = observe_count(handle.num_connects(), &mut info_failures);
        let redirects = observe_count(handle.redirect_count(), &mut info_failures);
        let name_lookup_time = observe_duration(handle.namelookup_time(), &mut info_failures);
        let connect_time = observe_duration(handle.connect_time(), &mut info_failures);
        let tls_time = observe_duration(handle.appconnect_time(), &mut info_failures);
        let start_transfer_time = observe_duration(handle.starttransfer_time(), &mut info_failures);
        let total_time = observe_duration(handle.total_time(), &mut info_failures);
        Self {
            new_connections,
            redirects,
            http2: final_status_line.is_some_and(|line| line.starts_with("HTTP/2 ")),
            info_failures,
            name_lookup_time,
            connect_time,
            tls_time,
            start_transfer_time,
            total_time,
        }
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn observe_count<T: Into<u64>>(
    value: std::result::Result<T, curl::Error>,
    failures: &mut u64,
) -> Option<u64> {
    match value {
        Ok(value) => Some(value.into()),
        Err(_) => {
            *failures = failures.saturating_add(1);
            None
        }
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn observe_duration(
    value: std::result::Result<Duration, curl::Error>,
    failures: &mut u64,
) -> Option<Duration> {
    match value {
        Ok(value) => Some(value),
        Err(_) => {
            *failures = failures.saturating_add(1);
            None
        }
    }
}

#[cfg(feature = "native-pc4-libcurl")]
impl NativeCurlPool {
    pub fn new(revision: &str) -> Result<Self> {
        if !hex(revision, 40) {
            return Err("pc4_online_identity_invalid");
        }
        curl::init();
        let mut multi = Multi::new();
        multi
            .pipelining(false, true)
            .and_then(|_| multi.set_max_host_connections(MAX_ACTIVE_TRANSFERS))
            .and_then(|_| multi.set_max_total_connections(MAX_ACTIVE_TRANSFERS))
            .and_then(|_| multi.set_max_connects(MAX_ACTIVE_TRANSFERS))
            .and_then(|_| multi.set_max_concurrent_streams(MAX_ACTIVE_TRANSFERS))
            .map_err(|_| "pc4_online_transport_unavailable")?;
        Ok(Self {
            revision: revision.to_owned(),
            multi,
            queued: VecDeque::new(),
            active: BTreeMap::new(),
            identities: BTreeMap::new(),
            next_token: 1,
            next_scalar_request_id: 1,
            observation: NativeCurlTransportObservation::default(),
        })
    }

    #[cfg(test)]
    pub(super) const fn observation(&self) -> NativeCurlTransportObservation {
        self.observation
    }

    /// Performs a bounded qualification read through this pool before the
    /// cooperative search starts. Keeping the same multi owner alive lets the
    /// first graph demand and later scalar index misses reuse the
    /// DNS/TCP/TLS/ALPN state established here. Graph work is still drained
    /// before a scalar miss is admitted so this synchronous helper cannot
    /// starve already-ready graph transfers.
    pub fn fetch_exact(
        &mut self,
        role: usize,
        artifact: &Artifact,
        offset: u64,
        length: u64,
    ) -> Result<HttpReply> {
        let mut replies = self.fetch_many_exact(vec![(role, artifact.clone(), offset, length)])?;
        replies.pop().ok_or("pc4_online_response_receipt_invalid")
    }

    /// Fetches one qualification dependency stage through the same bounded
    /// HTTP/2 pool. All immutable demands are validated and submitted before
    /// waiting, so six sampled index/graph reads cost one staged wait rather
    /// than six serialized network round trips. Returned replies preserve the
    /// caller's logical order even when transfers complete out of order.
    pub fn fetch_many_exact(
        &mut self,
        demands: Vec<(usize, Artifact, u64, u64)>,
    ) -> Result<Vec<HttpReply>> {
        if self.is_active() {
            return Err("pc4_online_transport_busy");
        }
        if demands.is_empty() || demands.len() > MAX_LOGICAL {
            return Err("pc4_online_batch_invalid");
        }
        let request_count = u64::try_from(demands.len()).map_err(|_| "pc4_online_batch_invalid")?;
        let first_request_id = self.next_scalar_request_id;
        self.next_scalar_request_id = self
            .next_scalar_request_id
            .checked_add(request_count)
            .filter(|next| *next != 0)
            .ok_or("pc4_online_transport_unavailable")?;
        let mut request_ids = Vec::with_capacity(demands.len());
        let mut planned = Vec::with_capacity(demands.len());
        for (index, (role, artifact, offset, length)) in demands.into_iter().enumerate() {
            let request_id = first_request_id
                .checked_add(u64::try_from(index).map_err(|_| "pc4_online_batch_invalid")?)
                .ok_or("pc4_online_transport_unavailable")?;
            request_ids.push(request_id);
            planned.push(NativeRangeDemand {
                role,
                lookup_session: u64::MAX,
                request_id,
                artifact,
                offset,
                length,
            });
        }
        let plan = self
            .plan_fresh(planned)?
            .ok_or("pc4_online_pending_missing")?;
        self.submit(plan)?;
        let mut completed = BTreeMap::new();
        loop {
            match self.poll(true)? {
                NativeCurlPoll::Admissions(admissions) => {
                    for admission in admissions {
                        if admission.lookup_session != u64::MAX
                            || !request_ids.contains(&admission.request_id)
                            || completed.insert(admission.request_id, admission).is_some()
                        {
                            return Err("pc4_online_response_receipt_invalid");
                        }
                    }
                    if completed.len() == request_ids.len() {
                        let mut replies = Vec::with_capacity(request_ids.len());
                        for request_id in &request_ids {
                            let admission = completed
                                .remove(request_id)
                                .ok_or("pc4_online_response_receipt_invalid")?;
                            replies.push(HttpReply {
                                status: 206,
                                content_range: content_range(
                                    admission.offset,
                                    admission.length,
                                    admission.total,
                                ),
                                bytes: admission.bytes,
                            });
                        }
                        return Ok(replies);
                    }
                }
                NativeCurlPoll::Pending => {}
                NativeCurlPoll::Finished => {
                    return Err("pc4_online_response_receipt_invalid");
                }
            }
        }
    }

    pub fn plan_fresh(&self, demands: Vec<NativeRangeDemand>) -> Result<Option<NativeCurlPlan>> {
        let mut fresh = Vec::new();
        let mut snapshot = BTreeMap::new();
        for demand in demands {
            let key = (demand.lookup_session, demand.request_id);
            let identity = demand_identity(&demand);
            if let Some(prior) = self.identities.get(&key).or_else(|| snapshot.get(&key)) {
                if prior != &identity {
                    return Err("pc4_online_pending_identity_changed");
                }
                continue;
            }
            snapshot.insert(key, identity);
            fresh.push(demand);
        }
        if fresh.is_empty() {
            return Ok(None);
        }
        if self.identities.len().saturating_add(fresh.len()) > MAX_LOGICAL {
            return Err("pc4_online_pending_limit");
        }
        NativeCurlPlan::new(fresh).map(Some)
    }

    pub fn submit(&mut self, plan: NativeCurlPlan) -> Result<()> {
        if self.identities.len().saturating_add(plan.logical_count()) > MAX_LOGICAL {
            return Err("pc4_online_pending_limit");
        }

        // Configure every easy handle before mutating the live multi owner.
        // A configuration error therefore cannot partially publish a batch.
        let mut configured = VecDeque::with_capacity(plan.transfers.len());
        for transfer in plan.transfers {
            let collector = Arc::new(Mutex::new(LibcurlCollector {
                limit: usize::try_from(transfer.length)
                    .map_err(|_| "pc4_online_response_too_large")?,
                body: Vec::with_capacity(
                    usize::try_from(transfer.length)
                        .map_err(|_| "pc4_online_response_too_large")?,
                ),
                content_range: None,
                status_line: None,
                oversized: false,
            }));
            let easy = configure_easy(&self.revision, &transfer, Arc::clone(&collector))?;
            configured.push_back(LibcurlPending {
                easy,
                transfer,
                collector,
            });
        }
        for pending in &configured {
            for projection in &pending.transfer.projections {
                self.identities.insert(
                    (projection.lookup_session, projection.request_id),
                    DemandIdentity {
                        role: pending.transfer.role,
                        path: pending.transfer.artifact.path,
                        digest: pending.transfer.artifact.digest.clone(),
                        offset: projection.offset,
                        length: projection.length,
                    },
                );
            }
        }
        self.queued.append(&mut configured);
        self.fill_slots()?;
        Ok(())
    }

    fn fill_slots(&mut self) -> Result<()> {
        while self.active.len() < MAX_ACTIVE_TRANSFERS {
            let Some(pending) = self.queued.pop_front() else {
                break;
            };
            let token = self.next_token;
            self.next_token = self
                .next_token
                .checked_add(1)
                .filter(|next| *next != 0)
                .ok_or("pc4_online_transport_unavailable")?;
            let mut handle = self
                .multi
                .add(pending.easy)
                .map_err(|_| "pc4_online_transport_unavailable")?;
            if handle.set_token(token).is_err() {
                let _ = self.multi.remove(handle);
                return Err("pc4_online_transport_unavailable");
            }
            self.active.insert(
                token,
                LibcurlActive {
                    handle,
                    transfer: pending.transfer,
                    collector: pending.collector,
                },
            );
        }
        self.multi
            .perform()
            .map_err(|_| "pc4_online_transport_unavailable")?;
        Ok(())
    }

    pub fn poll(&mut self, wait: bool) -> Result<NativeCurlPoll> {
        self.multi
            .perform()
            .map_err(|_| "pc4_online_transport_interrupted")?;
        let mut completed = self.completed_messages();
        if completed.is_empty() && wait && !self.active.is_empty() {
            self.multi
                .wait(&mut [], Duration::from_millis(10))
                .map_err(|_| "pc4_online_transport_interrupted")?;
            self.multi
                .perform()
                .map_err(|_| "pc4_online_transport_interrupted")?;
            completed = self.completed_messages();
        }

        let mut admissions = Vec::new();
        for (token, transfer_result) in completed {
            let active = self
                .active
                .remove(&token)
                .ok_or("pc4_online_response_receipt_invalid")?;
            let status = active
                .handle
                .response_code()
                .ok()
                .and_then(|status| u16::try_from(status).ok())
                .ok_or("pc4_online_response_receipt_invalid")?;
            let effective_https = active
                .handle
                .effective_url()
                .ok()
                .flatten()
                .is_some_and(|url| url.starts_with("https://"));
            let collector = active
                .collector
                .lock()
                .map_err(|_| "pc4_online_transport_interrupted")?;
            let oversized = collector.oversized;
            let status_line = collector.status_line.clone();
            let status_line_valid = collector
                .status_line
                .as_deref()
                .is_some_and(|line| line.starts_with("HTTP/") && line.len() <= 64);
            let content_range = collector.content_range.clone().unwrap_or_default();
            let body = collector.body.clone();
            drop(collector);
            let observation =
                NativeCurlTransferObservation::read(&active.handle, status_line.as_deref());
            let _easy = self
                .multi
                .remove(active.handle)
                .map_err(|_| "pc4_online_transport_interrupted")?;
            for projection in &active.transfer.projections {
                self.identities
                    .remove(&(projection.lookup_session, projection.request_id));
            }
            if oversized {
                return Err("pc4_online_response_too_large");
            }
            if transfer_result.is_err() {
                return Err("pc4_online_transport_interrupted");
            }
            if !effective_https || !status_line_valid {
                return Err("pc4_online_range_response_invalid");
            }
            validate_receipt(&active.transfer, status, &content_range)?;
            let bytes = HttpReply {
                status,
                content_range,
                bytes: body,
            }
            .validate(
                &active.transfer.artifact,
                active.transfer.offset,
                active.transfer.length,
            )?;
            self.observation.record(observation);
            for projection in &active.transfer.projections {
                let begin = usize::try_from(projection.offset - active.transfer.offset)
                    .map_err(|_| "pc4_online_response_too_large")?;
                let length = usize::try_from(projection.length)
                    .map_err(|_| "pc4_online_response_too_large")?;
                admissions.push(NativeRangeAdmission {
                    lookup_session: projection.lookup_session,
                    request_id: projection.request_id,
                    offset: projection.offset,
                    length: projection.length,
                    total: active.transfer.artifact.size,
                    bytes: bytes[begin..begin + length].to_vec(),
                });
            }
        }
        self.fill_slots()?;
        if !admissions.is_empty() {
            Ok(NativeCurlPoll::Admissions(admissions))
        } else if self.active.is_empty() && self.queued.is_empty() {
            Ok(NativeCurlPoll::Finished)
        } else {
            Ok(NativeCurlPoll::Pending)
        }
    }

    pub fn is_active(&self) -> bool {
        !self.active.is_empty() || !self.queued.is_empty()
    }

    fn completed_messages(&self) -> Vec<(usize, std::result::Result<(), curl::Error>)> {
        let mut completed = Vec::new();
        self.multi.messages(|message| {
            if let (Ok(token), Some(result)) = (message.token(), message.result()) {
                completed.push((token, result));
            }
        });
        completed
    }

    fn rollback_added(&mut self, tokens: &[usize]) {
        for token in tokens.iter().rev() {
            if let Some(active) = self.active.remove(token) {
                for projection in &active.transfer.projections {
                    self.identities
                        .remove(&(projection.lookup_session, projection.request_id));
                }
                let _ = self.multi.remove(active.handle);
            }
        }
    }
}

#[cfg(feature = "native-pc4-libcurl")]
impl Drop for NativeCurlPool {
    fn drop(&mut self) {
        self.queued.clear();
        let tokens = self.active.keys().copied().collect::<Vec<_>>();
        self.rollback_added(&tokens);
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn demand_identity(demand: &NativeRangeDemand) -> DemandIdentity {
    DemandIdentity {
        role: demand.role,
        path: demand.artifact.path,
        digest: demand.artifact.digest.clone(),
        offset: demand.offset,
        length: demand.length,
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn configure_easy(
    revision: &str,
    transfer: &Transfer,
    collector: Arc<Mutex<LibcurlCollector>>,
) -> Result<Easy> {
    let url = format!(
        "https://huggingface.co/datasets/{REPOSITORY}/resolve/{revision}/{}",
        transfer.artifact.path
    );
    let mut easy = Easy::new();
    easy.url(&url)
        .and_then(|_| easy.get(true))
        .and_then(|_| easy.follow_location(true))
        .and_then(|_| easy.unrestricted_auth(false))
        .and_then(|_| easy.max_redirections(5))
        .and_then(|_| easy.connect_timeout(Duration::from_secs(30)))
        .and_then(|_| easy.timeout(Duration::from_secs(30)))
        .and_then(|_| easy.low_speed_limit(1))
        .and_then(|_| easy.low_speed_time(Duration::from_secs(60)))
        .and_then(|_| easy.max_filesize(transfer.length))
        // Connection reuse is the latency policy. TCP keepalive is only a
        // dead-peer detector for an already-idle socket; it is not an
        // application heartbeat and never creates speculative Range traffic.
        .and_then(|_| easy.tcp_keepalive(true))
        .and_then(|_| easy.tcp_keepidle(Duration::from_secs(60)))
        .and_then(|_| easy.tcp_keepintvl(Duration::from_secs(30)))
        .and_then(|_| easy.maxage_conn(Duration::from_secs(300)))
        .and_then(|_| {
            easy.range(&format!(
                "{}-{}",
                transfer.offset,
                transfer.offset + transfer.length - 1
            ))
        })
        .and_then(|_| easy.http_version(HttpVersion::V2TLS))
        .and_then(|_| easy.pipewait(true))
        .and_then(|_| easy.http_09_allowed(false))
        .map_err(|_| "pc4_online_transport_unavailable")?;

    let body = Arc::clone(&collector);
    easy.write_function(move |data| {
        let Ok(mut state) = body.lock() else {
            return Ok(0);
        };
        Ok(collect_libcurl_body(&mut state, data))
    })
    .map_err(|_| "pc4_online_transport_unavailable")?;
    easy.header_function(move |data| {
        let Ok(mut state) = collector.lock() else {
            return false;
        };
        collect_libcurl_header(&mut state, data)
    })
    .map_err(|_| "pc4_online_transport_unavailable")?;
    Ok(easy)
}

#[cfg(feature = "native-pc4-libcurl")]
fn collect_libcurl_body(state: &mut LibcurlCollector, data: &[u8]) -> usize {
    if state.body.len().saturating_add(data.len()) > state.limit {
        state.oversized = true;
        return 0;
    }
    state.body.extend_from_slice(data);
    data.len()
}

#[cfg(feature = "native-pc4-libcurl")]
fn collect_libcurl_header(state: &mut LibcurlCollector, data: &[u8]) -> bool {
    if data.starts_with(b"HTTP/") {
        // Redirect and proxy header blocks precede the final Range response.
        // Only the body and Content-Range of the last status block are valid.
        state.body.clear();
        state.content_range = None;
        state.oversized = false;
        state.status_line = std::str::from_utf8(data)
            .ok()
            .map(|line| line.trim().to_owned());
    } else if let Some(separator) = data.iter().position(|byte| *byte == b':') {
        let (name, value) = data.split_at(separator);
        let value = &value[1..];
        if name.eq_ignore_ascii_case(b"content-range") && value.len() <= 130 {
            state.content_range = std::str::from_utf8(value)
                .ok()
                .map(|value| value.trim().to_owned());
        }
    }
    true
}

#[cfg(not(feature = "native-pc4-libcurl"))]
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

#[cfg(not(feature = "native-pc4-libcurl"))]
enum ReaderEvent {
    Receipt(Result<Receipt>),
    Finished,
}

#[cfg(not(feature = "native-pc4-libcurl"))]
struct Receipt {
    index: usize,
    status: u16,
    content_range: String,
}

#[cfg(not(feature = "native-pc4-libcurl"))]
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

#[cfg(not(feature = "native-pc4-libcurl"))]
struct Scratch {
    directory: PathBuf,
    body_paths: Vec<PathBuf>,
}

#[cfg(not(feature = "native-pc4-libcurl"))]
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

#[cfg(not(feature = "native-pc4-libcurl"))]
impl Drop for Scratch {
    fn drop(&mut self) {
        for path in &self.body_paths {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(all(not(feature = "native-pc4-libcurl"), unix))]
fn restrict_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "pc4_online_transport_unavailable")
}

#[cfg(all(not(feature = "native-pc4-libcurl"), not(unix)))]
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
    #[cfg(not(feature = "native-pc4-libcurl"))]
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
        let valid = content_range(100, 12, 131_072);
        assert!(validate_receipt(transfer, 206, &valid).is_ok());
        assert_eq!(
            validate_receipt(transfer, 200, "").unwrap_err(),
            "pc4_online_whole_content_rejected"
        );
        assert_eq!(
            validate_receipt(transfer, 429, "").unwrap_err(),
            "pc4_online_rate_limited"
        );
        assert_eq!(
            validate_receipt(transfer, 416, "").unwrap_err(),
            "pc4_online_range_unsatisfiable"
        );
        for status in [408, 425, 500, 502, 503, 504] {
            assert_eq!(
                validate_receipt(transfer, status, "").unwrap_err(),
                "pc4_online_dataset_unavailable"
            );
        }
        assert_eq!(
            validate_receipt(transfer, 206, "bytes 99-110/131072").unwrap_err(),
            "pc4_online_content_range_mismatch"
        );
    }

    #[test]
    #[cfg(not(feature = "native-pc4-libcurl"))]
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

    #[cfg(feature = "native-pc4-libcurl")]
    #[test]
    fn native_pool_collector_keeps_only_the_final_bounded_response() {
        let mut collector = LibcurlCollector {
            limit: 12,
            body: Vec::new(),
            content_range: None,
            status_line: None,
            oversized: false,
        };
        assert!(collect_libcurl_header(
            &mut collector,
            b"HTTP/1.1 302 Found\r\n"
        ));
        assert_eq!(collect_libcurl_body(&mut collector, b"redirect"), 8);
        assert!(collect_libcurl_header(&mut collector, b"HTTP/2 206\r\n"));
        assert!(collect_libcurl_header(
            &mut collector,
            b"Content-Range: bytes 100-111/131072\r\n"
        ));
        assert_eq!(collect_libcurl_body(&mut collector, b"abcdefghijkl"), 12);
        assert_eq!(collector.body, b"abcdefghijkl");
        assert_eq!(
            collector.content_range.as_deref(),
            Some("bytes 100-111/131072")
        );
        assert_eq!(collector.status_line.as_deref(), Some("HTTP/2 206"));
        assert!(!collector.oversized);
        assert_eq!(collect_libcurl_body(&mut collector, b"x"), 0);
        assert!(collector.oversized);
    }

    #[cfg(feature = "native-pc4-libcurl")]
    #[test]
    fn native_pool_filters_exact_inflight_identity_and_rejects_aliases() {
        let mut pool = NativeCurlPool::new(&"a".repeat(40)).unwrap();
        let known = demand(2, 7, 100, 12);
        pool.identities.insert(
            (known.lookup_session, known.request_id),
            demand_identity(&known),
        );
        assert!(pool.plan_fresh(vec![known.clone()]).unwrap().is_none());

        let mut changed = known;
        changed.offset += 1;
        assert_eq!(
            pool.plan_fresh(vec![changed]).unwrap_err(),
            "pc4_online_pending_identity_changed"
        );
    }

    #[cfg(feature = "native-pc4-libcurl")]
    #[test]
    fn native_pool_observation_delta_is_checked_and_counts_reuse() {
        let before = NativeCurlTransportObservation::default();
        let mut total = before;
        total.record(NativeCurlTransferObservation {
            new_connections: Some(1),
            redirects: Some(1),
            http2: true,
            name_lookup_time: Some(Duration::from_millis(2)),
            connect_time: Some(Duration::from_millis(5)),
            tls_time: Some(Duration::from_millis(8)),
            start_transfer_time: Some(Duration::from_millis(12)),
            total_time: Some(Duration::from_millis(15)),
            ..NativeCurlTransferObservation::default()
        });
        let after_qualification = total;
        total.record(NativeCurlTransferObservation {
            new_connections: Some(0),
            redirects: Some(1),
            http2: true,
            total_time: Some(Duration::from_millis(4)),
            ..NativeCurlTransferObservation::default()
        });

        let qualification = after_qualification.delta_since(before).unwrap();
        assert_eq!(qualification.completed_transfers, 1);
        assert_eq!(qualification.new_connections, 1);
        assert_eq!(qualification.reused_connection_transfers, 0);
        assert_eq!(qualification.http2_transfers, 1);
        assert_eq!(qualification.total_time, Duration::from_millis(15));

        let graph = total.delta_since(after_qualification).unwrap();
        assert_eq!(graph.completed_transfers, 1);
        assert_eq!(graph.connection_samples, 1);
        assert_eq!(graph.new_connections, 0);
        assert_eq!(graph.reused_connection_transfers, 1);
        assert_eq!(graph.http2_transfers, 1);
        assert_eq!(graph.total_time, Duration::from_millis(4));
        assert!(before.delta_since(total).is_none());
    }

    /// Explicit public-HF diagnostic. It is ignored by every ordinary test and
    /// is run only by the isolated non-publishing workflow with an opt-in env.
    #[cfg(feature = "native-pc4-libcurl")]
    #[test]
    #[ignore = "explicit live public-HF connection-reuse A/B"]
    fn native_pool_live_qualification_reuses_connection_for_graph_range() {
        assert_eq!(
            std::env::var("CLEARRA_PC4_HTTP_LIVE_AB").as_deref(),
            Ok("1"),
            "live HTTP A/B requires an explicit non-publishing opt-in"
        );
        let (revision, files) = super::super::transport::discover().unwrap();
        let mut pool = NativeCurlPool::new(&revision).unwrap();
        let before = pool.observation();
        let generation =
            super::super::format::qualify_with_reader_many(&revision, &files, |demands| {
                pool.fetch_many_exact(
                    demands
                        .iter()
                        .map(|demand| {
                            (
                                demand.role,
                                files[demand.role].clone(),
                                demand.offset,
                                demand.length as u64,
                            )
                        })
                        .collect(),
                )
                .map(|replies| replies.into_iter().map(|reply| reply.bytes).collect())
            })
            .unwrap();
        assert_eq!(generation["schema"], "clearra.pc4.host-generation.v1");
        let qualified = pool
            .observation()
            .delta_since(before)
            .expect("qualification observation is monotonic");

        let graph = &files[2];
        let graph_offset = 65_536_u64.min(graph.size.saturating_sub(1));
        let graph_length = 4_096_u64.min(graph.size - graph_offset);
        let before_graph = pool.observation();
        let reply = pool
            .fetch_exact(2, graph, graph_offset, graph_length)
            .unwrap();
        assert_eq!(reply.bytes.len() as u64, graph_length);
        let graph_phase = pool
            .observation()
            .delta_since(before_graph)
            .expect("graph observation is monotonic");

        println!(
            "pc4_http_live_ab revision={revision} phase=qualification transfers={} connection_samples={} new_connections={} reused={} http2={} redirects={} info_failures={} dns_ms={:.3} connect_ms={:.3} tls_ms={:.3} ttfb_ms={:.3} total_ms={:.3}",
            qualified.completed_transfers,
            qualified.connection_samples,
            qualified.new_connections,
            qualified.reused_connection_transfers,
            qualified.http2_transfers,
            qualified.redirects,
            qualified.info_failures,
            qualified.name_lookup_time.as_secs_f64() * 1_000.0,
            qualified.connect_time.as_secs_f64() * 1_000.0,
            qualified.tls_time.as_secs_f64() * 1_000.0,
            qualified.start_transfer_time.as_secs_f64() * 1_000.0,
            qualified.total_time.as_secs_f64() * 1_000.0,
        );
        println!(
            "pc4_http_live_ab revision={revision} phase=graph transfers={} connection_samples={} new_connections={} reused={} http2={} redirects={} info_failures={} dns_ms={:.3} connect_ms={:.3} tls_ms={:.3} ttfb_ms={:.3} total_ms={:.3}",
            graph_phase.completed_transfers,
            graph_phase.connection_samples,
            graph_phase.new_connections,
            graph_phase.reused_connection_transfers,
            graph_phase.http2_transfers,
            graph_phase.redirects,
            graph_phase.info_failures,
            graph_phase.name_lookup_time.as_secs_f64() * 1_000.0,
            graph_phase.connect_time.as_secs_f64() * 1_000.0,
            graph_phase.tls_time.as_secs_f64() * 1_000.0,
            graph_phase.start_transfer_time.as_secs_f64() * 1_000.0,
            graph_phase.total_time.as_secs_f64() * 1_000.0,
        );

        assert!(qualified.completed_transfers > 1);
        assert!(qualified.new_connections >= 1);
        assert_eq!(graph_phase.completed_transfers, 1);
        assert_eq!(graph_phase.connection_samples, 1);
        assert_eq!(graph_phase.new_connections, 0);
        assert_eq!(graph_phase.reused_connection_transfers, 1);
        assert_eq!(graph_phase.http2_transfers, 1);
        assert_eq!(graph_phase.info_failures, 0);
    }
}
