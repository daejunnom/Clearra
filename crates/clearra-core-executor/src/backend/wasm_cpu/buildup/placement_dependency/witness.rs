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
    reachability::{harddrop_path_mask, PrefixPoseWork, ReachabilityWorkspace},
    symbolic_prefix_domain::MAX_OPERATIONS,
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

#[derive(Clone, Copy)]
struct WitnessChoice {
    lock: LockChoice,
    next: CoverageState,
    supply_priority: u8,
}

struct PatternWitness<'a> {
    orders: &'a SymbolicPrefixOrders,
    problem: &'a SearchProblem,
    sequence: &'a [PieceKind],
    control: &'a ExecutionControl,
    supply_first: bool,
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

        // The ordinary graph representative orders operations before hold
        // choices. The former fixed-source shortcut orders Current before
        // Hold across all operations. Generate both from the same existing
        // supply transitions, changing only that bounded choice ordering.
        let mut choices = [None; MAX_OPERATIONS * 3];
        let mut choice_count = 0;
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
                choices[choice_count] = Some(WitnessChoice {
                    lock: LockChoice {
                        operation,
                        piece,
                        hold: "release-held-at-terminal",
                    },
                    next: CoverageState {
                        node: u32::from(next),
                        hold: None,
                        ..state
                    },
                    supply_priority: 0,
                });
                choice_count += 1;
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
                choices[choice_count] = Some(WitnessChoice {
                    lock: LockChoice {
                        operation,
                        piece,
                        hold,
                    },
                    next: CoverageState {
                        node: u32::from(next),
                        ..supply_state
                    },
                    supply_priority: u8::from(!matches!(
                        hold,
                        "use-current" | "use-unplaced-lookahead"
                    )),
                });
                choice_count += 1;
            }
        }
        if self.supply_first {
            choices[..choice_count].sort_unstable_by_key(|choice| {
                let choice = choice.expect("populated witness choice");
                (choice.supply_priority, choice.lock.operation)
            });
        }
        for choice in choices[..choice_count].iter().flatten().copied() {
            self.choices.push(choice.lock);
            if self.select_locks(choice.next)? {
                return Ok(true);
            }
            self.choices.pop();
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
    supply_first: bool,
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
        // The old fixed shortcut excluded inferred standard-bag current
        // pieces; those requests retain ordinary operation-first ordering.
        supply_first: supply_first && !problem.supply().projects_standard_bag_lookahead(),
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

    let geometry = orders.geometry();
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

#[cfg(test)]
mod tests {
    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
        SupplyWindowSize,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::builtin_rules::srs_plus;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;
    use crate::backend::wasm_cpu::symbolic_prefix_orders::SymbolicPrefixWork;

    #[test]
    fn fixed_witness_retains_supply_first_order_without_changing_graph_order() {
        let i = 0xf;
        let o = (3 << 6) | (3 << 16);
        let target = i | o;
        let catalog = GeometryCatalog::compile_build_stage(2, 0, target).unwrap();
        let rows = [
            catalog.skeleton_id(PieceKind::I, i).unwrap(),
            catalog.skeleton_id(PieceKind::O, o).unwrap(),
        ];
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        let held = catalog.skeleton(candidate.row_ids()[0]).piece;
        let current = catalog.skeleton(candidate.row_ids()[1]).piece;
        assert_ne!(held, current);
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, ((1 << 20) - 1) & !target),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![current, held])),
                PieceWindow::new(2),
            )
            .with_rule(srs_plus())
            .with_allow_hold(true)
            .with_hold_piece(Some(held))
            .with_exact_pieces(Some(2))
            .with_supply_window_size(SupplyWindowSize::new(2))
            .with_count_policy(PcCountPolicy::CountUnique),
        )
        .unwrap();
        let control = ExecutionControl::default();
        let mut reachability = ReachabilityWorkspace::default();
        let mut work = SymbolicPrefixWork::default();
        let orders = SymbolicPrefixOrders::compile_product_completion_with_workspace(
            &catalog,
            &candidate,
            problem.kick_profile().profile_id(),
            &control,
            &mut reachability,
            &mut work,
            0,
            target,
        )
        .unwrap();
        assert_eq!(orders.live_operations_after(0).count(), 2);

        let mut trace = |supply_first| {
            materialize(
                &problem,
                &catalog,
                &candidate,
                &orders,
                0,
                supply_first,
                &mut reachability,
                &control,
                &mut work.pose,
            )
            .unwrap()
        };
        let fixed = trace(true);
        let ordinary = trace(false);
        assert_eq!(fixed.len(), 2);
        assert_eq!(ordinary.len(), 2);
        assert_eq!(
            (fixed[0].piece(), fixed[0].hold()),
            (current, "use-current")
        );
        assert_eq!((fixed[1].piece(), fixed[1].hold()), (held, "use-current"));
        assert_eq!(
            (ordinary[0].piece(), ordinary[0].hold()),
            (held, "swap-held")
        );
        assert_eq!(
            (ordinary[1].piece(), ordinary[1].hold()),
            (current, "swap-held")
        );
    }
}
