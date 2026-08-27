use super::{catalog::SkeletonRow, mix_digest};

pub(super) const APDP_ARM: u8 = 1;
pub(super) const APDP_ELBOW: u8 = 2;

const APDP_ARM_ARM: u8 = 1;
const APDP_ARM_ELBOW: u8 = 2;
const APDP_ELBOW_ELBOW: u8 = 4;

#[derive(Clone, Copy, Debug)]
struct ExactParentRange {
    partial_cells: u64,
    parent_start: u32,
    parent_count: u16,
}

#[derive(Clone, Debug)]
pub(super) struct ExactArmPairIndex {
    ranges: Vec<ExactParentRange>,
    parent_rows: Vec<u32>,
    row_support_flags: Vec<u8>,
    identity_digest: u64,
}

impl ExactArmPairIndex {
    pub fn compile(width: u8, rows: &[SkeletonRow]) -> Option<Self> {
        // One tetromino has four three-cell partials and at most six unordered
        // partial pairs. Every accepted pair records both parents, so twelve
        // `(partial, row)` records per source row are a conservative bound.
        // Keeping the build flat makes the complete allocation shape checked
        // and fallible; it also avoids opaque per-node BTreeMap allocations in
        // the score-terminal memory surface.
        let association_capacity = rows.len().checked_mul(12)?;
        let mut parent_associations = Vec::<(u64, u32)>::new();
        parent_associations
            .try_reserve_exact(association_capacity)
            .ok()?;
        let mut row_support_flags = Vec::new();
        row_support_flags.try_reserve_exact(rows.len()).ok()?;

        for (row_id, row) in rows.iter().enumerate() {
            let mut partials = [0_u64; 4];
            let mut kinds = [0_u8; 4];
            let mut partial_count = 0_usize;
            let mut cells = row.cells;
            while cells != 0 {
                let bit = cells & cells.wrapping_neg();
                cells &= cells - 1;
                let partial = row.cells & !bit;
                let kind = partial_shape_kind(width, partial);
                if kind != 0 {
                    partials[partial_count] = partial;
                    kinds[partial_count] = kind;
                    partial_count += 1;
                }
            }

            let mut flags = 0_u8;
            for left in 0..partial_count {
                for right in left + 1..partial_count {
                    if partials[left] | partials[right] != row.cells {
                        continue;
                    }
                    flags |= match (kinds[left], kinds[right]) {
                        (APDP_ARM, APDP_ARM) => APDP_ARM_ARM,
                        (APDP_ELBOW, APDP_ELBOW) => APDP_ELBOW_ELBOW,
                        _ => APDP_ARM_ELBOW,
                    };
                    let row_id = u32::try_from(row_id).ok()?;
                    parent_associations.push((partials[left], row_id));
                    parent_associations.push((partials[right], row_id));
                }
            }
            row_support_flags.push(flags);
        }

        parent_associations.sort_unstable();
        parent_associations.dedup();

        let mut ranges = Vec::new();
        let mut parent_rows = Vec::new();
        ranges.try_reserve_exact(parent_associations.len()).ok()?;
        parent_rows
            .try_reserve_exact(parent_associations.len())
            .ok()?;
        let mut cursor = 0;
        while cursor < parent_associations.len() {
            let partial_cells = parent_associations[cursor].0;
            let parent_start = u32::try_from(parent_rows.len()).ok()?;
            let begin = cursor;
            while cursor < parent_associations.len()
                && parent_associations[cursor].0 == partial_cells
            {
                parent_rows.push(parent_associations[cursor].1);
                cursor += 1;
            }
            let parent_count = u16::try_from(cursor.checked_sub(begin)?).ok()?;
            ranges.push(ExactParentRange {
                partial_cells,
                parent_start,
                parent_count,
            });
        }

        let mut identity_digest = mix_digest(0, u64::from(width));
        for range in &ranges {
            identity_digest = mix_digest(identity_digest, range.partial_cells);
            identity_digest = mix_digest(identity_digest, u64::from(range.parent_start));
            identity_digest = mix_digest(identity_digest, u64::from(range.parent_count));
        }
        for parent in &parent_rows {
            identity_digest = mix_digest(identity_digest, u64::from(*parent));
        }

        Some(Self {
            ranges,
            parent_rows,
            row_support_flags,
            identity_digest,
        })
    }

    pub fn parent_rows(&self, partial_cells: u64) -> &[u32] {
        let Ok(index) = self
            .ranges
            .binary_search_by_key(&partial_cells, |range| range.partial_cells)
        else {
            return &[];
        };
        let range = self.ranges[index];
        let start = range.parent_start as usize;
        let end = start + range.parent_count as usize;
        &self.parent_rows[start..end]
    }

    pub fn row_supports(&self, row_id: u32, partial_cells: u64) -> bool {
        self.parent_rows(partial_cells)
            .binary_search(&row_id)
            .is_ok()
    }

    pub fn row_support_flags(&self, row_id: u32) -> u8 {
        self.row_support_flags[row_id as usize]
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub fn retained_bytes(&self) -> usize {
        self.ranges.capacity() * core::mem::size_of::<ExactParentRange>()
            + self.parent_rows.capacity() * core::mem::size_of::<u32>()
            + self.row_support_flags.capacity() * core::mem::size_of::<u8>()
    }

    /// Conservative peak for compiling the flat APDP association table and
    /// the retained range/row/flag arrays. This deliberately describes an
    /// upper bound, not the exact post-dedup allocation.
    pub fn checked_compile_peak_upper_bound(row_count: usize) -> Option<u128> {
        let row_count = row_count as u128;
        let association_count = row_count.checked_mul(12)?;
        association_count
            .checked_mul(core::mem::size_of::<(u64, u32)>() as u128)?
            .checked_add(
                association_count.checked_mul(core::mem::size_of::<ExactParentRange>() as u128)?,
            )?
            .checked_add(association_count.checked_mul(core::mem::size_of::<u32>() as u128)?)?
            .checked_add(row_count.checked_mul(core::mem::size_of::<u8>() as u128)?)
    }
}

pub(super) fn partial_shape_kind(width: u8, mut cells: u64) -> u8 {
    if width == 0 || cells.count_ones() != 3 {
        return 0;
    }
    let mut min_x = u8::MAX;
    let mut max_x = 0;
    let mut min_y = u8::MAX;
    let mut max_y = 0;
    while cells != 0 {
        let cell = cells.trailing_zeros() as u8;
        cells &= cells - 1;
        let x = cell % width;
        let y = cell / width;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    if (min_y == max_y && max_x - min_x == 2) || (min_x == max_x && max_y - min_y == 2) {
        APDP_ARM
    } else if max_x - min_x == 1 && max_y - min_y == 1 {
        APDP_ELBOW
    } else {
        0
    }
}
