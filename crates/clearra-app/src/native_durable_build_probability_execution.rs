// SRP rationale: this module has one behavior-level change reason: enforcing the admitted, crash-fenced durable native Build-probability delegation protocol.

//! Provider-admitted, crash-fenced native Build delegation.
//!
//! This module is a child of `native_build_probability_execution` so it can
//! reuse the actual producer, verifier, merger, memory projection, and final
//! materialization seam without creating a second nominal backend.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{with_compute_coordinator, with_compute_worker},
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_core_executor::resource::{
    DelegationBudget, DelegationIdentity, DelegationJournal, DelegationPhase, DelegationToken,
    DurableDelegationAuthority, ExecutableDelegationPermit, ResultApplicationDecision,
    WORKER_HEARTBEAT_INTERVAL_MS,
};
use clearra_core_executor::{
    canonical_wasm_candidate_packet_batch_sha256, CoreExecutionError, CoreExecutionResult,
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedVerifier,
    WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuSearchError,
    WasmDistributedGeometrySummary,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildSolutionProbabilityPolicy,
    FinesseMetric, FinessePatternKnowledge, SearchProblem,
};
use sha2::{Digest, Sha256};

use super::{
    checked_heap_backing_bytes, finish_native_build_probability, join_workers,
    native_coordinator_allocation_unavailable, native_worker_thread_name, run_native_build_compute,
    NativeBuildProbabilityExecutionOutput, NativeBuildProbabilityWorkerOutput,
    NativeCoordinatorMemoryProjection, NATIVE_BUILD_BATCH_CAPACITY,
    NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY, NATIVE_BUILD_INITIAL_BATCH_CAPACITY,
    NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY, NATIVE_BUILD_WORKER_STACK_BYTES,
};
use crate::app_services::AppCoreExecutorService;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBuildProbabilityAdmissionRequest {
    worker_count: usize,
    maximum_task_count: usize,
    worker_stack_bytes: u128,
    request_channel_capacity: usize,
    completion_channel_capacity: usize,
    candidate_batch_capacity: usize,
    known_coordinator_peak_bytes: u128,
    minimum_channel_control_bytes: u128,
    minimum_batch_owner_peak_bytes: u128,
    minimum_result_owner_peak_bytes: u128,
}

impl NativeBuildProbabilityAdmissionRequest {
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    pub const fn worker_stack_bytes(self) -> u128 {
        self.worker_stack_bytes
    }

    pub const fn maximum_task_count(self) -> usize {
        self.maximum_task_count
    }

    pub const fn request_channel_capacity(self) -> usize {
        self.request_channel_capacity
    }

    pub const fn completion_channel_capacity(self) -> usize {
        self.completion_channel_capacity
    }

    pub const fn candidate_batch_capacity(self) -> usize {
        self.candidate_batch_capacity
    }

    pub const fn known_coordinator_peak_bytes(self) -> u128 {
        self.known_coordinator_peak_bytes
    }

    pub const fn minimum_channel_control_bytes(self) -> u128 {
        self.minimum_channel_control_bytes
    }

    pub const fn minimum_batch_owner_peak_bytes(self) -> u128 {
        self.minimum_batch_owner_peak_bytes
    }

    pub const fn minimum_result_owner_peak_bytes(self) -> u128 {
        self.minimum_result_owner_peak_bytes
    }
}

/// Provider-observed native allocations which cannot be authorized from Rust
/// container projections alone. Each category is kept separate so a host
/// cannot satisfy the contract with one unrelated non-zero scalar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBuildProbabilityProviderMeasurement {
    worker_stack_bytes: u128,
    channel_control_bytes: u128,
    batch_owner_peak_bytes: u128,
    result_owner_peak_bytes: u128,
}

impl NativeBuildProbabilityProviderMeasurement {
    pub const fn new(
        worker_stack_bytes: u128,
        channel_control_bytes: u128,
        batch_owner_peak_bytes: u128,
        result_owner_peak_bytes: u128,
    ) -> Option<Self> {
        if worker_stack_bytes == 0
            || channel_control_bytes == 0
            || batch_owner_peak_bytes == 0
            || result_owner_peak_bytes == 0
        {
            return None;
        }
        Some(Self {
            worker_stack_bytes,
            channel_control_bytes,
            batch_owner_peak_bytes,
            result_owner_peak_bytes,
        })
    }

    pub const fn worker_stack_bytes(self) -> u128 {
        self.worker_stack_bytes
    }

    pub const fn channel_control_bytes(self) -> u128 {
        self.channel_control_bytes
    }

    pub const fn batch_owner_peak_bytes(self) -> u128 {
        self.batch_owner_peak_bytes
    }

    pub const fn result_owner_peak_bytes(self) -> u128 {
        self.result_owner_peak_bytes
    }

    fn validates(self, request: NativeBuildProbabilityAdmissionRequest) -> bool {
        self.worker_stack_bytes >= request.worker_stack_bytes
            && self.channel_control_bytes >= request.minimum_channel_control_bytes
            && self.batch_owner_peak_bytes >= request.minimum_batch_owner_peak_bytes
            && self.result_owner_peak_bytes >= request.minimum_result_owner_peak_bytes
            && self.checked_peak_bytes().is_some()
    }

    fn checked_peak_bytes(self) -> Option<u128> {
        self.worker_stack_bytes
            .checked_add(self.channel_control_bytes)?
            .checked_add(
                if self.batch_owner_peak_bytes > self.result_owner_peak_bytes {
                    self.batch_owner_peak_bytes
                } else {
                    self.result_owner_peak_bytes
                },
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBuildProbabilityHostProviderError {
    component: &'static str,
}

impl NativeBuildProbabilityHostProviderError {
    pub const fn new(component: &'static str) -> Self {
        Self { component }
    }

    pub const fn component(self) -> &'static str {
        self.component
    }
}

/// Non-clone provider authority retained until every native worker is joined.
#[derive(Debug)]
pub(crate) struct NativeBuildProbabilityAdmissionAuthority {
    worker_count: usize,
    maximum_task_count: usize,
    known_coordinator_peak_bytes: u128,
    provider_measurement: NativeBuildProbabilityProviderMeasurement,
    admitted_peak_bytes: u128,
}

impl NativeBuildProbabilityAdmissionAuthority {
    pub(crate) fn from_provider_measurement(
        request: NativeBuildProbabilityAdmissionRequest,
        provider_measurement: NativeBuildProbabilityProviderMeasurement,
    ) -> Option<Self> {
        if request.worker_count == 0 || !provider_measurement.validates(request) {
            return None;
        }
        let admitted_peak_bytes = request
            .known_coordinator_peak_bytes
            .checked_add(provider_measurement.checked_peak_bytes()?)?;
        Some(Self {
            worker_count: request.worker_count,
            maximum_task_count: request.maximum_task_count,
            known_coordinator_peak_bytes: request.known_coordinator_peak_bytes,
            provider_measurement,
            admitted_peak_bytes,
        })
    }

    fn validates(&self, request: NativeBuildProbabilityAdmissionRequest) -> bool {
        self.worker_count == request.worker_count
            && self.maximum_task_count == request.maximum_task_count
            && self.known_coordinator_peak_bytes == request.known_coordinator_peak_bytes
            && self.provider_measurement.validates(request)
            && self.admitted_peak_bytes
                == self.known_coordinator_peak_bytes.saturating_add(
                    self.provider_measurement
                        .checked_peak_bytes()
                        .unwrap_or(u128::MAX),
                )
    }

    const fn admitted_peak_bytes(&self) -> u128 {
        self.admitted_peak_bytes
    }
}

pub trait NativeBuildProbabilityAdmissionProvider: Send + Sync {
    fn admit_native_build_probability(
        &self,
        request: NativeBuildProbabilityAdmissionRequest,
    ) -> Result<NativeBuildProbabilityProviderMeasurement, NativeBuildProbabilityHostProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeBuildProbabilityDurableIdentity {
    job_id: String,
    coordinator_id: String,
    request_sha256: String,
}

impl NativeBuildProbabilityDurableIdentity {
    pub(crate) fn new(
        job_id: impl Into<String>,
        source_commit: impl Into<String>,
        boot_uuid: impl Into<String>,
        request_sha256: impl Into<String>,
    ) -> Result<Self, CoreExecutionError> {
        let job_id = job_id.into();
        let source_commit = source_commit.into();
        let boot_uuid = boot_uuid.into();
        let request_sha256 = request_sha256.into();
        if !is_canonical_uuid(&job_id)
            || !is_canonical_uuid(&boot_uuid)
            || !is_lower_hex(&source_commit, 40)
            || !is_lower_hex(&request_sha256, 64)
        {
            return Err(durable_runtime_unavailable(
                "native_build_probability_durable_identity_invalid",
            ));
        }
        Ok(Self {
            job_id,
            coordinator_id: format!("source-commit:{source_commit};boot:{boot_uuid}"),
            request_sha256,
        })
    }

    fn task_id(&self, ordinal: u64, operation: DurableWorkerOperationKind) -> String {
        format!("{}:{ordinal}:{}", self.job_id, operation.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableWorkerOperationKind {
    Initialize,
    Consume,
    Finish,
}

impl DurableWorkerOperationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Consume => "consume",
            Self::Finish => "finish",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct DurableWorkerTaskKey {
    task_id_sha256: [u8; 32],
    fencing_token: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DurableWorkerOffer {
    key: DurableWorkerTaskKey,
    worker_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompactExecutablePermit {
    job_id_sha256: [u8; 32],
    task_id_sha256: [u8; 32],
    worker_id: u32,
    payload_sha256: [u8; 32],
    request_sha256: [u8; 32],
    fencing_token: u64,
    publication_sequence: u64,
    publication_sha256: [u8; 32],
    expires_at_unix_ms: u64,
}

// The worker consumes each executable exactly once; keeping verifier state inline
// avoids heap indirection in the durable queue's hot hand-off path.
#[allow(clippy::large_enum_variant)]
enum DurableWorkerExecutable {
    Initialize(WasmBuildProbabilityDistributedVerifier),
    Consume(Vec<WasmCandidatePacket>),
    Finish,
}

impl DurableWorkerExecutable {
    const fn kind(&self) -> DurableWorkerOperationKind {
        match self {
            Self::Initialize(_) => DurableWorkerOperationKind::Initialize,
            Self::Consume(_) => DurableWorkerOperationKind::Consume,
            Self::Finish => DurableWorkerOperationKind::Finish,
        }
    }
}

// Requests are short-lived ownership messages consumed by one worker. Their
// inline representation avoids allocating each durable queue operation.
#[allow(clippy::large_enum_variant)]
enum DurableWorkerRequest {
    Produce {
        producer: Box<WasmBuildProbabilityCandidateProducer>,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        initialize: bool,
        batch_capacity: usize,
    },
    Offer(DurableWorkerOffer),
    Publish {
        permit: CompactExecutablePermit,
        observed_now_unix_ms: u64,
        executable: DurableWorkerExecutable,
    },
    Run(DurableWorkerTaskKey),
}

enum DurableWorkerResult {
    OfferAccepted(DurableWorkerTaskKey),
    Started(DurableWorkerTaskKey),
    Initialized(DurableWorkerTaskKey),
    Consumed(DurableWorkerTaskKey, usize),
    Finished(DurableWorkerTaskKey, Vec<CoreExecutionResult>),
}

struct DurableWorkerCompletion {
    worker_index: usize,
    result: Result<DurableWorkerResult, &'static str>,
}

struct ProducedBuildWork {
    producer: Box<WasmBuildProbabilityCandidateProducer>,
    verifier: Option<WasmBuildProbabilityDistributedVerifier>,
    batch: Vec<WasmCandidatePacket>,
    summary: Option<WasmDistributedGeometrySummary>,
}

// A completion transfers its owners through a rendezvous, never an unbounded
// result queue. The verifier is moved, not cloned, into its durable Initialize.
#[allow(clippy::large_enum_variant)]
enum NativeBuildPoolCompletion {
    Delegated(DurableWorkerCompletion),
    Produced {
        worker_index: usize,
        result: Result<ProducedBuildWork, CoreExecutionError>,
    },
}

struct PendingDurableDelegation {
    worker_index: usize,
    worker_id: u32,
    operation: DurableWorkerOperationKind,
    token: DelegationToken,
    payload_sha256: String,
    executable: Option<DurableWorkerExecutable>,
}

struct SealedDurableDelegation {
    token: DelegationToken,
    result_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableWorkerStage {
    Offered,
    Published,
    Running,
}

struct ActiveDurableDelegation {
    pending: PendingDurableDelegation,
    stage: DurableWorkerStage,
    candidate_count: usize,
    last_heartbeat_unix_ms: u64,
}

#[derive(Default)]
struct NativeBuildPoolWorker {
    initialized: bool,
    producing: bool,
    finished: bool,
    consumed_batches: usize,
    delegation: Option<ActiveDurableDelegation>,
    queued_batch: Option<Vec<WasmCandidatePacket>>,
}

impl NativeBuildPoolWorker {
    fn is_ready(&self) -> bool {
        !self.producing && !self.finished && self.delegation.is_none()
    }

    fn next_batch_capacity(&self) -> usize {
        NATIVE_BUILD_INITIAL_BATCH_CAPACITY
            .checked_shl(self.consumed_batches.min(3) as u32)
            .unwrap_or(NATIVE_BUILD_BATCH_CAPACITY)
            .min(NATIVE_BUILD_BATCH_CAPACITY)
    }
}

fn durable_runtime_unavailable(component: &'static str) -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable { component }
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_canonical_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

pub(crate) trait NativeDurableClock {
    fn now_unix_ms(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemNativeDurableClock;

impl NativeDurableClock for SystemNativeDurableClock {
    fn now_unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .unwrap_or(u64::MAX)
    }
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_bytes(hasher, value.as_bytes());
}

fn finish_sha256(hasher: Sha256) -> [u8; 32] {
    let digest = hasher.finalize();
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    output
}

fn sha256_text(value: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, value);
    finish_sha256(hasher)
}

fn hex_sha256(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn parse_sha256(value: &str) -> Option<[u8; 32]> {
    if !is_lower_hex(value, 64) {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hash_tiling_identity(hasher: &mut Sha256, identity: StandardBoard64TilingIdentity) {
    hasher.update(identity.initial_board_mask().to_le_bytes());
    hasher.update((identity.placement_count() as u64).to_le_bytes());
    hasher.update(identity.packed_piece_codes().to_le_bytes());
    for mask in identity.placement_masks() {
        hasher.update(mask.to_le_bytes());
    }
}

/// Canonical merger-input seal. This deliberately hashes only typed values
/// consumed by the shared distributed merger, in their producer order.
fn canonical_merger_result_sha256(results: &[CoreExecutionResult]) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.merger-result.v1");
    hasher.update((results.len() as u64).to_le_bytes());
    for result in results {
        hasher.update((result.summary_field_count() as u64).to_le_bytes());
        for (key, value) in result.summary_field_entries() {
            hash_text(&mut hasher, key);
            hash_text(&mut hasher, value);
        }
        hasher.update((result.path_steps().len() as u64).to_le_bytes());
        for step in result.path_steps() {
            hasher.update([step.piece().as_ascii() as u8, step.rotation()]);
            hasher.update(step.x().to_le_bytes());
            hasher.update(step.y().to_le_bytes());
            hash_text(&mut hasher, step.hold());
            hasher.update([step.cleared_lines()]);
        }
        hasher.update((result.normalized_solution_identities().len() as u64).to_le_bytes());
        for identity in result.normalized_solution_identities() {
            hash_tiling_identity(&mut hasher, *identity);
        }
        hasher.update((result.normalized_solution_keys().len() as u64).to_le_bytes());
        for key in result.normalized_solution_keys() {
            hash_text(&mut hasher, key);
        }
        match result.representative_solution_identity() {
            Some(identity) => {
                hasher.update([1]);
                hash_tiling_identity(&mut hasher, identity);
            }
            None => hasher.update([0]),
        }
        hasher.update((result.coverage_pattern_words().len() as u64).to_le_bytes());
        for word in result.coverage_pattern_words() {
            hasher.update(word.to_le_bytes());
        }
        hasher.update((result.solution_coverages().len() as u64).to_le_bytes());
        for coverage in result.solution_coverages() {
            hash_tiling_identity(&mut hasher, coverage.identity());
            hasher.update((coverage.covered_patterns().pattern_count() as u64).to_le_bytes());
            for word in coverage.covered_patterns().words() {
                hasher.update(word.to_le_bytes());
            }
        }
        hasher.update((result.normalized_solution_coverages().len() as u64).to_le_bytes());
        for coverage in result.normalized_solution_coverages() {
            hash_text(&mut hasher, coverage.solution_key());
            hasher.update((coverage.covered_patterns().pattern_count() as u64).to_le_bytes());
            for word in coverage.covered_patterns().words() {
                hasher.update(word.to_le_bytes());
            }
        }
        hasher.update((result.postprocess_score_cells().len() as u64).to_le_bytes());
        hasher.update([u8::from(result.postprocess_score_cells_complete())]);
        match result.postprocess_score_profile_id() {
            Some(profile) => {
                hasher.update([1]);
                hash_text(&mut hasher, profile);
            }
            None => hasher.update([0]),
        }
        for cell in result.postprocess_score_cells() {
            hash_tiling_identity(&mut hasher, cell.candidate_identity());
            hasher.update((cell.pattern_id() as u64).to_le_bytes());
            hash_text(&mut hasher, cell.trace_identity());
            hasher.update(cell.score().to_le_bytes());
            hasher.update(cell.attack().to_le_bytes());
        }
        hasher.update((result.postprocess_spin_coverages().len() as u64).to_le_bytes());
        for coverage in result.postprocess_spin_coverages() {
            hash_text(&mut hasher, coverage.target_id());
            hasher.update((coverage.pass_index() as u64).to_le_bytes());
            hasher.update((coverage.pattern_count() as u64).to_le_bytes());
            hasher.update((coverage.covered_pattern_words().len() as u64).to_le_bytes());
            for word in coverage.covered_pattern_words() {
                hasher.update(word.to_le_bytes());
            }
            hasher.update((coverage.candidate_keys().len() as u64).to_le_bytes());
            for key in coverage.candidate_keys() {
                hash_text(&mut hasher, key);
            }
            hasher.update(coverage.witnessed_pattern_count().to_le_bytes());
            hasher.update([u8::from(coverage.complete())]);
        }
    }
    hex_sha256(finish_sha256(hasher))
}

fn executable_payload_sha256(
    executable: &DurableWorkerExecutable,
    worker_id: u32,
    request_sha256: [u8; 32],
) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.executable.v1");
    hash_text(&mut hasher, executable.kind().as_str());
    hasher.update(worker_id.to_le_bytes());
    hasher.update(request_sha256);
    if let DurableWorkerExecutable::Consume(candidates) = executable {
        hash_text(
            &mut hasher,
            &canonical_wasm_candidate_packet_batch_sha256(candidates),
        );
    }
    hex_sha256(finish_sha256(hasher))
}

pub(crate) fn canonical_native_build_probability_request_sha256(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    requested_workers: usize,
) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.typed-request.v1");
    hash_text(&mut hasher, problem.problem_id().as_str());
    hash_text(&mut hasher, problem.preset().as_str());
    hash_text(&mut hasher, problem.problem_kind().as_str());
    hasher.update(problem.initial_board().width().to_le_bytes());
    hasher.update(problem.initial_board().visible_height().to_le_bytes());
    hasher.update(problem.initial_board().occupied_mask().to_le_bytes());
    hasher.update(problem.piece_source().id().get().to_le_bytes());
    hash_text(&mut hasher, problem.piece_source().kind().as_str());
    hasher.update(
        problem
            .piece_source()
            .pattern_universe_id()
            .map_or(0, |identity| identity.get())
            .to_le_bytes(),
    );
    hasher.update(
        problem
            .piece_source()
            .pattern_weight_model_id()
            .map_or(0, |identity| identity.get())
            .to_le_bytes(),
    );
    hash_text(&mut hasher, problem.rule_profile_value().id().as_str());
    hasher.update((problem.piece_window().max_pieces() as u64).to_le_bytes());
    hasher.update((problem.exact_pieces().unwrap_or(0) as u64).to_le_bytes());
    hash_text(&mut hasher, problem.queue_observation_policy().keyword());
    let objective = problem.objective();
    let objective_kind = match objective.kind() {
        clearra_core_domain::objective::objective_kind::ObjectiveKind::All => "all",
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique => "unique",
        clearra_core_domain::objective::objective_kind::ObjectiveKind::MinimumCover => {
            "minimum-cover"
        }
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling => "tiling",
    };
    hash_text(&mut hasher, objective_kind);
    let score = objective.score();
    hash_text(&mut hasher, score.mode().as_str());
    hash_text(&mut hasher, score.profile().as_str());
    hash_text(&mut hasher, score.spin_profile().as_str());
    hasher.update(score.initial_b2b().to_le_bytes());
    let constraints = objective.execution_constraints();
    hasher.update([u8::from(constraints.preserves_back_to_back())]);
    hash_text(&mut hasher, constraints.spin_profile().as_str());
    let backend = problem.backend_request();
    hasher.update((backend.max_nodes() as u64).to_le_bytes());
    hasher.update((backend.max_frontier_states() as u64).to_le_bytes());
    hasher.update((backend.max_candidates() as u64).to_le_bytes());
    hasher.update((backend.max_patterns() as u64).to_le_bytes());
    hasher.update(backend.max_memory_mib().unwrap_or(0).to_le_bytes());
    hasher.update([u8::from(backend.deterministic())]);
    hasher.update([field.height(), u8::from(field.includes_horizontal_mirror())]);
    for word in field.base_words() {
        hasher.update(word.to_le_bytes());
    }
    for word in field.target_words() {
        hasher.update(word.to_le_bytes());
    }
    hash_text(&mut hasher, aggregation.as_str());
    hash_text(
        &mut hasher,
        aggregation.spin_coverage_target_id().unwrap_or("none"),
    );
    hash_text(&mut hasher, finesse_metric.as_str());
    hash_text(&mut hasher, finesse_pattern_knowledge.as_str());
    hash_text(
        &mut hasher,
        match solution_probability_policy {
            BuildSolutionProbabilityPolicy::Omit => "omit",
            BuildSolutionProbabilityPolicy::Include => "include",
        },
    );
    hasher.update((requested_workers.max(2) as u64).to_le_bytes());
    hex_sha256(finish_sha256(hasher))
}

fn worker_reply_sha256(pending: &PendingDurableDelegation, result_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.worker-reply.v1");
    hash_text(&mut hasher, pending.operation.as_str());
    hash_text(&mut hasher, &pending.token.task_id);
    hasher.update(pending.token.fencing_token.to_le_bytes());
    hasher.update(pending.worker_id.to_le_bytes());
    hash_text(&mut hasher, result_sha256);
    hex_sha256(finish_sha256(hasher))
}

fn result_sha256_for_completion(
    pending: &PendingDurableDelegation,
    result: &DurableWorkerResult,
) -> Result<String, CoreExecutionError> {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.operation-result.v1");
    hash_text(&mut hasher, pending.operation.as_str());
    hash_text(&mut hasher, &pending.payload_sha256);
    match result {
        DurableWorkerResult::Initialized(_) => {}
        DurableWorkerResult::Consumed(_, count) => {
            hasher.update((*count as u64).to_le_bytes());
        }
        DurableWorkerResult::Finished(_, results) => {
            hash_text(&mut hasher, &canonical_merger_result_sha256(results));
        }
        DurableWorkerResult::OfferAccepted(_) | DurableWorkerResult::Started(_) => {
            return Err(durable_runtime_unavailable(
                "native_build_probability_durable_result_stage_invalid",
            ));
        }
    }
    Ok(hex_sha256(finish_sha256(hasher)))
}

impl DurableWorkerResult {
    const fn key(&self) -> DurableWorkerTaskKey {
        match self {
            Self::OfferAccepted(key)
            | Self::Started(key)
            | Self::Initialized(key)
            | Self::Consumed(key, _)
            | Self::Finished(key, _) => *key,
        }
    }
}

fn compact_permit(
    permit: ExecutableDelegationPermit,
) -> Result<CompactExecutablePermit, CoreExecutionError> {
    let worker_id = permit
        .worker_id
        .parse::<u32>()
        .ok()
        .filter(|value| *value != 0 && value.to_string() == permit.worker_id);
    Ok(CompactExecutablePermit {
        job_id_sha256: sha256_text(&permit.job_id),
        task_id_sha256: sha256_text(&permit.task_id),
        worker_id: worker_id.ok_or_else(|| {
            durable_runtime_unavailable("native_build_probability_worker_identity_invalid")
        })?,
        payload_sha256: parse_sha256(&permit.payload_sha256).ok_or_else(|| {
            durable_runtime_unavailable("native_build_probability_payload_digest_invalid")
        })?,
        request_sha256: parse_sha256(&permit.request_sha256).ok_or_else(|| {
            durable_runtime_unavailable("native_build_probability_request_digest_invalid")
        })?,
        fencing_token: permit.fencing_token,
        publication_sequence: permit.publication_sequence,
        publication_sha256: parse_sha256(&permit.publication_sha256).ok_or_else(|| {
            durable_runtime_unavailable("native_build_probability_publication_digest_invalid")
        })?,
        expires_at_unix_ms: permit.expires_at_unix_ms,
    })
}

fn publication_is_valid(
    offer: DurableWorkerOffer,
    permit: CompactExecutablePermit,
    observed_now_unix_ms: u64,
    executable: &DurableWorkerExecutable,
    expected_job_sha256: [u8; 32],
    expected_request_sha256: [u8; 32],
) -> bool {
    let expected_payload =
        executable_payload_sha256(executable, offer.worker_id, expected_request_sha256);
    permit.job_id_sha256 == expected_job_sha256
        && permit.task_id_sha256 == offer.key.task_id_sha256
        && permit.worker_id == offer.worker_id
        && permit.payload_sha256 == parse_sha256(&expected_payload).unwrap_or([0; 32])
        && permit.request_sha256 == expected_request_sha256
        && permit.fencing_token == offer.key.fencing_token
        && permit.fencing_token != 0
        && permit.publication_sequence != 0
        && permit.publication_sha256 != [0; 32]
        && observed_now_unix_ms <= permit.expires_at_unix_ms
}

fn durable_worker_main(
    worker_index: usize,
    worker_id: u32,
    expected_job_sha256: [u8; 32],
    expected_request_sha256: [u8; 32],
    requests: Receiver<DurableWorkerRequest>,
    completions: SyncSender<NativeBuildPoolCompletion>,
    control: ExecutionControl,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        durable_worker_loop(
            worker_index,
            worker_id,
            expected_job_sha256,
            expected_request_sha256,
            &requests,
            &completions,
            &control,
        );
    }));
    if result.is_err() {
        send_durable_completion(
            &completions,
            worker_index,
            Err("native_build_probability_worker_panicked"),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn durable_worker_loop(
    worker_index: usize,
    worker_id: u32,
    expected_job_sha256: [u8; 32],
    expected_request_sha256: [u8; 32],
    requests: &Receiver<DurableWorkerRequest>,
    completions: &SyncSender<NativeBuildPoolCompletion>,
    control: &ExecutionControl,
) {
    let mut accepted_offer = None;
    let mut published = None;
    let mut verifier = None;
    let mut finished = false;
    while let Ok(request) = requests.recv() {
        let stage = match request {
            DurableWorkerRequest::Produce {
                producer,
                field,
                aggregation,
                initialize,
                batch_capacity,
            } => {
                let result = if accepted_offer.is_some()
                    || published.is_some()
                    || finished
                    || initialize != verifier.is_none()
                    || batch_capacity > NATIVE_BUILD_BATCH_CAPACITY
                {
                    Err(durable_runtime_unavailable(
                        "native_build_probability_producer_state_invalid",
                    ))
                } else {
                    control
                        .compute_budget()
                        .acquire_worker(&control.cancellation)
                        .ok_or(CoreExecutionError::Cancelled)
                        .and_then(|permit| {
                            with_compute_worker(permit, || {
                                produce_build_work(
                                    producer,
                                    field,
                                    aggregation,
                                    initialize,
                                    batch_capacity,
                                    control,
                                )
                            })
                        })
                };
                let failed = result.is_err();
                if completions
                    .send(NativeBuildPoolCompletion::Produced {
                        worker_index,
                        result,
                    })
                    .is_err()
                    || failed
                {
                    return;
                }
                continue;
            }
            DurableWorkerRequest::Offer(offer) => {
                if offer.worker_id != worker_id
                    || accepted_offer.is_some()
                    || published.is_some()
                    || finished
                {
                    Err("native_build_probability_durable_offer_invalid")
                } else {
                    accepted_offer = Some(offer);
                    Ok(DurableWorkerResult::OfferAccepted(offer.key))
                }
            }
            DurableWorkerRequest::Publish {
                permit,
                observed_now_unix_ms,
                executable,
            } => {
                let Some(offer) = accepted_offer else {
                    send_durable_completion(
                        completions,
                        worker_index,
                        Err("native_build_probability_durable_publication_without_offer"),
                    );
                    return;
                };
                let operation_state_valid = match executable.kind() {
                    DurableWorkerOperationKind::Initialize => verifier.is_none() && !finished,
                    DurableWorkerOperationKind::Consume => {
                        verifier.is_some()
                            && !finished
                            && matches!(&executable, DurableWorkerExecutable::Consume(batch) if !batch.is_empty())
                    }
                    DurableWorkerOperationKind::Finish => verifier.is_some() && !finished,
                };
                if !operation_state_valid
                    || !publication_is_valid(
                        offer,
                        permit,
                        observed_now_unix_ms,
                        &executable,
                        expected_job_sha256,
                        expected_request_sha256,
                    )
                {
                    Err("native_build_probability_durable_publication_invalid")
                } else {
                    published = Some((offer.key, executable));
                    Ok(DurableWorkerResult::Started(offer.key))
                }
            }
            DurableWorkerRequest::Run(key) => {
                let Some((published_key, executable)) = published.take() else {
                    send_durable_completion(
                        completions,
                        worker_index,
                        Err("native_build_probability_durable_run_without_publication"),
                    );
                    return;
                };
                if published_key != key {
                    Err("native_build_probability_durable_run_key_mismatch")
                } else {
                    accepted_offer = None;
                    match executable {
                        DurableWorkerExecutable::Initialize(worker_verifier) => {
                            if verifier.replace(worker_verifier).is_some() {
                                Err("native_build_probability_durable_duplicate_initialize")
                            } else {
                                Ok(DurableWorkerResult::Initialized(key))
                            }
                        }
                        DurableWorkerExecutable::Consume(candidates) => {
                            let count = candidates.len();
                            let Some(worker_verifier) = verifier.as_mut() else {
                                send_durable_completion(
                                    completions,
                                    worker_index,
                                    Err("native_build_probability_durable_consume_before_initialize"),
                                );
                                return;
                            };
                            with_durable_compute(control, || {
                                candidates
                                    .iter()
                                    .try_for_each(|candidate| {
                                        worker_verifier.consume(candidate, control)
                                    })
                                    .map(|()| DurableWorkerResult::Consumed(key, count))
                            })
                        }
                        DurableWorkerExecutable::Finish => {
                            let Some(mut worker_verifier) = verifier.take() else {
                                send_durable_completion(
                                    completions,
                                    worker_index,
                                    Err(
                                        "native_build_probability_durable_finish_before_initialize",
                                    ),
                                );
                                return;
                            };
                            finished = true;
                            with_durable_compute(control, || {
                                worker_verifier
                                    .finish()
                                    .map(|results| DurableWorkerResult::Finished(key, results))
                            })
                        }
                    }
                }
            }
        };
        let terminal = stage.is_err() || matches!(stage, Ok(DurableWorkerResult::Finished(_, _)));
        if !send_durable_completion(completions, worker_index, stage) || terminal {
            return;
        }
    }
}

fn send_durable_completion(
    completions: &SyncSender<NativeBuildPoolCompletion>,
    worker_index: usize,
    result: Result<DurableWorkerResult, &'static str>,
) -> bool {
    completions
        .send(NativeBuildPoolCompletion::Delegated(
            DurableWorkerCompletion {
                worker_index,
                result,
            },
        ))
        .is_ok()
}

fn with_durable_compute<T>(
    control: &ExecutionControl,
    work: impl FnOnce() -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let permit = control
        .compute_budget()
        .acquire_worker(&control.cancellation)
        .ok_or("native_build_probability_worker_cancelled")?;
    with_compute_worker(permit, work)
}

fn produce_build_work(
    mut producer: Box<WasmBuildProbabilityCandidateProducer>,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    initialize: bool,
    batch_capacity: usize,
    control: &ExecutionControl,
) -> Result<ProducedBuildWork, CoreExecutionError> {
    let verifier = initialize
        .then(|| producer.new_delegated_verifier(field, aggregation))
        .transpose()
        .map_err(WasmCpuSearchError::into_core_execution_error)?;
    let mut batch = Vec::new();
    batch
        .try_reserve_exact(batch_capacity)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    let mut summary = None;
    while batch.len() < batch_capacity {
        match producer
            .advance(control)
            .map_err(|reason| CoreExecutionError::Pc(reason.to_owned()))?
        {
            WasmCandidateProducerAdvance::Pending => {}
            WasmCandidateProducerAdvance::Candidate(candidate) => batch.push(candidate),
            WasmCandidateProducerAdvance::Completed(completed) => {
                summary = Some(completed);
                break;
            }
            WasmCandidateProducerAdvance::Cancelled => return Err(CoreExecutionError::Cancelled),
        }
    }
    Ok(ProducedBuildWork {
        producer,
        verifier,
        batch,
        summary,
    })
}

fn checked_maximum_task_count(problem: &SearchProblem, worker_count: usize) -> Option<usize> {
    let maximum_candidates = problem.backend_request().max_candidates();
    if maximum_candidates == 0 {
        // The common request contract uses zero for no candidate-count cap.
        // This is not zero work or an unavailable worker admission. Receipt
        // storage is grown for issued tasks, never reserved for this sentinel.
        return Some(0);
    }
    let batches = maximum_candidates / NATIVE_BUILD_INITIAL_BATCH_CAPACITY
        + usize::from(!maximum_candidates.is_multiple_of(NATIVE_BUILD_INITIAL_BATCH_CAPACITY));
    batches.checked_add(worker_count.checked_mul(2)?)
}

fn reserve_durable_task_slots<T>(
    values: &mut Vec<T>,
    required_count: usize,
    maximum_task_count: usize,
) -> Result<(), CoreExecutionError> {
    if maximum_task_count != 0 && required_count > maximum_task_count {
        return Err(durable_runtime_unavailable(
            "native_build_probability_durable_task_bound_exceeded",
        ));
    }
    let additional = required_count
        .checked_sub(values.len())
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    // Amortized growth avoids copying the whole receipt ledger for every batch
    // of an unlimited search. The count bound above remains an authority check,
    // independent of allocator spare capacity.
    values
        .try_reserve(additional)
        .map_err(|_| native_coordinator_allocation_unavailable())
}

fn reserve_durable_task_receipts(
    live_tokens: &mut Vec<DelegationToken>,
    sealed: &mut Vec<SealedDurableDelegation>,
    additional: usize,
    maximum_task_count: usize,
) -> Result<(), CoreExecutionError> {
    let required_count = live_tokens
        .len()
        .checked_add(additional)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    reserve_durable_task_slots(live_tokens, required_count, maximum_task_count)?;
    reserve_durable_task_slots(sealed, required_count, maximum_task_count)
}

fn admission_request(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    worker_count: usize,
) -> Result<NativeBuildProbabilityAdmissionRequest, CoreExecutionError> {
    let projection = NativeCoordinatorMemoryProjection::checked(field, worker_count)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    // Unlike the reference coordinator, the durable pool retains per-worker
    // protocol state while independent workers can already return results.
    // Charge that overlap, the actual (larger) durable channel slot, and the
    // single boxed source that moves between the manager and compute workers.
    let worker_state_bytes = checked_heap_backing_bytes::<NativeBuildPoolWorker>(worker_count)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let producer_owner_bytes =
        core::mem::size_of::<WasmBuildProbabilityCandidateProducer>() as u128;
    let durable_request_slot_bytes = (worker_count as u128)
        .checked_mul(NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY as u128)
        .and_then(|count| count.checked_mul(core::mem::size_of::<DurableWorkerRequest>() as u128))
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let additional_channel_bytes = durable_request_slot_bytes
        .checked_sub(projection.request_channel_slot_bytes)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let pool_container_bytes = core::mem::size_of::<Vec<NativeBuildPoolWorker>>() as u128;
    let pool_owner_bytes = worker_state_bytes
        .checked_add(producer_owner_bytes)
        .and_then(|bytes| bytes.checked_add(pool_container_bytes))
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let known_coordinator_peak_bytes = projection
        .required_peak_bytes
        .checked_add(additional_channel_bytes)
        .and_then(|bytes| bytes.checked_add(pool_owner_bytes))
        .and_then(|bytes| bytes.checked_add(projection.worker_result_backing_bytes))
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let minimum_channel_control_bytes = projection
        .request_sender_backing_bytes
        .checked_add(projection.worker_handle_backing_bytes)
        .and_then(|bytes| bytes.checked_add(projection.request_channel_slot_bytes))
        .and_then(|bytes| bytes.checked_add(projection.request_receiver_owner_bytes))
        .and_then(|bytes| bytes.checked_add(projection.completion_sender_owner_bytes))
        .and_then(|bytes| bytes.checked_add(projection.completion_receiver_owner_bytes))
        .and_then(|bytes| bytes.checked_add(projection.completion_channel_slot_bytes))
        .and_then(|bytes| bytes.checked_add(projection.container_owner_bytes))
        .and_then(|bytes| bytes.checked_add(projection.thread_name_payload_bytes))
        .and_then(|bytes| bytes.checked_add(additional_channel_bytes))
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let minimum_batch_owner_peak_bytes = projection
        .available_queue_backing_bytes
        .checked_add(projection.candidate_packet_backing_bytes)
        .and_then(|bytes| bytes.checked_add(projection.candidate_row_id_payload_bytes))
        .and_then(|bytes| bytes.checked_add(pool_owner_bytes))
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let minimum_result_owner_peak_bytes = minimum_batch_owner_peak_bytes
        .checked_add(projection.worker_result_backing_bytes)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    Ok(NativeBuildProbabilityAdmissionRequest {
        worker_count,
        maximum_task_count: checked_maximum_task_count(problem, worker_count)
            .ok_or_else(native_coordinator_allocation_unavailable)?,
        worker_stack_bytes: projection.worker_stack_bytes,
        request_channel_capacity: NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY,
        completion_channel_capacity: NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY,
        candidate_batch_capacity: NATIVE_BUILD_BATCH_CAPACITY,
        known_coordinator_peak_bytes,
        minimum_channel_control_bytes,
        minimum_batch_owner_peak_bytes,
        minimum_result_owner_peak_bytes,
    })
}

fn worker_reservation_sha256(
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    worker_id: u32,
    admitted_peak_bytes: u128,
) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build.worker-reservation.v1");
    hash_text(&mut hasher, &durable_identity.job_id);
    hash_text(&mut hasher, &durable_identity.request_sha256);
    hasher.update(worker_id.to_le_bytes());
    hasher.update(admitted_peak_bytes.to_le_bytes());
    hex_sha256(finish_sha256(hasher))
}

// Durable identity, journal authority, executable, and budget are separate
// proof inputs and must remain explicit at the exactly-once prepare boundary.
#[allow(clippy::too_many_arguments)]
fn prepare_durable_delegation<J: DelegationJournal, C: NativeDurableClock>(
    authority: &mut DurableDelegationAuthority<J>,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    ordinal: &mut u64,
    maximum_task_count: usize,
    worker_index: usize,
    worker_id: u32,
    executable: DurableWorkerExecutable,
    budget: DelegationBudget,
    clock: &C,
) -> Result<PendingDurableDelegation, CoreExecutionError> {
    let operation = executable.kind();
    let task_ordinal = *ordinal;
    if maximum_task_count != 0 && task_ordinal >= maximum_task_count as u64 {
        return Err(durable_runtime_unavailable(
            "native_build_probability_durable_task_bound_exceeded",
        ));
    }
    *ordinal = ordinal.checked_add(1).ok_or_else(|| {
        durable_runtime_unavailable("native_build_probability_task_ordinal_exhausted")
    })?;
    let payload_sha256 = executable_payload_sha256(
        &executable,
        worker_id,
        parse_sha256(&durable_identity.request_sha256).ok_or_else(|| {
            durable_runtime_unavailable("native_build_probability_request_digest_invalid")
        })?,
    );
    let identity = DelegationIdentity::new(
        durable_identity.job_id.clone(),
        durable_identity.task_id(task_ordinal, operation),
        durable_identity.coordinator_id.clone(),
        payload_sha256.clone(),
        durable_identity.request_sha256.clone(),
    )
    .map_err(|_| {
        durable_runtime_unavailable("native_build_probability_delegation_identity_invalid")
    })?;
    let token = authority
        .prepare(identity, budget, clock.now_unix_ms())
        .map_err(|_| {
            durable_runtime_unavailable("native_build_probability_delegation_prepare_failed")
        })?;
    Ok(PendingDurableDelegation {
        worker_index,
        worker_id,
        operation,
        token,
        payload_sha256,
        executable: Some(executable),
    })
}

fn pending_key(pending: &PendingDurableDelegation) -> DurableWorkerTaskKey {
    DurableWorkerTaskKey {
        task_id_sha256: sha256_text(&pending.token.task_id),
        fencing_token: pending.token.fencing_token,
    }
}

#[allow(clippy::too_many_arguments)]
fn submit_durable_delegation<J: DelegationJournal, C: NativeDurableClock>(
    worker: &mut NativeBuildPoolWorker,
    worker_index: usize,
    executable: DurableWorkerExecutable,
    sender: &SyncSender<DurableWorkerRequest>,
    authority: &mut DurableDelegationAuthority<J>,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    budget: DelegationBudget,
    ordinal: &mut u64,
    maximum_task_count: usize,
    live_tokens: &mut Vec<DelegationToken>,
    sealed: &mut Vec<SealedDurableDelegation>,
    clock: &C,
) -> Result<(), CoreExecutionError> {
    if !worker.is_ready() {
        return Err(durable_runtime_unavailable(
            "native_build_probability_worker_not_ready",
        ));
    }
    reserve_durable_task_receipts(live_tokens, sealed, 1, maximum_task_count)?;
    let candidate_count = match &executable {
        DurableWorkerExecutable::Consume(batch) => batch.len(),
        _ => 0,
    };
    let pending = prepare_durable_delegation(
        authority,
        durable_identity,
        ordinal,
        maximum_task_count,
        worker_index,
        u32::try_from(worker_index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| {
                durable_runtime_unavailable("native_build_probability_worker_identity_overflow")
            })?,
        executable,
        budget,
        clock,
    )?;
    live_tokens.push(pending.token.clone());
    authority
        .offered(&pending.token, clock.now_unix_ms())
        .map_err(|_| {
            durable_runtime_unavailable("native_build_probability_delegation_offer_failed")
        })?;
    sender
        .send(DurableWorkerRequest::Offer(DurableWorkerOffer {
            key: pending_key(&pending),
            worker_id: pending.worker_id,
        }))
        .map_err(|_| {
            durable_runtime_unavailable("native_build_probability_worker_pool_unavailable")
        })?;
    worker.delegation = Some(ActiveDurableDelegation {
        pending,
        stage: DurableWorkerStage::Offered,
        candidate_count,
        last_heartbeat_unix_ms: clock.now_unix_ms(),
    });
    Ok(())
}

/// Advance only the worker that replied. An outstanding offer, publication or
/// slow Consume on another worker cannot hold this worker behind a wave barrier.
#[allow(clippy::too_many_arguments)]
fn advance_durable_delegation<J: DelegationJournal, C: NativeDurableClock>(
    worker: &mut NativeBuildPoolWorker,
    completion: DurableWorkerCompletion,
    sender: &SyncSender<DurableWorkerRequest>,
    authority: &mut DurableDelegationAuthority<J>,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    admitted_peak_bytes: u128,
    sealed: &mut Vec<SealedDurableDelegation>,
    clock: &C,
) -> Result<Option<DurableWorkerResult>, CoreExecutionError> {
    let result = completion
        .result
        .map_err(|reason| CoreExecutionError::Pc(reason.to_owned()))?;
    let mut active = worker.delegation.take().ok_or_else(|| {
        durable_runtime_unavailable("native_build_probability_unexpected_worker_result")
    })?;
    let item = &mut active.pending;
    if completion.worker_index != item.worker_index || result.key() != pending_key(item) {
        return Err(durable_runtime_unavailable(
            "native_build_probability_worker_result_identity_mismatch",
        ));
    }
    match (&result, active.stage) {
        (DurableWorkerResult::OfferAccepted(_), DurableWorkerStage::Offered) => {
            authority
                .accepted(
                    &item.token,
                    item.worker_id.to_string(),
                    worker_reservation_sha256(
                        durable_identity,
                        item.worker_id,
                        admitted_peak_bytes,
                    ),
                    clock.now_unix_ms(),
                )
                .map_err(|_| {
                    durable_runtime_unavailable("native_build_probability_delegation_accept_failed")
                })?;
            let permit = authority
                .publish(&item.token, clock.now_unix_ms())
                .map_err(|_| {
                    durable_runtime_unavailable(
                        "native_build_probability_delegation_publish_failed",
                    )
                })?;
            sender
                .send(DurableWorkerRequest::Publish {
                    permit: compact_permit(permit)?,
                    observed_now_unix_ms: clock.now_unix_ms(),
                    executable: item.executable.take().ok_or_else(|| {
                        durable_runtime_unavailable(
                            "native_build_probability_executable_payload_missing",
                        )
                    })?,
                })
                .map_err(|_| {
                    durable_runtime_unavailable("native_build_probability_worker_pool_unavailable")
                })?;
            active.stage = DurableWorkerStage::Published;
        }
        (DurableWorkerResult::Started(_), DurableWorkerStage::Published) => {
            authority
                .running(&item.token, clock.now_unix_ms())
                .map_err(|_| {
                    durable_runtime_unavailable(
                        "native_build_probability_delegation_running_failed",
                    )
                })?;
            sender
                .send(DurableWorkerRequest::Run(pending_key(item)))
                .map_err(|_| {
                    durable_runtime_unavailable("native_build_probability_worker_pool_unavailable")
                })?;
            active.stage = DurableWorkerStage::Running;
            active.last_heartbeat_unix_ms = clock.now_unix_ms();
        }
        (_, DurableWorkerStage::Running) => {
            let valid = match (&result, item.operation) {
                (DurableWorkerResult::Initialized(_), DurableWorkerOperationKind::Initialize) => {
                    !worker.initialized
                }
                (DurableWorkerResult::Consumed(_, count), DurableWorkerOperationKind::Consume) => {
                    worker.initialized && *count != 0 && *count == active.candidate_count
                }
                (DurableWorkerResult::Finished(_, _), DurableWorkerOperationKind::Finish) => {
                    worker.initialized && !worker.finished
                }
                _ => false,
            };
            if !valid {
                return Err(durable_runtime_unavailable(
                    "native_build_probability_worker_result_stage_invalid",
                ));
            }
            let result_sha256 = result_sha256_for_completion(item, &result)?;
            authority
                .result_sealed(
                    &item.token,
                    result_sha256.clone(),
                    worker_reply_sha256(item, &result_sha256),
                    clock.now_unix_ms(),
                )
                .map_err(|_| {
                    durable_runtime_unavailable("native_build_probability_result_seal_failed")
                })?;
            sealed.push(SealedDurableDelegation {
                token: item.token.clone(),
                result_sha256,
            });
            match &result {
                DurableWorkerResult::Initialized(_) => worker.initialized = true,
                DurableWorkerResult::Consumed(_, _) => {
                    worker.consumed_batches = worker.consumed_batches.saturating_add(1);
                }
                DurableWorkerResult::Finished(_, _) => worker.finished = true,
                _ => unreachable!("validated terminal operation"),
            }
            return Ok(Some(result));
        }
        _ => {
            return Err(durable_runtime_unavailable(
                "native_build_probability_worker_ack_stage_invalid",
            ))
        }
    }
    worker.delegation = Some(active);
    Ok(None)
}

fn heartbeat_running_delegations<J: DelegationJournal, C: NativeDurableClock>(
    workers: &mut [NativeBuildPoolWorker],
    authority: &mut DurableDelegationAuthority<J>,
    clock: &C,
) -> Result<(), CoreExecutionError> {
    let now = clock.now_unix_ms();
    for active in workers
        .iter_mut()
        .filter_map(|worker| worker.delegation.as_mut())
    {
        if active.stage == DurableWorkerStage::Running
            && now.saturating_sub(active.last_heartbeat_unix_ms) >= WORKER_HEARTBEAT_INTERVAL_MS
        {
            authority
                .heartbeat(&active.pending.token, now)
                .map_err(|_| {
                    durable_runtime_unavailable("native_build_probability_worker_heartbeat_failed")
                })?;
            active.last_heartbeat_unix_ms = now;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn drive_durable_build_probability<J: DelegationJournal, C: NativeDurableClock>(
    producer: WasmBuildProbabilityCandidateProducer,
    authority: &mut DurableDelegationAuthority<J>,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    admitted_peak_bytes: u128,
    maximum_task_count: usize,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    request_senders: &[SyncSender<DurableWorkerRequest>],
    completion_receiver: &Receiver<NativeBuildPoolCompletion>,
    control: &ExecutionControl,
    clock: &C,
    live_tokens: &mut Vec<DelegationToken>,
) -> Result<
    (
        WasmBuildProbabilityCandidateProducer,
        NativeBuildProbabilityWorkerOutput,
        Vec<SealedDurableDelegation>,
    ),
    CoreExecutionError,
> {
    let memory_bytes = u64::try_from(admitted_peak_bytes).map_err(|_| {
        durable_runtime_unavailable("native_build_probability_delegation_budget_overflow")
    })?;
    let budget = DelegationBudget::new(1, memory_bytes).ok_or_else(|| {
        durable_runtime_unavailable("native_build_probability_delegation_budget_invalid")
    })?;
    let mut workers = Vec::new();
    workers
        .try_reserve_exact(request_senders.len())
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    workers.resize_with(request_senders.len(), NativeBuildPoolWorker::default);
    let mut worker_results = Vec::new();
    worker_results
        .try_reserve_exact(request_senders.len())
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    let mut producer = Some(Box::new(producer));
    let mut summary = None;
    let mut sealed = Vec::new();
    let mut ordinal = 0_u64;
    let mut emitted_candidates = 0_u64;
    let mut next_completion = None;
    loop {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        heartbeat_running_delegations(&mut workers, authority, clock)?;
        let completion = match next_completion.take() {
            Some(completion) => Some(completion),
            None => match completion_receiver.try_recv() {
                Ok(completion) => Some(completion),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    return Err(durable_runtime_unavailable(
                        "native_build_probability_worker_pool_unavailable",
                    ));
                }
            },
        };
        if let Some(completion) = completion {
            match completion {
                NativeBuildPoolCompletion::Produced {
                    worker_index,
                    result,
                } => {
                    let worker = workers.get_mut(worker_index).ok_or_else(|| {
                        durable_runtime_unavailable(
                            "native_build_probability_worker_result_identity_mismatch",
                        )
                    })?;
                    if !worker.producing || producer.is_some() || worker.queued_batch.is_some() {
                        return Err(durable_runtime_unavailable(
                            "native_build_probability_producer_result_repeated",
                        ));
                    }
                    let produced = result?;
                    worker.producing = false;
                    producer = Some(produced.producer);
                    if let Some(completed) = produced.summary {
                        if summary.replace(completed).is_some() {
                            return Err(durable_runtime_unavailable(
                                "native_build_probability_geometry_summary_repeated",
                            ));
                        }
                    }
                    if !produced.batch.is_empty() {
                        emitted_candidates =
                            emitted_candidates.saturating_add(produced.batch.len() as u64);
                        control.report_progress(
                            "build-probability-candidates",
                            emitted_candidates,
                            None,
                        );
                        worker.queued_batch = Some(produced.batch);
                    }
                    if let Some(verifier) = produced.verifier {
                        submit_durable_delegation(
                            worker,
                            worker_index,
                            DurableWorkerExecutable::Initialize(verifier),
                            &request_senders[worker_index],
                            authority,
                            durable_identity,
                            budget,
                            &mut ordinal,
                            maximum_task_count,
                            live_tokens,
                            &mut sealed,
                            clock,
                        )?;
                    } else if !worker.initialized {
                        return Err(durable_runtime_unavailable(
                            "native_build_probability_worker_initialization_missing",
                        ));
                    }
                }
                NativeBuildPoolCompletion::Delegated(completion) => {
                    let worker_index = completion.worker_index;
                    let worker = workers.get_mut(worker_index).ok_or_else(|| {
                        durable_runtime_unavailable(
                            "native_build_probability_worker_result_identity_mismatch",
                        )
                    })?;
                    if let Some(DurableWorkerResult::Finished(_, results)) =
                        advance_durable_delegation(
                            worker,
                            completion,
                            &request_senders[worker_index],
                            authority,
                            durable_identity,
                            admitted_peak_bytes,
                            &mut sealed,
                            clock,
                        )?
                    {
                        worker_results.push((worker_index, results));
                        let producer = producer.as_ref().ok_or_else(|| {
                            durable_runtime_unavailable(
                                "native_build_probability_finish_before_source_return",
                            )
                        })?;
                        let retained = super::checked_worker_results_memory(
                            &worker_results,
                            worker_results.capacity(),
                        )
                        .and_then(|(container, payload)| container.checked_add(payload))
                        .and_then(|bytes| {
                            bytes.checked_add(
                                core::mem::size_of::<WasmDistributedGeometrySummary>() as u128,
                            )
                        })
                        .unwrap_or(u128::MAX);
                        producer
                            .validate_external_result_memory(retained)
                            .map_err(WasmCpuSearchError::into_core_execution_error)?;
                    }
                }
            }
        }

        // A worker owns at most one batch across source output, pending durable
        // publication and running Consume. Never prefetch a second batch into
        // the one-slot request channel behind a busy verifier.
        for (worker_index, worker) in workers.iter_mut().enumerate() {
            if worker.is_ready() && worker.initialized {
                if let Some(batch) = worker.queued_batch.take() {
                    submit_durable_delegation(
                        worker,
                        worker_index,
                        DurableWorkerExecutable::Consume(batch),
                        &request_senders[worker_index],
                        authority,
                        durable_identity,
                        budget,
                        &mut ordinal,
                        maximum_task_count,
                        live_tokens,
                        &mut sealed,
                        clock,
                    )?;
                }
            }
        }

        // Production is an ordinary pool job with a unique movable owner. Its
        // former worker can consume this batch while another ready worker
        // produces the next one; the manager never runs geometry or verification.
        if producer.is_some() {
            let available = workers
                .iter()
                .position(|worker| {
                    worker.is_ready() && worker.queued_batch.is_none() && !worker.initialized
                })
                .or_else(|| {
                    summary
                        .is_none()
                        .then(|| {
                            workers.iter().position(|worker| {
                                worker.is_ready() && worker.queued_batch.is_none()
                            })
                        })
                        .flatten()
                });
            if let Some(worker_index) = available {
                let worker = &mut workers[worker_index];
                request_senders[worker_index]
                    .send(DurableWorkerRequest::Produce {
                        producer: producer.take().expect("unique available source owner"),
                        field,
                        aggregation,
                        initialize: !worker.initialized,
                        batch_capacity: if summary.is_some() {
                            0
                        } else {
                            worker.next_batch_capacity()
                        },
                    })
                    .map_err(|_| {
                        durable_runtime_unavailable(
                            "native_build_probability_worker_pool_unavailable",
                        )
                    })?;
                worker.producing = true;
            }
        }

        // Once every replica has initialized and the source has completed, any
        // ready verifier can finish independently of slower Consume operations.
        if summary.is_some() && workers.iter().all(|worker| worker.initialized) {
            for (worker_index, worker) in workers.iter_mut().enumerate() {
                if worker.is_ready() && worker.queued_batch.is_none() {
                    submit_durable_delegation(
                        worker,
                        worker_index,
                        DurableWorkerExecutable::Finish,
                        &request_senders[worker_index],
                        authority,
                        durable_identity,
                        budget,
                        &mut ordinal,
                        maximum_task_count,
                        live_tokens,
                        &mut sealed,
                        clock,
                    )?;
                }
            }
        }
        if worker_results.len() == workers.len() {
            worker_results.sort_unstable_by_key(|(worker_index, _)| *worker_index);
            return Ok((
                *producer.ok_or_else(|| {
                    durable_runtime_unavailable("native_build_probability_producer_owner_missing")
                })?,
                NativeBuildProbabilityWorkerOutput {
                    summary: summary.ok_or_else(|| {
                        durable_runtime_unavailable(
                            "native_build_probability_worker_geometry_summary_missing",
                        )
                    })?,
                    worker_results,
                },
                sealed,
            ));
        }
        if workers
            .iter()
            .all(|worker| !worker.producing && worker.delegation.is_none())
        {
            return Err(durable_runtime_unavailable(
                "native_build_probability_worker_pool_stalled",
            ));
        }
        next_completion = match completion_receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(completion) => Some(completion),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(durable_runtime_unavailable(
                    "native_build_probability_worker_pool_unavailable",
                ))
            }
        };
    }
}

fn fail_all_live_delegations<J: DelegationJournal, C: NativeDurableClock>(
    authority: &mut DurableDelegationAuthority<J>,
    live_tokens: &[DelegationToken],
    reason: &'static str,
    clock: &C,
) {
    for token in live_tokens {
        let terminal = authority.phase(token).ok().is_some_and(|phase| {
            matches!(
                phase,
                DelegationPhase::Completed
                    | DelegationPhase::Revoked
                    | DelegationPhase::Expired
                    | DelegationPhase::Cancelled
                    | DelegationPhase::FailedClosed
            )
        });
        if !terminal {
            let _ = authority.fail_closed(token, reason, clock.now_unix_ms());
        }
    }
}

/// Executes the actual native producer/verifier/merger only under a provider
/// admission authority. The public App route intentionally remains disconnected
/// until a host-owned provider and source-commit identity are supplied.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_provider_admitted_native_build_probability<
    J: DelegationJournal,
    P: NativeBuildProbabilityAdmissionProvider + ?Sized,
    C: NativeDurableClock,
>(
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    requested_workers: usize,
    retain_private_score_authority: bool,
    control: &ExecutionControl,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    authority: &mut DurableDelegationAuthority<J>,
    admission_provider: &P,
    clock: &C,
) -> Result<NativeBuildProbabilityExecutionOutput, CoreExecutionError> {
    let owned_control = control
        .clone()
        .with_parallelism_if_unbound(requested_workers.max(1));
    if owned_control.parallelism() != requested_workers.max(1) {
        return Err(durable_runtime_unavailable(
            "native_build_probability_pool_compute_budget_mismatch",
        ));
    }
    with_compute_coordinator(&owned_control, || {
        run_provider_admitted_native_build_probability_coordinator(
            service,
            problem,
            field,
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
            solution_probability_policy,
            requested_workers,
            retain_private_score_authority,
            &owned_control,
            durable_identity,
            authority,
            admission_provider,
            clock,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn run_provider_admitted_native_build_probability_coordinator<
    J: DelegationJournal,
    P: NativeBuildProbabilityAdmissionProvider + ?Sized,
    C: NativeDurableClock,
>(
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    requested_workers: usize,
    retain_private_score_authority: bool,
    control: &ExecutionControl,
    durable_identity: &NativeBuildProbabilityDurableIdentity,
    authority: &mut DurableDelegationAuthority<J>,
    admission_provider: &P,
    clock: &C,
) -> Result<NativeBuildProbabilityExecutionOutput, CoreExecutionError> {
    let expected_request_sha256 = canonical_native_build_probability_request_sha256(
        problem,
        field,
        aggregation,
        finesse_metric,
        finesse_pattern_knowledge,
        solution_probability_policy,
        requested_workers,
    );
    if durable_identity.request_sha256 != expected_request_sha256 {
        return Err(durable_runtime_unavailable(
            "native_build_probability_request_identity_mismatch",
        ));
    }
    let request_sha256 = parse_sha256(&expected_request_sha256).ok_or_else(|| {
        durable_runtime_unavailable("native_build_probability_request_digest_invalid")
    })?;
    let total_workers = requested_workers.max(1);
    let worker_threads = total_workers;
    let admission_request = admission_request(problem, field, worker_threads)?;
    let provider_measurement = admission_provider
        .admit_native_build_probability(admission_request)
        .map_err(|error| durable_runtime_unavailable(error.component()))?;
    let provider_authority = NativeBuildProbabilityAdmissionAuthority::from_provider_measurement(
        admission_request,
        provider_measurement,
    )
    .filter(|authority| authority.validates(admission_request))
    .ok_or_else(|| {
        durable_runtime_unavailable("native_build_probability_provider_admission_mismatch")
    })?;
    let admitted_peak_bytes = provider_authority.admitted_peak_bytes();
    let producer = run_native_build_compute(control, || {
        WasmBuildProbabilityCandidateProducer::new_with_finesse_for_native_pool_typed(
            problem,
            field,
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
            admitted_peak_bytes,
            control,
        )
        .map_err(WasmCpuSearchError::into_core_execution_error)
    })?;

    let (completion_sender, completion_receiver) =
        mpsc::sync_channel::<NativeBuildPoolCompletion>(NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY);
    let mut request_senders = Vec::new();
    request_senders
        .try_reserve_exact(worker_threads)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    handles
        .try_reserve_exact(worker_threads)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    let expected_job_sha256 = sha256_text(&durable_identity.job_id);
    for worker_index in 0..worker_threads {
        let worker_id = u32::try_from(worker_index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| {
                durable_runtime_unavailable("native_build_probability_worker_identity_overflow")
            })?;
        let (request_sender, request_receiver) =
            mpsc::sync_channel(NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY);
        let worker_completion_sender = completion_sender.clone();
        let worker_control = control.clone();
        let thread_name = native_worker_thread_name(worker_index)?;
        let handle = match thread::Builder::new()
            .name(thread_name)
            .stack_size(NATIVE_BUILD_WORKER_STACK_BYTES)
            .spawn(move || {
                durable_worker_main(
                    worker_index,
                    worker_id,
                    expected_job_sha256,
                    request_sha256,
                    request_receiver,
                    worker_completion_sender,
                    worker_control,
                )
            }) {
            Ok(handle) => handle,
            Err(_) => {
                drop(request_senders);
                drop(completion_sender);
                drop(completion_receiver);
                let _ = join_workers(handles);
                return Err(durable_runtime_unavailable(
                    "native_build_probability_worker_pool_unavailable",
                ));
            }
        };
        request_senders.push(request_sender);
        handles.push(handle);
    }
    drop(completion_sender);

    let mut live_tokens = Vec::new();
    let drive_result = drive_durable_build_probability(
        producer,
        authority,
        durable_identity,
        admitted_peak_bytes,
        admission_request.maximum_task_count,
        field,
        aggregation,
        &request_senders,
        &completion_receiver,
        control,
        clock,
        &mut live_tokens,
    );
    if drive_result.is_err() {
        control.cancellation.handle().cancel();
    }
    drop(request_senders);
    drop(completion_receiver);
    let join_result = join_workers(handles);
    let (producer, worker_output, sealed) = match (drive_result, join_result) {
        (Ok(output), Ok(())) => output,
        (Err(error), _) => {
            fail_all_live_delegations(
                authority,
                &live_tokens,
                "native build durable drive failed",
                clock,
            );
            return Err(error);
        }
        (Ok(_), Err(error)) => {
            fail_all_live_delegations(
                authority,
                &live_tokens,
                "native build worker join failed",
                clock,
            );
            return Err(error);
        }
    };
    if sealed.len() != live_tokens.len() {
        fail_all_live_delegations(
            authority,
            &live_tokens,
            "native build result seal set incomplete",
            clock,
        );
        return Err(durable_runtime_unavailable(
            "native_build_probability_result_seal_set_incomplete",
        ));
    }

    let result = match run_native_build_compute(control, || {
        finish_native_build_probability(
            service,
            producer,
            worker_output,
            solution_probability_policy,
            total_workers,
            control,
            retain_private_score_authority,
        )
    }) {
        Ok(result) => result,
        Err(error) => {
            fail_all_live_delegations(
                authority,
                &live_tokens,
                "native build merger failed before result application",
                clock,
            );
            return Err(error);
        }
    };

    for sealed_result in &sealed {
        match authority
            .result_application_decision(&sealed_result.token, &sealed_result.result_sha256)
            .map_err(|_| {
                durable_runtime_unavailable(
                    "native_build_probability_result_application_decision_failed",
                )
            })? {
            ResultApplicationDecision::ApplyOnce => {}
            ResultApplicationDecision::AlreadyApplied => {
                fail_all_live_delegations(
                    authority,
                    &live_tokens,
                    "native build duplicate result application rejected",
                    clock,
                );
                return Err(durable_runtime_unavailable(
                    "native_build_probability_duplicate_result_application",
                ));
            }
        }
        if authority
            .result_applied(&sealed_result.token, clock.now_unix_ms())
            .and_then(|()| authority.complete(&sealed_result.token, clock.now_unix_ms()))
            .is_err()
        {
            fail_all_live_delegations(
                authority,
                &live_tokens,
                "native build durable result acknowledgement failed",
                clock,
            );
            return Err(durable_runtime_unavailable(
                "native_build_probability_result_acknowledgement_failed",
            ));
        }
    }
    // Move the non-clone admission proof into a final scope so it remains live
    // through every worker join and durable result acknowledgement above.
    let result = {
        let _provider_authority = provider_authority;
        result
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::BTreeMap};

    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_executor::resource::MemoryDelegationJournal;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{BuildProbabilityFinesseRequest, ProblemCompiler};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    struct StepClock(Cell<u64>);

    impl StepClock {
        fn new() -> Self {
            Self(Cell::new(1_000))
        }
    }

    impl NativeDurableClock for StepClock {
        fn now_unix_ms(&self) -> u64 {
            let value = self.0.get();
            self.0.set(value + 1);
            value
        }
    }

    struct ExactProvider;

    impl NativeBuildProbabilityAdmissionProvider for ExactProvider {
        fn admit_native_build_probability(
            &self,
            request: NativeBuildProbabilityAdmissionRequest,
        ) -> Result<
            NativeBuildProbabilityProviderMeasurement,
            NativeBuildProbabilityHostProviderError,
        > {
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes(),
                request.minimum_channel_control_bytes(),
                request.minimum_batch_owner_peak_bytes(),
                request.minimum_result_owner_peak_bytes(),
            )
            .ok_or_else(|| {
                NativeBuildProbabilityHostProviderError::new(
                    "test_native_build_probability_provider_rejected",
                )
            })
        }
    }

    struct RejectingProvider;

    impl NativeBuildProbabilityAdmissionProvider for RejectingProvider {
        fn admit_native_build_probability(
            &self,
            _request: NativeBuildProbabilityAdmissionRequest,
        ) -> Result<
            NativeBuildProbabilityProviderMeasurement,
            NativeBuildProbabilityHostProviderError,
        > {
            Err(NativeBuildProbabilityHostProviderError::new(
                "test_native_build_probability_provider_unavailable",
            ))
        }
    }

    fn one_piece_problem(workers: usize) -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(workers)
                .with_worker_hardware_limit(workers),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece problem")
    }

    fn field() -> BuildProbabilityField {
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target")
    }

    #[test]
    fn native_build_default_unlimited_candidates_are_not_zero_task_authority() {
        let problem = one_piece_problem(2);
        assert_eq!(problem.backend_request().max_candidates(), 0);
        assert_eq!(checked_maximum_task_count(&problem, 1), Some(0));
        assert_eq!(
            admission_request(&problem, field(), 1)
                .unwrap()
                .maximum_task_count(),
            0
        );
        // A large declared bound is not a request to allocate that many tokens.
        let mut tokens = Vec::<u8>::new();
        reserve_durable_task_slots(&mut tokens, 2, usize::MAX).unwrap();
        assert!(tokens.capacity() >= 2 && tokens.capacity() < 1024);
        tokens.extend([1, 2]);
        reserve_durable_task_slots(&mut tokens, 3, 0).unwrap();
        assert!(tokens.capacity() >= 3);
        assert!(reserve_durable_task_slots(&mut tokens, 4, 3).is_err());
        assert!(reserve_durable_task_slots(&mut tokens, 1, 0).is_err());
    }

    #[test]
    fn native_pool_batches_grow_within_the_admitted_owner_bound() {
        let mut worker = NativeBuildPoolWorker::default();
        for expected in [32, 64, 128, 256, 256] {
            assert_eq!(worker.next_batch_capacity(), expected);
            worker.consumed_batches += 1;
        }
        worker.consumed_batches = usize::MAX;
        assert_eq!(worker.next_batch_capacity(), 256);
        for workers in [1, 2, 4, 8] {
            let request = admission_request(&one_piece_problem(workers), field(), workers)
                .expect("bounded pool admission");
            let reference = NativeCoordinatorMemoryProjection::checked(field(), workers).unwrap();
            assert_eq!(request.worker_count(), workers);
            assert_eq!(request.candidate_batch_capacity(), 256);
            assert!(
                request.known_coordinator_peak_bytes()
                    >= reference.required_peak_bytes
                        + (workers * core::mem::size_of::<NativeBuildPoolWorker>()) as u128
                        + core::mem::size_of::<WasmBuildProbabilityCandidateProducer>() as u128
                        + reference.worker_result_backing_bytes
            );
            // Results may arrive while another worker still owns its maximum
            // batch; these allocations must be admitted together.
            assert!(
                request.minimum_result_owner_peak_bytes()
                    >= request.minimum_batch_owner_peak_bytes()
                        + reference.worker_result_backing_bytes
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_scripted_consume(
        workers: &mut [NativeBuildPoolWorker],
        worker_index: usize,
        senders: &[SyncSender<DurableWorkerRequest>],
        receivers: &[Receiver<DurableWorkerRequest>],
        authority: &mut DurableDelegationAuthority<MemoryDelegationJournal>,
        identity: &NativeBuildProbabilityDurableIdentity,
        sealed: &mut Vec<SealedDurableDelegation>,
        clock: &StepClock,
    ) -> DelegationToken {
        let token = workers[worker_index]
            .delegation
            .as_ref()
            .unwrap()
            .pending
            .token
            .clone();
        let DurableWorkerRequest::Offer(offer) = receivers[worker_index].try_recv().unwrap() else {
            panic!("worker must receive the durable offer first")
        };
        for result in [
            DurableWorkerResult::OfferAccepted(offer.key),
            DurableWorkerResult::Started(offer.key),
        ] {
            assert!(advance_durable_delegation(
                &mut workers[worker_index],
                DurableWorkerCompletion {
                    worker_index,
                    result: Ok(result)
                },
                &senders[worker_index],
                authority,
                identity,
                4096,
                sealed,
                clock,
            )
            .unwrap()
            .is_none());
            match receivers[worker_index].try_recv().unwrap() {
                DurableWorkerRequest::Publish {
                    permit, executable, ..
                } => {
                    assert_eq!(permit.worker_id, offer.worker_id);
                    assert!(matches!(executable, DurableWorkerExecutable::Consume(_)));
                }
                DurableWorkerRequest::Run(key) => assert_eq!(key, offer.key),
                _ => panic!("only the acknowledged worker may advance"),
            }
        }
        assert!(matches!(
            advance_durable_delegation(
                &mut workers[worker_index],
                DurableWorkerCompletion {
                    worker_index,
                    result: Ok(DurableWorkerResult::Consumed(offer.key, 1))
                },
                &senders[worker_index],
                authority,
                identity,
                4096,
                sealed,
                clock,
            )
            .unwrap(),
            Some(DurableWorkerResult::Consumed(_, 1))
        ));
        token
    }

    #[test]
    fn ready_worker_advances_and_takes_another_batch_before_a_slow_worker_accepts() {
        let problem = one_piece_problem(2);
        let identity = durable_identity(&problem, 2, BuildSolutionProbabilityPolicy::Omit);
        let mut authority =
            DurableDelegationAuthority::recover(MemoryDelegationJournal::default()).unwrap();
        let clock = StepClock::new();
        let mut workers = [
            NativeBuildPoolWorker {
                initialized: true,
                ..Default::default()
            },
            NativeBuildPoolWorker {
                initialized: true,
                ..Default::default()
            },
        ];
        let (senders, receivers): (Vec<_>, Vec<_>) = (0..2)
            .map(|_| mpsc::sync_channel(NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY))
            .unzip();
        let mut ordinal = 0;
        let mut live_tokens = Vec::new();
        let mut sealed = Vec::new();
        let budget = DelegationBudget::new(1, 4096).unwrap();
        for index in [0, 1] {
            submit_durable_delegation(
                &mut workers[index],
                index,
                DurableWorkerExecutable::Consume(vec![WasmCandidatePacket::new(
                    index as u64,
                    0,
                    vec![1],
                )]),
                &senders[index],
                &mut authority,
                &identity,
                budget,
                &mut ordinal,
                0,
                &mut live_tokens,
                &mut sealed,
                &clock,
            )
            .unwrap();
        }
        let slow = live_tokens[0].clone();
        let first_ready = complete_scripted_consume(
            &mut workers,
            1,
            &senders,
            &receivers,
            &mut authority,
            &identity,
            &mut sealed,
            &clock,
        );
        assert!(workers[1].is_ready());
        assert_eq!(authority.phase(&slow).unwrap(), DelegationPhase::Offered);
        submit_durable_delegation(
            &mut workers[1],
            1,
            DurableWorkerExecutable::Consume(vec![WasmCandidatePacket::new(2, 0, vec![1])]),
            &senders[1],
            &mut authority,
            &identity,
            budget,
            &mut ordinal,
            0,
            &mut live_tokens,
            &mut sealed,
            &clock,
        )
        .unwrap();
        let second_ready = complete_scripted_consume(
            &mut workers,
            1,
            &senders,
            &receivers,
            &mut authority,
            &identity,
            &mut sealed,
            &clock,
        );
        assert_eq!(authority.phase(&slow).unwrap(), DelegationPhase::Offered);
        complete_scripted_consume(
            &mut workers,
            0,
            &senders,
            &receivers,
            &mut authority,
            &identity,
            &mut sealed,
            &clock,
        );
        assert_eq!(ordinal, 3);
        assert_eq!(live_tokens.len(), sealed.len());
        assert_eq!(sealed[0].token, first_ready);
        assert_eq!(sealed[1].token, second_ready);
        assert_eq!(sealed[2].token, slow);
        // Out-of-order sealing still permits exactly one application per task.
        for result in &sealed {
            assert_eq!(
                authority
                    .result_application_decision(&result.token, &result.result_sha256)
                    .unwrap(),
                ResultApplicationDecision::ApplyOnce
            );
            authority
                .result_applied(&result.token, clock.now_unix_ms())
                .unwrap();
            assert_eq!(
                authority
                    .result_application_decision(&result.token, &result.result_sha256)
                    .unwrap(),
                ResultApplicationDecision::AlreadyApplied
            );
            authority
                .complete(&result.token, clock.now_unix_ms())
                .unwrap();
        }
        let journal_len = authority.journal().records().len();
        let repeated_key = DurableWorkerTaskKey {
            task_id_sha256: sha256_text(&second_ready.task_id),
            fencing_token: second_ready.fencing_token,
        };
        assert!(advance_durable_delegation(
            &mut workers[1],
            DurableWorkerCompletion {
                worker_index: 1,
                result: Ok(DurableWorkerResult::Consumed(repeated_key, 1))
            },
            &senders[1],
            &mut authority,
            &identity,
            4096,
            &mut sealed,
            &clock,
        )
        .is_err());
        assert_eq!(authority.journal().records().len(), journal_len);
    }

    #[test]
    fn completed_source_slot_can_join_all_compute_workers_without_overflow() {
        use std::sync::{Arc, Barrier};
        for count in [1, 2, 4] {
            let control = ExecutionControl::default().with_parallelism(count);
            let started = Arc::new(Barrier::new(count + 1));
            let release = Arc::new(Barrier::new(count + 1));
            thread::scope(|scope| {
                for index in 0..count {
                    let started = started.clone();
                    let release = release.clone();
                    let control = &control;
                    scope.spawn(move || {
                        if index == 0 {
                            // A completed Produce job returns its slot. The same
                            // worker can then enter Consume with all N peers.
                            with_durable_compute(control, || Ok(())).unwrap();
                        }
                        with_durable_compute(control, || {
                            started.wait();
                            release.wait();
                            Ok(())
                        })
                        .unwrap();
                    });
                }
                started.wait();
                assert!(control.compute_budget().try_acquire_worker().is_none());
                release.wait();
            });
            let permits: Vec<_> = (0..count)
                .map(|_| {
                    control
                        .compute_budget()
                        .try_acquire_worker()
                        .expect("returned pool slot")
                })
                .collect();
            assert!(control.compute_budget().try_acquire_worker().is_none());
            drop(permits);
        }
    }

    fn durable_identity(
        problem: &SearchProblem,
        requested_workers: usize,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
    ) -> NativeBuildProbabilityDurableIdentity {
        let request_sha256 = canonical_native_build_probability_request_sha256(
            problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            solution_probability_policy,
            requested_workers,
        );
        NativeBuildProbabilityDurableIdentity::new(
            "11111111-1111-1111-1111-111111111111",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "22222222-2222-2222-2222-222222222222",
            request_sha256,
        )
        .expect("durable identity")
    }

    #[test]
    fn provider_admitted_actual_pipeline_journals_every_operation_and_matches_serial() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let problem = one_piece_problem(2);
        let service = AppCoreExecutorService::wasm_cpu();
        let control = ExecutionControl::default();
        let serial = service
            .execute_build_probability_with_control(
                &problem,
                field(),
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Search {
                    pattern_knowledge: FinessePatternKnowledge::Both,
                },
                BuildSolutionProbabilityPolicy::Include,
                &control,
            )
            .expect("serial Build result");
        let identity = durable_identity(&problem, 2, BuildSolutionProbabilityPolicy::Include);
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("empty journal authority");
        let durable = run_provider_admitted_native_build_probability(
            service,
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            BuildSolutionProbabilityPolicy::Include,
            2,
            false,
            &control,
            &identity,
            &mut authority,
            &ExactProvider,
            &StepClock::new(),
        )
        .expect("provider-admitted durable Build result")
        .into_result();

        assert_eq!(
            durable.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(durable.finesse_report(), serial.finesse_report());
        assert_eq!(
            durable.solution_probabilities(),
            serial.solution_probabilities()
        );
        for field in [
            "solution_probabilities_requested",
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(durable.unique_field(field), serial.unique_field(field));
        }
        assert_eq!(durable.usize_field("workers_used"), Some(2));
        assert_eq!(
            durable.field("cpu_parallel_decision_reason"),
            Some("native-ready-worker-build-probability-pipeline")
        );

        let mut phases_by_task = BTreeMap::<String, Vec<DelegationPhase>>::new();
        for event in authority.journal().records() {
            phases_by_task
                .entry(event.identity.task_id.clone())
                .or_default()
                .push(event.phase);
            if matches!(
                event.phase,
                DelegationPhase::Accepted
                    | DelegationPhase::Published
                    | DelegationPhase::Running
                    | DelegationPhase::ResultSealed
                    | DelegationPhase::ResultApplied
                    | DelegationPhase::Completed
            ) {
                assert!(matches!(event.worker_id.as_deref(), Some("1" | "2")));
            }
            if matches!(
                event.phase,
                DelegationPhase::ResultSealed
                    | DelegationPhase::ResultApplied
                    | DelegationPhase::Completed
            ) {
                assert!(event
                    .result_sha256
                    .as_deref()
                    .is_some_and(|value| is_lower_hex(value, 64)));
                assert!(event
                    .worker_reply_sha256
                    .as_deref()
                    .is_some_and(|value| is_lower_hex(value, 64)));
            }
        }
        let expected = vec![
            DelegationPhase::Prepared,
            DelegationPhase::Offered,
            DelegationPhase::Accepted,
            DelegationPhase::Published,
            DelegationPhase::Running,
            DelegationPhase::ResultSealed,
            DelegationPhase::ResultApplied,
            DelegationPhase::Completed,
        ];
        assert!(phases_by_task.len() >= 3);
        assert_eq!(
            phases_by_task
                .keys()
                .filter(|task| task.ends_with(":initialize"))
                .count(),
            2
        );
        assert_eq!(
            phases_by_task
                .keys()
                .filter(|task| task.ends_with(":finish"))
                .count(),
            2
        );
        assert!(phases_by_task.keys().any(|task| task.ends_with(":consume")));
        assert!(phases_by_task.values().all(|phases| phases == &expected));
    }

    #[test]
    fn provider_rejection_and_request_identity_mismatch_fail_before_journal_or_execution() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let problem = one_piece_problem(2);
        let control = ExecutionControl::default();
        let mut authority = DurableDelegationAuthority::recover(MemoryDelegationJournal::default())
            .expect("empty journal authority");
        let error = run_provider_admitted_native_build_probability(
            AppCoreExecutorService::wasm_cpu(),
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            BuildSolutionProbabilityPolicy::Omit,
            2,
            false,
            &control,
            &durable_identity(&problem, 2, BuildSolutionProbabilityPolicy::Omit),
            &mut authority,
            &RejectingProvider,
            &StepClock::new(),
        )
        .expect_err("provider rejection must fail closed");
        assert_eq!(
            error.unsupported_reason(),
            Some("test_native_build_probability_provider_unavailable")
        );
        assert!(authority.journal().records().is_empty());

        let wrong = NativeBuildProbabilityDurableIdentity::new(
            "11111111-1111-1111-1111-111111111111",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "22222222-2222-2222-2222-222222222222",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("structurally valid wrong request identity");
        let error = run_provider_admitted_native_build_probability(
            AppCoreExecutorService::wasm_cpu(),
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            BuildSolutionProbabilityPolicy::Omit,
            2,
            false,
            &control,
            &wrong,
            &mut authority,
            &ExactProvider,
            &StepClock::new(),
        )
        .expect_err("request identity mismatch must fail closed");
        assert_eq!(
            error.unsupported_reason(),
            Some("native_build_probability_request_identity_mismatch")
        );
        assert!(authority.journal().records().is_empty());
        assert!(NativeBuildProbabilityProviderMeasurement::new(0, 1, 1, 1).is_none());
    }

    #[test]
    fn compact_publication_rejects_payload_request_and_expiry_mismatch_before_run() {
        let request_sha256 = [7_u8; 32];
        let executable =
            DurableWorkerExecutable::Consume(vec![WasmCandidatePacket::new(0, 0, vec![1])]);
        let offer = DurableWorkerOffer {
            key: DurableWorkerTaskKey {
                task_id_sha256: [3; 32],
                fencing_token: 9,
            },
            worker_id: 1,
        };
        let valid = CompactExecutablePermit {
            job_id_sha256: [2; 32],
            task_id_sha256: offer.key.task_id_sha256,
            worker_id: 1,
            payload_sha256: parse_sha256(&executable_payload_sha256(
                &executable,
                1,
                request_sha256,
            ))
            .expect("payload digest"),
            request_sha256,
            fencing_token: 9,
            publication_sequence: 1,
            publication_sha256: [4; 32],
            expires_at_unix_ms: 100,
        };
        assert!(publication_is_valid(
            offer,
            valid,
            100,
            &executable,
            [2; 32],
            request_sha256,
        ));
        assert!(!publication_is_valid(
            offer,
            CompactExecutablePermit {
                payload_sha256: [0; 32],
                ..valid
            },
            100,
            &executable,
            [2; 32],
            request_sha256,
        ));
        assert!(!publication_is_valid(
            offer,
            CompactExecutablePermit {
                request_sha256: [8; 32],
                ..valid
            },
            100,
            &executable,
            [2; 32],
            request_sha256,
        ));
        assert!(!publication_is_valid(
            offer,
            valid,
            101,
            &executable,
            [2; 32],
            request_sha256,
        ));
    }

    #[test]
    fn durable_identity_and_typed_request_digest_are_strict_and_option_bound() {
        let problem = one_piece_problem(2);
        let base = canonical_native_build_probability_request_sha256(
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            BuildSolutionProbabilityPolicy::Omit,
            2,
        );
        let changed = canonical_native_build_probability_request_sha256(
            &problem,
            field(),
            BuildProbabilityAggregation::TilingOnly,
            FinesseMetric::Off,
            FinessePatternKnowledge::Oracle,
            BuildSolutionProbabilityPolicy::Include,
            3,
        );
        assert_ne!(base, changed);
        assert!(NativeBuildProbabilityDurableIdentity::new(
            "not-a-uuid",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "22222222-2222-2222-2222-222222222222",
            base.clone(),
        )
        .is_err());
        assert!(NativeBuildProbabilityDurableIdentity::new(
            "11111111-1111-1111-1111-111111111111",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "22222222-2222-2222-2222-222222222222",
            base,
        )
        .is_err());
    }

    #[test]
    fn provider_measurement_requires_every_bounded_native_owner_category() {
        let request = admission_request(&one_piece_problem(2), field(), 1)
            .expect("native provider admission request");
        let exact = NativeBuildProbabilityProviderMeasurement::new(
            request.worker_stack_bytes(),
            request.minimum_channel_control_bytes(),
            request.minimum_batch_owner_peak_bytes(),
            request.minimum_result_owner_peak_bytes(),
        )
        .expect("nonzero exact provider measurement");
        assert!(exact.validates(request));

        let undermeasured = [
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes() - 1,
                request.minimum_channel_control_bytes(),
                request.minimum_batch_owner_peak_bytes(),
                request.minimum_result_owner_peak_bytes(),
            )
            .expect("undermeasured stack category"),
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes(),
                request.minimum_channel_control_bytes() - 1,
                request.minimum_batch_owner_peak_bytes(),
                request.minimum_result_owner_peak_bytes(),
            )
            .expect("undermeasured channel category"),
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes(),
                request.minimum_channel_control_bytes(),
                request.minimum_batch_owner_peak_bytes() - 1,
                request.minimum_result_owner_peak_bytes(),
            )
            .expect("undermeasured batch owner category"),
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes(),
                request.minimum_channel_control_bytes(),
                request.minimum_batch_owner_peak_bytes(),
                request.minimum_result_owner_peak_bytes() - 1,
            )
            .expect("undermeasured result owner category"),
        ];
        assert!(undermeasured
            .into_iter()
            .all(|measurement| !measurement.validates(request)));
    }
}
