//! Small archive correctness fixtures. No search or timing measurement runs.

use std::io::Cursor;

use clearra_ctk4::UnverifiedArchive;

use super::*;
use crate::backend::wasm_cpu::catalog::GeometryCatalog;
use crate::backend::wasm_cpu::geometry_partial_family::{ZddArena, ZddId, ZddLimits};
use crate::backend::wasm_cpu::symbolic_family::SymbolicFamilyReport;

fn singleton(sets: &mut ZddArena, variables: &[u32]) -> ZddId {
    variables.iter().fold(ZddId::UNIT, |root, &variable| {
        sets.add(root, variable).unwrap()
    })
}

fn fixture() -> SymbolicFamilyResult {
    // Two complete covers of the same eight required cells: two horizontal I
    // pieces, or two O pieces. Source correlation is II versus OO, not the
    // Cartesian product of the two independently projected families.
    let required = 0xf | (0xf << 10);
    let initial = ((1_u64 << 20) - 1) ^ required;
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 2, initial, required)
            .unwrap();
    let i_rows = [
        catalog.skeleton_id(PieceKind::I, 0xf).unwrap(),
        catalog.skeleton_id(PieceKind::I, 0xf << 10).unwrap(),
    ];
    let o_rows = [
        catalog.skeleton_id(PieceKind::O, 3 | (3 << 10)).unwrap(),
        catalog.skeleton_id(PieceKind::O, 12 | (12 << 10)).unwrap(),
    ];
    let source_base = catalog.skeleton_count() as u32;
    let mut sets = ZddArena::new(ZddLimits::default());
    let i_identity = singleton(&mut sets, &i_rows);
    let o_identity = singleton(&mut sets, &o_rows);
    let identities = sets.union(i_identity, o_identity).unwrap();
    let ii = singleton(&mut sets, &[source_base, source_base + 7]);
    let oo = singleton(&mut sets, &[source_base + 1, source_base + 8]);
    let covered_sources = sets.union(ii, oo).unwrap();
    let mut i_joint = ii;
    for row in i_rows {
        i_joint = sets.add(i_joint, row).unwrap();
    }
    let mut o_joint = oo;
    for row in o_rows {
        o_joint = sets.add(o_joint, row).unwrap();
    }
    let accepted = sets.union(i_joint, o_joint).unwrap();
    SymbolicFamilyResult {
        catalog,
        sets,
        accepted,
        identities,
        covered_sources,
        source_length: 2,
        report: SymbolicFamilyReport {
            unique_solution_count: 2,
            covered_source_count: 2,
            solution_source_pairs: 2,
            ..SymbolicFamilyReport::default()
        },
    }
}

fn encode(result: &mut SymbolicFamilyResult) -> (Vec<u8>, ProducerSeal) {
    let mut output = Cursor::new(Vec::new());
    let seal = result
        .write_ctk4(
            &mut output,
            b"diagnostic fingerprint, not query authority",
            EncodeOptions {
                atom_block_items: 1,
                node_block_items: 1,
            },
            ArchiveLimits::default(),
            OperationControl::default(),
        )
        .unwrap();
    (output.into_inner(), seal)
}

#[test]
fn symbolic_archive_round_trip_keeps_shared_roots_and_source_correlation() {
    let mut result = fixture();
    let identities = result.identity_page(0, 2).unwrap();
    let (bytes, seal) = encode(&mut result);
    let verified = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    let mut archive = SymbolicFamilyArchive::from_verified(verified).unwrap();
    assert_eq!(archive.metrics().block_fetches, 0);
    assert_eq!(archive.identity_row_set_count().unwrap(), 2);
    assert_eq!(archive.covered_source_set_count().unwrap(), 2);
    assert_eq!(archive.joint_set_count().unwrap(), 2);
    assert_eq!(
        archive.unverified_annotation(),
        b"diagnostic fingerprint, not query authority",
    );
    for offset in 0..2 {
        assert_eq!(
            archive
                .identity_page(offset, 1, OperationControl::default())
                .unwrap(),
            vec![identities[offset as usize]],
        );
    }
    assert!(archive
        .identity_page(2, 1, OperationControl::default())
        .unwrap()
        .is_empty());
    for source in [
        [PieceKind::I, PieceKind::I],
        [PieceKind::O, PieceKind::O],
        [PieceKind::I, PieceKind::O],
        [PieceKind::T, PieceKind::Z],
    ] {
        assert_eq!(
            archive
                .covers_source(&source, OperationControl::default())
                .unwrap(),
            result.covers_source(&source).unwrap(),
        );
    }
    assert!(archive
        .covers_source(&[PieceKind::I], OperationControl::default())
        .is_err());

    // Verify the retained joint relation directly, not merely its two counts
    // or its independent identity and source projections.
    let source_base = result.catalog.skeleton_count() as u32;
    let mut raw = archive.into_archive();
    let joint_root = raw
        .info()
        .roots
        .iter()
        .position(|root| root.name == "accepted")
        .unwrap();
    for identity in identities {
        let mut rows = (0..identity.placement_count())
            .map(|index| {
                let placement = identity.placement(index).unwrap();
                result
                    .catalog
                    .skeleton_id(placement.piece(), placement.cells_mask())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        rows.sort_unstable();
        let actual_kind = identity.placement(0).unwrap().piece();
        for (kind, code) in [(PieceKind::I, 0), (PieceKind::O, 1)] {
            let mut joint = rows.clone();
            joint.extend([source_base + code, source_base + 7 + code]);
            assert_eq!(
                raw.contains(joint_root, &joint, OperationControl::default())
                    .unwrap(),
                kind == actual_kind,
            );
        }
    }
}

#[test]
fn symbolic_archive_strict_scan_reopens_without_a_live_seal() {
    let mut result = fixture();
    let expected = result.identity_page(0, 2).unwrap();
    let (bytes, _) = encode(&mut result);
    let verified = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .verify_all(OperationControl::default())
    .unwrap();
    assert!(verified.full_scan_performed());
    let mut archive = SymbolicFamilyArchive::from_verified(verified).unwrap();
    assert_eq!(
        archive
            .identity_page(0, 2, OperationControl::default())
            .unwrap(),
        expected,
    );
}

#[test]
fn symbolic_archive_empty_result_has_no_nodes_or_eager_page_atoms() {
    let mut result = fixture();
    result.accepted = ZddId::EMPTY;
    result.identities = ZddId::EMPTY;
    result.covered_sources = ZddId::EMPTY;
    let (bytes, seal) = encode(&mut result);
    let verified = UnverifiedArchive::open(
        bytes.as_slice(),
        ArchiveLimits::default(),
        OperationControl::default(),
    )
    .unwrap()
    .accept_producer_seal(&seal)
    .unwrap();
    let mut archive = SymbolicFamilyArchive::from_verified(verified).unwrap();
    assert_eq!(archive.info().node_count, 0);
    assert_eq!(archive.joint_set_count().unwrap(), 0);
    assert!(archive
        .identity_page(0, 2, OperationControl::default())
        .unwrap()
        .is_empty());
    assert_eq!(archive.metrics().block_fetches, 0);
}

#[test]
fn symbolic_archive_cancellation_before_preparation_preserves_the_result() {
    let mut result = fixture();
    let epoch = result.sets.epoch();
    let nodes = result.sets.node_count();
    let mut output = Cursor::new(Vec::new());
    let cancelled = || true;
    assert!(matches!(
        result.write_ctk4(
            &mut output,
            b"",
            EncodeOptions::default(),
            ArchiveLimits::default(),
            OperationControl {
                cancellation: Some(&cancelled),
                max_work: 0,
            },
        ),
        Err(SymbolicFamilyArchiveError::Archive(ArchiveError::Cancelled)),
    ));
    assert_eq!(result.sets.epoch(), epoch);
    assert_eq!(result.sets.node_count(), nodes);
    assert!(output.get_ref().is_empty());
}

#[test]
fn symbolic_scope_and_requested_atoms_reject_malformed_shapes() {
    let scope = SymbolicFamilyArchiveScope {
        width: 10,
        height: 2,
        initial_board: ((1_u64 << 20) - 1) ^ (0xf | (0xf << 10)),
        required_cells: 0xf | (0xf << 10),
        row_count: 2,
        source_length: 2,
    };
    let encoded = encode_scope(scope, b"annotation").unwrap();
    assert_eq!(decode_scope(&encoded).unwrap(), scope);
    for length in 0..SCOPE_BYTES {
        assert!(decode_scope(&encoded[..length]).is_err());
    }
    let mut invalid = encoded.clone();
    invalid[10] = 1;
    assert!(decode_scope(&invalid).is_err());
    let mut invalid = encoded.clone();
    invalid.pop();
    assert!(decode_scope(&invalid).is_err());
    let mut wrong_frame = scope;
    wrong_frame.width = 9;
    assert!(validate_scope(wrong_frame).is_err());
    let mut wrong_range = scope;
    wrong_range.source_length = u32::MAX;
    assert!(matches!(
        validate_scope(wrong_range),
        Err(ArchiveError::IntegerOverflow),
    ));
    let mut atom = vec![ROW_ATOM, b'I'];
    atom.extend_from_slice(&0xf_u64.to_le_bytes());
    assert_eq!(decode_row_atom(&atom, scope).unwrap().piece(), PieceKind::I);
    atom[1] = b'X';
    assert!(decode_row_atom(&atom, scope).is_err());
    atom[1] = b'I';
    atom[2] = 3;
    assert!(decode_row_atom(&atom, scope).is_err());
}
