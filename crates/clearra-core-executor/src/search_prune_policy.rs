// SRP rationale: this module owns only the compile-time product prune policy
// and the explicitly local-only same-binary A/B override for that policy.

#[cfg(feature = "local-search-ab")]
use core::sync::atomic::{AtomicU8, Ordering};
#[cfg(feature = "local-search-ab")]
use std::sync::{Arc, OnceLock};

use crate::legal_board::{
    CompletionCapability, LegalBoardDecision, LegalBoardQuery, QualifiedExactLegalBoard,
};
#[cfg(feature = "local-search-ab")]
use crate::legal_board::{ExactLegalBoard, LegalBoardExpectation};
#[cfg(feature = "local-search-ab")]
const ADDITIVE_PARITY: u8 = 1 << 0;
#[cfg(feature = "local-search-ab")]
const APDP: u8 = 1 << 1;
#[cfg(feature = "local-search-ab")]
const DEPENDENCY_RELAXATION: u8 = 1 << 2;
#[cfg(feature = "local-search-ab")]
const LEGAL_BOARD: u8 = 1 << 3;
#[cfg(feature = "local-search-ab")]
const CONDITIONED_REACHABILITY: u8 = 1 << 4;
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
    pub conditioned_reachability: bool,
}

#[cfg(feature = "local-search-ab")]
impl LocalSearchPrunePolicy {
    pub const fn product_default() -> Self {
        Self {
            additive_parity: true,
            apdp: true,
            dependency_relaxation: false,
            legal_board: false,
            conditioned_reachability: false,
        }
    }

    pub const fn new(additive_parity: bool, apdp: bool, dependency_relaxation: bool) -> Self {
        Self {
            additive_parity,
            apdp,
            dependency_relaxation,
            legal_board: false,
            conditioned_reachability: false,
        }
    }

    pub const fn with_legal_board(mut self, legal_board: bool) -> Self {
        self.legal_board = legal_board;
        self
    }

    pub const fn with_conditioned_reachability(mut self, enabled: bool) -> Self {
        self.conditioned_reachability = enabled;
        self
    }

    const fn bits(self) -> u8 {
        (self.additive_parity as u8) * ADDITIVE_PARITY
            | (self.apdp as u8) * APDP
            | (self.dependency_relaxation as u8) * DEPENDENCY_RELAXATION
            | (self.legal_board as u8) * LEGAL_BOARD
            | (self.conditioned_reachability as u8) * CONDITIONED_REACHABILITY
    }

    const fn from_bits(bits: u8) -> Self {
        Self {
            additive_parity: bits & ADDITIVE_PARITY != 0,
            apdp: bits & APDP != 0,
            dependency_relaxation: bits & DEPENDENCY_RELAXATION != 0,
            legal_board: bits & LEGAL_BOARD != 0,
            conditioned_reachability: bits & CONDITIONED_REACHABILITY != 0,
        }
    }
}

/// Immutable, exact `F_k ∩ R_k` legal-board bundle used only by a local A/B
/// binary until the product installer and signed catalog are qualified.
#[cfg(feature = "local-search-ab")]
#[derive(Debug)]
pub struct LocalPc4LegalBoardIndex {
    board: ExactLegalBoard,
}

#[cfg(feature = "local-search-ab")]
impl LocalPc4LegalBoardIndex {
    pub fn load_bundle(
        bytes: Arc<[u8]>,
        expectation: LegalBoardExpectation,
    ) -> Result<Self, &'static str> {
        let board = ExactLegalBoard::load(bytes, expectation)
            .map_err(|_| "local_legal_board_bundle_invalid")?;
        Ok(Self { board })
    }

    pub fn compressed_bytes(&self) -> usize {
        self.board.compressed_bytes()
    }

    pub fn sparse_index_bytes(&self) -> usize {
        self.board.sparse_index_bytes()
    }

    pub fn decide_negative_only(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        self.board.decide_negative_only(query)
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

#[inline(always)]
pub(crate) fn conditioned_reachability_enabled() -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        return LOCAL_POLICY.load(Ordering::Relaxed) & CONDITIONED_REACHABILITY != 0;
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        true
    }
}

/// Returns false only for a qualified exact-intersection asset and its closed
/// empty-origin 10x4 domain. Local A/B builds additionally require their
/// explicit feature bit. Every scope, profile, state, or asset miss fails open
/// to the ordinary exact BuildUp traversal. The hot path uses a verified
/// no-false-negative filter: positive collisions only lose a prune opportunity.
#[inline(always)]
pub(crate) fn local_pc4_legal_board_allows(
    qualified: Option<&QualifiedExactLegalBoard>,
    width: u8,
    height: u8,
    initial_board: u64,
    kick_profile: clearra_rules::kicks::KickTableProfileId,
    clear_to_empty_completion: bool,
    physical_board: u64,
    deleted_rows: u16,
    depth: usize,
) -> bool {
    #[cfg(feature = "local-search-ab")]
    {
        if LOCAL_POLICY.load(Ordering::Relaxed) & LEGAL_BOARD == 0 {
            return true;
        }
        let query = LegalBoardQuery {
            width,
            height,
            initial_board,
            kick_profile,
            physical_board,
            deleted_original_rows: deleted_rows,
            placed_piece_count: depth,
            completion: if clear_to_empty_completion {
                CompletionCapability::ClearToEmpty
            } else {
                CompletionCapability::Other
            },
        };
        if let Some(index) = LOCAL_PC4_LEGAL_BOARD.get() {
            return !matches!(
                index.decide_negative_only(query),
                LegalBoardDecision::VerifiedAbsent
            );
        }
        let Some(index) = qualified else {
            return true;
        };
        return !matches!(
            index.decide_negative_only(query),
            LegalBoardDecision::VerifiedAbsent
        );
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        let Some(index) = qualified else {
            return true;
        };
        !matches!(
            index.decide_negative_only(LegalBoardQuery {
                width,
                height,
                initial_board,
                kick_profile,
                physical_board,
                deleted_original_rows: deleted_rows,
                placed_piece_count: depth,
                completion: if clear_to_empty_completion {
                    CompletionCapability::ClearToEmpty
                } else {
                    CompletionCapability::Other
                },
            }),
            LegalBoardDecision::VerifiedAbsent
        )
    }
}

#[cfg(all(test, feature = "local-search-ab"))]
mod tests {
    use std::sync::Arc;

    use crate::legal_board::{
        built_in_binding, bundle_key_from_clearra_board, encode_exact_intersection,
        LegalBoardExpectation,
    };
    use clearra_rules::kicks::KickTableProfileId;

    use super::{
        install_local_pc4_legal_board_index, local_pc4_legal_board_allows,
        set_local_search_prune_policy, LocalPc4LegalBoardIndex, LocalSearchPrunePolicy,
    };

    #[test]
    fn local_legal_board_filter_is_strictly_scoped_to_matching_empty_four_rows() {
        let mut layers: [Vec<u64>; 11] = std::array::from_fn(|_| Vec::new());
        layers[0].push(0);
        layers[1].push(bundle_key_from_clearra_board(0b1111));
        let bottom_prefix_target = ((1_u64 << 10) - 1) | (0b11 << 30);
        layers[3].push(bundle_key_from_clearra_board(bottom_prefix_target));
        layers[10].push((1_u64 << 40) - 1);
        let binding = built_in_binding(KickTableProfileId::SrsPlus).unwrap();
        let bytes = encode_exact_intersection(binding, &layers).unwrap();
        let generation = bytes[80..112].try_into().unwrap();
        install_local_pc4_legal_board_index(
            LocalPc4LegalBoardIndex::load_bundle(
                Arc::from(bytes),
                LegalBoardExpectation {
                    binding,
                    generation_identity: Some(generation),
                },
            )
            .unwrap(),
        )
        .unwrap();
        let previous = set_local_search_prune_policy(
            LocalSearchPrunePolicy::product_default().with_legal_board(true),
        );

        assert!(local_pc4_legal_board_allows(
            None,
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            true,
            0b1111,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            None,
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            true,
            0b11 << 20,
            1 << 2,
            3,
        ));
        assert!(!local_pc4_legal_board_allows(
            None,
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            true,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            None,
            10,
            5,
            0,
            KickTableProfileId::SrsPlus,
            true,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            None,
            10,
            4,
            1,
            KickTableProfileId::SrsPlus,
            true,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            None,
            10,
            4,
            0,
            KickTableProfileId::Jstris180,
            true,
            0b11110,
            0,
            1,
        ));
        assert!(local_pc4_legal_board_allows(
            None,
            10,
            4,
            0,
            KickTableProfileId::SrsPlus,
            false,
            0b11110,
            0,
            1,
        ));

        set_local_search_prune_policy(previous);
    }
}
