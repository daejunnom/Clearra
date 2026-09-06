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
use clearra_coverage::{
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    reducer::pattern_coverage_aggregation::{
        PatternCoverageAggregation, PatternCoverageCompleteness,
    },
    universe::coverage_universe_guard::CoverageUniverseGuard,
};
use clearra_finesse::{
    aggregate_unique_queue_costs, union_costed_geometry_languages, ClassicInputAction,
    CostedGeometryEdge, CostedGeometryLanguage, FinesseError, FinesseRouteWitnessError,
    FinesseSequenceInput, FinesseTarget, GeometryActionKey, GeometryLanguageError,
    GeometryLanguageNode, GeometryNodeId, PiecePose, QueueClass, QueueClassProductEvaluator,
    QueueClassSet, QueueCostAggregation, QueueCostTable, QueuePattern,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    FinesseMetric, FinessePatternKnowledge, FinesseScoreRequest, SearchProblem,
};
use clearra_replay::{ExactScoringExecutionBatch, ExactScoringExecutionGraph};
use clearra_rules::{kicks::KickTableProfile, spawn::SpawnProfile};
use clearra_supply::{
    hold_automaton::HoldAutomatonState,
    pattern_universe::{PackingPatternMembershipKind, PieceMultisetKey},
};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    resource::{
        admit_budget_bound_search_execution, ExecutionAdmission, ExecutionAdmissionPlan,
        ExecutionMemoryBound,
    },
    solution_probability::normalized_solution_probability_reports,
    CoreExecutionResult, CorePathStep, FinessePolicyResult, FinesseReport, FinesseReportInput,
    FinesseRepresentativeWitness, FinesseSolutionAverage, NormalizedSolutionCoverage,
    SolutionCoverage, TilingSolutionPageStore,
};

use super::{
    buildup::{
        exact_scoring_execution_graph_for_completion,
        exact_scoring_execution_graph_memory_projection, verify_candidate_for_completion,
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

// Completed results move directly out of the session without a second heap allocation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum BuildProbabilityAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildProbabilityCallerMemory {
    Compatibility,
    Finite {
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    },
}

impl BuildProbabilityCallerMemory {
    fn finite(
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        external_retained_owner_bytes
            .checked_add(returned_carrier_delta_bytes(returned_carrier_bytes))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_caller_memory_projection_overflow",
            ))?;
        Ok(Self::Finite {
            external_retained_owner_bytes,
            returned_carrier_bytes,
        })
    }

    const fn is_finite(self) -> bool {
        matches!(self, Self::Finite { .. })
    }

    const fn external_retained_owner_bytes(self) -> u128 {
        match self {
            Self::Compatibility => 0,
            Self::Finite {
                external_retained_owner_bytes,
                ..
            } => external_retained_owner_bytes,
        }
    }

    const fn returned_carrier_delta_bytes(self) -> u128 {
        match self {
            Self::Compatibility => 0,
            Self::Finite {
                returned_carrier_bytes,
                ..
            } => returned_carrier_delta_bytes(returned_carrier_bytes),
        }
    }
}

const fn returned_carrier_delta_bytes(returned_carrier_bytes: u128) -> u128 {
    returned_carrier_bytes.saturating_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
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
    _execution_admission: ExecutionAdmission,
    caller_memory: BuildProbabilityCallerMemory,
    finished: bool,
}

struct PendingFinesseScore {
    problem: SearchProblem,
    field: BuildProbabilityField,
    request: FinesseScoreRequest,
}

impl PendingFinesseScore {
    fn checked_nested_retained_bytes(&self) -> Option<u128> {
        // `PendingFinesseScore`, its `SearchProblem`, and the request's `Vec`
        // owner are inline in the outer session. Count only the problem's
        // nested heap plus the placement backing allocation.
        checked_build_probability_problem_nested_retained_bytes(&self.problem)?
            .checked_add(self.request.checked_retained_capacity_bytes()?)
    }
}

/// Returns only the problem's nested heap. A Compact or Extended session's
/// outer/backing allocation already contains its inline `SearchProblem`.
pub(super) fn checked_build_probability_problem_nested_retained_bytes(
    problem: &SearchProblem,
) -> Option<u128> {
    problem
        .checked_build_probability_pointee_retained_bytes()?
        .checked_sub(core::mem::size_of::<SearchProblem>() as u128)
}

// Sessions are long-lived state machines; keeping either representation inline avoids
// an allocation at every compact/extended dispatch boundary.
#[allow(clippy::large_enum_variant)]
enum BuildProbabilitySessionKind {
    Compact(CompactBuildProbabilitySession),
    Extended(super::extended_build_probability::ExtendedBuildProbabilitySession),
}

impl BuildProbabilitySessionKind {
    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Compact(session) => session.distributed_progress(),
            Self::Extended(session) => session.distributed_progress(),
        }
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(session) => session.checked_retained_bytes(),
            Self::Extended(session) => session.checked_retained_bytes(),
        }
    }

    fn set_coexisting_retained_bytes(&mut self, bytes: u128) {
        match self {
            Self::Compact(session) => session.set_coexisting_retained_bytes(bytes),
            Self::Extended(session) => session.set_coexisting_retained_bytes(bytes),
        }
    }

    fn checked_finesse_search_material_future_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(session) => session.checked_finesse_search_material_future_bytes(),
            Self::Extended(session) => session.checked_finesse_search_material_future_bytes(),
        }
    }
}

impl WasmBuildProbabilitySession {
    pub(crate) fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_caller_memory(
            problem,
            field,
            aggregation,
            finesse,
            BuildProbabilityCallerMemory::Compatibility,
        )
    }

    pub(crate) fn new_finite(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_caller_memory(
            problem,
            field,
            aggregation,
            finesse,
            BuildProbabilityCallerMemory::finite(
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )?,
        )
    }

    fn new_with_caller_memory(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        caller_memory: BuildProbabilityCallerMemory,
    ) -> Result<Self, WasmExactSearchError> {
        let finesse_metric = finesse.metric();
        let finesse_pattern_knowledge = finesse.pattern_knowledge();
        let finesse_score = finesse.score().cloned();
        let score_requested = finesse_score.is_some();
        if caller_memory.is_finite() && score_requested {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finite_build_probability_finesse_score_memory_authority_unavailable",
            ));
        }
        if problem.solution_probability_policy().requested() {
            if aggregation.is_tiling_only() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_solution_probabilities_unavailable_with_tiling",
                ));
            }
            if score_requested {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_solution_probabilities_unavailable_with_finesse_score",
                ));
            }
        }
        let mirror_included = !score_requested && field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let execution_admission = admit_budget_bound_search_execution(problem, 1)
            .map_err(WasmExactSearchError::resource_admission)?;
        let pass_count = usize::from(mirror_distinct) + 1;
        let allocation_plan = if score_requested {
            ExecutionAdmissionPlan::finesse_score(problem)
        } else {
            ExecutionAdmissionPlan::build_probability(problem, pass_count)
        };
        execution_admission
            .ensure_plan(
                allocation_plan,
                caller_memory.external_retained_owner_bytes(),
            )
            .map_err(WasmExactSearchError::resource_admission)?;
        let memory_bound = execution_admission.memory_bound();
        let mut pending = VecDeque::with_capacity(usize::from(mirror_distinct) + 1);
        if !score_requested {
            let initial_coexisting_retained_bytes =
                checked_initial_session_coexisting_retained_bytes(&pending, caller_memory)?;
            pending.push_back(build_probability_session_for_field(
                problem,
                original,
                aggregation,
                finesse_metric,
                memory_bound,
                initial_coexisting_retained_bytes,
            )?);
            if let Some(mirrored) = mirrored.filter(|candidate| *candidate != original) {
                let initial_coexisting_retained_bytes =
                    checked_initial_session_coexisting_retained_bytes(&pending, caller_memory)?;
                pending.push_back(build_probability_session_for_field(
                    problem,
                    mirrored,
                    aggregation,
                    finesse_metric,
                    memory_bound,
                    initial_coexisting_retained_bytes,
                )?);
            }
        }
        let session = Self {
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
            _execution_admission: execution_admission,
            caller_memory,
            finished: false,
        };
        if session.caller_memory.is_finite() {
            session.ensure_memory_bound(0)?;
        }
        Ok(session)
    }

    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if self.caller_memory.is_finite() {
            self.validate_finite_noncompleted_return_memory()?;
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_finite_build_probability_advance_requires_caller_memory",
            ));
        }
        self.advance_with_current_caller_memory(work_budget, control)
    }

    pub(crate) fn advance_finite(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if !self.caller_memory.is_finite() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_compatibility_session_rejects_finite_advance",
            ));
        }
        let caller_memory = BuildProbabilityCallerMemory::finite(
            external_retained_owner_bytes,
            returned_carrier_bytes,
        )?;
        // A Pending, Cancelled, or error return must coexist with the complete
        // outer carrier. Admit that carrier against the still-unmodified
        // session before any cooperative work can mutate or grow it.
        self.validate_finite_noncompleted_return_memory_with_caller_memory(caller_memory)?;
        let previous_caller_memory = self.caller_memory;
        self.caller_memory = caller_memory;
        let advance = self.advance_with_current_caller_memory(work_budget, control);
        if !matches!(&advance, Ok(BuildProbabilityAdvance::Completed(_))) {
            if let Err(error) = self.validate_finite_noncompleted_return_memory() {
                if matches!(&advance, Ok(BuildProbabilityAdvance::Cancelled)) {
                    self.caller_memory = previous_caller_memory;
                }
                return Err(error);
            }
        }
        if matches!(&advance, Ok(BuildProbabilityAdvance::Cancelled)) {
            self.caller_memory = previous_caller_memory;
        }
        advance
    }

    fn advance_with_current_caller_memory(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_session_already_finished",
            ));
        }
        self.ensure_memory_bound(0)?;
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
                self._execution_admission.memory_bound(),
            )?;
            self.finesse_score = None;
            self.finished = true;
            return Ok(BuildProbabilityAdvance::Completed(result));
        }
        let collect_search_finesse = self.finesse_metric.requested();
        let coexisting_retained_bytes = self.checked_front_coexisting_retained_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ),
        )?;
        let advance = {
            let Some(session) = self.pending.front_mut() else {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_pass_missing",
                ));
            };
            session.set_coexisting_retained_bytes(coexisting_retained_bytes);
            match session {
                BuildProbabilitySessionKind::Compact(session) => {
                    session.advance(work_budget, control)
                }
                BuildProbabilitySessionKind::Extended(session) => {
                    session.advance(work_budget, control)
                }
            }?
        };
        self.report_cooperative_progress(control);
        match advance {
            BuildProbabilityAdvance::Pending => {
                self.ensure_memory_bound(0)?;
                Ok(BuildProbabilityAdvance::Pending)
            }
            BuildProbabilityAdvance::Cancelled => Ok(BuildProbabilityAdvance::Cancelled),
            BuildProbabilityAdvance::Completed(result) => {
                let result_bytes = checked_public_result_bytes(&result).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_aggregate_memory_projection_overflow",
                    ),
                )?;
                self.ensure_memory_bound(result_bytes)?;
                if collect_search_finesse {
                    let session =
                        self.pending
                            .front()
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_build_probability_pass_missing",
                            ))?;
                    let material_future = session
                        .checked_finesse_search_material_future_bytes()
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_finesse_search_material_projection_overflow",
                        ))?;
                    let material_slot_future = if self.finesse_search_materials.len()
                        == self.finesse_search_materials.capacity()
                    {
                        (self.finesse_search_materials.len() as u128)
                            .checked_add(1)
                            .and_then(|slots| {
                                slots.checked_mul(
                                    core::mem::size_of::<FinesseSearchMaterial>() as u128
                                )
                            })
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_finesse_search_material_projection_overflow",
                            ))?
                    } else {
                        0
                    };
                    self.ensure_memory_bound(
                        result_bytes
                            .checked_add(material_future)
                            .and_then(|bytes| bytes.checked_add(material_slot_future))
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_finesse_search_material_projection_overflow",
                            ))?,
                    )?;
                    if material_slot_future != 0 {
                        // A plain `push` may geometrically over-allocate. Force
                        // the checked `len + 1` buffer after its old and new
                        // payload coexistence has been admitted above.
                        self.finesse_search_materials
                            .try_reserve_exact(1)
                            .map_err(|_| {
                                WasmExactSearchError::InvalidProblem(
                                    "wasm_finesse_search_material_storage_unavailable",
                                )
                            })?;
                    }
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
                self.ensure_memory_bound(0)?;
                if !self.pending.is_empty() {
                    return Ok(BuildProbabilityAdvance::Pending);
                }
                self.finished = true;
                let mut result = merge_symmetry_results_with_memory_guard(
                    core::mem::take(&mut self.completed),
                    self.mirror_included,
                    self.mirror_distinct,
                    &self.pattern_weights,
                    self.aggregation.requests_spin_coverage()
                        || self.execution_constraints_requested,
                    |source_bytes, future_bytes| {
                        let checked_future = source_bytes.checked_add(future_bytes).ok_or(
                            WasmExactSearchError::InvalidProblem(
                                "wasm_build_probability_symmetry_memory_projection_overflow",
                            ),
                        )?;
                        self.ensure_memory_bound(checked_future)
                    },
                )?;
                if collect_search_finesse {
                    result = attach_finesse_report_with_memory_guard(
                        result,
                        core::mem::take(&mut self.finesse_search_materials),
                        self.finesse_metric,
                        self.finesse_pattern_knowledge,
                        control,
                        |live, future| self.validate_public_result_memory_with_future(live, future),
                    )?;
                }
                let result_bytes = checked_public_result_bytes(&result).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_aggregate_memory_projection_overflow",
                    ),
                )?;
                self.ensure_completion_memory_bound(result_bytes)?;
                Ok(BuildProbabilityAdvance::Completed(result))
            }
        }
    }

    fn report_cooperative_progress(&self, control: &ExecutionControl) {
        // Native and finite-memory owners may deliberately install no progress
        // sink. Do not derive or retain any telemetry for those owners.
        if control.progress_sink.is_none() {
            return;
        }
        let mut progress = self.pending.front().map_or_else(
            WasmDistributedProgress::default,
            BuildProbabilitySessionKind::progress,
        );
        for result in &self.completed {
            progress.geometry_nodes = progress.geometry_nodes.saturating_add(
                result
                    .usize_field("geometry_searched_nodes")
                    .or_else(|| result.usize_field("searched_nodes"))
                    .unwrap_or(0),
            );
            progress.candidates = progress
                .candidates
                .saturating_add(result.usize_field("packing_candidate_count").unwrap_or(0));
            progress.build_nodes = progress.build_nodes.saturating_add(
                result
                    .usize_field("buildup_searched_nodes")
                    .or_else(|| result.usize_field("total_build_order_nodes"))
                    .unwrap_or(0),
            );
        }
        // A cooperative slice verifies each emitted candidate before returning.
        // Keep counts separate instead of using one count as a fabricated total.
        control.report_progress("build-geometry", progress.geometry_nodes as u64, None);
        control.report_progress("build-candidates", progress.candidates as u64, None);
        control.report_progress("build-verification", progress.build_nodes as u64, None);
    }

    pub(crate) fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let retained =
            checked_public_result_bytes(result).ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        let future = retained
            .checked_add(checked_future_bytes)
            .and_then(|bytes| bytes.checked_add(self.caller_memory.returned_carrier_delta_bytes()))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        self.ensure_memory_bound(future)
    }

    pub(crate) fn validate_public_result_memory_with_finite_caller_memory(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        if !self.caller_memory.is_finite() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_compatibility_session_rejects_finite_validation",
            ));
        }
        let caller_memory = BuildProbabilityCallerMemory::finite(
            external_retained_owner_bytes,
            returned_carrier_bytes,
        )?;
        let retained = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(caller_memory.external_retained_owner_bytes()))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        let future = checked_public_result_bytes(result)
            .and_then(|bytes| bytes.checked_add(checked_future_bytes))
            .and_then(|bytes| bytes.checked_add(caller_memory.returned_carrier_delta_bytes()))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        self._execution_admission
            .ensure_memory_bound(retained, future)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn ensure_completion_memory_bound(
        &self,
        checked_public_result_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let future = checked_public_result_bytes
            .checked_add(self.caller_memory.returned_carrier_delta_bytes())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        self.ensure_memory_bound(future)
    }

    pub(crate) fn validate_finite_noncompleted_return_memory(
        &self,
    ) -> Result<(), WasmExactSearchError> {
        if !self.caller_memory.is_finite() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_compatibility_session_rejects_finite_validation",
            ));
        }
        self.validate_finite_noncompleted_return_memory_with_caller_memory(self.caller_memory)
    }

    pub(crate) fn validate_finite_noncompleted_return_memory_with_replacement(
        &self,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        if !self.caller_memory.is_finite() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_compatibility_session_rejects_finite_validation",
            ));
        }
        let caller_memory = BuildProbabilityCallerMemory::finite(
            external_retained_owner_bytes,
            returned_carrier_bytes,
        )?;
        self.validate_finite_noncompleted_return_memory_with_caller_memory(caller_memory)
    }

    fn validate_finite_noncompleted_return_memory_with_caller_memory(
        &self,
        caller_memory: BuildProbabilityCallerMemory,
    ) -> Result<(), WasmExactSearchError> {
        let returned_carrier_bytes = match caller_memory {
            BuildProbabilityCallerMemory::Compatibility => {
                unreachable!("compatibility session was rejected before finite carrier validation")
            }
            BuildProbabilityCallerMemory::Finite {
                returned_carrier_bytes,
                ..
            } => returned_carrier_bytes,
        };
        self.ensure_memory_bound_with_caller_memory(returned_carrier_bytes, caller_memory)
    }

    fn ensure_memory_bound(&self, checked_future_bytes: u128) -> Result<(), WasmExactSearchError> {
        self.ensure_memory_bound_with_caller_memory(checked_future_bytes, self.caller_memory)
    }

    fn ensure_memory_bound_with_caller_memory(
        &self,
        checked_future_bytes: u128,
        caller_memory: BuildProbabilityCallerMemory,
    ) -> Result<(), WasmExactSearchError> {
        let retained = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(caller_memory.external_retained_owner_bytes()))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_aggregate_memory_projection_overflow",
            ))?;
        self._execution_admission
            .ensure_memory_bound(retained, checked_future_bytes)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut retained = (self.pending.capacity() as u128)
            .checked_mul(core::mem::size_of::<BuildProbabilitySessionKind>() as u128)?
            .checked_add(
                (self.completed.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
            )?
            .checked_add(
                (self.finesse_search_materials.capacity() as u128)
                    .checked_mul(core::mem::size_of::<FinesseSearchMaterial>() as u128)?,
            )?
            .checked_add(self.pattern_weights.checked_storage_retained_bytes()?)?;
        for session in &self.pending {
            retained = retained.checked_add(session.checked_retained_bytes()?)?;
        }
        let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
        for result in &self.completed {
            retained = retained.checked_add(
                result
                    .checked_resource_retained_bytes()?
                    .checked_sub(result_inline)?,
            )?;
        }
        for material in &self.finesse_search_materials {
            retained = retained.checked_add(material.checked_nested_retained_bytes()?)?;
        }
        if let Some(score) = &self.finesse_score {
            retained = retained.checked_add(score.checked_nested_retained_bytes()?)?;
        }
        Some(retained)
    }

    fn checked_front_coexisting_retained_bytes(&self) -> Option<u128> {
        let mut retained = self
            .caller_memory
            .external_retained_owner_bytes()
            .checked_add(
                (self.pending.capacity() as u128)
                    .checked_mul(core::mem::size_of::<BuildProbabilitySessionKind>() as u128)?,
            )?
            .checked_add(
                (self.completed.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
            )?
            .checked_add(
                (self.finesse_search_materials.capacity() as u128)
                    .checked_mul(core::mem::size_of::<FinesseSearchMaterial>() as u128)?,
            )?
            .checked_add(self.pattern_weights.checked_storage_retained_bytes()?)?;
        for session in self.pending.iter().skip(1) {
            retained = retained.checked_add(session.checked_retained_bytes()?)?;
        }
        let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
        for result in &self.completed {
            retained = retained.checked_add(
                result
                    .checked_resource_retained_bytes()?
                    .checked_sub(result_inline)?,
            )?;
        }
        for material in &self.finesse_search_materials {
            retained = retained.checked_add(material.checked_nested_retained_bytes()?)?;
        }
        if let Some(score) = &self.finesse_score {
            retained = retained.checked_add(score.checked_nested_retained_bytes()?)?;
        }
        Some(retained)
    }
}

fn checked_initial_session_coexisting_retained_bytes(
    pending: &VecDeque<BuildProbabilitySessionKind>,
    caller_memory: BuildProbabilityCallerMemory,
) -> Result<u128, WasmExactSearchError> {
    if !caller_memory.is_finite() {
        return Ok(0);
    }
    let mut retained = caller_memory
        .external_retained_owner_bytes()
        .checked_add(
            (pending.capacity() as u128)
                .checked_mul(core::mem::size_of::<BuildProbabilitySessionKind>() as u128)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_initial_memory_projection_overflow",
                ))?,
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_initial_memory_projection_overflow",
        ))?;
    for session in pending {
        retained = retained
            .checked_add(session.checked_retained_bytes().ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_initial_memory_projection_unavailable",
                ),
            )?)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_initial_memory_projection_overflow",
            ))?;
    }
    Ok(retained)
}

pub(super) fn checked_public_result_bytes(result: &CoreExecutionResult) -> Option<u128> {
    let bytes = result.checked_resource_retained_bytes()?;
    Some(bytes.max(result.usize_field("resource_peak_cpu_bytes").unwrap_or(0) as u128))
}

pub(super) fn checked_core_result_vec_retained_bytes(
    results: &Vec<CoreExecutionResult>,
) -> Option<u128> {
    let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
    let mut retained = (results.capacity() as u128).checked_mul(result_inline)?;
    for result in results {
        retained = retained.checked_add(
            result
                .checked_resource_retained_bytes()?
                .checked_sub(result_inline)?,
        )?;
    }
    Some(retained)
}

// A finite binary64 in [0, 1] has decimal exponent no smaller than -324 and
// needs at most 17 significant decimal digits for round-trip text. Rust's
// canonical `Display` may choose fixed notation, so reserve `0.`, every
// leading fractional position, and all significant digits.
pub(super) const MAX_CANONICAL_PROBABILITY_TEXT_BYTES: u128 = 2 + 324 + 17;

fn checked_symmetry_merge_future_bytes(
    results: &Vec<CoreExecutionResult>,
    pattern_weights: &WeightedPatternSet,
    materialize_postprocess_pattern_weights: bool,
    solution_probabilities_requested: bool,
) -> Option<u128> {
    // Two source-sized owners cover the sorted merge scratch and the final
    // cloned/derived solution surface while all raw pass results remain live.
    // Report and serialized-weight strings have no raw-pass counterpart and
    // are projected separately below.
    let source_bytes = checked_core_result_vec_retained_bytes(results)?;
    let mut future = source_bytes
        .checked_mul(2)?
        .checked_add(checked_build_probability_fixed_result_surface_bytes()?)?;
    if solution_probabilities_requested {
        let solution_count = results.iter().try_fold(0_usize, |count, result| {
            count.checked_add(result.normalized_solution_keys().len())
        })?;
        let solution_key_bytes = results
            .iter()
            .flat_map(|result| result.normalized_solution_keys())
            .try_fold(0_u128, |bytes, key| bytes.checked_add(key.len() as u128))?;
        future = future
            .checked_add(
                (solution_count as u128)
                    .checked_mul(core::mem::size_of::<crate::SolutionProbabilityReport>() as u128)?,
            )?
            .checked_add(solution_key_bytes)?
            .checked_add(
                (solution_count as u128).checked_mul(MAX_CANONICAL_PROBABILITY_TEXT_BYTES)?,
            )?;
    }
    if materialize_postprocess_pattern_weights || solution_probabilities_requested {
        future = future.checked_add(
            (pattern_weights.len() as u128).checked_mul(
                (core::mem::size_of::<String>() as u128)
                    .checked_add(MAX_CANONICAL_PROBABILITY_TEXT_BYTES)?,
            )?,
        )?;
    }
    Some(future)
}

fn build_probability_session_for_field(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    memory_bound: ExecutionMemoryBound,
    coexisting_retained_bytes: u128,
) -> Result<BuildProbabilitySessionKind, WasmExactSearchError> {
    if field.is_compact() {
        let session = CompactBuildProbabilitySession::new_with_external_geometry(
            problem,
            field,
            aggregation,
            false,
            None,
            finesse_metric.requested(),
            memory_bound,
            coexisting_retained_bytes,
        )?;
        Ok(BuildProbabilitySessionKind::Compact(session))
    } else {
        let session = super::extended_build_probability::ExtendedBuildProbabilitySession::new_with_memory_bound_and_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            finesse_metric.requested(),
            memory_bound,
            coexisting_retained_bytes,
        )?;
        Ok(BuildProbabilitySessionKind::Extended(session))
    }
}

pub(super) fn merge_symmetry_results(
    results: Vec<CoreExecutionResult>,
    mirror_included: bool,
    mirror_distinct: bool,
    pattern_weights: &WeightedPatternSet,
    materialize_postprocess_pattern_weights: bool,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    merge_symmetry_results_with_memory_guard(
        results,
        mirror_included,
        mirror_distinct,
        pattern_weights,
        materialize_postprocess_pattern_weights,
        |_, _| Ok(()),
    )
}

pub(super) fn merge_symmetry_results_with_memory_guard(
    mut results: Vec<CoreExecutionResult>,
    mirror_included: bool,
    mirror_distinct: bool,
    pattern_weights: &WeightedPatternSet,
    materialize_postprocess_pattern_weights: bool,
    mut memory_guard: impl FnMut(u128, u128) -> Result<(), WasmExactSearchError>,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    if results.is_empty() || results.len() != usize::from(mirror_distinct) + 1 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_symmetry_pass_mismatch",
        ));
    }
    let primary_source = &results[0];
    let tiling_only = primary_source.field("build_probability_aggregation") == Some("tiling");
    let solution_probabilities_requested = exact_solution_probabilities_requested(primary_source)?;
    for result in &results[1..] {
        if exact_solution_probabilities_requested(result)? != solution_probabilities_requested {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probability_policy_mismatch",
            ));
        }
    }
    if tiling_only && solution_probabilities_requested {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_solution_probabilities_unavailable_with_tiling",
        ));
    }
    let pattern_count = exact_usize_field(
        primary_source,
        "coverage_pattern_count",
        "wasm_build_probability_symmetry_pattern_count_invalid",
    )?;
    for result in &results[1..] {
        if exact_usize_field(
            result,
            "coverage_pattern_count",
            "wasm_build_probability_symmetry_pattern_count_invalid",
        )? != pattern_count
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_pattern_count_mismatch",
            ));
        }
    }
    if pattern_weights.len() != pattern_count {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_pattern_weights_missing",
        ));
    }
    let pattern_universe_id = exact_u64_field(
        primary_source,
        "pattern_universe_id",
        "wasm_build_probability_symmetry_pattern_universe_id_invalid",
    )?;
    let pattern_weight_model_id = exact_u64_field(
        primary_source,
        "pattern_weight_model_id",
        "wasm_build_probability_symmetry_pattern_weight_model_id_invalid",
    )?;
    for result in &results[1..] {
        if exact_u64_field(
            result,
            "pattern_universe_id",
            "wasm_build_probability_symmetry_pattern_universe_id_invalid",
        )? != pattern_universe_id
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_pattern_universe_id_mismatch",
            ));
        }
        if exact_u64_field(
            result,
            "pattern_weight_model_id",
            "wasm_build_probability_symmetry_pattern_weight_model_id_invalid",
        )? != pattern_weight_model_id
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_pattern_weight_model_id_mismatch",
            ));
        }
    }
    let source_retained_bytes = checked_core_result_vec_retained_bytes(&results).ok_or(
        WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_symmetry_memory_projection_overflow",
        ),
    )?;
    let merge_future_bytes = checked_symmetry_merge_future_bytes(
        &results,
        pattern_weights,
        materialize_postprocess_pattern_weights,
        solution_probabilities_requested,
    )
    .ok_or(WasmExactSearchError::InvalidProblem(
        "wasm_build_probability_symmetry_memory_projection_overflow",
    ))?;
    memory_guard(source_retained_bytes, merge_future_bytes)?;

    let mut primary = results.remove(0);
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

    let probability_complete = exact_bool_field(
        &primary,
        "probability_complete",
        "wasm_build_probability_symmetry_probability_complete_invalid",
    )? && results.iter().all(|result| {
        exact_bool_field(
            result,
            "probability_complete",
            "wasm_build_probability_symmetry_probability_complete_invalid",
        )
        .unwrap_or(false)
    });
    let count_complete = exact_bool_field(
        &primary,
        "count_complete",
        "wasm_build_probability_symmetry_count_complete_invalid",
    )? && results.iter().all(|result| {
        exact_bool_field(
            result,
            "count_complete",
            "wasm_build_probability_symmetry_count_complete_invalid",
        )
        .unwrap_or(false)
    });
    for result in &results {
        exact_bool_field(
            result,
            "probability_complete",
            "wasm_build_probability_symmetry_probability_complete_invalid",
        )?;
        exact_bool_field(
            result,
            "count_complete",
            "wasm_build_probability_symmetry_count_complete_invalid",
        )?;
    }
    let coverage_completeness =
        PatternCoverageCompleteness::new(probability_complete, true, !tiling_only);
    let guard = CoverageUniverseGuard::new(
        clearra_coverage::universe::pattern_universe_id::PatternUniverseId::new(
            pattern_universe_id,
        ),
        clearra_coverage::universe::pattern_weight_model_id::PatternWeightModelId::new(
            pattern_weight_model_id,
        ),
        pattern_count,
    );
    let original_coverage =
        crate::strict_coverage_pattern_bitset_from_words(pattern_count, &original_words).map_err(
            |_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_symmetry_original_coverage_invalid",
                )
            },
        )?;
    let union_coverage =
        crate::strict_coverage_pattern_bitset_from_words(pattern_count, &union_words).map_err(
            |_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_symmetry_union_coverage_invalid",
                )
            },
        )?;
    let original_aggregation = PatternCoverageAggregation::from_success_coverage(
        guard,
        exact_usize_field(
            &primary,
            "coverage_aggregation_source_row_count",
            "wasm_build_probability_symmetry_coverage_source_count_invalid",
        )?,
        &original_coverage,
        pattern_weights,
        coverage_completeness,
    )
    .map_err(|_| {
        WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_symmetry_original_aggregation_invalid",
        )
    })?;
    let union_source_row_count =
        core::iter::once(&primary)
            .chain(results.iter())
            .try_fold(0_usize, |count, result| {
                count
                    .checked_add(exact_usize_field(
                        result,
                        "coverage_aggregation_source_row_count",
                        "wasm_build_probability_symmetry_coverage_source_count_invalid",
                    )?)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symmetry_coverage_source_count_overflow",
                    ))
            })?;
    // With no distinct symmetry result, these inputs are identical (including
    // completeness and source row count). Reuse the owned aggregation rather
    // than scanning every P7 weight a second time. Keep the same reserved
    // owned bitset capacity and all ingress validation above.
    let union_aggregation = if results.is_empty() {
        original_aggregation.clone()
    } else {
        PatternCoverageAggregation::from_success_coverage(
            guard,
            union_source_row_count,
            &union_coverage,
            pattern_weights,
            coverage_completeness,
        )
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_union_aggregation_invalid",
            )
        })?
    };
    let mirror_aggregation = if mirror_included && results.is_empty() {
        Some(original_aggregation.clone())
    } else if mirror_included {
        let mirror = results.first().unwrap_or(&primary);
        let coverage = crate::strict_coverage_pattern_bitset_from_words(
            pattern_count,
            mirror.coverage_pattern_words(),
        )
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symmetry_mirror_coverage_invalid",
            )
        })?;
        Some(
            PatternCoverageAggregation::from_success_coverage(
                guard,
                exact_usize_field(
                    mirror,
                    "coverage_aggregation_source_row_count",
                    "wasm_build_probability_symmetry_coverage_source_count_invalid",
                )?,
                &coverage,
                pattern_weights,
                coverage_completeness,
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_symmetry_mirror_aggregation_invalid",
                )
            })?,
        )
    } else {
        None
    };
    let original_covered = original_aggregation.success_pattern_count();
    let union_covered = union_aggregation.success_pattern_count();
    let mirrored_covered = mirror_aggregation
        .as_ref()
        .map_or(0, PatternCoverageAggregation::success_pattern_count);
    let original_probability = original_aggregation.success_probability().get();
    let union_probability = union_aggregation.success_probability().get();
    let mirrored_probability = mirror_aggregation
        .as_ref()
        .map_or(0.0, |summary| summary.success_probability().get());
    let resource_truncated = exact_bool_field(
        &primary,
        "resource_truncated",
        "wasm_build_probability_symmetry_resource_truncated_invalid",
    )? || results.iter().any(|result| {
        exact_bool_field(
            result,
            "resource_truncated",
            "wasm_build_probability_symmetry_resource_truncated_invalid",
        )
        .unwrap_or(true)
    });
    for result in &results {
        exact_bool_field(
            result,
            "resource_truncated",
            "wasm_build_probability_symmetry_resource_truncated_invalid",
        )?;
    }
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
            .page_identities(
                0,
                if solution_probabilities_requested {
                    store.len()
                } else {
                    100
                },
            )
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
    let solution_probability_keys_complete = merged_page_store
        .as_ref()
        .map_or(normalized_solutions_complete, |store| {
            merged_solution_keys.len() == store.len()
        });
    let solution_probability_complete = !solution_probabilities_requested
        || (probability_complete
            && count_complete
            && !resource_truncated
            && solution_probability_keys_complete);
    let solution_probabilities =
        if solution_probabilities_requested && solution_probability_keys_complete {
            normalized_solution_probability_reports(
                &merged_solution_keys,
                &merged_normalized_solution_coverages,
                pattern_weights,
                solution_probability_complete,
            )
            .map_err(|error| WasmExactSearchError::InvalidProblem(error.reason()))?
        } else {
            Vec::new()
        };
    let search_output_policy = primary
        .field("search_output_policy")
        .filter(|policy| matches!(*policy, "summary" | "trace" | "coverage-rows"))
        .unwrap_or("summary")
        .to_owned();
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
            "failed_pattern_count",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                union_aggregation.failed_pattern_count().to_string()
            },
        ),
        field(
            "coverage_probability",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(union_probability)
            },
        ),
        field(
            "failed_coverage_probability",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(union_aggregation.failed_probability().get())
            },
        ),
        field(
            "materialized_probability_mass",
            probability_text(union_aggregation.materialized_probability_mass().get()),
        ),
        field(
            "coverage_aggregation_contract",
            PatternCoverageAggregation::CONTRACT_ID,
        ),
        field(
            "coverage_aggregation_availability",
            if tiling_only {
                "not-calculated"
            } else {
                union_aggregation.availability().as_str()
            },
        ),
        field("coverage_aggregation_complete", probability_complete),
        field(
            "coverage_aggregation_source_row_count",
            union_source_row_count,
        ),
        field(
            "coverage_probability_denominator",
            "full-materialized-pattern-universe",
        ),
        field(
            "success_conditional_probability_denominator",
            if tiling_only {
                "not-calculated".to_owned()
            } else {
                probability_text(union_aggregation.success_probability().get())
            },
        ),
        field("probability_complete", probability_complete),
        field("count_complete", count_complete),
        field(
            "solution_probabilities_requested",
            solution_probabilities_requested,
        ),
        field("solution_probability_count", solution_probabilities.len()),
        field(
            "solution_probability_complete",
            solution_probability_complete,
        ),
        field(
            "solution_probability_basis",
            if solution_probabilities_requested {
                "normalized-solution-pattern-bitset-or-union"
            } else {
                "not-requested"
            },
        ),
        field(
            "solution_probability_incomplete_reason",
            if !solution_probabilities_requested || solution_probability_complete {
                "none"
            } else if resource_truncated {
                "resource-truncated"
            } else if !count_complete {
                "solution-count-incomplete"
            } else if !solution_probability_keys_complete {
                "normalized-solution-set-incomplete"
            } else {
                "pattern-specific-coverage-incomplete"
            },
        ),
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
        let merged_solution_count = merged_page_store
            .as_ref()
            .map_or(merged_solution_keys.len(), |store| store.len());
        let materialized_key_count = merged_solution_keys.len();
        let keys_complete = materialized_key_count == merged_solution_count;
        let page_available = merged_page_store.is_some() && !keys_complete;
        replacements.extend([
            field("search_output_policy", &search_output_policy),
            field("unique_solution_count", merged_solution_count),
            field("normalized_unique_solution_count", merged_solution_count),
            field(
                "actual_normalized_unique_solution_count",
                merged_solution_count,
            ),
            field("solution_count_calculated", true),
            field("solution_set_materialized", true),
            field("solution_keys_materialized_count", materialized_key_count),
            field("solution_keys_complete", keys_complete),
            field("solution_page_available", page_available),
            field("normalized_solution_set_hash", &merged_solution_hash),
            field("actual_normalized_solution_set_hash", merged_solution_hash),
        ]);
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
        .with_solution_probabilities(solution_probabilities)
        .with_exact_scoring_execution_batches(scoring_batches)
        .with_spin_coverage_execution_batches(spin_coverage_batches)
        .with_replaced_fields(replacements);
    let merged = if materialize_postprocess_pattern_weights || solution_probabilities_requested {
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

pub(super) fn probability_text(probability: f64) -> String {
    if probability == 0.0 {
        "0".to_owned()
    } else {
        probability.to_string()
    }
}

pub(super) fn build_pattern_coverage_aggregation(
    problem: &SearchProblem,
    source_row_count: usize,
    success_coverage: &PatternBitSet,
    completeness: PatternCoverageCompleteness,
) -> Result<PatternCoverageAggregation, WasmExactSearchError> {
    let universe = problem.piece_source().materialized_universe().ok_or(
        WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
    )?;
    PatternCoverageAggregation::from_success_coverage(
        CoverageUniverseGuard::new(
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            universe.pattern_count(),
        ),
        source_row_count,
        success_coverage,
        universe.weights(),
        completeness,
    )
    .map_err(|_| {
        WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_shared_coverage_aggregation_invalid",
        )
    })
}

fn validate_exact_text_field(
    result: &CoreExecutionResult,
    key: &str,
    expected: &str,
    invalid_reason: &'static str,
    mismatch_reason: &'static str,
) -> Result<(), WasmExactSearchError> {
    if result.field_occurrence_count(key) != 1 {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    if result.unique_field(key) != Some(expected) {
        return Err(WasmExactSearchError::InvalidProblem(mismatch_reason));
    }
    Ok(())
}

/// Validates the worker-owned Build coverage authority against the retained
/// coordinator problem. Shape alone is insufficient: two universes can have
/// the same pattern count while assigning every bit to a different pattern or
/// weight. The caller must complete the value/bitset validation below before
/// committing the worker coverage with OR-union.
pub(super) fn validate_distributed_coverage_authority(
    problem: &SearchProblem,
    aggregation: BuildProbabilityAggregation,
    result: &CoreExecutionResult,
    pattern_count: usize,
    probability_complete: bool,
    coordinator_coverage_can_be_complete: bool,
) -> Result<usize, WasmExactSearchError> {
    let universe = problem.piece_source().materialized_universe().ok_or(
        WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
    )?;
    if pattern_count != universe.pattern_count() {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_pattern_universe_shape_mismatch",
        ));
    }
    if exact_usize_field(
        result,
        "materialized_pattern_count",
        "wasm_build_probability_distributed_materialized_pattern_count_invalid",
    )? != pattern_count
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_materialized_pattern_count_mismatch",
        ));
    }
    if exact_u64_field(
        result,
        "piece_source_id",
        "wasm_build_probability_distributed_piece_source_id_invalid",
    )? != problem.piece_source().id().get()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_piece_source_id_mismatch",
        ));
    }
    if exact_u64_field(
        result,
        "pattern_universe_id",
        "wasm_build_probability_distributed_pattern_universe_id_invalid",
    )? != universe.pattern_universe_id().get()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_pattern_universe_id_mismatch",
        ));
    }
    if exact_u64_field(
        result,
        "pattern_weight_model_id",
        "wasm_build_probability_distributed_pattern_weight_model_id_invalid",
    )? != universe.pattern_weight_model_id().get()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_pattern_weight_model_id_mismatch",
        ));
    }
    validate_exact_text_field(
        result,
        "build_probability_aggregation",
        aggregation.as_str(),
        "wasm_build_probability_distributed_aggregation_invalid",
        "wasm_build_probability_distributed_aggregation_mismatch",
    )?;
    validate_exact_text_field(
        result,
        "coverage_aggregation_contract",
        PatternCoverageAggregation::CONTRACT_ID,
        "wasm_build_probability_distributed_coverage_contract_invalid",
        "wasm_build_probability_distributed_coverage_contract_mismatch",
    )?;
    validate_exact_text_field(
        result,
        "coverage_probability_denominator",
        "full-materialized-pattern-universe",
        "wasm_build_probability_distributed_coverage_denominator_invalid",
        "wasm_build_probability_distributed_coverage_denominator_mismatch",
    )?;

    let coverage_complete = exact_bool_field(
        result,
        "coverage_aggregation_complete",
        "wasm_build_probability_distributed_coverage_complete_invalid",
    )?;
    if coverage_complete != probability_complete
        || (probability_complete && !coordinator_coverage_can_be_complete)
        || (aggregation.is_tiling_only() && probability_complete)
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_coverage_complete_mismatch",
        ));
    }
    let expected_availability = if aggregation.is_tiling_only() {
        "not-calculated"
    } else if probability_complete {
        "available"
    } else {
        "incomplete"
    };
    validate_exact_text_field(
        result,
        "coverage_aggregation_availability",
        expected_availability,
        "wasm_build_probability_distributed_coverage_availability_invalid",
        "wasm_build_probability_distributed_coverage_availability_mismatch",
    )?;

    let source_row_count = exact_usize_field(
        result,
        "coverage_aggregation_source_row_count",
        "wasm_build_probability_distributed_coverage_source_count_invalid",
    )?;
    Ok(source_row_count)
}

/// Recomputes every derived Build coverage value from the coordinator's
/// retained universe and the validated worker bitset. This must run before the
/// caller mutates its aggregate coverage.
pub(super) fn validate_distributed_coverage_aggregation_surface(
    problem: &SearchProblem,
    aggregation: BuildProbabilityAggregation,
    result: &CoreExecutionResult,
    coverage: &PatternBitSet,
    source_row_count: usize,
    probability_complete: bool,
) -> Result<(), WasmExactSearchError> {
    let universe = problem.piece_source().materialized_universe().ok_or(
        WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
    )?;
    validate_exact_text_field(
        result,
        "materialized_probability_mass",
        probability_text(universe.weights().total_weight().get()).as_str(),
        "wasm_build_probability_distributed_materialized_probability_mass_invalid",
        "wasm_build_probability_distributed_materialized_probability_mass_mismatch",
    )?;

    if aggregation.is_tiling_only() {
        if coverage.count_ones() != 0
            || exact_usize_field(
                result,
                "covered_pattern_count",
                "wasm_build_probability_distributed_covered_pattern_count_invalid",
            )? != 0
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_tiling_coverage_mismatch",
            ));
        }
        for key in [
            "failed_pattern_count",
            "coverage_probability",
            "failed_coverage_probability",
            "success_conditional_probability_denominator",
        ] {
            validate_exact_text_field(
                result,
                key,
                "not-calculated",
                "wasm_build_probability_distributed_tiling_coverage_field_invalid",
                "wasm_build_probability_distributed_tiling_coverage_field_mismatch",
            )?;
        }
        return Ok(());
    }

    let summary = build_pattern_coverage_aggregation(
        problem,
        source_row_count,
        coverage,
        PatternCoverageCompleteness::new(true, probability_complete, true),
    )?;
    if exact_usize_field(
        result,
        "covered_pattern_count",
        "wasm_build_probability_distributed_covered_pattern_count_invalid",
    )? != summary.success_pattern_count()
        || exact_usize_field(
            result,
            "failed_pattern_count",
            "wasm_build_probability_distributed_failed_pattern_count_invalid",
        )? != summary.failed_pattern_count()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_coverage_count_mismatch",
        ));
    }
    let success_probability = probability_text(summary.success_probability().get());
    validate_exact_text_field(
        result,
        "coverage_probability",
        success_probability.as_str(),
        "wasm_build_probability_distributed_coverage_probability_invalid",
        "wasm_build_probability_distributed_coverage_probability_mismatch",
    )?;
    validate_exact_text_field(
        result,
        "failed_coverage_probability",
        probability_text(summary.failed_probability().get()).as_str(),
        "wasm_build_probability_distributed_failed_probability_invalid",
        "wasm_build_probability_distributed_failed_probability_mismatch",
    )?;
    validate_exact_text_field(
        result,
        "success_conditional_probability_denominator",
        success_probability.as_str(),
        "wasm_build_probability_distributed_success_denominator_invalid",
        "wasm_build_probability_distributed_success_denominator_mismatch",
    )
}

pub(super) fn exact_solution_probabilities_requested(
    result: &CoreExecutionResult,
) -> Result<bool, WasmExactSearchError> {
    match result.field_occurrence_count("solution_probabilities_requested") {
        0 => Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_solution_probability_policy_missing",
        )),
        1 => match result.unique_field("solution_probabilities_requested") {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probability_policy_invalid",
            )),
        },
        _ => Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_solution_probability_policy_duplicate",
        )),
    }
}

pub(super) fn exact_bool_field(
    result: &CoreExecutionResult,
    key: &str,
    invalid_reason: &'static str,
) -> Result<bool, WasmExactSearchError> {
    if result.field_occurrence_count(key) != 1 {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    match result.unique_field(key) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(WasmExactSearchError::InvalidProblem(invalid_reason)),
    }
}

pub(super) fn exact_usize_field(
    result: &CoreExecutionResult,
    key: &str,
    invalid_reason: &'static str,
) -> Result<usize, WasmExactSearchError> {
    if result.field_occurrence_count(key) != 1 {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    let value = result
        .unique_field(key)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(WasmExactSearchError::InvalidProblem(invalid_reason))?;
    if result.unique_field(key) != Some(value.to_string().as_str()) {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    Ok(value)
}

pub(super) fn exact_u128_field(
    result: &CoreExecutionResult,
    key: &str,
    invalid_reason: &'static str,
) -> Result<u128, WasmExactSearchError> {
    if result.field_occurrence_count(key) != 1 {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    let value = result
        .unique_field(key)
        .and_then(|value| value.parse::<u128>().ok())
        .ok_or(WasmExactSearchError::InvalidProblem(invalid_reason))?;
    if result.unique_field(key) != Some(value.to_string().as_str()) {
        return Err(WasmExactSearchError::InvalidProblem(invalid_reason));
    }
    Ok(value)
}

pub(super) fn exact_u64_field(
    result: &CoreExecutionResult,
    key: &str,
    invalid_reason: &'static str,
) -> Result<u64, WasmExactSearchError> {
    let value = exact_u128_field(result, key, invalid_reason)?;
    u64::try_from(value).map_err(|_| WasmExactSearchError::InvalidProblem(invalid_reason))
}

pub(super) fn solution_coverage_union_matches_global<T>(
    pattern_count: usize,
    global_words: &[u64],
    coverages: &[T],
    covered_patterns: impl Fn(&T) -> &PatternBitSet,
) -> bool {
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    global_words.len() == word_count
        && (0..word_count).all(|word_index| {
            coverages.iter().fold(0_u64, |union, coverage| {
                union | covered_patterns(coverage).word_at(word_index)
            }) == global_words[word_index]
        })
}

pub(super) fn validate_worker_partial_probability_surface(
    result: &CoreExecutionResult,
) -> Result<(), WasmExactSearchError> {
    if !result.solution_probabilities().is_empty()
        || [
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ]
        .into_iter()
        .any(|key| result.field_occurrence_count(key) != 0)
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_final_probability_surface_forbidden",
        ));
    }
    Ok(())
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
    board_height: u8,
    target_cells: u64,
    final_board: u64,
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
    distributed_probability_complete: bool,
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
    memory_bound: ExecutionMemoryBound,
    coexisting_retained_bytes: u128,
    finished: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompactDistributedPartial {
    count_complete: bool,
    probability_complete: bool,
    resource_truncated: bool,
    build_variant_count: u128,
    coverage_source_row_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompactBuildProbabilitySharedCatalogKey {
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
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

// Browser workers call the distributed entrypoints; native builds retain the same
// session surface so both targets share one implementation.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl CompactBuildProbabilitySession {
    // Browser product execution always supplies an explicit finite-memory
    // boundary through the constructors below; keep this unbounded adapter for
    // native embeddings and parity tests only.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            false,
            None,
            false,
            memory_bound,
            0,
        )
    }

    pub(super) fn new_with_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            false,
            None,
            false,
            memory_bound,
            0,
        )
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_with_finesse(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_finesse_and_memory_bound(
            problem,
            field,
            aggregation,
            finesse_requested,
            memory_bound,
        )
    }

    pub(super) fn new_with_finesse_and_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            false,
            None,
            finesse_requested,
            memory_bound,
            0,
        )
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_external_geometry_with_memory_bound(problem, field, aggregation, memory_bound)
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_external_geometry_with_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            memory_bound,
            0,
        )
    }

    pub(super) fn new_external_geometry_with_memory_bound_and_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            true,
            None,
            false,
            memory_bound,
            coexisting_retained_bytes,
        )
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_with_shared_supply_catalog(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
    ) -> Result<Self, WasmExactSearchError> {
        let memory_bound = ExecutionMemoryBound::unbounded_for_problem(problem)
            .map_err(WasmExactSearchError::resource_admission)?;
        Self::new_with_shared_supply_catalog_and_memory_bound(
            problem,
            field,
            aggregation,
            external_geometry,
            shared_supply_catalog,
            memory_bound,
        )
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn new_with_shared_supply_catalog_and_memory_bound(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_shared_supply_catalog_and_memory_bound_and_coexisting_retained_bytes(
            problem,
            field,
            aggregation,
            external_geometry,
            shared_supply_catalog,
            memory_bound,
            0,
        )
    }

    pub(super) fn new_with_shared_supply_catalog_and_memory_bound_and_coexisting_retained_bytes(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: &CompactBuildProbabilitySharedCatalog,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_external_geometry(
            problem,
            field,
            aggregation,
            external_geometry,
            Some(shared_supply_catalog),
            false,
            memory_bound,
            coexisting_retained_bytes,
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

    // Constructor parameters make ownership and memory-accounting inputs explicit.
    #[allow(clippy::too_many_arguments)]
    fn new_with_external_geometry(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        external_geometry: bool,
        shared_supply_catalog: Option<&CompactBuildProbabilitySharedCatalog>,
        finesse_requested: bool,
        memory_bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        super::ensure_connected_kick_profile(problem)?;
        if aggregation.is_tiling_only() && problem.solution_probability_policy().requested() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probabilities_unavailable_with_tiling",
            ));
        }
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
            pattern_weight_model_id: universe.pattern_weight_model_id().get(),
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
                let family = universe.packing_multiset_family_for_execution(
                    target_piece_count,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    super::packing_hold_projection(problem),
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
        let session = Self {
            problem: problem.clone(),
            aggregation,
            board_height: field.height(),
            target_cells,
            final_board: super::buildup::place_and_clear(
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
            solution_coverage: (problem.solution_probability_policy().requested()
                || problem.objective().execution_constraints().requested())
            .then(ExactHashMap::default),
            candidate_count: 0,
            candidate_digest: 0,
            build_variant_count: 0,
            count_complete: true,
            distributed_probability_complete: true,
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
            memory_bound,
            coexisting_retained_bytes,
            finished: false,
        };
        session.ensure_memory_bound(0)?;
        Ok(session)
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
        self.ensure_memory_bound(0)?;
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
                    self.ensure_memory_bound(0)?;
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
                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
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
                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
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

    // The browser coordinator calls the explicit external-memory-guard form so
    // retained worker payloads are included in every admission decision.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn advance_distributed_geometry(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.advance_distributed_geometry_with_candidate_memory_guard(
            pass_index,
            control,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_candidate_row_storage_projection_overflow",
                    ))?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_candidate_row_storage_projection_overflow",
                    ))?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    pub(super) fn advance_distributed_geometry_with_candidate_memory_guard(
        &mut self,
        pass_index: u8,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<WasmCandidateProducerAdvance, WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        if self.trivial_target {
            return Ok(WasmCandidateProducerAdvance::Completed(
                self.distributed_geometry_summary(None),
            ));
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
                let row_ids = self.try_copy_distributed_candidate_row_ids_with_memory_guard(
                    candidate.row_ids(),
                    &mut memory_guard,
                )?;
                let ordinal = self.candidate_count as u64;
                self.candidate_count = self.candidate_count.saturating_add(1);
                self.candidate_digest =
                    super::mix_digest(self.candidate_digest, candidate.identity.bucket_hash());
                Ok(WasmCandidateProducerAdvance::Candidate(
                    WasmCandidatePacket::for_pass(
                        ordinal,
                        pass_index,
                        candidate.target_index,
                        row_ids,
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

    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn try_copy_distributed_candidate_row_ids(
        &self,
        source: &[u32],
    ) -> Result<Vec<u32>, WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.try_copy_distributed_candidate_row_ids_with_memory_guard(
            source,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_candidate_row_storage_projection_overflow",
                    ))?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_candidate_row_storage_projection_overflow",
                    ))?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    fn try_copy_distributed_candidate_row_ids_with_memory_guard(
        &self,
        source: &[u32],
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<Vec<u32>, WasmExactSearchError> {
        let requested_bytes = (source.len() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_candidate_row_storage_projection_overflow",
            ))?;
        memory_guard(self, 0, requested_bytes)?;

        let mut row_ids = Vec::new();
        row_ids.try_reserve_exact(source.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_candidate_row_storage_unavailable",
            )
        })?;
        let actual_bytes = (row_ids.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_candidate_row_storage_projection_overflow",
            ))?;
        memory_guard(self, actual_bytes, 0)?;
        row_ids.extend_from_slice(source);
        Ok(row_ids)
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
        self.ensure_memory_bound(0)?;
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
        self.process_candidate(geometry, Some(candidate.ordinal()), false, control)?;
        self.ensure_memory_bound(0)
    }

    pub(super) fn complete_distributed_worker(
        &mut self,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        self.ensure_memory_bound(0)?;
        if self.finished {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_verifier_already_finished",
            ));
        }
        if !self.aggregation.is_tiling_only() {
            self.ensure_symbolic_coverage_finalization_bound()?;
            if let Some(symbolic) = self.buildup.materialize_standard_bag_coverage()? {
                self.covered_patterns.union_with(&symbolic).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symbolic_coverage_mismatch",
                    )
                })?;
            }
        }
        self.ensure_result_materialization_bound()?;
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested()
            || self.problem.objective().score().requested();
        let scoring_batch = if execution_evidence_requested {
            Some(self.prepare_exact_spin_execution_batch()?)
        } else {
            None
        };
        self.finished = true;
        let result = self.build_result(scoring_batch)?;
        self.ensure_materialized_result_bound(&result)?;
        Ok(result)
    }

    // Browser result absorption likewise uses the coordinator-owned memory
    // guard; this convenience adapter remains for native/parity callers.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn absorb_distributed_result(
        &mut self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let memory_bound = self.memory_bound;
        let coexisting_retained_bytes = self.coexisting_retained_bytes;
        self.absorb_distributed_result_with_memory_guard(
            result,
            move |session, local_retained_bytes, checked_future_bytes| {
                let observed = session
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(local_retained_bytes))
                    .ok_or_else(|| {
                        WasmExactSearchError::resource_admission(
                            memory_bound.ensure(u128::MAX, 1).expect_err(
                                "checked compact absorb storage overflow is unavailable",
                            ),
                        )
                    })?;
                let future = coexisting_retained_bytes
                    .checked_add(checked_future_bytes)
                    .ok_or_else(|| {
                        WasmExactSearchError::resource_admission(
                            memory_bound.ensure(u128::MAX, 1).expect_err(
                                "checked compact absorb future overflow is unavailable",
                            ),
                        )
                    })?;
                memory_bound
                    .ensure(observed, future)
                    .map_err(WasmExactSearchError::resource_admission)
            },
        )
    }

    pub(super) fn absorb_distributed_result_with_memory_guard(
        &mut self,
        result: &CoreExecutionResult,
        mut memory_guard: impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        memory_guard(self, 0, 0)?;
        let pattern_count = exact_usize_field(
            result,
            "coverage_pattern_count",
            "wasm_build_probability_distributed_pattern_count_invalid",
        )?;
        let partial = self.validate_distributed_solution_surface(result, pattern_count)?;
        let coverage_future =
            PatternBitSet::checked_external_words_materialize_union_future_bytes(pattern_count)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_distributed_coverage_projection_overflow",
                ))?;
        memory_guard(self, 0, coverage_future)?;
        let mut coverage_words = Vec::new();
        coverage_words
            .try_reserve_exact(result.coverage_pattern_words().len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_distributed_coverage_storage_unavailable",
                )
            })?;
        let coverage_word_bytes = (coverage_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_projection_overflow",
            ))?;
        memory_guard(self, coverage_word_bytes, coverage_future)?;
        coverage_words.extend_from_slice(result.coverage_pattern_words());
        let coverage = PatternBitSet::from_words(pattern_count, coverage_words).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_invalid",
            )
        })?;
        let coverage_retained_bytes = coverage.checked_storage_retained_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_projection_overflow",
            ),
        )?;
        memory_guard(self, coverage_retained_bytes, coverage_future)?;
        if result
            .coverage_pattern_words()
            .iter()
            .copied()
            .enumerate()
            .any(|(word_index, word)| coverage.word_at(word_index) != word)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_invalid",
            ));
        }
        validate_distributed_coverage_aggregation_surface(
            &self.problem,
            self.aggregation,
            result,
            &coverage,
            partial.coverage_source_row_count,
            partial.probability_complete,
        )?;
        self.covered_patterns.union_with(&coverage).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_mismatch",
            )
        })?;
        memory_guard(self, coverage_retained_bytes, 0)?;
        drop(coverage);
        memory_guard(self, 0, 0)?;
        self.distributed_spin_materialized |= !result.postprocess_spin_coverages().is_empty();
        if self.problem.objective().execution_constraints().requested() {
            self.distributed_execution_constraint_materialized &= result
                .bool_field("execution_constraint_materialized")
                .unwrap_or(false);
        }

        for identity in result.normalized_solution_identities() {
            if !self.buildable_tilings.contains(identity) {
                memory_guard(
                    self,
                    0,
                    core::mem::size_of::<StandardBoard64TilingIdentity>() as u128,
                )?;
                self.buildable_tilings.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_solution_storage_unavailable",
                    )
                })?;
                memory_guard(self, 0, 0)?;
                self.buildable_tilings.insert(*identity);
                memory_guard(self, 0, 0)?;
            }
        }
        if self.solution_coverage.is_some() {
            for coverage in result.solution_coverages() {
                self.merge_solution_coverage_with_memory_guard(
                    coverage.identity(),
                    coverage.covered_patterns(),
                    &mut memory_guard,
                )?;
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
        let next_variants = self
            .build_variant_count
            .checked_add(partial.build_variant_count);
        self.build_variant_count = next_variants.unwrap_or(u128::MAX);
        self.count_complete &= next_variants.is_some() && partial.count_complete;
        self.distributed_probability_complete &= partial.probability_complete;
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
                let requested_path_bytes = (result.path_steps().len() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_path_projection_overflow",
                    ))?;
                memory_guard(self, 0, requested_path_bytes)?;
                let mut representative_path = Vec::new();
                representative_path
                    .try_reserve_exact(result.path_steps().len())
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_distributed_path_storage_unavailable",
                        )
                    })?;
                let actual_path_bytes = (representative_path.capacity() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_path_projection_overflow",
                    ))?;
                memory_guard(self, actual_path_bytes, 0)?;
                representative_path.extend_from_slice(result.path_steps());
                memory_guard(self, actual_path_bytes, 0)?;
                self.representative_path = representative_path;
                memory_guard(self, 0, 0)?;
            }
        }
        if partial.resource_truncated {
            self.truncated_reason = Some("distributed_worker_incomplete");
            self.count_complete = false;
            self.distributed_probability_complete = false;
        }
        memory_guard(self, 0, 0)
    }

    fn merge_solution_coverage_with_memory_guard(
        &mut self,
        identity: StandardBoard64TilingIdentity,
        coverage: &PatternBitSet,
        memory_guard: &mut impl FnMut(&Self, u128, u128) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        let is_new = !self
            .solution_coverage
            .as_ref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_solution_coverage_not_requested",
            ))?
            .contains_key(&identity);
        let union_future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            coverage.pattern_count(),
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_solution_coverage_projection_overflow",
        ))?;
        let mut requested_future = union_future;
        if is_new {
            requested_future = requested_future
                .checked_add(
                    (core::mem::size_of::<StandardBoard64TilingIdentity>()
                        + core::mem::size_of::<PatternBitSet>()) as u128,
                )
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_projection_overflow",
                ))?;
        }
        memory_guard(self, 0, requested_future)?;
        if is_new {
            self.solution_coverage
                .as_mut()
                .expect("the requested solution coverage map exists")
                .try_reserve(1)
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_solution_coverage_storage_unavailable",
                    )
                })?;
            memory_guard(self, 0, union_future)?;
            self.solution_coverage
                .as_mut()
                .expect("the requested solution coverage map exists")
                .insert(identity, PatternBitSet::new(coverage.pattern_count()));
            memory_guard(self, 0, union_future)?;
        }
        self.solution_coverage
            .as_mut()
            .expect("the requested solution coverage map exists")
            .get_mut(&identity)
            .expect("the requested solution coverage entry exists")
            .union_with(coverage)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_universe_mismatch",
                )
            })?;
        memory_guard(self, 0, 0)
    }

    fn validate_distributed_solution_surface(
        &self,
        result: &CoreExecutionResult,
        pattern_count: usize,
    ) -> Result<CompactDistributedPartial, WasmExactSearchError> {
        if pattern_count != self.covered_patterns.pattern_count() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_pattern_count_mismatch",
            ));
        }
        let board_height = exact_usize_field(
            result,
            "board_height",
            "wasm_build_probability_distributed_board_height_invalid",
        )?;
        if board_height != usize::from(self.board_height) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_board_height_mismatch",
            ));
        }
        let requested = exact_solution_probabilities_requested(result)?;
        if requested != self.problem.solution_probability_policy().requested() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_solution_probability_policy_mismatch",
            ));
        }
        let count_complete = exact_bool_field(
            result,
            "count_complete",
            "wasm_build_probability_distributed_count_complete_invalid",
        )?;
        let probability_complete = exact_bool_field(
            result,
            "probability_complete",
            "wasm_build_probability_distributed_probability_complete_invalid",
        )?;
        let resource_truncated = exact_bool_field(
            result,
            "resource_truncated",
            "wasm_build_probability_distributed_resource_truncated_invalid",
        )?;
        let worker_solution_count = exact_usize_field(
            result,
            "unique_solution_count",
            "wasm_build_probability_distributed_solution_count_invalid",
        )?;
        let build_variant_count = exact_u128_field(
            result,
            "build_variant_count",
            "wasm_build_probability_distributed_variant_count_invalid",
        )?;
        let universe = self.problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let coverage_source_row_count = validate_distributed_coverage_authority(
            &self.problem,
            self.aggregation,
            result,
            pattern_count,
            probability_complete,
            universe.complete() && !resource_truncated,
        )?;

        let identities = result.normalized_solution_identities();
        let keys = result.normalized_solution_keys();
        if worker_solution_count != identities.len() || worker_solution_count != keys.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_surface_incomplete",
            ));
        }
        if !identities.windows(2).all(|pair| pair[0] < pair[1])
            || keys.iter().any(String::is_empty)
            || !keys.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_surface_not_canonical",
            ));
        }
        for (identity, key) in identities.iter().copied().zip(keys) {
            self.validate_distributed_identity(identity, key)?;
        }

        let coverages = result.solution_coverages();
        let normalized_coverages = result.normalized_solution_coverages();
        let solution_coverage_required = self.solution_coverage.is_some();
        if !coverages
            .windows(2)
            .all(|pair| pair[0].identity() < pair[1].identity())
            || !normalized_coverages
                .windows(2)
                .all(|pair| pair[0].solution_key() < pair[1].solution_key())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_coverage_not_canonical",
            ));
        }
        if solution_coverage_required && coverages.len() != worker_solution_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_board64_solution_coverage_incomplete",
            ));
        }
        if !solution_coverage_required
            && (!coverages.is_empty() || !normalized_coverages.is_empty())
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_unexpected_solution_coverage",
            ));
        }
        if solution_coverage_required
            && !normalized_coverages.is_empty()
            && normalized_coverages.len() != worker_solution_count
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_normalized_solution_coverage_incomplete",
            ));
        }
        if !normalized_coverages.is_empty() && coverages.len() != normalized_coverages.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_coverage_surface_mismatch",
            ));
        }
        for coverage in coverages {
            let position = identities
                .binary_search(&coverage.identity())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_solution_coverage_foreign_identity",
                    )
                })?;
            if coverage.covered_patterns().pattern_count() != pattern_count {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_distributed_solution_coverage_mismatch",
                ));
            }
            if !normalized_coverages.is_empty() {
                let normalized = normalized_coverages.get(position).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_solution_coverage_mismatch",
                    ),
                )?;
                if normalized.solution_key() != keys[position]
                    || normalized.covered_patterns() != coverage.covered_patterns()
                {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_solution_coverage_mismatch",
                    ));
                }
            }
        }
        if solution_coverage_required
            && !solution_coverage_union_matches_global(
                pattern_count,
                result.coverage_pattern_words(),
                coverages,
                SolutionCoverage::covered_patterns,
            )
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_coverage_union_mismatch",
            ));
        }
        validate_worker_partial_probability_surface(result)?;
        if coverage_source_row_count != worker_solution_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_coverage_source_count_mismatch",
            ));
        }

        Ok(CompactDistributedPartial {
            count_complete,
            probability_complete,
            resource_truncated,
            build_variant_count,
            coverage_source_row_count,
        })
    }

    fn validate_distributed_identity(
        &self,
        identity: StandardBoard64TilingIdentity,
        key: &str,
    ) -> Result<(), WasmExactSearchError> {
        if identity.initial_board_mask() != self.catalog.initial_board()
            || identity.placement_count() != self.target_cells.count_ones() as usize / 4
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_identity_domain_mismatch",
            ));
        }
        let canonical = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
        if canonical.as_str() != key {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_solution_key_mismatch",
            ));
        }
        if self.target_cells == 0 {
            return Ok(());
        }

        let mut row_ids = [0_u32; MAX_BOARD64_PIECES];
        let mut covered = 0_u64;
        for (index, row_slot) in row_ids
            .iter_mut()
            .enumerate()
            .take(identity.placement_count())
        {
            let placement =
                identity
                    .placement(index)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_distributed_identity_placement_missing",
                    ))?;
            let row_id = self
                .catalog
                .skeleton_id(placement.piece(), placement.cells_mask())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_distributed_identity_not_in_catalog",
                ))?;
            covered |= placement.cells_mask();
            *row_slot = row_id;
        }
        if covered != self.target_cells {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_identity_target_mismatch",
            ));
        }
        let multiset =
            PieceMultisetKey::from_pieces((0..identity.placement_count()).map(|index| {
                identity
                    .placement(index)
                    .expect("the identity placement count was validated")
                    .piece()
            }));
        let target = self
            .shared_supply_catalog
            .targets
            .targets()
            .iter()
            .find(|target| target.key == multiset)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_identity_supply_mismatch",
            ))?;
        let reconstructed = GeometryCandidate::from_rows(
            &self.catalog,
            target.pattern_index_id,
            &row_ids[..identity.placement_count()],
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_distributed_identity_reconstruction_failed",
        ))?;
        if reconstructed.identity != identity {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_distributed_identity_reconstruction_mismatch",
            ));
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
                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
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
        self.distributed_probability_complete = true;
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
            self.ensure_symbolic_coverage_finalization_bound()?;
            if let Some(symbolic) = self.buildup.materialize_standard_bag_coverage()? {
                self.covered_patterns.union_with(&symbolic).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_build_probability_symbolic_coverage_mismatch",
                    )
                })?;
            }
        }
        self.ensure_result_materialization_bound()?;
        self.finished = true;
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested()
            || self.problem.objective().score().requested();
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
        let result = self.build_result(scoring_batch)?;
        self.ensure_materialized_result_bound(&result)?;
        Ok(BuildProbabilityAdvance::Completed(result))
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
                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
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
        let count_complete = self.count_complete
            && self.truncated_reason.is_none()
            && (!tiling_only || self.shared_supply_catalog.supply_projection_complete);
        let coverage_source_row_count = self
            .buildable_tilings
            .len()
            .checked_add(usize::from(self.trivial_target))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_coverage_source_count_overflow",
            ))?;
        let coverage_aggregation = if tiling_only {
            None
        } else {
            Some(build_pattern_coverage_aggregation(
                &self.problem,
                coverage_source_row_count,
                &self.covered_patterns,
                PatternCoverageCompleteness::new(
                    universe.complete(),
                    self.distributed_probability_complete && self.truncated_reason.is_none(),
                    true,
                ),
            )?)
        };
        let probability = coverage_aggregation.as_ref().map_or_else(
            || "not-calculated".to_owned(),
            |summary| probability_text(summary.success_probability().get()),
        );
        let probability_complete = coverage_aggregation
            .as_ref()
            .is_some_and(|summary| summary.completeness().is_complete());
        let build_variant_count_exact = !tiling_only
            && self.problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountAll
            && count_complete;
        let objective_policy = self.problem.objective();
        let execution_constraints = objective_policy.execution_constraints();
        let score_policy = objective_policy.score();
        let score_requested = score_policy.requested();
        let execution_constraint_complete = !execution_constraints.requested()
            || self.distributed_execution_constraint_materialized;
        let solution_found = self.trivial_target || !self.buildable_tilings.is_empty();
        let mut identities = self.buildable_tilings.iter().copied().collect::<Vec<_>>();
        if self.trivial_target {
            identities.push(
                StandardBoard64TilingIdentity::from_placements(
                    self.catalog.initial_board(),
                    core::iter::empty(),
                )
                .expect("the canonical empty compact tiling is valid"),
            );
        }
        identities.sort_unstable();
        identities.dedup();
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
        let mut solution_coverages = self
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
        if self.trivial_target && self.solution_coverage.is_some() {
            let empty = identities[0];
            if !solution_coverages
                .iter()
                .any(|coverage| coverage.identity() == empty)
            {
                solution_coverages.push(SolutionCoverage::new(
                    empty,
                    PatternBitSet::all(universe.pattern_count()),
                ));
                solution_coverages.sort_unstable_by_key(SolutionCoverage::identity);
            }
        }
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
            field("board_height", self.board_height),
            field(
                "build_probability_completion",
                "exact-board-with-inverse-lock-clear",
            ),
            field("build_base_mask", self.catalog.initial_board()),
            field("build_target_cells_mask", self.target_cells),
            field(
                "build_target_board_mask",
                self.catalog.initial_board() | self.target_cells,
            ),
            field("build_final_board_mask", self.final_board),
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
            field("unique_solution_count", identities.len()),
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
            field("piece_source_id", self.problem.piece_source().id().get()),
            field("pattern_universe_id", universe.pattern_universe_id().get()),
            field(
                "pattern_weight_model_id",
                universe.pattern_weight_model_id().get(),
            ),
            field(
                "coverage_aggregation_contract",
                PatternCoverageAggregation::CONTRACT_ID,
            ),
            field(
                "coverage_aggregation_availability",
                coverage_aggregation
                    .as_ref()
                    .map_or("not-calculated", |summary| summary.availability().as_str()),
            ),
            field("coverage_aggregation_complete", probability_complete),
            field(
                "coverage_aggregation_source_row_count",
                coverage_source_row_count,
            ),
            field(
                "covered_pattern_count",
                coverage_aggregation
                    .as_ref()
                    .map_or(0, PatternCoverageAggregation::success_pattern_count),
            ),
            field(
                "failed_pattern_count",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| summary.failed_pattern_count().to_string(),
                ),
            ),
            field("coverage_probability", probability),
            field(
                "failed_coverage_probability",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| probability_text(summary.failed_probability().get()),
                ),
            ),
            field(
                "materialized_probability_mass",
                probability_text(universe.weights().total_weight().get()),
            ),
            field(
                "coverage_probability_denominator",
                "full-materialized-pattern-universe",
            ),
            field(
                "success_conditional_probability_denominator",
                coverage_aggregation.as_ref().map_or_else(
                    || "not-calculated".to_owned(),
                    |summary| probability_text(summary.success_probability().get()),
                ),
            ),
            field("probability_complete", probability_complete),
            field("count_complete", count_complete),
            field(
                "solution_probabilities_requested",
                self.problem.solution_probability_policy().requested(),
            ),
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
            field("postprocess_scoring_requested", score_requested),
            field("score_objective_mode", score_policy.mode().as_str()),
            field("score_profile_requested", score_policy.profile().as_str()),
            field(
                "score_spin_profile_requested",
                score_policy.spin_profile().as_str(),
            ),
            field("score_initial_b2b", score_policy.initial_b2b()),
            field(
                "postprocess_execution_owner",
                "clearra-app->clearra-postprocess",
            ),
            field(
                "postprocess_replay_seed_available",
                !self.representative_path.is_empty(),
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
                    && !score_requested
                    && execution_constraint_complete,
            ),
            field(
                "objective_incomplete_reason",
                if score_requested {
                    "score_matrix_not_materialized"
                } else if !count_complete || (!tiling_only && !probability_complete) {
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
            .with_coverage_pattern_words(self.covered_patterns.to_owned_words())
            .with_solution_coverages(solution_coverages)
            .with_normalized_solution_coverages(normalized_solution_coverages)
            .with_exact_scoring_execution_batch(scoring_batch);
        let result = if execution_constraints.requested() || score_requested {
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

    pub(super) fn checked_finesse_search_material_future_bytes(&self) -> Option<u128> {
        let language_count = self
            .finesse_languages
            .len()
            .checked_add(usize::from(self.trivial_target))?;
        let mut bytes = (language_count as u128)
            .checked_mul(core::mem::size_of::<(String, CostedGeometryLanguage)>() as u128)?;
        for (identity, prepared) in &self.finesse_languages {
            bytes = bytes
                .checked_add(checked_board64_canonical_key_len(*identity)?)?
                .checked_add(
                    (prepared.nodes.len() as u128)
                        .checked_mul(core::mem::size_of::<GeometryLanguageNode>() as u128)?,
                )?
                .checked_add(
                    (prepared.edges.len() as u128)
                        .checked_mul(core::mem::size_of::<CostedGeometryEdge>() as u128)?,
                )?;
        }
        if self.trivial_target {
            let identity = StandardBoard64TilingIdentity::from_placements(
                self.catalog.initial_board(),
                std::iter::empty(),
            )
            .ok()?;
            bytes = bytes
                .checked_add(checked_board64_canonical_key_len(identity)?)?
                .checked_add(core::mem::size_of::<GeometryLanguageNode>() as u128)?;
        }
        bytes.checked_add(FinesseSearchMaterial::checked_fixed_creation_future_bytes(
            &self.problem,
        )?)
    }

    fn retained_bytes(&self) -> usize {
        self.checked_retained_bytes()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(usize::MAX)
    }

    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        checked_build_probability_problem_nested_retained_bytes(&self.problem)?
            .checked_add(self.checked_non_problem_retained_bytes()?)
    }

    fn checked_non_problem_retained_bytes(&self) -> Option<u128> {
        let mut total = 0_u128;
        for bytes in [
            self.catalog.retained_bytes(),
            self.geometry.retained_bytes(),
            self.buildup.retained_bytes(),
            self.coverage_evaluator.retained_bytes(),
            self.covered_patterns.retained_bytes(),
        ] {
            total = total.checked_add(bytes as u128)?;
        }
        total = total.checked_add(
            (self.buildable_tilings.capacity() as u128)
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?,
        )?;
        if let Some(coverage) = self.solution_coverage.as_ref() {
            total = total.checked_add((coverage.capacity() as u128).checked_mul(
                (core::mem::size_of::<StandardBoard64TilingIdentity>()
                    + core::mem::size_of::<PatternBitSet>()) as u128,
            )?)?;
            for patterns in coverage.values() {
                total = total.checked_add(patterns.retained_bytes() as u128)?;
            }
        }
        total = total.checked_add(
            (self.representative_path.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
        )?;
        total = total.checked_add((self.finesse_languages.capacity() as u128).checked_mul(
            core::mem::size_of::<(StandardBoard64TilingIdentity, PreparedFinesseLanguage)>()
                as u128,
        )?)?;
        for (_, language) in &self.finesse_languages {
            total = total.checked_add((language.nodes.capacity() as u128).checked_mul(
                core::mem::size_of::<super::buildup::PreparedFinesseNode>() as u128,
            )?)?;
            total = total.checked_add((language.edges.capacity() as u128).checked_mul(
                core::mem::size_of::<super::buildup::PreparedFinesseEdge>() as u128,
            )?)?;
        }
        Some(total)
    }

    pub(super) fn set_coexisting_retained_bytes(&mut self, bytes: u128) {
        self.coexisting_retained_bytes = bytes;
    }

    #[cfg(test)]
    pub(super) fn set_memory_bound_for_test(&mut self, memory_bound: ExecutionMemoryBound) {
        self.memory_bound = memory_bound;
    }

    fn ensure_result_materialization_bound(&self) -> Result<(), WasmExactSearchError> {
        let future = self.checked_result_materialization_future_bytes().ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_result_materialization_projection_overflow",
            ),
        )?;
        self.ensure_memory_bound(future)
    }

    fn ensure_symbolic_coverage_finalization_bound(&self) -> Result<(), WasmExactSearchError> {
        let Some(universe) = self.problem.piece_source().materialized_universe() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_symbolic_coverage_projection_unavailable",
            ));
        };
        if self.candidate_count == 0
            || !StandardBagCoverage::supports(universe, self.problem.initial_hold())
        {
            return Ok(());
        }
        let future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            universe.pattern_count(),
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_build_probability_symbolic_coverage_projection_overflow",
        ))?;
        self.ensure_memory_bound(future)
    }

    fn ensure_materialized_result_bound(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmExactSearchError> {
        let result_bytes =
            checked_public_result_bytes(result).ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_result_materialization_projection_overflow",
            ))?;
        self.ensure_memory_bound(result_bytes)
    }

    fn checked_result_materialization_future_bytes(&self) -> Option<u128> {
        let trivial_identity = self.trivial_target.then(|| {
            StandardBoard64TilingIdentity::from_placements(
                self.catalog.initial_board(),
                core::iter::empty(),
            )
            .expect("the canonical empty compact tiling is valid")
        });
        let identity_count = self
            .buildable_tilings
            .len()
            .checked_add(usize::from(trivial_identity.is_some()))?;
        let mut key_bytes = 0_u128;
        let mut peak_temporary_key_bytes = 0_u128;
        for identity in self
            .buildable_tilings
            .iter()
            .copied()
            .chain(trivial_identity)
        {
            let key_len = checked_board64_canonical_key_len(identity)?;
            key_bytes = key_bytes.checked_add(key_len)?;
            peak_temporary_key_bytes = peak_temporary_key_bytes
                .max((42_u128).checked_add((identity.placement_count() as u128).checked_mul(20)?)?);
        }
        let coverage_count = self.solution_coverage.as_ref().map_or(0, |coverage| {
            coverage.len() + usize::from(self.trivial_target && coverage.is_empty())
        });
        let mut coverage_key_bytes = 0_u128;
        if let Some(coverage) = self.solution_coverage.as_ref() {
            for identity in coverage.keys().copied() {
                coverage_key_bytes =
                    coverage_key_bytes.checked_add(checked_board64_canonical_key_len(identity)?)?;
            }
            if self.trivial_target && coverage.is_empty() {
                coverage_key_bytes =
                    coverage_key_bytes.checked_add(checked_board64_canonical_key_len(
                        trivial_identity.expect("trivial identity is available"),
                    )?)?;
            }
        }

        let mut future = (identity_count as u128)
            .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?
            .checked_add(
                (identity_count as u128).checked_mul(core::mem::size_of::<String>() as u128)?,
            )?
            .checked_add(key_bytes)?
            .checked_add(
                (coverage_count as u128)
                    .checked_mul(core::mem::size_of::<SolutionCoverage>() as u128)?,
            )?
            .checked_add(
                (coverage_count as u128)
                    .checked_mul(core::mem::size_of::<NormalizedSolutionCoverage>() as u128)?,
            )?
            .checked_add(coverage_key_bytes)?
            .checked_add(peak_temporary_key_bytes)?
            .checked_add(
                (self.covered_patterns.word_count() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?
            .checked_add(
                (self.representative_path.len() as u128)
                    .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
            )?;
        future = future.checked_add(checked_build_probability_fixed_result_surface_bytes()?)?;
        if self.trivial_target && self.solution_coverage.is_some() {
            future = future.checked_add(
                PatternBitSet::checked_all_projection(self.covered_patterns.pattern_count())?
                    .constructor_peak_bytes,
            )?;
        }
        if self.problem.objective().execution_constraints().requested()
            || self.problem.objective().score().requested()
        {
            future = future.checked_add(
                (self.covered_patterns.pattern_count() as u128).checked_mul(
                    (core::mem::size_of::<String>() as u128)
                        .checked_add(MAX_CANONICAL_PROBABILITY_TEXT_BYTES)?,
                )?,
            )?;
        }
        let execution_evidence_requested = self.aggregation.requests_spin_coverage()
            || self.problem.objective().execution_constraints().requested()
            || self.problem.objective().score().requested();
        let evidence_materialized = self.distributed_spin_materialized
            || (self.problem.objective().execution_constraints().requested()
                && self.distributed_execution_constraint_materialized);
        if execution_evidence_requested && !evidence_materialized {
            let identity_storage_bytes = (self.buildable_tilings.len() as u128)
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?;
            let mut retained_graph_bytes = (self.buildable_tilings.len() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionGraph>() as u128)?;
            let mut scoring_peak_bytes =
                identity_storage_bytes.checked_add(retained_graph_bytes)?;
            for identity in self.buildable_tilings.iter().copied() {
                let projection = exact_scoring_execution_graph_memory_projection(
                    &self.problem,
                    &self.catalog,
                    identity,
                )
                .ok()?;
                scoring_peak_bytes = scoring_peak_bytes.max(
                    identity_storage_bytes
                        .checked_add(retained_graph_bytes)?
                        .checked_add(projection.peak_additional_bytes)?,
                );
                retained_graph_bytes =
                    retained_graph_bytes.checked_add(projection.retained_graph_nested_bytes)?;
            }
            let universe = self.problem.piece_source().materialized_universe()?;
            let mut pattern_bytes = (universe.pattern_count() as u128)
                .checked_mul(core::mem::size_of::<Vec<PieceKind>>() as u128)?;
            for pattern in 0..universe.pattern_count() {
                pattern_bytes = pattern_bytes.checked_add(
                    (universe.sequence_at(pattern).len() as u128)
                        .checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
                )?;
            }
            future = future
                .checked_add(retained_graph_bytes)?
                .checked_add(pattern_bytes)?;
            future = future.max(scoring_peak_bytes);
        }
        Some(future)
    }

    fn ensure_memory_bound(&self, checked_future_bytes: u128) -> Result<(), WasmExactSearchError> {
        let observed = self.checked_retained_bytes().ok_or_else(|| {
            WasmExactSearchError::resource_admission(
                self.memory_bound
                    .ensure(u128::MAX, 1)
                    .expect_err("checked retained-byte overflow is unavailable"),
            )
        })?;
        let future = self
            .coexisting_retained_bytes
            .checked_add(checked_future_bytes)
            .ok_or_else(|| {
                WasmExactSearchError::resource_admission(
                    self.memory_bound
                        .ensure(u128::MAX, 1)
                        .expect_err("checked coexisting retained-byte overflow is unavailable"),
                )
            })?;
        self.memory_bound
            .ensure(observed, future)
            .map_err(WasmExactSearchError::resource_admission)
    }
}

fn checked_board64_canonical_key_len(identity: StandardBoard64TilingIdentity) -> Option<u128> {
    let placements = identity.placement_count() as u128;
    let separators = placements.saturating_sub(1);
    ("ctk1|initial=".len() as u128)
        .checked_add(16)?
        .checked_add("|placements=".len() as u128)?
        .checked_add(placements.checked_mul(18)?)?
        .checked_add(separators)
}

pub(super) fn checked_build_probability_fixed_result_surface_bytes() -> Option<u128> {
    const FIELD_COUNT_UPPER_BOUND: u128 = 128;
    const KEY_AND_VALUE_BYTES_PER_FIELD_UPPER_BOUND: u128 = 512;
    const EXECUTION_REPORT_STRING_BYTES_UPPER_BOUND: u128 = 4_096;

    // Build-probability summary keys are internal literals. Their dynamic
    // values are booleans, bounded integer/hex encodings, canonical
    // probabilities, or internal reason/profile identifiers. This fixed
    // checked envelope covers the result owner, every field slot, all field
    // string backing, and the six strings cloned by SearchExecutionReport.
    (core::mem::size_of::<CoreExecutionResult>() as u128)
        .checked_add(
            FIELD_COUNT_UPPER_BOUND.checked_mul(core::mem::size_of::<(String, String)>() as u128)?,
        )?
        .checked_add(
            FIELD_COUNT_UPPER_BOUND.checked_mul(KEY_AND_VALUE_BYTES_PER_FIELD_UPPER_BOUND)?,
        )?
        .checked_add(EXECUTION_REPORT_STRING_BYTES_UPPER_BOUND)
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

    pub(super) fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let mut bytes = core::mem::size_of_val(self.classes.classes()) as u128;
        for class in self.classes.classes() {
            bytes = bytes
                .checked_add(core::mem::size_of_val(class.queue()) as u128)?
                .checked_add(core::mem::size_of_val(class.pattern_ids()) as u128)?;
        }
        bytes =
            bytes
                .checked_add((self.languages.capacity() as u128).checked_mul(
                    core::mem::size_of::<(String, CostedGeometryLanguage)>() as u128,
                )?)?;
        for (key, language) in &self.languages {
            bytes = bytes
                .checked_add(key.capacity() as u128)?
                .checked_add(core::mem::size_of_val(language.nodes()) as u128)?;
            for node in language.nodes() {
                bytes = bytes.checked_add(core::mem::size_of_val(node.edges()) as u128)?;
            }
        }
        bytes = bytes.checked_add(core::mem::size_of_val(self.kick_profile.entries()) as u128)?;
        for entry in self.kick_profile.entries() {
            bytes = bytes
                .checked_add(core::mem::size_of_val(entry.sequence().offsets()) as u128)?
                .checked_add(entry.unsupported_reason().map_or(0, str::len) as u128)?;
        }
        Some(bytes)
    }

    pub(super) fn checked_fixed_creation_future_bytes(problem: &SearchProblem) -> Option<u128> {
        const GROUPING_ENTRY_OVERHEAD_UPPER_BOUND: u128 = 256;

        let universe = problem.piece_source().materialized_universe()?;
        let pattern_count = universe.pattern_count() as u128;
        let queue_len = (problem.exact_pieces().unwrap_or(0) as u128).checked_add(u128::from(
            problem.supply().projects_standard_bag_lookahead(),
        ))?;
        // QueueClassSet::group temporarily retains the source QueuePattern
        // owners, two queue-key owners, class-id staging, and the final class
        // owner. Count every one before finesse materialization starts.
        let queue_and_grouping = pattern_count.checked_mul(
            (core::mem::size_of::<QueuePattern>() as u128)
                .checked_add(core::mem::size_of::<QueueClass>() as u128)?
                .checked_add(core::mem::size_of::<PatternId>() as u128)?
                .checked_add(
                    queue_len
                        .checked_mul((core::mem::size_of::<PieceKind>() as u128).checked_mul(4)?)?,
                )?
                .checked_add(GROUPING_ENTRY_OVERHEAD_UPPER_BOUND)?,
        )?;

        let kick_profile =
            super::kick_profiles::builtin_kick_profile(problem.kick_profile().profile_id())?;
        let mut kick_bytes = core::mem::size_of_val(kick_profile.entries()) as u128;
        for entry in kick_profile.entries() {
            kick_bytes = kick_bytes
                .checked_add(core::mem::size_of_val(entry.sequence().offsets()) as u128)?
                .checked_add(entry.unsupported_reason().map_or(0, str::len) as u128)?;
        }
        queue_and_grouping.checked_add(kick_bytes)
    }
}

pub(super) fn attach_finesse_report_with_memory_guard(
    result: CoreExecutionResult,
    materials: Vec<FinesseSearchMaterial>,
    metric: FinesseMetric,
    pattern_knowledge: FinessePatternKnowledge,
    control: &ExecutionControl,
    mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), WasmExactSearchError>,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    let material_bytes = checked_finesse_material_vec_retained_bytes(&materials).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_finesse_report_memory_projection_overflow"),
    )?;
    let report_build_future =
        checked_finesse_report_build_future_upper_bound(&materials, pattern_knowledge).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_finesse_report_memory_projection_overflow"),
        )?;
    let borrowed_fields = [
        ("finesse_metric_requested", metric.as_str()),
        (
            "finesse_pattern_knowledge_requested",
            pattern_knowledge.as_str(),
        ),
    ];
    let field_projection = result
        .checked_borrowed_field_replacement_projection(&borrowed_fields)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_report_memory_projection_overflow",
        ))?;
    let initial_future = material_bytes
        .checked_add(report_build_future)
        .and_then(|bytes| bytes.checked_add(field_projection.required_future_bytes))
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_report_memory_projection_overflow",
        ))?;
    memory_guard(&result, initial_future)?;

    let report = build_finesse_report(materials, pattern_knowledge, control)?;
    let report_bytes =
        report
            .checked_nested_retained_bytes()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_report_memory_projection_overflow",
            ))?;
    if report_bytes > report_build_future {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_finesse_report_memory_projection_underestimated",
        ));
    }
    let fields = vec![
        (
            "finesse_metric_requested".to_owned(),
            metric.as_str().to_owned(),
        ),
        (
            "finesse_pattern_knowledge_requested".to_owned(),
            pattern_knowledge.as_str().to_owned(),
        ),
    ];
    let projection_error =
        WasmExactSearchError::InvalidProblem("wasm_finesse_report_memory_projection_overflow");
    let result = result
        .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
            memory_guard(
                live,
                future
                    .checked_add(report_bytes)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_finesse_report_memory_projection_overflow",
                    ))?,
            )
        })
        .map_err(|error| match error {
            crate::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow
            | crate::core_execution_result::CoreResultFieldReplacementError::AllocationFailed {
                ..
            } => projection_error,
            crate::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => {
                error
            }
        })?;
    Ok(result.with_finesse_report(report))
}

pub(super) fn checked_finesse_material_vec_retained_bytes(
    materials: &Vec<FinesseSearchMaterial>,
) -> Option<u128> {
    let mut bytes = (materials.capacity() as u128)
        .checked_mul(core::mem::size_of::<FinesseSearchMaterial>() as u128)?;
    for material in materials {
        bytes = bytes.checked_add(material.checked_nested_retained_bytes()?)?;
    }
    Some(bytes)
}

fn checked_finesse_report_build_future_upper_bound(
    materials: &[FinesseSearchMaterial],
    pattern_knowledge: FinessePatternKnowledge,
) -> Option<u128> {
    const TREE_ENTRY_OVERHEAD_UPPER_BOUND: u128 = 256;
    const PRODUCT_STATE_BYTES_UPPER_BOUND: u128 = 1_024;
    let first = materials.first()?;
    let mut language_count = 0_u128;
    let mut key_bytes = 0_u128;
    let mut max_key_bytes = 0_u128;
    let mut node_count = 0_u128;
    let mut edge_count = 0_u128;
    let mut max_depth = 0_u128;
    let mut max_edge_cost = 0_u128;
    let mut class_count = first.classes.classes().len() as u128;
    let mut pattern_count = first.classes.metadata().pattern_count as u128;
    let mut max_queue_len = first
        .classes
        .classes()
        .iter()
        .map(|class| class.queue().len() as u128)
        .max()
        .unwrap_or(0);
    for material in materials {
        class_count = class_count.max(material.classes.classes().len() as u128);
        pattern_count = pattern_count.max(material.classes.metadata().pattern_count as u128);
        max_queue_len = max_queue_len.max(
            material
                .classes
                .classes()
                .iter()
                .map(|class| class.queue().len() as u128)
                .max()
                .unwrap_or(0),
        );
        for (key, language) in &material.languages {
            language_count = language_count.checked_add(1)?;
            key_bytes = key_bytes.checked_add(key.len() as u128)?;
            max_key_bytes = max_key_bytes.max(key.len() as u128);
            node_count = node_count.checked_add(language.nodes().len() as u128)?;
            for node in language.nodes() {
                max_depth = max_depth.max(u128::from(node.depth()));
                edge_count = edge_count.checked_add(node.edges().len() as u128)?;
                for edge in node.edges() {
                    max_edge_cost = max_edge_cost.max(u128::from(edge.input_cost()));
                }
            }
        }
    }
    let effective_solution_count = language_count;
    let output_policy_count = match pattern_knowledge {
        FinessePatternKnowledge::Both => 2_u128,
        FinessePatternKnowledge::Oracle | FinessePatternKnowledge::VisibleSeven => 1_u128,
    };
    let evaluated_policy_count = if matches!(pattern_knowledge, FinessePatternKnowledge::Oracle) {
        1_u128
    } else {
        2_u128
    };

    let solution_average_bytes = effective_solution_count.checked_mul(
        (core::mem::size_of::<FinesseSolutionAverage>() as u128)
            .checked_add(MAX_CANONICAL_PROBABILITY_TEXT_BYTES)?,
    )?;
    let mut report_bytes = (core::mem::size_of::<FinessePolicyResult>() as u128)
        .checked_mul(output_policy_count)?
        .checked_add(solution_average_bytes.checked_mul(output_policy_count)?)?
        .checked_add(key_bytes.checked_mul(output_policy_count)?)?
        .checked_add(
            MAX_CANONICAL_PROBABILITY_TEXT_BYTES
                .checked_mul(8)?
                .checked_mul(output_policy_count)?,
        )?
        .checked_add(128)?;
    let route_inputs = max_depth.checked_mul(max_edge_cost.checked_add(1)?)?;
    report_bytes = report_bytes
        .checked_add(max_key_bytes)?
        .checked_add(pattern_count.checked_mul(core::mem::size_of::<usize>() as u128)?)?
        .checked_add(max_queue_len.checked_mul(core::mem::size_of::<PieceKind>() as u128)?)?
        .checked_add(route_inputs.checked_mul(core::mem::size_of::<FinesseReportInput>() as u128)?)?
        .checked_add(
            max_depth.checked_mul(core::mem::size_of::<crate::FinesseReportPlacement>() as u128)?,
        )?;

    let grouping_bytes = language_count.checked_mul(
        (core::mem::size_of::<(String, CostedGeometryLanguage)>() as u128)
            .checked_add(core::mem::size_of::<Vec<CostedGeometryLanguage>>() as u128)?
            .checked_add(TREE_ENTRY_OVERHEAD_UPPER_BOUND)?,
    )?;
    // `build_finesse_report` first unions alternatives that share one exact
    // solution key. Policy evaluation then unions the distinct solution
    // languages to calculate the overall adaptive cost. Any second source
    // language can therefore participate in one of those union stages.
    let language_union_required = language_count > 1;
    let (effective_node_count, union_peak) = if !language_union_required {
        (node_count, 0_u128)
    } else {
        let shift = u32::try_from(node_count).ok()?;
        let union_state_count = 1_u128.checked_shl(shift)?.checked_sub(1)?;
        let pair_bytes = node_count.checked_mul(
            (core::mem::size_of::<usize>() + core::mem::size_of::<GeometryNodeId>()) as u128,
        )?;
        let per_state = pair_bytes
            .checked_mul(3)?
            .checked_add(core::mem::size_of::<GeometryLanguageNode>() as u128)?
            .checked_add(
                edge_count.checked_mul(core::mem::size_of::<CostedGeometryEdge>() as u128)?,
            )?
            .checked_add(TREE_ENTRY_OVERHEAD_UPPER_BOUND.checked_mul(2)?)?;
        let retained = union_state_count.checked_mul(per_state)?;
        let current_group_scratch = edge_count.checked_mul(
            pair_bytes
                .checked_add(core::mem::size_of::<CostedGeometryEdge>() as u128)?
                .checked_add(TREE_ENTRY_OVERHEAD_UPPER_BOUND)?,
        )?;
        (
            union_state_count,
            retained.checked_add(current_group_scratch)?,
        )
    };

    let scalar_state_count = effective_node_count
        .checked_mul(max_queue_len.checked_add(1)?)?
        .checked_mul(8)?;
    let cost_vector_bytes = class_count
        .checked_mul(core::mem::size_of::<Option<u32>>() as u128)?
        .checked_add(core::mem::size_of::<QueueCostTable>() as u128)?;
    // An empty solution catalog never constructs a product evaluator:
    // `evaluate_finesse_policy` materializes only the one unreachable cost
    // table counted below. In particular, a large Visible-7 class set must not
    // be projected as `2^class_count` when there is no language to evaluate.
    let (oracle_product_peak, visible_product_peak) = if language_count == 0 {
        (0, 0)
    } else {
        let oracle = scalar_state_count
            .checked_mul(PRODUCT_STATE_BYTES_UPPER_BOUND.checked_add(cost_vector_bytes)?)?;
        let visible =
            if first.fixed_queue || matches!(pattern_knowledge, FinessePatternKnowledge::Oracle) {
                0
            } else {
                let shift = u32::try_from(class_count).ok()?;
                scalar_state_count
                    .checked_mul(1_u128.checked_shl(shift)?)?
                    .checked_mul(PRODUCT_STATE_BYTES_UPPER_BOUND.checked_add(cost_vector_bytes)?)?
            };
        (oracle, visible)
    };
    let retained_cost_tables = effective_solution_count
        .checked_add(1)?
        .checked_mul(cost_vector_bytes)?
        .checked_mul(evaluated_policy_count)?;
    let movement_peak =
        checked_finesse_movement_witness_peak_bytes(materials, route_inputs, max_depth)?;

    grouping_bytes
        .checked_add(union_peak)?
        .checked_add(oracle_product_peak.max(visible_product_peak))?
        .checked_add(retained_cost_tables)?
        .checked_add(report_bytes.checked_mul(2)?)?
        .checked_add(movement_peak)
}

fn checked_finesse_movement_witness_peak_bytes(
    materials: &[FinesseSearchMaterial],
    route_input_count: u128,
    placement_count: u128,
) -> Option<u128> {
    // `FrozenFinesseQuery` clones one kick profile and one target before each
    // selected movement replay. Count that clone from the concrete profile;
    // all placements are replayed sequentially, so only the largest BFS state
    // space coexists with the accumulated route output.
    let mut kick_clone_bytes = 0_u128;
    let mut max_state_count = 0_u128;
    for material in materials {
        let entries = material.kick_profile.entries();
        let mut material_kick_bytes = (entries.len() as u128)
            .checked_mul(core::mem::size_of::<clearra_rules::kicks::KickTableEntry>() as u128)?;
        for entry in entries {
            material_kick_bytes = material_kick_bytes
                .checked_add((entry.sequence().offsets().len() as u128).checked_mul(
                    core::mem::size_of::<clearra_rules::kicks::KickOffset>() as u128,
                )?)?
                .checked_add(entry.unsupported_reason().map_or(0, str::len) as u128)?;
        }
        kick_clone_bytes = kick_clone_bytes.max(material_kick_bytes);

        for (_, language) in &material.languages {
            for node in language.nodes() {
                let Some(board) = node.source_board() else {
                    continue;
                };
                for edge in node.edges().iter().copied() {
                    max_state_count = max_state_count.max(checked_finesse_movement_state_count(
                        board,
                        edge.piece(),
                        material.spawn_profile,
                        &material.kick_profile,
                    )?);
                }
            }
        }
    }

    let word = core::mem::size_of::<usize>() as u128;
    let rotation_arrival_field_bytes =
        (core::mem::size_of::<clearra_core_domain::piece::rotation::RotationState>() as u128)
            .checked_mul(2)?
            .checked_add(core::mem::size_of::<ClassicInputAction>() as u128)?
            .checked_add(core::mem::size_of::<u8>() as u128)?
            .checked_add((core::mem::size_of::<i8>() as u128).checked_mul(2)?)?
            .checked_add(core::mem::size_of::<PiecePose>() as u128)?;
    // Rust-layout padding and both Option discriminants are bounded by sixteen
    // pointer-alignment quanta for the concrete Parent field graph above.
    let option_parent_layout_upper_bound = rotation_arrival_field_bytes
        .checked_add(core::mem::size_of::<usize>() as u128)?
        .checked_add(core::mem::size_of::<ClassicInputAction>() as u128)?
        .checked_add(word.checked_mul(16)?)?;

    let bfs_arrays = max_state_count
        .checked_mul(core::mem::size_of::<u32>() as u128)?
        .checked_add(max_state_count.checked_mul(option_parent_layout_upper_bound)?)?
        // FIFO capacity grows geometrically and never needs more than the next
        // power of two above the number of reachable states.
        .checked_add(
            max_state_count
                .checked_mul(2)?
                .checked_mul(core::mem::size_of::<PiecePose>() as u128)?,
        )?;
    // `reconstruct_actions` may retain its geometrically grown Vec while
    // `into_boxed_slice` installs an exact-sized owner. Three state-sized
    // action arrays therefore bound that handoff peak.
    let reconstructed_actions = max_state_count
        .checked_mul(3)?
        .checked_mul(core::mem::size_of::<ClassicInputAction>() as u128)?;
    let accumulated_replay = route_input_count
        .checked_mul(2)?
        .checked_mul(core::mem::size_of::<FinesseSequenceInput>() as u128)?
        .checked_add(
            placement_count.checked_mul(core::mem::size_of::<GeometryActionKey>() as u128)?,
        )?
        .checked_add(
            route_input_count.checked_mul(core::mem::size_of::<FinesseReportInput>() as u128)?,
        )?
        .checked_add(
            placement_count
                .checked_mul(core::mem::size_of::<crate::FinesseReportPlacement>() as u128)?,
        )?;

    kick_clone_bytes
        // `FrozenFinesseQuery::new` copies the one-target input into a Vec and
        // converts it to boxed storage; both payloads can coexist in that
        // handoff under the allocation accounting contract.
        .checked_add((core::mem::size_of::<FinesseTarget>() as u128).checked_mul(2)?)?
        .checked_add(bfs_arrays)?
        .checked_add(reconstructed_actions)?
        .checked_add(accumulated_replay)
}

fn checked_finesse_movement_state_count(
    board: clearra_finesse::FinesseBoard,
    piece: PieceKind,
    spawn: SpawnProfile,
    kicks: &KickTableProfile,
) -> Option<u128> {
    // The movement crate normalizes I/JLSTZ rotation centers by at most four
    // rows. Adding that exact family-wide delta to the concrete i8 kick
    // offsets gives a conservative vertical margin without allocating a
    // normalized kick table.
    let vertical_margin = kicks
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .flat_map(|entry| entry.sequence().offsets())
        .map(|offset| u16::from(offset.dy().unsigned_abs()).saturating_add(4))
        .max()
        .unwrap_or(0);
    let height = i16::try_from(board.height()).ok()?;
    let ceiling = spawn
        .y()
        .max(height)
        .saturating_add(i16::try_from(vertical_margin).ok()?)
        .saturating_add(4);
    let non_negative_ceiling = u16::try_from(ceiling).ok()?;
    4_u128
        .checked_mul(u128::from(non_negative_ceiling).checked_add(1)?)?
        .checked_mul(u128::from(board.width()))
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

// This hot path receives precomputed policy surfaces separately to avoid cloning an aggregate.
#[allow(clippy::too_many_arguments)]
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
        let union;
        let language = if let [(_, language)] = languages {
            language
        } else {
            let references = languages
                .iter()
                .map(|(_, language)| language)
                .collect::<Vec<_>>();
            union = union_costed_geometry_languages(&references).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_finesse_overall_union_failed")
            })?;
            &union
        };
        let evaluator = QueueClassProductEvaluator::new(language)
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
    let mut merged = Vec::<SolutionCoverage>::new();
    for result in results {
        let incoming = result.solution_coverages();
        if merged.is_empty() {
            merged.try_reserve_exact(incoming.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_merge_storage_unavailable",
                )
            })?;
            merged.extend(incoming.iter().cloned());
            continue;
        }
        let current = core::mem::take(&mut merged);
        let mut next = Vec::new();
        next.try_reserve_exact(current.len().saturating_add(incoming.len()))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_solution_coverage_merge_storage_unavailable",
                )
            })?;
        let (mut left, mut right) = (0_usize, 0_usize);
        while left < current.len() || right < incoming.len() {
            match (current.get(left), incoming.get(right)) {
                (Some(existing), Some(candidate))
                    if existing.identity() == candidate.identity() =>
                {
                    let mut coverage = existing.covered_patterns().clone();
                    coverage
                        .union_with(candidate.covered_patterns())
                        .map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "wasm_build_probability_solution_coverage_merge_mismatch",
                            )
                        })?;
                    if coverage.pattern_count() != pattern_count {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_solution_coverage_merge_mismatch",
                        ));
                    }
                    next.push(SolutionCoverage::new(existing.identity(), coverage));
                    left += 1;
                    right += 1;
                }
                (Some(existing), Some(candidate)) if existing.identity() < candidate.identity() => {
                    next.push(existing.clone());
                    left += 1;
                }
                (Some(_), Some(candidate)) => {
                    next.push(candidate.clone());
                    right += 1;
                }
                (Some(existing), None) => {
                    next.push(existing.clone());
                    left += 1;
                }
                (None, Some(candidate)) => {
                    next.push(candidate.clone());
                    right += 1;
                }
                (None, None) => break,
            }
        }
        merged = next;
    }
    Ok(merged)
}

fn merge_normalized_solution_coverages(
    results: &[&CoreExecutionResult],
    pattern_count: usize,
) -> Result<Vec<NormalizedSolutionCoverage>, WasmExactSearchError> {
    let mut merged = Vec::<NormalizedSolutionCoverage>::new();
    for result in results {
        let incoming = result.normalized_solution_coverages();
        if merged.is_empty() {
            merged.try_reserve_exact(incoming.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_normalized_coverage_merge_storage_unavailable",
                )
            })?;
            merged.extend(incoming.iter().cloned());
            continue;
        }
        let current = core::mem::take(&mut merged);
        let mut next = Vec::new();
        next.try_reserve_exact(current.len().saturating_add(incoming.len()))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_build_probability_normalized_coverage_merge_storage_unavailable",
                )
            })?;
        let (mut left, mut right) = (0_usize, 0_usize);
        while left < current.len() || right < incoming.len() {
            match (current.get(left), incoming.get(right)) {
                (Some(existing), Some(candidate))
                    if existing.solution_key() == candidate.solution_key() =>
                {
                    let mut coverage = existing.covered_patterns().clone();
                    coverage
                        .union_with(candidate.covered_patterns())
                        .map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "wasm_build_probability_normalized_coverage_merge_mismatch",
                            )
                        })?;
                    if coverage.pattern_count() != pattern_count {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_build_probability_normalized_coverage_merge_mismatch",
                        ));
                    }
                    next.push(NormalizedSolutionCoverage::new(
                        existing.solution_key(),
                        coverage,
                    ));
                    left += 1;
                    right += 1;
                }
                (Some(existing), Some(candidate))
                    if existing.solution_key() < candidate.solution_key() =>
                {
                    next.push(existing.clone());
                    left += 1;
                }
                (Some(_), Some(candidate)) => {
                    next.push(candidate.clone());
                    right += 1;
                }
                (Some(existing), None) => {
                    next.push(existing.clone());
                    left += 1;
                }
                (None, Some(candidate)) => {
                    next.push(candidate.clone());
                    right += 1;
                }
                (None, None) => break,
            }
        }
        merged = next;
    }
    Ok(merged)
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod finesse_integration_tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        pc::pc_target::PcTarget,
        piece::rotation::RotationState,
        probability::probability_value::ProbabilityValue,
        solution::normalized_tiling_solution::PiecePlacementMask,
    };
    use clearra_finesse::FinesseBoard;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow,
    };
    use clearra_problem::{BuildProbabilityQuery, FinessePlacement, ProblemCompiler};
    use clearra_supply::queue::{
        fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
    };

    use super::*;
    use crate::backend::wasm_cpu::buildup::{PreparedFinesseEdge, PreparedFinesseNode};

    #[test]
    fn build_cooperative_progress_reports_monotonic_geometry_across_mirror_passes() {
        #[derive(Default)]
        struct Capture(
            std::sync::Mutex<Vec<clearra_core_domain::execution_cancellation::ExecutionProgress>>,
        );
        impl clearra_core_domain::execution_cancellation::ProgressSink for Capture {
            fn report(
                &self,
                progress: clearra_core_domain::execution_cancellation::ExecutionProgress,
            ) {
                self.0.lock().unwrap().push(progress);
            }
        }
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0f, 0, 0, 0])
            .unwrap()
            .with_horizontal_mirror_included(true);
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        )
        .unwrap();
        let capture = Arc::new(Capture::default());
        let control = ExecutionControl::default().with_progress_sink(capture.clone());
        let mut complete = false;
        for _ in 0..1_000 {
            if matches!(
                session.advance(1, &control).unwrap(),
                BuildProbabilityAdvance::Completed(_)
            ) {
                complete = true;
                break;
            }
        }
        assert!(complete, "the small two-pass fixture must complete");
        let captured = capture.0.lock().unwrap();
        for stage in ["build-geometry", "build-candidates", "build-verification"] {
            let values: Vec<_> = captured
                .iter()
                .filter(|progress| progress.stage == stage)
                .collect();
            assert!(
                values.len() > 1,
                "{stage} must be emitted before only the final result"
            );
            assert!(values.iter().all(|progress| progress.total.is_none()));
            assert!(values
                .windows(2)
                .all(|pair| pair[0].completed <= pair[1].completed));
        }
        let geometry: Vec<_> = captured
            .iter()
            .filter(|progress| progress.stage == "build-geometry")
            .collect();
        assert!(geometry.last().unwrap().completed > geometry.first().unwrap().completed);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn build_probability_six_line_descriptor_fails_before_compact_or_extended_allocation() {
        let problem =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::six_lines()))
                .expect("lazy six-line descriptor");
        let compact =
            BuildProbabilityField::from_words_preserving_height(6, [0; 4], [0; 4]).unwrap();
        let extended =
            BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0, 1, 0, 0]).unwrap();
        assert!(compact.is_compact());
        assert!(!extended.is_compact());

        for field in [compact, extended] {
            let error = match WasmBuildProbabilitySession::new(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Off,
            ) {
                Ok(_) => panic!("admission must precede catalog and dense bitset allocation"),
                Err(error) => error,
            };

            let WasmExactSearchError::ResourceAdmission(report) = error else {
                panic!("expected typed admission evidence, got {error:?}");
            };
            assert!(!report.execution_started());
            assert_eq!(
                report.execution_availability().descriptor_pattern_count(),
                Some(35_384_428_800)
            );
            assert_eq!(
                report.execution_availability().required_dense_bytes(),
                Some(4_423_053_600)
            );
            assert_eq!(
                report.execution_availability().required_memory_bytes(),
                Some(4_423_053_600)
            );
            assert_eq!(
                report.execution_availability().reason(),
                Some(
                    clearra_core_domain::resource::ExecutionAvailabilityReason::DensePatternRepresentationUnavailable,
                )
            );
        }
    }

    #[test]
    fn finesse_score_tiny_budget_fails_before_queue_class_vector_materialization() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_max_memory_mib(Some(0)));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4]).unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        )])
        .unwrap();
        let error = match WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::Both,
                request: score,
            },
        ) {
            Ok(_) => panic!("zero-byte request budget must fail before score allocation"),
            Err(error) => error,
        };
        let WasmExactSearchError::ResourceAdmission(report) = error else {
            panic!("expected typed admission evidence, got {error:?}");
        };
        assert_eq!(
            report.execution_availability().reason(),
            Some(clearra_core_domain::resource::ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
        assert!(!report.execution_started());
    }

    #[test]
    fn compact_distributed_candidate_rows_have_an_exact_allocator_capacity_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("compact test problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = CompactBuildProbabilitySession::new_external_geometry_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("compact external session");
        let coexisting = 257_u128;
        session.set_coexisting_retained_bytes(coexisting);
        let source = [3_u32, 7, 11, 19, 23];
        let rows = session
            .try_copy_distributed_candidate_row_ids(&source)
            .expect("unbounded candidate row copy");
        let actual_row_bytes = (rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .expect("checked candidate row capacity");
        let required = session
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(coexisting))
            .and_then(|bytes| bytes.checked_add(actual_row_bytes))
            .expect("checked candidate row peak");
        drop(rows);

        session.set_memory_bound_for_test(unbounded.with_cap(required).expect("exact bound"));
        assert_eq!(
            session
                .try_copy_distributed_candidate_row_ids(&source)
                .expect("the exact candidate row peak must fit"),
            source
        );
        session.set_memory_bound_for_test(
            unbounded
                .with_cap(required - 1)
                .expect("one-byte-short bound"),
        );
        assert!(matches!(
            session.try_copy_distributed_candidate_row_ids(&source),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
    }

    #[test]
    fn compact_retained_bytes_count_owned_problem_nested_heap_once() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("compact problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("compact field");
        let session = CompactBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
        )
        .expect("compact session");
        let source_pointee = problem
            .checked_build_probability_pointee_retained_bytes()
            .expect("typed BuildProbability problem");
        let source_nested = source_pointee
            .checked_sub(core::mem::size_of::<SearchProblem>() as u128)
            .expect("pointee includes its inline owner");
        let owned_pointee = session
            .problem
            .checked_build_probability_pointee_retained_bytes()
            .expect("owned typed BuildProbability problem");
        let owned_nested = owned_pointee
            .checked_sub(core::mem::size_of::<SearchProblem>() as u128)
            .expect("owned pointee includes its inline owner");

        assert!(source_nested > 0);
        assert!(owned_nested > 0);
        assert_eq!(
            session.checked_retained_bytes(),
            session
                .checked_non_problem_retained_bytes()
                .and_then(|bytes| bytes.checked_add(owned_nested))
        );
        assert_eq!(
            checked_build_probability_problem_nested_retained_bytes(&session.problem),
            Some(owned_nested)
        );
        assert_eq!(
            checked_build_probability_problem_nested_retained_bytes(&problem),
            Some(source_nested)
        );
    }

    #[test]
    fn pending_finesse_score_retained_bytes_count_problem_and_placements_once() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("score problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("score field");
        let mut placements = Vec::with_capacity(12);
        placements.push(FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        ));
        let request = FinesseScoreRequest::new(placements).expect("score request");
        let session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::Both,
                request,
            },
        )
        .expect("score session");
        let pending_score = session.finesse_score.as_ref().expect("pending score owner");
        let problem_nested = pending_score
            .problem
            .checked_build_probability_pointee_retained_bytes()
            .and_then(|bytes| bytes.checked_sub(core::mem::size_of::<SearchProblem>() as u128))
            .expect("pending problem pointee includes its inline owner");
        let placement_backing = pending_score
            .request
            .checked_retained_capacity_bytes()
            .expect("placement backing fits u128");
        let pending_nested = problem_nested
            .checked_add(placement_backing)
            .expect("pending nested owners fit u128");
        let expected = (session.pending.capacity() as u128)
            .checked_mul(core::mem::size_of::<BuildProbabilitySessionKind>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (session.completed.capacity() as u128)
                        .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (session.finesse_search_materials.capacity() as u128)
                        .checked_mul(core::mem::size_of::<FinesseSearchMaterial>() as u128)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(session.pattern_weights.checked_storage_retained_bytes()?)
            })
            .and_then(|bytes| bytes.checked_add(problem_nested))
            .and_then(|bytes| bytes.checked_add(placement_backing));

        assert!(session.pending.is_empty());
        assert!(session.completed.is_empty());
        assert!(session.finesse_search_materials.is_empty());
        assert!(problem_nested > 0);
        assert!(placement_backing > 0);
        assert_eq!(
            pending_score.checked_nested_retained_bytes(),
            Some(pending_nested)
        );
        assert_eq!(session.checked_retained_bytes(), expected);
        assert_eq!(session.checked_front_coexisting_retained_bytes(), expected);
    }

    #[test]
    fn symmetry_merge_emits_a_complete_explicit_solution_contract() {
        let input = symmetry_input(Some("false"), false);

        let merged = merge_symmetry_results(
            vec![input],
            false,
            false,
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            false,
        )
        .expect("symmetry merge");
        let availability = merged.execution_report().solution_set_availability();

        assert_eq!(merged.field("search_output_policy"), Some("summary"));
        assert!(availability.contract_valid());
        assert!(availability.solution_set_materialized());
        assert!(
            availability.materialized_key_count_matches(merged.normalized_solution_keys().len())
        );
    }

    #[test]
    fn symmetry_single_pass_reuses_equal_coverage_without_losing_solutions() {
        for mirror_included in [false, true] {
            let merged = merge_symmetry_results(
                vec![symmetry_input(Some("false"), false)],
                mirror_included,
                false,
                &WeightedPatternSet::uniform(1).expect("uniform weights"),
                false,
            )
            .expect("single pass exact aggregation");
            assert_eq!(
                merged.field("original_coverage_probability"),
                merged.field("coverage_probability")
            );
            assert_eq!(merged.field("original_covered_pattern_count"), Some("1"));
            assert_eq!(merged.field("probability_complete"), Some("true"));
            assert_eq!(merged.normalized_solution_keys(), &["solution".to_owned()]);
            if mirror_included {
                assert_eq!(
                    merged.field("mirror_coverage_probability"),
                    merged.field("original_coverage_probability")
                );
                assert_eq!(merged.field("mirror_union_added_pattern_count"), Some("0"));
            }
        }
    }

    fn symmetry_input(
        solution_probabilities_requested: Option<&str>,
        resource_truncated: bool,
    ) -> CoreExecutionResult {
        let mut fields = vec![
            ("coverage_pattern_count".to_owned(), "1".to_owned()),
            ("piece_source_id".to_owned(), "1".to_owned()),
            ("pattern_universe_id".to_owned(), "2".to_owned()),
            ("pattern_weight_model_id".to_owned(), "3".to_owned()),
            (
                "coverage_aggregation_source_row_count".to_owned(),
                "1".to_owned(),
            ),
            (
                "build_probability_aggregation".to_owned(),
                "buildability".to_owned(),
            ),
            ("unique_solution_count".to_owned(), "1".to_owned()),
            ("probability_complete".to_owned(), "true".to_owned()),
            ("count_complete".to_owned(), "true".to_owned()),
            (
                "resource_truncated".to_owned(),
                resource_truncated.to_string(),
            ),
            (
                "resource_truncation_reason".to_owned(),
                if resource_truncated {
                    "test-truncation"
                } else {
                    "none"
                }
                .to_owned(),
            ),
            ("board_storage".to_owned(), "board256-canonical".to_owned()),
        ];
        if let Some(requested) = solution_probabilities_requested {
            fields.push((
                "solution_probabilities_requested".to_owned(),
                requested.to_owned(),
            ));
        }
        CoreExecutionResult::new(fields, Vec::new())
            .with_coverage_pattern_words(vec![1])
            .with_normalized_solution_keys(vec!["solution".to_owned()])
            .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                "solution",
                PatternBitSet::all(1),
            )])
    }

    fn invalid_problem_reason(error: WasmExactSearchError) -> &'static str {
        match error {
            WasmExactSearchError::InvalidProblem(reason) => reason,
            other => panic!("expected invalid problem, got {other:?}"),
        }
    }

    #[test]
    fn symmetry_merge_rejects_missing_duplicate_non_bool_and_mismatched_probability_policy() {
        let weights = WeightedPatternSet::uniform(1).expect("uniform weights");

        let missing = merge_symmetry_results(
            vec![symmetry_input(None, false)],
            false,
            false,
            &weights,
            false,
        )
        .expect_err("missing request policy must fail closed");
        assert_eq!(
            invalid_problem_reason(missing),
            "wasm_build_solution_probability_policy_missing"
        );

        let duplicate = symmetry_input(Some("false"), false).with_additional_fields(vec![(
            "solution_probabilities_requested".to_owned(),
            "false".to_owned(),
        )]);
        let duplicate = merge_symmetry_results(vec![duplicate], false, false, &weights, false)
            .expect_err("duplicate request policy must fail closed");
        assert_eq!(
            invalid_problem_reason(duplicate),
            "wasm_build_solution_probability_policy_duplicate"
        );

        let non_bool = merge_symmetry_results(
            vec![symmetry_input(Some("include"), false)],
            false,
            false,
            &weights,
            false,
        )
        .expect_err("non-boolean request policy must fail closed");
        assert_eq!(
            invalid_problem_reason(non_bool),
            "wasm_build_solution_probability_policy_invalid"
        );

        let mismatch = merge_symmetry_results(
            vec![
                symmetry_input(Some("false"), false),
                symmetry_input(Some("true"), false),
            ],
            true,
            true,
            &weights,
            false,
        )
        .expect_err("symmetry passes with different policies must fail closed");
        assert_eq!(
            invalid_problem_reason(mismatch),
            "wasm_build_solution_probability_policy_mismatch"
        );
    }

    #[test]
    fn symmetry_merge_requires_one_canonical_equal_pattern_count_per_pass() {
        let weights = WeightedPatternSet::uniform(2).expect("uniform weights");
        for results in [
            vec![
                symmetry_input(Some("false"), false).with_additional_fields(vec![(
                    "coverage_pattern_count".to_owned(),
                    "1".to_owned(),
                )]),
            ],
            vec![
                symmetry_input(Some("false"), false),
                symmetry_input(Some("false"), false).with_additional_fields(vec![(
                    "coverage_pattern_count".to_owned(),
                    "1".to_owned(),
                )]),
            ],
            vec![
                symmetry_input(Some("false"), false).with_replaced_fields(vec![(
                    "coverage_pattern_count".to_owned(),
                    "01".to_owned(),
                )]),
            ],
        ] {
            let mirror = results.len() == 2;
            let error = merge_symmetry_results(results, mirror, mirror, &weights, false)
                .expect_err("duplicate and noncanonical counts must fail closed");
            assert_eq!(
                invalid_problem_reason(error),
                "wasm_build_probability_symmetry_pattern_count_invalid"
            );
        }

        let mismatch = merge_symmetry_results(
            vec![
                symmetry_input(Some("false"), false),
                symmetry_input(Some("false"), false).with_replaced_fields(vec![(
                    "coverage_pattern_count".to_owned(),
                    "2".to_owned(),
                )]),
            ],
            true,
            true,
            &weights,
            false,
        )
        .expect_err("different canonical counts must fail closed");
        assert_eq!(
            invalid_problem_reason(mismatch),
            "wasm_build_probability_symmetry_pattern_count_mismatch"
        );
    }

    #[test]
    fn symmetry_merge_rejects_a_foreign_pattern_weight_authority() {
        let weights = WeightedPatternSet::uniform(1).expect("uniform weights");
        let foreign = symmetry_input(Some("false"), false)
            .with_replaced_fields(vec![("pattern_weight_model_id".to_owned(), "4".to_owned())]);

        let error = merge_symmetry_results(
            vec![symmetry_input(Some("false"), false), foreign],
            true,
            true,
            &weights,
            false,
        )
        .expect_err("foreign pattern weights must not enter the symmetry OR-union");

        assert_eq!(
            invalid_problem_reason(error),
            "wasm_build_probability_symmetry_pattern_weight_model_id_mismatch"
        );
    }

    #[test]
    fn symmetry_merge_memory_guard_rejects_oversized_solution_surface_before_merge() {
        let oversized_key = "solution".repeat(32_768);
        let input = symmetry_input(Some("true"), false)
            .with_normalized_solution_keys(vec![oversized_key.clone()])
            .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                oversized_key,
                PatternBitSet::all(1),
            )]);
        let mut guard_called = false;
        let error = merge_symmetry_results_with_memory_guard(
            vec![input],
            false,
            false,
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            false,
            |source_bytes, future_bytes| {
                guard_called = true;
                assert!(source_bytes > 32_768);
                assert!(future_bytes > 32_768);
                Err(WasmExactSearchError::InvalidProblem(
                    "test_symmetry_memory_cap",
                ))
            },
        )
        .expect_err("oversized merge must stop at the preallocation guard");
        assert!(guard_called);
        assert_eq!(invalid_problem_reason(error), "test_symmetry_memory_cap");
    }

    #[test]
    fn canonical_probability_text_projection_covers_the_binary64_domain() {
        let smallest_positive_text = f64::from_bits(1).to_string();
        assert!(smallest_positive_text.len() > 32);
        assert!((smallest_positive_text.len() as u128) <= MAX_CANONICAL_PROBABILITY_TEXT_BYTES);
    }

    #[test]
    fn worker_coverage_equality_does_not_materialize_sparse_dense_caches() {
        let mut dense = PatternBitSet::new(1_024);
        dense
            .insert(PatternId::new(7))
            .expect("the test pattern belongs to the dense set");
        let sparse = PatternBitSet::from_pattern_indices(1_024, vec![7])
            .expect("the test pattern belongs to the sparse set");
        let sparse_component_count = sparse.storage_component_count();
        let sparse_retained_bytes = sparse
            .checked_storage_retained_bytes()
            .expect("checked sparse storage");
        assert_eq!(sparse_component_count, 2);

        assert!(dense == sparse);
        assert!(sparse == dense);
        assert_eq!(sparse.storage_component_count(), sparse_component_count);
        assert_eq!(
            sparse.checked_storage_retained_bytes(),
            Some(sparse_retained_bytes)
        );
    }

    #[test]
    fn compact_result_materialization_projection_has_an_exact_one_byte_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty-target problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("empty compact field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = CompactBuildProbabilitySession::new_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("compact session");
        let required = session
            .checked_retained_bytes()
            .and_then(|retained| {
                session
                    .checked_result_materialization_future_bytes()
                    .and_then(|future| retained.checked_add(future))
            })
            .expect("checked materialization projection");
        session.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short bound");
        assert!(matches!(
            session.ensure_result_materialization_bound(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        session.memory_bound = unbounded.with_cap(required).expect("exact bound");
        session
            .ensure_result_materialization_bound()
            .expect("exact projection fits");
    }

    #[test]
    fn compact_scoring_projection_counts_identity_and_graph_owners_at_the_exact_cap() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("one-I scoring problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-I field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = CompactBuildProbabilitySession::new_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("compact session");
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("canonical one-I identity");
        session.buildable_tilings.insert(identity);

        let identity_bytes = core::mem::size_of::<StandardBoard64TilingIdentity>() as u128;
        let graph_slots = core::mem::size_of::<ExactScoringExecutionGraph>() as u128;
        let graph_peak =
            exact_scoring_execution_graph_memory_projection(&problem, &session.catalog, identity)
                .expect("checked scoring projection")
                .peak_additional_bytes;
        let future = session
            .checked_result_materialization_future_bytes()
            .expect("checked result projection");
        assert!(future >= identity_bytes + graph_slots + graph_peak);

        let required = session
            .checked_retained_bytes()
            .and_then(|retained| retained.checked_add(future))
            .expect("checked exact cap");
        session.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short bound");
        assert!(matches!(
            session.ensure_result_materialization_bound(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        session.memory_bound = unbounded.with_cap(required).expect("exact bound");
        session
            .ensure_result_materialization_bound()
            .expect("exact projection fits");
    }

    #[test]
    fn serial_public_result_guard_counts_aggregate_storage_at_the_exact_cap() {
        fn session() -> WasmBuildProbabilitySession {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(Vec::new())),
                PieceWindow::new(0),
            )
            .with_exact_pieces(Some(0));
            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty problem");
            let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
                .expect("empty compact field");
            WasmBuildProbabilitySession::new(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Off,
            )
            .expect("serial aggregate")
        }

        fn required(
            session: &WasmBuildProbabilitySession,
            result: &CoreExecutionResult,
            future: u128,
        ) -> u128 {
            session
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(checked_public_result_bytes(result)?))
                .and_then(|bytes| bytes.checked_add(future))
                .expect("checked aggregate/public-result coexistence")
        }

        let result = symmetry_input(Some("true"), false);
        let future = 17_u128;
        let mut one_byte_short = session();
        let short_cap = required(&one_byte_short, &result, future) - 1;
        one_byte_short._execution_admission = one_byte_short
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(short_cap)
            .expect("one-byte-short delegated authority");
        assert!(matches!(
            one_byte_short.validate_public_result_memory_with_future(&result, future),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        drop(one_byte_short);

        let mut exact = session();
        let exact_cap = required(&exact, &result, future);
        exact._execution_admission = exact
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(exact_cap)
            .expect("exact delegated authority");
        exact
            .validate_public_result_memory_with_future(&result, future)
            .expect("aggregate plus result and future fit the exact cap");
    }

    #[test]
    fn finite_public_result_guard_counts_external_owner_and_only_carrier_delta() {
        fn session(
            external_retained_owner_bytes: u128,
            returned_carrier_bytes: u128,
        ) -> WasmBuildProbabilitySession {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(Vec::new())),
                PieceWindow::new(0),
            )
            .with_exact_pieces(Some(0));
            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty problem");
            let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
                .expect("empty compact field");
            WasmBuildProbabilitySession::new_finite(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Off,
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .expect("finite serial aggregate")
        }

        let result = symmetry_input(Some("true"), false);
        let external_retained_owner_bytes = 43_u128;
        let carrier_delta = 71_u128;
        let returned_carrier_bytes =
            core::mem::size_of::<CoreExecutionResult>() as u128 + carrier_delta;
        let checked_future_bytes = 17_u128;
        let required = |session: &WasmBuildProbabilitySession| {
            session
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(external_retained_owner_bytes))
                .and_then(|bytes| bytes.checked_add(checked_public_result_bytes(&result)?))
                .and_then(|bytes| bytes.checked_add(checked_future_bytes))
                .and_then(|bytes| bytes.checked_add(carrier_delta))
                .expect("checked finite terminal coexistence")
        };

        assert_eq!(
            returned_carrier_delta_bytes(returned_carrier_bytes),
            carrier_delta
        );
        assert_eq!(
            returned_carrier_delta_bytes(core::mem::size_of::<CoreExecutionResult>() as u128 - 1),
            0
        );

        let mut one_byte_short = session(external_retained_owner_bytes, returned_carrier_bytes);
        let short_cap = required(&one_byte_short) - 1;
        one_byte_short._execution_admission = one_byte_short
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(short_cap)
            .expect("one-byte-short delegated authority");
        assert!(matches!(
            one_byte_short.validate_public_result_memory_with_future(&result, checked_future_bytes),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        drop(one_byte_short);

        let mut exact = session(external_retained_owner_bytes, returned_carrier_bytes);
        let exact_cap = required(&exact);
        exact._execution_admission = exact
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(exact_cap)
            .expect("exact delegated authority");
        exact
            .validate_public_result_memory_with_future(&result, checked_future_bytes)
            .expect("external owner, public result, future, and carrier delta fit exact cap");
    }

    #[test]
    fn finite_noncompleted_return_guard_counts_the_full_carrier() {
        fn session(
            external_retained_owner_bytes: u128,
            returned_carrier_bytes: u128,
        ) -> WasmBuildProbabilitySession {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(Vec::new())),
                PieceWindow::new(0),
            )
            .with_exact_pieces(Some(0));
            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty problem");
            let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
                .expect("empty compact field");
            WasmBuildProbabilitySession::new_finite(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Off,
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .expect("finite serial aggregate")
        }

        let external_retained_owner_bytes = 47_u128;
        let returned_carrier_bytes = 83_u128;
        let required = |session: &WasmBuildProbabilitySession| {
            session
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(external_retained_owner_bytes))
                .and_then(|bytes| bytes.checked_add(returned_carrier_bytes))
                .expect("checked finite noncompleted return peak")
        };
        let mut one_byte_short = session(external_retained_owner_bytes, returned_carrier_bytes);
        let short_cap = required(&one_byte_short) - 1;
        one_byte_short._execution_admission = one_byte_short
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(short_cap)
            .expect("one-byte-short delegated authority");
        assert!(matches!(
            one_byte_short.validate_finite_noncompleted_return_memory(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        drop(one_byte_short);

        let mut exact = session(external_retained_owner_bytes, returned_carrier_bytes);
        let exact_cap = required(&exact);
        exact._execution_admission = exact
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(exact_cap)
            .expect("exact delegated authority");
        exact
            .validate_finite_noncompleted_return_memory()
            .expect("external owner and full returned carrier fit exact cap");
    }

    #[test]
    fn finite_one_byte_short_replacement_carrier_rejects_before_work_and_preserves_authority() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(Vec::new())),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("empty compact field");
        let mut session = WasmBuildProbabilitySession::new_finite(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
            11,
            13,
        )
        .expect("finite serial aggregate");

        let replacement_external_bytes = 47_u128;
        let replacement_carrier_bytes = 83_u128;
        let exact_required = session
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(replacement_external_bytes))
            .and_then(|bytes| bytes.checked_add(replacement_carrier_bytes))
            .expect("checked replacement-carrier requirement");
        session._execution_admission = session
            ._execution_admission
            .try_delegate_compute_only_with_memory_cap(exact_required - 1)
            .expect("one-byte-short delegated authority");

        let previous_caller_memory = session.caller_memory;
        let previous_pending_len = session.pending.len();
        let previous_completed_len = session.completed.len();
        let previous_finished = session.finished;
        let previous_retained_bytes = session.checked_retained_bytes();

        assert!(matches!(
            session.advance_finite(
                usize::MAX,
                &ExecutionControl::default(),
                replacement_external_bytes,
                replacement_carrier_bytes,
            ),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert_eq!(session.caller_memory, previous_caller_memory);
        assert_eq!(session.pending.len(), previous_pending_len);
        assert_eq!(session.completed.len(), previous_completed_len);
        assert_eq!(session.finished, previous_finished);
        assert_eq!(session.checked_retained_bytes(), previous_retained_bytes);
    }

    #[test]
    fn finite_compact_initial_peak_includes_external_owner_and_pending_backing() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(Vec::new())),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("empty problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("empty compact field");
        let external_retained_owner_bytes = 313_u128;
        let returned_carrier_bytes = core::mem::size_of::<CoreExecutionResult>() as u128 + 29;
        let session = WasmBuildProbabilitySession::new_finite(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
            external_retained_owner_bytes,
            returned_carrier_bytes,
        )
        .expect("finite compact session");
        let expected_coexisting = external_retained_owner_bytes
            + session.pending.capacity() as u128
                * core::mem::size_of::<BuildProbabilitySessionKind>() as u128;
        let BuildProbabilitySessionKind::Compact(compact) = &session.pending[0] else {
            panic!("compact field must create a compact session");
        };

        assert_eq!(compact.coexisting_retained_bytes, expected_coexisting);
    }

    #[test]
    fn finite_caller_memory_projection_overflow_fails_closed() {
        assert!(matches!(
            BuildProbabilityCallerMemory::finite(
                u128::MAX,
                core::mem::size_of::<CoreExecutionResult>() as u128 + 1,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_probability_caller_memory_projection_overflow"
            ))
        ));
    }

    #[test]
    fn compact_symbolic_coverage_finalization_has_an_exact_one_byte_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(0),
        )
        .with_exact_pieces(Some(0))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("standard-bag problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0; 4])
            .expect("empty compact field");
        let unbounded = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority");
        let mut session = CompactBuildProbabilitySession::new_with_memory_bound(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            unbounded,
        )
        .expect("compact session");
        session.candidate_count = 1;
        let future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            session.covered_patterns.pattern_count(),
        )
        .expect("checked symbolic finalization projection");
        let required = session
            .checked_retained_bytes()
            .and_then(|retained| retained.checked_add(future))
            .expect("checked exact cap");
        session.memory_bound = unbounded
            .with_cap(required - 1)
            .expect("one-byte-short bound");
        assert!(matches!(
            session.ensure_symbolic_coverage_finalization_bound(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        session.memory_bound = unbounded.with_cap(required).expect("exact bound");
        session
            .ensure_symbolic_coverage_finalization_bound()
            .expect("exact projection fits");
    }

    #[test]
    fn mirror_second_pass_guard_counts_completed_first_result_bytes() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("mirror problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0f, 0, 0, 0])
            .expect("asymmetric compact target")
            .with_horizontal_mirror_included(true);
        let mut session = WasmBuildProbabilitySession::new(
            &problem,
            field,
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        )
        .expect("two-pass session");
        while session.completed.is_empty() {
            assert!(matches!(
                session
                    .advance(1_024, &ExecutionControl::default())
                    .expect("first pass advance"),
                BuildProbabilityAdvance::Pending
            ));
        }
        assert_eq!(session.completed.len(), 1);
        assert_eq!(session.pending.len(), 1);
        let coexisting = session
            .checked_front_coexisting_retained_bytes()
            .expect("completed-result coexistence");
        let active_retained = session.pending[0]
            .checked_retained_bytes()
            .expect("active retained bytes");
        let one_byte_short = active_retained
            .checked_add(coexisting)
            .and_then(|bytes| bytes.checked_sub(1))
            .expect("one-byte-short active cap");
        let bound = ExecutionMemoryBound::unbounded_for_problem(&problem)
            .expect("unbounded test authority")
            .with_cap(one_byte_short)
            .expect("bounded second pass");
        match &mut session.pending[0] {
            BuildProbabilitySessionKind::Compact(active) => active.set_memory_bound_for_test(bound),
            BuildProbabilitySessionKind::Extended(_) => panic!("compact mirror expected"),
        }
        assert!(matches!(
            session.advance(1, &ExecutionControl::default()),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
    }

    #[test]
    fn requested_solution_probability_completeness_requires_nontruncated_source() {
        let merged = merge_symmetry_results(
            vec![symmetry_input(Some("true"), true)],
            false,
            false,
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            false,
        )
        .expect("a well-shaped truncated result remains an incomplete result");

        assert_eq!(merged.bool_field("probability_complete"), Some(true));
        assert_eq!(merged.bool_field("count_complete"), Some(true));
        assert_eq!(merged.bool_field("resource_truncated"), Some(true));
        assert_eq!(
            merged.bool_field("solution_probability_complete"),
            Some(false)
        );
        assert_eq!(
            merged.field("solution_probability_incomplete_reason"),
            Some("resource-truncated")
        );
        assert_eq!(merged.solution_probabilities().len(), 1);
        assert!(!merged.solution_probabilities()[0].probability_complete());
    }

    #[test]
    fn requested_page_store_materializes_every_key_beyond_the_initial_page() {
        let mut identities = Vec::new();
        'identities: for left in 2..64_u32 {
            for right in left + 1..64_u32 {
                let cells = 0b11_u64 | (1_u64 << left) | (1_u64 << right);
                identities.push(
                    StandardBoard64TilingIdentity::from_placements(
                        0,
                        [PiecePlacementMask::new(PieceKind::I, cells)],
                    )
                    .expect("syntactically valid paging identity"),
                );
                if identities.len() == 101 {
                    break 'identities;
                }
            }
        }
        identities.sort_unstable();
        identities.dedup();
        assert_eq!(identities.len(), 101);
        let keys = identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();
        let coverage = keys
            .iter()
            .cloned()
            .map(|key| NormalizedSolutionCoverage::new(key, PatternBitSet::all(1)))
            .collect::<Vec<_>>();
        let store = Arc::new(
            TilingSolutionPageStore::from_standard_identities(0, identities.clone())
                .expect("canonical paging store"),
        );
        let input = CoreExecutionResult::new(
            vec![
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("piece_source_id".to_owned(), "1".to_owned()),
                ("pattern_universe_id".to_owned(), "2".to_owned()),
                ("pattern_weight_model_id".to_owned(), "3".to_owned()),
                (
                    "coverage_aggregation_source_row_count".to_owned(),
                    "101".to_owned(),
                ),
                (
                    "build_probability_aggregation".to_owned(),
                    "buildability".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "101".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("resource_truncated".to_owned(), "false".to_owned()),
                ("resource_truncation_reason".to_owned(), "none".to_owned()),
                (
                    "solution_probabilities_requested".to_owned(),
                    "true".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_coverage_pattern_words(vec![1])
        .with_normalized_solution_keys(keys[..100].to_vec())
        .with_normalized_solution_identities(identities[..100].to_vec())
        .with_normalized_solution_coverages(coverage)
        .with_tiling_solution_page_store(store);

        let merged = merge_symmetry_results(
            vec![input],
            false,
            false,
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            false,
        )
        .expect("complete paging merge");

        assert_eq!(merged.normalized_solution_keys(), keys);
        assert_eq!(merged.solution_probabilities().len(), 101);
        assert_eq!(merged.usize_field("solution_probability_count"), Some(101));
        assert_eq!(merged.bool_field("solution_keys_complete"), Some(true));
        assert_eq!(
            merged.bool_field("solution_probability_complete"),
            Some(true)
        );
    }

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
    fn finesse_report_attachment_guards_material_report_and_fields_at_one_byte_boundary() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("finesse problem");
        let material = || {
            FinesseSearchMaterial::new(
                &problem,
                vec![(
                    "solution".to_owned(),
                    costed_finesse_language(&one_piece_language(PieceKind::I, 3))
                        .expect("costed language"),
                )],
                true,
            )
            .expect("finesse material")
        };

        let mut required = 0_u128;
        let attached = attach_finesse_report_with_memory_guard(
            CoreExecutionResult::default(),
            vec![material()],
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            &ExecutionControl::default(),
            |live, future| {
                let peak = checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "test_finesse_projection_overflow",
                    ))?;
                required = required.max(peak);
                Ok(())
            },
        )
        .expect("unbounded guarded attachment");
        assert!(attached.finesse_report().is_some());

        attach_finesse_report_with_memory_guard(
            CoreExecutionResult::default(),
            vec![material()],
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            &ExecutionControl::default(),
            |live, future| {
                let peak = checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "test_finesse_projection_overflow",
                    ))?;
                if peak <= required {
                    Ok(())
                } else {
                    Err(WasmExactSearchError::InvalidProblem(
                        "test_finesse_memory_cap",
                    ))
                }
            },
        )
        .expect("the recorded exact guarded attachment peak is sufficient");

        let mut guard_calls = 0;
        let error = attach_finesse_report_with_memory_guard(
            CoreExecutionResult::default(),
            vec![material()],
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            &ExecutionControl::default(),
            |live, future| {
                guard_calls += 1;
                let peak = checked_public_result_bytes(live)
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "test_finesse_projection_overflow",
                    ))?;
                if peak < required {
                    Ok(())
                } else {
                    Err(WasmExactSearchError::InvalidProblem(
                        "test_finesse_memory_cap",
                    ))
                }
            },
        )
        .expect_err("one byte below the guarded attachment peak must fail closed");
        assert_eq!(error.reason(), "test_finesse_memory_cap");
        assert_eq!(
            guard_calls, 1,
            "the oversized attachment must stop at the pre-build guard"
        );
    }

    #[test]
    fn finesse_projection_skips_product_state_space_for_an_empty_solution_catalog() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("finesse problem");
        let pieces = [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ];
        let patterns = (0..128)
            .map(|index| {
                QueuePattern::new(
                    PatternId::new(index),
                    vec![
                        pieces[index % pieces.len()],
                        pieces[(index / pieces.len()) % pieces.len()],
                        pieces[(index / (pieces.len() * pieces.len())) % pieces.len()],
                    ],
                    probability(1.0 / 128.0),
                )
            })
            .collect::<Vec<_>>();
        let mut material =
            FinesseSearchMaterial::new(&problem, Vec::new(), true).expect("finesse material");
        material.classes =
            QueueClassSet::group(&patterns, false).expect("128 distinct queue classes");
        assert_eq!(material.classes.classes().len(), 128);

        assert!(
            checked_finesse_report_build_future_upper_bound(
                &[material],
                FinessePatternKnowledge::Both,
            )
            .is_some(),
            "an empty language catalog must not create an imaginary Visible-7 product",
        );
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

        assert_eq!(compact.field_occurrence_count("board_height"), 1);
        assert_eq!(compact.unique_field("board_height"), Some("6"));
        assert_eq!(extended.field_occurrence_count("board_height"), 1);
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
