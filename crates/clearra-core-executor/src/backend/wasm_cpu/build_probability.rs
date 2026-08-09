use std::collections::{BTreeMap, VecDeque};

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_key_set_hash_from_sorted_strings,
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    },
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};
use clearra_finesse::{
    aggregate_unique_queue_costs, union_costed_geometry_languages, CostedGeometryEdge,
    CostedGeometryLanguage, FinesseError, FinesseRouteWitnessError, GeometryLanguageError,
    GeometryLanguageNode, GeometryNodeId, QueueClassProductEvaluator, QueueClassSet,
    QueueCostAggregation, QueueCostTable, QueuePattern,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    FinesseMetric, FinessePatternKnowledge, FinesseScoreRequest, SearchProblem,
};
use clearra_replay::ExactScoringExecutionBatch;
use clearra_rules::{kicks::KickTableProfile, spawn::SpawnProfile};
use clearra_supply::{
    hold_automaton::HoldAutomatonState,
    pattern_universe::{PackingPatternMembershipKind, PieceMultisetKey},
};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    CoreExecutionResult, CorePathStep, FinessePolicyResult, FinesseReport, FinesseReportInput,
    FinesseRepresentativeWitness, FinesseSolutionAverage, NormalizedSolutionCoverage,
    SolutionCoverage, TilingSolutionPageStore,
};

use super::{
    buildup::{
        exact_scoring_execution_graph_for_completion, verify_candidate_for_completion,
        verify_candidate_for_completion_with_finesse, BuildCompletion, BuildUpWorkspace,
        CandidateBuildResult, CandidateWitnessMode, PreparedFinesseLanguage,
    },
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedBackendExecution,
        WasmDistributedGeometrySummary, WasmDistributedProgress,
    },
    exact_collections::{ExactHashMap, ExactHashSet},
    geometry::{GeometryAdvance, GeometryCandidate, GeometrySearch, SharedTargetGroups},
    kick_profiles::replay_profile_ids,
    standard_bag_coverage::StandardBagCoverage,
    WasmExactSearchError, MAX_BOARD64_PIECES,
};

pub(crate) enum BuildProbabilityAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub(crate) struct WasmBuildProbabilitySession {
    pending: VecDeque<BuildProbabilitySessionKind>,
    completed: Vec<CoreExecutionResult>,
    pattern_weights: WeightedPatternSet,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    finesse_score: Option<PendingFinesseScore>,
    finesse_search_materials: Vec<FinesseSearchMaterial>,
    mirror_included: bool,
    mirror_distinct: bool,
    execution_constraints_requested: bool,
    finished: bool,
}

struct PendingFinesseScore {
    problem: SearchProblem,
    field: BuildProbabilityField,
    request: FinesseScoreRequest,
}

enum BuildProbabilitySessionKind {
    Compact(CompactBuildProbabilitySession),
    Extended(super::extended_build_probability::ExtendedBuildProbabilitySession),
}

impl WasmBuildProbabilitySession {
    pub(crate) fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
    ) -> Result<Self, WasmExactSearchError> {
        let finesse_metric = finesse.metric();
        let finesse_pattern_knowledge = finesse.pattern_knowledge();
        let finesse_score = finesse.score().cloned();
        let score_requested = finesse_score.is_some();
        let mirror_included = !score_requested && field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let mut pending = VecDeque::with_capacity(usize::from(mirror_distinct) + 1);
        if !score_requested {
            pending.push_back(build_probability_session_for_field(
                problem,
                original,
                aggregation,
                finesse_metric,
            )?);
            if let Some(mirrored) = mirrored.filter(|candidate| *candidate != original) {
                pending.push_back(build_probability_session_for_field(
                    problem,
                    mirrored,
                    aggregation,
                    finesse_metric,
                )?);
            }
        }
        Ok(Self {
            pending,
            completed: Vec::with_capacity(usize::from(mirror_distinct) + 1),
            pattern_weights: problem
                .piece_source()
                .materialized_pattern_weights()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_source_not_materialized",
                ))?
                .clone(),
            aggregation,
            finesse_metric,
            finesse_pattern_knowledge,
            finesse_score: finesse_score.map(|request| PendingFinesseScore {
                problem: problem.clone(),
                field: original,
                request,
            }),
            finesse_search_materials: Vec::new(),
            mirror_included,
            mirror_distinct,
            execution_constraints_requested: problem
                .objective()
                .execution_constraints()
                .requested(),
            finished: false,
        })
    }

    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_session_already_finished",
            ));
        }
        if control.is_cancelled() {
            return Ok(BuildProbabilityAdvance::Cancelled);
        }
        if let Some(score) = self.finesse_score.as_ref() {
            let result = super::finesse_score::execute_finesse_score(
                &score.problem,
                score.field,
                &score.request,
                self.finesse_pattern_knowledge,
                control,
            )?;
            self.finesse_score = None;
            self.finished = true;
            return Ok(BuildProbabilityAdvance::Completed(result));
        }
        let collect_search_finesse = self.finesse_metric.requested();
        let Some(session) = self.pending.front_mut() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_pass_missing",
            ));
        };
        let advance = match session {
            BuildProbabilitySessionKind::Compact(session) => session.advance(work_budget, control),
            BuildProbabilitySessionKind::Extended(session) => session.advance(work_budget, control),
        }?;
        match advance {
            BuildProbabilityAdvance::Pending => Ok(BuildProbabilityAdvance::Pending),
            BuildProbabilityAdvance::Cancelled => Ok(BuildProbabilityAdvance::Cancelled),
            BuildProbabilityAdvance::Completed(result) => {
                if collect_search_finesse {
                    let material = match session {
                        BuildProbabilitySessionKind::Compact(session) => {
                            session.finesse_search_material()?
                        }
                        BuildProbabilitySessionKind::Extended(session) => {
                            session.finesse_search_material()?
                        }
                    };
                    self.finesse_search_materials.push(material);
                }
                self.completed.push(result);
                self.pending.pop_front();
                if !self.pending.is_empty() {
                    return Ok(BuildProbabilityAdvance::Pending);
                }
                self.finished = true;
                let mut result = merge_symmetry_results(
                    core::mem::take(&mut self.completed),
                    self.mirror_included,
                    self.mirror_distinct,
                    &self.pattern_weights,
                    self.aggregation.requests_spin_coverage()
                        || self.execution_constraints_requested,
                )?;
                if collect_search_finesse {
                    result = result.with_additional_fields(vec![
                        (
                            "finesse_metric_requested".to_owned(),
                            self.finesse_metric.as_str().to_owned(),
                        ),
                        (
                            "finesse_pattern_knowledge_requested".to_owned(),
                            self.finesse_pattern_knowledge.as_str().to_owned(),
                        ),
                    ]);
                    result = result.with_finesse_report(build_finesse_report(
                        core::mem::take(&mut self.finesse_search_materials),
                        self.finesse_pattern_knowledge,
                        control,
                    )?);
                }
                Ok(BuildProbabilityAdvance::Completed(result))
            }
        }
    }
}

fn build_probability_session_for_field(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
) -> Result<BuildProbabilitySessionKind, WasmExactSearchError> {
    if field.is_compact() {
        Ok(BuildProbabilitySessionKind::Compact(
            CompactBuildProbabilitySession::new_with_finesse(
                problem,
                field,
                aggregation,
                finesse_metric.requested(),
            )?,
        ))
    } else {
        let session = if finesse_metric.requested() {
            super::extended_build_probability::ExtendedBuildProbabilitySession::new_with_finesse(
                problem,
                field,
                aggregation,
            )?
        } else {
            super::extended_build_probability::ExtendedBuildProbabilitySession::new(
                problem,
                field,
                aggregation,
            )?
        };
        Ok(BuildProbabilitySessionKind::Extended(session))
    }
}

pub(super) fn merge_symmetry_results(
    mut results: Vec<CoreExecutionResult>,
    mirror_included: bool,
    mirror_distinct: bool,
    pattern_weights: &WeightedPatternSet,
    materialize_postprocess_pattern_weights: bool,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    if results.is_empty() || results.len() != usize::from(mirror_distinct) + 1 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_symmetry_pass_mismatch",
        ));
    }
    let mut primary = results.remove(0);
    let tiling_only = primary.field("build_probability_aggregation") == Some("tiling");
    let pattern_count = primary.usize_field("coverage_pattern_count").ok_or(
        WasmExactSearchError::InvalidProblem("wasm_build_probability_pattern_count_missing"),
    )?;
    let original_words = primary.coverage_pattern_words().to_vec();
    let mut union_words = original_words.clone();
    for result in &results {
        if result.coverage_pattern_words().len() != union_words.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_coverage_mismatch",
            ));
        }
        for (union, incoming) in union_words.iter_mut().zip(result.coverage_pattern_words()) {
            *union |= incoming;
        }
    }

    let original_covered = count_coverage_words(&original_words, pattern_count);
    let union_covered = count_coverage_words(&union_words, pattern_count);
    let mirrored_covered = if mirror_included {
        results.first().map_or(original_covered, |result| {
            count_coverage_words(result.coverage_pattern_words(), pattern_count)
        })
    } else {
        0
    };
    let original_probability =
        probability_for_coverage_words(pattern_weights, &original_words, pattern_count)?;
    let union_probability =
        probability_for_coverage_words(pattern_weights, &union_words, pattern_count)?;
    let mirrored_probability = if mirror_included {
        results.first().map_or(Ok(original_probability), |result| {
            probability_for_coverage_words(
                pattern_weights,
                result.coverage_pattern_words(),
                pattern_count,
            )
        })?
    } else {
        0.0
    };

    let probability_complete = primary.bool_field("probability_complete").unwrap_or(false)
        && results
            .iter()
            .all(|result| result.bool_field("probability_complete").unwrap_or(false));
    let count_complete = primary.bool_field("count_complete").unwrap_or(false)
        && results
            .iter()
            .all(|result| result.bool_field("count_complete").unwrap_or(false));
    let resource_truncated = primary.bool_field("resource_truncated").unwrap_or(false)
        || results
            .iter()
            .any(|result| result.bool_field("resource_truncated").unwrap_or(false));
    let resource_reason = if resource_truncated {
        primary
            .field("resource_truncation_reason")
            .filter(|reason| *reason != "none")
            .or_else(|| {
                results.iter().find_map(|result| {
                    result
                        .field("resource_truncation_reason")
                        .filter(|reason| *reason != "none")
                })
            })
            .unwrap_or("symmetry_pass_incomplete")
    } else {
        "none"
    };

    let mirror_solution_count = if mirror_included {
        results
            .first()
            .and_then(|result| result.usize_field("unique_solution_count"))
            .unwrap_or_else(|| primary.usize_field("unique_solution_count").unwrap_or(0))
    } else {
        0
    };
    let mirror_candidate_count = results
        .first()
        .and_then(|result| result.usize_field("packing_candidate_count"))
        .unwrap_or(0);
    let mirror_solution_hash = results
        .first()
        .and_then(|result| result.field("normalized_solution_set_hash"))
        .unwrap_or("same-as-original")
        .to_owned();
    let all_results = core::iter::once(&primary)
        .chain(results.iter())
        .collect::<Vec<_>>();
    let page_stores = all_results
        .iter()
        .filter_map(|result| result.tiling_solution_page_store().cloned())
        .collect::<Vec<_>>();
    let merged_page_store = if page_stores.len() == all_results.len() {
        Some(
            TilingSolutionPageStore::merge_canonical_stores(page_stores)
                .map_err(WasmExactSearchError::InvalidProblem)?,
        )
    } else {
        None
    };
    let merged_solution_coverages = merge_board64_solution_coverages(&all_results, pattern_count)?;
    let merged_normalized_solution_coverages =
        merge_normalized_solution_coverages(&all_results, pattern_count)?;
    let board64_identity_surface = all_results
        .iter()
        .all(|result| result.field("board_storage") != Some("board256-canonical"));
    let normalized_identities_complete = board64_identity_surface
        && all_results.iter().all(|result| {
            result.usize_field("unique_solution_count")
                == Some(result.normalized_solution_identities().len())
        });
    let normalized_keys_complete = all_results.iter().all(|result| {
        result.usize_field("unique_solution_count") == Some(result.normalized_solution_keys().len())
    });
    let merged_identities = if let Some(store) = &merged_page_store {
        store
            .page_identities(0, 100)
            .map_err(WasmExactSearchError::InvalidProblem)?
    } else {
        let mut identities = all_results
            .iter()
            .flat_map(|result| result.normalized_solution_identities().iter().copied())
            .collect::<Vec<_>>();
        identities.sort_unstable();
        identities.dedup();
        identities
    };
    let mut merged_solution_keys = if merged_page_store.is_some() || normalized_identities_complete
    {
        merged_identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>()
    } else if normalized_keys_complete {
        all_results
            .iter()
            .flat_map(|result| result.normalized_solution_keys().iter().cloned())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    merged_solution_keys.sort_unstable();
    merged_solution_keys.dedup();
    let normalized_solutions_complete =
        merged_page_store.is_some() || normalized_identities_complete || normalized_keys_complete;
    let merged_solution_hash = if let Some(store) = &merged_page_store {
        store.normalized_hash().to_owned()
    } else if normalized_identities_complete {
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
            &merged_identities,
        )
    } else {
        normalized_string_solution_set_hash(&merged_solution_keys)
    };
    let mut replacements = vec![
        field(
            "build_symmetry_policy",
            if mirror_included {
                "original-or-horizontal-mirror"
            } else {
                "original-only"
            },
        ),
        field("build_mirror_included", mirror_included),
        field("build_mirror_distinct_target", mirror_distinct),
        field("build_mirror_search_executed", mirror_distinct),
        field(
            "solution_count_basis",
            if mirror_included && normalized_solutions_complete {
                "original-or-horizontal-mirror-union"
            } else {
                "original-field"
            },
        ),
        field(
            "coverage_basis",
            if tiling_only {
                "not-evaluated-tiling-only"
            } else if mirror_included {
                "original-or-horizontal-mirror-pattern-union"
            } else {
                "original-field-patterns"
            },
        ),
        field("original_covered_pattern_count", original_covered),
        field(
            "original_coverage_probability",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(original_probability)
            },
        ),
        field("mirror_covered_pattern_count", mirrored_covered),
        field(
            "mirror_coverage_probability",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(mirrored_probability)
            },
        ),
        field(
            "mirror_union_added_pattern_count",
            union_covered.saturating_sub(original_covered),
        ),
        field("mirror_unique_solution_count", mirror_solution_count),
        field("mirror_packing_candidate_count", mirror_candidate_count),
        field("mirror_normalized_solution_set_hash", mirror_solution_hash),
        field(
            "covered_pattern_count",
            if tiling_only { 0 } else { union_covered },
        ),
        field(
            "coverage_probability",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(union_probability)
            },
        ),
        field("probability_complete", probability_complete),
        field("count_complete", count_complete),
        field("resource_truncated", resource_truncated),
        field("resource_truncation_reason", resource_reason),
        field(
            "objective_search_complete",
            count_complete && (tiling_only || probability_complete),
        ),
        field(
            "objective_complete",
            count_complete && (tiling_only || probability_complete),
        ),
        field(
            "packing_candidate_count",
            sum_usize_field(&all_results, "packing_candidate_count"),
        ),
        field(
            "searched_nodes",
            sum_usize_field(&all_results, "searched_nodes"),
        ),
        field(
            "geometry_domain_pruned_states",
            sum_usize_field(&all_results, "geometry_domain_pruned_states"),
        ),
        field(
            "geometry_hall_pruned_states",
            sum_usize_field(&all_results, "geometry_hall_pruned_states"),
        ),
        field(
            "geometry_column_pruned_states",
            sum_usize_field(&all_results, "geometry_column_pruned_states"),
        ),
        field(
            "geometry_component_compositions",
            sum_usize_field(&all_results, "geometry_component_compositions"),
        ),
        field(
            "total_build_order_nodes",
            sum_usize_field(&all_results, "total_build_order_nodes"),
        ),
        field(
            "coverage_product_states",
            sum_usize_field(&all_results, "coverage_product_states"),
        ),
        field(
            "coverage_product_edge_checks",
            sum_usize_field(&all_results, "coverage_product_edge_checks"),
        ),
        field(
            "total_reachability_states",
            sum_usize_field(&all_results, "total_reachability_states"),
        ),
        field(
            "resource_peak_frontier_states",
            max_usize_field(&all_results, "resource_peak_frontier_states"),
        ),
        field(
            "peak_build_order_nodes",
            max_usize_field(&all_results, "peak_build_order_nodes"),
        ),
        field(
            "peak_reachability_states",
            max_usize_field(&all_results, "peak_reachability_states"),
        ),
        field(
            "resource_peak_cpu_bytes",
            sum_usize_field(&all_results, "resource_peak_cpu_bytes"),
        ),
    ];
    if normalized_solutions_complete {
        replacements.push(field(
            "unique_solution_count",
            merged_page_store
                .as_ref()
                .map_or(merged_solution_keys.len(), |store| store.len()),
        ));
        replacements.push(field("normalized_solution_set_hash", merged_solution_hash));
    }

    let mut scoring_batches = primary.take_exact_scoring_execution_batches();
    let mut spin_coverage_batches = primary.take_spin_coverage_execution_batches();
    for result in &mut results {
        scoring_batches.extend(result.take_exact_scoring_execution_batches());
        spin_coverage_batches.extend(result.take_spin_coverage_execution_batches());
    }
    let merged = primary
        .with_coverage_pattern_words(union_words)
        .with_solution_coverages(merged_solution_coverages)
        .with_normalized_solution_coverages(merged_normalized_solution_coverages)
        .with_exact_scoring_execution_batches(scoring_batches)
        .with_spin_coverage_execution_batches(spin_coverage_batches)
        .with_replaced_fields(replacements);
    let merged = if materialize_postprocess_pattern_weights {
        let pattern_weight_strings = (0..pattern_weights.len())
            .map(|pattern| {
                pattern_weights
                    .weight(PatternId::new(pattern))
                    .expect("validated pattern weight index")
                    .get()
                    .to_string()
            })
            .collect();
        merged.with_postprocess_execution_batch(
            Vec::new(),
            probability_complete && count_complete,
            pattern_weight_strings,
        )
    } else {
        merged
    };
    let mut merged = if merged_page_store.is_some() || normalized_identities_complete {
        merged
            .with_normalized_solution_keys(merged_solution_keys)
            .with_normalized_solution_identities(merged_identities)
    } else if normalized_keys_complete {
        merged.with_normalized_solution_keys(merged_solution_keys)
    } else {
        merged
    };
    if let Some(store) = merged_page_store {
        merged = merged.with_tiling_solution_page_store(store);
    }
    Ok(merged)
}

pub(super) fn normalized_string_solution_set_hash(keys: &[String]) -> String {
    normalized_tiling_solution_key_set_hash_from_sorted_strings(keys)
}

fn count_coverage_words(words: &[u64], pattern_count: usize) -> usize {
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let remaining = pattern_count.saturating_sub(index * u64::BITS as usize);
            if remaining >= u64::BITS as usize {
                word.count_ones() as usize
            } else if remaining == 0 {
                0
            } else {
                (word & ((1_u64 << remaining) - 1)).count_ones() as usize
            }
        })
        .sum()
}

fn probability_for_coverage_words(
    weights: &WeightedPatternSet,
    words: &[u64],
    pattern_count: usize,
) -> Result<f64, WasmExactSearchError> {
    if weights.len() != pattern_count {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_pattern_weights_missing",
        ));
    }
    let coverage = PatternBitSet::from_words(pattern_count, words.to_vec()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_build_probability_coverage_words_invalid")
    })?;
    weights
        .covered_weight(&coverage)
        .map(|probability| probability.get())
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_pattern_weight_mismatch",
        ))
}

fn probability_text(probability: f64) -> String {
    if probability == 0.0 {
        "0".to_owned()
    } else {
        probability.to_string()
    }
}

fn sum_usize_field(results: &[&CoreExecutionResult], key: &str) -> usize {
    results
        .iter()
        .filter_map(|result| result.usize_field(key))
        .fold(0_usize, usize::saturating_add)
}

fn max_usize_field(results: &[&CoreExecutionResult], key: &str) -> usize {
    results
        .iter()
        .filter_map(|result| result.usize_field(key))
        .max()
        .unwrap_or(0)
}

pub(super) struct CompactBuildProbabilitySession {
    problem: SearchProblem,
    aggregation: BuildProbabilityAggregation,
    target_cells: u64,
    target_board: u64,
    shared_supply_catalog: CompactBuildProbabilitySharedCatalog,
    catalog: GeometryCatalog,
    geometry: GeometrySearch,
    buildup: BuildUpWorkspace,
    coverage_evaluator: CoverageProductEvaluator,
    covered_patterns: PatternBitSet,
    buildable_tilings: ExactHashSet<StandardBoard64TilingIdentity>,
    solution_coverage: Option<ExactHashMap<StandardBoard64TilingIdentity, PatternBitSet>>,
    candidate_count: usize,
    candidate_digest: u64,
    build_variant_count: u128,
    count_complete: bool,
    representative_path: Vec<CorePathStep>,
    representative_pattern_id: Option<u32>,
    representative_rank: Option<u64>,
    peak_build_nodes: usize,
    total_build_nodes: usize,
    coverage_product_states: usize,
    coverage_product_edge_checks: usize,
    peak_reachability_states: usize,
    total_reachability_states: usize,
    truncated_reason: Option<&'static str>,
    trivial_target: bool,
    workers_used: usize,
    parallel_active_workers: usize,
    parallel_minimum_worker_candidates: usize,
    parallel_maximum_worker_candidates: usize,
    parallel_decision_reason: &'static str,
    distributed_spin_materialized: bool,
    distributed_execution_constraint_materialized: bool,
    finesse_requested: bool,
    finesse_languages: Vec<(StandardBoard64TilingIdentity, PreparedFinesseLanguage)>,
    finished: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompactBuildProbabilitySharedCatalogKey {
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_count: usize,
    target_piece_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    compile_pattern_indexes: bool,
}

#[derive(Clone, Debug)]
pub(super) struct CompactBuildProbabilitySharedCatalog {
    key: CompactBuildProbabilitySharedCatalogKey,
    targets: SharedTargetGroups,
    supply_projection_complete: bool,
}

impl CompactBuildProbabilitySession {
    pub(super) fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, field, aggregation, false, None, false)
    }

    pub(super) fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            false,
            None,
            finesse_requested,
        )
    }

    pub(super) fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, field, aggregation, true, None, false)
    }

    pub(super) fn new_with_shared_supply_catalog(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            external_geometry,
            Some(shared_supply_catalog),
            false,
        )
    }

    pub(super) fn shared_supply_catalog(&self) -> CompactBuildProbabilitySharedCatalog {
        self.shared_supply_catalog.clone()
    }

    pub(super) fn distributed_progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.geometry.expanded_nodes(),
            candidates: self.candidate_count,
            candidate_family_count: self.geometry.candidate_family_count(),
            build_nodes: self.total_build_nodes,
            coverage_checks: self.coverage_product_edge_checks,
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    fn new_with_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: Option<&CompactBuildProbabilitySharedCatalog>,
        finesse_requested: bool,
    ) -> Result<Self, WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
        let target_cells =
            field
                .compact_target_mask()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_compact_mask_missing",
                ))?;
        let initial_board =
            field
                .compact_base_mask()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_compact_base_missing",
                ))?;
        let catalog = GeometryCatalog::compile_for_required_cells_on_board(
            problem,
            initial_board,
            target_cells,
        )?;
        let target_piece_count = target_cells.count_ones() as usize / 4;
        if target_piece_count > MAX_BOARD64_PIECES {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_piece_count_exceeds_exact_limit",
            ));
        }
        if problem
            .exact_pieces()
            .is_some_and(|count| count != target_piece_count)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_piece_count_mismatch",
            ));
        }
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let symbolic = StandardBagCoverage::supports(universe, problem.initial_hold());
        let compile_pattern_indexes = !symbolic
            || finesse_requested
            || shared_supply_catalog.is_some_and(|shared| shared.key.compile_pattern_indexes);
        let shared_key = CompactBuildProbabilitySharedCatalogKey {
            piece_source_id: problem.piece_source().id().get(),
            pattern_universe_id: universe.pattern_universe_id().get(),
            pattern_count: universe.pattern_count(),
            target_piece_count,
            initial_hold: problem.initial_hold(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            compile_pattern_indexes,
        };
        let shared_supply_catalog = match shared_supply_catalog {
            Some(shared) if shared.key == shared_key => shared.clone(),
            Some(_) => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_shared_supply_catalog_mismatch",
                ));
            }
            None => {
                let family = universe.packing_multiset_family(
                    target_piece_count,
                    problem.initial_hold(),
                    super::packing_projection_hold_enabled(problem),
                );
                if family.is_empty() {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_supply_has_no_reachable_piece_multiset",
                    ));
                }
                let supply_projection_complete = universe.complete()
                    || family.membership_kind()
                        == PackingPatternMembershipKind::ExactSymbolicStandardBag;
                CompactBuildProbabilitySharedCatalog {
                    key: shared_key,
                    targets: SharedTargetGroups::compile(
                        universe,
                        &family,
                        compile_pattern_indexes,
                    )?,
                    supply_projection_complete,
                }
            }
        };
        let geometry = if external_geometry {
            GeometrySearch::external_shared(&shared_supply_catalog.targets)
        } else {
            GeometrySearch::new_shared(target_cells, &shared_supply_catalog.targets)
        };
        let covered_patterns = if target_cells == 0 {
            PatternBitSet::all(universe.pattern_count())
        } else {
            PatternBitSet::new(universe.pattern_count())
        };
        Ok(Self {
            problem: problem.clone(),
            aggregation,
            target_cells,
            target_board: super::buildup::place_and_clear(
                catalog.width(),
                catalog.height(),
                catalog.initial_board() | target_cells,
            )
            .0,
            shared_supply_catalog,
            catalog,
            geometry,
            buildup: BuildUpWorkspace::default(),
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns,
            buildable_tilings: ExactHashSet::default(),
            solution_coverage: problem
                .objective()
                .execution_constraints()
                .requested()
                .then(ExactHashMap::default),
            candidate_count: 0,
            candidate_digest: 0,
            build_variant_count: 0,
            count_complete: true,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            peak_build_nodes: 0,
            total_build_nodes: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            peak_reachability_states: 0,
            total_reachability_states: 0,
            truncated_reason: None,
            trivial_target: target_cells == 0,
            workers_used: 1,
            parallel_active_workers: usize::from(!external_geometry),
            parallel_minimum_worker_candidates: if external_geometry { usize::MAX } else { 0 },
            parallel_maximum_worker_candidates: 0,
            parallel_decision_reason: "serial-build-probability-session",
            distributed_spin_materialized: false,
            distributed_execution_constraint_materialized: false,
            finesse_requested,
            finesse_languages: Vec::new(),
            finished: false,
        })
    }

    fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_session_already_finished",
            ));
        }
        if control.is_cancelled() {
            return Ok(BuildProbabilityAdvance::Cancelled);
        }
        if self.trivial_target {
            return self.complete();
        }

        let mut completed_work = 0usize;
        while completed_work < work_budget.max(1) {
            if control.is_cancelled() {
                return Ok(BuildProbabilityAdvance::Cancelled);
            }
            match self.geometry.advance(&self.catalog) {
                GeometryAdvance::Pending => completed_work += 1,
                GeometryAdvance::Candidate(candidate) => {
                    let ordinal = self.candidate_count as u64;
                    self.process_candidate(candidate, Some(ordinal), true, control)?;
                    completed_work += 1;
                    if self.truncated_reason.is_some() {
                        return self.complete();
                    }
                }
                GeometryAdvance::Complete => return self.complete(),
                GeometryAdvance::ResourceIncomplete(reason) => {
                    self.truncated_reason = Some(reason);
                    self.count_complete = false;
                    return self.complete();
                }
            }
        }
        Ok(BuildProbabilityAdvance::Pending)
    }

    fn process_candidate(
        &mut self,
        candidate: GeometryCandidate,
        external_ordinal: Option<u64>,
        enforce_candidate_budget: bool,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let max_candidates = self.problem.backend_request().max_candidates();
        if enforce_candidate_budget && max_candidates != 0 && self.candidate_count >= max_candidates
        {
            self.truncated_reason = Some("candidate_budget_exceeded");
            self.count_complete = false;
            return Ok(());
        }
        self.candidate_count += 1;
        self.candidate_digest =
            super::mix_digest(self.candidate_digest, candidate.identity.bucket_hash());
        if self.aggregation.is_tiling_only() {
            self.buildable_tilings.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_storage_unavailable",
                )
            })?;
            self.buildable_tilings.insert(candidate.identity);
            if !self.finesse_requested {
                return Ok(());
            }
        }
        let target = self.geometry.target(candidate.target_index).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_build_probability_target_index_invalid"),
        )?;
        let solution_coverage_required = self.solution_coverage.is_some();
        let coverage_already_known = self.buildup.standard_bag_coverage_complete()
            || self
                .covered_patterns
                .is_superset(target.possible_patterns.as_ref())
                .expect("candidate pattern group belongs to the build probability universe");
        let witness_mode = CandidateWitnessMode::for_candidate(
            &self.problem,
            target,
            coverage_already_known,
            solution_coverage_required,
        );
        let result = if self.finesse_requested {
            verify_candidate_for_completion_with_finesse(
                &self.problem,
                &self.catalog,
                &candidate,
                target,
                &mut self.buildup,
                &mut self.coverage_evaluator,
                witness_mode,
                self.representative_path.is_empty(),
                0,
                BuildCompletion::ExactBoardAfterLineClears(self.target_board),
                self.aggregation.requests_spin_coverage(),
                control,
            )?
        } else {
            verify_candidate_for_completion(
                &self.problem,
                &self.catalog,
                &candidate,
                target,
                &mut self.buildup,
                &mut self.coverage_evaluator,
                witness_mode,
                self.representative_path.is_empty(),
                0,
                BuildCompletion::ExactBoardAfterLineClears(self.target_board),
                control,
            )?
        };

        self.apply_candidate_result(candidate.identity, external_ordinal, result)
    }

    fn apply_candidate_result(
        &mut self,
        identity: StandardBoard64TilingIdentity,
        external_ordinal: Option<u64>,
        mut result: CandidateBuildResult,
    ) -> Result<(), WasmExactSearchError> {
        self.peak_build_nodes = self.peak_build_nodes.max(result.graph_nodes);
        self.total_build_nodes = self.total_build_nodes.saturating_add(result.graph_nodes);
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(result.coverage_product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(result.coverage_product_edge_checks);
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(result.reachability_states);
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(result.reachability_states);

        let retain_solution_coverage = self.solution_coverage.is_some();
        let mut candidate_coverage = result
            .covered_patterns
            .as_ref()
            .filter(|_| retain_solution_coverage)
            .cloned();
        if let Some(bits) = result.covered_patterns.as_ref() {
            self.covered_patterns.union_with(bits).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_coverage_universe_mismatch",
                )
            })?;
        }
        if let Some(root) = result.symbolic_coverage_root {
            if retain_solution_coverage {
                let materialized = self.buildup.materialize_standard_bag_root(root)?;
                if let Some(coverage) = candidate_coverage.as_mut() {
                    coverage.union_with(&materialized).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_solution_coverage_universe_mismatch",
                        )
                    })?;
                } else {
                    candidate_coverage = Some(materialized);
                }
            }
            self.buildup.merge_standard_bag_coverage(root)?;
        }
        if result.buildable {
            if let Some(language) = result.finesse_language.take() {
                self.finesse_languages.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_language_storage_unavailable",
                    )
                })?;
                self.finesse_languages.push((identity, language));
            }
            if retain_solution_coverage {
                let candidate_coverage =
                    candidate_coverage
                        .as_ref()
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_solution_coverage_missing",
                        ))?;
                self.merge_solution_coverage(identity, candidate_coverage)?;
            }
            self.buildable_tilings.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_storage_unavailable",
                )
            })?;
            self.buildable_tilings.insert(identity);
            self.build_variant_count = self
                .build_variant_count
                .checked_add(result.build_variant_count)
                .unwrap_or_else(|| {
                    self.count_complete = false;
                    u128::MAX
                });
            self.count_complete &= result.count_complete;
            let rank =
                external_ordinal.unwrap_or_else(|| self.candidate_count.saturating_sub(1) as u64);
            if self
                .representative_rank
                .is_none_or(|current| rank < current)
            {
                self.representative_path = result.representative_path;
                self.representative_pattern_id = result.witness_pattern_id;
                self.representative_rank = Some(rank);
            }
        }
        Ok(())
    }

    fn merge_solution_coverage(
        &mut self,
        identity: StandardBoard64TilingIdentity,
        coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        let map = self
            .solution_coverage
            .as_mut()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_solution_coverage_not_requested",
            ))?;
        if !map.contains_key(&identity) {
            map.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_storage_unavailable",
                )
            })?;
        }
        map.entry(identity)
            .or_insert_with(|| PatternBitSet::new(coverage.pattern_count()))
            .union_with(coverage)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_universe_mismatch",
                )
            })
    }

    pub(super) fn advance_distributed_geometry(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        let max_candidates = self.problem.backend_request().max_candidates();
        if max_candidates != 0 && self.candidate_count >= max_candidates {
            self.truncated_reason = Some("candidate_budget_exceeded");
            self.count_complete = false;
            return Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(self.truncated_reason),
            ));
        }
        match self.geometry.advance(&self.catalog) {
            GeometryAdvance::Pending => Ok(WasmCandidateProducerAdvance::Pending),
            GeometryAdvance::Candidate(candidate) => {
                let ordinal = self.candidate_count as u64;
                self.candidate_count = self.candidate_count.saturating_add(1);
                self.candidate_digest =
                    super::mix_digest(self.candidate_digest, candidate.identity.bucket_hash());
                Ok(WasmCandidateProducerAdvance::Candidate(
                    WasmCandidatePacket::for_pass(
                        ordinal,
                        pass_index,
                        candidate.target_index,
                        candidate.row_ids().to_vec(),
                    ),
                ))
            }
            GeometryAdvance::Complete => Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(None),
            )),
            GeometryAdvance::ResourceIncomplete(reason) => {
                self.truncated_reason = Some(reason);
                self.count_complete = false;
                Ok(WasmCandidateProducerAdvance::Completed(
                    self.distributed_geometry_summary(Some(reason)),
                ))
            }
        }
    }

    fn distributed_geometry_summary(
        &self,
        truncated_reason: Option<&'static str>,
    ) -> WasmDistributedGeometrySummary {
        WasmDistributedGeometrySummary {
            candidate_count: self.candidate_count,
            candidate_digest: self.candidate_digest,
            candidate_family_count: self.geometry.candidate_family_count(),
            expanded_nodes: self.geometry.expanded_nodes(),
            peak_frontier: self.geometry.peak_frontier(),
            domain_pruned_states: self.geometry.domain_pruned_states(),
            hall_pruned_states: self.geometry.hall_pruned_states(),
            column_pruned_states: self.geometry.column_pruned_states(),
            component_compositions: self.geometry.component_compositions(),
            truncated_reason,
            backend_execution: WasmDistributedBackendExecution::Cpu,
        }
    }

    pub(super) fn prepare_distributed_finalizer(&mut self) {
        self.parallel_active_workers = 0;
        self.parallel_minimum_worker_candidates = usize::MAX;
        self.parallel_maximum_worker_candidates = 0;
        self.parallel_decision_reason = "browser-worker-build-probability-pipeline";
        self.distributed_execution_constraint_materialized =
            self.problem.objective().execution_constraints().requested();
    }

    pub(super) fn process_external_candidate(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_verifier_already_finished",
            ));
        }
        let geometry = GeometryCandidate::from_rows(
            &self.catalog,
            candidate.target_index(),
            candidate.row_ids(),
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_candidate_invalid",
        ))?;
        self.process_candidate(geometry, Some(candidate.ordinal()), false, control)
    }

    pub(super) fn complete_distributed_worker(
        &mut self,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_verifier_already_finished",
            ));
        }
        if !self.aggregation.is_tiling_only() {
            if let Some(symbolic) = self.buildup.materialize_standard_bag_coverage()? {
                self.covered_patterns.union_with(&symbolic).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symbolic_coverage_mismatch",
                    )
                })?;
            }
        }
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested();
        let scoring_batch = if execution_evidence_requested {
            Some(self.prepare_exact_spin_execution_batch()?)
        } else {
            None
        };
        self.finished = true;
        self.build_result(scoring_batch)
    }

    pub(super) fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let pattern_count = result.usize_field("coverage_pattern_count").ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_pattern_count_missing",
            ),
        )?;
        let coverage =
            PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_coverage_invalid",
                    )
                })?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_mismatch",
            )
        })?;
        self.distributed_spin_materialized |= !result.postprocess_spin_coverages().is_empty();
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &= result
                .bool_field("execution_constraint_materialized")
                .unwrap_or(false);
        }

        for identity in result.normalized_solution_identities() {
            if !self.buildable_tilings.contains(identity) {
                self.buildable_tilings.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_solution_storage_unavailable",
                    )
                })?;
                self.buildable_tilings.insert(*identity);
            }
        }
        if self.solution_coverage.is_some() {
            for coverage in result.solution_coverages() {
                self.merge_solution_coverage(coverage.identity(), coverage.covered_patterns())?;
            }
        }
        let worker_candidates = result.usize_field("packing_candidate_count").unwrap_or(0);
        if worker_candidates != 0 {
            self.parallel_active_workers = self.parallel_active_workers.saturating_add(1);
            self.parallel_minimum_worker_candidates = self
                .parallel_minimum_worker_candidates
                .min(worker_candidates);
            self.parallel_maximum_worker_candidates = self
                .parallel_maximum_worker_candidates
                .max(worker_candidates);
        }
        let next_variants = result
            .field("build_variant_count")
            .and_then(|value| value.parse::<u128>().ok())
            .and_then(|value| self.build_variant_count.checked_add(value));
        self.build_variant_count = next_variants.unwrap_or(u128::MAX);
        self.count_complete &=
            next_variants.is_some() && result.bool_field("count_complete").unwrap_or(false);
        self.peak_build_nodes = self
            .peak_build_nodes
            .max(result.usize_field("peak_build_order_nodes").unwrap_or(0));
        self.total_build_nodes = self
            .total_build_nodes
            .saturating_add(result.usize_field("total_build_order_nodes").unwrap_or(0));
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(result.usize_field("coverage_product_states").unwrap_or(0));
        self.coverage_product_edge_checks = self.coverage_product_edge_checks.saturating_add(
            result
                .usize_field("coverage_product_edge_checks")
                .unwrap_or(0),
        );
        self.peak_reachability_states = self
            .peak_reachability_states
            .max(result.usize_field("peak_reachability_states").unwrap_or(0));
        self.total_reachability_states = self
            .total_reachability_states
            .saturating_add(result.usize_field("total_reachability_states").unwrap_or(0));

        if let Some(rank) = result
            .field("representative_candidate_ordinal")
            .and_then(|value| value.parse::<u64>().ok())
        {
            if self
                .representative_rank
                .is_none_or(|current| rank < current)
            {
                self.representative_rank = Some(rank);
                self.representative_pattern_id = result
                    .field("representative_pattern_id")
                    .and_then(|value| value.parse::<u32>().ok());
                self.representative_path = result.path_steps().to_vec();
            }
        }
        if result.bool_field("resource_truncated").unwrap_or(true) {
            self.truncated_reason = Some("distributed_worker_incomplete");
            self.count_complete = false;
        }
        Ok(())
    }

    pub(super) fn complete_distributed_geometry(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        self.candidate_count = summary.candidate_count;
        self.candidate_digest = summary.candidate_digest;
        self.geometry.finish_external_summary(summary);
        self.workers_used = workers_used.max(1);
        self.parallel_decision_reason = "browser-worker-build-probability-pipeline";
        if self.parallel_minimum_worker_candidates == usize::MAX {
            self.parallel_minimum_worker_candidates = 0;
        }
        if let Some(reason) = summary.truncated_reason {
            self.truncated_reason = Some(reason);
            self.count_complete = false;
        }
        self.complete()
    }

    pub(super) fn annotate_distributed_finesse(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_distributed_annotation_after_finish",
            ));
        }
        let mut identities = self.buildable_tilings.iter().copied().collect::<Vec<_>>();
        identities.sort_unstable();
        self.reset_distributed_finesse_aggregation();
        for (ordinal, identity) in identities.into_iter().enumerate() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if identity.initial_board_mask() != self.catalog.initial_board() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_distributed_identity_initial_board_mismatch",
                ));
            }
            let mut row_ids = Vec::with_capacity(identity.placement_count());
            let mut pieces = Vec::with_capacity(identity.placement_count());
            for index in 0..identity.placement_count() {
                let placement =
                    identity
                        .placement(index)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_finesse_distributed_identity_placement_missing",
                        ))?;
                let row_id = self
                    .catalog
                    .skeleton_id(placement.piece(), placement.cells_mask())
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_distributed_identity_not_in_catalog",
                    ))?;
                row_ids.push(row_id);
                pieces.push(placement.piece());
            }
            let multiset = PieceMultisetKey::from_pieces(pieces);
            let target = self
                .shared_supply_catalog
                .targets
                .targets()
                .iter()
                .find(|target| target.key == multiset)
                .cloned()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_distributed_identity_supply_mismatch",
                ))?;
            let candidate =
                GeometryCandidate::from_rows(&self.catalog, target.pattern_index_id, &row_ids)
                    .filter(|candidate| candidate.identity == identity)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_distributed_identity_reconstruction_failed",
                    ))?;
            let solution_coverage_required = self.solution_coverage.is_some();
            let coverage_already_known = self.buildup.standard_bag_coverage_complete()
                || self
                    .covered_patterns
                    .is_superset(target.possible_patterns.as_ref())
                    .expect("candidate pattern group belongs to the build probability universe");
            let witness_mode = CandidateWitnessMode::for_candidate(
                &self.problem,
                &target,
                coverage_already_known,
                solution_coverage_required,
            );
            let result = verify_candidate_for_completion_with_finesse(
                &self.problem,
                &self.catalog,
                &candidate,
                &target,
                &mut self.buildup,
                &mut self.coverage_evaluator,
                witness_mode,
                self.representative_path.is_empty(),
                0,
                BuildCompletion::ExactBoardAfterLineClears(self.target_board),
                self.aggregation.requests_spin_coverage(),
                control,
            )?;
            self.apply_candidate_result(identity, Some(ordinal as u64), result)?;
        }
        Ok(())
    }

    fn reset_distributed_finesse_aggregation(&mut self) {
        self.finesse_requested = true;
        self.covered_patterns = if self.trivial_target {
            PatternBitSet::all(self.covered_patterns.pattern_count())
        } else {
            PatternBitSet::new(self.covered_patterns.pattern_count())
        };
        self.buildable_tilings.clear();
        if let Some(solution_coverage) = self.solution_coverage.as_mut() {
            solution_coverage.clear();
        }
        self.finesse_languages.clear();
        self.build_variant_count = 0;
        self.count_complete = self.truncated_reason.is_none();
        self.representative_path.clear();
        self.representative_pattern_id = None;
        self.representative_rank = None;
        self.peak_build_nodes = 0;
        self.total_build_nodes = 0;
        self.coverage_product_states = 0;
        self.coverage_product_edge_checks = 0;
        self.peak_reachability_states = 0;
        self.total_reachability_states = 0;
        self.distributed_spin_materialized = false;
        self.distributed_execution_constraint_materialized = false;
    }

    fn complete(&mut self) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if !self.aggregation.is_tiling_only() {
            if let Some(symbolic) = self.buildup.materialize_standard_bag_coverage()? {
                self.covered_patterns.union_with(&symbolic).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symbolic_coverage_mismatch",
                    )
                })?;
            }
        }
        self.finished = true;
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested();
        let evidence_materialized = self.distributed_spin_materialized
            || (self.problem.objective().execution_constraints().requested()
                && self.distributed_execution_constraint_materialized);
        let scoring_batch = if execution_evidence_requested && !evidence_materialized {
            let span = SearchStageSpan::begin(ExecutorSearchStage::WasmSpinExecutionGraphPrepare);
            let batch = self.prepare_exact_spin_execution_batch()?;
            span.finish(batch.graphs().len() as u64);
            Some(batch)
        } else {
            None
        };
        Ok(BuildProbabilityAdvance::Completed(
            self.build_result(scoring_batch)?,
        ))
    }

    fn prepare_exact_spin_execution_batch(
        &mut self,
    ) -> Result<ExactScoringExecutionBatch, WasmExactSearchError> {
        let mut identities = self.buildable_tilings.iter().copied().collect::<Vec<_>>();
        identities.sort_unstable();
        let mut graphs = Vec::new();
        graphs.try_reserve_exact(identities.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_build_spin_graph_storage_unavailable")
        })?;
        let mut complete = true;
        for (index, identity) in identities.into_iter().enumerate() {
            let candidate_id = u64::try_from(index + 1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_build_spin_candidate_id_overflow")
            })?;
            match exact_scoring_execution_graph_for_completion(
                &self.problem,
                &self.catalog,
                identity,
                candidate_id,
                &mut self.buildup,
                BuildCompletion::ExactBoardAfterLineClears(self.target_board),
            )? {
                Some(graph) => graphs.push(graph),
                None => complete = false,
            }
        }
        let board_size = BoardSize::new(
            u16::from(self.catalog.width()),
            u16::from(self.catalog.height()),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_build_spin_layout_invalid"))?;
        let layout = Board64Layout::new(board_size).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_build_spin_layout_not_board64")
        })?;
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let patterns = (0..universe.pattern_count())
            .map(|pattern| universe.sequence_at(pattern).into_owned())
            .collect();
        let (kick_table_id, rule_profile_id) = replay_profile_ids(&self.problem);
        Ok(ExactScoringExecutionBatch::new(
            layout,
            self.catalog.initial_board(),
            patterns,
            self.problem.initial_hold().cursor(),
            self.problem.initial_hold().hold_piece(),
            self.problem.supply().hold_enabled(),
            self.problem.supply().projects_unplaced_lookahead(),
            self.problem.supply().projects_standard_bag_lookahead(),
            kick_table_id,
            rule_profile_id,
            graphs,
            complete,
        ))
    }

    fn build_result(
        &self,
        scoring_batch: Option<ExactScoringExecutionBatch>,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        let tiling_only = self.aggregation.is_tiling_only();
        let universe = self
            .problem
            .piece_source()
            .materialized_universe()
            .expect("build probability requires a materialized supply");
        let probability = if tiling_only {
            "not-calculated".to_owned()
        } else {
            universe
                .weights()
                .covered_weight(&self.covered_patterns)
                .expect("build probability coverage belongs to its supply universe")
                .get()
                .to_string()
        };
        let probability_complete =
            !tiling_only && universe.complete() && self.truncated_reason.is_none();
        let count_complete = self.count_complete
            && self.truncated_reason.is_none()
            && (!tiling_only || self.shared_supply_catalog.supply_projection_complete);
        let build_variant_count_exact = !tiling_only
            && self.problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountAll
            && count_complete;
        let execution_constraints = self.problem.objective().execution_constraints();
        let execution_constraint_complete = !execution_constraints.requested()
            || self.distributed_execution_constraint_materialized;
        let solution_found = self.trivial_target || !self.buildable_tilings.is_empty();
        let mut identities = self.buildable_tilings.iter().copied().collect::<Vec<_>>();
        identities.sort_unstable();
        let normalized_hash =
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities,
            );
        let normalized_keys = identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();
        let solution_coverages = self
            .solution_coverage
            .as_ref()
            .map(|coverage| {
                let mut entries = coverage
                    .iter()
                    .map(|(identity, patterns)| SolutionCoverage::new(*identity, patterns.clone()))
                    .collect::<Vec<_>>();
                entries.sort_unstable_by_key(SolutionCoverage::identity);
                entries
            })
            .unwrap_or_default();
        let normalized_solution_coverages = solution_coverages
            .iter()
            .map(|coverage| {
                NormalizedSolutionCoverage::new(
                    NormalizedTilingSolutionKey::from_standard_board64_identity(
                        coverage.identity(),
                    )
                    .as_str(),
                    coverage.covered_patterns().clone(),
                )
            })
            .collect();
        let source_sequence_length = universe.sequence_at(0).len();
        let backend_requested = self.problem.backend_policy().requested_backend().as_str();
        let gpu_capability_requested = matches!(backend_requested, "gpu" | "hybrid");
        let hybrid_requested = backend_requested == "hybrid";
        let fields = vec![
            field("backend_requested", backend_requested),
            field("backend_selected", "wasm-cpu-build-probability"),
            field("actual_backend", "wasm-cpu-build-probability"),
            field(
                "backend_fallback_allowed",
                self.problem.backend_policy().allow_backend_fallback(),
            ),
            field("backend_fallback_used", false),
            field("fallback_used", false),
            field("backend_fallback_reason", "none"),
            field("fallback_backend", "none"),
            field("gpu_available", false),
            field(
                "gpu_disabled_reason",
                if gpu_capability_requested {
                    "gpu_kernel_unavailable"
                } else {
                    "not_requested"
                },
            ),
            field("gpu_trust_state", "not-used"),
            field(
                "hybrid_status",
                if hybrid_requested {
                    "cpu-selected"
                } else {
                    "not-requested"
                },
            ),
            field(
                "hybrid_disabled_reason",
                if hybrid_requested {
                    "gpu_kernel_unavailable"
                } else {
                    "not_requested"
                },
            ),
            field("gpu_failure_class", "none"),
            field("gpu_failure_stage", "none"),
            field("discarded_partial_gpu_result", false),
            field("gpu_original_result_incomplete", false),
            field("workers_requested", self.problem.backend_policy().workers()),
            field("workers_used", self.workers_used),
            field("cpu_parallel_execution", self.workers_used > 1),
            field(
                "cpu_parallel_decision_reason",
                self.parallel_decision_reason,
            ),
            field("cpu_parallel_active_workers", self.parallel_active_workers),
            field(
                "cpu_parallel_minimum_worker_candidates",
                self.parallel_minimum_worker_candidates,
            ),
            field(
                "cpu_parallel_maximum_worker_candidates",
                self.parallel_maximum_worker_candidates,
            ),
            field(
                "cpu_warmup_requested",
                self.problem.backend_policy().cpu_warmup(),
            ),
            field(
                "cpu_warmup_performed",
                self.problem.backend_policy().cpu_warmup(),
            ),
            field(
                "supply_window_resolution",
                self.problem.supply().supply_window_resolution(),
            ),
            field(
                "projects_unplaced_lookahead",
                self.problem.supply().projects_unplaced_lookahead(),
            ),
            field(
                "projects_standard_bag_lookahead",
                self.problem.supply().projects_standard_bag_lookahead(),
            ),
            field("source_sequence_length", source_sequence_length),
            field(
                "total_possible_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field("search_kind", "build-probability"),
            field(
                "build_probability_completion",
                "exact-board-with-inverse-lock-clear",
            ),
            field("build_base_mask", self.catalog.initial_board()),
            field("build_target_cells_mask", self.target_cells),
            field("build_target_board_mask", self.target_board),
            field("build_final_board_mask", self.target_board),
            field("target_piece_count", self.target_cells.count_ones() / 4),
            field("solution_found", solution_found),
            field("packing_candidate_count", self.candidate_count),
            field(
                "geometry_candidate_family_count",
                self.geometry.candidate_family_count().map_or_else(
                    || "overflow-or-incomplete".to_owned(),
                    |count| count.to_string(),
                ),
            ),
            field(
                "packing_candidate_set_digest",
                format!("{:016x}", self.candidate_digest),
            ),
            field(
                "unique_solution_count",
                self.buildable_tilings.len() + usize::from(self.trivial_target),
            ),
            field("normalized_solution_set_hash", normalized_hash),
            field("build_variant_count", self.build_variant_count),
            field("build_variant_count_exact", build_variant_count_exact),
            field(
                "build_probability_evaluation_basis",
                if tiling_only {
                    "geometry-only"
                } else {
                    "candidate-pattern-existence"
                },
            ),
            field("build_path_multiplicity_counted", false),
            field("materialized_pattern_count", universe.pattern_count()),
            field("coverage_pattern_count", universe.pattern_count()),
            field(
                "covered_pattern_count",
                if tiling_only {
                    0
                } else {
                    self.covered_patterns.count_ones()
                },
            ),
            field("coverage_probability", probability),
            field("probability_complete", probability_complete),
            field("count_complete", count_complete),
            field("searched_nodes", self.geometry.expanded_nodes()),
            field(
                "geometry_domain_pruned_states",
                self.geometry.domain_pruned_states(),
            ),
            field(
                "geometry_hall_pruned_states",
                self.geometry.hall_pruned_states(),
            ),
            field(
                "geometry_column_pruned_states",
                self.geometry.column_pruned_states(),
            ),
            field(
                "geometry_component_compositions",
                self.geometry.component_compositions(),
            ),
            field(
                "resource_peak_frontier_states",
                self.geometry.peak_frontier(),
            ),
            field("resource_peak_cpu_bytes", self.retained_bytes()),
            field("peak_build_order_nodes", self.peak_build_nodes),
            field("total_build_order_nodes", self.total_build_nodes),
            field("coverage_product_words", self.covered_patterns.word_count()),
            field("coverage_product_states", self.coverage_product_states),
            field(
                "coverage_product_edge_checks",
                self.coverage_product_edge_checks,
            ),
            field("peak_reachability_states", self.peak_reachability_states),
            field("total_reachability_states", self.total_reachability_states),
            field("resource_truncated", self.truncated_reason.is_some()),
            field(
                "resource_truncation_reason",
                self.truncated_reason.unwrap_or("none"),
            ),
            field("objective", "build-probability"),
            field("build_probability_aggregation", self.aggregation.as_str()),
            field("buildability_verified", !tiling_only),
            field("coverage_calculated", !tiling_only),
            field("probability_calculated", !tiling_only),
            field(
                "spin_profile_requested",
                self.aggregation
                    .spin_profile()
                    .map_or("none", |profile| profile.as_str()),
            ),
            field(
                "postprocess_build_spin_requested",
                self.aggregation.requests_spin_coverage(),
            ),
            field(
                "execution_constraint_preserve_b2b",
                execution_constraints.preserves_back_to_back(),
            ),
            field(
                "execution_constraint_spin_profile",
                execution_constraints.spin_profile().as_str(),
            ),
            field(
                "execution_constraint_materialized",
                self.distributed_execution_constraint_materialized,
            ),
            field(
                "objective_search_complete",
                count_complete && (tiling_only || probability_complete),
            ),
            field(
                "objective_complete",
                count_complete
                    && (tiling_only || probability_complete)
                    && execution_constraint_complete,
            ),
            field(
                "objective_incomplete_reason",
                if !count_complete || (!tiling_only && !probability_complete) {
                    self.truncated_reason
                        .unwrap_or("pattern_universe_incomplete")
                } else if !execution_constraint_complete {
                    "b2b_preservation_not_materialized"
                } else {
                    "none"
                },
            ),
            field(
                "representative_pattern_id",
                self.representative_pattern_id
                    .map_or_else(|| "none".to_owned(), |id| id.to_string()),
            ),
            field(
                "representative_candidate_ordinal",
                self.representative_rank
                    .map_or_else(|| "none".to_owned(), |rank| rank.to_string()),
            ),
        ];
        let result = CoreExecutionResult::new(fields, self.representative_path.clone())
            .with_normalized_solution_keys(normalized_keys)
            .with_normalized_solution_identities(identities)
            .with_coverage_pattern_words(self.covered_patterns.words().to_vec())
            .with_solution_coverages(solution_coverages)
            .with_normalized_solution_coverages(normalized_solution_coverages)
            .with_exact_scoring_execution_batch(scoring_batch);
        let result = if execution_constraints.requested() {
            let pattern_weights = (0..universe.pattern_count())
                .map(|pattern| universe.weight_at(pattern).get().to_string())
                .collect();
            result.with_postprocess_execution_batch(
                Vec::new(),
                count_complete && probability_complete,
                pattern_weights,
            )
        } else {
            result
        };
        Ok(result)
    }

    pub(super) fn finesse_search_material(
        &self,
    ) -> Result<FinesseSearchMaterial, WasmExactSearchError> {
        let mut languages = Vec::new();
        languages
            .try_reserve_exact(self.finesse_languages.len() + usize::from(self.trivial_target))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_evaluation_language_storage_unavailable",
                )
            })?;
        for (identity, prepared) in &self.finesse_languages {
            languages.push((
                NormalizedTilingSolutionKey::from_standard_board64_identity(*identity)
                    .as_str()
                    .to_owned(),
                costed_finesse_language(prepared)?,
            ));
        }
        if self.trivial_target {
            let identity = StandardBoard64TilingIdentity::from_placements(
                self.catalog.initial_board(),
                std::iter::empty(),
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_trivial_identity_invalid")
            })?;
            let language = CostedGeometryLanguage::new(
                GeometryNodeId::new(0),
                vec![GeometryLanguageNode::new(
                    0,
                    true,
                    Vec::<CostedGeometryEdge>::new(),
                )],
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_trivial_language_invalid")
            })?;
            languages.push((
                NormalizedTilingSolutionKey::from_standard_board64_identity(identity)
                    .as_str()
                    .to_owned(),
                language,
            ));
        }
        sort_finesse_language_alternatives(&mut languages);

        FinesseSearchMaterial::new(
            &self.problem,
            languages,
            self.truncated_reason.is_none() && self.count_complete,
        )
    }

    fn retained_bytes(&self) -> usize {
        self.catalog.retained_bytes()
            + self.geometry.retained_bytes()
            + self.buildup.retained_bytes()
            + self.coverage_evaluator.retained_bytes()
            + self.covered_patterns.retained_bytes()
            + self.buildable_tilings.capacity()
                * core::mem::size_of::<StandardBoard64TilingIdentity>()
            + self.solution_coverage.as_ref().map_or(0, |coverage| {
                coverage.capacity()
                    * (core::mem::size_of::<StandardBoard64TilingIdentity>()
                        + core::mem::size_of::<PatternBitSet>())
                    + coverage
                        .values()
                        .map(PatternBitSet::retained_bytes)
                        .sum::<usize>()
            })
            + self.finesse_languages.capacity()
                * core::mem::size_of::<(StandardBoard64TilingIdentity, PreparedFinesseLanguage)>()
            + self
                .finesse_languages
                .iter()
                .map(|(_, language)| {
                    language.nodes.capacity()
                        * core::mem::size_of::<super::buildup::PreparedFinesseNode>()
                        + language.edges.capacity()
                            * core::mem::size_of::<super::buildup::PreparedFinesseEdge>()
                })
                .sum::<usize>()
    }
}

pub(super) struct FinesseSearchMaterial {
    classes: QueueClassSet,
    languages: Vec<(String, CostedGeometryLanguage)>,
    fixed_queue: bool,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    kick_profile: KickTableProfile,
}

impl FinesseSearchMaterial {
    pub(super) fn new(
        problem: &SearchProblem,
        languages: Vec<(String, CostedGeometryLanguage)>,
        evaluation_complete: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let hold_enabled = problem.supply().hold_enabled();
        Ok(Self {
            classes: finesse_queue_classes_for_problem(problem, evaluation_complete)?,
            languages,
            fixed_queue: problem.piece_source().fixed_sequence().is_some(),
            initial_hold: hold_enabled
                .then(|| problem.initial_hold().hold_piece())
                .flatten(),
            hold_enabled,
            terminal_hold_release: problem.supply().projects_unplaced_lookahead(),
            spawn_profile: problem.spawn_profile(),
            kick_profile: super::kick_profiles::builtin_kick_profile(
                problem.kick_profile().profile_id(),
            )
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_kick_profile_unavailable",
            ))?
            .clone(),
        })
    }
}

fn sort_finesse_language_alternatives(languages: &mut [(String, CostedGeometryLanguage)]) {
    // A normalized occupancy key does not identify its concrete rotation or
    // realization language. Keep every alternative; the outer exact union
    // determinizes their placement actions after all symmetry passes arrive.
    languages.sort_unstable_by(|left, right| left.0.cmp(&right.0));
}

struct EvaluatedFinessePolicy {
    report: FinessePolicyResult,
    overall_costs: QueueCostTable,
    aggregation: QueueCostAggregation,
    representative: Option<FinesseRepresentativeSelection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FinesseRepresentativeSelection {
    pub(super) solution_index: usize,
    pub(super) class_index: usize,
    pub(super) expected_cost: u32,
}

pub(super) fn build_finesse_report(
    materials: Vec<FinesseSearchMaterial>,
    pattern_knowledge: FinessePatternKnowledge,
    control: &ExecutionControl,
) -> Result<FinesseReport, WasmExactSearchError> {
    ensure_finesse_not_cancelled(control)?;
    let mut materials = materials.into_iter();
    let first = materials
        .next()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_search_material_missing",
        ))?;
    let fixed_queue = first.fixed_queue;
    let initial_hold = first.initial_hold;
    let hold_enabled = first.hold_enabled;
    let terminal_hold_release = first.terminal_hold_release;
    let spawn_profile = first.spawn_profile;
    let kick_profile = first.kick_profile;
    let mut complete = first.classes.metadata().complete;
    let mut classes = first.classes;
    let mut language_groups = BTreeMap::<String, Vec<CostedGeometryLanguage>>::new();
    for (solution_key, language) in first.languages {
        language_groups
            .entry(solution_key)
            .or_default()
            .push(language);
    }
    for material in materials {
        ensure_finesse_not_cancelled(control)?;
        if material.fixed_queue != fixed_queue
            || material.initial_hold != initial_hold
            || material.hold_enabled != hold_enabled
            || material.terminal_hold_release != terminal_hold_release
            || material.spawn_profile != spawn_profile
            || material.kick_profile != kick_profile
            || material.classes.classes() != classes.classes()
            || material.classes.metadata().pattern_count != classes.metadata().pattern_count
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_symmetry_material_mismatch",
            ));
        }
        complete &= material.classes.metadata().complete;
        for (solution_key, language) in material.languages {
            language_groups
                .entry(solution_key)
                .or_default()
                .push(language);
        }
    }
    classes = classes.with_complete(complete);

    let mut languages = Vec::new();
    languages
        .try_reserve_exact(language_groups.len())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_union_storage_unavailable")
        })?;
    for (solution_key, mut alternatives) in language_groups {
        ensure_finesse_not_cancelled(control)?;
        let language = if alternatives.len() == 1 {
            alternatives
                .pop()
                .expect("one solution language is present")
        } else {
            let references = alternatives.iter().collect::<Vec<_>>();
            union_costed_geometry_languages(&references).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_solution_union_failed")
            })?
        };
        languages.push((solution_key, language));
    }

    let oracle_requested = matches!(
        pattern_knowledge,
        FinessePatternKnowledge::Both | FinessePatternKnowledge::Oracle
    );
    let visible_requested = matches!(
        pattern_knowledge,
        FinessePatternKnowledge::Both | FinessePatternKnowledge::VisibleSeven
    );
    // Visible-7 reports always include an Oracle baseline over the same
    // materialized universe, even when the caller does not request the Oracle
    // policy as a standalone result.
    let mut oracle = (oracle_requested || visible_requested)
        .then(|| {
            evaluate_finesse_policy(
                "oracle",
                &languages,
                &classes,
                fixed_queue,
                initial_hold,
                hold_enabled,
                terminal_hold_release,
                spawn_profile,
                control,
            )
        })
        .transpose()?;
    let mut visible = visible_requested
        .then(|| {
            evaluate_finesse_policy(
                "visible-7",
                &languages,
                &classes,
                fixed_queue,
                initial_hold,
                hold_enabled,
                terminal_hold_release,
                spawn_profile,
                control,
            )
        })
        .transpose()?;

    if let (Some(oracle_result), Some(visible_result)) = (&oracle, &mut visible) {
        let mut oracle_on_visible =
            QueueCostTable::unreachable(classes.classes().len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_oracle_comparison_storage_unavailable",
                )
            })?;
        for class_index in 0..classes.classes().len() {
            if visible_result
                .overall_costs
                .get(class_index)
                .flatten()
                .is_none()
            {
                continue;
            }
            if let Some(cost) = oracle_result.overall_costs.get(class_index).flatten() {
                oracle_on_visible.set_min(class_index, cost).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_oracle_comparison_cost_invalid",
                    )
                })?;
            }
        }
        let oracle_covered =
            aggregate_unique_queue_costs(&classes, &oracle_on_visible).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_oracle_comparison_aggregation_failed",
                )
            })?;
        let oracle_average = oracle_covered.conditional_mean_inputs;
        let information_penalty = visible_result
            .aggregation
            .conditional_mean_inputs
            .zip(oracle_average)
            .map(|(visible_average, oracle_average)| {
                (visible_average - oracle_average).max(0.0).to_string()
            });
        let success_probability_gap = (oracle_result.aggregation.successful_probability_mass
            - visible_result.aggregation.successful_probability_mass)
            .max(0.0)
            .to_string();
        visible_result.report = visible_result.report.clone().with_comparison(
            oracle_average.map(|average| average.to_string()),
            information_penalty,
            Some(success_probability_gap),
        );
    }

    let exact_total_inputs = (fixed_queue && classes.classes().len() == 1)
        .then(|| {
            oracle
                .as_ref()
                .or(visible.as_ref())
                .and_then(|result| result.overall_costs.get(0).flatten())
                .map(|cost| cost.to_string())
        })
        .flatten();
    let selected_policy = if oracle_requested {
        oracle.as_ref()
    } else {
        visible.as_ref()
    };
    let representative_witness = if fixed_queue && classes.classes().len() == 1 {
        fixed_queue_representative_witness(
            if oracle_requested {
                "oracle"
            } else {
                "visible-7"
            },
            &languages,
            &classes,
            initial_hold,
            hold_enabled,
            terminal_hold_release,
            spawn_profile,
            &kick_profile,
            control,
        )?
    } else {
        selected_policy
            .and_then(|evaluated| evaluated.representative)
            .map(|selection| {
                pattern_representative_witness(
                    if oracle_requested {
                        "oracle"
                    } else {
                        "visible-7"
                    },
                    selection,
                    &languages,
                    &classes,
                    initial_hold,
                    hold_enabled,
                    terminal_hold_release,
                    spawn_profile,
                    &kick_profile,
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
    let mut policy_results = Vec::with_capacity(
        usize::from(oracle_requested && oracle.is_some()) + usize::from(visible.is_some()),
    );
    if oracle_requested {
        if let Some(result) = oracle.take() {
            policy_results.push(result.report);
        }
    }
    if let Some(result) = visible.take() {
        policy_results.push(result.report);
    }
    let report_complete =
        !policy_results.is_empty() && policy_results.iter().all(FinessePolicyResult::complete);
    let report = FinesseReport::new(
        "search",
        pattern_knowledge.as_str(),
        report_complete,
        exact_total_inputs,
        policy_results,
    );
    Ok(match representative_witness {
        Some(witness) => report.with_representative_witness(witness),
        None => report,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fixed_queue_representative_witness(
    policy: &'static str,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    kick_profile: &KickTableProfile,
    control: &ExecutionControl,
) -> Result<Option<FinesseRepresentativeWitness>, WasmExactSearchError> {
    fixed_queue_representative_witness_with_cancel(
        policy,
        languages,
        classes,
        initial_hold,
        hold_enabled,
        terminal_hold_release,
        spawn_profile,
        kick_profile,
        || control.is_cancelled(),
    )
}

#[allow(clippy::too_many_arguments)]
fn fixed_queue_representative_witness_with_cancel(
    policy: &'static str,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    kick_profile: &KickTableProfile,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Option<FinesseRepresentativeWitness>, WasmExactSearchError> {
    ensure_finesse_not_cancelled_with(&mut is_cancelled)?;
    let Some(class) = classes
        .classes()
        .first()
        .filter(|_| classes.classes().len() == 1)
    else {
        return Ok(None);
    };
    let mut selected = None;
    for (solution_key, language) in languages {
        ensure_finesse_not_cancelled_with(&mut is_cancelled)?;
        let evaluator = QueueClassProductEvaluator::new(language)
            .with_spawn_profile(spawn_profile)
            .with_hold_enabled(hold_enabled)
            .with_terminal_hold_release_enabled(terminal_hold_release);
        let Some(cost) = evaluator
            .fixed_queue_cost_with_cancel(class.queue(), initial_hold, &mut is_cancelled)
            .map_err(|error| {
                map_finesse_product_error(
                    error,
                    "wasm_finesse_representative_cost_evaluation_failed",
                )
            })?
        else {
            continue;
        };
        if selected
            .as_ref()
            .is_none_or(|(_, _, best_cost)| cost < *best_cost)
        {
            selected = Some((solution_key, language, cost));
        }
    }
    let Some((solution_key, language, expected_cost)) = selected else {
        return Ok(None);
    };
    ensure_finesse_not_cancelled_with(&mut is_cancelled)?;
    let witness_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseWitness);
    let witness = QueueClassProductEvaluator::new(language)
        .with_spawn_profile(spawn_profile)
        .with_hold_enabled(hold_enabled)
        .with_terminal_hold_release_enabled(terminal_hold_release)
        .replay_fixed_queue_witness_with_cancel(
            class.queue(),
            initial_hold,
            spawn_profile,
            kick_profile,
            &mut is_cancelled,
        )
        .map_err(|error| {
            map_finesse_route_witness_error(error, "wasm_finesse_representative_witness_failed")
        })?
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_representative_witness_missing",
        ))?;
    if witness.total_cost() != expected_cost {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_representative_witness_cost_mismatch",
        ));
    }
    witness_span.finish(witness.inputs().len() as u64);
    Ok(Some(FinesseRepresentativeWitness::new(
        policy,
        Some(solution_key.clone()),
        class
            .pattern_ids()
            .iter()
            .map(|pattern| pattern.index())
            .collect(),
        class.queue().to_vec(),
        witness.total_cost(),
        witness
            .inputs()
            .iter()
            .copied()
            .map(FinesseReportInput::from)
            .collect(),
        witness
            .placements()
            .iter()
            .copied()
            .map(crate::FinesseReportPlacement::from)
            .collect(),
    )))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pattern_representative_witness(
    policy: &'static str,
    selection: FinesseRepresentativeSelection,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    kick_profile: &KickTableProfile,
    control: &ExecutionControl,
) -> Result<Option<FinesseRepresentativeWitness>, WasmExactSearchError> {
    pattern_representative_witness_with_cancel(
        policy,
        selection,
        languages,
        classes,
        initial_hold,
        hold_enabled,
        terminal_hold_release,
        spawn_profile,
        kick_profile,
        || control.is_cancelled(),
    )
}

#[allow(clippy::too_many_arguments)]
fn pattern_representative_witness_with_cancel(
    policy: &'static str,
    selection: FinesseRepresentativeSelection,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    kick_profile: &KickTableProfile,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Option<FinesseRepresentativeWitness>, WasmExactSearchError> {
    ensure_finesse_not_cancelled_with(&mut is_cancelled)?;
    let (solution_key, language) =
        languages
            .get(selection.solution_index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_representative_solution_missing",
            ))?;
    let class = classes.classes().get(selection.class_index).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_finesse_representative_queue_missing"),
    )?;
    let evaluator = QueueClassProductEvaluator::new(language)
        .with_spawn_profile(spawn_profile)
        .with_hold_enabled(hold_enabled)
        .with_terminal_hold_release_enabled(terminal_hold_release);
    let witness_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseWitness);
    let witness = match policy {
        "oracle" => evaluator.replay_fixed_queue_witness_with_cancel(
            class.queue(),
            initial_hold,
            spawn_profile,
            kick_profile,
            &mut is_cancelled,
        ),
        "visible-7" => evaluator.replay_visible_seven_class_witness_with_cancel(
            classes,
            initial_hold,
            selection.class_index,
            spawn_profile,
            kick_profile,
            &mut is_cancelled,
        ),
        _ => unreachable!("finesse policy is selected internally"),
    }
    .map_err(|error| {
        map_finesse_route_witness_error(error, "wasm_finesse_representative_witness_failed")
    })?
    .ok_or(WasmExactSearchError::InvalidProblem(
        "wasm_finesse_representative_witness_missing",
    ))?;
    if witness.total_cost() != selection.expected_cost {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_representative_witness_cost_mismatch",
        ));
    }
    witness_span.finish(witness.inputs().len() as u64);
    Ok(Some(FinesseRepresentativeWitness::new(
        policy,
        Some(solution_key.clone()),
        class
            .pattern_ids()
            .iter()
            .map(|pattern| pattern.index())
            .collect(),
        class.queue().to_vec(),
        witness.total_cost(),
        witness
            .inputs()
            .iter()
            .copied()
            .map(FinesseReportInput::from)
            .collect(),
        witness
            .placements()
            .iter()
            .copied()
            .map(crate::FinesseReportPlacement::from)
            .collect(),
    )))
}

pub(super) fn costed_finesse_language(
    prepared: &PreparedFinesseLanguage,
) -> Result<CostedGeometryLanguage, WasmExactSearchError> {
    let mut nodes = Vec::new();
    nodes.try_reserve_exact(prepared.nodes.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_costed_node_storage_unavailable")
    })?;
    for node in &prepared.nodes {
        let start = usize::try_from(node.edge_start)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_edge_range_invalid"))?;
        let count = usize::try_from(node.edge_count)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_edge_range_invalid"))?;
        let end = start
            .checked_add(count)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_edge_range_invalid",
            ))?;
        let source_edges =
            prepared
                .edges
                .get(start..end)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_finesse_edge_range_invalid",
                ))?;
        let mut edges = Vec::new();
        edges.try_reserve_exact(source_edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_costed_edge_storage_unavailable")
        })?;
        edges.extend(source_edges.iter().map(|edge| {
            let mut converted = CostedGeometryEdge::new(
                edge.piece,
                GeometryNodeId::new(edge.child),
                edge.cost,
                edge.transition_order,
            )
            .with_action_key(edge.action_key);
            if let Some(evidence) = edge.terminal_evidence {
                converted = converted.with_terminal_evidence(evidence);
            }
            converted
        }));
        let mut converted = GeometryLanguageNode::new(u16::from(node.depth), node.accepting, edges);
        if let Some(source_board) = node.source_board {
            converted = converted.with_source_board(source_board);
        }
        nodes.push(converted);
    }
    CostedGeometryLanguage::new(GeometryNodeId::new(prepared.root), nodes)
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_costed_language_invalid"))
}

fn evaluate_finesse_policy(
    policy: &'static str,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    fixed_queue: bool,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    control: &ExecutionControl,
) -> Result<EvaluatedFinessePolicy, WasmExactSearchError> {
    let product_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseProductDp);
    let mut solutions = Vec::new();
    solutions.try_reserve_exact(languages.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_solution_cost_storage_unavailable")
    })?;
    for (solution_key, language) in languages {
        ensure_finesse_not_cancelled(control)?;
        let evaluator = QueueClassProductEvaluator::new(language)
            .with_spawn_profile(spawn_profile)
            .with_hold_enabled(hold_enabled)
            .with_terminal_hold_release_enabled(terminal_hold_release);
        let costs = finesse_policy_costs(
            &evaluator,
            policy,
            classes,
            fixed_queue,
            initial_hold,
            control,
            "wasm_finesse_policy_evaluation_failed",
        )?;
        solutions.push((solution_key.clone(), costs));
    }
    let overall_costs = if languages.is_empty() {
        QueueCostTable::unreachable(classes.classes().len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_overall_cost_storage_unavailable")
        })?
    } else {
        ensure_finesse_not_cancelled(control)?;
        let references = languages
            .iter()
            .map(|(_, language)| language)
            .collect::<Vec<_>>();
        let union = union_costed_geometry_languages(&references).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_overall_union_failed")
        })?;
        let evaluator = QueueClassProductEvaluator::new(&union)
            .with_spawn_profile(spawn_profile)
            .with_hold_enabled(hold_enabled)
            .with_terminal_hold_release_enabled(terminal_hold_release);
        finesse_policy_costs(
            &evaluator,
            policy,
            classes,
            fixed_queue,
            initial_hold,
            control,
            "wasm_finesse_overall_policy_evaluation_failed",
        )?
    };
    product_span
        .finish((languages.len().saturating_add(1)).saturating_mul(classes.classes().len()) as u64);

    let aggregation_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAggregation);
    let mut solution_averages = Vec::new();
    solution_averages
        .try_reserve_exact(solutions.len())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_average_storage_unavailable")
        })?;
    for (solution_key, costs) in &solutions {
        ensure_finesse_not_cancelled(control)?;
        let aggregation = aggregate_unique_queue_costs(classes, costs).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_solution_aggregation_failed")
        })?;
        solution_averages.push(FinesseSolutionAverage::new(
            solution_key,
            finesse_average_text(aggregation.conditional_mean_inputs),
            aggregation.complete,
        ));
    }
    let aggregation = aggregate_unique_queue_costs(classes, &overall_costs).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_overall_aggregation_failed")
    })?;
    let mut representative = None;
    for (solution_index, (_, costs)) in solutions.iter().enumerate() {
        for class_index in 0..classes.classes().len() {
            let Some(expected_cost) = costs.get(class_index).flatten() else {
                continue;
            };
            let candidate = FinesseRepresentativeSelection {
                solution_index,
                class_index,
                expected_cost,
            };
            if representative
                .as_ref()
                .is_none_or(|current: &FinesseRepresentativeSelection| {
                    (
                        candidate.expected_cost,
                        candidate.solution_index,
                        candidate.class_index,
                    ) < (
                        current.expected_cost,
                        current.solution_index,
                        current.class_index,
                    )
                })
            {
                representative = Some(candidate);
            }
        }
    }
    let report = FinessePolicyResult::new(
        policy,
        finesse_average_text(aggregation.conditional_mean_inputs),
        aggregation.complete,
        solution_averages,
    )
    .with_success_summary(
        aggregation.successful_probability_mass.to_string(),
        aggregation.successful_unique_queue_count,
        aggregation.total_unique_queue_count,
    );
    aggregation_span.finish(solutions.len().saturating_add(1) as u64);
    Ok(EvaluatedFinessePolicy {
        report,
        overall_costs,
        aggregation,
        representative,
    })
}

pub(super) fn finesse_policy_costs(
    evaluator: &QueueClassProductEvaluator<'_>,
    policy: &'static str,
    classes: &QueueClassSet,
    fixed_queue: bool,
    initial_hold: Option<PieceKind>,
    control: &ExecutionControl,
    fallback: &'static str,
) -> Result<QueueCostTable, WasmExactSearchError> {
    if fixed_queue {
        let [class] = classes.classes() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_fixed_queue_class_mismatch",
            ));
        };
        let mut costs = QueueCostTable::unreachable(1)
            .map_err(|error| map_finesse_product_error(error, fallback))?;
        if let Some(cost) = evaluator
            .fixed_queue_cost_with_cancel(class.queue(), initial_hold, || control.is_cancelled())
            .map_err(|error| map_finesse_product_error(error, fallback))?
        {
            costs
                .set_min(0, cost)
                .map_err(|error| map_finesse_product_error(error, fallback))?;
        }
        return Ok(costs);
    }
    match policy {
        "oracle" => evaluator
            .oracle_with_cancel(classes, initial_hold, || control.is_cancelled())
            .map(|result| result.costs),
        "visible-7" => evaluator
            .visible_seven_with_cancel(classes, initial_hold, || control.is_cancelled())
            .map(|result| result.costs),
        _ => unreachable!("finesse policy is selected internally"),
    }
    .map_err(|error| map_finesse_product_error(error, fallback))
}

fn ensure_finesse_not_cancelled(control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
    ensure_finesse_not_cancelled_with(&mut || control.is_cancelled())
}

fn ensure_finesse_not_cancelled_with(
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), WasmExactSearchError> {
    if is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

fn map_finesse_product_error(
    error: GeometryLanguageError,
    fallback: &'static str,
) -> WasmExactSearchError {
    match error {
        GeometryLanguageError::Cancelled => WasmExactSearchError::Cancelled,
        _ => WasmExactSearchError::InvalidProblem(fallback),
    }
}

fn map_finesse_route_witness_error(
    error: FinesseRouteWitnessError,
    fallback: &'static str,
) -> WasmExactSearchError {
    match error {
        FinesseRouteWitnessError::Geometry(GeometryLanguageError::Cancelled)
        | FinesseRouteWitnessError::Movement(FinesseError::Cancelled) => {
            WasmExactSearchError::Cancelled
        }
        _ => WasmExactSearchError::InvalidProblem(fallback),
    }
}

fn finesse_average_text(average: Option<f64>) -> String {
    average.map_or_else(|| "not-calculated".to_owned(), |value| value.to_string())
}

pub(super) fn finesse_queue_classes_for_problem(
    problem: &SearchProblem,
    evaluation_complete: bool,
) -> Result<QueueClassSet, WasmExactSearchError> {
    let universe = problem.piece_source().materialized_universe().ok_or(
        WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
    )?;
    let initial_cursor = usize::from(problem.initial_hold().cursor());
    let mut patterns = Vec::new();
    patterns
        .try_reserve_exact(universe.pattern_count())
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_finesse_queue_storage_unavailable")
        })?;
    for pattern_index in 0..universe.pattern_count() {
        let mut sequence = universe.sequence_at(pattern_index).into_owned();
        if problem.supply().projects_standard_bag_lookahead() {
            append_projected_finesse_bag_piece(&mut sequence)?;
        }
        let queue = sequence
            .get(initial_cursor..)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_initial_cursor_out_of_range",
            ))?;
        patterns.push(QueuePattern::new(
            PatternId::new(pattern_index),
            queue.to_vec(),
            universe.weight_at(pattern_index),
        ));
    }
    QueueClassSet::group(&patterns, universe.complete() && evaluation_complete)
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_finesse_queue_grouping_failed"))
}

fn append_projected_finesse_bag_piece(
    sequence: &mut Vec<PieceKind>,
) -> Result<(), WasmExactSearchError> {
    if sequence.len() % 7 != 6 {
        return Ok(());
    }
    let mut present = 0_u8;
    for piece in &sequence[sequence.len() - 6..] {
        present |= 1_u8 << finesse_piece_index(*piece);
    }
    let missing = (!present) & 0x7f;
    if missing.count_ones() != 1 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_projected_bag_piece_invalid",
        ));
    }
    sequence.push(PieceKind::STANDARD_TETROMINOES[missing.trailing_zeros() as usize]);
    Ok(())
}

const fn finesse_piece_index(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn merge_board64_solution_coverages(
    results: &[&CoreExecutionResult],
    pattern_count: usize,
) -> Result<Vec<SolutionCoverage>, WasmExactSearchError> {
    let mut merged = BTreeMap::<StandardBoard64TilingIdentity, PatternBitSet>::new();
    for result in results {
        for coverage in result.solution_coverages() {
            let entry = merged
                .entry(coverage.identity())
                .or_insert_with(|| PatternBitSet::new(pattern_count));
            entry.union_with(coverage.covered_patterns()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_merge_mismatch",
                )
            })?;
        }
    }
    Ok(merged
        .into_iter()
        .map(|(identity, coverage)| SolutionCoverage::new(identity, coverage))
        .collect())
}

fn merge_normalized_solution_coverages(
    results: &[&CoreExecutionResult],
    pattern_count: usize,
) -> Result<Vec<NormalizedSolutionCoverage>, WasmExactSearchError> {
    let mut merged = BTreeMap::<String, PatternBitSet>::new();
    for result in results {
        for coverage in result.normalized_solution_coverages() {
            let entry = merged
                .entry(coverage.solution_key().to_owned())
                .or_insert_with(|| PatternBitSet::new(pattern_count));
            entry.union_with(coverage.covered_patterns()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_normalized_coverage_merge_mismatch",
                )
            })?;
        }
    }
    Ok(merged
        .into_iter()
        .map(|(key, coverage)| NormalizedSolutionCoverage::new(key, coverage))
        .collect())
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod finesse_integration_tests {
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        piece::rotation::RotationState,
        probability::probability_value::ProbabilityValue,
    };
    use clearra_finesse::FinesseBoard;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{BuildProbabilityQuery, FinessePlacement, ProblemCompiler};
    use clearra_supply::queue::{
        fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
    };

    use super::*;
    use crate::backend::wasm_cpu::buildup::{PreparedFinesseEdge, PreparedFinesseNode};

    fn probability(value: f64) -> ProbabilityValue {
        ProbabilityValue::new(value).expect("test probability is valid")
    }

    fn one_piece_language(piece: PieceKind, cost: u32) -> PreparedFinesseLanguage {
        let source_board = FinesseBoard::new(
            Board64Layout::new(BoardSize::new(10, 4).expect("test board size"))
                .expect("test board layout"),
            0,
        )
        .expect("empty finesse board");
        PreparedFinesseLanguage {
            nodes: vec![
                PreparedFinesseNode {
                    edge_start: 0,
                    edge_count: 1,
                    depth: 0,
                    accepting: false,
                    source_board: Some(source_board),
                },
                PreparedFinesseNode {
                    edge_start: 1,
                    edge_count: 0,
                    depth: 1,
                    accepting: true,
                    source_board: None,
                },
            ],
            edges: vec![PreparedFinesseEdge {
                child: 1,
                piece,
                cost,
                transition_order: 7,
                action_key: clearra_finesse::GeometryActionKey::new(
                    piece,
                    clearra_core_domain::piece::rotation::RotationState::Zero,
                    if piece == PieceKind::I && cost == 3 {
                        1
                    } else {
                        0
                    },
                    0,
                ),
                terminal_evidence: None,
            }],
            root: 0,
        }
    }

    #[test]
    fn representative_witness_cancellation_maps_to_executor_cancellation() {
        assert_eq!(
            map_finesse_route_witness_error(
                FinesseRouteWitnessError::Geometry(GeometryLanguageError::Cancelled),
                "fallback",
            ),
            WasmExactSearchError::Cancelled
        );
        assert_eq!(
            map_finesse_route_witness_error(
                FinesseRouteWitnessError::Movement(FinesseError::Cancelled),
                "fallback",
            ),
            WasmExactSearchError::Cancelled
        );
    }

    #[test]
    fn search_and_score_representative_helpers_forward_cancellation_to_every_policy() {
        let language = costed_finesse_language(&one_piece_language(PieceKind::O, 1)).unwrap();
        let languages = vec![("solution".to_owned(), language)];
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::O],
                ProbabilityValue::ONE,
            )],
            false,
        )
        .unwrap();
        let kicks = clearra_rules::kicks::NoKick::profile();

        // Search and Score share these fixed/pattern representative helpers.
        // Cancel after entering the lower product/replay API, rather than at
        // the helper's initial boundary, so closure forwarding is exercised.
        for policy in ["oracle", "visible-7"] {
            let mut checks = 0;
            assert_eq!(
                fixed_queue_representative_witness_with_cancel(
                    policy,
                    &languages,
                    &classes,
                    None,
                    false,
                    false,
                    SpawnProfile::new(0, 4),
                    &kicks,
                    || {
                        checks += 1;
                        checks == 3
                    },
                ),
                Err(WasmExactSearchError::Cancelled)
            );
            assert_eq!(checks, 3);
        }

        for policy in ["oracle", "visible-7"] {
            let mut checks = 0;
            assert_eq!(
                pattern_representative_witness_with_cancel(
                    policy,
                    FinesseRepresentativeSelection {
                        solution_index: 0,
                        class_index: 0,
                        expected_cost: 1,
                    },
                    &languages,
                    &classes,
                    None,
                    false,
                    false,
                    SpawnProfile::new(0, 4),
                    &kicks,
                    || {
                        checks += 1;
                        checks == 2
                    },
                ),
                Err(WasmExactSearchError::Cancelled)
            );
            assert_eq!(checks, 2);
        }
    }

    #[test]
    fn prepared_language_keeps_cost_and_transition_order() {
        let language = costed_finesse_language(&one_piece_language(PieceKind::T, 4)).unwrap();
        let edge = language.node(language.root()).unwrap().edges()[0];

        assert_eq!(edge.piece(), PieceKind::T);
        assert_eq!(edge.input_cost(), 4);
        assert_eq!(edge.transition_order(), 7);
        assert!(language.node(edge.child()).unwrap().accepting());
    }

    #[test]
    fn compact_material_keeps_same_occupancy_rotation_alternatives() {
        let alternative = |rotation, cost| {
            CostedGeometryLanguage::new(
                GeometryNodeId::new(0),
                vec![
                    GeometryLanguageNode::new(
                        0,
                        false,
                        vec![CostedGeometryEdge::new(
                            PieceKind::O,
                            GeometryNodeId::new(1),
                            cost,
                            0,
                        )
                        .with_action_key(
                            clearra_finesse::GeometryActionKey::new(PieceKind::O, rotation, 4, 0),
                        )],
                    ),
                    GeometryLanguageNode::new(1, true, Vec::<CostedGeometryEdge>::new()),
                ],
            )
            .unwrap()
        };
        let mut languages = vec![
            (
                "same-occupancy".to_owned(),
                alternative(RotationState::Zero, 4),
            ),
            (
                "same-occupancy".to_owned(),
                alternative(RotationState::Right, 2),
            ),
        ];
        sort_finesse_language_alternatives(&mut languages);
        assert_eq!(languages.len(), 2);

        let references = languages
            .iter()
            .map(|(_, language)| language)
            .collect::<Vec<_>>();
        let union = union_costed_geometry_languages(&references).unwrap();
        assert_eq!(
            QueueClassProductEvaluator::new(&union)
                .fixed_queue_cost(&[PieceKind::O], None)
                .unwrap(),
            Some(2)
        );
    }

    #[test]
    fn policy_report_keeps_raw_success_mass_and_universe_completeness() {
        let language = costed_finesse_language(&one_piece_language(PieceKind::I, 3)).unwrap();
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(PatternId::new(0), vec![PieceKind::I], probability(0.25)),
                QueuePattern::new(PatternId::new(1), vec![PieceKind::O], probability(0.5)),
            ],
            true,
        )
        .unwrap();

        let evaluated = evaluate_finesse_policy(
            "oracle",
            &[("solution".to_owned(), language)],
            &classes,
            false,
            None,
            false,
            false,
            SpawnProfile::STANDARD_10,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert!(evaluated.report.complete());
        assert_eq!(evaluated.report.overall_average_inputs(), "3");
        assert_eq!(evaluated.report.successful_probability_mass(), Some("0.25"));
        assert_eq!(evaluated.report.successful_unique_queue_count(), Some(1));
        assert_eq!(evaluated.report.total_unique_queue_count(), Some(2));
    }

    #[test]
    fn one_materialized_pattern_class_has_a_representative_but_no_exact_total() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("[I]!", 6).expect("single-queue pattern"),
            ),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        assert!(problem.piece_source().fixed_sequence().is_none());
        let language = costed_finesse_language(&one_piece_language(PieceKind::I, 3)).unwrap();
        let material =
            FinesseSearchMaterial::new(&problem, vec![("solution".to_owned(), language)], false)
                .unwrap();

        let report = build_finesse_report(
            vec![material],
            FinessePatternKnowledge::Oracle,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert!(!report.complete());
        assert_eq!(report.exact_total_inputs(), None);
        let witness = report
            .representative_witness()
            .expect("a successful materialized pattern has one representative");
        assert_eq!(witness.policy(), "oracle");
        assert_eq!(witness.solution_key(), Some("solution"));
        assert_eq!(witness.pattern_ids(), [0]);
        assert_eq!(witness.queue(), [PieceKind::I]);
        assert_eq!(witness.total_inputs(), 3);
        assert_eq!(report.policy_results()[0].overall_average_inputs(), "3");
    }

    #[test]
    fn visible_only_report_keeps_its_oracle_comparison_metrics() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("[I]!", 6).expect("single-queue pattern"),
            ),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let language = costed_finesse_language(&one_piece_language(PieceKind::I, 3)).unwrap();
        let material =
            FinesseSearchMaterial::new(&problem, vec![("solution".to_owned(), language)], true)
                .unwrap();

        let report = build_finesse_report(
            vec![material],
            FinessePatternKnowledge::VisibleSeven,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert_eq!(report.policy_results().len(), 1);
        let visible = &report.policy_results()[0];
        assert_eq!(visible.policy(), "visible-7");
        assert_eq!(visible.oracle_on_covered_average_inputs(), Some("3"));
        assert_eq!(visible.information_penalty_inputs(), Some("0"));
        assert_eq!(visible.success_probability_gap(), Some("0"));
        let witness = report
            .representative_witness()
            .expect("visible-only pattern replay remains available");
        assert_eq!(witness.policy(), "visible-7");
        assert_eq!(witness.total_inputs(), 3);
    }

    #[test]
    fn build_policy_evaluator_honors_no_hold() {
        let language = costed_finesse_language(&one_piece_language(PieceKind::O, 2)).unwrap();
        let classes = QueueClassSet::group(
            &[QueuePattern::new(
                PatternId::new(0),
                vec![PieceKind::I, PieceKind::O],
                ProbabilityValue::ONE,
            )],
            true,
        )
        .unwrap();

        let with_hold = evaluate_finesse_policy(
            "oracle",
            &[("solution".to_owned(), language.clone())],
            &classes,
            true,
            None,
            true,
            false,
            SpawnProfile::STANDARD_10,
            &ExecutionControl::default(),
        )
        .unwrap();
        let without_hold = evaluate_finesse_policy(
            "oracle",
            &[("solution".to_owned(), language)],
            &classes,
            true,
            None,
            false,
            false,
            SpawnProfile::STANDARD_10,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert_eq!(with_hold.report.overall_average_inputs(), "3");
        assert_eq!(
            without_hold.report.overall_average_inputs(),
            "not-calculated"
        );
        assert_eq!(without_hold.report.successful_unique_queue_count(), Some(0));
    }

    #[test]
    fn score_request_skips_build_sessions_and_horizontal_mirror() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .unwrap()
            .with_horizontal_mirror_included(true);
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        )])
        .unwrap();
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::Both,
                request: score,
            },
        )
        .unwrap();

        assert!(session.pending.is_empty());
        assert!(!session.mirror_included);
        assert!(!session.mirror_distinct);
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let result = match session.advance(1, &control).unwrap() {
            BuildProbabilityAdvance::Completed(result) => result,
            BuildProbabilityAdvance::Pending | BuildProbabilityAdvance::Cancelled => {
                panic!("score is one serial execution")
            }
        };

        assert_eq!(result.field("search_kind"), Some("finesse-score"));
        assert!(result.field("packing_candidate_count").is_none());
        assert_eq!(
            result
                .finesse_report()
                .and_then(FinesseReport::exact_total_inputs),
            Some("1")
        );
        let witness = result
            .finesse_report()
            .and_then(FinesseReport::representative_witness)
            .unwrap();
        assert_eq!(witness.policy(), "oracle");
        assert_eq!(witness.solution_key(), Some("given-operation-sequence"));
        assert_eq!(witness.queue(), [PieceKind::O]);
        assert_eq!(witness.total_inputs(), 1);
        assert_eq!(witness.input_sequence(), [FinesseReportInput::HardDrop]);
        assert_eq!(witness.placements().len(), 1);
        assert_eq!(witness.placements()[0].piece(), PieceKind::O);
        assert_eq!(witness.placements()[0].rotation(), RotationState::Zero);
        assert_eq!(
            (witness.placements()[0].x(), witness.placements()[0].y()),
            (4, 0)
        );
        assert!(session.advance(1, &control).is_err());
    }

    #[test]
    fn score_with_one_pattern_queue_reports_an_average_and_representative_witness() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("[O]!", 6).expect("single-queue pattern"),
            ),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        assert!(problem.piece_source().fixed_sequence().is_none());
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4]).unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        )])
        .unwrap();
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::VisibleSeven,
                request: score,
            },
        )
        .unwrap();
        let result = match session.advance(1, &ExecutionControl::default()).unwrap() {
            BuildProbabilityAdvance::Completed(result) => result,
            BuildProbabilityAdvance::Pending | BuildProbabilityAdvance::Cancelled => {
                panic!("score is one serial execution")
            }
        };
        let report = result.finesse_report().expect("score report");

        assert_eq!(report.exact_total_inputs(), None);
        let witness = report
            .representative_witness()
            .expect("a successful score pattern has one representative");
        assert_eq!(witness.policy(), "visible-7");
        assert_eq!(witness.solution_key(), Some("given-operation-sequence"));
        assert_eq!(witness.pattern_ids(), [0]);
        assert_eq!(witness.queue(), [PieceKind::O]);
        assert_eq!(witness.total_inputs(), 1);
        assert_eq!(witness.input_sequence(), [FinesseReportInput::HardDrop]);
        assert_eq!(witness.placements().len(), 1);
        assert_eq!(witness.placements()[0].piece(), PieceKind::O);
        assert_eq!(report.policy_results()[0].overall_average_inputs(), "1");
    }

    #[test]
    fn finesse_score_uses_the_precleared_initial_field_and_original_spawn_height() {
        let base_mask = 0x3ff_u64;
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, base_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&core).unwrap();
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [base_mask, 0, 0, 0], [0; 4])
                .unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            1,
        )])
        .unwrap();
        let query = BuildProbabilityQuery::new(core, field).with_finesse_score(score);
        assert_eq!(query.field().height(), 4);
        assert!(query.field().base().is_empty());
        assert_eq!(query.finesse_score().unwrap().initial_cleared_rows(), 1);

        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            query.field(),
            query.aggregation(),
            query.finesse_request().clone(),
        )
        .unwrap();
        let result = match session.advance(1, &ExecutionControl::default()).unwrap() {
            BuildProbabilityAdvance::Completed(result) => result,
            _ => panic!("score completes serially"),
        };

        assert_eq!(
            result.field("finesse_initial_board_words"),
            Some("0x0000000000000000000000000000000000000000000000000000000000000000")
        );
        assert_eq!(result.path_steps().len(), 1);
        assert_eq!(
            (result.path_steps()[0].x(), result.path_steps()[0].y()),
            (4, 0)
        );
    }

    #[test]
    fn cancelled_score_keeps_the_serial_request_pending() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4]).unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        )])
        .unwrap();
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::Oracle,
                request: score,
            },
        )
        .unwrap();
        let token = ExecutionCancellationToken::new();
        token.handle().cancel();
        let control = ExecutionControl::new(token);

        assert!(matches!(
            session.advance(1, &control).unwrap(),
            BuildProbabilityAdvance::Cancelled
        ));
        assert!(session.finesse_score.is_some());
        assert!(!session.finished);
    }

    #[test]
    fn score_with_no_successful_queue_keeps_a_report_but_no_path_artifact() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4]).unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        )])
        .unwrap();
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::Oracle,
                request: score,
            },
        )
        .unwrap();
        let result = match session.advance(1, &ExecutionControl::default()).unwrap() {
            BuildProbabilityAdvance::Completed(result) => result,
            _ => panic!("score completes serially"),
        };

        assert!(result.path_steps().is_empty());
        let report = result
            .finesse_report()
            .expect("typed failure report remains");
        assert_eq!(report.exact_total_inputs(), None);
        assert_eq!(report.representative_witness(), None);
        assert_eq!(
            report.policy_results()[0].successful_unique_queue_count(),
            Some(0)
        );
    }

    #[test]
    fn compact_and_extended_fixed_queue_searches_report_the_same_exact_cost() {
        let run = |height: u8, base_mask: u64| {
            let target_mask = (1_u64 << 4) | (1_u64 << 5) | (1_u64 << 14) | (1_u64 << 15);
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(u16::from(height), base_mask),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
                PieceWindow::new(1),
            )
            .with_exact_pieces(Some(1));
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let field = BuildProbabilityField::from_words(
                height,
                [base_mask, 0, 0, 0],
                [target_mask, 0, 0, 0],
            )
            .unwrap();
            assert_eq!(field.height(), height);
            let mut session = WasmBuildProbabilitySession::new(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Search {
                    pattern_knowledge: FinessePatternKnowledge::Oracle,
                },
            )
            .unwrap();
            let control = ExecutionControl::new(ExecutionCancellationToken::new());
            loop {
                match session.advance(1_024, &control).unwrap() {
                    BuildProbabilityAdvance::Pending => {}
                    BuildProbabilityAdvance::Completed(result) => break result,
                    BuildProbabilityAdvance::Cancelled => panic!("test search was not cancelled"),
                }
            }
        };
        let compact = run(6, 1_u64 << 50);
        let extended = run(7, 1_u64 << 60);

        assert_eq!(extended.field("board_height"), Some("7"));
        let exact_cost = |result: &CoreExecutionResult| {
            result
                .finesse_report()
                .and_then(FinesseReport::exact_total_inputs)
                .map(str::to_owned)
        };
        assert_eq!(exact_cost(&compact).as_deref(), Some("1"));
        assert_eq!(exact_cost(&extended), exact_cost(&compact));
    }

    #[test]
    fn finesse_search_preclears_initial_rows_in_compact_and_extended_fields() {
        let run = |height: u8| {
            let base_mask = 0x3ff_u64;
            let target_mask = (1_u64 << 14) | (1_u64 << 15) | (1_u64 << 24) | (1_u64 << 25);
            let core = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(u16::from(height), base_mask),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
                PieceWindow::new(1),
            )
            .with_allow_hold(false)
            .with_exact_pieces(Some(1));
            let problem = ProblemCompiler::compile_scenario_pc(&core).unwrap();
            let field = BuildProbabilityField::from_words_preserving_height(
                height,
                [base_mask, 0, 0, 0],
                [target_mask, 0, 0, 0],
            )
            .unwrap();
            let query = BuildProbabilityQuery::new(core, field)
                .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle);
            assert_eq!(query.field().height(), height);
            assert!(query.field().base().is_empty());
            assert_eq!(query.field().target_words(), [0xc030, 0, 0, 0]);

            let mut session = WasmBuildProbabilitySession::new(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
            )
            .unwrap();
            loop {
                match session
                    .advance(1_024, &ExecutionControl::default())
                    .unwrap()
                {
                    BuildProbabilityAdvance::Pending => {}
                    BuildProbabilityAdvance::Completed(result) => break result,
                    BuildProbabilityAdvance::Cancelled => panic!("test search was not cancelled"),
                }
            }
        };

        for height in [4, 7] {
            let result = run(height);
            if height <= 6 {
                assert_eq!(result.field("build_base_mask"), Some("0"));
                assert_eq!(result.field("build_target_cells_mask"), Some("49200"));
                assert_eq!(result.field("build_final_board_mask"), Some("49200"));
            } else {
                assert_eq!(result.field("build_base_mask"), Some("0x0"));
                assert_eq!(result.field("build_target_cells_mask"), Some("0xc030"));
                assert_eq!(result.field("build_final_board_mask"), Some("0xc030"));
                assert_eq!(result.field("board_storage"), Some("board256-canonical"));
            }
            let witness = result
                .finesse_report()
                .and_then(FinesseReport::representative_witness)
                .expect("fixed queue has an exact representative");
            assert_eq!(witness.total_inputs(), 1);
            assert_eq!(witness.placements().len(), 1);
            assert_eq!(
                (witness.placements()[0].x(), witness.placements()[0].y()),
                (4, 0)
            );
        }
    }
}
// SRP rationale: this module has one behavior-level change reason: exact pattern-specific build-probability evaluation.
