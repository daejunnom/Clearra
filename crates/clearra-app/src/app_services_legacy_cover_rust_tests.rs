use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::build_coverage_limits::BuildCoverageLimits,
    template::{BuildSlot, BuildSlotId, BuildTemplate},
};
use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    execution_cancellation::ExecutionCancellationToken,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::SolutionCoverage;
use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge};

use super::*;

fn one_horizontal_i(pattern_count: usize, max_patterns: usize) -> BuildCoverageQuery {
    let board = BoardSize::new(4, 1).expect("bounded mechanism board");
    let id = BuildSlotId::new(17);
    let slot = BuildSlot::new(id, (0..4).map(|x| CellCoord::new_unchecked(x, 0)).collect())
        .with_allowed_pieces(vec![PieceKind::I]);
    BuildCoverageQuery::new(
        BuildTemplate::new("legacy-single-positive-mechanism", vec![slot]).with_board_size(board),
        vec![SlotDomain::new(id, vec![PieceKind::I])],
        Vec::new(),
        pattern_count,
        BuildCoverageLimits::new(SearchDefaults::MVP1.build_max_assignments(), max_patterns),
    )
}

// Witness 1 from the bounded Native C positive seam in legacy_cover_native_witness.rs.
// Each mask uses original rows; the independent hard-drop trace clears [1], [3], [0, 2].
const NATIVE_FOUR_LINE_WITNESS: [(PieceKind, u64); 10] = [
    (PieceKind::I, 0x78),
    (PieceKind::O, 0xc0300),
    (PieceKind::T, 0x100e000),
    (PieceKind::S, 0x1803),
    (PieceKind::Z, 0x6030000),
    (PieceKind::J, 0xc0100400),
    (PieceKind::L, 0x3008000080),
    (PieceKind::I, 0xf00000000),
    (PieceKind::O, 0xc030000000),
    (PieceKind::T, 0xe00004),
];

fn exact_four_line_witness(pattern_count: usize) -> BuildCoverageQuery {
    let board = BoardSize::new(10, 4).expect("bounded standard four-line board");
    let (slots, domains) = NATIVE_FOUR_LINE_WITNESS
        .into_iter()
        .enumerate()
        .map(|(index, (piece, mask))| {
            let id = BuildSlotId::new(u32::try_from(index).expect("fixture slot fits u32"));
            let cells = (0..40_u16)
                .filter(|&cell| mask & (1_u64 << cell) != 0)
                .map(|cell| CellCoord::new_unchecked(cell % 10, cell / 10))
                .collect();
            (
                BuildSlot::new(id, cells).with_allowed_pieces(vec![piece]),
                SlotDomain::new(id, vec![piece]),
            )
        })
        .unzip();
    BuildCoverageQuery::new(
        BuildTemplate::new("legacy-four-line-materialized-prefix", slots).with_board_size(board),
        domains,
        Vec::new(),
        pattern_count,
        BuildCoverageLimits::default(),
    )
}

fn compile(query: &BuildCoverageQuery) -> SearchProblem {
    let template = query.template();
    let limits = query.limits();
    ProblemCompiler::compile_build(&BuildQuery::coverage_bridge(
        BuildTemplateBridge::new(template.id(), template.board_size(), template.slots().len()),
        query.pattern_count(),
        BuildProblemLimits::new(limits.max_assignments(), limits.max_patterns()),
    ))
    .expect("canonical Legacy coverage bridge")
}

#[test]
fn rust_legacy_cover_positive_mechanism_uses_original_ordinal_and_exact_assignment() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = one_horizontal_i(1, SearchDefaults::MVP1.build_max_patterns());
    let problem = compile(&query);
    let source = problem
        .piece_source()
        .materialized_universe()
        .expect("original observed supply");
    assert!(source.complete());
    assert!(
        source.pattern_count() > 1,
        "denominator is the requested prefix"
    );
    assert_eq!(source.sequence_at(0).as_ref(), &[PieceKind::I]);
    let expected = StandardBoard64TilingIdentity::from_placements(
        0,
        [PiecePlacementMask::new(PieceKind::I, 0xf)],
    )
    .expect("independent hard-drop I mask");
    assert_eq!(concrete_assignment_identity(&query), Ok(expected));

    let execution =
        execute_single_ordinal_candidate(&problem, &query, &ExecutionControl::default())
            .expect("a known Rust positive, not a C overlap or product routing proof");
    assert!(execution.exact_cover_complete());
    assert_eq!(execution.assignments().len(), 1);
    assert_eq!(execution.matrix().matrix().rows().len(), 1);
    assert_eq!(execution.union().covered_patterns().pattern_count(), 1);
    assert_eq!(execution.union().covered_patterns().word_at(0), 1);
    assert_eq!(execution.result().probability().get(), 1.0);
}

#[test]
fn rust_legacy_cover_truncated_source_exact_prefix_matches_native_four_line_witness() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = exact_four_line_witness(1);
    let legacy = compile(&query);
    let source = legacy.piece_source().materialized_universe().unwrap();
    assert!(!source.complete());
    assert_eq!(source.pattern_count(), query.limits().max_patterns());
    assert_eq!(
        source.sequence_at(0).as_ref(),
        &NATIVE_FOUR_LINE_WITNESS.map(|(piece, _)| piece)
    );

    let evidence =
        execute_truncated_source_exact_prefix(&legacy, &query, &ExecutionControl::default())
            .expect("only the native-proven original ordinal and physical assignment are admitted");
    let exact = StandardBoard64TilingIdentity::from_placements(
        0,
        NATIVE_FOUR_LINE_WITNESS.map(|(piece, mask)| PiecePlacementMask::new(piece, mask)),
    )
    .expect("independent native witness ownership");
    assert_eq!(evidence.exact_assignment, exact);
    assert_eq!(
        evidence.original_source_id,
        legacy.piece_source().id().get()
    );
    assert_eq!(evidence.original_ordinal, 0);
    assert_eq!(evidence.materialized_pattern_count, source.pattern_count());
    assert_eq!(
        evidence.total_possible_pattern_count,
        source.total_possible_pattern_count()
    );
    assert!(evidence.total_possible_pattern_count > evidence.materialized_pattern_count as u128);
    assert_eq!(
        evidence.truncation_reason,
        SupplyTruncationReason::ObservedWindowBudgetExceeded
    );
    assert_eq!(evidence.prefix_coverage.pattern_count(), 1);
    assert_eq!(evidence.prefix_coverage.word_at(0), 1);

    // There is still no complete result for the original Cover source.
    assert!(matches!(
        execute_single_ordinal_candidate(&legacy, &query, &ExecutionControl::default()),
        Err(RustLegacyCoverCandidateError::UnsupportedAuthority
            | RustLegacyCoverCandidateError::IncompleteOriginalSupply)
    ));
}

#[test]
fn rust_legacy_cover_truncated_prefix_admission_preserves_scope_and_cancellation() {
    let query = exact_four_line_witness(2);
    let legacy = compile(&query);
    assert!(matches!(
        execute_truncated_source_exact_prefix(&legacy, &query, &ExecutionControl::default()),
        Err(RustLegacyCoverCandidateError::UnsupportedAuthority)
    ));

    let query = exact_four_line_witness(1);
    let legacy = compile(&query);
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert!(matches!(
        execute_truncated_source_exact_prefix(&legacy, &query, &ExecutionControl::new(token)),
        Err(RustLegacyCoverCandidateError::Cancelled)
    ));
}

#[test]
fn rust_legacy_cover_rejects_truncated_original_supply_before_physical_execution() {
    let query = one_horizontal_i(1, 1);
    let problem = compile(&query);
    assert!(!problem.piece_source().complete());
    assert!(matches!(
        execute_single_ordinal_candidate(&problem, &query, &ExecutionControl::default()),
        Err(RustLegacyCoverCandidateError::IncompleteOriginalSupply)
    ));
}

#[test]
fn rust_legacy_cover_does_not_expand_ordinal_scope_or_clear_cancellation() {
    let query = one_horizontal_i(2, SearchDefaults::MVP1.build_max_patterns());
    let problem = compile(&query);
    assert!(matches!(
        execute_single_ordinal_candidate(&problem, &query, &ExecutionControl::default()),
        Err(RustLegacyCoverCandidateError::UnsupportedAuthority)
    ));
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert!(matches!(
        execute_single_ordinal_candidate(&problem, &query, &ExecutionControl::new(token)),
        Err(RustLegacyCoverCandidateError::Cancelled)
    ));
}

#[test]
fn rust_legacy_cover_refuses_a_same_color_foreign_concrete_row() {
    let exact = |masks: [u64; 4]| {
        StandardBoard64TilingIdentity::from_placements(
            0,
            masks.map(|cells| PiecePlacementMask::new(PieceKind::I, cells)),
        )
        .expect("disjoint placements")
    };
    let horizontal = exact([0x000f, 0x00f0, 0x0f00, 0xf000]);
    let vertical = exact([0x1111, 0x2222, 0x4444, 0x8888]);
    assert_ne!(horizontal, vertical);
    assert_eq!(
        StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(horizontal),
        StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(vertical),
    );
    let result = CoreExecutionResult::new(
        [
            ("coverage_pattern_count", "1"),
            ("count_complete", "true"),
            ("probability_complete", "true"),
            ("solution_probability_complete", "true"),
            ("build_variant_count_exact", "true"),
            ("resource_truncated", "false"),
        ]
        .map(|(key, value)| (key.into(), value.into()))
        .to_vec(),
        Vec::new(),
    )
    .with_solution_coverages(vec![SolutionCoverage::new(
        vertical,
        PatternBitSet::from_words(1, vec![1]).expect("bounded positive row bit"),
    )]);
    assert_eq!(
        exact_positive_bits(&result, horizontal),
        Err(RustLegacyCoverCandidateError::MissingExactPositiveRow)
    );
}
