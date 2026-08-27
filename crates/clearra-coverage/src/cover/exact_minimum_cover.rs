use crate::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverResult {
    row_indices: Vec<usize>,
    covered_patterns: PatternBitSet,
    complete: bool,
}

impl ExactMinimumCoverResult {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn into_parts(self) -> (Vec<usize>, PatternBitSet, bool) {
        (self.row_indices, self.covered_patterns, self.complete)
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.row_indices.capacity() as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)?
            .checked_add(self.covered_patterns.checked_storage_retained_bytes()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverError {
    RowPatternCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    MemoryGuardRejected,
    AllocationFailed {
        component: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactMinimumCoverMemoryProjection {
    pub memo_state_upper_bound: u128,
    pub memo_state_bytes_upper_bound: u128,
    pub fixed_workspace_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Debug)]
struct DenseRow {
    source_index: usize,
    words: Vec<u64>,
}

pub fn exact_minimum_cover(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    exact_minimum_cover_with_memory_guard(required, rows, &mut |_| Ok(()))
}

/// Returns the maximum number of distinct covered states representable by
/// `row_count` unions over `required_bit_count` required bits.
///
/// `None` is deliberately fail-closed: it means that even the state count
/// cannot be represented in the projection's `u128` accounting domain.
pub fn checked_exact_minimum_cover_state_upper_bound(
    required_bit_count: usize,
    row_count: usize,
) -> Option<u128> {
    let exponent = required_bit_count.min(row_count);
    1_u128.checked_shl(u32::try_from(exponent).ok()?)
}

/// Computes a sound, input-size-only upper bound for the exact solver's
/// requested heap capacities. The guarded solver below grows the exponential
/// memo incrementally and checks allocator-returned capacities after every
/// reserve, so callers do not need to reserve this worst case merely to
/// execute a small realized search.
pub fn checked_exact_minimum_cover_memory_projection(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Option<ExactMinimumCoverMemoryProjection> {
    if rows
        .iter()
        .any(|row| row.pattern_count() != required.pattern_count())
    {
        return None;
    }
    let row_count = rows.len() as u128;
    let word_count = required.word_count() as u128;
    let word_bytes = word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
    let required_bit_count = required.count_ones() as usize;
    let memo_state_upper_bound =
        checked_exact_minimum_cover_state_upper_bound(required_bit_count, rows.len())?;
    let memo_state_bytes_upper_bound = memo_state_upper_bound.checked_mul(
        (core::mem::size_of::<(Vec<u64>, usize)>() as u128).checked_add(word_bytes)?,
    )?;

    let dense_rows = row_count
        .checked_mul(core::mem::size_of::<DenseRow>() as u128)?
        .checked_add(row_count.checked_mul(word_bytes)?)?;
    let pattern_slots = word_count.checked_mul(u64::BITS as u128)?;
    let support_slots = pattern_slots
        .checked_mul(core::mem::size_of::<Vec<usize>>() as u128)?
        .checked_add(
            row_count
                .checked_mul(required_bit_count as u128)?
                .checked_mul(core::mem::size_of::<usize>() as u128)?,
        )?;
    let row_index_scratch = row_count
        .checked_mul(core::mem::size_of::<usize>() as u128)?
        .checked_mul(7)?;
    let recursive_branch_scratch = row_count
        .checked_mul(row_count)?
        .checked_mul(core::mem::size_of::<usize>() as u128)?;
    let selected_scratch = row_count
        .checked_mul(core::mem::size_of::<bool>() as u128)?
        .checked_mul(3)?;
    let recursive_changed_scratch = row_count
        .checked_mul(word_count)?
        .checked_mul(core::mem::size_of::<(usize, u64)>() as u128)?;
    let fixed_workspace_bytes = dense_rows
        .checked_add(word_bytes.checked_mul(7)?)?
        .checked_add(support_slots)?
        .checked_add(row_index_scratch)?
        .checked_add(recursive_branch_scratch)?
        .checked_add(selected_scratch)?
        .checked_add(recursive_changed_scratch)?
        .checked_add(PatternBitSet::checked_shared_construction_upper_bound(
            required.pattern_count(),
            1,
            required_bit_count as u128,
        )?)?;
    let required_peak_bytes = fixed_workspace_bytes.checked_add(memo_state_bytes_upper_bound)?;
    Some(ExactMinimumCoverMemoryProjection {
        memo_state_upper_bound,
        memo_state_bytes_upper_bound,
        fixed_workspace_bytes,
        required_peak_bytes,
    })
}

/// Runs the exact solver while reporting its complete currently-owned heap plus
/// the next requested allocation before each allocation and its actual
/// capacity immediately afterwards. The caller owns all external-live memory
/// accounting and may reject any reported peak.
pub fn exact_minimum_cover_with_memory_guard(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    memory_guard(0)?;
    for (row_index, row) in rows.iter().enumerate() {
        if row.pattern_count() != required.pattern_count() {
            return Err(ExactMinimumCoverError::RowPatternCountMismatch {
                row_index,
                expected: required.pattern_count(),
                actual: row.pattern_count(),
            });
        }
    }
    if required.is_empty() {
        return Ok(ExactMinimumCoverResult {
            row_indices: Vec::new(),
            covered_patterns: PatternBitSet::new(required.pattern_count()),
            complete: true,
        });
    }

    let mut dense_rows = try_vec_with_capacity(
        rows.len(),
        0,
        memory_guard,
        "exact_minimum_cover_dense_rows",
    )?;
    for (source_index, row) in rows.iter().enumerate() {
        let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
        let mut words = try_vec_with_capacity(
            required.word_count(),
            dense_live,
            memory_guard,
            "exact_minimum_cover_dense_row_words",
        )?;
        let mut nonempty = false;
        for word_index in 0..required.word_count() {
            let word = row.word_at(word_index) & required.word_at(word_index);
            nonempty |= word != 0;
            words.push(word);
        }
        if nonempty {
            dense_rows.push(DenseRow {
                source_index,
                words,
            });
        }
        memory_guard(checked_dense_rows_retained_bytes(&dense_rows)?)?;
    }
    remove_dominated_rows_with_memory_guard(&mut dense_rows, memory_guard)?;

    let dense_live = checked_dense_rows_retained_bytes(&dense_rows)?;
    let mut coverable_words = try_vec_with_capacity(
        required.word_count(),
        dense_live,
        memory_guard,
        "exact_minimum_cover_coverable_words",
    )?;
    coverable_words.resize(required.word_count(), 0);
    for row in &dense_rows {
        union_words(&mut coverable_words, &row.words);
    }
    let complete = (0..required.word_count())
        .all(|index| coverable_words[index] & required.word_at(index) == required.word_at(index));
    let coverable_live = checked_vec_retained_bytes(&coverable_words)?;
    let mut target_words = try_vec_with_capacity(
        required.word_count(),
        dense_live
            .checked_add(coverable_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_target_words",
    )?;
    for (word_index, coverable) in coverable_words.iter().copied().enumerate() {
        target_words.push(required.word_at(word_index) & coverable);
    }
    drop(coverable_words);
    memory_guard(
        dense_live
            .checked_add(checked_vec_retained_bytes(&target_words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let selected_rows = if target_words.iter().all(|word| *word == 0) {
        Vec::new()
    } else {
        MinimumCoverSearch::try_new(&dense_rows, target_words, dense_live, memory_guard)?
            .solve(dense_live, memory_guard)?
    };

    let selected_live = checked_vec_retained_bytes(&selected_rows)?;
    let mut covered_words = try_vec_with_capacity(
        required.word_count(),
        dense_live
            .checked_add(selected_live)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_result_words",
    )?;
    covered_words.resize(required.word_count(), 0);
    let mut row_indices = try_vec_with_capacity(
        selected_rows.len(),
        dense_live
            .checked_add(selected_live)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&covered_words).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_result_indices",
    )?;
    for row_index in selected_rows {
        union_words(&mut covered_words, &dense_rows[row_index].words);
        row_indices.push(dense_rows[row_index].source_index);
    }
    row_indices.sort_unstable();

    drop(dense_rows);
    let result_live = checked_vec_retained_bytes(&row_indices)?
        .checked_add(checked_vec_retained_bytes(&covered_words)?)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let construction_future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
        required.pattern_count(),
    )
    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        result_live
            .checked_add(construction_future)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let covered_patterns = PatternBitSet::from_words(required.pattern_count(), covered_words)
        .expect("minimum-cover words preserve the required pattern universe");
    memory_guard(
        checked_vec_retained_bytes(&row_indices)?
            .checked_add(
                covered_patterns
                    .checked_storage_retained_bytes()
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;

    Ok(ExactMinimumCoverResult {
        row_indices,
        covered_patterns,
        complete,
    })
}

pub fn exact_minimum_cover_with_memory_limit(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<ExactMinimumCoverResult, ExactMinimumCoverError> {
    exact_minimum_cover_with_memory_guard(required, rows, &mut |solver_owned_bytes| {
        let required_memory_bytes = already_retained_bytes
            .checked_add(solver_owned_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        Ok(())
    })
}

fn remove_dominated_rows_with_memory_guard(
    rows: &mut Vec<DenseRow>,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<(), ExactMinimumCoverError> {
    let rows_live = checked_dense_rows_retained_bytes(rows)?;
    let mut dominated = try_vec_with_capacity(
        rows.len(),
        rows_live,
        memory_guard,
        "exact_minimum_cover_dominated_rows",
    )?;
    dominated.resize(rows.len(), false);
    for left in 0..rows.len() {
        if dominated[left] {
            continue;
        }
        for right in 0..rows.len() {
            if left == right || dominated[left] {
                continue;
            }
            if is_superset(&rows[right].words, &rows[left].words) {
                let equal = rows[right].words == rows[left].words;
                if !equal || rows[right].source_index < rows[left].source_index {
                    dominated[left] = true;
                }
            }
        }
    }
    let mut index = 0;
    rows.retain(|_| {
        let keep = !dominated[index];
        index += 1;
        keep
    });
    drop(dominated);
    memory_guard(checked_dense_rows_retained_bytes(rows)?)?;
    Ok(())
}

struct MinimumCoverSearch<'a> {
    rows: &'a [DenseRow],
    target_words: Vec<u64>,
    support_by_pattern: Vec<Vec<usize>>,
    selected: Vec<bool>,
    current: Vec<usize>,
    best: Vec<usize>,
    memo_depth: Vec<(Vec<u64>, usize)>,
}

impl<'a> MinimumCoverSearch<'a> {
    fn try_new(
        rows: &'a [DenseRow],
        target_words: Vec<u64>,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, ExactMinimumCoverError> {
        let target_live = checked_vec_retained_bytes(&target_words)?;
        let best = greedy_cover_with_memory_guard(
            rows,
            &target_words,
            base_live_bytes
                .checked_add(target_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
        )?
        .unwrap_or_else(Vec::new);
        let best_live = checked_vec_retained_bytes(&best)?;
        let pattern_count = target_words
            .len()
            .checked_mul(u64::BITS as usize)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let persistent_live = base_live_bytes
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(best_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut support_by_pattern = try_vec_with_capacity(
            pattern_count,
            persistent_live,
            memory_guard,
            "exact_minimum_cover_support_slots",
        )?;
        support_by_pattern.resize_with(pattern_count, Vec::new);
        for pattern in 0..pattern_count {
            let word_index = pattern / u64::BITS as usize;
            let bit = pattern % u64::BITS as usize;
            let support_count = rows
                .iter()
                .filter(|row| row.words[word_index] & (1_u64 << bit) != 0)
                .count();
            if support_count == 0 {
                continue;
            }
            let support_live = checked_support_retained_bytes(&support_by_pattern)?;
            let mut support = try_vec_with_capacity(
                support_count,
                base_live_bytes
                    .checked_add(target_live)
                    .and_then(|bytes| bytes.checked_add(best_live))
                    .and_then(|bytes| bytes.checked_add(support_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                "exact_minimum_cover_support_rows",
            )?;
            for (row_index, row) in rows.iter().enumerate() {
                if row.words[word_index] & (1_u64 << bit) != 0 {
                    support.push(row_index);
                }
            }
            support_by_pattern[pattern] = support;
            memory_guard(
                base_live_bytes
                    .checked_add(target_live)
                    .and_then(|bytes| bytes.checked_add(best_live))
                    .and_then(|bytes| {
                        bytes.checked_add(checked_support_retained_bytes(&support_by_pattern).ok()?)
                    })
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )?;
        }
        let support_live = checked_support_retained_bytes(&support_by_pattern)?;
        let construction_live = base_live_bytes
            .checked_add(target_live)
            .and_then(|bytes| bytes.checked_add(best_live))
            .and_then(|bytes| bytes.checked_add(support_live))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        let mut selected = try_vec_with_capacity(
            rows.len(),
            construction_live,
            memory_guard,
            "exact_minimum_cover_selected_flags",
        )?;
        selected.resize(rows.len(), false);
        let selected_live = checked_vec_retained_bytes(&selected)?;
        let current = try_vec_with_capacity(
            rows.len(),
            construction_live
                .checked_add(selected_live)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_current_rows",
        )?;
        let search = Self {
            rows,
            target_words,
            support_by_pattern,
            selected,
            current,
            best,
            memo_depth: Vec::new(),
        };
        memory_guard(
            base_live_bytes
                .checked_add(search.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(search)
    }

    fn solve(
        mut self,
        base_live_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Vec<usize>, ExactMinimumCoverError> {
        let mut covered = try_vec_with_capacity(
            self.target_words.len(),
            base_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_search_covered",
        )?;
        covered.resize(self.target_words.len(), 0);
        let covered_live = checked_vec_retained_bytes(&covered)?;
        self.search(&mut covered, base_live_bytes, covered_live, memory_guard)?;
        self.best
            .sort_unstable_by_key(|index| self.rows[*index].source_index);
        let best = core::mem::take(&mut self.best);
        drop(covered);
        drop(self);
        memory_guard(
            base_live_bytes
                .checked_add(checked_vec_retained_bytes(&best)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        )?;
        Ok(best)
    }

    fn search(
        &mut self,
        covered: &mut [u64],
        base_live_bytes: u128,
        recursive_scratch_bytes: u128,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<(), ExactMinimumCoverError> {
        if is_superset(covered, &self.target_words) {
            if self.best.is_empty() || self.current.len() < self.best.len() {
                self.best.clone_from(&self.current);
            }
            return Ok(());
        }
        if !self.best.is_empty() && self.current.len() >= self.best.len() {
            return Ok(());
        }
        match self
            .memo_depth
            .binary_search_by(|(state, _)| state.as_slice().cmp(covered))
        {
            Ok(index) if self.memo_depth[index].1 <= self.current.len() => return Ok(()),
            Ok(index) => self.memo_depth[index].1 = self.current.len(),
            Err(index) => {
                let persistent_live = self.checked_heap_retained_bytes()?;
                let mut state = try_vec_with_capacity(
                    covered.len(),
                    base_live_bytes
                        .checked_add(persistent_live)
                        .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    memory_guard,
                    "exact_minimum_cover_memo_state",
                )?;
                state.extend_from_slice(covered);
                let state_live = checked_vec_retained_bytes(&state)?;
                let outer_before = checked_vec_retained_bytes(&self.memo_depth)?;
                let growth = checked_requested_growth_bytes(&self.memo_depth, 1)?;
                memory_guard(
                    base_live_bytes
                        .checked_add(persistent_live)
                        .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                        .and_then(|bytes| bytes.checked_add(state_live))
                        .and_then(|bytes| bytes.checked_add(growth))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )?;
                self.memo_depth.try_reserve_exact(1).map_err(|_| {
                    ExactMinimumCoverError::AllocationFailed {
                        component: "exact_minimum_cover_memo_entries",
                    }
                })?;
                let outer_after = checked_vec_retained_bytes(&self.memo_depth)?;
                let non_outer_persistent = persistent_live
                    .checked_sub(outer_before)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                memory_guard(
                    base_live_bytes
                        .checked_add(non_outer_persistent)
                        .and_then(|bytes| bytes.checked_add(outer_after))
                        .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                        .and_then(|bytes| bytes.checked_add(state_live))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )?;
                self.memo_depth.insert(index, (state, self.current.len()));
                memory_guard(
                    base_live_bytes
                        .checked_add(self.checked_heap_retained_bytes()?)
                        .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                )?;
            }
        }

        let Some((pivot, support)) = self.rarest_uncovered_pattern(covered) else {
            return Ok(());
        };
        if support == 0 {
            return Ok(());
        }
        let max_gain = self
            .rows
            .iter()
            .enumerate()
            .filter(|(index, _)| !self.selected[*index])
            .map(|(_, row)| uncovered_gain(&row.words, covered, &self.target_words))
            .max()
            .unwrap_or(0);
        if max_gain == 0 {
            return Ok(());
        }
        let lower_bound = uncovered_count(covered, &self.target_words).div_ceil(max_gain);
        if !self.best.is_empty() && self.current.len() + lower_bound >= self.best.len() {
            return Ok(());
        }

        let branch_count = self.support_by_pattern[pivot]
            .iter()
            .filter(|index| !self.selected[**index])
            .count();
        let mut branches = try_vec_with_capacity(
            branch_count,
            base_live_bytes
                .checked_add(self.checked_heap_retained_bytes()?)
                .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            memory_guard,
            "exact_minimum_cover_branches",
        )?;
        branches.extend(
            self.support_by_pattern[pivot]
                .iter()
                .copied()
                .filter(|index| !self.selected[*index]),
        );
        branches.sort_unstable_by(|left, right| {
            uncovered_gain(&self.rows[*right].words, covered, &self.target_words)
                .cmp(&uncovered_gain(
                    &self.rows[*left].words,
                    covered,
                    &self.target_words,
                ))
                .then_with(|| {
                    self.rows[*left]
                        .source_index
                        .cmp(&self.rows[*right].source_index)
                })
        });
        let branches_live = checked_vec_retained_bytes(&branches)?;

        for row_index in branches {
            self.selected[row_index] = true;
            self.current.push(row_index);
            let changed_count = self.rows[row_index]
                .words
                .iter()
                .copied()
                .zip(covered.iter().copied())
                .filter(|(row_word, covered_word)| *covered_word | *row_word != *covered_word)
                .count();
            let mut changed = try_vec_with_capacity(
                changed_count,
                base_live_bytes
                    .checked_add(self.checked_heap_retained_bytes()?)
                    .and_then(|bytes| bytes.checked_add(recursive_scratch_bytes))
                    .and_then(|bytes| bytes.checked_add(branches_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
                "exact_minimum_cover_changed_words",
            )?;
            for (word_index, row_word) in self.rows[row_index].words.iter().copied().enumerate() {
                let next = covered[word_index] | row_word;
                if next != covered[word_index] {
                    changed.push((word_index, covered[word_index]));
                    covered[word_index] = next;
                }
            }
            let changed_live = checked_vec_retained_bytes(&changed)?;
            self.search(
                covered,
                base_live_bytes,
                recursive_scratch_bytes
                    .checked_add(branches_live)
                    .and_then(|bytes| bytes.checked_add(changed_live))
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                memory_guard,
            )?;
            for (word_index, previous) in changed {
                covered[word_index] = previous;
            }
            self.current.pop();
            self.selected[row_index] = false;
        }
        Ok(())
    }

    fn checked_heap_retained_bytes(&self) -> Result<u128, ExactMinimumCoverError> {
        let mut bytes = checked_vec_retained_bytes(&self.target_words)?
            .checked_add(checked_support_retained_bytes(&self.support_by_pattern)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.selected).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.current).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.best).ok()?))
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&self.memo_depth).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        for (state, _) in &self.memo_depth {
            bytes = bytes
                .checked_add(checked_vec_retained_bytes(state)?)
                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        }
        Ok(bytes)
    }

    fn rarest_uncovered_pattern(&self, covered: &[u64]) -> Option<(usize, usize)> {
        let mut rarest = None;
        for (word_index, (target, covered)) in self.target_words.iter().zip(covered).enumerate() {
            let mut remaining = target & !covered;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let pattern = word_index * u64::BITS as usize + bit;
                let support = self.support_by_pattern[pattern].len();
                if rarest.is_none_or(|(_, current)| support < current) {
                    rarest = Some((pattern, support));
                    if support <= 1 {
                        return rarest;
                    }
                }
                remaining &= remaining - 1;
            }
        }
        rarest
    }
}

fn greedy_cover_with_memory_guard(
    rows: &[DenseRow],
    target: &[u64],
    base_live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Option<Vec<usize>>, ExactMinimumCoverError> {
    let mut covered = try_vec_with_capacity(
        target.len(),
        base_live_bytes,
        memory_guard,
        "exact_minimum_cover_greedy_covered",
    )?;
    covered.resize(target.len(), 0);
    let mut selected = try_vec_with_capacity(
        rows.len(),
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&covered)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_greedy_selected",
    )?;
    selected.resize(rows.len(), false);
    let mut result = try_vec_with_capacity(
        rows.len(),
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&covered)?)
            .and_then(|bytes| bytes.checked_add(checked_vec_retained_bytes(&selected).ok()?))
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
        memory_guard,
        "exact_minimum_cover_greedy_result",
    )?;
    while !is_superset(&covered, target) {
        let next = rows
            .iter()
            .enumerate()
            .filter(|(index, _)| !selected[*index])
            .map(|(index, row)| (uncovered_gain(&row.words, &covered, target), index))
            .filter(|(gain, _)| *gain > 0)
            .max_by(|(left_gain, left), (right_gain, right)| {
                left_gain
                    .cmp(right_gain)
                    .then_with(|| rows[*right].source_index.cmp(&rows[*left].source_index))
            })
            .map(|(_, index)| index);
        let Some(next) = next else {
            return Ok(None);
        };
        selected[next] = true;
        result.push(next);
        union_words(&mut covered, &rows[next].words);
    }
    drop(covered);
    drop(selected);
    memory_guard(
        base_live_bytes
            .checked_add(checked_vec_retained_bytes(&result)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(Some(result))
}

fn try_vec_with_capacity<T>(
    capacity: usize,
    live_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    component: &'static str,
) -> Result<Vec<T>, ExactMinimumCoverError> {
    let requested_bytes = (capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    memory_guard(
        live_bytes
            .checked_add(requested_bytes)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ExactMinimumCoverError::AllocationFailed { component })?;
    memory_guard(
        live_bytes
            .checked_add(checked_vec_retained_bytes(&values)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
    )?;
    Ok(values)
}

fn checked_requested_growth_bytes<T>(
    values: &Vec<T>,
    additional: usize,
) -> Result<u128, ExactMinimumCoverError> {
    let requested_capacity = values
        .len()
        .checked_add(additional)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let additional_capacity = requested_capacity.saturating_sub(values.capacity());
    (additional_capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_vec_retained_bytes<T>(values: &Vec<T>) -> Result<u128, ExactMinimumCoverError> {
    (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)
}

fn checked_dense_rows_retained_bytes(rows: &Vec<DenseRow>) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(rows)?;
    for row in rows {
        bytes = bytes
            .checked_add(checked_vec_retained_bytes(&row.words)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn checked_support_retained_bytes(
    support_by_pattern: &Vec<Vec<usize>>,
) -> Result<u128, ExactMinimumCoverError> {
    let mut bytes = checked_vec_retained_bytes(support_by_pattern)?;
    for support in support_by_pattern {
        bytes = bytes
            .checked_add(checked_vec_retained_bytes(support)?)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    }
    Ok(bytes)
}

fn uncovered_gain(row: &[u64], covered: &[u64], target: &[u64]) -> usize {
    row.iter()
        .zip(covered)
        .zip(target)
        .map(|((row, covered), target)| (row & target & !covered).count_ones() as usize)
        .sum()
}

fn uncovered_count(covered: &[u64], target: &[u64]) -> usize {
    covered
        .iter()
        .zip(target)
        .map(|(covered, target)| (target & !covered).count_ones() as usize)
        .sum()
}

fn union_words(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
}

fn is_superset(covered: &[u64], required: &[u64]) -> bool {
    covered
        .iter()
        .zip(required)
        .all(|(covered, required)| covered & required == *required)
}

#[cfg(test)]
mod tests {
    use crate::pattern::pattern_id::PatternId;

    use super::*;

    fn exact_fixture() -> (PatternBitSet, Vec<PatternBitSet>) {
        let required = PatternBitSet::from_patterns(
            4,
            [
                PatternId::new(0),
                PatternId::new(1),
                PatternId::new(2),
                PatternId::new(3),
            ],
        )
        .expect("required");
        let rows = vec![
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)]).expect("row 0"),
            PatternBitSet::from_patterns(4, [PatternId::new(2), PatternId::new(3)]).expect("row 1"),
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)]).expect("row 2"),
            PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(3)]).expect("row 3"),
        ];
        (required, rows)
    }

    #[test]
    fn guarded_exact_solver_accepts_exact_observed_peak_and_rejects_peak_minus_one() {
        let (required, rows) = exact_fixture();
        let mut peak = 0_u128;
        let expected =
            exact_minimum_cover_with_memory_guard(&required, &rows, &mut |owned_bytes| {
                peak = peak.max(owned_bytes);
                Ok(())
            })
            .expect("dry run");
        assert!(peak > 0);

        let already_retained_bytes = 37_u128;
        let exact_cap = already_retained_bytes.checked_add(peak).expect("cap");
        let exact = exact_minimum_cover_with_memory_limit(
            &required,
            &rows,
            already_retained_bytes,
            exact_cap,
        )
        .expect("exact observed cap");
        assert_eq!(exact, expected);

        assert!(matches!(
            exact_minimum_cover_with_memory_limit(
                &required,
                &rows,
                already_retained_bytes,
                exact_cap - 1,
            ),
            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            }) if required_memory_bytes > max_memory_bytes
        ));
    }

    #[test]
    fn guarded_exact_solver_noop_reports_zero_future_only() {
        let required = PatternBitSet::new(0);
        let mut observed = Vec::new();
        let result = exact_minimum_cover_with_memory_guard(&required, &[], &mut |owned_bytes| {
            observed.push(owned_bytes);
            Ok(())
        })
        .expect("empty exact cover");
        assert!(result.complete());
        assert_eq!(observed, vec![0]);
    }

    #[test]
    fn state_projection_and_external_addition_overflow_fail_closed() {
        assert_eq!(
            checked_exact_minimum_cover_state_upper_bound(4, 9),
            Some(16)
        );
        assert_eq!(
            checked_exact_minimum_cover_state_upper_bound(u128::BITS as usize, usize::MAX),
            None
        );

        let (required, rows) = exact_fixture();
        assert_eq!(
            exact_minimum_cover_with_memory_limit(&required, &rows, u128::MAX, u128::MAX),
            Err(ExactMinimumCoverError::ProjectionOverflow)
        );
    }

    #[test]
    fn checked_projection_includes_every_reachable_memo_state_request() {
        let (required, rows) = exact_fixture();
        let projection = checked_exact_minimum_cover_memory_projection(&required, &rows)
            .expect("representable projection");
        assert_eq!(projection.memo_state_upper_bound, 16);
        assert!(projection.memo_state_bytes_upper_bound > 0);
        assert_eq!(
            projection.required_peak_bytes,
            projection
                .fixed_workspace_bytes
                .checked_add(projection.memo_state_bytes_upper_bound)
                .expect("projection sum")
        );
    }
}
