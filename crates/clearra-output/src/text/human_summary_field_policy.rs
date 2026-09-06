#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HumanSummaryFieldPolicy;

impl HumanSummaryFieldPolicy {
    pub fn include_field(kind: &str, key: &str) -> bool {
        if is_common_summary_field(key) || is_common_result_field(key) {
            return true;
        }

        if is_probability_query_field(kind, key) {
            return true;
        }

        match kind {
            "pc" | "pc-scenario" | "path" | "percent" | "continue" => {
                is_pc_family_summary_field(key)
            }
            "failed-queue" => is_probability_query_field("percent", key),
            "build-probability" => is_legacy_build_probability_summary_field(key),
            "damage" | "spin-finder" | "ren" => is_forward_search_summary_field(key),
            "spin-structure" => is_legacy_spin_structure_summary_field(key),
            "build_coverage" => is_cover_summary_field(key),
            "rules" => is_rules_summary_field(key),
            "scoring" => is_scoring_summary_field(key),
            "convert" => is_convert_summary_field(key),
            "verify" | "verify-kicks" => is_verify_summary_field(key),
            "sequence" => is_sequence_summary_field(key),
            "sequence-dependencies" => is_sequence_dependencies_summary_field(key),
            "parity" => is_parity_summary_field(key),
            "fumen" => is_document_summary_field(key),
            "render" => is_render_summary_field(key),
            "to-gray" | "mirror" => is_document_transform_summary_field(key),
            "pc-tiling-family.v1" => is_pc_solution_family_summary_field(key),
            "pc-save-groups.v2" => is_pc_save_summary_field(key),
            "pc-best-save.v2" => is_pc_best_save_summary_field(key),
            "pc-minimum-cover.v2" | "pc-score-portfolio.v2" => is_portfolio_summary_field(key),
            "pc-path-family.v2" | "build-path-family.v1" => is_path_family_summary_field(key),
            "pc-probability.v2" | "pc-failed-queue.v2" | "pc-b2b-preservation-probability.v1" => {
                is_pc_probability_summary_field(key)
            }
            "pc-score-summary.v2" | "build-field-average-score.v1" => {
                is_field_average_score_summary_field(key)
            }
            "pc-fixed-score-witness.v2" | "build-fixed-score-witness.v1" => {
                is_score_witness_summary_field(key)
            }
            "pc-score-finder.v1" => is_score_winner_family_summary_field(key),
            "pc-b2b-preserving-witness.v1" => is_pc_b2b_witness_summary_field(key),
            "build-coverage-portfolio.v2" | "build-probability-score-minimum.v1" => {
                is_build_portfolio_summary_field(key)
            }
            "build-coverage.v2"
            | "build-congruence-family.v1"
            | "build-congruence-coverage.v1"
            | "build-setup-cover.v1"
            | "build-setup-cover-probability.v1"
            | "build-setup-cover-score.v1"
            | "build-supplied-coverage.v1"
            | "build-supplied-minimum-cover.v1"
            | "build-supplied-score.v1"
            | "build-supplied-b2b-coverage.v1"
            | "build-supplied-probability.v1" => is_build_v2_summary_field(key),
            "build-target-family.v2" => is_build_target_family_summary_field(key),
            "setup-build-ranking.v2" | "setup-joint-ranking.v2" | "setup-pc-ranking.v2" => {
                is_setup_ranked_summary_field(key)
            }
            "setup-score-ranking.v1" => is_setup_score_summary_field(key),
            "spin-structure-family.v2"
            | "spin-structure-guaranteed.v1"
            | "spin-structure-coverage.v1" => is_spin_structure_summary_field(key),
            "portfolio-alternative-page.v1" => is_portfolio_summary_field(key),
            "parity-report.v1" => is_parity_summary_field(key),
            "field-document.v1" => is_document_transform_summary_field(key),
            "field-document-set.v1" => is_document_summary_field(key),
            "render-artifact.v1" => is_render_summary_field(key),
            "setup" => is_setup_summary_field(key),
            "cover" => is_cover_summary_field(key),
            // Default human text is a public product surface. New typed result
            // contracts must opt in field-by-field instead of inheriting a
            // machine payload dump when a producer adds a schema.
            _ => false,
        }
    }

    pub fn human_facing_kind<'a>(kind: &'a str) -> &'a str {
        match kind {
            "pc-tiling-family.v1" => "perfect-clear solutions",
            "pc-save-groups.v2" => "save groups",
            "pc-best-save.v2" => "best save",
            "pc-minimum-cover.v2" => "minimum solutions",
            "pc-path-family.v2" => "perfect-clear replay paths",
            "pc-probability.v2" => "perfect-clear probability",
            "pc-failed-queue.v2" => "failed queues",
            "pc-score-summary.v2" => "perfect-clear field average score",
            "pc-fixed-score-witness.v2" => "fixed-queue maximum score",
            "pc-score-portfolio.v2" => "highest-score minimum solutions",
            "pc-score-finder.v1" => "per-pattern highest-score solutions",
            "pc-b2b-preserving-witness.v1" => "back-to-back preserving solutions",
            "pc-b2b-preservation-probability.v1" => "back-to-back preservation probability",
            "build-coverage-portfolio.v2" => "minimum build solutions",
            "build-target-family.v2" => "build solutions",
            "build-path-family.v1" => "build replay paths",
            "build-field-average-score.v1" => "build field average score",
            "build-fixed-score-witness.v1" => "build fixed-queue maximum score",
            "build-probability-score-minimum.v1" => "highest-score minimum build solutions",
            "build-coverage.v2"
            | "build-congruence-family.v1"
            | "build-congruence-coverage.v1"
            | "build-setup-cover.v1"
            | "build-setup-cover-probability.v1"
            | "build-setup-cover-score.v1"
            | "build-supplied-coverage.v1"
            | "build-supplied-minimum-cover.v1"
            | "build-supplied-score.v1"
            | "build-supplied-b2b-coverage.v1"
            | "build-supplied-probability.v1" => "build result",
            "setup-build-ranking.v2" | "setup-joint-ranking.v2" | "setup-pc-ranking.v2" => {
                "setup ranking"
            }
            "setup-score-ranking.v1" => "setup score ranking",
            "spin-structure-family.v2"
            | "spin-structure-guaranteed.v1"
            | "spin-structure-coverage.v1" => "spin structures",
            "portfolio-alternative-page.v1" => "solution portfolio page",
            "parity-report.v1" => "parity report",
            "field-document.v1" => "field document",
            "field-document-set.v1" => "field documents",
            "render-artifact.v1" => "rendered artifact",
            "build_coverage" => "build coverage",
            "sequence" => "operation sequence",
            "sequence-dependencies" => "operation dependencies",
            "to-gray" => "grayscale field document",
            "mirror" => "mirrored field document",
            _ if is_legacy_result_kind(kind) => kind,
            _ => "result",
        }
    }
}

pub(crate) fn is_versioned_result_kind(kind: &str) -> bool {
    kind.rsplit_once(".v").is_some_and(|(name, version)| {
        !name.is_empty() && !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub(crate) fn is_known_result_kind(kind: &str) -> bool {
    is_legacy_result_kind(kind) || is_versioned_result_kind(kind)
}

fn is_legacy_result_kind(kind: &str) -> bool {
    matches!(
        kind,
        "pc" | "pc-scenario"
            | "path"
            | "percent"
            | "failed-queue"
            | "setup"
            | "build-probability"
            | "damage"
            | "spin-finder"
            | "ren"
            | "spin-structure"
            | "build_coverage"
            | "cover"
            | "rules"
            | "scoring"
            | "convert"
            | "continue"
            | "verify"
            | "verify-kicks"
            | "sequence"
            | "sequence-dependencies"
            | "parity"
            | "fumen"
            | "render"
            | "to-gray"
            | "mirror"
    )
}

fn is_common_summary_field(key: &str) -> bool {
    matches!(
        key,
        "status"
            | "problem_preset"
            | "preset"
            | "lines"
            | "queue_len"
            | "queue_mode"
            | "hold_enabled"
            | "rule_profile"
            | "solver_backend"
            | "unsupported_reason"
            | "warning_count"
            | "diagnostic_count"
    )
}

fn is_common_result_field(key: &str) -> bool {
    matches!(
        key,
        "solution_found"
            | "total_solution_count"
            | "unique_solution_count"
            | "coverage_probability"
            | "covered_pattern_count"
            | "count_complete"
            | "retained_trace_count"
            | "trace_retention_truncated"
            | "trace_retention_reason"
            | "next_pc_available"
            | "continuation_token_available"
            | "continue_hint"
            | "continuation_kind"
    )
}

fn is_probability_query_field(kind: &str, key: &str) -> bool {
    matches!(
        kind,
        "percent"
            | "setup"
            | "pc-probability.v2"
            | "pc-failed-queue.v2"
            | "pc-b2b-preservation-probability.v1"
    ) && (matches!(
        key,
        "materialized_probability_mass"
            | "probability_complete"
            | "renormalized"
            | "truncation_reason"
            | "result_mode"
            | "failed_queue_probability"
            | "total_pattern_count"
            | "failed_pattern_count"
            | "failed_pattern_scope"
            | "failed_pattern_count_complete"
            | "failed_pattern_limit"
            | "failed_pattern_examples_materialized"
            | "failed_pattern_examples_truncated"
    ) || indexed_field_suffix(key, "failed_pattern_").is_some_and(|suffix| suffix.is_empty()))
}

fn is_pc_family_summary_field(key: &str) -> bool {
    matches!(key, "objective" | "interactive_prompt")
}

fn is_legacy_build_probability_summary_field(key: &str) -> bool {
    is_pc_family_summary_field(key)
        || is_probability_query_field("percent", key)
        || matches!(
            key,
            "aggregate"
                | "source_candidate_count"
                | "reachable_candidate_count"
                | "selected_candidate_count"
                | "pattern_count"
                | "required_pattern_count"
                | "union_probability"
                | "complete"
        )
}

fn is_forward_search_summary_field(key: &str) -> bool {
    matches!(
        key,
        "complete"
            | "workers_used"
            | "visited_states"
            | "generated_locks"
            | "peak_frontier"
            | "maximum_damage"
            | "maximum_ren"
            | "solution_data_status"
    )
}

fn is_legacy_spin_structure_summary_field(key: &str) -> bool {
    matches!(
        key,
        "complete"
            | "workers_used"
            | "spin_profile"
            | "minimality"
            | "line_requirement"
            | "minimum_placements"
            | "result_count"
            | "regular_count"
            | "mini_count"
            | "solution_data_status"
    )
}

fn is_rules_summary_field(key: &str) -> bool {
    matches!(
        key,
        "action"
            | "profile"
            | "profile_count"
            | "label"
            | "source_kind"
            | "source_description"
            | "source_rule"
            | "rule_profile"
            | "kick_profile"
            | "effective_kick_model"
            | "supports_180"
            | "supports_exact_180"
            | "requires_lock_reachability"
            | "requires_spawn_reachability"
            | "search_backend_supported"
            | "c_compact_descriptor_ready"
            | "verified_profile"
            | "verification_status"
            | "transition_complete"
            | "transition_count"
            | "issue_count"
            | "missing_transition_count"
            | "duplicate_transition_count"
            | "unsupported_annotation_count"
            | "kick_profile_registry_count"
            | "kick_verification_cases"
            | "kick_verification_failures"
            | "srs_plus_180_transitions"
            | "jstris_180_transitions"
            | "unsupported_reason"
            | "unsupported_backend_reason"
    ) || indexed_field_suffix(key, "profile_").is_some_and(|suffix| {
        matches!(
            suffix,
            "kick_profile"
                | "label"
                | "source_kind"
                | "source_description"
                | "supports_180"
                | "supports_exact_180"
                | "search_backend_supported"
                | "c_compact_descriptor_ready"
                | "unsupported_backend_reason"
                | "unsupported_reason"
        )
    })
}

fn is_scoring_summary_field(key: &str) -> bool {
    matches!(key, "action" | "profile" | "profile_count" | "json")
        || scoring_profile_public_suffix(key).is_some()
}

fn scoring_profile_public_suffix(key: &str) -> Option<&str> {
    let suffix = indexed_field_suffix(key, "profile_").unwrap_or(key);
    matches!(
        suffix,
        "id" | "display_name"
            | "score_model"
            | "attack_model"
            | "spin_rule"
            | "accuracy_level"
            | "profile_specific_exact"
            | "accuracy_reason"
            | "combo_enabled"
            | "combo_score_bonus_per_combo"
            | "combo_attack_bonus_per_combo"
            | "b2b_enabled"
            | "b2b_score_bonus"
            | "b2b_attack_bonus"
    )
    .then_some(suffix)
}

fn is_convert_summary_field(key: &str) -> bool {
    matches!(key, "from" | "to" | "page_count")
        || indexed_field_suffix(key, "page_").is_some_and(|suffix| suffix.is_empty())
}

fn indexed_field_suffix<'a>(key: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = key.strip_prefix(prefix)?;
    let digit_count = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    let suffix = &rest[digit_count..];
    if suffix.is_empty() {
        return Some(suffix);
    }
    suffix.strip_prefix('_')
}

fn is_verify_summary_field(key: &str) -> bool {
    matches!(
        key,
        "scope"
            | "probe_result_kind"
            | "probes_attempted"
            | "probes_passed"
            | "probes_failed"
            | "pc"
            | "setup"
            | "build_coverage"
            | "kicks"
            | "kick_verification_cases"
            | "kick_verification_failures"
            | "srs_jlstz_transitions"
            | "srs_i_transitions"
            | "srs_o_model"
            | "no_kick_transitions"
            | "srs_plus_effective_kick_model"
            | "srs_plus_180_transitions"
            | "jstris_180_transitions"
            | "kick_profile_registry_count"
            | "srs_plus_extension_reason"
    )
}

fn is_sequence_summary_field(key: &str) -> bool {
    matches!(
        key,
        "complete"
            | "width"
            | "height"
            | "operation_count"
            | "cleared_line_count"
            | "rule_profile"
            | "kick_profile"
    )
}

fn is_sequence_dependencies_summary_field(key: &str) -> bool {
    matches!(
        key,
        "complete"
            | "operation_count"
            | "exact_order_count"
            | "solution_count"
            | "universal_dependency_count"
            | "transitive_reduction_count"
            | "independent_pair_count"
            | "explored_state_count"
            | "live_transition_count"
            | "rule_profile"
            | "kick_profile"
            | "universal_dependencies_preview_truncated"
    )
}

fn is_parity_summary_field(key: &str) -> bool {
    matches!(
        key,
        "document_format"
            | "page_number"
            | "total_pages"
            | "coordinate_basis"
            | "occupied_cell_count"
            | "pending_garbage_occupied_cell_count"
            | "feasibility_claim"
            | "pruning_authority"
    )
}

fn is_document_summary_field(key: &str) -> bool {
    matches!(key, "format" | "transform" | "document_count")
}

fn is_document_transform_summary_field(key: &str) -> bool {
    matches!(key, "format" | "transform" | "page_count")
}

fn is_render_summary_field(key: &str) -> bool {
    matches!(
        key,
        "document_format"
            | "artifact_format"
            | "document_page_count"
            | "byte_length"
            | "render_exact"
    )
}

fn is_pc_solution_family_summary_field(key: &str) -> bool {
    matches!(
        key,
        "objective"
            | "interactive_prompt"
            | "tiling_family_complete"
            | "tiling_initial_page_count"
            | "tiling_initial_page_complete"
            | "tiling_initial_page_covers_family"
    )
}

fn is_pc_save_summary_field(key: &str) -> bool {
    matches!(
        key,
        "save_origin"
            | "save_problem_preset"
            | "save_materialized_pattern_count"
            | "save_pc_success_pattern_count"
            | "save_pc_probability"
    )
}

fn is_pc_best_save_summary_field(key: &str) -> bool {
    matches!(
        key,
        "best_save_probability_basis"
            | "best_save_origin"
            | "best_save_problem_preset"
            | "best_save_materialized_pattern_count"
            | "best_save_pc_success_pattern_count"
            | "best_save_pc_probability"
    )
}

fn is_portfolio_summary_field(key: &str) -> bool {
    matches!(
        key,
        "alternative_index"
            | "optimal_cardinality"
            | "known_alternative_count"
            | "total_alternative_count"
            | "enumeration_complete"
            | "member_page_number"
            | "total_member_pages"
    )
}

fn is_path_family_summary_field(key: &str) -> bool {
    matches!(
        key,
        "materialized_pattern_count" | "witness_count" | "complete"
    )
}

fn is_pc_probability_summary_field(key: &str) -> bool {
    is_probability_query_field("pc-probability.v2", key)
        || matches!(
            key,
            "probability"
                | "probability_complete"
                | "materialized_pattern_count"
                | "successful_pattern_count"
                | "b2b_preserving_pattern_count"
                | "b2b_preservation_probability"
                | "b2b_preservation_probability_complete"
        )
}

fn is_field_average_score_summary_field(key: &str) -> bool {
    matches!(
        key,
        "materialized_pattern_count"
            | "score_solution_field_count"
            | "score_success_pattern_count"
            | "score_failed_pc_pattern_count"
            | "score_covered_probability"
            | "score_overall_score"
            | "score_covered_pattern_conditional_average_score"
            | "score_summary_complete"
    )
}

fn is_score_witness_summary_field(key: &str) -> bool {
    matches!(
        key,
        "score_pattern_winner_count"
            | "score_pattern_winner_complete"
            | "score_best_score"
            | "score_best_attack"
            | "score_evaluation_complete"
    )
}

fn is_score_winner_family_summary_field(key: &str) -> bool {
    matches!(
        key,
        "score_pattern_winner_count" | "score_pattern_winner_complete"
    )
}

fn is_pc_b2b_witness_summary_field(key: &str) -> bool {
    matches!(
        key,
        "b2b_preserving_solution_count"
            | "b2b_preserving_pattern_count"
            | "b2b_preserving_candidate_pattern_count"
            | "b2b_preservation_witness_available"
    )
}

fn is_build_portfolio_summary_field(key: &str) -> bool {
    is_portfolio_summary_field(key)
        || matches!(
            key,
            "objective"
                | "probability_basis"
                | "source_candidate_count"
                | "selected_candidate_count"
                | "pattern_count"
                | "required_pattern_count"
                | "union_probability"
        )
}

fn is_build_v2_summary_field(key: &str) -> bool {
    matches!(
        key,
        "objective"
            | "score_profile"
            | "initial_b2b"
            | "score_accuracy"
            | "profile_specific_exact"
            | "source_candidate_count"
            | "reachable_candidate_count"
            | "selected_candidate_count"
            | "pattern_count"
            | "covered_pattern_count"
            | "required_pattern_count"
            | "union_probability"
            | "b2b_preservation_required"
    )
}

fn is_build_target_family_summary_field(key: &str) -> bool {
    is_build_v2_summary_field(key)
}

fn is_setup_ranked_summary_field(key: &str) -> bool {
    matches!(
        key,
        "rule_profile" | "resolved_length_preference" | "candidate_count"
    )
}

fn is_setup_score_summary_field(key: &str) -> bool {
    matches!(
        key,
        "document_format"
            | "rule_profile"
            | "score_profile"
            | "initial_b2b"
            | "source_page_count"
            | "candidate_count"
            | "setup_pattern_count"
            | "average_priority_score"
            | "complete"
    )
}

fn is_spin_structure_summary_field(key: &str) -> bool {
    matches!(
        key,
        "rule_profile"
            | "spin_profile"
            | "minimum_placements"
            | "guaranteed_final_piece"
            | "guarantee_basis"
            | "dependency_report_included"
            | "dependency_relation"
            | "dependency_edge_count"
            | "regular_count"
            | "mini_count"
            | "candidate_count"
            | "complete"
    )
}

fn is_setup_summary_field(key: &str) -> bool {
    matches!(
        key,
        "build_variant_count" | "tiling_variant_count" | "post_pc_solution_count" | "score_basis"
    )
}

fn is_cover_summary_field(key: &str) -> bool {
    matches!(
        key,
        "template" | "action" | "exported" | "coverage_row_source"
    )
}

#[cfg(test)]
mod tests {
    use super::HumanSummaryFieldPolicy;

    #[test]
    fn percent_human_output_keeps_failed_queue_fields() {
        for key in [
            "result_mode",
            "failed_queue_probability",
            "failed_pattern_count",
            "failed_pattern_0",
        ] {
            assert!(
                HumanSummaryFieldPolicy::include_field("percent", key),
                "{key}"
            );
        }
        assert!(!HumanSummaryFieldPolicy::include_field(
            "pc",
            "failed_pattern_0"
        ));
        for private_key in [
            "failed_queue_contract",
            "failed_pattern_candidate_id",
            "failed_pattern_0_candidate_id",
        ] {
            assert!(!HumanSummaryFieldPolicy::include_field(
                "percent",
                private_key
            ));
        }
    }

    #[test]
    fn legacy_dynamic_fields_require_an_index_and_an_explicit_public_suffix() {
        for (kind, key) in [
            ("rules", "profile_0_label"),
            ("rules", "profile_12_supports_180"),
            ("scoring", "profile_3_display_name"),
            ("convert", "page_0"),
        ] {
            assert!(
                HumanSummaryFieldPolicy::include_field(kind, key),
                "{kind}:{key}"
            );
        }
        for (kind, key) in [
            ("rules", "profile_label"),
            ("rules", "profile_0_candidate_id"),
            ("rules", "capability_candidate_id"),
            ("scoring", "profile_0_candidate_id"),
            ("convert", "page_candidate_id"),
            ("convert", "page_0_candidate_id"),
        ] {
            assert!(
                !HumanSummaryFieldPolicy::include_field(kind, key),
                "{kind}:{key}"
            );
        }
    }

    #[test]
    fn path_human_output_keeps_only_public_counts_and_completion() {
        for kind in ["pc-path-family.v2", "build-path-family.v1"] {
            for key in ["materialized_pattern_count", "witness_count", "complete"] {
                assert!(
                    HumanSummaryFieldPolicy::include_field(kind, key),
                    "{kind}:{key}"
                );
            }
            for key in [
                "capability_id",
                "witness_contract",
                "ordering",
                "problem_id",
                "target_terminal_board_mask",
                "canonical_selection",
                "canonical_witness",
                "witnesses",
            ] {
                assert!(
                    !HumanSummaryFieldPolicy::include_field(kind, key),
                    "{kind}:{key}"
                );
            }
        }
    }
}
