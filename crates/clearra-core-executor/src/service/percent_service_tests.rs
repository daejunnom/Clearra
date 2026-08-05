use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::{fixed_sequence::FixedSequence, observed_queue::ObservedQueue};

use super::*;

#[test]
fn percent_reports_pattern_counts_probability_and_c_buildup_rows() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(result.field("total_pattern_count"), Some("1"));
    assert_eq!(
        result.field("covered_pattern_count"),
        Some(expected_covered_pattern_count())
    );
    assert_eq!(result.field("probability"), Some(expected_probability()));
    assert_eq!(
        result.field("probability_complete"),
        Some(expected_probability_complete())
    );
    assert_eq!(result.field("renormalized"), Some("false"));
    assert_eq!(
        result.field("coverage_probability"),
        Some(expected_probability())
    );
    assert_eq!(
        result.field("c_buildup_coverage_row_count"),
        Some(expected_c_buildup_coverage_row_count())
    );
    assert_eq!(
        result.field("percent_reports_total_pattern_count"),
        Some("true")
    );
    assert_eq!(
        result.field("percent_reports_covered_pattern_count"),
        Some("true")
    );
    assert_eq!(
        result.field("percent_reports_probability_complete"),
        Some("true")
    );
    assert_eq!(
        result.field("pattern_bitset_union"),
        Some("PatternBitSet OR union")
    );
    assert_eq!(
        result.field("weighted_probability_reducer"),
        Some("union_probability")
    );
    assert_eq!(result.coverage_pattern_words().len(), 1);
    assert_eq!(
        result
            .coverage_pattern_words()
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>(),
        result
            .usize_field("covered_pattern_count")
            .unwrap_or_default()
    );
}

#[test]
fn complete_percent_with_no_solution_returns_an_exact_zero_coverage_set() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(result.field("covered_pattern_count"), Some("0"));
    assert_eq!(result.field("probability"), Some("0"));
    assert_eq!(result.field("probability_complete"), Some("true"));
    assert_eq!(result.coverage_pattern_words(), &[0]);
}

#[test]
fn truncated_observed_percent_reports_verified_materialized_patterns_without_claiming_completion() {
    let query = PcScenarioQuery::new(
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
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let packing = PackingRunner::run(&problem).expect("packing");
    let buildup = BuildUpRunner::run_for_coverage(&problem, &packing).expect("buildup");
    assert!(!buildup.coverage_complete());
    assert!(buildup.materialized_coverage_complete());

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(result.field("materialized_pattern_count"), Some("1"));
    assert_eq!(result.field("verified_pattern_count"), Some("1"));
    assert_eq!(result.field("probability_complete"), Some("false"));
    assert_eq!(
        result.field("truncation_reason"),
        Some("observed_universe_truncated")
    );
}

#[test]
fn percent_observed_queue_uses_materialized_patterns_as_build_up_inputs() {
    #[cfg(feature = "native-c-core")]
    let (board_mask, observed_pieces) = (
        0x1c80,
        vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
    );
    #[cfg(not(feature = "native-c-core"))]
    let (board_mask, observed_pieces) = (0x3f0, vec![PieceKind::I, PieceKind::O]);

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, board_mask),
        PcQueueInput::observed(ObservedQueue::new(observed_pieces)),
        PieceWindow::new(4),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert!(result.usize_field("total_pattern_count").unwrap_or(0) >= 1);
    assert_eq!(
        result.field("coverage_source"),
        Some("observed-materialized-pattern-specific")
    );
    assert_eq!(
        result.field("covered_pattern_count_basis"),
        Some("complete_pattern_universe")
    );
    assert_eq!(
        result.field("verified_pattern_count"),
        result.field("materialized_pattern_count")
    );
    if cfg!(feature = "native-c-core") {
        assert!(result.usize_field("covered_pattern_count").unwrap_or(0) >= 1);
        assert!(
            result
                .usize_field("c_buildup_coverage_row_count")
                .unwrap_or(0)
                >= 1
        );
    } else {
        assert_eq!(result.field("covered_pattern_count"), Some("0"));
        assert_eq!(result.field("c_buildup_coverage_row_count"), Some("0"));
        assert_eq!(result.field("probability_complete"), Some("false"));
    }
    assert!(result
        .field("percent_workflow")
        .unwrap_or("")
        .contains("queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows"));
}

#[test]
fn observed_coverage_verifies_all_materialized_patterns_when_complete() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(
        result.field("coverage_source"),
        Some("observed-materialized-pattern-specific")
    );
    assert_eq!(
        result.field("verified_pattern_count"),
        result.field("materialized_pattern_count")
    );
    assert!(result.usize_field("verified_pattern_count").unwrap_or(0) > 1);
    assert_eq!(
        result.field("probability_complete"),
        Some(expected_probability_complete())
    );
}

#[test]
fn percent_not_ranked_by_pattern_zero_only_when_complete_requested() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(
        result.field("coverage_source"),
        Some("observed-materialized-pattern-specific")
    );
    assert!(result.usize_field("verified_pattern_count").unwrap_or(0) > 1);
    assert_eq!(
        result.field("verified_pattern_count"),
        result.field("materialized_pattern_count")
    );
    assert_eq!(
        result.field("covered_pattern_count_basis"),
        Some("complete_pattern_universe")
    );
}

#[test]
fn bag_aligned_pattern_universe_not_collapsed_to_pattern_zero_when_materialized() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::bag_aligned_pattern(
            clearra_supply::queue::bag_aligned_pattern::BagAlignedPattern::new(vec![
                PieceKind::I,
                PieceKind::O,
            ]),
        ),
        PieceWindow::new(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(
        result.field("coverage_source"),
        Some("bag-aligned-single-pattern")
    );
    assert_eq!(result.field("coverage_pattern_count"), Some("1"));
    assert_eq!(result.field("verified_pattern_count"), Some("1"));
    assert_eq!(
        result.field("covered_pattern_count_basis"),
        Some("complete_pattern_universe")
    );
}

#[cfg(not(feature = "native-c-core"))]
#[test]
fn native_unavailable_percent_reports_probability_incomplete() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let result = PercentService::execute(&problem).expect("percent");

    assert_eq!(result.field("covered_pattern_count"), Some("0"));
    assert_eq!(result.field("coverage_probability"), Some("0"));
    assert_eq!(result.field("probability_complete"), Some("false"));
}

fn expected_covered_pattern_count() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "1"
    } else {
        "0"
    }
}

fn expected_probability() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "1"
    } else {
        "0"
    }
}

fn expected_probability_complete() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "true"
    } else {
        "false"
    }
}

fn expected_c_buildup_coverage_row_count() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "1"
    } else {
        "0"
    }
}
