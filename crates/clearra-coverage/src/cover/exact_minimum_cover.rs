use std::collections::HashMap;

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactMinimumCoverError {
    RowPatternCountMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
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

    let required_words = required.words();
    let mut dense_rows = rows
        .iter()
        .enumerate()
        .filter_map(|(source_index, row)| {
            let words = row
                .words()
                .iter()
                .zip(required_words)
                .map(|(row, required)| row & required)
                .collect::<Vec<_>>();
            words.iter().any(|word| *word != 0).then_some(DenseRow {
                source_index,
                words,
            })
        })
        .collect::<Vec<_>>();
    remove_dominated_rows(&mut dense_rows);

    let mut coverable_words = vec![0_u64; required.word_count()];
    for row in &dense_rows {
        union_words(&mut coverable_words, &row.words);
    }
    let complete = is_superset(&coverable_words, required_words);
    let target_words = required_words
        .iter()
        .zip(&coverable_words)
        .map(|(required, coverable)| required & coverable)
        .collect::<Vec<_>>();
    let selected_rows = if target_words.iter().all(|word| *word == 0) {
        Vec::new()
    } else {
        MinimumCoverSearch::new(&dense_rows, target_words).solve()
    };

    let mut covered_words = vec![0_u64; required.word_count()];
    let mut row_indices = selected_rows
        .into_iter()
        .map(|row_index| {
            union_words(&mut covered_words, &dense_rows[row_index].words);
            dense_rows[row_index].source_index
        })
        .collect::<Vec<_>>();
    row_indices.sort_unstable();

    Ok(ExactMinimumCoverResult {
        row_indices,
        covered_patterns: PatternBitSet::from_words(required.pattern_count(), covered_words)
            .expect("minimum-cover words preserve the required pattern universe"),
        complete,
    })
}

fn remove_dominated_rows(rows: &mut Vec<DenseRow>) {
    let mut dominated = vec![false; rows.len()];
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
}

struct MinimumCoverSearch<'a> {
    rows: &'a [DenseRow],
    target_words: Vec<u64>,
    support_by_pattern: Vec<Vec<usize>>,
    selected: Vec<bool>,
    current: Vec<usize>,
    best: Vec<usize>,
    memo_depth: HashMap<Vec<u64>, usize>,
}

impl<'a> MinimumCoverSearch<'a> {
    fn new(rows: &'a [DenseRow], target_words: Vec<u64>) -> Self {
        let pattern_count = target_words.len() * u64::BITS as usize;
        let mut support_by_pattern = vec![Vec::new(); pattern_count];
        for (row_index, row) in rows.iter().enumerate() {
            for (word_index, word) in row.words.iter().copied().enumerate() {
                let mut remaining = word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    support_by_pattern[word_index * u64::BITS as usize + bit].push(row_index);
                    remaining &= remaining - 1;
                }
            }
        }
        let best = greedy_cover(rows, &target_words).unwrap_or_default();
        Self {
            rows,
            target_words,
            support_by_pattern,
            selected: vec![false; rows.len()],
            current: Vec::new(),
            best,
            memo_depth: HashMap::new(),
        }
    }

    fn solve(mut self) -> Vec<usize> {
        let mut covered = vec![0_u64; self.target_words.len()];
        self.search(&mut covered);
        self.best
            .sort_unstable_by_key(|index| self.rows[*index].source_index);
        self.best
    }

    fn search(&mut self, covered: &mut [u64]) {
        if is_superset(covered, &self.target_words) {
            if self.best.is_empty() || self.current.len() < self.best.len() {
                self.best.clone_from(&self.current);
            }
            return;
        }
        if !self.best.is_empty() && self.current.len() >= self.best.len() {
            return;
        }
        if self
            .memo_depth
            .get(covered)
            .is_some_and(|depth| *depth <= self.current.len())
        {
            return;
        }
        self.memo_depth.insert(covered.to_vec(), self.current.len());

        let Some((pivot, support)) = self.rarest_uncovered_pattern(covered) else {
            return;
        };
        if support == 0 {
            return;
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
            return;
        }
        let lower_bound = uncovered_count(covered, &self.target_words).div_ceil(max_gain);
        if !self.best.is_empty() && self.current.len() + lower_bound >= self.best.len() {
            return;
        }

        let mut branches = self.support_by_pattern[pivot]
            .iter()
            .copied()
            .filter(|index| !self.selected[*index])
            .collect::<Vec<_>>();
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

        for row_index in branches {
            self.selected[row_index] = true;
            self.current.push(row_index);
            let mut changed = Vec::new();
            for (word_index, row_word) in self.rows[row_index].words.iter().copied().enumerate() {
                let next = covered[word_index] | row_word;
                if next != covered[word_index] {
                    changed.push((word_index, covered[word_index]));
                    covered[word_index] = next;
                }
            }
            self.search(covered);
            for (word_index, previous) in changed {
                covered[word_index] = previous;
            }
            self.current.pop();
            self.selected[row_index] = false;
        }
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

fn greedy_cover(rows: &[DenseRow], target: &[u64]) -> Option<Vec<usize>> {
    let mut covered = vec![0_u64; target.len()];
    let mut selected = vec![false; rows.len()];
    let mut result = Vec::new();
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
            .map(|(_, index)| index)?;
        selected[next] = true;
        result.push(next);
        union_words(&mut covered, &rows[next].words);
    }
    Some(result)
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
