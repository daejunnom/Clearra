//! No-I/O application owner for one resumable online PC4 fixed-queue candidate session.
//!
//! This module composes the range-admitted online lookup owner with the exact
//! fixed-queue candidate runtime. It owns neither transport nor fallback, and
//! exposes no candidate data until the runtime has sealed complete reducer
//! input.

use core::{fmt, num::NonZeroUsize};

use clearra_pc4_tablebase::{
    ConcretePathMaterializationBudgets, FixedQueueTraversalBudgets, FixedQueueTraversalGuard,
    FixedQueueTraversalPageBudgets, LookupFailure, LookupSessionId, MaterializationGuard,
    Pc4GraphPiece, PinnedPc4Generation, QualifiedPc4TargetIdentity, RangeAdmissionAttempt,
    RangeAdmissionGuard, RangeAdmissionInput, RangeAdmissionLimits, RangeAdmissionUsage,
    RangeRequest, TerminalDepthContract, UnsupportedProfileReason,
};

use super::{
    online_pc4_lookup_session::{
        AppOnlinePc4LookupSession, AppOnlinePc4LookupStartError, AppOnlinePc4LookupStep,
        AppOnlinePc4RangeDisposition, AppOnlinePc4RangeError, Pc4OfflineFallbackAuthorization,
        Pc4OnlineLookupRequest,
    },
    pc4_fixed_queue_candidate_runtime::{
        Pc4FixedQueueCandidateRuntime, Pc4FixedQueueCandidateRuntimeAdmissionError,
        Pc4FixedQueueCandidateRuntimeAdvanceError, Pc4FixedQueueCandidateRuntimeRequest,
        Pc4FixedQueueCandidateRuntimeStartError, Pc4FixedQueueCandidateRuntimeStep,
    },
    pc4_lookup_graph_runtime_adapter::{Pc4LookupGraphCacheLimits, Pc4LookupGraphCacheStartError},
    pc_candidate_page_boundary::{
        graph_candidate_adapter::{Pc4GraphCandidateAdapterBudgets, Pc4GraphCandidatePrepareError},
        PcCandidatePageGuard, PcCandidateReducerInput, PcCandidateSourceBinding,
    },
};

/// Freshness and cancellation authority sampled by every part of the composed
/// session. Implementations perform observation only and must not do I/O.
pub trait AppOnlinePc4FixedQueueCandidateGuard:
    FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard + RangeAdmissionGuard
{
}

impl<T> AppOnlinePc4FixedQueueCandidateGuard for T where
    T: FixedQueueTraversalGuard + MaterializationGuard + PcCandidatePageGuard + RangeAdmissionGuard
{
}

/// Immutable preparation parameters for one fixed queue and one qualified
/// online PC4 source.
pub struct AppOnlinePc4FixedQueueCandidateRequest<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    source: &'a PcCandidateSourceBinding,
    first_lookup_session: LookupSessionId,
    start_field_id: u32,
    queue: &'a [Pc4GraphPiece],
    terminal_depth_contract: TerminalDepthContract,
    traversal_budgets: FixedQueueTraversalBudgets,
    traversal_page_budgets: FixedQueueTraversalPageBudgets,
    materialization_budgets: ConcretePathMaterializationBudgets,
    candidate_budgets: Pc4GraphCandidateAdapterBudgets,
    cache_limits: Pc4LookupGraphCacheLimits,
    observation_page_size: NonZeroUsize,
    range_limits: RangeAdmissionLimits,
}

impl<'a> AppOnlinePc4FixedQueueCandidateRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        target: &'a QualifiedPc4TargetIdentity,
        source: &'a PcCandidateSourceBinding,
        first_lookup_session: LookupSessionId,
        start_field_id: u32,
        queue: &'a [Pc4GraphPiece],
        terminal_depth_contract: TerminalDepthContract,
        traversal_budgets: FixedQueueTraversalBudgets,
        traversal_page_budgets: FixedQueueTraversalPageBudgets,
        materialization_budgets: ConcretePathMaterializationBudgets,
        candidate_budgets: Pc4GraphCandidateAdapterBudgets,
        cache_limits: Pc4LookupGraphCacheLimits,
        observation_page_size: NonZeroUsize,
        range_limits: RangeAdmissionLimits,
    ) -> Self {
        Self {
            target,
            source,
            first_lookup_session,
            start_field_id,
            queue,
            terminal_depth_contract,
            traversal_budgets,
            traversal_page_budgets,
            materialization_budgets,
            candidate_budgets,
            cache_limits,
            observation_page_size,
            range_limits,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4FixedQueueCandidateStartError {
    TargetSnapshotMismatch,
    ProfileNotQualified {
        profile: clearra_pc4_tablebase::Pc4RuleProfile,
        reason: UnsupportedProfileReason,
    },
    PreparationFailed {
        reason: &'static str,
    },
}

impl AppOnlinePc4FixedQueueCandidateStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TargetSnapshotMismatch => "pc4_online_candidate_target_snapshot_mismatch",
            Self::ProfileNotQualified { .. } => "pc4_online_profile_not_qualified",
            Self::PreparationFailed { reason } => reason,
        }
    }

    fn from_runtime(error: Pc4FixedQueueCandidateRuntimeStartError) -> Self {
        match error {
            Pc4FixedQueueCandidateRuntimeStartError::Cache(
                Pc4LookupGraphCacheStartError::TargetSnapshotMismatch,
            ) => Self::TargetSnapshotMismatch,
            Pc4FixedQueueCandidateRuntimeStartError::Cache(
                Pc4LookupGraphCacheStartError::ProfileNotQualified { profile, reason },
            ) => Self::ProfileNotQualified { profile, reason },
            error => Self::PreparationFailed {
                reason: error.reason(),
            },
        }
    }
}

impl fmt::Display for AppOnlinePc4FixedQueueCandidateStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for AppOnlinePc4FixedQueueCandidateStartError {}

/// Typed failure retained by a terminal session. Detailed range admission
/// rejection remains returned directly by `admit_range` and does not mutate
/// the lookup transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4FixedQueueCandidateFailure {
    LookupSessionIdsExhausted {
        field_id: u32,
    },
    LookupStart {
        field_id: u32,
        error: AppOnlinePc4LookupStartError,
    },
    Lookup {
        field_id: u32,
        failure: LookupFailure,
    },
    LookupAdmission {
        field_id: u32,
        reason: &'static str,
    },
    CandidateAdvance {
        reason: &'static str,
    },
    CompletionUnavailable,
}

impl AppOnlinePc4FixedQueueCandidateFailure {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::LookupSessionIdsExhausted { .. } => {
                "pc4_online_candidate_lookup_session_ids_exhausted"
            }
            Self::LookupStart { error, .. } => error.reason(),
            Self::Lookup { failure, .. } => failure.reason(),
            Self::LookupAdmission { reason, .. } | Self::CandidateAdvance { reason } => reason,
            Self::CompletionUnavailable => "pc4_online_candidate_completion_unavailable",
        }
    }

    pub const fn field_id(&self) -> Option<u32> {
        match self {
            Self::LookupSessionIdsExhausted { field_id }
            | Self::LookupStart { field_id, .. }
            | Self::Lookup { field_id, .. }
            | Self::LookupAdmission { field_id, .. } => Some(*field_id),
            Self::CandidateAdvance { .. } | Self::CompletionUnavailable => None,
        }
    }
}

impl fmt::Display for AppOnlinePc4FixedQueueCandidateFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for AppOnlinePc4FixedQueueCandidateFailure {}

/// One externally observable no-I/O step. Progress reports cumulative bounded
/// counts only; candidates are available solely through complete reducer input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4FixedQueueCandidateStep {
    NeedRange(RangeRequest),
    Progress {
        observed_replays: usize,
        observed_candidates: usize,
    },
    Complete {
        replay_provenances: usize,
        canonical_candidates: usize,
    },
    Miss {
        field_id: u32,
    },
    Failed(AppOnlinePc4FixedQueueCandidateFailure),
    Cancelled,
}

struct ActiveLookup {
    field_id: u32,
    session: AppOnlinePc4LookupSession,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TerminalState {
    Complete {
        replay_provenances: usize,
        canonical_candidates: usize,
    },
    Miss {
        field_id: u32,
    },
    Failed(AppOnlinePc4FixedQueueCandidateFailure),
    Cancelled,
}

impl TerminalState {
    fn step(&self) -> AppOnlinePc4FixedQueueCandidateStep {
        match self {
            Self::Complete {
                replay_provenances,
                canonical_candidates,
            } => AppOnlinePc4FixedQueueCandidateStep::Complete {
                replay_provenances: *replay_provenances,
                canonical_candidates: *canonical_candidates,
            },
            Self::Miss { field_id } => AppOnlinePc4FixedQueueCandidateStep::Miss {
                field_id: *field_id,
            },
            Self::Failed(error) => AppOnlinePc4FixedQueueCandidateStep::Failed(error.clone()),
            Self::Cancelled => AppOnlinePc4FixedQueueCandidateStep::Cancelled,
        }
    }
}

/// Resumable owner of a pinned generation, one candidate runtime, and at most
/// one target-field lookup. Successive lookup IDs are allocated from the
/// caller-owned first ID so delayed Range responses cannot bind to a later
/// field lookup.
pub struct AppOnlinePc4FixedQueueCandidateSession {
    generation: PinnedPc4Generation,
    runtime: Pc4FixedQueueCandidateRuntime,
    range_limits: RangeAdmissionLimits,
    next_lookup_session: Option<LookupSessionId>,
    active_lookup: Option<ActiveLookup>,
    terminal: Option<TerminalState>,
}

impl AppOnlinePc4FixedQueueCandidateSession {
    pub fn start<G>(
        generation: PinnedPc4Generation,
        request: AppOnlinePc4FixedQueueCandidateRequest<'_>,
        guard: &G,
    ) -> Result<Self, AppOnlinePc4FixedQueueCandidateStartError>
    where
        G: AppOnlinePc4FixedQueueCandidateGuard,
    {
        let runtime = Pc4FixedQueueCandidateRuntime::prepare(
            Pc4FixedQueueCandidateRuntimeRequest {
                activated_snapshot: generation.activated_snapshot(),
                target: request.target,
                source: request.source,
                start_field_id: request.start_field_id,
                queue: request.queue,
                terminal_depth_contract: request.terminal_depth_contract,
                traversal_budgets: request.traversal_budgets,
                traversal_page_budgets: request.traversal_page_budgets,
                materialization_budgets: request.materialization_budgets,
                candidate_budgets: request.candidate_budgets,
                cache_limits: request.cache_limits,
                observation_page_size: request.observation_page_size,
            },
            guard,
        )
        .map_err(AppOnlinePc4FixedQueueCandidateStartError::from_runtime)?;
        Ok(Self {
            generation,
            runtime,
            range_limits: request.range_limits,
            next_lookup_session: Some(request.first_lookup_session),
            active_lookup: None,
            terminal: None,
        })
    }

    pub const fn generation(&self) -> &PinnedPc4Generation {
        &self.generation
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        self.runtime.target()
    }

    pub fn active_lookup_field_id(&self) -> Option<u32> {
        self.active_lookup.as_ref().map(|active| active.field_id)
    }

    pub fn active_range_usage(&self) -> Option<RangeAdmissionUsage> {
        self.active_lookup
            .as_ref()
            .map(|active| active.session.range_usage())
    }

    /// Returns candidate input only after the composed session has emitted and
    /// retained `Complete`. No running or failed state can expose a partial set.
    pub fn completed_reducer_input(&self) -> Option<&PcCandidateReducerInput> {
        if matches!(self.terminal, Some(TerminalState::Complete { .. })) {
            self.runtime.completed_reducer_input()
        } else {
            None
        }
    }

    /// Advances through internal hit admission until externally observable I/O,
    /// one bounded candidate page, or a terminal state is reached.
    pub fn step<G>(&mut self, guard: &G) -> AppOnlinePc4FixedQueueCandidateStep
    where
        G: AppOnlinePc4FixedQueueCandidateGuard,
    {
        if let Some(terminal) = &self.terminal {
            return terminal.step();
        }

        loop {
            if let Some(active) = &self.active_lookup {
                let field_id = active.field_id;
                match active.session.step() {
                    AppOnlinePc4LookupStep::NeedRange(request) => {
                        return AppOnlinePc4FixedQueueCandidateStep::NeedRange(request);
                    }
                    AppOnlinePc4LookupStep::Hit(hit) => {
                        self.active_lookup = None;
                        if let Err(error) = self.runtime.admit_lookup_hit(hit) {
                            return self.fail_lookup_admission(field_id, error);
                        }
                    }
                    AppOnlinePc4LookupStep::Miss => {
                        return self.finish(TerminalState::Miss { field_id });
                    }
                    AppOnlinePc4LookupStep::Failed(failure) => {
                        return self.finish(TerminalState::Failed(
                            AppOnlinePc4FixedQueueCandidateFailure::Lookup { field_id, failure },
                        ));
                    }
                    AppOnlinePc4LookupStep::Cancelled => {
                        return self.finish(TerminalState::Cancelled);
                    }
                }
                continue;
            }

            match self.runtime.advance(guard) {
                Ok(Pc4FixedQueueCandidateRuntimeStep::NeedLookup(field_id)) => {
                    if let Err(failure) = self.start_lookup(field_id) {
                        return self.finish(TerminalState::Failed(failure));
                    }
                }
                Ok(Pc4FixedQueueCandidateRuntimeStep::Advanced {
                    observed_replays,
                    observed_candidates,
                }) => {
                    return AppOnlinePc4FixedQueueCandidateStep::Progress {
                        observed_replays,
                        observed_candidates,
                    };
                }
                Ok(Pc4FixedQueueCandidateRuntimeStep::Complete {
                    replay_provenances,
                    canonical_candidates,
                }) => {
                    if self.runtime.completed_reducer_input().is_none() {
                        return self.finish(TerminalState::Failed(
                            AppOnlinePc4FixedQueueCandidateFailure::CompletionUnavailable,
                        ));
                    }
                    return self.finish(TerminalState::Complete {
                        replay_provenances,
                        canonical_candidates,
                    });
                }
                Err(error) => {
                    if is_cancelled_advance(&error) {
                        return self.finish(TerminalState::Cancelled);
                    }
                    return self.finish(TerminalState::Failed(
                        AppOnlinePc4FixedQueueCandidateFailure::CandidateAdvance {
                            reason: error.reason(),
                        },
                    ));
                }
            }
        }
    }

    /// Admits exactly one host-produced response to the currently active lookup.
    /// There is no raw response supply path and no transport is performed here.
    pub fn admit_range<G>(
        &mut self,
        attempt: RangeAdmissionAttempt,
        input: RangeAdmissionInput,
        guard: &G,
    ) -> Result<AppOnlinePc4RangeDisposition, AppOnlinePc4RangeError>
    where
        G: RangeAdmissionGuard + ?Sized,
    {
        let Some(active) = &mut self.active_lookup else {
            return Err(AppOnlinePc4RangeError::LookupNotAwaitingRange);
        };
        active.session.admit_range(attempt, input, guard)
    }

    pub fn cancel(&mut self) {
        if self.terminal.is_some() {
            return;
        }
        if let Some(active) = &mut self.active_lookup {
            active.session.cancel();
        }
        self.terminal = Some(TerminalState::Cancelled);
    }

    fn start_lookup(
        &mut self,
        field_id: u32,
    ) -> Result<(), AppOnlinePc4FixedQueueCandidateFailure> {
        debug_assert!(self.active_lookup.is_none());
        let lookup_session = self.next_lookup_session.ok_or(
            AppOnlinePc4FixedQueueCandidateFailure::LookupSessionIdsExhausted { field_id },
        )?;
        self.next_lookup_session = lookup_session
            .get()
            .checked_add(1)
            .and_then(LookupSessionId::new);
        let request = Pc4OnlineLookupRequest::from_field_id(
            lookup_session,
            self.runtime.target().clone(),
            field_id,
            self.range_limits,
            Pc4OfflineFallbackAuthorization::NotAuthorized,
        );
        let session = AppOnlinePc4LookupSession::start(self.generation.clone(), request).map_err(
            |error| AppOnlinePc4FixedQueueCandidateFailure::LookupStart { field_id, error },
        )?;
        self.active_lookup = Some(ActiveLookup { field_id, session });
        Ok(())
    }

    fn fail_lookup_admission(
        &mut self,
        field_id: u32,
        error: Pc4FixedQueueCandidateRuntimeAdmissionError,
    ) -> AppOnlinePc4FixedQueueCandidateStep {
        self.finish(TerminalState::Failed(
            AppOnlinePc4FixedQueueCandidateFailure::LookupAdmission {
                field_id,
                reason: error.reason(),
            },
        ))
    }

    fn finish(&mut self, terminal: TerminalState) -> AppOnlinePc4FixedQueueCandidateStep {
        let step = terminal.step();
        self.terminal = Some(terminal);
        step
    }
}

fn is_cancelled_advance(error: &Pc4FixedQueueCandidateRuntimeAdvanceError) -> bool {
    matches!(
        error,
        Pc4FixedQueueCandidateRuntimeAdvanceError::Candidate(
            Pc4GraphCandidatePrepareError::Cancelled
        )
    )
}

#[cfg(test)]
mod tests {
    use core::{
        cell::Cell,
        num::{NonZeroU16, NonZeroU32, NonZeroU64},
    };
    use std::sync::Arc;

    use clearra_pc4_tablebase::{
        clearra_board64_mask_to_hydra_field_hash_v1, ActivatedSnapshot, ArtifactDescriptor,
        DatasetSnapshotManifest, DatasetSnapshotVerifier, FieldIdIndexRelation,
        GraphTargetEncoding, ManifestContentIdentity, Pc4ArtifactRole, Pc4CurrentGeneration,
        Pc4GenerationRegistry, Pc4GenerationRetentionLimit, Pc4GenerationStageOutcome,
        Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalFieldIdentity,
        Pc4TerminalUseCase, ProfileAvailability, ProfileQualification,
        ProfileTargetCompletenessQualification, QualifiedSnapshotIdentity, RangeHttpResponse,
        RangeResponse, RangeResponseKind, RangeTransportFailure, SnapshotIdentity,
        SnapshotVerificationAttestation, SnapshotVerificationFailure, SnapshotVerificationRequest,
    };

    use super::*;
    use crate::{PcCandidateRequestIdentity, PcCandidateSessionId, PcCandidateSourceIdentity};

    const INITIAL_BOARD: u64 = 0b00_0011_1111;
    const TERMINAL_BOARD: u64 = 0b11_1111_1111;
    const EMPTY_TARGETS: &[u32] = &[];

    struct Verifier;

    impl DatasetSnapshotVerifier for Verifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "online-fixed-queue-candidate-session-test-attestation",
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

    impl FixedQueueTraversalGuard for Guard {
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

    impl RangeAdmissionGuard for Guard {
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

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("non-zero test budget")
    }

    fn range_limits() -> RangeAdmissionLimits {
        RangeAdmissionLimits::new(
            NonZeroU64::new(64).expect("response bytes"),
            NonZeroU64::new(512).expect("session bytes"),
            NonZeroU32::new(16).expect("request count"),
            NonZeroU16::new(1).expect("one active request"),
            60,
        )
    }

    fn attempt(ordinal: u32) -> RangeAdmissionAttempt {
        RangeAdmissionAttempt::new(
            NonZeroU32::new(ordinal).expect("request ordinal"),
            NonZeroU16::new(1).expect("one active request"),
        )
    }

    fn activated_snapshot(
        generation: &str,
        qualified_profile: Option<Pc4RuleProfile>,
    ) -> ActivatedSnapshot {
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
                let manifest = Pc4ProfileManifest::new(
                    profile,
                    2,
                    GraphTargetEncoding::U24LittleEndian,
                    FieldIdIndexRelation::RecordOrdinal,
                    4_096,
                    descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 32),
                    descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 28),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 27),
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
                if qualified_profile.is_none() || qualified_profile == Some(profile) {
                    ProfileAvailability::qualified(manifest)
                } else {
                    ProfileAvailability::Unsupported {
                        profile,
                        reason: UnsupportedProfileReason::MissingProfileArtifacts,
                    }
                }
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

    fn pin(snapshot: ActivatedSnapshot) -> PinnedPc4Generation {
        let mut registry = Pc4GenerationRegistry::new(
            Pc4GenerationRetentionLimit::new(1).expect("one retained generation"),
        );
        let token = match registry
            .stage(registry.version(), Arc::new(snapshot))
            .expect("stage generation")
        {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected staged generation, got {unexpected:?}"),
        };
        registry.promote(&token).expect("promote generation");
        match registry.pin_current() {
            Pc4CurrentGeneration::Current(generation) => generation,
            Pc4CurrentGeneration::NoCurrent => panic!("promoted generation must be current"),
        }
    }

    fn target(snapshot: &ActivatedSnapshot, profile: Pc4RuleProfile) -> QualifiedPc4TargetIdentity {
        snapshot
            .qualified_target(
                profile,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(1).expect("1L target"),
            )
            .expect("qualified target")
    }

    fn source(target: &QualifiedPc4TargetIdentity) -> PcCandidateSourceBinding {
        PcCandidateSourceBinding::online_pc4(
            PcCandidateSessionId::new(NonZeroU64::new(7).expect("candidate session")),
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            PcCandidateSourceIdentity::from_sha256([2; 32]),
            target.profile(),
            INITIAL_BOARD,
            target.snapshot().clone(),
        )
    }

    fn start(
        generation: PinnedPc4Generation,
        target: &QualifiedPc4TargetIdentity,
        source: &PcCandidateSourceBinding,
        guard: &Guard,
    ) -> Result<AppOnlinePc4FixedQueueCandidateSession, AppOnlinePc4FixedQueueCandidateStartError>
    {
        let queue = [Pc4GraphPiece::I];
        AppOnlinePc4FixedQueueCandidateSession::start(
            generation,
            AppOnlinePc4FixedQueueCandidateRequest::new(
                target,
                source,
                LookupSessionId::new(41).expect("lookup session"),
                0,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                FixedQueueTraversalBudgets::new(nonzero(16), nonzero(16), nonzero(1), nonzero(16)),
                FixedQueueTraversalPageBudgets::new(nonzero(16), nonzero(16)),
                ConcretePathMaterializationBudgets::new(
                    nonzero(1),
                    nonzero(16),
                    nonzero(16),
                    nonzero(16),
                ),
                Pc4GraphCandidateAdapterBudgets::new(
                    nonzero(16),
                    nonzero(1),
                    nonzero(16),
                    nonzero(16),
                    nonzero(16),
                    nonzero(1),
                ),
                Pc4LookupGraphCacheLimits::new(nonzero(2), nonzero(1_024), nonzero(1_024)),
                nonzero(1),
                range_limits(),
            ),
            guard,
        )
    }

    fn index_header(magic: [u8; 8], count: u32) -> Vec<u8> {
        let mut bytes = magic.to_vec();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes
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

    struct RangeDataset {
        field_index: Vec<u8>,
        graph_offsets: Vec<u8>,
        graph: Vec<u8>,
    }

    fn range_dataset() -> RangeDataset {
        let source_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(INITIAL_BOARD).expect("source hash");
        let terminal_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(TERMINAL_BOARD).expect("terminal hash");
        let source_record = hydra_record(
            source_hash,
            [
                &[1],
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
            ],
        );
        let terminal_record = hydra_record(
            terminal_hash,
            [
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
            ],
        );

        let mut field_index = index_header(*b"FHIDIDX1", 2);
        for (field_id, field_hash) in [source_hash, terminal_hash].into_iter().enumerate() {
            field_index.extend_from_slice(&field_hash.to_le_bytes()[..5]);
            field_index.extend_from_slice(&(field_id as u32).to_le_bytes()[..3]);
        }
        let mut graph_offsets = index_header(*b"GOFFIDX1", 2);
        for offset in [0_u32, source_record.len() as u32, 27] {
            graph_offsets.extend_from_slice(&offset.to_le_bytes());
        }
        let mut graph = source_record;
        graph.extend_from_slice(&terminal_record);
        assert_eq!(field_index.len(), 32);
        assert_eq!(graph_offsets.len(), 28);
        assert_eq!(graph.len(), 27);
        RangeDataset {
            field_index,
            graph_offsets,
            graph,
        }
    }

    fn partial_input(request: &RangeRequest, dataset: &RangeDataset) -> RangeAdmissionInput {
        let artifact = match request.artifact() {
            Pc4ArtifactRole::FieldHashIndex => &dataset.field_index,
            Pc4ArtifactRole::GraphOffsets => &dataset.graph_offsets,
            Pc4ArtifactRole::Graph => &dataset.graph,
        };
        let begin = request.offset() as usize;
        let end = request.end_exclusive() as usize;
        let response = RangeResponse {
            lookup_session: request.lookup_session(),
            request_id: request.request_id(),
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact(),
            artifact_content_identity: request.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: request.offset(),
            complete_length: artifact.len() as u64,
            bytes: artifact[begin..end].to_vec(),
        };
        RangeAdmissionInput::http(RangeHttpResponse::new(
            206,
            Some(format!(
                "bytes {}-{}/{}",
                request.offset(),
                request.end_exclusive() - 1,
                artifact.len()
            )),
            None,
            Some(response),
        ))
    }

    #[test]
    fn one_active_lookup_rejects_each_profile_unavailable_in_the_pinned_generation() {
        let full = activated_snapshot("generation-partial", None);
        let partial = pin(activated_snapshot(
            "generation-partial",
            Some(Pc4RuleProfile::Srs),
        ));
        assert_eq!(
            full.qualified_identity(),
            partial.activated_snapshot().qualified_identity()
        );

        for profile in Pc4RuleProfile::ALL
            .into_iter()
            .filter(|profile| *profile != Pc4RuleProfile::Srs)
        {
            let unavailable_target = target(&full, profile);
            let unavailable_source = source(&unavailable_target);
            let guard = Guard::new(unavailable_source.clone());
            let error = match start(
                partial.clone(),
                &unavailable_target,
                &unavailable_source,
                &guard,
            ) {
                Err(error) => error,
                Ok(_) => panic!("unqualified profile must fail before range I/O"),
            };
            assert_eq!(
                error,
                AppOnlinePc4FixedQueueCandidateStartError::ProfileNotQualified {
                    profile,
                    reason: UnsupportedProfileReason::MissingProfileArtifacts,
                }
            );
        }

        let supported_target = target(&full, Pc4RuleProfile::Srs);
        let supported_source = source(&supported_target);
        let guard = Guard::new(supported_source.clone());
        let mut session = start(partial, &supported_target, &supported_source, &guard)
            .expect("independently qualified profile");
        let first = session.step(&guard);
        assert!(matches!(
            first,
            AppOnlinePc4FixedQueueCandidateStep::NeedRange(_)
        ));
        assert_eq!(session.active_lookup_field_id(), Some(0));
        assert_eq!(session.step(&guard), first);
        assert_eq!(session.active_lookup_field_id(), Some(0));
    }

    #[test]
    fn source_and_target_ranges_resume_to_one_exact_complete_reducer_family() {
        let generation = pin(activated_snapshot("generation-a", None));
        let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut session = start(generation, &qualified_target, &candidate_source, &guard)
            .expect("online fixed-queue candidate session");
        let dataset = range_dataset();
        let mut active_lookup_session = None;
        let mut request_ordinal = 0;
        let mut looked_up_fields = Vec::new();

        let complete = loop {
            match session.step(&guard) {
                AppOnlinePc4FixedQueueCandidateStep::NeedRange(request) => {
                    if active_lookup_session != Some(request.lookup_session()) {
                        active_lookup_session = Some(request.lookup_session());
                        request_ordinal = 0;
                        looked_up_fields.push(
                            session
                                .active_lookup_field_id()
                                .expect("range belongs to active field"),
                        );
                    }
                    request_ordinal += 1;
                    assert_eq!(session.completed_reducer_input(), None);
                    assert_eq!(
                        session
                            .admit_range(
                                attempt(request_ordinal),
                                partial_input(&request, &dataset),
                                &guard,
                            )
                            .expect("admitted partial response"),
                        AppOnlinePc4RangeDisposition::PartialContentSupplied
                    );
                }
                AppOnlinePc4FixedQueueCandidateStep::Progress {
                    observed_replays,
                    observed_candidates,
                } => {
                    assert!(observed_replays <= 16);
                    assert!(observed_candidates <= 16);
                    assert_eq!(session.completed_reducer_input(), None);
                }
                step @ AppOnlinePc4FixedQueueCandidateStep::Complete { .. } => break step,
                terminal => panic!("unexpected terminal step: {terminal:?}"),
            }
        };

        assert_eq!(looked_up_fields, vec![0, 1]);
        let AppOnlinePc4FixedQueueCandidateStep::Complete {
            replay_provenances,
            canonical_candidates,
        } = complete.clone()
        else {
            unreachable!()
        };
        assert!(replay_provenances > 0);
        assert_eq!(canonical_candidates, 1);
        let reducer = session
            .completed_reducer_input()
            .expect("complete reducer input exists before Complete escapes");
        assert_eq!(reducer.source(), &candidate_source);
        assert_eq!(reducer.candidates().len(), 1);
        assert_eq!(reducer.candidates()[0].initial_board_mask(), INITIAL_BOARD);
        assert_eq!(session.step(&guard), complete);
    }

    #[test]
    fn miss_transport_failures_and_cancel_are_typed_and_never_expose_reducer_input() {
        for (transport, expected) in [
            (
                RangeTransportFailure::RateLimited {
                    retry_after_seconds: Some(3),
                },
                LookupFailure::RateLimited {
                    retry_after_seconds: Some(3),
                },
            ),
            (RangeTransportFailure::Offline, LookupFailure::Offline),
        ] {
            let generation = pin(activated_snapshot("generation-transport", None));
            let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
            let candidate_source = source(&qualified_target);
            let guard = Guard::new(candidate_source.clone());
            let mut session =
                start(generation, &qualified_target, &candidate_source, &guard).expect("session");
            assert!(matches!(
                session.step(&guard),
                AppOnlinePc4FixedQueueCandidateStep::NeedRange(_)
            ));
            assert_eq!(
                session
                    .admit_range(
                        attempt(1),
                        RangeAdmissionInput::TransportFailure(transport),
                        &guard
                    )
                    .expect("admitted transport result"),
                AppOnlinePc4RangeDisposition::TransportFailure(transport)
            );
            assert_eq!(
                session.step(&guard),
                AppOnlinePc4FixedQueueCandidateStep::Failed(
                    AppOnlinePc4FixedQueueCandidateFailure::Lookup {
                        field_id: 0,
                        failure: expected,
                    }
                )
            );
            assert_eq!(session.completed_reducer_input(), None);
        }

        let generation = pin(activated_snapshot("generation-cancel", None));
        let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut cancelled =
            start(generation, &qualified_target, &candidate_source, &guard).expect("session");
        assert!(matches!(
            cancelled.step(&guard),
            AppOnlinePc4FixedQueueCandidateStep::NeedRange(_)
        ));
        cancelled.cancel();
        assert_eq!(
            cancelled.step(&guard),
            AppOnlinePc4FixedQueueCandidateStep::Cancelled
        );
        assert_eq!(cancelled.completed_reducer_input(), None);

        // A direct qualified field-ID lookup cannot currently produce a lookup
        // miss, but the composed terminal remains explicit for that underlying
        // state and still cannot reveal candidate input.
        let generation = pin(activated_snapshot("generation-miss", None));
        let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut missed =
            start(generation, &qualified_target, &candidate_source, &guard).expect("session");
        assert_eq!(
            missed.finish(TerminalState::Miss { field_id: 0 }),
            AppOnlinePc4FixedQueueCandidateStep::Miss { field_id: 0 }
        );
        assert_eq!(missed.completed_reducer_input(), None);
    }

    #[test]
    fn stale_generation_and_source_fail_closed_without_range_or_reducer_input() {
        let old_generation = pin(activated_snapshot("generation-old", None));
        let old_target = target(old_generation.activated_snapshot(), Pc4RuleProfile::Srs);
        let old_source = source(&old_target);
        let current_generation = pin(activated_snapshot("generation-current", None));
        let guard = Guard::new(old_source.clone());
        let error = match start(current_generation, &old_target, &old_source, &guard) {
            Err(error) => error,
            Ok(_) => panic!("stale generation must fail before range I/O"),
        };
        assert_eq!(
            error,
            AppOnlinePc4FixedQueueCandidateStartError::TargetSnapshotMismatch
        );

        for stale_source in [true, false] {
            let generation = pin(activated_snapshot("generation-stale", None));
            let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
            let candidate_source = source(&qualified_target);
            let guard = Guard::new(candidate_source.clone());
            let mut session = start(generation, &qualified_target, &candidate_source, &guard)
                .expect("session starts while current");
            if stale_source {
                guard.source_current.set(false);
            } else {
                guard.snapshot_current.set(false);
            }
            let AppOnlinePc4FixedQueueCandidateStep::Failed(failure) = session.step(&guard) else {
                panic!("stale guard must fail closed")
            };
            assert!(matches!(
                failure.reason(),
                "pc4_graph_candidate_prepare_stale_source"
                    | "pc4_graph_candidate_prepare_stale_snapshot"
            ));
            assert_eq!(session.active_lookup_field_id(), None);
            assert_eq!(session.completed_reducer_input(), None);
        }

        let generation = pin(activated_snapshot("generation-range-stale", None));
        let qualified_target = target(generation.activated_snapshot(), Pc4RuleProfile::Srs);
        let candidate_source = source(&qualified_target);
        let guard = Guard::new(candidate_source.clone());
        let mut session =
            start(generation, &qualified_target, &candidate_source, &guard).expect("session");
        let AppOnlinePc4FixedQueueCandidateStep::NeedRange(request) = session.step(&guard) else {
            panic!("source lookup must request a range")
        };
        guard.snapshot_current.set(false);
        assert_eq!(
            session.admit_range(
                attempt(1),
                partial_input(&request, &range_dataset()),
                &guard,
            ),
            Err(AppOnlinePc4RangeError::Admission(
                clearra_pc4_tablebase::RangeAdmissionError::SnapshotStale
            ))
        );
        assert_eq!(
            session.step(&Guard::new(candidate_source)),
            AppOnlinePc4FixedQueueCandidateStep::NeedRange(request)
        );
        assert_eq!(session.completed_reducer_input(), None);
    }
}
