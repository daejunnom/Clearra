mod counters {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub(super) struct SetupEnumerationCounters {
        pub(super) next_tiling_variant_id: u32,
        pub(super) next_build_variant_id: u32,
        pub(super) tiling_variant_count: usize,
        pub(super) build_variant_count: usize,
        pub(super) tiling_variant_enumeration_complete: bool,
        pub(super) build_variant_enumeration_complete: bool,
    }

    impl SetupEnumerationCounters {
        pub(super) fn new() -> Self {
            Self {
                tiling_variant_enumeration_complete: true,
                build_variant_enumeration_complete: true,
                ..Self::default()
            }
        }
    }
    impl SetupEnumerationCounters {
        pub(super) fn take_tiling_id(&mut self) -> u32 {
            let id = self.next_tiling_variant_id;
            self.next_tiling_variant_id += 1;
            self.tiling_variant_count += 1;
            id
        }
    }
    impl SetupEnumerationCounters {
        pub(super) fn take_build_id(&mut self) -> u32 {
            let id = self.next_build_variant_id;
            self.next_build_variant_id += 1;
            self.build_variant_count += 1;
            id
        }
    }
}
mod error {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SetupSearchExecutionError {
        EmptyQueue,
        ExpandObservedQueue,
        BuildCoverage,
        EvaluateCoverage,
        CoreBuildUp,
    }
}
mod family_evaluator {
    use std::collections::BTreeSet;

    use clearra_core_domain::ids::setup_id::{BuildVariantId, TilingVariantId};
    use clearra_objectives::max_score::MaxScoreCoverPolicy;
    use clearra_scoring::profile::ScoreProfile;

    use crate::{
        enumerate::{BuildVariantEnumerator, TilingEnumerator},
        evaluate::{SetupEvaluator, SetupRawMetrics},
        identity::shape_family::ShapeFamily,
        query::SetupSearchQuery,
        result::{SetupBuildScoreInput, SetupFamilyScore, SetupResult, SetupScoreAggregation},
        variant::build_variant::BuildVariant,
    };

    use super::{counters::SetupEnumerationCounters, SetupSearchExecutionError};
    use crate::service::{
        setup_candidate_enumerator::SetupBuildCandidate,
        setup_core_buildup_gate::SetupCoreBuildGate,
        setup_coverage_plan::{coverage_for_patterns, SetupCoveragePlan},
        setup_family_grouper::{build_groups_for_tiling, tiling_groups_for_family},
        setup_pattern_source::SetupPatternSource,
        setup_post_pc_adapter::{evaluate_post_pc, SetupPostPcEvaluation},
    };

    pub(super) struct FamilyEvaluation {
        pub(super) result: SetupResult,
        pub(super) score: SetupFamilyScore,
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_family(
        query: &SetupSearchQuery,
        source: &SetupPatternSource,
        core_build_gate: &SetupCoreBuildGate,
        candidates: &[SetupBuildCandidate],
        family: ShapeFamily,
        score_profile: Option<&ScoreProfile>,
        counters: &mut SetupEnumerationCounters,
    ) -> Result<Option<FamilyEvaluation>, SetupSearchExecutionError> {
        let family_candidates = candidates
            .iter()
            .filter(|candidate| candidate.occupied_shape == family.occupied_shape())
            .cloned()
            .collect::<Vec<_>>();
        let tiling_groups = tiling_groups_for_family(&family_candidates);
        if tiling_groups.len() > query.limits().max_tiling_variants_per_family() {
            counters.tiling_variant_enumeration_complete = false;
        }

        let mut coverage_plan = SetupCoveragePlan::new(family, source.pattern_count);
        let mut family_builds = Vec::new();
        let mut score_inputs = Vec::new();
        let mut representative_post_pc: Option<SetupPostPcEvaluation> = None;

        for (tiling_key, tiling_candidates) in tiling_groups
            .into_iter()
            .take(query.limits().max_tiling_variants_per_family())
        {
            let tiling = TilingEnumerator::single_tiling(
                TilingVariantId::new(counters.take_tiling_id()),
                family,
                tiling_key.pieces,
            );
            let build_groups = build_groups_for_tiling(&tiling_candidates);
            if build_groups.len() > query.limits().max_build_variants_per_tiling() {
                counters.build_variant_enumeration_complete = false;
            }
            for (build_key, build_candidates) in build_groups
                .into_iter()
                .take(query.limits().max_build_variants_per_tiling())
            {
                let representative = build_candidates
                    .first()
                    .ok_or(SetupSearchExecutionError::CoreBuildUp)?;
                let coverage = coverage_for_patterns(
                    source.pattern_count,
                    build_candidates
                        .iter()
                        .map(|candidate| candidate.pattern_index),
                )?;
                let variant = BuildVariantEnumerator::from_core_buildup(
                    BuildVariantId::new(counters.take_build_id()),
                    &tiling,
                    build_key.final_hold,
                    coverage,
                    core_build_gate.proof_for_candidate(representative),
                )
                .ok_or(SetupSearchExecutionError::CoreBuildUp)?;
                coverage_plan.push_variant(&variant)?;
                let post_pc = evaluate_post_pc(query, &variant, &build_key, score_profile);
                if representative_post_pc.as_ref().is_none_or(|current| {
                    !current.evaluation().solution_found() && post_pc.evaluation().solution_found()
                }) {
                    representative_post_pc = Some(post_pc.clone());
                }
                score_inputs.push(SetupBuildScoreInput::from_build_variant(
                    family.id(),
                    &variant,
                    post_pc.into_evaluation(),
                ));
                family_builds.push(variant);
            }
        }

        if family_builds.is_empty() {
            return Ok(None);
        }
        let union = coverage_plan.build_union()?;
        let representative_post_pc = representative_post_pc.unwrap_or_else(|| {
            SetupPostPcEvaluation::unsupported(
                "setup search did not generate a post-PC build candidate",
            )
        });
        let result = SetupEvaluator::evaluate_union(union, &source.weights)
            .map_err(|_| SetupSearchExecutionError::EvaluateCoverage)?
            .with_setup_raw_metrics(SetupRawMetrics::from_query(
                query,
                1,
                family_builds
                    .iter()
                    .map(BuildVariant::tiling_variant_id)
                    .collect::<BTreeSet<_>>()
                    .len(),
                &family_builds,
                representative_post_pc.requires_180(),
                representative_post_pc.rule_profile_evidence(),
                representative_post_pc.into_evaluation(),
            ));
        let score = SetupScoreAggregation::aggregate_family(
            family.id(),
            &score_inputs,
            &source.weights,
            MaxScoreCoverPolicy::default(),
        )
        .map_err(|_| SetupSearchExecutionError::EvaluateCoverage)?;
        Ok(Some(FamilyEvaluation { result, score }))
    }
}
mod result {
    use crate::{
        result::{SetupFamilyScore, SetupResult},
        service::setup_summary_builder::SetupSummaryBuilder,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct SetupSearchExecutionResult {
        pub(crate) status: &'static str,
        pub(crate) execution_scope: &'static str,
        pub(crate) enumeration_strategy: &'static str,
        pub(crate) shape_family_enumeration_complete: bool,
        pub(crate) tiling_variant_enumeration_complete: bool,
        pub(crate) build_variant_enumeration_complete: bool,
        pub(crate) post_pc_mode: &'static str,
        pub(crate) post_pc_evaluation_attached: bool,
        pub(crate) setup_foundation_reason: &'static str,
        pub(crate) executor_flow: &'static str,
        pub(crate) build_variant_source: &'static str,
        pub(crate) packing_candidate_count: usize,
        pub(crate) core_buildup_variant_count: usize,
        pub(crate) core_coverage_row_count: usize,
        pub(crate) coverage_source: &'static str,
        pub(crate) coverage_pattern_count: usize,
        pub(crate) verified_pattern_count: usize,
        pub(crate) materialized_pattern_count: usize,
        pub(crate) covered_pattern_count_basis: &'static str,
        pub(crate) queue_mode: &'static str,
        pub(crate) queue_len: usize,
        pub(crate) pattern_count: usize,
        pub(crate) total_pattern_count: u128,
        pub(crate) materialized_probability_mass: String,
        pub(crate) probability_complete: bool,
        pub(crate) expansion_truncated: bool,
        pub(crate) family_count: usize,
        pub(crate) tiling_variant_count: usize,
        pub(crate) build_variant_count: usize,
        pub(crate) score_aggregation_attached: bool,
        pub(crate) result_count: usize,
        pub(crate) results: Vec<SetupResult>,
        pub(crate) family_scores: Vec<SetupFamilyScore>,
    }

    impl SetupSearchExecutionResult {
        pub fn summary_fields(&self) -> Vec<(String, String)> {
            SetupSummaryBuilder::summary_fields(self)
        }
    }
    impl SetupSearchExecutionResult {
        pub fn results(&self) -> &[SetupResult] {
            &self.results
        }
    }
    impl SetupSearchExecutionResult {
        pub fn family_scores(&self) -> &[SetupFamilyScore] {
            &self.family_scores
        }
    }
}
mod result_ordering {
    use crate::result::{SetupFamilyScore, SetupResult, SetupResultSorter};

    pub(super) fn order_results(
        results: &mut [SetupResult],
        family_scores: &mut [SetupFamilyScore],
    ) {
        SetupResultSorter::sort_by_probability_desc(results);
        family_scores.sort_by(|left, right| {
            results
                .iter()
                .position(|result| result.family_id() == left.family_id())
                .cmp(
                    &results
                        .iter()
                        .position(|result| result.family_id() == right.family_id()),
                )
        });
    }
}
mod service {
    use clearra_scoring::profile::ScoreProfile;

    use crate::{
        enumerate::ShapeEnumerator,
        query::SetupSearchQuery,
        result::SetupResultFilter,
        service::{
            setup_candidate_enumerator::enumerate_build_candidates,
            setup_core_buildup_gate::SetupCoreBuildGate,
            setup_family_grouper::{family_map_for_candidates, unique_shape_count},
            setup_pattern_source::SetupPatternSource,
            setup_summary_builder::format_probability,
        },
    };

    use super::{
        counters::SetupEnumerationCounters,
        family_evaluator::evaluate_family,
        result_ordering::order_results,
        source_labels::{coverage_source_for_source, covered_pattern_count_basis_for_source},
        SetupSearchExecutionError, SetupSearchExecutionResult,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SetupSearchService;

    impl SetupSearchService {
        pub fn execute(
            query: &SetupSearchQuery,
        ) -> Result<SetupSearchExecutionResult, SetupSearchExecutionError> {
            Self::execute_internal(query, None)
        }
    }
    impl SetupSearchService {
        pub fn execute_with_score_profile(
            query: &SetupSearchQuery,
            score_profile: &ScoreProfile,
        ) -> Result<SetupSearchExecutionResult, SetupSearchExecutionError> {
            Self::execute_internal(query, Some(score_profile))
        }
    }
    impl SetupSearchService {
        fn execute_internal(
            query: &SetupSearchQuery,
            score_profile: Option<&ScoreProfile>,
        ) -> Result<SetupSearchExecutionResult, SetupSearchExecutionError> {
            let core_build_gate = SetupCoreBuildGate::from_query(query)?;
            let source = SetupPatternSource::from_query(query)?;
            let candidates = enumerate_build_candidates(query, &source);
            let family_map =
                family_map_for_candidates(&candidates, query.limits().max_shape_families());
            let shape_enumeration_complete = family_map.len() == unique_shape_count(&candidates);
            let families = ShapeEnumerator::from_masks(family_map.keys().copied());
            let mut results = Vec::new();
            let mut family_scores = Vec::new();
            let mut counters = SetupEnumerationCounters::new();

            for family in families {
                let Some(evaluation) = evaluate_family(
                    query,
                    &source,
                    &core_build_gate,
                    &candidates,
                    family,
                    score_profile,
                    &mut counters,
                )?
                else {
                    continue;
                };
                if SetupResultFilter::new(query.probability_filter()).accepts(&evaluation.result) {
                    family_scores.push(evaluation.score);
                    results.push(evaluation.result);
                }
                if results.len() >= query.limits().max_results() {
                    break;
                }
            }
            order_results(&mut results, &mut family_scores);

            Ok(SetupSearchExecutionResult {
                status: "setup-searched",
                execution_scope: "mvp2",
                enumeration_strategy: "queue-pattern-shape-tiling-build-post-pc",
                shape_family_enumeration_complete: shape_enumeration_complete,
                tiling_variant_enumeration_complete: counters.tiling_variant_enumeration_complete,
                build_variant_enumeration_complete: counters.build_variant_enumeration_complete,
                post_pc_mode: "scenario-clear-to-empty",
                post_pc_evaluation_attached: true,
                setup_foundation_reason: "core_packing_buildup_build_variants_attached",
                executor_flow:
                    "SetupQuery->SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult",
                build_variant_source: "C BuildUp",
                packing_candidate_count: core_build_gate.packing_candidate_count(),
                core_buildup_variant_count: core_build_gate.successful_build_variant_count(),
                core_coverage_row_count: core_build_gate.coverage_row_count(),
                coverage_source: coverage_source_for_source(&source),
                coverage_pattern_count: source.pattern_count,
                verified_pattern_count: source.pattern_count,
                materialized_pattern_count: source.pattern_count,
                covered_pattern_count_basis: covered_pattern_count_basis_for_source(&source),
                queue_mode: source.mode,
                queue_len: query.queue().len(),
                pattern_count: source.pattern_count,
                total_pattern_count: source.total_pattern_count,
                materialized_probability_mass: format_probability(
                    source.weights.total_weight().get(),
                ),
                probability_complete: source.probability_complete,
                expansion_truncated: source.expansion_truncated,
                family_count: results.len(),
                tiling_variant_count: counters.tiling_variant_count,
                build_variant_count: counters.build_variant_count,
                score_aggregation_attached: score_profile.is_some(),
                result_count: results.len(),
                results,
                family_scores,
            })
        }
    }
}
mod source_labels {
    use crate::service::setup_pattern_source::SetupPatternSource;

    pub(super) fn coverage_source_for_source(source: &SetupPatternSource) -> &'static str {
        match source.mode {
            "observed" => "observed-materialized-pattern-specific",
            "bag-aligned" => "bag-aligned-single-pattern",
            _ => "fixed-single-pattern",
        }
    }

    pub(super) fn covered_pattern_count_basis_for_source(
        source: &SetupPatternSource,
    ) -> &'static str {
        match source.mode {
            "observed" => "materialized_pattern_universe",
            _ => "complete_pattern_universe",
        }
    }
}

pub use error::SetupSearchExecutionError;
pub use result::SetupSearchExecutionResult;
pub use service::SetupSearchService;

#[cfg(test)]
#[path = "setup_search_service_tests.rs"]
mod tests;
