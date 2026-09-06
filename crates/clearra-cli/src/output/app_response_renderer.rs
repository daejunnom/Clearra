// SRP rationale: this module has one behavior-level change reason: rendering typed application responses into the stable CLI output contract.

use clearra_app::{
    setup_ranked_candidate_id, spin_structure_search_candidate_id, AppErrorCode, AppRenderModel,
    AppResponse, AppResultKind, AppStatus, ForwardSearchOutcome, PcBestSaveWinnerV2,
    PcSaveCompletenessEvidence, PcSaveGroupV2, PcSavePieceMultiset, PcSaveWitness,
    ProductCapabilityContract, ProductCapabilityResult, ProductCapabilityResultKind,
    BUILD_FIELD_AVERAGE_CAPABILITY, BUILD_FIELD_AVERAGE_RESULT_CONTRACT,
    BUILD_FIXED_SCORE_CAPABILITY, BUILD_FIXED_SCORE_RESULT_CONTRACT,
    BUILD_FIXED_SCORE_WINNER_CONTRACT, PC_BEST_SAVE_CANONICAL_SELECTION,
    PC_MINIMUM_COVER_CANONICAL_SELECTION, PC_PATH_CANONICAL_SELECTION,
    PC_SCORE_CANONICAL_SELECTION, PC_SCORE_INFORMATIONAL_ATTACK_BASIS,
    PC_SCORE_PATTERN_WINNER_CONTRACT,
};
use clearra_host_contract::{
    BuildCoveragePortfolioV2Payload, BuildPathFamilyPayload, BuildSetupFamilyV1Payload,
    BuildV2PayloadKind, BuildV2ProductPayload, CoveragePortfolioPagePayload,
    ExecutionAvailabilityState, ExecutionCompletenessState, PcPathFamilyPayload,
    PcPathWitnessPayload, PcScoreFieldSummaryPayload, ProductResultPayload,
    ProductResultPayloadContent, ScorePatternWinnerFamilyPayload, SetupRankedFamilyPayload,
    SetupScoreRankingPayload, SpinStructureFamilyPayload,
};
use clearra_spin_structure_search::{SpinStructureOutcome, SpinStructureQuery, StructureOperation};

use crate::{
    error::CliErrorCode,
    output::{
        CliOutput, CommandRenderer, RenderField, RenderFieldValue, RenderFormat,
        SummaryRenderContract,
    },
    tie_snapshot::ExplicitPortfolioOutput,
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
        Self::render_with_explicit_result(
            response,
            format,
            default_error,
            include_solution_data,
            None,
            false,
        )
    }

    pub(crate) fn render_with_explicit_result(
        response: AppResponse,
        format: RenderFormat,
        default_error: CliErrorCode,
        include_solution_data: bool,
        explicit_portfolio: Option<&ExplicitPortfolioOutput>,
        include_score_winner_family: bool,
    ) -> CliOutput {
        match response.status() {
            AppStatus::Success => render_success(
                response,
                format,
                default_error,
                include_solution_data,
                explicit_portfolio,
                include_score_winner_family,
            ),
            AppStatus::ValidationFailed => CliOutput::validation_failed_with_format(
                response.diagnostics().validation(),
                format,
            ),
            AppStatus::Unsupported => render_unsupported_failure(&response, format, default_error),
            AppStatus::ExecutionFailed => {
                render_execution_failure(&response, format, default_error)
            }
        }
    }

    pub(crate) fn render_portfolio_continuation(
        portfolio: &ExplicitPortfolioOutput,
        format: RenderFormat,
    ) -> CliOutput {
        CommandRenderer::render_output(
            "portfolio-alternative-page.v1",
            [explicit_portfolio_field(portfolio)],
            format,
        )
    }
}

const PUBLIC_UNSUPPORTED_MESSAGE: &str = "the requested operation is not supported";
const PUBLIC_EXECUTION_FAILURE_MESSAGE: &str = "the operation could not be completed";

fn render_unsupported_failure(
    response: &AppResponse,
    format: RenderFormat,
    default_error: CliErrorCode,
) -> CliOutput {
    let Some(error) = response.error() else {
        return CliOutput::error(default_error, PUBLIC_UNSUPPORTED_MESSAGE);
    };
    let cli_error = cli_error_for_app_error(error.code(), default_error);
    let message = if exposes_developer_failure_evidence(format) {
        error.message()
    } else {
        PUBLIC_UNSUPPORTED_MESSAGE
    };
    CliOutput::error(cli_error, message)
}

fn render_execution_failure(
    response: &AppResponse,
    format: RenderFormat,
    default_error: CliErrorCode,
) -> CliOutput {
    let Some(error) = response.error() else {
        return CliOutput::error(default_error, PUBLIC_EXECUTION_FAILURE_MESSAGE);
    };
    let cli_error = cli_error_for_app_error(error.code(), default_error);
    if matches!(format, RenderFormat::Text | RenderFormat::FumenLike) {
        return CliOutput::error(cli_error, PUBLIC_EXECUTION_FAILURE_MESSAGE);
    }
    if matches!(
        format,
        RenderFormat::TextVerbose | RenderFormat::TextDiagnostics
    ) {
        let report = response.resource_report();
        if !has_authoritative_resource_report(report) {
            return CliOutput::error(cli_error, error.message());
        }
        return CliOutput::new(
            cli_error.default_exit_code(),
            "",
            execution_failure_text(error.message(), report),
        );
    }
    if format != RenderFormat::Json {
        return CliOutput::error(cli_error, PUBLIC_EXECUTION_FAILURE_MESSAGE);
    }
    let fields = [
        RenderField::new(
            "error",
            RenderFieldValue::object([
                ("code", RenderFieldValue::string(cli_error.as_str())),
                ("message", RenderFieldValue::string(error.message())),
            ]),
        ),
        RenderField::new(
            "resource_report",
            resource_report_value(response.resource_report()),
        ),
    ];
    match CommandRenderer::render("execution-failed", fields, RenderFormat::Json) {
        Ok(rendered) => CliOutput::new(cli_error.default_exit_code(), rendered, ""),
        Err(render_error) => CliOutput::error(
            CliErrorCode::CliOutputLimitExceeded,
            render_error.to_string(),
        ),
    }
}

const fn exposes_developer_failure_evidence(format: RenderFormat) -> bool {
    matches!(
        format,
        RenderFormat::TextVerbose | RenderFormat::TextDiagnostics | RenderFormat::Json
    )
}

fn has_authoritative_resource_report(report: &clearra_host_contract::ResourceReport) -> bool {
    let availability = report.execution_availability();
    !matches!(report.memory_status(), "not-executed" | "not-reported")
        || report.truncated()
        || report.truncation_reason().is_some()
        || report.peak_frontier_states != 0
        || report.peak_candidate_rows != 0
        || report.peak_hash_buckets != 0
        || report.peak_gpu_bytes != 0
        || report.peak_cpu_bytes != 0
        || report.build_worker_backlog_peak != 0
        || report.coverage_rows_emitted != 0
        || report.probability_complete()
        || !matches!(
            availability.reason(),
            Some(clearra_host_contract::ExecutionAvailabilityReason::NotExecuted)
                | Some(clearra_host_contract::ExecutionAvailabilityReason::PartialExecution)
        )
        || availability.descriptor_pattern_count().is_some()
        || availability.dense_pattern_count().is_some()
        || availability.required_dense_bytes().is_some()
        || availability.required_memory_bytes().is_some()
}

fn execution_failure_text(message: &str, report: &clearra_host_contract::ResourceReport) -> String {
    let availability = report.execution_availability();
    fn optional(value: Option<&str>) -> &str {
        value.unwrap_or("not-reported")
    }
    [
        message.to_owned(),
        format!(
            "resource_report.solver_executed: {}",
            report.solver_executed()
        ),
        format!(
            "resource_report.execution_availability.state: {}",
            availability.state().as_str()
        ),
        format!(
            "resource_report.execution_availability.reason: {}",
            availability
                .reason()
                .map_or("not-reported", |reason| reason.as_str())
        ),
        format!(
            "resource_report.execution_availability.surface: {}",
            availability.surface().as_str()
        ),
        format!(
            "resource_report.execution_availability.descriptor_pattern_count: {}",
            optional(availability.descriptor_pattern_count())
        ),
        format!(
            "resource_report.execution_availability.dense_pattern_count: {}",
            optional(availability.dense_pattern_count())
        ),
        format!(
            "resource_report.execution_availability.required_dense_bytes: {}",
            optional(availability.required_dense_bytes())
        ),
        format!(
            "resource_report.execution_availability.required_memory_bytes: {}",
            optional(availability.required_memory_bytes())
        ),
        format!(
            "resource_report.result_completeness: {}",
            report.result_completeness().as_str()
        ),
    ]
    .join("\n")
}

fn resource_report_value(report: &clearra_host_contract::ResourceReport) -> RenderFieldValue {
    let availability = report.execution_availability();
    RenderFieldValue::object([
        (
            "solver_executed",
            RenderFieldValue::bool(report.solver_executed()),
        ),
        (
            "memory_status",
            RenderFieldValue::string(report.memory_status()),
        ),
        ("truncated", RenderFieldValue::bool(report.truncated())),
        (
            "truncation_reason",
            optional_string_value(report.truncation_reason()),
        ),
        (
            "peak_frontier_states",
            RenderFieldValue::from(report.peak_frontier_states),
        ),
        (
            "peak_candidate_rows",
            RenderFieldValue::from(report.peak_candidate_rows),
        ),
        (
            "peak_hash_buckets",
            RenderFieldValue::from(report.peak_hash_buckets),
        ),
        (
            "peak_gpu_bytes",
            RenderFieldValue::from(report.peak_gpu_bytes),
        ),
        (
            "peak_cpu_bytes",
            RenderFieldValue::from(report.peak_cpu_bytes),
        ),
        (
            "build_worker_backlog_peak",
            RenderFieldValue::from(report.build_worker_backlog_peak),
        ),
        (
            "coverage_rows_emitted",
            RenderFieldValue::from(report.coverage_rows_emitted),
        ),
        (
            "probability_complete",
            RenderFieldValue::bool(report.probability_complete()),
        ),
        (
            "execution_availability",
            RenderFieldValue::object([
                (
                    "state",
                    RenderFieldValue::string(availability.state().as_str()),
                ),
                (
                    "reason",
                    availability
                        .reason()
                        .map(|reason| RenderFieldValue::string(reason.as_str()))
                        .unwrap_or(RenderFieldValue::Null),
                ),
                (
                    "surface",
                    RenderFieldValue::string(availability.surface().as_str()),
                ),
                (
                    "descriptor_pattern_count",
                    optional_string_value(availability.descriptor_pattern_count()),
                ),
                (
                    "dense_pattern_count",
                    optional_string_value(availability.dense_pattern_count()),
                ),
                (
                    "required_dense_bytes",
                    optional_string_value(availability.required_dense_bytes()),
                ),
                (
                    "required_memory_bytes",
                    optional_string_value(availability.required_memory_bytes()),
                ),
            ]),
        ),
        (
            "result_completeness",
            RenderFieldValue::string(report.result_completeness().as_str()),
        ),
    ])
}

fn render_success(
    response: AppResponse,
    format: RenderFormat,
    default_error: CliErrorCode,
    include_solution_data: bool,
    explicit_portfolio: Option<&ExplicitPortfolioOutput>,
    include_score_winner_family: bool,
) -> CliOutput {
    let product_result = response.product_capability_result();
    if let Some(payload) = response.public_result_payload() {
        if let Some(output) = render_public_build_result(
            payload,
            format,
            explicit_portfolio,
            product_result,
            include_score_winner_family,
            default_error,
        ) {
            return output;
        }
    }
    if let Some(payload) = product_result.and_then(ProductCapabilityResult::public_result_payload) {
        if let Some(output) = render_public_build_result(
            &payload,
            format,
            explicit_portfolio,
            product_result,
            include_score_winner_family,
            default_error,
        ) {
            return output;
        }
    }
    let product_identity = product_result.map(|result| (result.contract(), result.result_kind()));
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
            if let Err(reason) = append_pc_score_minimals_fields(&mut fields, product_result) {
                return CliOutput::error(default_error, reason);
            }
            if let Err(reason) = append_pc_save_product_fields(&mut fields, product_result) {
                return CliOutput::error(default_error, reason);
            }
            if let Some(portfolio) = explicit_portfolio {
                fields.push(explicit_portfolio_field(portfolio));
            }
            if include_score_winner_family {
                if let Err(reason) = append_pc_score_winner_family(&mut fields, product_result) {
                    return CliOutput::error(default_error, reason);
                }
            }
            CommandRenderer::render_output(
                public_success_render_kind(product_identity, model.kind()),
                fields,
                format,
            )
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
                                                "candidate_id",
                                                RenderFieldValue::string(
                                                    setup_ranked_candidate_id(
                                                        condition.condition_id(),
                                                        candidate,
                                                    ),
                                                ),
                                            ),
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
        AppRenderModel::Damage(result)
        | AppRenderModel::SpinFinder(result)
        | AppRenderModel::Ren(result) => {
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
                        "ren_count",
                        outcome.ren_count().map_or(RenderFieldValue::Null, |value| {
                            RenderFieldValue::number(value.to_string())
                        }),
                    ),
                    (
                        "total_damage",
                        RenderFieldValue::number(outcome.total_damage().to_string()),
                    ),
                    (
                        "evidence_path_count",
                        RenderFieldValue::string(outcome.evidence_path_count()),
                    ),
                    (
                        "evidence_complete",
                        RenderFieldValue::bool(outcome.evidence_complete()),
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
                RenderField::new(
                    "maximum_ren",
                    result
                        .maximum_ren()
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
                let forward_solution_outcome_value = |outcome: &ForwardSearchOutcome| {
                    RenderFieldValue::object([
                        ("id", RenderFieldValue::string(outcome.id().to_string())),
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
                            outcome
                                .spin_piece()
                                .map_or(RenderFieldValue::Null, |piece| {
                                    RenderFieldValue::string(piece.as_ascii().to_string())
                                }),
                        ),
                        ("spin_mini", RenderFieldValue::bool(outcome.spin_mini())),
                        ("spin_lines", RenderFieldValue::from(outcome.spin_lines())),
                        (
                            "ren_count",
                            outcome
                                .ren_count()
                                .map_or(RenderFieldValue::Null, RenderFieldValue::from),
                        ),
                        (
                            "total_damage",
                            RenderFieldValue::from(outcome.total_damage()),
                        ),
                        (
                            "evidence_path_count",
                            RenderFieldValue::string(outcome.evidence_path_count()),
                        ),
                        (
                            "evidence_complete",
                            RenderFieldValue::bool(outcome.evidence_complete()),
                        ),
                        (
                            "final_board",
                            RenderFieldValue::string(board_mask_hex(outcome.final_board())),
                        ),
                        (
                            "path",
                            RenderFieldValue::array(outcome.path().iter().map(|step| {
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
                                        RenderFieldValue::from(step.cleared_row_mask()),
                                    ),
                                    (
                                        "board_after",
                                        RenderFieldValue::string(board_mask_hex(
                                            step.board_after(),
                                        )),
                                    ),
                                ])
                            })),
                        ),
                    ])
                };
                let artifact_outcomes = result
                    .outcomes()
                    .iter()
                    .map(&forward_solution_outcome_value)
                    .collect::<Vec<_>>();
                let mut forward_fields = vec![
                    (
                        "initial_board",
                        RenderFieldValue::string(board_mask_hex(result.initial_board())),
                    ),
                    (
                        "outcomes",
                        RenderFieldValue::array(artifact_outcomes.iter().cloned()),
                    ),
                ];
                if model.kind() == AppResultKind::Ren {
                    forward_fields.extend([
                        (
                            "canonical_selection",
                            RenderFieldValue::string("smallest-canonical-candidate-id"),
                        ),
                        (
                            "canonical_outcome",
                            result
                                .canonical_outcome()
                                .map_or(RenderFieldValue::Null, &forward_solution_outcome_value),
                        ),
                    ]);
                }
                fields.extend([RenderField::new(
                    "forward_solution_data",
                    RenderFieldValue::object(forward_fields),
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
                            "candidate_id",
                            RenderFieldValue::string(spin_structure_search_candidate_id(outcome)),
                        ),
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
        | AppRenderModel::Continue(message) => {
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
        AppRenderModel::Verify(message) => {
            if let Some(raw) = message.raw_body() {
                CliOutput::success(raw.to_owned())
            } else {
                let fields = SummaryRenderContract::render_fields(
                    message
                        .fields()
                        .iter()
                        .map(|field| (field.key().to_owned(), field.value().as_text())),
                );
                CommandRenderer::render_output(message.kind().as_str(), fields, format)
            }
        }
    }
}

fn render_public_build_result(
    payload: &ProductResultPayload,
    format: RenderFormat,
    explicit_portfolio: Option<&ExplicitPortfolioOutput>,
    product_result: Option<&ProductCapabilityResult>,
    include_score_winner_family: bool,
    default_error: CliErrorCode,
) -> Option<CliOutput> {
    let mut fields = match payload.content() {
        ProductResultPayloadContent::CoveragePortfolio(page) => {
            match coverage_portfolio_fields(payload.contract(), payload.result_kind(), page) {
                Ok(fields) => fields,
                Err(reason) => return Some(CliOutput::error(default_error, reason)),
            }
        }
        ProductResultPayloadContent::BuildV2(build) => build_v2_fields(build),
        ProductResultPayloadContent::BuildCoveragePortfolioV2(build) => {
            build_cover_fields(payload.contract(), build)
        }
        ProductResultPayloadContent::BuildSetupFamilyV1(build) => {
            build_setup_fields(payload.contract(), build)
        }
        ProductResultPayloadContent::SetupRankedFamily(family) => {
            setup_ranked_family_fields(payload.contract(), family)
        }
        ProductResultPayloadContent::SetupScoreRanking(ranking) => setup_score_fields(ranking),
        ProductResultPayloadContent::SpinStructureFamily(family) => {
            spin_structure_family_fields(payload.contract(), family)
        }
        ProductResultPayloadContent::PcScoreFieldSummary(summary)
            if is_supported_score_field_summary(payload) =>
        {
            pc_score_field_summary_fields(payload, summary)
        }
        ProductResultPayloadContent::ScorePatternWinnerFamily(family)
            if is_supported_score_winner_family(payload, family) =>
        {
            match score_pattern_winner_family_fields(payload, family) {
                Ok(fields) => fields,
                Err(reason) => return Some(CliOutput::error(default_error, reason)),
            }
        }
        ProductResultPayloadContent::PcPathFamily(family) => match pc_path_fields(family) {
            Ok(fields) => fields,
            Err(reason) => return Some(CliOutput::error(default_error, reason)),
        },
        ProductResultPayloadContent::BuildPathFamily(family) => match build_path_fields(family) {
            Ok(fields) => fields,
            Err(reason) => return Some(CliOutput::error(default_error, reason)),
        },
        _ => return None,
    };
    if let Err(reason) = append_pc_score_minimals_resource_report(&mut fields, product_result) {
        return Some(CliOutput::error(default_error, reason));
    }
    if let Some(portfolio) = explicit_portfolio {
        fields.push(explicit_portfolio_field(portfolio));
    }
    if let Err(reason) = append_pc_score_minimals_fields(&mut fields, product_result) {
        return Some(CliOutput::error(default_error, reason));
    }
    if include_score_winner_family
        && payload.contract() == ProductCapabilityContract::PcScore.as_str()
    {
        if let Err(reason) = append_pc_score_winner_family(&mut fields, product_result) {
            return Some(CliOutput::error(default_error, reason));
        }
    }
    Some(CommandRenderer::render_output(
        payload.result_kind(),
        fields,
        format,
    ))
}

fn is_supported_score_field_summary(payload: &ProductResultPayload) -> bool {
    matches!(
        (payload.contract(), payload.result_kind()),
        ("pc.score", "pc-score-summary.v2")
            | (
                BUILD_FIELD_AVERAGE_CAPABILITY,
                BUILD_FIELD_AVERAGE_RESULT_CONTRACT
            )
    )
}

fn is_supported_score_winner_family(
    payload: &ProductResultPayload,
    family: &ScorePatternWinnerFamilyPayload,
) -> bool {
    matches!(
        (
            payload.contract(),
            payload.result_kind(),
            family.winner_contract()
        ),
        (
            "pc.score-finder",
            "pc-fixed-score-witness.v2",
            PC_SCORE_PATTERN_WINNER_CONTRACT
        ) | (
            BUILD_FIXED_SCORE_CAPABILITY,
            BUILD_FIXED_SCORE_RESULT_CONTRACT,
            BUILD_FIXED_SCORE_WINNER_CONTRACT
        )
    )
}

fn append_pc_score_minimals_resource_report(
    fields: &mut Vec<RenderField>,
    product: Option<&ProductCapabilityResult>,
) -> Result<(), &'static str> {
    let Some(product) = product else {
        return Ok(());
    };
    if (product.contract(), product.result_kind())
        != (
            ProductCapabilityContract::PcScoreMinimals,
            ProductCapabilityResultKind::PcScorePortfolioV2,
        )
    {
        return Ok(());
    }
    let resources = product.resource_evidence();
    if !resources.solver_executed()
        || resources.availability() != ExecutionAvailabilityState::Available
        || resources.completeness() != ExecutionCompletenessState::Complete
        || resources.truncated()
        || !resources.probability_complete()
    {
        return Err("pc score-minimals resource report was incomplete");
    }
    fields.push(RenderField::new(
        "resource_report",
        RenderFieldValue::object([
            (
                "probability_complete",
                RenderFieldValue::bool(resources.probability_complete()),
            ),
            ("count_complete", RenderFieldValue::bool(true)),
            ("truncated", RenderFieldValue::bool(resources.truncated())),
            ("truncation_reason", RenderFieldValue::Null),
            ("count_truncated_reason", RenderFieldValue::Null),
            ("renormalized", RenderFieldValue::bool(false)),
        ]),
    ));
    Ok(())
}

fn pc_score_field_summary_fields(
    payload: &ProductResultPayload,
    summary: &PcScoreFieldSummaryPayload,
) -> Vec<RenderField> {
    vec![
        RenderField::new("capability_id", payload.contract()),
        RenderField::new("result_contract", payload.result_kind()),
        RenderField::new("payload_kind", "pc-score-field-summary"),
        RenderField::new("score_solution_field_contract", summary.field_contract()),
        RenderField::new("score_solution_field_ordering", summary.ordering()),
        RenderField::new(
            "score_solution_field_average_basis",
            summary.solution_field_average_basis(),
        ),
        RenderField::new("score_evaluation_basis", summary.score_evaluation_basis()),
        RenderField::new("score_evaluation_scope", summary.score_evaluation_scope()),
        RenderField::new("score_overall_basis", summary.overall_score_basis()),
        RenderField::new("piece_source_id", summary.piece_source_id()),
        RenderField::new("pattern_universe_id", summary.pattern_universe_id()),
        RenderField::new("pattern_weight_model_id", summary.pattern_weight_model_id()),
        RenderField::new(
            "materialized_pattern_count",
            summary.materialized_pattern_count(),
        ),
        RenderField::new("score_solution_field_count", summary.solution_field_count()),
        RenderField::new(
            "score_success_pattern_count",
            summary.scored_pattern_count(),
        ),
        RenderField::new(
            "score_failed_pc_pattern_count",
            summary.failed_pc_pattern_count(),
        ),
        RenderField::new("score_covered_probability", summary.covered_probability()),
        RenderField::new("score_overall_score", summary.overall_score()),
        RenderField::new(
            "score_covered_pattern_conditional_average_score",
            optional_string_value(summary.score_covered_pattern_conditional_average_score()),
        ),
        RenderField::new("score_summary_complete", summary.complete()),
        RenderField::new(
            "score_solution_fields",
            RenderFieldValue::array(summary.fields().iter().map(|field| {
                RenderFieldValue::object([
                    (
                        "normalized_field_key",
                        RenderFieldValue::string(field.normalized_field_key()),
                    ),
                    (
                        "average_score",
                        RenderFieldValue::string(field.average_score()),
                    ),
                    (
                        "covered_pattern_count",
                        RenderFieldValue::string(field.covered_pattern_count()),
                    ),
                    (
                        "pattern_count",
                        RenderFieldValue::string(field.pattern_count()),
                    ),
                    (
                        "score_complete",
                        RenderFieldValue::bool(field.score_complete()),
                    ),
                ])
            })),
        ),
    ]
}

fn score_pattern_winner_family_fields(
    payload: &ProductResultPayload,
    family: &ScorePatternWinnerFamilyPayload,
) -> Result<Vec<RenderField>, &'static str> {
    if !valid_score_pattern_canonical_witness(family) {
        return Err("score-pattern winner canonical witness was invalid");
    }
    Ok(vec![
        RenderField::new("capability_id", payload.contract()),
        RenderField::new("result_contract", payload.result_kind()),
        RenderField::new("payload_kind", "score-pattern-winner-family"),
        RenderField::new("score_pattern_winner_contract", family.winner_contract()),
        RenderField::new("score_pattern_winner_ordering", family.ordering()),
        RenderField::new("score_pattern_winner_equality", family.equality()),
        RenderField::new(
            "score_informational_attack_basis",
            family.informational_attack_basis(),
        ),
        RenderField::new("score_pattern_winner_count", family.winner_count()),
        RenderField::new("score_pattern_winner_complete", true),
        RenderField::new(
            "score_pattern_canonical_selection",
            family.canonical_selection(),
        ),
        RenderField::new(
            "score_pattern_canonical_winner",
            score_pattern_winner_value(family.canonical_winner(), family),
        ),
        RenderField::new(
            "score_pattern_winners",
            RenderFieldValue::array(
                family
                    .winners()
                    .iter()
                    .map(|winner| score_pattern_winner_value(winner, family)),
            ),
        ),
    ])
}

fn valid_score_pattern_canonical_witness(family: &ScorePatternWinnerFamilyPayload) -> bool {
    if family.canonical_selection() != PC_SCORE_CANONICAL_SELECTION
        || family.winner_count().parse::<usize>().ok() != Some(family.winners().len())
    {
        return false;
    }
    let canonical = family.canonical_winner();
    let Some(canonical_candidate_id) = canonical_decimal_u64(canonical.candidate_id()) else {
        return false;
    };
    if canonical_candidate_id == 0 {
        return false;
    }
    let mut witness_matches = 0_usize;
    for winner in family.winners() {
        let Some(candidate_id) = canonical_decimal_u64(winner.candidate_id()) else {
            return false;
        };
        if candidate_id < canonical_candidate_id {
            return false;
        }
        if winner == canonical {
            witness_matches += 1;
        }
    }
    witness_matches == 1
}

fn canonical_decimal_u64(value: &str) -> Option<u64> {
    let parsed = value.parse::<u64>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn score_pattern_winner_value(
    winner: &clearra_host_contract::ScorePatternWinnerPayload,
    family: &ScorePatternWinnerFamilyPayload,
) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "contract",
            RenderFieldValue::string(family.winner_contract()),
        ),
        ("pattern_id", RenderFieldValue::string(winner.pattern_id())),
        (
            "candidate_id",
            RenderFieldValue::string(winner.candidate_id()),
        ),
        (
            "normalized_solution_key",
            RenderFieldValue::string(winner.normalized_solution_key()),
        ),
        ("score", RenderFieldValue::string(winner.score())),
        (
            "informational_attack",
            RenderFieldValue::string(winner.informational_attack()),
        ),
        (
            "informational_attack_basis",
            RenderFieldValue::string(family.informational_attack_basis()),
        ),
    ])
}

fn coverage_portfolio_fields(
    capability_id: &str,
    result_contract: &str,
    payload: &CoveragePortfolioPagePayload,
) -> Result<Vec<RenderField>, &'static str> {
    let expected_canonical_selection = match capability_id {
        "pc.minimals" => Some(PC_MINIMUM_COVER_CANONICAL_SELECTION),
        _ => None,
    };
    if expected_canonical_selection
        .is_some_and(|selection| !valid_coverage_portfolio_canonical_witness(payload, selection))
    {
        return Err("PC portfolio canonical witness was missing or mismatched");
    }
    let mut fields = vec![
        RenderField::new("capability_id", capability_id),
        RenderField::new("result_contract", result_contract),
        RenderField::new("payload_kind", "coverage-portfolio"),
        RenderField::new("set_contract", payload.set_contract()),
        RenderField::new("page_contract", payload.page_contract()),
        RenderField::new("member_page_contract", payload.member_page_contract()),
        RenderField::new("set_identity_sha256", payload.set_identity_sha256()),
        RenderField::new("candidate_map_sha256", payload.candidate_map_sha256()),
        RenderField::new("alternative_index", payload.alternative_index()),
        RenderField::new("optimal_cardinality", payload.optimal_cardinality()),
        RenderField::new("known_alternative_count", payload.known_alternative_count()),
        RenderField::new(
            "total_alternative_count",
            optional_string_value(payload.total_alternative_count()),
        ),
        RenderField::new("enumeration_complete", payload.enumeration_complete()),
        RenderField::new("member_page_number", payload.member_page_number()),
        RenderField::new("total_member_pages", payload.total_member_pages()),
        RenderField::new(
            "members",
            RenderFieldValue::array(payload.members().iter().map(product_candidate_member_value)),
        ),
        RenderField::new("page_handle_available", payload.page_handle_available()),
    ];
    if expected_canonical_selection.is_some() {
        fields.extend([
            RenderField::new(
                "canonical_selection",
                optional_string_value(payload.canonical_selection()),
            ),
            RenderField::new(
                "canonical_witness",
                payload
                    .canonical_witness()
                    .map_or(RenderFieldValue::Null, product_candidate_member_value),
            ),
        ]);
    }
    Ok(fields)
}

fn valid_coverage_portfolio_canonical_witness(
    payload: &CoveragePortfolioPagePayload,
    expected_selection: &str,
) -> bool {
    if payload.canonical_selection() != Some(expected_selection) {
        return false;
    }
    let (Some(canonical), Some(first)) = (payload.canonical_witness(), payload.members().first())
    else {
        return false;
    };
    if canonical != first {
        return false;
    }
    let mut previous = None;
    for member in payload.members() {
        let Some(candidate_id) = canonical_decimal_u64(member.candidate_id()) else {
            return false;
        };
        if candidate_id == 0 || previous.is_some_and(|previous| candidate_id <= previous) {
            return false;
        }
        previous = Some(candidate_id);
    }
    true
}

fn product_candidate_member_value(
    member: &clearra_host_contract::ProductCandidateMemberPayload,
) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "candidate_id",
            RenderFieldValue::string(member.candidate_id()),
        ),
        (
            "normalized_solution_key",
            RenderFieldValue::string(member.normalized_solution_key()),
        ),
    ])
}

fn setup_ranked_family_fields(
    capability_id: &str,
    payload: &SetupRankedFamilyPayload,
) -> Vec<RenderField> {
    vec![
        RenderField::new("capability_id", capability_id),
        RenderField::new("result_contract", payload.schema_id()),
        RenderField::new("payload_kind", "setup-ranked-family"),
        RenderField::new("query_identity_sha256", payload.query_identity_sha256()),
        RenderField::new("rule_profile", payload.rule_profile()),
        RenderField::new("supply_identity_sha256", payload.supply_identity_sha256()),
        RenderField::new(
            "universe_identity_sha256",
            payload.universe_identity_sha256(),
        ),
        RenderField::new("product_build", payload.product_build()),
        RenderField::new("ordering", payload.ordering()),
        RenderField::new(
            "resolved_length_preference",
            payload.resolved_length_preference(),
        ),
        RenderField::new("candidate_count", payload.candidate_count()),
        RenderField::new(
            "candidates",
            RenderFieldValue::array(payload.candidates().iter().map(|candidate| {
                RenderFieldValue::object([
                    (
                        "candidate_id",
                        RenderFieldValue::string(candidate.candidate_id()),
                    ),
                    (
                        "condition_id",
                        RenderFieldValue::string(candidate.condition_id()),
                    ),
                    ("setup_id", RenderFieldValue::string(candidate.setup_id())),
                ])
            })),
        ),
    ]
}

fn spin_structure_family_fields(
    capability_id: &str,
    payload: &SpinStructureFamilyPayload,
) -> Vec<RenderField> {
    vec![
        RenderField::new("capability_id", capability_id),
        RenderField::new("result_contract", payload.schema_id()),
        RenderField::new("payload_kind", "spin-structure-family"),
        RenderField::new("query_identity_sha256", payload.query_identity_sha256()),
        RenderField::new("rule_profile", payload.rule_profile()),
        RenderField::new("spin_profile", payload.spin_profile()),
        RenderField::new("supply_identity_sha256", payload.supply_identity_sha256()),
        RenderField::new(
            "universe_identity_sha256",
            payload.universe_identity_sha256(),
        ),
        RenderField::new("product_build", payload.product_build()),
        RenderField::new("ordering", payload.ordering()),
        RenderField::new(
            "minimum_placements",
            optional_string_value(payload.minimum_placements()),
        ),
        RenderField::new(
            "guaranteed_final_piece",
            optional_string_value(payload.guaranteed_final_piece()),
        ),
        RenderField::new(
            "guarantee_basis",
            optional_string_value(payload.guarantee_basis()),
        ),
        RenderField::new(
            "dependency_report_included",
            optional_bool_value(payload.dependency_report_included()),
        ),
        RenderField::new(
            "dependency_relation",
            optional_string_value(payload.dependency_relation()),
        ),
        RenderField::new(
            "dependency_edge_count",
            optional_string_value(payload.dependency_edge_count()),
        ),
        RenderField::new("regular_count", payload.regular_count()),
        RenderField::new("mini_count", payload.mini_count()),
        RenderField::new("candidate_count", payload.candidate_count()),
        RenderField::new("complete", payload.complete()),
        RenderField::new(
            "candidates",
            RenderFieldValue::array(payload.candidates().iter().map(|candidate| {
                RenderFieldValue::object([
                    (
                        "candidate_id",
                        RenderFieldValue::string(candidate.candidate_id()),
                    ),
                    ("partition", RenderFieldValue::string(candidate.partition())),
                    (
                        "placement_count",
                        RenderFieldValue::string(candidate.placement_count()),
                    ),
                ])
            })),
        ),
    ]
}

fn pc_path_fields(payload: &PcPathFamilyPayload) -> Result<Vec<RenderField>, &'static str> {
    if !valid_pc_path_canonical_witness(payload) {
        return Err("PC path canonical witness was missing or mismatched");
    }
    Ok(vec![
        RenderField::new("capability_id", "pc.path"),
        RenderField::new("witness_contract", payload.witness_contract()),
        RenderField::new("ordering", payload.ordering()),
        RenderField::new("problem_id", payload.problem_id()),
        RenderField::new(
            "materialized_pattern_count",
            payload.materialized_pattern_count(),
        ),
        RenderField::new("witness_count", payload.witness_count()),
        RenderField::new("complete", payload.complete()),
        RenderField::new("canonical_selection", payload.canonical_selection()),
        RenderField::new(
            "canonical_witness",
            payload
                .canonical_witness()
                .map_or(RenderFieldValue::Null, pc_path_witness_value),
        ),
        RenderField::new(
            "witnesses",
            RenderFieldValue::array(payload.witnesses().iter().map(pc_path_witness_value)),
        ),
    ])
}

fn valid_pc_path_canonical_witness(payload: &PcPathFamilyPayload) -> bool {
    if payload.canonical_selection() != PC_PATH_CANONICAL_SELECTION {
        return false;
    }
    match (payload.canonical_witness(), payload.witnesses().first()) {
        (None, None) => true,
        (Some(canonical), Some(first)) if canonical == first => {
            let Some(canonical_id) = canonical_decimal_u64(canonical.candidate_id()) else {
                return false;
            };
            if canonical_id == 0 {
                return false;
            }
            payload.witnesses().iter().all(|witness| {
                canonical_decimal_u64(witness.candidate_id())
                    .is_some_and(|candidate_id| candidate_id >= canonical_id)
            })
        }
        _ => false,
    }
}

fn build_path_fields(payload: &BuildPathFamilyPayload) -> Result<Vec<RenderField>, &'static str> {
    if payload.canonical_selection() != "smallest-canonical-candidate-id"
        || !valid_canonical_path_witness(payload.canonical_witness(), payload.witnesses())
        || !canonical_hex_mask(payload.target_terminal_board_mask())
        || payload
            .mirrored_terminal_board_mask()
            .is_some_and(|mask| !canonical_hex_mask(mask))
    {
        return Err("Build path canonical witness or terminal was missing or mismatched");
    }
    Ok(vec![
        RenderField::new("capability_id", "build.complete-replay-paths"),
        RenderField::new("witness_contract", payload.witness_contract()),
        RenderField::new("ordering", payload.ordering()),
        RenderField::new("problem_id", payload.problem_id()),
        RenderField::new(
            "target_terminal_board_mask",
            payload.target_terminal_board_mask(),
        ),
        RenderField::new(
            "mirrored_terminal_board_mask",
            payload
                .mirrored_terminal_board_mask()
                .map_or(RenderFieldValue::Null, |mask| RenderFieldValue::from(mask)),
        ),
        RenderField::new(
            "materialized_pattern_count",
            payload.materialized_pattern_count(),
        ),
        RenderField::new("witness_count", payload.witness_count()),
        RenderField::new("complete", payload.complete()),
        RenderField::new("canonical_selection", payload.canonical_selection()),
        RenderField::new(
            "canonical_witness",
            payload
                .canonical_witness()
                .map_or(RenderFieldValue::Null, pc_path_witness_value),
        ),
        RenderField::new(
            "witnesses",
            RenderFieldValue::array(payload.witnesses().iter().map(pc_path_witness_value)),
        ),
    ])
}

fn valid_canonical_path_witness(
    canonical: Option<&PcPathWitnessPayload>,
    witnesses: &[PcPathWitnessPayload],
) -> bool {
    match (canonical, witnesses.first()) {
        (None, None) => true,
        (Some(canonical), Some(first)) if canonical == first => {
            let Some(canonical_id) = canonical_decimal_u64(canonical.candidate_id()) else {
                return false;
            };
            canonical_id > 0
                && witnesses.iter().all(|witness| {
                    canonical_decimal_u64(witness.candidate_id())
                        .is_some_and(|candidate_id| candidate_id >= canonical_id)
                })
        }
        _ => false,
    }
}

fn canonical_hex_mask(value: &str) -> bool {
    value.len() == 18
        && value.starts_with("0x")
        && value[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn pc_path_witness_value(witness: &PcPathWitnessPayload) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "candidate_id",
            RenderFieldValue::string(witness.candidate_id()),
        ),
        (
            "producer_candidate_id",
            RenderFieldValue::string(witness.producer_candidate_id()),
        ),
        ("pattern_id", RenderFieldValue::string(witness.pattern_id())),
        (
            "trace_identity",
            RenderFieldValue::string(witness.trace_identity()),
        ),
        (
            "normalized_trace_key",
            RenderFieldValue::string(witness.normalized_trace_key()),
        ),
        (
            "consumed_piece_count",
            RenderFieldValue::string(witness.consumed_piece_count()),
        ),
        (
            "terminal_hold_piece",
            optional_string_value(witness.terminal_hold_piece()),
        ),
        (
            "steps",
            RenderFieldValue::array(witness.steps().iter().map(|step| {
                RenderFieldValue::object([
                    ("step_index", RenderFieldValue::string(step.step_index())),
                    (
                        "operation_id",
                        RenderFieldValue::string(step.operation_id()),
                    ),
                    (
                        "active_piece",
                        RenderFieldValue::string(step.active_piece()),
                    ),
                    (
                        "input_cursor",
                        RenderFieldValue::string(step.input_cursor()),
                    ),
                    (
                        "output_cursor",
                        RenderFieldValue::string(step.output_cursor()),
                    ),
                    (
                        "input_hold_piece",
                        optional_string_value(step.input_hold_piece()),
                    ),
                    (
                        "output_hold_piece",
                        optional_string_value(step.output_hold_piece()),
                    ),
                    (
                        "hold_decision",
                        RenderFieldValue::string(step.hold_decision()),
                    ),
                    ("rotation", RenderFieldValue::string(step.rotation())),
                    ("x", RenderFieldValue::string(step.x())),
                    ("y", RenderFieldValue::string(step.y())),
                    (
                        "placement_mask",
                        RenderFieldValue::string(step.placement_mask()),
                    ),
                    (
                        "board_before_mask",
                        RenderFieldValue::string(step.board_before_mask()),
                    ),
                    (
                        "board_after_placement_mask",
                        RenderFieldValue::string(step.board_after_placement_mask()),
                    ),
                    (
                        "board_after_line_clear_mask",
                        RenderFieldValue::string(step.board_after_line_clear_mask()),
                    ),
                    (
                        "cleared_row_mask",
                        RenderFieldValue::string(step.cleared_row_mask()),
                    ),
                    (
                        "cleared_lines",
                        RenderFieldValue::string(step.cleared_lines()),
                    ),
                    (
                        "line_clear_identity",
                        RenderFieldValue::string(step.line_clear_identity()),
                    ),
                ])
            })),
        ),
    ])
}

fn setup_score_fields(payload: &SetupScoreRankingPayload) -> Vec<RenderField> {
    vec![
        RenderField::new("capability_id", "setup.score"),
        RenderField::new("result_contract", payload.schema_id()),
        RenderField::new("payload_kind", "ranked-family"),
        RenderField::new("input_identity_sha256", payload.input_identity_sha256()),
        RenderField::new(
            "evaluation_identity_sha256",
            payload.evaluation_identity_sha256(),
        ),
        RenderField::new("document_format", payload.document_format()),
        RenderField::new("rule_profile", payload.rule_profile()),
        RenderField::new("score_profile", payload.score_profile()),
        RenderField::new("initial_b2b", payload.initial_b2b()),
        RenderField::new("ordering", payload.ordering()),
        RenderField::new("source_page_count", payload.source_page_count()),
        RenderField::new("candidate_count", payload.candidate_count()),
        RenderField::new("setup_pattern_count", payload.setup_pattern_count()),
        RenderField::new("average_priority_score", payload.average_priority_score()),
        RenderField::new("complete", payload.complete()),
        RenderField::new(
            "candidates",
            RenderFieldValue::array(payload.candidates().iter().map(|candidate| {
                RenderFieldValue::object([
                    ("rank", RenderFieldValue::string(candidate.rank())),
                    (
                        "candidate_id",
                        RenderFieldValue::string(candidate.candidate_id()),
                    ),
                    (
                        "completed_board_mask",
                        RenderFieldValue::string(candidate.completed_board_mask()),
                    ),
                    (
                        "setup_covered_pattern_count",
                        RenderFieldValue::string(candidate.setup_covered_pattern_count()),
                    ),
                    (
                        "setup_covered_probability",
                        RenderFieldValue::string(candidate.setup_covered_probability()),
                    ),
                    (
                        "continuation_probability",
                        RenderFieldValue::string(candidate.continuation_probability()),
                    ),
                    (
                        "unconditional_expected_score",
                        RenderFieldValue::string(candidate.unconditional_expected_score()),
                    ),
                ])
            })),
        ),
    ]
}

fn build_v2_fields(payload: &BuildV2ProductPayload) -> Vec<RenderField> {
    let completeness = payload.completeness();
    vec![
        RenderField::new("capability_id", payload.capability_id()),
        RenderField::new("result_contract", payload.result_contract()),
        RenderField::new(
            "payload_kind",
            match payload.kind() {
                BuildV2PayloadKind::CandidateFamily => "candidate-family",
                BuildV2PayloadKind::Probability => "probability",
                BuildV2PayloadKind::Portfolio => "portfolio",
                BuildV2PayloadKind::ScorePortfolio => "score-portfolio",
            },
        ),
        RenderField::new("input_identity_sha256", payload.input_identity_sha256()),
        RenderField::new(
            "evaluation_identity_sha256",
            optional_string_value(payload.evaluation_identity_sha256()),
        ),
        RenderField::new(
            "replay_basis",
            optional_string_value(payload.replay_basis()),
        ),
        RenderField::new("objective", payload.objective()),
        RenderField::new(
            "score_profile",
            optional_string_value(payload.score_profile()),
        ),
        RenderField::new("initial_b2b", optional_string_value(payload.initial_b2b())),
        RenderField::new(
            "score_accuracy",
            optional_string_value(payload.score_accuracy()),
        ),
        RenderField::new(
            "profile_specific_exact",
            optional_bool_value(payload.profile_specific_exact()),
        ),
        RenderField::new(
            "score_equality_basis",
            optional_string_value(payload.score_equality_basis()),
        ),
        RenderField::new(
            "informational_attack_basis",
            optional_string_value(payload.informational_attack_basis()),
        ),
        RenderField::new("source_candidate_count", payload.source_candidate_count()),
        RenderField::new(
            "reachable_candidate_count",
            payload.reachable_candidate_count(),
        ),
        RenderField::new(
            "selected_candidate_count",
            optional_string_value(payload.selected_candidate_count()),
        ),
        RenderField::new("pattern_count", payload.pattern_count()),
        RenderField::new(
            "covered_pattern_count",
            optional_string_value(payload.covered_pattern_count()),
        ),
        RenderField::new(
            "required_pattern_count",
            optional_string_value(payload.required_pattern_count()),
        ),
        RenderField::new(
            "union_probability",
            optional_string_value(payload.union_probability()),
        ),
        RenderField::new(
            "b2b_preservation_required",
            optional_bool_value(payload.b2b_preservation_required()),
        ),
        RenderField::new(
            "candidates",
            RenderFieldValue::array(payload.candidates().iter().map(|candidate| {
                RenderFieldValue::object([
                    (
                        "candidate_key",
                        RenderFieldValue::string(candidate.candidate_key()),
                    ),
                    (
                        "covered_pattern_count",
                        RenderFieldValue::string(candidate.covered_pattern_count()),
                    ),
                ])
            })),
        ),
        RenderField::new(
            "canonical_candidate_keys",
            RenderFieldValue::array(
                payload
                    .canonical_candidate_keys()
                    .iter()
                    .map(RenderFieldValue::string),
            ),
        ),
        RenderField::new(
            "winners",
            RenderFieldValue::array(payload.winners().iter().map(|winner| {
                RenderFieldValue::object([
                    ("pattern_id", RenderFieldValue::string(winner.pattern_id())),
                    (
                        "candidate_key",
                        RenderFieldValue::string(winner.candidate_key()),
                    ),
                    ("score", RenderFieldValue::string(winner.score())),
                    (
                        "informational_attack",
                        RenderFieldValue::string(winner.informational_attack()),
                    ),
                ])
            })),
        ),
        RenderField::new(
            "completeness",
            RenderFieldValue::object([
                (
                    "input_identity_bound",
                    RenderFieldValue::bool(completeness.input_identity_bound()),
                ),
                (
                    "producer_filter_bound",
                    RenderFieldValue::bool(completeness.producer_filter_bound()),
                ),
                (
                    "buildability_replay_complete",
                    RenderFieldValue::bool(completeness.buildability_replay_complete()),
                ),
                (
                    "coverage_rows_complete",
                    RenderFieldValue::bool(completeness.coverage_rows_complete()),
                ),
                (
                    "probability_weights_complete",
                    RenderFieldValue::bool(completeness.probability_weights_complete()),
                ),
                (
                    "exact_minimum_proven",
                    RenderFieldValue::bool(completeness.exact_minimum_proven()),
                ),
                (
                    "score_evidence_complete",
                    RenderFieldValue::bool(completeness.score_evidence_complete()),
                ),
            ]),
        ),
        RenderField::new("page_source_available", payload.page_source_available()),
        RenderField::new(
            "page_source_identity_sha256",
            optional_string_value(payload.page_source_identity_sha256()),
        ),
    ]
}

fn build_cover_fields(
    capability_id: &str,
    payload: &BuildCoveragePortfolioV2Payload,
) -> Vec<RenderField> {
    let completeness = payload.completeness();
    vec![
        RenderField::new("capability_id", capability_id),
        RenderField::new("result_contract", payload.contract()),
        RenderField::new("payload_kind", "portfolio"),
        RenderField::new("objective", payload.objective()),
        RenderField::new("probability_basis", payload.probability_basis()),
        RenderField::new("source_candidate_count", payload.source_candidate_count()),
        RenderField::new(
            "selected_candidate_count",
            payload.selected_candidate_count(),
        ),
        RenderField::new("pattern_count", payload.pattern_count()),
        RenderField::new("required_pattern_count", payload.required_pattern_count()),
        RenderField::new("union_probability", payload.union_probability()),
        RenderField::new(
            "normalized_solution_set_hash",
            payload.normalized_solution_set_hash(),
        ),
        RenderField::new(
            "canonical_first_candidate_id",
            payload.canonical_first_candidate_id(),
        ),
        RenderField::new(
            "completeness",
            RenderFieldValue::object([
                (
                    "source_universe_complete",
                    RenderFieldValue::bool(completeness.source_universe_complete()),
                ),
                (
                    "coverage_rows_complete",
                    RenderFieldValue::bool(completeness.coverage_rows_complete()),
                ),
                (
                    "probability_weights_complete",
                    RenderFieldValue::bool(completeness.probability_weights_complete()),
                ),
                (
                    "exact_minimum_proven",
                    RenderFieldValue::bool(completeness.exact_minimum_proven()),
                ),
                (
                    "query_bound",
                    RenderFieldValue::bool(completeness.query_bound()),
                ),
            ]),
        ),
        RenderField::new("page_source_available", payload.page_source_available()),
        RenderField::new(
            "page_source_identity_sha256",
            optional_string_value(payload.page_source_identity_sha256()),
        ),
    ]
}

fn build_setup_fields(
    capability_id: &str,
    payload: &BuildSetupFamilyV1Payload,
) -> Vec<RenderField> {
    let completeness = payload.completeness();
    vec![
        RenderField::new("capability_id", capability_id),
        RenderField::new("result_contract", payload.contract()),
        RenderField::new("payload_kind", "candidate-family"),
        RenderField::new("input_identity_sha256", payload.input_identity_sha256()),
        RenderField::new(
            "evaluation_identity_sha256",
            payload.evaluation_identity_sha256(),
        ),
        RenderField::new("objective", payload.objective()),
        RenderField::new("source_candidate_count", payload.source_candidate_count()),
        RenderField::new(
            "reachable_candidate_count",
            payload.reachable_candidate_count(),
        ),
        RenderField::new("pattern_count", payload.pattern_count()),
        RenderField::new("covered_pattern_count", payload.covered_pattern_count()),
        RenderField::new("union_probability", payload.union_probability()),
        RenderField::new(
            "candidates",
            RenderFieldValue::array(payload.candidates().iter().map(|candidate| {
                RenderFieldValue::object([
                    (
                        "candidate_key",
                        RenderFieldValue::string(candidate.candidate_key()),
                    ),
                    (
                        "covered_pattern_count",
                        RenderFieldValue::string(candidate.covered_pattern_count()),
                    ),
                ])
            })),
        ),
        RenderField::new(
            "completeness",
            RenderFieldValue::object([
                (
                    "input_identity_bound",
                    RenderFieldValue::bool(completeness.input_identity_bound()),
                ),
                (
                    "producer_filter_bound",
                    RenderFieldValue::bool(completeness.producer_filter_bound()),
                ),
                (
                    "buildability_replay_complete",
                    RenderFieldValue::bool(completeness.buildability_replay_complete()),
                ),
                (
                    "coverage_rows_complete",
                    RenderFieldValue::bool(completeness.coverage_rows_complete()),
                ),
                (
                    "probability_weights_complete",
                    RenderFieldValue::bool(completeness.probability_weights_complete()),
                ),
            ]),
        ),
    ]
}

fn explicit_portfolio_field(portfolio: &ExplicitPortfolioOutput) -> RenderField {
    RenderField::new(
        "portfolio_alternative_page",
        RenderFieldValue::object([
            (
                "set_contract",
                RenderFieldValue::string(portfolio.set_contract()),
            ),
            (
                "page_contract",
                RenderFieldValue::string(portfolio.page_contract()),
            ),
            (
                "set_identity_sha256",
                RenderFieldValue::string(portfolio.set_identity_sha256()),
            ),
            (
                "candidate_map_sha256",
                RenderFieldValue::string(portfolio.candidate_map_sha256()),
            ),
            (
                "alternative_index",
                portfolio
                    .alternative_index_decimal()
                    .map(RenderFieldValue::string)
                    .unwrap_or(RenderFieldValue::Null),
            ),
            (
                "optimal_cardinality",
                RenderFieldValue::from(portfolio.optimal_cardinality()),
            ),
            (
                "members",
                RenderFieldValue::array(portfolio.members().iter().map(|member| {
                    RenderFieldValue::object([
                        (
                            "candidate_id",
                            RenderFieldValue::string(member.candidate_id_decimal()),
                        ),
                        (
                            "normalized_solution_key",
                            RenderFieldValue::string(member.normalized_key()),
                        ),
                    ])
                })),
            ),
            (
                "known_alternative_count",
                RenderFieldValue::string(portfolio.known_alternative_count_decimal()),
            ),
            (
                "total_alternative_count",
                portfolio
                    .total_alternative_count_decimal()
                    .map(RenderFieldValue::string)
                    .unwrap_or(RenderFieldValue::Null),
            ),
            (
                "enumeration_complete",
                RenderFieldValue::bool(portfolio.enumeration_complete()),
            ),
            (
                "tie_cursor",
                portfolio
                    .cursor()
                    .map(RenderFieldValue::string)
                    .unwrap_or(RenderFieldValue::Null),
            ),
        ]),
    )
}

fn append_pc_score_winner_family(
    fields: &mut Vec<RenderField>,
    product: Option<&ProductCapabilityResult>,
) -> Result<(), &'static str> {
    let report = product
        .filter(|product| {
            matches!(
                (product.contract(), product.result_kind()),
                (
                    ProductCapabilityContract::PcScore,
                    ProductCapabilityResultKind::PcScoreSummaryV2
                ) | (
                    ProductCapabilityContract::PcScoreFinder,
                    ProductCapabilityResultKind::PcFixedScoreWitnessV2
                )
            )
        })
        .and_then(ProductCapabilityResult::pc_score_summary_v2)
        .ok_or("pc score result did not include its typed winner-family report")?;
    let canonical_winner = report
        .canonical_winner()
        .ok_or("pc score-finder canonical winner was missing")?;
    fields.extend([
        RenderField::new(
            "score_pattern_winner_contract",
            PC_SCORE_PATTERN_WINNER_CONTRACT,
        ),
        RenderField::new(
            "score_pattern_winner_ordering",
            "pattern-id-ascending-then-candidate-id-ascending",
        ),
        RenderField::new(
            "score_pattern_winner_equality",
            "score-only-attack-informational",
        ),
        RenderField::new(
            "score_informational_attack_basis",
            PC_SCORE_INFORMATIONAL_ATTACK_BASIS,
        ),
        RenderField::new(
            "score_pattern_canonical_selection",
            report.canonical_selection(),
        ),
        RenderField::new(
            "score_pattern_canonical_winner",
            RenderFieldValue::object([
                (
                    "contract",
                    RenderFieldValue::string(PC_SCORE_PATTERN_WINNER_CONTRACT),
                ),
                (
                    "pattern_id",
                    RenderFieldValue::from(canonical_winner.pattern_id()),
                ),
                (
                    "candidate_id",
                    RenderFieldValue::string(canonical_winner.candidate_id().to_string()),
                ),
                (
                    "normalized_solution_key",
                    RenderFieldValue::string(
                        canonical_winner.normalized_solution_key().to_string(),
                    ),
                ),
                ("score", RenderFieldValue::from(canonical_winner.score())),
                (
                    "informational_attack",
                    RenderFieldValue::from(canonical_winner.informational_attack()),
                ),
                (
                    "informational_attack_basis",
                    RenderFieldValue::string(PC_SCORE_INFORMATIONAL_ATTACK_BASIS),
                ),
            ]),
        ),
        RenderField::new(
            "score_pattern_winners",
            RenderFieldValue::array(report.pattern_winners().iter().map(|winner| {
                RenderFieldValue::object([
                    ("contract", RenderFieldValue::string(winner.contract_id())),
                    ("pattern_id", RenderFieldValue::from(winner.pattern_id())),
                    (
                        "candidate_id",
                        RenderFieldValue::string(winner.candidate_id().to_string()),
                    ),
                    (
                        "normalized_solution_key",
                        RenderFieldValue::string(winner.normalized_solution_key().to_string()),
                    ),
                    ("score", RenderFieldValue::from(winner.score())),
                    (
                        "informational_attack",
                        RenderFieldValue::from(winner.informational_attack()),
                    ),
                    (
                        "informational_attack_basis",
                        RenderFieldValue::string(winner.informational_attack_basis()),
                    ),
                ])
            })),
        ),
    ]);
    Ok(())
}

fn public_success_render_kind(
    product_identity: Option<(ProductCapabilityContract, ProductCapabilityResultKind)>,
    app_result_kind: AppResultKind,
) -> &'static str {
    match (product_identity, app_result_kind) {
        (
            Some((
                ProductCapabilityContract::PcTiling,
                ProductCapabilityResultKind::PcTilingFamilyV1,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcTilingFamilyV1.as_str(),
        (
            Some((ProductCapabilityContract::PcSaves, ProductCapabilityResultKind::PcSaveGroupsV2)),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcSaveGroupsV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcBestSave,
                ProductCapabilityResultKind::PcBestSaveV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcBestSaveV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcMinimumCoverV2.as_str(),
        (
            Some((ProductCapabilityContract::PcPath, ProductCapabilityResultKind::PcPathFamilyV2)),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcPathFamilyV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcProbabilityV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcScore,
                ProductCapabilityResultKind::PcScoreSummaryV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcScoreSummaryV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcScoreFinder,
                ProductCapabilityResultKind::PcFixedScoreWitnessV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcFixedScoreWitnessV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcScoreMinimals,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            )),
            AppResultKind::Pc | AppResultKind::Scenario,
        ) => ProductCapabilityResultKind::PcScorePortfolioV2.as_str(),
        (
            Some((
                ProductCapabilityContract::PcFailedQueue,
                ProductCapabilityResultKind::PcFailedQueueV2,
            )),
            AppResultKind::Percent,
        ) => ProductCapabilityResultKind::PcFailedQueueV2.as_str(),
        _ => app_result_kind.as_str(),
    }
}

fn append_pc_score_minimals_fields(
    fields: &mut Vec<RenderField>,
    product: Option<&ProductCapabilityResult>,
) -> Result<(), &'static str> {
    let Some(product) = product else {
        return Ok(());
    };
    if (product.contract(), product.result_kind())
        != (
            ProductCapabilityContract::PcScoreMinimals,
            ProductCapabilityResultKind::PcScorePortfolioV2,
        )
    {
        return Ok(());
    }
    let report = product
        .pc_score_portfolio_v2()
        .ok_or("pc score-minimals result did not include its typed portfolio report")?;
    if !report.completeness().complete() {
        return Err("pc score-minimals typed portfolio report was incomplete");
    }
    let canonical_candidate_id = report.canonical_score_candidate_id();
    let canonical_solution_key = report.canonical_solution_key().to_string();
    if !report
        .selected_score_candidate_ids()
        .iter()
        .zip(report.selected_solution_keys())
        .any(|(candidate_id, solution_key)| {
            *candidate_id == canonical_candidate_id && solution_key == &canonical_solution_key
        })
    {
        return Err("pc score-minimals canonical candidate was outside the selected portfolio");
    }
    fields.extend([
        RenderField::new("score_minimals_contract", report.contract_id()),
        RenderField::new("score_minimals_score_equality", "score-only"),
        RenderField::new("score_minimals_attack_role", "informational-only"),
        RenderField::new(
            "score_minimals_canonical_selection",
            report.canonical_selection(),
        ),
        RenderField::new(
            "score_minimals_canonical_candidate_id",
            RenderFieldValue::string(canonical_candidate_id.to_string()),
        ),
        RenderField::new(
            "score_minimals_canonical_solution_key",
            canonical_solution_key,
        ),
    ]);
    Ok(())
}

fn append_pc_save_product_fields(
    fields: &mut Vec<RenderField>,
    product: Option<&ProductCapabilityResult>,
) -> Result<(), &'static str> {
    let Some(product) = product else {
        return Ok(());
    };
    match (product.contract(), product.result_kind()) {
        (ProductCapabilityContract::PcSaves, ProductCapabilityResultKind::PcSaveGroupsV2) => {
            let report = product
                .pc_save_groups_v2()
                .ok_or("pc saves result did not include its typed report")?;
            fields.extend([
                RenderField::new("save_contract", report.contract_id()),
                RenderField::new("save_origin", report.origin().as_str()),
                RenderField::new("save_problem_preset", report.problem_preset().as_str()),
                RenderField::new(
                    "save_materialized_pattern_count",
                    report.materialized_pattern_count(),
                ),
                RenderField::new(
                    "save_pc_success_pattern_count",
                    report.pc_success_pattern_count(),
                ),
                RenderField::new(
                    "save_pc_probability",
                    RenderFieldValue::number(report.pc_probability().decimal()),
                ),
                RenderField::new(
                    "save_completeness",
                    pc_save_completeness_value(report.completeness()),
                ),
                RenderField::new(
                    "save_groups",
                    RenderFieldValue::array(report.groups().iter().map(pc_save_group_value)),
                ),
            ]);
            Ok(())
        }
        (ProductCapabilityContract::PcBestSave, ProductCapabilityResultKind::PcBestSaveV2) => {
            let report = product
                .pc_best_save_v2()
                .ok_or("pc best-save result did not include its typed report")?;
            fields.extend([
                RenderField::new("best_save_contract", report.contract_id()),
                RenderField::new("best_save_schema", report.schema_id()),
                RenderField::new("best_save_probability_basis", report.probability_basis()),
                RenderField::new("best_save_origin", report.origin().as_str()),
                RenderField::new("best_save_problem_preset", report.problem_preset().as_str()),
                RenderField::new(
                    "best_save_materialized_pattern_count",
                    report.materialized_pattern_count(),
                ),
                RenderField::new(
                    "best_save_pc_success_pattern_count",
                    report.pc_success_pattern_count(),
                ),
                RenderField::new(
                    "best_save_pc_probability",
                    RenderFieldValue::number(report.pc_probability().decimal()),
                ),
                RenderField::new(
                    "best_save_completeness",
                    pc_save_completeness_value(report.completeness()),
                ),
                RenderField::new(
                    "best_save_canonical_selection",
                    report.canonical_selection(),
                ),
                RenderField::new(
                    "best_save_canonical_winner",
                    report
                        .canonical_winner()
                        .map_or(RenderFieldValue::Null, pc_best_save_winner_value),
                ),
                RenderField::new(
                    "best_save_winners",
                    RenderFieldValue::array(report.winners().iter().map(pc_best_save_winner_value)),
                ),
            ]);
            if report.canonical_selection() != PC_BEST_SAVE_CANONICAL_SELECTION
                || report.canonical_winner() != report.winners().first()
            {
                return Err("pc best-save canonical winner was missing or mismatched");
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn pc_best_save_winner_value(winner: &PcBestSaveWinnerV2) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "weighted_total",
            RenderFieldValue::from(winner.weighted_total()),
        ),
        (
            "balanced_jl_count",
            RenderFieldValue::from(winner.balanced_jl_count()),
        ),
        (
            "exact_group_probability",
            RenderFieldValue::number(winner.exact_group_probability().decimal()),
        ),
        ("group", pc_save_group_value(winner.group())),
    ])
}

fn pc_save_group_value(group: &PcSaveGroupV2) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "identity_contract",
            RenderFieldValue::string(group.identity_contract()),
        ),
        (
            "identity",
            RenderFieldValue::string(group.identity().canonical_id()),
        ),
        ("piece_multiset", pc_save_multiset_value(group.identity())),
        (
            "successful_pattern_count",
            RenderFieldValue::from(group.successful_pattern_count()),
        ),
        (
            "unconditional_probability",
            RenderFieldValue::number(group.unconditional_probability().decimal()),
        ),
        (
            "conditional_probability_given_pc",
            RenderFieldValue::number(group.conditional_probability_given_pc().decimal()),
        ),
        (
            "canonical_candidate_id",
            // Candidate ids are producer-owned u64 values.  Keep their JSON
            // transport exact for JavaScript consumers by using canonical
            // base-10 decimal strings rather than lossy JSON numbers.
            RenderFieldValue::string(group.canonical_candidate_id().to_string()),
        ),
        (
            "witnesses",
            RenderFieldValue::array(group.witnesses().iter().map(pc_save_witness_value)),
        ),
    ])
}

fn pc_save_multiset_value(multiset: &PcSavePieceMultiset) -> RenderFieldValue {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    RenderFieldValue::object([
        ("T", RenderFieldValue::from(multiset.count(PieceKind::T))),
        ("I", RenderFieldValue::from(multiset.count(PieceKind::I))),
        ("O", RenderFieldValue::from(multiset.count(PieceKind::O))),
        ("J", RenderFieldValue::from(multiset.count(PieceKind::J))),
        ("L", RenderFieldValue::from(multiset.count(PieceKind::L))),
        ("S", RenderFieldValue::from(multiset.count(PieceKind::S))),
        ("Z", RenderFieldValue::from(multiset.count(PieceKind::Z))),
        (
            "total_count",
            RenderFieldValue::from(multiset.total_count()),
        ),
    ])
}

fn pc_save_witness_value(witness: &PcSaveWitness) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "pattern_index",
            RenderFieldValue::from(witness.pattern_index()),
        ),
        (
            "candidate_id",
            RenderFieldValue::string(witness.candidate_id().to_string()),
        ),
        (
            "trace_identity",
            RenderFieldValue::string(witness.trace_identity()),
        ),
        (
            "source_cursor",
            RenderFieldValue::from(witness.source_cursor()),
        ),
        (
            "terminal_hold",
            witness
                .terminal_hold()
                .map_or(RenderFieldValue::Null, |piece| {
                    RenderFieldValue::string(piece.as_ascii().to_string())
                }),
        ),
        (
            "active_bag_remainder",
            pc_save_multiset_value(witness.active_bag_remainder()),
        ),
    ])
}

fn pc_save_completeness_value(completeness: PcSaveCompletenessEvidence) -> RenderFieldValue {
    RenderFieldValue::object([
        (
            "source_universe_complete",
            RenderFieldValue::bool(completeness.source_universe_complete()),
        ),
        (
            "fixed_bag_boundary_proven",
            RenderFieldValue::bool(completeness.fixed_bag_boundary_proven()),
        ),
        (
            "execution_batch_complete",
            RenderFieldValue::bool(completeness.execution_batch_complete()),
        ),
        (
            "pattern_weights_complete",
            RenderFieldValue::bool(completeness.pattern_weights_complete()),
        ),
        (
            "count_complete",
            RenderFieldValue::bool(completeness.count_complete()),
        ),
        (
            "probability_complete",
            RenderFieldValue::bool(completeness.probability_complete()),
        ),
        ("complete", RenderFieldValue::bool(completeness.complete())),
    ])
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

fn optional_bool_value(value: Option<bool>) -> RenderFieldValue {
    value.map_or(RenderFieldValue::Null, RenderFieldValue::bool)
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
        AppErrorCode::OperationSequenceInvalid => CliErrorCode::OperationSequenceInvalid,
        AppErrorCode::OperationSequenceCancelled => CliErrorCode::OperationSequenceCancelled,
        AppErrorCode::OperationSequenceTimedOut => CliErrorCode::OperationSequenceTimedOut,
        AppErrorCode::OperationSequenceIncomplete => CliErrorCode::OperationSequenceIncomplete,
        AppErrorCode::UtilityParityInvalid => CliErrorCode::UtilityParityInvalid,
        AppErrorCode::UtilityFumenInvalid => CliErrorCode::UtilityFumenInvalid,
        AppErrorCode::UtilityRenderInvalid => CliErrorCode::UtilityRenderInvalid,
        AppErrorCode::UtilityRenderLimitExceeded => CliErrorCode::UtilityRenderLimitExceeded,
        AppErrorCode::UtilityToGrayInvalid => CliErrorCode::UtilityToGrayInvalid,
        AppErrorCode::UtilityMirrorInvalid => CliErrorCode::UtilityMirrorInvalid,
        _ => default_error,
    }
}

#[cfg(test)]
#[path = "app_response_renderer_tests.rs"]
mod tests;
