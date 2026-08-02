//! SRP rationale: this module has one change reason: exact partial-setup BuildUp state
//! transitions and their compact candidate representation.

use std::hash::{Hash, Hasher};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_problem::{SearchProblem, SetupPathDetail};

use super::{
    buildup::{merge_deleted_rows, place_and_clear},
    catalog::GeometryCatalog,
    exact_collections::ExactHashMap,
    geometry::{add_packed_piece, CompiledGeometryFamily, GeometryCompletionOracle},
    reachability::ReachabilityWorkspace,
    WasmExactSearchError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PartialStateKey {
    board: u64,
    remaining_and_deleted_rows: u64,
    placement_set_id: u32,
    packed_counts: u32,
}

impl PartialStateKey {
    fn new(
        board: u64,
        remaining: u64,
        placement_set_id: u32,
        packed_counts: u32,
        deleted_rows: u16,
    ) -> Result<Self, WasmExactSearchError> {
        if remaining >> PARTIAL_STATE_REMAINING_BITS != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_remaining_key_overflow",
            ));
        }
        Ok(Self {
            board,
            remaining_and_deleted_rows: remaining
                | (u64::from(deleted_rows) << PARTIAL_STATE_REMAINING_BITS),
            placement_set_id,
            packed_counts,
        })
    }
}

impl Hash for PartialStateKey {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut digest = self.board ^ self.remaining_and_deleted_rows.rotate_left(23);
        digest ^= u64::from(self.placement_set_id).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        digest ^= u64::from(self.packed_counts).rotate_left(37);
        state.write_u64(digest);
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PartialBuildNode {
    pub(super) board: u64,
    pub(super) edge_start: u32,
    pub(super) edge_count: u32,
    placement_set_or_shape_index: u32,
    pub(super) deleted_rows: u16,
    pub(super) depth: u8,
    flags: u8,
}

const NODE_LIVE: u8 = 1 << 0;
const NODE_ACCEPTING: u8 = 1 << 1;
const NODE_HAS_SHAPE: u8 = 1 << 2;
const MAX_SETUP_CANDIDATE_LOCKS: u8 = 10;

impl PartialBuildNode {
    pub(super) const fn live(self) -> bool {
        self.flags & NODE_LIVE != 0
    }

    pub(super) const fn accepting(self) -> bool {
        self.flags & NODE_ACCEPTING != 0
    }

    pub(super) const fn shape_index(self) -> Option<u32> {
        if self.flags & NODE_HAS_SHAPE != 0 {
            Some(self.placement_set_or_shape_index)
        } else {
            None
        }
    }

    const fn placement_set_id(self) -> u32 {
        self.placement_set_or_shape_index
    }

    fn set_shape_index(&mut self, shape_index: u32) {
        self.placement_set_or_shape_index = shape_index;
        self.flags |= NODE_HAS_SHAPE;
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
    placement_set_id: u32,
    deleted_rows: u16,
}

impl SetupShape {
    pub(super) const fn new(board: u64, placement_set_id: u32, deleted_rows: u16) -> Self {
        Self {
            board,
            placement_set_id,
            deleted_rows,
        }
    }
}

pub(super) struct PartialBuildGraph {
    pub(super) nodes: Vec<PartialBuildNode>,
    pub(super) edges: Vec<PartialBuildEdge>,
    edge_rows: Vec<u16>,
    pub(super) shapes: Vec<SetupShape>,
    placement_sets: PlacementSetStorage,
    shape_target_nodes: Vec<u32>,
    compact_continuation: bool,
    pub(super) root: u32,
    pub(super) resource_truncated: bool,
}

impl PartialBuildGraph {
    pub(super) fn setup_id_for_shape(&self, shape_index: usize) -> Option<String> {
        let shape = self.shapes.get(shape_index)?;
        let placement_rows = self.placement_sets.get(shape.placement_set_id)?;
        SetupPathDetail::setup_id_for(shape.board, shape.deleted_rows, placement_rows)
    }

    pub(super) fn shape_index_for_detail(&self, detail: &SetupPathDetail) -> Option<usize> {
        self.shapes.iter().position(|shape| {
            shape.board == detail.board_mask()
                && shape.deleted_rows == detail.deleted_rows()
                && self
                    .placement_sets
                    .get(shape.placement_set_id)
                    .is_some_and(|rows| rows == detail.placement_rows())
        })
    }

    pub(super) fn edge_row_id(&self, edge_index: usize) -> Option<u16> {
        self.edge_rows.get(edge_index).copied()
    }

    pub(super) fn shape_target_node(&self, shape_index: usize) -> Option<u32> {
        self.shape_target_nodes.get(shape_index).copied()
    }

    pub(super) const fn uses_compact_continuation(&self) -> bool {
        self.compact_continuation
    }

    #[cfg(test)]
    fn placement_rows_for_node(&self, node: PartialBuildNode) -> Option<u128> {
        let shape = self.shapes.get(node.shape_index()? as usize)?;
        self.placement_sets.get(shape.placement_set_id)
    }
}

pub(super) enum PartialBuildAdvance {
    Pending,
    Complete {
        graph: PartialBuildGraph,
        geometry_family_count: String,
        geometry_expanded_nodes: usize,
        tablebase_pruned_states: usize,
    },
    PrefixComplete {
        prefix: SetupPartialBuildPrefix,
        geometry_family_count: String,
        geometry_expanded_nodes: usize,
        tablebase_pruned_states: usize,
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
    node: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PartialBuildResidual {
    pub(super) remaining: u64,
    pub(super) packed_counts: u32,
}

pub(super) struct SetupPartialBuildPrefix {
    pub(super) nodes: Vec<PartialBuildNode>,
    pub(super) edges: Vec<PartialBuildEdge>,
    pub(super) edge_rows: Vec<u16>,
    pub(super) residuals: Vec<PartialBuildResidual>,
    pub(super) completion_oracle: Option<GeometryCompletionOracle>,
    placement_sets: PlacementSetStorage,
    candidate_depth: u8,
    resource_truncated: bool,
}

impl SetupPartialBuildPrefix {
    pub(super) const fn candidate_depth(&self) -> u8 {
        self.candidate_depth
    }

    pub(super) fn finalize(
        mut self,
        mut terminal_classes: Vec<u32>,
    ) -> Result<(PartialBuildGraph, Vec<u32>), WasmExactSearchError> {
        if terminal_classes.len() != self.nodes.len()
            || self.residuals.len() != self.nodes.len()
            || self.edges.len() != self.edge_rows.len()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_prefix_finalize_mismatch",
            ));
        }
        for index in 0..self.nodes.len() {
            if self.nodes[index].depth != self.candidate_depth {
                terminal_classes[index] = u32::MAX;
                continue;
            }
            if self.candidate_depth == MAX_SETUP_CANDIDATE_LOCKS {
                let accepting =
                    self.nodes[index].board == 0 && self.residuals[index].remaining == 0;
                self.nodes[index].set_accepting(accepting);
                if !accepting {
                    terminal_classes[index] = u32::MAX;
                }
            }
            self.nodes[index].set_live(terminal_classes[index] != u32::MAX);
        }
        for index in (0..self.nodes.len()).rev() {
            if self.nodes[index].live() || self.nodes[index].depth == self.candidate_depth {
                continue;
            }
            let start = self.nodes[index].edge_start as usize;
            let end = start + self.nodes[index].edge_count as usize;
            let live = self.edges[start..end]
                .iter()
                .any(|edge| self.nodes[edge.to as usize].live());
            self.nodes[index].set_live(live);
        }
        compact_prefix_graph(
            &mut self.nodes,
            &mut self.edges,
            &mut self.edge_rows,
            &mut self.residuals,
            &mut terminal_classes,
        )?;
        let (shapes, shape_target_nodes) =
            index_setup_shapes(&mut self.nodes, self.candidate_depth)?;
        Ok((
            PartialBuildGraph {
                nodes: self.nodes,
                edges: self.edges,
                edge_rows: self.edge_rows,
                shapes,
                placement_sets: std::mem::take(&mut self.placement_sets),
                shape_target_nodes,
                compact_continuation: true,
                root: 0,
                resource_truncated: self.resource_truncated,
            },
            terminal_classes,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingPartialBuildEdge {
    edge: PartialBuildEdge,
    row_id: u16,
}

#[derive(Clone, Copy, Debug)]
struct SelectedSetupDetail {
    board: u64,
    placement_rows: u128,
    rows: [u16; MAX_SETUP_CANDIDATE_LOCKS as usize],
    deleted_rows: u16,
    depth: u8,
}

const SETUP_ROW_BITS: usize = 12;
const SETUP_ROW_MASK: u128 = (1_u128 << SETUP_ROW_BITS) - 1;
const MAX_SETUP_ROW_ID: u32 = SETUP_ROW_MASK as u32 - 1;
const COMPACT_PLACEMENT_SET_MAX_DEPTH: u8 = u64::BITS as u8 / SETUP_ROW_BITS as u8;
const PARTIAL_STATE_REMAINING_BITS: u32 = 48;

enum PlacementSetStorage {
    Compact(Vec<u64>),
    Full(Vec<u128>),
}

impl PlacementSetStorage {
    fn new(max_depth: u8) -> Self {
        if max_depth <= COMPACT_PLACEMENT_SET_MAX_DEPTH {
            Self::Compact(vec![0])
        } else {
            Self::Full(vec![0])
        }
    }

    fn get(&self, id: u32) -> Option<u128> {
        let index = id as usize;
        match self {
            Self::Compact(values) => values.get(index).copied().map(u128::from),
            Self::Full(values) => values.get(index).copied(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Compact(values) => values.len(),
            Self::Full(values) => values.len(),
        }
    }

    fn try_push(&mut self, value: u128) -> Result<(), ()> {
        match self {
            Self::Compact(values) => {
                let value = u64::try_from(value).map_err(|_| ())?;
                values.try_reserve(1).map_err(|_| ())?;
                values.push(value);
            }
            Self::Full(values) => {
                values.try_reserve(1).map_err(|_| ())?;
                values.push(value);
            }
        }
        Ok(())
    }
}

impl Default for PlacementSetStorage {
    fn default() -> Self {
        Self::Full(Vec::new())
    }
}

enum PlacementSetIndex {
    Compact(ExactHashMap<u64, u32>),
    Full(ExactHashMap<u128, u32>),
}

impl PlacementSetIndex {
    fn new(max_depth: u8) -> Self {
        if max_depth <= COMPACT_PLACEMENT_SET_MAX_DEPTH {
            Self::Compact(ExactHashMap::default())
        } else {
            Self::Full(ExactHashMap::default())
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Compact(index) => index.clear(),
            Self::Full(index) => index.clear(),
        }
    }

    fn get(&self, value: u128) -> Option<u32> {
        match self {
            Self::Compact(index) => u64::try_from(value)
                .ok()
                .and_then(|value| index.get(&value).copied()),
            Self::Full(index) => index.get(&value).copied(),
        }
    }

    fn try_insert(&mut self, value: u128, id: u32) -> Result<(), ()> {
        match self {
            Self::Compact(index) => {
                let value = u64::try_from(value).map_err(|_| ())?;
                index.try_reserve(1).map_err(|_| ())?;
                index.insert(value, id);
            }
            Self::Full(index) => {
                index.try_reserve(1).map_err(|_| ())?;
                index.insert(value, id);
            }
        }
        Ok(())
    }
}

pub(super) struct PartialBuildGraphBuilder {
    completion_oracle: Option<GeometryCompletionOracle>,
    reachability: ReachabilityWorkspace,
    nodes: Vec<PartialBuildNode>,
    edges: Vec<PartialBuildEdge>,
    edge_rows: Vec<u16>,
    current_states: Vec<ActivePartialState>,
    next_states: Vec<ActivePartialState>,
    current_cursor: usize,
    current_depth: u8,
    phase: PartialBuildPhase,
    available_row_cursor: usize,
    edge_source: Option<ActivePartialState>,
    source_edges: Vec<PendingPartialBuildEdge>,
    available_rows: Vec<u32>,
    next_layer: ExactHashMap<PartialStateKey, u32>,
    placement_sets: PlacementSetStorage,
    next_placement_sets: PlacementSetIndex,
    geometry_family_count: String,
    geometry_expanded_nodes: usize,
    tablebase_pruned_states: usize,
    placement_identity_depth: u8,
    preserve_edge_row_identity_depth: u8,
    residuals: Vec<PartialBuildResidual>,
    selected_detail: Option<SelectedSetupDetail>,
    prefix_only: bool,
    resource_truncated: bool,
}

impl PartialBuildGraphBuilder {
    #[cfg(test)]
    pub(super) fn new(
        compiled: CompiledGeometryFamily,
        catalog: &GeometryCatalog,
        problem: &SearchProblem,
        placement_identity_depth: u8,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_mode(compiled, catalog, problem, placement_identity_depth, false)
    }

    pub(super) fn new_candidate_prefix(
        compiled: CompiledGeometryFamily,
        catalog: &GeometryCatalog,
        problem: &SearchProblem,
        candidate_depth: u8,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_mode(compiled, catalog, problem, candidate_depth, true)
    }

    pub(super) fn new_selected_detail(
        compiled: CompiledGeometryFamily,
        catalog: &GeometryCatalog,
        problem: &SearchProblem,
        detail: &SetupPathDetail,
    ) -> Result<Self, WasmExactSearchError> {
        let depth = packed_setup_row_count(detail.placement_rows(), MAX_SETUP_CANDIDATE_LOCKS)?;
        let depth = u8::try_from(depth).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_path_detail_depth_overflow")
        })?;
        let rows = decode_placement_rows(detail.placement_rows(), usize::from(depth))?;
        let mut builder = Self::new_with_mode(compiled, catalog, problem, depth, false)?;
        builder.selected_detail = Some(SelectedSetupDetail {
            board: detail.board_mask(),
            placement_rows: detail.placement_rows(),
            rows,
            deleted_rows: detail.deleted_rows(),
            depth,
        });
        builder.preserve_edge_row_identity_depth = 10;
        Ok(builder)
    }

    fn new_with_mode(
        compiled: CompiledGeometryFamily,
        catalog: &GeometryCatalog,
        problem: &SearchProblem,
        placement_identity_depth: u8,
        prefix_only: bool,
    ) -> Result<Self, WasmExactSearchError> {
        if placement_identity_depth > MAX_SETUP_CANDIDATE_LOCKS {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_identity_depth_invalid",
            ));
        }
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
            placement_set_or_shape_index: 0,
            deleted_rows: 0,
            depth: 0,
            flags: 0,
        }];
        let current_states = vec![ActivePartialState { node: 0 }];
        Ok(Self {
            completion_oracle: Some(compiled.completion_oracle),
            reachability,
            nodes,
            edges: Vec::new(),
            edge_rows: Vec::new(),
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
            placement_sets: PlacementSetStorage::new(placement_identity_depth),
            next_placement_sets: PlacementSetIndex::new(placement_identity_depth),
            geometry_family_count,
            geometry_expanded_nodes: compiled.expanded_nodes,
            tablebase_pruned_states: compiled.tablebase_pruned_states,
            placement_identity_depth,
            preserve_edge_row_identity_depth: 0,
            residuals: vec![PartialBuildResidual {
                remaining: catalog.required_cells(),
                packed_counts: 0,
            }],
            selected_detail: None,
            prefix_only,
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

    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn geometry_expanded_nodes(&self) -> usize {
        self.geometry_expanded_nodes
    }

    pub(super) fn frontier_progress(&self) -> (usize, usize, usize, usize) {
        let layer_count = usize::from(self.candidate_depth()).max(1);
        let layer_index = usize::from(self.current_depth).min(layer_count.saturating_sub(1));
        let completed = match self.phase {
            PartialBuildPhase::CollectRows => self.current_cursor,
            PartialBuildPhase::EmitEdges => self.current_cursor.saturating_sub(1),
        }
        .min(self.current_states.len());
        (
            layer_index,
            layer_count,
            completed,
            self.current_states.len(),
        )
    }

    fn collect_rows(
        &mut self,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
    ) -> Result<Option<PartialBuildAdvance>, WasmExactSearchError> {
        if self.prefix_only && self.current_depth == self.candidate_depth() {
            let prefix = self.finish_prefix()?;
            return Ok(Some(PartialBuildAdvance::PrefixComplete {
                prefix,
                geometry_family_count: self.geometry_family_count.clone(),
                geometry_expanded_nodes: self.geometry_expanded_nodes,
                tablebase_pruned_states: self.tablebase_pruned_states,
            }));
        }
        if self.current_cursor == self.current_states.len() {
            if self.current_depth == 10 || self.next_states.is_empty() {
                let graph = self.finish()?;
                return Ok(Some(PartialBuildAdvance::Complete {
                    graph,
                    geometry_family_count: self.geometry_family_count.clone(),
                    geometry_expanded_nodes: self.geometry_expanded_nodes,
                    tablebase_pruned_states: self.tablebase_pruned_states,
                }));
            }
            std::mem::swap(&mut self.current_states, &mut self.next_states);
            self.next_states.clear();
            self.next_layer.clear();
            self.next_placement_sets.clear();
            self.current_depth += 1;
            self.current_cursor = 0;
            return Ok(None);
        }

        let source = self.current_states[self.current_cursor];
        self.current_cursor += 1;
        if let Some(detail) = self.selected_detail {
            if source.node >= self.nodes.len() as u32 {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_path_detail_source_node_invalid",
                ));
            }
            let source_node = self.nodes[source.node as usize];
            if source_node.depth == detail.depth {
                let placement_rows = self
                    .placement_sets
                    .get(source_node.placement_set_id())
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_path_detail_placement_set_missing",
                    ))?;
                if source_node.board != detail.board
                    || source_node.deleted_rows != detail.deleted_rows
                    || placement_rows != detail.placement_rows
                {
                    return Ok(None);
                }
            }
        }
        if self.current_depth == 10 {
            let node = &mut self.nodes[source.node as usize];
            let accepting = node.board == 0 && self.residuals[source.node as usize].remaining == 0;
            node.set_accepting(accepting);
            node.set_live(accepting);
            return Ok(None);
        }
        let residual = self.residuals[source.node as usize];
        self.completion_oracle
            .as_mut()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_completion_oracle_missing",
            ))?
            .collect_available_rows(
                residual.remaining,
                residual.packed_counts,
                self.nodes[source.node as usize].deleted_rows != 0,
                catalog,
                &mut self.available_rows,
                control,
            )?;
        if let Some(detail) = self.selected_detail {
            if self.nodes[source.node as usize].depth < detail.depth {
                let selected_rows = &detail.rows[..usize::from(detail.depth)];
                self.available_rows.retain(|row_id| {
                    u16::try_from(*row_id)
                        .ok()
                        .is_some_and(|row_id| selected_rows.binary_search(&row_id).is_ok())
                });
            }
        }
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
        let source_residual = self.residuals[source_state.node as usize];
        let row = catalog.skeleton(row_id);
        let packed_counts =
            add_packed_piece(source_residual.packed_counts, super::piece_index(row.piece)).ok_or(
                WasmExactSearchError::InvalidProblem("setup_partial_build_piece_count_overflow"),
            )?;
        let placement_set_id = if source.depth < self.placement_identity_depth {
            self.extend_placement_set(source.placement_set_id(), source.depth, row_id)?
        } else {
            0
        };
        let remaining = source_residual.remaining ^ row.cells;
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
            let key = PartialStateKey::new(
                board,
                remaining,
                placement_set_id,
                packed_counts,
                deleted_rows,
            )?;
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
                    placement_set_or_shape_index: placement_set_id,
                    deleted_rows,
                    depth: source.depth + 1,
                    flags: 0,
                });
                self.residuals.push(PartialBuildResidual {
                    remaining,
                    packed_counts,
                });
                self.next_layer.insert(key, target);
                self.next_states.push(ActivePartialState { node: target });
                target
            };
            let row_id = u16::try_from(row_id).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_partial_build_edge_row_identity_overflow",
                )
            })?;
            self.source_edges.push(PendingPartialBuildEdge {
                edge: PartialBuildEdge::new(
                    target,
                    row.piece,
                    realization.rotation.quarter_turns(),
                    realization.x,
                    realization.lock_y,
                    cleared_current.count_ones() as u8,
                )?,
                row_id,
            });
        }
        Ok(())
    }

    fn extend_placement_set(
        &mut self,
        parent_id: u32,
        depth: u8,
        row_id: u32,
    ) -> Result<u32, WasmExactSearchError> {
        let parent =
            self.placement_sets
                .get(parent_id)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_partial_build_placement_set_missing",
                ))?;
        let placement_rows = insert_setup_placement_row(parent, depth, row_id)?;
        if let Some(id) = self.next_placement_sets.get(placement_rows) {
            return Ok(id);
        }
        let id = u32::try_from(self.placement_sets.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_placement_set_index_overflow")
        })?;
        self.placement_sets.try_push(placement_rows).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "setup_partial_build_placement_set_storage_unavailable",
            )
        })?;
        self.next_placement_sets
            .try_insert(placement_rows, id)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_partial_build_placement_set_index_storage_unavailable",
                )
            })?;
        Ok(id)
    }

    fn flush_source_edges(&mut self) -> Result<(), WasmExactSearchError> {
        let Some(source) = self.edge_source else {
            return Ok(());
        };
        self.source_edges.sort_unstable_by_key(|pending| {
            let edge = pending.edge;
            (
                edge.to,
                edge.piece,
                pending.row_id,
                edge.rotation(),
                edge.x,
                edge.y,
                edge.cleared_lines(),
            )
        });
        if self.nodes[source.node as usize].depth < self.preserve_edge_row_identity_depth {
            self.source_edges.dedup_by_key(|pending| {
                (
                    pending.edge.to,
                    pending.edge.piece,
                    pending.row_id,
                    pending.edge.rotation(),
                    pending.edge.x,
                    pending.edge.y,
                    pending.edge.cleared_lines(),
                )
            });
        } else {
            self.source_edges
                .dedup_by_key(|pending| (pending.edge.to, pending.edge.piece));
        }
        self.edges
            .try_reserve(self.source_edges.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_partial_build_edge_storage_unavailable")
            })?;
        self.edge_rows
            .try_reserve(self.source_edges.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_partial_build_edge_row_storage_unavailable",
                )
            })?;
        let edge_start = self.edges.len();
        for pending in self.source_edges.drain(..) {
            self.edges.push(pending.edge);
            self.edge_rows.push(pending.row_id);
        }
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
        let (shapes, shape_target_nodes) =
            index_setup_shapes(&mut self.nodes, self.placement_identity_depth)?;
        Ok(PartialBuildGraph {
            nodes: std::mem::take(&mut self.nodes),
            edges: std::mem::take(&mut self.edges),
            edge_rows: std::mem::take(&mut self.edge_rows),
            shapes,
            placement_sets: std::mem::take(&mut self.placement_sets),
            shape_target_nodes,
            compact_continuation: false,
            root: 0,
            resource_truncated: self.resource_truncated,
        })
    }

    fn finish_prefix(&mut self) -> Result<SetupPartialBuildPrefix, WasmExactSearchError> {
        let candidate_depth = self.candidate_depth();
        if candidate_depth == 0
            || self.current_depth != candidate_depth
            || self.nodes.len() != self.residuals.len()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_partial_build_prefix_state_invalid",
            ));
        }
        Ok(SetupPartialBuildPrefix {
            nodes: std::mem::take(&mut self.nodes),
            edges: std::mem::take(&mut self.edges),
            edge_rows: std::mem::take(&mut self.edge_rows),
            residuals: std::mem::take(&mut self.residuals),
            completion_oracle: self.completion_oracle.take(),
            placement_sets: std::mem::take(&mut self.placement_sets),
            candidate_depth,
            resource_truncated: self.resource_truncated,
        })
    }

    const fn candidate_depth(&self) -> u8 {
        self.placement_identity_depth
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
                self.edge_rows[edge_write] = self.edge_rows[edge_read];
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
        self.edge_rows.truncate(edge_write);
        Ok(())
    }

    const fn root_index(&self) -> usize {
        0
    }
}

fn compact_prefix_graph(
    nodes: &mut Vec<PartialBuildNode>,
    edges: &mut Vec<PartialBuildEdge>,
    edge_rows: &mut Vec<u16>,
    residuals: &mut Vec<PartialBuildResidual>,
    terminal_classes: &mut Vec<u32>,
) -> Result<(), WasmExactSearchError> {
    let original_node_count = nodes.len();
    if residuals.len() != original_node_count || terminal_classes.len() != original_node_count {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_partial_build_prefix_compaction_state_mismatch",
        ));
    }
    let mut node_remap = Vec::<u32>::new();
    node_remap
        .try_reserve_exact(original_node_count)
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "setup_partial_build_prefix_compaction_storage_unavailable",
            )
        })?;
    node_remap.resize(original_node_count, u32::MAX);

    let mut node_write = 0_usize;
    for node_read in 0..original_node_count {
        if node_read != 0 && !nodes[node_read].live() {
            continue;
        }
        node_remap[node_read] = u32::try_from(node_write).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_node_index_overflow")
        })?;
        if node_write != node_read {
            nodes[node_write] = nodes[node_read];
            residuals[node_write] = residuals[node_read];
            terminal_classes[node_write] = terminal_classes[node_read];
        }
        node_write += 1;
    }
    nodes.truncate(node_write);
    residuals.truncate(node_write);
    terminal_classes.truncate(node_write);

    let mut edge_write = 0_usize;
    for node in nodes {
        let old_start = node.edge_start as usize;
        let old_end = old_start + node.edge_count as usize;
        let new_start = edge_write;
        for edge_read in old_start..old_end {
            let mut edge = edges[edge_read];
            let target = node_remap[edge.to as usize];
            if target == u32::MAX {
                continue;
            }
            edge.to = target;
            edges[edge_write] = edge;
            edge_rows[edge_write] = edge_rows[edge_read];
            edge_write += 1;
        }
        node.edge_start = u32::try_from(new_start).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_edge_index_overflow")
        })?;
        node.edge_count = u32::try_from(edge_write - new_start).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_edge_count_overflow")
        })?;
    }
    edges.truncate(edge_write);
    edge_rows.truncate(edge_write);
    Ok(())
}

fn packed_setup_row_count(
    placement_rows: u128,
    max_depth: u8,
) -> Result<usize, WasmExactSearchError> {
    let mut count = 0_usize;
    while count < usize::from(max_depth) {
        let encoded = (placement_rows >> (count * SETUP_ROW_BITS)) & SETUP_ROW_MASK;
        if encoded == 0 {
            break;
        }
        count += 1;
    }
    if count == 0
        || placement_rows >> (count * SETUP_ROW_BITS) != 0
        || count > usize::from(candidate_depth_limit(max_depth))
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_path_detail_depth_invalid",
        ));
    }
    Ok(count)
}

const fn candidate_depth_limit(max_depth: u8) -> u8 {
    if max_depth < MAX_SETUP_CANDIDATE_LOCKS {
        max_depth
    } else {
        MAX_SETUP_CANDIDATE_LOCKS
    }
}

fn index_setup_shapes(
    nodes: &mut [PartialBuildNode],
    placement_identity_depth: u8,
) -> Result<(Vec<SetupShape>, Vec<u32>), WasmExactSearchError> {
    let mut shapes = Vec::<SetupShape>::new();
    let mut target_nodes = Vec::<u32>::new();
    for (node_index, node) in nodes.iter_mut().enumerate() {
        if !node.live() || !(1..=placement_identity_depth).contains(&node.depth) {
            continue;
        }
        let shape_index = u32::try_from(shapes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_shape_index_overflow")
        })?;
        shapes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_shape_storage_unavailable")
        })?;
        let placement_set_id = node.placement_set_id();
        node.set_shape_index(shape_index);
        shapes.push(SetupShape::new(
            node.board,
            placement_set_id,
            node.deleted_rows,
        ));
        target_nodes.push(u32::try_from(node_index).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_partial_build_node_index_overflow")
        })?);
    }
    Ok((shapes, target_nodes))
}

pub(super) fn insert_setup_placement_row(
    packed: u128,
    depth: u8,
    row_id: u32,
) -> Result<u128, WasmExactSearchError> {
    if row_id > MAX_SETUP_ROW_ID || depth >= MAX_SETUP_CANDIDATE_LOCKS {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_partial_build_placement_row_identity_overflow",
        ));
    }
    let encoded = u128::from(row_id + 1);
    let mut output = 0_u128;
    let mut output_index = 0_usize;
    let mut inserted = false;
    for input_index in 0..usize::from(depth) {
        let current = (packed >> (input_index * SETUP_ROW_BITS)) & SETUP_ROW_MASK;
        if !inserted && encoded < current {
            output |= encoded << (output_index * SETUP_ROW_BITS);
            output_index += 1;
            inserted = true;
        }
        output |= current << (output_index * SETUP_ROW_BITS);
        output_index += 1;
    }
    if !inserted {
        output |= encoded << (output_index * SETUP_ROW_BITS);
    }
    Ok(output)
}

pub(super) fn decode_placement_rows(
    packed: u128,
    row_count: usize,
) -> Result<[u16; MAX_SETUP_CANDIDATE_LOCKS as usize], WasmExactSearchError> {
    if row_count > MAX_SETUP_CANDIDATE_LOCKS as usize {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_candidate_catalog_row_count_overflow",
        ));
    }
    let mut rows = [0_u16; MAX_SETUP_CANDIDATE_LOCKS as usize];
    for (index, output) in rows.iter_mut().enumerate().take(row_count) {
        let encoded = (packed >> (index * SETUP_ROW_BITS)) & SETUP_ROW_MASK;
        if encoded == 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_candidate_catalog_row_identity_missing",
            ));
        }
        *output = u16::try_from(encoded - 1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_candidate_catalog_row_identity_overflow")
        })?;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::wasm_cpu::{
        geometry::{pack_piece_counts, GeometryFamilyCompileAdvance, GeometryFamilyCompileSession},
        packing_projection_hold_enabled,
        setup_finder::compile_setup_admissible_prefixes,
    };
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
    use clearra_problem::{compile_setup_search_conditions, SetupSearchQuery};
    use clearra_supply::pattern_universe::PieceMultisetKey;

    #[test]
    fn setup_partial_build_hot_state_stays_compact() {
        assert_eq!(std::mem::size_of::<PartialBuildNode>(), 24);
        assert_eq!(std::mem::size_of::<PartialStateKey>(), 24);
        assert_eq!(std::mem::size_of::<ActivePartialState>(), 4);
    }

    #[test]
    fn placement_set_storage_keeps_the_exact_depth_range() {
        let compact_rows = (1_u128 << 59) | 0x123;
        let mut compact = PlacementSetStorage::new(5);
        compact.try_push(compact_rows).expect("compact rows");
        assert_eq!(compact.get(1), Some(compact_rows));

        let full_rows = (1_u128 << 71) | compact_rows;
        let mut full = PlacementSetStorage::new(6);
        full.try_push(full_rows).expect("full rows");
        assert_eq!(full.get(1), Some(full_rows));
    }

    #[test]
    fn partial_state_key_rejects_cells_outside_the_setup_board_domain() {
        assert!(PartialStateKey::new(0, 1_u64 << 47, 0, 0, u16::MAX).is_ok());
        assert!(PartialStateKey::new(0, 1_u64 << 48, 0, 0, 0).is_err());
    }

    fn live_node(board: u64, depth: u8) -> PartialBuildNode {
        PartialBuildNode {
            board,
            edge_start: 0,
            edge_count: 0,
            placement_set_or_shape_index: u32::from(depth),
            deleted_rows: 0,
            depth,
            flags: NODE_LIVE,
        }
    }

    fn mirror_piece_language(language: &[u8]) -> Vec<u8> {
        language
            .iter()
            .map(|piece| match piece {
                3 => 4,
                4 => 3,
                5 => 6,
                6 => 5,
                piece => *piece,
            })
            .collect()
    }

    fn piece_language_frontier_counts(
        graph: &PartialBuildGraph,
        shape_index: usize,
        language: &[u8],
    ) -> Vec<usize> {
        let start = graph
            .nodes
            .iter()
            .position(|node| node.shape_index() == Some(shape_index as u32))
            .expect("shape node") as u32;
        let mut frontier = vec![start];
        let mut counts = vec![frontier.len()];
        for piece in language {
            let mut next = Vec::new();
            for node_index in frontier {
                let node = graph.nodes[node_index as usize];
                let start = node.edge_start as usize;
                let end = start + node.edge_count as usize;
                for edge in &graph.edges[start..end] {
                    if super::super::piece_index(edge.piece) as u8 == *piece {
                        next.push(edge.to);
                    }
                }
            }
            next.sort_unstable();
            next.dedup();
            counts.push(next.len());
            frontier = next;
        }
        counts
    }

    #[test]
    fn setup_candidate_identity_keeps_exact_partial_states_separate() {
        let mut nodes = [live_node(0x3c, 2), live_node(0x3c, 9), live_node(0x3c, 2)];

        let (shapes, _) =
            index_setup_shapes(&mut nodes, MAX_SETUP_CANDIDATE_LOCKS).expect("shape index");

        assert_eq!(shapes.len(), 3);
        assert_ne!(nodes[0].shape_index(), nodes[2].shape_index());
        assert_ne!(nodes[0].shape_index(), nodes[1].shape_index());
        assert_eq!(
            shapes.iter().map(|shape| shape.board).collect::<Vec<_>>(),
            vec![0x3c, 0x3c, 0x3c]
        );
    }

    #[test]
    fn setup_prefix_compaction_keeps_residuals_aligned() {
        let mut root = live_node(0, 0);
        root.edge_count = 2;
        let mut dead = live_node(0x0f, 1);
        dead.set_live(false);
        let live = live_node(0xf0, 1);
        let mut nodes = vec![root, dead, live];
        let mut edges = vec![
            PartialBuildEdge::new(1, PieceKind::I, 0, 0, 0, 0).expect("dead edge"),
            PartialBuildEdge::new(2, PieceKind::O, 0, 1, 0, 0).expect("live edge"),
        ];
        let mut edge_rows = vec![11, 22];
        let mut residuals = vec![
            PartialBuildResidual {
                remaining: 1,
                packed_counts: 10,
            },
            PartialBuildResidual {
                remaining: 2,
                packed_counts: 20,
            },
            PartialBuildResidual {
                remaining: 3,
                packed_counts: 30,
            },
        ];
        let mut terminal_classes = vec![100, 101, 102];

        compact_prefix_graph(
            &mut nodes,
            &mut edges,
            &mut edge_rows,
            &mut residuals,
            &mut terminal_classes,
        )
        .expect("compact prefix");

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].edge_count, 1);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, 1);
        assert_eq!(edges[0].piece, PieceKind::O);
        assert_eq!(edge_rows, vec![22]);
        assert_eq!(residuals[1].remaining, 3);
        assert_eq!(residuals[1].packed_counts, 30);
        assert_eq!(terminal_classes, vec![100, 102]);
    }

    #[test]
    fn setup_placement_identity_is_order_independent_and_exact() {
        let rows_3_11 = insert_setup_placement_row(
            insert_setup_placement_row(0, 0, 3).expect("first row"),
            1,
            11,
        )
        .expect("second row");
        let rows_11_3 = insert_setup_placement_row(
            insert_setup_placement_row(0, 0, 11).expect("first row"),
            1,
            3,
        )
        .expect("second row");
        let rows_3_12 = insert_setup_placement_row(
            insert_setup_placement_row(0, 0, 3).expect("first row"),
            1,
            12,
        )
        .expect("second row");

        assert_eq!(rows_3_11, rows_11_3);
        assert_ne!(rows_3_11, rows_3_12);
    }

    #[test]
    fn setup_placement_rows_decode_exact_row_ids() {
        let packed = 4_u128 | (8_u128 << 12) | (13_u128 << 24);
        let rows = decode_placement_rows(packed, 3).expect("rows");

        assert_eq!(&rows[..3], &[3, 7, 12]);
    }

    #[test]
    fn setup_id_selects_one_exact_tiling_even_when_boards_match() {
        let rows_a = insert_setup_placement_row(0, 0, 3).expect("rows a");
        let rows_b = insert_setup_placement_row(0, 0, 12).expect("rows b");
        let graph = PartialBuildGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            edge_rows: Vec::new(),
            shapes: vec![SetupShape::new(0x3c, 1, 0), SetupShape::new(0x3c, 2, 0)],
            placement_sets: PlacementSetStorage::Full(vec![0, rows_a, rows_b]),
            shape_target_nodes: vec![0, 1],
            compact_continuation: false,
            root: 0,
            resource_truncated: false,
        };
        let setup_id = graph.setup_id_for_shape(1).expect("setup id");
        let detail = SetupPathDetail::from_setup_id(&setup_id, "hold-empty").expect("path detail");

        assert_eq!(graph.shape_index_for_detail(&detail), Some(1));
        assert_ne!(graph.setup_id_for_shape(0), graph.setup_id_for_shape(1));
    }

    #[test]
    #[ignore = "full empty-4L setup graph acceptance"]
    fn line_clear_prefix_retains_its_exact_pc_completion_family() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::S, PieceKind::Z])
            .with_tablebase_requested(false);
        let conditions = compile_setup_search_conditions(&query).expect("setup conditions");
        let catalog = GeometryCatalog::compile(conditions[0].problem()).expect("geometry catalog");
        let rows = [
            (PieceKind::S, 0x0000_0000_0001_8030),
            (PieceKind::J, 0x0000_0000_000e_0200),
            (PieceKind::L, 0x0000_0000_0010_0403),
            (PieceKind::I, 0x0000_0000_0000_7800),
            (PieceKind::O, 0x0000_00c0_3000_0000),
            (PieceKind::T, 0x0000_0004_0380_0000),
            (PieceKind::Z, 0x0000_0000_0060_000c),
            (PieceKind::Z, 0x0000_0000_0c00_0180),
            (PieceKind::I, 0x0000_0003_c000_0000),
            (PieceKind::T, 0x0000_0038_0000_0040),
        ]
        .map(|(piece, cells)| {
            catalog
                .skeleton_id(piece, cells)
                .expect("known inverse-lock-clear row")
        });
        let prefix_before_clear = rows[..3]
            .iter()
            .copied()
            .enumerate()
            .fold(Ok(0_u128), |packed, (depth, row)| {
                insert_setup_placement_row(packed?, depth as u8, row)
            })
            .expect("three-row prefix");
        let prefix_after_clear = rows[..4]
            .iter()
            .copied()
            .enumerate()
            .fold(Ok(0_u128), |packed, (depth, row)| {
                insert_setup_placement_row(packed?, depth as u8, row)
            })
            .expect("four-row prefix");
        let complete = rows
            .iter()
            .copied()
            .enumerate()
            .fold(Ok(0_u128), |packed, (depth, row)| {
                insert_setup_placement_row(packed?, depth as u8, row)
            })
            .expect("complete row set");

        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let admissible_prefixes =
            compile_setup_admissible_prefixes(&conditions).expect("admissible prefixes");
        let mut target_keys = Vec::<PieceMultisetKey>::new();
        for condition in &conditions {
            let problem = condition.problem();
            let universe = problem
                .piece_source()
                .materialized_universe()
                .expect("materialized universe");
            let family = universe.packing_multiset_family(
                10,
                problem.initial_hold(),
                packing_projection_hold_enabled(problem),
            );
            target_keys.extend(family.groups().iter().map(|group| group.key()));
        }
        let target_counts = rows.iter().fold([0_u8; 7], |mut counts, row_id| {
            counts[super::super::piece_index(catalog.skeleton(*row_id).piece)] += 1;
            counts
        });
        assert!(target_keys.contains(&PieceMultisetKey::from_counts(target_counts)));
        assert!(admissible_prefixes
            .binary_search(&pack_piece_counts(target_counts))
            .is_ok());

        let mut geometry = GeometryFamilyCompileSession::new_with_tablebase(
            catalog.required_cells(),
            target_keys,
            admissible_prefixes,
            None,
        )
        .expect("geometry session");
        let mut compiled = loop {
            match geometry.advance(&catalog, 65_536, &control) {
                GeometryFamilyCompileAdvance::Pending => {}
                GeometryFamilyCompileAdvance::Complete(compiled) => break compiled,
                GeometryFamilyCompileAdvance::ResourceIncomplete(reason) => panic!("{reason}"),
                GeometryFamilyCompileAdvance::Cancelled => panic!("geometry was not cancelled"),
            }
        };
        let mut remaining = catalog.required_cells();
        let mut packed_counts = 0_u32;
        let mut available = Vec::new();
        for (index, row_id) in rows.into_iter().enumerate() {
            compiled
                .completion_oracle
                .collect_available_rows(
                    remaining,
                    packed_counts,
                    index >= 4,
                    &catalog,
                    &mut available,
                    &control,
                )
                .expect("completion rows");
            assert!(
                available.contains(&row_id),
                "geometry family lost row {row_id} from residual {remaining:010x}"
            );
            let row = catalog.skeleton(row_id);
            remaining ^= row.cells;
            packed_counts = add_packed_piece(packed_counts, super::super::piece_index(row.piece))
                .expect("packed count");
        }
        assert_eq!(remaining, 0);

        let mut builder = PartialBuildGraphBuilder::new(
            compiled,
            &catalog,
            conditions[0].problem(),
            MAX_SETUP_CANDIDATE_LOCKS,
        )
        .expect("partial graph builder");
        let graph = loop {
            match builder
                .advance(&catalog, 65_536, &control)
                .expect("partial graph advance")
            {
                PartialBuildAdvance::Pending => {}
                PartialBuildAdvance::Complete { graph, .. } => break graph,
                PartialBuildAdvance::PrefixComplete { .. } => {
                    panic!("complete graph builder returned a setup prefix")
                }
                PartialBuildAdvance::Cancelled => panic!("partial graph was not cancelled"),
            }
        };
        let before_clear = graph
            .nodes
            .iter()
            .position(|node| {
                node.board == 0x001f_8633
                    && node.deleted_rows == 0
                    && graph.placement_rows_for_node(*node) == Some(prefix_before_clear)
            })
            .expect("line-clear predecessor");
        let after_clear = graph
            .nodes
            .iter()
            .position(|node| {
                node.board == 0x633
                    && node.deleted_rows == 0b10
                    && graph.placement_rows_for_node(*node) == Some(prefix_after_clear)
            })
            .expect("line-clear prefix");
        let accepting = graph
            .nodes
            .iter()
            .position(|node| {
                node.accepting() && graph.placement_rows_for_node(*node) == Some(complete)
            })
            .expect("exact completion family");

        assert!(graph.nodes[before_clear].edge_count > 0);
        assert!(graph.nodes[after_clear].edge_count > 0);
        assert!(accepting > after_clear);
    }

    #[test]
    #[ignore = "full empty-4L setup geometry acceptance"]
    fn mirrored_s_setup_retains_known_exact_pc_suffix() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::S, PieceKind::Z])
            .with_tablebase_requested(false);
        let conditions = compile_setup_search_conditions(&query).expect("setup conditions");
        let catalog = GeometryCatalog::compile(conditions[0].problem()).expect("geometry catalog");
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let admissible_prefixes =
            compile_setup_admissible_prefixes(&conditions).expect("admissible prefixes");
        let expected_execution_prefixes = admissible_prefixes.clone();
        let mut target_keys = Vec::<PieceMultisetKey>::new();
        for condition in &conditions {
            let problem = condition.problem();
            let universe = problem
                .piece_source()
                .materialized_universe()
                .expect("materialized universe");
            let family = universe.packing_multiset_family(
                10,
                problem.initial_hold(),
                packing_projection_hold_enabled(problem),
            );
            target_keys.extend(family.groups().iter().map(|group| group.key()));
        }
        let expected_target = PieceMultisetKey::from_counts([1, 2, 2, 2, 2, 0, 1]);
        assert!(
            target_keys.contains(&expected_target),
            "known completion target is absent from setup geometry targets"
        );
        let mut geometry = GeometryFamilyCompileSession::new_with_tablebase(
            catalog.required_cells(),
            target_keys,
            admissible_prefixes,
            None,
        )
        .expect("geometry session");
        let mut compiled = loop {
            match geometry.advance(&catalog, 65_536, &control) {
                GeometryFamilyCompileAdvance::Pending => {}
                GeometryFamilyCompileAdvance::Complete(compiled) => break compiled,
                GeometryFamilyCompileAdvance::ResourceIncomplete(reason) => panic!("{reason}"),
                GeometryFamilyCompileAdvance::Cancelled => panic!("geometry was not cancelled"),
            }
        };

        let operations = [
            (PieceKind::S, 0, 3, 0),
            (PieceKind::I, 1, 0, 0),
            (PieceKind::O, 0, 1, 0),
            (PieceKind::T, 0, 7, 0),
            (PieceKind::S, 0, 5, 0),
            (PieceKind::Z, 0, 4, 1),
            (PieceKind::Z, 0, 6, 1),
            (PieceKind::L, 3, 8, 0),
            (PieceKind::T, 1, 3, 0),
            (PieceKind::O, 0, 1, 0),
        ];
        let mut board = 0_u64;
        let mut remaining = catalog.required_cells();
        let mut packed_counts = 0_u32;
        let mut deleted_rows = 0_u16;
        let mut available = Vec::new();
        let mut reachability = ReachabilityWorkspace::default();
        let mut expected_states = Vec::new();
        let mut expected_row_ids = Vec::new();
        let mut placement_rows = 0_u128;
        reachability.configure(catalog.skeleton_count());
        reachability.configure_kick_profile(conditions[0].problem().kick_profile().profile_id());

        for (depth, (piece, rotation, x, y)) in operations.into_iter().enumerate() {
            compiled
                .completion_oracle
                .collect_available_rows(
                    remaining,
                    packed_counts,
                    deleted_rows != 0,
                    &catalog,
                    &mut available,
                    &control,
                )
                .expect("completion rows");
            let (row_id, realization) = (0..catalog.skeleton_count() as u32)
                .filter(|row_id| {
                    let row = catalog.skeleton(*row_id);
                    row.piece == piece && row.cells & remaining == row.cells
                })
                .find_map(|row_id| {
                    catalog
                        .instantiations(row_id, deleted_rows)
                        .find(|realization| {
                            realization.rotation.quarter_turns() == rotation
                                && realization.x == x
                                && realization.lock_y == y
                        })
                        .map(|realization| (row_id, realization))
                })
                .unwrap_or_else(|| panic!("operation {depth} has no inverse-lock row"));
            let expected_next_counts =
                add_packed_piece(packed_counts, super::super::piece_index(piece))
                    .expect("expected packed count");
            assert!(
                expected_execution_prefixes
                    .binary_search(&expected_next_counts)
                    .is_ok(),
                "supply prefix rejected operation {depth}: {piece:?}, counts={expected_next_counts:07x}"
            );
            assert!(
                available.contains(&row_id),
                "completion oracle lost operation {depth}: {piece:?}, row={row_id}, \
                 remaining={remaining:010x}, counts={packed_counts:07x}, \
                 deleted={deleted_rows:04x}"
            );
            assert_eq!(
                board & realization.lock_mask,
                0,
                "operation {depth} overlaps"
            );
            assert!(
                reachability.lock_reachable_instantiated(&catalog, board, piece, realization,),
                "operation {depth} is not reachable"
            );
            let row = catalog.skeleton(row_id);
            expected_row_ids.push(row_id);
            remaining ^= row.cells;
            packed_counts = add_packed_piece(packed_counts, super::super::piece_index(piece))
                .expect("packed count");
            let (next_board, cleared_current, _) = place_and_clear(
                catalog.width(),
                catalog.height(),
                board | realization.lock_mask,
            );
            board = next_board;
            deleted_rows = merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)
                .expect("deleted row merge");
            if depth < 4 {
                placement_rows = insert_setup_placement_row(placement_rows, depth as u8, row_id)
                    .expect("placement row identity");
            }
            expected_states.push((piece, board, deleted_rows, placement_rows));
        }

        assert_eq!(remaining, 0);
        assert_eq!(board, 0);
        assert_eq!(expected_row_ids.len(), operations.len());

        let mut builder =
            PartialBuildGraphBuilder::new(compiled, &catalog, conditions[0].problem(), 4)
                .expect("partial graph builder");
        let graph = loop {
            match builder
                .advance(&catalog, 65_536, &control)
                .expect("partial graph advance")
            {
                PartialBuildAdvance::Pending => {}
                PartialBuildAdvance::Complete { graph, .. } => break graph,
                PartialBuildAdvance::PrefixComplete { .. } => {
                    panic!("complete graph builder returned a setup prefix")
                }
                PartialBuildAdvance::Cancelled => panic!("partial graph was not cancelled"),
            }
        };
        let mut frontier = vec![graph.root];
        for (depth, (piece, expected_board, expected_deleted, expected_rows)) in
            expected_states.into_iter().enumerate()
        {
            let mut next = Vec::new();
            for source in frontier {
                let node = graph.nodes[source as usize];
                let start = node.edge_start as usize;
                let end = start + node.edge_count as usize;
                for edge in &graph.edges[start..end] {
                    let target = graph.nodes[edge.to as usize];
                    if edge.piece != piece
                        || target.board != expected_board
                        || target.deleted_rows != expected_deleted
                        || (depth < 4
                            && graph.placement_rows_for_node(target) != Some(expected_rows))
                    {
                        continue;
                    }
                    next.push(edge.to);
                }
            }
            next.sort_unstable();
            next.dedup();
            assert!(
                !next.is_empty(),
                "partial graph lost known completion at operation {depth}: {piece:?}, \
                 board={expected_board:010x}, deleted={expected_deleted:04x}"
            );
            frontier = next;
        }
        assert!(frontier
            .iter()
            .any(|node| graph.nodes[*node as usize].accepting()));

        let z_detail = SetupPathDetail::from_setup_id(
            "setup-000000c060-0000-00000000000000000000000000015d",
            "hold-empty",
        )
        .expect("Z setup detail");
        let s_detail = SetupPathDetail::from_setup_id(
            "setup-000000c018-0000-000000000000000000000000000108",
            "hold-empty",
        )
        .expect("S setup detail");
        let z_shape = graph
            .shape_index_for_detail(&z_detail)
            .expect("Z setup shape");
        let s_shape = graph
            .shape_index_for_detail(&s_detail)
            .expect("S setup shape");
        let z_only_on_s = [0, 1, 2, 3, 4, 4, 6, 2, 1];
        let z_original = mirror_piece_language(&z_only_on_s);
        let s_only = [0, 1, 2, 4, 3, 6, 5, 6, 5];
        let s_only_on_z = mirror_piece_language(&s_only);
        let z_original_counts = piece_language_frontier_counts(&graph, z_shape, &z_original);
        let z_only_s_counts = piece_language_frontier_counts(&graph, s_shape, &z_only_on_s);
        let s_only_counts = piece_language_frontier_counts(&graph, s_shape, &s_only);
        let s_only_z_counts = piece_language_frontier_counts(&graph, z_shape, &s_only_on_z);
        assert!(
            z_only_s_counts.last().copied().unwrap_or(0) > 0
                && s_only_z_counts.last().copied().unwrap_or(0) > 0,
            "mirrored one-piece setup lost a valid piece language: \
             z_original={z_original_counts:?}, mirrored_on_s={z_only_s_counts:?}, \
             s_original={s_only_counts:?}, mirrored_on_z={s_only_z_counts:?}"
        );
    }
}
