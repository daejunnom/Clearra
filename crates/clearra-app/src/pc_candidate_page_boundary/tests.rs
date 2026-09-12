use std::{cell::Cell, num::NonZeroU64};

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    execution_cancellation::{ExecutionControl, ExecutionPartition},
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_core_executor::WasmCpuSearchBackend;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc4_tablebase::{
    ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, FixedQueueHoldState,
    GraphTargetEncoding, ManifestContentIdentity, Pc4ArtifactRole, Pc4BagProfile, Pc4GraphPiece,
    Pc4ProfileManifest, Pc4TargetLines, Pc4TerminalFieldIdentity, Pc4TerminalUseCase,
    ProfileAvailability, ProfileQualification, ProfileTargetCompletenessQualification,
    QualifiedPc4TargetIdentity, SnapshotIdentity, SnapshotVerificationAttestation,
    SnapshotVerificationFailure, SnapshotVerificationRequest,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::{jstris_180, srs};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;
use crate::pc4_input_disclosure_policy::{
    prepare_pc4_input_disclosure, Pc4BagDisclosure, Pc4HiddenQueueDisclosure, Pc4HiddenQueueSource,
    Pc4InputDisclosureDecision, Pc4InputDisclosureRequest, Pc4InputSurface, Pc4PartialBagRemainder,
    Pc4PreparedOnlineInput, Pc4QueueDisclosure,
};
use crate::{
    pc4_search_problem_compatibility::{
        validate_pc4_search_problem_compatibility, Pc4SearchProblemCompatibilityError,
    },
    pc_candidate_execution_bridge::{
        execute_validated_pc_candidate_input, PcCandidateExecutionError,
    },
};

struct SyntheticVerifier;

impl DatasetSnapshotVerifier for SyntheticVerifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
        Ok(SnapshotVerificationAttestation::new(
            request.snapshot_identity().clone(),
            request.manifest_content_identity().clone(),
            "synthetic-candidate-boundary-verification",
        )
        .expect("synthetic verification attestation"))
    }
}

fn qualified_snapshot(generation: &str) -> QualifiedSnapshotIdentity {
    let identity = SnapshotIdentity::new(
        "synthetic/repository",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        generation,
    )
    .expect("snapshot identity");
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
            ProfileAvailability::qualified(
                Pc4ProfileManifest::new(
                    profile,
                    1,
                    GraphTargetEncoding::U24LittleEndian,
                    clearra_pc4_tablebase::FieldIdIndexRelation::RecordOrdinal,
                    64,
                    descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 24),
                    descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 24),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 64),
                    ProfileQualification::new(
                        format!("{prefix}-index-spec"),
                        format!("{prefix}-graph-spec"),
                        format!("{prefix}-provenance"),
                        format!("{prefix}-kat"),
                    )
                    .expect("qualification"),
                )
                .expect("profile manifest"),
            )
        })
        .collect();
    DatasetSnapshotManifest::new(
        identity,
        ManifestContentIdentity::new(format!("manifest-{generation}"))
            .expect("manifest content identity"),
        profiles,
    )
    .expect("manifest")
    .activate(&mut SyntheticVerifier)
    .expect("qualified snapshot")
    .qualified_identity()
    .clone()
}

fn qualified_target(
    generation: &str,
    profile: Pc4RuleProfile,
    use_case: Pc4TerminalUseCase,
    target_lines: u8,
) -> QualifiedPc4TargetIdentity {
    let profiles = Pc4RuleProfile::ALL
        .into_iter()
        .map(|manifest_profile| {
            let prefix = manifest_profile.as_str();
            let descriptor = |role, suffix: &str, byte_len| {
                ArtifactDescriptor::new(
                    role,
                    format!("{prefix}/{suffix}"),
                    byte_len,
                    format!("{prefix}-{suffix}-identity"),
                )
                .expect("artifact")
            };
            let qualifications = [
                Pc4TerminalUseCase::PcSearch,
                Pc4TerminalUseCase::SetupSearch,
            ]
            .into_iter()
            .flat_map(|qualified_use_case| {
                (Pc4TargetLines::MIN..=Pc4TargetLines::MAX).map(move |lines| {
                    let target = Pc4TargetLines::new(lines).expect("target lines");
                    ProfileTargetCompletenessQualification::new(
                        qualified_use_case,
                        target,
                        Pc4TerminalFieldIdentity::full_rows(target, u32::from(lines - 1)),
                        format!("{prefix}:terminal:{qualified_use_case:?}:{lines}"),
                        format!("{prefix}:outgoing:{qualified_use_case:?}:{lines}"),
                        format!("{prefix}:kat:{qualified_use_case:?}:{lines}"),
                        format!("{prefix}:offline:{qualified_use_case:?}:{lines}"),
                    )
                    .expect("target qualification")
                })
            })
            .collect();
            ProfileAvailability::qualified(
                Pc4ProfileManifest::new(
                    manifest_profile,
                    4,
                    GraphTargetEncoding::U24LittleEndian,
                    clearra_pc4_tablebase::FieldIdIndexRelation::RecordOrdinal,
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
                    .expect("qualification"),
                )
                .expect("profile manifest")
                .with_target_qualifications(qualifications)
                .expect("target qualifications"),
            )
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
            .expect("manifest content identity"),
        profiles,
    )
    .expect("manifest")
    .activate(&mut SyntheticVerifier)
    .expect("activated snapshot")
    .qualified_target(
        profile,
        use_case,
        Pc4TargetLines::new(target_lines).expect("target lines"),
    )
    .expect("qualified target")
}

fn prepared_fixed(
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    queue: Vec<Pc4GraphPiece>,
) -> Pc4PreparedOnlineInput {
    match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
        target,
        surface,
        Pc4QueueDisclosure::FixedExplicit(queue),
    ))
    .expect("fixed input is ready")
    {
        Pc4InputDisclosureDecision::Ready(prepared) => prepared,
        _ => panic!("fixed input must be ready"),
    }
}

#[allow(clippy::too_many_arguments)]
fn prepared_hidden(
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    source: Pc4HiddenQueueSource,
    visible_queue: Vec<Pc4GraphPiece>,
    preview_length: usize,
    hidden_draws: usize,
    placement_count: usize,
    bag_profile: Pc4BagProfile,
    remainder: [u32; 7],
    bag_epoch: u64,
) -> Pc4PreparedOnlineInput {
    let hidden = Pc4HiddenQueueDisclosure::new(
        source,
        visible_queue,
        preview_length,
        hidden_draws,
        placement_count,
        bag_profile,
        bag_epoch,
        Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete(remainder)),
    )
    .expect("normalized hidden input");
    match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
        target,
        surface,
        Pc4QueueDisclosure::PatternOrHidden(hidden),
    ))
    .expect("complete hidden input is ready")
    {
        Pc4InputDisclosureDecision::Ready(prepared) => prepared,
        _ => panic!("complete hidden input must be ready"),
    }
}

fn initial_board(lines: u8, mask: u64) -> StandardPcBoard {
    StandardPcBoard::from_words(lines, [mask, 0, 0, 0]).expect("normalized initial board")
}

fn request_identity(
    prepared: &Pc4PreparedOnlineInput,
    board: StandardPcBoard,
    hold: FixedQueueHoldState,
) -> PcCandidateRequestIdentity {
    PcCandidateRequestIdentity::derive_pc4_candidate_universe(prepared, board, hold)
        .expect("canonical request identity")
}

fn session(value: u64) -> PcCandidateSessionId {
    PcCandidateSessionId::new(NonZeroU64::new(value).expect("non-zero session"))
}

fn offline_source(session_id: u64) -> PcCandidateSourceBinding {
    PcCandidateSourceBinding::offline_exact(
        session(session_id),
        PcCandidateRequestIdentity::from_sha256([1; 32]),
        PcCandidateSourceIdentity::from_sha256([2; 32]),
        Pc4RuleProfile::Srs,
        0,
    )
}

fn online_source(session_id: u64, generation: &str) -> PcCandidateSourceBinding {
    PcCandidateSourceBinding::online_pc4(
        session(session_id),
        PcCandidateRequestIdentity::from_sha256([1; 32]),
        PcCandidateSourceIdentity::from_sha256([2; 32]),
        Pc4RuleProfile::Srs,
        0,
        qualified_snapshot(generation),
    )
}

fn candidate(piece: PieceKind, mask: u64) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_placements(0, [PiecePlacementMask::new(piece, mask)])
        .expect("candidate")
}

fn candidates() -> Vec<StandardBoard64TilingIdentity> {
    let mut values = vec![
        candidate(PieceKind::T, 0b1111),
        candidate(PieceKind::I, 0b1111),
        candidate(PieceKind::O, 0b1111),
    ];
    values.sort_unstable();
    values
}

struct Guard {
    current: PcCandidateSourceBinding,
    cancelled: Cell<bool>,
    snapshot_current: Cell<bool>,
}

impl Guard {
    fn new(current: PcCandidateSourceBinding) -> Self {
        Self {
            current,
            cancelled: Cell::new(false),
            snapshot_current: Cell::new(true),
        }
    }
}

impl PcCandidatePageGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        source == &self.current
    }

    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.snapshot_current.get()
            && self
                .current
                .qualified_snapshot()
                .is_some_and(|current| current == snapshot)
    }
}

fn evidence(
    source: &PcCandidateSourceBinding,
    values: &[StandardBoard64TilingIdentity],
) -> PcCandidateCompletenessEvidence {
    PcCandidateCompletenessEvidence::from_test_verified_complete_source(
        source.clone(),
        None,
        values.len() as u64,
        PcCandidateSetDigest::calculate(values).expect("candidate digest"),
    )
    .expect("offline exact completeness evidence")
}

#[test]
fn complete_universe_evidence_requires_a_provider_consistent_qualified_target() {
    let values = candidates();
    let digest = PcCandidateSetDigest::calculate(&values).expect("candidate digest");
    let online = online_source(1, "generation-a");
    assert_eq!(
        PcCandidateCompletenessEvidence::from_test_verified_complete_source(
            online.clone(),
            None,
            values.len() as u64,
            digest,
        ),
        Err(PcCandidateBoundaryError::CompletenessTargetBindingMismatch)
    );
    assert_eq!(
        PcCandidateCompletenessEvidence::from_test_verified_complete_source(
            online,
            Some(qualified_target(
                "generation-b",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                4,
            )),
            values.len() as u64,
            digest,
        ),
        Err(PcCandidateBoundaryError::CompletenessTargetBindingMismatch)
    );
    assert_eq!(
        PcCandidateCompletenessEvidence::from_test_verified_complete_source(
            offline_source(1),
            Some(qualified_target(
                "generation-a",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                4,
            )),
            values.len() as u64,
            digest,
        ),
        Err(PcCandidateBoundaryError::CompletenessTargetBindingMismatch)
    );
}

#[test]
fn canonical_fixed_queue_request_identity_binds_each_universe_field() {
    let target = qualified_target(
        "request-generation-a",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let prepared = prepared_fixed(
        target,
        Pc4InputSurface::Gui,
        vec![Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T],
    );
    let board = initial_board(4, 0b11);
    let baseline = request_identity(&prepared, board, FixedQueueHoldState::Disabled);

    let target_mutations = [
        (
            "snapshot",
            qualified_target(
                "request-generation-b",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                4,
            ),
        ),
        (
            "profile",
            qualified_target(
                "request-generation-a",
                Pc4RuleProfile::SrsPlus,
                Pc4TerminalUseCase::PcSearch,
                4,
            ),
        ),
        (
            "use case",
            qualified_target(
                "request-generation-a",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::SetupSearch,
                4,
            ),
        ),
        (
            "target lines",
            qualified_target(
                "request-generation-a",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                3,
            ),
        ),
    ];
    for (field, target) in target_mutations {
        let mutated = prepared_fixed(
            target,
            Pc4InputSurface::Gui,
            vec![Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T],
        );
        assert_ne!(
            request_identity(&mutated, board, FixedQueueHoldState::Disabled),
            baseline,
            "{field} must bind the request identity"
        );
    }

    for (field, mutated_board) in [
        ("initial board mask", initial_board(4, 0b1011)),
        ("initial board dimensions", initial_board(3, 0b11)),
    ] {
        assert_ne!(
            request_identity(&prepared, mutated_board, FixedQueueHoldState::Disabled),
            baseline,
            "{field} must bind the request identity"
        );
    }

    for (field, queue) in [
        (
            "fixed queue piece",
            vec![Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::L],
        ),
        (
            "fixed queue order",
            vec![Pc4GraphPiece::O, Pc4GraphPiece::I, Pc4GraphPiece::T],
        ),
        (
            "fixed queue length",
            vec![Pc4GraphPiece::I, Pc4GraphPiece::O],
        ),
    ] {
        let mutated = prepared_fixed(prepared.target().clone(), Pc4InputSurface::Gui, queue);
        assert_ne!(
            request_identity(&mutated, board, FixedQueueHoldState::Disabled),
            baseline,
            "{field} must bind the request identity"
        );
    }

    for hold in [
        FixedQueueHoldState::Empty,
        FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
    ] {
        assert_ne!(
            request_identity(&prepared, board, hold),
            baseline,
            "initial hold must bind the request identity"
        );
    }
}

#[test]
fn canonical_hidden_request_identity_binds_reveal_and_exact_bag_fields() {
    let target = qualified_target(
        "request-generation-a",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let board = initial_board(4, 0b11);
    let baseline_input = prepared_hidden(
        target.clone(),
        Pc4InputSurface::Gui,
        Pc4HiddenQueueSource::Pattern,
        vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
        1,
        2,
        5,
        Pc4BagProfile::standard_seven_bag(),
        [1, 0, 1, 1, 1, 1, 1],
        3,
    );
    let baseline = request_identity(&baseline_input, board, FixedQueueHoldState::Empty);
    let cases = [
        (
            "hidden source",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::HiddenQueue,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                2,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "visible queue",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::I, Pc4GraphPiece::T],
                1,
                2,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "preview and visible scope",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I, Pc4GraphPiece::O],
                2,
                2,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "hidden draws",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                3,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "placement count",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                2,
                4,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "bag profile",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                2,
                5,
                Pc4BagProfile::new([2, 1, 1, 1, 1, 1, 1]).expect("bag profile"),
                [1, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "bag remainder",
            prepared_hidden(
                target.clone(),
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                2,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [0, 0, 1, 1, 1, 1, 1],
                3,
            ),
        ),
        (
            "bag epoch",
            prepared_hidden(
                target,
                Pc4InputSurface::Gui,
                Pc4HiddenQueueSource::Pattern,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                2,
                5,
                Pc4BagProfile::standard_seven_bag(),
                [1, 0, 1, 1, 1, 1, 1],
                4,
            ),
        ),
    ];
    for (field, mutated) in cases {
        assert_ne!(
            request_identity(&mutated, board, FixedQueueHoldState::Empty),
            baseline,
            "{field} must bind the request identity"
        );
    }

    let fixed = prepared_fixed(
        baseline_input.target().clone(),
        Pc4InputSurface::Gui,
        vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
    );
    assert_ne!(
        request_identity(&fixed, board, FixedQueueHoldState::Empty),
        baseline,
        "fixed and hidden queue domains must remain distinct"
    );
}

#[test]
fn input_surface_and_product_objective_do_not_change_universe_identity() {
    let target = qualified_target(
        "request-generation-a",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let queue = vec![Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T];
    let board = initial_board(4, 0b11);
    let baseline = prepared_fixed(target.clone(), Pc4InputSurface::Gui, queue.clone());

    for surface in [
        Pc4InputSurface::NonInteractiveCli,
        Pc4InputSurface::InteractiveCli,
        Pc4InputSurface::Discord,
    ] {
        let other_surface = prepared_fixed(target.clone(), surface, queue.clone());
        assert_eq!(
            request_identity(&other_surface, board, FixedQueueHoldState::Disabled),
            request_identity(&baseline, board, FixedQueueHoldState::Disabled)
        );
    }

    // Product objectives have no representation in this constructor. They are
    // downstream reductions over the already identified candidate universe.
    assert_eq!(
        PC_CANDIDATE_REQUEST_IDENTITY_ALGORITHM,
        "sha256:clearra-pc4-candidate-universe-request-v1"
    );
}

#[test]
fn public_online_source_binding_derives_the_exact_prepared_request_identity() {
    let target = qualified_target(
        "request-generation-a",
        Pc4RuleProfile::SrsPlus,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let prepared = prepared_fixed(
        target.clone(),
        Pc4InputSurface::Gui,
        vec![Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T],
    );
    let board = initial_board(4, 0b11);
    let hold = FixedQueueHoldState::Disabled;
    let source_identity = PcCandidateSourceIdentity::from_sha256([7; 32]);

    let source = PcCandidateSourceBinding::online_pc4_for_prepared_input(
        session(17),
        source_identity,
        &prepared,
        board,
        hold,
    )
    .expect("prepared input owns the public source binding");

    assert_eq!(source.session_id(), session(17));
    assert_eq!(
        source.request_identity(),
        request_identity(&prepared, board, hold)
    );
    assert_eq!(source.source_identity(), source_identity);
    assert_eq!(source.profile(), Pc4RuleProfile::SrsPlus);
    assert_eq!(source.initial_board_mask(), 0b11);
    assert_eq!(source.qualified_snapshot(), Some(target.snapshot()));
}

#[test]
fn public_online_source_binding_rejects_a_board_from_another_target_height() {
    let target = qualified_target(
        "request-generation-a",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let prepared = prepared_fixed(
        target,
        Pc4InputSurface::Gui,
        vec![Pc4GraphPiece::I, Pc4GraphPiece::O],
    );

    assert_eq!(
        PcCandidateSourceBinding::online_pc4_for_prepared_input(
            session(18),
            PcCandidateSourceIdentity::from_sha256([8; 32]),
            &prepared,
            initial_board(3, 0),
            FixedQueueHoldState::Empty,
        ),
        Err(
            PcCandidateSourceBindingError::InitialBoardTargetLinesMismatch {
                board_lines: 3,
                target_lines: 4,
            }
        )
    );
}

#[test]
fn exact_offline_pages_seal_one_canonical_reducer_input() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    let mut collector = PcCandidatePageCollector::new(source.clone());

    let first = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        all[..2].to_vec(),
        false,
    )
    .expect("first page");
    let second_cursor = first.next_cursor();
    collector.accept(first, &guard).expect("accept first");
    let last = PcConcreteCandidatePage::complete(
        source.clone(),
        second_cursor,
        all[2..].to_vec(),
        evidence(&source, &all),
    )
    .expect("complete page");
    collector.accept(last, &guard).expect("accept complete");

    let collection = collector.finish().expect("terminal collection");
    assert_eq!(
        collection.completeness(),
        PcCandidateCollectionCompleteness::CompleteRequestUniverse
    );
    let reducer = collection
        .into_reducer_input()
        .expect("complete reducer input");
    assert_eq!(reducer.source(), &source);
    assert_eq!(reducer.candidates(), all);
    assert_eq!(reducer.universe_identity().qualified_target(), None);
    assert_eq!(
        reducer.universe_identity().request_identity(),
        source.request_identity()
    );
    assert_eq!(
        reducer.universe_identity().source_identity(),
        source.source_identity()
    );
    assert_eq!(reducer.universe_identity().initial_board_mask(), 0);
    assert_eq!(reducer.universe_identity().exact_candidate_count(), 3);
    assert_eq!(
        reducer.universe_identity().candidate_set_digest(),
        PcCandidateSetDigest::calculate(reducer.candidates()).expect("candidate digest")
    );
}

#[test]
fn terminal_partial_tablebase_data_cannot_cross_the_reducer_seam() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    let mut collector = PcCandidatePageCollector::new(source.clone());
    collector
        .accept(
            PcConcreteCandidatePage::partial(
                source,
                PcCandidatePageCursor::initial(),
                candidates(),
                true,
            )
            .expect("terminal partial page"),
            &guard,
        )
        .expect("collect known candidates");

    let collection = collector.finish().expect("partial terminal collection");
    assert_eq!(
        collection.completeness(),
        PcCandidateCollectionCompleteness::PartialKnownCandidates
    );
    assert_eq!(
        collection.into_reducer_input(),
        Err(PcCandidateBoundaryError::IncompleteCannotReduce)
    );
}

#[test]
fn mixed_provider_generation_request_and_source_pages_are_rejected() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    for mixed in [
        offline_source(1),
        online_source(1, "generation-b"),
        PcCandidateSourceBinding::online_pc4(
            session(1),
            PcCandidateRequestIdentity::from_sha256([9; 32]),
            PcCandidateSourceIdentity::from_sha256([2; 32]),
            Pc4RuleProfile::Srs,
            0,
            qualified_snapshot("generation-a"),
        ),
        PcCandidateSourceBinding::online_pc4(
            session(1),
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            PcCandidateSourceIdentity::from_sha256([9; 32]),
            Pc4RuleProfile::Srs,
            0,
            qualified_snapshot("generation-a"),
        ),
    ] {
        let mut collector = PcCandidatePageCollector::new(source.clone());
        let page = PcConcreteCandidatePage::partial(
            mixed,
            PcCandidatePageCursor::initial(),
            candidates(),
            true,
        )
        .expect("synthetic mixed page");
        assert_eq!(
            collector.accept(page, &guard),
            Err(PcCandidateBoundaryError::SourceBindingMismatch)
        );
        assert_eq!(
            collector.expected_cursor(),
            PcCandidatePageCursor::initial()
        );
    }
}

#[test]
fn duplicate_or_out_of_order_candidates_and_cursor_replay_are_rejected() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            vec![all[0], all[0]],
            true,
        ),
        Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical)
    );

    let mut collector = PcCandidatePageCollector::new(source.clone());
    let first = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        vec![all[0]],
        false,
    )
    .expect("first");
    let replay_cursor = first.cursor();
    collector.accept(first, &guard).expect("accept first");
    let duplicate = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        vec![all[0]],
        true,
    )
    .expect("cross-page duplicate");
    assert_eq!(
        collector.accept(duplicate, &guard),
        Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical)
    );
    assert_eq!(collector.expected_cursor().next_ordinal(), 1);
    let replay = PcConcreteCandidatePage::partial(source, replay_cursor, vec![all[1]], true)
        .expect("replayed cursor page");
    assert_eq!(
        collector.accept(replay, &guard),
        Err(PcCandidateBoundaryError::CursorMismatch)
    );
}

struct CancelsOnSecondObservation {
    current: PcCandidateSourceBinding,
    observations: Cell<u8>,
}

impl PcCandidatePageGuard for CancelsOnSecondObservation {
    fn is_cancelled(&self) -> bool {
        let next = self.observations.get().saturating_add(1);
        self.observations.set(next);
        next >= 2
    }

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        source == &self.current
    }

    fn is_current_snapshot(&self, _snapshot: &QualifiedSnapshotIdentity) -> bool {
        true
    }
}

#[test]
fn cancellation_observed_after_validation_does_not_commit_page_state() {
    let source = offline_source(1);
    let guard = CancelsOnSecondObservation {
        current: source.clone(),
        observations: Cell::new(0),
    };
    let page = PcConcreteCandidatePage::partial(
        source.clone(),
        PcCandidatePageCursor::initial(),
        candidates(),
        true,
    )
    .expect("page");
    let mut collector = PcCandidatePageCollector::new(source);
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::Cancelled)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
    assert_eq!(
        collector.finish(),
        Err(PcCandidateBoundaryError::SourceNotTerminal)
    );
}

#[test]
fn false_complete_count_or_digest_is_rejected_without_mutating_cursor() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    let wrong_count = PcCandidateCompletenessEvidence::from_test_verified_complete_source(
        source.clone(),
        None,
        99,
        PcCandidateSetDigest::calculate(&all).expect("candidate digest"),
    )
    .expect("offline exact completeness evidence");
    let page = PcConcreteCandidatePage::complete(
        source.clone(),
        PcCandidatePageCursor::initial(),
        all.clone(),
        wrong_count,
    )
    .expect("count-mismatched page");
    let mut collector = PcCandidatePageCollector::new(source.clone());
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::CompletenessCountMismatch)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );

    let wrong_digest = PcCandidateCompletenessEvidence::from_test_verified_complete_source(
        source.clone(),
        None,
        all.len() as u64,
        PcCandidateSetDigest([7; 32]),
    )
    .expect("offline exact completeness evidence");
    let page = PcConcreteCandidatePage::complete(
        source,
        PcCandidatePageCursor::initial(),
        all,
        wrong_digest,
    )
    .expect("digest-mismatched page");
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::CompletenessDigestMismatch)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
}

#[test]
fn cancellation_stale_session_and_stale_generation_are_transactional() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    let page = || {
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            candidates(),
            true,
        )
        .expect("page")
    };

    let mut collector = PcCandidatePageCollector::new(source.clone());
    guard.cancelled.set(true);
    assert_eq!(
        collector.accept(page(), &guard),
        Err(PcCandidateBoundaryError::Cancelled)
    );
    guard.cancelled.set(false);
    guard.snapshot_current.set(false);
    assert_eq!(
        collector.accept(page(), &guard),
        Err(PcCandidateBoundaryError::StaleSnapshot)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );

    let stale_guard = Guard::new(online_source(2, "generation-a"));
    assert_eq!(
        collector.accept(page(), &stale_guard),
        Err(PcCandidateBoundaryError::StaleSession)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
}

#[test]
fn pages_require_one_initial_board_and_nonterminal_progress() {
    let source = offline_source(1);
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            Vec::new(),
            false,
        ),
        Err(PcCandidateBoundaryError::EmptyNonTerminalPage)
    );
    let wrong_initial = StandardBoard64TilingIdentity::from_placements(
        1 << 20,
        [PiecePlacementMask::new(PieceKind::I, 0b1111)],
    )
    .expect("different initial board");
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source,
            PcCandidatePageCursor::initial(),
            vec![wrong_initial],
            true,
        ),
        Err(PcCandidateBoundaryError::InitialBoardMismatch)
    );
}

fn pc_candidate_execution_bridge_problem(
    rule: clearra_rules::profile::rule_profile::RuleProfile,
) -> SearchProblem {
    pc_candidate_execution_bridge_problem_for_piece(rule, PieceKind::I)
}

fn pc_candidate_execution_bridge_problem_for_piece(
    rule: clearra_rules::profile::rule_profile::RuleProfile,
    queue_piece: PieceKind,
) -> SearchProblem {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![queue_piece])),
        PieceWindow::new(1),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_objective(ObjectivePolicy::all().with_score_summary())
    .with_rule(rule)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_max_memory_mib(Some(64)),
    );
    ProblemCompiler::compile_scenario_pc(&query).expect("one-piece bridge problem")
}

fn pc_candidate_execution_bridge_input(
    target: QualifiedPc4TargetIdentity,
    initial_board_mask: u64,
    candidates: Vec<StandardBoard64TilingIdentity>,
) -> PcCandidateReducerInput {
    let prepared = prepared_fixed(target.clone(), Pc4InputSurface::Gui, vec![Pc4GraphPiece::I]);
    let source = PcCandidateSourceBinding::online_pc4_for_prepared_input(
        session(71),
        PcCandidateSourceIdentity::from_sha256([72; 32]),
        &prepared,
        initial_board(target.target_lines().get(), initial_board_mask),
        FixedQueueHoldState::Disabled,
    )
    .expect("request-bound bridge source");
    PcCandidateReducerInput::from_test_parts(source, Some(target), candidates)
}

#[test]
fn pc_candidate_execution_bridge_preserves_ordinary_exact_product_payloads() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let problem = pc_candidate_execution_bridge_problem(srs());
    let control = ExecutionControl::default();
    let ordinary = WasmCpuSearchBackend::execute_with_control(&problem, &control)
        .expect("ordinary exact execution");
    assert_eq!(ordinary.normalized_solution_identities().len(), 1);
    assert!(!ordinary.path_steps().is_empty());
    assert!(ordinary.exact_scoring_execution_batch().is_some());
    let target = qualified_target(
        "execution-bridge-generation",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        1,
    );
    let input = pc_candidate_execution_bridge_input(
        target,
        problem.initial_board().occupied_mask(),
        ordinary.normalized_solution_identities().to_vec(),
    );
    let compatibility = validate_pc4_search_problem_compatibility(Pc4RuleProfile::Srs, &problem)
        .expect("SRS compatibility");

    let (injected, evidence) =
        execute_validated_pc_candidate_input(&input, compatibility, &problem, &control)
            .expect("validated candidate execution");

    assert_eq!(evidence.universe_identity(), input.universe_identity());
    assert_eq!(evidence.compatibility(), compatibility);
    assert_eq!(evidence.problem_id(), problem.problem_id());
    assert_eq!(
        injected.normalized_solution_identities(),
        ordinary.normalized_solution_identities()
    );
    assert_eq!(
        injected.normalized_solution_keys(),
        ordinary.normalized_solution_keys()
    );
    assert_eq!(injected.path_steps(), ordinary.path_steps());
    assert_eq!(
        injected.coverage_pattern_words(),
        ordinary.coverage_pattern_words()
    );
    assert_eq!(injected.solution_coverages(), ordinary.solution_coverages());
    assert_eq!(
        injected.exact_scoring_execution_batches(),
        ordinary.exact_scoring_execution_batches()
    );
    for field in [
        "unique_solution_count",
        "normalized_solution_set_hash",
        "build_variant_count",
        "count_complete",
        "coverage_complete",
        "score_requested",
    ] {
        assert_eq!(injected.field(field), ordinary.field(field), "{field}");
    }
}

#[test]
fn pc_candidate_execution_bridge_rejects_other_profile_target_and_board() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let srs_problem = pc_candidate_execution_bridge_problem(srs());
    let control = ExecutionControl::default();
    let ordinary = WasmCpuSearchBackend::execute_with_control(&srs_problem, &control)
        .expect("ordinary exact execution");
    let candidates = ordinary.normalized_solution_identities().to_vec();
    let pc_target = qualified_target(
        "execution-bridge-rejections",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        1,
    );
    let valid_input = pc_candidate_execution_bridge_input(
        pc_target.clone(),
        srs_problem.initial_board().occupied_mask(),
        candidates.clone(),
    );

    let jstris_problem = pc_candidate_execution_bridge_problem(jstris_180());
    let jstris_compatibility =
        validate_pc4_search_problem_compatibility(Pc4RuleProfile::Jstris180, &jstris_problem)
            .expect("Jstris compatibility");
    assert_eq!(
        execute_validated_pc_candidate_input(
            &valid_input,
            jstris_compatibility,
            &jstris_problem,
            &control,
        ),
        Err(PcCandidateExecutionError::ProfileMismatch)
    );

    let srs_compatibility =
        validate_pc4_search_problem_compatibility(Pc4RuleProfile::Srs, &srs_problem)
            .expect("SRS compatibility");
    let other_target = qualified_target(
        "execution-bridge-rejections",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        2,
    );
    let other_target_input = pc_candidate_execution_bridge_input(
        other_target,
        srs_problem.initial_board().occupied_mask(),
        candidates.clone(),
    );
    assert_eq!(
        execute_validated_pc_candidate_input(
            &other_target_input,
            srs_compatibility,
            &srs_problem,
            &control,
        ),
        Err(PcCandidateExecutionError::TargetLinesMismatch)
    );

    let setup_target = qualified_target(
        "execution-bridge-rejections",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::SetupSearch,
        1,
    );
    let setup_input = pc_candidate_execution_bridge_input(
        setup_target,
        srs_problem.initial_board().occupied_mask(),
        candidates.clone(),
    );
    assert_eq!(
        execute_validated_pc_candidate_input(
            &setup_input,
            srs_compatibility,
            &srs_problem,
            &control,
        ),
        Err(PcCandidateExecutionError::TargetUseCaseMismatch)
    );

    let other_board_input = pc_candidate_execution_bridge_input(pc_target, 0, candidates);
    assert_eq!(
        execute_validated_pc_candidate_input(
            &other_board_input,
            srs_compatibility,
            &srs_problem,
            &control,
        ),
        Err(PcCandidateExecutionError::InitialBoardMismatch)
    );

    assert!(matches!(
        execute_validated_pc_candidate_input(
            &valid_input,
            srs_compatibility,
            &jstris_problem,
            &control,
        ),
        Err(PcCandidateExecutionError::SearchProblemCompatibility(
            Pc4SearchProblemCompatibilityError::RuleProfileMismatch { .. }
        ))
    ));
}

#[test]
fn pc_candidate_execution_bridge_rejects_tampered_count_digest_and_request_control() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let problem = pc_candidate_execution_bridge_problem(srs());
    let control = ExecutionControl::default();
    let ordinary = WasmCpuSearchBackend::execute_with_control(&problem, &control)
        .expect("ordinary exact execution");
    let target = qualified_target(
        "execution-bridge-integrity",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        1,
    );
    let input = pc_candidate_execution_bridge_input(
        target,
        problem.initial_board().occupied_mask(),
        ordinary.normalized_solution_identities().to_vec(),
    );
    let compatibility = validate_pc4_search_problem_compatibility(Pc4RuleProfile::Srs, &problem)
        .expect("SRS compatibility");

    assert_eq!(
        execute_validated_pc_candidate_input(
            &input.clone().with_test_exact_candidate_count(99),
            compatibility,
            &problem,
            &control,
        ),
        Err(PcCandidateExecutionError::CandidateCountMismatch)
    );
    assert_eq!(
        execute_validated_pc_candidate_input(
            &input.clone().with_test_candidate_set_digest([99; 32]),
            compatibility,
            &problem,
            &control,
        ),
        Err(PcCandidateExecutionError::CandidateDigestMismatch)
    );

    let wrong_queue_problem = pc_candidate_execution_bridge_problem_for_piece(srs(), PieceKind::O);
    assert_eq!(
        execute_validated_pc_candidate_input(&input, compatibility, &wrong_queue_problem, &control,),
        Err(PcCandidateExecutionError::RequestIdentityMismatch)
    );

    let partitioned = ExecutionControl::default()
        .with_partition(ExecutionPartition::new(0, 2).expect("valid test partition"));
    assert_eq!(
        execute_validated_pc_candidate_input(&input, compatibility, &problem, &partitioned,),
        Err(PcCandidateExecutionError::PartitionedExecutionControl)
    );
}
