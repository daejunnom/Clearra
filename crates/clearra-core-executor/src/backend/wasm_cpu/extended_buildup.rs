use std::collections::HashMap;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_replay::{ScoringExecutionEdge, ScoringExecutionNode, SpinCoverageExecutionGraph};

use super::{
    buildup::{BuildEdge, BuildOrderGraph, BuildOrderNodeSpec},
    extended_board::{compact_logical_board, merge_deleted_rows, place_and_clear, ExtendedBoard},
    extended_geometry::ExtendedGeometryCandidate,
    extended_inverse_catalog::ExtendedInverseCatalog,
    extended_reachability::{ExtendedReachabilityWorkspace, ExtendedReachableLocks},
    mix_digest, piece_index, WasmExactSearchError,
};

const NO_BUILD_NODE: u32 = u32::MAX;

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
            if lock_sets[piece_slot].is_none() {
                lock_sets[piece_slot] = Some(workspace.reachability.reachable_locks_with_scoring(
                    board,
                    piece,
                    spin_candidate.is_some(),
                ));
            }
            let locks = lock_sets[piece_slot]
                .as_ref()
                .expect("extended piece reachability was initialized");
            let child_subset = subset | operation_bit;
            let (expected_board, expected_deleted) = projection.state_for_subset(child_subset);
            let row_id = rows[operation];
            let mut build_edge_added = false;
            for realization in catalog.instantiations(row_id, deleted_rows) {
                if !locks.contains(realization.rotation, realization.x, realization.lock_y)
                    || board.intersects(realization.lock_mask)
                {
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
                if !build_edge_added {
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
                if spin_candidate.is_some() {
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
                if spin_candidate.is_none() {
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
    // build_edge_added emits at most one edge per operation, and each
    // operation advances to a distinct child subset.
    let graph =
        BuildOrderGraph::from_topological_parts(specs, edges, 0, reachability_states, true)?;
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
        spin_graph,
        searched_nodes,
        reachability_states,
        scratch_bytes,
    })
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
