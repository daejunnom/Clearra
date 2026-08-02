use std::{cell::RefCell, sync::Arc};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::{compile_setup_search_conditions, SetupSearchCondition, SetupSearchQuery};
use clearra_supply::pattern_universe::PieceMultisetKey;

use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    catalog::GeometryCatalog,
    geometry::{pack_piece_counts, GeometryFamilyCompileAdvance, GeometryFamilyCompileSession},
    pc4_tablebase::{
        loaded_pc4_compact_tablebase, pc4_tablebase_profile_identity, Pc4CompactTablebase,
    },
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::{
        compile_setup_admissible_prefixes_with_word_counts, compile_setup_pattern_index,
        CompletedSetupCoverage,
    },
    setup_partial_build::{PartialBuildAdvance, PartialBuildGraph, PartialBuildGraphBuilder},
    setup_suffix_coverage::{SetupSuffixCoverageAdvance, SetupSuffixCoverageSession},
    WasmExactSearchError,
};

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
}

enum SetupGraphBuildStage {
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
    catalog: Option<Arc<GeometryCatalog>>,
    cached_detail_coverage: Option<Arc<[CompletedSetupCoverage]>>,
    tablebase_status: &'static str,
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
        Self::new_internal(query)
    }

    pub(super) fn new_parallel(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
        Self::new_internal(query)
    }

    fn new_internal(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
        let mut conditions = compile_setup_search_conditions(query).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_residue_condition_compile_failed")
        })?;
        if let Some(detail) = query.path_detail() {
            conditions.retain(|condition| condition.condition_id() == detail.condition_id());
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
        let cached = cached_setup_graph(query);
        if let Some(cached) = cached {
            if !cached.compact_continuation {
                let graph = cached.graph.ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_cached_graph_missing",
                ))?;
                let coverage_graph =
                    cached
                        .coverage_graph
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "setup_cached_coverage_graph_missing",
                        ))?;
                let tablebase_status = cached.tablebase_status;
                return Ok(Self {
                    query: query.clone(),
                    conditions,
                    condition_pattern_word_counts: None,
                    catalog: None,
                    cached_detail_coverage: None,
                    tablebase_status,
                    stage: SetupGraphBuildStage::Cached(Some(CachedSetupGraph {
                        graph: Some(graph),
                        coverage_graph: Some(coverage_graph),
                        compact_continuation: false,
                        geometry_family_count: cached.geometry_family_count,
                        geometry_expanded_nodes: cached.geometry_expanded_nodes,
                        tablebase_status: cached.tablebase_status,
                        tablebase_pruned_states: cached.tablebase_pruned_states,
                        coverage: cached.coverage,
                    })),
                });
            }
            let cached_detail_coverage = cached.coverage;
            return Self::new_uncached(query, conditions, cached_detail_coverage);
        }
        Self::new_uncached(query, conditions, None)
    }

    fn new_uncached(
        query: &SetupSearchQuery,
        conditions: Vec<SetupSearchCondition>,
        cached_detail_coverage: Option<Arc<[CompletedSetupCoverage]>>,
    ) -> Result<Self, WasmExactSearchError> {
        let first = conditions
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
        let loaded_tablebase = query
            .tablebase_requested()
            .then(loaded_pc4_compact_tablebase)
            .flatten();
        let expected_tablebase_profile =
            pc4_tablebase_profile_identity(first.problem(), catalog.identity_digest());
        let (tablebase, tablebase_status) = select_setup_tablebase(
            query.tablebase_requested(),
            loaded_tablebase,
            catalog.identity_digest(),
            expected_tablebase_profile,
        );

        let (mut admissible_prefixes, condition_pattern_word_counts) =
            compile_setup_admissible_prefixes_with_word_counts(&conditions)?;
        let mut target_keys = Vec::<PieceMultisetKey>::new();
        for condition in &conditions {
            let problem = condition.problem();
            let universe = problem.piece_source().materialized_universe().ok_or(
                WasmExactSearchError::InvalidProblem("setup_pattern_universe_not_materialized"),
            )?;
            let family = universe.packing_multiset_family(
                10,
                problem.initial_hold(),
                super::packing_projection_hold_enabled(problem),
            );
            target_keys.extend(family.groups().iter().map(|group| group.key()));
        }
        if conditions
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
        let geometry = GeometryFamilyCompileSession::new_with_tablebase(
            catalog.required_cells(),
            target_keys,
            admissible_prefixes,
            tablebase,
        )?;
        Ok(Self {
            query: query.clone(),
            conditions,
            condition_pattern_word_counts: Some(condition_pattern_word_counts),
            catalog: Some(catalog),
            cached_detail_coverage,
            tablebase_status,
            stage: SetupGraphBuildStage::Geometry(geometry),
        })
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
            SetupGraphBuildStage::Cached(None) | SetupGraphBuildStage::Finished => 0,
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
            | SetupGraphBuildStage::Geometry(_)
            | SetupGraphBuildStage::Finished => 0,
        }
    }

    pub(super) fn progress(&self) -> SetupGraphBuildProgress {
        match &self.stage {
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

    pub(super) fn condition_pattern_word_counts(&self) -> Result<Vec<usize>, WasmExactSearchError> {
        if let Some(word_counts) = self.condition_pattern_word_counts.as_ref() {
            return Ok(word_counts.clone());
        }
        if let SetupGraphBuildStage::Cached(Some(cached)) = &self.stage {
            if cached_detail_candidate_exists(&self.query, cached.coverage.as_deref()) {
                return Ok(vec![0; self.conditions.len()]);
            }
        }
        self.conditions
            .iter()
            .map(|condition| compile_setup_pattern_index(condition).map(|index| index.word_count()))
            .collect()
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
            SetupGraphBuildStage::Cached(mut cached) => {
                let cached = cached.take().ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_cached_graph_already_consumed",
                ))?;
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
        let Some(entry) = cache.as_ref() else {
            return None;
        };
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
