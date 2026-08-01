use std::collections::{BTreeMap, VecDeque};

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_key_set_hash_from_sorted_strings,
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    },
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};
use clearra_replay::ExactScoringExecutionBatch;
use clearra_supply::{
    hold_automaton::HoldAutomatonState, pattern_universe::PackingPatternMembershipKind,
};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    CoreExecutionResult, CorePathStep, NormalizedSolutionCoverage, SolutionCoverage,
    TilingSolutionPageStore,
};

use super::{
    buildup::{
        exact_scoring_execution_graph_for_completion, verify_candidate_for_completion,
        BuildCompletion, BuildUpWorkspace, CandidateWitnessMode,
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
    mirror_included: bool,
    mirror_distinct: bool,
    execution_constraints_requested: bool,
    finished: bool,
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
    ) -> Result<Self, WasmExactSearchError> {
        let mirror_included = field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let mut pending = VecDeque::with_capacity(usize::from(mirror_distinct) + 1);
        pending.push_back(build_probability_session_for_field(
            problem,
            original,
            aggregation,
        )?);
        if let Some(mirrored) = mirrored.filter(|candidate| *candidate != original) {
            pending.push_back(build_probability_session_for_field(
                problem,
                mirrored,
                aggregation,
            )?);
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
                self.completed.push(result);
                self.pending.pop_front();
                if !self.pending.is_empty() {
                    return Ok(BuildProbabilityAdvance::Pending);
                }
                self.finished = true;
                Ok(BuildProbabilityAdvance::Completed(merge_symmetry_results(
                    core::mem::take(&mut self.completed),
                    self.mirror_included,
                    self.mirror_distinct,
                    &self.pattern_weights,
                    self.aggregation.requests_spin_coverage()
                        || self.execution_constraints_requested,
                )?))
            }
        }
    }
}

fn build_probability_session_for_field(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
) -> Result<BuildProbabilitySessionKind, WasmExactSearchError> {
    if field.is_compact() {
        Ok(BuildProbabilitySessionKind::Compact(
            CompactBuildProbabilitySession::new(problem, field, aggregation)?,
        ))
    } else {
        Ok(BuildProbabilitySessionKind::Extended(
            super::extended_build_probability::ExtendedBuildProbabilitySession::new(
                problem,
                field,
                aggregation,
            )?,
        ))
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
        Self::new_with_external_geometry(problem, field, aggregation, false, None)
    }

    pub(super) fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(problem, field, aggregation, true, None)
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
        let shared_key = CompactBuildProbabilitySharedCatalogKey {
            piece_source_id: problem.piece_source().id().get(),
            pattern_universe_id: universe.pattern_universe_id().get(),
            pattern_count: universe.pattern_count(),
            target_piece_count,
            initial_hold: problem.initial_hold(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            compile_pattern_indexes: !symbolic,
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
                    targets: SharedTargetGroups::compile(universe, &family, !symbolic)?,
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
            return Ok(());
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
        let result = verify_candidate_for_completion(
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
        )?;

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
            if retain_solution_coverage {
                let candidate_coverage =
                    candidate_coverage
                        .as_ref()
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_solution_coverage_missing",
                        ))?;
                self.merge_solution_coverage(candidate.identity, candidate_coverage)?;
            }
            self.buildable_tilings.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_storage_unavailable",
                )
            })?;
            self.buildable_tilings.insert(candidate.identity);
            self.build_variant_count = self
                .build_variant_count
                .checked_add(result.build_variant_count)
                .unwrap_or_else(|| {
                    self.count_complete = false;
                    u128::MAX
                });
            self.count_complete &= result.count_complete;
            let rank = external_ordinal.unwrap_or((self.candidate_count - 1) as u64);
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
        Ok(self.build_result(scoring_batch))
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
            self.build_result(scoring_batch),
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
    ) -> CoreExecutionResult {
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
        let fields = vec![
            field(
                "backend_requested",
                self.problem.backend_policy().requested_backend().as_str(),
            ),
            field("backend_selected", "wasm-cpu-build-probability"),
            field("actual_backend", "wasm-cpu-build-probability"),
            field("backend_fallback_used", false),
            field("backend_fallback_reason", "none"),
            field("fallback_backend", "none"),
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
        if execution_constraints.requested() {
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
        }
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
// SRP rationale: this module has one behavior-level change reason: exact pattern-specific build-probability evaluation.
