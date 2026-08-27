use std::{cell::Cell, sync::Arc};

use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::{pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::{
    fixed_sequence::FixedSequence, observed_queue::ObservedQueue,
    queue_pattern_expression::QueuePatternExpression,
};

#[cfg(not(feature = "native-c-core"))]
use crate::service::PcFailedQueueExecutionError;
use crate::service::PercentService;

use super::*;

fn problem() -> Arc<SearchProblem> {
    let expression = QueuePatternExpression::parse("[IOTZS]", 5).expect("five-pattern expression");
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1));
    Arc::new(ProblemCompiler::compile_scenario_percent(&query).expect("failed-queue problem"))
}

fn service_problem() -> Arc<SearchProblem> {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique);
    Arc::new(ProblemCompiler::compile_scenario_pc(&query).expect("typed service problem"))
}

fn identity(problem: &SearchProblem) -> (u64, PatternUniverseId, PatternWeightModelId, usize) {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("materialized universe");
    (
        problem.piece_source().id().get(),
        universe.pattern_universe_id(),
        universe.pattern_weight_model_id(),
        universe.pattern_count(),
    )
}

fn complete() -> PcFailedQueueSourceCompleteness {
    PcFailedQueueSourceCompleteness::new(true, true, true, true)
}

fn unbounded_admission(problem: &SearchProblem) -> PcFailedQueueProducerAdmission {
    PcFailedQueueProducerAdmission::new(
        ExecutionMemoryBound::unbounded_for_problem(problem).expect("unbounded test authority"),
        0,
    )
}

struct TestRawRow {
    candidate_id: u64,
    row_kind: CoverageRowKind,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    words: Vec<u64>,
    word_access_count: Cell<usize>,
}

impl TestRawRow {
    fn new(problem: &SearchProblem, candidate_id: u64, words: Vec<u64>) -> Self {
        let (piece_source_id, pattern_universe_id, pattern_weight_model_id, pattern_count) =
            identity(problem);
        Self {
            candidate_id,
            row_kind: CoverageRowKind::Build,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            words,
            word_access_count: Cell::new(0),
        }
    }

    fn word_access_count(&self) -> usize {
        self.word_access_count.get()
    }
}

impl RawFailedQueueCoverageRow for TestRawRow {
    fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    fn row_kind(&self) -> &CoverageRowKind {
        &self.row_kind
    }

    fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }

    fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }

    fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }

    fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    fn raw_word_count(&self) -> usize {
        self.words.len()
    }

    fn projected_word_materialization_bytes(&self, _dense_word_bytes: u128) -> Option<u128> {
        Some(0)
    }

    fn raw_words(&self) -> &[u64] {
        self.word_access_count.set(self.word_access_count.get() + 1);
        &self.words
    }
}

fn produce(
    problem: Arc<SearchProblem>,
    rows: &[TestRawRow],
    example_limit: usize,
) -> Result<PcFailedQueueEvidence, PcFailedQueueEvidenceError> {
    let authority = PcFailedQueueExecutionAuthority::new(Arc::clone(&problem));
    produce_from_raw_rows(
        authority,
        rows,
        example_limit,
        complete(),
        unbounded_admission(problem.as_ref()),
    )
}

#[test]
fn exact_or_checked_counts_direct_probabilities_and_ascending_piece_evidence() {
    let problem = problem();
    let rows = [
        TestRawRow::new(problem.as_ref(), 10, vec![0b0000_0101]),
        TestRawRow::new(problem.as_ref(), 20, vec![0b0000_0010]),
    ];

    let evidence = produce(Arc::clone(&problem), &rows, usize::MAX).expect("typed evidence");
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("universe");
    let direct_success = (0..3)
        .map(|index| {
            universe
                .weights()
                .weight(PatternId::new(index))
                .expect("weight")
                .get()
        })
        .sum::<f64>();
    let direct_failed = (3..5)
        .map(|index| {
            universe
                .weights()
                .weight(PatternId::new(index))
                .expect("weight")
                .get()
        })
        .sum::<f64>();
    let direct_mass = (0..5)
        .map(|index| {
            universe
                .weights()
                .weight(PatternId::new(index))
                .expect("weight")
                .get()
        })
        .sum::<f64>();

    assert_eq!(evidence.success_coverage().words(), &[0b0000_0111]);
    assert_eq!(evidence.success_pattern_count(), 3);
    assert_eq!(evidence.failed_pattern_count(), 2);
    assert_eq!(
        evidence.success_probability().get().to_bits(),
        direct_success.to_bits()
    );
    assert_eq!(
        evidence.failed_probability().get().to_bits(),
        direct_failed.to_bits()
    );
    assert_eq!(
        evidence.materialized_probability_mass().get().to_bits(),
        direct_mass.to_bits()
    );
    assert_ne!(
        evidence.failed_probability().get().to_bits(),
        (1.0_f64 - evidence.success_probability().get()).to_bits(),
        "the authoritative failure sum must not be reconstructed as 1-success"
    );
    assert_eq!(
        evidence
            .examples()
            .iter()
            .map(PcFailedQueueExampleEvidence::pattern_index)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    for example in evidence.examples() {
        assert_eq!(
            example.pieces(),
            universe.sequence_at(example.pattern_index()).as_ref()
        );
    }
    assert_eq!(rows[0].word_access_count(), 1);
    assert_eq!(rows[1].word_access_count(), 1);
    let memory = evidence.memory_report();
    assert!(memory.admitted_producer_peak_bytes() > 0);
    assert_eq!(
        memory.retained_producer_bytes(),
        memory.retained_union_bytes() + memory.retained_example_bytes()
    );
    assert!(memory.example_materialization_upper_bound_bytes() >= memory.retained_example_bytes());
    assert!(
        memory.admitted_producer_peak_bytes()
            >= memory.source_word_materialization_upper_bound_bytes()
                + memory.union_constructor_peak_bytes()
                + memory.retained_example_bytes()
    );
}

#[test]
fn raw_word_length_and_tail_padding_fail_before_pattern_bitset_construction() {
    let problem = problem();
    let missing = TestRawRow::new(problem.as_ref(), 10, Vec::new());
    assert_eq!(
        produce(Arc::clone(&problem), std::slice::from_ref(&missing), 0),
        Err(PcFailedQueueEvidenceError::RawWordCountMismatch {
            row_index: 0,
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(missing.word_access_count(), 0);

    let padded = TestRawRow::new(problem.as_ref(), 10, vec![1_u64 << 63]);
    assert_eq!(
        produce(Arc::clone(&problem), std::slice::from_ref(&padded), 0),
        Err(PcFailedQueueEvidenceError::NonZeroRawPaddingBits {
            row_index: 0,
            word_index: 0,
            invalid_bits: 1_u64 << 63,
        })
    );
    assert_eq!(padded.word_access_count(), 1);
}

#[test]
fn row_kind_dimensions_and_every_universe_identity_fail_closed() {
    let problem = problem();

    let mut wrong_kind = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    wrong_kind.row_kind = CoverageRowKind::Pc;
    assert!(matches!(
        produce(Arc::clone(&problem), &[wrong_kind], 0),
        Err(PcFailedQueueEvidenceError::RowKindMismatch { row_index: 0 })
    ));

    let mut wrong_source = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    wrong_source.piece_source_id ^= 1;
    assert!(matches!(
        produce(Arc::clone(&problem), &[wrong_source], 0),
        Err(PcFailedQueueEvidenceError::PieceSourceMismatch { row_index: 0, .. })
    ));

    let mut wrong_universe = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    wrong_universe.pattern_universe_id = PatternUniverseId::new(999);
    assert!(matches!(
        produce(Arc::clone(&problem), &[wrong_universe], 0),
        Err(PcFailedQueueEvidenceError::PatternUniverseMismatch { row_index: 0, .. })
    ));

    let mut wrong_weights = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    wrong_weights.pattern_weight_model_id = PatternWeightModelId::new(999);
    assert!(matches!(
        produce(Arc::clone(&problem), &[wrong_weights], 0),
        Err(PcFailedQueueEvidenceError::PatternWeightModelMismatch { row_index: 0, .. })
    ));

    let mut wrong_count = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    wrong_count.pattern_count += 1;
    assert!(matches!(
        produce(Arc::clone(&problem), &[wrong_count], 0),
        Err(PcFailedQueueEvidenceError::PatternCountMismatch { row_index: 0, .. })
    ));
}

#[test]
fn zero_duplicate_and_out_of_order_candidate_rows_are_rejected() {
    let problem = problem();
    let zero = TestRawRow::new(problem.as_ref(), 0, vec![1]);
    assert_eq!(
        produce(Arc::clone(&problem), &[zero], 0),
        Err(PcFailedQueueEvidenceError::ZeroCandidateId { row_index: 0 })
    );

    let duplicate = [
        TestRawRow::new(problem.as_ref(), 10, vec![1]),
        TestRawRow::new(problem.as_ref(), 10, vec![2]),
    ];
    assert_eq!(
        produce(Arc::clone(&problem), &duplicate, 0),
        Err(PcFailedQueueEvidenceError::DuplicateCandidateId {
            row_index: 1,
            candidate_id: 10,
        })
    );

    let reversed = [
        TestRawRow::new(problem.as_ref(), 20, vec![1]),
        TestRawRow::new(problem.as_ref(), 10, vec![2]),
    ];
    assert_eq!(
        produce(Arc::clone(&problem), &reversed, 0),
        Err(PcFailedQueueEvidenceError::CandidateOrderViolation {
            row_index: 1,
            previous_candidate_id: 20,
            candidate_id: 10,
        })
    );
}

#[test]
fn every_incomplete_execution_stage_fails_before_raw_words_are_read() {
    let problem = problem();
    let stages = [
        (
            PcFailedQueueSourceCompleteness::new(false, true, true, true),
            PcFailedQueueIncompleteStage::Packing,
        ),
        (
            PcFailedQueueSourceCompleteness::new(true, false, true, true),
            PcFailedQueueIncompleteStage::BuildUpCount,
        ),
        (
            PcFailedQueueSourceCompleteness::new(true, true, false, true),
            PcFailedQueueIncompleteStage::MaterializedCoverage,
        ),
        (
            PcFailedQueueSourceCompleteness::new(true, true, true, false),
            PcFailedQueueIncompleteStage::Objective,
        ),
    ];
    for (completeness, stage) in stages {
        let row = TestRawRow::new(problem.as_ref(), 10, vec![1]);
        let result = produce_from_raw_rows(
            PcFailedQueueExecutionAuthority::new(Arc::clone(&problem)),
            std::slice::from_ref(&row),
            0,
            completeness,
            unbounded_admission(problem.as_ref()),
        );
        assert_eq!(
            result,
            Err(PcFailedQueueEvidenceError::IncompleteExecution { stage })
        );
        assert_eq!(row.word_access_count(), 0);
    }
}

#[test]
fn evidence_retains_the_exact_executed_problem_and_execution_authority() {
    let problem = problem();
    let equal_but_foreign_owner = Arc::new(problem.as_ref().clone());
    assert_eq!(problem.as_ref(), equal_but_foreign_owner.as_ref());
    let row = TestRawRow::new(problem.as_ref(), 10, vec![1]);
    let evidence = produce(Arc::clone(&problem), &[row], 0).expect("evidence");

    assert!(evidence.matches_problem_owner(&problem));
    assert!(!evidence.matches_problem_owner(&equal_but_foreign_owner));
    assert!(evidence.authority().same_execution(evidence.authority()));
    let separate_authority = PcFailedQueueExecutionAuthority::new(Arc::clone(&problem));
    assert!(!evidence.authority().same_execution(&separate_authority));
}

#[test]
fn one_byte_short_memory_authority_rejects_before_raw_word_or_output_allocation() {
    let problem = problem();
    let baseline_rows = [
        TestRawRow::new(problem.as_ref(), 10, vec![1]),
        TestRawRow::new(problem.as_ref(), 20, vec![2]),
    ];
    let baseline = produce(Arc::clone(&problem), &baseline_rows, 2).expect("baseline evidence");
    let required = baseline.memory_report().admitted_producer_peak_bytes();
    assert!(required > 0);

    let short_bound = ExecutionMemoryBound::unbounded_for_problem(problem.as_ref())
        .expect("unbounded authority")
        .with_cap(required - 1)
        .expect("narrow authority");
    let short_rows = [
        TestRawRow::new(problem.as_ref(), 10, vec![1]),
        TestRawRow::new(problem.as_ref(), 20, vec![2]),
    ];
    let result = produce_from_raw_rows(
        PcFailedQueueExecutionAuthority::new(Arc::clone(&problem)),
        &short_rows,
        2,
        complete(),
        PcFailedQueueProducerAdmission::new(short_bound, 0),
    );
    assert!(matches!(
        result,
        Err(PcFailedQueueEvidenceError::MemoryAdmission(_))
    ));
    assert_eq!(short_rows[0].word_access_count(), 0);
    assert_eq!(short_rows[1].word_access_count(), 0);

    let exact_bound = ExecutionMemoryBound::unbounded_for_problem(problem.as_ref())
        .expect("unbounded authority")
        .with_cap(required)
        .expect("exact authority");
    let exact_rows = [
        TestRawRow::new(problem.as_ref(), 10, vec![1]),
        TestRawRow::new(problem.as_ref(), 20, vec![2]),
    ];
    let exact = produce_from_raw_rows(
        PcFailedQueueExecutionAuthority::new(Arc::clone(&problem)),
        &exact_rows,
        2,
        complete(),
        PcFailedQueueProducerAdmission::new(exact_bound, 0),
    )
    .expect("exact cap admits producer");
    assert_eq!(exact.memory_report().admission_cap_bytes(), required);
}

#[test]
fn synthetic_union_outer_and_inner_capacities_are_each_bound_into_re_admission() {
    let problem = problem();
    let logical_union =
        checked_union_constructor_peak_bytes(1, 1).expect("logical union projection");
    let oversized_union =
        checked_union_constructor_peak_bytes(2, 1).expect("actual union capacity projection");
    assert!(oversized_union > logical_union);

    let one_piece_bytes = checked_bytes::<PieceKind>(1).expect("one piece capacity");
    let logical_examples =
        checked_example_capacity_bytes(1, one_piece_bytes).expect("logical example projection");
    let oversized_outer = checked_example_capacity_bytes(2, one_piece_bytes)
        .expect("actual outer capacity projection");
    let oversized_inner = checked_example_capacity_bytes(
        1,
        checked_bytes::<PieceKind>(2).expect("actual inner capacity"),
    )
    .expect("actual inner capacity projection");
    assert!(oversized_outer > logical_examples);
    assert!(oversized_inner > logical_examples);

    let source_bytes = 8;
    let logical_peak = checked_producer_peak_bytes(source_bytes, logical_union, logical_examples)
        .expect("logical producer projection");

    let bound = ExecutionMemoryBound::unbounded_for_problem(problem.as_ref())
        .expect("unbounded authority")
        .with_cap(logical_peak)
        .expect("logical-only authority");
    let admission = PcFailedQueueProducerAdmission::new(bound, 0);
    assert_eq!(admission.ensure(logical_peak), Ok(()));
    for actual_peak in [
        checked_producer_peak_bytes(source_bytes, oversized_union, logical_examples)
            .expect("oversized union producer peak"),
        checked_producer_peak_bytes(source_bytes, logical_union, oversized_outer)
            .expect("oversized outer producer peak"),
        checked_producer_peak_bytes(source_bytes, logical_union, oversized_inner)
            .expect("oversized inner producer peak"),
    ] {
        assert!(matches!(
            admission.ensure(actual_peak),
            Err(PcFailedQueueEvidenceError::MemoryAdmission(_))
        ));
    }
}

#[test]
fn checked_complement_rejects_impossible_success_count_without_saturation() {
    assert_eq!(checked_failed_count(5, 3), Ok(2));
    assert_eq!(
        checked_failed_count(2, 3),
        Err(PcFailedQueueEvidenceError::FailedCountUnderflow {
            pattern_count: 2,
            success_pattern_count: 3,
        })
    );
}

#[test]
fn authoritative_coverage_row_entrypoint_uses_the_same_validated_producer() {
    let problem = problem();
    let (piece_source_id, pattern_universe_id, pattern_weight_model_id, pattern_count) =
        identity(problem.as_ref());
    let rows = vec![CoverageRow::new_with_piece_source(
        10,
        CoverageRowKind::Build,
        piece_source_id,
        pattern_universe_id,
        pattern_weight_model_id,
        PatternBitSet::from_patterns(pattern_count, [PatternId::new(0), PatternId::new(2)])
            .expect("coverage"),
    )];
    let evidence = PcFailedQueueEvidenceProducer::produce(
        PcFailedQueueExecutionAuthority::new(Arc::clone(&problem)),
        &rows,
        1,
        complete(),
        unbounded_admission(problem.as_ref()),
    )
    .expect("authoritative producer");

    assert_eq!(evidence.success_coverage().words(), &[0b00101]);
    assert_eq!(evidence.failed_pattern_count(), 3);
    assert_eq!(evidence.examples()[0].pattern_index(), 1);
    assert_eq!(
        evidence
            .memory_report()
            .source_word_materialization_upper_bound_bytes(),
        core::mem::size_of::<u64>() as u128
    );
}

#[test]
fn complete_source_mass_is_checked_because_weighted_sets_can_represent_subprobability_mass() {
    let weights = WeightedPatternSet::new(vec![
        ProbabilityValue::new(0.25).expect("weight"),
        ProbabilityValue::new(0.25).expect("weight"),
    ])
    .expect("subprobability weights are valid in the generic weight container");
    let (_, _, mass) = directly_sum_probabilities(&weights, &[0], 2).expect("direct sums");

    assert_eq!(mass.get().to_bits(), 0.5_f64.to_bits());
    assert_eq!(
        validate_complete_materialized_probability_mass(mass),
        Err(
            PcFailedQueueEvidenceError::CompleteProbabilityMassMismatch {
                actual_bits: 0.5_f64.to_bits(),
            }
        )
    );
    let rounding_near_one = checked_probability_from_direct_sum(
        PcFailedQueueProbabilityClass::MaterializedMass,
        1.0 - f64::EPSILON,
        2,
    )
    .expect("summation-scale rounding is canonicalized");
    assert_eq!(rounding_near_one.get().to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        validate_complete_materialized_probability_mass(rounding_near_one),
        Ok(())
    );
    for class in [
        PcFailedQueueProbabilityClass::Success,
        PcFailedQueueProbabilityClass::Failed,
    ] {
        let direct_subset_sum = 1.0 - f64::EPSILON;
        let probability = checked_probability_from_direct_sum(class, direct_subset_sum, 2)
            .expect("direct subset sum remains representable");
        assert_eq!(
            probability.get().to_bits(),
            direct_subset_sum.to_bits(),
            "success and failure probabilities must retain their direct-sum bits"
        );
    }
}

#[test]
fn typed_percent_service_returns_same_run_authority_or_explicit_unavailable_error() {
    let problem = service_problem();
    let execution = PercentService::execute_failed_queue(Arc::clone(&problem), 4);

    #[cfg(feature = "native-c-core")]
    {
        let execution = execution.expect("native typed failed-queue execution");
        assert!(execution.evidence().matches_problem_owner(&problem));
        assert_eq!(
            execution.evidence().success_coverage().words(),
            execution.result().coverage_pattern_words()
        );
        assert_eq!(
            execution.evidence().success_pattern_count(),
            execution
                .result()
                .usize_field("covered_pattern_count")
                .expect("covered count")
        );
    }

    #[cfg(not(feature = "native-c-core"))]
    match execution {
        Err(PcFailedQueueExecutionError::Percent(error)) => assert_eq!(
            error.unsupported_reason(),
            Some("core_c_packing_runtime_unavailable")
        ),
        other => panic!("expected explicit native runtime unavailability, got {other:?}"),
    }
}

#[test]
fn typed_service_emits_no_evidence_for_cancelled_incomplete_or_memory_rejected_runs() {
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    assert!(PercentService::execute_failed_queue_with_cancellation(
        service_problem(),
        1,
        &cancellation,
    )
    .is_err());

    let incomplete_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::observed(ObservedQueue::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_supply_window_size(SupplyWindowSize::new(3))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_patterns(1));
    let incomplete = Arc::new(
        ProblemCompiler::compile_scenario_pc(&incomplete_query)
            .expect("incomplete service problem"),
    );
    assert!(PercentService::execute_failed_queue(incomplete, 1).is_err());

    let memory_rejected_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_memory_mib(Some(0)));
    let memory_rejected = Arc::new(
        ProblemCompiler::compile_scenario_pc(&memory_rejected_query)
            .expect("memory-rejected service problem"),
    );
    assert!(PercentService::execute_failed_queue(memory_rejected, 1).is_err());
}
