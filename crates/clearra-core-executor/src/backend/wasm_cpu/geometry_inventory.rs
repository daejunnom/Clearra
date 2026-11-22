use super::{pack_piece_counts, unpack_piece_counts, TargetGroup};

const ALL_KINDS: u8 = 0x7f;
const RAW_COUNTS_MASK: u32 = 0x0fff_ffff;
const TWO_OF_EACH: u32 = 0x0222_2222;
const INVENTORY_ALIAS: u32 = 1 << 31;
const ONE_EXTRA_CHOICE: u32 = 1 << 30;
const COMPLETE_PREFIX_COUNT: usize = 7_290;

/// A capability derived from the actual seven target keys and their complete
/// permutation-closed prefix domain. It says nothing about queue membership,
/// source ordinals, Hold, coverage or physical buildability.
#[derive(Clone, Copy, Debug)]
pub(super) struct TwoBagPlusOneInventory {
    _authenticated: (),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InventoryAnalysis {
    feasible_piece_mask: u8,
    memo_code: u32,
}

impl InventoryAnalysis {
    pub(super) const fn feasible_piece_mask(self) -> u8 {
        self.feasible_piece_mask
    }

    pub(super) const fn memo_code(self) -> u32 {
        self.memo_code
    }
}

impl TwoBagPlusOneInventory {
    pub(super) fn compile(targets: &[TargetGroup], prefixes: &[u32]) -> Option<Self> {
        if !Self::matches_targets(targets)
            || prefixes.len() != COMPLETE_PREFIX_COUNT
            || prefixes.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return None;
        }
        let descriptor = Self { _authenticated: () };
        // There are exactly 3^7 prefixes with every count <= 2 and 7*3^6
        // prefixes with one count == 3. A strictly increasing list of this
        // size containing only these values is the complete domain.
        for packed in prefixes.iter().copied() {
            if packed & !RAW_COUNTS_MASK != 0
                || descriptor
                    .analyze_used(unpack_piece_counts(packed), packed)
                    .is_none()
            {
                return None;
            }
        }
        Some(descriptor)
    }

    pub(super) fn matches_targets(targets: &[TargetGroup]) -> bool {
        if targets.len() != 7 {
            return false;
        }
        let mut seen = 0_u8;
        for target in targets {
            let mut extra = None;
            for (piece, count) in target.key.counts().into_iter().enumerate() {
                match count {
                    2 => {}
                    3 if extra.is_none() => extra = Some(piece),
                    _ => return false,
                }
            }
            let Some(extra) = extra else {
                return false;
            };
            let bit = 1 << extra;
            if seen & bit != 0 {
                return false;
            }
            seen |= bit;
        }
        seen == ALL_KINDS
    }

    pub(super) fn analyze_used(
        self,
        used_counts: [u8; 7],
        packed_counts: u32,
    ) -> Option<InventoryAnalysis> {
        debug_assert_eq!(packed_counts, pack_piece_counts(used_counts));
        let mut extra_used = None;
        let mut below_two = 0_u8;
        for (piece, count) in used_counts.into_iter().enumerate() {
            match count {
                0 | 1 => below_two |= 1 << piece,
                2 => {}
                3 if extra_used.is_none() => extra_used = Some(piece),
                _ => return None,
            }
        }
        let (feasible_piece_mask, memo_code) = if let Some(extra) = extra_used {
            // The extra kind has already been fixed. All valid targets leave
            // this one exact inventory, independent of which history used it.
            let remaining = TWO_OF_EACH + (1 << (extra * 4)) - packed_counts;
            (below_two, INVENTORY_ALIAS | remaining)
        } else {
            // Every target is still possible: residual base plus one of the
            // seven kinds. Keep this seven-element language distinct from a
            // singleton even when their packed base vectors are equal.
            let remaining_base = TWO_OF_EACH - packed_counts;
            (
                ALL_KINDS,
                INVENTORY_ALIAS | ONE_EXTRA_CHOICE | remaining_base,
            )
        };
        Some(InventoryAnalysis {
            feasible_piece_mask,
            memo_code,
        })
    }
}
