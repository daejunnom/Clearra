//! Differential oracle for the positive-requirement/source-cap relaxation.
//!
//! The reference enumerates locked operation subsets, automatically clears
//! every completed row, and keeps every positive realization alternative.
//! It intentionally has no SRS, grounding, collision or forbidden-clear rule.
//! Equality with this abstract oracle never grants physical buildability.

use super::{temporal_prefix_relaxation, FullDrawSourcePrefix, TemporalPrefixResult};

#[derive(Clone)]
struct Model {
    bag_size: usize,
    extra: usize,
    kind_operations: [u16; 7],
    rows: Vec<u16>,
    alternatives: Vec<Vec<u16>>,
}

fn kind_operations(bag_size: usize, extra: usize) -> [u16; 7] {
    let mut masks = [0; 7];
    let mut operation = 0;
    for (kind, mask) in masks.iter_mut().enumerate().take(bag_size) {
        for _ in 0..2 + usize::from(kind == extra) {
            *mask |= 1 << operation;
            operation += 1;
        }
    }
    masks
}

impl Model {
    fn new(bag_size: usize, extra: usize, rows: Vec<u16>, alternatives: Vec<Vec<u16>>) -> Self {
        assert_eq!(alternatives.len(), 2 * bag_size + 1);
        Self {
            bag_size,
            extra,
            kind_operations: kind_operations(bag_size, extra),
            rows,
            alternatives,
        }
    }

    fn operation_count(&self) -> usize {
        2 * self.bag_size + 1
    }

    fn all_operations(&self) -> u16 {
        (1 << self.operation_count()) - 1
    }

    fn cap_holds(&self, subset: u16) -> bool {
        self.kind_operations
            .iter()
            .enumerate()
            .all(|(kind, &mask)| (subset & mask).count_ones() <= 1 + u32::from(kind == self.extra))
    }

    /// Independent operation-subset traversal. There is no row-state search
    /// and no A(D)/C(D) recurrence in this oracle.
    fn subset_oracle_has_prefix(&self) -> bool {
        let mut reachable = vec![false; 1 << self.operation_count()];
        reachable[0] = true;
        let mut found = false;
        for subset in 0..reachable.len() {
            if !reachable[subset] {
                continue;
            }
            let subset = subset as u16;
            assert!(self.cap_holds(subset));
            if subset.count_ones() as usize == self.bag_size {
                found = true;
                continue;
            }
            // Actual AND completion, including simultaneous and automatic
            // extra clears. Empty contributor AND is true in this abstraction.
            let deleted = self
                .rows
                .iter()
                .enumerate()
                .fold(0_u16, |mask, (row, &owners)| {
                    mask | if owners & !subset == 0 { 1 << row } else { 0 }
                });
            for operation in 0..self.operation_count() {
                let bit = 1 << operation;
                let child = subset | bit;
                if subset & bit == 0
                    && self.cap_holds(child)
                    && self.alternatives[operation]
                        .iter()
                        .any(|&required| required & !deleted == 0)
                {
                    reachable[usize::from(child)] = true;
                }
            }
        }
        found
    }

    fn row_relaxation(&self) -> TemporalPrefixResult {
        let prefix =
            FullDrawSourcePrefix::from_kind_operations(self.kind_operations, self.bag_size as u8)
                .unwrap();
        temporal_prefix_relaxation(&prefix, &self.rows, |operation, deleted| {
            self.alternatives[operation]
                .iter()
                .any(|&required| required & !deleted == 0)
        })
    }

    fn assert_matches(&self) -> TemporalPrefixResult {
        let expected = self.subset_oracle_has_prefix();
        let actual = self.row_relaxation();
        assert_eq!(
            !actual.proved_infeasible, expected,
            "n={} extra={} rows={:?} requirements={:?}",
            self.bag_size, self.extra, self.rows, self.alternatives,
        );
        assert!(actual.visited_row_states <= 1 << self.rows.len());
        if actual.root_fast_pass {
            assert!(expected);
        }
        actual
    }
}

#[test]
fn temporal_source_prefix_matches_independent_all_subset_oracle() {
    let mut models = 0;
    let mut passes = 0;
    let mut rejects = 0;
    let mut slow_passes = 0;
    let mut check = |model: Model| {
        let result = model.assert_matches();
        models += 1;
        rejects += usize::from(result.proved_infeasible);
        passes += usize::from(!result.proved_infeasible);
        slow_passes += usize::from(!result.proved_infeasible && !result.root_fast_pass);
    };
    // Every assignment of five operations to row 0, row 1, or both rows.
    // Six positive requirement schemes include AND, distinct OR alternatives,
    // unconditional availability and an operation with no realization.
    for extra in 0..2 {
        for assignment in 0..3_usize.pow(5) {
            let mut code = assignment;
            let mut rows = vec![0_u16; 2];
            for operation in 0..5 {
                let occupied = code % 3 + 1;
                code /= 3;
                for (row, owners) in rows.iter_mut().enumerate() {
                    if occupied & (1 << row) != 0 {
                        *owners |= 1 << operation;
                    }
                }
            }
            for phase in 0..6 {
                let alternatives = (0..5)
                    .map(|operation| match (operation + phase) % 6 {
                        0 => vec![0],
                        1 => vec![1],
                        2 => vec![2],
                        3 => vec![3],
                        4 => vec![1, 2],
                        _ => Vec::new(),
                    })
                    .collect();
                check(Model::new(2, extra, rows.clone(), alternatives));
            }
        }
    }
    // Larger source alphabets and three/four-row domains exercise longer
    // first-bag prefixes. Each model is still checked over every reachable
    // operation subset; these are finite deterministic fixtures, not sampling.
    for bag_size in 3..=4 {
        for extra in 0..bag_size {
            for variant in 0..64 {
                let row_count = bag_size;
                let row_domain = (1_u16 << row_count) - 1;
                let mut rows = vec![0; row_count];
                let mut alternatives = Vec::new();
                for operation in 0..2 * bag_size + 1 {
                    let occupied = (((operation + 1) * (variant + 3)) as u16 & row_domain).max(1);
                    for (row, owners) in rows.iter_mut().enumerate() {
                        if occupied & (1 << row) != 0 {
                            *owners |= 1 << operation;
                        }
                    }
                    let a = 1 << ((operation + variant) % row_count);
                    let b = 1 << ((operation + variant + 1) % row_count);
                    alternatives.push(match (operation + variant) % 5 {
                        0 => vec![0],
                        1 => vec![a],
                        2 => vec![a | b],
                        3 => vec![a, b],
                        _ => Vec::new(),
                    });
                }
                check(Model::new(bag_size, extra, rows, alternatives));
            }
        }
    }
    assert_eq!(models, 3_364);
    assert!(passes > 0 && rejects > 0 && slow_passes > 0);
    eprintln!(
        "source_prefix_temporal_oracle models={models} passes={passes} rejects={rejects} slow_passes={slow_passes}"
    );
}

#[test]
fn temporal_source_prefix_preserves_joint_clear_shared_owners_and_realization_or() {
    // n=3: operation kinds are I,I,I,O,O,T,T. Two already available
    // operations jointly fill both rows, then a third operation becomes ready.
    // Summing the two rows' contributor counts would charge four operations
    // and wrongly exceed the three-lock budget; their UNION contains only two.
    let shared = (1 << 0) | (1 << 3);
    let simultaneous = Model::new(
        3,
        0,
        vec![shared, shared],
        vec![
            vec![0],
            vec![3],
            vec![3],
            vec![0],
            vec![3],
            vec![3],
            vec![3],
        ],
    );
    let result = simultaneous.assert_matches();
    assert!(!result.proved_infeasible);
    assert!(!result.root_fast_pass);
    assert!(result.visited_row_states > 1);

    // The first alternative requires the operation's own completed row and
    // cannot start. Its second alternative is enabled by the available O.
    let alternatives = Model::new(
        2,
        0,
        vec![1 << 3, 1 << 0],
        vec![vec![2, 1], vec![3], vec![3], vec![0], vec![3]],
    );
    assert!(!alternatives.assert_matches().proved_infeasible);
    let mut reordered_or = alternatives.clone();
    reordered_or.alternatives[0].reverse();
    assert!(!reordered_or.assert_matches().proved_infeasible);

    // Additional automatic deletion can only help a positive predicate. In
    // this abstract fixture an empty row is complete before any lock; a row
    // search may add it as a zero-cost event and must not reject the prefix.
    let empty_and = Model::new(
        2,
        0,
        vec![0],
        vec![vec![1], vec![1], vec![1], vec![0], vec![0]],
    );
    assert!(!empty_and.assert_matches().proved_infeasible);

    let all_ready = Model::new(2, 0, vec![0x1f], vec![vec![0]; 5]);
    let result = all_ready.assert_matches();
    assert!(!result.proved_infeasible);
    assert!(result.root_fast_pass);
}

#[test]
fn temporal_source_prefix_rejects_cap_blocked_rows_and_positive_dependency_cycles() {
    // Only the two O operations are initially available, but source cap O<=1
    // means they cannot be two legal first-bag locks. Both rows can clear
    // individually; clearing their UNION would need both O operations.
    let cap_blocked = Model::new(
        2,
        0,
        vec![1 << 3, 1 << 4],
        vec![vec![3], vec![3], vec![3], vec![0], vec![0]],
    );
    assert!(cap_blocked.assert_matches().proved_infeasible);

    let cycle = Model::new(
        2,
        0,
        vec![1 << 0, 1 << 3],
        vec![vec![2], vec![3], vec![3], vec![1], vec![3]],
    );
    assert!(cycle.assert_matches().proved_infeasible);
    let mut escape = cycle.clone();
    escape.alternatives[0].push(0);
    assert!(!escape.assert_matches().proved_infeasible);
    assert!(cycle.assert_matches().proved_infeasible);

    // Popcount alone is insufficient even when the row can be filled by
    // two operations: the row itself would consume the unavailable second O.
    let one_row_cap = Model::new(
        2,
        0,
        vec![(1 << 3) | (1 << 4)],
        vec![vec![1], vec![1], vec![1], vec![0], vec![0]],
    );
    assert!(one_row_cap.assert_matches().proved_infeasible);
}

fn remap_mask(mask: u16, permutation: &[usize]) -> u16 {
    permutation
        .iter()
        .enumerate()
        .fold(0, |mapped, (old, &new)| {
            mapped | if mask & (1 << old) != 0 { 1 << new } else { 0 }
        })
}

fn visit_permutations(values: &mut [usize], offset: usize, visit: &mut impl FnMut(&[usize])) {
    if offset == values.len() {
        visit(values);
        return;
    }
    for index in offset..values.len() {
        values.swap(offset, index);
        visit_permutations(values, offset + 1, visit);
        values.swap(offset, index);
    }
}

#[test]
fn temporal_source_prefix_reindexing_and_unsupported_input_never_gain_negative_authority() {
    let model = Model::new(
        2,
        0,
        vec![1 << 3, 1 << 0, 0x1f, 0x1f, 0x1f, 0x1f],
        vec![vec![2, 1], vec![3], vec![3], vec![0], vec![3]],
    );
    assert!(!model.assert_matches().proved_infeasible);
    let negative = Model::new(
        2,
        0,
        vec![1 << 3, 1 << 4, 0x1f, 0x1f, 0x1f, 0x1f],
        vec![vec![3], vec![3], vec![3], vec![0], vec![0]],
    );
    assert!(negative.assert_matches().proved_infeasible);
    for original in [model.clone(), negative] {
        let expected = original.subset_oracle_has_prefix();
        let mut operation_permutations = 0;
        visit_permutations(&mut [0, 1, 2, 3, 4], 0, &mut |permutation| {
            let mut mapped = original.clone();
            mapped.kind_operations = original
                .kind_operations
                .map(|mask| remap_mask(mask, permutation));
            mapped.rows = original
                .rows
                .iter()
                .map(|&mask| remap_mask(mask, permutation))
                .collect();
            for (old, &new) in permutation.iter().enumerate() {
                mapped.alternatives[new] = original.alternatives[old].clone();
            }
            assert_eq!(!mapped.assert_matches().proved_infeasible, expected);
            operation_permutations += 1;
        });
        assert_eq!(operation_permutations, 120);
        let mut row_permutations = 0;
        visit_permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |permutation| {
            let mut mapped = original.clone();
            for (old, &new) in permutation.iter().enumerate() {
                mapped.rows[new] = original.rows[old];
            }
            mapped.alternatives = original
                .alternatives
                .iter()
                .map(|requirements| {
                    requirements
                        .iter()
                        .map(|&mask| remap_mask(mask, permutation))
                        .collect()
                })
                .collect();
            assert_eq!(!mapped.assert_matches().proved_infeasible, expected);
            row_permutations += 1;
        });
        assert_eq!(row_permutations, 720);
    }

    let prefix = FullDrawSourcePrefix::from_kind_operations(model.kind_operations, 2).unwrap();
    for rows in [
        vec![model.all_operations(); 7],
        vec![1 << model.operation_count()],
    ] {
        let unsupported = temporal_prefix_relaxation(&prefix, &rows, |_, _| false);
        assert!(!unsupported.proved_infeasible);
        assert_eq!(unsupported.visited_row_states, 0);
    }
}
