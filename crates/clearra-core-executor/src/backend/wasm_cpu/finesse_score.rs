use clearra_core_domain::board::standard_pc_board::StandardPcBoard;
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_finesse::{
    aggregate_unique_queue_costs, CostedGeometryEdge, CostedGeometryLanguage, FinesseBoard,
    FinesseTarget, FrozenFinesseQuery, GeometryActionKey, GeometryLanguageNode, GeometryNodeId,
    QueueClassProductEvaluator, QueueClassSet, QueueCostAggregation, QueueCostTable,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_problem::{
    BuildProbabilityField, FinessePatternKnowledge, FinessePlacement, FinesseScoreRequest,
    SearchProblem,
};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    CoreExecutionResult, CorePathStep, FinessePolicyResult, FinesseReport, FinesseSolutionAverage,
};

use super::{
    build_probability::{
        finesse_policy_costs, finesse_queue_classes_for_problem,
        fixed_queue_representative_witness, pattern_representative_witness,
        FinesseRepresentativeSelection,
    },
    extended_board::{place_and_clear, ExtendedBoard},
    kick_profiles::builtin_kick_profile,
    WasmExactSearchError,
};

pub(super) fn execute_finesse_score(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    score: &FinesseScoreRequest,
    knowledge: FinessePatternKnowledge,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    ensure_not_cancelled(control)?;
    let (language, path, initial_board_words) =
        fixed_operation_language(problem, field, score, control)?;

    ensure_not_cancelled(control)?;
    let grouping_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseTargetGrouping);
    let classes = queue_classes(problem)?;
    grouping_span.finish(classes.classes().len() as u64);

    ensure_not_cancelled(control)?;
    let product_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseProductDp);
    let evaluator = QueueClassProductEvaluator::new(&language)
        .with_hold_enabled(problem.supply().hold_enabled())
        .with_terminal_hold_release_enabled(problem.supply().projects_unplaced_lookahead());
    let initial_hold = problem
        .supply()
        .hold_enabled()
        .then(|| problem.initial_hold().hold_piece())
        .flatten();
    let fixed_queue = problem.piece_source().fixed_sequence().is_some();
    let oracle = finesse_policy_costs(
        &evaluator,
        "oracle",
        &classes,
        fixed_queue,
        initial_hold,
        control,
        "wasm_finesse_oracle_failed",
    )?;
    ensure_not_cancelled(control)?;
    let visible = if matches!(
        knowledge,
        FinessePatternKnowledge::Both | FinessePatternKnowledge::VisibleSeven
    ) {
        Some(finesse_policy_costs(
            &evaluator,
            "visible-7",
            &classes,
            fixed_queue,
            initial_hold,
            control,
            "wasm_finesse_visible_seven_failed",
        )?)
    } else {
        None
    };
    ensure_not_cancelled(control)?;
    product_span.finish(classes.classes().len() as u64);

    let aggregation_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAggregation);
    let oracle_aggregate = aggregate_unique_queue_costs(&classes, &oracle)
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_aggregate_failed"))?;
    let mut policy_results = Vec::with_capacity(2);
    if matches!(
        knowledge,
        FinessePatternKnowledge::Both | FinessePatternKnowledge::Oracle
    ) {
        policy_results.push(policy_result(
            "oracle",
            &oracle_aggregate,
            "given-operation-sequence",
        ));
    }
    if let Some(visible) = visible.as_ref() {
        let visible_aggregate = aggregate_unique_queue_costs(&classes, visible)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_aggregate_failed"))?;
        let oracle_on_visible = conditional_mean_on_success_set(&oracle, visible);
        let information_penalty = visible_aggregate
            .conditional_mean_inputs
            .zip(oracle_on_visible)
            .map(|(online, oracle)| format_number((online - oracle).max(0.0)));
        let probability_gap = format_number(
            (oracle_aggregate.successful_probability_mass
                - visible_aggregate.successful_probability_mass)
                .max(0.0),
        );
        policy_results.push(
            policy_result("visible-7", &visible_aggregate, "given-operation-sequence")
                .with_comparison(
                    oracle_on_visible.map(format_number),
                    information_penalty,
                    Some(probability_gap),
                ),
        );
    }
    let exact_total_inputs = if fixed_queue && classes.classes().len() == 1 {
        oracle.get(0).flatten().map(|cost| cost.to_string())
    } else {
        None
    };
    let report_complete = classes.metadata().complete;
    let representative_witness = if fixed_queue && classes.classes().len() == 1 {
        let languages = vec![("given-operation-sequence".to_owned(), language.clone())];
        let kick_profile = builtin_kick_profile(problem.kick_profile().profile_id()).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_finesse_kick_profile_unavailable"),
        )?;
        fixed_queue_representative_witness(
            if matches!(
                knowledge,
                FinessePatternKnowledge::Both | FinessePatternKnowledge::Oracle
            ) {
                "oracle"
            } else {
                "visible-7"
            },
            &languages,
            &classes,
            initial_hold,
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            problem.spawn_profile(),
            kick_profile,
            control,
        )?
    } else {
        let policy = if matches!(
            knowledge,
            FinessePatternKnowledge::Both | FinessePatternKnowledge::Oracle
        ) {
            "oracle"
        } else {
            "visible-7"
        };
        let selected_costs = if policy == "oracle" {
            &oracle
        } else {
            visible
                .as_ref()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_visible_seven_missing",
                ))?
        };
        let selection = (0..classes.classes().len())
            .filter_map(|class_index| {
                selected_costs
                    .get(class_index)
                    .flatten()
                    .map(|expected_cost| FinesseRepresentativeSelection {
                        solution_index: 0,
                        class_index,
                        expected_cost,
                    })
            })
            .min_by_key(|selection| (selection.expected_cost, selection.class_index));
        selection
            .map(|selection| {
                let languages = [("given-operation-sequence".to_owned(), language.clone())];
                let kick_profile = builtin_kick_profile(problem.kick_profile().profile_id())
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_kick_profile_unavailable",
                    ))?;
                pattern_representative_witness(
                    policy,
                    selection,
                    &languages,
                    &classes,
                    initial_hold,
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                    problem.spawn_profile(),
                    kick_profile,
                    control,
                )
            })
            .transpose()?
            .flatten()
    };
    if let Some(exact_total_inputs) = exact_total_inputs.as_deref() {
        if representative_witness
            .as_ref()
            .map(|witness| witness.total_inputs().to_string())
            .as_deref()
            != Some(exact_total_inputs)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_exact_witness_cost_mismatch",
            ));
        }
    }
    let report = FinesseReport::new(
        "score",
        knowledge.as_str(),
        report_complete,
        exact_total_inputs,
        policy_results,
    );
    let report = match representative_witness {
        Some(witness) => report.with_representative_witness(witness),
        None => report,
    };
    let any_policy_succeeds = report.policy_results().iter().any(|policy| {
        policy
            .successful_unique_queue_count()
            .is_some_and(|count| count != 0)
    });
    aggregation_span.finish(classes.classes().len() as u64);

    Ok(CoreExecutionResult::new(
        vec![
            ("search_kind".to_owned(), "finesse-score".to_owned()),
            ("objective".to_owned(), "finesse".to_owned()),
            ("finesse_metric_requested".to_owned(), "inputs".to_owned()),
            (
                "finesse_pattern_knowledge_requested".to_owned(),
                knowledge.as_str().to_owned(),
            ),
            (
                "materialized_pattern_count".to_owned(),
                classes.metadata().pattern_count.to_string(),
            ),
            (
                "unique_queue_count".to_owned(),
                classes.metadata().unique_queue_count.to_string(),
            ),
            ("objective_complete".to_owned(), report_complete.to_string()),
            (
                "finesse_initial_board_words".to_owned(),
                board_words_hex(initial_board_words),
            ),
            ("finesse_height".to_owned(), field.height().to_string()),
        ],
        if any_policy_succeeds {
            path
        } else {
            Vec::new()
        },
    )
    .with_finesse_report(report))
}

fn fixed_operation_language(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    score: &FinesseScoreRequest,
    control: &ExecutionControl,
) -> Result<(CostedGeometryLanguage, Vec<CorePathStep>, [u64; 4]), WasmExactSearchError> {
    let kick_profile = builtin_kick_profile(problem.kick_profile().profile_id())
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_kick_profile_unavailable",
        ))?
        .clone();
    let registry = standard_tetromino_registry();
    let placements = normalized_score_placements(field, score)?;
    let (mut board, _, _) = place_and_clear(
        field.width(),
        field.height(),
        ExtendedBoard::from_mask(field.base()),
    );
    let initial_board_words = board.words();
    let geometry_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseGeometry);
    let mut source_boards = Vec::with_capacity(placements.len());
    let mut path = Vec::with_capacity(placements.len());

    for placement in placements.iter().copied() {
        ensure_not_cancelled(control)?;
        let standard_board =
            StandardPcBoard::from_words(field.height(), board.words()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_score_board_invalid")
            })?;
        source_boards.push(FinesseBoard::from_standard_pc(standard_board));

        let shape = registry
            .get(placement.piece())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_piece_definition_missing",
            ))?
            .shape(placement.rotation());
        let mut placement_board = ExtendedBoard::EMPTY;
        for cell in shape.cells() {
            let x = placement.x() + i16::from(cell.x());
            let y = placement.y() + i16::from(cell.y());
            if x < 0 || y < 0 || x >= i16::from(field.width()) || y >= i16::from(field.height()) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_score_placement_outside_field",
                ));
            }
            let cell_index =
                u16::try_from(y).unwrap() * u16::from(field.width()) + u16::try_from(x).unwrap();
            if !placement_board.insert(cell_index) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_score_placement_outside_field",
                ));
            }
        }
        if board.intersects(placement_board) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_score_placement_overlap",
            ));
        }
        let (next_board, _, cleared_lines) =
            place_and_clear(field.width(), field.height(), board.union(placement_board));
        board = next_board;
        path.push(CorePathStep::new(
            placement.piece(),
            placement.rotation().quarter_turns(),
            i32::from(placement.x()),
            i32::from(placement.y()),
            "none",
            cleared_lines,
        ));
    }
    geometry_span.finish(placements.len() as u64);

    ensure_not_cancelled(control)?;
    let movement_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseMovementBfs);
    let mut costs = Vec::with_capacity(placements.len());
    for (source_board, placement) in source_boards
        .iter()
        .copied()
        .zip(placements.iter().copied())
    {
        ensure_not_cancelled(control)?;
        let target = FinesseTarget::new(placement.rotation(), placement.x(), placement.y());
        let query = FrozenFinesseQuery::new(
            source_board,
            placement.piece(),
            problem.spawn_profile(),
            kick_profile.clone(),
            vec![target].into_boxed_slice(),
        );
        costs.push(
            query
                .costs()
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_finesse_movement_search_failed")
                })?
                .get(0)
                .flatten(),
        );
    }
    movement_span.finish(costs.len() as u64);

    ensure_not_cancelled(control)?;
    let annotation_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAnnotationPrune);
    let mut nodes = Vec::with_capacity(placements.len() + 1);
    for (index, ((source_board, placement), cost)) in source_boards
        .into_iter()
        .zip(placements.iter().copied())
        .zip(costs)
        .enumerate()
    {
        let edge = cost.map(|cost| {
            CostedGeometryEdge::new(
                placement.piece(),
                GeometryNodeId::new(index as u32 + 1),
                cost,
                index as u32,
            )
            .with_action_key(GeometryActionKey::new(
                placement.piece(),
                placement.rotation(),
                placement.x(),
                placement.y(),
            ))
        });
        nodes.push(
            GeometryLanguageNode::new(index as u16, false, edge.into_iter().collect::<Vec<_>>())
                .with_source_board(source_board),
        );
    }
    nodes.push(GeometryLanguageNode::new(
        placements.len() as u16,
        true,
        Vec::new(),
    ));
    let language = CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes)
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_score_language_invalid"))?;
    annotation_span.finish(placements.len() as u64);
    Ok((language, path, initial_board_words))
}

fn normalized_score_placements(
    field: BuildProbabilityField,
    score: &FinesseScoreRequest,
) -> Result<Vec<FinessePlacement>, WasmExactSearchError> {
    let cleared_rows = score.initial_cleared_rows();
    if cleared_rows == 0 {
        return Ok(score.placements().to_vec());
    }
    if cleared_rows >> field.height() != 0 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_initial_clear_rows_invalid",
        ));
    }
    let mut placements = score.placements().to_vec();
    let first = placements
        .first_mut()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_score_placements_missing",
        ))?;
    let registry = standard_tetromino_registry();
    let placement = *first;
    let shape = registry
        .get(placement.piece())
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_piece_definition_missing",
        ))?
        .shape(placement.rotation());
    let mut normalized_anchor = None;
    for cell in shape.cells() {
        let source_y = placement.y().checked_add(i16::from(cell.y())).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_finesse_score_placement_outside_field"),
        )?;
        if source_y < 0 || source_y >= i16::from(field.height()) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_score_placement_outside_field",
            ));
        }
        let source_row = u32::try_from(source_y).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_score_placement_outside_field")
        })?;
        if cleared_rows & (1_u32 << source_row) != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_score_placement_overlaps_initial_clear",
            ));
        }
        let rows_below = if source_row == 0 {
            0
        } else {
            (cleared_rows & ((1_u32 << source_row) - 1)).count_ones()
        };
        let normalized_cell_y = source_y
            .checked_sub(i16::try_from(rows_below).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_initial_clear_rows_invalid")
            })?)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_initial_clear_rows_invalid",
            ))?;
        let anchor = normalized_cell_y - i16::from(cell.y());
        if normalized_anchor
            .replace(anchor)
            .is_some_and(|prior| prior != anchor)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_score_placement_split_by_initial_clear",
            ));
        }
    }
    *first = FinessePlacement::new(
        placement.piece(),
        placement.rotation(),
        placement.x(),
        normalized_anchor.unwrap_or(placement.y()),
    );
    Ok(placements)
}

fn ensure_not_cancelled(control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
    if control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

fn queue_classes(problem: &SearchProblem) -> Result<QueueClassSet, WasmExactSearchError> {
    finesse_queue_classes_for_problem(problem, true)
}

fn policy_result(
    policy: &str,
    aggregate: &QueueCostAggregation,
    solution_key: &str,
) -> FinessePolicyResult {
    let average = aggregate
        .conditional_mean_inputs
        .map_or_else(|| "unavailable".to_owned(), format_number);
    FinessePolicyResult::new(
        policy,
        average.clone(),
        aggregate.complete,
        vec![FinesseSolutionAverage::new(
            solution_key,
            average,
            aggregate.complete,
        )],
    )
    .with_success_summary(
        format_number(aggregate.successful_probability_mass),
        aggregate.successful_unique_queue_count,
        aggregate.total_unique_queue_count,
    )
}

fn conditional_mean_on_success_set(
    oracle: &QueueCostTable,
    visible: &QueueCostTable,
) -> Option<f64> {
    let mut total = 0_u64;
    let mut count = 0_usize;
    for index in 0..visible.len() {
        if visible.get(index).flatten().is_some() {
            if let Some(cost) = oracle.get(index).flatten() {
                total = total.saturating_add(u64::from(cost));
                count += 1;
            }
        }
    }
    (count != 0).then_some(total as f64 / count as f64)
}

fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let mut rendered = format!("{value:.6}");
    while rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    rendered
}

fn board_words_hex(words: [u64; 4]) -> String {
    format!(
        "0x{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, FinessePlacement, FinesseScoreRequest,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{board_words_hex, format_number, normalized_score_placements};

    #[test]
    fn finesse_number_format_is_stable_and_trimmed() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(1.0 / 3.0), "0.333333");
    }

    #[test]
    fn board_words_use_canonical_big_endian_hex_text() {
        assert_eq!(
            board_words_hex([1, 2, 3, 4]),
            "0x0000000000000004000000000000000300000000000000020000000000000001"
        );
    }

    #[test]
    fn initial_clear_moves_only_the_first_progressive_document_operation() {
        let base = 0x3ff_u64;
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, base),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::I])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2));
        let field = BuildProbabilityField::from_words_preserving_height(4, [base, 0, 0, 0], [0; 4])
            .unwrap();
        let score = FinesseScoreRequest::new(vec![
            FinessePlacement::new(PieceKind::O, RotationState::Zero, 4, 1),
            FinessePlacement::new(PieceKind::I, RotationState::Zero, 3, 1),
        ])
        .unwrap();
        let query = BuildProbabilityQuery::new(core, field).with_finesse_score(score);

        let placements = normalized_score_placements(
            query.field(),
            query.finesse_score().expect("tagged score request"),
        )
        .unwrap();

        assert_eq!((placements[0].x(), placements[0].y()), (4, 0));
        assert_eq!((placements[1].x(), placements[1].y()), (3, 1));
    }
}
