use std::collections::HashMap;

use clearra_problem::{
    SetupCandidatePriority, SetupLengthPreference, SetupSearchCondition, SetupTerminalSupplyTarget,
};
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use crate::CorePathStep;

use super::{
    piece_index,
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::{
        compile_setup_pattern_index, prefers_setup_representative_depth, setup_supply_transitions,
        terminal_supply_target_word, SetupHoldAction, SetupSupplyStateLayout,
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

#[derive(Clone, Copy)]
struct PrefixRepresentativeRecord {
    state: usize,
    previous: u32,
    edge_index: u32,
    hold_action: SetupHoldAction,
}

struct CoverageCompletionScratch {
    values: Vec<u8>,
    touched: Vec<usize>,
}

const COMPLETION_UNKNOWN: u8 = 0;
const COMPLETION_FALSE: u8 = 1;
const COMPLETION_TRUE: u8 = 2;

impl CoverageCompletionScratch {
    fn new(state_capacity: usize) -> Result<Self, WasmExactSearchError> {
        let mut values = Vec::new();
        values.try_reserve_exact(state_capacity).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_witness_completion_storage_unavailable")
        })?;
        values.resize(state_capacity, COMPLETION_UNKNOWN);
        Ok(Self {
            values,
            touched: Vec::new(),
        })
    }

    fn clear(&mut self) {
        for state in self.touched.drain(..) {
            self.values[state] = COMPLETION_UNKNOWN;
        }
    }

    fn get(&self, state: usize) -> Option<bool> {
        match self.values.get(state).copied()? {
            COMPLETION_FALSE => Some(false),
            COMPLETION_TRUE => Some(true),
            _ => None,
        }
    }

    fn set(&mut self, state: usize, value: bool) -> Result<(), WasmExactSearchError> {
        let slot = self
            .values
            .get_mut(state)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_witness_completion_state_out_of_range",
            ))?;
        if *slot == COMPLETION_UNKNOWN {
            self.touched.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_witness_completion_index_unavailable")
            })?;
            self.touched.push(state);
        }
        *slot = if value {
            COMPLETION_TRUE
        } else {
            COMPLETION_FALSE
        };
        Ok(())
    }
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
    coverage_graph: &'a SetupCoverageGraph,
    pattern_index: PatternPiecePositionIndex,
    initial_hold: Option<clearra_core_domain::piece::piece_kind::PieceKind>,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    initial_cursor: u16,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    state_layout: SetupSupplyStateLayout,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    max_setup_pieces: u8,
}

impl<'a> SetupRepresentativeResolver<'a> {
    pub(super) fn new(
        condition: &SetupSearchCondition,
        graph: &'a PartialBuildGraph,
        coverage_graph: &'a SetupCoverageGraph,
        candidate_priority: SetupCandidatePriority,
        length_preference: SetupLengthPreference,
        max_setup_pieces: u8,
    ) -> Result<Self, WasmExactSearchError> {
        let problem = condition.problem();
        let pattern_index = compile_setup_pattern_index(condition)?;
        Ok(Self {
            graph,
            coverage_graph,
            pattern_index,
            initial_hold: condition.initial_hold(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: problem.supply().projects_standard_bag_lookahead(),
            initial_cursor: problem.initial_hold().cursor(),
            terminal_supply_target: condition.terminal_supply_target(),
            state_layout: SetupSupplyStateLayout::new(),
            candidate_priority,
            length_preference,
            max_setup_pieces,
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
        if self.graph.uses_compact_continuation() {
            let state_capacity = self
                .state_layout
                .state_capacity(self.coverage_graph.nodes.len())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_witness_completion_state_capacity_overflow",
                ))?;
            let mut completion = CoverageCompletionScratch::new(state_capacity)?;
            for (pattern_id, pattern_targets) in groups {
                completion.clear();
                let word_index = pattern_id as usize / u64::BITS as usize;
                let pattern_bit = 1_u64 << (pattern_id % u64::BITS);
                self.prefix_paths_for_pattern(
                    word_index,
                    pattern_bit,
                    &pattern_targets,
                    &mut paths,
                    &mut completion,
                )?;
            }
            return Ok(paths);
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
            .encode(self.graph.root as usize, 0, hold_code);
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
                let (node_index, extra_draw, state_hold_code) =
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
                        pattern_bit,
                        word_index,
                    );
                    for transition in transitions.iter() {
                        let (target_node, _, _) = self.state_layout.decode(transition.target);
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
            let (node_index, extra_draw, hold_code) =
                self.state_layout.decode(record.state as usize);
            let node = self.graph.nodes[node_index];
            record.can_complete = node.accepting()
                && self.terminal_supply_target.is_none_or(|target| {
                    terminal_supply_target_word(
                        &self.pattern_index,
                        target,
                        self.initial_cursor,
                        node.depth,
                        extra_draw,
                        hold_code,
                        word_index,
                        pattern_bit,
                    ) != 0
                });
        }
        for depth in (0..scratch.depth_records.len()).rev() {
            for cursor in 0..scratch.depth_records[depth].len() {
                let record_index = scratch.depth_records[depth][cursor];
                let record = scratch.records[record_index as usize];
                let (node_index, extra_draw, state_hold_code) =
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
            let (node_index, _, _) = self.state_layout.decode(record.state as usize);
            let Some(shape_index) = self.graph.nodes[node_index].shape_index() else {
                continue;
            };
            if !target_outputs.contains_key(&shape_index) {
                continue;
            }
            let candidate_depth = self.graph.nodes[node_index].depth;
            if candidate_depth > self.max_setup_pieces {
                continue;
            }
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

    fn prefix_paths_for_pattern(
        &self,
        word_index: usize,
        pattern_bit: u64,
        targets: &[(usize, usize)],
        paths: &mut [Vec<CorePathStep>],
        completion_scratch: &mut CoverageCompletionScratch,
    ) -> Result<(), WasmExactSearchError> {
        let initial_hold_code = self
            .initial_hold
            .map_or(0, |piece| piece_index(piece) as u8 + 1);
        let root_state = self
            .state_layout
            .encode(self.graph.root as usize, 0, initial_hold_code);
        for (output_index, shape_index) in targets.iter().copied() {
            let target_node = self.graph.shape_target_node(shape_index).ok_or(
                WasmExactSearchError::InvalidProblem("setup_prefix_representative_target_missing"),
            )?;
            let target_depth = self
                .graph
                .nodes
                .get(target_node as usize)
                .map(|node| node.depth)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_prefix_representative_target_invalid",
                ))?;
            let mut records = vec![PrefixRepresentativeRecord {
                state: root_state,
                previous: NO_RECORD,
                edge_index: NO_RECORD,
                hold_action: SetupHoldAction::UseCurrent,
            }];
            let mut seen = HashMap::<usize, u32>::new();
            seen.insert(root_state, 0);
            let mut cursor = 0_usize;
            let mut selected = None;
            while cursor < records.len() {
                let record_index = cursor as u32;
                let record = records[cursor];
                cursor += 1;
                let (node_index, extra_draw, hold_code) = self.state_layout.decode(record.state);
                let node = self.graph.nodes[node_index];
                if node_index == target_node as usize {
                    let coverage_node = self.coverage_graph.source_class(node_index as u32).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "setup_prefix_representative_source_class_missing",
                        ),
                    )? as usize;
                    if self.coverage_can_complete(
                        coverage_node,
                        extra_draw,
                        hold_code,
                        word_index,
                        pattern_bit,
                        completion_scratch,
                    )? {
                        selected = Some(record_index);
                        break;
                    }
                }
                if node.depth >= target_depth {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                for edge_index in edge_start..edge_end {
                    let edge = self.graph.edges[edge_index];
                    for transition in self
                        .original_transitions(
                            node,
                            edge,
                            extra_draw,
                            hold_code,
                            pattern_bit,
                            word_index,
                        )
                        .iter()
                    {
                        if seen.contains_key(&transition.target) {
                            continue;
                        }
                        let next_index = u32::try_from(records.len()).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "setup_prefix_representative_record_overflow",
                            )
                        })?;
                        records.try_reserve(1).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "setup_prefix_representative_storage_unavailable",
                            )
                        })?;
                        seen.try_reserve(1).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "setup_prefix_representative_index_unavailable",
                            )
                        })?;
                        records.push(PrefixRepresentativeRecord {
                            state: transition.target,
                            previous: record_index,
                            edge_index: u32::try_from(edge_index).map_err(|_| {
                                WasmExactSearchError::InvalidProblem(
                                    "setup_witness_edge_index_overflow",
                                )
                            })?,
                            hold_action: transition.hold_action,
                        });
                        seen.insert(transition.target, next_index);
                    }
                }
            }
            let selected = selected.ok_or(WasmExactSearchError::InvalidProblem(
                "setup_witness_path_reconstruction_failed",
            ))?;
            paths[output_index] = self.reconstruct_prefix_path(selected, &records)?;
        }
        Ok(())
    }

    fn reconstruct_prefix_path(
        &self,
        mut record_index: u32,
        records: &[PrefixRepresentativeRecord],
    ) -> Result<Vec<CorePathStep>, WasmExactSearchError> {
        let mut path = Vec::new();
        loop {
            let record = records.get(record_index as usize).copied().ok_or(
                WasmExactSearchError::InvalidProblem("setup_prefix_representative_record_missing"),
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

    fn coverage_can_complete(
        &self,
        node_index: usize,
        extra_draw: u8,
        hold_code: u8,
        word_index: usize,
        pattern_bit: u64,
        scratch: &mut CoverageCompletionScratch,
    ) -> Result<bool, WasmExactSearchError> {
        let state = self.state_layout.encode(node_index, extra_draw, hold_code);
        if let Some(completion) = scratch.get(state) {
            return Ok(completion);
        }
        let node = *self.coverage_graph.nodes.get(node_index).ok_or(
            WasmExactSearchError::InvalidProblem("setup_witness_completion_node_out_of_range"),
        )?;
        let completion = if node.accepting() {
            self.terminal_supply_target.is_none_or(|target| {
                terminal_supply_target_word(
                    &self.pattern_index,
                    target,
                    self.initial_cursor,
                    node.depth,
                    extra_draw,
                    hold_code,
                    word_index,
                    pattern_bit,
                ) != 0
            })
        } else {
            let edge_start = node.edge_start as usize;
            let edge_end = edge_start + node.edge_count as usize;
            let mut found = false;
            'edges: for edge in &self.coverage_graph.edges[edge_start..edge_end] {
                let child = edge.child() as usize;
                let child_node = self.coverage_graph.nodes[child];
                let transitions = setup_supply_transitions(
                    &self.pattern_index,
                    self.initial_cursor,
                    self.hold_enabled,
                    self.projects_unplaced_lookahead,
                    self.projects_standard_bag_lookahead,
                    node.depth,
                    edge.piece_code(),
                    extra_draw,
                    hold_code,
                    pattern_bit,
                    word_index,
                    child_node.accepting(),
                );
                for transition in transitions.iter() {
                    if self.coverage_can_complete(
                        child,
                        transition.extra_draw,
                        transition.hold_code,
                        word_index,
                        pattern_bit,
                        scratch,
                    )? {
                        found = true;
                        break 'edges;
                    }
                }
            }
            found
        };
        scratch.set(state, completion)?;
        Ok(completion)
    }

    #[allow(clippy::too_many_arguments)]
    fn original_transitions(
        &self,
        node: PartialBuildNode,
        edge: PartialBuildEdge,
        extra_draw: u8,
        hold_code: u8,
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
            transitions.push(
                self.state_layout.encode(
                    edge.to as usize,
                    transition.extra_draw,
                    transition.hold_code,
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
