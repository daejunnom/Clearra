// SRP rationale: this module owns only the compile-time product prune policy
// and the explicitly local-only same-binary A/B override for that policy.

#[cfg(feature = "local-search-ab")]
use core::sync::atomic::{AtomicU8, Ordering};
#[cfg(feature = "local-search-ab")]
use std::sync::{Arc, OnceLock};

#[cfg(feature = "local-search-ab")]
use clearra_rules::kicks::KickTableProfileId;

#[cfg(feature = "local-search-ab")]
const ADDITIVE_PARITY: u8 = 1 << 0;
#[cfg(feature = "local-search-ab")]
const APDP: u8 = 1 << 1;
#[cfg(feature = "local-search-ab")]
const DEPENDENCY_RELAXATION: u8 = 1 << 2;
#[cfg(feature = "local-search-ab")]
const LEGAL_BOARD: u8 = 1 << 3;
#[cfg(feature = "local-search-ab")]
const PRODUCT_BITS: u8 = ADDITIVE_PARITY | APDP;

#[cfg(feature = "local-search-ab")]
static LOCAL_POLICY: AtomicU8 = AtomicU8::new(PRODUCT_BITS);

#[cfg(feature = "local-search-ab")]
static LOCAL_PC4_LEGAL_BOARD: OnceLock<Arc<LocalPc4LegalBoardIndex>> = OnceLock::new();

#[cfg(feature = "local-search-ab")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSearchPrunePolicy {
    pub additive_parity: bool,
    pub apdp: bool,
    pub dependency_relaxation: bool,
    pub legal_board: bool,
}

#[cfg(feature = "local-search-ab")]
impl LocalSearchPrunePolicy {
    pub const fn product_default() -> Self {
        Self {
            additive_parity: true,
            apdp: true,
            dependency_relaxation: false,
            legal_board: false,
        }
    }

    pub const fn new(additive_parity: bool, apdp: bool, dependency_relaxation: bool) -> Self {
        Self {
            additive_parity,
            apdp,
            dependency_relaxation,
            legal_board: false,
        }
    }

    pub const fn with_legal_board(mut self, legal_board: bool) -> Self {
        self.legal_board = legal_board;
        self
    }

    const fn bits(self) -> u8 {
        (self.additive_parity as u8) * ADDITIVE_PARITY
            | (self.apdp as u8) * APDP
            | (self.dependency_relaxation as u8) * DEPENDENCY_RELAXATION
            | (self.legal_board as u8) * LEGAL_BOARD
    }

    const fn from_bits(bits: u8) -> Self {
        Self {
            additive_parity: bits & ADDITIVE_PARITY != 0,
            apdp: bits & APDP != 0,
            dependency_relaxation: bits & DEPENDENCY_RELAXATION != 0,
            legal_board: bits & LEGAL_BOARD != 0,
        }
    }
}

/// Immutable, profile-bound reverse-completable board domain used only by a
/// local benchmark binary. Product builds neither expose an installer nor pay
/// a lookup cost. Each layer contains sorted Clearra Board64 masks after line
/// clears have been normalized into a full bottom-row prefix.
#[cfg(feature = "local-search-ab")]
#[derive(Debug)]
pub struct LocalPc4LegalBoardIndex {
    kick_profile: KickTableProfileId,
    layers: [Vec<u64>; 11],
}

#[cfg(feature = "local-search-ab")]
impl LocalPc4LegalBoardIndex {
    pub fn new(
        kick_profile: KickTableProfileId,
        mut layers: [Vec<u64>; 11],
    ) -> Result<Self, &'static str> {
        for (layer, fields) in layers.iter_mut().enumerate() {
            fields.sort_unstable();
            fields.dedup();
            if fields
                .iter()
                .any(|field| field.count_ones() != (layer as u32) * 4)
            {
                return Err("local_legal_board_layer_area_mismatch");
            }
        }
        if layers[0].binary_search(&0).is_err()
            || layers[10].binary_search(&((1_u64 << 40) - 1)).is_err()
        {
            return Err("local_legal_board_terminal_domain_incomplete");
        }
        Ok(Self {
            kick_profile,
            layers,
        })
    }

    fn contains(&self, depth: usize, normalized_board: u64) -> bool {
        self.layers
            .get(depth)
            .is_some_and(|layer| layer.binary_search(&normalized_board).is_ok())
    }
}

#[cfg(feature = "local-search-ab")]
pub fn install_local_pc4_legal_board_index(
    index: LocalPc4LegalBoardIndex,
) -> Result<(), &'static str> {
    LOCAL_PC4_LEGAL_BOARD
        .set(Arc::new(index))
        .map_err(|_| "local_legal_board_index_already_installed")
}

/// Changes the process-wide policy only in a binary explicitly built for
/// local A/B. Callers must serialize complete searches and restore the prior
/// value; release builds have no setter or atomic load.
#[cfg(feature = "local-search-ab")]
pub fn set_local_search_prune_policy(policy: LocalSearchPrunePolicy) -> LocalSearchPrunePolicy {
    LocalSearchPrunePolicy::from_bits(LOCAL_POLICY.swap(policy.bits(), Ordering::SeqCst))
}

#[cfg(feature = "local-search-ab")]
pub fn local_search_prune_policy() -> LocalSearchPrunePolicy {
    LocalSearchPrunePolicy::from_bits(LOCAL_POLICY.load(Ordering::SeqCst))
}

#[inline(always)]
pub(crate) fn additive_parity_enabled() -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        return LOCAL_POLICY.load(Ordering::Relaxed) & ADDITIVE_PARITY != 0;
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        true
    }
}

#[inline(always)]
pub(crate) fn apdp_enabled() -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        return LOCAL_POLICY.load(Ordering::Relaxed) & APDP != 0;
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        true
    }
}

#[inline(always)]
pub(crate) fn dependency_relaxation_enabled() -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        return LOCAL_POLICY.load(Ordering::Relaxed) & DEPENDENCY_RELAXATION != 0;
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

/// Returns false only inside the explicitly local A/B feature and only for an
/// empty-origin 10x4 query bound to the installed kick profile. Missing data,
/// arbitrary initial fields, 5L+ targets, profile mismatch, or malformed
/// normalization all fail open to the ordinary exact BuildUp traversal.
#[inline(always)]
pub(crate) fn local_pc4_legal_board_allows(
    width: u8,
    height: u8,
    initial_board: u64,
    kick_profile: clearra_rules::kicks::KickTableProfileId,
    physical_board: u64,
    deleted_rows: u16,
    depth: usize,
) -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        if LOCAL_POLICY.load(Ordering::Relaxed) & LEGAL_BOARD == 0
            || width != 10
            || height != 4
            || initial_board != 0
            || depth > 10
        {
            return true;
        }
        let Some(index) = LOCAL_PC4_LEGAL_BOARD.get() else {
            return true;
        };
        if index.kick_profile != kick_profile {
            return true;
        }
        if deleted_rows >> height != 0 {
            return true;
        }
        let row_mask = (1_u64 << width) - 1;
        let mut normalized_board = 0_u64;
        let mut physical_row = 0_u32;
        for target_row in 0..height {
            let row = if deleted_rows & (1_u16 << target_row) != 0 {
                row_mask
            } else {
                let row = (physical_board >> (physical_row * u32::from(width))) & row_mask;
                physical_row += 1;
                row
            };
            normalized_board |= row << (u32::from(target_row) * u32::from(width));
        }
        if physical_board >> (physical_row * u32::from(width)) != 0 {
            return true;
        }
        if normalized_board.count_ones() != (depth as u32) * 4 {
            return true;
        }
        return index.contains(depth, normalized_board);
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        let _ = (
            width,
            height,
            initial_board,
            kick_profile,
            physical_board,
            deleted_rows,
            depth,
        );
        true
    }
}

#[cfg(all(test, feature = "local-search-ab"))]
mod tests {
    use clearra_rules::kicks::KickTableProfileId;

    use super::{
        install_local_pc4_legal_board_index, local_pc4_legal_board_allows,
        set_local_search_prune_policy, LocalPc4LegalBoardIndex, LocalSearchPrunePolicy,
    };

    #[test]
    fn local_legal_board_filter_is_strictly_scoped_to_matching_empty_four_rows() {
        let mut layers: [Vec<u64>; 11] = std::array::from_fn(|_| Vec::new());
        layers[0].push(0);
        layers[1].push(0b1111);
        let non_prefix_cleared_target = (((1_u64 << 10) - 1) << 20) | (0b11 << 30);
        layers[3].push(non_prefix_cleared_target);
        layers[10].push((1_u64 << 40) - 1);
        install_local_pc4_legal_board_index(
            LocalPc4LegalBoardIndex::new(KickTableProfileId::SrsPlus, layers).unwrap(),
        )
        .unwrap();
        let previous = set_local_search_prune_policy(
            LocalSearchPrunePolicy::product_default().with_legal_board(true),
        );

        assert!(local_pc4_legal_board_allows(
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            0b1111,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            0b11 << 20,
            1 << 2,
            3,
        ));
        assert!(!local_pc4_legal_board_allows(
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            10,
            5,
            0,
            KickTableProfileId::SrsPlus,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            10,
            4,
            1,
            KickTableProfileId::SrsPlus,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            10,
            4,
            0,
            KickTableProfileId::Jstris180,
            0b11110,
            0,
            1,
        ));

        set_local_search_prune_policy(previous);
    }
}
