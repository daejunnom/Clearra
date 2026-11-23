//! Exhaustive finite algebra checks; these are not small-input speed tests.

use std::collections::BTreeSet;

use super::*;
use crate::backend::wasm_cpu::geometry_partial_family::GeometryDownView;

fn fixture() -> (GeometrySolutionFamily, u32, Vec<u64>) {
    let mut family = GeometrySolutionFamily::new();
    let a = family.append(0, FAMILY_EMPTY).unwrap();
    let b = family.append(1, FAMILY_EMPTY).unwrap();
    let alias = family.append(4, FAMILY_EMPTY).unwrap();
    let choice = family.union(a, alias).unwrap();
    let product = family.product(choice, b).unwrap();
    let d = family.append(3, FAMILY_EMPTY).unwrap();
    let alternative = family.append(2, d).unwrap();
    let root = family.union(product, alternative).unwrap();
    (family, root, vec![0x0f, 0xf0, 0x55, 0xaa, 0x0f])
}

fn singleton(arena: &mut ZddArena, variables: &[u32]) -> ZddId {
    variables.iter().rev().fold(ZddId::UNIT, |root, &variable| {
        arena.mk(variable, ZddId::EMPTY, root).unwrap()
    })
}

#[test]
fn guard_suffix_cutoffs_match_pairwise_conflicts_and_exclude_only_self() {
    // Distinct row identities can share a footprint. The greatest owner must
    // be replaced by the second greatest only when it is the guarded row.
    for cells in [
        vec![0x0f, 0xf0, 0x55, 0xaa, 0x0f],
        vec![0xf000_0000_0000_0000, 0x0f, 0xf000_0000_0000_0000],
        vec![0x0f, 0xf0, 0xf00],
    ] {
        let mut budget = WorkBudget::new(ZddLimits::default(), "test_guard_cutoffs");
        let cutoffs = compile_guard_cutoffs(&cells, &mut budget).unwrap();
        for (row, &mask) in cells.iter().enumerate() {
            let expected = cells
                .iter()
                .enumerate()
                .filter(|&(other, &footprint)| other != row && footprint & mask != 0)
                .map(|(other, _)| (other + 1) as u32)
                .max()
                .unwrap_or(0);
            assert_eq!(cutoffs[row], expected, "row={row} cells={cells:?}");
        }
    }
}

#[test]
fn guard_suffix_cutoff_preserves_correlated_tails_with_less_guard_work() {
    let mut cells = vec![0x0f_u64, 0x0f];
    cells.extend((1..8).map(|column| 0x0f_u64 << (column * 4)));
    let row_count = cells.len() as u32;
    let mut geometry = GeometrySolutionFamily::new();
    let mut root = FAMILY_EMPTY;
    for row in (2..row_count).rev() {
        root = geometry.append(row, root).unwrap();
    }
    root = geometry.append(0, root).unwrap();
    let required = cells.iter().copied().fold(0, |mask, cells| mask | cells);
    let limits = ZddLimits::default();
    let control = ExecutionControl::default();
    let mut direct = GeometryCompatibilityFilter::new(
        &geometry,
        root,
        cells.clone(),
        required,
        limits,
        &control,
    )
    .unwrap();
    assert_eq!(direct.guard_cutoffs[0], 2);
    assert_eq!(direct.guard_cutoffs[1], 1);
    assert!(direct.guard_cutoffs[2..].iter().all(|&cutoff| cutoff == 0));
    let mut reference =
        GeometryCompatibilityFilter::new(&geometry, root, cells, required, limits, &control)
            .unwrap();
    // This restores the former guard traversal through every row variable.
    // Both paths retain the same source-only suffix base case.
    reference.guard_cutoffs.fill(row_count);

    let mut arena = ZddArena::new(limits);
    let source_a = singleton(&mut arena, &[row_count]);
    let source_b = singleton(&mut arena, &[row_count + 1]);
    // The highest row selects its original source. Earlier tail rows are
    // optional, including equal low/high branches required by ZDD semantics.
    let mut tail = arena.mk(row_count - 1, source_a, source_b).unwrap();
    for row in (2..row_count - 1).rev() {
        tail = arena.mk(row, tail, tail).unwrap();
    }
    let with_conflict = arena.mk(1, tail, tail).unwrap();
    let joint = arena.mk(0, with_conflict, with_conflict).unwrap();
    let expected = arena.mk(0, tail, tail).unwrap();
    let before_direct = direct.work_steps();
    let actual = direct.filter_extendible(&mut arena, joint).unwrap();
    let before_reference = reference.work_steps();
    let previous = reference.filter_extendible(&mut arena, joint).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(previous, expected);
    assert_eq!(arena.count(joint), Ok(512));
    assert_eq!(arena.count(actual), Ok(256));
    assert!(arena.contains(actual, [row_count]).unwrap());
    assert!(!arena.contains(actual, [row_count + 1]).unwrap());
    assert!(arena
        .contains(actual, [row_count - 1, row_count + 1])
        .unwrap());
    assert!(!arena.contains(actual, [row_count - 1, row_count]).unwrap());
    assert!(direct.metrics().guard_suffix_elisions > 0);
    assert_eq!(reference.metrics().guard_suffix_elisions, 0);
    assert!(direct.metrics().guard_misses < reference.metrics().guard_misses);
    assert!(direct.work_steps() - before_direct < reference.work_steps() - before_reference);
}

#[test]
fn immutable_cover_filter_matches_down_and_explicit_correlated_members() {
    let (geometry, root, cells) = fixture();
    let limits = ZddLimits::default();
    let mut arena = ZddArena::new(limits);
    let mut joint = ZddId::EMPTY;
    let covers = [0b00011_u32, 0b01100, 0b10010];
    let mut expected = BTreeSet::new();
    for bits in 0..32_u32 {
        let mut member: Vec<_> = (0..5).filter(|&row| bits & (1 << row) != 0).collect();
        // The original source is correlated with the selected rows. It must
        // never be projected away or independently multiplied by a Product.
        member.push(5 + (bits.count_ones() & 1));
        let entry = singleton(&mut arena, &member);
        joint = arena.union(joint, entry).unwrap();
        if covers.iter().any(|cover| bits & !cover == 0) {
            expected.insert(member);
        }
    }
    let control = ExecutionControl::default();
    let mut direct =
        GeometryCompatibilityFilter::new(&geometry, root, cells.clone(), 0xff, limits, &control)
            .unwrap();
    let mut reference = GeometryDownView::new(&geometry, root, 5, limits).unwrap();
    reference.set_row_cells(&cells).unwrap();
    let filtered = direct.filter_extendible(&mut arena, joint).unwrap();
    assert_eq!(reference.filter_extendible(&mut arena, joint), Ok(filtered));
    let rows = arena.project_prefix(joint, 5).unwrap();
    let allowed = direct.filter_extendible(&mut arena, rows).unwrap();
    assert_eq!(arena.filter_prefix(joint, allowed, 5), Ok(filtered));
    assert_eq!(
        arena
            .page(filtered, 0, 128)
            .unwrap()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        expected
    );
    let mut roots = [joint, filtered];
    arena.compact(&mut roots).unwrap();
    assert_eq!(direct.filter_extendible(&mut arena, roots[0]), Ok(roots[1]));
    direct.clear_memo();
    assert_eq!(direct.filter_extendible(&mut arena, roots[0]), Ok(roots[1]));

    // Numeric IDs and epoch zero belong to a different arena here. A memo
    // may be reused only for the same stable owner, even after an arena move.
    let mut other = ZddArena::new(limits);
    let incompatible = singleton(&mut other, &[0, 4, 5]);
    assert_eq!(
        direct.filter_extendible(&mut other, incompatible),
        Ok(ZddId::EMPTY)
    );
    let compatible = singleton(&mut other, &[4, 6]);
    assert_eq!(
        direct.filter_extendible(&mut other, compatible),
        Ok(compatible)
    );
    let mut third = ZddArena::new(limits);
    let different_member_same_id = singleton(&mut third, &[2, 3, 6]);
    assert_eq!(incompatible, different_member_same_id);
    assert_eq!(
        direct.filter_extendible(&mut third, different_member_same_id),
        Ok(different_member_same_id),
    );
}

#[test]
fn union_remainder_keeps_shared_and_exclusive_row_source_members() {
    let mut geometry = GeometrySolutionFamily::new();
    let left_tail = geometry.append(1, FAMILY_EMPTY).unwrap();
    let right_tail = geometry.append(2, FAMILY_EMPTY).unwrap();
    let left = geometry.append(0, left_tail).unwrap();
    let right = geometry.append(0, right_tail).unwrap();
    let root = geometry.union(left, right).unwrap();
    // Rows 1 and 2 have identical cells but distinct identities. The covers
    // are {0,1} and {0,2}; their partial families overlap at {0} and empty.
    let cells = vec![0x0f, 0xf0, 0xf0];
    let limits = ZddLimits::default();
    let mut arena = ZddArena::new(limits);
    let shared = singleton(&mut arena, &[0, 3]);
    let only_left = singleton(&mut arena, &[1, 4]);
    let only_right = singleton(&mut arena, &[2, 5]);
    let rejected = singleton(&mut arena, &[1, 2, 6]);
    let mut expected = arena.union(shared, only_left).unwrap();
    expected = arena.union(expected, only_right).unwrap();
    let joint = arena.union(expected, rejected).unwrap();
    let mut direct = GeometryCompatibilityFilter::new(
        &geometry,
        root,
        cells.clone(),
        0xff,
        limits,
        &ExecutionControl::default(),
    )
    .unwrap();
    let actual = direct.filter_extendible(&mut arena, joint).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(arena.count(actual), Ok(3));
    assert_eq!(arena.count(joint), Ok(4));
    assert!(direct.metrics().union_remainder_calls > 0);
    assert!(direct.metrics().union_remainder_work_steps > 0);
    let mut reference = GeometryDownView::new(&geometry, root, 3, limits).unwrap();
    reference.set_row_cells(&cells).unwrap();
    assert_eq!(reference.filter_extendible(&mut arena, joint), Ok(actual));
    // The right branch's surviving source is its own original suffix, not
    // one taken from a row-independent union of the left and right sources.
    assert!(arena.contains(actual, [2, 5]).unwrap());
    assert!(!arena.contains(actual, [2, 3]).unwrap());
    assert!(!arena.contains(actual, [1, 2, 6]).unwrap());
}

#[test]
fn certificate_rejects_missing_cells_overlap_and_foreign_catalog_rows() {
    let control = ExecutionControl::default();
    let limits = ZddLimits::default();
    let (geometry, root, mut cells) = fixture();
    cells[4] = 0;
    assert!(matches!(
        GeometryCompatibilityFilter::new(&geometry, root, cells, 255, limits, &control,),
        Err(ZddError::InvalidGeometry)
    ));
    assert!(matches!(
        GeometryCompatibilityFilter::new(
            &geometry,
            root,
            vec![0x0f, 0xf0, 0x55, 0xaa, 0x10f],
            255,
            limits,
            &control,
        ),
        Err(ZddError::InvalidGeometry)
    ));

    let mut geometry = GeometrySolutionFamily::new();
    let a = geometry.append(0, FAMILY_EMPTY).unwrap();
    let overlap = geometry.append(1, a).unwrap();
    assert!(matches!(
        GeometryCompatibilityFilter::new(&geometry, overlap, vec![3, 6], 7, limits, &control,),
        Err(ZddError::InvalidGeometry)
    ));

    let mut geometry = GeometrySolutionFamily::new();
    let a = geometry.append(0, FAMILY_EMPTY).unwrap();
    let b = geometry.append(1, FAMILY_EMPTY).unwrap();
    let unequal_union = geometry.union(a, b).unwrap();
    assert!(matches!(
        GeometryCompatibilityFilter::new(
            &geometry,
            unequal_union,
            vec![3, 12],
            15,
            limits,
            &control,
        ),
        Err(ZddError::InvalidGeometry)
    ));

    let mut geometry = GeometrySolutionFamily::new();
    let a = geometry.append(0, FAMILY_EMPTY).unwrap();
    let b = geometry.append(1, FAMILY_EMPTY).unwrap();
    let overlapping_product = geometry.product(a, b).unwrap();
    assert!(matches!(
        GeometryCompatibilityFilter::new(
            &geometry,
            overlapping_product,
            vec![3, 6],
            7,
            limits,
            &control,
        ),
        Err(ZddError::InvalidGeometry)
    ));
}

#[test]
fn empty_geometry_and_explicit_refusal_never_publish_a_false_success() {
    let geometry = GeometrySolutionFamily::new();
    let control = ExecutionControl::default();
    let mut arena = ZddArena::new(ZddLimits::default());
    let input = singleton(&mut arena, &[0, 1]);
    let mut empty = GeometryCompatibilityFilter::new(
        &geometry,
        FAMILY_INVALID,
        vec![15],
        15,
        ZddLimits::default(),
        &control,
    )
    .unwrap();
    assert_eq!(empty.filter_extendible(&mut arena, input), Ok(ZddId::EMPTY));

    let (geometry, root, cells) = fixture();
    assert!(matches!(
        GeometryCompatibilityFilter::new(
            &geometry,
            root,
            cells.clone(),
            255,
            ZddLimits {
                max_work: 1,
                ..ZddLimits::default()
            },
            &control,
        ),
        Err(ZddError::WorkLimit)
    ));
    control.cancellation.handle().cancel();
    assert!(matches!(
        GeometryCompatibilityFilter::new(
            &geometry,
            root,
            cells,
            255,
            ZddLimits::default(),
            &control,
        ),
        Err(ZddError::Cancelled)
    ));
}
