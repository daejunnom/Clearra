// SRP rationale: this module owns the exact, compact continuation automaton
// after the shared setup prefix. Physical placement evidence remains outside
// this graph and is reconstructed only for requested setup details.
use std::{
    collections::hash_map::Entry,
    hash::{Hash, Hasher},
    sync::Arc,
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use super::{
    buildup::{merge_deleted_rows, place_and_clear},
    catalog::GeometryCatalog,
    exact_collections::ExactHashMap,
    geometry::{add_packed_piece, AvailableRowSlice, GeometryCompletionOracle},
    reachability::ReachabilityWorkspace,
    setup_coverage_graph::{
        SetupCoverageEdge, SetupCoverageGraph, SetupCoverageInterner, EMPTY_COVERAGE_REFERENCE,
    },
    setup_partial_build::SetupPartialBuildPrefix,
    WasmExactSearchError,
};

const MAX_PC_LOCKS: u8 = 10;
const CANCELLATION_INTERVAL_MASK: usize = 4095;
const LINEAR_EDGE_DEDUPE_LIMIT: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SuffixState {
    board: u64,
    remaining: u64,
    packed_counts: u32,
    deleted_rows: u16,
}

impl Hash for SuffixState {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut digest = self.board ^ self.remaining.rotate_left(23);
        digest ^= u64::from(self.packed_counts).rotate_left(37);
        digest ^= u64::from(self.deleted_rows).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        state.write_u64(digest);
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct SuffixLayerNode {
    edge_start: u32,
    edge_count: u32,
}

const _: () = assert!(core::mem::size_of::<SuffixLayerNode>() == 8);

#[derive(Default)]
struct SuffixLayer {
    depth: u8,
    nodes: Vec<SuffixLayerNode>,
    edges: Vec<SetupCoverageEdge>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SuffixPhase {
    Build,
    Compile,
    Complete,
}

pub(super) enum SetupSuffixCoverageAdvance {
    Pending,
    Complete {
        graph: super::setup_partial_build::PartialBuildGraph,
        coverage_graph: SetupCoverageGraph,
    },
    Cancelled,
}

pub(super) struct SetupSuffixCoverageSession {
    prefix: Option<SetupPartialBuildPrefix>,
    catalog: Arc<GeometryCatalog>,
    completion_oracle: GeometryCompletionOracle,
    reachability: ReachabilityWorkspace,
    interner: SetupCoverageInterner,

    terminal_nodes: Vec<u32>,
    terminal_state_indices: Vec<u32>,
    terminal_classes: Vec<u32>,

    phase: SuffixPhase,
    current_depth: u8,
    current_states: Vec<SuffixState>,
    next_states: Vec<SuffixState>,
    next_layer: ExactHashMap<SuffixState, u32>,
    layers: Vec<SuffixLayer>,

    current_state_cursor: usize,
    source_active: bool,
    cached_rows: Option<AvailableRowSlice>,
    available_rows: Vec<u32>,
    available_row_cursor: usize,
    source_edges: Vec<SetupCoverageEdge>,
    source_edges_need_sort: bool,
    building_nodes: Vec<SuffixLayerNode>,
    building_edges: Vec<SetupCoverageEdge>,

    compile_layer_count: usize,
    compile_node_cursor: usize,
    compile_next_classes: Vec<u32>,
    compile_current_classes: Vec<u32>,
    compile_edge_scratch: Vec<SetupCoverageEdge>,

    expanded_states: usize,
    processed_work: usize,
    cancellation_work: usize,
}

impl SetupSuffixCoverageSession {
    pub(super) fn new(
        mut prefix: SetupPartialBuildPrefix,
        catalog: Arc<GeometryCatalog>,
        problem: &SearchProblem,
    ) -> Result<Self, WasmExactSearchError> {
        if prefix.nodes.len() != prefix.residuals.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_suffix_prefix_residual_count_mismatch",
            ));
        }
        let completion_oracle =
            prefix
                .completion_oracle
                .take()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_suffix_completion_oracle_missing",
                ))?;
        let candidate_depth = prefix.candidate_depth();
        let mut terminal_nodes = Vec::new();
        let mut terminal_state_indices = Vec::new();
        terminal_nodes
            .try_reserve(prefix.nodes.len())
            .map_err(|_| storage_error("setup_suffix_terminal_storage_unavailable"))?;
        terminal_state_indices
            .try_reserve(prefix.nodes.len())
            .map_err(|_| storage_error("setup_suffix_terminal_storage_unavailable"))?;

        let mut root_states = Vec::<SuffixState>::new();
        let mut root_index = ExactHashMap::<SuffixState, u32>::default();
        for (index, node) in prefix.nodes.iter().copied().enumerate() {
            if node.depth != candidate_depth {
                continue;
            }
            let residual = prefix.residuals[index];
            let state = SuffixState {
                board: node.board,
                remaining: residual.remaining,
                packed_counts: residual.packed_counts,
                deleted_rows: node.deleted_rows,
            };
            ensure_map_insert_capacity(
                &mut root_index,
                "setup_suffix_root_index_storage_unavailable",
            )?;
            let state_index = match root_index.entry(state) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    root_states
                        .try_reserve(1)
                        .map_err(|_| storage_error("setup_suffix_root_storage_unavailable"))?;
                    let state_index = u32::try_from(root_states.len()).map_err(|_| {
                        WasmExactSearchError::InvalidProblem("setup_suffix_state_index_overflow")
                    })?;
                    root_states.push(state);
                    entry.insert(state_index);
                    state_index
                }
            };
            terminal_nodes.push(u32::try_from(index).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_suffix_terminal_index_overflow")
            })?);
            terminal_state_indices.push(state_index);
        }

        let mut reachability = ReachabilityWorkspace::default();
        reachability.configure(catalog.skeleton_count());
        reachability.configure_kick_profile(problem.kick_profile().profile_id());
        let prefix_node_count = prefix.nodes.len();
        let mut layers = Vec::new();
        layers
            .try_reserve_exact(usize::from(MAX_PC_LOCKS.saturating_sub(candidate_depth)))
            .map_err(|_| storage_error("setup_suffix_layer_storage_unavailable"))?;
        let mut building_nodes = Vec::new();
        building_nodes
            .try_reserve_exact(root_states.len())
            .map_err(|_| storage_error("setup_suffix_layer_node_storage_unavailable"))?;

        Ok(Self {
            prefix: Some(prefix),
            catalog,
            completion_oracle,
            reachability,
            interner: SetupCoverageInterner::new(),
            terminal_nodes,
            terminal_state_indices,
            terminal_classes: vec![EMPTY_COVERAGE_REFERENCE; prefix_node_count],
            phase: SuffixPhase::Build,
            current_depth: candidate_depth,
            current_states: root_states,
            next_states: Vec::new(),
            next_layer: ExactHashMap::default(),
            layers,
            current_state_cursor: 0,
            source_active: false,
            cached_rows: None,
            available_rows: Vec::new(),
            available_row_cursor: 0,
            source_edges: Vec::new(),
            source_edges_need_sort: false,
            building_nodes,
            building_edges: Vec::new(),
            compile_layer_count: 0,
            compile_node_cursor: 0,
            compile_next_classes: Vec::new(),
            compile_current_classes: Vec::new(),
            compile_edge_scratch: Vec::new(),
            expanded_states: 0,
            processed_work: 0,
            cancellation_work: 0,
        })
    }

    pub(super) fn expanded_states(&self) -> usize {
        self.expanded_states
    }

    pub(super) fn prefix_node_count(&self) -> usize {
        self.prefix.as_ref().map_or(0, |prefix| prefix.nodes.len())
    }

    pub(super) fn layer_progress(&self) -> (usize, usize, usize, usize) {
        match self.phase {
            SuffixPhase::Build => {
                let candidate_depth = self
                    .prefix
                    .as_ref()
                    .map_or(self.current_depth, SetupPartialBuildPrefix::candidate_depth);
                let layer_count = usize::from(MAX_PC_LOCKS.saturating_sub(candidate_depth)).max(1);
                let layer_index = usize::from(self.current_depth.saturating_sub(candidate_depth))
                    .min(layer_count.saturating_sub(1));
                (
                    layer_index,
                    layer_count,
                    self.current_state_cursor.min(self.current_states.len()),
                    self.current_states.len(),
                )
            }
            SuffixPhase::Compile => {
                let layer_count = self.layers.len().max(1);
                let layer_index = self
                    .layers
                    .len()
                    .saturating_sub(self.compile_layer_count)
                    .min(layer_count.saturating_sub(1));
                let layer_total = self
                    .compile_layer_count
                    .checked_sub(1)
                    .and_then(|index| self.layers.get(index))
                    .map_or(0, |layer| layer.nodes.len());
                (
                    layer_index,
                    layer_count,
                    self.compile_node_cursor.min(layer_total),
                    layer_total,
                )
            }
            SuffixPhase::Complete => (1, 1, 1, 1),
        }
    }

    pub(super) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<SetupSuffixCoverageAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(SetupSuffixCoverageAdvance::Cancelled);
        }
        let work_target = self.processed_work.saturating_add(work_budget.max(1));
        while self.phase != SuffixPhase::Complete {
            self.check_cancel(control)?;
            match self.phase {
                SuffixPhase::Build => self.step_build(control)?,
                SuffixPhase::Compile => self.step_compile()?,
                SuffixPhase::Complete => break,
            }
            self.processed_work = self.processed_work.saturating_add(1);
            if self.processed_work >= work_target {
                return Ok(SetupSuffixCoverageAdvance::Pending);
            }
        }

        let prefix = self
            .prefix
            .take()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_suffix_prefix_already_consumed",
            ))?;
        let terminal_classes = std::mem::take(&mut self.terminal_classes);
        let (graph, terminal_classes) = prefix.finalize(terminal_classes)?;
        let interner = std::mem::replace(&mut self.interner, SetupCoverageInterner::new());
        let coverage_graph =
            SetupCoverageGraph::compile_from_suffix(&graph, terminal_classes, interner)?;
        Ok(SetupSuffixCoverageAdvance::Complete {
            graph,
            coverage_graph,
        })
    }

    fn step_build(&mut self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if self.current_depth >= MAX_PC_LOCKS {
            return self.begin_compile();
        }
        if self.current_state_cursor == self.current_states.len() {
            return self.finish_build_layer();
        }
        if !self.source_active {
            return self.start_source(control);
        }
        let row_count = self
            .cached_rows
            .map_or(self.available_rows.len(), AvailableRowSlice::len);
        if self.available_row_cursor < row_count {
            return self.expand_source_row();
        }
        self.finish_source()
    }

    fn start_source(&mut self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        let state = self.current_states[self.current_state_cursor];
        self.expanded_states = self.expanded_states.saturating_add(1);
        let cached_rows = self.completion_oracle.collect_available_rows_storage(
            state.remaining,
            state.packed_counts,
            state.deleted_rows != 0,
            &self.catalog,
            &mut self.available_rows,
            control,
        )?;
        self.cached_rows = cached_rows;
        self.available_row_cursor = 0;
        self.source_edges.clear();
        self.source_edges_need_sort = false;
        self.source_active = true;
        Ok(())
    }

    fn expand_source_row(&mut self) -> Result<(), WasmExactSearchError> {
        let state = self.current_states[self.current_state_cursor];
        let row_id = self.source_row_id(self.available_row_cursor)?;
        self.available_row_cursor += 1;
        let row = self.catalog.skeleton(row_id);
        let packed_counts = add_packed_piece(state.packed_counts, super::piece_index(row.piece))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_suffix_piece_count_overflow",
            ))?;
        let remaining = state.remaining ^ row.cells;
        let realization_count = self
            .catalog
            .instantiated_realizations(row_id, state.deleted_rows)
            .map_or(usize::from(row.realization_count), <[_]>::len);
        ensure_vec_additional_capacity(
            &mut self.source_edges,
            realization_count,
            "setup_suffix_source_edge_storage_unavailable",
        )?;
        for realization in self.catalog.instantiations(row_id, state.deleted_rows) {
            if state.board & realization.lock_mask != 0 {
                continue;
            }
            if !self.reachability.lock_reachable_instantiated(
                &self.catalog,
                state.board,
                row.piece,
                realization,
            ) {
                continue;
            }
            let (board, cleared_current, _) = place_and_clear(
                self.catalog.width(),
                self.catalog.height(),
                state.board | realization.lock_mask,
            );
            let Some(deleted_rows) =
                merge_deleted_rows(self.catalog.height(), state.deleted_rows, cleared_current)
            else {
                continue;
            };
            let child = SuffixState {
                board,
                remaining,
                packed_counts,
                deleted_rows,
            };
            let (child_index, _) =
                intern_next_state(&mut self.next_layer, &mut self.next_states, child)?;
            push_source_edge(
                &mut self.source_edges,
                &mut self.source_edges_need_sort,
                SetupCoverageEdge::new(child_index, row.piece)?,
            );
        }
        Ok(())
    }

    fn finish_source(&mut self) -> Result<(), WasmExactSearchError> {
        if self.source_edges_need_sort {
            self.source_edges.sort_unstable();
            self.source_edges.dedup();
        }
        let edge_start = u32::try_from(self.building_edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_suffix_edge_index_overflow")
        })?;
        let edge_count = u32::try_from(self.source_edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_suffix_edge_count_overflow")
        })?;
        ensure_vec_additional_capacity(
            &mut self.building_edges,
            self.source_edges.len(),
            "setup_suffix_edge_storage_unavailable",
        )?;
        self.building_edges.extend_from_slice(&self.source_edges);
        self.building_nodes.push(SuffixLayerNode {
            edge_start,
            edge_count,
        });
        self.current_state_cursor += 1;
        self.source_active = false;
        self.cached_rows = None;
        self.available_rows.clear();
        self.available_row_cursor = 0;
        Ok(())
    }

    fn finish_build_layer(&mut self) -> Result<(), WasmExactSearchError> {
        if self.building_nodes.len() != self.current_states.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_suffix_layer_node_count_mismatch",
            ));
        }
        self.layers
            .try_reserve(1)
            .map_err(|_| storage_error("setup_suffix_layer_storage_unavailable"))?;
        self.layers.push(SuffixLayer {
            depth: self.current_depth,
            nodes: std::mem::take(&mut self.building_nodes),
            edges: std::mem::take(&mut self.building_edges),
        });
        self.current_depth += 1;
        std::mem::swap(&mut self.current_states, &mut self.next_states);
        self.next_states.clear();
        self.next_layer.clear();
        self.current_state_cursor = 0;
        self.source_active = false;
        self.building_nodes
            .try_reserve_exact(self.current_states.len())
            .map_err(|_| storage_error("setup_suffix_layer_node_storage_unavailable"))?;
        if self.current_depth >= MAX_PC_LOCKS {
            self.begin_compile()?;
        }
        Ok(())
    }

    fn begin_compile(&mut self) -> Result<(), WasmExactSearchError> {
        if self.phase == SuffixPhase::Compile {
            return Ok(());
        }
        self.compile_next_classes.clear();
        self.compile_next_classes
            .try_reserve_exact(self.current_states.len())
            .map_err(|_| storage_error("setup_suffix_class_storage_unavailable"))?;
        let mut empty_edges = Vec::new();
        let accepting_class =
            self.interner
                .intern_language_node(MAX_PC_LOCKS, true, &mut empty_edges)?;
        for state in &self.current_states {
            self.compile_next_classes
                .push(if state.remaining == 0 && state.board == 0 {
                    accepting_class
                } else {
                    EMPTY_COVERAGE_REFERENCE
                });
        }
        self.compile_layer_count = self.layers.len();
        self.compile_node_cursor = 0;
        self.current_states.clear();
        self.next_states.clear();
        self.next_layer.clear();
        self.phase = SuffixPhase::Compile;
        Ok(())
    }

    fn step_compile(&mut self) -> Result<(), WasmExactSearchError> {
        if self.compile_layer_count == 0 {
            if self.terminal_nodes.len() != self.terminal_state_indices.len() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_suffix_terminal_state_count_mismatch",
                ));
            }
            for (&node, &state_index) in
                self.terminal_nodes.iter().zip(&self.terminal_state_indices)
            {
                let class = *self.compile_next_classes.get(state_index as usize).ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "setup_suffix_terminal_state_out_of_range",
                    ),
                )?;
                self.terminal_classes[node as usize] = class;
            }
            self.phase = SuffixPhase::Complete;
            return Ok(());
        }

        let layer_index = self.compile_layer_count - 1;
        let layer = &self.layers[layer_index];
        if self.compile_node_cursor == 0 && self.compile_current_classes.is_empty() {
            self.compile_current_classes
                .try_reserve_exact(layer.nodes.len())
                .map_err(|_| storage_error("setup_suffix_class_storage_unavailable"))?;
        }
        if self.compile_node_cursor < layer.nodes.len() {
            let node = layer.nodes[self.compile_node_cursor];
            let edge_start = node.edge_start as usize;
            let edge_end = edge_start + node.edge_count as usize;
            self.compile_edge_scratch.clear();
            ensure_vec_additional_capacity(
                &mut self.compile_edge_scratch,
                node.edge_count as usize,
                "setup_suffix_class_edge_storage_unavailable",
            )?;
            for edge in &layer.edges[edge_start..edge_end] {
                let child_class = *self.compile_next_classes.get(edge.child() as usize).ok_or(
                    WasmExactSearchError::InvalidProblem("setup_suffix_child_class_out_of_range"),
                )?;
                if child_class != EMPTY_COVERAGE_REFERENCE {
                    self.compile_edge_scratch
                        .push(edge.with_child(child_class)?);
                }
            }
            let class = if self.compile_edge_scratch.is_empty() {
                EMPTY_COVERAGE_REFERENCE
            } else {
                self.interner.intern_language_node(
                    layer.depth,
                    false,
                    &mut self.compile_edge_scratch,
                )?
            };
            self.compile_current_classes.push(class);
            self.compile_node_cursor += 1;
            return Ok(());
        }

        if self.compile_current_classes.len() != layer.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_suffix_class_count_mismatch",
            ));
        }
        std::mem::swap(
            &mut self.compile_next_classes,
            &mut self.compile_current_classes,
        );
        self.compile_current_classes.clear();
        self.layers[layer_index] = SuffixLayer::default();
        self.compile_layer_count -= 1;
        self.compile_node_cursor = 0;
        Ok(())
    }

    fn source_row_id(&self, index: usize) -> Result<u32, WasmExactSearchError> {
        if let Some(cached) = self.cached_rows {
            return self
                .completion_oracle
                .available_rows(cached)?
                .get(index)
                .copied()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_suffix_cached_row_out_of_range",
                ));
        }
        self.available_rows
            .get(index)
            .copied()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_suffix_owned_row_out_of_range",
            ))
    }

    fn check_cancel(&mut self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        self.cancellation_work = self.cancellation_work.wrapping_add(1);
        if self.cancellation_work & CANCELLATION_INTERVAL_MASK == 0 && control.is_cancelled() {
            Err(WasmExactSearchError::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn ensure_map_insert_capacity<K, V>(
    map: &mut ExactHashMap<K, V>,
    error: &'static str,
) -> Result<(), WasmExactSearchError>
where
    K: Eq + Hash,
{
    if map.len() < map.capacity() {
        return Ok(());
    }
    map.try_reserve(map.len().max(1024))
        .map_err(|_| storage_error(error))
}

fn intern_next_state(
    next_layer: &mut ExactHashMap<SuffixState, u32>,
    next_states: &mut Vec<SuffixState>,
    state: SuffixState,
) -> Result<(u32, bool), WasmExactSearchError> {
    ensure_map_insert_capacity(next_layer, "setup_suffix_state_index_storage_unavailable")?;
    let next_index = u32::try_from(next_states.len())
        .map_err(|_| WasmExactSearchError::InvalidProblem("setup_suffix_state_index_overflow"))?;
    match next_layer.entry(state) {
        Entry::Occupied(entry) => Ok((*entry.get(), true)),
        Entry::Vacant(entry) => {
            next_states
                .try_reserve(1)
                .map_err(|_| storage_error("setup_suffix_state_storage_unavailable"))?;
            next_states.push(state);
            entry.insert(next_index);
            Ok((next_index, false))
        }
    }
}

fn ensure_vec_additional_capacity<T>(
    values: &mut Vec<T>,
    additional: usize,
    error: &'static str,
) -> Result<(), WasmExactSearchError> {
    let required = values
        .len()
        .checked_add(additional)
        .ok_or_else(|| storage_error(error))?;
    if required <= values.capacity() {
        return Ok(());
    }
    values
        .try_reserve(required - values.len())
        .map_err(|_| storage_error(error))
}

fn push_source_edge(
    edges: &mut Vec<SetupCoverageEdge>,
    needs_sort: &mut bool,
    edge: SetupCoverageEdge,
) {
    if edges.len() < LINEAR_EDGE_DEDUPE_LIMIT {
        if !edges.contains(&edge) {
            edges.push(edge);
        }
        return;
    }
    *needs_sort = true;
    edges.push(edge);
}

const fn storage_error(reason: &'static str) -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem(reason)
}
