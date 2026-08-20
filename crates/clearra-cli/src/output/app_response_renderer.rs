// SRP rationale: this module has one behavior-level change reason: rendering typed application responses into the stable CLI output contract.

use clearra_app::{AppErrorCode, AppRenderModel, AppResponse, AppStatus};
use clearra_spin_structure_search::{SpinStructureOutcome, SpinStructureQuery, StructureOperation};

use crate::{
    error::CliErrorCode,
    output::{
        CliOutput, CommandRenderer, RenderField, RenderFieldValue, RenderFormat,
        SummaryRenderContract,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppResponseRenderer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SolutionDataStatus {
    NotRequested,
    Unavailable,
    Partial,
    Complete,
}

impl SolutionDataStatus {
    fn for_request(requested: bool, materialized: bool, complete: bool) -> Self {
        if !requested {
            Self::NotRequested
        } else if !materialized {
            Self::Unavailable
        } else if complete {
            Self::Complete
        } else {
            Self::Partial
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not-requested",
            Self::Unavailable => "unavailable",
            Self::Partial => "partial",
            Self::Complete => "complete",
        }
    }

    const fn exposes_artifacts(self) -> bool {
        matches!(self, Self::Partial | Self::Complete)
    }

    const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Unavailable => Some("solution-set-not-materialized"),
            Self::Partial => Some("solution-set-incomplete"),
            Self::NotRequested | Self::Complete => None,
        }
    }
}

impl AppResponseRenderer {
    pub fn render(
        response: AppResponse,
        format: RenderFormat,
        default_error: CliErrorCode,
    ) -> CliOutput {
        Self::render_with_solution_data(response, format, default_error, false)
    }

    pub fn render_with_solution_data(
        response: AppResponse,
        format: RenderFormat,
        default_error: CliErrorCode,
        include_solution_data: bool,
    ) -> CliOutput {
        match response.status() {
            AppStatus::Success => {
                render_success(response, format, default_error, include_solution_data)
            }
            AppStatus::ValidationFailed => CliOutput::validation_failed_with_format(
                response.diagnostics().validation(),
                format,
            ),
            AppStatus::Unsupported | AppStatus::ExecutionFailed => {
                let Some(error) = response.error() else {
                    return CliOutput::error(default_error, "app execution failed");
                };
                CliOutput::error(
                    cli_error_for_app_error(error.code(), default_error),
                    error.message(),
                )
            }
        }
    }
}

fn render_success(
    response: AppResponse,
    format: RenderFormat,
    default_error: CliErrorCode,
    include_solution_data: bool,
) -> CliOutput {
    let Some(model) = response.render_model() else {
        return CliOutput::error(default_error, "app response did not include a render model");
    };
    match model {
        AppRenderModel::Pc(result)
        | AppRenderModel::BuildProbability(result)
        | AppRenderModel::Scenario(result)
        | AppRenderModel::Cover(result)
        | AppRenderModel::Percent(result) => {
            let mut fields =
                SummaryRenderContract::render_fields(result.fail_closed_solution_summary_fields());
            let solution_availability = result.execution_report().solution_set_availability();
            let finesse_score_exception = result
                .finesse_report()
                .is_some_and(|report| report.mode() == "score");
            let solution_contract_valid = solution_availability.contract_valid()
                && solution_availability
                    .materialized_key_count_matches(result.normalized_solution_keys().len());
            let solution_set_materialized =
                solution_contract_valid && solution_availability.solution_set_materialized();
            let solution_keys_complete =
                solution_contract_valid && solution_availability.solution_keys_complete();
            let solution_data_available = solution_set_materialized || finesse_score_exception;
            let solution_data_complete = if solution_set_materialized {
                solution_keys_complete
            } else {
                result
                    .finesse_report()
                    .is_some_and(|report| report.mode() == "score" && report.complete())
            };
            let solution_data_status = SolutionDataStatus::for_request(
                include_solution_data,
                solution_data_available,
                solution_data_complete,
            );
            append_solution_data_contract(&mut fields, solution_data_status, format);
            if solution_set_materialized && !result.solution_probabilities().is_empty() {
                fields.push(RenderField::new(
                    "solution_probabilities",
                    RenderFieldValue::array(result.solution_probabilities().iter().map(|entry| {
                        RenderFieldValue::object([
                            (
                                "solution_key",
                                RenderFieldValue::string(entry.solution_key()),
                            ),
                            ("probability", RenderFieldValue::number(entry.probability())),
                            (
                                "covered_pattern_count",
                                RenderFieldValue::from(entry.covered_pattern_count()),
                            ),
                            (
                                "pattern_count",
                                RenderFieldValue::from(entry.pattern_count()),
                            ),
                            (
                                "probability_complete",
                                RenderFieldValue::bool(entry.probability_complete()),
                            ),
                        ])
                    })),
                ));
            }
            if let Some(report) = result.finesse_report().filter(|report| {
                solution_set_materialized || finesse_score_exception && report.mode() == "score"
            }) {
                fields.push(RenderField::new(
                    "finesse_report",
                    finesse_report_value(report),
                ));
            }
            if solution_data_status.exposes_artifacts() {
                fields.extend([RenderField::new(
                    "solution_keys",
                    RenderFieldValue::array(
                        result
                            .normalized_solution_keys()
                            .iter()
                            .map(RenderFieldValue::string),
                    ),
                )]);
                if result
                    .finesse_report()
                    .is_some_and(|report| report.mode() == "score")
                {
                    fields.push(RenderField::new(
                        "finesse_score_data",
                        RenderFieldValue::object([
                            (
                                "initial_board",
                                RenderFieldValue::string(
                                    result
                                        .field("finesse_initial_board_words")
                                        .unwrap_or("0x0000000000000000000000000000000000000000000000000000000000000000"),
                                ),
                            ),
                            (
                                "height",
                                result
                                    .field("finesse_height")
                                    .and_then(|value| value.parse::<u8>().ok())
                                    .map_or(RenderFieldValue::Null, RenderFieldValue::from),
                            ),
                            (
                                "representative_path",
                                RenderFieldValue::array(result.path_steps().iter().map(|step| {
                                    RenderFieldValue::object([
                                        (
                                            "piece",
                                            RenderFieldValue::string(
                                                step.piece().as_ascii().to_string(),
                                            ),
                                        ),
                                        ("rotation", RenderFieldValue::from(step.rotation())),
                                        ("x", RenderFieldValue::from(step.x())),
                                        ("y", RenderFieldValue::from(step.y())),
                                        (
                                            "cleared_lines",
                                            RenderFieldValue::from(step.cleared_lines()),
                                        ),
                                    ])
                                })),
                            ),
                        ]),
                    ));
                }
            }
            CommandRenderer::render_output(model.kind().as_str(), fields, format)
        }
        AppRenderModel::Setup(result) => {
            let mut fields = SummaryRenderContract::render_fields(result.summary_fields());
            let Some(report) = result.setup_finder_report() else {
                return CliOutput::error(
                    default_error,
                    "setup result did not include a setup finder report",
                );
            };
            fields.extend([
                RenderField::new("search_mode", report.search_mode().keyword()),
                RenderField::new("cycle", report.cycle()),
                RenderField::new("remaining_pieces", report.remaining_pieces()),
                RenderField::new(
                    "post_cycle_borrow_enabled",
                    report.post_cycle_borrow_enabled(),
                ),
                RenderField::new("coverage_semantics", report.coverage_semantics()),
                RenderField::new(
                    "geometry_family_count",
                    RenderFieldValue::string(report.geometry_family_count()),
                ),
                RenderField::new(
                    "partial_build_node_count",
                    report.partial_build_node_count(),
                ),
                RenderField::new("complete", report.complete()),
                RenderField::new(
                    "hold_conditions",
                    RenderFieldValue::array(report.hold_conditions().iter().map(|condition| {
                        RenderFieldValue::object([
                            (
                                "condition_id",
                                RenderFieldValue::string(condition.condition_id()),
                            ),
                            (
                                "initial_hold",
                                condition
                                    .initial_hold()
                                    .map_or(RenderFieldValue::Null, |piece| {
                                        RenderFieldValue::string(piece.as_ascii().to_string())
                                    }),
                            ),
                            (
                                "pattern_expression",
                                RenderFieldValue::string(condition.pattern_expression()),
                            ),
                            (
                                "pattern_count",
                                RenderFieldValue::from(condition.pattern_count()),
                            ),
                            (
                                "candidate_count",
                                RenderFieldValue::from(condition.candidate_count()),
                            ),
                            (
                                "result_truncated",
                                RenderFieldValue::bool(condition.result_truncated()),
                            ),
                            ("complete", RenderFieldValue::bool(condition.complete())),
                            (
                                "candidates",
                                RenderFieldValue::array(condition.candidates().iter().map(
                                    |candidate| {
                                        RenderFieldValue::object([
                                            (
                                                "setup_id",
                                                RenderFieldValue::string(candidate.setup_id()),
                                            ),
                                            (
                                                "board_mask",
                                                RenderFieldValue::string(format!(
                                                    "0x{:x}",
                                                    candidate.board_mask()
                                                )),
                                            ),
                                            (
                                                "min_locks",
                                                RenderFieldValue::from(candidate.min_locks()),
                                            ),
                                            (
                                                "max_locks",
                                                RenderFieldValue::from(candidate.max_locks()),
                                            ),
                                            (
                                                "build_covered_patterns",
                                                RenderFieldValue::from(
                                                    candidate.build_covered_patterns(),
                                                ),
                                            ),
                                            (
                                                "joint_covered_patterns",
                                                RenderFieldValue::from(
                                                    candidate.joint_covered_patterns(),
                                                ),
                                            ),
                                            (
                                                "build_probability",
                                                RenderFieldValue::number(
                                                    candidate.build_probability(),
                                                ),
                                            ),
                                            (
                                                "joint_probability",
                                                RenderFieldValue::number(
                                                    candidate.joint_probability(),
                                                ),
                                            ),
                                            (
                                                "conditional_pc_probability",
                                                RenderFieldValue::number(
                                                    candidate.conditional_pc_probability(),
                                                ),
                                            ),
                                            (
                                                "representative_path",
                                                RenderFieldValue::array(
                                                    candidate.representative_path().iter().map(
                                                        |step| {
                                                            RenderFieldValue::object([
                                                                (
                                                                    "piece",
                                                                    RenderFieldValue::string(
                                                                        step.piece()
                                                                            .as_ascii()
                                                                            .to_string(),
                                                                    ),
                                                                ),
                                                                (
                                                                    "rotation",
                                                                    RenderFieldValue::from(
                                                                        step.rotation(),
                                                                    ),
                                                                ),
                                                                (
                                                                    "x",
                                                                    RenderFieldValue::from(
                                                                        step.x(),
                                                                    ),
                                                                ),
                                                                (
                                                                    "y",
                                                                    RenderFieldValue::from(
                                                                        step.y(),
                                                                    ),
                                                                ),
                                                                (
                                                                    "hold",
                                                                    RenderFieldValue::string(
                                                                        step.hold(),
                                                                    ),
                                                                ),
                                                                (
                                                                    "cleared_lines",
                                                                    RenderFieldValue::from(
                                                                        step.cleared_lines(),
                                                                    ),
                                                                ),
                                                            ])
                                                        },
                                                    ),
                                                ),
                                            ),
                                        ])
                                    },
                                )),
                            ),
                        ])
                    })),
                ),
            ]);
            append_solution_data_contract(
                &mut fields,
                SolutionDataStatus::for_request(include_solution_data, true, report.complete()),
                format,
            );
            CommandRenderer::render_output(model.kind().as_str(), fields, format)
        }
        AppRenderModel::Damage(result) | AppRenderModel::SpinFinder(result) => {
            let outcomes = RenderFieldValue::array(result.outcomes().iter().map(|outcome| {
                RenderFieldValue::object([
                    ("id", RenderFieldValue::number(outcome.id().to_string())),
                    (
                        "source_pattern_index",
                        RenderFieldValue::number(outcome.source_pattern_index().to_string()),
                    ),
                    (
                        "source_queue",
                        RenderFieldValue::string(
                            outcome
                                .source_queue()
                                .iter()
                                .map(|piece| piece.as_ascii())
                                .collect::<String>(),
                        ),
                    ),
                    (
                        "group",
                        outcome.group().map_or(RenderFieldValue::Null, |group| {
                            RenderFieldValue::string(group.as_str())
                        }),
                    ),
                    (
                        "spin_piece",
                        outcome
                            .spin_piece()
                            .map_or(RenderFieldValue::Null, |piece| {
                                RenderFieldValue::string(piece.as_ascii().to_string())
                            }),
                    ),
                    ("spin_mini", RenderFieldValue::bool(outcome.spin_mini())),
                    (
                        "spin_lines",
                        RenderFieldValue::number(outcome.spin_lines().to_string()),
                    ),
                    (
                        "total_damage",
                        RenderFieldValue::number(outcome.total_damage().to_string()),
                    ),
                    (
                        "path",
                        RenderFieldValue::array(outcome.path().iter().map(|step| {
                            RenderFieldValue::object([
                                (
                                    "piece",
                                    RenderFieldValue::string(step.piece().as_ascii().to_string()),
                                ),
                                (
                                    "rotation",
                                    RenderFieldValue::number(
                                        step.rotation().quarter_turns().to_string(),
                                    ),
                                ),
                                ("x", RenderFieldValue::number(step.x().to_string())),
                                ("y", RenderFieldValue::number(step.y().to_string())),
                                ("hold", RenderFieldValue::string(step.hold_decision())),
                                (
                                    "cleared_lines",
                                    RenderFieldValue::number(step.cleared_lines().to_string()),
                                ),
                                (
                                    "damage",
                                    RenderFieldValue::number(step.damage().to_string()),
                                ),
                            ])
                        })),
                    ),
                ])
            }));
            let mut fields = vec![
                RenderField::new("complete", result.complete()),
                RenderField::new("workers_used", result.workers_used()),
                RenderField::new("visited_states", result.visited_states()),
                RenderField::new("generated_locks", result.generated_locks()),
                RenderField::new("peak_frontier", result.peak_frontier()),
                RenderField::new(
                    "maximum_damage",
                    result
                        .maximum_damage()
                        .map_or(RenderFieldValue::Null, |value| {
                            RenderFieldValue::number(value.to_string())
                        }),
                ),
                RenderField::new("outcomes", outcomes),
            ];
            let solution_data_status =
                SolutionDataStatus::for_request(include_solution_data, true, result.complete());
            append_solution_data_contract(&mut fields, solution_data_status, format);
            if solution_data_status.exposes_artifacts() {
                fields.extend([RenderField::new(
                    "forward_solution_data",
                    RenderFieldValue::object([
                        (
                            "initial_board",
                            RenderFieldValue::string(board_mask_hex(result.initial_board())),
                        ),
                        (
                            "outcomes",
                            RenderFieldValue::array(result.outcomes().iter().map(|outcome| {
                                RenderFieldValue::object([
                                    ("id", RenderFieldValue::from(outcome.id())),
                                    (
                                        "source_pattern_index",
                                        RenderFieldValue::from(outcome.source_pattern_index()),
                                    ),
                                    (
                                        "source_queue",
                                        RenderFieldValue::string(
                                            outcome
                                                .source_queue()
                                                .iter()
                                                .map(|piece| piece.as_ascii())
                                                .collect::<String>(),
                                        ),
                                    ),
                                    (
                                        "group",
                                        outcome.group().map_or(RenderFieldValue::Null, |group| {
                                            RenderFieldValue::string(group.as_str())
                                        }),
                                    ),
                                    (
                                        "spin_piece",
                                        outcome.spin_piece().map_or(
                                            RenderFieldValue::Null,
                                            |piece| {
                                                RenderFieldValue::string(
                                                    piece.as_ascii().to_string(),
                                                )
                                            },
                                        ),
                                    ),
                                    ("spin_mini", RenderFieldValue::bool(outcome.spin_mini())),
                                    ("spin_lines", RenderFieldValue::from(outcome.spin_lines())),
                                    (
                                        "total_damage",
                                        RenderFieldValue::from(outcome.total_damage()),
                                    ),
                                    (
                                        "final_board",
                                        RenderFieldValue::string(board_mask_hex(
                                            outcome.final_board(),
                                        )),
                                    ),
                                    (
                                        "path",
                                        RenderFieldValue::array(outcome.path().iter().map(
                                            |step| {
                                                RenderFieldValue::object([
                                                    (
                                                        "piece",
                                                        RenderFieldValue::string(
                                                            step.piece().as_ascii().to_string(),
                                                        ),
                                                    ),
                                                    (
                                                        "placement_mask",
                                                        RenderFieldValue::string(board_mask_hex(
                                                            step.placement_mask(),
                                                        )),
                                                    ),
                                                    (
                                                        "cleared_row_mask",
                                                        RenderFieldValue::from(
                                                            step.cleared_row_mask(),
                                                        ),
                                                    ),
                                                    (
                                                        "board_after",
                                                        RenderFieldValue::string(board_mask_hex(
                                                            step.board_after(),
                                                        )),
                                                    ),
                                                ])
                                            },
                                        )),
                                    ),
                                ])
                            })),
                        ),
                    ]),
                )]);
            }
            CommandRenderer::render_output(model.kind().as_str(), fields, format)
        }
        AppRenderModel::SpinStructure(result) => {
            let render_structure_operation = |operation: StructureOperation| {
                RenderFieldValue::object([
                    (
                        "piece",
                        RenderFieldValue::string(operation.piece().as_ascii().to_string()),
                    ),
                    (
                        "rotation",
                        RenderFieldValue::number(operation.rotation().quarter_turns().to_string()),
                    ),
                    ("x", RenderFieldValue::number(operation.x().to_string())),
                    ("y", RenderFieldValue::number(operation.y().to_string())),
                    (
                        "logical_mask",
                        RenderFieldValue::string(board_mask_hex(operation.mask().words())),
                    ),
                    (
                        "need_deleted_rows",
                        RenderFieldValue::number(operation.need_deleted_rows().to_string()),
                    ),
                ])
            };
            let render_bucket = |mini: bool| {
                let outcomes = if mini { &result.mini } else { &result.regular };
                RenderFieldValue::array(outcomes.iter().map(|outcome| {
                    RenderFieldValue::object([
                        (
                            "class",
                            RenderFieldValue::string(if mini { "mini" } else { "regular" }),
                        ),
                        (
                            "placement_count",
                            RenderFieldValue::number(outcome.placement_count().to_string()),
                        ),
                        (
                            "board_before_spin",
                            RenderFieldValue::string(board_mask_hex(
                                outcome.board_before_spin.words(),
                            )),
                        ),
                        (
                            "final_board",
                            RenderFieldValue::string(board_mask_hex(outcome.final_board.words())),
                        ),
                        (
                            "spin",
                            RenderFieldValue::object([
                                (
                                    "piece",
                                    RenderFieldValue::string(
                                        outcome.spin.piece.as_ascii().to_string(),
                                    ),
                                ),
                                (
                                    "rotation",
                                    RenderFieldValue::number(
                                        outcome.spin.rotation.quarter_turns().to_string(),
                                    ),
                                ),
                                ("x", RenderFieldValue::number(outcome.spin.x.to_string())),
                                ("y", RenderFieldValue::number(outcome.spin.y.to_string())),
                                (
                                    "cleared_lines",
                                    RenderFieldValue::number(
                                        outcome.spin.cleared_lines.to_string(),
                                    ),
                                ),
                                (
                                    "structure",
                                    render_structure_operation(outcome.logical_spin()),
                                ),
                                (
                                    "logical_cleared_rows",
                                    RenderFieldValue::number(
                                        outcome.logical_spin_cleared_rows().to_string(),
                                    ),
                                ),
                            ]),
                        ),
                        (
                            "structure",
                            RenderFieldValue::array(
                                outcome
                                    .logical_operations()
                                    .iter()
                                    .copied()
                                    .map(render_structure_operation),
                            ),
                        ),
                        (
                            "build",
                            RenderFieldValue::array(outcome.build.iter().map(|placement| {
                                RenderFieldValue::object([
                                    (
                                        "piece",
                                        RenderFieldValue::string(
                                            placement.piece.as_ascii().to_string(),
                                        ),
                                    ),
                                    (
                                        "rotation",
                                        RenderFieldValue::number(
                                            placement.rotation.quarter_turns().to_string(),
                                        ),
                                    ),
                                    ("x", RenderFieldValue::number(placement.x.to_string())),
                                    ("y", RenderFieldValue::number(placement.y.to_string())),
                                    (
                                        "placement_mask",
                                        RenderFieldValue::string(board_mask_hex(
                                            placement.mask_before_clear.words(),
                                        )),
                                    ),
                                    (
                                        "cleared_rows",
                                        RenderFieldValue::number(
                                            placement.cleared_rows.to_string(),
                                        ),
                                    ),
                                ])
                            })),
                        ),
                    ])
                }))
            };
            let query = result.query.as_ref();
            let mut fields = vec![
                RenderField::new("complete", result.complete),
                RenderField::new("workers_used", result.workers_used()),
                RenderField::new(
                    "spin_profile",
                    query.map_or(RenderFieldValue::Null, |query| {
                        RenderFieldValue::string(query.mode.as_str())
                    }),
                ),
                RenderField::new(
                    "minimality",
                    query.map_or(RenderFieldValue::Null, |query| {
                        RenderFieldValue::string(query.minimality.as_str())
                    }),
                ),
                RenderField::new(
                    "line_requirement",
                    query.map_or(RenderFieldValue::Null, |query| {
                        RenderFieldValue::string(query.line_requirement.as_str())
                    }),
                ),
                RenderField::new(
                    "minimum_placements",
                    result
                        .minimum_placements
                        .map_or(RenderFieldValue::Null, RenderFieldValue::from),
                ),
                RenderField::new("result_count", result.outcome_count()),
                RenderField::new("regular_count", result.regular.len()),
                RenderField::new("mini_count", result.mini.len()),
                RenderField::new("regular", render_bucket(false)),
                RenderField::new("mini", render_bucket(true)),
                RenderField::new(
                    "stages",
                    RenderFieldValue::object([
                        (
                            "build_states",
                            RenderFieldValue::number(result.stages.build_states.to_string()),
                        ),
                        (
                            "fill_checks",
                            RenderFieldValue::number(result.stages.fill_checks.to_string()),
                        ),
                        (
                            "support_locks",
                            RenderFieldValue::number(result.stages.support_locks.to_string()),
                        ),
                        (
                            "corner_checks",
                            RenderFieldValue::number(result.stages.corner_checks.to_string()),
                        ),
                        (
                            "entry_states",
                            RenderFieldValue::number(result.stages.entry_states.to_string()),
                        ),
                        (
                            "verification_checks",
                            RenderFieldValue::number(result.stages.verification_checks.to_string()),
                        ),
                    ]),
                ),
                RenderField::new(
                    "layers",
                    RenderFieldValue::array(result.layers.iter().map(|layer| {
                        RenderFieldValue::object([
                            ("depth", RenderFieldValue::from(layer.depth)),
                            (
                                "input_states",
                                RenderFieldValue::number(layer.input_states.to_string()),
                            ),
                            (
                                "piece_choices",
                                RenderFieldValue::number(layer.piece_choices.to_string()),
                            ),
                            (
                                "reachable_locks",
                                RenderFieldValue::number(layer.reachable_locks.to_string()),
                            ),
                            (
                                "generated_states",
                                RenderFieldValue::number(layer.generated_states.to_string()),
                            ),
                            (
                                "accepted_regular",
                                RenderFieldValue::number(layer.accepted_regular.to_string()),
                            ),
                            (
                                "accepted_mini",
                                RenderFieldValue::number(layer.accepted_mini.to_string()),
                            ),
                        ])
                    })),
                ),
            ];
            let solution_data_status = SolutionDataStatus::for_request(
                include_solution_data,
                query.is_some(),
                result.complete,
            );
            append_solution_data_contract(&mut fields, solution_data_status, format);
            if solution_data_status.exposes_artifacts() {
                let encode_solution_key =
                    |outcome: &SpinStructureOutcome, query: &SpinStructureQuery| {
                        let mut operations = outcome.logical_operations().to_vec();
                        operations.sort_unstable();
                        let mut key = format!(
                            "ctk2|height={}|initial={}|placements=",
                            query.height,
                            board_mask_hex(query.initial_board.words()).trim_start_matches("0x"),
                        );
                        for (index, operation) in operations.into_iter().enumerate() {
                            if index != 0 {
                                key.push(',');
                            }
                            key.push(operation.piece().as_ascii());
                            key.push(':');
                            key.push_str(
                                board_mask_hex(operation.mask().words()).trim_start_matches("0x"),
                            );
                        }
                        key
                    };
                let solution_keys = query.map_or_else(Vec::new, |query| {
                    result
                        .outcomes()
                        .map(|outcome| encode_solution_key(outcome, query))
                        .collect::<Vec<_>>()
                });
                let solution_classes = result
                    .regular
                    .iter()
                    .map(|_| "regular")
                    .chain(result.mini.iter().map(|_| "mini"))
                    .collect::<Vec<_>>();
                fields.extend([
                    RenderField::new(
                        "solution_keys",
                        RenderFieldValue::array(solution_keys.iter().map(RenderFieldValue::string)),
                    ),
                    RenderField::new(
                        "solution_classes",
                        RenderFieldValue::array(
                            solution_classes
                                .iter()
                                .copied()
                                .map(RenderFieldValue::string),
                        ),
                    ),
                ]);
            }
            CommandRenderer::render_output(model.kind().as_str(), fields, format)
        }
        AppRenderModel::CoverMessage(message)
        | AppRenderModel::ScenarioMessage(message)
        | AppRenderModel::Path(message)
        | AppRenderModel::Rules(message)
        | AppRenderModel::Scoring(message)
        | AppRenderModel::Convert(message)
        | AppRenderModel::Continue(message)
        | AppRenderModel::Verify(message) => {
            if let Some(raw) = message.raw_body() {
                CliOutput::success(raw.to_owned())
            } else {
                CommandRenderer::render_output(
                    message.kind().as_str(),
                    message.fields().to_vec(),
                    format,
                )
            }
        }
    }
}

fn append_solution_data_contract(
    fields: &mut Vec<RenderField>,
    status: SolutionDataStatus,
    format: RenderFormat,
) {
    if format != RenderFormat::Json {
        return;
    }
    fields.extend([
        RenderField::new(
            "solution_data_requested",
            status != SolutionDataStatus::NotRequested,
        ),
        RenderField::new("solution_data_status", status.as_str()),
        RenderField::new(
            "solution_data_reason",
            status
                .reason()
                .map_or(RenderFieldValue::Null, RenderFieldValue::string),
        ),
    ]);
}

fn finesse_report_value(report: &clearra_app::FinesseReport) -> RenderFieldValue {
    RenderFieldValue::object([
        ("mode", RenderFieldValue::string(report.mode())),
        ("metric", RenderFieldValue::string(report.metric())),
        (
            "pattern_knowledge",
            RenderFieldValue::string(report.pattern_knowledge()),
        ),
        ("complete", RenderFieldValue::bool(report.complete())),
        (
            "exact_total_inputs",
            report
                .exact_total_inputs()
                .map_or(RenderFieldValue::Null, |value| {
                    RenderFieldValue::string(value.to_owned())
                }),
        ),
        (
            "representative_witness",
            report
                .representative_witness()
                .map_or(RenderFieldValue::Null, |witness| {
                    RenderFieldValue::object([
                        ("policy", RenderFieldValue::string(witness.policy())),
                        (
                            "solution_key",
                            optional_string_value(witness.solution_key()),
                        ),
                        (
                            "pattern_ids",
                            RenderFieldValue::array(
                                witness
                                    .pattern_ids()
                                    .iter()
                                    .copied()
                                    .map(RenderFieldValue::from),
                            ),
                        ),
                        (
                            "queue",
                            RenderFieldValue::array(witness.queue().iter().map(|piece| {
                                RenderFieldValue::string(piece.as_ascii().to_string())
                            })),
                        ),
                        (
                            "total_inputs",
                            RenderFieldValue::from(witness.total_inputs()),
                        ),
                        (
                            "input_sequence",
                            RenderFieldValue::array(
                                witness
                                    .input_sequence()
                                    .iter()
                                    .map(|input| RenderFieldValue::string(input.as_str())),
                            ),
                        ),
                        (
                            "placements",
                            RenderFieldValue::array(witness.placements().iter().map(|placement| {
                                RenderFieldValue::object([
                                    (
                                        "piece",
                                        RenderFieldValue::string(
                                            placement.piece().as_ascii().to_string(),
                                        ),
                                    ),
                                    (
                                        "rotation",
                                        RenderFieldValue::from(
                                            placement.rotation().quarter_turns(),
                                        ),
                                    ),
                                    ("x", RenderFieldValue::from(placement.x())),
                                    ("y", RenderFieldValue::from(placement.y())),
                                ])
                            })),
                        ),
                    ])
                }),
        ),
        (
            "policy_results",
            RenderFieldValue::array(report.policy_results().iter().map(|policy| {
                RenderFieldValue::object([
                    ("policy", RenderFieldValue::string(policy.policy())),
                    (
                        "overall_average_inputs",
                        RenderFieldValue::string(policy.overall_average_inputs()),
                    ),
                    ("complete", RenderFieldValue::bool(policy.complete())),
                    (
                        "oracle_on_covered_average_inputs",
                        optional_string_value(policy.oracle_on_covered_average_inputs()),
                    ),
                    (
                        "information_penalty_inputs",
                        optional_string_value(policy.information_penalty_inputs()),
                    ),
                    (
                        "success_probability_gap",
                        optional_string_value(policy.success_probability_gap()),
                    ),
                    (
                        "successful_probability_mass",
                        optional_string_value(policy.successful_probability_mass()),
                    ),
                    (
                        "successful_unique_queue_count",
                        policy
                            .successful_unique_queue_count()
                            .map_or(RenderFieldValue::Null, RenderFieldValue::from),
                    ),
                    (
                        "total_unique_queue_count",
                        policy
                            .total_unique_queue_count()
                            .map_or(RenderFieldValue::Null, RenderFieldValue::from),
                    ),
                    (
                        "solution_averages",
                        RenderFieldValue::array(policy.solution_averages().iter().map(
                            |solution| {
                                RenderFieldValue::object([
                                    (
                                        "solution_key",
                                        RenderFieldValue::string(solution.solution_key()),
                                    ),
                                    (
                                        "average_inputs",
                                        RenderFieldValue::string(solution.average_inputs()),
                                    ),
                                    ("complete", RenderFieldValue::bool(solution.complete())),
                                ])
                            },
                        )),
                    ),
                ])
            })),
        ),
    ])
}

fn optional_string_value(value: Option<&str>) -> RenderFieldValue {
    value.map_or(RenderFieldValue::Null, RenderFieldValue::string)
}

fn board_mask_hex(words: [u64; 4]) -> String {
    format!(
        "0x{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}

fn cli_error_for_app_error(code: AppErrorCode, default_error: CliErrorCode) -> CliErrorCode {
    match code {
        AppErrorCode::TraceUnavailable => CliErrorCode::PathTraceUnavailable,
        AppErrorCode::NoSolution => CliErrorCode::PathNoSolution,
        AppErrorCode::Unsupported => CliErrorCode::ProductRuntimeUnsupported,
        AppErrorCode::NativeCoreUnavailable => CliErrorCode::NativeCoreUnavailable,
        AppErrorCode::BackendGpuUnavailable => CliErrorCode::BackendGpuUnavailable,
        AppErrorCode::CliCommandUnsupported => CliErrorCode::CliCommandUnsupported,
        AppErrorCode::PcScenarioExpectedMismatch => CliErrorCode::PcScenarioExpectedMismatch,
        AppErrorCode::RulesProfileUnknown => CliErrorCode::RulesProfileUnknown,
        AppErrorCode::RulesInputRequired => CliErrorCode::RulesInputRequired,
        AppErrorCode::RulesInputInvalid => CliErrorCode::RulesInputInvalid,
        AppErrorCode::RulesExportUnsupported => CliErrorCode::RulesExportUnsupported,
        AppErrorCode::ScoringProfileUnknown => CliErrorCode::ScoringProfileUnknown,
        AppErrorCode::ScoringInputRequired => CliErrorCode::ScoringInputRequired,
        AppErrorCode::ScoringInputInvalid => CliErrorCode::ScoringInputInvalid,
        AppErrorCode::ConvertInputRequired => CliErrorCode::ConvertInputRequired,
        AppErrorCode::ConvertDirectionUnsupported => CliErrorCode::ConvertDirectionUnsupported,
        AppErrorCode::ConvertInputInvalid => CliErrorCode::ConvertInputInvalid,
        AppErrorCode::ContinueTokenRequired => CliErrorCode::ContinueTokenRequired,
        AppErrorCode::ContinueTokenInvalid => CliErrorCode::ContinueTokenInvalid,
        AppErrorCode::VerifyTargetUnknown => CliErrorCode::VerifyTargetUnknown,
        AppErrorCode::VerifyKicksFailed => CliErrorCode::VerifyKicksFailed,
        _ => default_error,
    }
}

#[cfg(test)]
#[path = "app_response_renderer_tests.rs"]
mod tests;
