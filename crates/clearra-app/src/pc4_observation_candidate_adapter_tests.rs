use core::{convert::Infallible, num::NonZeroUsize};
use std::{cell::Cell, collections::BTreeMap};

use clearra_core_domain::board::standard_pc_board::StandardPcBoard;
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
use crate::pc4_input_disclosure_policy::{
    prepare_pc4_input_disclosure, Pc4BagDisclosure, Pc4HiddenQueueDisclosure, Pc4HiddenQueueSource,
    Pc4InputDisclosureDecision, Pc4InputDisclosureRequest, Pc4InputSurface, Pc4PartialBagRemainder,
    Pc4PreparedOnlineInput, Pc4QueueDisclosure,
};
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

fn target_for_profile(
    profile_to_qualify: Pc4RuleProfile,
    use_case: Pc4TerminalUseCase,
) -> QualifiedPc4TargetIdentity {
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
            let manifest = manifest
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
                            format!("{prefix}-terminal:{qualified_use_case:?}:4"),
                            format!("{prefix}-outgoing:{qualified_use_case:?}:4"),
                            format!("{prefix}-kat:{qualified_use_case:?}:4"),
                            format!("{prefix}-offline:{qualified_use_case:?}:4"),
                        )
                        .expect("target qualification")
                    })
                    .collect(),
                )
                .expect("qualified target");
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
        profile_to_qualify,
        use_case,
        Pc4TargetLines::new(4).expect("target"),
    )
    .expect("qualified target identity")
}

fn target(use_case: Pc4TerminalUseCase) -> QualifiedPc4TargetIdentity {
    target_for_profile(Pc4RuleProfile::Srs, use_case)
}

fn prepared_input(target: &QualifiedPc4TargetIdentity) -> Pc4PreparedOnlineInput {
    let hidden = Pc4HiddenQueueDisclosure::new(
        Pc4HiddenQueueSource::Pattern,
        vec![Pc4GraphPiece::I],
        0,
        1,
        1,
        Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag"),
        5,
        Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
    )
    .expect("normalized hidden queue");
    prepared_hidden_input(target, hidden)
}

fn prepared_hidden_input(
    target: &QualifiedPc4TargetIdentity,
    hidden: Pc4HiddenQueueDisclosure,
) -> Pc4PreparedOnlineInput {
    match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
        target.clone(),
        Pc4InputSurface::Gui,
        Pc4QueueDisclosure::PatternOrHidden(hidden),
    ))
    .expect("prepared input")
    {
        Pc4InputDisclosureDecision::Ready(prepared) => prepared,
        _ => panic!("complete bag disclosure is ready"),
    }
}

fn source(
    target: &QualifiedPc4TargetIdentity,
    prepared_input: &Pc4PreparedOnlineInput,
) -> PcCandidateSourceBinding {
    source_with_hold(
        target,
        prepared_input,
        FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
    )
}

fn source_with_hold(
    target: &QualifiedPc4TargetIdentity,
    prepared_input: &Pc4PreparedOnlineInput,
    initial_hold: FixedQueueHoldState,
) -> PcCandidateSourceBinding {
    let initial_board = StandardPcBoard::empty(target.target_lines().get()).expect("empty board");
    let request_identity = PcCandidateRequestIdentity::derive_pc4_candidate_universe(
        prepared_input,
        initial_board,
        initial_hold,
    )
    .expect("request identity");
    PcCandidateSourceBinding::online_pc4(
        PcCandidateSessionId::new(core::num::NonZeroU64::new(17).expect("session")),
        request_identity,
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

struct InterruptingProvider<'a> {
    provider: Provider,
    flag: &'a Cell<bool>,
    value: bool,
}

impl QualifiedCompleteAdjacencyProvider for InterruptingProvider<'_> {
    type Error = Infallible;

    fn target(&self) -> &QualifiedPc4TargetIdentity {
        self.provider.target()
    }

    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
        let result = self.provider.complete_outgoing_edges(query);
        self.flag.set(self.value);
        result
    }
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
    profile: Pc4RuleProfile,
    calls: usize,
}

impl Pc4PlacementMaterializer for Materializer {
    type Error = Infallible;

    fn profile(&self) -> Pc4RuleProfile {
        self.profile
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
    let (rotation, y, cells) = match edge.piece() {
        Pc4GraphPiece::I if edge.source_field_id() == 1 => (PlacementRotation::Zero, 1, 0x3c00),
        Pc4GraphPiece::I => (PlacementRotation::Zero, 0, 0x000f),
        Pc4GraphPiece::T => (PlacementRotation::Right, 0, 0x00f0),
        _ => unreachable!("synthetic hold branches place only I or T"),
    };
    MaterializationOutput {
        snapshot: edge.snapshot().clone(),
        profile: edge.profile(),
        source_field_id: edge.source_field_id(),
        piece: edge.piece(),
        target_field_id: edge.target_field_id(),
        placements: vec![
            ClearraPlacementIdentity::new(edge.piece(), rotation, 0, y, cells).expect("placement"),
        ],
    }
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("non-zero")
}

fn frontier() -> clearra_pc4_tablebase::Pc4ObservationFrontierFamily {
    frontier_with(1, FixedQueueHoldState::Occupied(Pc4GraphPiece::T), 1)
}

fn frontier_with(
    hidden_draws: usize,
    initial_hold: FixedQueueHoldState,
    placement_count: usize,
) -> clearra_pc4_tablebase::Pc4ObservationFrontierFamily {
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
            hidden_draws,
            initial_hold,
            placement_count,
            budgets,
        ),
        &|| false,
    )
    .expect("frontier")
}

fn graph_family(target: &QualifiedPc4TargetIdentity, guard: &Guard) -> Pc4ObservationGraphFamily {
    graph_family_from_frontier(target, guard, frontier())
}

fn graph_family_from_frontier(
    target: &QualifiedPc4TargetIdentity,
    guard: &Guard,
    frontier: clearra_pc4_tablebase::Pc4ObservationFrontierFamily,
) -> Pc4ObservationGraphFamily {
    prepare_pc4_observation_graph_family(
        Pc4ObservationGraphRequest::new(
            target.clone(),
            0,
            frontier,
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
    let prepared_input = prepared_input(&target);
    let source = source(&target, &prepared_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &prepared_input,
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
        Materializer {
            profile: Pc4RuleProfile::Srs,
            calls: 0,
        },
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
    assert_eq!(
        family.total_reveal_probability(),
        Pc4ExactProbability::one()
    );
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
    let probability_sum = family
        .reveal_outcomes()
        .iter()
        .try_fold(Pc4ExactProbability::zero(), |sum, outcome| {
            sum.checked_add(outcome.reveal().probability())
        })
        .expect("exact probability sum");
    assert_eq!(probability_sum, Pc4ExactProbability::one());

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
fn zero_solution_reveals_remain_in_the_complete_probability_ledger() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared_input = prepared_input(&target);
    let source = source(&target, &prepared_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let mut session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &prepared_input,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(16),
        ),
        &graph,
        &guard,
    )
    .expect("candidate session");
    let mut provider = Provider {
        target: target.clone(),
        graph: BTreeMap::new(),
        calls: 0,
    };
    let mut terminal = ManifestQualifiedPc4ObservationTerminal::new(target.clone());
    let mut materializer = Materializer {
        profile: Pc4RuleProfile::Srs,
        calls: 0,
    };

    let first = session
        .advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect("first bounded zero-solution advance");
    assert_eq!(first.observed_reveal_outcomes(), 1);
    assert_eq!(
        first.status(),
        Pc4ObservationCandidateAdvanceStatus::InProgress
    );
    assert!(!session.is_exhausted());
    let graph_calls_after_graph_exhaustion = provider.calls;

    while !session.is_exhausted() {
        session
            .advance(
                nonzero(1),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("bounded ledger-only continuation");
    }
    assert_eq!(provider.calls, graph_calls_after_graph_exhaustion);
    let family = session
        .finish(&guard)
        .expect("complete zero-solution family");

    assert_eq!(family.reveal_outcomes().len(), 2);
    assert!(family
        .reveal_outcomes()
        .iter()
        .all(|outcome| outcome.candidates().is_empty()));
    assert!(family.canonical_candidates().is_empty());
    assert_eq!(
        family.total_reveal_probability(),
        Pc4ExactProbability::one()
    );
    assert_eq!(family.retained_element_count(), 2);
    assert_eq!(materializer.calls, 0);
    let reducer_input = family
        .reducer_input()
        .expect("the exact all-reveal union is reducer input");
    assert!(reducer_input.candidates().is_empty());
    assert_eq!(
        reducer_input.universe_identity().qualified_target(),
        Some(&target)
    );
}

#[test]
fn mixed_success_and_zero_solution_reveals_share_one_exact_denominator() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let hidden = Pc4HiddenQueueDisclosure::new(
        Pc4HiddenQueueSource::Pattern,
        vec![Pc4GraphPiece::I],
        0,
        1,
        2,
        Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag"),
        5,
        Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
    )
    .expect("two-placement hidden queue");
    let prepared = prepared_hidden_input(&target, hidden);
    let source = source_with_hold(&target, &prepared, FixedQueueHoldState::Disabled);
    let guard = Guard::new(source.clone());
    let graph = graph_family_from_frontier(
        &target,
        &guard,
        frontier_with(1, FixedQueueHoldState::Disabled, 2),
    );
    let mut session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &prepared,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(16),
        ),
        &graph,
        &guard,
    )
    .expect("mixed-outcome session");
    let mut provider = Provider {
        target: target.clone(),
        graph: BTreeMap::from([
            ((0, Pc4GraphPiece::I), vec![1]),
            ((1, Pc4GraphPiece::I), vec![3]),
        ]),
        calls: 0,
    };
    let mut terminal = ManifestQualifiedPc4ObservationTerminal::new(target);
    let mut materializer = Materializer {
        profile: Pc4RuleProfile::Srs,
        calls: 0,
    };
    while !session.is_exhausted() {
        session
            .advance(
                nonzero(1),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("mixed-outcome advance");
    }

    let family = session.finish(&guard).expect("complete mixed outcomes");
    let outcomes = family.reveal_outcomes();
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].reveal().reveal_rank(), 0);
    assert_eq!(outcomes[0].reveal().revealed_pieces(), [Pc4GraphPiece::I]);
    assert_eq!(outcomes[0].candidates().len(), 1);
    assert_eq!(outcomes[1].reveal().reveal_rank(), 1);
    assert_eq!(outcomes[1].reveal().revealed_pieces(), [Pc4GraphPiece::O]);
    assert!(outcomes[1].candidates().is_empty());
    assert!(outcomes.iter().all(|outcome| {
        outcome.reveal().probability().numerator() == 1
            && outcome.reveal().probability().denominator() == 2
    }));
    assert_eq!(
        family.total_reveal_probability(),
        Pc4ExactProbability::one()
    );
    assert_eq!(family.canonical_candidates().len(), 1);
    assert_eq!(family.replay_provenance_count(), 1);
    assert_eq!(
        family.reducer_input().expect("exact union").candidates(),
        family.canonical_candidates()
    );
}

#[test]
fn canonical_complete_family_is_independent_of_advance_size() {
    let one_at_a_time = drain(1);
    let batched = drain(8);
    assert_eq!(one_at_a_time, batched);
    let reducer = one_at_a_time.reducer_input().expect("complete union");
    assert_eq!(
        reducer,
        batched.reducer_input().expect("same complete union")
    );
    assert_eq!(reducer.candidates(), one_at_a_time.canonical_candidates());
    assert_eq!(reducer.universe_identity().exact_candidate_count(), 2);
}

#[test]
fn late_guard_failures_roll_back_both_graph_and_reveal_ledger_for_retry() {
    for interruption in 0..3 {
        let (mut session, guard, provider, mut terminal, mut materializer) =
            session_with_budget(16);
        let (flag, value, expected) = match interruption {
            0 => (
                &guard.cancelled,
                true,
                Pc4ObservationCandidateError::Cancelled,
            ),
            1 => (
                &guard.source_current,
                false,
                Pc4ObservationCandidateError::StaleSource,
            ),
            _ => (
                &guard.snapshot_current,
                false,
                Pc4ObservationCandidateError::StaleSnapshot,
            ),
        };
        let mut interrupted_provider = InterruptingProvider {
            provider,
            flag,
            value,
        };
        assert_eq!(
            session.advance(
                nonzero(8),
                &mut interrupted_provider,
                &mut terminal,
                &mut materializer,
                &guard,
            ),
            Err(expected)
        );
        assert!(interrupted_provider.provider.calls > 0);
        assert_eq!(session.observed_reveal_outcome_count(), 0);
        assert_eq!(session.observed_concrete_path_count(), 0);
        assert_eq!(session.observed_candidate_membership_count(), 0);
        assert_eq!(session.reveal_ledger_cursor.emitted_outcomes(), 0);
        assert_eq!(session.graph_cursor.frontier_entries_started(), 0);
        assert!(!session.is_exhausted());

        guard.cancelled.set(false);
        guard.source_current.set(true);
        guard.snapshot_current.set(true);
        while !session.is_exhausted() {
            session
                .advance(
                    nonzero(8),
                    &mut interrupted_provider.provider,
                    &mut terminal,
                    &mut materializer,
                    &guard,
                )
                .expect("retry after guard recovery");
        }
        assert_eq!(session.finish(&guard).expect("complete retry"), drain(8));
    }
}

#[test]
fn zero_solution_reveals_consume_outcome_and_retained_element_budgets() {
    let (mut session, guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    session.binding.adapter_budgets.reveal_outcomes = nonzero(1);
    provider.graph.clear();
    session
        .advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect("first zero-solution rank fits the outcome budget");
    assert_eq!(session.observed_reveal_outcome_count(), 1);
    assert!(!session.is_exhausted());
    assert_eq!(
        session.advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::BudgetExceeded(
            Pc4ObservationCandidateBudgetExceeded {
                kind: Pc4ObservationCandidateBudgetKind::RevealOutcomes,
                limit: 1,
                attempted: 2,
            }
        ))
    );
    assert_eq!(session.observed_reveal_outcome_count(), 1);
    assert_eq!(session.reveal_ledger_cursor.emitted_outcomes(), 1);
    assert_eq!(
        session.finish(&guard),
        Err(Pc4ObservationCandidateError::IncompleteCannotFinalize)
    );

    let (mut session, guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    session.binding.adapter_budgets.retained_elements = nonzero(1);
    provider.graph.clear();
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
                kind: Pc4ObservationCandidateBudgetKind::RetainedElements,
                limit: 1,
                attempted: 2,
            }
        ))
    );
    assert_eq!(session.observed_reveal_outcome_count(), 0);
    assert_eq!(session.reveal_ledger_cursor.emitted_outcomes(), 0);
    assert_eq!(session.graph_cursor.frontier_entries_started(), 0);
    assert_eq!(
        session.finish(&guard),
        Err(Pc4ObservationCandidateError::IncompleteCannotFinalize)
    );
}

#[test]
fn exhausted_ledger_cannot_finalize_while_concrete_materialization_remains() {
    let (mut session, guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);

    for _ in 0..2 {
        session
            .advance(
                nonzero(1),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("bounded graph and ledger advance");
    }

    assert_eq!(session.observed_reveal_outcome_count(), 2);
    assert_eq!(session.observed_concrete_path_count(), 2);
    assert!(!session.is_exhausted());

    while !session.is_exhausted() {
        session
            .advance(
                nonzero(1),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("remaining concrete materialization");
    }
    let complete = session.finish(&guard).expect("jointly exhausted family");
    assert_eq!(complete.replay_provenance_count(), 4);
    assert_eq!(
        complete.total_reveal_probability(),
        Pc4ExactProbability::one()
    );
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
fn cancelled_and_stale_sessions_reject_without_committing_ledger_rows() {
    let (mut cancelled, cancelled_guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    cancelled_guard.cancelled.set(true);
    assert_eq!(
        cancelled.advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &cancelled_guard,
        ),
        Err(Pc4ObservationCandidateError::Cancelled)
    );
    assert_eq!(cancelled.observed_reveal_outcome_count(), 0);
    assert!(!cancelled.is_exhausted());

    let (mut stale, stale_guard, mut provider, mut terminal, mut materializer) =
        session_with_budget(16);
    stale_guard.snapshot_current.set(false);
    assert_eq!(
        stale.advance(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &stale_guard,
        ),
        Err(Pc4ObservationCandidateError::StaleSnapshot)
    );
    assert_eq!(stale.observed_reveal_outcome_count(), 0);
    assert!(!stale.is_exhausted());
}

#[test]
fn queue_scope_mismatch_is_rejected_before_any_session_is_prepared() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared_input = prepared_input(&target);
    let source = source(&target, &prepared_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let mismatched_hidden = Pc4HiddenQueueDisclosure::new(
        Pc4HiddenQueueSource::Pattern,
        vec![Pc4GraphPiece::T],
        0,
        1,
        1,
        Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag"),
        5,
        Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
    )
    .expect("mismatched hidden queue");
    let mismatched_input = prepared_hidden_input(&target, mismatched_hidden);

    assert!(matches!(
        prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(
                &target,
                &mismatched_input,
                &source,
                0,
                materialization_budgets(),
                adapter_budgets(16),
            ),
            &graph,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::QueueScopeMismatch
        ))
    ));
}

#[test]
fn bag_and_placement_scope_mismatches_are_rejected_before_session_preparation() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared_input = prepared_input(&target);
    let source = source(&target, &prepared_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let bag_profile = Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag");
    let mismatches = [
        Pc4HiddenQueueDisclosure::new(
            Pc4HiddenQueueSource::Pattern,
            vec![Pc4GraphPiece::I],
            0,
            1,
            1,
            bag_profile,
            6,
            Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
        )
        .expect("mismatched bag epoch"),
        Pc4HiddenQueueDisclosure::new(
            Pc4HiddenQueueSource::Pattern,
            vec![Pc4GraphPiece::I],
            0,
            1,
            2,
            bag_profile,
            5,
            Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
        )
        .expect("mismatched placement horizon"),
    ];

    for hidden in mismatches {
        let mismatched_input = prepared_hidden_input(&target, hidden);
        assert!(matches!(
            prepare_pc4_observation_candidate_session(
                Pc4ObservationCandidateAdapterRequest::new(
                    &target,
                    &mismatched_input,
                    &source,
                    0,
                    materialization_budgets(),
                    adapter_budgets(16),
                ),
                &graph,
                &guard,
            ),
            Err(Pc4ObservationCandidateError::Binding(
                Pc4ObservationCandidateBindingError::QueueScopeMismatch
            ))
        ));
    }
}

#[test]
fn hold_and_queue_bound_request_identity_cannot_be_substituted() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared_input = prepared_input(&target);
    let wrong_request_identity = PcCandidateRequestIdentity::derive_pc4_candidate_universe(
        &prepared_input,
        StandardPcBoard::empty(target.target_lines().get()).expect("empty board"),
        FixedQueueHoldState::Empty,
    )
    .expect("alternate hold request identity");
    let source = PcCandidateSourceBinding::online_pc4(
        PcCandidateSessionId::new(core::num::NonZeroU64::new(18).expect("session")),
        wrong_request_identity,
        PcCandidateSourceIdentity::from_sha256([9; 32]),
        target.profile(),
        0,
        target.snapshot().clone(),
    );
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);

    assert!(matches!(
        prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(
                &target,
                &prepared_input,
                &source,
                0,
                materialization_budgets(),
                adapter_budgets(16),
            ),
            &graph,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::RequestIdentityMismatch
        ))
    ));
}

#[test]
fn source_board_and_profile_mismatches_are_rejected_before_session_preparation() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared_input = prepared_input(&target);
    let matching_source = source(&target, &prepared_input);
    for (profile, initial_board_mask, expected) in [
        (
            target.profile(),
            1,
            Pc4ObservationCandidateBindingError::RequestIdentityMismatch,
        ),
        (
            target.profile(),
            1_u64 << 40,
            Pc4ObservationCandidateBindingError::InitialBoardMismatch,
        ),
        (
            Pc4RuleProfile::Jstris180,
            0,
            Pc4ObservationCandidateBindingError::SourceProfileMismatch,
        ),
    ] {
        let substituted_source = PcCandidateSourceBinding::online_pc4(
            PcCandidateSessionId::new(core::num::NonZeroU64::new(19).expect("session")),
            matching_source.request_identity(),
            matching_source.source_identity(),
            profile,
            initial_board_mask,
            target.snapshot().clone(),
        );
        let guard = Guard::new(substituted_source.clone());
        let graph = graph_family(&target, &guard);
        assert!(matches!(
            prepare_pc4_observation_candidate_session(
                Pc4ObservationCandidateAdapterRequest::new(
                    &target,
                    &prepared_input,
                    &substituted_source,
                    0,
                    materialization_budgets(),
                    adapter_budgets(16),
                ),
                &graph,
                &guard,
            ),
            Err(Pc4ObservationCandidateError::Binding(actual)) if actual == expected
        ));
    }
}

#[test]
fn bag_free_prepared_input_cannot_authorize_arbitrary_ledger_bag_provenance() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let hidden = Pc4HiddenQueueDisclosure::new(
        Pc4HiddenQueueSource::Pattern,
        vec![Pc4GraphPiece::I],
        0,
        0,
        1,
        Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag"),
        5,
        Pc4BagDisclosure::Refused,
    )
    .expect("bag-free hidden scope");
    let prepared = prepared_hidden_input(&target, hidden);
    assert!(matches!(
        prepared.queue(),
        Pc4PreparedQueueInput::PatternOrHidden {
            bag_state: None,
            ..
        }
    ));
    let source = source(&target, &prepared);
    let guard = Guard::new(source.clone());
    let graph = graph_family_from_frontier(
        &target,
        &guard,
        frontier_with(0, FixedQueueHoldState::Occupied(Pc4GraphPiece::T), 1),
    );
    assert!(matches!(
        prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(
                &target,
                &prepared,
                &source,
                0,
                materialization_budgets(),
                adapter_budgets(16),
            ),
            &graph,
            &guard,
        ),
        Err(Pc4ObservationCandidateError::Binding(
            Pc4ObservationCandidateBindingError::QueueScopeMismatch
        ))
    ));
}

#[test]
fn prepared_target_and_graph_source_field_cannot_be_substituted() {
    let pc_target = target(Pc4TerminalUseCase::PcSearch);
    let pc_input = prepared_input(&pc_target);
    let source = source(&pc_target, &pc_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&pc_target, &guard);
    let setup_input = prepared_input(&target(Pc4TerminalUseCase::SetupSearch));

    for (input, source_field_id, expected) in [
        (
            &setup_input,
            0,
            Pc4ObservationCandidateBindingError::PreparedInputTargetMismatch,
        ),
        (
            &pc_input,
            1,
            Pc4ObservationCandidateBindingError::GraphSourceFieldMismatch,
        ),
    ] {
        assert!(matches!(
            prepare_pc4_observation_candidate_session(
                Pc4ObservationCandidateAdapterRequest::new(
                    &pc_target,
                    input,
                    &source,
                    source_field_id,
                    materialization_budgets(),
                    adapter_budgets(16),
                ),
                &graph,
                &guard,
            ),
            Err(Pc4ObservationCandidateError::Binding(actual)) if actual == expected
        ));
    }
}

#[test]
fn all_five_independently_qualified_profiles_retain_exact_reducer_identity() {
    for profile in Pc4RuleProfile::ALL {
        let target = target_for_profile(profile, Pc4TerminalUseCase::PcSearch);
        let prepared_input = prepared_input(&target);
        let source = source(&target, &prepared_input);
        let guard = Guard::new(source.clone());
        let graph = graph_family(&target, &guard);
        let mut session = prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(
                &target,
                &prepared_input,
                &source,
                0,
                materialization_budgets(),
                adapter_budgets(16),
            ),
            &graph,
            &guard,
        )
        .expect("profile-qualified candidate session");
        let mut provider = Provider {
            target: target.clone(),
            graph: BTreeMap::new(),
            calls: 0,
        };
        let mut terminal = ManifestQualifiedPc4ObservationTerminal::new(target.clone());
        let mut materializer = Materializer { profile, calls: 0 };

        while !session.is_exhausted() {
            session
                .advance(
                    nonzero(1),
                    &mut provider,
                    &mut terminal,
                    &mut materializer,
                    &guard,
                )
                .expect("independently qualified profile advance");
        }
        let family = session.finish(&guard).expect("complete profile family");
        let reducer = family.reducer_input().expect("complete reducer input");

        assert_eq!(family.target(), &target);
        assert_eq!(family.source(), &source);
        assert_eq!(
            family.total_reveal_probability(),
            Pc4ExactProbability::one()
        );
        assert_eq!(reducer.universe_identity().profile(), profile);
        assert_eq!(
            reducer.universe_identity().request_identity(),
            source.request_identity()
        );
        assert_eq!(
            reducer.universe_identity().qualified_target(),
            Some(&target)
        );
    }
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
    let prepared_input = prepared_input(&target);
    let source = source(&target, &prepared_input);
    let guard = Guard::new(source.clone());
    let graph = graph_family(&target, &guard);
    let mut session = prepare_pc4_observation_candidate_session(
        Pc4ObservationCandidateAdapterRequest::new(
            &target,
            &prepared_input,
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
