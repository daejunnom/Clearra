use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::{compile_setup_search_conditions, SetupSearchCondition, SetupSearchQuery};
use clearra_supply::pattern_universe::PieceMultisetKey;

use super::{
    catalog::GeometryCatalog,
    geometry::{pack_piece_counts, GeometryFamilyCompileAdvance, GeometryFamilyCompileSession},
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::compile_setup_admissible_prefixes,
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
}

enum SetupGraphBuildStage {
    Geometry(GeometryFamilyCompileSession),
    PartialBuild(PartialBuildGraphBuilder),
    Finished,
}

pub(super) struct SetupGraphBuildSession {
    query: SetupSearchQuery,
    conditions: Vec<SetupSearchCondition>,
    catalog: Arc<GeometryCatalog>,
    stage: SetupGraphBuildStage,
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
        let catalog = Arc::new(GeometryCatalog::compile(first.problem())?);
        if catalog.width() != 10 || catalog.height() != 4 || catalog.initial_board() != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_finder_requires_empty_10x4_target",
            ));
        }

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
        let mut admissible_prefixes = compile_setup_admissible_prefixes(&conditions)?;
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
            catalog,
            stage: SetupGraphBuildStage::Geometry(geometry),
        })
    }

    pub(super) fn condition_count(&self) -> usize {
        self.conditions.len()
    }

    pub(super) fn condition_pattern_word_counts(&self) -> Result<Vec<usize>, WasmExactSearchError> {
        self.conditions
            .iter()
            .map(|condition| {
                condition
                    .problem()
                    .piece_source()
                    .materialized_universe()
                    .map(|universe| universe.pattern_count().div_ceil(u64::BITS as usize))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_pattern_universe_not_materialized",
                    ))
            })
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
            SetupGraphBuildStage::Geometry(mut geometry) => {
                match geometry.advance(&self.catalog, budget, control) {
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
                            &self.catalog,
                            self.conditions[0].problem(),
                        )?;
                        self.stage = SetupGraphBuildStage::PartialBuild(builder);
                        Ok(SetupGraphBuildAdvance::Pending)
                    }
                }
            }
            SetupGraphBuildStage::PartialBuild(mut builder) => {
                match builder.advance(&self.catalog, budget, control)? {
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
                        Ok(SetupGraphBuildAdvance::Complete(SetupSharedGraph {
                            query: self.query.clone(),
                            conditions: std::mem::take(&mut self.conditions),
                            graph,
                            coverage_graph,
                            geometry_family_count,
                            geometry_expanded_nodes,
                        }))
                    }
                }
            }
            SetupGraphBuildStage::Finished => Err(WasmExactSearchError::InvalidProblem(
                "setup_graph_build_session_already_finished",
            )),
        }
    }
}
