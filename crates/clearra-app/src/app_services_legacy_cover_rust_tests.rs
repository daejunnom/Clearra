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
    token.cancel();
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
