//! Test-only observed graph comparison with independent forward lock enumeration.
use super::super::reachability::{
    search_reachable_locks, ReachabilityScratch, ReachabilityTemplate,
};
use super::*;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use std::collections::{BTreeMap, BTreeSet};

type Lock = (RotationState, i8, i8, u64, u8);

fn cells_from_hash(hash: u64) -> u64 {
    let mut result = 0;
    for y in 0..4 {
        for x in 0..10 {
            if hash & (1 << (y * 10 + 9 - x)) != 0 {
                result |= 1 << (y * 10 + x);
            }
        }
    }
    result
}

// Enumerate forward motion, not Geometry skeletons or inverse graph edges.
// The expected transition and clear mask are derived by explicit row packing.
fn forward_locks(source: u64, piece: PieceKind) -> BTreeMap<u64, BTreeSet<Lock>> {
    let prefix = (0..4)
        .take_while(|row| (source >> (row * 10)) & 1023 == 1023)
        .count();
    let height = 4 - prefix as u8;
    let physical = source >> (prefix * 10);
    let template = ReachabilityTemplate::compile(10, height, piece, KickTableProfileId::Jstris180);
    let reachable = search_reachable_locks(
        &template,
        physical,
        &mut ReachabilityScratch::default(),
        None,
    );
    assert!(reachable.exhaustive);
    let definition = standard_tetromino_registry().get(piece).unwrap();
    let mut result: BTreeMap<u64, BTreeSet<Lock>> = BTreeMap::new();
    for rotation in RotationState::ALL {
        let shape = definition.shape(rotation);
        if shape.height() > height {
            continue;
        }
        for y in 0..=(height - shape.height()) {
            for x in 0..=(10 - shape.width()) {
                if !reachable.locks.contains(10, rotation, x as i8, y as i8) {
                    continue;
                }
                let mut lock = 0_u64;
                for cell in shape.cells() {
                    lock |= 1
                        << ((u32::from(y) + cell.y() as u32) * 10 + u32::from(x) + cell.x() as u32);
                }
                assert_eq!(lock & physical, 0);
                assert!(y == 0 || (lock >> 10) & physical != 0);
                let merged = physical | lock;
                let mut compact = 0;
                let mut kept = 0;
                let mut clear = 0_u8;
                for row in 0..height {
                    let cells = (merged >> (row * 10)) & 1023;
                    if cells == 1023 {
                        clear |= 1 << row;
                    } else {
                        compact |= cells << (kept * 10);
                        kept += 1;
                    }
                }
                let shift = (prefix as u32 + clear.count_ones()) * 10;
                let normalized = (compact << shift) | ((1_u64 << shift) - 1);
                result
                    .entry(normalized)
                    .or_default()
                    .insert((rotation, x as i8, y as i8, lock, clear));
            }
        }
    }
    result
}

// A fully occupied column remains a wall after every row clear (the active
// target height shrinks too). A tetromino cannot cross it. Each strip therefore
// needs a multiple of four empty cells, independently of kicks and line order.
// This is only a test-side impossibility proof, not a new production filter.
fn indivisible_full_column_strip(cells: u64) -> bool {
    let mut empty = 0;
    for x in 0..10 {
        let vacancies = (0..4).filter(|y| cells & (1 << (y * 10 + x)) == 0).count();
        if vacancies == 0 {
            if empty % 4 != 0 {
                return true;
            }
            empty = 0;
        } else {
            empty += vacancies;
        }
    }
    empty % 4 != 0
}

// Over-approximate every possible inverse-clear tetromino independently of
// GeometryCatalog: preserve horizontal coordinates and lift its occupied rows
// into every increasing subset of the remaining target rows. We intentionally
// ignore whether intervening rows can actually clear, piece supply and kicks.
// Thus an exact-cover failure proves PC impossibility, but success proves none.
fn no_projected_tetromino_cover(source: u64) -> bool {
    use std::collections::HashSet;
    let prefix = (0..4)
        .take_while(|row| (source >> (row * 10)) & 1023 == 1023)
        .count();
    let height = 4 - prefix;
    let source = source >> (prefix * 10);
    let empty = ((1_u64 << (height * 10)) - 1) & !source;
    let mut masks = BTreeSet::new();
    for piece in [
        PieceKind::I,
        PieceKind::J,
        PieceKind::L,
        PieceKind::O,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        let definition = standard_tetromino_registry().get(piece).unwrap();
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for rows in 1_u8..(1 << height) {
                if rows.count_ones() != u32::from(shape.height()) {
                    continue;
                }
                let lifted: Vec<_> = (0..height).filter(|r| rows & (1 << r) != 0).collect();
                for x in 0..=(10 - shape.width()) {
                    let mut mask = 0_u64;
                    for cell in shape.cells() {
                        mask |= 1
                            << (lifted[cell.y() as usize] * 10
                                + usize::from(x)
                                + cell.x() as usize);
                    }
                    if mask & !empty == 0 {
                        masks.insert(mask);
                    }
                }
            }
        }
    }
    let supports: [Vec<u64>; 40] = std::array::from_fn(|i| {
        masks
            .iter()
            .copied()
            .filter(|m| m & (1 << i) != 0)
            .collect()
    });
    fn cover(empty: u64, supports: &[Vec<u64>; 40], dead: &mut HashSet<u64>) -> bool {
        if empty == 0 {
            return true;
        }
        if dead.contains(&empty) {
            return false;
        }
        // A test budget failure is not a negative certificate.
        assert!(
            dead.len() < 250_000,
            "projected-cover oracle exhausted its fixture budget"
        );
        let mut cells = empty;
        let mut selected = None;
        let mut size = usize::MAX;
        while cells != 0 {
            let bit = cells.trailing_zeros() as usize;
            cells &= cells - 1;
            let count = supports[bit]
                .iter()
                .filter(|mask| **mask & !empty == 0)
                .count();
            if count == 0 {
                dead.insert(empty);
                return false;
            }
            if count < size {
                size = count;
                selected = Some(bit);
            }
        }
        for mask in &supports[selected.unwrap()] {
            if mask & !empty == 0 && cover(empty ^ mask, supports, dead) {
                return true;
            }
        }
        dead.insert(empty);
        false
    }
    !cover(empty, &supports, &mut HashSet::new())
}

#[test]
fn negative_oracles_do_not_confuse_row_lifting_or_tileable_fields_with_failure() {
    let full = (1_u64 << 40) - 1;
    let projected_o = 3 | (3 << 20);
    let four_corners = 1 | (1 << 9) | (1 << 30) | (1 << 39);
    assert!(!no_projected_tetromino_cover(0));
    assert!(!no_projected_tetromino_cover(full));
    assert!(!no_projected_tetromino_cover(full ^ projected_o));
    assert!(no_projected_tetromino_cover(full ^ four_corners));
    assert!(indivisible_full_column_strip(2149584127));
    assert!(!indivisible_full_column_strip(0));
    assert!(!indivisible_full_column_strip(
        full ^ (1 | (1 << 10) | (1 << 20) | (1 << 30))
    ));
}

#[test]
fn observed_nonempty_edges_are_exact_but_profile_completeness_is_unqualified() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/pc4-hf-nonempty-20260913.json"
    ))
    .unwrap();
    let mut edges = 0;
    let mut clears = 0;
    let mut upper_clears = 0;
    let mut nonmonotone = 0;
    let mut excluded_dead_strips = 0;
    let mut excluded_no_cover = 0;
    let mut unexplained = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        // Keep the independently observed index miss in the research record;
        // it is not an empty outgoing set or an unsatisfiability certificate.
        if case["source_id"].is_null() {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let source = cells_from_hash(case["source_hash"].as_u64().unwrap());
        for (key, piece) in [
            ("I", PieceKind::I),
            ("J", PieceKind::J),
            ("L", PieceKind::L),
            ("O", PieceKind::O),
            ("S", PieceKind::S),
            ("T", PieceKind::T),
            ("Z", PieceKind::Z),
        ] {
            let expected = forward_locks(source, piece);
            let mut observed = BTreeSet::new();
            for target in case["pieces"][key].as_array().unwrap() {
                let cells = cells_from_hash(target["hash"].as_u64().unwrap());
                assert!(observed.insert(cells), "duplicate {name} {key}");
                let placements = materialize_pc4_ilc_transition(
                    source,
                    cells,
                    piece,
                    KickTableProfileId::Jstris180,
                )
                .unwrap();
                let materialized: BTreeSet<_> = placements
                    .iter()
                    .map(|p| {
                        (
                            p.rotation(),
                            p.x(),
                            p.y(),
                            p.occupied_cells() >> (p.source_cleared_prefix() * 10),
                            p.physical_cleared_rows(),
                        )
                    })
                    .collect();
                assert!(!materialized.is_empty(), "unrealized {name} {key} {cells}");
                assert_eq!(
                    Some(&materialized),
                    expected.get(&cells),
                    "locks {name} {key} {cells}"
                );
                edges += 1;
                clears += usize::from(placements.iter().any(|p| p.physical_cleared_rows() != 0));
                upper_clears += usize::from(
                    placements
                        .iter()
                        .any(|p| p.physical_cleared_rows() & !1 != 0),
                );
                nonmonotone += usize::from(source & !cells != 0);
            }
            // Do not silently intersect two sets. Classify every omitted edge;
            // unresolved omissions prevent a full-solution qualification.
            for cells in expected.keys().filter(|cells| !observed.contains(cells)) {
                if indivisible_full_column_strip(*cells) {
                    excluded_dead_strips += 1;
                } else if no_projected_tetromino_cover(*cells) {
                    excluded_no_cover += 1;
                } else {
                    unexplained.push(format!("{name} {key} {cells}"));
                }
            }
        }
    }
    eprintln!(
        "hf_nonempty materialized_edges={edges} clearing_edges={clears} upper_clearing_edges={upper_clears} nonmonotone_edges={nonmonotone} excluded_dead_strips={excluded_dead_strips} excluded_no_cover={excluded_no_cover} qualification=not_qualified unresolved={unexplained:?}"
    );
    assert_eq!((edges, clears, upper_clears, nonmonotone), (194, 11, 5, 4));
    assert_eq!((excluded_dead_strips, excluded_no_cover), (109, 48));
    // Preserve the exact unresolved comparison, not a passing completeness
    // claim or an allowlist for production. A future qualification must prove
    // these omissions harmless, or provide every missing viable transition.
    assert_eq!(
        unexplained,
        [
            "two-horizontal-I S 805699839",
            "two-horizontal-I S 2153779455",
            "two-horizontal-I T 14684415",
            "two-horizontal-I T 269353215",
            "two-horizontal-I Z 6303999",
            "one-cleared-row-six-live-cells S 402915327",
            "one-cleared-row-six-live-cells T 469958655",
            "one-cleared-row-six-live-cells Z 275280756735",
            "tileable-upper-clear J 470154457",
            "tileable-upper-clear L 275347799257",
            "tileable-upper-clear S 805829849",
            "tileable-upper-clear S 412518317273",
            "tileable-upper-clear T 268658687",
        ]
    );
}
