//! Differential reference for the former lookup-then-insert interner.
//! These tests compare the entire index, not merely counts or hash equality.

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};

use super::super::{catalog::GeometryCatalog, mix_digest, mix_order_independent_candidate_digest};
use super::*;

fn append_node(row_id: u32, suffix: u32) -> FamilyNode {
    FamilyNode {
        left: suffix,
        right: FAMILY_INVALID,
        row_id,
        kind: FamilyNodeKind::Append,
        reserved: [0; 3],
    }
}

/// The old algorithm, including its second hash/probe and original growth order.
fn reference_intern(family: &mut GeometrySolutionFamily, node: FamilyNode) -> Option<u32> {
    if !family.interning_disabled && !family.intern_slots.is_empty() {
        let hash = family.hash_node(node);
        let mask = family.intern_slots.len() - 1;
        let mut slot = hash as usize & mask;
        for _ in 0..family.intern_slots.len() {
            family.intern_work.lookup_slots += 1;
            let reference = family.intern_slots[slot];
            if reference == FAMILY_INVALID {
                break;
            }
            if family.node(reference) == Some(node) {
                return Some(reference);
            }
            slot = (slot + 1) & mask;
        }
    }
    let reference = family.push_node(node)?;
    if !family.interning_disabled {
        family.ensure_intern_capacity();
        if !family.interning_disabled {
            let hash = family.hash_node(node);
            family.insert_interned(reference, hash);
        }
    }
    Some(reference)
}

struct Builder {
    family: GeometrySolutionFamily,
    reference: bool,
}

impl Builder {
    fn new(reference: bool, slots: usize) -> Self {
        assert!(slots.is_power_of_two());
        let mut family = GeometrySolutionFamily::new();
        // A small test-only table reaches the same collision/growth/retirement
        // transitions without constructing thousands of irrelevant nodes.
        family.intern_slots = vec![FAMILY_INVALID; slots];
        family.interning_disabled = false;
        Self { family, reference }
    }

    fn append(&mut self, row: u32, suffix: u32) -> Option<u32> {
        if !self.reference {
            return self.family.append(row, suffix);
        }
        if suffix == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        reference_intern(&mut self.family, append_node(row, suffix))
    }

    fn union(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if !self.reference {
            return self.family.union(left, right);
        }
        if left == FAMILY_INVALID {
            return Some(right);
        }
        if right == FAMILY_INVALID || left == right {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        reference_intern(
            &mut self.family,
            FamilyNode {
                left,
                right,
                row_id: 0,
                kind: FamilyNodeKind::Union,
                reserved: [0; 3],
            },
        )
    }

    fn product(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if !self.reference {
            return self.family.product(left, right);
        }
        if left == FAMILY_INVALID || right == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        if left == FAMILY_EMPTY {
            return Some(right);
        }
        if right == FAMILY_EMPTY {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        reference_intern(
            &mut self.family,
            FamilyNode {
                left,
                right,
                row_id: 0,
                kind: FamilyNodeKind::Product,
                reserved: [0; 3],
            },
        )
    }
}

fn assert_same(actual: &GeometrySolutionFamily, reference: &GeometrySolutionFamily) {
    assert_eq!(actual.node_count, reference.node_count);
    assert_eq!(
        actual.intern_slots, reference.intern_slots,
        "exact slot layout"
    );
    assert_eq!(
        actual.intern_slots.capacity(),
        reference.intern_slots.capacity()
    );
    assert_eq!(actual.intern_count, reference.intern_count);
    assert_eq!(actual.intern_generation, reference.intern_generation);
    assert_eq!(actual.interning_disabled, reference.interning_disabled);
    assert_eq!(actual.retained_limit_bytes, reference.retained_limit_bytes);
    assert_eq!(
        actual.refused_retained_bytes,
        reference.refused_retained_bytes
    );
    assert_eq!(actual.retained_bytes(), reference.retained_bytes());
    assert_eq!(
        actual.chunk_payload_capacity_bytes,
        reference.chunk_payload_capacity_bytes
    );
    assert_eq!(actual.chunks.capacity(), reference.chunks.capacity());
    assert_eq!(
        actual.chunks.iter().map(Vec::capacity).collect::<Vec<_>>(),
        reference
            .chunks
            .iter()
            .map(Vec::capacity)
            .collect::<Vec<_>>()
    );
    for id in 2..actual.node_count + 2 {
        assert_eq!(actual.node(id), reference.node(id), "node ID {id}");
        assert_eq!(
            node_hash(actual.node(id).unwrap()),
            node_hash(reference.node(id).unwrap())
        );
    }
    assert_eq!(actual.path_count_table(), reference.path_count_table());
}

#[test]
fn family_interner_probe_reuse_eliminates_second_collision_walk_and_hash() {
    const SLOTS: usize = 64;
    const COUNT: usize = 12;
    // Start at the last slot so the collision chain also wraps around zero.
    let rows: Vec<_> = (0..100_000_u32)
        .filter(|&row| {
            node_hash(append_node(row, FAMILY_EMPTY)) as usize & (SLOTS - 1) == SLOTS - 1
        })
        .take(COUNT)
        .collect();
    assert_eq!(rows.len(), COUNT);
    let mut actual = Builder::new(false, SLOTS);
    let mut reference = Builder::new(true, SLOTS);
    for &row in &rows {
        let id = actual.append(row, FAMILY_EMPTY);
        assert_eq!(id, reference.append(row, FAMILY_EMPTY));
        assert_eq!(actual.append(row, FAMILY_EMPTY), id);
        assert_eq!(reference.append(row, FAMILY_EMPTY), id);
        assert_same(&actual.family, &reference.family);
    }
    let old = reference.family.intern_work;
    let new = actual.family.intern_work;
    assert_eq!(new.lookup_slots, old.lookup_slots);
    assert_eq!(new.hashes, COUNT * 2);
    assert_eq!(old.hashes, COUNT * 3);
    assert_eq!(old.insertion_slots, COUNT * (COUNT + 1) / 2);
    assert_eq!(new.insertion_slots, 0);
    assert_eq!(new.vacancy_validation_slots, COUNT);
    assert_eq!(new.reused_vacancies, COUNT);
    let old_slot_reads = old.lookup_slots + old.insertion_slots + old.vacancy_validation_slots;
    let new_slot_reads = new.lookup_slots + new.insertion_slots + new.vacancy_validation_slots;
    assert_eq!(old_slot_reads - new_slot_reads, COUNT * (COUNT - 1) / 2);
    assert_eq!(actual.family.intern_slots.len(), SLOTS);
    eprintln!("family_interner_collision hashes {} -> {}, lookup_slots {}, insertion_probe_slots {} -> {}, vacancy_checks {}, total_slot_reads {} -> {}, reused_vacancies {}",
        old.hashes, new.hashes, new.lookup_slots, old.insertion_slots, new.insertion_slots, new.vacancy_validation_slots, old_slot_reads, new_slot_reads, new.reused_vacancies);
}

fn paths(family: &GeometrySolutionFamily, root: u32) -> Vec<Vec<u32>> {
    if root == FAMILY_INVALID {
        return vec![];
    }
    if root == FAMILY_EMPTY {
        return vec![vec![]];
    }
    let node = family.node(root).unwrap();
    let left = paths(family, node.left);
    match node.kind {
        FamilyNodeKind::Append => left
            .into_iter()
            .map(|mut path| {
                path.insert(0, node.row_id);
                path
            })
            .collect(),
        FamilyNodeKind::Union => left.into_iter().chain(paths(family, node.right)).collect(),
        FamilyNodeKind::Product => {
            let right = paths(family, node.right);
            left.into_iter()
                .flat_map(|prefix| {
                    right.iter().map(move |suffix| {
                        let mut result = prefix.clone();
                        result.extend(suffix);
                        result
                    })
                })
                .collect()
        }
    }
}

fn mixed_fixture(builder: &mut Builder, catalog: &GeometryCatalog) -> u32 {
    let mut halves = Vec::new();
    for shift in [0, 8] {
        let mut alternatives = Vec::new();
        for (piece, masks) in [(PieceKind::O, [0x33, 0xcc]), (PieceKind::I, [0x0f, 0xf0])] {
            let mut path = FAMILY_EMPTY;
            for mask in masks.into_iter().rev() {
                let row = catalog.skeleton_id(piece, mask << shift).unwrap();
                path = builder.append(row, path).unwrap();
            }
            alternatives.push(path);
        }
        let choice = builder.union(alternatives[0], alternatives[1]).unwrap();
        assert_eq!(
            builder.union(alternatives[1], alternatives[0]),
            Some(choice)
        );
        halves.push(choice);
    }
    let root = builder.product(halves[0], halves[1]).unwrap();
    assert_eq!(builder.product(halves[1], halves[0]), Some(root));
    assert_eq!(builder.union(root, root), Some(root));
    assert_eq!(builder.union(FAMILY_INVALID, root), Some(root));
    assert_eq!(builder.append(0, FAMILY_INVALID), Some(FAMILY_INVALID));
    assert_eq!(builder.product(root, FAMILY_EMPTY), Some(root));
    assert_eq!(builder.product(root, FAMILY_INVALID), Some(FAMILY_INVALID));
    root
}

fn identities(catalog: &GeometryCatalog, paths: &[Vec<u32>]) -> Vec<StandardBoard64TilingIdentity> {
    paths
        .iter()
        .map(|path| {
            StandardBoard64TilingIdentity::from_placements(
                0,
                path.iter().map(|&id| {
                    let row = catalog.skeleton(id);
                    PiecePlacementMask::new(row.piece, row.cells)
                }),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn family_interner_probe_reuse_preserves_mixed_leaf_order_identity_and_hashes() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    let mut actual = Builder::new(false, 8);
    let mut reference = Builder::new(true, 8);
    let root = mixed_fixture(&mut actual, &catalog);
    assert_eq!(root, mixed_fixture(&mut reference, &catalog));
    // Repeating the same construction exercises lookup hits after growth.
    assert_eq!(mixed_fixture(&mut actual, &catalog), root);
    assert_eq!(mixed_fixture(&mut reference, &catalog), root);
    assert_same(&actual.family, &reference.family);
    let actual_paths = paths(&actual.family, root);
    let reference_paths = paths(&reference.family, root);
    assert_eq!(actual_paths, reference_paths);
    assert_eq!(actual_paths.len(), 4);
    assert_eq!(actual.family.path_count(root), Some(4));
    let actual_ids = identities(&catalog, &actual_paths);
    let reference_ids = identities(&catalog, &reference_paths);
    assert_eq!(actual_ids, reference_ids);
    assert_eq!(
        actual_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4
    );
    let digests = |ids: &[StandardBoard64TilingIdentity]| {
        ids.iter().fold((0, 0), |(ordered, unordered), identity| {
            let hash = identity.bucket_hash();
            (
                mix_digest(ordered, hash),
                mix_order_independent_candidate_digest(unordered, hash),
            )
        })
    };
    assert_eq!(digests(&actual_ids), digests(&reference_ids));
    assert!(actual.family.intern_generation > 0, "fixture must resize");
    assert!(
        actual.family.intern_work.insertion_slots > 0,
        "growth takes the old probe fallback"
    );
}

#[test]
fn family_interner_probe_reuse_preserves_rewind_and_rejects_stale_generation() {
    let mut actual = Builder::new(false, 16);
    let mut reference = Builder::new(true, 16);
    for row in 0..20 {
        assert_eq!(
            actual.append(row, FAMILY_EMPTY),
            reference.append(row, FAMILY_EMPTY)
        );
    }
    let checkpoint = actual.family.checkpoint();
    for row in 20..100 {
        assert_eq!(
            actual.append(row, FAMILY_EMPTY),
            reference.append(row, FAMILY_EMPTY)
        );
    }
    assert_same(&actual.family, &reference.family);
    let node = append_node(100_001, FAMILY_EMPTY);
    let stale = actual.family.probe_interned(node).unwrap_err();
    actual.family.rewind(checkpoint);
    reference.family.rewind(checkpoint);
    assert_eq!(actual.family.intern_slots.len(), stale.slot_count);
    assert_ne!(actual.family.intern_generation, stale.generation);
    assert_eq!(
        actual.family.intern_slots[stale.vacant_slot.unwrap()],
        FAMILY_INVALID
    );
    let id = actual.family.push_node(node).unwrap();
    assert!(!actual.family.insert_at_probed_vacancy(id, stale));
    actual.family.insert_interned(id, stale.hash);
    assert_eq!(Some(id), reference.append(node.row_id, FAMILY_EMPTY));
    assert_same(&actual.family, &reference.family);
    for row in 20..100 {
        assert_eq!(
            actual.append(row, FAMILY_EMPTY),
            reference.append(row, FAMILY_EMPTY)
        );
    }
    assert_same(&actual.family, &reference.family);

    let stale = actual
        .family
        .probe_interned(append_node(200_001, FAMILY_EMPTY))
        .unwrap_err();
    actual.family.disable_interning();
    reference.family.disable_interning();
    assert!(!actual
        .family
        .insert_at_probed_vacancy(FAMILY_INVALID, stale));
    let hashes = actual.family.intern_work.hashes;
    for _ in 0..3 {
        assert_eq!(
            actual.append(0, FAMILY_EMPTY),
            reference.append(0, FAMILY_EMPTY)
        );
    }
    assert_eq!(
        actual.family.intern_work.hashes, hashes,
        "disabled owners do not probe"
    );
    assert_same(&actual.family, &reference.family);
}

#[test]
fn family_interner_probe_reuse_preserves_exact_bounds_and_growth_retirement() {
    let mut measured = Builder::new(true, 16);
    measured.append(7, FAMILY_EMPTY).unwrap();
    let cap = measured.family.retained_bytes() as u128;
    for limit in [cap, cap - 1] {
        let mut actual = Builder::new(false, 16);
        let mut reference = Builder::new(true, 16);
        actual.family.set_retained_limit_bytes(Some(limit));
        reference.family.set_retained_limit_bytes(Some(limit));
        assert_eq!(
            actual.append(7, FAMILY_EMPTY),
            reference.append(7, FAMILY_EMPTY)
        );
        assert_eq!(actual.family.node_count(), u32::from(limit == cap));
        assert_same(&actual.family, &reference.family);
    }

    let mut actual = Builder::new(false, 16);
    let mut reference = Builder::new(true, 16);
    for row in 0..11 {
        assert_eq!(
            actual.append(row, FAMILY_EMPTY),
            reference.append(row, FAMILY_EMPTY)
        );
    }
    assert_eq!(actual.family.intern_slots.len(), 16);
    let cap = actual.family.retained_bytes() as u128;
    actual.family.set_retained_limit_bytes(Some(cap));
    reference.family.set_retained_limit_bytes(Some(cap));
    let generation = actual.family.intern_generation;
    assert_eq!(
        actual.append(0, FAMILY_EMPTY),
        reference.append(0, FAMILY_EMPTY)
    );
    assert_eq!(actual.family.node_count(), 11);
    assert_eq!(
        actual.family.intern_generation, generation,
        "a hit must not attempt growth"
    );
    assert!(!actual.family.interning_disabled);
    // The node fits its existing chunk, but the required table replacement
    // exceeds the cap. Both algorithms retain that node and retire the index.
    assert_eq!(
        actual.append(11, FAMILY_EMPTY),
        reference.append(11, FAMILY_EMPTY)
    );
    assert_eq!(actual.family.node_count(), 12);
    assert!(actual.family.interning_disabled);
    assert_eq!(actual.family.intern_slots.capacity(), 0);
    assert_same(&actual.family, &reference.family);
    for _ in 0..3 {
        assert_eq!(
            actual.append(0, FAMILY_EMPTY),
            reference.append(0, FAMILY_EMPTY)
        );
    }
    assert_same(&actual.family, &reference.family);
}
