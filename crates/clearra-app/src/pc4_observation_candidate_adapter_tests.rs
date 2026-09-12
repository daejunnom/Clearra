use core::{convert::Infallible, num::NonZeroUsize};
use std::{cell::Cell, collections::BTreeMap};

use clearra_pc4_tablebase::{
    prepare_pc4_observation_frontier, prepare_pc4_observation_graph_family, ArtifactDescriptor,
    DatasetSnapshotManifest, DatasetSnapshotVerifier, FieldIdIndexRelation,
    FixedQueueAdjacencyQuery, FixedQueueHoldBudgets, FixedQueueHoldDecision, FixedQueueHoldState,
    FixedQueueTraversalBudgets, FixedQueueTraversalPageBudgets, GraphTargetEncoding,
    ManifestContentIdentity, MaterializationOutput, Pc4ArtifactRole, Pc4BagProfile,
    Pc4BagRevealBudgets, Pc4BagState, Pc4ObservationFrontierBudgets, Pc4ObservationFrontierRequest,
    Pc4ObservationGraphBudgets, Pc4ObservationGraphRequest, Pc4ProfileManifest, Pc4RuleProfile,
    Pc4TargetLines, Pc4TerminalFieldIdentity, Pc4TerminalUseCase, PlacementMaterializationError,
    PlacementRotation, ProfileAvailability, ProfileQualification,
    ProfileTargetCompletenessQualification, QualifiedCompleteAdjacency, QualifiedPc4GraphEdge,
    QualifiedSnapshotIdentity, SnapshotIdentity, SnapshotVerificationAttestation,
    SnapshotVerificationFailure, SnapshotVerificationRequest, TerminalDepthContract,
};

use super::*;
use crate::pc_candidate_page_boundary::{
    PcCandidateRequestIdentity, PcCandidateSessionId, PcCandidateSourceIdentity,
};

struct Verifier;

impl DatasetSnapshotVerifier for Verifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
        SnapshotVerificationAttestation::new(
            request.snapshot_identity().clone(),
            request.manifest_content_identity().clone(),
            "synthetic-observation-candidate-verification",
        )
        .map_err(|_| SnapshotVerificationFailure::Rejected)
    }
}

fn target(use_case: Pc4TerminalUseCase) -> QualifiedPc4TargetIdentity {
    let profiles = Pc4RuleProfile::ALL
        .into_iter()
        .map(|profile| {
            let prefix = profile.as_str();
            let descriptor = |role, suffix: &str, byte_len| {
                ArtifactDescriptor::new(
                    role,
                    format!("{prefix}/{suffix}"),
                    byte_len,
                    format!("{prefix}-{suffix}-identity"),
                )
                .expect("artifact")
            };
            let manifest = Pc4ProfileManifest::new(
                profile,
                4,
                GraphTargetEncoding::U24LittleEndian,
                FieldIdIndexRelation::RecordOrdinal,
                64,
                descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 48),
                descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 36),
                descriptor(Pc4ArtifactRole::Graph, "graph.bin", 64),
                ProfileQualification::new(
                    format!("{prefix}-index-spec"),
                    format!("{prefix}-graph-spec"),
                    format!("{prefix}-provenance"),
                    format!("{prefix}-kat"),
                )
                .expect("profile qualification"),
            )
            .expect("profile manifest");
            let manifest = if profile == Pc4RuleProfile::Srs {
                manifest
                    .with_target_qualifications(
                        [
                            Pc4TerminalUseCase::PcSearch,
                            Pc4TerminalUseCase::SetupSearch,
                        ]
                        .into_iter()
                        .map(|qualified_use_case| {
                            ProfileTargetCompletenessQualification::new(
                                qualified_use_case,
                                Pc4TargetLines::new(4).expect("target"),
                                Pc4TerminalFieldIdentity::full_rows(
                                    Pc4TargetLines::new(4).expect("target"),
                                    3,
                                ),
                                format!("terminal:{qualified_use_case:?}:4"),
                                format!("outgoing:{qualified_use_case:?}:4"),
                                format!("kat:{qualified_use_case:?}:4"),
                                format!("offline:{qualified_use_case:?}:4"),
                            )
                            .expect("target qualification")
                        })
                        .collect(),
                    )
                    .expect("qualified target")
            } else {
                manifest
            };
            ProfileAvailability::qualified(manifest)
        })
        .collect();
    DatasetSnapshotManifest::new(
        SnapshotIdentity::new(
            "synthetic/repository",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "observation-candidate-generation-a",
        )
        .expect("snapshot identity"),
        ManifestContentIdentity::new("observation-candidate-manifest-a")
            .expect("manifest identity"),
        profiles,
    )
    .expect("manifest")
    .activate(&mut Verifier)
    .expect("activated snapshot")
    .qualified_target(
        Pc4RuleProfile::Srs,
        use_case,
        Pc4TargetLines::new(4).expect("target"),
    )
    .expect("qualified target identity")
}

fn source(target: &QualifiedPc4TargetIdentity) -> PcCandidateSourceBinding {
    PcCandidateSourceBinding::online_pc4(
        PcCandidateSessionId::new(core::num::NonZeroU64::new(17).expect("session")),
        PcCandidateRequestIdentity::from_sha256([7; 32]),
        PcCandidateSourceIdentity::from_sha256([9; 32]),
        target.profile(),
        0,
        target.snapshot().clone(),
    )
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
                .is_some_and(|current| current == expected)
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
                .is_some_and(|current| current == expected)
    }
}

impl PcCandidatePageGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    fn is_current_source(&self, candidate_source: &PcCandidateSourceBinding) -> bool {
        self.source_current.get() && &self.source == candidate_source
    }

    fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
        self.snapshot_current.get()
            && self
                .source
                .qualified_snapshot()
                .is_some_and(|current| current == expected)
    }
}

struct Provider {
    target: QualifiedPc4TargetIdentity,
    graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>,
    calls: usize,
}

impl QualifiedCompleteAdjacencyProvider for Provider {
    type Error = Infallible;

    fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
        self.calls += 1;
        let edges = self
            .graph
            .get(&(query.source_field_id(), query.piece()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|target_field_id| {
                QualifiedPc4GraphEdge::from_qualified_record(
                    query.target(),
                    query.source_field_id(),
                    query.piece(),
                    target_field_id,
                )
            })
            .collect();
        Ok(QualifiedCompleteAdjacency::from_qualified_provider(
            query.target(),
            query.source_field_id(),
            query.piece(),
            query.queue_index(),
            edges,
        ))
    }
}

fn provider(target: &QualifiedPc4TargetIdentity) -> Provider {
    Provider {
        target: target.clone(),
        graph: BTreeMap::from([
            ((0, Pc4GraphPiece::I), vec![3]),
            ((0, Pc4GraphPiece::T), vec![3]),
        ]),
        calls: 0,
    }
}

struct Materializer {
    calls: usize,
}

impl Pc4PlacementMaterializer for Materializer {
    type Error = Infallible;

    fn profile(&self) -> Pc4RuleProfile {
        Pc4RuleProfile::Srs
    }

    fn enumerate(
        &mut self,
        edge: &QualifiedPc4GraphEdge,
    ) -> Result<MaterializationOutput, Self::Error> {
        self.calls += 1;
        Ok(materialization_output(edge))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntheticMaterializerError {
    Injected,
}

struct FailingMaterializer {
    calls: usize,
    fail: bool,
}

impl Pc4PlacementMaterializer for FailingMaterializer {
    type Error = SyntheticMaterializerError;

    fn profile(&self) -> Pc4RuleProfile {
        Pc4RuleProfile::Srs
    }

    fn enumerate(
        &mut self,
        edge: &QualifiedPc4GraphEdge,
    ) -> Result<MaterializationOutput, Self::Error> {
        self.calls += 1;
        if self.fail {
            Err(SyntheticMaterializerError::Injected)
        } else {
            Ok(materialization_output(edge))
        }
    }
}

fn materialization_output(edge: &QualifiedPc4GraphEdge) -> MaterializationOutput {
    let (rotation, cells) = match edge.piece() {
        Pc4GraphPiece::I => (PlacementRotation::Zero, 0x000f),
        Pc4GraphPiece::T => (PlacementRotation::Right, 0x00f0),
        _ => unreachable!("synthetic hold branches place only I or T"),
    };
    MaterializationOutput {
        snapshot: edge.snapshot().clone(),
        profile: edge.profile(),
        source_field_id: edge.source_field_id(),
        piece: edge.piece(),
        target_field_id: edge.target_field_id(),
        placements: vec![
            ClearraPlacementIdentity::new(edge.piece(), rotation, 0, 0, cells).expect("placement"),
        ],
    }
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("non-zero")
}

fn frontier() -> clearra_pc4_tablebase::Pc4ObservationFrontierFamily {
    let profile = Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag");
    let state = Pc4BagState::new(profile, [1, 1, 0, 0, 0, 0, 0], 5).expect("synthetic state");
    let budgets = Pc4ObservationFrontierBudgets::new(
        Pc4BagRevealBudgets::new(
            nonzero(8),
            nonzero(128),
            nonzero(128),
            nonzero(8),
            nonzero(128),
            nonzero(128),
        ),
        FixedQueueHoldBudgets::new(nonzero(128), nonzero(128), nonzero(128), nonzero(128)),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(128),
    );
    prepare_pc4_observation_frontier(
        Pc4ObservationFrontierRequest::new(
            &[Pc4GraphPiece::I],
            0,
            state,
            1,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
            budgets,
        ),
        &|| false,
    )
    .expect("frontier")
}

fn graph_family(target: &QualifiedPc4TargetIdentity, guard: &Guard) -> Pc4ObservationGraphFamily {
    prepare_pc4_observation_graph_family(
        Pc4ObservationGraphRequest::new(
            target.clone(),
            0,
            frontier(),
            TerminalDepthContract::QueueExhaustedOnly,
            FixedQueueTraversalBudgets::new(nonzero(128), nonzero(128), nonzero(8), nonzero(128)),
            FixedQueueTraversalPageBudgets::new(nonzero(16), nonzero(8)),
            Pc4ObservationGraphBudgets::new(
                nonzero(32),
                nonzero(256),
                nonzero(256),
                nonzero(64),
                nonzero(32),
                nonzero(32),
                nonzero(8),
            ),
        ),
        guard,
    )
    .expect("observation graph family")
}

fn materialization_budgets() -> ConcretePathMaterializationBudgets {
    ConcretePathMaterializationBudgets::new(nonzero(4), nonzero(4), nonzero(8), nonzero(8))
}

fn adapter_budgets(replay_provenances: usize) -> Pc4ObservationCandidateBudgets {
    Pc4ObservationCandidateBudgets::new(
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(16),
        nonzero(replay_provenances),
        nonzero(256),
    )
}

fn session_with_budget(
    replay_provenances: usize,
) -> (
    Pc4ObservationCandidateSession,
    Guard,
    Provider,
    ManifestQualifiedPc4ObservationTerminal,
    Materializer,
) {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(replay_provenances),
        ),
        &graph,
        &guard,
    )
    .expect("candidate session");
    (
        session,
        guard,
        provider(&target),
        ManifestQualifiedPc4ObservationTerminal::new(target),
        Materializer { calls: 0 },
    )
}

fn drain(limit: usize) -> Pc4CompleteObservationCandidateFamily {
    let (mut session, guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    while !session.is_exhausted() {
        session
            .advance(
                nonzero(limit),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("bounded advance");
    }
    session.finish(&guard).expect("complete family")
}

#[test]
fn exhausted_family_groups_probability_once_per_reveal_and_preserves_hold_provenance() {
    let family = drain(8);

    assert_eq!(
        family.contract_id(),
        PC4_OBSERVATION_CANDIDATE_FAMILY_CONTRACT
    );
    assert_eq!(family.successful_reveals().len(), 2);
    assert_eq!(family.canonical_candidates().len(), 2);
    assert_eq!(family.replay_provenance_count(), 4);
    assert_eq!(
        family
            .successful_reveals()
            .iter()
            .map(|outcome| outcome.reveal().reveal_rank())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(family.successful_reveals().iter().all(|outcome| {
        outcome.reveal().probability().numerator() == 1
            && outcome.reveal().probability().denominator() == 2
            && outcome.candidates().len() == 2
    }));
    assert_eq!(
        family
            .successful_reveals()
            .iter()
            .map(|outcome| outcome.reveal().probability().numerator())
            .sum::<u128>(),
        2
    );

    for outcome in family.successful_reveals() {
        let mut hold_paths = outcome
            .candidates()
            .iter()
            .flat_map(Pc4ObservationCanonicalCandidate::provenances)
            .map(|provenance| {
                (
                    provenance.hold_path_index(),
                    provenance.hold_steps()[0].decision(),
                )
            })
            .collect::<Vec<_>>();
        hold_paths.sort_unstable();
        assert_eq!(
            hold_paths,
            vec![
                (0, FixedQueueHoldDecision::UseCurrent),
                (1, FixedQueueHoldDecision::SwapHeld),
            ]
        );
    }
}

#[test]
fn canonical_complete_family_is_independent_of_advance_size() {
    assert_eq!(drain(1), drain(8));
}

#[test]
fn partial_session_cannot_be_finalized_or_observed_as_complete() {
    let (mut session, guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    let advance = session
        .advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect("partial advance");

    assert_eq!(advance.discovered_concrete_paths(), 1);
    assert_eq!(
        advance.status(),
        Pc4ObservationCandidateAdvanceStatus::InProgress
    );
    assert!(!session.is_exhausted());
    assert_eq!(
        session.finish(&guard),
        Err(Pc4ObservationCandidateError::IncompleteCannotFinalize)
    );
}

#[test]
fn provider_target_mismatch_fails_before_provider_or_materializer_callbacks() {
    let (mut session, guard, _provider, mut terminal, mut materializer) = session_with_budget(16);
    let wrong_target = target(Pc4TerminalUseCase::SetupSearch);
    let mut wrong_provider = provider(&wrong_target);

    assert!(matches!(
        session.advance(
            nonzero(1),
            &mut wrong_provider,
            &mut terminal,
            &mut materializer,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::ProviderTargetMismatch
        ))
    ));
    assert_eq!(wrong_provider.calls, 0);
    assert_eq!(materializer.calls, 0);
    assert_eq!(session.observed_concrete_path_count(), 0);
}

#[test]
fn materializer_failure_leaves_session_state_uncommitted_for_retry() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let mut session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(16),
        ),
        &graph,
        &guard,
    )
    .expect("candidate session");
    let mut provider = provider(&target);
    let mut terminal = ManifestQualifiedPc4ObservationTerminal::new(target);
    let mut materializer = FailingMaterializer {
        calls: 0,
        fail: true,
    };

    assert!(matches!(
        session.advance(
            nonzero(8),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::Materialization(
            ConcretePathMaterializationError::Edge {
                source: PlacementMaterializationError::Materializer(
                    SyntheticMaterializerError::Injected
                ),
                ..
            }
        ))
    ));
    assert_eq!(session.observed_concrete_path_count(), 0);
    assert_eq!(session.observed_successful_reveal_count(), 0);

    materializer.fail = false;
    while !session.is_exhausted() {
        session
            .advance(
                nonzero(8),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("retry advance");
    }
    assert_eq!(
        session
            .finish(&guard)
            .expect("complete family")
            .replay_provenance_count(),
        4
    );
}

#[test]
fn replay_budget_failure_does_not_publish_a_partial_accumulator() {
    let (mut session, guard, mut provider, mut terminal, mut materializer) = session_with_budget(3);

    assert_eq!(
        session.advance(
            nonzero(8),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::BudgetExceeded(
            Pc4ObservationCandidateBudgetExceeded {
                kind: Pc4ObservationCandidateBudgetKind::ReplayProvenances,
                limit: 3,
                attempted: 4,
            }
        ))
    );
    assert_eq!(session.observed_concrete_path_count(), 0);
    assert_eq!(session.observed_candidate_membership_count(), 0);
    assert!(!session.is_exhausted());
}
