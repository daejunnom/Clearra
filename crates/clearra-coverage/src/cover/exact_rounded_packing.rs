// Integer cover cuts, independently implemented from their mathematical idea.
// Choose uncovered constraints so that each eligible row occurs at most twice.
// Within each connected support component C, sum its covering inequalities:
//   sum_j multiplicity(C,j) * x_j >= |C|, multiplicity <= 2.
// Therefore at least ceil(|C|/2) rows are necessary. Different components have
// disjoint row support, so their integer lower bounds ADD (not just round once
// after summing all components). No LP floats, optimum hint, or row-ID reduction
// participates in this proof. Failure/overflow means no additional bound.

const ROOT: usize = 1_usize << (usize::BITS - 1);
const EMPTY: usize = usize::MAX;

fn root(parents: &mut [usize], mut index: usize) -> usize {
    let mut top = index;
    while parents[top] < ROOT {
        top = parents[top];
    }
    while parents[index] < ROOT {
        let next = parents[index];
        parents[index] = top;
        index = next;
    }
    top
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rounded_support_components_lower_bound(
    target: &[u64],
    covered: &[u64],
    order: &[usize],
    support: &[Vec<usize>],
    selected: &[bool],
    excluded: &[u64],
    row_scratch: &mut [usize],
    parents: &mut Vec<usize>,
) -> Option<usize> {
    if target.len() != covered.len()
        || selected.len() != row_scratch.len()
        || excluded.len() != selected.len().div_ceil(64)
        || parents.capacity() < order.len()
        || order.len() >= ROOT / 2
    {
        return None;
    }
    row_scratch.fill(EMPTY);
    parents.clear();
    let eligible = |row: usize| !selected[row] && excluded[row / 64] & (1 << (row % 64)) == 0;
    for &pattern in order {
        if target.get(pattern / 64)? & !covered.get(pattern / 64)? & (1 << (pattern % 64)) == 0 {
            continue;
        }
        let rows = support.get(pattern)?;
        let mut nonempty = false;
        let mut saturated = false;
        for &row in rows {
            if row >= selected.len() {
                return None;
            }
            if !eligible(row) {
                continue;
            }
            nonempty = true;
            let entry = row_scratch[row];
            if entry != EMPTY && entry & 1 != 0 {
                saturated = true;
                break;
            }
        }
        if !nonempty {
            return None;
        } // Caller owns zero-support infeasibility.
        if saturated {
            continue;
        }
        let node = parents.len();
        parents.push(ROOT | 1);
        for &row in rows {
            if !eligible(row) {
                continue;
            }
            let entry = row_scratch[row];
            if entry == EMPTY {
                row_scratch[row] = node << 1;
            } else {
                let a = root(parents, node);
                let b = root(parents, entry >> 1);
                if a != b {
                    let combined = (parents[a] & !ROOT).checked_add(parents[b] & !ROOT)?;
                    if combined >= ROOT {
                        return None;
                    }
                    // Union by size, independent of original candidate IDs.
                    let (large, small) = if parents[a] >= parents[b] {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    parents[large] = ROOT | combined;
                    parents[small] = large;
                }
                row_scratch[row] |= 1;
            }
        }
    }
    parents
        .iter()
        .filter(|entry| **entry >= ROOT)
        .try_fold(0_usize, |bound, entry| {
            bound.checked_add((entry & !ROOT).div_ceil(2))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_odd_cycles_round_before_addition() {
        // Two triangle cover components: LP=3, exact integer cut=4.
        let supports = vec![
            vec![0, 1],
            vec![1, 2],
            vec![0, 2],
            vec![3, 4],
            vec![4, 5],
            vec![3, 5],
        ];
        let mut parents = Vec::with_capacity(6);
        assert_eq!(
            rounded_support_components_lower_bound(
                &[63],
                &[0],
                &[0, 1, 2, 3, 4, 5],
                &supports,
                &[false; 6],
                &[0],
                &mut [0; 6],
                &mut parents
            ),
            Some(4)
        );
    }

    #[test]
    fn all_three_covered_by_one_row_must_not_produce_two_row_cut() {
        let supports = vec![vec![0, 1], vec![0, 2], vec![0, 3]];
        assert_eq!(
            rounded_support_components_lower_bound(
                &[7],
                &[0],
                &[0, 1, 2],
                &supports,
                &[false; 4],
                &[0],
                &mut [0; 4],
                &mut Vec::with_capacity(3)
            ),
            Some(1)
        );
    }

    #[test]
    fn exact_bound_never_exceeds_a_cover_over_small_models_and_exclusions() {
        let mut seed = 7_u64;
        for _ in 0..100 {
            let rows: Vec<_> = (0..7)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 32) & 31
                })
                .collect();
            let supports: Vec<_> = (0..5)
                .map(|p| {
                    rows.iter()
                        .enumerate()
                        .filter_map(|(r, m)| (m & (1 << p) != 0).then_some(r))
                        .collect()
                })
                .collect();
            for excluded in [0_u64, 1, 16, 65] {
                for covered in [0_u64, 3, 12] {
                    let bound = rounded_support_components_lower_bound(
                        &[31],
                        &[covered],
                        &[0, 1, 2, 3, 4],
                        &supports,
                        &[false; 7],
                        &[excluded],
                        &mut [0; 7],
                        &mut Vec::with_capacity(5),
                    );
                    for set in 0_u64..128 {
                        if set & excluded != 0 {
                            continue;
                        }
                        let union = rows.iter().enumerate().fold(covered, |bits, (r, m)| {
                            if set & (1 << r) != 0 {
                                bits | m
                            } else {
                                bits
                            }
                        });
                        if union == 31 {
                            assert!(bound.is_none_or(|b| b <= set.count_ones() as usize));
                        }
                    }
                }
            }
        }
    }
}
