//! Process-owned registration and durable startup boundary for native Build.
//!
//! The executor-facing durable module owns the worker protocol. This module
//! owns the separate host concerns: one-time provider registration, verified
//! compiled source identity, boot/job identities, startup quarantine, and the
//! native JSONL journal factory.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::resource::{
    default_native_delegation_journal_root, DelegationJournal, DelegationJournalError,
    DurableDelegationAuthority, NativeJsonlDelegationJournal,
};
use clearra_core_executor::CoreExecutionError;
use clearra_host_contract::ProductBuildIdentity;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    BuildSolutionProbabilityPolicy, SearchProblem,
};
use sha2::{Digest, Sha256};

use super::durable::{
    canonical_native_build_probability_request_sha256,
    run_provider_admitted_native_build_probability, NativeBuildProbabilityDurableIdentity,
    NativeDurableClock, SystemNativeDurableClock,
};
pub use super::durable::{
    NativeBuildProbabilityAdmissionProvider, NativeBuildProbabilityAdmissionRequest,
    NativeBuildProbabilityHostProviderError, NativeBuildProbabilityProviderMeasurement,
};
use super::system_provider::system_boot_uuid;
pub use super::system_provider::SystemNativeBuildProbabilityAdmissionProvider;
use super::NativeBuildProbabilityExecutionOutput;
use crate::app_services::AppCoreExecutorService;

const BOOT_MARKER_SCHEMA: &str = "clearra.native-build-provider-boot.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBuildProbabilityHostRegistrationError {
    InvalidBootUuid,
    BootEntropyUnavailable,
    JournalRootMustBeAbsolute,
    UnverifiedSourceCommit,
    AlreadyRegistered,
    RegistrationLockUnavailable,
    BootIdentityAlreadyUsed,
    JournalUnavailable,
}

impl NativeBuildProbabilityHostRegistrationError {
    pub const fn component(self) -> &'static str {
        match self {
            Self::InvalidBootUuid => "native_build_probability_boot_uuid_invalid",
            Self::BootEntropyUnavailable => "native_build_probability_boot_entropy_unavailable",
            Self::JournalRootMustBeAbsolute => "native_build_probability_journal_root_not_absolute",
            Self::UnverifiedSourceCommit => "native_build_probability_source_commit_unverified",
            Self::AlreadyRegistered => "native_build_probability_provider_already_registered",
            Self::RegistrationLockUnavailable => {
                "native_build_probability_provider_registration_lock_unavailable"
            }
            Self::BootIdentityAlreadyUsed => "native_build_probability_boot_identity_already_used",
            Self::JournalUnavailable => "native_build_probability_durable_journal_unavailable",
        }
    }
}

impl fmt::Display for NativeBuildProbabilityHostRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.component())
    }
}

impl std::error::Error for NativeBuildProbabilityHostRegistrationError {}

/// Host-owned inputs needed before a native durable Build route can exist.
///
/// The provider is intentionally an explicit process-lifetime authority. A
/// scalar supplied in a command or environment variable cannot register the
/// route or stand in for an actual allocation measurement.
pub struct NativeBuildProbabilityHostRegistration {
    provider: Arc<dyn NativeBuildProbabilityAdmissionProvider>,
    journal_root: PathBuf,
    boot_uuid: String,
}

impl NativeBuildProbabilityHostRegistration {
    pub fn new<P>(
        provider: P,
        journal_root: impl Into<PathBuf>,
        boot_uuid: impl Into<String>,
    ) -> Result<Self, NativeBuildProbabilityHostRegistrationError>
    where
        P: NativeBuildProbabilityAdmissionProvider + 'static,
    {
        Self::from_arc(Arc::new(provider), journal_root, boot_uuid)
    }

    pub fn from_arc(
        provider: Arc<dyn NativeBuildProbabilityAdmissionProvider>,
        journal_root: impl Into<PathBuf>,
        boot_uuid: impl Into<String>,
    ) -> Result<Self, NativeBuildProbabilityHostRegistrationError> {
        let journal_root = journal_root.into();
        let boot_uuid = boot_uuid.into();
        if !is_canonical_uuid(&boot_uuid) {
            return Err(NativeBuildProbabilityHostRegistrationError::InvalidBootUuid);
        }
        if !journal_root.is_absolute() {
            return Err(NativeBuildProbabilityHostRegistrationError::JournalRootMustBeAbsolute);
        }
        Ok(Self {
            provider,
            journal_root,
            boot_uuid,
        })
    }

    pub fn with_default_journal_root<P>(
        provider: P,
        boot_uuid: impl Into<String>,
    ) -> Result<Self, NativeBuildProbabilityHostRegistrationError>
    where
        P: NativeBuildProbabilityAdmissionProvider + 'static,
    {
        let root = default_native_delegation_journal_root()
            .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
        Self::new(provider, root, boot_uuid)
    }
}

struct RegisteredNativeBuildProbabilityHost {
    provider: Arc<dyn NativeBuildProbabilityAdmissionProvider>,
    journal_root: PathBuf,
    boot_uuid: String,
    source_commit: String,
    next_job_sequence: AtomicU64,
    _boot_marker: File,
}

static REGISTERED_NATIVE_BUILD_PROBABILITY_HOST: OnceLock<RegisteredNativeBuildProbabilityHost> =
    OnceLock::new();
static NATIVE_BUILD_PROBABILITY_REGISTRATION_LOCK: Mutex<()> = Mutex::new(());

/// Installs exactly one native provider for the lifetime of this process.
/// Source identity is read only from the compile-time product identity.
pub fn register_native_build_probability_host(
    registration: NativeBuildProbabilityHostRegistration,
) -> Result<(), NativeBuildProbabilityHostRegistrationError> {
    let _guard = NATIVE_BUILD_PROBABILITY_REGISTRATION_LOCK
        .lock()
        .map_err(|_| NativeBuildProbabilityHostRegistrationError::RegistrationLockUnavailable)?;
    if REGISTERED_NATIVE_BUILD_PROBABILITY_HOST.get().is_some() {
        return Err(NativeBuildProbabilityHostRegistrationError::AlreadyRegistered);
    }
    let runtime = prepare_registered_host(registration, &ProductBuildIdentity::current())?;
    REGISTERED_NATIVE_BUILD_PROBABILITY_HOST
        .set(runtime)
        .map_err(|_| NativeBuildProbabilityHostRegistrationError::AlreadyRegistered)
}

/// Registers the production process provider. Local unverified builds remain
/// unregistered, so the native route stays unavailable without preventing
/// unrelated CLI or desktop commands from starting.
pub fn register_system_native_build_probability_host(
) -> Result<(), NativeBuildProbabilityHostRegistrationError> {
    let identity = ProductBuildIdentity::current();
    verified_source_commit(&identity)?;
    let boot_uuid = system_boot_uuid()
        .map_err(|()| NativeBuildProbabilityHostRegistrationError::BootEntropyUnavailable)?;
    let registration = NativeBuildProbabilityHostRegistration::with_default_journal_root(
        SystemNativeBuildProbabilityAdmissionProvider,
        boot_uuid,
    )?;
    register_native_build_probability_host(registration)
}

pub(crate) fn native_build_probability_host_registered() -> bool {
    REGISTERED_NATIVE_BUILD_PROBABILITY_HOST.get().is_some()
}

fn prepare_registered_host(
    registration: NativeBuildProbabilityHostRegistration,
    identity: &ProductBuildIdentity,
) -> Result<RegisteredNativeBuildProbabilityHost, NativeBuildProbabilityHostRegistrationError> {
    let source_commit = verified_source_commit(identity)?.to_owned();
    ensure_journal_root(&registration.journal_root)?;
    recover_existing_journals(
        &registration.journal_root,
        &source_commit,
        SystemNativeDurableClock.now_unix_ms(),
    )?;
    let boot_marker = create_boot_marker(
        &registration.journal_root,
        &source_commit,
        &registration.boot_uuid,
    )?;
    Ok(RegisteredNativeBuildProbabilityHost {
        provider: registration.provider,
        journal_root: registration.journal_root,
        boot_uuid: registration.boot_uuid,
        source_commit,
        next_job_sequence: AtomicU64::new(1),
        _boot_marker: boot_marker,
    })
}

fn verified_source_commit(
    identity: &ProductBuildIdentity,
) -> Result<&str, NativeBuildProbabilityHostRegistrationError> {
    let source_commit = identity.source_commit();
    if is_lower_hex(source_commit, 40) {
        Ok(source_commit)
    } else {
        Err(NativeBuildProbabilityHostRegistrationError::UnverifiedSourceCommit)
    }
}

fn ensure_journal_root(root: &Path) -> Result<(), NativeBuildProbabilityHostRegistrationError> {
    if let Ok(metadata) = fs::symlink_metadata(root) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(NativeBuildProbabilityHostRegistrationError::JournalUnavailable);
        }
    }
    fs::create_dir_all(root)
        .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)
}

fn recover_existing_journals(
    root: &Path,
    source_commit: &str,
    now_unix_ms: u64,
) -> Result<(), NativeBuildProbabilityHostRegistrationError> {
    let entries = fs::read_dir(root)
        .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
    for entry in entries {
        let entry =
            entry.map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let journal = match NativeJsonlDelegationJournal::open_existing_job_for_recovery(&path) {
            Ok(journal) => journal,
            Err(DelegationJournalError::Quarantined { .. }) => continue,
            Err(_) => return Err(NativeBuildProbabilityHostRegistrationError::JournalUnavailable),
        };
        if journal
            .records()
            .iter()
            .any(|event| !coordinator_matches_source(&event.identity.coordinator_id, source_commit))
        {
            let mut journal = journal;
            journal
                .quarantine("journal coordinator source commit is not the current verified build")
                .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
            continue;
        }
        let original_path = journal.path().to_path_buf();
        match DurableDelegationAuthority::recover(journal) {
            Ok(mut authority) => {
                authority
                    .compact_expired_terminal_tombstones(now_unix_ms)
                    .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
            }
            Err(_) if !original_path.exists() => {
                // Replay corruption and every unresolved non-terminal phase are
                // moved to read-only quarantine by the authority recovery.
            }
            Err(_) => return Err(NativeBuildProbabilityHostRegistrationError::JournalUnavailable),
        }
    }
    Ok(())
}

fn create_boot_marker(
    root: &Path,
    source_commit: &str,
    boot_uuid: &str,
) -> Result<File, NativeBuildProbabilityHostRegistrationError> {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, BOOT_MARKER_SCHEMA);
    hash_text(&mut hasher, source_commit);
    hash_text(&mut hasher, boot_uuid);
    let marker_path = root.join(format!("boot-{}.active", hex_digest(hasher.finalize())));
    let mut marker = OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(marker_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                NativeBuildProbabilityHostRegistrationError::BootIdentityAlreadyUsed
            } else {
                NativeBuildProbabilityHostRegistrationError::JournalUnavailable
            }
        })?;
    let body = format!(
        "{{\"schema\":\"{BOOT_MARKER_SCHEMA}\",\"source_commit\":\"{source_commit}\",\"boot_uuid\":\"{boot_uuid}\"}}\n"
    );
    marker
        .write_all(body.as_bytes())
        .and_then(|()| marker.flush())
        .and_then(|()| marker.sync_all())
        .map_err(|_| NativeBuildProbabilityHostRegistrationError::JournalUnavailable)?;
    Ok(marker)
}

fn coordinator_matches_source(coordinator_id: &str, source_commit: &str) -> bool {
    coordinator_id
        .strip_prefix("source-commit:")
        .and_then(|value| value.split_once(";boot:"))
        .is_some_and(|(source, boot)| source == source_commit && is_canonical_uuid(boot))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_registered_native_build_probability(
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse: &BuildProbabilityFinesseRequest,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    retain_private_score_authority: bool,
    control: &ExecutionControl,
) -> Result<NativeBuildProbabilityExecutionOutput, CoreExecutionError> {
    let runtime = REGISTERED_NATIVE_BUILD_PROBABILITY_HOST.get().ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "native_build_probability_host_provider_not_registered",
        },
    )?;
    run_with_registered_host(
        runtime,
        service,
        problem,
        field,
        aggregation,
        finesse,
        solution_probability_policy,
        retain_private_score_authority,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_with_registered_host(
    runtime: &RegisteredNativeBuildProbabilityHost,
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse: &BuildProbabilityFinesseRequest,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    retain_private_score_authority: bool,
    control: &ExecutionControl,
) -> Result<NativeBuildProbabilityExecutionOutput, CoreExecutionError> {
    if finesse.score().is_some() {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "native_build_probability_finesse_score_not_supported",
        });
    }
    let requested_workers = problem.backend_request().workers();
    let request_sha256 = canonical_native_build_probability_request_sha256(
        problem,
        field,
        aggregation,
        finesse.metric(),
        finesse.pattern_knowledge(),
        solution_probability_policy,
        requested_workers,
    );
    let sequence = runtime
        .next_job_sequence
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(1)
        })
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "native_build_probability_job_sequence_exhausted",
        })?;
    let job_id = deterministic_job_uuid(
        &runtime.source_commit,
        &runtime.boot_uuid,
        &request_sha256,
        sequence,
    );
    let durable_identity = NativeBuildProbabilityDurableIdentity::new(
        &job_id,
        &runtime.source_commit,
        &runtime.boot_uuid,
        &request_sha256,
    )?;
    let journal = NativeJsonlDelegationJournal::open_for_job(&runtime.journal_root, &job_id)
        .map_err(map_journal_error)?;
    let mut authority = DurableDelegationAuthority::recover(journal).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "native_build_probability_durable_journal_recovery_failed",
        }
    })?;
    run_provider_admitted_native_build_probability(
        service,
        problem,
        field,
        aggregation,
        finesse.metric(),
        finesse.pattern_knowledge(),
        solution_probability_policy,
        requested_workers,
        retain_private_score_authority,
        control,
        &durable_identity,
        &mut authority,
        runtime.provider.as_ref(),
        &SystemNativeDurableClock,
    )
}

fn map_journal_error(error: DelegationJournalError) -> CoreExecutionError {
    let component = match error {
        DelegationJournalError::Quarantined { .. } => {
            "native_build_probability_durable_journal_quarantined"
        }
        DelegationJournalError::WriterAlreadyActive { .. } => {
            "native_build_probability_durable_journal_writer_active"
        }
        DelegationJournalError::Io(_)
        | DelegationJournalError::Corrupt { .. }
        | DelegationJournalError::HeadChanged { .. }
        | DelegationJournalError::InjectedFailure => {
            "native_build_probability_durable_journal_unavailable"
        }
    };
    CoreExecutionError::RuntimeUnavailable { component }
}

fn deterministic_job_uuid(
    source_commit: &str,
    boot_uuid: &str,
    request_sha256: &str,
    sequence: u64,
) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "clearra.native-build-job.v1");
    hash_text(&mut hasher, source_commit);
    hash_text(&mut hasher, boot_uuid);
    hash_text(&mut hasher, request_sha256);
    hasher.update(sequence.to_le_bytes());
    let digest = hasher.finalize();
    let mut uuid = [0_u8; 16];
    uuid.copy_from_slice(&digest[..16]);
    uuid[6] = (uuid[6] & 0x0f) | 0x50;
    uuid[8] = (uuid[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex_digest(&uuid[..4]),
        hex_digest(&uuid[4..6]),
        hex_digest(&uuid[6..8]),
        hex_digest(&uuid[8..10]),
        hex_digest(&uuid[10..16]),
    )
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
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

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_executor::resource::{DelegationBudget, DelegationIdentity, DelegationPhase};
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{FinessePatternKnowledge, ProblemCompiler};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    const SOURCE_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const BOOT_ONE: &str = "11111111-1111-1111-1111-111111111111";
    const BOOT_TWO: &str = "22222222-2222-2222-2222-222222222222";

    #[derive(Clone)]
    struct ExactMeasurementProvider {
        calls: Arc<AtomicUsize>,
    }

    impl NativeBuildProbabilityAdmissionProvider for ExactMeasurementProvider {
        fn admit_native_build_probability(
            &self,
            request: NativeBuildProbabilityAdmissionRequest,
        ) -> Result<
            NativeBuildProbabilityProviderMeasurement,
            NativeBuildProbabilityHostProviderError,
        > {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            NativeBuildProbabilityProviderMeasurement::new(
                request.worker_stack_bytes(),
                request.minimum_channel_control_bytes(),
                request.minimum_batch_owner_peak_bytes(),
                request.minimum_result_owner_peak_bytes(),
            )
            .ok_or_else(|| {
                NativeBuildProbabilityHostProviderError::new(
                    "test_native_build_probability_measurement_unavailable",
                )
            })
        }
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new(label: &str) -> Self {
            static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            Self(env::temp_dir().join(format!(
                "clearra-native-build-host-{label}-{}-{nonce}-{}",
                std::process::id(),
                NEXT_ROOT.fetch_add(1, Ordering::SeqCst),
            )))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            if self.0.exists() {
                fs::remove_dir_all(&self.0).expect("remove owned native Build test root");
            }
        }
    }

    fn provider(calls: &Arc<AtomicUsize>) -> ExactMeasurementProvider {
        ExactMeasurementProvider {
            calls: Arc::clone(calls),
        }
    }

    fn product_identity(source_commit: &str) -> ProductBuildIdentity {
        ProductBuildIdentity::from_owned_memory_authorized_parts(
            SOURCE_COMMIT.to_owned(),
            source_commit.to_owned(),
            "clearra.search.contract.v2".to_owned(),
            "clearra.supply.projected-terminal-lookahead.v1".to_owned(),
            "clearra.solution-data.v1".to_owned(),
        )
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
                .with_worker_hardware_limit(workers)
                .with_max_candidates(1_024),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece problem")
    }

    fn one_piece_auto_score_problem(hardware_limit: usize) -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_objective(ObjectivePolicy::unique().with_score_summary())
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_worker_hardware_limit(hardware_limit)
                .with_max_candidates(1_024),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece auto score problem")
    }

    fn field() -> BuildProbabilityField {
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target")
    }

    fn jsonl_paths(root: &Path) -> Vec<PathBuf> {
        let mut paths = fs::read_dir(root)
            .expect("read native Build journal root")
            .map(|entry| entry.expect("journal entry").path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    #[test]
    fn registered_host_runs_actual_provider_admitted_pipeline_and_persists_terminal_journal() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let root = TestRoot::new("actual");
        let calls = Arc::new(AtomicUsize::new(0));
        let registration = NativeBuildProbabilityHostRegistration::new(
            provider(&calls),
            root.path().to_path_buf(),
            BOOT_ONE,
        )
        .expect("registration request");
        let runtime = prepare_registered_host(registration, &product_identity(SOURCE_COMMIT))
            .expect("verified registered host");
        let problem = one_piece_problem(2);
        let result = run_with_registered_host(
            &runtime,
            AppCoreExecutorService::wasm_cpu(),
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            &BuildProbabilityFinesseRequest::Search {
                pattern_knowledge: FinessePatternKnowledge::Both,
            },
            BuildSolutionProbabilityPolicy::Omit,
            false,
            &ExecutionControl::default(),
        )
        .expect("registered durable Build result")
        .into_result();

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(result.usize_field("workers_used"), Some(2));
        assert_eq!(
            result.field("cpu_parallel_decision_reason"),
            Some("native-ready-worker-build-probability-pipeline")
        );
        let journals = jsonl_paths(root.path());
        assert_eq!(journals.len(), 1);
        drop(runtime);

        let journal = NativeJsonlDelegationJournal::open_existing_job_for_recovery(&journals[0])
            .expect("reopen terminal journal");
        assert!(journal.job_id().is_some_and(is_canonical_uuid));
        assert!(journal.records().len() >= 24);
        assert_eq!(
            journal
                .records()
                .iter()
                .filter(|event| event.phase == DelegationPhase::Completed)
                .count(),
            journal
                .records()
                .iter()
                .filter(|event| event.phase == DelegationPhase::Prepared)
                .count(),
        );
        DurableDelegationAuthority::recover(journal).expect("terminal journal is recoverable");
    }

    #[test]
    fn registered_host_retains_auto_reserved_native_score_derivation_through_ack() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let root = TestRoot::new("auto-score");
        let calls = Arc::new(AtomicUsize::new(0));
        let registration = NativeBuildProbabilityHostRegistration::new(
            provider(&calls),
            root.path().to_path_buf(),
            BOOT_TWO,
        )
        .expect("registration request");
        let runtime = prepare_registered_host(registration, &product_identity(SOURCE_COMMIT))
            .expect("verified registered host");
        let problem = one_piece_auto_score_problem(8);
        assert_eq!(problem.backend_request().workers(), 7);

        let output = run_with_registered_host(
            &runtime,
            AppCoreExecutorService::wasm_cpu(),
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            &BuildProbabilityFinesseRequest::Off,
            BuildSolutionProbabilityPolicy::Omit,
            true,
            &ExecutionControl::default(),
        )
        .expect("registered durable Build score result");
        let (result, derivation) = output.into_parts();
        let derivation = derivation.expect("typed score derivation survives durable completion");

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(result.usize_field("workers_used"), Some(7));
        assert_eq!(result.bool_field("score_summary_complete"), Some(true));
        assert!(derivation.execution_source_complete());
        assert_eq!(derivation.solution_field_average_owner().len(), 1);
        assert!(!derivation.pattern_winners().is_empty());
        let journals = jsonl_paths(root.path());
        assert_eq!(journals.len(), 1);
        let journal = NativeJsonlDelegationJournal::open_existing_job_for_recovery(&journals[0])
            .expect("reopen typed terminal journal");
        assert!(journal
            .records()
            .iter()
            .any(|event| event.phase == DelegationPhase::Completed));
        DurableDelegationAuthority::recover(journal)
            .expect("typed terminal journal is recoverable after result acknowledgement");
    }

    #[test]
    fn system_provider_probes_every_requested_worker_and_runs_the_durable_pipeline() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let root = TestRoot::new("system-provider");
        let registration = NativeBuildProbabilityHostRegistration::new(
            SystemNativeBuildProbabilityAdmissionProvider,
            root.path().to_path_buf(),
            BOOT_TWO,
        )
        .expect("system-provider registration request");
        let runtime = prepare_registered_host(registration, &product_identity(SOURCE_COMMIT))
            .expect("verified system-provider host");
        let problem = one_piece_problem(4);

        let result = run_with_registered_host(
            &runtime,
            AppCoreExecutorService::wasm_cpu(),
            &problem,
            field(),
            BuildProbabilityAggregation::Buildability,
            &BuildProbabilityFinesseRequest::Search {
                pattern_knowledge: FinessePatternKnowledge::Both,
            },
            BuildSolutionProbabilityPolicy::Omit,
            false,
            &ExecutionControl::default(),
        )
        .expect("system-provider durable Build result")
        .into_result();

        // Four total executor participants means three bounded native worker
        // threads plus the coordinator/producer, and the provider probes all
        // three worker stacks in one simultaneously-live wave.
        assert_eq!(result.usize_field("workers_used"), Some(3));
        let journals = jsonl_paths(root.path());
        assert_eq!(journals.len(), 1);
        let journal = NativeJsonlDelegationJournal::open_existing_job_for_recovery(&journals[0])
            .expect("reopen system-provider terminal journal");
        assert!(journal
            .records()
            .iter()
            .any(|event| event.phase == DelegationPhase::Completed));
        DurableDelegationAuthority::recover(journal)
            .expect("system-provider terminal journal is recoverable");
    }

    #[test]
    fn source_commit_and_boot_identity_fail_closed_before_provider_registration() {
        let root = TestRoot::new("identity");
        let calls = Arc::new(AtomicUsize::new(0));
        let registration = NativeBuildProbabilityHostRegistration::new(
            provider(&calls),
            root.path().to_path_buf(),
            BOOT_ONE,
        )
        .expect("registration request");
        let error =
            prepare_registered_host(registration, &product_identity("unverified-local-build"))
                .err();
        assert_eq!(
            error,
            Some(NativeBuildProbabilityHostRegistrationError::UnverifiedSourceCommit)
        );
        assert!(!root.path().exists());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);

        let first = prepare_registered_host(
            NativeBuildProbabilityHostRegistration::new(
                provider(&calls),
                root.path().to_path_buf(),
                BOOT_ONE,
            )
            .expect("first registration request"),
            &product_identity(SOURCE_COMMIT),
        )
        .expect("first boot identity");
        drop(first);
        let reused = prepare_registered_host(
            NativeBuildProbabilityHostRegistration::new(
                provider(&calls),
                root.path().to_path_buf(),
                BOOT_ONE,
            )
            .expect("reused registration request"),
            &product_identity(SOURCE_COMMIT),
        )
        .err();
        assert_eq!(
            reused,
            Some(NativeBuildProbabilityHostRegistrationError::BootIdentityAlreadyUsed)
        );
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn startup_moves_unresolved_matching_source_journal_to_quarantine_before_new_boot() {
        let root = TestRoot::new("restart");
        fs::create_dir_all(root.path()).expect("create restart root");
        let job_id = "33333333-3333-3333-3333-333333333333";
        let original_path;
        {
            let journal = NativeJsonlDelegationJournal::open_for_job(root.path(), job_id)
                .expect("open crashed job journal");
            original_path = journal.path().to_path_buf();
            let mut authority =
                DurableDelegationAuthority::recover(journal).expect("fresh authority");
            authority
                .prepare(
                    DelegationIdentity::new(
                        job_id,
                        format!("{job_id}:1:initialize"),
                        format!("source-commit:{SOURCE_COMMIT};boot:{BOOT_ONE}"),
                        "11".repeat(32),
                        "22".repeat(32),
                    )
                    .expect("durable identity"),
                    DelegationBudget::new(1, 4096).expect("durable budget"),
                    100,
                )
                .expect("prepared event before crash");
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = prepare_registered_host(
            NativeBuildProbabilityHostRegistration::new(
                provider(&calls),
                root.path().to_path_buf(),
                BOOT_TWO,
            )
            .expect("restart registration"),
            &product_identity(SOURCE_COMMIT),
        )
        .expect("restart quarantines unresolved work");
        assert!(!original_path.exists());
        assert!(fs::read_dir(root.path())
            .expect("restart root")
            .map(|entry| entry.expect("restart entry").file_name())
            .any(|name| name.to_string_lossy().contains(".quarantine-")));
        assert!(matches!(
            NativeJsonlDelegationJournal::open_existing_job_for_recovery(&original_path),
            Err(DelegationJournalError::Quarantined { .. })
        ));
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        drop(runtime);
    }

    #[test]
    fn deterministic_job_uuid_is_canonical_stable_and_sequence_bound() {
        let first = deterministic_job_uuid(
            SOURCE_COMMIT,
            BOOT_TWO,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            7,
        );
        let repeated = deterministic_job_uuid(
            SOURCE_COMMIT,
            BOOT_TWO,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            7,
        );
        let next = deterministic_job_uuid(
            SOURCE_COMMIT,
            BOOT_TWO,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            8,
        );
        assert_eq!(first, repeated);
        assert_eq!(first, "10882de6-7712-5a8a-af52-68e4c97e30e2");
        assert_ne!(first, next);
        assert!(is_canonical_uuid(&first));
    }
}
