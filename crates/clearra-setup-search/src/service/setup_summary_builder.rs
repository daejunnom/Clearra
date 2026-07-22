mod base_fields {
    use crate::service::setup_search_service::SetupSearchExecutionResult;

    pub(super) fn base_fields(result: &SetupSearchExecutionResult) -> Vec<(String, String)> {
        vec![
            ("status".to_owned(), result.status.to_owned()),
            (
                "execution_scope".to_owned(),
                result.execution_scope.to_owned(),
            ),
            (
                "enumeration_strategy".to_owned(),
                result.enumeration_strategy.to_owned(),
            ),
            (
                "shape_family_enumeration_complete".to_owned(),
                result.shape_family_enumeration_complete.to_string(),
            ),
            (
                "tiling_variant_enumeration_complete".to_owned(),
                result.tiling_variant_enumeration_complete.to_string(),
            ),
            (
                "build_variant_enumeration_complete".to_owned(),
                result.build_variant_enumeration_complete.to_string(),
            ),
            ("post_pc_mode".to_owned(), result.post_pc_mode.to_owned()),
            (
                "post_pc_evaluation_attached".to_owned(),
                result.post_pc_evaluation_attached.to_string(),
            ),
            (
                "setup_foundation_reason".to_owned(),
                result.setup_foundation_reason.to_owned(),
            ),
            ("executor_flow".to_owned(), result.executor_flow.to_owned()),
            (
                "build_variant_source".to_owned(),
                result.build_variant_source.to_owned(),
            ),
            (
                "packing_candidate_count".to_owned(),
                result.packing_candidate_count.to_string(),
            ),
            (
                "core_buildup_variant_count".to_owned(),
                result.core_buildup_variant_count.to_string(),
            ),
            (
                "core_coverage_row_count".to_owned(),
                result.core_coverage_row_count.to_string(),
            ),
            (
                "coverage_source".to_owned(),
                result.coverage_source.to_owned(),
            ),
            (
                "coverage_pattern_count".to_owned(),
                result.coverage_pattern_count.to_string(),
            ),
            (
                "verified_pattern_count".to_owned(),
                result.verified_pattern_count.to_string(),
            ),
            (
                "materialized_pattern_count".to_owned(),
                result.materialized_pattern_count.to_string(),
            ),
            (
                "covered_pattern_count_basis".to_owned(),
                result.covered_pattern_count_basis.to_owned(),
            ),
            ("queue_mode".to_owned(), result.queue_mode.to_owned()),
            ("queue_len".to_owned(), result.queue_len.to_string()),
            ("pattern_count".to_owned(), result.pattern_count.to_string()),
            (
                "total_pattern_count".to_owned(),
                result.total_pattern_count.to_string(),
            ),
            (
                "materialized_probability_mass".to_owned(),
                result.materialized_probability_mass.clone(),
            ),
            (
                "probability_complete".to_owned(),
                result.probability_complete.to_string(),
            ),
            (
                "expansion_truncated".to_owned(),
                result.expansion_truncated.to_string(),
            ),
            ("family_count".to_owned(), result.family_count.to_string()),
            (
                "tiling_variant_count".to_owned(),
                result.tiling_variant_count.to_string(),
            ),
            (
                "build_variant_count".to_owned(),
                result.build_variant_count.to_string(),
            ),
            (
                "score_aggregation_attached".to_owned(),
                result.score_aggregation_attached.to_string(),
            ),
            ("result_count".to_owned(), result.result_count.to_string()),
            (
                "setup_raw_metrics_schema_version".to_owned(),
                "2".to_owned(),
            ),
            ("metrics_kind".to_owned(), "setup_raw_metrics".to_owned()),
            ("setup_raw_metrics".to_owned(), "per-result".to_owned()),
            (
                "setup_raw_coverage_export".to_owned(),
                "union-coverage-fields".to_owned(),
            ),
            ("raw_coverage_schema_version".to_owned(), "2".to_owned()),
            (
                "raw_coverage_export_kind".to_owned(),
                "setup_raw_coverage_export".to_owned(),
            ),
            (
                "pattern_universe_id".to_owned(),
                "setup-family-universe-v2".to_owned(),
            ),
            (
                "pattern_weight_model_id".to_owned(),
                "setup-family-weight-model-v2".to_owned(),
            ),
            ("rows".to_owned(), "per-result".to_owned()),
            ("family_unions".to_owned(), "per-family".to_owned()),
            ("overlap_report".to_owned(), "visible".to_owned()),
            ("backend_report".to_owned(), "per-result".to_owned()),
            (
                "coverage_overlap_report".to_owned(),
                "union-probability-no-variant-sum".to_owned(),
            ),
            ("build_variant_metrics".to_owned(), "per-result".to_owned()),
            ("diagnostic_evidence".to_owned(), "per-result".to_owned()),
        ]
    }
}
mod builder {
    use crate::service::setup_search_service::SetupSearchExecutionResult;

    use super::{
        base_fields::base_fields, coverage_fields::append_coverage_fields,
        family_score_fields::append_family_score_fields, identity_fields::append_identity_fields,
        post_pc_detail_fields::append_post_pc_detail_fields,
        post_pc_status_fields::append_post_pc_status_fields,
        result_context::SetupResultSummaryContext,
        rule_evidence_fields::append_rule_evidence_fields, supply_fields::append_supply_fields,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub(crate) struct SetupSummaryBuilder;

    impl SetupSummaryBuilder {
        pub(crate) fn summary_fields(result: &SetupSearchExecutionResult) -> Vec<(String, String)> {
            let mut fields = base_fields(result);
            for (index, setup_result) in result.results.iter().enumerate() {
                let context = SetupResultSummaryContext {
                    index,
                    result: setup_result,
                    family_score: result
                        .family_scores
                        .iter()
                        .find(|score| score.family_id() == setup_result.family_id()),
                    pattern_count: result.pattern_count,
                };
                append_identity_fields(&mut fields, &context);
                append_coverage_fields(&mut fields, &context);
                append_supply_fields(&mut fields, &context);
                append_rule_evidence_fields(&mut fields, &context);
                append_post_pc_status_fields(&mut fields, &context);
                append_family_score_fields(&mut fields, &context);
                append_post_pc_detail_fields(&mut fields, &context);
            }
            fields
        }
    }
}
mod coverage_fields {
    use super::result_context::SetupResultSummaryContext;

    pub(super) fn append_coverage_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let result = context.result;
        let covered = result
            .union_coverage()
            .covered_patterns()
            .count_ones()
            .to_string();
        let family_id = result.family_id().get();
        fields.extend([
            (context.key("covered_patterns"), covered.clone()),
            (context.key("covered_pattern_count"), covered),
            (
                context.key("raw_coverage_export_path"),
                format!("inline://clearra/setup/raw-coverage/{family_id}/union"),
            ),
            (context.key("raw_coverage_schema_version"), "2".to_owned()),
            (
                context.key("raw_coverage_export_kind"),
                "setup_raw_coverage_export".to_owned(),
            ),
            (
                context.key("pattern_universe_id"),
                format!("setup-family-universe-{family_id}"),
            ),
            (
                context.key("pattern_weight_model_id"),
                format!("setup-family-weight-model-{family_id}"),
            ),
            (
                context.key("pattern_count"),
                context.pattern_count.to_string(),
            ),
            (
                context.key("rows"),
                "machine-readable-coverage-rows".to_owned(),
            ),
            (
                context.key("family_unions"),
                "machine-readable-family-unions".to_owned(),
            ),
            (context.key("overlap_report"), "visible".to_owned()),
            (context.key("setup_raw_metrics"), "attached".to_owned()),
            (
                context.key("setup_raw_coverage_export"),
                "inline".to_owned(),
            ),
            (context.key("backend_report"), "attached".to_owned()),
        ]);
    }
}
mod family_score_fields {
    use super::{formatters::format_probability, result_context::SetupResultSummaryContext};

    pub(super) fn append_family_score_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let Some(score) = context.family_score else {
            return;
        };
        fields.extend([
            (
                context.key("post_pc_probability"),
                format_probability(score.post_pc_probability().get()),
            ),
            (
                context.key("expected_score"),
                format_probability(score.expected_score()),
            ),
            (
                context.key("expected_attack"),
                format_probability(score.expected_attack()),
            ),
            (
                context.key("total_solution_count"),
                score.total_solution_count().to_string(),
            ),
            (
                context.key("continuation_available"),
                score.continuation_available().to_string(),
            ),
            (
                context.key("continuation_available_complete"),
                score.continuation_available_complete().to_string(),
            ),
        ]);
    }
}
mod formatters {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    pub(crate) fn format_probability(value: f64) -> String {
        if value == 0.0 || value == 1.0 {
            return format!("{value:.0}");
        }
        format!("{value:.12}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }

    pub(super) fn format_piece_sequence(pieces: &[PieceKind]) -> String {
        if pieces.is_empty() {
            return "none".to_owned();
        }
        pieces.iter().map(|piece| piece.as_ascii()).collect()
    }

    pub(super) fn format_usize_list(values: &[usize]) -> String {
        if values.is_empty() {
            return "none".to_owned();
        }
        values
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}
mod identity_fields {
    use super::{formatters::format_probability, result_context::SetupResultSummaryContext};

    pub(super) fn append_identity_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let result = context.result;
        let summary = result.setup_raw_metrics();
        let family_id = result.family_id().get().to_string();
        fields.extend([
            (context.key("family_id"), family_id.clone()),
            (context.key("shape_family_id"), family_id),
            (
                context.key("shape_family_count"),
                summary.shape_family_count().to_string(),
            ),
            (
                context.key("tiling_variant_count"),
                summary.tiling_variant_count().to_string(),
            ),
            (
                context.key("build_variant_count"),
                summary.build_variant_count().to_string(),
            ),
            (
                context.key("setup_raw_metrics_schema_version"),
                "2".to_owned(),
            ),
            (context.key("metrics_kind"), "setup_raw_metrics".to_owned()),
            (
                context.key("probability"),
                format_probability(result.probability().get()),
            ),
            (
                context.key("coverage_probability"),
                format_probability(result.probability().get()),
            ),
        ]);
    }
}
mod post_pc_detail_fields {
    use super::result_context::SetupResultSummaryContext;

    pub(super) fn append_post_pc_detail_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let Some(post_pc) = context
            .result
            .setup_raw_metrics()
            .post_pc_evaluation()
            .summary()
        else {
            return;
        };
        fields.extend([
            (
                context.key("post_pc_min_queue_consumed"),
                post_pc.min_queue_consumed().to_string(),
            ),
            (
                context.key("post_pc_max_queue_consumed"),
                post_pc.max_queue_consumed().to_string(),
            ),
            (
                context.key("post_pc_sample_queue_consumed"),
                post_pc.sample_queue_consumed().to_string(),
            ),
            (
                context.key("post_pc_placed_piece_count"),
                post_pc.placed_piece_count().to_string(),
            ),
            (
                context.key("post_pc_best_remaining_queue_len"),
                post_pc.best_remaining_queue_len().to_string(),
            ),
            (
                context.key("post_pc_continuation_available_complete"),
                post_pc.continuation_available_complete().to_string(),
            ),
            (
                context.key("score_evaluation_trace_count"),
                post_pc.score_evaluation_trace_count().to_string(),
            ),
            (
                context.key("score_evaluation_complete"),
                post_pc.score_evaluation_complete().to_string(),
            ),
            (
                context.key("score_evaluation_basis"),
                post_pc.score_evaluation_basis().as_str().to_owned(),
            ),
        ]);
    }
}
mod post_pc_status_fields {
    use super::result_context::SetupResultSummaryContext;

    pub(super) fn append_post_pc_status_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let summary = context.result.setup_raw_metrics();
        fields.extend([
            (
                context.key("post_pc_solution_found"),
                summary.post_pc_solution_found().to_string(),
            ),
            (
                context.key("post_pc_solution_count"),
                summary
                    .post_pc_evaluation()
                    .summary()
                    .map(|post_pc| post_pc.total_solution_count())
                    .unwrap_or(0)
                    .to_string(),
            ),
            (
                context.key("post_pc_status"),
                summary.post_pc_evaluation().status().to_owned(),
            ),
            (
                context.key("score_basis"),
                summary
                    .post_pc_evaluation()
                    .summary()
                    .map(|post_pc| post_pc.score_evaluation_basis().as_str())
                    .unwrap_or("none")
                    .to_owned(),
            ),
        ]);
        if let Some(reason) = summary.post_pc_evaluation().unsupported_reason() {
            fields.push((context.key("post_pc_reason"), reason.to_owned()));
        }
    }
}
mod result_context {
    use crate::result::{SetupFamilyScore, SetupResult};

    pub(super) struct SetupResultSummaryContext<'a> {
        pub(super) index: usize,
        pub(super) result: &'a SetupResult,
        pub(super) family_score: Option<&'a SetupFamilyScore>,
        pub(super) pattern_count: usize,
    }

    impl SetupResultSummaryContext<'_> {
        pub(super) fn key(&self, suffix: &str) -> String {
            format!("result_{}_{}", self.index, suffix)
        }
    }
}
mod rule_evidence_fields {
    use super::result_context::SetupResultSummaryContext;

    pub(super) fn append_rule_evidence_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let summary = context.result.setup_raw_metrics();
        fields.extend([
            (
                context.key("requires_180"),
                summary.requires_180().to_string(),
            ),
            (
                context.key("requires_180_evidence"),
                summary.requires_180_evidence().as_str().to_owned(),
            ),
            (
                context.key("rule_profile_evidence"),
                summary.rule_profile_evidence().as_str().to_owned(),
            ),
            (
                context.key("build_variant_metrics_required_hold"),
                summary
                    .hold_piece()
                    .map(|piece| piece.as_ascii().to_string())
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            (
                context.key("diagnostic_evidence_rule_profile"),
                summary.rule_profile_evidence().as_str().to_owned(),
            ),
            (
                context.key("raw_condition_data_requires_180"),
                summary.requires_180_evidence().as_str().to_owned(),
            ),
        ]);
        if let Some(rule) = summary.post_pc_rule_profile() {
            fields.push((
                context.key("backend_report_post_pc_rule_profile"),
                rule.as_str().to_owned(),
            ));
        }
        if summary.requires_180_evidence().is_modeled() {
            fields.push((
                context.key("raw_condition_data_requires_180_required"),
                summary.requires_180().to_string(),
            ));
        }
    }
}
mod supply_fields {
    use super::{
        formatters::{format_piece_sequence, format_usize_list},
        result_context::SetupResultSummaryContext,
    };

    pub(super) fn append_supply_fields(
        fields: &mut Vec<(String, String)>,
        context: &SetupResultSummaryContext<'_>,
    ) {
        let summary = context.result.setup_raw_metrics();
        fields.extend([
            (
                context.key("queue_prefix"),
                format_piece_sequence(summary.queue_prefix()),
            ),
            (
                context.key("queue_prefix_len"),
                summary.queue_prefix_len().to_string(),
            ),
            (
                context.key("hold_required"),
                summary.hold_required().to_string(),
            ),
            (
                context.key("hold_piece"),
                summary
                    .hold_piece()
                    .map(|piece| piece.as_ascii().to_string())
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            (
                context.key("bag_boundary_offsets"),
                format_usize_list(summary.bag_boundary_offsets()),
            ),
            (
                context.key("bag_boundary_ambiguous"),
                summary.bag_boundary_ambiguous().to_string(),
            ),
        ]);
    }
}

pub(crate) use builder::SetupSummaryBuilder;
pub(crate) use formatters::format_probability;
