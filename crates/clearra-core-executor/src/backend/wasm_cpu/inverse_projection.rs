use clearra_piece_registry::registry::piece_registry::ShapeCell;

/// Optional catalog pruning only; the final lock-clear projection remains
/// authoritative. Legacy and Off intentionally use the identical leaf filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum InverseProjectionPolicy {
    Legacy = 0,
    Off = 1,
    Early = 2,
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
std::thread_local! {
    static INVERSE_PROJECTION_POLICY: std::cell::Cell<InverseProjectionPolicy> =
        const { std::cell::Cell::new(InverseProjectionPolicy::Legacy) };
}

#[inline]
pub(super) fn inverse_projection_policy() -> InverseProjectionPolicy {
    #[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
    { INVERSE_PROJECTION_POLICY.with(std::cell::Cell::get) }
    #[cfg(not(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab")))]
    { InverseProjectionPolicy::Legacy }
}

#[cfg(any(test, feature = "wasm-stage-profiling", feature = "minimum-physical-ab"))]
pub(super) fn set_inverse_projection_policy(
    policy: InverseProjectionPolicy,
) -> InverseProjectionPolicy {
    INVERSE_PROJECTION_POLICY.with(|current| current.replace(policy))
}

/// Target-frame row compatibility. No future row or deletion schedule is
/// chosen here. Bit y records only whether the shape cells in one already
/// selected local row fit the available target cells at target row y.
pub(super) struct ProjectionRowFilter {
    allowed_target_rows: [u32; 4],
    columns_possible: bool,
}

impl ProjectionRowFilter {
    pub fn compile(
        policy: InverseProjectionPolicy,
        width: u8,
        height: u8,
        cells: [ShapeCell; 4],
        local_rows: &[u8],
        x: i8,
        mut available_row: impl FnMut(u8) -> u64,
    ) -> Option<Self> {
        if policy != InverseProjectionPolicy::Early {
            return None;
        }
        // Both current catalogs enforce narrower bounds. If a future caller
        // has a different domain, disable this optional filter and retain the
        // existing projection enumerator as the sole leaf authority.
        if width == 0 || width > 64 || height == 0 || height > 32
            || local_rows.is_empty() || local_rows.len() > cells.len()
            || local_rows.windows(2).any(|rows| rows[0] >= rows[1])
        {
            return None;
        }
        let mut row_columns = [0_u64; 4];
        let mut column_demand = [0_u8; 64];
        for cell in cells {
            let target_x = i16::from(x) + i16::from(cell.x());
            if target_x < 0 || target_x >= i16::from(width) {
                return Some(Self { allowed_target_rows: [0; 4], columns_possible: false });
            }
            let local_y = u8::try_from(cell.y()).ok()?;
            let local_index = local_rows.binary_search(&local_y).ok()?;
            row_columns[local_index] |= 1_u64 << target_x as u32;
            column_demand[target_x as usize] += 1;
        }
        let mut column_capacity = [0_u8; 64];
        let mut allowed_target_rows = [0_u32; 4];
        for target_y in 0..height {
            let available = available_row(target_y);
            for local_index in 0..local_rows.len() {
                if row_columns[local_index] & !available == 0 {
                    allowed_target_rows[local_index] |= 1_u32 << target_y;
                }
            }
            let mut columns = available;
            while columns != 0 {
                let column = columns.trailing_zeros() as usize;
                columns &= columns - 1;
                column_capacity[column] += 1;
            }
        }
        let columns_possible = (0..usize::from(width))
            .all(|column| column_demand[column] <= column_capacity[column]);
        Some(Self { allowed_target_rows, columns_possible })
    }

    pub const fn columns_possible(&self) -> bool {
        self.columns_possible
    }

    pub fn row_allowed(&self, local_index: usize, target_y: u8) -> bool {
        self.allowed_target_rows[local_index] & (1_u32 << target_y) != 0
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
    use super::{InverseProjectionPolicy, ProjectionRowFilter};

    #[test]
    fn row_filter_preserves_every_assigned_row_in_the_leaf_domain() {
        let registry = standard_tetromino_registry();
        // All standard rotations against six bounded 4x4 available fields.
        // The expected predicate directly checks individual cells.
        for piece in PieceKind::STANDARD_TETROMINOES {
            for rotation in RotationState::ALL {
                let shape = registry.get(piece).expect("standard piece").shape(rotation);
                let mut local_rows = shape.cells().map(|cell| cell.y() as u8).to_vec();
                local_rows.sort_unstable();
                local_rows.dedup();
                for available in [0_u64, 0xffff, 0x3333, 0x5a5a, 0x087f, 0x7007] {
                    let filter = ProjectionRowFilter::compile(
                        InverseProjectionPolicy::Early, 4, 4, shape.cells(), &local_rows, 0,
                        |row| (available >> (u32::from(row) * 4)) & 0xf,
                    ).expect("early filter");
                    for (local_index, local_row) in local_rows.iter().copied().enumerate() {
                        for target_row in 0..4_u8 {
                            let expected = shape.cells().iter().filter(|cell| cell.y() as u8 == local_row)
                                .all(|cell| available & (1_u64 << (u32::from(target_row) * 4 + cell.x() as u32)) != 0);
                            assert_eq!(filter.row_allowed(local_index, target_row), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn column_demand_does_not_depend_on_unselected_row_gaps() {
        let registry = standard_tetromino_registry();
        let shape = registry.get(PieceKind::O).expect("O").shape(RotationState::ALL[0]);
        let rows = [0_u8, 1];
        let sparse = ProjectionRowFilter::compile(
            InverseProjectionPolicy::Early, 10, 8, shape.cells(), &rows, 3,
            |row| if row == 1 || row == 7 { 0b11 << 3 } else { 0 },
        ).expect("early filter");
        assert!(sparse.columns_possible());
        assert!(sparse.row_allowed(0, 1));
        assert!(sparse.row_allowed(1, 7));
        let missing = ProjectionRowFilter::compile(
            InverseProjectionPolicy::Early, 10, 8, shape.cells(), &rows, 3,
            |row| if row == 1 { 0b11 << 3 } else { 0 },
        ).expect("early filter");
        assert!(!missing.columns_possible());
        for policy in [InverseProjectionPolicy::Legacy, InverseProjectionPolicy::Off] {
            assert!(ProjectionRowFilter::compile(policy, 10, 8, shape.cells(), &rows, 3, |_| 0).is_none());
        }
        for (width, height) in [(0, 8), (65, 1), (10, 0), (10, 33)] {
            assert!(ProjectionRowFilter::compile(
                InverseProjectionPolicy::Early, width, height, shape.cells(), &rows, 3,
                |_| unreachable!("unsupported dimensions skip row preparation"),
            ).is_none());
        }
        let unsupported_rows: [&[u8]; 5] = [&[], &[0], &[1, 0], &[0, 1, 1], &[0, 1, 2, 3, 4]];
        for local_rows in unsupported_rows {
            assert!(ProjectionRowFilter::compile(
                InverseProjectionPolicy::Early, 10, 8, shape.cells(), local_rows, 3,
                |_| unreachable!("unsupported metadata skips row preparation"),
            ).is_none());
        }
    }

    #[test]
    fn early_projection_preserves_catalog_ids_and_every_temporal_instantiation() {
        use clearra_core_domain::pc::pc_target::PcTarget;
        use clearra_pc_graph::request::OpeningPcSearchQuery;
        use clearra_problem::{BuildProbabilityField, ProblemCompiler};
        use super::{inverse_projection_policy, set_inverse_projection_policy};
        use crate::backend::wasm_cpu::{
            catalog::GeometryCatalog, extended_board::ExtendedBoard,
            extended_inverse_catalog::ExtendedInverseCatalog,
        };
        struct Restore(InverseProjectionPolicy);
        impl Drop for Restore {
            fn drop(&mut self) { set_inverse_projection_policy(self.0); }
        }
        let _restore = Restore(inverse_projection_policy());
        use crate::backend::wasm_cpu::inverse_parent::{inverse_parent_policy,
            set_inverse_parent_policy, InverseParentPolicy};
        struct RestoreParents(InverseParentPolicy);
        impl Drop for RestoreParents {
            fn drop(&mut self) { set_inverse_parent_policy(self.0); }
        }
        let _restore_parents = RestoreParents(inverse_parent_policy());
        let problem = ProblemCompiler::compile_opening_pc(
            &OpeningPcSearchQuery::new(PcTarget::four_lines()),
        ).expect("four-line problem");
        let all = (1_u64 << 40) - 1;
        for (initial, required) in [(0, all), (all & !0x300003, 0x300003)] {
            let mut canonical_reference = None;
            for parent_policy in [InverseParentPolicy::EagerTable, InverseParentPolicy::EagerRaw,
                InverseParentPolicy::Deferred] {
            set_inverse_parent_policy(parent_policy);
            let mut baseline = None;
            for policy in [InverseProjectionPolicy::Legacy, InverseProjectionPolicy::Off, InverseProjectionPolicy::Early] {
                set_inverse_projection_policy(policy);
                let catalog = GeometryCatalog::compile_for_required_cells_on_board(
                    &problem, initial, required,
                ).expect("bounded inverse catalog");
                let retained_before = catalog.retained_bytes();
                let digest_before = catalog.identity_digest();
                if parent_policy == InverseParentPolicy::Deferred {
                    assert_eq!(catalog.deferred_parent_counts(), Some((0, catalog.skeleton_count())));
                } else { assert_eq!(catalog.deferred_parent_counts(), None); }
                assert_eq!(catalog.has_instantiation_table(), parent_policy == InverseParentPolicy::EagerTable);
                let payloads = (0..catalog.skeleton_count() as u32)
                    .map(|row_id| (catalog.skeleton(row_id), catalog.realizations(row_id).to_vec()))
                    .collect::<Vec<_>>();
                if let Some(expected) = &canonical_reference { assert_eq!(&payloads, expected); }
                else { canonical_reference = Some(payloads.clone()); }
                assert_eq!(catalog.identity_digest(), digest_before, "lazy materialization cannot change source identity");
                assert_eq!(catalog.retained_bytes(), retained_before, "parent cache was preallocated under source admission");
                if parent_policy == InverseParentPolicy::Deferred {
                    assert_eq!(catalog.deferred_parent_counts(), Some((catalog.skeleton_count(), catalog.skeleton_count())));
                }
                for (skeleton, parents) in &payloads {
                    use crate::backend::wasm_cpu::inverse_parent::{InverseParentCursor, ParentAdvance};
                    let mut mask = skeleton.cells;
                    let cells = std::array::from_fn(|_| {
                        let cell = mask.trailing_zeros() as u16; mask &= mask - 1; cell
                    });
                    let mut cursor = InverseParentCursor::new(10, 4, skeleton.piece, cells).unwrap();
                    let mut generated = Vec::new();
                    for _ in 0..4 {
                        match cursor.advance() {
                            ParentAdvance::Parent(parent) => generated.push(super::super::catalog::Realization {
                                piece: skeleton.piece, cells: skeleton.cells,
                                required_deleted_rows: parent.required_deleted_rows as u16,
                                rotation: parent.rotation, x: parent.x, target_anchor_y: parent.target_anchor_y,
                            }),
                            ParentAdvance::Pending => {}
                            ParentAdvance::Complete => panic!("parent source closed before checking all rotations"),
                        }
                    }
                    assert!(matches!(cursor.advance(), ParentAdvance::Complete));
                    generated.sort_unstable();
                    assert_eq!(&generated, parents, "inverse reconstruction changed an original temporal parent");
                }
                let observed = (catalog.identity_digest(), payloads);
                if let Some(expected) = &baseline { assert_eq!(&observed, expected); }
                else { baseline = Some(observed); }
            }
            }
        }
        for (height, cells) in [(8, [13, 14, 73, 74]), (24, [3, 13, 223, 233])] {
            let mut target = ExtendedBoard::EMPTY;
            for cell in cells { target.insert(cell); }
            let field = BuildProbabilityField::from_words_preserving_height(height, [0; 4], target.words())
                .expect("extended tetromino with unresolved row-deletion gaps");
            let gap = ((1_u32 << 22) - 1) & !3;
            let deleted_domains: Vec<u32> = if height == 8 { (0..256).collect() }
                else { vec![0, gap, gap & !(1 << 21), gap | 1, (1 << 24) - 1, u32::MAX] };
            let mut canonical_reference = None;
            for parent_policy in [InverseParentPolicy::EagerTable, InverseParentPolicy::EagerRaw,
                InverseParentPolicy::Deferred] {
            set_inverse_parent_policy(parent_policy);
            let mut baseline = None;
            for policy in [InverseProjectionPolicy::Legacy, InverseProjectionPolicy::Off, InverseProjectionPolicy::Early] {
            set_inverse_projection_policy(policy);
            let catalog = ExtendedInverseCatalog::compile(field).expect("bounded extended catalog");
            let retained_before = catalog.retained_bytes();
            let digest_before = catalog.identity_digest();
            assert_eq!(catalog.deferred_parent_counts(),
                (parent_policy == InverseParentPolicy::Deferred).then_some((0, catalog.skeletons().len())));
            let parents = catalog.temporal_parent_signature();
            assert!(catalog.temporal_parent_reconstruction_matches());
            assert!(!catalog.skeletons().is_empty());
            let temporal = (0..catalog.skeletons().len() as u32)
                .map(|row_id| deleted_domains.iter().copied()
                    .map(|deleted| catalog.instantiations(row_id, deleted).collect::<Vec<_>>())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>();
            assert!(temporal.iter().flatten().any(|values| !values.is_empty()));
            let payloads = (catalog.skeletons().to_vec(), parents, temporal);
            if let Some(expected) = &canonical_reference { assert_eq!(&payloads, expected); }
            else { canonical_reference = Some(payloads.clone()); }
            assert_eq!(catalog.identity_digest(), digest_before);
            assert_eq!(catalog.retained_bytes(), retained_before,
                "extended packed parent storage is fully admitted before materialization");
            assert_eq!(catalog.deferred_parent_counts(), (parent_policy == InverseParentPolicy::Deferred)
                .then_some((catalog.skeletons().len(), catalog.skeletons().len())));
            let observed = (digest_before, payloads);
            if let Some(expected) = &baseline { assert_eq!(&observed, expected); }
            else { baseline = Some(observed); }
            }
            }
        }
    }
}
