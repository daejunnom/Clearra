// SRP rationale: one change reason owns the exact single-worker Setup Finder product state machine.
// It spans residue-aware coverage traversal through canonical setup result
// assembly; shared graph construction and parallel transport remain separate
// owners.
use std::{cmp::Ordering, collections::HashMap, sync::Arc};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_problem::{
    SetupCandidatePriority, SetupCycleResetBorrowPolicy, SetupLengthPreference,
    SetupSearchCondition, SetupSearchQuery,
};
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use crate::{
    CoreExecutionResult, SetupCandidateReport, SetupFinderReport, SetupHoldConditionReport,
};

use super::{
    exact_collections::ExactHashMap,
    geometry::add_packed_piece,
    piece_index,
    setup_all_paths::{SetupAllPathEnumerator, SetupAllPathGraph},
    setup_coverage_graph::SetupCoverageGraph,
    setup_graph_builder::{SetupGraphBuildAdvance, SetupGraphBuildSession, SetupSharedGraph},
    setup_partial_build::{PartialBuildEdge, PartialBuildGraph, PartialBuildNode},
    WasmExactSearchError,
};

pub(super) const HOLD_STATE_COUNT: usize = 8;
pub(super) const EXTRA_DRAW_STATE_COUNT: usize = 2;
pub(super) const COVERAGE_WORD_LANES: usize = 2;
const EMPTY_COVERAGE_WORDS: [u64; COVERAGE_WORD_LANES] = [0; COVERAGE_WORD_LANES];

pub(crate) enum WasmSetupSearchAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

enum SetupSearchStage {
    Building(SetupGraphBuildSession),
    Coverage {
        query: SetupSearchQuery,
        conditions: Vec<SetupSearchCondition>,
        graph: Arc<PartialBuildGraph>,
        coverage_graph: Arc<SetupCoverageGraph>,
        next_condition: usize,
        active: Option<SetupCoverageSession>,
        completed: Vec<CompletedSetupCoverage>,
        geometry_family_count: String,
        geometry_expanded_nodes: usize,
    },
    Finished,
}

pub(crate) struct WasmSetupSearchSession {
    stage: SetupSearchStage,
}

impl WasmSetupSearchSession {
    pub fn new(query: &SetupSearchQuery) -> Result<Self, WasmExactSearchError> {
        Ok(Self {
            stage: SetupSearchStage::Building(SetupGraphBuildSession::new(query)?),
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmSetupSearchAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            self.stage = SetupSearchStage::Finished;
            return Ok(WasmSetupSearchAdvance::Cancelled);
        }
        let budget = work_budget.max(1);
        let stage = std::mem::replace(&mut self.stage, SetupSearchStage::Finished);
        match stage {
            SetupSearchStage::Building(mut builder) => match builder.advance(budget, control)? {
                SetupGraphBuildAdvance::Pending => {
                    self.stage = SetupSearchStage::Building(builder);
                    Ok(WasmSetupSearchAdvance::Pending)
                }
                SetupGraphBuildAdvance::Cancelled => Ok(WasmSetupSearchAdvance::Cancelled),
                SetupGraphBuildAdvance::Complete(SetupSharedGraph {
                    query,
                    conditions,
                    graph,
                    coverage_graph,
                    geometry_family_count,
                    geometry_expanded_nodes,
                }) => {
                    self.stage = SetupSearchStage::Coverage {
                        query,
                        conditions,
                        graph,
                        coverage_graph,
                        next_condition: 0,
                        active: None,
                        completed: Vec::new(),
                        geometry_family_count,
                        geometry_expanded_nodes,
                    };
                    Ok(WasmSetupSearchAdvance::Pending)
                }
            },
            SetupSearchStage::Coverage {
                query,
                conditions,
                graph,
                coverage_graph,
                mut next_condition,
                mut active,
                mut completed,
                geometry_family_count,
                geometry_expanded_nodes,
            } => {
                if active.is_none() {
                    if next_condition == conditions.len() {
                        let result = finish_setup_result(
                            &query,
                            &graph,
                            completed,
                            geometry_family_count,
                            geometry_expanded_nodes,
                            1,
                            false,
                            "setup-family-quotient-serial",
                        );
                        self.stage = SetupSearchStage::Finished;
                        return Ok(WasmSetupSearchAdvance::Completed(result));
                    }
                    active = Some(SetupCoverageSession::new(
                        &conditions[next_condition],
                        Arc::clone(&graph),
                        Arc::clone(&coverage_graph),
                        query.limits().max_results(),
                        query.candidate_priority(),
                        query.length_preference(),
                        query.path_detail(),
                    )?);
                }
                let mut session = active.take().expect("coverage session exists");
                match session.advance(budget, control)? {
                    SetupCoverageAdvance::Pending => {
                        self.stage = SetupSearchStage::Coverage {
                            query,
                            conditions,
                            graph,
                            coverage_graph,
                            next_condition,
                            active: Some(session),
                            completed,
                            geometry_family_count,
                            geometry_expanded_nodes,
                        };
                        Ok(WasmSetupSearchAdvance::Pending)
                    }
                    SetupCoverageAdvance::Cancelled => Ok(WasmSetupSearchAdvance::Cancelled),
                    SetupCoverageAdvance::Complete(result) => {
                        completed.push(result);
                        next_condition += 1;
                        self.stage = SetupSearchStage::Coverage {
                            query,
                            conditions,
                            graph,
                            coverage_graph,
                            next_condition,
                            active: None,
                            completed,
                            geometry_family_count,
                            geometry_expanded_nodes,
                        };
                        Ok(WasmSetupSearchAdvance::Pending)
                    }
                }
            }
            SetupSearchStage::Finished => Err(WasmExactSearchError::InvalidProblem(
                "setup_search_session_already_finished",
            )),
        }
    }
}

pub(super) fn finish_setup_result(
    query: &SetupSearchQuery,
    graph: &PartialBuildGraph,
    completed: Vec<CompletedSetupCoverage>,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    workers_used: usize,
    parallel: bool,
    parallel_decision_reason: &'static str,
) -> CoreExecutionResult {
    let solution_found = completed
        .iter()
        .any(|result| result.report.candidate_count() != 0);
    let result_count = completed
        .iter()
        .map(|result| result.report.candidate_count())
        .sum::<usize>();
    let normalized_solution_set_hash = setup_candidate_set_hash(&completed);
    let reports = completed.into_iter().map(|result| result.report).collect();
    let remaining_pieces = query
        .residue()
        .pieces()
        .iter()
        .map(|piece| piece.as_ascii())
        .collect::<String>();
    let queue_based_pieces = query
        .queue()
        .as_fixed_sequence()
        .map(|queue| {
            queue
                .pieces()
                .iter()
                .map(|piece| piece.as_ascii())
                .collect::<String>()
        })
        .unwrap_or_default();
    let report = SetupFinderReport::new(
        query.search_mode(),
        query.residue().cycle().unwrap_or_default(),
        remaining_pieces.clone(),
        queue_based_pieces.clone(),
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
        geometry_family_count.clone(),
        graph.nodes.len(),
        true,
        reports,
    );
    CoreExecutionResult::new(
        vec![
            ("status".to_owned(), "setup-finder-complete".to_owned()),
            (
                "backend_selected".to_owned(),
                "wasm-cpu-setup-family-quotient".to_owned(),
            ),
            ("workers_used".to_owned(), workers_used.to_string()),
            ("cpu_parallel_execution".to_owned(), parallel.to_string()),
            (
                "cpu_parallel_decision_reason".to_owned(),
                parallel_decision_reason.to_owned(),
            ),
            ("solution_found".to_owned(), solution_found.to_string()),
            ("count_complete".to_owned(), "true".to_owned()),
            ("probability_complete".to_owned(), "true".to_owned()),
            ("setup_coverage_semantics".to_owned(), "oracle".to_owned()),
            (
                "setup_search_mode".to_owned(),
                query.search_mode().keyword().to_owned(),
            ),
            ("remaining_pieces".to_owned(), remaining_pieces),
            ("queue_based_pieces".to_owned(), queue_based_pieces),
            (
                "setup_cycle".to_owned(),
                query.residue().cycle().unwrap_or_default().to_string(),
            ),
            (
                "setup_candidate_priority".to_owned(),
                query.candidate_priority().keyword().to_owned(),
            ),
            (
                "setup_length_preference".to_owned(),
                query.length_preference().keyword().to_owned(),
            ),
            (
                "geometry_candidate_family_count".to_owned(),
                geometry_family_count,
            ),
            (
                "searched_nodes".to_owned(),
                geometry_expanded_nodes.to_string(),
            ),
            (
                "partial_build_node_count".to_owned(),
                graph.nodes.len().to_string(),
            ),
            ("unique_solution_count".to_owned(), result_count.to_string()),
            (
                "normalized_unique_solution_count".to_owned(),
                result_count.to_string(),
            ),
            (
                "normalized_solution_key_algorithm".to_owned(),
                "clearra-setup-candidate-key-v1".to_owned(),
            ),
            (
                "normalized_solution_set_hash_algorithm".to_owned(),
                "clearra-setup-candidate-set-fnv64-v1".to_owned(),
            ),
            (
                "normalized_solution_set_hash".to_owned(),
                normalized_solution_set_hash.clone(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                normalized_solution_set_hash,
            ),
            (
                "resource_truncated".to_owned(),
                graph.resource_truncated.to_string(),
            ),
            (
                "resource_truncation_reason".to_owned(),
                if graph.resource_truncated {
                    "setup_partial_graph_storage_unavailable"
                } else {
                    "none"
                }
                .to_owned(),
            ),
        ],
        Vec::new(),
    )
    .with_setup_finder_report(report)
}

struct ShapeCoverageAccumulator {
    build_covered_patterns: usize,
    joint_covered_patterns: usize,
    build_weight: f64,
    joint_weight: f64,
    min_covered_locks: u8,
    max_covered_locks: u8,
    witness: Option<SetupWitness>,
}

impl Default for ShapeCoverageAccumulator {
    fn default() -> Self {
        Self {
            build_covered_patterns: 0,
            joint_covered_patterns: 0,
            build_weight: 0.0,
            joint_weight: 0.0,
            min_covered_locks: u8::MAX,
            max_covered_locks: 0,
            witness: None,
        }
    }
}

#[derive(Clone, Copy)]
struct SetupWitness {
    word_index: usize,
    pattern_bit: u64,
}

const NO_REPRESENTATIVE_RECORD: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct RepresentativeRecord {
    state: u32,
    previous: u32,
    edge_index: u32,
    hold_action: SetupHoldAction,
    can_complete: bool,
}

struct RepresentativeScratch {
    state_records: Vec<u32>,
    records: Vec<RepresentativeRecord>,
    depth_records: Vec<Vec<u32>>,
}

impl RepresentativeScratch {
    fn new(state_capacity: usize) -> Result<Self, WasmExactSearchError> {
        let mut state_records = Vec::new();
        state_records
            .try_reserve_exact(state_capacity)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_witness_state_index_storage_unavailable",
                )
            })?;
        state_records.resize(state_capacity, NO_REPRESENTATIVE_RECORD);
        Ok(Self {
            state_records,
            records: Vec::new(),
            depth_records: (0..=10).map(|_| Vec::new()).collect(),
        })
    }

    fn clear(&mut self) {
        for record in &self.records {
            self.state_records[record.state as usize] = NO_REPRESENTATIVE_RECORD;
        }
        self.records.clear();
        for depth in &mut self.depth_records {
            depth.clear();
        }
    }

    fn activate(
        &mut self,
        state: usize,
        depth: u8,
        previous: u32,
        edge_index: u32,
        hold_action: SetupHoldAction,
    ) -> Result<u32, WasmExactSearchError> {
        let slot =
            self.state_records
                .get_mut(state)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_witness_state_out_of_range",
                ))?;
        if *slot != NO_REPRESENTATIVE_RECORD {
            return Ok(*slot);
        }
        let record = u32::try_from(self.records.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_witness_record_index_overflow")
        })?;
        self.records.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_witness_record_storage_unavailable")
        })?;
        self.depth_records[depth as usize]
            .try_reserve(1)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_witness_queue_storage_unavailable")
            })?;
        self.records.push(RepresentativeRecord {
            state: u32::try_from(state).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_witness_state_index_overflow")
            })?,
            previous,
            edge_index,
            hold_action,
            can_complete: false,
        });
        self.depth_records[depth as usize].push(record);
        *slot = record;
        Ok(record)
    }

    fn record_for_state(&self, state: usize) -> Option<u32> {
        self.state_records
            .get(state)
            .copied()
            .filter(|record| *record != NO_REPRESENTATIVE_RECORD)
    }
}

enum SetupCoverageAdvance {
    Pending,
    Complete(CompletedSetupCoverage),
    Cancelled,
}

pub(super) struct CompletedSetupCoverage {
    pub(super) report: SetupHoldConditionReport,
    pub(super) candidate_boards: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(super) enum SetupHoldAction {
    #[default]
    UseCurrent = 0,
    SwapHeld = 1,
    StoreCurrentUseNext = 2,
    UseHeldTerminal = 3,
}

impl SetupHoldAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::UseCurrent => "use-current",
            Self::SwapHeld => "swap-held",
            Self::StoreCurrentUseNext => "store-current-use-next",
            Self::UseHeldTerminal => "use-held-terminal",
        }
    }

    pub(super) const fn code(self) -> u8 {
        self as u8
    }

    pub(super) fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::UseCurrent),
            1 => Some(Self::SwapHeld),
            2 => Some(Self::StoreCurrentUseNext),
            3 => Some(Self::UseHeldTerminal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SetupSupplyStateLayout {
    queue_based_queue_start: u8,
    queue_based_queue_len: u8,
    hold_provenance_state_count: usize,
}

impl SetupSupplyStateLayout {
    pub(super) const fn new(queue_based_queue_start: u8, queue_based_queue_len: u8) -> Self {
        Self {
            queue_based_queue_start,
            queue_based_queue_len,
            hold_provenance_state_count: if queue_based_queue_len == 0 { 1 } else { 2 },
        }
    }

    pub(super) fn state_capacity(self, node_count: usize) -> Option<usize> {
        node_count
            .checked_mul(EXTRA_DRAW_STATE_COUNT * HOLD_STATE_COUNT)
            .and_then(|value| value.checked_mul(self.hold_provenance_state_count))
    }

    pub(super) const fn has_same_encoding(self, other: Self) -> bool {
        self.hold_provenance_state_count == other.hold_provenance_state_count
    }

    pub(super) fn encode(
        self,
        node: usize,
        extra_draw: u8,
        hold_code: u8,
        hold_from_queue_based_prefix: bool,
    ) -> usize {
        debug_assert!(
            self.queue_based_queue_len != 0 || !hold_from_queue_based_prefix,
            "shape-oracle states never carry QB hold provenance"
        );
        ((node * EXTRA_DRAW_STATE_COUNT + usize::from(extra_draw)) * HOLD_STATE_COUNT
            + usize::from(hold_code))
            * self.hold_provenance_state_count
            + usize::from(hold_from_queue_based_prefix)
    }

    pub(super) fn decode(self, index: usize) -> (usize, u8, u8, bool) {
        let hold_from_queue_based_prefix = index % self.hold_provenance_state_count != 0;
        let state_without_provenance = index / self.hold_provenance_state_count;
        let hold_code = (state_without_provenance % HOLD_STATE_COUNT) as u8;
        let node_and_extra = state_without_provenance / HOLD_STATE_COUNT;
        (
            node_and_extra / EXTRA_DRAW_STATE_COUNT,
            (node_and_extra % EXTRA_DRAW_STATE_COUNT) as u8,
            hold_code,
            hold_from_queue_based_prefix,
        )
    }

    pub(super) fn next_hold_provenance(
        self,
        initial_cursor: u16,
        depth: u8,
        extra_draw: u8,
        current_hold_from_queue_based_prefix: bool,
        action: SetupHoldAction,
    ) -> bool {
        let queue_position =
            usize::from(initial_cursor) + usize::from(depth) + usize::from(extra_draw);
        let queue_start = usize::from(self.queue_based_queue_start);
        let queue_end = queue_start + usize::from(self.queue_based_queue_len);
        let current_is_queue_based = (queue_start..queue_end).contains(&queue_position);
        match action {
            SetupHoldAction::UseCurrent => current_hold_from_queue_based_prefix,
            SetupHoldAction::SwapHeld | SetupHoldAction::StoreCurrentUseNext => {
                current_is_queue_based
            }
            SetupHoldAction::UseHeldTerminal => false,
        }
    }

    pub(super) fn accepts_setup_candidate(
        self,
        initial_cursor: u16,
        depth: u8,
        extra_draw: u8,
        hold_from_queue_based_prefix: bool,
    ) -> bool {
        let queue_end =
            usize::from(self.queue_based_queue_start) + usize::from(self.queue_based_queue_len);
        self.queue_based_queue_len == 0
            || (usize::from(initial_cursor) + usize::from(depth) + usize::from(extra_draw)
                >= queue_end
                && !hold_from_queue_based_prefix)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SetupSupplyPrefixState {
    packed_counts: u32,
    hold_code: u8,
    extra_draw: u8,
}

#[derive(Clone, Copy, Default)]
pub(super) struct SetupSupplyTransition {
    pub(super) extra_draw: u8,
    pub(super) hold_code: u8,
    pub(super) mask: u64,
    pub(super) hold_action: SetupHoldAction,
}

#[derive(Clone, Copy)]
pub(super) struct SetupSupplyTransitionSet {
    entries: [SetupSupplyTransition; 9],
    len: u8,
}

impl SetupSupplyTransitionSet {
    fn new() -> Self {
        Self {
            entries: [SetupSupplyTransition::default(); 9],
            len: 0,
        }
    }

    fn push(&mut self, extra_draw: u8, hold_code: u8, mask: u64, hold_action: SetupHoldAction) {
        if mask == 0 {
            return;
        }
        debug_assert!((self.len as usize) < self.entries.len());
        self.entries[self.len as usize] = SetupSupplyTransition {
            extra_draw,
            hold_code,
            mask,
            hold_action,
        };
        self.len += 1;
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = SetupSupplyTransition> + '_ {
        self.entries[..self.len as usize].iter().copied()
    }
}

pub(super) struct SetupSupplyTransitionCatalog {
    sets: Vec<SetupSupplyTransitionSet>,
}

impl SetupSupplyTransitionCatalog {
    const DEPTH_COUNT: usize = 11;
    const PIECE_COUNT: usize = 7;
    const TERMINAL_STATE_COUNT: usize = 2;

    #[allow(clippy::too_many_arguments)]
    pub(super) fn compile(
        pattern_index: &PatternPiecePositionIndex,
        initial_cursor: u16,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        projects_standard_bag_lookahead: bool,
        word_start: usize,
        root_bits: [u64; COVERAGE_WORD_LANES],
        lane_count: usize,
    ) -> Result<Self, WasmExactSearchError> {
        let entry_count = Self::DEPTH_COUNT
            * Self::PIECE_COUNT
            * EXTRA_DRAW_STATE_COUNT
            * HOLD_STATE_COUNT
            * Self::TERMINAL_STATE_COUNT
            * COVERAGE_WORD_LANES;
        let mut sets = Vec::new();
        sets.try_reserve_exact(entry_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "setup_supply_transition_catalog_storage_unavailable",
            )
        })?;
        sets.resize_with(entry_count, SetupSupplyTransitionSet::new);
        for depth in 0..Self::DEPTH_COUNT as u8 {
            for desired_piece in 1..=Self::PIECE_COUNT as u8 {
                for extra_draw in 0..EXTRA_DRAW_STATE_COUNT as u8 {
                    for hold_code in 0..HOLD_STATE_COUNT as u8 {
                        for terminal in [false, true] {
                            for lane in 0..lane_count {
                                let index = Self::index(
                                    depth,
                                    desired_piece,
                                    extra_draw,
                                    hold_code,
                                    terminal,
                                    lane,
                                );
                                sets[index] = setup_supply_transitions(
                                    pattern_index,
                                    initial_cursor,
                                    hold_enabled,
                                    projects_unplaced_lookahead,
                                    projects_standard_bag_lookahead,
                                    depth,
                                    desired_piece,
                                    extra_draw,
                                    hold_code,
                                    root_bits[lane],
                                    word_start + lane,
                                    terminal,
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(Self { sets })
    }

    pub(super) fn get(
        &self,
        depth: u8,
        desired_piece: u8,
        extra_draw: u8,
        hold_code: u8,
        terminal: bool,
        lane: usize,
    ) -> &SetupSupplyTransitionSet {
        &self.sets[Self::index(depth, desired_piece, extra_draw, hold_code, terminal, lane)]
    }

    fn index(
        depth: u8,
        desired_piece: u8,
        extra_draw: u8,
        hold_code: u8,
        terminal: bool,
        lane: usize,
    ) -> usize {
        (((((usize::from(depth) * Self::PIECE_COUNT
            + usize::from(desired_piece.saturating_sub(1)))
            * EXTRA_DRAW_STATE_COUNT
            + usize::from(extra_draw))
            * HOLD_STATE_COUNT
            + usize::from(hold_code))
            * Self::TERMINAL_STATE_COUNT
            + usize::from(terminal))
            * COVERAGE_WORD_LANES)
            + lane
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn setup_supply_transitions(
    pattern_index: &PatternPiecePositionIndex,
    initial_cursor: u16,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    depth: u8,
    desired_piece: u8,
    extra_draw: u8,
    hold_code: u8,
    active: u64,
    word_index: usize,
    terminal: bool,
) -> SetupSupplyTransitionSet {
    let mut transitions = SetupSupplyTransitionSet::new();
    let queue_position = usize::from(initial_cursor) + usize::from(depth) + usize::from(extra_draw);
    if terminal
        && hold_enabled
        && projects_unplaced_lookahead
        && hold_code == desired_piece
        && queue_position == pattern_index.sequence_len()
    {
        transitions.push(
            extra_draw,
            hold_code,
            active,
            SetupHoldAction::UseHeldTerminal,
        );
    }

    let use_current = active
        & pattern_index.piece_word_with_projected_standard_bag_lookahead(
            queue_position,
            desired_piece,
            word_index,
            projects_standard_bag_lookahead,
        );
    transitions.push(
        extra_draw,
        hold_code,
        use_current,
        SetupHoldAction::UseCurrent,
    );
    if !hold_enabled {
        return transitions;
    }
    if hold_code != 0 && hold_code == desired_piece {
        for current_piece in 1..=7 {
            let swap_bits = active
                & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                    queue_position,
                    current_piece,
                    word_index,
                    projects_standard_bag_lookahead,
                );
            transitions.push(
                extra_draw,
                current_piece,
                swap_bits,
                SetupHoldAction::SwapHeld,
            );
        }
    } else if hold_code == 0 && extra_draw == 0 {
        for current_piece in 1..=7 {
            let store_bits = active
                & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                    queue_position,
                    current_piece,
                    word_index,
                    projects_standard_bag_lookahead,
                )
                & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                    queue_position + 1,
                    desired_piece,
                    word_index,
                    projects_standard_bag_lookahead,
                );
            transitions.push(
                1,
                current_piece,
                store_bits,
                SetupHoldAction::StoreCurrentUseNext,
            );
        }
    }
    transitions
}

pub(super) fn compile_setup_admissible_prefixes(
    conditions: &[SetupSearchCondition],
) -> Result<Vec<u32>, WasmExactSearchError> {
    let mut prefixes = vec![0_u32];
    for condition in conditions {
        let problem = condition.problem();
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("setup_pattern_universe_not_materialized"),
        )?;
        let pattern_index = PatternPiecePositionIndex::compile(universe).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_pattern_index_compile_failed")
        })?;
        let hold_enabled = problem.supply().hold_enabled();
        let projects_unplaced_lookahead = problem.supply().projects_unplaced_lookahead();
        let projects_standard_bag_lookahead = problem.supply().projects_standard_bag_lookahead();
        let initial_cursor = problem.initial_hold().cursor();
        let initial_hold_code = condition
            .initial_hold()
            .map_or(0, |piece| piece_index(piece) as u8 + 1);
        let mut current = ExactHashMap::<SetupSupplyPrefixState, u64>::default();
        let mut next = ExactHashMap::<SetupSupplyPrefixState, u64>::default();
        for word_index in 0..pattern_index.word_count() {
            current.clear();
            current.insert(
                SetupSupplyPrefixState {
                    packed_counts: 0,
                    hold_code: initial_hold_code,
                    extra_draw: 0,
                },
                pattern_index.active_word(word_index),
            );
            for depth in 0..10_u8 {
                next.clear();
                for (state, active) in current.iter().map(|(state, mask)| (*state, *mask)) {
                    for desired_piece in 1..=7_u8 {
                        let Some(packed_counts) =
                            add_packed_piece(state.packed_counts, usize::from(desired_piece - 1))
                        else {
                            return Err(WasmExactSearchError::InvalidProblem(
                                "setup_supply_prefix_piece_count_overflow",
                            ));
                        };
                        let transitions = setup_supply_transitions(
                            &pattern_index,
                            initial_cursor,
                            hold_enabled,
                            projects_unplaced_lookahead,
                            projects_standard_bag_lookahead,
                            depth,
                            desired_piece,
                            state.extra_draw,
                            state.hold_code,
                            active,
                            word_index,
                            depth + 1 == 10,
                        );
                        next.try_reserve(usize::from(transitions.len))
                            .map_err(|_| {
                                WasmExactSearchError::InvalidProblem(
                                    "setup_supply_prefix_storage_unavailable",
                                )
                            })?;
                        for transition in transitions.iter() {
                            let target = SetupSupplyPrefixState {
                                packed_counts,
                                hold_code: transition.hold_code,
                                extra_draw: transition.extra_draw,
                            };
                            *next.entry(target).or_default() |= transition.mask;
                        }
                    }
                }
                prefixes.extend(next.keys().map(|state| state.packed_counts));
                std::mem::swap(&mut current, &mut next);
                if current.is_empty() {
                    break;
                }
            }
        }
    }
    prefixes.sort_unstable();
    prefixes.dedup();
    Ok(prefixes)
}

#[derive(Clone, Copy, Default)]
struct CoverageTransition {
    target: usize,
    hold_action: SetupHoldAction,
}

struct CoverageTransitionSet {
    entries: [CoverageTransition; 9],
    len: u8,
}

impl CoverageTransitionSet {
    fn new() -> Self {
        Self {
            entries: [CoverageTransition::default(); 9],
            len: 0,
        }
    }

    fn push(&mut self, target: usize, mask: u64, hold_action: SetupHoldAction) {
        if mask == 0 {
            return;
        }
        debug_assert!((self.len as usize) < self.entries.len());
        self.entries[self.len as usize] = CoverageTransition {
            target,
            hold_action,
        };
        self.len += 1;
    }

    fn iter(&self) -> impl Iterator<Item = CoverageTransition> + '_ {
        self.entries[..self.len as usize].iter().copied()
    }
}

struct SetupCoverageSession {
    condition_id: String,
    initial_hold: Option<PieceKind>,
    pattern_expression: String,
    graph: Arc<PartialBuildGraph>,
    coverage_graph: Arc<SetupCoverageGraph>,
    pattern_index: PatternPiecePositionIndex,
    weights: WeightedPatternSet,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    initial_cursor: u16,
    state_layout: SetupSupplyStateLayout,
    next_word: usize,
    alpha: Vec<[u64; COVERAGE_WORD_LANES]>,
    beta: Vec<[u64; COVERAGE_WORD_LANES]>,
    depth_states: Vec<Vec<usize>>,
    touched_alpha: Vec<usize>,
    touched_beta: Vec<usize>,
    shape_build_words: Vec<[u64; COVERAGE_WORD_LANES]>,
    shape_joint_words: Vec<[u64; COVERAGE_WORD_LANES]>,
    touched_shapes: Vec<usize>,
    shape_touched: Vec<bool>,
    accumulators: Vec<ShapeCoverageAccumulator>,
    max_results: usize,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    path_target_board: Option<u64>,
    all_paths: Option<SetupAllPathEnumerator>,
}

impl SetupCoverageSession {
    fn new(
        condition: &SetupSearchCondition,
        graph: Arc<PartialBuildGraph>,
        coverage_graph: Arc<SetupCoverageGraph>,
        max_results: usize,
        candidate_priority: SetupCandidatePriority,
        length_preference: SetupLengthPreference,
        path_detail: Option<&clearra_problem::SetupPathDetail>,
    ) -> Result<Self, WasmExactSearchError> {
        let problem = condition.problem();
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("setup_pattern_universe_not_materialized"),
        )?;
        let pattern_index = PatternPiecePositionIndex::compile(universe).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_pattern_index_compile_failed")
        })?;
        let state_layout = SetupSupplyStateLayout::new(
            condition.queue_based_queue_start(),
            condition.queue_based_queue_len(),
        );
        let state_capacity = state_layout
            .state_capacity(coverage_graph.nodes.len())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_coverage_state_capacity_overflow",
            ))?;
        let shape_count = graph.shapes.len();
        let path_target_board = path_detail.map(clearra_problem::SetupPathDetail::board_mask);
        let all_paths = if let Some(detail) = path_detail {
            let shape_index = graph
                .shapes
                .iter()
                .position(|shape| shape.board == detail.board_mask())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_path_detail_shape_not_found",
                ))?;
            Some(SetupAllPathEnumerator::new(
                condition,
                Arc::new(SetupAllPathGraph::from_partial(&graph)),
                u32::try_from(shape_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_path_detail_shape_index_overflow")
                })?,
            )?)
        } else {
            None
        };
        Ok(Self {
            condition_id: condition.condition_id().to_owned(),
            initial_hold: condition.initial_hold(),
            pattern_expression: condition.pattern_expression().to_owned(),
            graph,
            coverage_graph,
            pattern_index,
            weights: universe.weights().clone(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: problem.supply().projects_standard_bag_lookahead(),
            initial_cursor: problem.initial_hold().cursor(),
            state_layout,
            next_word: 0,
            alpha: vec![EMPTY_COVERAGE_WORDS; state_capacity],
            beta: vec![EMPTY_COVERAGE_WORDS; state_capacity],
            depth_states: (0..=10).map(|_| Vec::new()).collect(),
            touched_alpha: Vec::new(),
            touched_beta: Vec::new(),
            shape_build_words: vec![EMPTY_COVERAGE_WORDS; shape_count],
            shape_joint_words: vec![EMPTY_COVERAGE_WORDS; shape_count],
            touched_shapes: Vec::new(),
            shape_touched: vec![false; shape_count],
            accumulators: (0..shape_count)
                .map(|_| ShapeCoverageAccumulator::default())
                .collect(),
            max_results,
            candidate_priority,
            length_preference,
            path_target_board,
            all_paths,
        })
    }

    fn advance(
        &mut self,
        _work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<SetupCoverageAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(SetupCoverageAdvance::Cancelled);
        }
        if self.next_word == self.pattern_index.word_count() {
            return Ok(SetupCoverageAdvance::Complete(self.finish()?));
        }
        let lane_count =
            (self.pattern_index.word_count() - self.next_word).min(COVERAGE_WORD_LANES);
        match self.process_word_block(self.next_word, lane_count, control) {
            Ok(()) => {}
            Err(WasmExactSearchError::Cancelled) => {
                return Ok(SetupCoverageAdvance::Cancelled);
            }
            Err(error) => return Err(error),
        }
        self.next_word += lane_count;
        Ok(SetupCoverageAdvance::Pending)
    }

    fn process_word_block(
        &mut self,
        word_start: usize,
        lane_count: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        let mut cancellation_work = 0_usize;
        for queue in &mut self.depth_states {
            queue.clear();
        }
        let hold_code = self
            .initial_hold
            .map_or(0, |piece| piece_index(piece) as u8 + 1);
        let mut root_bits = EMPTY_COVERAGE_WORDS;
        for (lane, bits) in root_bits.iter_mut().enumerate().take(lane_count) {
            *bits = self.pattern_index.active_word(word_start + lane);
        }
        let transition_catalog = SetupSupplyTransitionCatalog::compile(
            &self.pattern_index,
            self.initial_cursor,
            self.hold_enabled,
            self.projects_unplaced_lookahead,
            self.projects_standard_bag_lookahead,
            word_start,
            root_bits,
            lane_count,
        )?;
        if let Some(all_paths) = &mut self.all_paths {
            all_paths.run_word_range(word_start, word_start + lane_count, control)?;
        }
        self.activate_alpha(
            self.coverage_graph.root as usize,
            0,
            hold_code,
            false,
            root_bits,
        );

        for depth in 0..self.depth_states.len() {
            let mut cursor = 0;
            while cursor < self.depth_states[depth].len() {
                check_setup_coverage_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                cursor += 1;
                let active = self.alpha[state_index];
                let (node_index, extra_draw, hold_code, hold_from_qb) =
                    self.state_layout.decode(state_index);
                let node = self.coverage_graph.nodes[node_index];
                if node.accepting() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                for edge_index in edge_start..edge_end {
                    check_setup_coverage_cancel(control, &mut cancellation_work)?;
                    let edge = self.coverage_graph.edges[edge_index];
                    let target_node = edge.child() as usize;
                    let terminal = self.coverage_graph.nodes[target_node].accepting();
                    for lane in 0..lane_count {
                        let transitions = transition_catalog.get(
                            node.depth,
                            edge.piece_code(),
                            extra_draw,
                            hold_code,
                            terminal,
                            lane,
                        );
                        for transition in transitions.iter() {
                            let next_hold_from_qb = self.state_layout.next_hold_provenance(
                                self.initial_cursor,
                                node.depth,
                                extra_draw,
                                hold_from_qb,
                                transition.hold_action,
                            );
                            self.activate_alpha_lane(
                                self.state_layout.encode(
                                    target_node,
                                    transition.extra_draw,
                                    transition.hold_code,
                                    next_hold_from_qb,
                                ),
                                lane,
                                active[lane] & transition.mask,
                            );
                        }
                    }
                }
            }
        }

        for cursor in 0..self.touched_alpha.len() {
            let state_index = self.touched_alpha[cursor];
            let (node_index, _, _, _) = self.state_layout.decode(state_index);
            if self.coverage_graph.nodes[node_index].accepting() {
                self.activate_beta(state_index, self.alpha[state_index]);
            }
        }
        for depth in (0..self.depth_states.len()).rev() {
            for cursor in 0..self.depth_states[depth].len() {
                check_setup_coverage_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                let active = self.alpha[state_index];
                let (node_index, extra_draw, hold_code, hold_from_qb) =
                    self.state_layout.decode(state_index);
                let node = self.coverage_graph.nodes[node_index];
                if node.accepting() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                let mut successful = EMPTY_COVERAGE_WORDS;
                for edge_index in edge_start..edge_end {
                    check_setup_coverage_cancel(control, &mut cancellation_work)?;
                    let edge = self.coverage_graph.edges[edge_index];
                    let target_node = edge.child() as usize;
                    let terminal = self.coverage_graph.nodes[target_node].accepting();
                    for lane in 0..lane_count {
                        let transitions = transition_catalog.get(
                            node.depth,
                            edge.piece_code(),
                            extra_draw,
                            hold_code,
                            terminal,
                            lane,
                        );
                        for transition in transitions.iter() {
                            let next_hold_from_qb = self.state_layout.next_hold_provenance(
                                self.initial_cursor,
                                node.depth,
                                extra_draw,
                                hold_from_qb,
                                transition.hold_action,
                            );
                            let target = self.state_layout.encode(
                                target_node,
                                transition.extra_draw,
                                transition.hold_code,
                                next_hold_from_qb,
                            );
                            successful[lane] |=
                                active[lane] & transition.mask & self.beta[target][lane];
                        }
                    }
                }
                self.activate_beta(state_index, successful);
            }
        }

        for cursor in 0..self.touched_alpha.len() {
            let state_index = self.touched_alpha[cursor];
            let (node_index, extra_draw, _, hold_from_qb) = self.state_layout.decode(state_index);
            let node = self.coverage_graph.nodes[node_index];
            if !self.state_layout.accepts_setup_candidate(
                self.initial_cursor,
                node.depth,
                extra_draw,
                hold_from_qb,
            ) {
                continue;
            }
            let Some(shape_index) = node.shape_index().map(|index| index as usize) else {
                continue;
            };
            if !self.shape_touched[shape_index] {
                self.shape_touched[shape_index] = true;
                self.touched_shapes.push(shape_index);
            }
            for lane in 0..lane_count {
                let joint = merge_exact_state_coverage(
                    &mut self.shape_build_words[shape_index][lane],
                    &mut self.shape_joint_words[shape_index][lane],
                    self.alpha[state_index][lane],
                    self.beta[state_index][lane],
                    root_bits[lane],
                );
                let accumulator = &mut self.accumulators[shape_index];
                if joint != 0 && accumulator.witness.is_none() {
                    accumulator.witness = Some(SetupWitness {
                        word_index: word_start + lane,
                        pattern_bit: 1_u64 << joint.trailing_zeros(),
                    });
                }
                if joint != 0 {
                    include_setup_depth_range(
                        &mut accumulator.min_covered_locks,
                        &mut accumulator.max_covered_locks,
                        node.depth,
                    );
                }
            }
        }
        for shape_index in self.touched_shapes.drain(..) {
            let build = self.shape_build_words[shape_index];
            let joint = self.shape_joint_words[shape_index];
            let accumulator = &mut self.accumulators[shape_index];
            for lane in 0..lane_count {
                accumulator.build_covered_patterns += build[lane].count_ones() as usize;
                accumulator.joint_covered_patterns += joint[lane].count_ones() as usize;
                accumulator.build_weight += covered_word_weight(
                    &self.pattern_index,
                    &self.weights,
                    word_start + lane,
                    build[lane],
                );
                accumulator.joint_weight += covered_word_weight(
                    &self.pattern_index,
                    &self.weights,
                    word_start + lane,
                    joint[lane],
                );
            }
            self.shape_build_words[shape_index] = EMPTY_COVERAGE_WORDS;
            self.shape_joint_words[shape_index] = EMPTY_COVERAGE_WORDS;
            self.shape_touched[shape_index] = false;
        }

        for state in self.touched_alpha.drain(..) {
            self.alpha[state] = EMPTY_COVERAGE_WORDS;
        }
        for state in self.touched_beta.drain(..) {
            self.beta[state] = EMPTY_COVERAGE_WORDS;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn original_transitions(
        &self,
        node: PartialBuildNode,
        edge: PartialBuildEdge,
        extra_draw: u8,
        hold_code: u8,
        hold_from_qb: bool,
        active: u64,
        word_index: usize,
    ) -> CoverageTransitionSet {
        let mut transitions = CoverageTransitionSet::new();
        let desired_piece = piece_index(edge.piece) as u8 + 1;
        let target_node = self.graph.nodes[edge.to as usize];
        let supply_transitions = setup_supply_transitions(
            &self.pattern_index,
            self.initial_cursor,
            self.hold_enabled,
            self.projects_unplaced_lookahead,
            self.projects_standard_bag_lookahead,
            node.depth,
            desired_piece,
            extra_draw,
            hold_code,
            active,
            word_index,
            target_node.accepting(),
        );
        for transition in supply_transitions.iter() {
            let next_hold_from_qb = self.state_layout.next_hold_provenance(
                self.initial_cursor,
                node.depth,
                extra_draw,
                hold_from_qb,
                transition.hold_action,
            );
            transitions.push(
                self.state_layout.encode(
                    edge.to as usize,
                    transition.extra_draw,
                    transition.hold_code,
                    next_hold_from_qb,
                ),
                transition.mask,
                transition.hold_action,
            );
        }
        transitions
    }

    fn activate_alpha(
        &mut self,
        node: usize,
        extra_draw: u8,
        hold_code: u8,
        hold_from_qb: bool,
        mask: [u64; COVERAGE_WORD_LANES],
    ) {
        self.activate_alpha_index(
            self.state_layout
                .encode(node, extra_draw, hold_code, hold_from_qb),
            mask,
        );
    }

    fn activate_alpha_index(&mut self, state: usize, mask: [u64; COVERAGE_WORD_LANES]) {
        if mask == EMPTY_COVERAGE_WORDS {
            return;
        }
        if self.alpha[state] == EMPTY_COVERAGE_WORDS {
            let (node, _, _, _) = self.state_layout.decode(state);
            self.depth_states[self.coverage_graph.nodes[node].depth as usize].push(state);
            self.touched_alpha.push(state);
        }
        for lane in 0..COVERAGE_WORD_LANES {
            self.alpha[state][lane] |= mask[lane];
        }
    }

    fn activate_alpha_lane(&mut self, state: usize, lane: usize, mask: u64) {
        if mask == 0 {
            return;
        }
        if self.alpha[state] == EMPTY_COVERAGE_WORDS {
            let (node, _, _, _) = self.state_layout.decode(state);
            self.depth_states[self.coverage_graph.nodes[node].depth as usize].push(state);
            self.touched_alpha.push(state);
        }
        self.alpha[state][lane] |= mask;
    }

    fn activate_beta(&mut self, state: usize, mask: [u64; COVERAGE_WORD_LANES]) {
        if mask == EMPTY_COVERAGE_WORDS {
            return;
        }
        if self.beta[state] == EMPTY_COVERAGE_WORDS {
            self.touched_beta.push(state);
        }
        for lane in 0..COVERAGE_WORD_LANES {
            self.beta[state][lane] |= mask[lane];
        }
    }

    fn finish(&mut self) -> Result<CompletedSetupCoverage, WasmExactSearchError> {
        let mut shape_indexes = self
            .graph
            .shapes
            .iter()
            .enumerate()
            .filter(|(shape_index, _)| self.accumulators[*shape_index].joint_covered_patterns != 0)
            .map(|(shape_index, _)| shape_index)
            .collect::<Vec<_>>();
        if let Some(target_board) = self.path_target_board {
            shape_indexes
                .retain(|shape_index| self.graph.shapes[*shape_index].board == target_board);
        }
        if shape_indexes.iter().any(|shape_index| {
            let coverage = &self.accumulators[*shape_index];
            coverage.min_covered_locks == u8::MAX
                || coverage.min_covered_locks > coverage.max_covered_locks
        }) {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_covered_depth_range_missing",
            ));
        }
        shape_indexes.sort_by(|left, right| {
            let left_shape = &self.graph.shapes[*left];
            let right_shape = &self.graph.shapes[*right];
            let left_coverage = &self.accumulators[*left];
            let right_coverage = &self.accumulators[*right];
            compare_setup_candidates(
                self.candidate_priority,
                self.length_preference,
                left_coverage.build_weight,
                left_coverage.joint_weight,
                left_coverage.min_covered_locks,
                left_coverage.max_covered_locks,
                left_shape.board,
                right_coverage.build_weight,
                right_coverage.joint_weight,
                right_coverage.min_covered_locks,
                right_coverage.max_covered_locks,
                right_shape.board,
            )
        });
        let candidate_count = shape_indexes.len();
        let mut candidate_boards = shape_indexes
            .iter()
            .map(|shape_index| self.graph.shapes[*shape_index].board)
            .collect::<Vec<_>>();
        candidate_boards.sort_unstable();
        let result_truncated = shape_indexes.len() > self.max_results;
        shape_indexes.truncate(self.max_results);
        let representative_paths = self.representative_paths(&shape_indexes)?;
        let mut all_solution_paths = self
            .all_paths
            .take()
            .map(SetupAllPathEnumerator::into_paths)
            .unwrap_or_default()
            .into_iter()
            .map(|path| path.into_core_path())
            .collect::<Vec<_>>();
        let detail_requested = self.path_target_board.is_some();
        let candidates = shape_indexes
            .into_iter()
            .zip(representative_paths)
            .map(|(shape_index, representative_path)| {
                let shape = &self.graph.shapes[shape_index];
                let coverage = &self.accumulators[shape_index];
                let conditional = if coverage.build_weight == 0.0 {
                    0.0
                } else {
                    coverage.joint_weight / coverage.build_weight
                };
                let candidate = SetupCandidateReport::new(
                    format!("setup-{:010x}", shape.board),
                    shape.board,
                    coverage.min_covered_locks,
                    coverage.max_covered_locks,
                    coverage.build_covered_patterns,
                    coverage.joint_covered_patterns,
                    probability_string(coverage.build_weight),
                    probability_string(coverage.joint_weight),
                    probability_string(conditional),
                    representative_path,
                );
                if detail_requested {
                    candidate.with_solution_paths(std::mem::take(&mut all_solution_paths))
                } else {
                    candidate
                }
            })
            .collect();
        Ok(CompletedSetupCoverage {
            report: SetupHoldConditionReport::new(
                self.condition_id.clone(),
                self.initial_hold,
                self.pattern_expression.clone(),
                self.pattern_index.global_pattern_count(),
                candidate_count,
                result_truncated,
                true,
                candidates,
            ),
            candidate_boards,
        })
    }

    fn representative_paths(
        &self,
        shape_indexes: &[usize],
    ) -> Result<Vec<Vec<crate::CorePathStep>>, WasmExactSearchError> {
        let mut paths = vec![Vec::new(); shape_indexes.len()];
        let mut groups = HashMap::<(usize, u64), Vec<(usize, usize)>>::new();
        for (output_index, shape_index) in shape_indexes.iter().copied().enumerate() {
            let witness = self.accumulators[shape_index].witness.ok_or(
                WasmExactSearchError::InvalidProblem("setup_joint_coverage_missing_witness"),
            )?;
            groups
                .entry((witness.word_index, witness.pattern_bit))
                .or_default()
                .push((output_index, shape_index));
        }
        let state_capacity = self
            .state_layout
            .state_capacity(self.graph.nodes.len())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_witness_state_capacity_overflow",
            ))?;
        let mut scratch = RepresentativeScratch::new(state_capacity)?;
        for ((word_index, pattern_bit), targets) in groups {
            self.representative_paths_for_pattern(
                word_index,
                pattern_bit,
                &targets,
                &mut paths,
                &mut scratch,
            )?;
        }
        Ok(paths)
    }

    fn representative_paths_for_pattern(
        &self,
        word_index: usize,
        pattern_bit: u64,
        targets: &[(usize, usize)],
        paths: &mut [Vec<crate::CorePathStep>],
        scratch: &mut RepresentativeScratch,
    ) -> Result<(), WasmExactSearchError> {
        scratch.clear();
        let hold_code = self
            .initial_hold
            .map_or(0, |piece| piece_index(piece) as u8 + 1);
        let root = self
            .state_layout
            .encode(self.graph.root as usize, 0, hold_code, false);
        if self.pattern_index.active_word(word_index) & pattern_bit == 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_witness_pattern_is_not_active",
            ));
        }
        scratch.activate(
            root,
            0,
            NO_REPRESENTATIVE_RECORD,
            NO_REPRESENTATIVE_RECORD,
            SetupHoldAction::UseCurrent,
        )?;

        for depth in 0..scratch.depth_records.len() {
            let mut cursor = 0;
            while cursor < scratch.depth_records[depth].len() {
                let record_index = scratch.depth_records[depth][cursor];
                cursor += 1;
                let state_index = scratch.records[record_index as usize].state as usize;
                let (node_index, extra_draw, state_hold_code, hold_from_qb) =
                    self.state_layout.decode(state_index);
                let node = self.graph.nodes[node_index];
                if node.accepting() || !node.live() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                for edge_index in edge_start..edge_end {
                    let edge = self.graph.edges[edge_index];
                    let transitions = self.original_transitions(
                        node,
                        edge,
                        extra_draw,
                        state_hold_code,
                        hold_from_qb,
                        pattern_bit,
                        word_index,
                    );
                    for transition in transitions.iter() {
                        let (target_node, _, _, _) = self.state_layout.decode(transition.target);
                        scratch.activate(
                            transition.target,
                            self.graph.nodes[target_node].depth,
                            record_index,
                            u32::try_from(edge_index).map_err(|_| {
                                WasmExactSearchError::InvalidProblem(
                                    "setup_witness_edge_index_overflow",
                                )
                            })?,
                            transition.hold_action,
                        )?;
                    }
                }
            }
        }

        for record in &mut scratch.records {
            let (node_index, _, _, _) = self.state_layout.decode(record.state as usize);
            record.can_complete = self.graph.nodes[node_index].accepting();
        }
        for depth in (0..scratch.depth_records.len()).rev() {
            for cursor in 0..scratch.depth_records[depth].len() {
                let record_index = scratch.depth_records[depth][cursor];
                let record = scratch.records[record_index as usize];
                let (node_index, extra_draw, state_hold_code, hold_from_qb) =
                    self.state_layout.decode(record.state as usize);
                let node = self.graph.nodes[node_index];
                if node.accepting() || !node.live() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                let mut can_complete = false;
                'edges: for edge_index in edge_start..edge_end {
                    let edge = self.graph.edges[edge_index];
                    let transitions = self.original_transitions(
                        node,
                        edge,
                        extra_draw,
                        state_hold_code,
                        hold_from_qb,
                        pattern_bit,
                        word_index,
                    );
                    for transition in transitions.iter() {
                        let Some(target_record) = scratch.record_for_state(transition.target)
                        else {
                            continue;
                        };
                        if scratch.records[target_record as usize].can_complete {
                            can_complete = true;
                            break 'edges;
                        }
                    }
                }
                scratch.records[record_index as usize].can_complete = can_complete;
            }
        }

        let target_outputs = targets
            .iter()
            .map(|(output_index, shape_index)| (*shape_index as u32, *output_index))
            .collect::<HashMap<_, _>>();
        let mut selected_records = HashMap::<u32, (u32, u8)>::new();
        for record_index in 0..scratch.records.len() {
            let record = scratch.records[record_index];
            if !record.can_complete {
                continue;
            }
            let (node_index, extra_draw, _, hold_from_qb) =
                self.state_layout.decode(record.state as usize);
            if !self.state_layout.accepts_setup_candidate(
                self.initial_cursor,
                self.graph.nodes[node_index].depth,
                extra_draw,
                hold_from_qb,
            ) {
                continue;
            }
            let Some(shape_index) = self.graph.nodes[node_index].shape_index() else {
                continue;
            };
            if !target_outputs.contains_key(&shape_index) {
                continue;
            }
            let candidate_depth = self.graph.nodes[node_index].depth;
            let should_replace =
                selected_records
                    .get(&shape_index)
                    .is_none_or(|(_, selected_depth)| {
                        prefers_setup_representative_depth(
                            self.candidate_priority,
                            self.length_preference,
                            candidate_depth,
                            *selected_depth,
                        )
                    });
            if should_replace {
                selected_records.insert(shape_index, (record_index as u32, candidate_depth));
            }
        }
        if selected_records.len() != target_outputs.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_witness_path_reconstruction_failed",
            ));
        }
        for (shape_index, output_index) in target_outputs {
            let (record_index, _) = selected_records[&shape_index];
            paths[output_index] = self.reconstruct_representative_path(record_index, scratch)?;
        }
        Ok(())
    }

    fn reconstruct_representative_path(
        &self,
        mut record_index: u32,
        scratch: &RepresentativeScratch,
    ) -> Result<Vec<crate::CorePathStep>, WasmExactSearchError> {
        let mut path = Vec::new();
        loop {
            let record = scratch.records.get(record_index as usize).copied().ok_or(
                WasmExactSearchError::InvalidProblem("setup_witness_record_out_of_range"),
            )?;
            if record.previous == NO_REPRESENTATIVE_RECORD {
                break;
            }
            let edge = self
                .graph
                .edges
                .get(record.edge_index as usize)
                .copied()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_witness_edge_out_of_range",
                ))?;
            path.push(crate::CorePathStep::new(
                edge.piece,
                edge.rotation(),
                i32::from(edge.x),
                i32::from(edge.y),
                record.hold_action.label(),
                edge.cleared_lines(),
            ));
            record_index = record.previous;
        }
        path.reverse();
        Ok(path)
    }
}

fn merge_exact_state_coverage(
    shape_build: &mut u64,
    shape_joint: &mut u64,
    forward: u64,
    backward: u64,
    active_patterns: u64,
) -> u64 {
    let build = forward & active_patterns;
    let joint = build & backward;
    *shape_build |= build;
    *shape_joint |= joint;
    joint
}

#[inline]
fn check_setup_coverage_cancel(
    control: &ExecutionControl,
    work: &mut usize,
) -> Result<(), WasmExactSearchError> {
    *work = work.wrapping_add(1);
    if *work & 4095 == 0 && control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn covered_word_weight(
    pattern_index: &PatternPiecePositionIndex,
    weights: &WeightedPatternSet,
    word_index: usize,
    mut word: u64,
) -> f64 {
    let mut total = 0.0;
    while word != 0 {
        let bit = word.trailing_zeros() as usize;
        word &= word - 1;
        let local_pattern = word_index * u64::BITS as usize + bit;
        let Some(global_pattern) = pattern_index.global_pattern_index(local_pattern) else {
            continue;
        };
        let Some(weight) = weights.weight(clearra_coverage::pattern::pattern_id::PatternId::new(
            global_pattern,
        )) else {
            continue;
        };
        total += weight.get();
    }
    total
}

pub(super) fn probability_string(value: f64) -> String {
    let mut output = format!("{:.12}", value.clamp(0.0, 1.0));
    while output.ends_with('0') {
        output.pop();
    }
    if output.ends_with('.') {
        output.push('0');
    }
    output
}

#[inline]
pub(super) fn include_setup_depth_range(min_depth: &mut u8, max_depth: &mut u8, depth: u8) {
    *min_depth = (*min_depth).min(depth);
    *max_depth = (*max_depth).max(depth);
}

#[allow(clippy::too_many_arguments)]
#[inline]
pub(super) fn compare_setup_candidates(
    priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    left_build: f64,
    left_joint: f64,
    left_min_locks: u8,
    left_max_locks: u8,
    left_board: u64,
    right_build: f64,
    right_joint: f64,
    right_min_locks: u8,
    right_max_locks: u8,
    right_board: u64,
) -> Ordering {
    let length_preference = length_preference.resolve(priority);
    let length_order = || match length_preference {
        SetupLengthPreference::Longer => right_max_locks.cmp(&left_max_locks),
        SetupLengthPreference::Shorter => left_min_locks.cmp(&right_min_locks),
        SetupLengthPreference::Auto => unreachable!("setup length preference is resolved"),
    };
    let candidate_order = match priority {
        SetupCandidatePriority::All => right_joint
            .total_cmp(&left_joint)
            .then_with(length_order)
            .then_with(|| right_build.total_cmp(&left_build)),
        SetupCandidatePriority::BuildProbabilityFirst => right_build
            .total_cmp(&left_build)
            .then_with(length_order)
            .then_with(|| {
                conditional_pc_probability(right_build, right_joint)
                    .total_cmp(&conditional_pc_probability(left_build, left_joint))
            }),
        SetupCandidatePriority::PcProbabilityFirst => {
            conditional_pc_probability(right_build, right_joint)
                .total_cmp(&conditional_pc_probability(left_build, left_joint))
                .then_with(length_order)
                .then_with(|| right_build.total_cmp(&left_build))
        }
    };
    candidate_order.then_with(|| left_board.cmp(&right_board))
}

pub(super) const fn prefers_setup_representative_depth(
    priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    candidate_depth: u8,
    selected_depth: u8,
) -> bool {
    match length_preference.resolve(priority) {
        SetupLengthPreference::Longer => candidate_depth > selected_depth,
        SetupLengthPreference::Shorter => candidate_depth < selected_depth,
        SetupLengthPreference::Auto => false,
    }
}

fn conditional_pc_probability(build: f64, joint: f64) -> f64 {
    if build == 0.0 {
        0.0
    } else {
        joint / build
    }
}

fn setup_candidate_set_hash(results: &[CompletedSetupCoverage]) -> String {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut condition_order = (0..results.len()).collect::<Vec<_>>();
    condition_order.sort_unstable_by(|left, right| {
        results[*left]
            .report
            .condition_id()
            .cmp(results[*right].report.condition_id())
    });
    let mut hash = FNV_OFFSET;
    for condition_index in condition_order {
        let result = &results[condition_index];
        let condition_id = result.report.condition_id();
        for board in &result.candidate_boards {
            for byte in condition_id.bytes().chain(core::iter::once(b'|')) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            for shift in (0..10).rev() {
                let nibble = ((board >> (shift * 4)) & 0x0f) as u8;
                let byte = if nibble < 10 {
                    b'0' + nibble
                } else {
                    b'a' + nibble - 10
                };
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("css1:{hash:016x}")
}

#[cfg(test)]
#[path = "setup_finder_tests.rs"]
mod tests;
