use std::{cell::RefCell, sync::Arc};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::{compile_setup_search_conditions, SetupSearchCondition, SetupSearchQuery};
use clearra_supply::pattern_universe::PieceMultisetKey;

use super::{
    catalog::GeometryCatalog,
    geometry::{pack_piece_counts, GeometryFamilyCompileAdvance, GeometryFamilyCompileSession},
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::{
        compile_setup_admissible_prefixes, compile_setup_pattern_index, CompletedSetupCoverage,
    },
    setup_partial_build::{PartialBuildAdvance, PartialBuildGraph, PartialBuildGraphBuilder},
    WasmExactSearchError,
};

pub(super) enum SetupGraphBuildAdvance {
    Pending,
    Complete(SetupSharedGraph),
    Cancelled,
}

pub(super) struct SetupSharedGraph {
    pub(super) query: SetupSearchQuery,
    pub(super) conditions: Vec<SetupSearchCondition>,
    pub(super) graph: Arc<PartialBuildGraph>,
    pub(super) coverage_graph: Arc<SetupCoverageGraph>,
    pub(super) geometry_family_count: String,
    pub(super) geometry_expanded_nodes: usize,
    pub(super) cached_coverage: Option<Arc<[CompletedSetupCoverage]>>,
}

enum SetupGraphBuildStage {
    Cached(Option<CachedSetupGraph>),
    Geometry(GeometryFamilyCompileSession),
    PartialBuild(PartialBuildGraphBuilder),
    Finished,
}

pub(super) struct SetupGraphBuildSession {
    query: SetupSearchQuery,
    conditions: Vec<SetupSearchCondition>,
    catalog: Option<Arc<GeometryCatalog>>,
    stage: SetupGraphBuildStage,
}

struct CachedSetupGraph {
    graph: Arc<PartialBuildGraph>,
    coverage_graph: Arc<SetupCoverageGraph>,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    coverage: Option<Arc<[CompletedSetupCoverage]>>,
}

struct SetupGraphCacheEntry {
    query: SetupSearchQuery,
    graph: Arc<PartialBuildGraph>,
    coverage_graph: Arc<SetupCoverageGraph>,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    coverage: Option<Arc<[CompletedSetupCoverage]>>,
}

thread_local! {
    static SETUP_GRAPH_CACHE: RefCell<Option<SetupGraphCacheEntry>> = const {
        RefCell::new(None)
    };
}

impl SetupGraphBuildSession {
    pub(super) fn new(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
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
        if let Some(cached) = cached_setup_graph(query) {
            return Ok(Self {
                query: query.clone(),
                conditions,
                catalog: None,
                stage: SetupGraphBuildStage::Cached(Some(cached)),
            });
        }
        let catalog = Arc::new(GeometryCatalog::compile(first.problem())?);
        if catalog.width() != 10 || catalog.height() != 4 || catalog.initial_board() != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_finder_requires_empty_10x4_target",
            ));
        }

        let mut admissible_prefixes = compile_setup_admissible_prefixes(&conditions)?;
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
        let geometry = GeometryFamilyCompileSession::new(
            catalog.required_cells(),
            target_keys,
            admissible_prefixes,
        )?;
        Ok(Self {
            query: query.clone(),
            conditions,
            catalog: Some(catalog),
            stage: SetupGraphBuildStage::Geometry(geometry),
        })
    }

    pub(super) fn condition_count(&self) -> usize {
        self.conditions.len()
    }

    pub(super) fn condition_pattern_word_counts(&self) -> Result<Vec<usize>, WasmExactSearchError> {
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
                    graph: cached.graph,
                    coverage_graph: cached.coverage_graph,
                    geometry_family_count: cached.geometry_family_count,
                    geometry_expanded_nodes: cached.geometry_expanded_nodes,
                    cached_coverage: cached.coverage,
                }))
            }
            SetupGraphBuildStage::Geometry(mut geometry) => {
                let catalog = self
                    .catalog
                    .as_ref()
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_geometry_catalog_missing",
                    ))?;
                match geometry.advance(catalog, budget, control) {
                    GeometryFamilyCompileAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::Geometry(geometry);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    GeometryFamilyCompileAdvance::Cancelled => {
                        Ok(SetupGraphBuildAdvance::Cancelled)
                    }
                    GeometryFamilyCompileAdvance::ResourceIncomplete(reason) => {
                        Err(WasmExactSearchError::InvalidProblem(reason))
                    }
                    GeometryFamilyCompileAdvance::Complete(compiled) => {
                        let builder = PartialBuildGraphBuilder::new(
                            compiled,
                            catalog,
                            self.conditions[0].problem(),
                        )?;
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
                match builder.advance(catalog, budget, control)? {
                    PartialBuildAdvance::Pending => {
                        self.stage = SetupGraphBuildStage::PartialBuild(builder);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                    PartialBuildAdvance::Cancelled => Ok(SetupGraphBuildAdvance::Cancelled),
                    PartialBuildAdvance::Complete {
                        graph,
                        geometry_family_count,
                        geometry_expanded_nodes,
                    } => {
                        let graph = Arc::new(graph);
                        let coverage_graph = Arc::new(SetupCoverageGraph::compile(&graph)?);
                        let shared = SetupSharedGraph {
                            query: self.query.clone(),
                            conditions: std::mem::take(&mut self.conditions),
                            graph,
                            coverage_graph,
                            geometry_family_count,
                            geometry_expanded_nodes,
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
            graph: Arc::clone(&entry.graph),
            coverage_graph: Arc::clone(&entry.coverage_graph),
            geometry_family_count: entry.geometry_family_count.clone(),
            geometry_expanded_nodes: entry.geometry_expanded_nodes,
            coverage: entry.coverage.as_ref().map(Arc::clone),
        })
    })
}

fn cache_setup_graph(shared: &SetupSharedGraph) {
    if shared.query.path_detail().is_some() || shared.graph.resource_truncated {
        return;
    }
    SETUP_GRAPH_CACHE.with(|cache| {
        *cache.borrow_mut() = Some(SetupGraphCacheEntry {
            query: shared.query.clone().without_path_detail(),
            graph: Arc::clone(&shared.graph),
            coverage_graph: Arc::clone(&shared.coverage_graph),
            geometry_family_count: shared.geometry_family_count.clone(),
            geometry_expanded_nodes: shared.geometry_expanded_nodes,
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
