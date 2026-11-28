// SRP rationale: this module has one behavior-level change reason: constructing and annotating exact extended-board BuildUp order languages.

use std::{fmt::Write as _, sync::Arc};

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_finesse::{FinesseBoard, FinesseTarget, GeometryActionKey};
use clearra_postprocess::score_batch::BackToBackEdgePolicy;
use clearra_problem::SearchProblem;
use clearra_replay::{ScoringExecutionEdge, ScoringExecutionNode, SpinCoverageExecutionGraph};

use crate::{
    performance::{ExecutorSearchStage, SearchStageSpan},
    resource::{ExecutionMemoryBound, ExecutionMemoryOwner, SharedExecutionNodeBudget},
};

use super::{
    buildup::{
        annotate_frozen_finesse_target_groups, finesse_b2b_policy,
        finesse_scoring_edge_requirement, finesse_terminal_label, push_frozen_finesse_target,
        BuildEdge, BuildOrderGraph, BuildOrderNodeSpec, FrozenFinesseTargetGroups,
        GroupedFinesseTarget, PreparedFinesseEdge, PreparedFinesseLanguage, PreparedFinesseNode,
    },
    candidate_relation::{
        traverse_candidate_relation, CandidateRelationCatalog, CandidateRelationMode,
        CandidateRelationOwner, RelationInstantiation, RelationMovement, RelationSubset,
        RelationSubsetStart, RelationTraversalOutcome,
    },
    candidate_row_projection::{CandidateCells, CandidateRowProjection},
    extended_board::ExtendedBoard,
    extended_geometry::ExtendedGeometryCandidate,
    extended_inverse_catalog::{ExtendedInstantiation, ExtendedInverseCatalog},
    extended_reachability::{
        ExtendedHarddropReachability, ExtendedReachabilityWorkspace, ExtendedReachableLocks,
    },
    mix_digest, piece_index,
    subset_node_index::SubsetNodeIndex,
    WasmExactSearchError,
};

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
        let mut key = String::with_capacity(self.canonical_key_len(height));
        let words = initial_board.words();
        write!(
            &mut key,
            "ctk2|height={height}|initial={:016x}{:016x}{:016x}{:016x}|placements=",
            words[3], words[2], words[1], words[0]
        )
        .expect("writing to String cannot fail");
        for (index, placement) in self.placements.iter().enumerate() {
            if index != 0 {
                key.push(',');
            }
            key.push(placement.piece.as_ascii());
            key.push(':');
            let words = placement.cells.words();
            write!(
                &mut key,
                "{:016x}{:016x}{:016x}{:016x}",
                words[3], words[2], words[1], words[0]
            )
            .expect("writing to String cannot fail");
        }
        debug_assert_eq!(key.len(), self.canonical_key_len(height));
        debug_assert!(
            clearra_core_domain::solution::ExtendedTilingSolutionKey::parse_canonical(&key).is_ok()
        );
        key
    }

    pub fn canonical_key_len(&self, height: u8) -> usize {
        let height_digits = if height >= 10 { 2 } else { 1 };
        "ctk2|height=".len()
            + height_digits
            + "|initial=".len()
            + 64
            + "|placements=".len()
            + self.placements.len() * 66
            + self.placements.len().saturating_sub(1)
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.placements.capacity() * core::mem::size_of::<ExtendedPlacementKey>()
    }
}

// Successful build-order state is transferred without an additional allocation.
#[allow(clippy::large_enum_variant)]
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
    subset_nodes: SubsetNodeIndex,
    subset_queue: Vec<u64>,
    edge_scratch: Vec<BuildEdge>,
    resource_boundary: Option<ExtendedBuildOrderResourceBoundary>,
}

struct ExtendedBuildOrderResourceBoundary {
    memory: ExecutionMemoryBound,
    external_private_bytes: u128,
    owner: Option<ExecutionMemoryOwner>,
    nodes: Option<Arc<SharedExecutionNodeBudget>>,
}

impl ExtendedBuildOrderWorkspace {
    pub fn new(
        width: u8,
        height: u8,
        kick_profile_id: clearra_rules::kicks::KickTableProfileId,
    ) -> Self {
        Self {
            reachability: ExtendedReachabilityWorkspace::new(width, height, kick_profile_id),
            subset_nodes: SubsetNodeIndex::new(),
            subset_queue: Vec::new(),
            edge_scratch: Vec::new(),
            resource_boundary: None,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.reachability.retained_bytes()
            + self.subset_nodes.retained_bytes()
            + self.subset_queue.capacity() * core::mem::size_of::<u64>()
            + self.edge_scratch.capacity() * core::mem::size_of::<BuildEdge>()
    }

    pub(super) fn set_resource_boundary(
        &mut self,
        memory: ExecutionMemoryBound,
        external_private_bytes: u128,
        owner: Option<ExecutionMemoryOwner>,
        nodes: Option<Arc<SharedExecutionNodeBudget>>,
    ) {
        self.resource_boundary = Some(ExtendedBuildOrderResourceBoundary {
            memory,
            external_private_bytes,
            owner,
            nodes,
        });
    }

    fn ensure_graph_allocation(
        &self,
        transient_bytes: u128,
        future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        let Some(boundary) = self.resource_boundary.as_ref() else {
            return Ok(());
        };
        let observed = boundary
            .external_private_bytes
            .checked_add(self.retained_bytes() as u128)
            .and_then(|bytes| bytes.checked_add(transient_bytes))
            .ok_or_else(build_memory_projection_error)?;
        if let Some(owner) = boundary.owner.as_ref() {
            owner
                .ensure(
                    observed
                        .checked_add(future_bytes)
                        .ok_or_else(build_memory_projection_error)?,
                )
                .map_err(WasmExactSearchError::resource_admission)
        } else {
            boundary
                .memory
                .ensure(observed, future_bytes)
                .map_err(WasmExactSearchError::resource_admission)
        }
    }

    fn try_debit_node(&self) -> bool {
        self.resource_boundary
            .as_ref()
            .and_then(|boundary| boundary.nodes.as_ref())
            .is_none_or(|budget| budget.try_consume())
    }
}

struct CandidateProjection {
    operation_cells: Vec<ExtendedBoard>,
    operation_pieces: Vec<PieceKind>,
    row_projection: CandidateRowProjection,
    completed_target_rows: u32,
    initial_board: ExtendedBoard,
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
        for row_id in rows.iter().copied() {
            let row = catalog.skeleton(row_id);
            operation_cells.push(row.cells);
            operation_pieces.push(row.piece);
        }
        let logical_target = catalog.initial_board().union(catalog.required_cells());
        let row_projection = CandidateRowProjection::compile(
            catalog.width(),
            catalog.height(),
            logical_target,
            CandidateCells::Extended(&operation_cells),
        );
        let completed_target_rows = row_projection.completed_rows();
        let final_board = row_projection.compact(logical_target, completed_target_rows);
        let all_operations = if rows.len() == 64 {
            u64::MAX
        } else {
            (1_u64 << rows.len()) - 1
        };
        Ok(Self {
            operation_cells,
            operation_pieces,
            row_projection,
            completed_target_rows,
            initial_board: catalog.initial_board(),
            width: catalog.width(),
            height: catalog.height(),
            all_operations,
            final_board,
        })
    }

    fn state_for_subset(&self, subset: u64) -> (ExtendedBoard, u32) {
        self.row_projection.state(
            self.initial_board,
            CandidateCells::Extended(&self.operation_cells),
            subset,
        )
    }

    fn retained_bytes(&self) -> usize {
        self.operation_cells.capacity() * core::mem::size_of::<ExtendedBoard>()
            + self.operation_pieces.capacity() * core::mem::size_of::<PieceKind>()
    }
}

/// Full-height storage and evidence owner for the common candidate traversal.
/// Its graph vectors remain the same owners consumed by finesse/spin projection.
pub(super) struct ExtendedRelationAdapter<'a> {
    catalog: &'a ExtendedInverseCatalog,
    projection: &'a CandidateProjection,
    rows: &'a [u32],
    workspace: &'a mut ExtendedBuildOrderWorkspace,
    specs: &'a mut Vec<BuildOrderNodeSpec>,
    edges: &'a mut Vec<BuildEdge>,
    spin_node_spans: &'a mut Vec<(u32, u32)>,
    spin_edges: &'a mut Vec<ScoringExecutionEdge>,
    control: &'a ExecutionControl,
    remaining_node_budget: usize,
    searched_nodes: usize,
    geometric_only: bool,
    retain_scoring: bool,
    capture_lock_evidence: bool,
    reachability_prefilter: bool,
    plain_harddrop_proof: bool,
    lock_sets: [Option<ExtendedReachableLocks>; 7],
    harddrop: Option<ExtendedHarddropReachability>,
    child_projection: Option<(u64, ExtendedBoard, u32)>,
    spin_edge_start: u32,
}

impl ExtendedRelationAdapter<'_> {
    pub(super) fn begin_subset(
        &mut self,
        cursor: usize,
    ) -> Result<RelationSubsetStart, WasmExactSearchError> {
        if cursor == self.workspace.subset_queue.len() {
            return Ok(RelationSubsetStart::Exhausted);
        }
        if self.control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if (self.remaining_node_budget != 0 && self.searched_nodes >= self.remaining_node_budget)
            || !self.workspace.try_debit_node()
        {
            return Ok(RelationSubsetStart::Incomplete);
        }
        self.searched_nodes = self.searched_nodes.saturating_add(1);
        let subset = self.workspace.subset_queue[cursor];
        let node = self
            .workspace
            .subset_nodes
            .get(subset)
            .expect("queued extended subset has a node");
        self.specs[node as usize].edge_start = self.edges.len() as u32;
        self.spin_edge_start = self.spin_edges.len() as u32;
        let (board, deleted_rows) = self.projection.state_for_subset(subset);
        if subset == self.projection.all_operations {
            self.specs[node as usize].accepting = board == self.projection.final_board
                && deleted_rows == self.projection.completed_target_rows;
            return Ok(RelationSubsetStart::Terminal);
        }
        self.harddrop = self
            .plain_harddrop_proof
            .then(|| ExtendedHarddropReachability::new(self.projection.width, board));
        self.lock_sets = std::array::from_fn(|_| None);
        self.child_projection = None;
        self.reserve_children(subset)?;
        Ok(RelationSubsetStart::Expand(RelationSubset {
            subset,
            node_index: node,
            board,
            deleted_rows,
            permitted_operations: self.projection.all_operations & !subset,
        }))
    }

    fn reserve_children(&mut self, subset: u64) -> Result<(), WasmExactSearchError> {
        let remaining = self.rows.len() - subset.count_ones() as usize;
        let concrete_edges = self
            .rows
            .iter()
            .enumerate()
            .filter(|(operation, _)| subset & (1_u64 << operation) == 0)
            .try_fold(0_usize, |count, (_, row)| {
                count.checked_add(self.catalog.realization_count(*row))
            })
            .ok_or_else(build_memory_projection_error)?;
        let graph_live = checked_build_storage_bytes(
            self.projection,
            self.specs.capacity(),
            self.edges.capacity(),
            self.spin_node_spans.capacity(),
            self.spin_edges.capacity(),
        )?;
        let mut future = 0_u128;
        for (capacity, len, additional, item) in [
            (
                self.specs.capacity(),
                self.specs.len(),
                remaining,
                core::mem::size_of::<BuildOrderNodeSpec>(),
            ),
            (
                self.workspace.subset_queue.capacity(),
                self.workspace.subset_queue.len(),
                remaining,
                core::mem::size_of::<u64>(),
            ),
            (
                self.workspace.edge_scratch.capacity(),
                0,
                concrete_edges,
                core::mem::size_of::<BuildEdge>(),
            ),
            (
                self.edges.capacity(),
                self.edges.len(),
                concrete_edges,
                core::mem::size_of::<BuildEdge>(),
            ),
            (
                self.spin_node_spans.capacity(),
                self.spin_node_spans.len(),
                if self.retain_scoring { remaining } else { 0 },
                core::mem::size_of::<(u32, u32)>(),
            ),
            (
                self.spin_edges.capacity(),
                self.spin_edges.len(),
                if self.retain_scoring {
                    concrete_edges
                } else {
                    0
                },
                core::mem::size_of::<ScoringExecutionEdge>(),
            ),
        ] {
            future = future
                .checked_add(checked_vec_growth_bytes(capacity, len, additional, item)?)
                .ok_or_else(build_memory_projection_error)?;
        }
        let index_growth = self
            .workspace
            .subset_nodes
            .growth_for_children(subset, self.projection.all_operations & !subset);
        future = future
            .checked_add(
                self.workspace
                    .subset_nodes
                    .checked_reserve_future_bytes(index_growth)
                    .ok_or_else(build_memory_projection_error)?,
            )
            .ok_or_else(build_memory_projection_error)?;
        let mut pieces = [false; 7];
        if !self.geometric_only || self.retain_scoring {
            for (operation, &piece) in self.projection.operation_pieces.iter().enumerate() {
                if subset & (1_u64 << operation) != 0 || pieces[piece_index(piece)] {
                    continue;
                }
                pieces[piece_index(piece)] = true;
                future = future
                    .checked_add(
                        self.workspace
                            .reachability
                            .checked_query_future_bytes(piece, self.capture_lock_evidence)
                            .ok_or_else(build_memory_projection_error)?,
                    )
                    .ok_or_else(build_memory_projection_error)?;
            }
        }
        self.workspace.ensure_graph_allocation(graph_live, future)?;
        reserve_graph_vec(self.specs, remaining)?;
        reserve_graph_vec(&mut self.workspace.subset_queue, remaining)?;
        self.workspace.edge_scratch.clear();
        reserve_graph_vec(&mut self.workspace.edge_scratch, concrete_edges)?;
        reserve_graph_vec(self.edges, concrete_edges)?;
        self.workspace
            .subset_nodes
            .try_reserve(index_growth)
            .map_err(|_| build_memory_projection_error())?;
        if self.retain_scoring {
            reserve_graph_vec(self.spin_node_spans, remaining)?;
            reserve_graph_vec(self.spin_edges, concrete_edges)?;
        }
        Ok(())
    }

    pub(super) fn begin_operation(
        &mut self,
        _state: RelationSubset,
        _operation: usize,
        child: u64,
    ) -> Result<bool, WasmExactSearchError> {
        let (board, deleted) = self.projection.state_for_subset(child);
        self.child_projection = Some((child, board, deleted));
        Ok(true)
    }

    pub(super) fn projected_state(&mut self, child: u64) -> (ExtendedBoard, u32) {
        if let Some((key, board, deleted)) = self.child_projection {
            if key == child {
                return (board, deleted);
            }
        }
        self.projection.state_for_subset(child)
    }

    pub(super) fn confirm_transition(
        &mut self,
        child: u64,
        board: ExtendedBoard,
        deleted: u32,
    ) -> bool {
        self.projected_state(child) == (board, deleted)
    }

    pub(super) fn admit_child(&mut self, _child: u64) -> bool {
        true
    }

    pub(super) fn prepare_realizations(&mut self, state: RelationSubset, piece: PieceKind) {
        let slot = piece_index(piece);
        if !self.reachability_prefilter
            && (!self.geometric_only || self.retain_scoring)
            && self.lock_sets[slot].is_none()
        {
            self.lock_sets[slot] = Some(self.workspace.reachability.reachable_locks_with_scoring(
                state.board,
                piece,
                self.capture_lock_evidence,
            ));
        }
    }

    pub(super) fn known_reachable(
        &mut self,
        _state: RelationSubset,
        piece: PieceKind,
        pose: RelationInstantiation,
    ) -> Option<bool> {
        let slot = piece_index(piece);
        self.lock_sets[slot]
            .as_ref()
            .map(|locks| locks.contains(pose.rotation, pose.x, pose.lock_y))
    }

    pub(super) fn movement(
        &mut self,
        state: RelationSubset,
        piece: PieceKind,
        pose: RelationInstantiation,
        capture_scoring: bool,
    ) -> RelationMovement {
        let slot = piece_index(piece);
        let known = self.lock_sets[slot]
            .as_ref()
            .map(|locks| locks.contains(pose.rotation, pose.x, pose.lock_y));
        let harddrop = known.is_none()
            && self.harddrop.as_ref().is_some_and(|proof| {
                proof.contains(ExtendedInstantiation {
                    lock_mask: pose.lock_mask,
                    rotation: pose.rotation,
                    x: pose.x,
                    lock_y: pose.lock_y,
                })
            });
        if self.lock_sets[slot].is_none() && !harddrop {
            self.lock_sets[slot] = Some(self.workspace.reachability.reachable_locks_with_scoring(
                state.board,
                piece,
                self.capture_lock_evidence || capture_scoring,
            ));
        }
        let locks = self.lock_sets[slot].as_ref();
        let reachable = known.unwrap_or_else(|| {
            harddrop
                || locks.is_some_and(|locks| locks.contains(pose.rotation, pose.x, pose.lock_y))
        });
        let evidence = (capture_scoring && reachable).then(|| {
            locks
                .expect("scoring mode acquires exact reachability evidence")
                .scoring_evidence(pose.rotation, pose.x, pose.lock_y)
        });
        RelationMovement {
            reachable,
            evidence,
        }
    }

    pub(super) fn child_node_index(&mut self, child: u64) -> Result<u32, WasmExactSearchError> {
        self.workspace.subset_nodes.get(child).map_or_else(
            || {
                u32::try_from(self.specs.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_extended_build_order_node_index_overflow",
                    )
                })
            },
            Ok,
        )
    }

    pub(super) fn emit_edge(
        &mut self,
        child: u64,
        edge: BuildEdge,
        scoring: Option<ScoringExecutionEdge>,
        retain_build_edge: bool,
    ) -> Result<(), WasmExactSearchError> {
        if self.workspace.subset_nodes.get(child).is_none() {
            self.workspace.subset_nodes.insert_reserved(child, edge.to);
            self.workspace.subset_queue.push(child);
            self.specs.push(BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 0,
                depth: child.count_ones() as u8,
                accepting: false,
            });
            if self.retain_scoring {
                self.spin_node_spans.push((0, 0));
            }
        }
        if retain_build_edge {
            self.workspace.edge_scratch.push(edge);
        }
        if let Some(scoring) = scoring {
            self.spin_edges.push(scoring);
        }
        Ok(())
    }

    pub(super) fn finish_subset(
        &mut self,
        state: RelationSubset,
    ) -> Result<(), WasmExactSearchError> {
        self.workspace
            .edge_scratch
            .sort_unstable_by_key(BuildEdge::canonical_key);
        self.workspace.edge_scratch.dedup();
        self.edges.extend_from_slice(&self.workspace.edge_scratch);
        let node = state.node_index as usize;
        self.specs[node].edge_count = self.edges.len() as u32 - self.specs[node].edge_start;
        if self.retain_scoring {
            let start = self.spin_edge_start as usize;
            self.spin_edges[start..].sort_unstable_by_key(spin_edge_key);
            let mut write = start;
            for read in start..self.spin_edges.len() {
                if write == start
                    || spin_edge_key(&self.spin_edges[write - 1])
                        != spin_edge_key(&self.spin_edges[read])
                {
                    self.spin_edges[write] = self.spin_edges[read];
                    write += 1;
                }
            }
            self.spin_edges.truncate(write);
            self.spin_node_spans[node] = (
                self.spin_edge_start,
                self.spin_edges.len() as u32 - self.spin_edge_start,
            );
        }
        Ok(())
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
    build_extended_order_graph_mode::<true>(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        None,
        false,
        None,
        control,
    )
}

// Execution constraints must be applied before supply coverage, minimum-source
// proof, and replay authority are minted. Passing a finesse problem here would turn
// this into geometry-only enumeration and skip ordinary exact reachability.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_extended_order_graph_with_constraints(
    problem: &SearchProblem,
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    control: &ExecutionControl,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    build_extended_order_graph_mode::<true>(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        None,
        false,
        finesse_b2b_policy(problem),
        control,
    )
}

// The builder borrows distinct admission and finesse workspaces on its hot path.
#[allow(clippy::too_many_arguments)]
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
    build_extended_order_graph_mode::<true>(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        Some(problem),
        spin_coverage_requested,
        None,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_extended_order_graph_mode<const EARLY_B2B_PRUNING: bool>(
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    finesse_problem: Option<&SearchProblem>,
    finesse_spin_coverage_requested: bool,
    b2b_policy: Option<BackToBackEdgePolicy>,
    control: &ExecutionControl,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    build_extended_order_graph_mode_with_options::<EARLY_B2B_PRUNING>(
        catalog,
        candidate,
        workspace,
        remaining_node_budget,
        spin_candidate,
        finesse_problem,
        finesse_spin_coverage_requested,
        b2b_policy,
        control,
        true,
        true,
    )
}

// The two options are explicit only for exact oracle comparisons in tests;
// production callers always enable both proven sufficient/necessary checks.
#[allow(clippy::too_many_arguments)]
fn build_extended_order_graph_mode_with_options<const EARLY_B2B_PRUNING: bool>(
    catalog: &ExtendedInverseCatalog,
    candidate: &ExtendedGeometryCandidate,
    workspace: &mut ExtendedBuildOrderWorkspace,
    remaining_node_budget: usize,
    spin_candidate: Option<(u64, String)>,
    finesse_problem: Option<&SearchProblem>,
    finesse_spin_coverage_requested: bool,
    b2b_policy: Option<BackToBackEdgePolicy>,
    control: &ExecutionControl,
    reachability_prefilter: bool,
    harddrop_proof: bool,
) -> Result<ExtendedBuildOrderResult, WasmExactSearchError> {
    let geometry_only = finesse_problem.is_some();
    let plain_harddrop_proof =
        harddrop_proof && !geometry_only && spin_candidate.is_none() && b2b_policy.is_none();
    let mut finesse_geometry_span =
        geometry_only.then(|| SearchStageSpan::begin(ExecutorSearchStage::FinesseGeometry));
    let rows = candidate.row_ids();
    workspace.ensure_graph_allocation(
        0,
        (rows.len() as u128)
            .checked_mul(
                (core::mem::size_of::<ExtendedBoard>() + core::mem::size_of::<PieceKind>()) as u128,
            )
            .ok_or_else(build_memory_projection_error)?,
    )?;
    let projection = CandidateProjection::compile(catalog, candidate)?;
    workspace.subset_nodes.clear();
    let root_growth = workspace.subset_nodes.growth_for_key(0);
    let initial_future = workspace
        .subset_nodes
        .checked_reserve_future_bytes(root_growth)
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<BuildOrderNodeSpec>() as u128))
        .and_then(|bytes| bytes.checked_add(128))
        .ok_or_else(build_memory_projection_error)?;
    workspace.ensure_graph_allocation(projection.retained_bytes() as u128, initial_future)?;
    workspace
        .subset_nodes
        .try_reserve(root_growth)
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_extended_build_order_index_storage_unavailable",
            )
        })?;
    workspace.subset_nodes.insert_reserved(0, 0);
    workspace.subset_queue.clear();
    reserve_graph_vec(&mut workspace.subset_queue, 1)?;
    workspace.subset_queue.push(0);

    let mut specs = Vec::new();
    reserve_graph_vec(&mut specs, 1)?;
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
    let reachability_before = workspace.reachability.visited_state_count();
    let mut adapter = ExtendedRelationAdapter {
        catalog,
        projection: &projection,
        rows,
        workspace,
        specs: &mut specs,
        edges: &mut edges,
        spin_node_spans: &mut spin_node_spans,
        spin_edges: &mut spin_edges,
        control,
        remaining_node_budget,
        searched_nodes: 0,
        geometric_only: geometry_only,
        retain_scoring: spin_candidate.is_some(),
        capture_lock_evidence: spin_candidate.is_some() || b2b_policy.is_some(),
        reachability_prefilter,
        plain_harddrop_proof,
        lock_sets: std::array::from_fn(|_| None),
        harddrop: None,
        child_projection: None,
        spin_edge_start: 0,
    };
    let outcome = traverse_candidate_relation(
        &mut CandidateRelationOwner::Extended(&mut adapter),
        CandidateRelationCatalog::Extended(catalog),
        rows,
        CandidateRelationMode {
            geometric_only: geometry_only,
            grounded: !geometry_only,
            visit_all_poses: geometry_only || spin_candidate.is_some(),
            retain_all_build_poses: geometry_only,
            capture_scoring: spin_candidate.is_some(),
            early_b2b: EARLY_B2B_PRUNING,
            b2b_policy,
        },
    )?;
    let searched_nodes = adapter.searched_nodes;
    drop(adapter);
    if outcome == RelationTraversalOutcome::Incomplete {
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

    let reachability_states = workspace
        .reachability
        .visited_state_count()
        .saturating_sub(reachability_before);
    if let Some(span) = finesse_geometry_span.take() {
        span.finish(searched_nodes as u64);
    }
    let terminal_future = (specs.len() as u128)
        .checked_mul(
            8 * (core::mem::size_of::<super::buildup::BuildNode>()
                + core::mem::size_of::<PreparedFinesseNode>()
                + core::mem::size_of::<ScoringExecutionNode>()) as u128,
        )
        .and_then(|bytes| {
            bytes.checked_add(
                (edges.len() as u128)
                    * 4
                    * (core::mem::size_of::<BuildEdge>()
                        + core::mem::size_of::<PreparedFinesseEdge>())
                        as u128,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (spin_edges.len() as u128)
                    * 2
                    * core::mem::size_of::<ScoringExecutionEdge>() as u128,
            )
        })
        .ok_or_else(build_memory_projection_error)?;
    workspace.ensure_graph_allocation(
        checked_build_storage_bytes(
            &projection,
            specs.capacity(),
            edges.capacity(),
            spin_node_spans.capacity(),
            spin_edges.capacity(),
        )?,
        terminal_future,
    )?;
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

fn build_memory_projection_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_build_order_memory_projection_unavailable")
}

fn checked_vec_growth_bytes(
    capacity: usize,
    len: usize,
    additional: usize,
    item: usize,
) -> Result<u128, WasmExactSearchError> {
    let needed = len
        .checked_add(additional)
        .ok_or_else(build_memory_projection_error)?;
    let future = if needed > capacity {
        needed
            .checked_next_power_of_two()
            .ok_or_else(build_memory_projection_error)?
    } else {
        capacity
    };
    // A reallocation may keep old and new backing alive while copying. The
    // observed owner already counts the old capacity, so admit the complete
    // replacement allocation, not merely its retained-byte delta.
    (if future > capacity { future as u128 } else { 0 })
        .checked_mul(item as u128)
        .ok_or_else(build_memory_projection_error)
}

fn reserve_graph_vec<T>(
    values: &mut Vec<T>,
    additional: usize,
) -> Result<(), WasmExactSearchError> {
    let needed = values
        .len()
        .checked_add(additional)
        .ok_or_else(build_memory_projection_error)?;
    if needed > values.capacity() {
        let capacity = needed
            .checked_next_power_of_two()
            .ok_or_else(build_memory_projection_error)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| build_memory_projection_error())?;
    }
    Ok(())
}

fn checked_build_storage_bytes(
    projection: &CandidateProjection,
    specs: usize,
    edges: usize,
    spans: usize,
    scoring: usize,
) -> Result<u128, WasmExactSearchError> {
    // Capacity, not length: recently-cleared or mostly-empty Vecs still own
    // their backing allocation and must not disappear from admission.
    let mut bytes = projection.retained_bytes() as u128;
    for (capacity, item) in [
        (specs, core::mem::size_of::<BuildOrderNodeSpec>()),
        (edges, core::mem::size_of::<BuildEdge>()),
        (spans, core::mem::size_of::<(u32, u32)>()),
        (scoring, core::mem::size_of::<ScoringExecutionEdge>()),
    ] {
        bytes = bytes
            .checked_add(
                (capacity as u128)
                    .checked_mul(item as u128)
                    .ok_or_else(build_memory_projection_error)?,
            )
            .ok_or_else(build_memory_projection_error)?;
    }
    Ok(bytes)
}

// Pruning operates on independent caller-owned graph and policy surfaces.
#[allow(clippy::too_many_arguments)]
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
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow,
    };
    use clearra_problem::{BuildProbabilityField, ProblemCompiler, SearchProblem};
    use clearra_replay::{RotationRequest, ScoringLockEvidence};
    use clearra_rules::kicks::KickTableProfileId;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::super::extended_geometry::ExtendedGeometrySearch;
    use super::*;

    use super::super::build_probability::inverse_dependency_build_fixture;

    #[test]
    fn extended_reachability_prefilter_preserves_physical_finesse_and_scoring_graphs() {
        for cyclic in [false, true] {
            let (problem, field) = inverse_dependency_build_fixture(8, cyclic);
            let catalog = ExtendedInverseCatalog::compile(field).unwrap();
            let universe = problem.piece_source().materialized_universe().unwrap();
            let family = universe.packing_multiset_family_for_execution(
                field.target_piece_count(),
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                super::super::packing_hold_projection(&problem),
            );
            let geometry = ExtendedGeometrySearch::new(universe, &family, &catalog).unwrap();
            let placements = if cyclic {
                vec![
                    (PieceKind::T, (0b111_u64 << 12) | (1_u64 << 33)),
                    (PieceKind::Z, (0b11_u64 << 23) | (0b11_u64 << 4)),
                ]
            } else {
                vec![(PieceKind::I, 0x0f)]
            };
            let rows = placements
                .iter()
                .map(|&(piece, cells)| {
                    catalog
                        .skeletons()
                        .iter()
                        .position(|row| row.piece == piece && row.cells.words() == [cells, 0, 0, 0])
                        .expect("the exact inverse realization belongs to the catalog")
                        as u32
                })
                .collect::<Vec<_>>();
            let candidate = geometry.external_candidate(&catalog, &rows).unwrap();
            let key = ExtendedTilingKey::from_candidate(&catalog, &candidate)
                .canonical_key(catalog.initial_board(), catalog.height());

            for (geometry_only, scoring, b2b) in [
                (false, false, false),
                (false, true, false),
                (false, false, true),
                (false, true, true),
                (true, false, false),
                (true, true, false),
            ] {
                let run = |prefilter, harddrop| {
                    let mut workspace = ExtendedBuildOrderWorkspace::new(
                        catalog.width(),
                        catalog.height(),
                        problem.kick_profile().profile_id(),
                    );
                    let result = build_extended_order_graph_mode_with_options::<false>(
                        &catalog,
                        &candidate,
                        &mut workspace,
                        0,
                        scoring.then(|| (0, key.clone())),
                        geometry_only.then_some(&problem),
                        scoring,
                        b2b.then(|| BackToBackEdgePolicy::new(SpinProfileSelection::TSpins)),
                        &ExecutionControl::default(),
                        prefilter,
                        harddrop,
                    )
                    .unwrap();
                    let ExtendedBuildOrderResult::Complete {
                        graph,
                        finesse_language,
                        spin_graph,
                        searched_nodes,
                        reachability_states,
                        ..
                    } = result
                    else {
                        panic!("the bounded fixture has no node limit")
                    };
                    (
                        graph,
                        finesse_language,
                        spin_graph,
                        searched_nodes,
                        reachability_states,
                    )
                };
                // Pass the mode directly: concurrent tests never mutate the
                // process-wide local A/B environment.
                let eager = run(false, false);
                let lazy = run(true, false);
                let proof = run(true, true);
                for optimized in [&lazy, &proof] {
                    assert_eq!(eager.0.root, optimized.0.root);
                    assert_eq!(eager.0.nodes.len(), optimized.0.nodes.len());
                    for (index, (left, right)) in
                        eager.0.nodes.iter().zip(&optimized.0.nodes).enumerate()
                    {
                        assert_eq!(
                            (left.depth, left.live, left.accepting()),
                            (right.depth, right.live, right.accepting())
                        );
                        assert_eq!(eager.0.edges(index), optimized.0.edges(index));
                        assert_eq!(eager.0.piece_edges(index), optimized.0.piece_edges(index));
                    }
                    match (&eager.1, &optimized.1) {
                        (Some(left), Some(right)) => {
                            assert_eq!(left.root, right.root);
                            let nodes = |language: &PreparedFinesseLanguage| {
                                language
                                    .nodes
                                    .iter()
                                    .map(|node| {
                                        (
                                            node.edge_start,
                                            node.edge_count,
                                            node.depth,
                                            node.accepting,
                                            node.source_board,
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };
                            let edges = |language: &PreparedFinesseLanguage| {
                                language
                                    .edges
                                    .iter()
                                    .map(|edge| {
                                        (
                                            edge.child,
                                            edge.piece,
                                            edge.cost,
                                            edge.transition_order,
                                            edge.action_key,
                                            edge.terminal_evidence,
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };
                            assert_eq!(nodes(left), nodes(right));
                            assert_eq!(edges(left), edges(right));
                        }
                        (None, None) => {}
                        _ => panic!("finesse ownership must not depend on prefilter mode"),
                    }
                    assert_eq!(eager.2, optimized.2);
                    assert_eq!(eager.3, optimized.3);
                    assert!(optimized.4 <= eager.4);
                }
                if !cyclic && !geometry_only && !scoring && !b2b {
                    assert!(lazy.4 > 0);
                    assert_eq!(proof.4, 0, "the open I is proved without a BFS");
                } else {
                    assert_eq!(proof.4, lazy.4, "evidence modes keep their complete BFS");
                }
                if cyclic && (!geometry_only || scoring) {
                    // Both operations require the other's cleared row. Their
                    // empty instantiation lists must not trigger any BFS.
                    assert!(eager.4 > 0);
                    assert_eq!(lazy.4, 0);
                    assert!(!lazy.0.nodes[lazy.0.root as usize].live);
                } else if !cyclic {
                    assert!(lazy.0.nodes[lazy.0.root as usize].live);
                }
            }
        }
    }

    #[test]
    fn extended_harddrop_proof_preserves_oracle_graph_coverage_counts_and_witnesses() {
        use clearra_pc_graph::request::{
            PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
        };
        use clearra_problem::BuildProbabilityField;
        use clearra_supply::{pattern_universe::PatternPiecePositionIndex, QueueObservationPolicy};

        use super::super::{
            buildup::representative_pattern_path, coverage_product::CoverageProductEvaluator,
            extended_geometry::ExtendedGeometrySearch, packing_hold_projection,
        };

        for (base, placements, needs_fallback) in [
            (
                0,
                [(PieceKind::I, 0x0f), (PieceKind::O, 0x0003_00c0)],
                false,
            ),
            // I clears the bottom row by a straight drop. O reaches beneath
            // the shelf through its side, before or after that clear. This
            // exercises proofs and successful full-search fallback together.
            (
                0x3f0 | (0xf0_u64 << 40),
                [(PieceKind::I, 0x0f), (PieceKind::O, 0x0300_c000)],
                true,
            ),
        ] {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(8, base),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(2),
            )
            .with_exact_pieces(Some(2))
            .with_supply_window_size(SupplyWindowSize::new(2))
            .with_allow_hold(true)
            .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle);
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let target = placements[0].1 | placements[1].1;
            let field = BuildProbabilityField::from_words_preserving_height(
                8,
                [base, 0, 0, 0],
                [target, 0, 0, 0],
            )
            .unwrap();
            let catalog = ExtendedInverseCatalog::compile(field).unwrap();
            let universe = problem.piece_source().materialized_universe().unwrap();
            let family = universe.packing_multiset_family_for_execution(
                field.target_piece_count(),
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                packing_hold_projection(&problem),
            );
            let geometry = ExtendedGeometrySearch::new(universe, &family, &catalog).unwrap();
            let rows = placements.map(|(piece, cells)| {
                catalog
                    .skeletons()
                    .iter()
                    .position(|row| row.piece == piece && row.cells.words() == [cells, 0, 0, 0])
                    .unwrap() as u32
            });
            let candidate = geometry.external_candidate(&catalog, &rows).unwrap();
            let run = |harddrop| {
                let mut workspace = ExtendedBuildOrderWorkspace::new(
                    catalog.width(),
                    catalog.height(),
                    problem.kick_profile().profile_id(),
                );
                let ExtendedBuildOrderResult::Complete {
                    graph,
                    searched_nodes,
                    reachability_states,
                    ..
                } = build_extended_order_graph_mode_with_options::<false>(
                    &catalog,
                    &candidate,
                    &mut workspace,
                    0,
                    None,
                    None,
                    false,
                    None,
                    &ExecutionControl::default(),
                    true,
                    harddrop,
                )
                .unwrap()
                else {
                    panic!("the finite two-piece graph must complete")
                };
                (graph, searched_nodes, reachability_states)
            };
            let baseline = run(false);
            let proof = run(true);
            assert_eq!(baseline.0.root, proof.0.root);
            assert_eq!(baseline.0.nodes.len(), proof.0.nodes.len());
            assert_eq!(baseline.1, proof.1);
            assert!(baseline.2 > 0);
            assert!(proof.2 < baseline.2);
            if needs_fallback {
                assert!(proof.2 > 0);
            } else {
                assert_eq!(proof.2, 0);
            }
            for (index, (left, right)) in baseline.0.nodes.iter().zip(&proof.0.nodes).enumerate() {
                assert_eq!(
                    (left.depth, left.live, left.accepting()),
                    (right.depth, right.live, right.accepting())
                );
                assert_eq!(baseline.0.edges(index), proof.0.edges(index));
                assert_eq!(baseline.0.piece_edges(index), proof.0.piece_edges(index));
            }
            let index = PatternPiecePositionIndex::compile(universe).unwrap();
            for count_paths in [false, true] {
                let evaluate = |graph: &BuildOrderGraph| {
                    CoverageProductEvaluator::default()
                        .evaluate(
                            graph,
                            &index,
                            problem.initial_hold(),
                            problem.supply().hold_enabled(),
                            problem.supply().projects_unplaced_lookahead(),
                            problem.supply().projects_standard_bag_lookahead(),
                            count_paths,
                            false,
                            &ExecutionControl::default(),
                        )
                        .unwrap()
                };
                let expected = evaluate(&baseline.0);
                let actual = evaluate(&proof.0);
                assert_eq!(actual.coverage_bits, expected.coverage_bits);
                assert_eq!(actual.coverage_bits.count_ones(), 2);
                assert_eq!(actual.path_count, expected.path_count);
                assert_eq!(actual.count_complete, expected.count_complete);
            }
            for ordinal in 0..universe.pattern_count() {
                let sequence = universe.sequence(
                    clearra_coverage::pattern::pattern_id::PatternId::new(ordinal),
                );
                assert_eq!(
                    representative_pattern_path(&problem, &proof.0, sequence.as_ref(), None),
                    representative_pattern_path(&problem, &baseline.0, sequence.as_ref(), None),
                    "original queue ordinal {ordinal}",
                );
            }
        }
    }

    #[test]
    fn extended_build_order_preallocation_refusal_keeps_private_vectors_unchanged() {
        let mut workspace =
            ExtendedBuildOrderWorkspace::new(10, 24, problem().kick_profile().profile_id());
        let before = workspace.retained_bytes();
        workspace.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(before as u128),
            0,
            None,
            None,
        );
        let error = workspace.ensure_graph_allocation(0, 1).unwrap_err();
        assert!(matches!(error, WasmExactSearchError::ResourceAdmission(_)));
        assert_eq!(workspace.retained_bytes(), before);
        assert!(workspace.subset_nodes.is_empty());
        assert!(workspace.subset_queue.is_empty());
        assert!(workspace.edge_scratch.is_empty());
    }

    #[test]
    fn extended_build_order_debit_shares_one_request_without_allocating_a_graph() {
        let budget = Arc::new(SharedExecutionNodeBudget::new(3));
        let mut left =
            ExtendedBuildOrderWorkspace::new(10, 24, problem().kick_profile().profile_id());
        let mut right =
            ExtendedBuildOrderWorkspace::new(10, 24, problem().kick_profile().profile_id());
        left.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(1_000_000),
            0,
            None,
            Some(Arc::clone(&budget)),
        );
        right.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(1_000_000),
            0,
            None,
            Some(Arc::clone(&budget)),
        );
        assert!(left.try_debit_node());
        assert!(right.try_debit_node());
        assert!(left.try_debit_node());
        assert!(!right.try_debit_node());
        assert_eq!(budget.consumed(), 3);
    }

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

    fn extended_b2b_candidate(
        base: u64,
        placements: &[(PieceKind, u64)],
    ) -> (ExtendedInverseCatalog, ExtendedGeometryCandidate) {
        let target = placements.iter().fold(0, |cells, (_, mask)| cells | mask);
        let field = BuildProbabilityField::from_words_preserving_height(
            7,
            [base, 0, 0, 0],
            [target, 0, 0, 0],
        )
        .unwrap();
        let catalog = ExtendedInverseCatalog::compile(field).unwrap();
        assert_eq!(catalog.height(), 7, "must exercise the extended builder");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(7, base),
            PcQueueInput::fixed_sequence(FixedSequence::new(
                placements.iter().map(|(piece, _)| *piece).collect(),
            )),
            PieceWindow::new(placements.len()),
        )
        .with_exact_pieces(Some(placements.len()))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let universe = problem.piece_source().materialized_universe().unwrap();
        let family = universe.packing_multiset_family_for_execution(
            placements.len(),
            problem.initial_hold(),
            false,
            super::super::packing_hold_projection(&problem),
        );
        // Supply/catalog validation only: no geometry-family enumeration is
        // needed for the explicitly specified one- or two-piece candidate.
        let geometry = ExtendedGeometrySearch::new(universe, &family, &catalog).unwrap();
        let row_ids = placements
            .iter()
            .map(|&(piece, cells)| {
                catalog
                    .skeletons()
                    .iter()
                    .position(|row| {
                        row.piece == piece
                            && row.cells == ExtendedBoard::from_words([cells, 0, 0, 0])
                    })
                    .expect("specified placement belongs to the actual catalog")
                    as u32
            })
            .collect::<Vec<_>>();
        let candidate = geometry.external_candidate(&catalog, &row_ids).unwrap();
        (catalog, candidate)
    }

    fn extended_b2b_arm<const EARLY: bool>(
        catalog: &ExtendedInverseCatalog,
        candidate: &ExtendedGeometryCandidate,
        rule: KickTableProfileId,
        policy: Option<BackToBackEdgePolicy>,
    ) -> (BuildOrderGraph, SpinCoverageExecutionGraph, usize) {
        let mut workspace = ExtendedBuildOrderWorkspace::new(10, 7, rule);
        let output = build_extended_order_graph_mode::<EARLY>(
            catalog,
            candidate,
            &mut workspace,
            0,
            Some((0, "bounded-b2b-candidate".to_owned())),
            None,
            false,
            policy,
            &ExecutionControl::default(),
        )
        .unwrap();
        let ExtendedBuildOrderResult::Complete {
            graph,
            spin_graph,
            reachability_states,
            ..
        } = output
        else {
            panic!("tiny exact candidate must complete");
        };
        (graph, spin_graph.unwrap(), reachability_states)
    }

    #[test]
    fn extended_b2b_early_clear_pruning_matches_late_filter_across_rules_and_spins() {
        use PieceKind::{I, O, T};
        let o = 0xc03;
        let vertical_i = 1 | (1 << 10) | (1 << 20) | (1 << 30);
        let cases: &[(&str, u64, &[(PieceKind, u64)])] = &[
            ("plain-I-clear", 0x803f0, &[(I, 0xf)]),
            ("I-PC", 0x3f0, &[(I, 0xf)]),
            ("zero-line-I", 0, &[(I, 0xf)]),
            ("T-mini-pocket", 0x27f8, &[(T, 0x807)]),
            ("two-O-orders", 0, &[(O, o), (O, o << 2)]),
            (
                "non-PC-tetris",
                (((1_u64 << 40) - 1) & !vertical_i) | (1 << 49),
                &[(I, vertical_i)],
            ),
        ];
        for &(label, base, placements) in cases {
            let (catalog, candidate) = extended_b2b_candidate(base, placements);
            for rule in [
                KickTableProfileId::Srs90,
                KickTableProfileId::SrsPlus,
                KickTableProfileId::SrsX,
                KickTableProfileId::Jstris180,
                KickTableProfileId::NoKick,
            ] {
                for spin in [
                    SpinProfileSelection::TSpins,
                    SpinProfileSelection::TSpinsPlus,
                    SpinProfileSelection::AllSpin,
                    SpinProfileSelection::AllSpinPlus,
                    SpinProfileSelection::AllMini,
                    SpinProfileSelection::AllMiniPlus,
                ] {
                    let policy = Some(BackToBackEdgePolicy::new(spin));
                    let (reference, reference_spin, reference_states) =
                        extended_b2b_arm::<false>(&catalog, &candidate, rule, policy);
                    let (pruned, pruned_spin, pruned_states) =
                        extended_b2b_arm::<true>(&catalog, &candidate, rule, policy);
                    assert_eq!(reference.root, pruned.root, "{label}/{rule:?}/{spin:?}");
                    assert_eq!(
                        reference.nodes.len(),
                        pruned.nodes.len(),
                        "{label}/{rule:?}/{spin:?}"
                    );
                    for (index, (before, after)) in
                        reference.nodes.iter().zip(&pruned.nodes).enumerate()
                    {
                        assert_eq!(
                            (before.depth, before.live, before.accepting()),
                            (after.depth, after.live, after.accepting()),
                            "{label}/{rule:?}/{spin:?}, node={index}",
                        );
                        assert_eq!(
                            reference
                                .edges(index)
                                .iter()
                                .map(BuildEdge::canonical_key)
                                .collect::<Vec<_>>(),
                            pruned
                                .edges(index)
                                .iter()
                                .map(BuildEdge::canonical_key)
                                .collect::<Vec<_>>(),
                            "{label}/{rule:?}/{spin:?}, node={index}",
                        );
                    }
                    assert_eq!(reference_spin, pruned_spin, "{label}/{rule:?}/{spin:?}");
                    assert!(
                        pruned_states <= reference_states,
                        "{label}/{rule:?}/{spin:?}"
                    );
                    if label == "plain-I-clear"
                        && matches!(
                            spin,
                            SpinProfileSelection::TSpins | SpinProfileSelection::TSpinsPlus
                        )
                    {
                        assert!(!pruned.nodes[pruned.root as usize].live);
                        assert!(
                            reference_states > 0,
                            "the old path really traverses movement"
                        );
                        assert_eq!(
                            pruned_states, 0,
                            "the clear-only negative skips the entire movement query"
                        );
                    }
                    if matches!(label, "I-PC" | "zero-line-I" | "two-O-orders") {
                        assert!(
                            pruned.nodes[pruned.root as usize].live,
                            "{label}/{rule:?}/{spin:?} must retain a real path"
                        );
                        assert!(pruned_states > 0);
                    }
                    if label == "T-mini-pocket" && rule == KickTableProfileId::Srs90 {
                        assert!(
                            pruned.nodes[pruned.root as usize].live,
                            "spin-eligible transitions need exact kick evidence"
                        );
                        assert!(pruned_states > 0);
                    }
                }
            }
        }
    }

    #[test]
    fn extended_b2b_early_clear_pruning_keeps_unconstrained_build_language() {
        let (catalog, candidate) = extended_b2b_candidate(0x803f0, &[(PieceKind::I, 0xf)]);
        let (reference, reference_spin, reference_states) =
            extended_b2b_arm::<false>(&catalog, &candidate, KickTableProfileId::SrsPlus, None);
        let (pruned, pruned_spin, pruned_states) =
            extended_b2b_arm::<true>(&catalog, &candidate, KickTableProfileId::SrsPlus, None);
        assert!(reference.nodes[reference.root as usize].live);
        assert!(pruned.nodes[pruned.root as usize].live);
        assert_eq!(reference_spin, pruned_spin);
        assert!(reference_states > 0);
        assert_eq!(reference_states, pruned_states);
    }

    fn one_t_projection(height: u8) -> CandidateProjection {
        let mut cells = ExtendedBoard::EMPTY;
        for index in [3_u16, 4, 5, 14] {
            assert!(cells.insert(index));
        }
        CandidateProjection {
            operation_cells: vec![cells],
            operation_pieces: vec![PieceKind::T],
            row_projection: CandidateRowProjection::compile(
                10,
                height,
                cells,
                CandidateCells::Extended(&[cells]),
            ),
            completed_target_rows: 0,
            initial_board: ExtendedBoard::EMPTY,
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
            row_projection: CandidateRowProjection::compile(
                10,
                7,
                cells,
                CandidateCells::Extended(&[cells]),
            ),
            completed_target_rows: 0,
            initial_board: ExtendedBoard::EMPTY,
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
