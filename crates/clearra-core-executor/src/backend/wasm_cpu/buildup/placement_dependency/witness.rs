//! Materialize one representative from a proved placement language.
//!
//! The dependency relation supplies all allowed subset/operation edges.
//! Resolve the original queue and hold choices on that relation, then
//! recover a concrete pose only for the chosen locks. No BuildUp graph
//! or candidate-wide forward movement traversal is needed.

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_problem::SearchProblem;

use super::super::{
    accepts_pattern_path, can_release_held_at_terminal, hold_successors, placement_is_grounded,
    CoverageState,
};
use crate::backend::wasm_cpu::{
    catalog::{GeometryCatalog, InstantiatedRealization},
    exact_collections::ExactHashSet,
    geometry::GeometryCandidate,
    placement_poset::CandidateGeometry,
    reachability::{harddrop_path_mask, PrefixPoseWork, ReachabilityWorkspace},
    symbolic_prefix_orders::SymbolicPrefixOrders,
    WasmExactSearchError,
};
use crate::CorePathStep;

#[derive(Clone, Copy)]
struct LockChoice {
    operation: usize,
    piece: PieceKind,
    hold: &'static str,
}

struct PatternWitness<'a> {
    orders: &'a SymbolicPrefixOrders,
    problem: &'a SearchProblem,
    sequence: &'a [PieceKind],
    control: &'a ExecutionControl,
    seen: ExactHashSet<CoverageState>,
    choices: Vec<LockChoice>,
}

impl PatternWitness<'_> {
    fn select_locks(&mut self, state: CoverageState) -> Result<bool, WasmExactSearchError> {
        if self.control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let supply = self.problem.supply();
        // The node is the executed subset, not an index into a BuildUp graph.
        let subset = state.node as u16;
        let terminal = self.orders.terminal_prefix();
        if subset == terminal {
            return Ok(accepts_pattern_path(
                self.sequence.len(),
                supply.projects_unplaced_lookahead(),
                state,
            ));
        }
        if !self.seen.insert(state) {
            return Ok(false);
        }

        for (operation, piece) in self.orders.live_operations_after(subset) {
            let next = subset | (1_u16 << operation);
            if next == terminal
                && can_release_held_at_terminal(
                    self.sequence,
                    supply.hold_enabled(),
                    supply.projects_unplaced_lookahead(),
                    supply.projects_standard_bag_lookahead(),
                    state,
                    piece,
                )
            {
                self.choices.push(LockChoice {
                    operation,
                    piece,
                    hold: "release-held-at-terminal",
                });
                return Ok(true);
            }
            for (hold, supply_state) in hold_successors(
                self.sequence,
                supply.hold_enabled(),
                supply.projects_unplaced_lookahead(),
                supply.projects_standard_bag_lookahead(),
                self.problem.initial_hold(),
                state,
                piece,
                None,
            ) {
                self.choices.push(LockChoice {
                    operation,
                    piece,
                    hold,
                });
                if self.select_locks(CoverageState {
                    node: u32::from(next),
                    ..supply_state
                })? {
                    return Ok(true);
                }
                self.choices.pop();
            }
        }
        Ok(false)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    orders: &SymbolicPrefixOrders,
    pattern_id: u32,
    reachability: &mut ReachabilityWorkspace,
    control: &ExecutionControl,
    work: &mut PrefixPoseWork,
) -> Result<Vec<CorePathStep>, WasmExactSearchError> {
    let sequence = problem
        .piece_source()
        .materialized_universe()
        .and_then(|universe| universe.try_sequence_at(pattern_id as usize))
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_placement_dependency_witness_pattern_invalid",
        ))?;
    let mut witness = PatternWitness {
        orders,
        problem,
        sequence: sequence.as_ref(),
        control,
        seen: ExactHashSet::default(),
        choices: Vec::with_capacity(candidate.row_ids().len()),
    };
    if !witness.select_locks(CoverageState {
        node: 0,
        cursor: problem.initial_hold().cursor(),
        hold: problem.initial_hold().hold_piece(),
    })? {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_placement_dependency_witness_order_missing",
        ));
    }

    let geometry = CandidateGeometry::for_symbolic_prefix(catalog, candidate).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_placement_dependency_witness_frame_invalid"),
    )?;
    let mut path = Vec::with_capacity(witness.choices.len());
    let mut executed = 0_u16;
    for choice in witness.choices {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let deleted = geometry.symbolic_deleted_rows(executed);
        let board = geometry.symbolic_physical_board(executed, deleted);
        let lock = representative_lock(
            catalog,
            reachability,
            catalog.instantiations(candidate.row_ids()[choice.operation], deleted),
            choice.piece,
            board,
            control,
            work,
        )?;
        executed |= 1 << choice.operation;
        let cleared = geometry.symbolic_deleted_rows(executed) & !deleted;
        path.push(CorePathStep::new(
            choice.piece,
            lock.rotation.quarter_turns(),
            i32::from(lock.x),
            i32::from(lock.lock_y),
            choice.hold,
            cleared.count_ones() as u8,
        ));
    }
    Ok(path)
}

fn representative_lock(
    catalog: &GeometryCatalog,
    reachability: &mut ReachabilityWorkspace,
    locks: impl Iterator<Item = InstantiatedRealization>,
    piece: PieceKind,
    board: u64,
    control: &ExecutionControl,
    work: &mut PrefixPoseWork,
) -> Result<InstantiatedRealization, WasmExactSearchError> {
    let occupied = core::array::from_fn(|cell| (board >> cell) & 1);
    let key = |lock: InstantiatedRealization| (lock.rotation, lock.x, lock.lock_y);
    let mut selected = None;
    for lock in locks {
        // Keep the concrete representative's orientation and position order
        // even when the raw ILC catalog iterates alternative deletion frames.
        if selected.is_some_and(|previous| key(previous) <= key(lock))
            || board & lock.lock_mask != 0
            || !placement_is_grounded(catalog.width(), board, lock.lock_mask)
        {
            continue;
        }
        if board & harddrop_path_mask(lock.lock_mask, catalog.width()) == 0 {
            selected = Some(lock);
            continue;
        }
        let lanes = reachability
            .compile_prefix_target_lanes_from_planes(
                catalog,
                piece,
                &occupied,
                1,
                &[lock],
                control,
                work,
            )?
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_placement_dependency_witness_relation_limit",
            ))?;
        if lanes == 1 {
            selected = Some(lock);
        }
    }
    selected.ok_or(WasmExactSearchError::InvalidProblem(
        "wasm_placement_dependency_witness_pose_missing",
    ))
}
