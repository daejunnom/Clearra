use clearra_piece_registry::registry::piece_registry::ShapeCell;

/// Catalog pruning policy. The early filter removes only row assignments that
/// the existing leaf check would reject, while the final lock-clear projection
/// remains authoritative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InverseProjectionPolicy {
    #[cfg(test)]
    Legacy,
    Early,
}

const PRODUCT_INVERSE_PROJECTION_POLICY: InverseProjectionPolicy = InverseProjectionPolicy::Early;

#[cfg(test)]
std::thread_local! {
    static INVERSE_PROJECTION_POLICY: std::cell::Cell<InverseProjectionPolicy> =
        const { std::cell::Cell::new(PRODUCT_INVERSE_PROJECTION_POLICY) };
}

#[inline]
pub(super) fn inverse_projection_policy() -> InverseProjectionPolicy {
    #[cfg(test)]
    {
        INVERSE_PROJECTION_POLICY.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        PRODUCT_INVERSE_PROJECTION_POLICY
    }
}

#[cfg(test)]
fn set_inverse_projection_policy(policy: InverseProjectionPolicy) -> InverseProjectionPolicy {
    INVERSE_PROJECTION_POLICY.with(|current| current.replace(policy))
}

/// Compatibility of each occupied local shape row with each target row.
///
/// Bit y records only whether the cells already fixed by `(shape, x)` fit in
/// the available cells of target row y. It does not choose future rows or a
/// deletion schedule, so the ordinary projection leaf remains the authority.
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
        // Current callers enforce narrower limits. A future unsupported domain
        // falls back to the existing enumerator instead of gaining authority.
        if width == 0
            || width > 64
            || height == 0
            || height > 32
            || local_rows.is_empty()
            || local_rows.len() > cells.len()
            || local_rows.windows(2).any(|rows| rows[0] >= rows[1])
        {
            return None;
        }

        let mut row_columns = [0_u64; 4];
        let mut column_demand = [0_u8; 64];
        for cell in cells {
            let target_x = i16::from(x) + i16::from(cell.x());
            if target_x < 0 || target_x >= i16::from(width) {
                return Some(Self {
                    allowed_target_rows: [0; 4],
                    columns_possible: false,
                });
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

        let columns_possible =
            (0..usize::from(width)).all(|column| column_demand[column] <= column_capacity[column]);
        Some(Self {
            allowed_target_rows,
            columns_possible,
        })
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
    use clearra_pc_graph::request::OpeningPcSearchQuery;
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
    use clearra_problem::{BuildProbabilityField, ProblemCompiler};

    use super::{
        inverse_projection_policy, set_inverse_projection_policy, InverseProjectionPolicy,
        ProjectionRowFilter,
    };
    use crate::backend::wasm_cpu::{
        catalog::GeometryCatalog, extended_board::ExtendedBoard,
        extended_inverse_catalog::ExtendedInverseCatalog,
    };

    struct RestorePolicy(InverseProjectionPolicy);

    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            set_inverse_projection_policy(self.0);
        }
    }

    #[test]
    fn row_filter_matches_the_existing_leaf_cell_predicate() {
        let registry = standard_tetromino_registry();
        for piece in PieceKind::STANDARD_TETROMINOES {
            for rotation in RotationState::ALL {
                let shape = registry.get(piece).expect("standard piece").shape(rotation);
                let mut local_rows = shape.cells().map(|cell| cell.y() as u8).to_vec();
                local_rows.sort_unstable();
                local_rows.dedup();
                for available in [0_u64, 0xffff, 0x3333, 0x5a5a, 0x087f, 0x7007] {
                    let filter = ProjectionRowFilter::compile(
                        InverseProjectionPolicy::Early,
                        4,
                        4,
                        shape.cells(),
                        &local_rows,
                        0,
                        |row| (available >> (u32::from(row) * 4)) & 0xf,
                    )
                    .expect("early filter");
                    for (local_index, local_row) in local_rows.iter().copied().enumerate() {
                        for target_row in 0..4_u8 {
                            let expected = shape
                                .cells()
                                .iter()
                                .filter(|cell| cell.y() as u8 == local_row)
                                .all(|cell| {
                                    available
                                        & (1_u64 << (u32::from(target_row) * 4 + cell.x() as u32))
                                        != 0
                                });
                            assert_eq!(filter.row_allowed(local_index, target_row), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn early_projection_preserves_compact_and_extended_catalogs() {
        use clearra_core_domain::pc::pc_target::PcTarget;

        let _restore = RestorePolicy(inverse_projection_policy());
        let problem =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::four_lines()))
                .expect("four-line problem");
        let all = (1_u64 << 40) - 1;

        for (initial, required) in [(0, all), (all & !0x300003, 0x300003)] {
            let mut baseline = None;
            for policy in [
                InverseProjectionPolicy::Legacy,
                InverseProjectionPolicy::Early,
            ] {
                set_inverse_projection_policy(policy);
                let catalog = GeometryCatalog::compile_for_required_cells_on_board(
                    &problem, initial, required,
                )
                .expect("bounded compact catalog");
                let rows = (0..catalog.skeleton_count() as u32)
                    .map(|row_id| {
                        (
                            catalog.skeleton(row_id),
                            catalog.realizations(row_id).to_vec(),
                            [0_u16, 1, 3, 15]
                                .into_iter()
                                .map(|deleted| {
                                    catalog.instantiations(row_id, deleted).collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>();
                let observed = (catalog.identity_digest(), rows);
                if let Some(expected) = &baseline {
                    assert_eq!(&observed, expected);
                } else {
                    baseline = Some(observed);
                }
            }
        }

        let mut target = ExtendedBoard::EMPTY;
        for cell in [13_u16, 14, 73, 74] {
            target.insert(cell);
        }
        let field = BuildProbabilityField::from_words_preserving_height(8, [0; 4], target.words())
            .expect("extended target");
        let mut baseline = None;
        for policy in [
            InverseProjectionPolicy::Legacy,
            InverseProjectionPolicy::Early,
        ] {
            set_inverse_projection_policy(policy);
            let catalog = ExtendedInverseCatalog::compile(field).expect("bounded extended catalog");
            let temporal = (0..catalog.skeletons().len() as u32)
                .map(|row_id| {
                    (0..256_u32)
                        .map(|deleted| catalog.instantiations(row_id, deleted).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let observed = (
                catalog.identity_digest(),
                catalog.skeletons().to_vec(),
                temporal,
            );
            if let Some(expected) = &baseline {
                assert_eq!(&observed, expected);
            } else {
                baseline = Some(observed);
            }
        }
    }
}
