use std::{cell::RefCell, sync::Arc};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::{compile_setup_search_conditions, SetupSearchCondition, SetupSearchQuery};
use clearra_supply::pattern_universe::{PatternPiecePositionIndex, PieceMultisetKey};

use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    catalog::GeometryCatalog,
    geometry::{pack_piece_counts, GeometryFamilyCompileAdvance, GeometryFamilyCompileSession},
    pc4_tablebase::{
        loaded_pc4_compact_tablebase, pc4_tablebase_profile_identity, Pc4CompactTablebase,
    },
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::{
        CompletedSetupCoverage, SetupAdmissiblePrefixCompileAdvance,
        SetupAdmissiblePrefixCompileSession,
    },
    setup_partial_build::{PartialBuildAdvance, PartialBuildGraph, PartialBuildGraphBuilder},
    setup_suffix_coverage::{SetupSuffixCoverageAdvance, SetupSuffixCoverageSession},
    WasmExactSearchError,
};

// Completed graphs move directly into the consumer session.
#[allow(clippy::large_enum_variant)]
pub(super) enum SetupGraphBuildAdvance {
    Pending,
    Complete(SetupSharedGraph),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SetupGraphBuildProgress {
    pub(super) pass_index: usize,
    pub(super) pass_count: usize,
    pub(super) layer_index: usize,
    pub(super) layer_count: usize,
    pub(super) layer_done: usize,
    pub(super) layer_total: usize,
}

impl SetupGraphBuildProgress {
    const PASS_COUNT: usize = 4;

    const fn stage(pass_index: usize) -> Self {
        Self {
            pass_index,
            pass_count: Self::PASS_COUNT,
            layer_index: 0,
            layer_count: 0,
            layer_done: 0,
            layer_total: 0,
        }
    }

    const fn with_layer(
        mut self,
        layer_index: usize,
        layer_count: usize,
        layer_done: usize,
        layer_total: usize,
    ) -> Self {
        self.layer_index = layer_index;
        self.layer_count = layer_count;
        self.layer_done = layer_done;
        self.layer_total = layer_total;
        self
    }
}

pub(super) struct SetupSharedGraph {
    pub(super) query: SetupSearchQuery,
    pub(super) conditions: Vec<SetupSearchCondition>,
    pub(super) graph: Arc<PartialBuildGraph>,
    pub(super) coverage_graph: Arc<SetupCoverageGraph>,
    pub(super) geometry_family_count: String,
    pub(super) geometry_expanded_nodes: usize,
    pub(super) tablebase_status: &'static str,
    pub(super) tablebase_pruned_states: usize,
    pub(super) cached_coverage: Option<Arc<[CompletedSetupCoverage]>>,
    pub(super) condition_pattern_word_counts: Vec<usize>,
    pub(super) condition_pattern_indices: Vec<Arc<PatternPiecePositionIndex>>,
}

// The graph builder owns one stage at a time; inline storage avoids transition allocation.
#[allow(clippy::large_enum_variant)]
enum SetupGraphBuildStage {
    Conditions {
        cached: Option<CachedSetupGraph>,
    },
    CachedPrefixes {
        session: SetupAdmissiblePrefixCompileSession,
        cached: Option<CachedSetupGraph>,
    },
    Catalog,
    Prefixes(SetupAdmissiblePrefixCompileSession),
    Targets {
        admissible_prefixes: Vec<u32>,
        target_keys: Vec<PieceMultisetKey>,
        next_condition: usize,
    },
    Cached(Option<CachedSetupGraph>),
    Geometry(GeometryFamilyCompileSession),
    PartialBuild(PartialBuildGraphBuilder),
    SuffixCoverage {
        session: SetupSuffixCoverageSession,
        geometry_family_count: String,
        geometry_expanded_nodes: usize,
        tablebase_pruned_states: usize,
    },
    Finished,
}

pub(super) struct SetupGraphBuildSession {
    query: SetupSearchQuery,
    conditions: Vec<SetupSearchCondition>,
    condition_pattern_word_counts: Option<Vec<usize>>,
    condition_pattern_indices: Option<Vec<Arc<PatternPiecePositionIndex>>>,
    catalog: Option<Arc<GeometryCatalog>>,
    tablebase: Option<Arc<Pc4CompactTablebase>>,
    cached_detail_coverage: Option<Arc<[CompletedSetupCoverage]>>,
    tablebase_status: &'static str,
    parallel_task_count_hint: usize,
    retain_pattern_indices: bool,
    stage: SetupGraphBuildStage,
}

struct CachedSetupGraph {
    graph: Option<Arc<PartialBuildGraph>>,
    coverage_graph: Option<Arc<SetupCoverageGraph>>,
    compact_continuation: bool,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    tablebase_status: &'static str,
    tablebase_pruned_states: usize,
    coverage: Option<Arc<[CompletedSetupCoverage]>>,
}

struct SetupGraphCacheEntry {
    query: SetupSearchQuery,
    graph: Option<Arc<PartialBuildGraph>>,
    coverage_graph: Option<Arc<SetupCoverageGraph>>,
    compact_continuation: bool,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    tablebase_status: &'static str,
    tablebase_pruned_states: usize,
    coverage: Option<Arc<[CompletedSetupCoverage]>>,
}

thread_local! {
    static SETUP_GRAPH_CACHE: RefCell<Option<SetupGraphCacheEntry>> = const {
        RefCell::new(None)
    };
}

impl SetupGraphBuildSession {
    pub(super) fn new(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
        Self::new_internal(query, false)
    }

    pub(super) fn new_parallel(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
        Self::new_internal(query, true)
    }

    fn new_internal(
        query: &SetupSearchQuery,
        parallel: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let cached = cached_setup_graph(query);
        let parallel_task_count_hint = if cached.as_ref().is_some_and(|cached| {
            !cached.compact_continuation
                && cached_detail_candidate_exists(query, cached.coverage.as_deref())
        }) {
            1
        } else {
            2
        };
        Ok(Self {
            query: query.clone(),
            conditions: Vec::new(),
            condition_pattern_word_counts: None,
            condition_pattern_indices: None,
            catalog: None,
            tablebase: None,
            cached_detail_coverage: None,
            tablebase_status: "disabled",
            parallel_task_count_hint,
            retain_pattern_indices: !parallel || cfg!(not(target_family = "wasm")),
            stage: SetupGraphBuildStage::Conditions { cached },
        })
    }

    pub(super) const fn parallel_task_count_hint(&self) -> usize {
        self.parallel_task_count_hint
    }

    pub(super) fn condition_count(&self) -> usize {
        self.conditions.len()
    }

    pub(super) fn geometry_nodes(&self) -> usize {
        match &self.stage {
            SetupGraphBuildStage::Geometry(session) => session.progress_nodes(),
            SetupGraphBuildStage::PartialBuild(builder) => builder.geometry_expanded_nodes(),
            SetupGraphBuildStage::SuffixCoverage {
                geometry_expanded_nodes,
                ..
            } => *geometry_expanded_nodes,
            SetupGraphBuildStage::Cached(Some(cached)) => cached.geometry_expanded_nodes,
            SetupGraphBuildStage::Conditions { .. }
            | SetupGraphBuildStage::CachedPrefixes { .. }
            | SetupGraphBuildStage::Catalog
            | SetupGraphBuildStage::Prefixes(_)
            | SetupGraphBuildStage::Targets { .. }
            | SetupGraphBuildStage::Cached(None)
            | SetupGraphBuildStage::Finished => 0,
        }
    }

    pub(super) fn partial_build_nodes(&self) -> usize {
        match &self.stage {
            SetupGraphBuildStage::PartialBuild(builder) => builder.node_count(),
            SetupGraphBuildStage::SuffixCoverage { session, .. } => session.prefix_node_count(),
            SetupGraphBuildStage::Cached(Some(cached)) => {
                cached.graph.as_ref().map_or(0, |graph| graph.nodes.len())
            }
            SetupGraphBuildStage::Cached(None)
            | SetupGraphBuildStage::Conditions { .. }
            | SetupGraphBuildStage::CachedPrefixes { .. }
            | SetupGraphBuildStage::Catalog
            | SetupGraphBuildStage::Prefixes(_)
            | SetupGraphBuildStage::Targets { .. }
            | SetupGraphBuildStage::Geometry(_)
            | SetupGraphBuildStage::Finished => 0,
        }
    }

    pub(super) fn progress(&self) -> SetupGraphBuildProgress {
        match &self.stage {
            SetupGraphBuildStage::Conditions { .. } => {
                SetupGraphBuildProgress::stage(0).with_layer(0, 1, 0, 1)
            }
            SetupGraphBuildStage::Catalog => {
                SetupGraphBuildProgress::stage(0).with_layer(0, 1, 1, 1)
            }
            SetupGraphBuildStage::Prefixes(session)
            | SetupGraphBuildStage::CachedPrefixes { session, .. } => {
                let (layer_index, layer_count, layer_done, layer_total) = session.progress();
                SetupGraphBuildProgress::stage(0).with_layer(
                    layer_index,
                    layer_count,
                    layer_done,
                    layer_total,
                )
            }
            SetupGraphBuildStage::Targets { next_condition, .. } => {
                SetupGraphBuildProgress::stage(0).with_layer(
                    *next_condition,
                    self.conditions.len().max(1),
                    *next_condition,
                    self.conditions.len().max(1),
                )
            }
            SetupGraphBuildStage::Geometry(_) => SetupGraphBuildProgress::stage(0),
            SetupGraphBuildStage::PartialBuild(builder) => {
                let (layer_index, layer_count, layer_done, layer_total) =
                    builder.frontier_progress();
                SetupGraphBuildProgress::stage(1).with_layer(
                    layer_index,
                    layer_count,
                    layer_done,
                    layer_total,
                )
            }
            SetupGraphBuildStage::SuffixCoverage { session, .. } => {
                let (layer_index, layer_count, layer_done, layer_total) = session.layer_progress();
                SetupGraphBuildProgress::stage(2).with_layer(
                    layer_index,
                    layer_count,
                    layer_done,
                    layer_total,
                )
            }
            SetupGraphBuildStage::Cached(_) | SetupGraphBuildStage::Finished => {
                SetupGraphBuildProgress::stage(3)
            }
        }
    }

    pub(super) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<SetupGraphBuildAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            self.stage = SetupGraphBuildStage::Finished;
            return Ok(SetupGraphBuildAdvance::Cancelled);
        }
        let budget = work_budget.max(1);
        let stage = std::mem::replace(&mut self.stage, SetupGraphBuildStage::Finished);
        match stage {
            SetupGraphBuildStage::Conditions { cached } => {
                let mut conditions =
                    compile_setup_search_conditions(&self.query).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "setup_residue_condition_compile_failed",
                        )
                    })?;
                if let Some(detail) = self.query.path_detail() {
                    conditions
                        .retain(|condition| condition.condition_id() == detail.condition_id());
                    if conditions.is_empty() {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "setup_path_detail_condition_not_found",
                        ));
                    }
                }
                let first = conditions
                    .first()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_residue_has_no_hold_condition",
                    ))?;
                super::ensure_connected_kick_profile(first.problem())?;
                self.conditions = conditions;
                if let Some(cached) = cached {
                    if !cached.compact_continuation {
                        if cached_detail_candidate_exists(&self.query, cached.coverage.as_deref()) {
                            self.condition_pattern_word_counts =
                                Some(vec![0; self.conditions.len()]);
                            self.condition_pattern_indices = Some(Vec::new());
                            self.stage = SetupGraphBuildStage::Cached(Some(cached));
                        } else {
                            self.stage = SetupGraphBuildStage::CachedPrefixes {
                                session:
                                    SetupAdmissiblePrefixCompileSession::new_with_retained_indices(
                                        &self.conditions,
                                        self.retain_pattern_indices,
                                    )?,
                                cached: Some(cached),
                            };
                        }
                        return Ok(SetupGraphBuildAdvance::Pending);
                    }
                    self.cached_detail_coverage = cached.coverage;
                }
                self.stage = SetupGraphBuildStage::Catalog;
                Ok(SetupGraphBuildAdvance::Pending)
            }
            SetupGraphBuildStage::CachedPrefixes {
                mut session,
                mut cached,
            } => match session.advance(budget, control)? {
                SetupAdmissiblePrefixCompileAdvance::Pending => {
                    self.stage = SetupGraphBuildStage::CachedPrefixes { session, cached };
                    Ok(SetupGraphBuildAdvance::Pending)
                }
                SetupAdmissiblePrefixCompileAdvance::Cancelled => {
                    Ok(SetupGraphBuildAdvance::Cancelled)
                }
                SetupAdmissiblePrefixCompileAdvance::Complete {
                    word_counts,
                    pattern_indices,
                    ..
                } => {
                    self.condition_pattern_word_counts = Some(word_counts);
                    self.condition_pattern_indices = Some(pattern_indices);
                    self.stage = SetupGraphBuildStage::Cached(cached.take());
                    Ok(SetupGraphBuildAdvance::Pending)
                }
            },
            SetupGraphBuildStage::Catalog => {
                let first = self
                    .conditions
                    .first()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_residue_has_no_hold_condition",
                    ))?;
                let catalog = Arc::new(GeometryCatalog::compile(first.problem())?);
                if catalog.width() != 10 || catalog.height() != 4 || catalog.initial_board() != 0 {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "setup_finder_requires_empty_10x4_target",
                    ));
                }
                let loaded_tablebase = self
                    .query
                    .tablebase_requested()
                    .then(loaded_pc4_compact_tablebase)
                    .flatten();
                let expected_tablebase_profile =
                    pc4_tablebase_profile_identity(first.problem(), catalog.identity_digest());
                let (tablebase, tablebase_status) = select_setup_tablebase(
                    self.query.tablebase_requested(),
                    loaded_tablebase,
                    catalog.identity_digest(),
                    expected_tablebase_profile,
                );
                self.tablebase = tablebase;
                self.tablebase_status = tablebase_status;
                self.catalog = Some(catalog);
                self.stage = SetupGraphBuildStage::Prefixes(
                    SetupAdmissiblePrefixCompileSession::new_with_retained_indices(
                        &self.conditions,
                        self.retain_pattern_indices,
                    )?,
                );
                Ok(SetupGraphBuildAdvance::Pending)
            }
            SetupGraphBuildStage::Prefixes(mut session) => {
                let span = SearchStageSpan::begin(ExecutorSearchStage::WasmSetupGeometryCompile);
                let advance = session.advance(budget, control);
                span.finish(budget as u64);
                match advance? {
                    SetupAdmissiblePrefixCompileAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::Prefixes(session);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    SetupAdmissiblePrefixCompileAdvance::Cancelled => {
                        Ok(SetupGraphBuildAdvance::Cancelled)
                    }
                    SetupAdmissiblePrefixCompileAdvance::Complete {
                        prefixes,
                        word_counts,
                        pattern_indices,
                    } => {
                        self.condition_pattern_word_counts = Some(word_counts);
                        self.condition_pattern_indices = Some(pattern_indices);
                        self.stage = SetupGraphBuildStage::Targets {
                            admissible_prefixes: prefixes,
                            target_keys: Vec::new(),
                            next_condition: 0,
                        };
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                }
            }
            SetupGraphBuildStage::Targets {
                mut admissible_prefixes,
                mut target_keys,
                mut next_condition,
            } => {
                if let Some(condition) = self.conditions.get(next_condition) {
                    let problem = condition.problem();
                    let universe = problem.piece_source().materialized_universe().ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "setup_pattern_universe_not_materialized",
                        ),
                    )?;
                    let family = universe.packing_multiset_family_for_execution(
                        10,
                        problem.initial_hold(),
                        problem.supply().hold_enabled(),
                        super::packing_hold_projection(problem),
                    );
                    target_keys.extend(family.groups().iter().map(|group| group.key()));
                    next_condition += 1;
                    self.stage = SetupGraphBuildStage::Targets {
                        admissible_prefixes,
                        target_keys,
                        next_condition,
                    };
                    return Ok(SetupGraphBuildAdvance::Pending);
                }
                if self
                    .conditions
                    .iter()
                    .any(|condition| condition.terminal_supply_target().is_some())
                {
                    target_keys.retain(|target| {
                        admissible_prefixes
                            .binary_search(&pack_piece_counts(target.counts()))
                            .is_ok()
                    });
                }
                admissible_prefixes.extend(
                    target_keys
                        .iter()
                        .map(|target| pack_piece_counts(target.counts())),
                );
                let catalog = self
                    .catalog
                    .as_ref()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_geometry_catalog_missing",
                    ))?;
                let geometry = GeometryFamilyCompileSession::new_with_tablebase(
                    catalog.required_cells(),
                    target_keys,
                    admissible_prefixes,
                    self.tablebase.take(),
                )?;
                self.stage = SetupGraphBuildStage::Geometry(geometry);
                Ok(SetupGraphBuildAdvance::Pending)
            }
            SetupGraphBuildStage::Cached(mut cached) => {
                let cached = cached.take().ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_cached_graph_already_consumed",
                ))?;
                let condition_pattern_word_counts =
                    self.condition_pattern_word_counts.take().ok_or(
                        WasmExactSearchError::InvalidProblem("setup_pattern_word_counts_missing"),
                    )?;
                let condition_pattern_indices = self.condition_pattern_indices.take().ok_or(
                    WasmExactSearchError::InvalidProblem("setup_pattern_indices_missing"),
                )?;
                Ok(SetupGraphBuildAdvance::Complete(SetupSharedGraph {
                    query: self.query.clone(),
                    conditions: std::mem::take(&mut self.conditions),
                    graph: cached.graph.ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_cached_graph_missing",
                    ))?,
                    coverage_graph: cached.coverage_graph.ok_or(
                        WasmExactSearchError::InvalidProblem("setup_cached_coverage_graph_missing"),
                    )?,
                    geometry_family_count: cached.geometry_family_count,
                    geometry_expanded_nodes: cached.geometry_expanded_nodes,
                    tablebase_status: cached.tablebase_status,
                    tablebase_pruned_states: cached.tablebase_pruned_states,
                    cached_coverage: cached.coverage,
                    condition_pattern_word_counts,
                    condition_pattern_indices,
                }))
            }
            SetupGraphBuildStage::Geometry(mut session) => {
                let catalog = self
                    .catalog
                    .as_ref()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_geometry_catalog_missing",
                    ))?;
                let span = SearchStageSpan::begin(ExecutorSearchStage::WasmSetupGeometryCompile);
                let advance = session.advance(catalog, budget, control);
                span.finish(budget as u64);
                match advance {
                    GeometryFamilyCompileAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::Geometry(session);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    GeometryFamilyCompileAdvance::Cancelled => {
                        Ok(SetupGraphBuildAdvance::Cancelled)
                    }
                    GeometryFamilyCompileAdvance::ResourceIncomplete(reason) => {
                        Err(WasmExactSearchError::InvalidProblem(reason))
                    }
                    GeometryFamilyCompileAdvance::Complete(compiled) => {
                        let builder = if let Some(detail) = self.query.path_detail() {
                            PartialBuildGraphBuilder::new_selected_detail(
                                compiled,
                                catalog,
                                self.conditions[0].problem(),
                                detail,
                            )?
                        } else {
                            PartialBuildGraphBuilder::new_candidate_prefix(
                                compiled,
                                catalog,
                                self.conditions[0].problem(),
                                self.query.max_setup_pieces(),
                            )?
                        };
                        self.stage = SetupGraphBuildStage::PartialBuild(builder);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                }
            }
            SetupGraphBuildStage::PartialBuild(mut builder) => {
                let catalog = self
                    .catalog
                    .as_ref()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_geometry_catalog_missing",
                    ))?;
                let span = SearchStageSpan::begin(ExecutorSearchStage::WasmSetupPartialBuild);
                let advance = builder.advance(catalog, budget, control);
                span.finish(budget as u64);
                match advance? {
                    PartialBuildAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::PartialBuild(builder);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    PartialBuildAdvance::Cancelled => Ok(SetupGraphBuildAdvance::Cancelled),
                    PartialBuildAdvance::PrefixComplete {
                        prefix,
                        geometry_family_count,
                        geometry_expanded_nodes,
                        tablebase_pruned_states,
                    } => {
                        let session = SetupSuffixCoverageSession::new(
                            prefix,
                            Arc::clone(catalog),
                            self.conditions[0].problem(),
                        )?;
                        self.stage = SetupGraphBuildStage::SuffixCoverage {
                            session,
                            geometry_family_count,
                            geometry_expanded_nodes,
                            tablebase_pruned_states,
                        };
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    PartialBuildAdvance::Complete {
                        graph,
                        geometry_family_count,
                        geometry_expanded_nodes,
                        tablebase_pruned_states,
                    } => {
                        let graph = Arc::new(graph);
                        let span = SearchStageSpan::begin(
                            ExecutorSearchStage::WasmSetupCoverageGraphCompile,
                        );
                        let coverage_graph = SetupCoverageGraph::compile(&graph);
                        span.finish(graph.nodes.len() as u64);
                        let coverage_graph = Arc::new(coverage_graph?);
                        let shared = SetupSharedGraph {
                            query: self.query.clone(),
                            conditions: std::mem::take(&mut self.conditions),
                            graph,
                            coverage_graph,
                            geometry_family_count,
                            geometry_expanded_nodes,
                            tablebase_status: self.tablebase_status,
                            tablebase_pruned_states,
                            cached_coverage: self.cached_detail_coverage.take(),
                            condition_pattern_word_counts: self
                                .condition_pattern_word_counts
                                .take()
                                .ok_or(WasmExactSearchError::InvalidProblem(
                                    "setup_pattern_word_counts_missing",
                                ))?,
                            condition_pattern_indices: self
                                .condition_pattern_indices
                                .take()
                                .ok_or(WasmExactSearchError::InvalidProblem(
                                    "setup_pattern_indices_missing",
                                ))?,
                        };
                        cache_setup_graph(&shared);
                        Ok(SetupGraphBuildAdvance::Complete(shared))
                    }
                }
            }
            SetupGraphBuildStage::SuffixCoverage {
                mut session,
                geometry_family_count,
                geometry_expanded_nodes,
                tablebase_pruned_states,
            } => {
                let span =
                    SearchStageSpan::begin(ExecutorSearchStage::WasmSetupCoverageGraphCompile);
                let expanded_before = session.expanded_states();
                let advance = session.advance(budget, control);
                span.finish(session.expanded_states().saturating_sub(expanded_before) as u64);
                match advance? {
                    SetupSuffixCoverageAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::SuffixCoverage {
                            session,
                            geometry_family_count,
                            geometry_expanded_nodes,
                            tablebase_pruned_states,
                        };
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    SetupSuffixCoverageAdvance::Cancelled => Ok(SetupGraphBuildAdvance::Cancelled),
                    SetupSuffixCoverageAdvance::Complete {
                        graph,
                        coverage_graph,
                    } => {
                        let graph = Arc::new(graph);
                        let coverage_graph = Arc::new(coverage_graph);
                        let shared = SetupSharedGraph {
                            query: self.query.clone(),
                            conditions: std::mem::take(&mut self.conditions),
                            graph,
                            coverage_graph,
                            geometry_family_count,
                            geometry_expanded_nodes,
                            tablebase_status: self.tablebase_status,
                            tablebase_pruned_states,
                            cached_coverage: None,
                            condition_pattern_word_counts: self
                                .condition_pattern_word_counts
                                .take()
                                .ok_or(WasmExactSearchError::InvalidProblem(
                                    "setup_pattern_word_counts_missing",
                                ))?,
                            condition_pattern_indices: self
                                .condition_pattern_indices
                                .take()
                                .ok_or(WasmExactSearchError::InvalidProblem(
                                    "setup_pattern_indices_missing",
                                ))?,
                        };
                        cache_setup_graph(&shared);
                        Ok(SetupGraphBuildAdvance::Complete(shared))
                    }
                }
            }
            SetupGraphBuildStage::Finished => Err(WasmExactSearchError::InvalidProblem(
                "setup_graph_build_session_already_finished",
            )),
        }
    }
}

fn select_setup_tablebase(
    requested: bool,
    loaded: Option<Arc<Pc4CompactTablebase>>,
    catalog_identity: u64,
    compiler_identity: u64,
) -> (Option<Arc<Pc4CompactTablebase>>, &'static str) {
    match loaded {
        None if requested => (None, "unavailable"),
        None => (None, "disabled"),
        Some(_) if !requested => (None, "disabled"),
        Some(loaded)
            if loaded.catalog_identity() != catalog_identity
                || loaded.compiler_identity() != compiler_identity =>
        {
            (None, "profile-mismatch")
        }
        Some(loaded) => (Some(loaded), "connected-exact-dead-index"),
    }
}

fn cached_setup_graph(query: &SetupSearchQuery) -> Option<CachedSetupGraph> {
    if query.path_detail().is_none() {
        SETUP_GRAPH_CACHE.with(|cache| *cache.borrow_mut() = None);
        return None;
    }
    let identity = query.clone().without_path_detail();
    SETUP_GRAPH_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let entry = cache.as_ref()?;
        if entry.query != identity {
            *cache = None;
            return None;
        }
        Some(CachedSetupGraph {
            graph: entry.graph.as_ref().map(Arc::clone),
            coverage_graph: entry.coverage_graph.as_ref().map(Arc::clone),
            compact_continuation: entry.compact_continuation,
            geometry_family_count: entry.geometry_family_count.clone(),
            geometry_expanded_nodes: entry.geometry_expanded_nodes,
            tablebase_status: entry.tablebase_status,
            tablebase_pruned_states: entry.tablebase_pruned_states,
            coverage: entry.coverage.as_ref().map(Arc::clone),
        })
    })
}

fn cache_setup_graph(shared: &SetupSharedGraph) {
    if shared.query.path_detail().is_some() || shared.graph.resource_truncated {
        return;
    }
    SETUP_GRAPH_CACHE.with(|cache| {
        let compact_continuation = shared.graph.uses_compact_continuation();
        *cache.borrow_mut() = Some(SetupGraphCacheEntry {
            query: shared.query.clone().without_path_detail(),
            graph: (!compact_continuation).then(|| Arc::clone(&shared.graph)),
            coverage_graph: (!compact_continuation).then(|| Arc::clone(&shared.coverage_graph)),
            compact_continuation,
            geometry_family_count: shared.geometry_family_count.clone(),
            geometry_expanded_nodes: shared.geometry_expanded_nodes,
            tablebase_status: shared.tablebase_status,
            tablebase_pruned_states: shared.tablebase_pruned_states,
            coverage: None,
        });
    });
}

pub(super) fn cache_setup_coverage_result(
    query: &SetupSearchQuery,
    completed: &[CompletedSetupCoverage],
) {
    if query.path_detail().is_some() {
        return;
    }
    let identity = query.clone().without_path_detail();
    SETUP_GRAPH_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let Some(entry) = cache.as_mut() else {
            return;
        };
        if entry.query == identity {
            entry.coverage = Some(Arc::from(completed.to_vec()));
        }
    });
}

#[cfg(test)]
pub(super) fn install_setup_graph_cache_for_test(
    query: &SetupSearchQuery,
    graph: Arc<PartialBuildGraph>,
    coverage_graph: Arc<SetupCoverageGraph>,
    coverage: Vec<CompletedSetupCoverage>,
) {
    let shared = SetupSharedGraph {
        query: query.clone().without_path_detail(),
        conditions: Vec::new(),
        graph,
        coverage_graph,
        geometry_family_count: "1".to_owned(),
        geometry_expanded_nodes: 1,
        tablebase_status: "disabled",
        tablebase_pruned_states: 0,
        cached_coverage: None,
        condition_pattern_word_counts: Vec::new(),
        condition_pattern_indices: Vec::new(),
    };
    cache_setup_graph(&shared);
    cache_setup_coverage_result(&shared.query, &coverage);
}

#[cfg(test)]
pub(super) fn clear_setup_graph_cache_for_test() {
    SETUP_GRAPH_CACHE.with(|cache| *cache.borrow_mut() = None);
}

fn cached_detail_candidate_exists(
    query: &SetupSearchQuery,
    coverage: Option<&[CompletedSetupCoverage]>,
) -> bool {
    let Some(detail) = query.path_detail() else {
        return false;
    };
    let setup_id = detail.setup_id();
    coverage.is_some_and(|coverage| {
        coverage.iter().any(|completed| {
            completed.report.condition_id() == detail.condition_id()
                && completed
                    .report
                    .candidates()
                    .iter()
                    .any(|candidate| candidate.setup_id() == setup_id)
        })
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::select_setup_tablebase;
    use crate::backend::wasm_cpu::pc4_tablebase::Pc4CompactTablebase;

    const PRODUCT_ARTIFACT: &[u8] = include_bytes!(
        "../../../../../apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin"
    );

    #[test]
    fn setup_tablebase_selection_is_opt_in_and_profile_exact() {
        let tablebase =
            Arc::new(Pc4CompactTablebase::from_bytes(PRODUCT_ARTIFACT).expect("product tablebase"));
        let catalog_identity = tablebase.catalog_identity();
        let compiler_identity = tablebase.compiler_identity();

        let (selected, status) = select_setup_tablebase(
            false,
            Some(Arc::clone(&tablebase)),
            catalog_identity,
            compiler_identity,
        );
        assert!(selected.is_none());
        assert_eq!(status, "disabled");

        let (selected, status) =
            select_setup_tablebase(true, None, catalog_identity, compiler_identity);
        assert!(selected.is_none());
        assert_eq!(status, "unavailable");

        let (selected, status) = select_setup_tablebase(
            true,
            Some(Arc::clone(&tablebase)),
            catalog_identity ^ 1,
            compiler_identity,
        );
        assert!(selected.is_none());
        assert_eq!(status, "profile-mismatch");

        let (selected, status) =
            select_setup_tablebase(true, Some(tablebase), catalog_identity, compiler_identity);
        assert!(selected.is_some());
        assert_eq!(status, "connected-exact-dead-index");
    }
}
