use std::collections::HashMap;

use clearra_problem::{SetupCandidatePriority, SetupLengthPreference, SetupSearchCondition};
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use crate::CorePathStep;

use super::{
    piece_index,
    setup_finder::{
        prefers_setup_representative_depth, setup_supply_transitions, SetupHoldAction,
        SetupSupplyStateLayout,
    },
    setup_partial_build::{PartialBuildEdge, PartialBuildGraph, PartialBuildNode},
    WasmExactSearchError,
};

const NO_RECORD: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub(super) struct SetupWitness {
    pub(super) pattern_id: u32,
}

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
        state_records.resize(state_capacity, NO_RECORD);
        Ok(Self {
            state_records,
            records: Vec::new(),
            depth_records: (0..=10).map(|_| Vec::new()).collect(),
        })
    }

    fn clear(&mut self) {
        for record in &self.records {
            self.state_records[record.state as usize] = NO_RECORD;
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
        if *slot != NO_RECORD {
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
            .filter(|record| *record != NO_RECORD)
    }
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

pub(super) struct SetupRepresentativeResolver<'a> {
    graph: &'a PartialBuildGraph,
    pattern_index: PatternPiecePositionIndex,
    initial_hold: Option<clearra_core_domain::piece::piece_kind::PieceKind>,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    initial_cursor: u16,
    state_layout: SetupSupplyStateLayout,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
}

impl<'a> SetupRepresentativeResolver<'a> {
    pub(super) fn new(
        condition: &SetupSearchCondition,
        graph: &'a PartialBuildGraph,
        candidate_priority: SetupCandidatePriority,
        length_preference: SetupLengthPreference,
    ) -> Result<Self, WasmExactSearchError> {
        let problem = condition.problem();
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("setup_pattern_universe_not_materialized"),
        )?;
        let pattern_index = PatternPiecePositionIndex::compile(universe).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_pattern_index_compile_failed")
        })?;
        Ok(Self {
            graph,
            pattern_index,
            initial_hold: condition.initial_hold(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: problem.supply().projects_standard_bag_lookahead(),
            initial_cursor: problem.initial_hold().cursor(),
            state_layout: SetupSupplyStateLayout::new(
                condition.queue_based_queue_start(),
                condition.queue_based_queue_len(),
            ),
            candidate_priority,
            length_preference,
        })
    }

    pub(super) fn paths(
        &self,
        targets: &[(usize, SetupWitness)],
    ) -> Result<Vec<Vec<CorePathStep>>, WasmExactSearchError> {
        let mut paths = vec![Vec::new(); targets.len()];
        let mut groups = HashMap::<u32, Vec<(usize, usize)>>::new();
        for (output_index, (shape_index, witness)) in targets.iter().copied().enumerate() {
            groups
                .entry(witness.pattern_id)
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
        for (pattern_id, pattern_targets) in groups {
            self.paths_for_pattern(pattern_id, &pattern_targets, &mut paths, &mut scratch)?;
        }
        Ok(paths)
    }

    fn paths_for_pattern(
        &self,
        pattern_id: u32,
        targets: &[(usize, usize)],
        paths: &mut [Vec<CorePathStep>],
        scratch: &mut RepresentativeScratch,
    ) -> Result<(), WasmExactSearchError> {
        scratch.clear();
        let word_index = pattern_id as usize / u64::BITS as usize;
        let pattern_bit = 1_u64 << (pattern_id % u64::BITS);
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
        scratch.activate(root, 0, NO_RECORD, NO_RECORD, SetupHoldAction::UseCurrent)?;

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
        for (record_index, record) in scratch.records.iter().copied().enumerate() {
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
            paths[output_index] = self.reconstruct_path(record_index, scratch)?;
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

    fn reconstruct_path(
        &self,
        mut record_index: u32,
        scratch: &RepresentativeScratch,
    ) -> Result<Vec<CorePathStep>, WasmExactSearchError> {
        let mut path = Vec::new();
        loop {
            let record = scratch.records.get(record_index as usize).copied().ok_or(
                WasmExactSearchError::InvalidProblem("setup_witness_record_out_of_range"),
            )?;
            if record.previous == NO_RECORD {
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
            path.push(CorePathStep::new(
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
