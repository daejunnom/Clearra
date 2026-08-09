// SRP rationale: this module has one behavior-level change reason: constructing and annotating exact extended-board BuildUp order languages.

use std::collections::HashMap;

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_finesse::{FinesseBoard, FinesseTarget, GeometryActionKey};
use clearra_problem::SearchProblem;
use clearra_replay::{ScoringExecutionEdge, ScoringExecutionNode, SpinCoverageExecutionGraph};

use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    buildup::{
        annotate_frozen_finesse_target_groups, finesse_b2b_policy,
        finesse_scoring_edge_requirement, finesse_terminal_label, push_frozen_finesse_target,
        BuildEdge, BuildOrderGraph, BuildOrderNodeSpec, FrozenFinesseTargetGroups,
        GroupedFinesseTarget, PreparedFinesseEdge, PreparedFinesseLanguage, PreparedFinesseNode,
    },
    extended_board::{compact_logical_board, merge_deleted_rows, place_and_clear, ExtendedBoard},
    extended_geometry::ExtendedGeometryCandidate,
    extended_inverse_catalog::ExtendedInverseCatalog,
    extended_reachability::{ExtendedReachabilityWorkspace, ExtendedReachableLocks},
    mix_digest, piece_index, WasmExactSearchError,
};

const NO_BUILD_NODE: u32 = u32::MAX;
type ScoringEdgeSlices<'a> = (&'a [(u32, u32)], &'a [ScoringExecutionEdge]);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ExtendedTilingKey {
    placements: Vec<ExtendedPlacementKey>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ExtendedPlacementKey {
    piece: PieceKind,
    cells: ExtendedBoard,
}

impl ExtendedTilingKey {
    pub fn empty() -> Self {
        Self {
            placements: Vec::new(),
        }
    }

    pub fn from_candidate(
        catalog: &ExtendedInverseCatalog,
        candidate: &ExtendedGeometryCandidate,
    ) -> Self {
        let mut placements = candidate
            .row_ids()
            .iter()
            .map(|row_id| {
                let row = catalog.skeleton(*row_id);
                ExtendedPlacementKey {
                    piece: row.piece,
                    cells: row.cells,
                }
            })
            .collect::<Vec<_>>();
        placements.sort_unstable();
        Self { placements }
    }

    pub fn digest(&self) -> u64 {
        let mut digest = 0_u64;
        for placement in &self.placements {
            digest = mix_digest(digest, piece_index(placement.piece) as u64);
            for word in placement.cells.words() {
                digest = mix_digest(digest, word);
            }
        }
        digest
    }

    pub fn canonical_key(&self, initial_board: ExtendedBoard, height: u8) -> String {
        let mut key = format!(
            "ctk2|height={height}|initial={}|placements=",
            board_hex(initial_board)
        );
        for (index, placement) in self.placements.iter().enumerate() {
            if index != 0 {
                key.push(',');
            }
            key.push(placement.piece.as_ascii());
            key.push(':');
            key.push_str(&board_hex(placement.cells));
        }
        key
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.placements.capacity() * core::mem::size_of::<ExtendedPlacementKey>()
    }
}

fn board_hex(board: ExtendedBoard) -> String {
    let words = board.words();
    format!(
        "{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}

pub(super) enum ExtendedBuildOrderResult {
    Incomplete {
        searched_nodes: usize,
        reachability_states: usize,
        scratch_bytes: usize,
    },
    Complete {
        graph: BuildOrderGraph,
        finesse_language: Option<PreparedFinesseLanguage>,
        spin_graph: Option<SpinCoverageExecutionGraph>,
        searched_nodes: usize,
        reachability_states: usize,
        scratch_bytes: usize,
    },
}

pub(super) struct ExtendedBuildOrderWorkspace {
    reachability: ExtendedReachabilityWorkspace,
    subset_node_ids: Vec<u32>,
    subset_node_map: HashMap<u64, u32>,
    subset_queue: Vec<u64>,
    edge_scratch: Vec<BuildEdge>,
}

impl ExtendedBuildOrderWorkspace {
    pub fn new(
        width: u8,
        height: u8,
        kick_profile_id: clearra_rules::kicks::KickTableProfileId,
    ) -> Self {
        Self {
            reachability: ExtendedReachabilityWorkspace::new(width, height, kick_profile_id),
            subset_node_ids: Vec::new(),
            subset_node_map: HashMap::new(),
            subset_queue: Vec::new(),
            edge_scratch: Vec::new(),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.reachability.retained_bytes()
            + self.subset_node_ids.capacity() * core::mem::size_of::<u32>()
            + self.subset_node_map.capacity()
                * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
            + self.subset_queue.capacity() * core::mem::size_of::<u64>()
            + self.edge_scratch.capacity() * core::mem::size_of::<BuildEdge>()
    }
}

struct CandidateProjection {
    operation_cells: Vec<ExtendedBoard>,
    operation_pieces: Vec<PieceKind>,
    row_contributors: [u64; 24],
    completed_target_rows: u32,
    initial_board: ExtendedBoard,
    target_cells: ExtendedBoard,
    width: u8,
    height: u8,
    all_operations: u64,
    final_board: ExtendedBoard,
}

impl CandidateProjection {
    fn compile(
        catalog: &ExtendedInverseCatalog,
        candidate: &ExtendedGeometryCandidate,
    ) -> Result<Self, WasmExactSearchError> {
        let rows = candidate.row_ids();
        if rows.is_empty() || rows.len() > 60 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_extended_candidate_operation_count_invalid",
            ));
        }
        let mut operation_cells = Vec::with_capacity(rows.len());
        let mut operation_pieces = Vec::with_capacity(rows.len());
        let mut row_contributors = [0_u64; 24];
        for (operation, row_id) in rows.iter().copied().enumerate() {
            let row = catalog.skeleton(row_id);
            operation_cells.push(row.cells);
            operation_pieces.push(row.piece);
            for cell in row.cells.cells() {
                row_contributors[cell as usize / usize::from(catalog.width())] |=
                    1_u64 << operation;
            }
        }
        let logical_target = catalog.initial_board().union(catalog.required_cells());
        let full_row = (1_u16 << catalog.width()) - 1;
        let mut completed_target_rows = 0_u32;
        for row in 0..catalog.height() {
            if logical_target.row_bits(catalog.width(), row) == full_row {
                completed_target_rows |= 1_u32 << row;
            }
        }
        let final_board = compact_logical_board(
            catalog.width(),
            catalog.height(),
            logical_target,
            completed_target_rows,
        );
        let all_operations = if rows.len() == 64 {
            u64::MAX
        } else {
            (1_u64 << rows.len()) - 1
        };
        Ok(Self {
            operation_cells,
            operation_pieces,
            row_contributors,
            completed_target_rows,
            initial_board: catalog.initial_board(),
            target_cells: catalog.required_cells(),
            width: catalog.width(),
            height: catalog.height(),
            all_operations,
            final_board,
        })
    }

    fn expected_deleted_rows(&self, remaining_operations: u64) -> u32 {
        let mut deleted = 0_u32;
        for row in 0..self.height as usize {
            if self.completed_target_rows & (1_u32 << row) == 0 {
                continue;
            }
            let contributors = self.row_contributors[row];
            if contributors != 0 && contributors & remaining_operations == 0 {
                deleted |= 1_u32 << row;
            }
        }
        deleted
    }

    fn expected_board(&self, remaining_operations: u64, deleted_rows: u32) -> ExtendedBoard {
        let mut remaining_cells = ExtendedBoard::EMPTY;
        let mut remaining = remaining_operations;
        while remaining != 0 {
            let operation = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            remaining_cells = remaining_cells.union(self.operation_cells[operation]);
        }
        let placed = self.target_cells.without(remaining_cells);
        compact_logical_board(
            self.width,
            self.height,
            self.initial_board.union(placed),
            deleted_rows,
        )
    }

    fn state_for_subset(&self, subset: u64) -> (ExtendedBoard, u32) {
        let remaining = self.all_operations & !subset;
        let deleted_rows = self.expected_deleted_rows(remaining);
        (self.expected_board(remaining, deleted_rows), deleted_rows)
    }

    fn retained_bytes(&self) -> usize {
        self.operation_cells.capacity() * core::mem::size_of::<ExtendedBoard>()
            + self.operation_pieces.capacity() * core::mem::size_of::<PieceKind>()
    }
}

pub(super) fn build_extended_order_graph(
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    control: &ExecutionControl,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    build_extended_order_graph_mode(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        None,
        false,
        control,
    )
}

pub(super) fn build_extended_order_graph_with_finesse(
    problem: &SearchProblem,
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    build_extended_order_graph_mode(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        Some(problem),
        spin_coverage_requested,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_extended_order_graph_mode(
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    finesse_problem: Option<&SearchProblem>,
    finesse_spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    let geometry_only = finesse_problem.is_some();
    let mut finesse_geometry_span =
        geometry_only.then(|| SearchStageSpan::begin(ExecutorSearchStage::FinesseGeometry));
    let projection = CandidateProjection::compile(catalog, candidate)?;
    let rows = candidate.row_ids();
    let dense_state_count =
        (rows.len() <= super::MAX_BOARD64_PIECES).then(|| 1_usize << rows.len());
    if let Some(state_count) = dense_state_count {
        if workspace.subset_node_ids.len() < state_count {
            workspace
                .subset_node_ids
                .try_reserve_exact(state_count - workspace.subset_node_ids.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_build_order_index_storage_unavailable",
                    )
                })?;
            workspace.subset_node_ids.resize(state_count, NO_BUILD_NODE);
        }
        workspace.subset_node_ids[..state_count].fill(NO_BUILD_NODE);
        workspace.subset_node_ids[0] = 0;
    } else {
        workspace.subset_node_map.clear();
        workspace.subset_node_map.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_order_index_storage_unavailable",
            )
        })?;
        workspace.subset_node_map.insert(0, 0);
    }
    workspace.subset_queue.clear();
    workspace.subset_queue.push(0);

    let mut specs = Vec::new();
    specs
        .try_reserve(dense_state_count.unwrap_or(1024).min(1024))
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_order_node_storage_unavailable",
            )
        })?;
    specs.push(BuildOrderNodeSpec {
        edge_start: 0,
        edge_count: 0,
        depth: 0,
        accepting: false,
    });
    let mut edges = Vec::new();
    let mut spin_node_spans = Vec::<(u32, u32)>::new();
    let mut spin_edges = Vec::<ScoringExecutionEdge>::new();
    if spin_candidate.is_some() {
        spin_node_spans.push((0, 0));
    }
    let mut queue_cursor = 0usize;
    let mut searched_nodes = 0usize;
    let reachability_before = workspace.reachability.visited_state_count();

    while queue_cursor < workspace.subset_queue.len() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if remaining_node_budget != 0 && searched_nodes >= remaining_node_budget {
            if let Some(span) = finesse_geometry_span.take() {
                span.finish(searched_nodes as u64);
            }
            return Ok(ExtendedBuildOrderResult::Incomplete {
                searched_nodes,
                reachability_states: workspace
                    .reachability
                    .visited_state_count()
                    .saturating_sub(reachability_before),
                scratch_bytes: workspace.retained_bytes()
                    + specs.capacity() * core::mem::size_of::<BuildOrderNodeSpec>()
                    + edges.capacity() * core::mem::size_of::<BuildEdge>(),
            });
        }
        searched_nodes = searched_nodes.saturating_add(1);
        let subset = workspace.subset_queue[queue_cursor];
        queue_cursor += 1;
        let node_index = if dense_state_count.is_some() {
            workspace.subset_node_ids[subset as usize]
        } else {
            *workspace
                .subset_node_map
                .get(&subset)
                .expect("queued extended subset has a node")
        } as usize;
        specs[node_index].edge_start = edges.len() as u32;
        let spin_edge_start = spin_edges.len() as u32;
        if subset == projection.all_operations {
            let (board, deleted_rows) = projection.state_for_subset(subset);
            specs[node_index].accepting =
                board == projection.final_board && deleted_rows == projection.completed_target_rows;
            continue;
        }

        let (board, deleted_rows) = projection.state_for_subset(subset);
        workspace.edge_scratch.clear();
        let mut lock_sets: [Option<ExtendedReachableLocks>; 7] = std::array::from_fn(|_| None);
        for operation in 0..rows.len() {
            let operation_bit = 1_u64 << operation;
            if subset & operation_bit != 0 {
                continue;
            }
            let piece = projection.operation_pieces[operation];
            let piece_slot = piece_index(piece);
            if (!geometry_only || spin_candidate.is_some()) && lock_sets[piece_slot].is_none() {
                lock_sets[piece_slot] = Some(workspace.reachability.reachable_locks_with_scoring(
                    board,
                    piece,
                    spin_candidate.is_some(),
                ));
            }
            let locks = lock_sets[piece_slot].as_ref();
            let child_subset = subset | operation_bit;
            let (expected_board, expected_deleted) = projection.state_for_subset(child_subset);
            let row_id = rows[operation];
            let mut build_edge_added = false;
            for realization in catalog.instantiations(row_id, deleted_rows) {
                let lock_reachable = locks.is_some_and(|locks| {
                    locks.contains(realization.rotation, realization.x, realization.lock_y)
                });
                if (!geometry_only && !lock_reachable) || board.intersects(realization.lock_mask) {
                    continue;
                }
                let (next_board, cleared_physical, cleared_lines) = place_and_clear(
                    projection.width,
                    projection.height,
                    board.union(realization.lock_mask),
                );
                let Some(next_deleted) =
                    merge_deleted_rows(projection.height, deleted_rows, cleared_physical)
                else {
                    continue;
                };
                if next_board != expected_board || next_deleted != expected_deleted {
                    continue;
                }
                let child_index = if dense_state_count.is_some() {
                    if workspace.subset_node_ids[child_subset as usize] == NO_BUILD_NODE {
                        let child_index = u32::try_from(specs.len()).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "wasm_extended_build_order_node_index_overflow",
                            )
                        })?;
                        workspace.subset_node_ids[child_subset as usize] = child_index;
                        workspace.subset_queue.push(child_subset);
                        specs.push(BuildOrderNodeSpec {
                            edge_start: 0,
                            edge_count: 0,
                            depth: child_subset.count_ones() as u8,
                            accepting: false,
                        });
                        if spin_candidate.is_some() {
                            spin_node_spans.push((0, 0));
                        }
                        child_index
                    } else {
                        workspace.subset_node_ids[child_subset as usize]
                    }
                } else if let Some(child_index) = workspace.subset_node_map.get(&child_subset) {
                    *child_index
                } else {
                    let child_index = u32::try_from(specs.len()).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_build_order_node_index_overflow",
                        )
                    })?;
                    workspace.subset_node_map.try_reserve(1).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_extended_build_order_index_storage_unavailable",
                        )
                    })?;
                    workspace.subset_node_map.insert(child_subset, child_index);
                    workspace.subset_queue.push(child_subset);
                    specs.push(BuildOrderNodeSpec {
                        edge_start: 0,
                        edge_count: 0,
                        depth: child_subset.count_ones() as u8,
                        accepting: false,
                    });
                    if spin_candidate.is_some() {
                        spin_node_spans.push((0, 0));
                    }
                    child_index
                };
                if geometry_only || !build_edge_added {
                    workspace.edge_scratch.push(BuildOrderGraph::edge(
                        child_index,
                        operation as u8,
                        piece,
                        realization.rotation,
                        realization.x,
                        realization.lock_y,
                        cleared_lines,
                    ));
                    build_edge_added = true;
                }
                if spin_candidate.is_some() && lock_reachable {
                    let locks = locks.expect("spin evidence requires initialized reachability");
                    let lock_evidence = locks.scoring_evidence(
                        realization.rotation,
                        realization.x,
                        realization.lock_y,
                    );
                    let (blocked_t_corners, blocked_t_front_corners) = if piece == PieceKind::T {
                        extended_t_corner_evidence(
                            projection.width,
                            projection.height,
                            board,
                            realization.rotation,
                            realization.x,
                            realization.lock_y,
                        )
                    } else {
                        (0, 0)
                    };
                    spin_edges.push(
                        ScoringExecutionEdge::new(
                            child_index,
                            operation as u8,
                            piece,
                            realization.rotation,
                            realization.x,
                            realization.lock_y,
                            cleared_lines,
                            blocked_t_corners,
                            blocked_t_front_corners,
                            lock_evidence,
                        )
                        .with_perfect_clear(cleared_lines > 0 && next_board.is_empty()),
                    );
                }
                // Buildability depends on the exact operation order, not on how
                // many equivalent movement paths reach the same lock state.
                if !geometry_only && spin_candidate.is_none() {
                    break;
                }
            }
        }
        workspace
            .edge_scratch
            .sort_unstable_by_key(BuildEdge::canonical_key);
        workspace.edge_scratch.dedup();
        edges.extend_from_slice(&workspace.edge_scratch);
        specs[node_index].edge_count = edges.len() as u32 - specs[node_index].edge_start;
        if spin_candidate.is_some() {
            let start = spin_edge_start as usize;
            spin_edges[start..].sort_unstable_by_key(spin_edge_key);
            let mut write = start;
            for read in start..spin_edges.len() {
                if write == start
                    || spin_edge_key(&spin_edges[write - 1]) != spin_edge_key(&spin_edges[read])
                {
                    spin_edges[write] = spin_edges[read];
                    write += 1;
                }
            }
            spin_edges.truncate(write);
            spin_node_spans[node_index] =
                (spin_edge_start, spin_edges.len() as u32 - spin_edge_start);
        }
    }

    let reachability_states = workspace
        .reachability
        .visited_state_count()
        .saturating_sub(reachability_before);
    if let Some(span) = finesse_geometry_span.take() {
        span.finish(searched_nodes as u64);
    }
    let finesse_language = finesse_problem
        .map(|problem| {
            annotate_and_prune_extended_finesse_graph(
                problem,
                &projection,
                &workspace.subset_queue,
                &mut specs,
                &mut edges,
                spin_candidate
                    .is_some()
                    .then_some((spin_node_spans.as_slice(), spin_edges.as_slice())),
                finesse_spin_coverage_requested,
                control,
            )
        })
        .transpose()?;
    // The normal path emits one edge per operation and therefore keeps its
    // existing shared piece-edge representation. Finesse retains concrete
    // poses until movement costs have been attached and pruned.
    let graph = BuildOrderGraph::from_topological_parts(
        specs,
        edges,
        0,
        reachability_states,
        !geometry_only,
    )?;
    let spin_graph = spin_candidate.map(|(candidate_id, candidate_key)| {
        let mut live_nodes = Vec::with_capacity(graph.nodes.len());
        let mut live_edges = Vec::new();
        for (node_index, node) in graph.nodes.iter().enumerate() {
            let edge_start = live_edges.len() as u32;
            if node.live {
                let (start, count) = spin_node_spans[node_index];
                live_edges.extend(
                    spin_edges[start as usize..(start + count) as usize]
                        .iter()
                        .copied()
                        .filter(|edge| graph.nodes[edge.to() as usize].live),
                );
            }
            live_nodes.push(ScoringExecutionNode::new(
                edge_start,
                live_edges.len() as u32 - edge_start,
                node.accepting(),
            ));
        }
        SpinCoverageExecutionGraph::new(
            candidate_id,
            candidate_key,
            graph.root,
            live_nodes,
            live_edges,
        )
    });
    let scratch_bytes = workspace
        .retained_bytes()
        .saturating_add(graph.retained_bytes())
        .saturating_add(projection.retained_bytes())
        .saturating_add(spin_node_spans.capacity() * core::mem::size_of::<(u32, u32)>())
        .saturating_add(spin_edges.capacity() * core::mem::size_of::<ScoringExecutionEdge>());
    Ok(ExtendedBuildOrderResult::Complete {
        graph,
        finesse_language,
        spin_graph,
        searched_nodes,
        reachability_states,
        scratch_bytes,
    })
}

fn annotate_and_prune_extended_finesse_graph(
    problem: &SearchProblem,
    projection: &CandidateProjection,
    node_subsets: &[u64],
    specs: &mut [BuildOrderNodeSpec],
    edges: &mut Vec<BuildEdge>,
    scoring_edges: Option<ScoringEdgeSlices<'_>>,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
) -> Result<PreparedFinesseLanguage, WasmExactSearchError> {
    annotate_and_prune_extended_finesse_graph_with_query_count(
        problem,
        projection,
        node_subsets,
        specs,
        edges,
        scoring_edges,
        spin_coverage_requested,
        control,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn annotate_and_prune_extended_finesse_graph_with_query_count(
    problem: &SearchProblem,
    projection: &CandidateProjection,
    node_subsets: &[u64],
    specs: &mut [BuildOrderNodeSpec],
    edges: &mut Vec<BuildEdge>,
    scoring_edges: Option<ScoringEdgeSlices<'_>>,
    spin_coverage_requested: bool,
    control: &ExecutionControl,
    query_count: Option<&mut usize>,
) -> Result<PreparedFinesseLanguage, WasmExactSearchError> {
    if specs.len() != node_subsets.len() {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_extended_finesse_subset_node_mismatch",
        ));
    }
    let target_grouping_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseTargetGrouping);
    let grouped_target_count = edges.len();
    let kick_profile =
        super::kick_profiles::builtin_kick_profile(problem.kick_profile().profile_id())
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_kick_profile_unavailable",
            ))?
            .clone();
    let mut edge_costs = vec![None; edges.len()];
    let mut edge_terminal_evidence = vec![None; edges.len()];
    let b2b_policy = finesse_b2b_policy(problem);
    let mut groups = FrozenFinesseTargetGroups::new();
    for (node_index, spec) in specs.iter().copied().enumerate() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let start = spec.edge_start as usize;
        let end = start.checked_add(spec.edge_count as usize).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_extended_finesse_edge_range_overflow"),
        )?;
        let source_edges = edges
            .get(start..end)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_extended_finesse_edge_range_invalid",
            ))?;
        if source_edges.is_empty() {
            continue;
        }
        let source_board = extended_finesse_board(projection, node_subsets[node_index])?;
        let scoring = if let Some((node_spans, scoring)) = scoring_edges {
            let (scoring_start, scoring_count) =
                *node_spans
                    .get(node_index)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_extended_finesse_scoring_node_span_missing",
                    ))?;
            let scoring_start = scoring_start as usize;
            let scoring_end = scoring_start.checked_add(scoring_count as usize).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_scoring_edge_range_overflow",
                ),
            )?;
            Some(scoring.get(scoring_start..scoring_end).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "wasm_extended_finesse_scoring_edge_range_invalid",
                ),
            )?)
        } else {
            None
        };
        for (offset, edge) in source_edges.iter().copied().enumerate() {
            let edge_index = start + offset;
            let (_, piece, _, rotation, x, y, _) = edge.canonical_key();
            let target = FinesseTarget::new(rotation, i16::from(x), i16::from(y));
            let (terminal_evidence, allowed) = if let Some(scoring) = scoring {
                let matching_edge = scoring
                    .iter()
                    .find(|candidate| scoring_edge_matches_build_edge(candidate, &edge))
                    .copied();
                let (allowed, exact_evidence_required) =
                    matching_edge.map_or((false, false), |candidate| {
                        finesse_scoring_edge_requirement(
                            spin_coverage_requested,
                            b2b_policy,
                            candidate,
                        )
                    });
                (
                    (allowed && exact_evidence_required)
                        .then_some(matching_edge)
                        .flatten()
                        .map(|evidence| {
                            finesse_terminal_label(evidence.lock_evidence(), target.pose().rotation)
                        })
                        .transpose()?,
                    allowed,
                )
            } else {
                (None, true)
            };
            push_frozen_finesse_target(
                &mut groups,
                source_board,
                piece,
                GroupedFinesseTarget::new(edge_index, target, terminal_evidence, allowed),
            );
        }
    }
    target_grouping_span.finish(grouped_target_count as u64);

    let movement_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseMovementBfs);
    let query_traversals = annotate_frozen_finesse_target_groups(
        groups,
        problem.spawn_profile(),
        &kick_profile,
        &mut edge_costs,
        &mut edge_terminal_evidence,
        control,
        "wasm_extended_finesse_movement_search_failed",
        "wasm_extended_finesse_scoring_evidence_search_failed",
    )?;
    if let Some(query_count) = query_count {
        *query_count = query_traversals;
    }
    movement_span.finish(grouped_target_count as u64);

    let prune_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAnnotationPrune);
    let mut live = specs.iter().map(|spec| spec.accepting).collect::<Vec<_>>();
    for node_index in (0..specs.len()).rev() {
        if live[node_index] {
            continue;
        }
        let spec = specs[node_index];
        let start = spec.edge_start as usize;
        let end = start + spec.edge_count as usize;
        live[node_index] = edges[start..end]
            .iter()
            .enumerate()
            .any(|(offset, edge)| edge_costs[start + offset].is_some() && live[edge.to as usize]);
    }

    let old_edges = core::mem::take(edges);
    let mut retained_edges = Vec::with_capacity(old_edges.len());
    let mut prepared_nodes = Vec::with_capacity(specs.len());
    let mut prepared_edges = Vec::with_capacity(old_edges.len());
    for (node_index, spec) in specs.iter_mut().enumerate() {
        let old_start = spec.edge_start as usize;
        let old_end = old_start + spec.edge_count as usize;
        spec.edge_start = retained_edges.len() as u32;
        let prepared_start = prepared_edges.len() as u32;
        if live[node_index] {
            for (offset, edge) in old_edges[old_start..old_end].iter().copied().enumerate() {
                let original_index = old_start + offset;
                let Some(cost) = edge_costs[original_index] else {
                    continue;
                };
                if !live[edge.to as usize] {
                    continue;
                }
                let (_, piece, _, rotation, x, y, _) = edge.canonical_key();
                retained_edges.push(edge);
                prepared_edges.push(PreparedFinesseEdge {
                    child: edge.to,
                    piece,
                    cost,
                    transition_order: u32::try_from(original_index).unwrap_or(u32::MAX),
                    action_key: GeometryActionKey::new(piece, rotation, i16::from(x), i16::from(y)),
                    terminal_evidence: edge_terminal_evidence[original_index],
                });
            }
        }
        spec.edge_count = retained_edges.len() as u32 - spec.edge_start;
        prepared_nodes.push(PreparedFinesseNode {
            edge_start: prepared_start,
            edge_count: prepared_edges.len() as u32 - prepared_start,
            depth: spec.depth,
            accepting: spec.accepting,
            source_board: Some(extended_finesse_board(
                projection,
                node_subsets[node_index],
            )?),
        });
    }
    *edges = retained_edges;
    prune_span.finish(prepared_edges.len() as u64);
    Ok(PreparedFinesseLanguage {
        nodes: prepared_nodes,
        edges: prepared_edges,
        root: 0,
    })
}

fn scoring_edge_matches_build_edge(scoring: &ScoringExecutionEdge, edge: &BuildEdge) -> bool {
    let (to, piece, operation, rotation, x, y, cleared_lines) = edge.canonical_key();
    scoring.to() == to
        && scoring.operation_index() == operation
        && scoring.piece() == piece
        && scoring.rotation() == rotation
        && scoring.x() == x
        && scoring.y() == y
        && scoring.cleared_lines() == cleared_lines
}

fn extended_finesse_board(
    projection: &CandidateProjection,
    subset: u64,
) -> Result<FinesseBoard, WasmExactSearchError> {
    let occupied = projection.state_for_subset(subset).0;
    let board = StandardPcBoard::from_words(projection.height, occupied.words())
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_extended_finesse_board_invalid"))?;
    Ok(FinesseBoard::from_standard_pc(board))
}

fn spin_edge_key(
    edge: &ScoringExecutionEdge,
) -> (
    u32,
    u8,
    PieceKind,
    clearra_core_domain::piece::rotation::RotationState,
    i8,
    i8,
    u8,
) {
    (
        edge.to(),
        edge.operation_index(),
        edge.piece(),
        edge.rotation(),
        edge.x(),
        edge.y(),
        edge.cleared_lines(),
    )
}

fn extended_t_corner_evidence(
    width: u8,
    height: u8,
    board_before: ExtendedBoard,
    rotation: clearra_core_domain::piece::rotation::RotationState,
    x: i8,
    y: i8,
) -> (u8, u8) {
    let x = i32::from(x);
    let y = i32::from(y);
    let (center_x, center_y) = match rotation.quarter_turns() {
        0 => (x + 1, y),
        1 => (x, y + 1),
        2 | 3 => (x + 1, y + 1),
        _ => return (0, 0),
    };
    let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
    let front = match rotation.quarter_turns() {
        0 => [(-1, 1), (1, 1)],
        1 => [(1, -1), (1, 1)],
        2 => [(-1, -1), (1, -1)],
        3 => [(-1, -1), (-1, 1)],
        _ => return (0, 0),
    };
    let blocked = |(dx, dy): (i32, i32)| {
        let cell_x = center_x + dx;
        let cell_y = center_y + dy;
        if cell_x < 0 || cell_y < 0 || cell_x >= i32::from(width) {
            return true;
        }
        if cell_y >= i32::from(height) {
            return false;
        }
        board_before.contains((cell_y * i32::from(width) + cell_x) as u16)
    };
    (
        corners
            .into_iter()
            .filter(|corner| blocked(*corner))
            .count() as u8,
        front.into_iter().filter(|corner| blocked(*corner)).count() as u8,
    )
}

#[cfg(test)]
mod finesse_tests {
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        pc::pc_target::PcTarget,
        piece::{piece_kind::PieceKind, rotation::RotationState},
    };
    use clearra_finesse::{
        FinesseSequenceInput, FrozenFinesseQuery, QueueClassProductEvaluator,
        TerminalEvidenceClass, TerminalEvidenceLabel,
    };
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
    use clearra_problem::{ProblemCompiler, SearchProblem};
    use clearra_replay::{RotationRequest, ScoringLockEvidence};

    use super::*;

    fn problem() -> SearchProblem {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::standard_7_bag())
            .with_hold_policy(PcHoldPolicy::EnabledEmpty)
            .with_objective(ObjectivePolicy::unique());
        ProblemCompiler::compile_opening_pc(&query).expect("test problem")
    }

    fn b2b_problem() -> SearchProblem {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::standard_7_bag())
            .with_hold_policy(PcHoldPolicy::EnabledEmpty)
            .with_objective(
                ObjectivePolicy::unique()
                    .with_back_to_back_preservation(SpinProfileSelection::TSpins),
            );
        ProblemCompiler::compile_opening_pc(&query).expect("B2B test problem")
    }

    fn one_t_projection(height: u8) -> CandidateProjection {
        let mut cells = ExtendedBoard::EMPTY;
        for index in [3_u16, 4, 5, 14] {
            assert!(cells.insert(index));
        }
        CandidateProjection {
            operation_cells: vec![cells],
            operation_pieces: vec![PieceKind::T],
            row_contributors: [0; 24],
            completed_target_rows: 0,
            initial_board: ExtendedBoard::EMPTY,
            target_cells: cells,
            width: 10,
            height,
            all_operations: 1,
            final_board: cells,
        }
    }

    fn annotate_one_t(height: u8) -> PreparedFinesseLanguage {
        let projection = one_t_projection(height);
        let mut specs = [
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 1,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let mut edges = vec![BuildOrderGraph::edge(
            1,
            0,
            PieceKind::T,
            RotationState::Zero,
            3,
            0,
            0,
        )];
        annotate_and_prune_extended_finesse_graph(
            &problem(),
            &projection,
            &[0, 1],
            &mut specs,
            &mut edges,
            None,
            false,
            &ExecutionControl::new(ExecutionCancellationToken::new()),
        )
        .expect("extended finesse annotation")
    }

    #[test]
    fn extended_finesse_cost_is_stable_across_the_six_seven_row_boundary() {
        let compact_boundary = annotate_one_t(6);
        let extended_boundary = annotate_one_t(7);
        let compact_edge = compact_boundary.edges[0];
        let extended_edge = extended_boundary.edges[0];

        assert_eq!(compact_edge.cost, extended_edge.cost);
        assert_eq!(compact_edge.action_key, extended_edge.action_key);
        assert_eq!(compact_edge.cost, 2);
        assert_eq!(
            compact_boundary.nodes[0]
                .source_board
                .expect("compact boundary source")
                .height(),
            6
        );
        assert_eq!(
            extended_boundary.nodes[0]
                .source_board
                .expect("extended boundary source")
                .height(),
            7
        );
    }

    #[test]
    fn extended_repeated_semantic_nodes_share_one_board_piece_query() {
        let projection = one_t_projection(7);
        let mut specs = [
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 1,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 2,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
            BuildOrderNodeSpec {
                edge_start: 2,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let mut edges = vec![
            BuildOrderGraph::edge(2, 0, PieceKind::T, RotationState::Zero, 3, 0, 0),
            BuildOrderGraph::edge(3, 0, PieceKind::T, RotationState::Zero, 3, 0, 0),
        ];
        let mut query_count = usize::MAX;
        let language = annotate_and_prune_extended_finesse_graph_with_query_count(
            &problem(),
            &projection,
            &[0, 0, 1, 1],
            &mut specs,
            &mut edges,
            None,
            false,
            &ExecutionControl::default(),
            Some(&mut query_count),
        )
        .unwrap();

        assert_eq!(query_count, 1);
        assert_eq!(edges.len(), 2);
        assert_eq!(language.edges.len(), 2);
        assert_eq!(language.edges[0].cost, language.edges[1].cost);
        assert_eq!(language.edges[0].transition_order, 0);
        assert_eq!(language.edges[1].transition_order, 1);
    }

    #[test]
    fn finesse_annotation_prunes_only_the_opt_in_language_copy() {
        let projection = one_t_projection(7);
        let specs = vec![
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 2,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 2,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let edges = vec![
            BuildOrderGraph::edge(1, 0, PieceKind::T, RotationState::Zero, -3, 0, 0),
            BuildOrderGraph::edge(1, 0, PieceKind::T, RotationState::Zero, 3, 0, 0),
        ];
        let normal_graph =
            BuildOrderGraph::from_topological_parts(specs.clone(), edges.clone(), 0, 0, false)
                .expect("normal graph remains valid");
        assert_eq!(normal_graph.edges(0).len(), 2);

        let mut finesse_specs = specs;
        let mut finesse_edges = edges;
        let language = annotate_and_prune_extended_finesse_graph(
            &problem(),
            &projection,
            &[0, 1],
            &mut finesse_specs,
            &mut finesse_edges,
            None,
            false,
            &ExecutionControl::new(ExecutionCancellationToken::new()),
        )
        .expect("finesse graph annotation");

        assert_eq!(finesse_edges.len(), 1);
        assert_eq!(language.edges.len(), 1);
        assert_eq!(language.edges[0].action_key.x(), 3);
        assert_eq!(normal_graph.edges(0).len(), 2);
    }

    #[test]
    fn scoring_annotation_keeps_exact_rotation_while_b2b_harmless_edge_uses_hard_drop() {
        let mut cells = ExtendedBoard::EMPTY;
        for index in [4_u16, 5, 14, 15] {
            assert!(cells.insert(index));
        }
        let projection = CandidateProjection {
            operation_cells: vec![cells],
            operation_pieces: vec![PieceKind::O],
            row_contributors: [0; 24],
            completed_target_rows: 0,
            initial_board: ExtendedBoard::EMPTY,
            target_cells: cells,
            width: 10,
            height: 7,
            all_operations: 1,
            final_board: cells,
        };
        let problem = problem();
        let source_board = extended_finesse_board(&projection, 0).unwrap();
        let query = FrozenFinesseQuery::new(
            source_board,
            PieceKind::O,
            problem.spawn_profile(),
            super::super::kick_profiles::builtin_kick_profile(problem.kick_profile().profile_id())
                .unwrap()
                .clone(),
            [FinesseTarget::new(RotationState::Zero, 4, 0)],
        );
        assert_eq!(query.costs().unwrap().as_slice(), [Some(1)]);
        let rotation = query
            .route_labels()
            .unwrap()
            .get(0)
            .unwrap()
            .get(TerminalEvidenceClass::Rotation)
            .expect("a slower terminal rotation reaches the same O lock");
        let TerminalEvidenceLabel::Rotation {
            from,
            request,
            kick_index,
            kick_dx,
            kick_dy,
            predecessor,
            ..
        } = rotation.terminal_evidence
        else {
            panic!("rotation class carries rotation evidence")
        };
        let request = match request {
            clearra_finesse::ClassicInputAction::RotateClockwise => RotationRequest::Clockwise,
            clearra_finesse::ClassicInputAction::RotateCounterClockwise => {
                RotationRequest::CounterClockwise
            }
            clearra_finesse::ClassicInputAction::Rotate180 => RotationRequest::HalfTurn,
            _ => panic!("terminal evidence is a rotation"),
        };
        let evidence = ScoringLockEvidence::rotation(
            from,
            request,
            kick_index,
            kick_dx,
            kick_dy,
            predecessor.x as i8,
            predecessor.y as i8,
        );
        let scoring_edges = [ScoringExecutionEdge::new(
            1,
            0,
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
            0,
            0,
            0,
            evidence,
        )];
        let spans = [(0, 1), (1, 0)];
        let mut specs = [
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 1,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let mut edges = vec![BuildOrderGraph::edge(
            1,
            0,
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
            0,
        )];
        let language = annotate_and_prune_extended_finesse_graph(
            &problem,
            &projection,
            &[0, 1],
            &mut specs,
            &mut edges,
            Some((&spans, &scoring_edges)),
            true,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert!(rotation.cost > 1);
        assert_eq!(language.edges[0].cost, rotation.cost);
        assert_eq!(
            language.edges[0].terminal_evidence,
            Some(rotation.terminal_evidence)
        );
        let costed = super::super::build_probability::costed_finesse_language(&language).unwrap();
        let replay = QueueClassProductEvaluator::new(&costed)
            .replay_fixed_queue_witness(
                &[PieceKind::O],
                None,
                problem.spawn_profile(),
                super::super::kick_profiles::builtin_kick_profile(
                    problem.kick_profile().profile_id(),
                )
                .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(replay.total_cost(), rotation.cost);
        assert!(replay.inputs().iter().any(|input| matches!(
            input,
            FinesseSequenceInput::Movement(
                clearra_finesse::ClassicInputAction::RotateClockwise
                    | clearra_finesse::ClassicInputAction::RotateCounterClockwise
                    | clearra_finesse::ClassicInputAction::Rotate180
            )
        )));

        let mut b2b_specs = [
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 1,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let mut b2b_edges = vec![BuildOrderGraph::edge(
            1,
            0,
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
            0,
        )];
        let b2b_language = annotate_and_prune_extended_finesse_graph(
            &b2b_problem(),
            &projection,
            &[0, 1],
            &mut b2b_specs,
            &mut b2b_edges,
            Some((&spans, &scoring_edges)),
            false,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert_eq!(b2b_language.edges[0].cost, 1);
        assert_eq!(b2b_language.edges[0].terminal_evidence, None);
        let b2b_costed =
            super::super::build_probability::costed_finesse_language(&b2b_language).unwrap();
        let b2b_replay = QueueClassProductEvaluator::new(&b2b_costed)
            .replay_fixed_queue_witness(
                &[PieceKind::O],
                None,
                problem.spawn_profile(),
                super::super::kick_profiles::builtin_kick_profile(
                    problem.kick_profile().profile_id(),
                )
                .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            b2b_replay.inputs(),
            [FinesseSequenceInput::Movement(
                clearra_finesse::ClassicInputAction::HardDrop
            )]
        );
    }

    #[test]
    fn b2b_finesse_language_prunes_the_same_non_spin_clear_as_postprocess() {
        let projection = one_t_projection(7);
        let scoring_edges = [ScoringExecutionEdge::new(
            1,
            0,
            PieceKind::T,
            RotationState::Zero,
            3,
            0,
            1,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )];
        let spans = [(0, 1), (1, 0)];
        let mut specs = [
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 1,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 1,
                edge_count: 0,
                depth: 1,
                accepting: true,
            },
        ];
        let mut edges = vec![BuildOrderGraph::edge(
            1,
            0,
            PieceKind::T,
            RotationState::Zero,
            3,
            0,
            1,
        )];

        let language = annotate_and_prune_extended_finesse_graph(
            &b2b_problem(),
            &projection,
            &[0, 1],
            &mut specs,
            &mut edges,
            Some((&spans, &scoring_edges)),
            false,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert!(edges.is_empty());
        assert!(language.edges.is_empty());
        assert_eq!(language.nodes[language.root as usize].edge_count, 0);
        let costed = super::super::build_probability::costed_finesse_language(&language).unwrap();
        assert_eq!(
            QueueClassProductEvaluator::new(&costed)
                .fixed_queue_cost(&[PieceKind::T], None)
                .unwrap(),
            None
        );
    }
}
