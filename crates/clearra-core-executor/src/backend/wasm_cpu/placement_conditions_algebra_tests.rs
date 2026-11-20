//! Bounded algebra differential. Contributors are concrete operation IDs,
//! not pieces or sampled boards. These tests prove no movement facts.
use super::{
    FramePrefixBounds, FramePrefixCompileError, GroupedClearConditions,
    PrefixOperationOutsideDomain,
};

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
        let compiled = conditions.compile_frame_prefix(frame, OPERATIONS as u8);
        if frame & !complete != 0 {
            assert_eq!(compiled, Err(FramePrefixCompileError::NonCompletableRows));
            continue;
        }
        let Some(minimum) = exact_minimum else {
            assert_eq!(
                compiled,
                Err(FramePrefixCompileError::NotClosed {
                    closure: closure(frame),
                })
            );
            continue;
        };
        let compiled = compiled.unwrap();
        assert_eq!(compiled.deleted_rows(), frame);
        let allowed = conditions.possible_operations_in_frame(frame, minimum, ALL);
        assert_eq!(allowed, exact_union, "{rows:?} frame={frame:04b}");
        assert_eq!(
            compiled.constrain(0, 0),
            Ok(Some(FramePrefixBounds {
                minimum,
                individually_possible: allowed,
            }))
        );
        for (index, &a) in compiled.forbidden_masks().iter().enumerate() {
            for &b in &compiled.forbidden_masks()[index + 1..] {
                assert_ne!(a & !b, 0, "compiled masks must form an antichain");
                assert_ne!(b & !a, 0, "compiled masks must form an antichain");
            }
        }
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
            assert_eq!(compiled.admits(executed), Ok(frame_of(executed) == frame));
        }

        for must_execute in subsets() {
            for must_be_absent in subsets().filter(|absent| must_execute & absent == 0) {
                let matching = || {
                    possible().filter(|executed| {
                        must_execute & !executed == 0 && must_be_absent & executed == 0
                    })
                };
                let reference = matching()
                    .reduce(|intersection, executed| intersection & executed)
                    .map(|minimum| FramePrefixBounds {
                        minimum,
                        individually_possible: matching().fold(0, |union, mask| union | mask),
                    });
                assert_eq!(
                    compiled.constrain(must_execute, must_be_absent),
                    Ok(reference),
                    "{rows:?} frame={frame:04b} execute={must_execute:04b} exclude={must_be_absent:04b}",
                );
            }
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

#[test]
fn placement_algebra_compiled_frame_bounds_preserve_domain_and_clause_capacity() {
    let four = GroupedClearConditions::from_completed_rows(&[0x8001], 1).unwrap();
    assert_eq!(
        four.compile_frame_prefix(0, 4),
        Err(FramePrefixCompileError::ContributorOutsideOperationDomain)
    );
    assert_eq!(
        four.compile_frame_prefix(0, 17),
        Err(FramePrefixCompileError::OperationCountOutOfRange)
    );
    let full_mask = GroupedClearConditions::from_completed_rows(&[u16::MAX], 1).unwrap();
    let unfilled = full_mask.compile_frame_prefix(0, 16).unwrap();
    assert_eq!(unfilled.constrain(0x8000, 0x8000), Ok(None));
    assert_eq!(unfilled.admits(u16::MAX), Ok(false));
    assert_eq!(
        unfilled.constrain(0x7fff, 0),
        Ok(Some(FramePrefixBounds {
            minimum: 0x7fff,
            individually_possible: 0x7fff,
        }))
    );

    // Six incomparable masks attain the compact frame's entire slot budget.
    let pairs = [0b0011, 0b0101, 0b1001, 0b0110, 0b1010, 0b1100];
    let pairs = GroupedClearConditions::from_completed_rows(&pairs, 0b11_1111).unwrap();
    let unfilled = pairs.compile_frame_prefix(0, 4).unwrap();
    assert_eq!(unfilled.forbidden_masks().len(), 6);
    assert_eq!(unfilled.admits(0x8000), Err(PrefixOperationOutsideDomain));
    assert_eq!(
        unfilled.constrain(0, 0x10),
        Err(PrefixOperationOutsideDomain)
    );
    assert_eq!(unfilled.constrain(0b1100, 0), Ok(None));
    assert_eq!(
        unfilled.constrain(0b1000, 0),
        Ok(Some(FramePrefixBounds {
            minimum: 0b1000,
            individually_possible: 0b1000,
        }))
    );

    // The last clause subsumes all earlier ones; no extra slot is needed.
    let subsumed = [0b1111, 0b0111, 0b0011, 0b0101, 0b1001, 0b0001];
    let subsumed = GroupedClearConditions::from_completed_rows(&subsumed, 0b11_1111).unwrap();
    let unfilled = subsumed.compile_frame_prefix(0, 4).unwrap();
    assert_eq!(unfilled.forbidden_masks(), &[1]);
    assert_eq!(unfilled.admits(0b1110), Ok(true));
    assert_eq!(unfilled.admits(0b1111), Ok(false));
}
