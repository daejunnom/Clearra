// SRP rationale: this module owns only the resumable, no-I/O composition of a
// target-qualified fixed queue with the qualified graph-record cache and the
// complete PC candidate reducer boundary. Range transport, fallback, product
// routing, objectives, presentation, and future-PC probability remain outside.

use core::{convert::Infallible, fmt, num::NonZeroUsize};

use clearra_pc4_tablebase::{
    prepare_fixed_queue_traversal_family, ActivatedSnapshot, ConcretePathMaterializationBudgets,
    ConcretePathMaterializationError, FixedQueueTraversalBudgets, FixedQueueTraversalError,
    FixedQueueTraversalFamilyRequest, FixedQueueTraversalPageBudgets, FixedQueueTraversalPageError,
    FixedQueueTraversalPrepareError, LookupHit, Pc4GraphPiece, Pc4TerminalFieldIdentity,
    PlacementMaterializationError, TerminalDepthContract,
};

use super::{
    online_pc4_lookup_session::AppQualifiedPc4LookupHit,
    pc4_lookup_graph_runtime_adapter::{
        Pc4LookupAdjacencyError, Pc4LookupGraphCache, Pc4LookupGraphCacheAdmission,
        Pc4LookupGraphCacheError, Pc4LookupGraphCacheLimits, Pc4LookupGraphCacheStartError,
        Pc4LookupMaterializationError,
    },
    pc_candidate_page_boundary::{
        graph_candidate_adapter::{
            prepare_pc4_graph_candidate_stream, ManifestQualifiedPc4Terminal,
            Pc4GraphCandidateAdapterBudgets, Pc4GraphCandidateAdapterRequest,
            Pc4GraphCandidateFamily, Pc4GraphCandidatePageError, Pc4GraphCandidatePrepareError,
            Pc4GraphCandidateSessionError, Pc4GraphCandidateStreamSession,
        },
        PcCandidateReducerInput, PcCandidateSourceBinding,
    },
};

type CandidateAdvanceError = Pc4GraphCandidatePrepareError<
    Pc4LookupAdjacencyError,
    Infallible,
    Pc4LookupMaterializationError,
>;

/// Immutable preparation request for one disclosed fixed queue.
///
/// Construction performs no graph lookup and no placement materialization.
pub(crate) struct Pc4FixedQueueCandidateRuntimeRequest<'a> {
    pub activated_snapshot: &'a ActivatedSnapshot,
    pub target: &'a clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
    pub source: &'a PcCandidateSourceBinding,
    pub start_field_id: u32,
    pub queue: &'a [Pc4GraphPiece],
    pub terminal_depth_contract: TerminalDepthContract,
    pub traversal_budgets: FixedQueueTraversalBudgets,
    pub traversal_page_budgets: FixedQueueTraversalPageBudgets,
    pub materialization_budgets: ConcretePathMaterializationBudgets,
    pub candidate_budgets: Pc4GraphCandidateAdapterBudgets,
    pub cache_limits: Pc4LookupGraphCacheLimits,
    pub observation_page_size: NonZeroUsize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Pc4FixedQueueCandidateRuntimeStartError {
    Cache(Pc4LookupGraphCacheStartError),
    Traversal(FixedQueueTraversalPrepareError),
    Candidate(Pc4GraphCandidateSessionError),
}

impl Pc4FixedQueueCandidateRuntimeStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cache(error) => error.reason(),
            Self::Traversal(error) => error.reason(),
            Self::Candidate(error) => error.reason(),
        }
    }
}

impl fmt::Display for Pc4FixedQueueCandidateRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Pc4FixedQueueCandidateRuntimeAdmissionError {
    TargetMismatch,
    TerminalFieldIdentityMismatch {
        expected: Pc4TerminalFieldIdentity,
        actual_field_id: u32,
        actual_field_hash: u64,
    },
    Cache(Pc4LookupGraphCacheError),
}

impl Pc4FixedQueueCandidateRuntimeAdmissionError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TargetMismatch => "pc4_fixed_queue_candidate_lookup_target_mismatch",
            Self::TerminalFieldIdentityMismatch { .. } => {
                "pc4_fixed_queue_candidate_terminal_field_identity_mismatch"
            }
            Self::Cache(error) => error.reason(),
        }
    }
}

impl fmt::Display for Pc4FixedQueueCandidateRuntimeAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Pc4FixedQueueCandidateRuntimeAdvanceError {
    CompletionUnavailable,
    Candidate(CandidateAdvanceError),
    CompletionSession(Pc4GraphCandidateSessionError),
    Completion(Pc4GraphCandidatePageError),
}

impl Pc4FixedQueueCandidateRuntimeAdvanceError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::CompletionUnavailable => "pc4_fixed_queue_candidate_completion_unavailable",
            Self::Candidate(error) => error.reason(),
            Self::CompletionSession(error) => error.reason(),
            Self::Completion(error) => error.reason(),
        }
    }
}

impl fmt::Display for Pc4FixedQueueCandidateRuntimeAdvanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// One bounded no-I/O state-machine result.
///
/// `NeedLookup` is a request to the owner; this module never performs the
/// lookup itself. `Advanced` exposes counts only, never partial candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pc4FixedQueueCandidateRuntimeStep {
    NeedLookup(u32),
    Advanced {
        observed_replays: usize,
        observed_candidates: usize,
    },
    Complete {
        replay_provenances: usize,
        canonical_candidates: usize,
    },
}

enum Pc4FixedQueueCandidateRuntimeState {
    Running(Box<Pc4GraphCandidateStreamSession>),
    Complete(Box<Pc4FixedQueueCandidateCompletion>),
    Poisoned,
}

struct Pc4FixedQueueCandidateCompletion {
    family: Pc4GraphCandidateFamily,
    reducer_input: PcCandidateReducerInput,
}

/// Resumable exact fixed-queue candidate composition over one activated
/// generation, profile, use case, and target.
///
/// The cache retains every admitted qualified graph record. Both traversal and
/// materialization borrow that same cache, so a lookup miss can be supplied and
/// retried without rebuilding or silently switching generations. Completion is
/// minted only after graph traversal and every concrete ILC materialization are
/// exhausted.
pub(crate) struct Pc4FixedQueueCandidateRuntime {
    cache: Pc4LookupGraphCache,
    terminal: ManifestQualifiedPc4Terminal,
    observation_page_size: NonZeroUsize,
    state: Pc4FixedQueueCandidateRuntimeState,
}

impl Pc4FixedQueueCandidateRuntime {
    pub fn prepare<G>(
        request: Pc4FixedQueueCandidateRuntimeRequest<'_>,
        guard: &G,
    ) -> Result<Self, Pc4FixedQueueCandidateRuntimeStartError>
    where
        G: super::pc_candidate_page_boundary::graph_candidate_adapter::Pc4GraphCandidateGuard,
    {
        let cache = Pc4LookupGraphCache::new(
            request.activated_snapshot,
            request.target.clone(),
            request.cache_limits,
        )
        .map_err(Pc4FixedQueueCandidateRuntimeStartError::Cache)?;
        let traversal = prepare_fixed_queue_traversal_family(
            FixedQueueTraversalFamilyRequest::new(
                request.target,
                request.start_field_id,
                request.queue,
                request.terminal_depth_contract,
                request.traversal_budgets,
                request.traversal_page_budgets,
            ),
            guard,
        )
        .map_err(Pc4FixedQueueCandidateRuntimeStartError::Traversal)?;
        let stream = prepare_pc4_graph_candidate_stream(
            Pc4GraphCandidateAdapterRequest::new(
                request.target,
                request.source,
                request.start_field_id,
                request.materialization_budgets,
                request.candidate_budgets,
            ),
            &traversal,
            guard,
        )
        .map_err(Pc4FixedQueueCandidateRuntimeStartError::Candidate)?;
        Ok(Self {
            cache,
            terminal: ManifestQualifiedPc4Terminal::new(request.target.clone()),
            observation_page_size: request.observation_page_size,
            state: Pc4FixedQueueCandidateRuntimeState::Running(Box::new(stream)),
        })
    }

    pub const fn target(&self) -> &clearra_pc4_tablebase::QualifiedPc4TargetIdentity {
        self.cache.target()
    }

    pub const fn completed_reducer_input(&self) -> Option<&PcCandidateReducerInput> {
        match &self.state {
            Pc4FixedQueueCandidateRuntimeState::Complete(completion) => {
                Some(&completion.reducer_input)
            }
            Pc4FixedQueueCandidateRuntimeState::Running(_)
            | Pc4FixedQueueCandidateRuntimeState::Poisoned => None,
        }
    }

    pub const fn completed_candidate_family(&self) -> Option<&Pc4GraphCandidateFamily> {
        match &self.state {
            Pc4FixedQueueCandidateRuntimeState::Complete(completion) => Some(&completion.family),
            Pc4FixedQueueCandidateRuntimeState::Running(_)
            | Pc4FixedQueueCandidateRuntimeState::Poisoned => None,
        }
    }

    /// Admits a hit minted by the target-bound online lookup session. The
    /// terminal field's ID and Hydra hash must agree as one manifest-owned
    /// identity; matching only one half fails closed.
    pub fn admit_lookup_hit(
        &mut self,
        hit: AppQualifiedPc4LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4FixedQueueCandidateRuntimeAdmissionError> {
        let (lookup_target, lookup) = hit.into_parts();
        self.admit_lookup_parts(&lookup_target, lookup)
    }

    fn admit_lookup_parts(
        &mut self,
        lookup_target: &clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
        lookup: LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4FixedQueueCandidateRuntimeAdmissionError> {
        if lookup_target != self.cache.target() {
            return Err(Pc4FixedQueueCandidateRuntimeAdmissionError::TargetMismatch);
        }
        let terminal = self.cache.target().terminal_field();
        let matches_terminal_id = lookup.field_id == terminal.field_id();
        let matches_terminal_hash = lookup.field_hash == terminal.field_hash();
        if matches_terminal_id != matches_terminal_hash {
            return Err(
                Pc4FixedQueueCandidateRuntimeAdmissionError::TerminalFieldIdentityMismatch {
                    expected: terminal,
                    actual_field_id: lookup.field_id,
                    actual_field_hash: lookup.field_hash,
                },
            );
        }
        self.cache
            .admit(lookup_target, lookup)
            .map_err(Pc4FixedQueueCandidateRuntimeAdmissionError::Cache)
    }

    /// Performs at most one bounded candidate observation page. A missing
    /// record is returned as data and leaves stream state uncommitted, allowing
    /// an exact retry after `admit_lookup_hit`.
    pub fn advance<G>(
        &mut self,
        guard: &G,
    ) -> Result<Pc4FixedQueueCandidateRuntimeStep, Pc4FixedQueueCandidateRuntimeAdvanceError>
    where
        G: super::pc_candidate_page_boundary::graph_candidate_adapter::Pc4GraphCandidateGuard,
    {
        if let Pc4FixedQueueCandidateRuntimeState::Complete(completion) = &self.state {
            return Ok(Pc4FixedQueueCandidateRuntimeStep::Complete {
                replay_provenances: completion.family.replay_provenance_count(),
                canonical_candidates: completion.family.candidate_count(),
            });
        }
        if matches!(self.state, Pc4FixedQueueCandidateRuntimeState::Poisoned) {
            return Err(Pc4FixedQueueCandidateRuntimeAdvanceError::CompletionUnavailable);
        }

        let result = {
            let cache = &self.cache;
            let terminal = &mut self.terminal;
            let Pc4FixedQueueCandidateRuntimeState::Running(stream) = &mut self.state else {
                return Err(Pc4FixedQueueCandidateRuntimeAdvanceError::CompletionUnavailable);
            };
            let mut provider = cache.adjacency_provider();
            let mut materializer = cache.placement_materializer();
            stream.next_observation_page(
                self.observation_page_size,
                &mut provider,
                terminal,
                &mut materializer,
                guard,
            )
        };

        match result {
            Ok(_) => {
                let (is_exhausted, observed_replays, observed_candidates) = match &self.state {
                    Pc4FixedQueueCandidateRuntimeState::Running(stream) => (
                        stream.is_exhausted(),
                        stream.observed_replay_count(),
                        stream.observed_candidate_count(),
                    ),
                    _ => unreachable!("advance did not replace runtime state"),
                };
                if is_exhausted {
                    self.seal(guard)
                } else {
                    Ok(Pc4FixedQueueCandidateRuntimeStep::Advanced {
                        observed_replays,
                        observed_candidates,
                    })
                }
            }
            Err(error) => {
                if let Some(field_id) = required_lookup_field(&error) {
                    Ok(Pc4FixedQueueCandidateRuntimeStep::NeedLookup(field_id))
                } else {
                    Err(Pc4FixedQueueCandidateRuntimeAdvanceError::Candidate(error))
                }
            }
        }
    }

    fn seal<G>(
        &mut self,
        guard: &G,
    ) -> Result<Pc4FixedQueueCandidateRuntimeStep, Pc4FixedQueueCandidateRuntimeAdvanceError>
    where
        G: super::pc_candidate_page_boundary::graph_candidate_adapter::Pc4GraphCandidateGuard,
    {
        let state = core::mem::replace(
            &mut self.state,
            Pc4FixedQueueCandidateRuntimeState::Poisoned,
        );
        let Pc4FixedQueueCandidateRuntimeState::Running(stream) = state else {
            unreachable!("only an exhausted running stream can be sealed")
        };
        let family = (*stream)
            .finish(guard)
            .map_err(Pc4FixedQueueCandidateRuntimeAdvanceError::CompletionSession)?;
        let replay_provenances = family.replay_provenance_count();
        let canonical_candidates = family.candidate_count();
        let reducer_input = family
            .try_reducer_input(guard)
            .map_err(Pc4FixedQueueCandidateRuntimeAdvanceError::Completion)?;
        self.state = Pc4FixedQueueCandidateRuntimeState::Complete(Box::new(
            Pc4FixedQueueCandidateCompletion {
                family,
                reducer_input,
            },
        ));
        Ok(Pc4FixedQueueCandidateRuntimeStep::Complete {
            replay_provenances,
            canonical_candidates,
        })
    }
}

fn required_lookup_field(error: &CandidateAdvanceError) -> Option<u32> {
    match error {
        Pc4GraphCandidatePrepareError::Traversal(FixedQueueTraversalError::Provider(
            Pc4LookupAdjacencyError::RecordRequired { field_id },
        ))
        | Pc4GraphCandidatePrepareError::TraversalPage(FixedQueueTraversalPageError::Provider(
            Pc4LookupAdjacencyError::RecordRequired { field_id },
        )) => Some(*field_id),
        Pc4GraphCandidatePrepareError::Materialization(
            ConcretePathMaterializationError::Edge {
                source:
                    PlacementMaterializationError::Materializer(
                        Pc4LookupMaterializationError::SourceRecordRequired { field_id }
                        | Pc4LookupMaterializationError::TargetRecordRequired { field_id },
                    ),
                ..
            },
        ) => Some(*field_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use core::{cell::Cell, num::NonZeroU64};

    use clearra_pc4_tablebase::{
        clearra_board64_mask_to_hydra_field_hash_v1, ArtifactDescriptor, DatasetSnapshotManifest,
        DatasetSnapshotVerifier, FieldIdIndexRelation, GraphTargetEncoding, LookupSessionId,
        ManifestContentIdentity, MaterializationGuard, Pc4ArtifactRole, Pc4PlacementMaterializer,
        Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalFieldIdentity,
        Pc4TerminalUseCase, ProfileAvailability, ProfileQualification,
        ProfileTargetCompletenessQualification, QualifiedSnapshotIdentity, SnapshotIdentity,
        SnapshotVerificationAttestation, SnapshotVerificationFailure, SnapshotVerificationRequest,
    };

    use super::*;
    use crate::pc_candidate_page_boundary::{
        PcCandidatePageGuard, PcCandidateRequestIdentity, PcCandidateSessionId,
        PcCandidateSourceIdentity,
    };

    const EMPTY_TARGETS: &[u32] = &[];
    const INITIAL_BOARD: u64 = 0b00_0011_1111;
    const TERMINAL_BOARD: u64 = 0b11_1111_1111;

    struct Verifier;

    impl DatasetSnapshotVerifier for Verifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "fixed-queue-runtime-test-attestation",
            )
            .map_err(|_| SnapshotVerificationFailure::Rejected)
        }
    }

    struct Guard {
        source: PcCandidateSourceBinding,
        cancelled: Cell<bool>,
        source_current: Cell<bool>,
        snapshot_current: Cell<bool>,
    }

    impl Guard {
        fn new(source: PcCandidateSourceBinding) -> Self {
            Self {
                source,
                cancelled: Cell::new(false),
                source_current: Cell::new(true),
                snapshot_current: Cell::new(true),
            }
        }
    }

    impl clearra_pc4_tablebase::FixedQueueTraversalGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            self.snapshot_current.get()
                && self
                    .source
                    .qualified_snapshot()
                    .is_some_and(|actual| actual == expected)
        }
    }

    impl MaterializationGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            self.snapshot_current.get()
                && self
                    .source
                    .qualified_snapshot()
                    .is_some_and(|actual| actual == expected)
        }
    }

    impl PcCandidatePageGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
            self.source_current.get() && source == &self.source
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            self.snapshot_current.get()
                && self
                    .source
                    .qualified_snapshot()
                    .is_some_and(|actual| actual == expected)
        }
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("non-zero test budget")
    }

    fn activated_snapshot(generation: &str) -> ActivatedSnapshot {
        let target_lines = Pc4TargetLines::new(1).expect("1L target");
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                let prefix = profile.as_str();
                let descriptor = |role, suffix: &str, byte_len| {
                    ArtifactDescriptor::new(
                        role,
                        format!("{prefix}/{suffix}"),
                        byte_len,
                        format!("{generation}-{prefix}-{suffix}"),
                    )
                    .expect("synthetic artifact")
                };
                let profile_manifest = Pc4ProfileManifest::new(
                    profile,
                    2,
                    GraphTargetEncoding::U24LittleEndian,
                    FieldIdIndexRelation::RecordOrdinal,
                    4_096,
                    descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 32),
                    descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 28),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 1_024),
                    ProfileQualification::new(
                        format!("{prefix}-index"),
                        format!("{prefix}-graph"),
                        format!("{prefix}-provenance"),
                        format!("{prefix}-kat"),
                    )
                    .expect("profile qualification"),
                )
                .expect("profile manifest")
                .with_target_qualifications(vec![ProfileTargetCompletenessQualification::new(
                    Pc4TerminalUseCase::PcSearch,
                    target_lines,
                    Pc4TerminalFieldIdentity::full_rows(target_lines, 1),
                    format!("{prefix}-pc-terminal"),
                    format!("{prefix}-pc-outgoing"),
                    format!("{prefix}-pc-kat"),
                    format!("{prefix}-pc-parity"),
                )
                .expect("target qualification")])
                .expect("target qualification set");
                ProfileAvailability::qualified(profile_manifest)
            })
            .collect();
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                generation,
            )
            .expect("snapshot identity"),
            ManifestContentIdentity::new(format!("manifest-{generation}"))
                .expect("manifest identity"),
            profiles,
        )
        .expect("manifest")
        .activate(&mut Verifier)
        .expect("activated snapshot")
    }

    fn target(snapshot: &ActivatedSnapshot) -> clearra_pc4_tablebase::QualifiedPc4TargetIdentity {
        snapshot
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(1).expect("1L target"),
            )
            .expect("qualified target")
    }

    fn source(
        target: &clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
    ) -> PcCandidateSourceBinding {
        PcCandidateSourceBinding::online_pc4(
            PcCandidateSessionId::new(NonZeroU64::new(7).expect("candidate session")),
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            PcCandidateSourceIdentity::from_sha256([2; 32]),
            target.profile(),
            INITIAL_BOARD,
            target.snapshot().clone(),
        )
    }

    fn runtime<'a>(
        snapshot: &'a ActivatedSnapshot,
        target: &'a clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
        source: &'a PcCandidateSourceBinding,
        guard: &Guard,
    ) -> Pc4FixedQueueCandidateRuntime {
        let queue = [Pc4GraphPiece::I];
        Pc4FixedQueueCandidateRuntime::prepare(
            Pc4FixedQueueCandidateRuntimeRequest {
                activated_snapshot: snapshot,
                target,
                source,
                start_field_id: 0,
                queue: &queue,
                terminal_depth_contract: TerminalDepthContract::QueueExhaustedOnly,
                traversal_budgets: FixedQueueTraversalBudgets::new(
                    nonzero(16),
                    nonzero(16),
                    nonzero(1),
                    nonzero(16),
                ),
                traversal_page_budgets: FixedQueueTraversalPageBudgets::new(
                    nonzero(16),
                    nonzero(16),
                ),
                materialization_budgets: ConcretePathMaterializationBudgets::new(
                    nonzero(1),
                    nonzero(16),
                    nonzero(16),
                    nonzero(16),
                ),
                candidate_budgets: Pc4GraphCandidateAdapterBudgets::new(
                    nonzero(16),
                    nonzero(1),
                    nonzero(16),
                    nonzero(16),
                    nonzero(16),
                    nonzero(1),
                ),
                cache_limits: Pc4LookupGraphCacheLimits::new(
                    nonzero(2),
                    nonzero(1_024),
                    nonzero(1_024),
                ),
                observation_page_size: nonzero(1),
            },
            guard,
        )
        .expect("prepared fixed queue runtime")
    }

    fn hydra_record(source_hash: u64, per_piece_targets: [&[u32]; 7]) -> Vec<u8> {
        let mut bytes = source_hash.to_be_bytes()[3..].to_vec();
        for targets in per_piece_targets {
            bytes.push(u8::try_from(targets.len()).expect("small test degree"));
            for target in targets {
                bytes.extend_from_slice(&target.to_le_bytes()[..3]);
            }
        }
        bytes
    }

    fn hit(
        target: &clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
        lookup_session: u64,
        field_id: u32,
        field_hash: u64,
        i_targets: &[u32],
    ) -> LookupHit {
        LookupHit {
            lookup_session: LookupSessionId::new(lookup_session).expect("lookup session"),
            snapshot: target.snapshot().clone(),
            profile: target.profile(),
            field_id,
            field_hash,
            graph_target_encoding: GraphTargetEncoding::U24LittleEndian,
            graph_record: hydra_record(
                field_hash,
                [
                    i_targets,
                    EMPTY_TARGETS,
                    EMPTY_TARGETS,
                    EMPTY_TARGETS,
                    EMPTY_TARGETS,
                    EMPTY_TARGETS,
                    EMPTY_TARGETS,
                ],
            ),
        }
    }

    fn admit_test_hit(
        runtime: &mut Pc4FixedQueueCandidateRuntime,
        target: &clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
        hit: LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4FixedQueueCandidateRuntimeAdmissionError> {
        runtime.admit_lookup_parts(target, hit)
    }

    #[test]
    fn miss_supply_resume_preserves_all_exact_replays_and_seals_only_at_exhaustion() {
        let snapshot = activated_snapshot("generation-a");
        let qualified_target = target(&snapshot);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut runtime = runtime(&snapshot, &qualified_target, &candidate_source, &guard);
        assert_eq!(runtime.target(), &qualified_target);
        let _: fn(
            &mut Pc4FixedQueueCandidateRuntime,
            AppQualifiedPc4LookupHit,
        ) -> Result<
            Pc4LookupGraphCacheAdmission,
            Pc4FixedQueueCandidateRuntimeAdmissionError,
        > = Pc4FixedQueueCandidateRuntime::admit_lookup_hit;

        assert_eq!(
            runtime.advance(&guard).expect("source miss"),
            Pc4FixedQueueCandidateRuntimeStep::NeedLookup(0)
        );
        assert!(runtime.completed_reducer_input().is_none());

        let source_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(INITIAL_BOARD).expect("initial Hydra hash");
        admit_test_hit(
            &mut runtime,
            &qualified_target,
            hit(&qualified_target, 1, 0, source_hash, &[1]),
        )
        .expect("source record");
        assert_eq!(
            runtime.advance(&guard).expect("target miss"),
            Pc4FixedQueueCandidateRuntimeStep::NeedLookup(1)
        );
        assert!(runtime.completed_reducer_input().is_none());

        let terminal_hash = clearra_board64_mask_to_hydra_field_hash_v1(TERMINAL_BOARD)
            .expect("terminal Hydra hash");
        admit_test_hit(
            &mut runtime,
            &qualified_target,
            hit(&qualified_target, 2, 1, terminal_hash, EMPTY_TARGETS),
        )
        .expect("terminal record");

        let expected_replays = runtime
            .cache
            .placement_materializer()
            .enumerate(
                &clearra_pc4_tablebase::QualifiedPc4GraphEdge::from_qualified_record(
                    &qualified_target,
                    0,
                    Pc4GraphPiece::I,
                    1,
                ),
            )
            .expect("exact materialization")
            .placements
            .len();
        assert!(expected_replays > 0);

        let final_step = loop {
            let step = runtime.advance(&guard).expect("resumed exact traversal");
            if matches!(step, Pc4FixedQueueCandidateRuntimeStep::Complete { .. }) {
                break step;
            }
            assert!(matches!(
                step,
                Pc4FixedQueueCandidateRuntimeStep::Advanced { .. }
            ));
            assert!(runtime.completed_reducer_input().is_none());
        };
        assert_eq!(
            final_step,
            Pc4FixedQueueCandidateRuntimeStep::Complete {
                replay_provenances: expected_replays,
                canonical_candidates: 1,
            }
        );
        let reducer = runtime
            .completed_reducer_input()
            .expect("complete reducer input");
        let complete_family = runtime
            .completed_candidate_family()
            .expect("complete replay-preserving family");
        assert_eq!(complete_family.replay_provenance_count(), expected_replays);
        assert_eq!(complete_family.candidate_count(), 1);
        assert_eq!(reducer.source(), &candidate_source);
        assert_eq!(reducer.candidates().len(), 1);
        assert_eq!(reducer.candidates()[0].initial_board_mask(), INITIAL_BOARD);
    }

    #[test]
    fn terminal_identity_target_binding_cancellation_and_generation_drift_fail_closed() {
        let snapshot = activated_snapshot("generation-a");
        let qualified_target = target(&snapshot);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut runtime = runtime(&snapshot, &qualified_target, &candidate_source, &guard);
        let source_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(INITIAL_BOARD).expect("initial Hydra hash");

        assert_eq!(
            admit_test_hit(
                &mut runtime,
                &qualified_target,
                hit(&qualified_target, 1, 1, source_hash, EMPTY_TARGETS),
            ),
            Err(
                Pc4FixedQueueCandidateRuntimeAdmissionError::TerminalFieldIdentityMismatch {
                    expected: qualified_target.terminal_field(),
                    actual_field_id: 1,
                    actual_field_hash: source_hash,
                }
            )
        );

        let other_snapshot = activated_snapshot("generation-b");
        let other_target = target(&other_snapshot);
        assert_eq!(
            admit_test_hit(
                &mut runtime,
                &other_target,
                hit(&other_target, 2, 0, source_hash, &[1]),
            ),
            Err(Pc4FixedQueueCandidateRuntimeAdmissionError::TargetMismatch)
        );

        guard.cancelled.set(true);
        assert!(matches!(
            runtime.advance(&guard),
            Err(Pc4FixedQueueCandidateRuntimeAdvanceError::Candidate(
                Pc4GraphCandidatePrepareError::Cancelled
            ))
        ));
        assert!(runtime.completed_reducer_input().is_none());
        guard.cancelled.set(false);
        guard.snapshot_current.set(false);
        assert!(matches!(
            runtime.advance(&guard),
            Err(Pc4FixedQueueCandidateRuntimeAdvanceError::Candidate(
                Pc4GraphCandidatePrepareError::StaleSource
                    | Pc4GraphCandidatePrepareError::StaleSnapshot
            ))
        ));
        assert!(runtime.completed_reducer_input().is_none());
    }

    #[test]
    fn preparation_rejects_a_source_from_another_generation_without_lookup() {
        let snapshot = activated_snapshot("generation-a");
        let qualified_target = target(&snapshot);
        let other_snapshot = activated_snapshot("generation-b");
        let other_target = target(&other_snapshot);
        let wrong_source = source(&other_target);
        let guard = Guard::new(wrong_source.clone());
        let queue = [Pc4GraphPiece::I];
        let result = Pc4FixedQueueCandidateRuntime::prepare(
            Pc4FixedQueueCandidateRuntimeRequest {
                activated_snapshot: &snapshot,
                target: &qualified_target,
                source: &wrong_source,
                start_field_id: 0,
                queue: &queue,
                terminal_depth_contract: TerminalDepthContract::QueueExhaustedOnly,
                traversal_budgets: FixedQueueTraversalBudgets::new(
                    nonzero(4),
                    nonzero(4),
                    nonzero(1),
                    nonzero(4),
                ),
                traversal_page_budgets: FixedQueueTraversalPageBudgets::new(nonzero(4), nonzero(4)),
                materialization_budgets: ConcretePathMaterializationBudgets::new(
                    nonzero(1),
                    nonzero(4),
                    nonzero(4),
                    nonzero(4),
                ),
                candidate_budgets: Pc4GraphCandidateAdapterBudgets::new(
                    nonzero(4),
                    nonzero(1),
                    nonzero(4),
                    nonzero(4),
                    nonzero(4),
                    nonzero(1),
                ),
                cache_limits: Pc4LookupGraphCacheLimits::new(
                    nonzero(2),
                    nonzero(1_024),
                    nonzero(1_024),
                ),
                observation_page_size: nonzero(1),
            },
            &guard,
        );
        assert!(matches!(
            result,
            Err(Pc4FixedQueueCandidateRuntimeStartError::Traversal(
                FixedQueueTraversalPrepareError::StaleSnapshot
            ))
        ));
    }
}
