use clearra_core_domain::{
    piece::piece_kind::PieceKind, solution::StandardBoard64ColoredTilingIdentity,
};
use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
use clearra_supply::{
    finite_build_piece_source_returned_carrier_delta_bytes,
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        queue_pattern_expression::QueuePatternExpression,
    },
    FiniteSupplyAllocationLedger, PatternUniverseMaterializationError,
};

use super::*;
use crate::query::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
};

const EXTERNAL_OWNER_BYTES: u128 = 31;
const RETURNED_CARRIER_BYTES: u128 = 47;

fn fixed_query() -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false)
}

fn explicit_pattern_query() -> PcScenarioQuery {
    let expression = QueuePatternExpression::parse("[IO]!", 2).expect("two explicit permutations");
    assert!(!expression.is_factorized());
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_allow_hold(false)
}

fn factorized_pattern_query() -> PcScenarioQuery {
    let expression = QueuePatternExpression::parse("P7P7P2", 1_066_867_200)
        .expect("factorized standard-bag expression");
    assert!(expression.is_factorized());
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(16),
    )
    .with_exact_pieces(Some(16))
    .with_allow_hold(false)
}

fn standard_bag_query() -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false)
}

fn assert_exact_cap_and_one_byte_short(factory: fn() -> PcScenarioQuery) {
    let unbounded = FiniteScenarioPcCompileBudget::try_new(
        u128::MAX,
        EXTERNAL_OWNER_BYTES,
        RETURNED_CARRIER_BYTES,
    )
    .expect("finite budget");
    let discovered = ProblemCompiler::compile_scenario_pc_finite_build(factory(), unbounded)
        .expect("supported finite compilation");
    let exact_peak = discovered.peak_required_memory_bytes();
    let expected_problem_id = ProblemCompiler::compile_scenario_pc(&factory())
        .expect("compatibility compilation")
        .problem_id()
        .as_str()
        .to_owned();
    assert_eq!(
        discovered.problem().problem_id().as_str(),
        expected_problem_id
    );
    assert_eq!(
        discovered.problem_retained_bytes(),
        discovered
            .problem()
            .checked_build_probability_pointee_retained_bytes()
            .expect("finite problem remains measurable")
    );

    let exact = FiniteScenarioPcCompileBudget::try_new(
        exact_peak,
        EXTERNAL_OWNER_BYTES,
        RETURNED_CARRIER_BYTES,
    )
    .expect("exact budget");
    let exact_compilation = ProblemCompiler::compile_scenario_pc_finite_build(factory(), exact)
        .expect("actual peak must be admitted exactly");
    assert_eq!(exact_compilation.peak_required_memory_bytes(), exact_peak);

    let one_byte_short = FiniteScenarioPcCompileBudget::try_new(
        exact_peak
            .checked_sub(1)
            .expect("finite peak includes the problem owner"),
        EXTERNAL_OWNER_BYTES,
        RETURNED_CARRIER_BYTES,
    )
    .expect("short budget");
    assert_eq!(
        ProblemCompiler::compile_scenario_pc_finite_build(factory(), one_byte_short),
        Err(FiniteScenarioPcCompileError::MemoryCapacityExceeded {
            required_memory_bytes: exact_peak,
            max_memory_bytes: exact_peak - 1,
        })
    );
}

#[test]
fn every_supported_queue_has_an_actual_exact_closed_cap_boundary() {
    for factory in [
        fixed_query as fn() -> PcScenarioQuery,
        explicit_pattern_query,
        factorized_pattern_query,
        standard_bag_query,
    ] {
        assert_exact_cap_and_one_byte_short(factory);
    }
}

#[test]
fn finite_compile_returns_a_measured_move_only_problem() {
    let budget =
        FiniteScenarioPcCompileBudget::try_new(u128::MAX, 0, 0).expect("unbounded finite budget");
    let compilation = ProblemCompiler::compile_scenario_pc_finite_build(fixed_query(), budget)
        .expect("finite builders are active");
    let retained = compilation.problem_retained_bytes();
    let (problem, peak, returned_retained) = compilation.into_parts();

    assert_eq!(returned_retained, retained);
    assert!(peak >= retained);
    assert_eq!(
        problem.checked_build_probability_pointee_retained_bytes(),
        Some(retained)
    );
}

#[test]
fn finite_compile_moves_the_original_query_queue_and_owns_one_governed_supply_duplicate() {
    let mut pieces = Vec::with_capacity(43);
    pieces.push(PieceKind::O);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false);
    let original_sequence = query
        .remaining_queue()
        .as_fixed_sequence()
        .expect("fixed query queue");
    let original_pointer = original_sequence.pieces().as_ptr();
    let original_capacity_bytes = original_sequence
        .checked_retained_capacity_bytes()
        .expect("fixed query capacity");
    let budget =
        FiniteScenarioPcCompileBudget::try_new(u128::MAX, 0, 0).expect("unbounded finite budget");

    let compilation = ProblemCompiler::compile_scenario_pc_finite_build(query, budget)
        .expect("finite compilation");
    let moved_sequence = compilation
        .problem()
        .scenario()
        .core_query()
        .remaining_queue()
        .as_fixed_sequence()
        .expect("moved scenario queue");
    let retained_sequence = compilation
        .problem()
        .supply()
        .queue()
        .as_fixed_sequence()
        .expect("retained supply duplicate");

    assert_eq!(moved_sequence.pieces().as_ptr(), original_pointer);
    assert_eq!(
        moved_sequence.checked_retained_capacity_bytes(),
        Some(original_capacity_bytes)
    );
    assert_ne!(retained_sequence.pieces().as_ptr(), original_pointer);
    assert_eq!(retained_sequence.pieces(), moved_sequence.pieces());
}

#[test]
fn rejected_queue_shape_fails_before_cap_admission() {
    let bag_aligned = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false);
    let zero_budget = FiniteScenarioPcCompileBudget::try_new(0, 0, 0).expect("zero budget");

    assert_eq!(
        ProblemCompiler::compile_scenario_pc_finite_build(bag_aligned, zero_budget),
        Err(FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape)
    );
}

#[test]
fn finite_compile_moves_supplied_identity_filter_without_cloning_it() {
    let identity = StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
        .expect("empty colored identity is structurally valid");
    let query = fixed_query().with_allowed_colored_solution_identities([identity]);
    let original = query
        .allowed_colored_solution_identities()
        .expect("query owns the supplied filter");
    let original_pointer = original.as_ptr();
    let budget =
        FiniteScenarioPcCompileBudget::try_new(u128::MAX, 0, 0).expect("unbounded finite budget");

    let compilation = ProblemCompiler::compile_scenario_pc_finite_build(query, budget)
        .expect("selected-solution finite compilation");
    let problem = compilation.problem();
    let moved = problem
        .allowed_colored_solution_identities()
        .expect("compiled problem preserves the actual filter");

    assert_eq!(moved, &[identity]);
    assert_eq!(moved.as_ptr(), original_pointer);
    assert!(problem
        .scenario()
        .core_query()
        .allowed_colored_solution_identities()
        .is_none());
    assert_eq!(
        problem.checked_build_probability_pointee_retained_bytes(),
        Some(compilation.problem_retained_bytes())
    );
}

#[test]
fn semantic_rejection_is_copy_and_precedes_allocation_authority_use() {
    fn assert_copy<T: Copy>() {}
    assert_copy::<FiniteScenarioPcCompileError>();

    let max_pieces = usize::from(u16::MAX) + 1;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(max_pieces),
    );
    let budget = FiniteScenarioPcCompileBudget::try_new(u128::MAX, 0, 0)
        .expect("unbounded projection budget");

    assert_eq!(
        ProblemCompiler::compile_scenario_pc_finite_build(query, budget),
        Err(FiniteScenarioPcCompileError::ProblemCompile(
            ProblemCompileError::PackingPieceWindowTooLarge { max_pieces }
        ))
    );

    let empty_standard_bag = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(0),
    )
    .with_exact_pieces(Some(0))
    .with_allow_hold(false);
    assert_eq!(
        ProblemCompiler::compile_scenario_pc_finite_build(empty_standard_bag, budget),
        Err(FiniteScenarioPcCompileError::ProblemCompile(
            ProblemCompileError::PatternUniverseMaterialization(
                PatternUniverseMaterializationError::NoPatterns
            )
        ))
    );
}

#[test]
fn budget_and_projection_arithmetic_fail_closed() {
    assert_eq!(
        FiniteScenarioPcCompileBudget::try_new(u128::MAX, u128::MAX, 1),
        Err(FiniteScenarioPcCompileError::ProjectionOverflow)
    );

    let budget = FiniteScenarioPcCompileBudget::try_new(u128::MAX, 11, 13).expect("finite budget");
    let projection = ProblemCompiler::checked_finite_build_scenario_pc_compile_projection(
        &fixed_query(),
        budget,
    )
    .expect("fixed projection");
    assert!(projection.requested_peak_bytes() >= projection.projected_problem_retained_bytes());
    assert!(projection.effective_returned_carrier_bytes() >= budget.returned_carrier_bytes());
    assert_eq!(
        projection.requested_peak_bytes(),
        budget.external_retained_owner_bytes()
            + projection.effective_returned_carrier_bytes()
            + projection.projected_problem_retained_bytes()
            + finite_build_piece_source_returned_carrier_delta_bytes()
    );

    let compiler_only =
        FiniteScenarioPcCompileBudget::try_new(u128::MAX, 0, 0).expect("compiler-only budget");
    let compiler_only_projection =
        ProblemCompiler::checked_finite_build_scenario_pc_compile_projection(
            &fixed_query(),
            compiler_only,
        )
        .expect("fixed compiler-only projection");
    assert_eq!(
        compiler_only_projection.effective_returned_carrier_bytes(),
        (core::mem::size_of::<Result<FiniteScenarioPcCompilation, FiniteScenarioPcCompileError>>()
            as u128)
            - core::mem::size_of::<SearchProblem>() as u128
    );

    let actual = ProblemCompiler::compile_scenario_pc_finite_build(fixed_query(), compiler_only)
        .expect("finite compilation");
    assert!(actual.peak_required_memory_bytes() >= compiler_only_projection.requested_peak_bytes());
}

#[test]
fn finite_problem_id_writer_rejects_growth_before_mutating_the_string() {
    let mut ledger = FiniteSupplyAllocationLedger::try_new(64, 7).expect("finite ledger");
    let original_live = ledger.live_memory_bytes();
    let original_peak = ledger.peak_memory_bytes();
    {
        let mut transaction = ledger.transaction();
        let mut value = transaction
            .try_string_with_capacity(16)
            .expect("governed test string");
        let mut writer = FiniteProblemIdWriter::new(&mut value, 1);

        assert_eq!(
            writer.try_push_str("ab"),
            Err(
                FiniteScenarioPcCompileError::ProblemIdAuthorizedLengthExceeded {
                    authorized_bytes: 1,
                    attempted_bytes: 2,
                }
            )
        );
        assert!(writer.value.is_empty());
    }
    assert_eq!(ledger.live_memory_bytes(), original_live);
    assert_eq!(ledger.peak_memory_bytes(), original_peak);
}

#[test]
fn typed_build_query_splits_without_cloning_the_scenario_queue() {
    let mut pieces = Vec::with_capacity(43);
    pieces.push(PieceKind::O);
    let core = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false);
    let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0c03, 0, 0, 0])
        .expect("one-piece field");
    let query = BuildProbabilityQuery::new(core, field);
    let sequence = query
        .core_query()
        .remaining_queue()
        .as_fixed_sequence()
        .expect("fixed sequence");
    let original_pointer = sequence.pieces().as_ptr();
    let original_capacity_bytes = sequence
        .checked_retained_capacity_bytes()
        .expect("fixed retained capacity");

    let (core, returned_field, aggregation, finesse, solution_probability_policy) =
        query.into_finite_compile_parts();
    let moved_sequence = core
        .remaining_queue()
        .as_fixed_sequence()
        .expect("moved fixed sequence");

    assert_eq!(moved_sequence.pieces().as_ptr(), original_pointer);
    assert_eq!(
        moved_sequence.checked_retained_capacity_bytes(),
        Some(original_capacity_bytes)
    );
    assert_eq!(returned_field, field);
    assert_eq!(aggregation, BuildProbabilityAggregation::Buildability);
    assert_eq!(finesse, BuildProbabilityFinesseRequest::Off);
    assert_eq!(
        solution_probability_policy,
        BuildSolutionProbabilityPolicy::Omit
    );
}
