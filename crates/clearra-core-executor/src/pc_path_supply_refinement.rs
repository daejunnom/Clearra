//! Refine a symbolic bag's coarse all-pattern rows once per multiset family.
//! Candidate count never enters the allocation shape or the traversal width.
use std::sync::{Arc, Mutex};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use clearra_supply::{
    hold_automaton::HoldAutomatonState, pattern_universe::ReachableMultisetWorkspace,
};

use super::*;

const WORD_CHUNK: usize = 16;

pub(super) fn checked_future_bytes(problem: &SearchProblem, groups: usize) -> Option<u128> {
    let words = problem
        .piece_source()
        .materialized_universe()?
        .pattern_count()
        .div_ceil(64);
    let workers = if cfg!(target_family = "wasm") {
        1
    } else {
        problem
            .backend_policy()
            .workers()
            .min(words.div_ceil(WORD_CHUNK))
    };
    let row_bytes = (words as u128).checked_mul(core::mem::size_of::<u64>() as u128)?;
    let owner_bytes = (groups as u128).checked_mul(
        core::mem::size_of::<Mutex<Vec<u64>>>() as u128 + 2 * core::mem::size_of::<usize>() as u128,
    )?;
    let worker_bytes = (workers as u128).checked_mul(
        (groups as u128)
            .checked_mul(WORD_CHUNK as u128)?
            .checked_mul(8)?
            .checked_add(core::mem::size_of::<ReachableMultisetWorkspace>() as u128 + 64 + 2048)?,
    )?;
    // Dense Vec -> immutable Arc may coexist during transfer. Three rows per
    // group cover the output, its conversion peak, and all private word tiles.
    row_bytes
        .checked_mul(groups as u128)?
        .checked_mul(3)?
        .checked_add(owner_bytes)?
        .checked_add(worker_bytes)
}

impl PcPathSupplyEvidence {
    pub(crate) fn refine_pattern_membership(
        &mut self,
        problem: &SearchProblem,
        batch: &ExactScoringExecutionBatch,
        control: &ExecutionControl,
    ) -> Result<(), ParallelWorkError<&'static str>> {
        let pattern_count = batch.patterns().len();
        let words = pattern_count.div_ceil(64);
        let placed = self
            .groups
            .first()
            .map(|(key, _)| key.total_count())
            .unwrap_or(0);
        if self.groups.is_empty() || words == 0 {
            return Ok(());
        }
        if self
            .groups
            .iter()
            .any(|(key, _)| key.total_count() != placed)
        {
            return Err(ParallelWorkError::Worker(
                "pc_path_supply_piece_count_mismatch",
            ));
        }
        let mut rows = Vec::new();
        rows.try_reserve_exact(self.groups.len())
            .map_err(|_| allocation_error())?;
        for _ in &self.groups {
            let mut row = Vec::new();
            row.try_reserve_exact(words)
                .map_err(|_| allocation_error())?;
            row.resize(words, 0_u64);
            rows.push(Mutex::new(row));
        }
        let terminal_release = problem.supply().projects_unplaced_lookahead()
            && !problem.supply().projects_standard_bag_lookahead();
        let original_hold = problem.initial_hold();
        let held = terminal_release
            .then(|| original_hold.hold_piece())
            .flatten();
        let (count, hold, hold_enabled) =
            if terminal_release {
                let count = placed.checked_sub(u8::from(held.is_some())).ok_or(
                    ParallelWorkError::Worker("pc_path_supply_piece_count_mismatch"),
                )?;
                (
                    count,
                    HoldAutomatonState::new(
                        original_hold.piece_source_id(),
                        original_hold.cursor(),
                        None,
                        original_hold.bag_epoch(),
                        original_hold.bag_remainder_key(),
                        original_hold.provenance(),
                    ),
                    false,
                )
            } else {
                (placed, original_hold, problem.supply().hold_enabled())
            };
        let groups = &self.groups;
        try_for_each_chunk(
            control,
            words,
            WORD_CHUNK,
            || {
                let mut tile = Vec::new();
                let len = groups
                    .len()
                    .checked_mul(WORD_CHUNK)
                    .ok_or("pc_path_supply_projection_overflow")?;
                tile.try_reserve_exact(len)
                    .map_err(|_| "pc_path_supply_storage_unavailable")?;
                tile.resize(len, 0_u64);
                Ok((ReachableMultisetWorkspace::for_placed_pieces(count), tile))
            },
            |(workspace, tile), range| {
                tile.fill(0);
                for pattern in range.start * 64..range.end.saturating_mul(64).min(pattern_count) {
                    for key in workspace.for_sequence(
                        &batch.patterns()[pattern],
                        count,
                        hold,
                        hold_enabled,
                    ) {
                        let key = if let Some(piece) = held {
                            let mut counts = key.counts();
                            let index = clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
                                .iter().position(|candidate| *candidate == piece).expect("standard piece");
                            counts[index] += 1;
                            PieceMultisetKey::from_counts(counts)
                        } else {
                            *key
                        };
                        if let Ok(group) = groups.binary_search_by_key(&key, |(key, _)| *key) {
                            tile[group * WORD_CHUNK + pattern / 64 - range.start] |=
                                1_u64 << (pattern % 64);
                        }
                    }
                }
                for (group, row) in rows.iter().enumerate() {
                    let mut row = row.lock().map_err(|_| "pc_path_supply_worker_failed")?;
                    for word in range.clone() {
                        row[word] = tile[group * WORD_CHUNK + word - range.start]
                            & groups[group].1.word_at(word);
                    }
                }
                Ok(())
            },
        )?;
        for (group, row) in self.groups.iter_mut().zip(rows) {
            let row = row
                .into_inner()
                .map_err(|_| ParallelWorkError::Worker("pc_path_supply_worker_failed"))?;
            group.1 = PatternBitSet::from_shared_words(pattern_count, Arc::from(row))
                .map_err(|_| ParallelWorkError::Worker("pc_path_supply_universe_mismatch"))?;
        }
        Ok(())
    }
}

fn allocation_error() -> ParallelWorkError<&'static str> {
    ParallelWorkError::Worker("pc_path_supply_storage_unavailable")
}
