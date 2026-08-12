//! Owns the public response boundary that removes private solution authority from summary output.

use clearra_core_executor::CoreExecutionResult;

pub(crate) fn finalize_coverage_summary_public_surface(
    result: CoreExecutionResult,
) -> CoreExecutionResult {
    let availability = result.execution_report().solution_set_availability();
    let fields = result.summary_fields();
    let coverage_summary = fields
        .iter()
        .any(|(key, value)| key == "search_output_policy" && value == "coverage-summary");
    let has_declared_policy = fields.iter().any(|(key, _)| key == "search_output_policy");
    let invalid_declared_contract = (has_declared_policy || availability.uses_explicit_contract())
        && (!availability.contract_valid()
            || !availability
                .materialized_key_count_matches(result.normalized_solution_keys().len()));
    if !coverage_summary && !invalid_declared_contract {
        return result;
    }

    // Worker partitions bypass this public-response boundary and retain filtered authority for
    // their coordinator. Serial execution and the distributed coordinator both arrive here only
    // after all requested post-processing has consumed that authority.
    result.into_fail_closed_public_solution_surface()
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
    };
    use clearra_core_executor::{
        solution_probability::probability_reports, CoreExecutionResult, CorePathStep,
        CorePostProcessScoreCell, CorePostProcessSpinCoverage, FinesseReport,
        NormalizedSolutionCoverage, SolutionAverageScoreReport, SolutionCoverage,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };
    use clearra_output::{
        json::{JsonContract, JsonWriter},
        model::RenderField,
        text::{HumanSummaryFieldPolicy, TextWriter},
    };
    use clearra_replay::SpinCoverageExecutionBatch;

    use crate::AppRenderModel;

    use super::finalize_coverage_summary_public_surface;

    #[test]
    fn coverage_summary_public_surface_removes_materialized_solution_authority() {
        let input = materialized_coverage_summary_result();
        assert_eq!(input.spin_coverage_execution_batches().len(), 1);
        assert!(input.postprocess_execution_complete());
        assert_eq!(input.postprocess_pattern_weights(), &["private-weight"]);
        assert_eq!(input.postprocess_score_cells().len(), 1);
        assert_eq!(input.postprocess_spin_coverages().len(), 1);
        assert_eq!(
            input.finesse_report().map(FinesseReport::mode),
            Some("search")
        );
        let public = finalize_coverage_summary_public_surface(input.clone());

        assert_eq!(public.coverage_pattern_words(), &[1]);
        assert_eq!(public.usize_field("covered_pattern_count"), Some(1));
        assert_eq!(
            public.field("b2b_preservation_evaluation_basis"),
            input.field("b2b_preservation_evaluation_basis")
        );
        assert_eq!(
            public.field("unique_solution_count"),
            Some("not-calculated")
        );
        assert_eq!(
            public.field("normalized_unique_solution_count"),
            Some("not-calculated")
        );
        assert_eq!(
            public.field("b2b_preserving_solution_count"),
            Some("not-calculated")
        );
        for key in [
            "coverage_row_count",
            "b2b_preserving_candidate_pattern_count",
            "pattern_verified_execution_count",
            "original_unique_solution_count",
            "mirror_unique_solution_count",
            "minimum_cover_source_solution_count",
            "minimum_cover_selected_solution_count",
        ] {
            assert_eq!(public.field(key), Some("not-calculated"), "{key}");
        }
        assert_eq!(
            public.field("mirror_normalized_solution_set_hash"),
            Some("not-calculated")
        );
        assert_eq!(public.bool_field("solution_count_calculated"), Some(false));
        assert_eq!(public.bool_field("solution_set_materialized"), Some(false));
        assert_eq!(
            public.usize_field("solution_keys_materialized_count"),
            Some(0)
        );
        assert_eq!(public.bool_field("solution_keys_complete"), Some(false));
        assert_eq!(public.bool_field("solution_page_available"), Some(false));
        assert_eq!(
            public.field("normalized_solution_set_hash"),
            Some("not-calculated")
        );
        assert_eq!(
            public.field("actual_normalized_solution_set_hash"),
            Some("not-calculated")
        );
        assert!(public.packing_candidate_keys().is_empty());
        assert!(public.path_steps().is_empty());
        assert!(public.representative_solution_identity().is_none());
        assert!(public.normalized_solution_keys().is_empty());
        assert!(public.normalized_solution_identities().is_empty());
        assert!(public.solution_coverages().is_empty());
        assert!(public.normalized_solution_coverages().is_empty());
        assert!(public.solution_probabilities().is_empty());
        assert!(public.solution_average_scores().is_empty());
        assert!(public.exact_scoring_execution_batches().is_empty());
        assert!(public.spin_coverage_execution_batches().is_empty());
        assert!(public.postprocess_executions().is_empty());
        assert!(!public.postprocess_execution_complete());
        assert!(public.postprocess_pattern_weights().is_empty());
        assert!(public.postprocess_replay_trace().is_none());
        assert!(public.postprocess_score_cells().is_empty());
        assert!(!public.postprocess_score_cells_complete());
        assert!(public.postprocess_score_profile_id().is_none());
        assert!(public.postprocess_spin_coverages().is_empty());
        assert!(public.finesse_report().is_none());
        assert!(public.tiling_solution_page_store().is_none());
    }

    #[test]
    fn malformed_declared_contract_is_physically_hidden_from_typed_core_result() {
        let malformed = materialized_coverage_summary_result().with_replaced_fields(vec![(
            "search_output_policy".to_owned(),
            "coverage-summray".to_owned(),
        )]);
        let model = AppRenderModel::Percent(finalize_coverage_summary_public_surface(malformed));
        let public = model.core_result().expect("typed core result");

        assert_eq!(public.field("search_output_policy"), None);
        assert_eq!(
            public.field("unique_solution_count"),
            Some("not-calculated")
        );
        assert_eq!(
            public.field("normalized_solution_set_hash"),
            Some("not-calculated")
        );
        assert_eq!(public.bool_field("solution_set_materialized"), Some(false));
        assert!(public.normalized_solution_keys().is_empty());
        assert!(public.spin_coverage_execution_batches().is_empty());
        assert!(public.postprocess_score_cells().is_empty());
        assert!(public.postprocess_spin_coverages().is_empty());
        assert!(public.finesse_report().is_none());
        assert!(public.tiling_solution_page_store().is_none());
    }

    #[test]
    fn explicit_markers_without_a_policy_fail_closed_but_legacy_results_remain_unchanged() {
        let invalid = CoreExecutionResult::new(
            vec![
                ("unique_solution_count".to_owned(), "1".to_owned()),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec!["private-key".to_owned()])
        .with_finesse_report(FinesseReport::new(
            "search",
            "oracle",
            true,
            None,
            Vec::new(),
        ));
        let public = finalize_coverage_summary_public_surface(invalid);
        assert_eq!(
            public.field("unique_solution_count"),
            Some("not-calculated")
        );
        assert!(public.normalized_solution_keys().is_empty());
        assert!(public.finesse_report().is_none());

        let legacy = CoreExecutionResult::new(
            vec![("unique_solution_count".to_owned(), "1".to_owned())],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec!["legacy-key".to_owned()]);
        let expected = legacy.clone();
        assert_eq!(finalize_coverage_summary_public_surface(legacy), expected);
    }

    #[test]
    fn finesse_score_is_the_only_report_exception_on_an_unavailable_solution_surface() {
        let input = CoreExecutionResult::new(
            vec![
                (
                    "search_output_policy".to_owned(),
                    "coverage-summary".to_owned(),
                ),
                (
                    "unique_solution_count".to_owned(),
                    "not-calculated".to_owned(),
                ),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "not-calculated".to_owned(),
                ),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "not-calculated".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "not-calculated".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "false".to_owned()),
                ("solution_set_materialized".to_owned(), "false".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "0".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "false".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
            ],
            Vec::new(),
        )
        .with_finesse_report(FinesseReport::new(
            "score",
            "oracle",
            true,
            Some("1".to_owned()),
            Vec::new(),
        ));

        let public = finalize_coverage_summary_public_surface(input);
        assert_eq!(
            public.finesse_report().map(FinesseReport::mode),
            Some("score")
        );
    }

    #[test]
    fn coverage_summary_json_and_text_do_not_leak_solution_count_aliases() {
        let public =
            finalize_coverage_summary_public_surface(materialized_coverage_summary_result());
        let render_fields = public
            .summary_fields()
            .into_iter()
            .map(|(key, value)| RenderField::new(key, value))
            .collect::<Vec<_>>();
        let json = JsonWriter::write(&JsonContract::from_render_message(
            "percent",
            &render_fields,
        ));
        let text = TextWriter::lines(
            &public
                .summary_fields()
                .into_iter()
                .filter(|(key, _)| HumanSummaryFieldPolicy::include_field("percent", key))
                .map(|(key, value)| TextWriter::line(&key, value))
                .collect::<Vec<_>>(),
        );

        assert!(!json.contains("solution-count-secret-47"), "{json}");
        assert!(!json.contains("\"47\""), "{json}");
        assert!(!text.contains("47"), "{text}");
    }

    fn materialized_coverage_summary_result() -> CoreExecutionResult {
        let identity = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty())
            .expect("empty identity");
        let patterns = PatternBitSet::from_words(1, vec![1]).expect("coverage bitset");
        let board64_coverage = SolutionCoverage::new(identity, patterns.clone());
        let probabilities = probability_reports(
            &[identity],
            std::slice::from_ref(&board64_coverage),
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            true,
        );

        CoreExecutionResult::new(
            vec![
                (
                    "search_output_policy".to_owned(),
                    "coverage-summary".to_owned(),
                ),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "true".to_owned()),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "materialized".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "materialized".to_owned(),
                ),
                ("b2b_preserving_solution_count".to_owned(), "1".to_owned()),
                ("coverage_row_count".to_owned(), "47".to_owned()),
                (
                    "b2b_preserving_candidate_pattern_count".to_owned(),
                    "47".to_owned(),
                ),
                (
                    "pattern_verified_execution_count".to_owned(),
                    "47".to_owned(),
                ),
                ("original_unique_solution_count".to_owned(), "47".to_owned()),
                ("mirror_unique_solution_count".to_owned(), "47".to_owned()),
                (
                    "minimum_cover_source_solution_count".to_owned(),
                    "47".to_owned(),
                ),
                (
                    "minimum_cover_selected_solution_count".to_owned(),
                    "47".to_owned(),
                ),
                (
                    "mirror_normalized_solution_set_hash".to_owned(),
                    "solution-count-secret-47".to_owned(),
                ),
                ("covered_pattern_count".to_owned(), "1".to_owned()),
                (
                    "b2b_preservation_evaluation_basis".to_owned(),
                    "candidate-pattern-existence".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_packing_candidate_keys(vec!["packing-candidate".to_owned()])
        .with_path_steps(vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)])
        .with_representative_solution_identity(Some(identity))
        .with_normalized_solution_keys(vec!["preserved-candidate".to_owned()])
        .with_normalized_solution_identities(vec![identity])
        .with_coverage_pattern_words(vec![1])
        .with_solution_coverages(vec![board64_coverage])
        .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
            "preserved-candidate",
            patterns,
        )])
        .with_solution_probabilities(probabilities)
        .with_solution_average_scores(vec![SolutionAverageScoreReport::new(
            "preserved-candidate",
            "100",
            1,
            1,
            true,
        )])
        .with_finesse_report(FinesseReport::new(
            "search",
            "oracle",
            true,
            None,
            Vec::new(),
        ))
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I]],
            0,
            None,
            true,
            false,
            false,
            1,
            1,
            Vec::new(),
            true,
        )))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["private-weight".to_owned()])
        .with_postprocess_score_cells(
            vec![CorePostProcessScoreCell::new(
                identity,
                0,
                "private-trace",
                100,
                1,
            )],
            true,
            "tetrio",
        )
        .with_postprocess_spin_coverages(vec![CorePostProcessSpinCoverage::new(
            "private-target",
            0,
            1,
            vec![1],
            vec!["private-candidate".to_owned()],
            1,
            true,
        )])
    }
}
