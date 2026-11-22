//! Independent oracle for selected-operation source-prefix negatives.
//!
//! For each compatible extra kind, concretize every missing operation as a
//! distinct, immediately available free operation with no row contributions.
//! Enumerate locked operation subsets and automatic row completion. This
//! reference does not use the product's row-state DP, deficit, or contributor
//! union recurrence. A positive result remains only an optimistic relaxation.

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    source_prefix::{
        selected_source_prefix_relaxation, selected_source_prefix_stalls, SourcePrefixDecision,
    },
    GeometryCatalog,
};

#[derive(Clone, Debug)]
struct Model {
    bag_size: u8,
    kinds: Vec<usize>,
    rows: Vec<u16>,
    requirements: Vec<Vec<u16>>,
}

impl Model {
    fn masks(&self) -> [u16; 7] {
        let mut masks = [0; 7];
        for (operation, &kind) in self.kinds.iter().enumerate() {
            masks[kind] |= 1 << operation;
        }
        masks
    }

    fn decision(&self) -> SourcePrefixDecision {
        selected_source_prefix_relaxation(
            self.bag_size,
            self.masks(),
            &self.rows,
            |operation, deleted| {
                self.requirements[operation]
                    .iter()
                    .any(|&required| required & !deleted == 0)
            },
        )
    }

    fn oracle_has_prefix(&self) -> bool {
        let n = usize::from(self.bag_size);
        let mut used = [0_usize; 7];
        for &kind in &self.kinds {
            used[kind] += 1;
        }
        for extra in 0..n {
            let total: [usize; 7] = std::array::from_fn(|kind| {
                if kind < n {
                    2 + usize::from(kind == extra)
                } else {
                    0
                }
            });
            if (0..n).any(|kind| used[kind] > total[kind]) {
                continue;
            }
            let mut pieces = self.kinds.clone();
            for kind in 0..n {
                pieces.extend(std::iter::repeat_n(kind, total[kind] - used[kind]));
            }
            assert_eq!(pieces.len(), 2 * n + 1);
            let mut reachable = vec![false; 1 << pieces.len()];
            reachable[0] = true;
            for placed in 0..reachable.len() {
                if !reachable[placed] {
                    continue;
                }
                if placed.count_ones() as usize == n {
                    return true;
                }
                let mut counts = [0_usize; 7];
                for (operation, &kind) in pieces.iter().enumerate() {
                    counts[kind] += usize::from(placed & (1 << operation) != 0);
                }
                // All complete rows clear together, including empty-AND rows
                // before the first lock. Free future operations own no rows.
                let deleted =
                    self.rows
                        .iter()
                        .enumerate()
                        .fold(0_u16, |deleted, (row, &owners)| {
                            deleted
                                | if owners & !(placed as u16) == 0 {
                                    1 << row
                                } else {
                                    0
                                }
                        });
                for (operation, &kind) in pieces.iter().enumerate() {
                    let bit = 1 << operation;
                    if placed & bit != 0 || counts[kind] == 1 + usize::from(kind == extra) {
                        continue;
                    }
                    let available = operation >= self.kinds.len()
                        || self.requirements[operation]
                            .iter()
                            .any(|&required| required & !deleted == 0);
                    if available {
                        reachable[placed | bit] = true;
                    }
                }
            }
        }
        false
    }

    fn assert_matches(&self) -> SourcePrefixDecision {
        assert_eq!(self.kinds.len(), self.requirements.len());
        let expected = self.oracle_has_prefix();
        let decision = self.decision();
        assert_eq!(!decision.proved_infeasible, expected, "model={self:?}");
        assert!(usize::from(decision.visited_row_states) <= 1 << self.rows.len());
        if decision.root_fast_pass {
            assert!(expected);
        }
        decision
    }

    fn selected(&self, selected: u16) -> Self {
        let old_ids: Vec<_> = (0..self.kinds.len())
            .filter(|&operation| selected & (1 << operation) != 0)
            .collect();
        Self {
            bag_size: self.bag_size,
            kinds: old_ids.iter().map(|&old| self.kinds[old]).collect(),
            rows: self
                .rows
                .iter()
                .map(|&row| {
                    old_ids.iter().enumerate().fold(0, |mask, (new, &old)| {
                        mask | if row & (1 << old) != 0 { 1 << new } else { 0 }
                    })
                })
                .collect(),
            requirements: old_ids
                .iter()
                .map(|&old| self.requirements[old].clone())
                .collect(),
        }
    }

    /// new operation index -> old operation index, likewise for rows/kinds.
    fn reordered(&self, operations: &[usize], rows: &[usize], kinds: &[usize]) -> Self {
        let remap_rows = |mask: u16| {
            rows.iter().enumerate().fold(0, |result, (new, &old)| {
                result | if mask & (1 << old) != 0 { 1 << new } else { 0 }
            })
        };
        Self {
            bag_size: self.bag_size,
            kinds: operations
                .iter()
                .map(|&old| {
                    kinds
                        .iter()
                        .position(|&kind| kind == self.kinds[old])
                        .unwrap()
                })
                .collect(),
            rows: rows
                .iter()
                .map(|&old_row| {
                    operations.iter().enumerate().fold(0, |mask, (new, &old)| {
                        mask | if self.rows[old_row] & (1 << old) != 0 {
                            1 << new
                        } else {
                            0
                        }
                    })
                })
                .collect(),
            requirements: operations
                .iter()
                .map(|&old| {
                    self.requirements[old]
                        .iter()
                        .map(|&mask| remap_rows(mask))
                        .collect()
                })
                .collect(),
        }
    }
}

fn full_kinds(n: usize, extra: usize) -> Vec<usize> {
    (0..n)
        .flat_map(|kind| std::iter::repeat_n(kind, 2 + usize::from(kind == extra)))
        .collect()
}

#[test]
fn geometry_source_prefix_partial_subsets_match_independent_free_operation_oracle() {
    let mut models = 0;
    let mut positives = 0;
    let mut negatives = 0;
    let mut slow_positives = 0;
    for (n, variants, phases) in [(2_usize, 12, 4), (3, 4, 2)] {
        for extra in 0..n {
            let kinds = full_kinds(n, extra);
            for variant in 0..variants {
                let row_count = 2 + variant % 2;
                let row_domain = (1_u16 << row_count) - 1;
                let mut rows = vec![0_u16; row_count];
                for operation in 0..kinds.len() {
                    let occupied = ((operation + 1) * (variant + 3)) as u16 & row_domain;
                    for (row, owners) in rows.iter_mut().enumerate() {
                        if occupied & (1 << row) != 0 {
                            *owners |= 1 << operation;
                        }
                    }
                }
                for phase in 0..phases {
                    let requirements = (0..kinds.len())
                        .map(|operation| match phase {
                            0 => vec![0],
                            1 => vec![row_domain],
                            2 => vec![
                                1 << (operation % row_count),
                                1 << ((operation + 1) % row_count),
                            ],
                            _ => match operation % 4 {
                                0 => vec![0],
                                1 => vec![1],
                                2 => vec![row_domain],
                                _ => vec![],
                            },
                        })
                        .collect();
                    let full = Model {
                        bag_size: n as u8,
                        kinds: kinds.clone(),
                        rows: rows.clone(),
                        requirements,
                    };
                    for selected in 0..1_u16 << kinds.len() {
                        let result = full.selected(selected).assert_matches();
                        models += 1;
                        positives += usize::from(!result.proved_infeasible);
                        negatives += usize::from(result.proved_infeasible);
                        slow_positives +=
                            usize::from(!result.proved_infeasible && !result.root_fast_pass);
                    }
                }
            }
        }
    }
    assert_eq!(models, 6_144);
    assert!(positives > 0 && negatives > 0 && slow_positives > 0);
    eprintln!("geometry_source_prefix_partial_oracle models={models} positives={positives} negatives={negatives} slow_positives={slow_positives}");
}

fn or_escape_model() -> Model {
    Model {
        bag_size: 2,
        kinds: vec![0, 0, 0, 1, 1],
        rows: vec![1 << 0, 0, 1 << 3],
        requirements: vec![vec![1, 2], vec![7], vec![7], vec![1], vec![7]],
    }
}

#[test]
fn geometry_source_prefix_preserves_future_empty_and_realization_or_and_joint_clears() {
    let future_only_row = Model {
        bag_size: 2,
        kinds: vec![0, 0, 1, 1],
        rows: vec![0, 0b1111],
        requirements: vec![vec![1]; 4],
    };
    let result = future_only_row.assert_matches();
    assert!(!result.proved_infeasible && !result.root_fast_pass);
    assert!(result.visited_row_states > 1);

    let positive = or_escape_model();
    assert!(!positive.assert_matches().proved_infeasible);
    let mut reordered = positive.clone();
    reordered.requirements[0].reverse();
    assert!(!reordered.assert_matches().proved_infeasible);
    let mut no_escape = positive;
    no_escape.requirements[0] = vec![1];
    assert!(no_escape.assert_matches().proved_infeasible);

    // Both rows are completed by the SAME two operations. Adding two row
    // cardinalities would wrongly consume four locks; automatic clearing
    // consumes only the two concrete operations in the subset oracle.
    let shared = (1 << 0) | (1 << 3);
    let simultaneous = Model {
        bag_size: 3,
        kinds: full_kinds(3, 0),
        rows: vec![shared, shared],
        requirements: (0..7)
            .map(|operation| {
                if operation == 0 || operation == 3 {
                    vec![0]
                } else {
                    vec![3]
                }
            })
            .collect(),
    };
    let result = simultaneous.assert_matches();
    assert!(!result.proved_infeasible && !result.root_fast_pass);
    assert!(result.visited_row_states > 1);
    for rotation in 0..3 {
        let kinds: Vec<_> = (0..3).map(|kind| (kind + rotation) % 3).collect();
        assert!(
            !simultaneous
                .reordered(&(0..7).collect::<Vec<_>>(), &[0, 1], &kinds)
                .assert_matches()
                .proved_infeasible
        );
    }

    // Lift the same obstruction/escape to the product's seven-kind alphabet.
    // The four additional kinds are concrete free future operations in the
    // oracle, and every possible extra-kind bit is exercised by renaming.
    let mut full_alphabet = simultaneous.clone();
    full_alphabet.bag_size = 7;
    for rotation in 0..7 {
        let kinds: Vec<_> = (0..7).map(|kind| (kind + rotation) % 7).collect();
        let result = full_alphabet
            .reordered(&(0..7).collect::<Vec<_>>(), &[0, 1], &kinds)
            .assert_matches();
        assert!(!result.proved_infeasible && !result.root_fast_pass);
    }

    // The partial relaxation must retain all 64 row states: future-only rows
    // can be added for free, and the positive OR may first hold at state 63.
    let all_six_rows = Model {
        bag_size: 2,
        kinds: vec![0, 0, 1, 1],
        rows: vec![0; 6],
        requirements: vec![vec![63]; 4],
    };
    let result = all_six_rows.assert_matches();
    assert!(!result.proved_infeasible && !result.root_fast_pass);
    assert_eq!(result.visited_row_states, 64);

    // A valid dense selection that fits none of the possible full inventories
    // is an exact negative, distinct from an unsupported input representation.
    let impossible_inventory = Model {
        bag_size: 3,
        kinds: vec![0, 0, 0, 1, 1, 1],
        rows: vec![],
        requirements: vec![vec![0]; 6],
    };
    assert!(impossible_inventory.assert_matches().proved_infeasible);
}

fn permutations(count: usize) -> Vec<Vec<usize>> {
    fn visit(prefix: &mut Vec<usize>, count: usize, result: &mut Vec<Vec<usize>>) {
        if prefix.len() == count {
            result.push(prefix.clone());
            return;
        }
        for next in 0..count {
            if !prefix.contains(&next) {
                prefix.push(next);
                visit(prefix, count, result);
                prefix.pop();
            }
        }
    }
    let mut result = Vec::new();
    visit(&mut Vec::new(), count, &mut result);
    result
}

#[test]
fn geometry_source_prefix_operation_row_and_extra_kind_reindexing_preserve_decisions() {
    let positive = or_escape_model();
    let mut negative = positive.clone();
    negative.requirements[0] = vec![1];
    let operations = permutations(5);
    let rows = permutations(3);
    let kinds = permutations(2);
    for model in [positive, negative] {
        let expected = model.assert_matches().proved_infeasible;
        for operation_order in &operations {
            for row_order in &rows {
                for kind_order in &kinds {
                    assert_eq!(
                        model
                            .reordered(operation_order, row_order, kind_order)
                            .assert_matches()
                            .proved_infeasible,
                        expected
                    );
                }
            }
        }
    }
}

#[test]
fn geometry_source_prefix_unsupported_inputs_keep_unknown_authority() {
    let cases = [
        (0, [0; 7], vec![]),
        (8, [0; 7], vec![]),
        (1, [0b1111, 0, 0, 0, 0, 0, 0], vec![]),
        (2, [1, 1, 0, 0, 0, 0, 0], vec![]),
        (2, [0b100, 0, 0, 0, 0, 0, 0], vec![]),
        (2, [0, 0, 1, 0, 0, 0, 0], vec![]),
        (2, [1, 2, 0, 0, 0, 0, 0], vec![0; 7]),
        (2, [1, 2, 0, 0, 0, 0, 0], vec![4]),
        (7, [u16::MAX, 0, 0, 0, 0, 0, 0], vec![]),
    ];
    for (n, kinds, rows) in cases {
        let result = selected_source_prefix_relaxation(n, kinds, &rows, |_, _| {
            panic!("unsupported input must not call its predicate")
        });
        assert!(!result.proved_infeasible && !result.root_fast_pass);
        assert_eq!(result.visited_row_states, 0);
    }
}

fn mixed15_catalog_and_rows() -> (GeometryCatalog, Vec<u32>) {
    use PieceKind::{I, J, L, O, S, T, Z};
    // This is the geometry from the executed exact graph/source regression
    // full6_source_prefix_preserves_exact_graph_source_intersection_and_witness:
    // 5,846,037 original P1P7P7 queues, minimum ordinal 0, with a 15-lock
    // first_pattern_path witness. The order written here is ONLY a geometry
    // listing/physical witness; I,T,T,Z,S,S,J,... itself violates the source cap.
    let placements = [
        (I, 0x4010_0401_u64),
        (T, 0x100e),
        (T, 0x8070),
        (Z, 0x0003_0180),
        (S, 0x0300_6000),
        (S, 0x0006_00c0_0000),
        (J, 0x0001_8020_0800),
        (Z, 0x0018_0c00_0000),
        (L, 0x0060_1004_0000),
        (I, 0x0080_2008_0200),
        (I, 0x001e << 40),
        (O, 0x0003_00c0 << 40),
        (O, 0x000c_0300 << 40),
        (L, 0x1c01 << 40),
        (J, 0xe020 << 40),
    ];
    let full = (1_u64 << 60) - 1;
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, full).unwrap();
    let mut occupied = 0;
    let rows = placements
        .iter()
        .map(|&(piece, cells)| {
            assert_eq!(occupied & cells, 0);
            occupied |= cells;
            catalog.skeleton_id(piece, cells).unwrap()
        })
        .collect();
    assert_eq!(occupied, full);
    (catalog, rows)
}

#[test]
fn geometry_source_prefix_preserves_all_32768_subsets_of_source_positive_mixed15() {
    let (catalog, rows) = mixed15_catalog_and_rows();
    for selected in 0..1_u16 << rows.len() {
        let mut prefix: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(operation, &row)| (selected & (1 << operation) != 0).then_some(row))
            .collect();
        assert!(
            !selected_source_prefix_stalls(&catalog, &prefix),
            "selected={selected:#x}"
        );
        prefix.reverse();
        assert!(
            !selected_source_prefix_stalls(&catalog, &prefix),
            "reversed selected={selected:#x}"
        );
    }
    assert!(!selected_source_prefix_stalls(&catalog, &[u32::MAX]));
    assert!(!selected_source_prefix_stalls(&catalog, &[rows[0]; 16]));
    let other = GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    assert!(!selected_source_prefix_stalls(&other, &[]));
    let nonempty = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        6,
        0xf,
        ((1_u64 << 60) - 1) & !0xf,
    )
    .unwrap();
    assert!(!selected_source_prefix_stalls(&nonempty, &[]));
    let partial = GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, 0xf).unwrap();
    assert!(!selected_source_prefix_stalls(&partial, &[]));
}
