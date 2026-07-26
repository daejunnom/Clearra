use clearra_app::{AppErrorCode, AppRenderModel, AppResponse, AppStatus};

use crate::{
    error::CliErrorCode,
    output::{
        CliOutput, CommandRenderer, RenderField, RenderFieldValue, RenderFormat,
        SummaryRenderContract,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppResponseRenderer;

impl AppResponseRenderer {
    pub fn render(
        response: AppResponse,
        format: RenderFormat,
        default_error: CliErrorCode,
    ) -> CliOutput {
        match response.status() {
            AppStatus::Success => render_success(response, format, default_error),
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
            let mut fields = SummaryRenderContract::render_fields(result.summary_fields());
            if !result.solution_probabilities().is_empty() {
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
            CliOutput::success(CommandRenderer::render(
                model.kind().as_str(),
                fields,
                format,
            ))
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
            CliOutput::success(CommandRenderer::render(
                model.kind().as_str(),
                fields,
                format,
            ))
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
            let fields = vec![
                RenderField::new("complete", result.complete()),
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
            CliOutput::success(CommandRenderer::render(
                model.kind().as_str(),
                fields,
                format,
            ))
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
                CliOutput::success(CommandRenderer::render(
                    message.kind().as_str(),
                    message.fields().to_vec(),
                    format,
                ))
            }
        }
    }
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
