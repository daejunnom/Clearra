use core::{convert::Infallible, num::NonZeroUsize};
use std::{cell::Cell, collections::BTreeMap};

use clearra_pc4_tablebase::{
    prepare_fixed_queue_traversal_family, ArtifactDescriptor, DatasetSnapshotManifest,
    DatasetSnapshotVerifier, FieldIdIndexRelation, FixedQueueAdjacencyQuery,
    FixedQueueTraversalBudgets, FixedQueueTraversalFamilyRequest, FixedQueueTraversalPageBudgets,
    GraphTargetEncoding, ManifestContentIdentity, MaterializationOutput, Pc4ArtifactRole,
    Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalFieldIdentity,
    Pc4TerminalUseCase, PlacementRotation, ProfileAvailability, ProfileQualification,
    ProfileTargetCompletenessQualification, QualifiedCompleteAdjacency, QualifiedPc4GraphEdge,
    QualifiedSnapshotIdentity, SnapshotIdentity, SnapshotVerificationAttestation,
    SnapshotVerificationFailure, SnapshotVerificationRequest, TerminalDepthContract,
};

use super::*;
use crate::pc_candidate_page_boundary::{
    PcCandidateCollectionCompleteness, PcCandidatePageCollector, PcCandidateRequestIdentity,
    PcCandidateSessionId, PcCandidateSourceIdentity,
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
            "synthetic-graph-candidate-verification",
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
            "candidate-generation-a",
        )
        .expect("snapshot identity"),
        ManifestContentIdentity::new("candidate-manifest-a").expect("manifest identity"),
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
        PcCandidateSessionId::new(core::num::NonZeroU64::new(7).expect("session")),
        PcCandidateRequestIdentity::from_sha256([1; 32]),
        PcCandidateSourceIdentity::from_sha256([2; 32]),
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

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        self.source_current.get() && &self.source == source
    }

    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.snapshot_current.get()
            && self
                .source
                .qualified_snapshot()
                .is_some_and(|current| current == snapshot)
    }
}

struct Terminal {
    target: QualifiedPc4TargetIdentity,
    calls: usize,
}

impl FixedQueueTerminalPredicate for Terminal {
    type Error = Infallible;

    fn is_terminal(
        &mut self,
        query: &clearra_pc4_tablebase::FixedQueueTerminalQuery<'_>,
    ) -> Result<bool, Self::Error> {
        self.calls += 1;
        Ok(query.queue_is_exhausted())
    }
}

impl QualifiedPc4CandidateTerminalPredicate for Terminal {
    fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    fn terminal_semantics_identity(&self) -> &str {
        self.target.qualification().terminal_semantics_identity()
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
        graph: [
            ((0, Pc4GraphPiece::I), vec![2, 1, 1]),
            ((1, Pc4GraphPiece::O), vec![3]),
            ((2, Pc4GraphPiece::O), vec![3]),
        ]
        .into_iter()
        .collect(),
        calls: 0,
    }
}

fn terminal(target: &QualifiedPc4TargetIdentity) -> Terminal {
    Terminal {
        target: target.clone(),
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

fn materialization_output(edge: &QualifiedPc4GraphEdge) -> MaterializationOutput {
    let placements = match edge.piece() {
        Pc4GraphPiece::I if edge.target_field_id() == 1 => vec![
            placement(Pc4GraphPiece::I, PlacementRotation::Zero, 0, 0x000f),
            placement(Pc4GraphPiece::I, PlacementRotation::Right, 1, 0x000f),
            placement(Pc4GraphPiece::I, PlacementRotation::Two, 2, 0x0f00),
        ],
        Pc4GraphPiece::I => vec![placement(
            Pc4GraphPiece::I,
            PlacementRotation::Left,
            0,
            0x000f,
        )],
        Pc4GraphPiece::O => vec![placement(
            Pc4GraphPiece::O,
            PlacementRotation::Zero,
            0,
            0x00f0,
        )],
        _ => unreachable!("synthetic queue contains only I and O"),
    };
    MaterializationOutput {
        snapshot: edge.snapshot().clone(),
        profile: edge.profile(),
        source_field_id: edge.source_field_id(),
        piece: edge.piece(),
        target_field_id: edge.target_field_id(),
        placements,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntheticMaterializerError {
    Injected,
}

struct FailingMaterializer {
    calls: usize,
    fail_at_call: Option<usize>,
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
        if self.fail_at_call == Some(self.calls) {
            Err(SyntheticMaterializerError::Injected)
        } else {
            Ok(materialization_output(edge))
        }
    }
}

fn placement(
    piece: Pc4GraphPiece,
    rotation: PlacementRotation,
    x: u16,
    cells: u64,
) -> ClearraPlacementIdentity {
    ClearraPlacementIdentity::new(piece, rotation, x, 0, cells).expect("placement")
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("non-zero")
}

fn traversal_budgets() -> FixedQueueTraversalBudgets {
    FixedQueueTraversalBudgets::new(nonzero(64), nonzero(16), nonzero(4), nonzero(16))
}

fn materialization_budgets() -> ConcretePathMaterializationBudgets {
    ConcretePathMaterializationBudgets::new(nonzero(4), nonzero(8), nonzero(16), nonzero(8))
}

fn adapter_budgets(page_size: usize) -> Pc4GraphCandidateAdapterBudgets {
    Pc4GraphCandidateAdapterBudgets::new(
        nonzero(2),
        nonzero(2),
        nonzero(8),
        nonzero(16),
        nonzero(32),
        nonzero(page_size),
    )
}

fn lazy_family(target: &QualifiedPc4TargetIdentity, guard: &Guard) -> FixedQueueTraversalFamily {
    prepare_fixed_queue_traversal_family(
        FixedQueueTraversalFamilyRequest::new(
            target,
            0,
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            TerminalDepthContract::QueueExhaustedOnly,
            traversal_budgets(),
            FixedQueueTraversalPageBudgets::new(nonzero(16), nonzero(2)),
        ),
        guard,
    )
    .expect("lazy family")
}

fn prepare_lazy(
    use_case: Pc4TerminalUseCase,
    page_size: usize,
) -> (Pc4GraphCandidateFamily, Guard) {
    let target = target(use_case);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let mut provider = provider(&target);
    let mut terminal = terminal(&target);
    let mut materializer = Materializer { calls: 0 };
    let mut stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(page_size),
        ),
        &family,
        &guard,
    )
    .expect("candidate stream");
    while !stream.is_exhausted() {
        stream
            .next_observation_page(
                nonzero(page_size),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("observation page");
    }
    let candidates = stream.finish(&guard).expect("candidate family");
    (candidates, guard)
}

#[test]
fn preparation_is_callback_free_and_partial_stream_cannot_be_finalized() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let provider = provider(&target);
    let terminal = terminal(&target);
    let materializer = Materializer { calls: 0 };

    let stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(1),
        ),
        &family,
        &guard,
    )
    .expect("candidate stream");

    assert_eq!(stream.target(), &target);
    assert_eq!(stream.source(), &source);
    assert_eq!(provider.calls, 0);
    assert_eq!(terminal.calls, 0);
    assert_eq!(materializer.calls, 0);
    assert_eq!(stream.observed_candidate_count(), 0);
    assert_eq!(stream.observed_replay_count(), 0);
    assert_eq!(
        stream
            .finish(&guard)
            .expect_err("incomplete stream cannot be finalized"),
        Pc4GraphCandidatePrepareError::IncompleteCannotFinalize
    );
}

#[test]
fn stream_rejects_an_unrepresentable_materializer_callback_bound() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let oversized_materialization_budgets = ConcretePathMaterializationBudgets::new(
        nonzero(usize::MAX),
        nonzero(1),
        nonzero(1),
        nonzero(1),
    );

    assert!(matches!(
        prepare_pc4_graph_candidate_stream(
            Pc4GraphCandidateAdapterRequest::new(
                &target,
                &source,
                0,
                oversized_materialization_budgets,
                adapter_budgets(1),
            ),
            &family,
            &guard,
        ),
        Err(Pc4GraphCandidatePrepareError::CounterOverflow)
    ));
}

#[test]
fn observation_pages_are_bounded_and_non_authoritative_until_exhaustion() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let mut provider = provider(&target);
    let mut terminal = terminal(&target);
    let mut materializer = Materializer { calls: 0 };
    let mut stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(1),
        ),
        &family,
        &guard,
    )
    .expect("candidate stream");

    let first = stream
        .next_observation_page(
            nonzero(1),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect("first observation");
    assert_eq!(first.first_replay_ordinal(), 0);
    assert_eq!(first.observations().len(), 1);
    let first_observation = first.observations()[0].clone();
    assert_eq!(first_observation.provenance().start_field_id(), 0);
    assert_eq!(
        first.status(),
        Pc4GraphCandidateObservationStatus::InProgress
    );
    assert!(provider.calls > 0);
    assert!(terminal.calls > 0);
    assert!(materializer.calls > 0);

    while !stream.is_exhausted() {
        stream
            .next_observation_page(
                nonzero(1),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("remaining observation");
    }
    assert_eq!(stream.observed_replay_count(), 4);
    let family = stream.finish(&guard).expect("sealed family");
    assert_eq!(family.candidate_count(), 2);
    assert!(family.candidates().iter().any(|candidate| {
        candidate.identity() == first_observation.identity()
            && candidate
                .replay_provenances()
                .contains(first_observation.provenance())
    }));
}

#[test]
fn later_callback_failure_rolls_back_the_entire_observation_page() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let request = || {
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(4),
        )
    };
    let mut stream =
        prepare_pc4_graph_candidate_stream(request(), &family, &guard).expect("candidate stream");
    let mut failing_provider = provider(&target);
    let mut failing_terminal = terminal(&target);
    let mut materializer = FailingMaterializer {
        calls: 0,
        fail_at_call: None,
    };

    let first_page_callbacks = materializer.calls;
    let committed_prefix = stream
        .next_observation_page(
            nonzero(1),
            &mut failing_provider,
            &mut failing_terminal,
            &mut materializer,
            &guard,
        )
        .expect("first observation page commits");
    assert_eq!(committed_prefix.first_replay_ordinal(), 0);
    assert_eq!(committed_prefix.observations().len(), 1);
    assert_eq!(stream.observed_replay_count(), 1);
    assert_eq!(stream.observed_candidate_count(), 1);
    let callback_limit = stream.maximum_materializer_callbacks_per_observation_page();
    assert!(materializer.calls - first_page_callbacks <= callback_limit);

    // The first concrete family still has observations to emit. Fail on the
    // second callback needed by the following graph path so the failing page
    // has already staged output and materializer work before it aborts.
    let callbacks_before_failure = materializer.calls;
    materializer.fail_at_call = Some(callbacks_before_failure + 2);
    let error = stream
        .next_observation_page(
            nonzero(4),
            &mut failing_provider,
            &mut failing_terminal,
            &mut materializer,
            &guard,
        )
        .expect_err("second graph path materialization must fail");
    assert!(matches!(
        error,
        Pc4GraphCandidatePrepareError::Materialization(ConcretePathMaterializationError::Edge {
            source: clearra_pc4_tablebase::PlacementMaterializationError::Materializer(
                SyntheticMaterializerError::Injected
            ),
            ..
        })
    ));
    assert_eq!(stream.observed_replay_count(), 1);
    assert_eq!(stream.observed_candidate_count(), 1);
    assert!(!stream.is_exhausted());
    assert!(materializer.calls - callbacks_before_failure <= callback_limit);

    let mut reference =
        prepare_pc4_graph_candidate_stream(request(), &family, &guard).expect("reference stream");
    let mut reference_provider = provider(&target);
    let mut reference_terminal = terminal(&target);
    let mut reference_materializer = FailingMaterializer {
        calls: 0,
        fail_at_call: None,
    };
    let reference_prefix = reference
        .next_observation_page(
            nonzero(1),
            &mut reference_provider,
            &mut reference_terminal,
            &mut reference_materializer,
            &guard,
        )
        .expect("reference prefix");
    assert_eq!(reference_prefix, committed_prefix);
    let expected = reference
        .next_observation_page(
            nonzero(4),
            &mut reference_provider,
            &mut reference_terminal,
            &mut reference_materializer,
            &guard,
        )
        .expect("reference page");
    materializer.fail_at_call = None;
    let actual = stream
        .next_observation_page(
            nonzero(4),
            &mut failing_provider,
            &mut failing_terminal,
            &mut materializer,
            &guard,
        )
        .expect("retry page");
    assert_eq!(actual, expected);

    let reference_family = reference.finish(&guard).expect("reference family");
    let retried_family = stream.finish(&guard).expect("retried family");
    assert_eq!(retried_family.target(), reference_family.target());
    assert_eq!(retried_family.source(), reference_family.source());
    assert_eq!(retried_family.candidates(), reference_family.candidates());
    assert_eq!(
        retried_family.replay_provenance_count(),
        reference_family.replay_provenance_count()
    );
    assert_eq!(
        retried_family
            .try_reducer_input(&guard)
            .expect("retried reducer input"),
        reference_family
            .try_reducer_input(&guard)
            .expect("reference reducer input")
    );
}

#[test]
fn duplicate_transition_is_suppressed_while_all_concrete_and_converging_replays_survive() {
    let (family, _) = prepare_lazy(Pc4TerminalUseCase::PcSearch, 2);

    assert_eq!(family.contract_id(), PC4_GRAPH_CANDIDATE_ADAPTER_CONTRACT);
    assert_eq!(family.candidate_count(), 2);
    assert_eq!(family.replay_provenance_count(), 4);
    assert_eq!(
        family
            .candidates()
            .iter()
            .map(|candidate| candidate.replay_provenances().len())
            .sum::<usize>(),
        4
    );
    let first = &family.candidates()[0];
    assert!(first
        .replay_provenances()
        .iter()
        .all(|replay| replay.start_field_id() == 0 && replay.terminal_field_id() == 3));
    assert!(first.replay_provenances().iter().any(|replay| {
        replay.target_field_ids() == [1, 3]
            && replay.placements()[0].rotation() == PlacementRotation::Zero
    }));
    assert!(first
        .replay_provenances()
        .iter()
        .any(|replay| replay.target_field_ids() == [2, 3]));
}

#[test]
fn manifest_terminal_identity_drives_the_complete_candidate_family() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    assert_eq!(target.terminal_field().field_id(), 3);
    assert_eq!(target.terminal_field().field_hash(), 0x00ff_ffff_ffff);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let graph_family = lazy_family(&target, &guard);
    let mut provider = provider(&target);
    let mut terminal = ManifestQualifiedPc4Terminal::new(target.clone());
    let mut materializer = Materializer { calls: 0 };
    let mut stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(2),
        ),
        &graph_family,
        &guard,
    )
    .expect("manifest-bound candidate stream");

    while !stream.is_exhausted() {
        stream
            .next_observation_page(
                nonzero(2),
                &mut provider,
                &mut terminal,
                &mut materializer,
                &guard,
            )
            .expect("manifest-bound observation page");
    }
    let family = stream.finish(&guard).expect("complete candidate family");
    assert_eq!(family.candidate_count(), 2);
    assert_eq!(family.replay_provenance_count(), 4);
    assert!(family
        .candidates()
        .iter()
        .flat_map(|candidate| candidate.replay_provenances())
        .all(|replay| replay.terminal_field_id() == 3));
}

#[test]
fn bounded_pages_seal_completeness_only_on_the_terminal_page() {
    let (family, guard) = prepare_lazy(Pc4TerminalUseCase::PcSearch, 1);
    let mut cursor = family.cursor();
    let first = family
        .next_page(&mut cursor, nonzero(1), &guard)
        .expect("first page");
    assert!(!first.candidate_page().terminal());
    assert_eq!(first.candidates().len(), 1);

    let mut incomplete = PcCandidatePageCollector::new(family.source().clone());
    incomplete
        .accept(first.clone().into_candidate_page(), &guard)
        .expect("accept partial page");
    assert_eq!(
        incomplete.finish(),
        Err(PcCandidateBoundaryError::SourceNotTerminal)
    );

    let second = family
        .next_page(&mut cursor, nonzero(1), &guard)
        .expect("terminal page");
    assert!(second.candidate_page().terminal());
    assert!(cursor.is_exhausted());

    let mut collector = PcCandidatePageCollector::new(family.source().clone());
    let mut replay_cursor = family.cursor();
    loop {
        let page = family
            .next_page(&mut replay_cursor, nonzero(1), &guard)
            .expect("replayed page");
        collector
            .accept(page.into_candidate_page(), &guard)
            .expect("accepted page");
        if replay_cursor.is_exhausted() {
            break;
        }
    }
    let collection = collector.finish().expect("complete collection");
    assert_eq!(
        collection.completeness(),
        PcCandidateCollectionCompleteness::CompleteRequestUniverse
    );
    assert_eq!(
        collection
            .into_reducer_input()
            .expect("complete reducer input")
            .candidates(),
        family
            .try_reducer_input(&guard)
            .expect("direct reducer input")
            .candidates()
    );
}

#[test]
fn eager_and_lazy_traversal_produce_the_same_candidate_and_replay_family() {
    let use_case = Pc4TerminalUseCase::PcSearch;
    let target = target(use_case);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let (lazy, _) = prepare_lazy(use_case, 2);
    let mut provider = provider(&target);
    let mut terminal = terminal(&target);
    let mut materializer = Materializer { calls: 0 };
    let eager = prepare_pc4_graph_candidate_family_from_eager_reference(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(2),
        ),
        FixedQueueTraversalRequest::new(
            &target,
            0,
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            TerminalDepthContract::QueueExhaustedOnly,
            traversal_budgets(),
        ),
        &mut provider,
        &mut terminal,
        &mut materializer,
        &guard,
    )
    .expect("eager family");

    assert_eq!(eager.candidates(), lazy.candidates());
    assert_eq!(eager.replay_provenance_count(), 4);
}

#[test]
fn setup_target_remains_distinct_and_uses_the_same_complete_candidate_contract() {
    let (family, guard) = prepare_lazy(Pc4TerminalUseCase::SetupSearch, 2);

    assert_eq!(family.target().use_case(), Pc4TerminalUseCase::SetupSearch);
    assert_eq!(family.candidate_count(), 2);
    assert_eq!(
        family
            .try_reducer_input(&guard)
            .expect("setup reducer input")
            .source(),
        family.source()
    );
}

#[test]
fn replay_budget_failure_is_typed_and_never_returns_a_partial_family() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let source = source(&target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&target, &guard);
    let mut provider = provider(&target);
    let mut terminal = terminal(&target);
    let mut materializer = Materializer { calls: 0 };
    let budgets = Pc4GraphCandidateAdapterBudgets::new(
        nonzero(2),
        nonzero(2),
        nonzero(8),
        nonzero(2),
        nonzero(32),
        nonzero(2),
    );
    let mut stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &target,
            &source,
            0,
            materialization_budgets(),
            budgets,
        ),
        &family,
        &guard,
    )
    .expect("candidate stream");
    let first = stream
        .next_observation_page(
            nonzero(2),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect("first bounded page");
    assert_eq!(first.observations().len(), 2);
    let error = stream
        .next_observation_page(
            nonzero(2),
            &mut provider,
            &mut terminal,
            &mut materializer,
            &guard,
        )
        .expect_err("replay limit must reject the transaction");

    assert_eq!(
        error,
        Pc4GraphCandidatePrepareError::BudgetExceeded(Pc4GraphCandidateBudgetExceeded {
            kind: Pc4GraphCandidateBudgetKind::ReplayProvenances,
            limit: 2,
            attempted: 3,
        })
    );
    assert_eq!(stream.observed_replay_count(), 2);
}

#[test]
fn source_target_and_terminal_bindings_fail_closed() {
    let qualified_target = target(Pc4TerminalUseCase::PcSearch);
    let wrong_target = target(Pc4TerminalUseCase::SetupSearch);
    let source = source(&qualified_target);
    let guard = Guard::new(source.clone());
    let family = lazy_family(&qualified_target, &guard);
    let mut qualified_provider = provider(&qualified_target);
    let mut wrong_terminal = terminal(&wrong_target);
    let mut materializer = Materializer { calls: 0 };
    let mut stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &qualified_target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(2),
        ),
        &family,
        &guard,
    )
    .expect("candidate stream");
    let error = stream
        .next_observation_page(
            nonzero(2),
            &mut qualified_provider,
            &mut wrong_terminal,
            &mut materializer,
            &guard,
        )
        .expect_err("terminal target mismatch");

    assert_eq!(
        error,
        Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::TerminalTargetMismatch
        )
    );

    assert_eq!(qualified_target.snapshot(), wrong_target.snapshot());
    let mut wrong_provider = provider(&wrong_target);
    let mut right_terminal = terminal(&qualified_target);
    let mut second_stream = prepare_pc4_graph_candidate_stream(
        Pc4GraphCandidateAdapterRequest::new(
            &qualified_target,
            &source,
            0,
            materialization_budgets(),
            adapter_budgets(2),
        ),
        &family,
        &guard,
    )
    .expect("second stream");
    let provider_error = second_stream
        .next_observation_page(
            nonzero(2),
            &mut wrong_provider,
            &mut right_terminal,
            &mut materializer,
            &guard,
        )
        .expect_err("same-snapshot cross-target provider must fail closed");
    assert_eq!(
        provider_error,
        Pc4GraphCandidatePrepareError::Binding(
            Pc4GraphCandidateBindingError::ProviderTargetMismatch
        )
    );
}

struct CancelAfterOneCheck<'a> {
    source: &'a PcCandidateSourceBinding,
    checks: Cell<usize>,
}

impl PcCandidatePageGuard for CancelAfterOneCheck<'_> {
    fn is_cancelled(&self) -> bool {
        let checks = self.checks.get();
        self.checks.set(checks + 1);
        checks >= 1
    }

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        self.source == source
    }

    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.source.qualified_snapshot() == Some(snapshot)
    }
}

#[test]
fn cancelled_candidate_page_does_not_advance_its_cursor() {
    let (family, _) = prepare_lazy(Pc4TerminalUseCase::PcSearch, 1);
    let mut cursor = family.cursor();
    let guard = CancelAfterOneCheck {
        source: family.source(),
        checks: Cell::new(0),
    };

    assert_eq!(
        family.next_page(&mut cursor, nonzero(1), &guard),
        Err(Pc4GraphCandidatePageError::Cancelled)
    );
    assert_eq!(
        Pc4GraphCandidatePageError::Cancelled.reason(),
        "pc4_graph_candidate_page_cancelled"
    );
    assert_eq!(cursor.candidate_index(), 0);
    assert!(!cursor.is_exhausted());
}
