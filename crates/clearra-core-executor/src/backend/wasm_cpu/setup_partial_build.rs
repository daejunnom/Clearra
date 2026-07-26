use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_problem::SearchProblem;

use super::{
    buildup::{merge_deleted_rows, place_and_clear},
    catalog::GeometryCatalog,
    exact_collections::ExactHashMap,
    geometry::{add_packed_piece, CompiledGeometryFamily, GeometryCompletionOracle},
    reachability::ReachabilityWorkspace,
    WasmExactSearchError,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PartialStateKey {
    board: u64,
    remaining: u64,
    packed_counts: u32,
    deleted_rows: u16,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PartialBuildNode {
    pub(super) board: u64,
    pub(super) edge_start: u32,
    pub(super) edge_count: u32,
    shape_index: u32,
    pub(super) deleted_rows: u16,
    pub(super) depth: u8,
    flags: u8,
}

const NODE_LIVE: u8 = 1 << 0;
const NODE_ACCEPTING: u8 = 1 << 1;
const NO_SHAPE_INDEX: u32 = u32::MAX;
const MAX_SETUP_CANDIDATE_LOCKS: u8 = 8;

impl PartialBuildNode {
    pub(super) const fn live(self) -> bool {
        self.flags & NODE_LIVE != 0
    }

    pub(super) const fn accepting(self) -> bool {
        self.flags & NODE_ACCEPTING != 0
    }

    pub(super) const fn shape_index(self) -> Option<u32> {
        if self.shape_index == NO_SHAPE_INDEX {
            None
        } else {
            Some(self.shape_index)
        }
    }

    fn set_live(&mut self, live: bool) {
        self.flags = (self.flags & !NODE_LIVE) | u8::from(live) * NODE_LIVE;
    }

    fn set_accepting(&mut self, accepting: bool) {
        self.flags = (self.flags & !NODE_ACCEPTING) | u8::from(accepting) * NODE_ACCEPTING;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PartialBuildEdge {
    pub(super) to: u32,
    pub(super) piece: PieceKind,
    pub(super) x: i8,
    pub(super) y: i8,
    rotation_and_cleared_lines: u8,
}

impl PartialBuildEdge {
    fn new(
        to: u32,
        piece: PieceKind,
        rotation: u8,
        x: i8,
        y: i8,
        cleared_lines: u8,
    ) -> Result<Self, WasmExactSearchError> {
        if rotation > 3 || cleared_lines > 7 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_edge_metadata_overflow",
            ));
        }
        Ok(Self {
            to,
            piece,
            x,
            y,
            rotation_and_cleared_lines: rotation | (cleared_lines << 2),
        })
    }

    pub(super) const fn rotation(self) -> u8 {
        self.rotation_and_cleared_lines & 0b11
    }

    pub(super) const fn cleared_lines(self) -> u8 {
        self.rotation_and_cleared_lines >> 2
    }
}

#[derive(Clone, Debug)]
pub(super) struct SetupShape {
    pub(super) board: u64,
    pub(super) min_locks: u8,
    pub(super) max_locks: u8,
}

pub(super) struct PartialBuildGraph {
    pub(super) nodes: Vec<PartialBuildNode>,
    pub(super) edges: Vec<PartialBuildEdge>,
    pub(super) shapes: Vec<SetupShape>,
    pub(super) root: u32,
    pub(super) resource_truncated: bool,
}

pub(super) enum PartialBuildAdvance {
    Pending,
    Complete {
        graph: PartialBuildGraph,
        geometry_family_count: String,
        geometry_expanded_nodes: usize,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PartialBuildPhase {
    CollectRows,
    EmitEdges,
}

#[derive(Clone, Copy, Debug)]
struct ActivePartialState {
    remaining: u64,
    node: u32,
    packed_counts: u32,
}

pub(super) struct PartialBuildGraphBuilder {
    completion_oracle: GeometryCompletionOracle,
    reachability: ReachabilityWorkspace,
    nodes: Vec<PartialBuildNode>,
    edges: Vec<PartialBuildEdge>,
    current_states: Vec<ActivePartialState>,
    next_states: Vec<ActivePartialState>,
    current_cursor: usize,
    current_depth: u8,
    phase: PartialBuildPhase,
    available_row_cursor: usize,
    edge_source: Option<ActivePartialState>,
    source_edges: Vec<PartialBuildEdge>,
    available_rows: Vec<u32>,
    next_layer: ExactHashMap<PartialStateKey, u32>,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    resource_truncated: bool,
}

impl PartialBuildGraphBuilder {
    pub(super) fn new(
        compiled: CompiledGeometryFamily,
        catalog: &GeometryCatalog,
        problem: &SearchProblem,
    ) -> Result<Self, WasmExactSearchError> {
        let mut reachability = ReachabilityWorkspace::default();
        reachability.configure(catalog.skeleton_count());
        reachability.configure_kick_profile(problem.kick_profile().profile_id());
        let geometry_family_count = compiled
            .candidate_family_count
            .map_or_else(|| "overflow".to_owned(), |count| count.to_string());
        let nodes = vec![PartialBuildNode {
            board: catalog.initial_board(),
            edge_start: 0,
            edge_count: 0,
            shape_index: NO_SHAPE_INDEX,
            deleted_rows: 0,
            depth: 0,
            flags: 0,
        }];
        let current_states = vec![ActivePartialState {
            remaining: catalog.required_cells(),
            node: 0,
            packed_counts: 0,
        }];
        Ok(Self {
            completion_oracle: compiled.completion_oracle,
            reachability,
            nodes,
            edges: Vec::new(),
            current_states,
            next_states: Vec::new(),
            current_cursor: 0,
            current_depth: 0,
            phase: PartialBuildPhase::CollectRows,
            available_row_cursor: 0,
            edge_source: None,
            source_edges: Vec::new(),
            available_rows: Vec::new(),
            next_layer: ExactHashMap::default(),
            geometry_family_count,
            geometry_expanded_nodes: compiled.expanded_nodes,
            resource_truncated: false,
        })
    }

    pub(super) fn advance(
        &mut self,
        catalog: &GeometryCatalog,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<PartialBuildAdvance, WasmExactSearchError> {
        for _ in 0..work_budget.max(1) {
            if control.is_cancelled() {
                return Ok(PartialBuildAdvance::Cancelled);
            }
            match self.phase {
                PartialBuildPhase::CollectRows => {
                    if let Some(complete) = self.collect_rows(catalog, control)? {
                        return Ok(complete);
                    }
                }
                PartialBuildPhase::EmitEdges => {
                    self.emit_edge_task(catalog, control)?;
                }
            }
        }
        Ok(PartialBuildAdvance::Pending)
    }

    fn collect_rows(
        &mut self,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
    ) -> Result<Option<PartialBuildAdvance>, WasmExactSearchError> {
        if self.current_cursor == self.current_states.len() {
            if self.current_depth == 10 || self.next_states.is_empty() {
                return Ok(Some(PartialBuildAdvance::Complete {
                    graph: self.finish()?,
                    geometry_family_count: self.geometry_family_count.clone(),
                    geometry_expanded_nodes: self.geometry_expanded_nodes,
                }));
            }
            std::mem::swap(&mut self.current_states, &mut self.next_states);
            self.next_states.clear();
            self.next_layer.clear();
            self.current_depth += 1;
            self.current_cursor = 0;
            return Ok(None);
        }

        let source = self.current_states[self.current_cursor];
        self.current_cursor += 1;
        if self.current_depth == 10 {
            let node = &mut self.nodes[source.node as usize];
            let accepting = node.board == 0 && source.remaining == 0;
            node.set_accepting(accepting);
            node.set_live(accepting);
            return Ok(None);
        }
        self.completion_oracle.collect_available_rows(
            source.remaining,
            source.packed_counts,
            catalog,
            &mut self.available_rows,
            control,
        )?;
        self.available_row_cursor = 0;
        self.edge_source = Some(source);
        self.source_edges.clear();
        self.phase = PartialBuildPhase::EmitEdges;
        Ok(None)
    }

    fn emit_edge_task(
        &mut self,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.available_row_cursor == self.available_rows.len() {
            self.flush_source_edges()?;
            self.edge_source = None;
            self.available_rows.clear();
            self.available_row_cursor = 0;
            self.phase = PartialBuildPhase::CollectRows;
            return Ok(());
        }

        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let source_state = self
            .edge_source
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_source_missing",
            ))?;
        let row_id = self.available_rows[self.available_row_cursor];
        self.available_row_cursor += 1;
        let source = self.nodes[source_state.node as usize];
        let row = catalog.skeleton(row_id);
        let packed_counts =
            add_packed_piece(source_state.packed_counts, super::piece_index(row.piece)).ok_or(
                WasmExactSearchError::InvalidProblem("setup_partial_build_piece_count_overflow"),
            )?;
        let remaining = source_state.remaining ^ row.cells;
        for realization in catalog.instantiations(row_id, source.deleted_rows) {
            if source.board & realization.lock_mask != 0
                || !self.reachability.lock_reachable_instantiated(
                    catalog,
                    source.board,
                    row.piece,
                    realization,
                )
            {
                continue;
            }
            let (board, cleared_current, _) = place_and_clear(
                catalog.width(),
                catalog.height(),
                source.board | realization.lock_mask,
            );
            let Some(deleted_rows) =
                merge_deleted_rows(catalog.height(), source.deleted_rows, cleared_current)
            else {
                continue;
            };
            let key = PartialStateKey {
                board,
                remaining,
                packed_counts,
                deleted_rows,
            };
            let target = if let Some(target) = self.next_layer.get(&key).copied() {
                target
            } else {
                let target = u32::try_from(self.nodes.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_partial_build_node_index_overflow")
                })?;
                self.nodes.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "setup_partial_build_node_storage_unavailable",
                    )
                })?;
                self.next_states.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "setup_partial_build_frontier_storage_unavailable",
                    )
                })?;
                self.next_layer.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "setup_partial_build_dedupe_storage_unavailable",
                    )
                })?;
                self.nodes.push(PartialBuildNode {
                    board,
                    edge_start: 0,
                    edge_count: 0,
                    shape_index: NO_SHAPE_INDEX,
                    deleted_rows,
                    depth: source.depth + 1,
                    flags: 0,
                });
                self.next_layer.insert(key, target);
                self.next_states.push(ActivePartialState {
                    remaining,
                    node: target,
                    packed_counts,
                });
                target
            };
            self.source_edges.push(PartialBuildEdge::new(
                target,
                row.piece,
                realization.rotation.quarter_turns(),
                realization.x,
                realization.lock_y,
                cleared_current.count_ones() as u8,
            )?);
        }
        Ok(())
    }

    fn flush_source_edges(&mut self) -> Result<(), WasmExactSearchError> {
        let Some(source) = self.edge_source else {
            return Ok(());
        };
        self.source_edges.sort_unstable_by_key(|edge| {
            (
                edge.to,
                edge.piece,
                edge.rotation(),
                edge.x,
                edge.y,
                edge.cleared_lines(),
            )
        });
        self.source_edges.dedup_by_key(|edge| (edge.to, edge.piece));
        self.edges
            .try_reserve(self.source_edges.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_partial_build_edge_storage_unavailable")
            })?;
        let edge_start = self.edges.len();
        self.edges.extend(self.source_edges.drain(..));
        self.nodes[source.node as usize].edge_start = edge_start as u32;
        self.nodes[source.node as usize].edge_count = (self.edges.len() - edge_start) as u32;
        Ok(())
    }

    fn finish(&mut self) -> Result<PartialBuildGraph, WasmExactSearchError> {
        for index in (0..self.nodes.len()).rev() {
            if self.nodes[index].accepting() {
                continue;
            }
            let start = self.nodes[index].edge_start as usize;
            let end = start + self.nodes[index].edge_count as usize;
            let live = self.edges[start..end]
                .iter()
                .any(|edge| self.nodes[edge.to as usize].live());
            self.nodes[index].set_live(live);
        }
        self.compact_live_graph()?;

        let mut shape_indexes = ExactHashMap::<u64, u32>::default();
        let mut shapes = Vec::<SetupShape>::new();
        for node in &mut self.nodes {
            if !node.live() || !(1..=MAX_SETUP_CANDIDATE_LOCKS).contains(&node.depth) {
                continue;
            }
            let shape_index = if let Some(index) = shape_indexes.get(&node.board).copied() {
                let shape = &mut shapes[index as usize];
                shape.min_locks = shape.min_locks.min(node.depth);
                shape.max_locks = shape.max_locks.max(node.depth);
                index
            } else {
                let index = shapes.len() as u32;
                shapes.push(SetupShape {
                    board: node.board,
                    min_locks: node.depth,
                    max_locks: node.depth,
                });
                shape_indexes.insert(node.board, index);
                index
            };
            node.shape_index = shape_index;
        }
        Ok(PartialBuildGraph {
            nodes: std::mem::take(&mut self.nodes),
            edges: std::mem::take(&mut self.edges),
            shapes,
            root: 0,
            resource_truncated: self.resource_truncated,
        })
    }

    fn compact_live_graph(&mut self) -> Result<(), WasmExactSearchError> {
        let original_node_count = self.nodes.len();
        let mut node_remap = Vec::<u32>::new();
        node_remap
            .try_reserve_exact(original_node_count)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_partial_build_compaction_storage_unavailable",
                )
            })?;
        node_remap.resize(original_node_count, u32::MAX);

        let mut node_write = 0_usize;
        for node_read in 0..original_node_count {
            if node_read != self.root_index() && !self.nodes[node_read].live() {
                continue;
            }
            node_remap[node_read] = u32::try_from(node_write).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_partial_build_node_index_overflow")
            })?;
            if node_write != node_read {
                self.nodes[node_write] = self.nodes[node_read];
            }
            node_write += 1;
        }
        self.nodes.truncate(node_write);

        let mut edge_write = 0_usize;
        for node in &mut self.nodes {
            let old_start = node.edge_start as usize;
            let old_end = old_start + node.edge_count as usize;
            let new_start = edge_write;
            for edge_read in old_start..old_end {
                let mut edge = self.edges[edge_read];
                let target = node_remap[edge.to as usize];
                if target == u32::MAX {
                    continue;
                }
                edge.to = target;
                self.edges[edge_write] = edge;
                edge_write += 1;
            }
            node.edge_start = u32::try_from(new_start).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_partial_build_edge_index_overflow")
            })?;
            node.edge_count = u32::try_from(edge_write - new_start).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_partial_build_edge_count_overflow")
            })?;
        }
        self.edges.truncate(edge_write);
        Ok(())
    }

    const fn root_index(&self) -> usize {
        0
    }
}
