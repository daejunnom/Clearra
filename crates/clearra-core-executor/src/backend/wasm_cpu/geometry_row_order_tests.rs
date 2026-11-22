//! Independent finite oracles for the 24-order negative-only capability.
//! Expected bits inspect every concrete realization and rank comparison;
//! they do not use the product's compiled requirement-domain lookup.

use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::row_order::{
    mask_for_requirement_alternatives, FamilyRowOrderBuilder, FamilyRowOrderTable,
    RowOrderBuildAdvance, RowOrderCapability, ROW_ORDER_ALL,
};
use super::{
    prefix_pruning::selected_prefix_stalls, FamilyNodeKind, GeometryCatalog,
    GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID,
};

const INTERNAL_ROWS: u16 = 0b01_1110;

fn full6_catalog() -> GeometryCatalog {
    GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, (1_u64 << 60) - 1).unwrap()
}

/// A separate, deliberately simple lexicographic permutation enumeration.
fn reference_orders() -> Vec<[u8; 4]> {
    let mut result = Vec::new();
    for a in 1..=4 {
        for b in 1..=4 {
            if b == a {
                continue;
            }
            for c in 1..=4 {
                if c == a || c == b {
                    continue;
                }
                for d in 1..=4 {
                    if d != a && d != b && d != c {
                        result.push([a, b, c, d]);
                    }
                }
            }
        }
    }
    assert_eq!(result.len(), 24);
    assert_eq!(result.first(), Some(&[1, 2, 3, 4]));
    assert_eq!(result.last(), Some(&[4, 3, 2, 1]));
    result
}

fn reference_mask(occupied_rows: u16, alternatives: &[u16]) -> u32 {
    assert_eq!(occupied_rows & !0b11_1111, 0);
    assert!(alternatives
        .iter()
        .all(|required| required & !INTERNAL_ROWS == 0));
    let mut mask = 0_u32;
    for (bit, order) in reference_orders().into_iter().enumerate() {
        let mut rank = [0; 6];
        for (index, row) in order.into_iter().enumerate() {
            rank[usize::from(row)] = index;
        }
        let possible = alternatives.iter().any(|&required| {
            (1..=4).all(|source| {
                required & (1 << source) == 0
                    || (1..=4).all(|destination| {
                        occupied_rows & (1 << destination) == 0 || rank[source] < rank[destination]
                    })
            })
        });
        if possible {
            mask |= 1 << bit;
        }
    }
    mask
}

fn reference_occupied_rows(catalog: &GeometryCatalog, row: u32) -> u16 {
    let mut occupied = 0_u16;
    let cells = catalog.skeleton(row).cells;
    for index in 0..u32::from(catalog.width()) * u32::from(catalog.height()) {
        if cells & (1_u64 << index) != 0 {
            occupied |= 1 << (index / u32::from(catalog.width()));
        }
    }
    occupied
}

fn reference_row_mask(catalog: &GeometryCatalog, row: u32) -> u32 {
    let alternatives = catalog
        .realizations(row)
        .iter()
        .map(|pose| pose.required_deleted_rows)
        .collect::<Vec<_>>();
    reference_mask(reference_occupied_rows(catalog, row), &alternatives)
}

fn o_row(catalog: &GeometryCatalog, lower: u8, upper: u8, x: u8) -> u32 {
    let width = u32::from(catalog.width());
    let cells = (3_u64 << (u32::from(lower) * width + u32::from(x)))
        | (3_u64 << (u32::from(upper) * width + u32::from(x)));
    catalog.skeleton_id(PieceKind::O, cells).unwrap()
}

#[test]
fn row_order_capability_matches_every_full6l_catalog_realization() {
    let catalog = full6_catalog();
    let capability = RowOrderCapability::try_compile(&catalog).unwrap();
    assert_eq!(ROW_ORDER_ALL, (1_u32 << 24) - 1);
    assert_eq!(capability.selected_mask(&[]), Some(ROW_ORDER_ALL));
    let mut nontrivial_rows = 0;
    let mut concrete_poses = 0;
    for row in 0..catalog.skeleton_count() as u32 {
        assert!(!catalog.realizations(row).is_empty());
        // Diagnose the actual empty-board Full6 catalog structure without
        // granting an implementation shortcut: all concrete alternatives
        // must still remain in the product's positive requirement OR.
        let occupied = reference_occupied_rows(&catalog, row);
        assert_ne!(occupied, 0);
        let first = occupied.trailing_zeros();
        let last = u16::BITS - 1 - occupied.leading_zeros();
        let span = ((1_u16 << (last - first + 1)) - 1) << first;
        let gaps = span & !occupied;
        for pose in catalog.realizations(row) {
            assert_eq!(pose.required_deleted_rows & !INTERNAL_ROWS, 0);
            assert_eq!(pose.required_deleted_rows, gaps, "row={row}");
            concrete_poses += 1;
        }
        let actual = capability.row_mask(row).unwrap();
        assert_eq!(actual & !ROW_ORDER_ALL, 0);
        assert_eq!(actual, reference_row_mask(&catalog, row), "row={row}");
        nontrivial_rows += usize::from(actual != ROW_ORDER_ALL);
    }
    assert!(concrete_poses > catalog.skeleton_count());
    assert!(nontrivial_rows > 0);
    eprintln!(
        "row_order_catalog skeletons={} realizations={} nontrivial_rows={}",
        catalog.skeleton_count(),
        concrete_poses,
        nontrivial_rows
    );
    assert_eq!(capability.row_mask(catalog.skeleton_count() as u32), None);
    assert_eq!(capability.selected_mask(&[u32::MAX]), None);

    // This O requires every internal row, but occupies only boundary rows.
    // It can wait until all internal rows are processed in any order.
    let outside_only = o_row(&catalog, 0, 5, 0);
    assert!(catalog
        .realizations(outside_only)
        .iter()
        .all(|pose| pose.required_deleted_rows == INTERNAL_ROWS));
    assert_eq!(capability.row_mask(outside_only), Some(ROW_ORDER_ALL));
}

fn compare_every_selected_subset(
    catalog: &GeometryCatalog,
    capability: &RowOrderCapability,
    rows: &[u32],
) -> (usize, usize) {
    let mut selected = Vec::with_capacity(rows.len());
    let mut passes = 0;
    let mut stalls = 0;
    for subset in 0..1_usize << rows.len() {
        selected.clear();
        selected.extend(
            rows.iter()
                .copied()
                .enumerate()
                .filter_map(|(index, row)| (subset & (1 << index) != 0).then_some(row)),
        );
        let actual = capability.selected_mask(&selected).unwrap();
        let concrete_stalls = selected_prefix_stalls(catalog, &selected);
        assert_eq!(actual == 0, concrete_stalls, "selected subset={subset:#x}");
        passes += usize::from(!concrete_stalls);
        stalls += usize::from(concrete_stalls);
        selected.reverse();
        assert_eq!(capability.selected_mask(&selected), Some(actual));
        assert_eq!(selected_prefix_stalls(catalog, &selected), concrete_stalls);
    }
    (passes, stalls)
}

#[test]
fn row_order_matches_concrete_closure_for_full15_physical_and_ilc_subsets() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let catalog = full6_catalog();
    let capability = RowOrderCapability::try_compile(&catalog).unwrap();
    // The executed mixed15 physical witness: lower four rows clear together,
    // then the upper pair. Its exact SRS+ graph and P1P7P7 source intersection
    // are checked by the separate buildup regression, not assumed here.
    let mixed = [
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
    let mixed_rows = mixed
        .iter()
        .map(|&(piece, cells)| catalog.skeleton_id(piece, cells).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        compare_every_selected_subset(&catalog, &capability, &mixed_rows),
        (32_768, 0)
    );

    // A second, genuinely gapped 15-row geometric tiling exercises both
    // sides of the equivalence. Its all-O inventory is intentionally only a
    // catalog/closure stress case, not a P1P7P7 success claim.
    let pairs = [
        [(0, 2), (1, 3), (4, 5)],
        [(0, 1), (2, 4), (3, 5)],
        [(0, 3), (1, 4), (2, 5)],
        [(0, 5), (1, 2), (3, 4)],
        [(0, 1), (2, 3), (4, 5)],
    ];
    let mut gapped = Vec::new();
    let mut covered = 0_u64;
    for (column_pair, rows) in pairs.into_iter().enumerate() {
        for (lower, upper) in rows {
            let row = o_row(&catalog, lower, upper, (column_pair * 2) as u8);
            let cells = catalog.skeleton(row).cells;
            assert_eq!(covered & cells, 0);
            covered |= cells;
            gapped.push(row);
        }
    }
    assert_eq!(gapped.len(), 15);
    assert_eq!(covered, (1_u64 << 60) - 1);
    let (passes, stalls) = compare_every_selected_subset(&catalog, &capability, &gapped);
    assert!(passes > 0 && stalls > 0);
    assert_eq!(passes + stalls, 32_768);
}

#[test]
fn row_order_preserves_realization_or_and_common_order_conjunction() {
    let occupied = 1 << 2;
    let first = mask_for_requirement_alternatives(occupied, &[1 << 1]).unwrap();
    let alternative = mask_for_requirement_alternatives(occupied, &[1 << 3]).unwrap();
    let either = mask_for_requirement_alternatives(occupied, &[1 << 1, 1 << 3]).unwrap();
    let both_required =
        mask_for_requirement_alternatives(occupied, &[(1 << 1) | (1 << 3)]).unwrap();
    let reverse = mask_for_requirement_alternatives(1 << 1, &[1 << 2]).unwrap();
    assert_eq!(first, reference_mask(occupied, &[1 << 1]));
    assert_eq!(alternative, reference_mask(occupied, &[1 << 3]));
    assert_eq!(either, reference_mask(occupied, &[1 << 1, 1 << 3]));
    assert_eq!(
        both_required,
        reference_mask(occupied, &[(1 << 1) | (1 << 3)])
    );
    assert_eq!(either, first | alternative);
    assert!(first != 0 && reverse != 0);
    assert_eq!(
        first & reverse,
        0,
        "different per-child orders do not form a Product witness"
    );
    assert_ne!(
        either & reverse,
        0,
        "the second realization supplies a compatible order"
    );
    assert_eq!(
        both_required & reverse,
        0,
        "OR alternatives cannot be merged into AND requirements"
    );
    assert_eq!((either & reverse).count_ones(), 4);

    assert_eq!(mask_for_requirement_alternatives(occupied, &[]), Some(0));
    assert_eq!(
        mask_for_requirement_alternatives(occupied, &[0]),
        Some(ROW_ORDER_ALL)
    );
    assert_eq!(
        mask_for_requirement_alternatives((1 << 0) | (1 << 5), &[INTERNAL_ROWS]),
        Some(ROW_ORDER_ALL),
    );
    assert_eq!(mask_for_requirement_alternatives(1 << 6, &[0]), None);
    assert_eq!(mask_for_requirement_alternatives(occupied, &[1 << 0]), None);
    assert_eq!(mask_for_requirement_alternatives(occupied, &[1 << 5]), None);
}

#[test]
fn row_order_capability_declines_unsupported_catalogs_without_negative_authority() {
    let tall =
        GeometryCatalog::compile_for_required_cells_on_dimensions(8, 7, 0, (1_u64 << 56) - 1)
            .unwrap();
    assert!((0..tall.skeleton_count() as u32).any(|row| {
        tall.realizations(row)
            .iter()
            .any(|pose| pose.required_deleted_rows & !INTERNAL_ROWS != 0)
    }));
    assert!(RowOrderCapability::try_compile(&tall).is_none());
    // Height six alone does not authorize a row that the full target never fills.
    let incomplete =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, 0xf).unwrap();
    assert!(RowOrderCapability::try_compile(&incomplete).is_none());
    let family = Arc::new(GeometrySolutionFamily::new());
    assert!(FamilyRowOrderBuilder::try_new(&tall, &family, None).is_none());
    assert!(FamilyRowOrderBuilder::try_new(&incomplete, &family, None).is_none());
}

fn enumerate_family_paths(family: &GeometrySolutionFamily, reference: u32) -> Vec<Vec<u32>> {
    if reference == FAMILY_INVALID {
        return Vec::new();
    }
    if reference == FAMILY_EMPTY {
        return vec![Vec::new()];
    }
    let node = family.node(reference).unwrap();
    match node.kind {
        FamilyNodeKind::Append => enumerate_family_paths(family, node.left)
            .into_iter()
            .map(|mut path| {
                path.insert(0, node.row_id);
                path
            })
            .collect(),
        FamilyNodeKind::Union => {
            let mut paths = enumerate_family_paths(family, node.left);
            paths.extend(enumerate_family_paths(family, node.right));
            paths
        }
        FamilyNodeKind::Product => {
            let left = enumerate_family_paths(family, node.left);
            let right = enumerate_family_paths(family, node.right);
            let mut paths = Vec::new();
            for a in &left {
                for b in &right {
                    let mut path = a.clone();
                    path.extend(b);
                    paths.push(path);
                }
            }
            paths
        }
    }
}

fn build_table(
    catalog: &GeometryCatalog,
    family: &Arc<GeometrySolutionFamily>,
) -> FamilyRowOrderTable {
    let mut builder = FamilyRowOrderBuilder::try_new(catalog, family, None).unwrap();
    for _ in 0..family.node_count() as usize + 2 {
        match builder.advance(family, 3) {
            RowOrderBuildAdvance::Pending => {}
            RowOrderBuildAdvance::Complete => return builder.finish().unwrap(),
            RowOrderBuildAdvance::Unavailable => panic!("small valid family must admit its table"),
        }
    }
    panic!("finite small table did not finish");
}

#[test]
fn row_order_family_masks_match_every_explicit_path_and_shared_product_choice() {
    let catalog = full6_catalog();
    let row_a = o_row(&catalog, 0, 2, 0);
    let row_b = o_row(&catalog, 1, 3, 2);
    let row_c = o_row(&catalog, 4, 5, 0);
    let row_d = o_row(&catalog, 0, 1, 6);
    let mut family = GeometrySolutionFamily::new();
    let a = family.append(row_a, FAMILY_EMPTY).unwrap();
    let b = family.append(row_b, FAMILY_EMPTY).unwrap();
    let c = family.append(row_c, FAMILY_EMPTY).unwrap();
    let d = family.append(row_d, FAMILY_EMPTY).unwrap();
    let alternatives = family.union(a, b).unwrap();
    let incompatible = family.product(a, b).unwrap();
    let reverse_chain = family.append(row_b, a).unwrap();
    let positive_product = family.product(alternatives, c).unwrap();
    let mixed = family.union(incompatible, positive_product).unwrap();
    let root = family.product(mixed, d).unwrap();
    // The same immutable family value is referenced from both arms. It does
    // not bind their Union choices to one hidden shared decision.
    let shared_left = family.product(alternatives, c).unwrap();
    let shared_right = family.product(alternatives, d).unwrap();
    let shared = family.product(shared_left, shared_right).unwrap();
    let family = Arc::new(family);
    let table = build_table(&catalog, &family);
    assert!(table.matches(&catalog, &family));
    assert_eq!(table.mask(FAMILY_INVALID), Some(0));
    assert_eq!(table.mask(FAMILY_EMPTY), Some(ROW_ORDER_ALL));
    assert_eq!(
        table.row_mask(row_a),
        Some(reference_row_mask(&catalog, row_a))
    );
    assert_eq!(table.selected_mask(&[row_a, row_b]), Some(0));
    assert_eq!(table.mask(a).unwrap() & table.mask(b).unwrap(), 0);
    assert_eq!(table.mask(incompatible), Some(0));
    assert_eq!(table.mask(reverse_chain), Some(0));
    assert_ne!(table.mask(alternatives), Some(0));
    assert_ne!(table.mask(root), Some(0));
    assert_ne!(table.mask(shared), Some(0));
    let reference_count = family.node_count() + 2;
    for reference in 0..reference_count {
        let expected = enumerate_family_paths(&family, reference)
            .iter()
            .fold(0, |mask, path| {
                mask | path.iter().fold(ROW_ORDER_ALL, |allowed, &row| {
                    allowed & reference_row_mask(&catalog, row)
                })
            });
        assert_eq!(
            table.mask(reference),
            Some(expected),
            "reference={reference}"
        );
    }
    assert_eq!(table.mask(reference_count), None);
    // A task's later Product continuation must share the same order too:
    // adding the incompatible continuation changes this nonzero task to zero.
    let prefix = table.prefix_state(&[row_a]).unwrap();
    assert_ne!(prefix & table.mask(c).unwrap() & table.mask(d).unwrap(), 0);
    assert_eq!(
        prefix & table.mask(c).unwrap() & table.mask(d).unwrap() & table.mask(b).unwrap(),
        0
    );
    assert_ne!(table.task_mask(prefix, c, &[d]).unwrap(), 0);
    assert_eq!(table.task_mask(prefix, c, &[d, b]), Some(0));
    assert_eq!(table.task_mask(prefix, c, &[reference_count]), None);
}

#[test]
fn row_order_first_lock_combines_union_product_prefix_and_continuations() {
    let catalog = full6_catalog();
    let floating = o_row(&catalog, 4, 5, 0);
    let floor = o_row(&catalog, 0, 1, 6);
    let gap_a = o_row(&catalog, 0, 2, 0);
    let gap_b = o_row(&catalog, 1, 3, 2);
    let mut family = GeometrySolutionFamily::new();
    let tail = family
        .append(floating, FAMILY_EMPTY)
        .expect("floating leaf");
    let grounded = family.append(floor, FAMILY_EMPTY).expect("grounded leaf");
    let deferred = family.append(gap_a, tail).expect("gapped prefix");
    let choice = family
        .union(deferred, grounded)
        .expect("union alternatives");
    let product = family.product(tail, grounded).expect("product components");
    let appended = family.append(floor, tail).expect("grounded prefix");
    let cycle_a = family.append(gap_a, FAMILY_EMPTY).expect("first cycle row");
    let cycle = family.append(gap_b, cycle_a).expect("second cycle row");
    let grounded_cycle = family.product(cycle, grounded).expect("grounded cycle");
    let family = Arc::new(family);
    let table = build_table(&catalog, &family);
    let empty = table.prefix_state(&[]).expect("empty prefix state");
    let prefix = table.prefix_state(&[floor]).expect("grounded prefix state");
    assert_eq!(table.extend_prefix(empty, floor), Some(prefix));
    assert_ne!(table.mask(tail), Some(0));
    assert_ne!(table.mask(deferred), Some(0));
    assert_eq!(table.task_mask(empty, tail, &[]), Some(0));
    assert_eq!(table.task_mask(empty, deferred, &[]), Some(0));
    assert_eq!(table.task_mask(empty, FAMILY_EMPTY, &[]), Some(0));
    for reference in [choice, product, appended] {
        assert_ne!(table.task_mask(empty, reference, &[]), Some(0));
    }
    // The first lock may be in a later Product continuation or in the fixed
    // selected prefix. Requiring each operand to contain one would lose paths.
    assert_ne!(table.task_mask(empty, tail, &[grounded]), Some(0));
    assert_ne!(table.task_mask(prefix, tail, &[]), Some(0));
    assert_ne!(table.task_mask(prefix, FAMILY_EMPTY, &[]), Some(0));
    // A possible first lock cannot rescue an incompatible common row order.
    assert_eq!(table.task_mask(empty, grounded_cycle, &[]), Some(0));
    assert_eq!(table.task_mask(prefix, cycle, &[]), Some(0));
    assert_eq!(family.path_count(choice), Some(2));
    assert_eq!(family.path_count(product), Some(1));
}

#[test]
fn row_order_first_lock_nonempty_board_keeps_unknown_feasibility() {
    let initial = 3_u64 | (3_u64 << 10);
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        6,
        initial,
        ((1_u64 << 60) - 1) & !initial,
    )
    .expect("complete nonempty six-row catalog");
    let supported = o_row(&catalog, 2, 3, 0);
    let floating = o_row(&catalog, 4, 5, 4);
    assert!(catalog
        .instantiations(supported, 0)
        .any(|pose| { pose.lock_y == 2 && initial & (pose.lock_mask >> 10) != 0 }));
    let mut family = GeometrySolutionFamily::new();
    let supported = family
        .append(supported, FAMILY_EMPTY)
        .expect("supported leaf");
    let floating = family
        .append(floating, FAMILY_EMPTY)
        .expect("floating leaf");
    let family = Arc::new(family);
    let table = build_table(&catalog, &family);
    let empty = table.prefix_state(&[]).expect("empty prefix state");
    for reference in [supported, floating] {
        // No floor-only negative inference is made on a nonempty board,
        // even when this particular leaf has no obvious supporting cell.
        assert_eq!(
            table.task_mask(empty, reference, &[]),
            table.mask(reference)
        );
        assert_ne!(table.task_mask(empty, reference, &[]), Some(0));
    }
}
