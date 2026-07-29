use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    buildup::{
        merge_deleted_rows, place_and_clear, placement_is_grounded, BuildCompletion,
        CandidateProjection,
    },
    catalog::GeometryCatalog,
    geometry::GeometryCandidate,
    WasmExactSearchError, MAX_BOARD64_PIECES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FeasibilityKind {
    Feasible,
    Infeasible,
    Unknown,
}

/// Handle for the complete canonical-operation dependency graph retained in
/// `RealizationFeasibilityWorkspace`.
///
/// Nodes are exact placed-operation subsets. Edges are labeled by canonical
/// operation ID, so several distinct parents may converge on one child without
/// collapsing duplicate tetromino placements or choosing a representative
/// order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildPartialDependencyGraph {
    generation: u32,
    operation_count: u8,
}

/// Result produced only by the complete realization-feasibility engine.
/// Callers can inspect it but cannot manufacture an infeasible conclusion.
#[derive(Clone, Copy, Debug)]
pub(super) struct RealizationFeasibility {
    kind: FeasibilityKind,
    explored_states: usize,
    generation: u32,
    operation_count: u8,
    partial_dependency_graph: Option<BuildPartialDependencyGraph>,
}

impl RealizationFeasibility {
    pub fn is_infeasible(self) -> bool {
        self.kind == FeasibilityKind::Infeasible
    }

    pub fn explored_states(self) -> usize {
        self.explored_states
    }
}

/// Reusable exact failed-state table. A subset is memoized only after every
/// operation and every concrete realization has been exhaustively rejected.
#[derive(Default)]
pub(super) struct RealizationFeasibilityWorkspace {
    failed_generations: Vec<u32>,
    reachable_generations: Vec<u32>,
    live_generations: Vec<u32>,
    transition_masks: Vec<u16>,
    subset_queue: Vec<u16>,
    generation: u32,
    explored_states: usize,
    operation_priority: [u8; MAX_BOARD64_PIECES],
}

impl RealizationFeasibilityWorkspace {
    /// Rejects only candidates whose inverse lock-clear dependencies cannot
    /// make progress even after collision and grounding constraints are
    /// relaxed. Every concrete build order is contained in this monotone
    /// closure, so a stalled closure is an exact infeasibility certificate.
    pub fn dependency_relaxation_is_infeasible(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> bool {
        let operation_count = candidate.row_ids().len();
        if operation_count == 0 || operation_count > MAX_BOARD64_PIECES {
            return false;
        }

        let mut row_contributors = [0_u16; 16];
        for (operation_index, row_id) in candidate.row_ids().iter().copied().enumerate() {
            let operation_bit = 1_u16 << operation_index;
            let mut occupied_rows = catalog.skeleton_occupied_rows(row_id);
            while occupied_rows != 0 {
                let row = occupied_rows.trailing_zeros() as usize;
                occupied_rows &= occupied_rows - 1;
                row_contributors[row] |= operation_bit;
            }
        }

        let all_placed = if operation_count == u16::BITS as usize {
            u16::MAX
        } else {
            (1_u16 << operation_count) - 1
        };
        let mut placed = 0_u16;
        while placed != all_placed {
            let mut deleted_rows = 0_u16;
            for (row, contributors) in row_contributors
                .iter()
                .copied()
                .take(usize::from(catalog.height()))
                .enumerate()
            {
                if contributors != 0 && contributors & !placed == 0 {
                    deleted_rows |= 1_u16 << row;
                }
            }

            let mut ready = 0_u16;
            for (operation_index, row_id) in candidate.row_ids().iter().copied().enumerate() {
                let operation_bit = 1_u16 << operation_index;
                if placed & operation_bit != 0 {
                    continue;
                }
                if catalog.realization_requirement_is_satisfied(row_id, deleted_rows) {
                    ready |= operation_bit;
                }
            }
            if ready == 0 {
                return true;
            }
            placed |= ready;
        }
        false
    }

    pub fn analyze(
        &mut self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        completion: BuildCompletion,
        precompute_dependencies: bool,
        control: &ExecutionControl,
    ) -> Result<RealizationFeasibility, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let operation_count = projection.operation_count();
        if operation_count == 0 || operation_count > MAX_BOARD64_PIECES {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_realization_feasibility_operation_count_invalid",
            ));
        }
        let state_count = projection.state_count();
        if state_count != 1_usize << operation_count || projection.all_placed + 1 != state_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_realization_feasibility_projection_invalid",
            ));
        }
        if !self.begin_generation(state_count) {
            return Ok(RealizationFeasibility {
                kind: FeasibilityKind::Unknown,
                explored_states: 0,
                generation: 0,
                operation_count: operation_count as u8,
                partial_dependency_graph: None,
            });
        }
        self.compile_operation_priority(catalog, candidate, operation_count);
        self.explored_states = 0;
        if precompute_dependencies && self.prepare_dependency_storage(state_count) {
            if let Some(result) = self.analyze_complete_graph(
                catalog,
                candidate,
                projection,
                operation_count,
                completion,
                control,
            )? {
                return Ok(result);
            }
            self.explored_states = 0;
        }
        let feasible = self.search(
            catalog,
            candidate,
            projection,
            0,
            operation_count,
            completion,
            control,
        )?;
        Ok(RealizationFeasibility {
            kind: if feasible {
                FeasibilityKind::Feasible
            } else {
                FeasibilityKind::Infeasible
            },
            explored_states: self.explored_states,
            generation: self.generation,
            operation_count: operation_count as u8,
            partial_dependency_graph: None,
        })
    }

    pub fn proves_subset_infeasible(&self, proof: RealizationFeasibility, subset: usize) -> bool {
        proof.generation != 0
            && proof.generation == self.generation
            && self.failed_generations.get(subset).copied() == Some(proof.generation)
    }

    pub fn has_current_partial_dependency_graph(&self, proof: RealizationFeasibility) -> bool {
        proof.partial_dependency_graph.is_some_and(|graph| {
            graph.generation != 0
                && graph.generation == proof.generation
                && graph.generation == self.generation
                && graph.operation_count == proof.operation_count
        })
    }

    /// Returns the canonical operations that may extend this exact parent
    /// subset and still reach a completed realization.
    ///
    /// A missing or stale graph deliberately fails open to the ordinary
    /// BuildUp search. A valid graph returns zero for a dead parent and keeps
    /// every live incoming edge to a converged child.
    pub fn permitted_operation_mask(&self, proof: RealizationFeasibility, subset: usize) -> u16 {
        let operation_count = usize::from(proof.operation_count);
        if operation_count == 0 || operation_count > MAX_BOARD64_PIECES {
            return u16::MAX;
        }
        let all_operations = (1_u16 << operation_count) - 1;
        let unplaced = all_operations & !(subset as u16);
        let Some(graph) = proof.partial_dependency_graph else {
            return unplaced;
        };
        if !self.has_current_partial_dependency_graph(proof) {
            return unplaced;
        }
        if subset >= 1_usize << operation_count
            || self.live_generations.get(subset).copied() != Some(graph.generation)
        {
            return 0;
        }
        self.transition_masks.get(subset).copied().unwrap_or(0) & unplaced
    }

    pub fn retained_bytes(&self) -> usize {
        self.failed_generations.capacity() * core::mem::size_of::<u32>()
            + (self.reachable_generations.capacity() + self.live_generations.capacity())
                * core::mem::size_of::<u32>()
            + self.transition_masks.capacity() * core::mem::size_of::<u16>()
            + self.subset_queue.capacity() * core::mem::size_of::<u16>()
    }

    fn begin_generation(&mut self, state_count: usize) -> bool {
        if self.failed_generations.len() < state_count {
            if self
                .failed_generations
                .try_reserve_exact(state_count - self.failed_generations.len())
                .is_err()
            {
                return false;
            }
            self.failed_generations.resize(state_count, 0);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.failed_generations.fill(0);
            self.reachable_generations.fill(0);
            self.live_generations.fill(0);
            self.generation = 1;
        }
        true
    }

    fn prepare_dependency_storage(&mut self, state_count: usize) -> bool {
        reserve_storage(&mut self.reachable_generations, state_count, 0_u32)
            && reserve_storage(&mut self.live_generations, state_count, 0_u32)
            && reserve_storage(&mut self.transition_masks, state_count, 0_u16)
            && self
                .subset_queue
                .try_reserve(state_count.saturating_sub(self.subset_queue.len()))
                .is_ok()
    }

    fn analyze_complete_graph(
        &mut self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        operation_count: usize,
        completion: BuildCompletion,
        control: &ExecutionControl,
    ) -> Result<Option<RealizationFeasibility>, WasmExactSearchError> {
        // This is a geometry-only prefilter. Kick-sensitive dependencies stay
        // in the exact BuildUp reachability pass so that reachability is not
        // computed twice when dependency precomputation is enabled.
        let all_placed = projection.all_placed;
        let generation = self.generation;
        self.subset_queue.clear();
        self.subset_queue.push(0);
        self.reachable_generations[0] = generation;

        let mut cursor = 0usize;
        while cursor < self.subset_queue.len() {
            let subset = usize::from(self.subset_queue[cursor]);
            cursor += 1;
            self.explored_states = self.explored_states.saturating_add(1);
            if self.explored_states & 0xff == 0 && control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if subset == all_placed {
                self.transition_masks[subset] = 0;
                continue;
            }

            let (board, deleted_rows) = projection.state(subset);
            let mut transitions = 0_u16;
            for priority_index in 0..operation_count {
                let operation_index = self.operation_priority[priority_index] as usize;
                let operation_bit = 1_usize << operation_index;
                if subset & operation_bit != 0 {
                    continue;
                }
                let child = subset | operation_bit;
                let row_id = candidate.row_ids()[operation_index];
                let mut legal = false;
                for realization in catalog.instantiations(row_id, deleted_rows) {
                    let lock_mask = realization.lock_mask;
                    if board & lock_mask != 0
                        || !placement_is_grounded(catalog.width(), board, lock_mask)
                    {
                        continue;
                    }
                    let (next_board, cleared_current, _) =
                        place_and_clear(catalog.width(), catalog.height(), board | lock_mask);
                    let Some(next_deleted_rows) =
                        merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)
                    else {
                        continue;
                    };
                    if projection.confirm_transition(child, next_board, next_deleted_rows) {
                        legal = true;
                        break;
                    }
                }
                if !legal {
                    continue;
                }
                transitions |= 1_u16 << operation_index;
                if self.reachable_generations[child] != generation {
                    self.reachable_generations[child] = generation;
                    self.subset_queue.push(child as u16);
                }
            }
            self.transition_masks[subset] = transitions;
        }

        if self.reachable_generations[all_placed] == generation
            && completion.accepts(projection, all_placed)
        {
            self.live_generations[all_placed] = generation;
        }
        for subset in (0..all_placed).rev() {
            if self.reachable_generations[subset] != generation {
                continue;
            }
            let mut transitions = self.transition_masks[subset];
            while transitions != 0 {
                let operation_index = transitions.trailing_zeros() as usize;
                transitions &= transitions - 1;
                let child = subset | (1_usize << operation_index);
                if self.live_generations[child] == generation {
                    self.live_generations[subset] = generation;
                    break;
                }
            }
            if self.live_generations[subset] != generation {
                self.failed_generations[subset] = generation;
            }
        }
        if self.live_generations[0] != generation {
            return Ok(Some(RealizationFeasibility {
                kind: FeasibilityKind::Infeasible,
                explored_states: self.explored_states,
                generation,
                operation_count: operation_count as u8,
                partial_dependency_graph: None,
            }));
        }

        // Seal the complete graph by retaining every edge whose parent and
        // child can reach completion. This preserves diamonds such as
        // A -> AB <- B instead of reducing them to a single preferred order.
        for subset in 0..=all_placed {
            if self.live_generations[subset] != generation {
                self.transition_masks[subset] = 0;
                continue;
            }
            let mut transitions = self.transition_masks[subset];
            let mut live_transitions = 0_u16;
            while transitions != 0 {
                let operation_index = transitions.trailing_zeros() as usize;
                transitions &= transitions - 1;
                let child = subset | (1_usize << operation_index);
                if self.live_generations[child] == generation {
                    live_transitions |= 1_u16 << operation_index;
                }
            }
            self.transition_masks[subset] = live_transitions;
        }
        Ok(Some(RealizationFeasibility {
            kind: FeasibilityKind::Feasible,
            explored_states: self.explored_states,
            generation,
            operation_count: operation_count as u8,
            partial_dependency_graph: Some(BuildPartialDependencyGraph {
                generation,
                operation_count: operation_count as u8,
            }),
        }))
    }

    fn compile_operation_priority(
        &mut self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        operation_count: usize,
    ) {
        for index in 0..operation_count {
            self.operation_priority[index] = index as u8;
        }
        self.operation_priority[..operation_count].sort_unstable_by_key(|operation| {
            let cells = catalog
                .skeleton(candidate.row_ids()[*operation as usize])
                .cells;
            (lowest_target_row(catalog.width(), cells), *operation)
        });
    }

    fn search(
        &mut self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        projection: &mut CandidateProjection,
        subset: usize,
        operation_count: usize,
        completion: BuildCompletion,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if subset == projection.all_placed {
            return Ok(completion.accepts(projection, subset));
        }
        if self.failed_generations[subset] == self.generation {
            return Ok(false);
        }
        self.explored_states = self.explored_states.saturating_add(1);
        if self.explored_states & 0xff == 0 && control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }

        let (board, deleted_rows) = projection.state(subset);
        for priority_index in 0..operation_count {
            let operation_index = self.operation_priority[priority_index] as usize;
            let operation_bit = 1_usize << operation_index;
            if subset & operation_bit != 0 {
                continue;
            }
            let child = subset | operation_bit;
            let row_id = candidate.row_ids()[operation_index];
            for realization in catalog.instantiations(row_id, deleted_rows) {
                let lock_mask = realization.lock_mask;
                if board & lock_mask != 0
                    || !placement_is_grounded(catalog.width(), board, lock_mask)
                {
                    continue;
                }
                let (next_board, cleared_current, _) =
                    place_and_clear(catalog.width(), catalog.height(), board | lock_mask);
                let Some(next_deleted_rows) =
                    merge_deleted_rows(catalog.height(), deleted_rows, cleared_current)
                else {
                    continue;
                };
                if !projection.confirm_transition(child, next_board, next_deleted_rows) {
                    continue;
                }
                if self.search(
                    catalog,
                    candidate,
                    projection,
                    child,
                    operation_count,
                    completion,
                    control,
                )? {
                    return Ok(true);
                }
            }
        }
        self.failed_generations[subset] = self.generation;
        Ok(false)
    }
}

fn reserve_storage<T: Copy>(storage: &mut Vec<T>, len: usize, empty: T) -> bool {
    if storage.len() >= len {
        return true;
    }
    if storage.try_reserve_exact(len - storage.len()).is_err() {
        return false;
    }
    storage.resize(len, empty);
    true
}

fn lowest_target_row(width: u8, cells: u64) -> u8 {
    (cells.trailing_zeros() as u8) / width
}

#[cfg(test)]
mod tests {
    use super::{
        BuildPartialDependencyGraph, FeasibilityKind, RealizationFeasibility,
        RealizationFeasibilityWorkspace,
    };

    #[test]
    fn partial_dependency_graph_preserves_two_parents_of_one_child() {
        let (workspace, proof) = converged_graph_fixture();

        // A -> AB and B -> AB are both retained. Neither parent is selected
        // as the representative of the converged child.
        assert_eq!(workspace.permitted_operation_mask(proof, 0b001), 0b010);
        assert_eq!(workspace.permitted_operation_mask(proof, 0b010), 0b001);
        assert_eq!(workspace.permitted_operation_mask(proof, 0b011), 0b100);
    }

    #[test]
    fn canonical_operation_ids_remain_distinct_at_a_convergence() {
        let (workspace, proof) = converged_graph_fixture();

        // Operations 0 and 1 may be placements of the same piece kind. The
        // graph addresses their canonical operation bits independently.
        assert_ne!(
            workspace.permitted_operation_mask(proof, 0b001),
            workspace.permitted_operation_mask(proof, 0b010)
        );
    }

    #[test]
    fn dead_parent_has_no_permitted_partial_dependency_transition() {
        let (mut workspace, proof) = converged_graph_fixture();
        workspace.live_generations[0b001] = 0;

        assert_eq!(workspace.permitted_operation_mask(proof, 0b001), 0);
    }

    #[test]
    fn stale_partial_dependency_graph_fails_open() {
        let (mut workspace, proof) = converged_graph_fixture();
        workspace.generation = workspace.generation.wrapping_add(1);

        assert_eq!(workspace.permitted_operation_mask(proof, 0b001), 0b110);
    }

    fn converged_graph_fixture() -> (RealizationFeasibilityWorkspace, RealizationFeasibility) {
        let generation = 7;
        let mut workspace = RealizationFeasibilityWorkspace {
            generation,
            live_generations: vec![0; 8],
            transition_masks: vec![0; 8],
            ..RealizationFeasibilityWorkspace::default()
        };
        for subset in [0b000, 0b001, 0b010, 0b011, 0b111] {
            workspace.live_generations[subset] = generation;
        }
        workspace.transition_masks[0b000] = 0b011;
        workspace.transition_masks[0b001] = 0b010;
        workspace.transition_masks[0b010] = 0b001;
        workspace.transition_masks[0b011] = 0b100;
        let proof = RealizationFeasibility {
            kind: FeasibilityKind::Feasible,
            explored_states: 5,
            generation,
            operation_count: 3,
            partial_dependency_graph: Some(BuildPartialDependencyGraph {
                generation,
                operation_count: 3,
            }),
        };
        (workspace, proof)
    }
}
