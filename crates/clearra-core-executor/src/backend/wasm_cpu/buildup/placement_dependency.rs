//! Candidate-local placement dependency proof and exact supply composition.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use super::super::{
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometryCandidate, TargetGroup},
    WasmExactSearchError,
};
use super::{certified_language_result, BuildUpWorkspace, CandidateBuildResult};
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

#[cfg(feature = "placement-dependency-pc")]
mod witness;

#[cfg(feature = "placement-dependency-pc")]
mod fresh_bag_observation;

/// The request plan certifies the precise fresh-bag/empty-Hold theorem
/// domain once. This path proves only existential candidate membership,
/// compiles the observed suffix roots, and produces no oracle coverage row.
#[cfg(feature = "placement-dependency-pc")]
pub(super) fn produce_board_observation(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<super::ObservationProducerDecision, WasmExactSearchError> {
    use super::super::piece_order_language::PrefixSuffixCompilation;
    use super::super::symbolic_prefix_orders::{with_product_prefix_order, SymbolicPrefixOrders};
    use super::ObservationProducerDecision;

    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    if !supports_candidate(catalog, candidate) || !(7..=11).contains(&candidate.row_ids().len()) {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_observation_source_candidate_out_of_scope",
        ));
    }
    workspace.observation_source_existence[0] += 1;
    if !fresh_bag_observation::counts_fit(
        candidate
            .row_ids()
            .iter()
            .map(|&row| catalog.skeleton(row).piece),
    ) {
        workspace.observation_source_existence[1] += 1;
        return Ok(ObservationProducerDecision::Rejected);
    }
    let span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::PlacementDependencyPc, profile_scale);
    let result = with_product_prefix_order(|work| {
        work.candidates_attempted += 1;
        let orders = SymbolicPrefixOrders::compile_product_completion_with_workspace(
            catalog,
            candidate,
            problem.kick_profile().profile_id(),
            control,
            &mut workspace.reachability,
            work,
            profile_scale,
            0,
        )
        .map_err(|reason| map_prefix_error(PrefixEvaluationError::Refused(reason)))?;
        let witness =
            fresh_bag_observation::has_fresh_bag_witness(orders.observation_relation()?, control)?;
        workspace.observation_source_existence[4] =
            workspace.observation_source_existence[4].saturating_add(witness.checked_prefixes);
        if !witness.feasible {
            workspace.observation_source_existence[2] += 1;
            return Ok(ObservationProducerDecision::Rejected);
        }
        #[cfg(feature = "parallel")]
        let compiled = if let Some(shared) = &workspace.shared_observation_producer {
            workspace
                .dependency_observation
                .produce_shared_board_relation(
                    orders,
                    &mut workspace.piece_order_languages,
                    work,
                    shared,
                    &mut workspace.shared_observation_import,
                    control,
                    profile_scale,
                )
        } else {
            workspace.dependency_observation.produce_board_relation(
                orders,
                &mut workspace.piece_order_languages,
                work,
                control,
                profile_scale,
            )
        };
        #[cfg(not(feature = "parallel"))]
        let compiled = workspace.dependency_observation.produce_board_relation(
            orders,
            &mut workspace.piece_order_languages,
            work,
            control,
            profile_scale,
        );
        workspace.observation_source_existence[5] =
            workspace.observation_source_existence[5].saturating_add(work.language_transitions);
        if compiled? != PrefixSuffixCompilation::Compiled {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_witness_language_missing",
            ));
        }
        workspace.observation_source_existence[3] += 1;
        Ok(ObservationProducerDecision::Registered)
    });
    span.finish(u64::from(result.is_ok()));
    result
}

#[cfg(feature = "placement-dependency-pc")]
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_product(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    retain_trace: bool,
    supply_first: bool,
    expected_final_board: u64,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    use super::super::symbolic_prefix_orders::with_product_prefix_order;

    #[cfg(test)]
    super::record_product_verifier(true);
    let span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::PlacementDependencyPc, profile_scale);
    let result = with_product_prefix_order(|work| {
        evaluate_prefix_order(
            problem,
            catalog,
            candidate,
            target,
            workspace,
            evaluator,
            control,
            profile_scale,
            work,
            PrefixEvaluation::Product {
                retain_trace,
                supply_first,
                expected_final_board,
            },
        )
    });
    span.finish(u64::from(result.is_ok()));
    // A bounded refusal is neither a negative result nor permission to
    // start the unrequested BuildUp verifier.
    result.map_err(map_prefix_error)
}

#[cfg(feature = "placement-dependency-pc")]
pub(super) fn supports_candidate(catalog: &GeometryCatalog, candidate: &GeometryCandidate) -> bool {
    catalog.width() == 10
        && (1..=6).contains(&catalog.height())
        && (1..=11).contains(&candidate.row_ids().len())
}

#[cfg(any(
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
pub(super) fn verify_strict(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    use super::super::symbolic_prefix_orders::record_strict_prefix_work;

    record_strict_prefix_work(|work| {
        evaluate_prefix_order(
            problem,
            catalog,
            candidate,
            target,
            workspace,
            evaluator,
            control,
            profile_scale,
            work,
            PrefixEvaluation::NativeExperiment,
        )
    })
    .map_err(map_prefix_error)
}

fn map_prefix_error(error: PrefixEvaluationError) -> WasmExactSearchError {
    use super::super::symbolic_prefix_orders::SymbolicPrefixRefusal;
    match error {
        PrefixEvaluationError::UnsupportedSupply => {
            WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_unsupported_supply")
        }
        PrefixEvaluationError::Execution(error) => error,
        PrefixEvaluationError::Refused(reason) => match reason {
            SymbolicPrefixRefusal::Cancelled => WasmExactSearchError::Cancelled,
            SymbolicPrefixRefusal::Unsupported => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_unsupported_candidate")
            }
            SymbolicPrefixRefusal::RelationLimit => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_relation_limit")
            }
            SymbolicPrefixRefusal::InconsistentProjection => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_inconsistent_projection")
            }
        },
    }
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[derive(Clone, Copy)]
enum PrefixEvaluation {
    #[cfg(feature = "placement-dependency-pc")]
    Product {
        retain_trace: bool,
        supply_first: bool,
        expected_final_board: u64,
    },
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    NativeExperiment,
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
enum PrefixEvaluationError {
    UnsupportedSupply,
    Refused(super::super::symbolic_prefix_orders::SymbolicPrefixRefusal),
    Execution(WasmExactSearchError),
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[allow(clippy::too_many_arguments)]
fn evaluate_prefix_order(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    _target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    _evaluator: &mut CoverageProductEvaluator,
    control: &ExecutionControl,
    profile_scale: u64,
    work: &mut super::super::symbolic_prefix_orders::SymbolicPrefixWork,
    evaluation: PrefixEvaluation,
) -> Result<CandidateBuildResult, PrefixEvaluationError> {
    use super::super::symbolic_prefix_orders::SymbolicPrefixOrders;

    let _universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or(PrefixEvaluationError::UnsupportedSupply)?;
    let supported = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product { .. } => true,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => {
            super::super::standard_bag_coverage::StandardBagCoverage::supports(
                _universe,
                problem.initial_hold(),
            )
        }
    };
    if !supported {
        return Err(PrefixEvaluationError::UnsupportedSupply);
    }
    work.candidates_attempted += 1;
    let orders = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product {
            expected_final_board,
            ..
        } => SymbolicPrefixOrders::compile_product_completion_with_workspace(
            catalog,
            candidate,
            problem.kick_profile().profile_id(),
            control,
            &mut workspace.reachability,
            work,
            profile_scale,
            expected_final_board,
        ),
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => SymbolicPrefixOrders::compile_with_workspace(
            catalog,
            candidate,
            problem.kick_profile().profile_id(),
            control,
            &mut workspace.reachability,
            work,
            profile_scale,
        ),
    }
    .map_err(PrefixEvaluationError::Refused)?;
    let compile_language = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product { .. } => SymbolicPrefixOrders::compile_product_language,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => SymbolicPrefixOrders::compile_language,
    };
    let language_stage = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product { .. } => ExecutorSearchStage::PlacementDependencyPcLanguage,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => ExecutorSearchStage::StrictPrefixLanguage,
    };
    let language_span = SearchStageSpan::begin_scaled(language_stage, profile_scale);
    #[cfg(feature = "placement-dependency-pc")]
    let observation_requested = matches!(evaluation, PrefixEvaluation::Product { .. })
        && problem
            .queue_observation_policy()
            .requires_observation_policy();
    #[cfg(feature = "placement-dependency-pc")]
    let language = if observation_requested {
        workspace.dependency_observation.compile_relation_language(
            &orders,
            &mut workspace.piece_order_languages,
            work,
            control,
        )
    } else {
        compile_language(&orders, &mut workspace.piece_order_languages, work)
    }
    .map_err(PrefixEvaluationError::Execution)?;
    #[cfg(not(feature = "placement-dependency-pc"))]
    let language = compile_language(&orders, &mut workspace.piece_order_languages, work)
        .map_err(PrefixEvaluationError::Execution)?;
    language_span.finish(1);
    let result = match language {
        Some(root) => {
            let product_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::WasmCoverageLanguageProduct,
                profile_scale,
            );
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            let bag_product_span =
                super::super::standard_bag_coverage::StandardBagCoverage::supports(
                    _universe,
                    problem.initial_hold(),
                )
                .then(|| {
                    SearchStageSpan::begin_scaled(
                        ExecutorSearchStage::StrictStandardBagProduct,
                        profile_scale,
                    )
                });
            let result = match evaluation {
                #[cfg(feature = "placement-dependency-pc")]
                PrefixEvaluation::Product { .. } => workspace
                    .cover_piece_language(problem, _target, root, _evaluator, control)
                    .map_err(PrefixEvaluationError::Execution)?,
                #[cfg(any(
                    test,
                    all(
                        feature = "local-search-ab",
                        feature = "parallel",
                        not(target_family = "wasm")
                    )
                ))]
                PrefixEvaluation::NativeExperiment => certified_language_result(Some(
                    workspace
                        .cover_standard_bag_language(problem, root, control)
                        .map_err(PrefixEvaluationError::Execution)?
                        .ok_or(PrefixEvaluationError::UnsupportedSupply)?,
                )),
            };
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            if let Some(span) = bag_product_span {
                span.finish(result.coverage_product_edge_checks as u64);
            }
            product_span.finish(result.coverage_product_edge_checks as u64);
            result
        }
        None => certified_language_result(None),
    };
    if control.is_cancelled() {
        return Err(PrefixEvaluationError::Execution(
            WasmExactSearchError::Cancelled,
        ));
    }
    work.candidates_certified += 1;
    #[cfg(feature = "placement-dependency-pc")]
    let result = match evaluation {
        PrefixEvaluation::Product {
            retain_trace: true,
            supply_first,
            ..
        } if result.buildable => {
            let mut result = result;
            let pattern_id = result
                .witness_pattern_id
                .ok_or(PrefixEvaluationError::Execution(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_placement_dependency_witness_pattern_missing",
                    ),
                ))?;
            result.representative_path = witness::materialize(
                problem,
                catalog,
                candidate,
                &orders,
                pattern_id,
                supply_first,
                &mut workspace.reachability,
                control,
                &mut work.pose,
            )
            .map_err(PrefixEvaluationError::Execution)?;
            result
        }
        _ => result,
    };
    #[cfg(feature = "placement-dependency-pc")]
    if observation_requested && result.buildable {
        // Each nonterminal lock consumes at least one source token; the
        // terminal held release cannot lead to another decision. Retain
        // exact suffix IDs through the last possibly hidden lock depth.
        // Earlier inference from the *original* source is owned by the
        // observation evaluator, never by this successful-candidate filter.
        let source_len = problem.supply().source_sequence_length();
        let projected_last =
            problem.supply().projects_standard_bag_lookahead() && source_len % 7 == 6;
        let visible = problem
            .queue_observation_policy()
            .visible_piece_count()
            .ok_or(PrefixEvaluationError::UnsupportedSupply)?;
        let hidden = source_len
            .saturating_add(usize::from(projected_last))
            .saturating_sub(usize::from(problem.initial_hold().cursor()))
            .saturating_sub(usize::from(visible));
        let hidden = u8::try_from(hidden).map_err(|_| {
            PrefixEvaluationError::Execution(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch",
            ))
        })?;
        let reveal_floor = original_standard_bag_reveal_floor(
            _universe,
            source_len,
            projected_last,
            candidate.row_ids().len(),
            usize::from(visible),
            usize::from(problem.initial_hold().cursor()),
            problem.supply().hold_enabled() && problem.initial_hold().hold_piece().is_none(),
        );
        workspace
            .dependency_observation
            .register_relation_with_reveal_floor(orders, hidden, reveal_floor, control)
            .map_err(PrefixEvaluationError::Execution)?;
    }
    Ok(result)
}

/// Prove a lower bound on the first possible reveal using only the complete
/// original fresh-bag source. Actual storage and weights provide authority;
/// a descriptive queue label, a matching sample or candidate-local successes
/// cannot do so. Checked cardinality also covers the 12 draws of 11-lock hold
/// requests without depending on the narrower compact ordinal codec.
#[cfg(feature = "placement-dependency-pc")]
pub(super) fn original_standard_bag_reveal_floor(
    universe: &clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse,
    source_len: usize,
    projected_last: bool,
    operation_count: usize,
    visible: usize,
    initial_cursor: usize,
    empty_hold_extra: bool,
) -> u8 {
    let Some(sequence_len) = complete_fresh_bag_sequence_len(universe) else {
        return 0;
    };
    if sequence_len == 0 || sequence_len != source_len || (projected_last && sequence_len % 7 != 6)
    {
        return 0;
    }
    let Some(effective_len) = source_len.checked_add(usize::from(projected_last)) else {
        return 0;
    };
    let Some(max_initial_view) = initial_cursor
        .checked_add(visible)
        .and_then(|depth| depth.checked_add(usize::from(empty_hold_extra)))
    else {
        return 0;
    };
    if initial_cursor > effective_len {
        return 0;
    }

    // A fresh bag's final piece is determined after its other six pieces.
    // A partial final bag still has at least two possible last draws until
    // that draw is seen. No earlier original information set is determined.
    let determined_depth = effective_len - usize::from(effective_len % 7 == 0);
    // After d locks, the view cannot exceed c0 + W + d + E. The separate
    // relevant-draw certificate cannot hold while n-d > W. Treating E as
    // available even at d=0 only weakens this lower bound, so root inference
    // and every hold path remain covered. The owner always keeps prefix 0.
    let floor = operation_count
        .saturating_sub(visible)
        .min(determined_depth.saturating_sub(max_initial_view));
    u8::try_from(floor).unwrap_or(0)
}

#[cfg(feature = "placement-dependency-pc")]
pub(super) fn complete_fresh_bag_sequence_len(
    universe: &clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse,
) -> Option<usize> {
    use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;

    if !universe.complete() || universe.truncation_reason().is_some() {
        return None;
    }
    let UniformCompactPatternSource::Standard7Bag {
        sequence_len,
        pattern_count,
    } = universe.uniform_compact_source()?
    else {
        return None;
    };
    let complete_count = (0..sequence_len).try_fold(1_u128, |count, draw| {
        count.checked_mul((7 - draw % 7) as u128)
    })?;
    (sequence_len > 0
        && pattern_count == universe.pattern_count()
        && complete_count == pattern_count as u128
        && complete_count == universe.total_possible_pattern_count())
    .then_some(sequence_len)
}

#[cfg(all(test, feature = "placement-dependency-pc"))]
mod observation_reveal_floor_tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::{
        pattern_universe::pattern_universe_materializer::PatternUniverseMaterializer,
        queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    };

    use super::original_standard_bag_reveal_floor;

    #[test]
    fn complete_fresh_bag_floor_covers_eleven_and_twelve_draw_sources() {
        for (draws, operations, floor) in [(11, 10, 3), (12, 11, 4)] {
            let universe = PatternUniverseMaterializer::standard_7_bag(draws, 0, 1).unwrap();
            if draws == 12 {
                assert!(universe.complete_standard_bag_ordinals().is_none());
            }
            assert_eq!(
                original_standard_bag_reveal_floor(
                    &universe,
                    usize::from(draws),
                    false,
                    operations,
                    7,
                    0,
                    true,
                ),
                floor,
            );
            assert_eq!(
                original_standard_bag_reveal_floor(
                    &universe,
                    usize::from(draws) + 1,
                    false,
                    operations,
                    7,
                    0,
                    true,
                ),
                0,
                "a different provenance length supplies no certificate",
            );
        }

        let complete_bag = PatternUniverseMaterializer::standard_7_bag(7, 0, 1).unwrap();
        assert_eq!(
            original_standard_bag_reveal_floor(&complete_bag, 7, false, 7, 1, 0, false),
            5,
            "the last full-bag piece is inferred after the sixth is observed",
        );
        let projected_bag = PatternUniverseMaterializer::standard_7_bag(6, 0, 1).unwrap();
        assert_eq!(
            original_standard_bag_reveal_floor(&projected_bag, 6, true, 6, 1, 0, false),
            5,
        );
    }

    #[test]
    fn narrowed_and_generic_sources_keep_all_suffix_depths() {
        let narrowed = PatternUniverseMaterializer::standard_7_bag(12, 2, 1).unwrap();
        let explicit = PatternUniverseMaterializer::fixed_sequence(
            &FixedSequence::new(vec![PieceKind::I; 12]),
            1,
        );
        let expression = QueuePatternExpression::parse("P7P5", 12_700_800).unwrap();
        let factorized =
            PatternUniverseMaterializer::queue_pattern_expression(&expression, 1).unwrap();
        for universe in [&narrowed, &explicit, &factorized] {
            assert_eq!(
                original_standard_bag_reveal_floor(universe, 12, false, 11, 7, 0, true),
                0,
            );
        }
    }
}
