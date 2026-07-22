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

/// Result produced only by the complete realization-feasibility engine.
/// Callers can inspect it but cannot manufacture an infeasible conclusion.
#[derive(Clone, Copy, Debug)]
pub(super) struct RealizationFeasibility {
    kind: FeasibilityKind,
    explored_states: usize,
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
            });
        }
        self.compile_operation_priority(catalog, candidate, operation_count);
        self.explored_states = 0;
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
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.failed_generations.capacity() * core::mem::size_of::<u32>()
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
            self.generation = 1;
        }
        true
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

fn lowest_target_row(width: u8, cells: u64) -> u8 {
    (cells.trailing_zeros() as u8) / width
}
