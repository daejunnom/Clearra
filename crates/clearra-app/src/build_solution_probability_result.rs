// SRP rationale: this module has one behavior-level change reason: defining Build probability result contracts and their application-boundary projections.

//! SRP rationale: this module owns the App boundary contract for Build's
//! optional per-solution probability projection.

pub(crate) mod build_v2_colored_result;
pub(crate) mod build_v2_contract;
pub(crate) mod build_v2_facade;
pub(crate) mod build_v2_options;
pub(crate) mod build_v2_result;
pub(crate) mod build_v2_supplied_result;

use core::fmt::{self, Write as _};

use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::pattern::weighted_pattern_set::covered_weight_in_pattern_order;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    BuildSolutionProbabilityPolicy, FinessePatternKnowledge, FinesseScoreRequest,
};

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    render::AppRenderModel,
};

const REQUESTED_FIELD: &str = "solution_probabilities_requested";
const COUNT_FIELD: &str = "solution_probability_count";
const COMPLETE_FIELD: &str = "solution_probability_complete";
const BASIS_FIELD: &str = "solution_probability_basis";
const INCOMPLETE_REASON_FIELD: &str = "solution_probability_incomplete_reason";
const INCLUDE_BASIS: &str = "normalized-solution-pattern-bitset-or-union";
const RESOURCE_INCOMPLETE_REASON: &str = "resource-truncated";
const COUNT_INCOMPLETE_REASON: &str = "solution-count-incomplete";
const KEYS_INCOMPLETE_REASON: &str = "normalized-solution-set-incomplete";
const COVERAGE_INCOMPLETE_REASON: &str = "pattern-specific-coverage-incomplete";
const SEARCH_KIND_FIELD: &str = "search_kind";
const BUILD_PROBABILITY_SEARCH_KIND: &str = "build-probability";
const FINESSE_SCORE_SEARCH_KIND: &str = "finesse-score";
const FINESSE_METRIC_FIELD: &str = "finesse_metric_requested";
const FINESSE_KNOWLEDGE_FIELD: &str = "finesse_pattern_knowledge_requested";
const SCORE_SUMMARY_FIELDS: [&str; 9] = [
    SEARCH_KIND_FIELD,
    "objective",
    FINESSE_METRIC_FIELD,
    FINESSE_KNOWLEDGE_FIELD,
    "materialized_pattern_count",
    "unique_queue_count",
    "objective_complete",
    "finesse_initial_board_words",
    "finesse_height",
];

#[cfg(test)]
pub(crate) fn build_probability_resource_test_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::execution_resource_test_support::execution_resource_test_guard()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildSolutionProbabilityInputState {
    pub(crate) requested: bool,
    pub(crate) complete: bool,
    pub(crate) count_complete: bool,
    pub(crate) probability_complete: bool,
    pub(crate) solution_keys_complete: bool,
    pub(crate) resource_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildSolutionProbabilityResultError {
    MissingOrDuplicateField(&'static str),
    InvalidBooleanField(&'static str),
    InvalidCountField(&'static str),
    UnexpectedWorkerPartialField(&'static str),
    WorkerPartialAlreadyMaterialized,
    WorkerPartialReportsPresent,
    RequestPolicyMismatch,
    OmittedContractMismatch,
    IncludeBasisMismatch,
    ProbabilityCountMismatch,
    SolutionKeysNotCanonical,
    SolutionKeyCountMismatch,
    CoverageKeysMismatch,
    CoveragePatternCountMismatch,
    CoverageUnionMismatch,
    PatternDenominatorEmpty,
    ReportKeysMismatch,
    ReportPatternCountMismatch,
    ReportCoveredCountInvalid,
    ReportCoveredCountMismatch,
    ReportProbabilityInvalid,
    ReportCompletenessMismatch,
    PatternWeightAuthorityInvalid(&'static str),
    ReportVectorMismatch,
    CompletenessMismatch,
    IncompleteReasonMismatch,
    ResultKindMismatch,
    UnexpectedFinesseField(&'static str),
    FinesseMetadataMismatch(&'static str),
    FinesseReportMissing,
    FinesseReportMetadataMismatch,
    FinessePolicyListMismatch,
    FinesseScoreSummaryFieldSetMismatch,
    FinesseScoreRequestAuthorityMismatch,
    FinesseScoreReportContractMismatch(&'static str),
    UnexpectedGenericSolutionAuthority,
    BuildQuerySurfaceMismatch(&'static str),
}

impl fmt::Display for BuildSolutionProbabilityResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOrDuplicateField(field) => {
                write!(
                    formatter,
                    "required field `{field}` does not occur exactly once"
                )
            }
            Self::InvalidBooleanField(field) => {
                write!(
                    formatter,
                    "required field `{field}` is not a canonical boolean"
                )
            }
            Self::InvalidCountField(field) => {
                write!(
                    formatter,
                    "required field `{field}` is not a canonical count"
                )
            }
            Self::UnexpectedWorkerPartialField(field) => write!(
                formatter,
                "final-only field `{field}` is present in a raw Build worker partial"
            ),
            Self::WorkerPartialAlreadyMaterialized => formatter.write_str(
                "raw Build worker partial claims that execution constraints are already materialized",
            ),
            Self::WorkerPartialReportsPresent => formatter.write_str(
                "raw Build worker partial unexpectedly contains final probability reports",
            ),
            Self::RequestPolicyMismatch => {
                formatter.write_str("result request marker does not match the Build query")
            }
            Self::OmittedContractMismatch => {
                formatter.write_str("omitted per-solution probability contract is inconsistent")
            }
            Self::IncludeBasisMismatch => {
                formatter.write_str("included per-solution probability basis is invalid")
            }
            Self::ProbabilityCountMismatch => {
                formatter.write_str("solution probability count does not match the report array")
            }
            Self::SolutionKeysNotCanonical => {
                formatter.write_str("normalized solution keys are not in strict canonical order")
            }
            Self::SolutionKeyCountMismatch => {
                formatter.write_str("materialized normalized solution key count is inconsistent")
            }
            Self::CoverageKeysMismatch => formatter.write_str(
                "normalized solution coverage rows do not match the canonical solution keys",
            ),
            Self::CoveragePatternCountMismatch => formatter.write_str(
                "normalized solution coverage does not use the authoritative pattern denominator",
            ),
            Self::CoverageUnionMismatch => formatter.write_str(
                "normalized solution coverage union does not match the global coverage bitset",
            ),
            Self::PatternDenominatorEmpty => {
                formatter.write_str("solution probability pattern denominator is empty")
            }
            Self::ReportKeysMismatch => formatter
                .write_str("solution probability reports do not match the canonical solution keys"),
            Self::ReportPatternCountMismatch => formatter.write_str(
                "solution probability report does not use the authoritative pattern denominator",
            ),
            Self::ReportCoveredCountInvalid => formatter
                .write_str("solution probability report covers more patterns than its denominator"),
            Self::ReportCoveredCountMismatch => formatter.write_str(
                "solution probability report covered count does not match its coverage bitset",
            ),
            Self::ReportProbabilityInvalid => {
                formatter.write_str("solution probability report value is invalid")
            }
            Self::ReportCompletenessMismatch => formatter.write_str(
                "solution probability report completeness disagrees with its result metadata",
            ),
            Self::PatternWeightAuthorityInvalid(reason) => write!(
                formatter,
                "solution probability pattern weight authority is invalid: {reason}"
            ),
            Self::ReportVectorMismatch => formatter.write_str(
                "solution probability reports do not exactly match the authoritative reconstruction",
            ),
            Self::CompletenessMismatch => formatter.write_str(
                "solution probability completeness disagrees with its authoritative inputs",
            ),
            Self::IncompleteReasonMismatch => formatter.write_str(
                "solution probability incomplete reason does not match the actual cause",
            ),
            Self::ResultKindMismatch => {
                formatter.write_str("result kind does not match the Build query")
            }
            Self::UnexpectedFinesseField(field) => {
                write!(formatter, "unexpected finesse field `{field}` is present")
            }
            Self::FinesseMetadataMismatch(field) => {
                write!(formatter, "finesse field `{field}` does not match the Build query")
            }
            Self::FinesseReportMissing => {
                formatter.write_str("required typed finesse report is missing")
            }
            Self::FinesseReportMetadataMismatch => formatter
                .write_str("typed finesse report metadata does not match the Build query"),
            Self::FinessePolicyListMismatch => formatter
                .write_str("typed finesse report policy list does not match the Build query"),
            Self::FinesseScoreSummaryFieldSetMismatch => formatter.write_str(
                "finesse score summary fields do not exactly match the canonical producer set",
            ),
            Self::FinesseScoreRequestAuthorityMismatch => formatter.write_str(
                "finesse score result does not match its retained query-owned request authority",
            ),
            Self::FinesseScoreReportContractMismatch(reason) => write!(
                formatter,
                "typed finesse score report is not producer-realizable: {reason}"
            ),
            Self::UnexpectedGenericSolutionAuthority => formatter.write_str(
                "finesse score result unexpectedly retains generic solution authority",
            ),
            Self::BuildQuerySurfaceMismatch(field) => write!(
                formatter,
                "Build field `{field}` does not match the retained query surface"
            ),
        }
    }
}

impl BuildSolutionProbabilityResultError {
    pub(crate) fn input_component(&self) -> &'static str {
        match self {
            Self::MissingOrDuplicateField(REQUESTED_FIELD) => {
                "build_solution_probability_input_requested_field_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField(COUNT_FIELD) => {
                "build_solution_probability_input_report_count_field_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField(COMPLETE_FIELD) => {
                "build_solution_probability_input_complete_field_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField(BASIS_FIELD) => {
                "build_solution_probability_input_basis_field_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField(INCOMPLETE_REASON_FIELD) => {
                "build_solution_probability_input_reason_field_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("count_complete") => {
                "build_solution_probability_input_count_complete_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("probability_complete") => {
                "build_solution_probability_input_probability_complete_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("solution_keys_complete") => {
                "build_solution_probability_input_keys_complete_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("resource_truncated") => {
                "build_solution_probability_input_resource_truncated_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("unique_solution_count") => {
                "build_solution_probability_worker_partial_solution_count_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("coverage_pattern_count") => {
                "build_solution_probability_input_pattern_count_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField("solution_keys_materialized_count") => {
                "build_solution_probability_input_materialized_key_count_missing_or_duplicate"
            }
            Self::MissingOrDuplicateField(_) => {
                "build_solution_probability_input_field_missing_or_duplicate"
            }
            Self::InvalidBooleanField(_) => {
                "build_solution_probability_input_boolean_field_invalid"
            }
            Self::InvalidCountField(_) => "build_solution_probability_input_count_field_invalid",
            Self::UnexpectedWorkerPartialField(_) => {
                "build_solution_probability_worker_partial_final_field_present"
            }
            Self::WorkerPartialAlreadyMaterialized => {
                "build_solution_probability_worker_partial_already_materialized"
            }
            Self::WorkerPartialReportsPresent => {
                "build_solution_probability_worker_partial_reports_present"
            }
            Self::RequestPolicyMismatch => "build_solution_probability_input_policy_mismatch",
            Self::OmittedContractMismatch => {
                "build_solution_probability_input_omit_contract_invalid"
            }
            Self::IncludeBasisMismatch => "build_solution_probability_input_basis_invalid",
            Self::ProbabilityCountMismatch => {
                "build_solution_probability_input_report_count_mismatch"
            }
            Self::CompletenessMismatch => "build_solution_probability_input_completeness_mismatch",
            Self::IncompleteReasonMismatch => {
                "build_solution_probability_input_incomplete_reason_mismatch"
            }
            _ => "build_solution_probability_input_contract_invalid",
        }
    }
}

pub(crate) fn declared_build_solution_probability_policy(
    result: &CoreExecutionResult,
) -> Result<BuildSolutionProbabilityPolicy, BuildSolutionProbabilityResultError> {
    Ok(if required_bool(result, REQUESTED_FIELD)? {
        BuildSolutionProbabilityPolicy::Include
    } else {
        BuildSolutionProbabilityPolicy::Omit
    })
}

pub(crate) const fn build_solution_probability_incomplete_reason(
    requested: bool,
    complete: bool,
    resource_truncated: bool,
    count_complete: bool,
    solution_keys_complete: bool,
) -> &'static str {
    if !requested || complete {
        "none"
    } else if resource_truncated {
        RESOURCE_INCOMPLETE_REASON
    } else if !count_complete {
        COUNT_INCOMPLETE_REASON
    } else if !solution_keys_complete {
        KEYS_INCOMPLETE_REASON
    } else {
        COVERAGE_INCOMPLETE_REASON
    }
}

pub(crate) fn build_probability_response(
    expected_finesse: &BuildProbabilityFinesseRequest,
    expected_field: BuildProbabilityField,
    expected_aggregation: BuildProbabilityAggregation,
    expected_policy: BuildSolutionProbabilityPolicy,
    result: CoreExecutionResult,
) -> AppResponse {
    let validation = validate_build_probability_response(
        expected_finesse,
        expected_field,
        expected_aggregation,
        expected_policy,
        &result,
    );
    match validation {
        Ok(()) => AppResponse::success(AppRenderModel::BuildProbability(result)),
        Err(error) => AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("build solution probability result rejected: {error}"),
            ),
        ),
    }
}

/// Performs the same query-owned Build result authorization as
/// [`build_probability_response`] without constructing either a success or a
/// rich rejection response. The distributed finite-memory boundary uses this
/// before allocating Host response fields, so a malformed result can fail
/// closed with a static Core error.
pub(crate) fn build_probability_response_is_authorized(
    expected_finesse: &BuildProbabilityFinesseRequest,
    expected_field: BuildProbabilityField,
    expected_aggregation: BuildProbabilityAggregation,
    expected_policy: BuildSolutionProbabilityPolicy,
    result: &CoreExecutionResult,
) -> bool {
    validate_build_probability_response(
        expected_finesse,
        expected_field,
        expected_aggregation,
        expected_policy,
        result,
    )
    .is_ok()
}

fn validate_build_probability_response(
    expected_finesse: &BuildProbabilityFinesseRequest,
    expected_field: BuildProbabilityField,
    expected_aggregation: BuildProbabilityAggregation,
    expected_policy: BuildSolutionProbabilityPolicy,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    match expected_finesse {
        BuildProbabilityFinesseRequest::Off => {
            require_result_kind(result, BUILD_PROBABILITY_SEARCH_KIND)?;
            require_exact_field(result, "objective", BUILD_PROBABILITY_SEARCH_KIND)?;
            validate_build_query_surface(expected_field, expected_aggregation, result)?;
            validate_build_solution_probability_result(expected_policy, result)?;
            validate_finesse_absent(result)
        }
        BuildProbabilityFinesseRequest::Search { pattern_knowledge } => {
            require_result_kind(result, BUILD_PROBABILITY_SEARCH_KIND)?;
            require_exact_field(result, "objective", BUILD_PROBABILITY_SEARCH_KIND)?;
            validate_build_query_surface(expected_field, expected_aggregation, result)?;
            validate_build_solution_probability_result(expected_policy, result)?;
            validate_finesse_search(*pattern_knowledge, result)
        }
        BuildProbabilityFinesseRequest::Score {
            pattern_knowledge,
            request,
        } => {
            require_result_kind(result, FINESSE_SCORE_SEARCH_KIND)?;
            validate_finesse_score(
                *pattern_knowledge,
                request,
                expected_field,
                expected_policy,
                result,
            )
        }
    }
}

fn validate_build_query_surface(
    expected_field: BuildProbabilityField,
    expected_aggregation: BuildProbabilityAggregation,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    require_exact_build_query_field(
        result,
        "build_probability_aggregation",
        expected_aggregation.as_str(),
    )?;
    require_exact_build_query_field(
        result,
        "spin_profile_requested",
        expected_aggregation
            .spin_profile()
            .map_or("none", |profile| profile.as_str()),
    )?;
    if required_bool(result, "postprocess_build_spin_requested")?
        != expected_aggregation.requests_spin_coverage()
    {
        return Err(
            BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(
                "postprocess_build_spin_requested",
            ),
        );
    }

    let base_words = expected_field.base_words();
    let target_words = expected_field.target_words();
    let target_board_words = [
        base_words[0] | target_words[0],
        base_words[1] | target_words[1],
        base_words[2] | target_words[2],
        base_words[3] | target_words[3],
    ];
    if expected_field.is_compact() {
        if result.field_occurrence_count("board_storage") != 0 {
            return Err(
                BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch("board_storage"),
            );
        }
        if required_usize(result, "board_height")? != usize::from(expected_field.height()) {
            return Err(
                BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch("board_height"),
            );
        }
        require_exact_display_build_query_field(result, "build_base_mask", base_words[0])?;
        require_exact_display_build_query_field(
            result,
            "build_target_cells_mask",
            target_words[0],
        )?;
        require_exact_display_build_query_field(
            result,
            "build_target_board_mask",
            target_board_words[0],
        )?;
    } else {
        require_exact_build_query_field(result, "board_storage", "board256-canonical")?;
        if required_usize(result, "board_height")? != usize::from(expected_field.height()) {
            return Err(
                BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch("board_height"),
            );
        }
        require_exact_words_build_query_field(result, "build_base_mask", base_words)?;
        require_exact_words_build_query_field(result, "build_target_cells_mask", target_words)?;
        require_exact_words_build_query_field(
            result,
            "build_target_board_mask",
            target_board_words,
        )?;
    }

    if required_usize(result, "target_piece_count")? != expected_field.target_piece_count() {
        return Err(
            BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch("target_piece_count"),
        );
    }
    let mirror_included = expected_field.includes_applicable_horizontal_mirror();
    let original = expected_field.original_only();
    let mirror_distinct = mirror_included && original.mirrored_horizontally() != original;
    require_exact_build_query_field(
        result,
        "build_symmetry_policy",
        if mirror_included {
            "original-or-horizontal-mirror"
        } else {
            "original-only"
        },
    )?;
    for (field, expected) in [
        ("build_mirror_included", mirror_included),
        ("build_mirror_distinct_target", mirror_distinct),
        ("build_mirror_search_executed", mirror_distinct),
    ] {
        if required_bool(result, field)? != expected {
            return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
        }
    }
    Ok(())
}

fn require_exact_build_query_field(
    result: &CoreExecutionResult,
    field: &'static str,
    expected: &str,
) -> Result<(), BuildSolutionProbabilityResultError> {
    if required_field(result, field)? != expected {
        return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
    }
    Ok(())
}

fn require_exact_display_build_query_field(
    result: &CoreExecutionResult,
    field: &'static str,
    expected: impl fmt::Display,
) -> Result<(), BuildSolutionProbabilityResultError> {
    let actual = required_field(result, field)?;
    let mut writer = ExactTextWriter::new(actual);
    if write!(&mut writer, "{expected}").is_err() || !writer.finish() {
        return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
    }
    Ok(())
}

fn require_exact_words_build_query_field(
    result: &CoreExecutionResult,
    field: &'static str,
    expected: [u64; 4],
) -> Result<(), BuildSolutionProbabilityResultError> {
    let actual = required_field(result, field)?;
    let highest = expected.iter().rposition(|word| *word != 0).unwrap_or(0);
    let mut writer = ExactTextWriter::new(actual);
    if write!(&mut writer, "0x{:x}", expected[highest]).is_err() {
        return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
    }
    for word in expected[..highest].iter().rev() {
        if write!(&mut writer, "{word:016x}").is_err() {
            return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
        }
    }
    if !writer.finish() {
        return Err(BuildSolutionProbabilityResultError::BuildQuerySurfaceMismatch(field));
    }
    Ok(())
}

fn validate_finesse_score(
    expected_knowledge: FinessePatternKnowledge,
    expected_request: &FinesseScoreRequest,
    expected_field: BuildProbabilityField,
    expected_policy: BuildSolutionProbabilityPolicy,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    if expected_policy != BuildSolutionProbabilityPolicy::Omit {
        return Err(BuildSolutionProbabilityResultError::RequestPolicyMismatch);
    }
    if result.summary_field_count() != SCORE_SUMMARY_FIELDS.len()
        || result
            .summary_field_entries()
            .any(|(key, _)| !SCORE_SUMMARY_FIELDS.contains(&key))
    {
        return Err(BuildSolutionProbabilityResultError::FinesseScoreSummaryFieldSetMismatch);
    }

    require_exact_field(result, "objective", "finesse")?;
    require_exact_field(result, FINESSE_METRIC_FIELD, "inputs")?;
    require_exact_field(result, FINESSE_KNOWLEDGE_FIELD, expected_knowledge.as_str())?;
    let materialized_pattern_count = required_usize(result, "materialized_pattern_count")?;
    if materialized_pattern_count == 0 {
        return Err(
            BuildSolutionProbabilityResultError::FinesseMetadataMismatch(
                "materialized_pattern_count",
            ),
        );
    }
    let unique_queue_count = required_usize(result, "unique_queue_count")?;
    if unique_queue_count == 0 {
        return Err(
            BuildSolutionProbabilityResultError::FinesseMetadataMismatch("unique_queue_count"),
        );
    }
    if required_usize(result, "finesse_height")? != usize::from(expected_field.height()) {
        return Err(BuildSolutionProbabilityResultError::FinesseMetadataMismatch("finesse_height"));
    }
    if !canonical_board_words_match(
        required_field(result, "finesse_initial_board_words")?,
        expected_field.base_words(),
    ) {
        return Err(
            BuildSolutionProbabilityResultError::FinesseMetadataMismatch(
                "finesse_initial_board_words",
            ),
        );
    }
    let objective_complete = required_bool(result, "objective_complete")?;
    let report = result
        .finesse_report()
        .ok_or(BuildSolutionProbabilityResultError::FinesseReportMissing)?;
    validate_finesse_report_metadata(
        report,
        "score",
        expected_knowledge,
        Some(objective_complete),
    )?;
    validate_finesse_score_report(
        report,
        expected_knowledge,
        expected_request,
        expected_field,
        materialized_pattern_count,
        unique_queue_count,
        result,
    )?;

    if retains_generic_solution_authority(result) {
        return Err(BuildSolutionProbabilityResultError::UnexpectedGenericSolutionAuthority);
    }
    Ok(())
}

fn validate_finesse_search(
    expected_knowledge: FinessePatternKnowledge,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    require_exact_field(result, FINESSE_METRIC_FIELD, "inputs")?;
    require_exact_field(result, FINESSE_KNOWLEDGE_FIELD, expected_knowledge.as_str())?;
    for score_only_field in ["finesse_initial_board_words", "finesse_height"] {
        if result.field_occurrence_count(score_only_field) != 0 {
            return Err(BuildSolutionProbabilityResultError::UnexpectedFinesseField(
                score_only_field,
            ));
        }
    }
    let report = result
        .finesse_report()
        .ok_or(BuildSolutionProbabilityResultError::FinesseReportMissing)?;
    validate_finesse_report_metadata(report, "search", expected_knowledge, None)
}

fn validate_finesse_absent(
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    for field in [
        FINESSE_METRIC_FIELD,
        FINESSE_KNOWLEDGE_FIELD,
        "finesse_initial_board_words",
        "finesse_height",
    ] {
        if result.field_occurrence_count(field) != 0 {
            return Err(BuildSolutionProbabilityResultError::UnexpectedFinesseField(
                field,
            ));
        }
    }
    if result.finesse_report().is_some() {
        return Err(BuildSolutionProbabilityResultError::FinesseReportMetadataMismatch);
    }
    Ok(())
}

fn validate_finesse_report_metadata(
    report: &clearra_core_executor::FinesseReport,
    expected_mode: &str,
    expected_knowledge: FinessePatternKnowledge,
    expected_complete: Option<bool>,
) -> Result<(), BuildSolutionProbabilityResultError> {
    if report.mode() != expected_mode
        || report.metric() != "inputs"
        || report.pattern_knowledge() != expected_knowledge.as_str()
        || expected_complete.is_some_and(|complete| report.complete() != complete)
        || report
            .policy_results()
            .iter()
            .any(|policy| policy.complete() != report.complete())
    {
        return Err(BuildSolutionProbabilityResultError::FinesseReportMetadataMismatch);
    }
    let expected_policies: &[&str] = match expected_knowledge {
        FinessePatternKnowledge::Both => &["oracle", "visible-7"],
        FinessePatternKnowledge::Oracle => &["oracle"],
        FinessePatternKnowledge::VisibleSeven => &["visible-7"],
    };
    if report.policy_results().len() != expected_policies.len()
        || report
            .policy_results()
            .iter()
            .zip(expected_policies)
            .any(|(actual, expected)| actual.policy() != *expected)
    {
        return Err(BuildSolutionProbabilityResultError::FinessePolicyListMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_finesse_score_report(
    report: &clearra_core_executor::FinesseReport,
    expected_knowledge: FinessePatternKnowledge,
    expected_request: &FinesseScoreRequest,
    expected_field: BuildProbabilityField,
    materialized_pattern_count: usize,
    unique_queue_count: usize,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    let mut any_policy_succeeds = false;
    for policy in report.policy_results() {
        let successful_count = policy.successful_unique_queue_count().ok_or(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "successful queue count is missing",
            ),
        )?;
        let total_count = policy.total_unique_queue_count().ok_or(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "total queue count is missing",
            ),
        )?;
        let successful_mass = policy.successful_probability_mass().ok_or(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "successful probability mass is missing",
            ),
        )?;
        if total_count != unique_queue_count || successful_count > total_count {
            return Err(
                BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                    "success counts disagree with the top-level queue denominator",
                ),
            );
        }
        if canonical_finesse_number(successful_mass, true).is_none() {
            return Err(
                BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                    "successful probability mass is not canonical",
                ),
            );
        }
        let averages = policy.solution_averages();
        if averages.len() != 1
            || averages[0].solution_key() != "given-operation-sequence"
            || averages[0].complete() != policy.complete()
            || averages[0].average_inputs() != policy.overall_average_inputs()
        {
            return Err(
                BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                    "solution average is not the canonical linked singleton",
                ),
            );
        }
        if successful_count == 0 {
            if policy.overall_average_inputs() != "unavailable" {
                return Err(
                    BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                        "failed policy exposes an average",
                    ),
                );
            }
        } else {
            any_policy_succeeds = true;
            if canonical_finesse_number(policy.overall_average_inputs(), false).is_none() {
                return Err(
                    BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                        "successful policy average is not canonical",
                    ),
                );
            }
        }

        match policy.policy() {
            "oracle" => {
                if policy.oracle_on_covered_average_inputs().is_some()
                    || policy.information_penalty_inputs().is_some()
                    || policy.success_probability_gap().is_some()
                {
                    return Err(
                        BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                            "oracle policy retains visible-policy comparison fields",
                        ),
                    );
                }
            }
            "visible-7" => {
                if policy
                    .success_probability_gap()
                    .and_then(|value| canonical_finesse_number(value, true))
                    .is_none()
                {
                    return Err(
                        BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                            "visible policy success gap is missing or noncanonical",
                        ),
                    );
                }
                let oracle_average = policy.oracle_on_covered_average_inputs();
                let information_penalty = policy.information_penalty_inputs();
                if (oracle_average.is_some() != (successful_count != 0))
                    || (information_penalty.is_some() != (successful_count != 0))
                    || oracle_average
                        .is_some_and(|value| canonical_finesse_number(value, false).is_none())
                    || information_penalty
                        .is_some_and(|value| canonical_finesse_number(value, false).is_none())
                {
                    return Err(
                        BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                            "visible policy comparison fields do not match its success state",
                        ),
                    );
                }
            }
            _ => unreachable!("metadata validation already fixed the policy list"),
        }
    }

    if !any_policy_succeeds {
        if report.representative_witness().is_some() || report.exact_total_inputs().is_some() {
            return Err(
                BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                    "all-failure result retains a witness or exact cost",
                ),
            );
        }
        if !report.matches_score_request_authority(
            expected_field,
            expected_request,
            materialized_pattern_count,
            result.path_steps(),
            false,
        ) {
            return Err(BuildSolutionProbabilityResultError::FinesseScoreRequestAuthorityMismatch);
        }
        return Ok(());
    }

    let selected_policy = match expected_knowledge {
        FinessePatternKnowledge::Both | FinessePatternKnowledge::Oracle => "oracle",
        FinessePatternKnowledge::VisibleSeven => "visible-7",
    };
    if !report.policy_results().iter().any(|policy| {
        policy.policy() == selected_policy
            && policy
                .successful_unique_queue_count()
                .is_some_and(|count| count != 0)
    }) {
        return Err(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "selected witness policy has no successful queue",
            ),
        );
    }
    let witness = report.representative_witness().ok_or(
        BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
            "successful result is missing its representative witness",
        ),
    )?;
    if witness.policy() != selected_policy
        || witness.solution_key() != Some("given-operation-sequence")
        || witness.pattern_ids().is_empty()
        || witness
            .pattern_ids()
            .iter()
            .any(|pattern| *pattern >= materialized_pattern_count)
        || !witness
            .pattern_ids()
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || usize::try_from(witness.total_inputs()).ok() != Some(witness.input_sequence().len())
    {
        return Err(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "representative witness metadata or cost is invalid",
            ),
        );
    }
    if witness.placements().len() != result.path_steps().len()
        || witness
            .placements()
            .iter()
            .zip(result.path_steps())
            .any(|(placement, step)| {
                placement.piece() != step.piece()
                    || placement.rotation().quarter_turns() != step.rotation()
                    || i32::from(placement.x()) != step.x()
                    || i32::from(placement.y()) != step.y()
                    || step.hold() != "none"
            })
    {
        return Err(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "representative witness placements disagree with the score path",
            ),
        );
    }
    if report.exact_total_inputs().and_then(parse_canonical_u32) != Some(witness.total_inputs()) {
        return Err(
            BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(
                "exact total input cost is missing or disagrees with the representative witness",
            ),
        );
    }
    if !report.matches_score_request_authority(
        expected_field,
        expected_request,
        materialized_pattern_count,
        result.path_steps(),
        true,
    ) {
        return Err(BuildSolutionProbabilityResultError::FinesseScoreRequestAuthorityMismatch);
    }
    Ok(())
}

fn canonical_finesse_number(value: &str, at_most_one: bool) -> Option<f64> {
    let (integer, fractional) = match value.split_once('.') {
        Some((integer, fractional)) => (integer, Some(fractional)),
        None => (value, None),
    };
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || fractional.is_some_and(|fractional| {
            fractional.is_empty()
                || fractional.len() > 6
                || !fractional.bytes().all(|byte| byte.is_ascii_digit())
                || fractional.ends_with('0')
        })
        || value.matches('.').count() > 1
    {
        return None;
    }
    let parsed = value.parse::<f64>().ok()?;
    if !parsed.is_finite()
        || parsed < 0.0
        || (parsed == 0.0 && value != "0")
        || (at_most_one && parsed > 1.0)
    {
        return None;
    }
    Some(parsed)
}

fn parse_canonical_u32(value: &str) -> Option<u32> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse::<u32>().ok()
}

fn canonical_board_words_match(value: &str, expected_words: [u64; 4]) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 66 || !bytes.starts_with(b"0x") {
        return false;
    }
    for output_word_index in 0..4 {
        let mut parsed = 0_u64;
        for byte in &bytes[2 + output_word_index * 16..2 + (output_word_index + 1) * 16] {
            let nibble = match *byte {
                value @ b'0'..=b'9' => u64::from(value - b'0'),
                value @ b'a'..=b'f' => u64::from(value - b'a' + 10),
                _ => return false,
            };
            parsed = (parsed << 4) | nibble;
        }
        if parsed != expected_words[3 - output_word_index] {
            return false;
        }
    }
    true
}

fn require_result_kind(
    result: &CoreExecutionResult,
    expected: &'static str,
) -> Result<(), BuildSolutionProbabilityResultError> {
    if required_field(result, SEARCH_KIND_FIELD)? != expected {
        return Err(BuildSolutionProbabilityResultError::ResultKindMismatch);
    }
    Ok(())
}

fn require_exact_field(
    result: &CoreExecutionResult,
    field: &'static str,
    expected: &str,
) -> Result<(), BuildSolutionProbabilityResultError> {
    if required_field(result, field)? != expected {
        return Err(BuildSolutionProbabilityResultError::FinesseMetadataMismatch(field));
    }
    Ok(())
}

fn retains_generic_solution_authority(result: &CoreExecutionResult) -> bool {
    result.postprocess_replay_trace().is_some()
        || !result.postprocess_executions().is_empty()
        || result.postprocess_execution_complete()
        || !result.postprocess_pattern_weights().is_empty()
        || !result.packing_candidate_keys().is_empty()
        || !result.normalized_solution_keys().is_empty()
        || !result.normalized_solution_identities().is_empty()
        || result.representative_solution_identity().is_some()
        || !result.coverage_pattern_words().is_empty()
        || result.pc_chance_coverage_evidence().is_some()
        || result.pc_score_problem_evidence().is_some()
        || !result.solution_coverages().is_empty()
        || !result.normalized_solution_coverages().is_empty()
        || !result.solution_probabilities().is_empty()
        || !result.solution_average_scores().is_empty()
        || !result.exact_scoring_execution_batches().is_empty()
        || !result.spin_coverage_execution_batches().is_empty()
        || !result.postprocess_score_cells().is_empty()
        || result.postprocess_score_cells_complete()
        || result.postprocess_score_profile_id().is_some()
        || !result.postprocess_spin_coverages().is_empty()
        || result.setup_finder_report().is_some()
        || result.tiling_solution_page_store().is_some()
        || result.pc_tiling_memory_admission_evidence().is_some()
        || result
            .pre_b2b_produced_solution_audit_checkpoint()
            .is_some()
        || result.pre_b2b_solution_audit_checkpoint().is_some()
        || result.solution_set_audit_report().is_some()
}

pub(crate) fn validate_build_solution_probability_result(
    expected_policy: BuildSolutionProbabilityPolicy,
    result: &CoreExecutionResult,
) -> Result<(), BuildSolutionProbabilityResultError> {
    let input = validate_build_solution_probability_reducer_input(Some(expected_policy), result)?;

    if !input.requested {
        return Ok(());
    }

    let pattern_count = required_usize(result, "coverage_pattern_count")?;
    if pattern_count == 0 {
        return Err(BuildSolutionProbabilityResultError::PatternDenominatorEmpty);
    }
    let materialized_key_count = required_usize(result, "solution_keys_materialized_count")?;
    let keys = result.normalized_solution_keys();

    if keys.iter().any(String::is_empty) || !keys.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(BuildSolutionProbabilityResultError::SolutionKeysNotCanonical);
    }
    if materialized_key_count != keys.len() {
        return Err(BuildSolutionProbabilityResultError::SolutionKeyCountMismatch);
    }
    if input.solution_keys_complete {
        let solution_count = required_usize(result, "unique_solution_count")?;
        if solution_count != keys.len() {
            return Err(BuildSolutionProbabilityResultError::SolutionKeyCountMismatch);
        }
    }

    let coverage = result.normalized_solution_coverages();
    if coverage.len() != keys.len()
        || keys
            .iter()
            .zip(coverage)
            .any(|(key, row)| key != row.solution_key())
    {
        return Err(BuildSolutionProbabilityResultError::CoverageKeysMismatch);
    }
    if coverage
        .iter()
        .any(|row| row.covered_patterns().pattern_count() != pattern_count)
    {
        return Err(BuildSolutionProbabilityResultError::CoveragePatternCountMismatch);
    }
    validate_normalized_coverage_union(result, pattern_count, coverage)?;

    let reports = result.solution_probabilities();
    if reports.len() != keys.len()
        || keys
            .iter()
            .zip(reports)
            .any(|(key, report)| key != report.solution_key())
    {
        return Err(BuildSolutionProbabilityResultError::ReportKeysMismatch);
    }
    for (report, row) in reports.iter().zip(coverage) {
        if report.pattern_count() != pattern_count {
            return Err(BuildSolutionProbabilityResultError::ReportPatternCountMismatch);
        }
        if report.covered_pattern_count() > pattern_count {
            return Err(BuildSolutionProbabilityResultError::ReportCoveredCountInvalid);
        }
        if report.covered_pattern_count() != row.covered_patterns().count_ones() as usize {
            return Err(BuildSolutionProbabilityResultError::ReportCoveredCountMismatch);
        }
        let probability = report
            .probability()
            .parse::<f64>()
            .ok()
            .and_then(|value| ProbabilityValue::new(value).ok());
        if probability.is_none() {
            return Err(BuildSolutionProbabilityResultError::ReportProbabilityInvalid);
        }
    }

    if reports
        .iter()
        .any(|report| report.probability_complete() != input.complete)
    {
        return Err(BuildSolutionProbabilityResultError::ReportCompletenessMismatch);
    }

    validate_streaming_solution_probability_reports(
        result,
        pattern_count,
        coverage,
        reports,
        input.complete,
    )?;

    Ok(())
}

pub(crate) fn validate_build_solution_probability_reducer_input(
    expected_policy: Option<BuildSolutionProbabilityPolicy>,
    result: &CoreExecutionResult,
) -> Result<BuildSolutionProbabilityInputState, BuildSolutionProbabilityResultError> {
    let requested = required_bool(result, REQUESTED_FIELD)?;
    let report_count = required_usize(result, COUNT_FIELD)?;
    let complete = required_bool(result, COMPLETE_FIELD)?;
    let basis = required_field(result, BASIS_FIELD)?;
    let incomplete_reason = required_field(result, INCOMPLETE_REASON_FIELD)?;
    let count_complete = required_bool(result, "count_complete")?;
    let probability_complete = required_bool(result, "probability_complete")?;
    let solution_keys_complete = required_bool(result, "solution_keys_complete")?;
    let resource_truncated = required_bool(result, "resource_truncated")?;

    if expected_policy.is_some_and(|policy| requested != policy.requested()) {
        return Err(BuildSolutionProbabilityResultError::RequestPolicyMismatch);
    }

    if report_count != result.solution_probabilities().len() {
        return Err(BuildSolutionProbabilityResultError::ProbabilityCountMismatch);
    }

    if !requested {
        if report_count != 0
            || !complete
            || basis != "not-requested"
            || incomplete_reason != "none"
            || !result.solution_probabilities().is_empty()
        {
            return Err(BuildSolutionProbabilityResultError::OmittedContractMismatch);
        }
    } else if basis != INCLUDE_BASIS {
        return Err(BuildSolutionProbabilityResultError::IncludeBasisMismatch);
    }

    let expected_complete = !requested
        || (count_complete
            && probability_complete
            && solution_keys_complete
            && !resource_truncated);
    if complete != expected_complete {
        return Err(BuildSolutionProbabilityResultError::CompletenessMismatch);
    }
    let expected_reason = build_solution_probability_incomplete_reason(
        requested,
        expected_complete,
        resource_truncated,
        count_complete,
        solution_keys_complete,
    );
    if incomplete_reason != expected_reason {
        return Err(BuildSolutionProbabilityResultError::IncompleteReasonMismatch);
    }

    Ok(BuildSolutionProbabilityInputState {
        requested,
        complete,
        count_complete,
        probability_complete,
        solution_keys_complete,
        resource_truncated,
    })
}

pub(crate) fn validate_build_solution_probability_worker_partial(
    expected_policy: BuildSolutionProbabilityPolicy,
    result: &CoreExecutionResult,
) -> Result<BuildSolutionProbabilityInputState, BuildSolutionProbabilityResultError> {
    let requested = required_bool(result, REQUESTED_FIELD)?;
    if requested != expected_policy.requested() {
        return Err(BuildSolutionProbabilityResultError::RequestPolicyMismatch);
    }
    for final_only_field in [
        COUNT_FIELD,
        COMPLETE_FIELD,
        BASIS_FIELD,
        INCOMPLETE_REASON_FIELD,
        "solution_keys_materialized_count",
        "solution_keys_complete",
    ] {
        if result.field_occurrence_count(final_only_field) != 0 {
            return Err(
                BuildSolutionProbabilityResultError::UnexpectedWorkerPartialField(final_only_field),
            );
        }
    }
    if !result.solution_probabilities().is_empty() {
        return Err(BuildSolutionProbabilityResultError::WorkerPartialReportsPresent);
    }
    if required_bool(result, "execution_constraint_materialized")? {
        return Err(BuildSolutionProbabilityResultError::WorkerPartialAlreadyMaterialized);
    }

    let count_complete = required_bool(result, "count_complete")?;
    let probability_complete = required_bool(result, "probability_complete")?;
    let resource_truncated = required_bool(result, "resource_truncated")?;
    let pattern_count = required_usize(result, "coverage_pattern_count")?;
    if pattern_count == 0 {
        return Err(BuildSolutionProbabilityResultError::PatternDenominatorEmpty);
    }
    let worker_solution_count = required_usize(result, "unique_solution_count")?;
    let keys = result.normalized_solution_keys();
    if keys.iter().any(String::is_empty) || !keys.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(BuildSolutionProbabilityResultError::SolutionKeysNotCanonical);
    }
    if worker_solution_count != keys.len() {
        return Err(BuildSolutionProbabilityResultError::SolutionKeyCountMismatch);
    }
    let coverage = result.normalized_solution_coverages();
    if coverage.len() != keys.len()
        || keys
            .iter()
            .zip(coverage)
            .any(|(key, row)| key != row.solution_key())
    {
        return Err(BuildSolutionProbabilityResultError::CoverageKeysMismatch);
    }
    if coverage
        .iter()
        .any(|row| row.covered_patterns().pattern_count() != pattern_count)
    {
        return Err(BuildSolutionProbabilityResultError::CoveragePatternCountMismatch);
    }
    if requested {
        validate_normalized_coverage_union(result, pattern_count, coverage)?;
        validate_streaming_pattern_weight_authority(result, pattern_count)?;
    }

    Ok(BuildSolutionProbabilityInputState {
        requested,
        // A worker owns only its canonical local key/coverage partition. It
        // cannot assert that the coordinator's normalized solution set is
        // complete, so the final-only key-completeness authority remains
        // conservatively false until the coordinator validates the union.
        complete: !requested,
        count_complete,
        probability_complete,
        solution_keys_complete: false,
        resource_truncated,
    })
}

fn validate_normalized_coverage_union(
    result: &CoreExecutionResult,
    pattern_count: usize,
    coverage: &[clearra_core_executor::NormalizedSolutionCoverage],
) -> Result<(), BuildSolutionProbabilityResultError> {
    let expected_word_count = pattern_count.div_ceil(u64::BITS as usize);
    let global_words = result.coverage_pattern_words();
    if global_words.len() != expected_word_count {
        return Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch);
    }
    let used_tail_bits = pattern_count % (u64::BITS as usize);
    if used_tail_bits != 0
        && global_words
            .last()
            .is_some_and(|word| *word & !((1_u64 << used_tail_bits) - 1) != 0)
    {
        return Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch);
    }
    for (word_index, global_word) in global_words.iter().copied().enumerate() {
        let union = coverage.iter().fold(0_u64, |union, row| {
            union | row.covered_patterns().word_at(word_index)
        });
        if union != global_word {
            return Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch);
        }
    }
    Ok(())
}

fn validate_streaming_solution_probability_reports(
    result: &CoreExecutionResult,
    pattern_count: usize,
    coverage: &[clearra_core_executor::NormalizedSolutionCoverage],
    reports: &[clearra_core_executor::SolutionProbabilityReport],
    complete: bool,
) -> Result<(), BuildSolutionProbabilityResultError> {
    validate_streaming_pattern_weight_authority(result, pattern_count)?;
    let serialized_weights = result.postprocess_pattern_weights();

    for (row, report) in coverage.iter().zip(reports) {
        let covered_count = row.covered_patterns().count_ones() as usize;
        let expected_probability =
            covered_weight_in_pattern_order(pattern_count, row.covered_patterns(), |pattern| {
                // The whole source was checked above, including uncovered
                // weights. This callback only streams canonical covered values.
                let value =
                    parse_canonical_probability(serialized_weights.get(pattern.index())?).ok()?;
                ProbabilityValue::new(value).ok()
            })
            .ok_or(BuildSolutionProbabilityResultError::ReportProbabilityInvalid)?
            .get();
        let reported_probability = parse_canonical_probability(report.probability())
            .map_err(|_| BuildSolutionProbabilityResultError::ReportProbabilityInvalid)?;
        if reported_probability.to_bits() != expected_probability.to_bits()
            || !canonical_probability_text_matches(expected_probability, report.probability())
            || report.covered_pattern_count() != covered_count
            || report.pattern_count() != pattern_count
            || report.probability_complete() != complete
        {
            return Err(BuildSolutionProbabilityResultError::ReportVectorMismatch);
        }
    }
    Ok(())
}

fn validate_streaming_pattern_weight_authority(
    result: &CoreExecutionResult,
    pattern_count: usize,
) -> Result<f64, BuildSolutionProbabilityResultError> {
    let serialized_weights = result.postprocess_pattern_weights();
    if serialized_weights.len() != pattern_count {
        return Err(
            BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(
                "solution_probability_pattern_weight_count_mismatch",
            ),
        );
    }
    let mut total = 0.0_f64;
    for serialized_weight in serialized_weights {
        total += parse_canonical_probability(serialized_weight).map_err(canonical_weight_error)?;
    }
    let summation_tolerance = f64::EPSILON * pattern_count.max(1) as f64 * 2.0;
    if total > 1.0 + summation_tolerance {
        return Err(
            BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(
                "solution_probability_pattern_weight_set_invalid",
            ),
        );
    }
    Ok(total)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalProbabilityParseError {
    Invalid,
    NotCanonical,
}

fn parse_canonical_probability(serialized: &str) -> Result<f64, CanonicalProbabilityParseError> {
    let parsed = serialized
        .parse::<f64>()
        .ok()
        .and_then(|value| ProbabilityValue::new(value).ok())
        .ok_or(CanonicalProbabilityParseError::Invalid)?;
    if !canonical_probability_text_matches(parsed.get(), serialized) {
        return Err(CanonicalProbabilityParseError::NotCanonical);
    }
    Ok(parsed.get())
}

fn canonical_weight_error(
    error: CanonicalProbabilityParseError,
) -> BuildSolutionProbabilityResultError {
    BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(match error {
        CanonicalProbabilityParseError::Invalid => "solution_probability_pattern_weight_invalid",
        CanonicalProbabilityParseError::NotCanonical => {
            "solution_probability_pattern_weight_not_canonical"
        }
    })
}

fn canonical_probability_text_matches(value: f64, expected: &str) -> bool {
    if value == 0.0 {
        return expected == "0";
    }
    if value == 1.0 {
        return expected == "1";
    }
    let mut writer = ExactTextWriter::new(expected);
    write!(&mut writer, "{value}").is_ok() && writer.finish()
}

struct ExactTextWriter<'a> {
    expected: &'a [u8],
    offset: usize,
    matches: bool,
}

impl<'a> ExactTextWriter<'a> {
    const fn new(expected: &'a str) -> Self {
        Self {
            expected: expected.as_bytes(),
            offset: 0,
            matches: true,
        }
    }

    fn finish(self) -> bool {
        self.matches && self.offset == self.expected.len()
    }
}

impl fmt::Write for ExactTextWriter<'_> {
    fn write_str(&mut self, rendered: &str) -> fmt::Result {
        let end = self.offset.saturating_add(rendered.len());
        if end > self.expected.len()
            || self.expected.get(self.offset..end) != Some(rendered.as_bytes())
        {
            self.matches = false;
        }
        self.offset = end;
        Ok(())
    }
}

fn required_field<'a>(
    result: &'a CoreExecutionResult,
    field: &'static str,
) -> Result<&'a str, BuildSolutionProbabilityResultError> {
    if result.field_occurrence_count(field) != 1 {
        return Err(BuildSolutionProbabilityResultError::MissingOrDuplicateField(field));
    }
    result
        .unique_field(field)
        .ok_or(BuildSolutionProbabilityResultError::MissingOrDuplicateField(field))
}

fn required_bool(
    result: &CoreExecutionResult,
    field: &'static str,
) -> Result<bool, BuildSolutionProbabilityResultError> {
    match required_field(result, field)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(BuildSolutionProbabilityResultError::InvalidBooleanField(
            field,
        )),
    }
}

fn required_usize(
    result: &CoreExecutionResult,
    field: &'static str,
) -> Result<usize, BuildSolutionProbabilityResultError> {
    let value = required_field(result, field)?;
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(BuildSolutionProbabilityResultError::InvalidCountField(
            field,
        ));
    }
    value
        .parse::<usize>()
        .map_err(|_| BuildSolutionProbabilityResultError::InvalidCountField(field))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        probability::probability_value::ProbabilityValue,
        solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };
    use clearra_core_executor::{
        normalized_solution_probability_reports, CoreExecutionResult, CorePostProcessScoreCell,
        FinessePolicyResult, FinesseReport, FinesseReportInput, FinesseReportPlacement,
        FinesseRepresentativeWitness, FinesseSolutionAverage, NormalizedSolutionCoverage,
        SolutionAverageScoreReport, SolutionCoverage,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
        BuildProbabilityQuery, BuildSolutionProbabilityPolicy, FinesseMetric,
        FinessePatternKnowledge, FinessePlacement, FinesseScoreRequest, ProblemCompiler,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        build_probability_resource_test_guard,
        build_probability_response as build_probability_response_with_query,
        build_probability_response_is_authorized, validate_build_solution_probability_result,
        validate_build_solution_probability_worker_partial, validate_finesse_score_report,
        BuildSolutionProbabilityResultError, BASIS_FIELD, COMPLETE_FIELD, COUNT_FIELD,
        INCOMPLETE_REASON_FIELD, REQUESTED_FIELD,
    };
    use crate::{AppCoreExecutorService, AppStatus};

    fn build_probability_response(
        expected_finesse: &BuildProbabilityFinesseRequest,
        expected_field: BuildProbabilityField,
        expected_policy: BuildSolutionProbabilityPolicy,
        result: CoreExecutionResult,
    ) -> crate::AppResponse {
        build_probability_response_with_query(
            expected_finesse,
            expected_field,
            BuildProbabilityAggregation::Buildability,
            expected_policy,
            result,
        )
    }

    fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
        (key.into(), value.to_string())
    }

    fn key(piece: PieceKind, cells: u64) -> String {
        NormalizedTilingSolutionKey::from_placements(0, [PiecePlacementMask::new(piece, cells)])
            .expect("canonical one-piece key")
            .as_str()
            .to_owned()
    }

    fn bitset(pattern_count: usize, words: Vec<u64>) -> PatternBitSet {
        PatternBitSet::from_words(pattern_count, words).expect("matching pattern bitset")
    }

    fn build_field() -> BuildProbabilityField {
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("canonical one-piece field")
    }

    fn canonical_build_fixture(
        height: u8,
        target_mask: u64,
        aggregation: BuildProbabilityAggregation,
        include_horizontal_mirror: bool,
    ) -> (BuildProbabilityField, CoreExecutionResult) {
        canonical_build_fixture_with_base(
            height,
            0,
            target_mask,
            aggregation,
            include_horizontal_mirror,
        )
    }

    fn canonical_build_fixture_with_base(
        height: u8,
        base_mask: u64,
        target_mask: u64,
        aggregation: BuildProbabilityAggregation,
        include_horizontal_mirror: bool,
    ) -> (BuildProbabilityField, CoreExecutionResult) {
        let _resource_guard = build_probability_resource_test_guard();
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(u16::from(height), base_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let query = BuildProbabilityQuery::new(
            core,
            BuildProbabilityField::from_words_preserving_height(
                height,
                [base_mask, 0, 0, 0],
                [target_mask, 0, 0, 0],
            )
            .expect("one-piece Build field")
            .with_horizontal_mirror_included(include_horizontal_mirror),
        )
        .with_aggregation(aggregation);
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("canonical Build problem compiles");
        let result = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("canonical Build producer result");
        (query.field(), result)
    }

    fn canonical_build_query_fields() -> Vec<(String, String)> {
        vec![
            field("build_probability_aggregation", "buildability"),
            field("spin_profile_requested", "none"),
            field("postprocess_build_spin_requested", false),
            field("board_height", 4),
            field("build_base_mask", 0),
            field("build_target_cells_mask", 0xf),
            field("build_target_board_mask", 0xf),
            field("target_piece_count", 1),
            field("build_symmetry_policy", "original-only"),
            field("build_mirror_included", false),
            field("build_mirror_distinct_target", false),
            field("build_mirror_search_executed", false),
        ]
    }

    fn included_result(inputs_complete: bool) -> CoreExecutionResult {
        let keys = vec![key(PieceKind::I, 0xf)];
        let coverage = vec![NormalizedSolutionCoverage::new(
            keys[0].clone(),
            bitset(1, vec![1]),
        )];
        let reports = normalized_solution_probability_reports(
            &keys,
            &coverage,
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            inputs_complete,
        )
        .expect("validated reports");
        CoreExecutionResult::new(
            vec![
                field("search_kind", "build-probability"),
                field("objective", "build-probability"),
                field(REQUESTED_FIELD, true),
                field(COUNT_FIELD, 1),
                field(COMPLETE_FIELD, inputs_complete),
                field(BASIS_FIELD, "normalized-solution-pattern-bitset-or-union"),
                field(
                    INCOMPLETE_REASON_FIELD,
                    if inputs_complete {
                        "none"
                    } else {
                        "solution-count-incomplete"
                    },
                ),
                field("coverage_pattern_count", 1),
                field("count_complete", inputs_complete),
                field("probability_complete", inputs_complete),
                field("solution_keys_complete", true),
                field("resource_truncated", false),
                field("solution_keys_materialized_count", 1),
                field("unique_solution_count", 1),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(keys)
        .with_normalized_solution_coverages(coverage)
        .with_coverage_pattern_words(vec![1])
        .with_solution_probabilities(reports)
        .with_postprocess_execution_batch(Vec::new(), inputs_complete, vec!["1".to_owned()])
        .with_additional_fields(canonical_build_query_fields())
    }

    fn weighted_included_result(
        serialized_weights: &[&str],
        covered_words: Vec<u64>,
    ) -> CoreExecutionResult {
        let pattern_count = serialized_weights.len();
        let keys = vec![key(PieceKind::I, 0xf)];
        let coverage = vec![NormalizedSolutionCoverage::new(
            keys[0].clone(),
            bitset(pattern_count, covered_words.clone()),
        )];
        let weights = WeightedPatternSet::new(
            serialized_weights
                .iter()
                .map(|weight| {
                    ProbabilityValue::new(weight.parse::<f64>().expect("numeric test weight"))
                        .expect("bounded test weight")
                })
                .collect(),
        )
        .expect("valid test weight set");
        let reports = normalized_solution_probability_reports(&keys, &coverage, &weights, true)
            .expect("validated weighted reports");
        CoreExecutionResult::new(
            vec![
                field("search_kind", "build-probability"),
                field("objective", "build-probability"),
                field(REQUESTED_FIELD, true),
                field(COUNT_FIELD, 1),
                field(COMPLETE_FIELD, true),
                field(BASIS_FIELD, "normalized-solution-pattern-bitset-or-union"),
                field(INCOMPLETE_REASON_FIELD, "none"),
                field("coverage_pattern_count", pattern_count),
                field("count_complete", true),
                field("probability_complete", true),
                field("solution_keys_complete", true),
                field("resource_truncated", false),
                field("solution_keys_materialized_count", 1),
                field("unique_solution_count", 1),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(keys)
        .with_normalized_solution_coverages(coverage)
        .with_coverage_pattern_words(covered_words)
        .with_solution_probabilities(reports)
        .with_postprocess_execution_batch(
            Vec::new(),
            true,
            serialized_weights
                .iter()
                .map(|weight| (*weight).to_owned())
                .collect(),
        )
        .with_additional_fields(canonical_build_query_fields())
    }

    fn omitted_result() -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![
                field("search_kind", "build-probability"),
                field("objective", "build-probability"),
                field(REQUESTED_FIELD, false),
                field(COUNT_FIELD, 0),
                field(COMPLETE_FIELD, true),
                field(BASIS_FIELD, "not-requested"),
                field(INCOMPLETE_REASON_FIELD, "none"),
                field("count_complete", true),
                field("probability_complete", true),
                field("solution_keys_complete", true),
                field("resource_truncated", false),
            ],
            Vec::new(),
        )
        .with_additional_fields(canonical_build_query_fields())
    }

    fn finesse_policy(policy: &str, complete: bool) -> FinessePolicyResult {
        FinessePolicyResult::new(
            policy,
            "1",
            complete,
            vec![FinesseSolutionAverage::new(
                "given-operation-sequence",
                "1",
                complete,
            )],
        )
        .with_success_summary("1", 1, 1)
    }

    fn expected_finesse_policies(
        knowledge: FinessePatternKnowledge,
        complete: bool,
    ) -> Vec<FinessePolicyResult> {
        match knowledge {
            FinessePatternKnowledge::Both => vec![
                finesse_policy("oracle", complete),
                finesse_policy("visible-7", complete),
            ],
            FinessePatternKnowledge::Oracle => vec![finesse_policy("oracle", complete)],
            FinessePatternKnowledge::VisibleSeven => {
                vec![finesse_policy("visible-7", complete)]
            }
        }
    }

    fn canonical_score_fixture(
        knowledge: FinessePatternKnowledge,
        x: i16,
    ) -> (
        BuildProbabilityFinesseRequest,
        BuildProbabilityField,
        CoreExecutionResult,
    ) {
        let _resource_guard = build_probability_resource_test_guard();
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let query = BuildProbabilityQuery::new(core, build_field())
            .with_finesse(FinesseMetric::Inputs, knowledge)
            .with_finesse_score(
                FinesseScoreRequest::new(vec![FinessePlacement::new(
                    PieceKind::I,
                    RotationState::Zero,
                    x,
                    0,
                )])
                .expect("one score placement"),
            );
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("canonical score problem compiles");
        let result = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("canonical score producer result");
        (query.finesse_request().clone(), query.field(), result)
    }

    fn initial_clear_score_fixture() -> (
        BuildProbabilityFinesseRequest,
        BuildProbabilityField,
        CoreExecutionResult,
    ) {
        let _resource_guard = build_probability_resource_test_guard();
        let completed_row = 0x3ff_u64;
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, completed_row),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let input_field = BuildProbabilityField::from_words_preserving_height(
            4,
            [completed_row, 0, 0, 0],
            [0xf_u64 << 10, 0, 0, 0],
        )
        .expect("one completed input row");
        let query = BuildProbabilityQuery::new(core, input_field)
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle)
            .with_finesse_score(
                FinesseScoreRequest::new(vec![FinessePlacement::new(
                    PieceKind::I,
                    RotationState::Zero,
                    0,
                    1,
                )])
                .expect("pre-clear score placement"),
            );
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("initial-clear score problem compiles");
        let result = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("initial-clear score producer result");
        (query.finesse_request().clone(), query.field(), result)
    }

    fn canonical_all_failure_score_fixture() -> (
        BuildProbabilityFinesseRequest,
        BuildProbabilityField,
        CoreExecutionResult,
    ) {
        let _resource_guard = build_probability_resource_test_guard();
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("empty score field");
        let query = BuildProbabilityQuery::new(core, field)
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle)
            .with_finesse_score(
                FinesseScoreRequest::new(vec![FinessePlacement::new(
                    PieceKind::O,
                    RotationState::Zero,
                    4,
                    0,
                )])
                .expect("one score placement"),
            );
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("canonical all-failure score problem compiles");
        let result = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("canonical all-failure score producer result");
        (query.finesse_request().clone(), query.field(), result)
    }

    fn score_result_without_field(
        result: &CoreExecutionResult,
        removed: &str,
    ) -> CoreExecutionResult {
        let fields = result
            .summary_field_entries()
            .filter(|(key, _)| *key != removed)
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        CoreExecutionResult::new(fields, result.path_steps().to_vec()).with_finesse_report(
            result
                .finesse_report()
                .expect("canonical score report")
                .clone(),
        )
    }

    fn search_result(knowledge: FinessePatternKnowledge) -> CoreExecutionResult {
        omitted_result()
            .with_additional_fields(vec![
                field("finesse_metric_requested", "inputs"),
                field("finesse_pattern_knowledge_requested", knowledge.as_str()),
            ])
            .with_finesse_report(FinesseReport::new(
                "search",
                knowledge.as_str(),
                true,
                Some("1".to_owned()),
                expected_finesse_policies(knowledge, true),
            ))
    }

    fn requested_worker_partial() -> CoreExecutionResult {
        let solution_key = key(PieceKind::I, 0xf);
        CoreExecutionResult::new(
            vec![
                field(REQUESTED_FIELD, true),
                field("execution_constraint_materialized", false),
                field("count_complete", true),
                field("probability_complete", true),
                field("resource_truncated", false),
                field("coverage_pattern_count", 2),
                field("unique_solution_count", 1),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec![solution_key.clone()])
        .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
            solution_key,
            bitset(2, vec![1]),
        )])
        .with_coverage_pattern_words(vec![1])
        .with_postprocess_execution_batch(
            Vec::new(),
            true,
            vec!["0.5".to_owned(), "0.5".to_owned()],
        )
    }

    #[test]
    fn exact_include_and_omit_contracts_are_accepted() {
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &included_result(true)
            ),
            Ok(())
        );
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Omit,
                &omitted_result()
            ),
            Ok(())
        );

        let key_incomplete = included_result(false).with_replaced_fields(vec![
            field("count_complete", true),
            field("probability_complete", true),
            field("solution_keys_complete", false),
            field(
                INCOMPLETE_REASON_FIELD,
                "normalized-solution-set-incomplete",
            ),
        ]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &key_incomplete
            ),
            Ok(())
        );
    }

    #[test]
    fn each_legitimate_partial_reason_uses_core_precedence() {
        let resource = included_result(false).with_replaced_fields(vec![
            field("count_complete", true),
            field("probability_complete", true),
            field("solution_keys_complete", true),
            field("resource_truncated", true),
            field(INCOMPLETE_REASON_FIELD, "resource-truncated"),
        ]);
        let count = included_result(false);
        let keys = included_result(false).with_replaced_fields(vec![
            field("count_complete", true),
            field("probability_complete", true),
            field("solution_keys_complete", false),
            field(
                INCOMPLETE_REASON_FIELD,
                "normalized-solution-set-incomplete",
            ),
        ]);
        let coverage = included_result(false).with_replaced_fields(vec![
            field("count_complete", true),
            field("probability_complete", false),
            field("solution_keys_complete", true),
            field(
                INCOMPLETE_REASON_FIELD,
                "pattern-specific-coverage-incomplete",
            ),
        ]);

        for partial in [&resource, &count, &keys, &coverage] {
            assert_eq!(
                validate_build_solution_probability_result(
                    BuildSolutionProbabilityPolicy::Include,
                    partial,
                ),
                Ok(())
            );
        }
    }

    #[test]
    fn each_solution_probability_metadata_field_must_occur_exactly_once() {
        for metadata in [
            REQUESTED_FIELD,
            COUNT_FIELD,
            COMPLETE_FIELD,
            BASIS_FIELD,
            INCOMPLETE_REASON_FIELD,
        ] {
            let malformed =
                included_result(true).with_additional_fields(vec![field(metadata, "duplicate")]);
            assert_eq!(
                validate_build_solution_probability_result(
                    BuildSolutionProbabilityPolicy::Include,
                    &malformed
                ),
                Err(BuildSolutionProbabilityResultError::MissingOrDuplicateField(metadata)),
                "{metadata}"
            );
        }
    }

    #[test]
    fn included_contract_rejects_count_key_coverage_and_reason_mutations() {
        let wrong_count = included_result(true).with_replaced_fields(vec![field(COUNT_FIELD, 0)]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_count
            ),
            Err(BuildSolutionProbabilityResultError::ProbabilityCountMismatch)
        );

        let wrong_key_count = included_result(true)
            .with_replaced_fields(vec![field("solution_keys_materialized_count", 0)]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_key_count
            ),
            Err(BuildSolutionProbabilityResultError::SolutionKeyCountMismatch)
        );

        let foreign_key = key(PieceKind::O, 0x0c03);
        let wrong_coverage = included_result(true).with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new(foreign_key, bitset(1, vec![1])),
        ]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_coverage
            ),
            Err(BuildSolutionProbabilityResultError::CoverageKeysMismatch)
        );

        let wrong_reason = included_result(false)
            .with_replaced_fields(vec![field(INCOMPLETE_REASON_FIELD, "none")]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_reason
            ),
            Err(BuildSolutionProbabilityResultError::IncompleteReasonMismatch)
        );
    }

    #[test]
    fn included_contract_rejects_covered_count_and_probability_weight_mutations() {
        let wrong_covered_count = included_result(true)
            .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                key(PieceKind::I, 0xf),
                bitset(1, vec![0]),
            )])
            .with_coverage_pattern_words(vec![0]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_covered_count,
            ),
            Err(BuildSolutionProbabilityResultError::ReportCoveredCountMismatch)
        );

        let keys = vec![key(PieceKind::I, 0xf)];
        let coverage = vec![NormalizedSolutionCoverage::new(
            keys[0].clone(),
            bitset(2, vec![1]),
        )];
        let weights = WeightedPatternSet::new(vec![
            ProbabilityValue::new(0.25).expect("valid probability"),
            ProbabilityValue::new(0.75).expect("valid probability"),
        ])
        .expect("normalized weights");
        let reports = normalized_solution_probability_reports(&keys, &coverage, &weights, true)
            .expect("validated reports");
        let wrong_weight_authority = CoreExecutionResult::new(
            vec![
                field(REQUESTED_FIELD, true),
                field(COUNT_FIELD, 1),
                field(COMPLETE_FIELD, true),
                field(BASIS_FIELD, "normalized-solution-pattern-bitset-or-union"),
                field(INCOMPLETE_REASON_FIELD, "none"),
                field("coverage_pattern_count", 2),
                field("count_complete", true),
                field("probability_complete", true),
                field("solution_keys_complete", true),
                field("resource_truncated", false),
                field("solution_keys_materialized_count", 1),
                field("unique_solution_count", 1),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(keys)
        .with_normalized_solution_coverages(coverage)
        .with_coverage_pattern_words(vec![1])
        .with_solution_probabilities(reports)
        .with_postprocess_execution_batch(
            Vec::new(),
            true,
            vec!["0.75".to_owned(), "0.25".to_owned()],
        );
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_weight_authority,
            ),
            Err(BuildSolutionProbabilityResultError::ReportVectorMismatch)
        );
    }

    #[test]
    fn ordered_solution_probability_p7_compact_reports_pass_exact_app_validation() {
        let count = 5040_usize;
        let serialized = (1.0 / count as f64).to_string();
        let serialized_weights = vec![serialized.as_str(); count];
        let mut words = vec![0; count.div_ceil(64)];
        words[0] = 0b11_1111;
        let canonical = weighted_included_result(&serialized_weights, words);
        let compact = WeightedPatternSet::uniform(count).unwrap();
        let reports = normalized_solution_probability_reports(
            canonical.normalized_solution_keys(),
            canonical.normalized_solution_coverages(),
            &compact,
            true,
        )
        .unwrap();
        assert_eq!(reports[0].probability(), "0.0011904761904761904");
        let canonical = canonical.with_solution_probabilities(reports);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &canonical,
            ),
            Ok(())
        );

        // Keep the original exact weights and coverage, but substitute reports
        // from a different (still individually valid) compact distribution.
        // Representation independence must not become tolerance-based trust.
        let wrong_weights = WeightedPatternSet::uniform_with_weight(
            count,
            ProbabilityValue::new(0.5 / count as f64).unwrap(),
        )
        .unwrap();
        let wrong_reports = normalized_solution_probability_reports(
            canonical.normalized_solution_keys(),
            canonical.normalized_solution_coverages(),
            &wrong_weights,
            true,
        )
        .unwrap();
        let tampered = canonical.with_solution_probabilities(wrong_reports);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &tampered,
            ),
            Err(BuildSolutionProbabilityResultError::ReportVectorMismatch)
        );
    }

    #[test]
    fn streaming_probability_validation_matches_the_core_weighted_reducer_exactly() {
        let canonical = weighted_included_result(&["0.1", "0.2", "0.7"], vec![0b101]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &canonical,
            ),
            Ok(())
        );

        let noncanonical = canonical.clone().with_postprocess_execution_batch(
            Vec::new(),
            true,
            vec!["0.10".to_owned(), "0.2".to_owned(), "0.7".to_owned()],
        );
        assert!(matches!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &noncanonical,
            ),
            Err(BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(_))
        ));

        let excessive_total = canonical.with_postprocess_execution_batch(
            Vec::new(),
            true,
            vec!["0.5".to_owned(), "0.5".to_owned(), "0.5".to_owned()],
        );
        assert!(matches!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &excessive_total,
            ),
            Err(BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(_))
        ));
    }

    #[test]
    fn final_and_worker_streaming_validators_accept_a_large_weight_surface_without_rebuilds() {
        const PATTERN_COUNT: usize = 4_096;
        let serialized = vec!["0.000244140625"; PATTERN_COUNT];
        let coverage_words = vec![u64::MAX; PATTERN_COUNT / u64::BITS as usize];
        let final_result = weighted_included_result(&serialized, coverage_words.clone());
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &final_result,
            ),
            Ok(())
        );

        let solution_key = key(PieceKind::I, 0xf);
        let worker = CoreExecutionResult::new(
            vec![
                field(REQUESTED_FIELD, true),
                field("execution_constraint_materialized", false),
                field("count_complete", true),
                field("probability_complete", true),
                field("resource_truncated", false),
                field("coverage_pattern_count", PATTERN_COUNT),
                field("unique_solution_count", 1),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec![solution_key.clone()])
        .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
            solution_key,
            bitset(PATTERN_COUNT, coverage_words.clone()),
        )])
        .with_coverage_pattern_words(coverage_words)
        .with_postprocess_execution_batch(
            Vec::new(),
            true,
            serialized.into_iter().map(str::to_owned).collect(),
        );
        assert_eq!(
            validate_build_solution_probability_worker_partial(
                BuildSolutionProbabilityPolicy::Include,
                &worker,
            ),
            Ok(super::BuildSolutionProbabilityInputState {
                requested: true,
                complete: false,
                count_complete: true,
                probability_complete: true,
                solution_keys_complete: false,
                resource_truncated: false,
            })
        );
    }

    #[test]
    fn requested_final_and_worker_surfaces_require_the_exact_coverage_union() {
        assert!(validate_build_solution_probability_worker_partial(
            BuildSolutionProbabilityPolicy::Include,
            &requested_worker_partial(),
        )
        .is_ok());

        let wrong_final_union = included_result(true).with_coverage_pattern_words(vec![0]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_final_union,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );

        let wrong_worker_union = requested_worker_partial().with_coverage_pattern_words(vec![2]);
        assert_eq!(
            validate_build_solution_probability_worker_partial(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_worker_union,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );

        let noncanonical_worker_weights = requested_worker_partial()
            .with_postprocess_execution_batch(
                Vec::new(),
                true,
                vec!["0.50".to_owned(), "0.5".to_owned()],
            );
        assert!(matches!(
            validate_build_solution_probability_worker_partial(
                BuildSolutionProbabilityPolicy::Include,
                &noncanonical_worker_weights,
            ),
            Err(BuildSolutionProbabilityResultError::PatternWeightAuthorityInvalid(_))
        ));
    }

    #[test]
    fn requested_coverage_global_shape_is_exact_and_tail_clean() {
        let wrong_word_count = included_result(true).with_coverage_pattern_words(Vec::new());
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_word_count,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );

        let dirty_tail = included_result(true).with_coverage_pattern_words(vec![3]);
        assert_eq!(
            validate_build_solution_probability_result(
                BuildSolutionProbabilityPolicy::Include,
                &dirty_tail,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );

        let wrong_worker_word_count =
            requested_worker_partial().with_coverage_pattern_words(vec![1, 0]);
        assert_eq!(
            validate_build_solution_probability_worker_partial(
                BuildSolutionProbabilityPolicy::Include,
                &wrong_worker_word_count,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );

        let dirty_worker_tail = requested_worker_partial().with_coverage_pattern_words(vec![0b101]);
        assert_eq!(
            validate_build_solution_probability_worker_partial(
                BuildSolutionProbabilityPolicy::Include,
                &dirty_worker_tail,
            ),
            Err(BuildSolutionProbabilityResultError::CoverageUnionMismatch)
        );
    }

    #[test]
    fn build_query_surface_is_bound_to_compact_extended_field_and_aggregation() {
        let (compact_field, compact_result) =
            canonical_build_fixture(4, 0xf, BuildProbabilityAggregation::Buildability, false);
        let compact = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            compact_field,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            compact_result.clone(),
        );
        assert_eq!(compact.status(), AppStatus::Success, "{compact:?}");

        let (line_clearing_field, line_clearing_result) = canonical_build_fixture_with_base(
            4,
            0x3f,
            0x3c0,
            BuildProbabilityAggregation::Buildability,
            false,
        );
        assert_eq!(
            line_clearing_result.field("build_target_board_mask"),
            Some("1023")
        );
        assert_eq!(
            line_clearing_result.field("build_final_board_mask"),
            Some("0")
        );
        let line_clearing = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            line_clearing_field,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            line_clearing_result,
        );
        assert_eq!(
            line_clearing.status(),
            AppStatus::Success,
            "{line_clearing:?}"
        );

        let (extended_field, extended_result) =
            canonical_build_fixture(7, 0xf, BuildProbabilityAggregation::Buildability, false);
        let extended = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            extended_field,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            extended_result.clone(),
        );
        assert_eq!(extended.status(), AppStatus::Success, "{extended:?}");

        let (_, foreign_field_result) =
            canonical_build_fixture(4, 0x1e, BuildProbabilityAggregation::Buildability, false);
        let swapped_field = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            compact_field,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            foreign_field_result,
        );
        assert_eq!(swapped_field.status(), AppStatus::ExecutionFailed);

        let (_, foreign_aggregation_result) =
            canonical_build_fixture(4, 0xf, BuildProbabilityAggregation::TilingOnly, false);
        let swapped_aggregation = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            compact_field,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            foreign_aggregation_result,
        );
        assert_eq!(swapped_aggregation.status(), AppStatus::ExecutionFailed);

        let foreign_compact_height =
            BuildProbabilityField::from_words_preserving_height(5, [0; 4], [0xf, 0, 0, 0])
                .expect("same compact masks at a different retained height");
        let swapped_height = build_probability_response_with_query(
            &BuildProbabilityFinesseRequest::Off,
            foreign_compact_height,
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            compact_result.clone(),
        );
        assert_eq!(swapped_height.status(), AppStatus::ExecutionFailed);

        for malformed in [
            compact_result
                .clone()
                .with_replaced_fields(vec![field("build_base_mask", "00")]),
            compact_result
                .clone()
                .with_replaced_fields(vec![field("target_piece_count", 2)]),
            compact_result.with_replaced_fields(vec![field("build_mirror_included", true)]),
            extended_result
                .clone()
                .with_replaced_fields(vec![field("board_height", 8)]),
            extended_result.with_replaced_fields(vec![field("build_target_cells_mask", "0Xf")]),
        ] {
            let expected_field = if malformed.field("board_storage") == Some("board256-canonical") {
                extended_field
            } else {
                compact_field
            };
            let response = build_probability_response_with_query(
                &BuildProbabilityFinesseRequest::Off,
                expected_field,
                BuildProbabilityAggregation::Buildability,
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn malformed_results_fail_the_response_boundary_without_projection() {
        let malformed =
            included_result(true).with_replaced_fields(vec![field(COMPLETE_FIELD, false)]);
        let response = build_probability_response(
            &BuildProbabilityFinesseRequest::Off,
            build_field(),
            BuildSolutionProbabilityPolicy::Include,
            malformed,
        );

        assert_eq!(response.status(), AppStatus::ExecutionFailed);
        assert!(response.render_model().is_none());
        assert!(response.error().is_some_and(|error| error
            .message()
            .contains("build solution probability result rejected")));
    }

    #[test]
    fn allocation_free_response_authority_matches_canonical_query_shape() {
        let canonical = omitted_result();
        assert!(build_probability_response_is_authorized(
            &BuildProbabilityFinesseRequest::Off,
            build_field(),
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            &canonical,
        ));

        let malformed = canonical.with_replaced_fields(vec![field("objective", "pc")]);
        assert!(!build_probability_response_is_authorized(
            &BuildProbabilityFinesseRequest::Off,
            build_field(),
            BuildProbabilityAggregation::Buildability,
            BuildSolutionProbabilityPolicy::Omit,
            &malformed,
        ));
    }

    #[test]
    fn canonical_finesse_score_is_accepted_only_for_its_query_authority() {
        let (request, expected_field, score_result) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let response = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            score_result.clone(),
        );
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");

        let requested = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Include,
            score_result.clone(),
        );
        assert_eq!(requested.status(), AppStatus::ExecutionFailed);

        let foreign_field = BuildProbabilityField::from_words_preserving_height(
            expected_field.height(),
            [1, 0, 0, 0],
            [0xf0, 0, 0, 0],
        )
        .expect("different score field");
        let wrong_field = build_probability_response(
            &request,
            foreign_field,
            BuildSolutionProbabilityPolicy::Omit,
            score_result.clone(),
        );
        assert_eq!(wrong_field.status(), AppStatus::ExecutionFailed);

        let injected_surface = score_result
            .clone()
            .with_additional_fields(vec![field(REQUESTED_FIELD, false)]);
        let injected = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            injected_surface,
        );
        assert_eq!(injected.status(), AppStatus::ExecutionFailed);

        let (_, _, swapped_result) = canonical_score_fixture(FinessePatternKnowledge::Oracle, 1);
        let swapped = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            swapped_result,
        );
        assert_eq!(swapped.status(), AppStatus::ExecutionFailed);

        let (_, cleared_field, cleared_result) = initial_clear_score_fixture();
        assert_eq!(cleared_field.base_words(), expected_field.base_words());
        assert_eq!(cleared_result.path_steps(), score_result.path_steps());
        let same_public_path_from_different_original_request = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            cleared_result,
        );
        assert_eq!(
            same_public_path_from_different_original_request.status(),
            AppStatus::ExecutionFailed
        );
    }

    #[test]
    fn canonical_finesse_score_producer_shapes_pass_for_every_knowledge_policy() {
        for knowledge in [
            FinessePatternKnowledge::Both,
            FinessePatternKnowledge::Oracle,
            FinessePatternKnowledge::VisibleSeven,
        ] {
            let (request, expected_field, result) = canonical_score_fixture(knowledge, 0);
            let response = build_probability_response(
                &request,
                expected_field,
                BuildSolutionProbabilityPolicy::Omit,
                result,
            );
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "{knowledge:?}: {response:?}"
            );
        }
    }

    #[test]
    fn canonical_all_failure_finesse_score_producer_shape_passes() {
        let (request, expected_field, result) = canonical_all_failure_score_fixture();
        assert!(result.path_steps().is_empty());
        let report = result
            .finesse_report()
            .expect("canonical all-failure score report");
        assert_eq!(report.exact_total_inputs(), None);
        assert_eq!(report.representative_witness(), None);
        assert_eq!(report.policy_results().len(), 1);
        assert_eq!(
            report.policy_results()[0].successful_unique_queue_count(),
            Some(0)
        );
        assert_eq!(
            report.policy_results()[0].overall_average_inputs(),
            "unavailable"
        );

        let response = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            result,
        );
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    }

    #[test]
    fn finesse_score_summary_fields_are_an_exact_unique_producer_allowlist() {
        let (request, expected_field, canonical) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let larger_materialized_pattern_count = canonical
            .unique_field("materialized_pattern_count")
            .expect("canonical materialized pattern count")
            .parse::<usize>()
            .expect("canonical count parses")
            .checked_add(1)
            .expect("small fixture count increments");
        for required in super::SCORE_SUMMARY_FIELDS {
            let missing = score_result_without_field(&canonical, required);
            let duplicate = canonical.clone().with_additional_fields(vec![field(
                required,
                canonical
                    .unique_field(required)
                    .expect("canonical required score field"),
            )]);
            for malformed in [missing, duplicate] {
                let response = build_probability_response(
                    &request,
                    expected_field,
                    BuildSolutionProbabilityPolicy::Omit,
                    malformed,
                );
                assert_eq!(response.status(), AppStatus::ExecutionFailed, "{required}");
            }
        }

        for malformed in [
            canonical
                .clone()
                .with_additional_fields(vec![field("unknown-score-authority", "forged")]),
            canonical.clone().with_replaced_fields(vec![field(
                "finesse_initial_board_words",
                "0X0000000000000000000000000000000000000000000000000000000000000000",
            )]),
            canonical.clone().with_replaced_fields(vec![field(
                "finesse_initial_board_words",
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )]),
            canonical
                .clone()
                .with_replaced_fields(vec![field("materialized_pattern_count", "01")]),
            canonical.clone().with_replaced_fields(vec![field(
                "materialized_pattern_count",
                larger_materialized_pattern_count,
            )]),
            canonical.with_replaced_fields(vec![field("objective_complete", "TRUE")]),
        ] {
            let response = build_probability_response(
                &request,
                expected_field,
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn public_finesse_report_mutation_revokes_the_score_request_authority() {
        let (request, expected_field, canonical) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let report = canonical
            .finesse_report()
            .expect("canonical score report")
            .clone();
        let witness = report
            .representative_witness()
            .expect("canonical score witness")
            .clone();
        let mutated = canonical.with_finesse_report(report.with_representative_witness(witness));

        let response = build_probability_response(
            &request,
            expected_field,
            BuildSolutionProbabilityPolicy::Omit,
            mutated,
        );
        assert_eq!(response.status(), AppStatus::ExecutionFailed);
    }

    #[test]
    fn result_kind_is_exact_unique_and_selected_from_the_query() {
        let (score_request, score_field, finesse_as_build) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let build_as_score = omitted_result().with_finesse_report(FinesseReport::new(
            "score",
            "oracle",
            true,
            None,
            vec![finesse_policy("oracle", true)],
        ));
        let response = build_probability_response(
            &score_request,
            score_field,
            BuildSolutionProbabilityPolicy::Omit,
            build_as_score,
        );
        assert_eq!(response.status(), AppStatus::ExecutionFailed);

        let response = build_probability_response(
            &BuildProbabilityFinesseRequest::Off,
            build_field(),
            BuildSolutionProbabilityPolicy::Omit,
            finesse_as_build,
        );
        assert_eq!(response.status(), AppStatus::ExecutionFailed);

        let missing_kind = CoreExecutionResult::new(
            vec![
                field(REQUESTED_FIELD, false),
                field(COUNT_FIELD, 0),
                field(COMPLETE_FIELD, true),
                field(BASIS_FIELD, "not-requested"),
                field(INCOMPLETE_REASON_FIELD, "none"),
                field("count_complete", true),
                field("probability_complete", true),
                field("solution_keys_complete", true),
                field("resource_truncated", false),
            ],
            Vec::new(),
        );
        let duplicate_kind = omitted_result()
            .with_additional_fields(vec![field("search_kind", "build-probability")]);
        for malformed in [missing_kind, duplicate_kind] {
            let response = build_probability_response(
                &BuildProbabilityFinesseRequest::Off,
                build_field(),
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn finesse_score_requires_complete_query_bound_metadata_and_report() {
        let (request, expected_field, canonical) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let skeletal =
            CoreExecutionResult::new(vec![field("search_kind", "finesse-score")], Vec::new());
        let missing_report =
            CoreExecutionResult::new(canonical.summary_fields(), canonical.path_steps().to_vec());
        let wrong_report = canonical.clone().with_finesse_report(FinesseReport::new(
            "search",
            "visible-7",
            false,
            None,
            vec![finesse_policy("visible-7", false)],
        ));
        let wrong_policy_list = canonical.with_finesse_report(FinesseReport::new(
            "score",
            "oracle",
            true,
            None,
            vec![finesse_policy("visible-7", true)],
        ));

        for malformed in [skeletal, missing_report, wrong_report, wrong_policy_list] {
            let response = build_probability_response(
                &request,
                expected_field,
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn finesse_score_report_rejects_each_non_producer_summary_and_witness_shape() {
        let (request, expected_field, canonical) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let score = request.score().expect("score request");
        let average = || {
            vec![FinesseSolutionAverage::new(
                "given-operation-sequence",
                "1",
                true,
            )]
        };
        let malformed_reports = [
            FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![FinessePolicyResult::new("oracle", "1", true, average())],
            ),
            FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![FinessePolicyResult::new("oracle", "1", true, average())
                    .with_success_summary("1", 2, 1)],
            ),
            FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![FinessePolicyResult::new("oracle", "1", true, Vec::new())
                    .with_success_summary("1", 1, 1)],
            ),
            FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![FinessePolicyResult::new("oracle", "1", true, average())
                    .with_success_summary("1", 1, 1)],
            ),
            FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![FinessePolicyResult::new("oracle", "1", true, average())
                    .with_success_summary("1", 1, 1)],
            )
            .with_representative_witness(FinesseRepresentativeWitness::new(
                "oracle",
                Some("given-operation-sequence".to_owned()),
                vec![0],
                vec![PieceKind::I],
                1,
                vec![FinesseReportInput::HardDrop],
                vec![FinesseReportPlacement::new(
                    PieceKind::I,
                    RotationState::Zero,
                    0,
                    0,
                )],
            )),
            FinesseReport::new(
                "score",
                "oracle",
                true,
                Some("2".to_owned()),
                vec![FinessePolicyResult::new("oracle", "1", true, average())
                    .with_success_summary("1", 1, 1)],
            )
            .with_representative_witness(FinesseRepresentativeWitness::new(
                "oracle",
                Some("given-operation-sequence".to_owned()),
                vec![0],
                vec![PieceKind::I],
                2,
                vec![FinesseReportInput::HardDrop],
                vec![FinesseReportPlacement::new(
                    PieceKind::I,
                    RotationState::Zero,
                    0,
                    0,
                )],
            )),
        ];

        for malformed in malformed_reports {
            assert!(matches!(
                validate_finesse_score_report(
                    &malformed,
                    FinessePatternKnowledge::Oracle,
                    score,
                    expected_field,
                    1,
                    1,
                    &canonical,
                ),
                Err(BuildSolutionProbabilityResultError::FinesseScoreReportContractMismatch(_))
            ));
        }
    }

    #[test]
    fn finesse_score_rejects_residual_private_solution_authority() {
        let (request, expected_field, canonical) =
            canonical_score_fixture(FinessePatternKnowledge::Oracle, 0);
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("one-piece identity");
        let included = included_result(true);
        let private_probability = included.solution_probabilities().to_vec();
        for malformed in
            [
                canonical
                    .clone()
                    .with_packing_candidate_keys(vec!["private-candidate".to_owned()]),
                canonical.clone().with_coverage_pattern_words(vec![1]),
                canonical
                    .clone()
                    .with_postprocess_execution_batch(Vec::new(), true, Vec::new()),
                canonical
                    .clone()
                    .with_normalized_solution_keys(vec![key(PieceKind::I, 0xf)]),
                canonical
                    .clone()
                    .with_normalized_solution_identities(vec![identity]),
                canonical
                    .clone()
                    .with_representative_solution_identity(Some(identity)),
                canonical
                    .clone()
                    .with_solution_coverages(vec![SolutionCoverage::new(
                        identity,
                        bitset(1, vec![1]),
                    )]),
                canonical.clone().with_normalized_solution_coverages(vec![
                    NormalizedSolutionCoverage::new(key(PieceKind::I, 0xf), bitset(1, vec![1])),
                ]),
                canonical
                    .clone()
                    .with_solution_probabilities(private_probability),
                canonical.clone().with_solution_average_scores(vec![
                    SolutionAverageScoreReport::new(key(PieceKind::I, 0xf), "1", 1, 1, true),
                ]),
                canonical.with_postprocess_score_cells(
                    vec![CorePostProcessScoreCell::new(
                        identity,
                        0,
                        "private-trace",
                        0,
                        0,
                    )],
                    true,
                    "private-profile",
                ),
            ]
        {
            let response = build_probability_response(
                &request,
                expected_field,
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn finesse_search_requires_query_bound_fields_report_and_policy_order() {
        for knowledge in [
            FinessePatternKnowledge::Both,
            FinessePatternKnowledge::Oracle,
            FinessePatternKnowledge::VisibleSeven,
        ] {
            let request = BuildProbabilityFinesseRequest::Search {
                pattern_knowledge: knowledge,
            };
            let response = build_probability_response(
                &request,
                build_field(),
                BuildSolutionProbabilityPolicy::Omit,
                search_result(knowledge),
            );
            assert_eq!(response.status(), AppStatus::Success, "{knowledge:?}");
        }

        let request = BuildProbabilityFinesseRequest::Search {
            pattern_knowledge: FinessePatternKnowledge::Oracle,
        };
        for malformed in [
            search_result(FinessePatternKnowledge::Oracle)
                .with_replaced_fields(vec![field("finesse_metric_requested", "off")]),
            search_result(FinessePatternKnowledge::Oracle)
                .with_replaced_fields(vec![field("objective", "finesse")]),
            search_result(FinessePatternKnowledge::Oracle).with_replaced_fields(vec![field(
                "finesse_pattern_knowledge_requested",
                "visible-7",
            )]),
            search_result(FinessePatternKnowledge::Oracle).with_finesse_report(FinesseReport::new(
                "score",
                "oracle",
                true,
                None,
                vec![finesse_policy("oracle", true)],
            )),
        ] {
            let response = build_probability_response(
                &request,
                build_field(),
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }

    #[test]
    fn finesse_off_rejects_every_finesse_surface() {
        for malformed in [
            omitted_result()
                .with_additional_fields(vec![field("finesse_metric_requested", "inputs")]),
            omitted_result().with_replaced_fields(vec![field("objective", "finesse")]),
            omitted_result().with_finesse_report(FinesseReport::new(
                "search",
                "oracle",
                true,
                None,
                vec![finesse_policy("oracle", true)],
            )),
        ] {
            let response = build_probability_response(
                &BuildProbabilityFinesseRequest::Off,
                build_field(),
                BuildSolutionProbabilityPolicy::Omit,
                malformed,
            );
            assert_eq!(response.status(), AppStatus::ExecutionFailed);
        }
    }
}
