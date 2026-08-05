use super::*;

fn expected_opening_coverage_row_count() -> usize {
    4
}

fn expected_coverage_probability() -> &'static str {
    "1.0"
}

fn expected_coverage_source() -> &'static str {
    "pattern-specific-exact-buildability"
}

fn expected_opening_count_complete() -> bool {
    true
}

fn expected_setup_coverage_probability() -> &'static str {
    "1.0"
}

fn expected_build_foundation_solution_found() -> bool {
    !cfg!(feature = "native-c-core")
}

fn expected_setup_coverage_row_count(candidate_count: usize) -> usize {
    if cfg!(feature = "native-c-core") {
        candidate_count
    } else {
        0
    }
}

fn expected_build_coverage_row_count(candidate_count: usize) -> usize {
    if cfg!(feature = "native-c-core") {
        candidate_count
    } else {
        0
    }
}

#[cfg(feature = "native-c-core")]
mod case_coverage_request_preserves_the_count_unique_fast_path_boundary {
    use super::*;

    #[test]
    fn coverage_request_preserves_the_count_unique_fast_path_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let default = BuildUpRunner::run(&problem, &packing).expect("default buildup");
        let coverage =
            BuildUpRunner::run_for_coverage(&problem, &packing).expect("coverage buildup");

        assert_eq!(default.execution_mode(), BuildUpExecutionMode::VerifyFirst);
        assert_eq!(default.coverage_row_count(), 0);
        assert!(!default.objective_complete());
        assert_eq!(
            coverage.execution_mode(),
            BuildUpExecutionMode::EnumerateVariants
        );
        assert_eq!(coverage.coverage_row_count(), 1);
        assert!(coverage.objective_complete());
        assert_eq!(coverage.covered_pattern_count(), 1);
    }
}

#[cfg(feature = "native-c-core")]
mod case_buildup_runner_promotes_packing_candidates_to_coverage_and_objectives {
    use super::*;

    #[test]
    fn buildup_runner_promotes_packing_candidates_to_coverage_and_objectives() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");

        assert!(buildup.solution_found());
        assert_eq!(buildup.candidate_result_count(), packing.candidate_count());
        assert_eq!(
            buildup.coverage_row_count(),
            expected_opening_coverage_row_count()
        );
        assert_eq!(
            buildup.coverage_probability(),
            expected_coverage_probability()
        );
        assert_eq!(
            buildup.execution_mode(),
            BuildUpExecutionMode::EnumerateVariants
        );
        assert_eq!(buildup.coverage_source(), expected_coverage_source());
        assert_eq!(
            buildup.execution_variants().len(),
            buildup.build_variant_count()
        );
        assert!(buildup.unique_trace_count() <= buildup.build_variant_count());
        assert!(buildup.pattern_verified_execution_count() >= buildup.build_variant_count());
        if cfg!(feature = "native-c-core") {
            assert!(
                buildup
                    .objective_result()
                    .expect("objective")
                    .total_solution_count()
                    > 0
            );
        } else {
            assert!(buildup.objective_result().is_none());
        }
        assert_eq!(buildup.count_complete(), expected_opening_count_complete());
    }
}

#[cfg(feature = "native-c-core")]

mod case_setup_preset_promotes_packing_candidates_to_buildup_variants {
    use super::*;

    #[test]
    fn setup_preset_promotes_packing_candidates_to_buildup_variants() {
        let query = clearra_problem::query::SetupSearchQuery::default()
            .with_queue(clearra_problem::query::SetupQueueInput::fixed_sequence(
                FixedSequence::new(vec![
                    PieceKind::I,
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::I,
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::O,
                ]),
            ))
            .with_piece_budget(
                clearra_problem::query::PieceBudget::new(vec![PieceKind::I, PieceKind::O], 10)
                    .expect("piece budget"),
            );
        let problem = ProblemCompiler::compile_setup(&query).expect("setup problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");

        assert_eq!(buildup.candidate_result_count(), packing.candidate_count());
        let retained_limit = problem.trace_policy().retained_trace_limit();
        assert!(packing.candidate_count() > retained_limit);
        assert_eq!(buildup.build_variant_count(), retained_limit);
        assert_eq!(buildup.retained_trace_count(), retained_limit);
        assert!(buildup.trace_retention_truncated());
        assert_eq!(buildup.trace_retention_reason(), "retained_trace_limit");
        assert!(buildup.count_complete());
        assert_eq!(buildup.count_truncated_reason(), "none");
        assert_eq!(
            buildup.coverage_row_count(),
            expected_setup_coverage_row_count(packing.candidate_count())
        );
        assert_eq!(
            buildup.coverage_probability(),
            expected_setup_coverage_probability()
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_build_preset_generates_c_coverage_rows_for_build_coverage {
    use super::*;

    #[test]
    fn build_preset_generates_c_coverage_rows_for_build_coverage() {
        let query = clearra_problem::query::BuildQuery::coverage_bridge(
            clearra_problem::query::BuildTemplateBridge::new(
                "template-a",
                clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
                2,
            ),
            8,
            clearra_problem::query::BuildProblemLimits::new(12, 8),
        );
        let problem = ProblemCompiler::compile_build(&query).expect("build problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");

        assert_eq!(
            buildup.solution_found(),
            expected_build_foundation_solution_found()
        );
        assert_eq!(buildup.candidate_result_count(), packing.candidate_count());
        assert_eq!(
            buildup.coverage_row_count(),
            expected_build_coverage_row_count(packing.candidate_count())
        );
        if expected_build_foundation_solution_found() && buildup.coverage_row_count() > 0 {
            assert_eq!(buildup.coverage_rows()[0].pattern_count(), 8);
        }
        assert_eq!(buildup.build_variant_count(), packing.candidate_count());
    }
}

mod case_buildup_enumeration_retains_one_pattern_witness {
    use super::*;

    #[test]
    fn buildup_enumeration_retains_one_pattern_witness() {
        let query = clearra_problem::query::BuildQuery::coverage_bridge(
            clearra_problem::query::BuildTemplateBridge::new(
                "template-a",
                clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
                2,
            ),
            8,
            clearra_problem::query::BuildProblemLimits::new(12, 8),
        );
        let problem = ProblemCompiler::compile_build(&query).expect("build problem");
        let limits = buildup_enumeration_limits(&problem);

        assert_eq!(limits.max_variants, 1);
        assert_eq!(limits.preserve_hold_branches, 1);
    }
}
