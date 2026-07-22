use super::{catalog::SkeletonRow, mix_digest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CertifiedSeparatorSplit {
    pub owner_cells: u64,
    pub remainder_cells: u64,
    pub separator_column: u8,
}

#[derive(Clone, Debug)]
pub(super) struct SeparatorCatalog {
    width: u8,
    height: u8,
    column_masks: Vec<u64>,
    left_masks: Vec<u64>,
    right_masks: Vec<u64>,
    safe_column_bits: u64,
    identity_digest: u64,
}

impl SeparatorCatalog {
    pub fn compile(width: u8, height: u8, rows: &[SkeletonRow]) -> Self {
        let mut column_masks = vec![0_u64; width as usize];
        let mut left_masks = vec![0_u64; width as usize];
        let mut right_masks = vec![0_u64; width as usize];
        let mut safe_column_bits = 0_u64;
        let board_cells = usize::from(width) * usize::from(height);
        let board_mask = if board_cells == 64 {
            u64::MAX
        } else {
            (1_u64 << board_cells) - 1
        };

        for column in 0..width {
            let mut column_mask = 0_u64;
            let mut left_mask = 0_u64;
            for y in 0..height {
                column_mask |= 1_u64 << (y * width + column);
                for x in 0..column {
                    left_mask |= 1_u64 << (y * width + x);
                }
            }
            let right_mask = board_mask & !(left_mask | column_mask);
            column_masks[column as usize] = column_mask;
            left_masks[column as usize] = left_mask;
            right_masks[column as usize] = right_mask;

            let crossing_is_certified = rows.iter().all(|row| {
                let has_left = row.cells & left_mask != 0;
                let has_right = row.cells & right_mask != 0;
                !has_left || !has_right || row.cells & column_mask != 0
            });
            if crossing_is_certified {
                safe_column_bits |= 1_u64 << column;
            }
        }

        let mut identity_digest = mix_digest(0, u64::from(width));
        identity_digest = mix_digest(identity_digest, u64::from(height));
        identity_digest = mix_digest(identity_digest, u64::from(safe_column_bits));
        for column in 0..width as usize {
            identity_digest = mix_digest(identity_digest, column_masks[column]);
            identity_digest = mix_digest(identity_digest, left_masks[column]);
            identity_digest = mix_digest(identity_digest, right_masks[column]);
        }

        Self {
            width,
            height,
            column_masks,
            left_masks,
            right_masks,
            safe_column_bits,
            identity_digest,
        }
    }

    pub fn certified_split(&self, remaining: u64) -> Option<CertifiedSeparatorSplit> {
        let mut best = None;
        for column in 0..self.width {
            if self.safe_column_bits & (1_u64 << column) == 0
                || remaining & self.column_masks[column as usize] != 0
            {
                continue;
            }
            let left = remaining & self.left_masks[column as usize];
            let right = remaining & self.right_masks[column as usize];
            if left == 0 || right == 0 {
                continue;
            }
            let (owner_cells, remainder_cells) = ordered_components(left, right);
            let key = (
                owner_cells.count_ones(),
                owner_cells.trailing_zeros(),
                column,
            );
            if best.is_none_or(|(best_key, _)| key < best_key) {
                best = Some((
                    key,
                    CertifiedSeparatorSplit {
                        owner_cells,
                        remainder_cells,
                        separator_column: column,
                    },
                ));
            }
        }
        best.map(|(_, split)| split)
    }

    pub fn dynamic_bumper_cells(&self, remaining: u64) -> impl Iterator<Item = u8> + '_ {
        (0..self.width).filter_map(move |column| {
            if self.safe_column_bits & (1_u64 << column) == 0 {
                return None;
            }
            let top_cell = (self.height - 1) * self.width + column;
            (remaining & self.column_masks[column as usize] == 1_u64 << top_cell)
                .then_some(top_cell)
        })
    }

    pub fn bumper_row_compatible(&self, remaining: u64, bumper_cell: u8, row: u64) -> bool {
        let column = bumper_cell % self.width;
        if self.safe_column_bits & (1_u64 << column) == 0
            || row & (1_u64 << bumper_cell) == 0
            || row & self.column_masks[column as usize] != 1_u64 << bumper_cell
        {
            return false;
        }
        let left_mask = self.left_masks[column as usize];
        let right_mask = self.right_masks[column as usize];
        let left_demand = (remaining & left_mask).count_ones();
        let right_demand = (remaining & right_mask).count_ones();
        let left_supply = (row & left_mask).count_ones();
        let right_supply = (row & right_mask).count_ones();
        left_supply <= left_demand
            && right_supply <= right_demand
            && (left_demand - left_supply).is_multiple_of(4)
            && (right_demand - right_supply).is_multiple_of(4)
    }

    pub const fn safe_column_bits(&self) -> u64 {
        self.safe_column_bits
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub fn retained_bytes(&self) -> usize {
        self.column_masks.capacity() * core::mem::size_of::<u64>()
            + self.left_masks.capacity() * core::mem::size_of::<u64>()
            + self.right_masks.capacity() * core::mem::size_of::<u64>()
    }
}

fn ordered_components(left: u64, right: u64) -> (u64, u64) {
    let left_key = (left.count_ones(), left.trailing_zeros());
    let right_key = (right.count_ones(), right.trailing_zeros());
    if left_key <= right_key {
        (left, right)
    } else {
        (right, left)
    }
}
