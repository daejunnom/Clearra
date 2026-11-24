use super::*;

struct Frame {
    position: BuildPosition,
    remaining: Vec<StructureOperation>,
    entered: bool,
    operation: usize,
    entry: Option<EntryResult>,
    lock: usize,
}

impl Frame {
    fn new(position: BuildPosition, remaining: Vec<StructureOperation>) -> Self {
        Self {
            position,
            remaining,
            entered: false,
            operation: 0,
            entry: None,
            lock: 0,
        }
    }
    fn key(&self) -> BuildMemoKey {
        BuildMemoKey {
            logical_board: self.position.logical_board,
            deleted_rows: self.position.deleted_rows,
            remaining: self.remaining.clone(),
        }
    }
}

/// An exact continuation of the same ordered DFS used by `verify`. A step
/// visits one memo state, initializes one physically bounded entry graph or
/// examines one lock. It never drains the factorial build-order tree.
pub(crate) struct VerificationCursor {
    target: StructureOperation,
    canonical_operations: Vec<StructureOperation>,
    stack: Vec<Frame>,
    failed: BTreeSet<BuildMemoKey>,
    witness: Vec<StructurePlacement>,
    order: Option<Vec<PieceKind>>,
    result: Option<SpinStructureOutcome>,
    done: bool,
}

impl VerificationCursor {
    pub(crate) fn new(
        query: &SpinStructureQuery,
        operations: &[StructureOperation],
        target: StructureOperation,
        order: Option<Vec<PieceKind>>,
    ) -> Self {
        let target_index = operations.iter().position(|operation| *operation == target);
        let invalid_order = order.as_ref().is_some_and(|order| {
            order.len() != operations.len() || order.last().copied() != Some(target.piece())
        });
        let mut remaining = operations.to_vec();
        if let Some(index) = target_index {
            remaining.remove(index);
        }
        remaining.sort_unstable();
        let logical_board = LogicalBoard::from_initial(query.initial_board);
        let deleted_rows = logical_board.initial_deleted_rows(query.height);
        let position = BuildPosition {
            board: logical_board.compact(deleted_rows, query.height),
            logical_board,
            deleted_rows,
        };
        Self {
            target,
            canonical_operations: operations.to_vec(),
            stack: vec![Frame::new(position, remaining)],
            failed: BTreeSet::new(),
            witness: Vec::with_capacity(operations.len()),
            order,
            result: None,
            done: target_index.is_none() || invalid_order,
        }
    }

    fn pop(&mut self) {
        self.stack.pop();
        if !self.stack.is_empty() {
            self.witness.pop();
        } else {
            self.done = true;
        }
    }

    pub(crate) fn step(
        &mut self,
        query: &SpinStructureQuery,
        verifier: &mut StructuralBuildVerifier,
    ) -> bool {
        if self.done {
            return true;
        }
        let frame = self.stack.last_mut().expect("active verification frame");
        if !frame.entered {
            verifier.metrics.build_states += 1;
            if frame.remaining.is_empty() {
                self.result = verifier.lock_target(
                    query,
                    frame.position,
                    self.target,
                    &self.canonical_operations,
                    &self.witness,
                );
                if self.result.is_some() {
                    self.done = true;
                } else {
                    self.pop();
                }
                return self.done;
            }
            if self.failed.contains(&frame.key()) {
                self.pop();
                return self.done;
            }
            frame.entered = true;
            return false;
        }
        let Some(operation) = frame.remaining.get(frame.operation).copied() else {
            self.failed.insert(frame.key());
            self.pop();
            return self.done;
        };
        let next_piece = self.order.as_ref().map(|order| order[self.witness.len()]);
        if next_piece.is_some_and(|piece| piece != operation.piece())
            || (frame.operation != 0 && frame.remaining[frame.operation - 1] == operation)
        {
            frame.operation += 1;
            return false;
        }
        if frame.entry.is_none() {
            frame.entry = Some(verifier.reachable_locks(
                frame.position.board,
                operation.piece(),
                false,
                false,
            ));
            frame.lock = 0;
            return false;
        }
        let Some(lock) = frame.entry.as_ref().unwrap().locks.get(frame.lock).copied() else {
            frame.operation += 1;
            frame.entry = None;
            return false;
        };
        frame.lock += 1;
        let occupied = frame.position.board.union(lock.mask);
        let (board_after, cleared_rows, cleared_lines) = place_and_clear(query.height, occupied);
        let Some(logical_lock) = apply_physical_lock(
            frame.position.logical_board,
            frame.position.deleted_rows,
            query.height,
            operation.piece(),
            lock.rotation,
            lock.x,
            lock.mask,
        ) else {
            return false;
        };
        if logical_lock.identity != operation {
            return false;
        }
        let mut remaining = frame.remaining.clone();
        remaining.remove(frame.operation);
        let next = Frame::new(
            BuildPosition {
                board: board_after,
                logical_board: logical_lock.board_after,
                deleted_rows: logical_lock.deleted_after,
            },
            remaining,
        );
        self.witness.push(verify::placement_from_lock(
            operation.piece(),
            lock,
            cleared_rows,
            cleared_lines,
        ));
        self.stack.push(next);
        false
    }

    pub(crate) fn finish(self) -> Option<Option<SpinStructureOutcome>> {
        self.done.then_some(self.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PieceInventory, SpinLineRequirement, SpinStructureMode, SpinStructureSearcher};
    #[test]
    fn cooperative_verification_preserves_exact_witness_and_failed_state_metrics() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.line_requirement = SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        assert!(report.outcomes().next().is_some());
        for outcome in report.outcomes() {
            let mut expected = StructuralBuildVerifier::new(&query);
            let mut actual = StructuralBuildVerifier::new(&query);
            let oracle =
                expected.verify(&query, outcome.logical_operations(), outcome.logical_spin());
            let mut cursor = VerificationCursor::new(
                &query,
                outcome.logical_operations(),
                outcome.logical_spin(),
                None,
            );
            while !cursor.step(&query, &mut actual) {}
            assert_eq!(cursor.finish().unwrap(), oracle);
            assert_eq!(actual.metrics(), expected.metrics());
        }
    }
}
