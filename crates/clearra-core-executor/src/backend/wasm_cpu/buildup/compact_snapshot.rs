//! Frozen, selected-candidate BuildUp evidence for normalized compact PCs.
//! The query constructs the existing subset graph once; it never enumerates
//! other candidates or runs the queue/hold product.

use std::fmt;

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::normalized_tiling_solution::PiecePlacementMask,
};
use clearra_finesse::{
    CostedGeometryEdge, CostedGeometryLanguage, FinesseBoard, FinesseTarget, GeometryActionKey,
    GeometryLanguageNode, GeometryNodeId,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_problem::{goal::SearchGoal, SearchProblem};
use clearra_rules::{kicks::KickTableProfileId, spawn::SpawnProfile};

use super::{
    annotate_frozen_finesse_target_groups, checked_candidate_verification_peak_upper_bound,
    compact_target_board, push_frozen_finesse_target, BuildCompletion, BuildOrderGraph,
    BuildReachabilityMode, BuildUpWorkspace, CandidateMovementProofs, CandidateProjection,
    FrozenFinesseTargetGroups, GeometryCandidate, GeometryCatalog, GroupedFinesseTarget,
    WasmExactSearchError,
};
use crate::backend::wasm_cpu::{ensure_canonical_kick_profile, kick_profiles, MAX_BOARD64_PIECES};
use crate::row_frame::CompactedRowFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactBuildUpTransitionMode {
    /// Collision, grounding, row clears and completion, without movement.
    GroundedGeometry,
    /// Grounded geometry additionally verified under the request's kick rules.
    Reachable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactBuildUpNode {
    pub board_mask: u64,
    /// Indices into the snapshot's canonical operation array.
    pub placed_operations: u16,
    /// Deleted row bits in the original candidate frame, not current y.
    pub deleted_original_rows: u16,
    pub accepting: bool,
    first_edge: u32,
    edge_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactBuildUpEdge {
    pub target_mask: u64,
    pub child_index: u32,
    pub operation_index: u8,
    /// Rows cleared by this lock in its compacted source board.
    pub cleared_physical_rows: u16,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
}

#[derive(Debug)]
pub struct CompactBuildUpSnapshot {
    layout: Board64Layout,
    spawn: SpawnProfile,
    profile: KickTableProfileId,
    mode: CompactBuildUpTransitionMode,
    operations: Vec<PiecePlacementMask>,
    nodes: Vec<CompactBuildUpNode>,
    edges: Vec<CompactBuildUpEdge>,
}

impl CompactBuildUpSnapshot {
    pub fn mode(&self) -> CompactBuildUpTransitionMode {
        self.mode
    }

    pub fn layout(&self) -> Board64Layout {
        self.layout
    }

    pub fn operations(&self) -> &[PiecePlacementMask] {
        &self.operations
    }

    pub fn nodes(&self) -> &[CompactBuildUpNode] {
        &self.nodes
    }

    pub fn edges_from(&self, node_index: usize) -> Option<&[CompactBuildUpEdge]> {
        let node = self.nodes.get(node_index)?;
        let start = node.first_edge as usize;
        Some(&self.edges[start..start + node.edge_count as usize])
    }

    /// Root is always the first node, even when nothing can reach completion.
    pub fn root_index(&self) -> usize {
        0
    }

    /// Annotate a frozen geometry-only snapshot without rebuilding it. This
    /// carries minimum classic input costs, not spin, B2B or queue evidence.
    /// The caller owns any retained cost graph in addition to the snapshot.
    pub fn classic_finesse(
        &self,
        control: &ExecutionControl,
    ) -> Result<CostedGeometryLanguage, CompactBuildUpSnapshotError> {
        check_cancelled(control)?;
        if self.mode != CompactBuildUpTransitionMode::GroundedGeometry {
            return Err(CompactBuildUpSnapshotError::GeometryOnlyRequired);
        }
        let kicks = kick_profiles::builtin_kick_profile(self.profile)
            .ok_or(CompactBuildUpSnapshotError::UnsupportedProblem)?;
        let mut groups = FrozenFinesseTargetGroups::new();
        let mut boards = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            check_cancelled(control)?;
            let board = FinesseBoard::new(self.layout, node.board_mask)
                .map_err(|_| CompactBuildUpSnapshotError::InvalidGraph)?;
            boards.push(board);
            for (offset, edge) in self
                .edges_from(index)
                .expect("owned node")
                .iter()
                .enumerate()
            {
                let target =
                    FinesseTarget::new(edge.rotation, i16::from(edge.x), i16::from(edge.y));
                push_frozen_finesse_target(
                    &mut groups,
                    board,
                    edge.piece,
                    GroupedFinesseTarget::new(
                        node.first_edge as usize + offset,
                        target,
                        None,
                        true,
                    ),
                );
            }
        }
        let mut edge_costs = vec![None; self.edges.len()];
        let mut edge_evidence = vec![None; self.edges.len()];
        annotate_frozen_finesse_target_groups(
            groups,
            self.spawn,
            kicks,
            &mut edge_costs,
            &mut edge_evidence,
            control,
            "compact_snapshot_finesse_movement_failed",
            "compact_snapshot_finesse_evidence_failed",
        )?;
        let mut live = vec![false; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate().rev() {
            check_cancelled(control)?;
            let start = node.first_edge as usize;
            live[index] = node.accepting
                || self
                    .edges_from(index)
                    .expect("owned node")
                    .iter()
                    .enumerate()
                    .any(|(offset, edge)| {
                        edge_costs[start + offset].is_some() && live[edge.child_index as usize]
                    });
        }
        let nodes = self
            .nodes
            .iter()
            .zip(boards)
            .enumerate()
            .map(|(index, (node, board))| {
                check_cancelled(control)?;
                let edges = self.edges_from(index).expect("owned node");
                let mut costed = Vec::new();
                for (offset, edge) in edges.iter().enumerate() {
                    let transition = node.first_edge + offset as u32;
                    if let Some(cost) = edge_costs[transition as usize] {
                        if live[edge.child_index as usize] {
                            costed.push(
                                CostedGeometryEdge::new(
                                    edge.piece,
                                    GeometryNodeId::new(edge.child_index),
                                    cost,
                                    transition,
                                )
                                .with_action_key(
                                    GeometryActionKey::new(
                                        edge.piece,
                                        edge.rotation,
                                        i16::from(edge.x),
                                        i16::from(edge.y),
                                    ),
                                ),
                            );
                        }
                    }
                }
                Ok(GeometryLanguageNode::new(
                    node.placed_operations.count_ones() as u16,
                    node.accepting,
                    costed,
                )
                .with_source_board(board))
            })
            .collect::<Result<Vec<_>, CompactBuildUpSnapshotError>>()?;
        CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes)
            .map_err(|_| CompactBuildUpSnapshotError::InvalidGraph)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactBuildUpSnapshotError {
    UnsupportedProblem,
    InvalidCandidate,
    InvalidGraph,
    GeometryOnlyRequired,
    ResourceLimit,
    Cancelled,
    Engine(&'static str),
}

impl fmt::Display for CompactBuildUpSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(reason) => formatter.write_str(reason),
            error => write!(formatter, "{error:?}"),
        }
    }
}

impl std::error::Error for CompactBuildUpSnapshotError {}

impl From<WasmExactSearchError> for CompactBuildUpSnapshotError {
    fn from(error: WasmExactSearchError) -> Self {
        match error {
            WasmExactSearchError::Cancelled => Self::Cancelled,
            WasmExactSearchError::ResourceAdmission(_) => Self::ResourceLimit,
            WasmExactSearchError::InvalidProblem(reason) => Self::Engine(reason),
        }
    }
}

/// Reuses the compact ILC catalog and BuildUp scratch across candidate queries
/// for exactly one compiled request. Previously returned snapshots are owned,
/// immutable, and independent of subsequent queries.
pub struct RustCompactBuildUpWorkspace<'a> {
    problem: &'a SearchProblem,
    layout: Board64Layout,
    catalog: GeometryCatalog,
    workspace: BuildUpWorkspace,
}

impl<'a> RustCompactBuildUpWorkspace<'a> {
    pub fn new(problem: &'a SearchProblem) -> Result<Self, CompactBuildUpSnapshotError> {
        if problem.initial_board().width() != 10
            || !(1..=6).contains(&problem.compact_pc_geometry_height())
            || problem.visible_height() > 6
            || !matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
            || problem.spawn_profile() != SpawnProfile::STANDARD_10
        {
            return Err(CompactBuildUpSnapshotError::UnsupportedProblem);
        }
        // A snapshot may contain pieces outside the request's supply and
        // retains concrete pose/finesse evidence under its builtin profile.
        // PC's supply-restricted or Boolean ordered-table authority cannot
        // certify this separate consumer.
        ensure_canonical_kick_profile(problem)?;
        let size = BoardSize::new(10, problem.compact_pc_geometry_height())
            .map_err(|_| CompactBuildUpSnapshotError::UnsupportedProblem)?;
        let layout = Board64Layout::new(size)
            .map_err(|_| CompactBuildUpSnapshotError::UnsupportedProblem)?;
        let catalog_peak = GeometryCatalog::checked_compile_peak_upper_bound(problem)
            .ok_or(CompactBuildUpSnapshotError::ResourceLimit)?;
        check_memory_limit(problem, catalog_peak)?;
        Ok(Self {
            problem,
            layout,
            catalog: GeometryCatalog::compile(problem)?,
            workspace: BuildUpWorkspace::default(),
        })
    }

    /// Each input mask describes one tetromino in the original PC frame. The
    /// masks must cover precisely the request's required cells. Partial,
    /// overlapping and ambiguous-origin inputs do not create negative proof.
    pub fn prepare(
        &mut self,
        placements: &[PiecePlacementMask],
        mode: CompactBuildUpTransitionMode,
        control: &ExecutionControl,
    ) -> Result<CompactBuildUpSnapshot, CompactBuildUpSnapshotError> {
        check_cancelled(control)?;
        if placements.is_empty() || placements.len() > MAX_BOARD64_PIECES {
            return Err(CompactBuildUpSnapshotError::InvalidCandidate);
        }
        let subset_count = 1_usize << placements.len();
        let node_budget = self.problem.budget().max_nodes();
        if node_budget != 0 && subset_count > node_budget {
            return Err(CompactBuildUpSnapshotError::ResourceLimit);
        }
        let mut rows = [0; MAX_BOARD64_PIECES];
        let mut used = 0_u64;
        for (index, placement) in placements.iter().copied().enumerate() {
            let mask = placement.cells_mask();
            if used & mask != 0 {
                return Err(CompactBuildUpSnapshotError::InvalidCandidate);
            }
            rows[index] = self
                .catalog
                .skeleton_id(placement.piece(), mask)
                .ok_or(CompactBuildUpSnapshotError::InvalidCandidate)?;
            used |= mask;
        }
        if used != self.catalog.required_cells() {
            return Err(CompactBuildUpSnapshotError::InvalidCandidate);
        }
        let candidate = GeometryCandidate::from_rows(&self.catalog, 0, &rows[..placements.len()])
            .ok_or(CompactBuildUpSnapshotError::InvalidCandidate)?;
        if self.problem.backend_request().max_memory_mib().is_some() {
            let catalog = GeometryCatalog::checked_compile_peak_upper_bound(self.problem)
                .ok_or(CompactBuildUpSnapshotError::ResourceLimit)?;
            let search = checked_candidate_verification_peak_upper_bound(
                self.problem,
                &self.catalog,
                &candidate,
                false,
            )
            .ok_or(CompactBuildUpSnapshotError::ResourceLimit)?;
            let retained = self.workspace.retained_bytes() as u128;
            // One physical pose per rotation can realize a mask in a fixed
            // subset/frame. Charge its output in addition to solver scratch.
            let subsets = (1_u128) << placements.len();
            let output = subsets
                * ((core::mem::size_of::<CompactBuildUpNode>()
                    + core::mem::size_of::<(usize, usize)>()) as u128
                    + 4 * placements.len() as u128
                        * core::mem::size_of::<CompactBuildUpEdge>() as u128);
            check_memory_limit(self.problem, catalog + retained + search + output)?;
        }
        let completion = BuildCompletion::ClearToEmpty;
        if mode == CompactBuildUpTransitionMode::Reachable {
            self.workspace.reachability.configure(placements.len());
            self.workspace
                .reachability
                .configure_kick_profile(self.problem.kick_profile().profile_id(), false);
        }
        let mut projection = CandidateProjection::compile(
            &self.catalog,
            &candidate,
            &mut self.workspace,
            completion,
        )?;
        let graph = BuildOrderGraph::build(
            self.problem,
            &self.catalog,
            &candidate,
            &mut projection,
            &mut self.workspace,
            false,
            completion,
            None,
            match mode {
                CompactBuildUpTransitionMode::GroundedGeometry => {
                    BuildReachabilityMode::GroundedGeometryOnly
                }
                CompactBuildUpTransitionMode::Reachable => BuildReachabilityMode::Existing,
            },
            &CandidateMovementProofs::default(),
        );
        let result = match graph {
            Ok(graph) => {
                let result = self.freeze(mode, &candidate, &graph, &mut projection, control);
                self.workspace.recycle_graph(graph);
                result
            }
            Err(error) => Err(error.into()),
        };
        self.workspace.recycle_projection(projection);
        result
    }

    fn freeze(
        &self,
        mode: CompactBuildUpTransitionMode,
        candidate: &GeometryCandidate,
        graph: &BuildOrderGraph,
        projection: &mut CandidateProjection,
        control: &ExecutionControl,
    ) -> Result<CompactBuildUpSnapshot, CompactBuildUpSnapshotError> {
        check_cancelled(control)?;
        // A BuildOrderGraph is breadth-first and its internal indexes are
        // topological. Export only the root and accepting suffixes, remapped
        // once before writing edges.
        let mut remap = vec![(usize::MAX, usize::MAX); graph.nodes.len()];
        let mut node_count = 0_usize;
        for (index, node) in graph.nodes.iter().enumerate() {
            if node.live || index == graph.root as usize {
                remap[index].0 = node_count;
                node_count += 1;
            }
        }
        remap[graph.root as usize].1 = 0;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(node_count)
            .map_err(|_| CompactBuildUpSnapshotError::ResourceLimit)?;
        let mut edges = Vec::new();
        edges
            .try_reserve_exact(graph.edges.len())
            .map_err(|_| CompactBuildUpSnapshotError::ResourceLimit)?;
        for (index, node) in graph.nodes.iter().enumerate() {
            check_cancelled(control)?;
            let (destination, subset) = remap[index];
            if destination == usize::MAX {
                continue;
            }
            if subset == usize::MAX || subset.count_ones() != u32::from(node.depth) {
                return Err(CompactBuildUpSnapshotError::InvalidGraph);
            }
            let (board_mask, deleted_original_rows) = projection.state(subset);
            let frame = CompactedRowFrame::new(self.catalog.height(), deleted_original_rows)
                .ok_or(CompactBuildUpSnapshotError::InvalidGraph)?;
            let first_edge = edges.len() as u32;
            for edge in graph.edges(index) {
                let child = edge.to as usize;
                if !graph.nodes[child].live {
                    continue;
                }
                let bit = 1_usize << edge.operation_index;
                if subset & bit != 0 || child <= index {
                    return Err(CompactBuildUpSnapshotError::InvalidGraph);
                }
                let child_subset = subset | bit;
                let child_info = &mut remap[child];
                if child_info.1 != usize::MAX && child_info.1 != child_subset {
                    return Err(CompactBuildUpSnapshotError::InvalidGraph);
                }
                child_info.1 = child_subset;
                // The existing BuildUp pass already validated the edge's
                // pose and transition. Recover the export masks from its
                // original operation and row-frame facts, not a second lock.
                let row_id = candidate.row_ids()[usize::from(edge.operation_index)];
                let target_mask = compact_target_board(
                    self.catalog.width(),
                    self.catalog.height(),
                    self.catalog.skeleton(row_id).cells,
                    deleted_original_rows,
                );
                let mut newly_deleted = projection.state(child_subset).1 & !deleted_original_rows;
                let mut cleared_physical_rows = 0_u16;
                while newly_deleted != 0 {
                    let original_row = newly_deleted.trailing_zeros() as u8;
                    newly_deleted &= newly_deleted - 1;
                    let physical_row = frame
                        .physical_row_for_original(original_row)
                        .ok_or(CompactBuildUpSnapshotError::InvalidGraph)?;
                    cleared_physical_rows |= 1_u16 << physical_row;
                }
                if cleared_physical_rows.count_ones() != u32::from(edge.cleared_lines) {
                    return Err(CompactBuildUpSnapshotError::InvalidGraph);
                }
                edges.push(CompactBuildUpEdge {
                    target_mask,
                    child_index: child_info.0 as u32,
                    operation_index: edge.operation_index,
                    cleared_physical_rows,
                    piece: edge.piece,
                    rotation: edge.rotation,
                    x: edge.x,
                    y: edge.y,
                });
            }
            nodes.push(CompactBuildUpNode {
                board_mask,
                placed_operations: subset as u16,
                deleted_original_rows,
                accepting: node.accepting(),
                first_edge,
                edge_count: edges.len() as u32 - first_edge,
            });
        }
        Ok(CompactBuildUpSnapshot {
            layout: self.layout,
            spawn: self.problem.spawn_profile(),
            profile: self.problem.kick_profile().profile_id(),
            mode,
            operations: candidate
                .row_ids()
                .iter()
                .map(|&row_id| {
                    let row = self.catalog.skeleton(row_id);
                    PiecePlacementMask::new(row.piece, row.cells)
                })
                .collect(),
            nodes,
            edges,
        })
    }
}

fn check_memory_limit(
    problem: &SearchProblem,
    required: u128,
) -> Result<(), CompactBuildUpSnapshotError> {
    if problem
        .backend_request()
        .max_memory_mib()
        .is_some_and(|mib| required > u128::from(mib) * 1024 * 1024)
    {
        return Err(CompactBuildUpSnapshotError::ResourceLimit);
    }
    Ok(())
}

fn check_cancelled(control: &ExecutionControl) -> Result<(), CompactBuildUpSnapshotError> {
    if control.is_cancelled() {
        return Err(CompactBuildUpSnapshotError::Cancelled);
    }
    Ok(())
}

#[cfg(test)]
#[path = "compact_snapshot_tests.rs"]
mod tests;
