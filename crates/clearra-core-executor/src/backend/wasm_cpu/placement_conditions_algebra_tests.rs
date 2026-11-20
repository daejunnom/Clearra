//! Bounded algebra differential. Contributors are concrete operation IDs,
//! not pieces or sampled boards. These tests prove no movement facts.
use super::GroupedClearConditions;

const OPERATIONS: usize = 4;
const ALL: u16 = (1 << OPERATIONS) - 1;

fn subsets() -> core::ops::RangeInclusive<u16> {
    0..=ALL
}

fn flags(mask: u16) -> [bool; OPERATIONS] {
    core::array::from_fn(|operation| mask & (1 << operation) != 0)
}

/// Independent finite oracle: explicitly mark which original rows ever
/// complete and inspect every one of their concrete contributors.
fn oracle_frame(rows: &[u16], complete: u16, executed: u16) -> u16 {
    let executed = flags(executed);
    let mut deleted = 0;
    for (row, &contributors) in rows.iter().enumerate() {
        if complete & (1 << row) == 0 {
            continue;
        }
        let contributors = flags(contributors);
        if (0..OPERATIONS).all(|operation| !contributors[operation] || executed[operation]) {
            deleted |= 1 << row;
        }
    }
    deleted
}

fn mandatory_for(rows: &[u16], deleted: u16) -> u16 {
    rows.iter()
        .enumerate()
        .filter(|(row, _)| deleted & (1 << row) != 0)
        .fold(0, |required, (_, &contributors)| required | contributors)
}

fn check_model(completed_groups: &[u16]) {
    let mut rows = completed_groups.to_vec();
    // A row with an occupied contributor but unfillable in the exact
    // target must never become a line-clear premise.
    rows.push(ALL);
    let complete = (1 << completed_groups.len()) - 1;
    let conditions = GroupedClearConditions::from_completed_rows(&rows, complete).unwrap();
    let oracle: [u16; 1 << OPERATIONS] =
        core::array::from_fn(|executed| oracle_frame(&rows, complete, executed as u16));
    let frame_of = |executed: u16| oracle[usize::from(executed)];

    for executed in subsets() {
        let previous = frame_of(executed);
        let (deleted, deltas) = conditions.deleted_rows_with_child_deltas::<OPERATIONS>(executed);
        assert_eq!(conditions.deleted_rows(executed), previous, "{rows:?}");
        assert_eq!(deleted, previous, "{rows:?}");
        for (operation, delta) in deltas.into_iter().enumerate() {
            let next = frame_of(executed | (1 << operation));
            assert_eq!(next & previous, previous, "clear frames are monotone");
            assert_eq!(delta, next & !previous, "{rows:?} executed={executed:04b}");
        }
    }

    let closure = |frame| conditions.deleted_rows(mandatory_for(&rows, frame));
    for frame in 0..1 << rows.len() {
        if frame & !complete == 0 {
            let closed = closure(frame);
            assert_eq!(closed & frame, frame, "closure is extensive");
            assert_eq!(closure(closed), closed, "closure is idempotent");
            for subset in 0..=complete {
                if subset & !frame == 0 {
                    assert_eq!(
                        closure(subset) & closed,
                        closure(subset),
                        "closure is monotone"
                    );
                }
            }
        }

        let possible = || subsets().filter(|&executed| frame_of(executed) == frame);
        let exact_minimum = possible().reduce(|intersection, mask| intersection & mask);
        let exact_union = possible().fold(0, |union, mask| union | mask);
        assert_eq!(
            conditions.required_for_frame(frame),
            exact_minimum,
            "{rows:?} frame={frame:04b}"
        );
        assert_eq!(closure(frame) == frame, exact_minimum.is_some());
        let Some(minimum) = exact_minimum else {
            continue;
        };
        let allowed = conditions.possible_operations_in_frame(frame, minimum, ALL);
        assert_eq!(allowed, exact_union, "{rows:?} frame={frame:04b}");
        let mandatory = mandatory_for(&rows, frame);
        assert_eq!(minimum, mandatory);
        let bad_additions: Vec<u16> = rows
            .iter()
            .enumerate()
            .filter(|(row, _)| (complete & !frame) & (1 << row) != 0)
            .map(|(_, contributors)| contributors & !minimum)
            .collect();
        assert!(!bad_additions.contains(&0));
        for executed in subsets() {
            // Each uncleared row forbids only the *whole* remaining
            // conjunction, not the individual possible contributors.
            let admissible =
                minimum & !executed == 0 && bad_additions.iter().all(|bad| bad & !executed != 0);
            assert_eq!(admissible, frame_of(executed) == frame);
        }

        for operation in 0..OPERATIONS {
            let pending = 1 << operation;
            if minimum & pending != 0 {
                continue;
            }
            let prefixes = || possible().filter(|executed| executed & pending == 0);
            // With no ordering assumptions, a supporter q is possible alone
            // exactly when it is possible at all. Some of the individually
            // possible supporters may be mutually exclusive as a group.
            assert!(prefixes().any(|executed| executed == minimum));
            let feasible_operations = allowed & !pending;
            let masks = || subsets().filter(|mask| mask & pending == 0);
            for supporters in masks() {
                for blockers in masks() {
                    for base_support in [false, true] {
                        let lock_via_sweep = |executed| {
                            (base_support || executed & supporters != 0) && executed & blockers == 0
                        };
                        let all_by_algebra = (base_support || minimum & supporters != 0)
                            && feasible_operations & blockers == 0;
                        let any_by_algebra = lock_via_sweep(minimum)
                            || (minimum & blockers == 0
                                && feasible_operations & supporters & !blockers != 0);
                        assert_eq!(
                            all_by_algebra,
                            prefixes().all(lock_via_sweep),
                            "{rows:?} D={frame:04b} p={pending:04b} supporters={supporters:04b} blockers={blockers:04b}",
                        );
                        assert_eq!(
                            any_by_algebra,
                            prefixes().any(lock_via_sweep),
                            "{rows:?} D={frame:04b} p={pending:04b} supporters={supporters:04b} blockers={blockers:04b}",
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn placement_algebra_exhaustive_four_operations_three_row_conjunctions() {
    // Use ordered representatives of the contributor multisets. Repeated
    // masks remain in the domain to verify atomic clears of equal groups.
    check_model(&[]);
    for a in 1..=ALL {
        check_model(&[a]);
        for b in a..=ALL {
            check_model(&[a, b]);
            for c in b..=ALL {
                check_model(&[a, b, c]);
            }
        }
    }
}

#[test]
fn placement_algebra_three_contributors_and_implied_frames_are_not_binary_cases() {
    let conditions =
        GroupedClearConditions::from_completed_rows(&[0b0111, 0b0111, 0b0011, 0b0001], 0b1111)
            .unwrap();
    assert_eq!(conditions.required_for_frame(0b0001), None);
    assert_eq!(conditions.required_for_frame(0b0011), None);
    assert_eq!(conditions.required_for_frame(0b0100), None);
    assert_eq!(conditions.required_for_frame(0b1100), Some(0b0011));
    assert_eq!(conditions.required_for_frame(0b1111), Some(0b0111));
    let (deleted, deltas) = conditions.deleted_rows_with_child_deltas::<OPERATIONS>(0b0011);
    assert_eq!(deleted, 0b1100);
    assert_eq!(deltas, [0, 0, 0b0011, 0]);

    let three = GroupedClearConditions::from_completed_rows(&[0b0111], 1).unwrap();
    assert_eq!(three.possible_operations_in_frame(0, 0, ALL), ALL);
    assert_eq!(three.deleted_rows(0b0011), 0);
    assert_eq!(three.deleted_rows(0b0101), 0);
    assert_eq!(three.deleted_rows(0b0110), 0);
    assert_eq!(three.deleted_rows(0b0111), 1);
}
