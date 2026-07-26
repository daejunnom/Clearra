use std::{collections::HashSet, sync::Arc};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_problem::SetupSearchCondition;
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use crate::CorePathStep;

use super::{
    piece_index,
    setup_finder::{
        covered_word_weight, include_setup_depth_range, SetupHoldAction, SetupSupplyStateLayout,
        SetupSupplyTransitionCatalog, COVERAGE_WORD_LANES,
    },
    setup_parallel_segmented::SegmentedGenerationArray,
    setup_partial_build::PartialBuildGraph,
    WasmExactSearchError,
};

const EMPTY_WORDS: [u64; COVERAGE_WORD_LANES] = [0; COVERAGE_WORD_LANES];
const NO_WITNESS: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub(super) struct SetupAllPathNode {
    pub(super) board: u64,
    pub(super) edge_start: u32,
    pub(super) edge_count: u32,
    pub(super) shape_index: u32,
    pub(super) depth: u8,
    pub(super) live: bool,
    pub(super) accepting: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SetupAllPathEdge {
    pub(super) to: u32,
    pub(super) piece: PieceKind,
    pub(super) rotation: u8,
    pub(super) x: i8,
    pub(super) y: i8,
    pub(super) cleared_lines: u8,
}

#[derive(Clone, Debug)]
pub(super) struct SetupAllPathGraph {
    pub(super) nodes: Vec<SetupAllPathNode>,
    pub(super) edges: Vec<SetupAllPathEdge>,
    pub(super) root: u32,
}

impl SetupAllPathGraph {
    pub(super) fn from_partial(graph: &PartialBuildGraph) -> Self {
        Self {
            nodes: graph
                .nodes
                .iter()
                .map(|node| SetupAllPathNode {
                    board: node.board,
                    edge_start: node.edge_start,
                    edge_count: node.edge_count,
                    shape_index: node.shape_index().unwrap_or(u32::MAX),
                    depth: node.depth,
                    live: node.live(),
                    accepting: node.accepting(),
                })
                .collect(),
            edges: graph
                .edges
                .iter()
                .map(|edge| SetupAllPathEdge {
                    to: edge.to,
                    piece: edge.piece,
                    rotation: edge.rotation(),
                    x: edge.x,
                    y: edge.y,
                    cleared_lines: edge.cleared_lines(),
                })
                .collect(),
            root: graph.root,
        }
    }

    pub(super) fn from_wire_parts(
        nodes: Vec<SetupAllPathNode>,
        edges: Vec<SetupAllPathEdge>,
        root: u32,
    ) -> Result<Self, WasmExactSearchError> {
        if root as usize >= nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_all_paths_root_out_of_range",
            ));
        }
        for node in &nodes {
            let end = (node.edge_start as usize)
                .checked_add(node.edge_count as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_edge_range_overflow",
                ))?;
            if end > edges.len() || node.depth > 10 {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_node_invalid",
                ));
            }
        }
        if edges.iter().any(|edge| {
            edge.to as usize >= nodes.len() || edge.rotation > 3 || edge.cleared_lines > 7
        }) {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_all_paths_edge_invalid",
            ));
        }
        Ok(Self { nodes, edges, root })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionStep {
    pub(super) piece: PieceKind,
    pub(super) rotation: u8,
    pub(super) x: i8,
    pub(super) y: i8,
    pub(super) hold_action: SetupHoldAction,
    pub(super) cleared_lines: u8,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionPath {
    pub(super) steps: Vec<SetupSolutionStep>,
}

impl SetupSolutionPath {
    pub(super) fn into_core_path(self) -> Vec<CorePathStep> {
        self.steps
            .into_iter()
            .map(|step| {
                CorePathStep::new(
                    step.piece,
                    step.rotation,
                    i32::from(step.x),
                    i32::from(step.y),
                    step.hold_action.label(),
                    step.cleared_lines,
                )
            })
            .collect()
    }
}

#[derive(Clone, Copy, Default)]
struct PathStateCoverage {
    alpha: [u64; COVERAGE_WORD_LANES],
    beta: [u64; COVERAGE_WORD_LANES],
}

#[derive(Clone, Copy, Default)]
struct PathTransition {
    target: usize,
    hold_action: SetupHoldAction,
    mask: [u64; COVERAGE_WORD_LANES],
}

#[derive(Clone, Copy)]
pub(super) struct SetupAllPathCoverage {
    pub(super) shape_index: u32,
    pub(super) build_covered_patterns: u32,
    pub(super) joint_covered_patterns: u32,
    pub(super) build_weight: f64,
    pub(super) joint_weight: f64,
    pub(super) min_covered_locks: u8,
    pub(super) max_covered_locks: u8,
    pub(super) witness_pattern_id: u32,
}

impl SetupAllPathCoverage {
    fn new(shape_index: u32) -> Self {
        Self {
            shape_index,
            build_covered_patterns: 0,
            joint_covered_patterns: 0,
            build_weight: 0.0,
            joint_weight: 0.0,
            min_covered_locks: u8::MAX,
            max_covered_locks: 0,
            witness_pattern_id: NO_WITNESS,
        }
    }
}

pub(super) struct SetupAllPathEnumerator {
    graph: Arc<SetupAllPathGraph>,
    target_shape_index: u32,
    target_reachable: Vec<bool>,
    pattern_index: PatternPiecePositionIndex,
    weights: WeightedPatternSet,
    initial_hold_code: u8,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    initial_cursor: u16,
    state_layout: SetupSupplyStateLayout,
    states: SegmentedGenerationArray<PathStateCoverage>,
    depth_states: Vec<Vec<usize>>,
    touched_states: Vec<usize>,
    paths: HashSet<SetupSolutionPath>,
    coverage: SetupAllPathCoverage,
    peak_segment_pages: usize,
}

impl SetupAllPathEnumerator {
    pub(super) fn new(
        condition: &SetupSearchCondition,
        graph: Arc<SetupAllPathGraph>,
        target_shape_index: u32,
    ) -> Result<Self, WasmExactSearchError> {
        if !graph
            .nodes
            .iter()
            .any(|node| node.shape_index == target_shape_index)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_all_paths_target_shape_missing",
            ));
        }
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
        let state_capacity = state_layout.state_capacity(graph.nodes.len()).ok_or(
            WasmExactSearchError::InvalidProblem("setup_all_paths_state_capacity_overflow"),
        )?;
        let target_reachable = compile_target_reachability(&graph, target_shape_index);
        let mut paths = HashSet::new();
        paths.try_reserve(256).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_result_storage_unavailable")
        })?;
        Ok(Self {
            graph,
            target_shape_index,
            target_reachable,
            pattern_index,
            weights: universe.weights().clone(),
            initial_hold_code: condition
                .initial_hold()
                .map_or(0, |piece| piece_index(piece) as u8 + 1),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: problem.supply().projects_standard_bag_lookahead(),
            initial_cursor: problem.initial_hold().cursor(),
            state_layout,
            states: SegmentedGenerationArray::new(state_capacity)?,
            depth_states: (0..=10).map(|_| Vec::new()).collect(),
            touched_states: Vec::new(),
            paths,
            coverage: SetupAllPathCoverage::new(target_shape_index),
            peak_segment_pages: 0,
        })
    }

    pub(super) fn run_word_range(
        &mut self,
        word_start: usize,
        word_end: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if word_start >= word_end || word_end > self.pattern_index.word_count() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_all_paths_word_range_invalid",
            ));
        }
        let mut cursor = word_start;
        while cursor < word_end {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let lane_count = (word_end - cursor).min(COVERAGE_WORD_LANES);
            self.process_word_block(cursor, lane_count, control)?;
            cursor += lane_count;
        }
        Ok(())
    }

    pub(super) fn peak_segment_pages(&self) -> usize {
        self.peak_segment_pages
    }

    pub(super) fn take_coverage(&mut self) -> SetupAllPathCoverage {
        std::mem::replace(
            &mut self.coverage,
            SetupAllPathCoverage::new(self.target_shape_index),
        )
    }

    pub(super) fn into_paths(self) -> Vec<SetupSolutionPath> {
        let mut paths = self.paths.into_iter().collect::<Vec<_>>();
        paths.sort_unstable();
        paths
    }

    pub(super) fn drain_paths(&mut self) -> Result<Vec<SetupSolutionPath>, WasmExactSearchError> {
        let mut replacement = HashSet::new();
        replacement.try_reserve(256).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_result_storage_unavailable")
        })?;
        let mut paths = std::mem::replace(&mut self.paths, replacement)
            .into_iter()
            .collect::<Vec<_>>();
        paths.sort_unstable();
        Ok(paths)
    }

    fn process_word_block(
        &mut self,
        word_start: usize,
        lane_count: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.states.begin_generation();
        self.touched_states.clear();
        for states in &mut self.depth_states {
            states.clear();
        }
        let mut root_bits = EMPTY_WORDS;
        for (lane, bits) in root_bits.iter_mut().enumerate().take(lane_count) {
            *bits = self.pattern_index.active_word(word_start + lane);
        }
        let catalog = SetupSupplyTransitionCatalog::compile(
            &self.pattern_index,
            self.initial_cursor,
            self.hold_enabled,
            self.projects_unplaced_lookahead,
            self.projects_standard_bag_lookahead,
            word_start,
            root_bits,
            lane_count,
        )?;
        let root =
            self.state_layout
                .encode(self.graph.root as usize, 0, self.initial_hold_code, false);
        self.activate_alpha(root, root_bits)?;
        let mut cancellation_work = 0_usize;

        for depth in 0..self.depth_states.len() {
            let mut cursor = 0;
            while cursor < self.depth_states[depth].len() {
                check_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                cursor += 1;
                let active = self
                    .states
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                let (node_index, extra_draw, hold_code, hold_from_qb) =
                    self.state_layout.decode(state_index);
                let node = self.graph.nodes[node_index];
                if node.accepting || !node.live {
                    continue;
                }
                for edge_index in
                    node.edge_start as usize..node.edge_start as usize + node.edge_count as usize
                {
                    let edge = self.graph.edges[edge_index];
                    let target_node = self.graph.nodes[edge.to as usize];
                    for lane in 0..lane_count {
                        for transition in catalog
                            .get(
                                node.depth,
                                piece_index(edge.piece) as u8 + 1,
                                extra_draw,
                                hold_code,
                                target_node.accepting,
                                lane,
                            )
                            .iter()
                        {
                            let next_hold_from_qb = self.state_layout.next_hold_provenance(
                                self.initial_cursor,
                                node.depth,
                                extra_draw,
                                hold_from_qb,
                                transition.hold_action,
                            );
                            self.activate_alpha_lane(
                                self.state_layout.encode(
                                    edge.to as usize,
                                    transition.extra_draw,
                                    transition.hold_code,
                                    next_hold_from_qb,
                                ),
                                lane,
                                active[lane] & transition.mask,
                            )?;
                        }
                    }
                }
            }
        }

        for cursor in 0..self.touched_states.len() {
            let state_index = self.touched_states[cursor];
            let (node_index, _, _, _) = self.state_layout.decode(state_index);
            if self.graph.nodes[node_index].accepting {
                let alpha = self
                    .states
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                self.activate_beta(state_index, alpha)?;
            }
        }
        for depth in (0..self.depth_states.len()).rev() {
            for cursor in 0..self.depth_states[depth].len() {
                check_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                let active = self
                    .states
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                let (node_index, extra_draw, hold_code, hold_from_qb) =
                    self.state_layout.decode(state_index);
                let node = self.graph.nodes[node_index];
                if node.accepting || !node.live {
                    continue;
                }
                let mut successful = EMPTY_WORDS;
                for edge_index in
                    node.edge_start as usize..node.edge_start as usize + node.edge_count as usize
                {
                    let edge = self.graph.edges[edge_index];
                    let target_node = self.graph.nodes[edge.to as usize];
                    for lane in 0..lane_count {
                        for transition in catalog
                            .get(
                                node.depth,
                                piece_index(edge.piece) as u8 + 1,
                                extra_draw,
                                hold_code,
                                target_node.accepting,
                                lane,
                            )
                            .iter()
                        {
                            let next_hold_from_qb = self.state_layout.next_hold_provenance(
                                self.initial_cursor,
                                node.depth,
                                extra_draw,
                                hold_from_qb,
                                transition.hold_action,
                            );
                            let target = self.state_layout.encode(
                                edge.to as usize,
                                transition.extra_draw,
                                transition.hold_code,
                                next_hold_from_qb,
                            );
                            let backward =
                                self.states.get(target).map_or(0, |state| state.beta[lane]);
                            successful[lane] |= active[lane] & transition.mask & backward;
                        }
                    }
                }
                self.activate_beta(state_index, successful)?;
            }
        }

        let mut target_build = EMPTY_WORDS;
        let mut target_joint = EMPTY_WORDS;
        let mut min_covered_locks = u8::MAX;
        let mut max_covered_locks = 0;
        for cursor in 0..self.touched_states.len() {
            let state_index = self.touched_states[cursor];
            let (node_index, extra_draw, _, hold_from_qb) = self.state_layout.decode(state_index);
            let node = self.graph.nodes[node_index];
            if node.shape_index != self.target_shape_index
                || !self.state_layout.accepts_setup_candidate(
                    self.initial_cursor,
                    node.depth,
                    extra_draw,
                    hold_from_qb,
                )
            {
                continue;
            }
            let state = self.states.get(state_index).copied().unwrap_or_default();
            for lane in 0..lane_count {
                let build = state.alpha[lane] & root_bits[lane];
                let joint = build & state.beta[lane];
                target_build[lane] |= build;
                target_joint[lane] |= joint;
                if joint != 0 {
                    include_setup_depth_range(
                        &mut min_covered_locks,
                        &mut max_covered_locks,
                        node.depth,
                    );
                }
            }
        }
        if min_covered_locks != u8::MAX {
            include_setup_depth_range(
                &mut self.coverage.min_covered_locks,
                &mut self.coverage.max_covered_locks,
                min_covered_locks,
            );
            include_setup_depth_range(
                &mut self.coverage.min_covered_locks,
                &mut self.coverage.max_covered_locks,
                max_covered_locks,
            );
        }
        for lane in 0..lane_count {
            self.coverage.build_covered_patterns = self
                .coverage
                .build_covered_patterns
                .checked_add(target_build[lane].count_ones())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_build_coverage_count_overflow",
                ))?;
            self.coverage.joint_covered_patterns = self
                .coverage
                .joint_covered_patterns
                .checked_add(target_joint[lane].count_ones())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_joint_coverage_count_overflow",
                ))?;
            self.coverage.build_weight += covered_word_weight(
                &self.pattern_index,
                &self.weights,
                word_start + lane,
                target_build[lane],
            );
            self.coverage.joint_weight += covered_word_weight(
                &self.pattern_index,
                &self.weights,
                word_start + lane,
                target_joint[lane],
            );
            if target_joint[lane] != 0 && self.coverage.witness_pattern_id == NO_WITNESS {
                let local = target_joint[lane].trailing_zeros() as usize;
                self.coverage.witness_pattern_id = u32::try_from((word_start + lane) * 64 + local)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "setup_all_paths_witness_pattern_overflow",
                        )
                    })?;
            }
        }

        let states = &self.states;
        let mut path = Vec::new();
        let mut context = EnumerateContext {
            graph: &self.graph,
            target_shape_index: self.target_shape_index,
            target_reachable: &self.target_reachable,
            states,
            state_layout: self.state_layout,
            initial_cursor: self.initial_cursor,
            catalog: &catalog,
            lane_count,
            control,
            cancellation_work: &mut cancellation_work,
            paths: &mut self.paths,
        };
        enumerate_paths(&mut context, root, root_bits, &mut path)?;
        self.peak_segment_pages = self.peak_segment_pages.max(self.states.active_page_count());
        Ok(())
    }

    fn activate_alpha(
        &mut self,
        state_index: usize,
        mask: [u64; COVERAGE_WORD_LANES],
    ) -> Result<(), WasmExactSearchError> {
        if mask == EMPTY_WORDS {
            return Ok(());
        }
        let (state, first) = self.states.get_mut_or_default(state_index)?;
        let was_empty = first || state.alpha == EMPTY_WORDS;
        for lane in 0..COVERAGE_WORD_LANES {
            state.alpha[lane] |= mask[lane];
        }
        if was_empty {
            let (node, _, _, _) = self.state_layout.decode(state_index);
            self.depth_states[self.graph.nodes[node].depth as usize].push(state_index);
            self.touched_states.push(state_index);
        }
        Ok(())
    }

    fn activate_alpha_lane(
        &mut self,
        state_index: usize,
        lane: usize,
        mask: u64,
    ) -> Result<(), WasmExactSearchError> {
        if mask == 0 {
            return Ok(());
        }
        let (state, first) = self.states.get_mut_or_default(state_index)?;
        let was_empty = first || state.alpha == EMPTY_WORDS;
        state.alpha[lane] |= mask;
        if was_empty {
            let (node, _, _, _) = self.state_layout.decode(state_index);
            self.depth_states[self.graph.nodes[node].depth as usize].push(state_index);
            self.touched_states.push(state_index);
        }
        Ok(())
    }

    fn activate_beta(
        &mut self,
        state_index: usize,
        mask: [u64; COVERAGE_WORD_LANES],
    ) -> Result<(), WasmExactSearchError> {
        if mask == EMPTY_WORDS {
            return Ok(());
        }
        let (state, _) = self.states.get_mut_or_default(state_index)?;
        for lane in 0..COVERAGE_WORD_LANES {
            state.beta[lane] |= mask[lane];
        }
        Ok(())
    }
}

struct EnumerateContext<'a> {
    graph: &'a SetupAllPathGraph,
    target_shape_index: u32,
    target_reachable: &'a [bool],
    states: &'a SegmentedGenerationArray<PathStateCoverage>,
    state_layout: SetupSupplyStateLayout,
    initial_cursor: u16,
    catalog: &'a SetupSupplyTransitionCatalog,
    lane_count: usize,
    control: &'a ExecutionControl,
    cancellation_work: &'a mut usize,
    paths: &'a mut HashSet<SetupSolutionPath>,
}

fn enumerate_paths(
    context: &mut EnumerateContext<'_>,
    state_index: usize,
    active: [u64; COVERAGE_WORD_LANES],
    path: &mut Vec<SetupSolutionStep>,
) -> Result<(), WasmExactSearchError> {
    check_cancel(context.control, context.cancellation_work)?;
    let (node_index, extra_draw, hold_code, hold_from_qb) =
        context.state_layout.decode(state_index);
    let node = context.graph.nodes[node_index];
    if node.shape_index == context.target_shape_index
        && context.state_layout.accepts_setup_candidate(
            context.initial_cursor,
            node.depth,
            extra_draw,
            hold_from_qb,
        )
        && active.iter().any(|word| *word != 0)
    {
        reserve_path_slot(context.paths)?;
        context.paths.insert(SetupSolutionPath {
            steps: path.clone(),
        });
        return Ok(());
    }
    if node.accepting || !node.live || !context.target_reachable[node_index] {
        return Ok(());
    }

    for edge_index in node.edge_start as usize..node.edge_start as usize + node.edge_count as usize
    {
        let edge = context.graph.edges[edge_index];
        if !context.target_reachable[edge.to as usize] {
            continue;
        }
        let target_node = context.graph.nodes[edge.to as usize];
        let mut transitions = Vec::<PathTransition>::new();
        transitions.try_reserve(18).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_transition_storage_unavailable")
        })?;
        for lane in 0..context.lane_count {
            for transition in context
                .catalog
                .get(
                    node.depth,
                    piece_index(edge.piece) as u8 + 1,
                    extra_draw,
                    hold_code,
                    target_node.accepting,
                    lane,
                )
                .iter()
            {
                let next_hold_from_qb = context.state_layout.next_hold_provenance(
                    context.initial_cursor,
                    node.depth,
                    extra_draw,
                    hold_from_qb,
                    transition.hold_action,
                );
                let target = context.state_layout.encode(
                    edge.to as usize,
                    transition.extra_draw,
                    transition.hold_code,
                    next_hold_from_qb,
                );
                let backward = context
                    .states
                    .get(target)
                    .map_or(0, |state| state.beta[lane]);
                let mask = active[lane] & transition.mask & backward;
                if mask == 0 {
                    continue;
                }
                let slot = transitions
                    .iter()
                    .position(|candidate| {
                        candidate.target == target
                            && candidate.hold_action == transition.hold_action
                    })
                    .unwrap_or(transitions.len());
                if slot == transitions.len() {
                    transitions.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "setup_all_paths_transition_storage_unavailable",
                        )
                    })?;
                    transitions.push(PathTransition {
                        target,
                        hold_action: transition.hold_action,
                        mask: EMPTY_WORDS,
                    });
                }
                transitions[slot].mask[lane] |= mask;
            }
        }
        for transition in transitions.iter().copied() {
            path.push(SetupSolutionStep {
                piece: edge.piece,
                rotation: edge.rotation,
                x: edge.x,
                y: edge.y,
                hold_action: transition.hold_action,
                cleared_lines: edge.cleared_lines,
            });
            enumerate_paths(context, transition.target, transition.mask, path)?;
            path.pop();
        }
    }
    Ok(())
}

fn compile_target_reachability(graph: &SetupAllPathGraph, target_shape_index: u32) -> Vec<bool> {
    let mut reachable = graph
        .nodes
        .iter()
        .map(|node| node.shape_index == target_shape_index)
        .collect::<Vec<_>>();
    for depth in (0..=10_u8).rev() {
        for (node_index, node) in graph.nodes.iter().copied().enumerate() {
            if node.depth != depth || reachable[node_index] {
                continue;
            }
            reachable[node_index] = graph.edges
                [node.edge_start as usize..node.edge_start as usize + node.edge_count as usize]
                .iter()
                .any(|edge| reachable[edge.to as usize]);
        }
    }
    reachable
}

fn reserve_path_slot(paths: &mut HashSet<SetupSolutionPath>) -> Result<(), WasmExactSearchError> {
    if paths.len() == paths.capacity() {
        paths.try_reserve(paths.capacity().max(256)).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_result_storage_unavailable")
        })?;
    }
    Ok(())
}

#[inline]
fn check_cancel(control: &ExecutionControl, work: &mut usize) -> Result<(), WasmExactSearchError> {
    *work = work.wrapping_add(1);
    if *work & 4095 == 0 && control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}
