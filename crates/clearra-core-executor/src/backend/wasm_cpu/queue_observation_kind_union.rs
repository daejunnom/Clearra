//! Exact derivatives of unions of canonical kind-word languages.
//!
//! Canonical kind nodes can have several children for one kind. A union
//! derivative therefore collects every matching child from every member;
//! choosing one member before reading the suffix would change the language.

use crate::backend::wasm_cpu::{
    piece_order_language::PieceOrderLanguageCache, WasmExactSearchError,
};

#[cfg(feature = "placement-dependency-pc")]
#[path = "queue_observation_word_bitmap.rs"]
mod word_bitmap;

#[cfg(feature = "placement-dependency-pc")]
pub(in crate::backend::wasm_cpu) use word_bitmap::KnownSupplyQuery;

/// Shared by the existing eager union builder and the demand-driven owner.
pub(in crate::backend::wasm_cpu) fn append_kind_children(
    source: &PieceOrderLanguageCache,
    root: u32,
    piece: u8,
    output: &mut Vec<u32>,
) -> Result<usize, WasmExactSearchError> {
    append_kind_children_bounded(source, root, piece, output, usize::MAX)
}

fn append_kind_children_bounded(
    source: &PieceOrderLanguageCache,
    root: u32,
    piece: u8,
    output: &mut Vec<u32>,
    limit: usize,
) -> Result<usize, WasmExactSearchError> {
    let edges = source
        .edges_for_piece(root, piece)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_observation_language_edge_out_of_range",
        ))?;
    output
        .len()
        .checked_add(edges.len())
        .filter(|&len| len <= limit)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_observation_physical_union_limit",
        ))?;
    output.try_reserve(edges.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_observation_language_union_storage_unavailable")
    })?;
    output.extend(edges.iter().map(|edge| edge.child()));
    Ok(edges.len())
}

#[cfg(feature = "placement-dependency-pc")]
mod lazy {
    use std::{
        collections::{hash_map::DefaultHasher, HashMap},
        hash::{Hash, Hasher},
        sync::Arc,
    };

    use clearra_core_domain::execution_cancellation::ExecutionControl;

    use super::{
        append_kind_children_bounded,
        word_bitmap::{
            compile_target_query, multiset_size, BitmapCoverage, KnownSupplyQuery, QueryKey,
            QueryProfile, RankedBits, StoredBitmap, TargetQueryKey, MAX_QUERY_CACHE,
            MAX_QUERY_GROUPS,
        },
        PieceOrderLanguageCache, WasmExactSearchError,
    };

    pub(in crate::backend::wasm_cpu) const KIND_SOURCE_TAG: u32 = 1 << 31;
    const NO_NODE: u32 = u32::MAX;
    const NO_MULTISET: u32 = u32::MAX;
    const MULTISET_COUNTS_MASK: u32 = (1 << 28) - 1;
    const FULL_MULTISET: u32 = 1 << 28;

    /// Group indices stay below the existing finite node bound. Their spare
    /// high bit records a Full witness without another allocation or member.
    #[derive(Clone, Copy)]
    struct MultisetGroupIndex(usize);

    impl MultisetGroupIndex {
        const FULL: usize = 1 << (usize::BITS - 1);

        fn new(index: usize) -> Self {
            debug_assert!(index < Self::FULL);
            Self(index)
        }

        fn index(self) -> usize {
            self.0 & !Self::FULL
        }

        fn is_full(self) -> bool {
            self.0 & Self::FULL != 0
        }

        fn mark_full(&mut self) {
            self.0 |= Self::FULL;
        }
    }

    struct StoredWordProfile {
        profile: QueryProfile,
        targets: [Option<TargetQueryKey>; MAX_QUERY_GROUPS],
    }

    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    struct BoardWordKey {
        board: u32,
        counts: u32,
    }

    enum BoardWordPayload {
        Empty,
        Full,
        Partial(Vec<u64>),
    }

    impl BoardWordPayload {
        fn retained_bytes(&self) -> usize {
            match self {
                Self::Partial(words) => words.capacity() * core::mem::size_of::<u64>(),
                Self::Empty | Self::Full => 0,
            }
        }
    }

    struct BoardWordBitmap {
        remaining: u8,
        root_count: usize,
        // Unknown NFA multisets or an optional root-bitmap refusal cannot
        // turn a negative known-root union into a negative whole language.
        unknown: bool,
        payload: BoardWordPayload,
    }

    #[derive(Clone, Copy)]
    pub(in crate::backend::wasm_cpu) struct KindUnionResources {
        pub nodes: usize,
        pub edges: usize,
        pub members: usize,
        pub work: usize,
    }

    struct UnionNode {
        sources: Vec<u32>,
        // References are either local union IDs or KIND_SOURCE_TAG | kind ID.
        // Each requested kind is appended once, keeping old edge IDs stable.
        edges: Vec<(u8, u32)>,
        remaining: u8,
        prepared: u8,
        next_same_hash: u32,
    }

    /// An optional index of one frozen board's immutable root list. Unknown
    /// certificates stay in their own group and are included in every query.
    struct BoardMultisetGroups {
        remaining: u8,
        has_full: bool,
        root_count: usize,
        by_multiset: HashMap<u32, MultisetGroupIndex>,
        groups: Vec<Vec<u32>>,
        unknown: Vec<u32>,
        member_capacity_bytes: usize,
    }

    impl BoardMultisetGroups {
        fn new(remaining: u8, root_count: usize) -> Self {
            Self {
                remaining,
                has_full: false,
                root_count,
                by_multiset: HashMap::new(),
                groups: Vec::new(),
                unknown: Vec::new(),
                member_capacity_bytes: 0,
            }
        }

        fn retained_bytes(&self) -> usize {
            self.by_multiset.capacity() * (core::mem::size_of::<(u32, MultisetGroupIndex)>() + 1)
                + self.groups.capacity() * core::mem::size_of::<Vec<u32>>()
                + self.member_capacity_bytes
        }

        // Dropping a Vec<u32> deallocates its buffer without visiting each
        // primitive member. Count the board/group owners actually destroyed.
        fn drop_work(&self) -> usize {
            1 + self.groups.len() + usize::from(self.unknown.capacity() != 0)
        }
    }

    pub(in crate::backend::wasm_cpu) struct KindUnionArena {
        base: KindUnionResources,
        limits: KindUnionResources,
        nodes: Vec<UnionNode>,
        heads: HashMap<u64, u32>,
        scratch: Vec<u32>,
        members: usize,
        edges: usize,
        source_visits: usize,
        work: usize,
        member_capacity_bytes: usize,
        edge_capacity_bytes: usize,
        peak_bytes: usize,
        failed: bool,
        kind_arena: Option<Arc<()>>,
        // Sparse and demand-driven: only the suffix DAG reached by an actual
        // revealed supply is certified. Numeric IDs stay pinned to this arena.
        multisets: HashMap<u32, u32>,
        supply_aliases: HashMap<u64, u32>,
        supply_metrics: [usize; 4],
        full_multiset_enabled: bool,
        // Queries, roots checked while building shared groups, new uniform
        // certificate nodes, sufficient Full proofs, positive witnesses,
        // and additional work (a subset of the same cumulative kind work).
        full_multiset_metrics: [usize; 6],
        multiset_grouping_enabled: bool,
        multiset_groups: Vec<BoardMultisetGroups>,
        multiset_group_index: HashMap<u32, usize>,
        group_members: usize,
        group_capacity_bytes: usize,
        group_staging_bytes: usize,
        group_drop_work: usize,
        // builds, root visits, key lookups, matched visits, unknown visits,
        // fallbacks, work, evictions. Retained members is read from the owner.
        group_metrics: [usize; 8],
        // Optional known-supply membership, scoped by the same kind Arc.
        // One u64 payload word uses TWO of the existing u32 member credits;
        // original family/union/group members are never subtracted or reset.
        word_bitmaps: HashMap<u32, StoredBitmap>,
        word_queries: HashMap<TargetQueryKey, StoredBitmap>,
        word_profiles: HashMap<QueryKey, StoredWordProfile>,
        board_word_bitmaps: HashMap<BoardWordKey, BoardWordBitmap>,
        // Board/M lookups, hits, builds, published payload words, optional
        // fallbacks and the actual lookup/build work (a subset of kind work).
        word_bitmap_board_metrics: [usize; 6],
        word_bitmap_members: usize,
        word_bitmap_capacity_bytes: usize,
        word_bitmap_staging_bytes: usize,
        word_bitmap_peak_bytes: usize,
        word_bitmap_drop_work: usize,
        // queries, query hits, builds, query steps, root bitmap nodes,
        // all published payload words, intersection words, fallbacks, work.
        word_bitmap_metrics: [usize; 9],
    }

    impl KindUnionArena {
        pub(in crate::backend::wasm_cpu) fn new(
            base: KindUnionResources,
            limits: KindUnionResources,
        ) -> Result<Self, WasmExactSearchError> {
            if base.nodes > limits.nodes
                || base.edges > limits.edges
                || base.members > limits.members
                || base.work > limits.work
                || limits.nodes >= KIND_SOURCE_TAG as usize
            {
                return Err(invalid("wasm_observation_kind_union_base_limit"));
            }
            Ok(Self {
                base,
                limits,
                nodes: Vec::new(),
                heads: HashMap::new(),
                scratch: Vec::new(),
                members: 0,
                edges: 0,
                source_visits: 0,
                work: 0,
                member_capacity_bytes: 0,
                edge_capacity_bytes: 0,
                peak_bytes: 0,
                failed: false,
                kind_arena: None,
                multisets: HashMap::new(),
                supply_aliases: HashMap::new(),
                supply_metrics: [0; 4],
                full_multiset_enabled: false,
                full_multiset_metrics: [0; 6],
                multiset_grouping_enabled: false,
                multiset_groups: Vec::new(),
                multiset_group_index: HashMap::new(),
                group_members: 0,
                group_capacity_bytes: 0,
                group_staging_bytes: 0,
                group_drop_work: 0,
                group_metrics: [0; 8],
                word_bitmaps: HashMap::new(),
                word_queries: HashMap::new(),
                word_profiles: HashMap::new(),
                board_word_bitmaps: HashMap::new(),
                word_bitmap_board_metrics: [0; 6],
                word_bitmap_members: 0,
                word_bitmap_capacity_bytes: 0,
                word_bitmap_staging_bytes: 0,
                word_bitmap_peak_bytes: 0,
                word_bitmap_drop_work: 0,
                word_bitmap_metrics: [0; 9],
            })
        }

        pub(in crate::backend::wasm_cpu) fn set_multiset_grouping_enabled(
            &mut self,
            enabled: bool,
        ) {
            self.multiset_grouping_enabled = enabled;
        }

        /// Configure once on a fresh view. Previously cached ordinary
        /// certificates remain conservative if a direct caller enables this
        /// later: an absent Full flag never means the language is incomplete.
        pub(in crate::backend::wasm_cpu) fn set_full_multiset_enabled(&mut self, enabled: bool) {
            self.full_multiset_enabled = enabled;
        }

        pub(in crate::backend::wasm_cpu) fn full_multiset_metrics(&self) -> [usize; 6] {
            self.full_multiset_metrics
        }

        fn full_multiset_charge(
            &mut self,
            steps: usize,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            self.charge(steps, control)?;
            self.full_multiset_metrics[5] += steps;
            Ok(())
        }

        /// A positive-only shortcut for the caller's actual known suffix.
        /// A Full root supplies every word of one exact multiset M. Under
        /// KnownSupplyQuery's existing no-EOF contract, len >= remaining and
        /// M <= available ensure at least one legal Hold word. An unproved
        /// root, optional cache refusal or unavailable M keeps the old DP.
        ///
        /// `board` pins an immutable root list in this frozen view. Group
        /// construction is shared with intern_for_supply; hot queries inspect
        /// at most seven M keys, never the complete root list again.
        #[allow(clippy::too_many_arguments)]
        pub(in crate::backend::wasm_cpu) fn full_multiset_membership(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            let result = self.bind_kinds(kinds).and_then(|()| {
                self.full_multiset_membership_inner(board, remaining, roots, query, kinds, control)
            });
            if result.is_err() {
                self.failed = true;
            }
            result
        }

        #[allow(clippy::too_many_arguments)]
        fn full_multiset_membership_inner(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            self.charge(0, control)?;
            if !self.full_multiset_enabled || !self.multiset_grouping_enabled {
                return Ok(None);
            }
            self.full_multiset_metrics[0] += 1;
            self.full_multiset_charge(1, control)?;
            let Some(key) = query.key(remaining)? else {
                return Ok(None);
            };
            if roots.is_empty() {
                return Err(invalid("wasm_observation_kind_union_empty"));
            }
            if roots.len() > self.limits.members {
                return Err(invalid("wasm_observation_physical_union_limit"));
            }
            if key.supply.len < remaining {
                return Ok(None);
            }

            let before = self.work;
            self.full_multiset_charge(1, control)?;
            let grouped = self.ensure_multiset_groups(board, remaining, roots, kinds, control);
            self.group_metrics[6] += self.work - before;
            self.group_staging_bytes = 0;
            let Some(index) = grouped? else {
                return Ok(None);
            };
            // Most non-Full families require neither source counting nor
            // target construction here. The following ordinary DP reuses the
            // already validated group and uniform certificates.
            if !self.multiset_groups[index].has_full {
                return Ok(None);
            }
            let profile =
                QueryProfile::new(key, |steps| self.full_multiset_charge(steps, control))?;
            debug_assert!(profile.target_has_a_word());
            for target in 0..profile.target_count() {
                self.full_multiset_charge(1, control)?;
                self.group_metrics[2] += 1;
                let counts = profile
                    .target(target)
                    .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
                if self.multiset_groups[index]
                    .by_multiset
                    .get(&counts)
                    .is_some_and(|group| group.is_full())
                {
                    self.full_multiset_metrics[4] += 1;
                    return Ok(Some(true));
                }
            }
            Ok(None)
        }

        pub(in crate::backend::wasm_cpu) fn multiset_group_metrics(&self) -> [usize; 9] {
            [
                self.group_metrics[0],
                self.group_metrics[1],
                self.group_metrics[2],
                self.group_metrics[3],
                self.group_metrics[4],
                self.group_metrics[5],
                self.group_members,
                self.group_metrics[6],
                self.group_metrics[7],
            ]
        }

        pub(in crate::backend::wasm_cpu) fn metrics(&self) -> [usize; 5] {
            [
                self.nodes.len(),
                self.edges,
                self.members,
                self.source_visits,
                self.work,
            ]
        }

        pub(in crate::backend::wasm_cpu) fn supply_multiset_metrics(&self) -> [usize; 5] {
            [
                self.supply_metrics[0],
                self.supply_metrics[1],
                self.supply_metrics[2],
                self.supply_metrics[3],
                self.supply_aliases.len(),
            ]
        }

        pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
            self.nodes.capacity() * core::mem::size_of::<UnionNode>()
                + self.heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
                + self.scratch.capacity() * core::mem::size_of::<u32>()
                + self.member_capacity_bytes
                + self.edge_capacity_bytes
                + self.multisets.capacity() * (core::mem::size_of::<(u32, u32)>() + 1)
                + self.supply_aliases.capacity() * (core::mem::size_of::<(u64, u32)>() + 1)
                + self.multiset_groups.capacity() * core::mem::size_of::<BoardMultisetGroups>()
                + self.multiset_group_index.capacity() * (core::mem::size_of::<(u32, usize)>() + 1)
                + self.group_capacity_bytes
                + self.word_bitmap_retained_bytes()
        }

        pub(in crate::backend::wasm_cpu) fn peak_bytes(&self) -> usize {
            self.peak_bytes
        }

        fn observe_peak(&mut self, extra: usize) {
            self.peak_bytes = self.peak_bytes.max(
                self.retained_bytes()
                    .saturating_add(self.group_staging_bytes)
                    .saturating_add(self.word_bitmap_staging_bytes)
                    .saturating_add(extra),
            );
        }

        pub(in crate::backend::wasm_cpu) fn word_bitmap_metrics(&self) -> [usize; 9] {
            self.word_bitmap_metrics
        }

        pub(in crate::backend::wasm_cpu) fn word_bitmap_board_metrics(&self) -> [usize; 6] {
            self.word_bitmap_board_metrics
        }

        pub(in crate::backend::wasm_cpu) fn word_bitmap_retained_bytes(&self) -> usize {
            self.word_bitmaps.capacity() * (core::mem::size_of::<(u32, StoredBitmap)>() + 1)
                + self.word_queries.capacity()
                    * (core::mem::size_of::<(TargetQueryKey, StoredBitmap)>() + 1)
                + self.word_profiles.capacity()
                    * (core::mem::size_of::<(QueryKey, StoredWordProfile)>() + 1)
                + self.board_word_bitmaps.capacity()
                    * (core::mem::size_of::<(BoardWordKey, BoardWordBitmap)>() + 1)
                + self.word_bitmap_capacity_bytes
        }

        #[cfg(test)]
        pub(in crate::backend::wasm_cpu) fn word_bitmap_peak_bytes(&self) -> usize {
            self.word_bitmap_peak_bytes
        }

        fn observe_word_bitmap_peak(&mut self, extra: usize) {
            self.word_bitmap_peak_bytes = self.word_bitmap_peak_bytes.max(
                self.word_bitmap_retained_bytes()
                    .saturating_add(self.word_bitmap_staging_bytes)
                    .saturating_add(extra),
            );
            self.observe_peak(extra);
        }

        /// Exact existential membership for an actually revealed fixed
        /// suffix. The caller owns the original-source/no-target/no-projection
        /// certificate. None is an optional representation fallback, never a
        /// negative result. It leaves all charged work and exact languages.
        #[cfg(test)]
        pub(in crate::backend::wasm_cpu) fn word_bitmap_membership(
            &mut self,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            self.word_bitmap_membership_checked(None, remaining, roots, query, kinds, control)
        }

        /// The immutable board identity belongs to one frozen view and fixes
        /// its remaining depth and complete root set. Root order is irrelevant.
        /// The kind-arena pin is shared with every other union operation.
        pub(in crate::backend::wasm_cpu) fn word_bitmap_membership_for_board(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            self.word_bitmap_membership_checked(
                Some(board),
                remaining,
                roots,
                query,
                kinds,
                control,
            )
        }

        fn word_bitmap_membership_checked(
            &mut self,
            board: Option<u32>,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            let before = self.work;
            self.word_bitmap_metrics[0] += 1;
            let result = self.bind_kinds(kinds).and_then(|()| {
                self.word_bitmap_membership_inner(board, remaining, roots, query, kinds, control)
            });
            self.word_bitmap_metrics[8] += self.work - before;
            self.word_bitmap_staging_bytes = 0;
            if matches!(result, Ok(None)) {
                self.word_bitmap_metrics[7] += 1;
            }
            if result.is_err() {
                self.failed = true;
            }
            result
        }

        fn word_bitmap_membership_inner(
            &mut self,
            board: Option<u32>,
            remaining: u8,
            roots: &[u32],
            query: &KnownSupplyQuery,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            // Even a hot profile/board hit validates the caller's raw query
            // and passes charge(), including failed/cancel/peer-abort checks.
            let Some(key) = self.ensure_word_profile(remaining, query, control)? else {
                return Ok(None);
            };
            if roots.is_empty() {
                return Err(invalid("wasm_observation_kind_union_empty"));
            }
            if roots.len() > self.limits.members {
                return Err(invalid("wasm_observation_physical_union_limit"));
            }
            if let Some(board) = board {
                if self.word_profiles[&key].profile.target_has_a_word() {
                    return self
                        .board_word_membership(board, remaining, roots, key, kinds, control);
                }
            }
            self.root_word_membership(remaining, roots, key, kinds, control)
        }

        fn root_word_membership(
            &mut self,
            remaining: u8,
            roots: &[u32],
            key: QueryKey,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            let mut unknown = false;
            for &root in roots {
                self.charge(1, control)?;
                Self::check_supply_root(root, remaining, kinds)?;
                // Reuse the existing all-children uniform certificate. Once
                // a bitmap exists, its stored counts are that same proof.
                let counts = match self.word_bitmaps.get(&root) {
                    Some(bitmap) => Some(bitmap.counts),
                    None => self.uniform_multiset(root, kinds, control)?,
                };
                let Some(counts) = counts else {
                    unknown = true;
                    continue;
                };
                if multiset_size(counts) != Some(remaining) {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                self.word_query_charge(1, control)?;
                let Some(target) = self.word_profiles[&key].profile.target_index(counts) else {
                    // Every output uses r of at most r+1 available pieces.
                    continue;
                };
                let target = self.canonical_word_target(key, target, control)?;
                if !self.ensure_word_query(target, control)? {
                    unknown = true;
                    continue;
                }
                if !self.ensure_word_bitmap(root, counts, kinds, control)? {
                    unknown = true;
                    continue;
                }
                let words = self.word_bitmaps[&root].words.len();
                if self.word_queries[&target].words.len() != words {
                    return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                }
                // Precharge and perform every intersection word. The fold
                // intentionally does not short-circuit after an early bit.
                self.charge(words, control)?;
                self.word_bitmap_metrics[6] += words;
                let positive = self.word_bitmaps[&root]
                    .words
                    .iter()
                    .zip(&self.word_queries[&target].words)
                    .fold(false, |positive, (&word, &allowed)| {
                        positive | (word & allowed != 0)
                    });
                if positive {
                    return Ok(Some(true));
                }
            }
            // A positive witness needs only one certified root. A negative
            // answer needs every root; mixed/uncached roots keep the old DP.
            Ok((!unknown).then_some(false))
        }

        fn board_word_membership(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            key: QueryKey,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<bool>, WasmExactSearchError> {
            let mut unknown = false;
            let targets = self.word_profiles[&key].profile.target_count();
            for index in 0..targets {
                let counts = self.word_profiles[&key]
                    .profile
                    .target(index)
                    .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
                let board_key = BoardWordKey { board, counts };
                if !self.ensure_board_word_bitmap(board_key, remaining, roots, kinds, control)? {
                    unknown = true;
                    continue;
                }
                let cached = &self.board_word_bitmaps[&board_key];
                unknown |= cached.unknown;
                match &cached.payload {
                    // A target M<=A has a legal hold word iff len>=r under
                    // this API's no-EOF-release contract. That condition was
                    // checked before entering this function. Unknown roots
                    // cannot invalidate a positive known-language witness.
                    BoardWordPayload::Full => return Ok(Some(true)),
                    BoardWordPayload::Empty => continue,
                    BoardWordPayload::Partial(_) => {}
                }
                let target = self.canonical_word_target(key, index, control)?;
                if !self.ensure_word_query(target, control)? {
                    unknown = true;
                    continue;
                }
                let words = match &self.board_word_bitmaps[&board_key].payload {
                    BoardWordPayload::Partial(words) => words.len(),
                    _ => return Err(invalid("wasm_observation_word_bitmap_multiset_invalid")),
                };
                if self.word_queries[&target].words.len() != words {
                    return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                }
                self.charge(words, control)?;
                self.word_bitmap_metrics[6] += words;
                let BoardWordPayload::Partial(bitmap) =
                    &self.board_word_bitmaps[&board_key].payload
                else {
                    return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                };
                if bitmap
                    .iter()
                    .zip(&self.word_queries[&target].words)
                    .fold(false, |positive, (&word, &allowed)| {
                        positive | (word & allowed != 0)
                    })
                {
                    return Ok(Some(true));
                }
            }
            Ok((!unknown).then_some(false))
        }

        fn word_bitmap_room(&self, words: usize) -> bool {
            words
                .checked_mul(2)
                .and_then(|new| self.word_bitmap_members.checked_add(new))
                .and_then(|count| count.checked_add(self.base.members))
                .and_then(|count| count.checked_add(self.members))
                .and_then(|count| count.checked_add(self.group_members))
                .is_some_and(|count| count <= self.limits.members)
        }

        fn word_bitmap_metadata_room(&self) -> bool {
            self.word_bitmaps
                .len()
                .checked_add(self.board_word_bitmaps.len())
                .is_some_and(|len| len < self.limits.nodes)
        }

        fn word_query_charge(
            &mut self,
            steps: usize,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            self.charge(steps, control)?;
            // v17 includes raw profile/target-key preparation and cache
            // operations as well as actual target automaton/rank steps.
            self.word_bitmap_metrics[3] += steps;
            Ok(())
        }

        fn ensure_word_profile(
            &mut self,
            remaining: u8,
            query: &KnownSupplyQuery,
            control: &ExecutionControl,
        ) -> Result<Option<QueryKey>, WasmExactSearchError> {
            self.word_query_charge(1, control)?;
            let Some(key) = query.key(remaining)? else {
                return Ok(None);
            };
            if self.word_profiles.contains_key(&key) {
                return Ok(Some(key));
            }
            if self.word_profiles.len() >= MAX_QUERY_CACHE {
                return Ok(None);
            }
            let profile = QueryProfile::new(key, |steps| self.word_query_charge(steps, control))?;
            self.word_query_charge(1, control)?;
            let old = self.word_profiles.capacity()
                * (core::mem::size_of::<(QueryKey, StoredWordProfile)>() + 1);
            if self.word_profiles.try_reserve(1).is_err() {
                return Ok(None);
            }
            self.observe_word_bitmap_peak(
                if self.word_profiles.capacity()
                    * (core::mem::size_of::<(QueryKey, StoredWordProfile)>() + 1)
                    != old
                {
                    old
                } else {
                    0
                },
            );
            self.word_profiles.insert(
                key,
                StoredWordProfile {
                    profile,
                    targets: [None; MAX_QUERY_GROUPS],
                },
            );
            self.observe_word_bitmap_peak(0);
            Ok(Some(key))
        }

        fn canonical_word_target(
            &mut self,
            key: QueryKey,
            target: usize,
            control: &ExecutionControl,
        ) -> Result<TargetQueryKey, WasmExactSearchError> {
            self.word_query_charge(1, control)?;
            let entry = self
                .word_profiles
                .get(&key)
                .ok_or_else(|| invalid("wasm_observation_known_supply_invalid"))?;
            if target >= entry.profile.target_count() {
                return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
            }
            if let Some(key) = entry.targets[target] {
                return Ok(key);
            }
            let profile = entry.profile;
            let canonical =
                profile.canonical_target(target, |steps| self.word_query_charge(steps, control))?;
            self.word_query_charge(1, control)?;
            self.word_profiles
                .get_mut(&key)
                .ok_or_else(|| invalid("wasm_observation_known_supply_invalid"))?
                .targets[target] = Some(canonical);
            Ok(canonical)
        }

        fn ensure_word_query(
            &mut self,
            key: TargetQueryKey,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            self.word_query_charge(1, control)?;
            if self.word_queries.contains_key(&key) {
                self.word_bitmap_metrics[1] += 1;
                return Ok(true);
            }
            if self.word_queries.len() >= MAX_QUERY_CACHE {
                return Ok(false);
            }
            self.word_bitmap_metrics[2] += 1;
            let compiled =
                compile_target_query(key, |steps| self.word_query_charge(steps, control))?;
            let word_count = compiled.words().len();
            if !self.word_bitmap_room(word_count) {
                return Ok(false);
            }
            self.word_query_charge(word_count.saturating_add(1), control)?;
            let mut words = Vec::new();
            if words.try_reserve_exact(word_count).is_err() {
                return Ok(false);
            }
            words.extend_from_slice(compiled.words());
            let stored = StoredBitmap {
                counts: key.counts,
                words,
            };
            self.word_bitmap_staging_bytes = stored.retained_bytes();
            self.observe_word_bitmap_peak(0);
            self.word_query_charge(1, control)?;
            let old = self.word_queries.capacity()
                * (core::mem::size_of::<(TargetQueryKey, StoredBitmap)>() + 1);
            if self.word_queries.try_reserve(1).is_err() {
                self.word_bitmap_staging_bytes = 0;
                return Ok(false);
            }
            self.observe_word_bitmap_peak(
                if self.word_queries.capacity()
                    * (core::mem::size_of::<(TargetQueryKey, StoredBitmap)>() + 1)
                    != old
                {
                    old
                } else {
                    0
                },
            );
            self.word_bitmap_members += word_count * 2;
            self.word_bitmap_capacity_bytes += stored.retained_bytes();
            self.word_bitmap_drop_work += 1;
            self.word_bitmap_metrics[5] += word_count;
            self.word_queries.insert(key, stored);
            self.word_bitmap_staging_bytes = 0;
            self.observe_word_bitmap_peak(0);
            Ok(true)
        }

        fn ensure_board_word_bitmap(
            &mut self,
            key: BoardWordKey,
            remaining: u8,
            roots: &[u32],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            self.word_bitmap_board_metrics[0] += 1;
            let before = self.work;
            let result = self.ensure_board_word_bitmap_inner(key, remaining, roots, kinds, control);
            self.word_bitmap_board_metrics[5] += self.work - before;
            self.word_bitmap_staging_bytes = 0;
            if matches!(result, Ok(false)) {
                self.word_bitmap_board_metrics[4] += 1;
            }
            result
        }

        fn ensure_board_word_bitmap_inner(
            &mut self,
            key: BoardWordKey,
            remaining: u8,
            roots: &[u32],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            self.charge(1, control)?;
            if let Some(bitmap) = self.board_word_bitmaps.get(&key) {
                if bitmap.remaining != remaining || bitmap.root_count != roots.len() {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                self.word_bitmap_board_metrics[1] += 1;
                return Ok(true);
            }
            if !self.word_bitmap_metadata_room() {
                return Ok(false);
            }
            if multiset_size(key.counts) != Some(remaining) {
                return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
            }
            self.word_bitmap_board_metrics[2] += 1;
            let mut bitmap = RankedBits::new(key.counts)
                .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
            let mut unknown = false;
            // Complete the whole root validation before publishing even a
            // Full certificate. Different same-kind NFA children are preserved
            // by ensure_word_bitmap; unknown roots cannot prove a negative.
            for &root in roots {
                self.charge(1, control)?;
                Self::check_supply_root(root, remaining, kinds)?;
                let counts = match self.word_bitmaps.get(&root) {
                    Some(bitmap) => Some(bitmap.counts),
                    None => self.uniform_multiset(root, kinds, control)?,
                };
                let Some(counts) = counts else {
                    unknown = true;
                    continue;
                };
                if multiset_size(counts) != Some(remaining) {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                if counts != key.counts {
                    continue;
                }
                if !self.ensure_word_bitmap(root, counts, kinds, control)? {
                    unknown = true;
                    continue;
                }
                self.charge(
                    self.word_bitmaps[&root].words.len().saturating_add(1),
                    control,
                )?;
                bitmap.or_same_counts(counts, &self.word_bitmaps[&root].words, |_| Ok(()))?;
            }
            let coverage = bitmap.coverage(|steps| self.charge(steps, control))?;
            // Descendant materialization may have filled the optional budget.
            if !self.word_bitmap_metadata_room() {
                return Ok(false);
            }
            let payload = match coverage {
                BitmapCoverage::Empty => BoardWordPayload::Empty,
                BitmapCoverage::Full => BoardWordPayload::Full,
                BitmapCoverage::Partial => {
                    if !self.word_bitmap_room(bitmap.words().len()) {
                        return Ok(false);
                    }
                    self.charge(bitmap.words().len().saturating_add(1), control)?;
                    let mut words = Vec::new();
                    if words.try_reserve_exact(bitmap.words().len()).is_err() {
                        return Ok(false);
                    }
                    words.extend_from_slice(bitmap.words());
                    BoardWordPayload::Partial(words)
                }
            };
            let word_count = match &payload {
                BoardWordPayload::Partial(words) => words.len(),
                _ => 0,
            };
            self.word_bitmap_staging_bytes = payload.retained_bytes();
            self.observe_word_bitmap_peak(0);
            self.charge(1, control)?;
            let old = self.board_word_bitmaps.capacity()
                * (core::mem::size_of::<(BoardWordKey, BoardWordBitmap)>() + 1);
            if self.board_word_bitmaps.try_reserve(1).is_err() {
                return Ok(false);
            }
            self.observe_word_bitmap_peak(
                if self.board_word_bitmaps.capacity()
                    * (core::mem::size_of::<(BoardWordKey, BoardWordBitmap)>() + 1)
                    != old
                {
                    old
                } else {
                    0
                },
            );
            self.word_bitmap_members += word_count * 2;
            self.word_bitmap_capacity_bytes += payload.retained_bytes();
            self.word_bitmap_drop_work += 1;
            self.word_bitmap_metrics[5] += word_count;
            self.word_bitmap_board_metrics[3] += word_count;
            self.board_word_bitmaps.insert(
                key,
                BoardWordBitmap {
                    remaining,
                    root_count: roots.len(),
                    unknown,
                    payload,
                },
            );
            self.word_bitmap_staging_bytes = 0;
            self.observe_word_bitmap_peak(0);
            Ok(true)
        }

        /// `counts` is either the existing uniform certificate or the exact
        /// M-e_p inherited from one of its edges. No approximate multiset or
        /// single same-kind child is substituted for the NFA language.
        fn ensure_word_bitmap(
            &mut self,
            root: u32,
            counts: u32,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            self.charge(1, control)?;
            if let Some(bitmap) = self.word_bitmaps.get(&root) {
                if bitmap.counts != counts {
                    return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                }
                return Ok(true);
            }
            if !self.word_bitmap_metadata_room() {
                return Ok(false);
            }
            let node = kinds
                .node(root)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            let edge_count = kinds
                .node_edge_count(root)
                .ok_or_else(|| invalid("wasm_observation_language_edge_out_of_range"))?;
            if multiset_size(counts) != Some(node.remaining_depth)
                || node.accepting != (node.remaining_depth == 0)
                || (node.accepting && edge_count != 0)
                || (!node.accepting && edge_count == 0)
            {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            let mut bitmap = RankedBits::new(counts)
                .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
            if node.accepting {
                self.charge(1, control)?;
                bitmap.accept_empty()?;
            } else {
                for index in 0..edge_count {
                    self.charge(1, control)?;
                    let edge = kinds
                        .edge_at(root, index)
                        .ok_or_else(|| invalid("wasm_observation_language_edge_out_of_range"))?;
                    let child = kinds
                        .node(edge.child())
                        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                    if edge.child() >= root
                        || !(1..=7).contains(&edge.piece_code())
                        || child.remaining_depth.checked_add(1) != Some(node.remaining_depth)
                    {
                        return Err(invalid("wasm_observation_physical_depth_mismatch"));
                    }
                    let shift = usize::from(edge.piece_code() - 1) * 4;
                    if (counts >> shift) & 15 == 0 {
                        return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                    }
                    let child_counts = counts - (1 << shift);
                    if !self.ensure_word_bitmap(edge.child(), child_counts, kinds, control)? {
                        return Ok(false);
                    }
                    let word_work = bitmap
                        .child_work(edge.piece_code(), child_counts)
                        .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
                    self.charge(word_work, control)?;
                    bitmap.or_child(
                        edge.piece_code(),
                        child_counts,
                        &self.word_bitmaps[&edge.child()].words,
                        |_| Ok(()),
                    )?;
                }
            }
            // Descendant insertion may have filled either optional budget.
            // A partial cache never turns this unknown root into a reject.
            if !self.word_bitmap_metadata_room() || !self.word_bitmap_room(bitmap.words().len()) {
                return Ok(false);
            }
            self.charge(bitmap.words().len().saturating_add(1), control)?;
            let mut words = Vec::new();
            words
                .try_reserve_exact(bitmap.words().len())
                .map_err(|_| storage())?;
            words.extend_from_slice(bitmap.words());
            let stored = StoredBitmap { counts, words };
            self.word_bitmap_staging_bytes = stored.retained_bytes();
            self.observe_word_bitmap_peak(0);
            let old =
                self.word_bitmaps.capacity() * (core::mem::size_of::<(u32, StoredBitmap)>() + 1);
            self.word_bitmaps.try_reserve(1).map_err(|_| storage())?;
            self.observe_word_bitmap_peak(
                if self.word_bitmaps.capacity() * (core::mem::size_of::<(u32, StoredBitmap)>() + 1)
                    != old
                {
                    old
                } else {
                    0
                },
            );
            self.word_bitmap_members += stored.words.len() * 2;
            self.word_bitmap_capacity_bytes += stored.retained_bytes();
            self.word_bitmap_metrics[4] += 1;
            self.word_bitmap_metrics[5] += stored.words.len();
            self.word_bitmap_drop_work += 1;
            self.word_bitmaps.insert(root, stored);
            self.word_bitmap_staging_bytes = 0;
            self.observe_word_bitmap_peak(0);
            Ok(true)
        }

        fn evict_word_bitmaps(
            &mut self,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            let before = self.work;
            self.charge(self.word_bitmap_drop_work.saturating_add(4), control)?;
            self.word_bitmap_metrics[8] += self.work - before;
            self.word_bitmap_board_metrics[5] += self.board_word_bitmaps.len().saturating_add(1);
            self.word_bitmaps = HashMap::new();
            self.word_queries = HashMap::new();
            self.word_profiles = HashMap::new();
            self.board_word_bitmaps = HashMap::new();
            self.word_bitmap_members = 0;
            self.word_bitmap_capacity_bytes = 0;
            self.word_bitmap_drop_work = 0;
            self.word_bitmap_staging_bytes = 0;
            // Cumulative counters/work and the owner's peak remain intact.
            Ok(())
        }

        pub(in crate::backend::wasm_cpu) fn intern(
            &mut self,
            remaining: u8,
            roots: &[u32],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            let result = self
                .bind_kinds(kinds)
                .and_then(|()| self.intern_inner(remaining, roots, kinds, control));
            if result.is_err() {
                self.failed = true;
            }
            result
        }

        fn bind_kinds(
            &mut self,
            kinds: &PieceOrderLanguageCache,
        ) -> Result<(), WasmExactSearchError> {
            if let Some(identity) = &self.kind_arena {
                if !Arc::ptr_eq(identity, kinds.arena_identity()) {
                    return Err(invalid("wasm_observation_suffix_arena_changed"));
                }
            } else {
                self.kind_arena = Some(Arc::clone(kinds.arena_identity()));
            }
            Ok(())
        }

        /// The board identity belongs to one frozen view and fixes both the
        /// root list and remaining depth. Counts come only from actual observed
        /// relevant draws; future-only reveal uses the separate unfiltered path.
        /// None is an optional-cache/encoding fallback, never a reject language.
        pub(in crate::backend::wasm_cpu) fn intern_for_supply(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            available: [u8; 7],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<u32>, WasmExactSearchError> {
            let result = self.bind_kinds(kinds).and_then(|()| {
                self.intern_for_supply_inner(board, remaining, roots, available, kinds, control)
            });
            if result.is_err() {
                self.failed = true;
            }
            result
        }

        fn intern_for_supply_inner(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            available: [u8; 7],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<u32>, WasmExactSearchError> {
            // One bounded seven-kind context construction and alias lookup.
            self.charge(1, control)?;
            let Some(packed) = pack_multiset(available) else {
                return Ok(None);
            };
            let key = u64::from(board) | (u64::from(packed) << 32);
            if let Some(&reference) = self.supply_aliases.get(&key) {
                let depth = if reference & KIND_SOURCE_TAG != 0 {
                    kinds
                        .node(reference & !KIND_SOURCE_TAG)
                        .map(|node| node.remaining_depth)
                } else {
                    self.node(reference).map(|node| node.0)
                };
                if depth != Some(remaining) {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                return Ok(Some(reference));
            }
            // These optional annotation caches use the existing finite node
            // bound. Reaching it keeps the original language rather than
            // admitting an unbounded cache or treating unknown as impossible.
            if self.supply_aliases.len() >= self.limits.nodes {
                return Ok(None);
            }
            if roots.is_empty() {
                return Err(invalid("wasm_observation_kind_union_empty"));
            }
            if roots.len() > self.limits.members {
                return Err(invalid("wasm_observation_physical_union_limit"));
            }
            self.scratch.clear();
            if !self.multiset_grouping_enabled
                || !self.scratch_from_multiset_groups(
                    board, remaining, roots, packed, available, kinds, control,
                )?
            {
                for &root in roots {
                    self.charge(1, control)?;
                    self.supply_metrics[0] += 1;
                    Self::check_supply_root(root, remaining, kinds)?;
                    if self
                        .uniform_multiset(root, kinds, control)?
                        .is_some_and(|required| !multiset_fits(required, packed))
                    {
                        self.supply_metrics[1] += 1;
                        continue;
                    }
                    let old = self.scratch.capacity() * core::mem::size_of::<u32>();
                    self.scratch.try_reserve(1).map_err(|_| storage())?;
                    self.scratch.push(root);
                    self.observe_peak(
                        if self.scratch.capacity() * core::mem::size_of::<u32>() != old {
                            old
                        } else {
                            0
                        },
                    );
                }
            }
            // The filter already checked each root's owner/depth, so reuse
            // that validation instead of traversing the retained list again.
            // A fully pruned set is a normal empty language at this depth;
            // intern([]) still rejects missing unfiltered input separately.
            self.scratch.sort_unstable();
            self.scratch.dedup();
            let reference = self.intern_validated_scratch(remaining, control)?;
            let old = self.supply_aliases.capacity() * (core::mem::size_of::<(u64, u32)>() + 1);
            self.supply_aliases.try_reserve(1).map_err(|_| storage())?;
            self.supply_aliases.insert(key, reference);
            self.observe_peak(
                if self.supply_aliases.capacity() * (core::mem::size_of::<(u64, u32)>() + 1) != old
                {
                    old
                } else {
                    0
                },
            );
            Ok(Some(reference))
        }

        fn check_supply_root(
            root: u32,
            remaining: u8,
            kinds: &PieceOrderLanguageCache,
        ) -> Result<(), WasmExactSearchError> {
            let node = kinds
                .node(root)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            if root >= KIND_SOURCE_TAG - 1
                || node.remaining_depth != remaining
                || node.accepting != (remaining == 0)
            {
                return Err(invalid("wasm_observation_physical_depth_mismatch"));
            }
            Ok(())
        }

        /// At most r relevant draws plus one empty-hold draw or one held
        /// piece are available. Hence a fitting r-piece multiset is A itself
        /// or A minus exactly one kind; only seven exact keys are needed.
        /// Broader direct API inputs keep the ordinary scalar filter.
        #[allow(clippy::too_many_arguments)]
        fn scratch_from_multiset_groups(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            packed: u32,
            available: [u8; 7],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            let before = self.work;
            let result = self.scratch_from_multiset_groups_inner(
                board, remaining, roots, packed, available, kinds, control,
            );
            // Includes shared certificate work performed during construction,
            // even when allocation, cancellation, or the work bound fails.
            self.group_metrics[6] += self.work - before;
            self.group_staging_bytes = 0;
            result
        }

        #[allow(clippy::too_many_arguments)]
        fn scratch_from_multiset_groups_inner(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            packed: u32,
            available: [u8; 7],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<bool, WasmExactSearchError> {
            self.charge(1, control)?;
            let total: u16 = available.into_iter().map(u16::from).sum();
            if total > u16::from(remaining) + 1 {
                self.group_metrics[5] += 1;
                return Ok(false);
            }
            let mut keys = [0_u32; 7];
            let mut key_count = 0;
            if total == u16::from(remaining) {
                keys[0] = packed;
                key_count = 1;
            } else if total == u16::from(remaining) + 1 {
                for (kind, count) in available.into_iter().enumerate() {
                    if count != 0 {
                        keys[key_count] = packed - (1 << (kind * 4));
                        key_count += 1;
                    }
                }
            }

            let Some(index) =
                self.ensure_multiset_groups(board, remaining, roots, kinds, control)?
            else {
                return Ok(false);
            };

            for &key in &keys[..key_count] {
                self.charge(1, control)?;
                self.group_metrics[2] += 1;
                if let Some(&group) = self.multiset_groups[index].by_multiset.get(&key) {
                    self.append_grouped_members(index, Some(group.index()), control)?;
                }
            }
            self.append_grouped_members(index, None, control)?;
            Ok(true)
        }

        /// The caller charges the one lookup before entering this shared
        /// construction kernel. It never copies roots on an existing group.
        fn ensure_multiset_groups(
            &mut self,
            board: u32,
            remaining: u8,
            roots: &[u32],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<usize>, WasmExactSearchError> {
            let index = if let Some(&index) = self.multiset_group_index.get(&board) {
                let cached = &self.multiset_groups[index];
                if cached.remaining != remaining {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                if cached.root_count != roots.len() {
                    return Err(invalid("wasm_observation_board_supply_roots_changed"));
                }
                index
            } else {
                let admitted = self
                    .base
                    .members
                    .checked_add(self.members)
                    .and_then(|count| count.checked_add(self.group_members))
                    .and_then(|count| count.checked_add(self.word_bitmap_members))
                    .and_then(|count| count.checked_add(roots.len()))
                    .is_some_and(|count| count <= self.limits.members);
                if !admitted || self.multiset_groups.len() >= self.limits.nodes {
                    self.group_metrics[5] += 1;
                    return Ok(None);
                }
                let mut grouped = BoardMultisetGroups::new(remaining, roots.len());
                for &root in roots {
                    self.charge(1, control)?;
                    self.group_metrics[1] += 1;
                    Self::check_supply_root(root, remaining, kinds)?;
                    let certificate = self.uniform_multiset_certificate(root, kinds, control)?;
                    let counts = certificate.map(|value| value & MULTISET_COUNTS_MASK);
                    let full = if self.full_multiset_enabled && self.multiset_grouping_enabled {
                        self.full_multiset_charge(1, control)?;
                        self.full_multiset_metrics[1] += 1;
                        certificate.is_some_and(|value| value & FULL_MULTISET != 0)
                    } else {
                        false
                    };
                    self.charge(1, control)?;
                    self.push_grouped_root(&mut grouped, counts, root, full)?;
                }
                self.charge(1, control)?;
                let old =
                    self.multiset_groups.capacity() * core::mem::size_of::<BoardMultisetGroups>();
                self.multiset_groups.try_reserve(1).map_err(|_| storage())?;
                self.observe_peak(
                    if self.multiset_groups.capacity() * core::mem::size_of::<BoardMultisetGroups>()
                        != old
                    {
                        old
                    } else {
                        0
                    },
                );
                let old = self.multiset_group_index.capacity()
                    * (core::mem::size_of::<(u32, usize)>() + 1);
                self.multiset_group_index
                    .try_reserve(1)
                    .map_err(|_| storage())?;
                self.observe_peak(
                    if self.multiset_group_index.capacity()
                        * (core::mem::size_of::<(u32, usize)>() + 1)
                        != old
                    {
                        old
                    } else {
                        0
                    },
                );
                let index = self.multiset_groups.len();
                self.group_members += roots.len();
                self.group_capacity_bytes += grouped.retained_bytes();
                self.group_drop_work += grouped.drop_work();
                self.multiset_groups.push(grouped);
                self.multiset_group_index.insert(board, index);
                self.group_staging_bytes = 0;
                self.group_metrics[0] += 1;
                self.observe_peak(0);
                index
            };

            Ok(Some(index))
        }

        fn observe_group_staging(&mut self, grouped: &BoardMultisetGroups, extra: usize) {
            self.group_staging_bytes = grouped.retained_bytes();
            self.observe_peak(extra);
        }

        fn push_grouped_root(
            &mut self,
            grouped: &mut BoardMultisetGroups,
            counts: Option<u32>,
            root: u32,
            full: bool,
        ) -> Result<(), WasmExactSearchError> {
            let group = if let Some(counts) = counts {
                if let Some(index) = grouped.by_multiset.get_mut(&counts) {
                    if full {
                        index.mark_full();
                        grouped.has_full = true;
                    }
                    Some(index.index())
                } else {
                    let old = grouped.groups.capacity() * core::mem::size_of::<Vec<u32>>();
                    grouped.groups.try_reserve(1).map_err(|_| storage())?;
                    self.observe_group_staging(
                        grouped,
                        if grouped.groups.capacity() * core::mem::size_of::<Vec<u32>>() != old {
                            old
                        } else {
                            0
                        },
                    );
                    let old = grouped.by_multiset.capacity()
                        * (core::mem::size_of::<(u32, MultisetGroupIndex)>() + 1);
                    grouped.by_multiset.try_reserve(1).map_err(|_| storage())?;
                    self.observe_group_staging(
                        grouped,
                        if grouped.by_multiset.capacity()
                            * (core::mem::size_of::<(u32, MultisetGroupIndex)>() + 1)
                            != old
                        {
                            old
                        } else {
                            0
                        },
                    );
                    let index = grouped.groups.len();
                    grouped.groups.push(Vec::new());
                    let mut entry = MultisetGroupIndex::new(index);
                    if full {
                        entry.mark_full();
                        grouped.has_full = true;
                    }
                    grouped.by_multiset.insert(counts, entry);
                    Some(index)
                }
            } else {
                None
            };
            let roots = match group {
                Some(index) => &mut grouped.groups[index],
                None => &mut grouped.unknown,
            };
            let old = roots.capacity() * core::mem::size_of::<u32>();
            roots.try_reserve(1).map_err(|_| storage())?;
            let new = roots.capacity() * core::mem::size_of::<u32>();
            roots.push(root);
            grouped.member_capacity_bytes += new - old;
            self.observe_group_staging(grouped, if new != old { old } else { 0 });
            Ok(())
        }

        fn append_grouped_members(
            &mut self,
            board: usize,
            group: Option<usize>,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            let count = match group {
                Some(index) => self.multiset_groups[board].groups[index].len(),
                None => self.multiset_groups[board].unknown.len(),
            };
            self.charge(count, control)?;
            self.group_metrics[if group.is_some() { 3 } else { 4 }] += count;
            let old = self.scratch.capacity() * core::mem::size_of::<u32>();
            self.scratch.try_reserve(count).map_err(|_| storage())?;
            match group {
                Some(index) => self
                    .scratch
                    .extend_from_slice(&self.multiset_groups[board].groups[index]),
                None => self
                    .scratch
                    .extend_from_slice(&self.multiset_groups[board].unknown),
            }
            self.observe_peak(
                if self.scratch.capacity() * core::mem::size_of::<u32>() != old {
                    old
                } else {
                    0
                },
            );
            Ok(())
        }

        fn evict_multiset_groups(
            &mut self,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            let before = self.work;
            self.charge(self.group_drop_work, control)?;
            self.group_metrics[6] += self.work - before;
            self.group_metrics[7] += 1;
            // Release the optional capacities as well as their members. The
            // mandatory union and every cached supply alias remain unchanged.
            self.multiset_groups = Vec::new();
            self.multiset_group_index = HashMap::new();
            self.group_members = 0;
            self.group_capacity_bytes = 0;
            self.group_drop_work = 0;
            Ok(())
        }

        fn uniform_multiset(
            &mut self,
            root: u32,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<u32>, WasmExactSearchError> {
            self.uniform_multiset_certificate(root, kinds, control)
                .map(|certificate| certificate.map(|value| value & MULTISET_COUNTS_MASK))
        }

        /// Counts occupy exactly seven nibbles. The private spare flag means
        /// a sufficient Full proof, never that an unflagged language is not
        /// Full. Every NFA edge still participates in the uniform proof.
        ///
        /// For each kind p in M, one Full p-child witnesses every permutation
        /// of M-e_p. Their disjoint first-kind blocks cover Perm(M). Several
        /// non-Full same-kind children may jointly cover a block; this cheap
        /// test deliberately leaves that case unproved for the existing DP.
        fn uniform_multiset_certificate(
            &mut self,
            root: u32,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<Option<u32>, WasmExactSearchError> {
            self.charge(1, control)?;
            if let Some(&certificate) = self.multisets.get(&root) {
                return Ok((certificate != NO_MULTISET).then_some(certificate));
            }
            if self.multisets.len() >= self.limits.nodes {
                return Ok(None);
            }
            self.supply_metrics[2] += 1;
            let prove_full = self.full_multiset_enabled && self.multiset_grouping_enabled;
            if prove_full {
                self.full_multiset_metrics[2] += 1;
            }
            let node = kinds
                .node(root)
                .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
            let edges = kinds
                .node_edge_count(root)
                .ok_or_else(|| invalid("wasm_observation_language_edge_out_of_range"))?;
            let certificate = if node.accepting {
                if node.remaining_depth != 0 || edges != 0 {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                if prove_full {
                    self.full_multiset_charge(1, control)?;
                    Some(FULL_MULTISET)
                } else {
                    Some(0)
                }
            } else {
                if node.remaining_depth == 0 || edges == 0 {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
                let mut common = None;
                let mut full_first_kinds = 0_u32;
                for index in 0..edges {
                    self.charge(1, control)?;
                    self.supply_metrics[3] += 1;
                    let edge = kinds
                        .edge_at(root, index)
                        .ok_or_else(|| invalid("wasm_observation_language_edge_out_of_range"))?;
                    let child = kinds
                        .node(edge.child())
                        .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                    if edge.child() >= root
                        || child.remaining_depth.checked_add(1) != Some(node.remaining_depth)
                        || !(1..=7).contains(&edge.piece_code())
                    {
                        return Err(invalid("wasm_observation_physical_depth_mismatch"));
                    }
                    let Some(child_certificate) =
                        self.uniform_multiset_certificate(edge.child(), kinds, control)?
                    else {
                        common = None;
                        break;
                    };
                    let counts = child_certificate & MULTISET_COUNTS_MASK;
                    let shift = usize::from(edge.piece_code() - 1) * 4;
                    if (counts >> shift) & 15 == 15 {
                        common = None;
                        break;
                    }
                    let counts = counts + (1 << shift);
                    if common.is_some_and(|expected| expected != counts) {
                        common = None;
                        break;
                    }
                    common = Some(counts);
                    if prove_full {
                        // One additional flag inspection/word-mask update in
                        // the existing child loop, with no second edge pass.
                        self.full_multiset_charge(1, control)?;
                        if child_certificate & FULL_MULTISET != 0 {
                            full_first_kinds |= 1 << shift;
                        }
                    }
                }
                if let Some(counts) = common {
                    if prove_full {
                        // Nibble witnesses are 0/1, so multiplying by 15
                        // expands them to disjoint 0/F fields without carry.
                        // Charge both expansion and required-kind containment.
                        self.full_multiset_charge(2, control)?;
                        if counts & !(full_first_kinds * 15) == 0 {
                            Some(counts | FULL_MULTISET)
                        } else {
                            Some(counts)
                        }
                    } else {
                        Some(counts)
                    }
                } else {
                    None
                }
            };
            if prove_full && certificate.is_some_and(|value| value & FULL_MULTISET != 0) {
                self.full_multiset_metrics[3] += 1;
            }
            // Children can fill the cache while this frame is being proved.
            // Not retaining this optional result never licenses a prune.
            if self.multisets.len() < self.limits.nodes {
                let old = self.multisets.capacity() * (core::mem::size_of::<(u32, u32)>() + 1);
                self.multisets.try_reserve(1).map_err(|_| storage())?;
                self.multisets
                    .insert(root, certificate.unwrap_or(NO_MULTISET));
                self.observe_peak(
                    if self.multisets.capacity() * (core::mem::size_of::<(u32, u32)>() + 1) != old {
                        old
                    } else {
                        0
                    },
                );
            }
            Ok(certificate)
        }

        fn intern_inner(
            &mut self,
            remaining: u8,
            roots: &[u32],
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            self.charge(roots.len().saturating_add(1), control)?;
            if roots.len() > self.limits.members {
                return Err(invalid("wasm_observation_physical_union_limit"));
            }
            self.scratch.clear();
            let old = self.scratch.capacity() * core::mem::size_of::<u32>();
            self.scratch
                .try_reserve(roots.len())
                .map_err(|_| storage())?;
            self.scratch.extend_from_slice(roots);
            self.observe_peak(
                if self.scratch.capacity() * core::mem::size_of::<u32>() != old {
                    old
                } else {
                    0
                },
            );
            self.intern_scratch(remaining, kinds, control)
        }

        fn intern_scratch(
            &mut self,
            remaining: u8,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            if self.scratch.is_empty() {
                return Err(invalid("wasm_observation_kind_union_empty"));
            }
            self.scratch.sort_unstable();
            self.scratch.dedup();
            for (index, &root) in self.scratch.iter().enumerate() {
                if index & 255 == 0 && control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                let node = kinds
                    .node(root)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                if root >= KIND_SOURCE_TAG - 1
                    || node.remaining_depth != remaining
                    || node.accepting != (remaining == 0)
                {
                    return Err(invalid("wasm_observation_physical_depth_mismatch"));
                }
            }
            self.intern_validated_scratch(remaining, control)
        }

        fn intern_validated_scratch(
            &mut self,
            remaining: u8,
            control: &ExecutionControl,
        ) -> Result<u32, WasmExactSearchError> {
            if self.scratch.len() == 1 {
                return Ok(self.scratch[0] | KIND_SOURCE_TAG);
            }
            let mut hasher = DefaultHasher::new();
            remaining.hash(&mut hasher);
            self.scratch.hash(&mut hasher);
            let hash = hasher.finish();
            let head = self.heads.get(&hash).copied().unwrap_or(NO_NODE);
            let mut existing = head;
            while existing != NO_NODE {
                self.charge(1, control)?;
                let node = &self.nodes[existing as usize];
                if node.remaining == remaining && node.sources == self.scratch {
                    return Ok(existing);
                }
                existing = node.next_same_hash;
            }
            if self.base.nodes.saturating_add(self.nodes.len()) >= self.limits.nodes {
                return Err(invalid("wasm_observation_physical_node_limit"));
            }
            let required_members = self
                .base
                .members
                .checked_add(self.members)
                .and_then(|n| n.checked_add(self.scratch.len()))
                .filter(|&n| n <= self.limits.members)
                .ok_or_else(|| invalid("wasm_observation_physical_union_limit"))?;
            if required_members
                .checked_add(self.group_members)
                .and_then(|count| count.checked_add(self.word_bitmap_members))
                .is_none_or(|count| count > self.limits.members)
                && self.word_bitmap_members != 0
            {
                // An optional membership cache must not make an otherwise
                // admissible original union fail. Release its actual owners
                // before applying the existing optional-group eviction rule.
                self.evict_word_bitmaps(control)?;
            }
            if required_members
                .checked_add(self.group_members)
                .is_none_or(|count| count > self.limits.members)
            {
                // Optional indexing must not consume the space needed by an
                // otherwise admissible exact union. Keep its charged work.
                self.evict_multiset_groups(control)?;
            }
            let mut sources = Vec::new();
            sources
                .try_reserve_exact(self.scratch.len())
                .map_err(|_| storage())?;
            sources.extend_from_slice(&self.scratch);
            let source_bytes = sources.capacity() * core::mem::size_of::<u32>();
            self.observe_peak(source_bytes);
            let old = self.nodes.capacity() * core::mem::size_of::<UnionNode>();
            self.nodes.try_reserve(1).map_err(|_| storage())?;
            self.observe_peak(
                source_bytes
                    + if self.nodes.capacity() * core::mem::size_of::<UnionNode>() != old {
                        old
                    } else {
                        0
                    },
            );
            let old = self.heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1);
            self.heads.try_reserve(1).map_err(|_| storage())?;
            self.observe_peak(
                source_bytes
                    + if self.heads.capacity() * (core::mem::size_of::<(u64, u32)>() + 1) != old {
                        old
                    } else {
                        0
                    },
            );
            let node = self.nodes.len() as u32;
            self.members += sources.len();
            self.member_capacity_bytes += source_bytes;
            self.nodes.push(UnionNode {
                sources,
                edges: Vec::new(),
                remaining,
                prepared: 0,
                next_same_hash: head,
            });
            self.heads.insert(hash, node);
            self.observe_peak(0);
            Ok(node)
        }

        pub(in crate::backend::wasm_cpu) fn node(&self, node: u32) -> Option<(u8, bool)> {
            let node = self.nodes.get(node as usize)?;
            Some((
                node.remaining,
                node.remaining == 0 && !node.sources.is_empty(),
            ))
        }

        pub(in crate::backend::wasm_cpu) fn edge_count(&self, node: u32) -> Option<usize> {
            self.nodes.get(node as usize).map(|node| node.edges.len())
        }

        pub(in crate::backend::wasm_cpu) fn edge(
            &self,
            node: u32,
            index: usize,
        ) -> Option<(u8, u32)> {
            self.nodes.get(node as usize)?.edges.get(index).copied()
        }

        pub(in crate::backend::wasm_cpu) fn prepare_edges(
            &mut self,
            node: u32,
            piece_mask: u8,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            let result = self
                .bind_kinds(kinds)
                .and_then(|()| self.prepare_inner(node, piece_mask, kinds, control));
            if result.is_err() {
                self.failed = true;
            }
            result
        }

        fn prepare_inner(
            &mut self,
            node: u32,
            piece_mask: u8,
            kinds: &PieceOrderLanguageCache,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            self.charge(0, control)?;
            if piece_mask & !0x7f != 0 {
                return Err(invalid("wasm_observation_physical_piece_mask_invalid"));
            }
            let target = self
                .nodes
                .get(node as usize)
                .ok_or_else(|| invalid("wasm_observation_kind_union_node_invalid"))?;
            let remaining = target.remaining;
            if remaining == 0 || target.sources.is_empty() {
                return Ok(());
            }
            let mut pending = piece_mask & !target.prepared;
            while pending != 0 {
                let index = pending.trailing_zeros() as u8;
                let bit = 1_u8 << index;
                pending &= !bit;
                let piece = index + 1;
                self.scratch.clear();
                self.charge(1, control)?;
                let sources = self.nodes[node as usize].sources.len();
                for index in 0..sources {
                    self.charge(1, control)?;
                    self.source_visits += 1;
                    let root = self.nodes[node as usize].sources[index];
                    let old = self.scratch.capacity() * core::mem::size_of::<u32>();
                    let children = append_kind_children_bounded(
                        kinds,
                        root,
                        piece,
                        &mut self.scratch,
                        self.limits.members,
                    )?;
                    self.observe_peak(
                        if self.scratch.capacity() * core::mem::size_of::<u32>() != old {
                            old
                        } else {
                            0
                        },
                    );
                    self.charge(children, control)?;
                }
                if !self.scratch.is_empty() {
                    let child = self.intern_scratch(remaining - 1, kinds, control)?;
                    if self.base.edges.saturating_add(self.edges) >= self.limits.edges {
                        return Err(invalid("wasm_observation_physical_edge_limit"));
                    }
                    let edges = &mut self.nodes[node as usize].edges;
                    let old = edges.capacity() * core::mem::size_of::<(u8, u32)>();
                    edges.try_reserve(1).map_err(|_| storage())?;
                    let new = edges.capacity() * core::mem::size_of::<(u8, u32)>();
                    edges.push((piece, child));
                    self.edges += 1;
                    self.edge_capacity_bytes += new - old;
                    self.observe_peak(if new != old { old } else { 0 });
                }
                self.nodes[node as usize].prepared |= bit;
            }
            Ok(())
        }

        fn charge(
            &mut self,
            steps: usize,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if self.failed {
                return Err(invalid("wasm_observation_kind_union_failed"));
            }
            let Some(work) = self.work.checked_add(steps).filter(|&work| {
                self.base
                    .work
                    .checked_add(work)
                    .is_some_and(|total| total <= self.limits.work)
            }) else {
                #[cfg(feature = "local-search-ab")]
                eprintln!("clearra_observation_kind_union_work_limit base_work={} metrics={:?} supply_multiset={:?} attempted_steps={steps} retained_bytes={} peak_bytes={}", self.base.work, self.metrics(), self.supply_multiset_metrics(), self.retained_bytes(), self.peak_bytes);
                return Err(invalid("wasm_observation_physical_work_limit"));
            };
            self.work = work;
            Ok(())
        }
    }

    fn invalid(code: &'static str) -> WasmExactSearchError {
        WasmExactSearchError::InvalidProblem(code)
    }
    fn storage() -> WasmExactSearchError {
        invalid("wasm_observation_language_union_storage_unavailable")
    }

    fn pack_multiset(counts: [u8; 7]) -> Option<u32> {
        counts
            .into_iter()
            .enumerate()
            .try_fold(0, |packed, (index, count)| {
                (count <= 15).then_some(packed | (u32::from(count) << (index * 4)))
            })
    }

    fn multiset_fits(required: u32, available: u32) -> bool {
        (0..7).all(|index| ((required >> (index * 4)) & 15) <= ((available >> (index * 4)) & 15))
    }
}

#[cfg(feature = "placement-dependency-pc")]
pub(in crate::backend::wasm_cpu) use lazy::{KindUnionArena, KindUnionResources, KIND_SOURCE_TAG};

#[cfg(all(test, feature = "placement-dependency-pc"))]
mod tests {
    use std::collections::BTreeSet;

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
    };

    use crate::backend::wasm_cpu::{
        queue_observation_policy::{ObservationPieceLanguage, RootedPieceLanguageUnion},
        symbolic_prefix_domain::MAX_PREFIX_WORDS,
    };

    use super::*;

    fn relation(
        kinds: &mut PieceOrderLanguageCache,
        pieces: &[PieceKind],
        paths: &[&[usize]],
    ) -> u32 {
        let mut parents = vec![[0; MAX_PREFIX_WORDS]; pieces.len()];
        let mut active = [0; MAX_PREFIX_WORDS];
        for path in paths {
            let mut prefix = 0;
            active[0] |= 1;
            for &operation in *path {
                parents[operation][prefix / 64] |= 1_u64 << (prefix % 64);
                prefix |= 1 << operation;
                active[prefix / 64] |= 1_u64 << (prefix % 64);
            }
        }
        kinds
            .compile_product_closed_prefix_language(pieces, &parents, &active, &mut 0)
            .unwrap()
            .unwrap()
    }

    fn fixture(kinds: &mut PieceOrderLanguageCache) -> [u32; 2] {
        use PieceKind::{I, O, T};
        [
            relation(kinds, &[I, I, O], &[&[0, 2, 1], &[1, 0, 2], &[1, 2, 0]]),
            relation(kinds, &[T, O, I], &[&[0, 1, 2]]),
        ]
    }

    fn resources() -> KindUnionResources {
        KindUnionResources {
            nodes: 256,
            edges: 256,
            members: 256,
            work: 10_000,
        }
    }

    fn known_supply(queue: &[u8], hold_code: u8, hold_enabled: bool) -> KnownSupplyQuery {
        let mut query = KnownSupplyQuery {
            queue: [0; 7],
            len: queue.len() as u8,
            hold_code,
            hold_enabled,
        };
        query.queue[..queue.len()].copy_from_slice(queue);
        query
    }

    fn reference_known_membership(
        kinds: &PieceOrderLanguageCache,
        roots: &[u32],
        query: KnownSupplyQuery,
    ) -> bool {
        use crate::backend::wasm_cpu::queue_observation_policy::{
            piece_from_code, QueueObservationPolicyEvaluator,
        };
        use clearra_core_domain::probability::probability_value::ProbabilityValue;
        use clearra_coverage::universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        };
        use clearra_supply::{
            pattern_universe::MaterializedPatternUniverse, QueueObservationPolicy,
        };

        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(7_001),
            PatternWeightModelId::new(7_002),
            vec![query.queue[..usize::from(query.len)]
                .iter()
                .copied()
                .map(piece_from_code)
                .collect()],
            vec![ProbabilityValue::new(1.0).unwrap()],
            1,
            true,
            None,
        )
        .unwrap();
        let language = RootedPieceLanguageUnion::new(kinds, roots).unwrap();
        QueueObservationPolicyEvaluator::for_original_universe(
            &universe,
            QueueObservationPolicy::VisibleSeven,
            0,
            (query.hold_code != 0).then(|| piece_from_code(query.hold_code)),
            query.hold_enabled,
            false,
            false,
            None,
        )
        .unwrap()
        .evaluate(&language, &ExecutionControl::default())
        .unwrap()
        .covered_pattern_count
            == 1
    }

    mod full_multiset_tests {
        use crate::backend::wasm_cpu::{
            buildup::{BuildOrderGraph, BuildOrderNodeSpec},
            queue_observation_policy::piece_code,
        };
        use clearra_core_domain::piece::rotation::RotationState;

        use super::*;

        fn enabled() -> KindUnionArena {
            let mut result = arena();
            result.set_multiset_grouping_enabled(true);
            result.set_full_multiset_enabled(true);
            result
        }

        fn permutations(size: usize) -> Vec<Vec<usize>> {
            fn visit(prefix: &mut Vec<usize>, size: usize, output: &mut Vec<Vec<usize>>) {
                if prefix.len() == size {
                    output.push(prefix.clone());
                    return;
                }
                for operation in 0..size {
                    if !prefix.contains(&operation) {
                        prefix.push(operation);
                        visit(prefix, size, output);
                        prefix.pop();
                    }
                }
            }
            let mut output = Vec::new();
            visit(&mut Vec::new(), size, &mut output);
            output
        }

        /// Separate chains preserve several NFA children for one first kind.
        fn independent_words(kinds: &mut PieceOrderLanguageCache, words: &[&[PieceKind]]) -> u32 {
            let depth = words[0].len();
            assert!(depth > 0 && words.iter().all(|word| word.len() == depth));
            let mut edges = words
                .iter()
                .enumerate()
                .map(|(index, word)| {
                    BuildOrderGraph::edge(
                        (1 + index * depth) as u32,
                        0,
                        word[0],
                        RotationState::Zero,
                        0,
                        0,
                        0,
                    )
                })
                .collect::<Vec<_>>();
            let mut nodes = vec![BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: words.len() as u32,
                depth: 0,
                accepting: false,
            }];
            for word in words {
                for position in 1..=depth {
                    let accepting = position == depth;
                    nodes.push(BuildOrderNodeSpec {
                        edge_start: edges.len() as u32,
                        edge_count: u32::from(!accepting),
                        depth: position as u8,
                        accepting,
                    });
                    if !accepting {
                        edges.push(BuildOrderGraph::edge(
                            nodes.len() as u32,
                            0,
                            word[position],
                            RotationState::Zero,
                            0,
                            0,
                            0,
                        ));
                    }
                }
            }
            let graph = BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, false).unwrap();
            kinds.canonicalize(&graph).unwrap()
        }

        #[test]
        fn full_multiset_certifies_every_small_multiset_permutation_without_bitmaps() {
            use PieceKind::{I, L, O, T};
            let control = ExecutionControl::default();
            for pieces in [
                vec![I],
                vec![I, I],
                vec![I, O, T],
                vec![I, I, O, T],
                vec![I, I, O, O, T],
                vec![I, I, O, T, T, L],
            ] {
                let paths = permutations(pieces.len());
                let paths = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
                let mut kinds = PieceOrderLanguageCache::default();
                let root = relation(&mut kinds, &pieces, &paths);
                let expected: BTreeSet<Vec<u8>> = paths
                    .iter()
                    .map(|path| {
                        path.iter()
                            .map(|&index| piece_code(pieces[index]))
                            .collect()
                    })
                    .collect();
                let mut full = enabled();
                assert_eq!(
                    lazy_words(&mut full, &kinds, root | KIND_SOURCE_TAG),
                    expected
                );
                for word in &expected {
                    assert_eq!(
                        full.full_multiset_membership(
                            1,
                            pieces.len() as u8,
                            &[root],
                            &known_supply(word, 0, false),
                            &kinds,
                            &control,
                        )
                        .unwrap(),
                        Some(true)
                    );
                }
                let metrics = full.full_multiset_metrics();
                assert_eq!(metrics[0], expected.len());
                assert_eq!(metrics[1], 1, "the frozen root list is read once");
                assert_eq!(metrics[4], expected.len());
                assert!(metrics[3] > 0 && metrics[3] <= metrics[2]);
                assert!(metrics[5] > 0 && metrics[5] <= full.metrics()[4]);
                assert_eq!(full.word_bitmap_metrics(), [0; 9]);
                assert_eq!(full.word_bitmap_retained_bytes(), 0);
                assert!(full.peak_bytes() >= full.retained_bytes());
            }
        }

        #[test]
        fn full_multiset_missing_first_kind_and_nfa_union_only_full_keep_the_old_dp() {
            use PieceKind::{I, O, T};
            let control = ExecutionControl::default();
            let mut kinds = PieceOrderLanguageCache::default();
            let fixed = relation(&mut kinds, &[I, O], &[&[0, 1]]);
            let mut full = enabled();
            let query = known_supply(&[1, 2], 0, false);
            assert_eq!(
                full.full_multiset_membership(1, 2, &[fixed], &query, &kinds, &control)
                    .unwrap(),
                None,
                "an I-first witness cannot prove the required O-first block"
            );
            assert!(reference_known_membership(&kinds, &[fixed], query));

            let nfa = independent_words(
                &mut kinds,
                &[
                    &[I, O, T],
                    &[I, T, O],
                    &[O, I, T],
                    &[O, T, I],
                    &[T, I, O],
                    &[T, O, I],
                ],
            );
            assert_eq!(kinds.edges_for_piece(nfa, 1).unwrap().len(), 2);
            let expected = BTreeSet::from([
                vec![1, 2, 3],
                vec![1, 3, 2],
                vec![2, 1, 3],
                vec![2, 3, 1],
                vec![3, 1, 2],
                vec![3, 2, 1],
            ]);
            assert_eq!(
                lazy_words(&mut full, &kinds, nfa | KIND_SOURCE_TAG),
                expected
            );
            for word in &expected {
                let query = known_supply(word, 0, false);
                assert_eq!(
                    full.full_multiset_membership(2, 3, &[nfa], &query, &kinds, &control)
                        .unwrap(),
                    None,
                    "same-kind non-Full children can jointly cover all permutations"
                );
                assert!(reference_known_membership(&kinds, &[nfa], query));
            }
            // The ordinary exact bitmap may prove what the sufficient test
            // leaves open. Private Full bits must not leak into its M key.
            assert_eq!(
                full.word_bitmap_membership(
                    3,
                    &[nfa],
                    &known_supply(&[3, 2, 1], 0, false),
                    &kinds,
                    &control,
                )
                .unwrap(),
                Some(true)
            );
        }

        #[test]
        fn full_multiset_known_hold_eof_and_group_reuse_preserve_exact_counts() {
            use PieceKind::{I, O};
            let control = ExecutionControl::default();
            let mut kinds = PieceOrderLanguageCache::default();
            let roots = [
                relation(&mut kinds, &[I, O], &[&[0, 1]]),
                relation(&mut kinds, &[I, O], &[&[0, 1], &[1, 0]]),
            ];
            let mut full = enabled();
            for query in [
                known_supply(&[2, 1], 0, false),
                known_supply(&[2, 1], 3, true),
                known_supply(&[2, 3], 1, true),
                known_supply(&[2, 2], 1, true),
                known_supply(&[3, 1, 2], 0, true),
            ] {
                assert!(reference_known_membership(&kinds, &roots, query));
                assert_eq!(
                    full.full_multiset_membership(7, 2, &roots, &query, &kinds, &control)
                        .unwrap(),
                    Some(true)
                );
            }
            let before = full.full_multiset_metrics();
            let groups = full.multiset_group_metrics();
            for query in [
                known_supply(&[2], 1, true),        // No EOF held-only release.
                known_supply(&[2, 2], 0, false),    // Available M does not fit.
                known_supply(&[1, 2], 1, false),    // Forbidden Hold keeps old path.
                known_supply(&[3, 1, 2], 0, false), // Broader than strict query.
            ] {
                assert_eq!(
                    full.full_multiset_membership(7, 2, &roots, &query, &kinds, &control)
                        .unwrap(),
                    None
                );
            }
            let filtered = full
                .intern_for_supply(7, 2, &roots, [1, 1, 0, 0, 0, 0, 0], &kinds, &control)
                .unwrap()
                .unwrap();
            assert_eq!(
                lazy_words(&mut full, &kinds, filtered),
                BTreeSet::from([vec![1, 2], vec![2, 1]])
            );
            assert_eq!(full.multiset_group_metrics()[0], groups[0]);
            assert_eq!(full.multiset_group_metrics()[1], groups[1]);
            assert_eq!(full.full_multiset_metrics()[1..4], before[1..4]);
            assert_eq!(before[1], roots.len());
            assert_eq!(full.word_bitmap_metrics(), [0; 9]);
        }

        #[test]
        fn full_multiset_unknown_roots_allow_positive_witness_but_not_a_false_certificate() {
            use PieceKind::{I, O, T};
            let control = ExecutionControl::default();
            let mut kinds = PieceOrderLanguageCache::default();
            let mixed = independent_words(&mut kinds, &[&[I, O], &[O, I], &[I, T]]);
            let full_root = relation(&mut kinds, &[I, O], &[&[0, 1], &[1, 0]]);
            let query = known_supply(&[2, 1], 0, false);
            let mut full = enabled();
            assert_eq!(
                full.full_multiset_membership(1, 2, &[mixed], &query, &kinds, &control)
                    .unwrap(),
                None,
                "a nonuniform NFA cannot acquire a uniform Full certificate"
            );
            assert_eq!(
                full.full_multiset_membership(2, 2, &[mixed, full_root], &query, &kinds, &control)
                    .unwrap(),
                Some(true)
            );
            let mut malformed = enabled();
            assert!(matches!(
                malformed.full_multiset_membership(
                    1,
                    2,
                    &[full_root, u32::MAX],
                    &query,
                    &kinds,
                    &control,
                ),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_suffix_language_missing"
                ))
            ));
            assert_eq!(malformed.full_multiset_metrics()[4], 0);
        }

        #[test]
        fn full_multiset_optional_bounds_and_disabled_modes_preserve_the_language() {
            use PieceKind::{I, O};
            let control = ExecutionControl::default();
            let mut kinds = PieceOrderLanguageCache::default();
            let root = relation(&mut kinds, &[I, O], &[&[0, 1], &[1, 0]]);
            let query = known_supply(&[2, 1], 0, false);
            for (full_enabled, groups_enabled) in [(false, true), (true, false)] {
                let mut disabled = arena();
                disabled.set_full_multiset_enabled(full_enabled);
                disabled.set_multiset_grouping_enabled(groups_enabled);
                assert_eq!(
                    disabled
                        .full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                        .unwrap(),
                    None
                );
                assert_eq!(disabled.full_multiset_metrics(), [0; 6]);
            }
            let mut bounded = KindUnionArena::new(
                KindUnionResources {
                    nodes: 0,
                    edges: 0,
                    members: 1,
                    work: 0,
                },
                KindUnionResources {
                    members: 1,
                    ..resources()
                },
            )
            .unwrap();
            bounded.set_multiset_grouping_enabled(true);
            bounded.set_full_multiset_enabled(true);
            assert_eq!(
                bounded
                    .full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                    .unwrap(),
                None,
                "an optional group cannot exceed the existing shared member cap"
            );
            assert_eq!(bounded.multiset_group_metrics()[0], 0);
            assert_eq!(bounded.full_multiset_metrics()[1], 0);
            let filtered = bounded
                .intern_for_supply(1, 2, &[root], [1, 1, 0, 0, 0, 0, 0], &kinds, &control)
                .unwrap()
                .unwrap();
            assert_eq!(filtered, root | KIND_SOURCE_TAG);
            assert_eq!(
                lazy_words(&mut bounded, &kinds, filtered),
                BTreeSet::from([vec![1, 2], vec![2, 1]])
            );
            assert!(bounded.peak_bytes() >= bounded.retained_bytes());
            assert!(bounded.full_multiset_metrics()[5] <= bounded.metrics()[4]);
        }

        #[test]
        fn full_multiset_work_owner_hot_validation_and_cancellation_fail_closed() {
            use PieceKind::{I, O};
            let control = ExecutionControl::default();
            let mut kinds = PieceOrderLanguageCache::default();
            let root = relation(&mut kinds, &[I, O], &[&[0, 1], &[1, 0]]);
            let query = known_supply(&[2, 1], 0, false);
            let mut limited = KindUnionArena::new(
                KindUnionResources {
                    nodes: 0,
                    edges: 0,
                    members: 0,
                    work: 9,
                },
                KindUnionResources {
                    work: 10,
                    ..resources()
                },
            )
            .unwrap();
            limited.set_multiset_grouping_enabled(true);
            limited.set_full_multiset_enabled(true);
            assert!(matches!(
                limited.full_multiset_membership(1, 2, &[root], &query, &kinds, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_physical_work_limit"
                ))
            ));
            assert_eq!(limited.metrics()[4], 1);
            assert_eq!(limited.full_multiset_metrics()[5], 1);
            assert!(matches!(
                limited.full_multiset_membership(1, 2, &[root], &query, &kinds, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_kind_union_failed"
                ))
            ));

            let mut hot = enabled();
            assert_eq!(
                hot.full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                    .unwrap(),
                Some(true)
            );
            let before = hot.metrics();
            let cancelled = ExecutionControl::default();
            cancelled.cancellation.handle().cancel();
            assert!(matches!(
                hot.full_multiset_membership(1, 2, &[root], &query, &kinds, &cancelled),
                Err(WasmExactSearchError::Cancelled)
            ));
            assert_eq!(hot.metrics(), before);

            let mut wrong_owner = enabled();
            wrong_owner
                .full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                .unwrap();
            let mut other = PieceOrderLanguageCache::default();
            let other_root = relation(&mut other, &[I, O], &[&[0, 1], &[1, 0]]);
            assert!(matches!(
                wrong_owner.full_multiset_membership(1, 2, &[other_root], &query, &other, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_suffix_arena_changed"
                ))
            ));

            let mut invalid_query = enabled();
            invalid_query
                .full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                .unwrap();
            assert!(matches!(
                invalid_query.full_multiset_membership(
                    1,
                    2,
                    &[root],
                    &known_supply(&[0, 2], 0, false),
                    &kinds,
                    &control
                ),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_known_supply_invalid"
                ))
            ));
            let mut changed = enabled();
            changed
                .full_multiset_membership(1, 2, &[root], &query, &kinds, &control)
                .unwrap();
            assert!(matches!(
                changed.full_multiset_membership(1, 2, &[root, root], &query, &kinds, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_board_supply_roots_changed"
                ))
            ));
        }
    }

    #[test]
    fn word_bitmap_membership_matches_existing_known_supply_dp_and_reuses_queries() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let mut bitmap = arena();
        let cases = [
            known_supply(&[1, 2, 1], 0, true),
            known_supply(&[1, 1, 2], 0, false),
            known_supply(&[3, 2, 1], 0, false),
            known_supply(&[2, 2, 2], 0, false),
            known_supply(&[1, 2, 1, 3], 0, true),
            known_supply(&[1, 2, 3], 1, true),
            known_supply(&[1, 2], 1, true),
            known_supply(&[2, 1, 1], 3, true),
        ];
        for query in cases {
            let expected = reference_known_membership(&kinds, &roots, query);
            assert_eq!(
                bitmap
                    .word_bitmap_membership(3, &roots, &query, &kinds, &control)
                    .unwrap(),
                Some(expected),
                "query={query:?}",
            );
        }
        let before = bitmap.word_bitmap_metrics();
        let work = bitmap.metrics()[4];
        let expected = reference_known_membership(&kinds, &roots, cases[0]);
        assert_eq!(
            bitmap
                .word_bitmap_membership(3, &roots, &cases[0], &kinds, &control)
                .unwrap(),
            Some(expected),
        );
        let after = bitmap.word_bitmap_metrics();
        assert_eq!(after[1], before[1] + 1);
        assert_eq!(after[2], before[2]);
        // Hot target reuse still pays the actual raw/profile/target lookups;
        // it performs no source recount, rename, automaton walk, or payload copy.
        assert!(after[3] > before[3]);
        assert_eq!(after[4], before[4]);
        assert_eq!(after[5], before[5]);
        assert!(after[6] > before[6]);
        assert_eq!(after[8] - before[8], bitmap.metrics()[4] - work);
        assert!(bitmap.word_bitmap_retained_bytes() > 0);
        assert!(bitmap.word_bitmap_peak_bytes() >= bitmap.word_bitmap_retained_bytes());
        assert!(bitmap.peak_bytes() >= bitmap.retained_bytes());
    }

    #[test]
    fn word_bitmap_board_union_matches_rootwise_dp_and_reuses_immutable_unsorted_roots() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let [first, second] = fixture(&mut kinds);
        let roots = [second, first]; // Frozen root sets need not be sorted.
        let mut board = arena();
        let mut rootwise = arena();
        for query in [
            known_supply(&[1, 2, 1], 0, true),
            known_supply(&[1, 1, 2], 0, false),
            known_supply(&[3, 2, 1], 0, false),
            known_supply(&[2, 2, 2], 0, false),
            known_supply(&[1, 2, 1, 3], 0, true),
            known_supply(&[1, 2, 3], 1, true),
            known_supply(&[1, 2], 1, true),
            known_supply(&[2, 1, 1], 3, true),
        ] {
            let expected = Some(reference_known_membership(&kinds, &roots, query));
            assert_eq!(
                rootwise
                    .word_bitmap_membership(3, &roots, &query, &kinds, &control)
                    .unwrap(),
                expected
            );
            assert_eq!(
                board
                    .word_bitmap_membership_for_board(7, 3, &roots, &query, &kinds, &control)
                    .unwrap(),
                expected
            );
        }
        let before = board.word_bitmap_board_metrics();
        let word_before = board.word_bitmap_metrics();
        let supply_before = board.supply_multiset_metrics();
        assert_eq!(
            board
                .word_bitmap_membership_for_board(
                    7,
                    3,
                    &[first, second],
                    &known_supply(&[1, 2, 1], 0, true),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        let after = board.word_bitmap_board_metrics();
        assert_eq!(after[0], before[0] + 1);
        assert_eq!(after[1], before[1] + 1);
        assert_eq!(after[2], before[2]);
        assert_eq!(after[3], before[3]);
        assert_eq!(after[5], before[5] + 1); // One cache lookup; no root rescan/OR.
        assert_eq!(board.supply_multiset_metrics(), supply_before);
        assert_eq!(board.word_bitmap_metrics()[2], word_before[2]);
        assert_eq!(board.word_bitmap_metrics()[4], word_before[4]);
        assert!(after[5] <= board.word_bitmap_metrics()[8]);
        assert!(board.word_bitmap_peak_bytes() >= board.word_bitmap_retained_bytes());
        assert!(board.peak_bytes() >= board.retained_bytes());
    }

    #[test]
    fn word_bitmap_board_full_empty_skip_query_payload_and_validate_all_roots() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = [
            relation(&mut kinds, &[I, O], &[&[0, 1]]),
            relation(&mut kinds, &[I, O], &[&[1, 0]]),
        ];
        let mut bitmap = arena();
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    9,
                    2,
                    &roots,
                    &known_supply(&[2, 1], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        let before = bitmap.word_bitmap_metrics();
        assert_eq!(before[2], 0);
        assert_eq!(bitmap.word_bitmap_board_metrics()[3], 0); // Full has no payload.
        assert!(before[4] > 0); // The exact root languages were still proved.
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    9,
                    2,
                    &roots,
                    &known_supply(&[1, 1], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(false)
        );
        assert_eq!(bitmap.word_bitmap_metrics()[2], 0);
        assert_eq!(bitmap.word_bitmap_board_metrics()[3], 0); // Empty has no payload either.
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    9,
                    2,
                    &roots,
                    &known_supply(&[1], 2, true),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(false)
        ); // No held-only EOF release.

        let mut malformed = arena();
        assert!(malformed
            .word_bitmap_membership_for_board(
                9,
                2,
                &[roots[0], roots[1], u32::MAX],
                &known_supply(&[1, 2], 0, false),
                &kinds,
                &control
            )
            .is_err());
        assert_eq!(malformed.word_bitmap_board_metrics()[3], 0);
        assert!(matches!(
            malformed.intern(2, &roots, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));

        let before_work = bitmap.metrics()[4];
        let cancelled = ExecutionControl::default();
        cancelled.cancellation.handle().cancel();
        assert!(matches!(
            bitmap.word_bitmap_membership_for_board(
                9,
                2,
                &roots,
                &known_supply(&[2, 1], 0, false),
                &kinds,
                &cancelled
            ),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(bitmap.metrics()[4], before_work);
    }

    #[test]
    fn word_bitmap_board_payload_uses_shared_credits_and_eviction_keeps_languages() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = [
            relation(&mut kinds, &[I, O], &[&[0, 1]]),
            relation(&mut kinds, &[I, O], &[&[1, 0]]),
        ];
        let mut bitmap = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 14,
                work: 0,
            },
            KindUnionResources {
                members: 24,
                ..resources()
            },
        )
        .unwrap();
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    4,
                    2,
                    &roots[..1],
                    &known_supply(&[1, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        assert_eq!(bitmap.word_bitmap_board_metrics()[3], 1);
        assert_eq!(bitmap.word_bitmap_metrics()[5], 5); // Three root words, board OR, target query.
        let before = bitmap.word_bitmap_metrics();
        let peak = bitmap.word_bitmap_peak_bytes();
        let union = bitmap.intern(2, &roots, &kinds, &control).unwrap();
        assert_eq!(
            lazy_words(&mut bitmap, &kinds, union),
            BTreeSet::from([vec![1, 2], vec![2, 1]])
        );
        assert_eq!(bitmap.word_bitmap_retained_bytes(), 0);
        assert_eq!(&bitmap.word_bitmap_metrics()[..8], &before[..8]);
        assert!(bitmap.word_bitmap_metrics()[8] > before[8]);
        assert!(bitmap.word_bitmap_peak_bytes() >= peak);

        let mut bounded = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                nodes: 1,
                ..resources()
            },
        )
        .unwrap();
        assert_eq!(
            bounded
                .word_bitmap_membership_for_board(
                    4,
                    2,
                    &roots,
                    &known_supply(&[1, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            None
        );
        assert!(bounded.word_bitmap_board_metrics()[4] > 0);
        assert!(bounded.metrics()[4] > 0);
    }

    #[test]
    fn word_bitmap_board_targets_reuse_increasing_renames_but_validate_raw_queries() {
        use PieceKind::{I, O, S, T};
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let first = relation(&mut kinds, &[I, O], &[&[0, 1]]);
        let renamed = relation(&mut kinds, &[T, S], &[&[0, 1]]);
        let mut bitmap = arena();
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    1,
                    2,
                    &[first],
                    &known_supply(&[1, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        let before = bitmap.word_bitmap_metrics();
        assert_eq!(before[2], 1);
        assert_eq!(
            bitmap
                .word_bitmap_membership_for_board(
                    2,
                    2,
                    &[renamed],
                    &known_supply(&[3, 4], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        assert_eq!(bitmap.word_bitmap_metrics()[2], before[2]);
        assert_eq!(bitmap.word_bitmap_metrics()[1], before[1] + 1);
        assert_eq!(bitmap.word_bitmap_board_metrics()[2], 2);

        let before = bitmap.word_bitmap_board_metrics();
        assert!(matches!(
            bitmap.word_bitmap_membership_for_board(
                1,
                2,
                &[first],
                &known_supply(&[0, 2], 0, false),
                &kinds,
                &control
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_known_supply_invalid"
            ))
        ));
        assert_eq!(bitmap.word_bitmap_board_metrics(), before);
    }

    #[test]
    fn word_bitmap_six_piece_rank_blocks_preserve_sparse_and_same_kind_nfa_paths() {
        use PieceKind::{I, J, O, S, T, Z};
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let first = relation(
            &mut kinds,
            &[I, O, T, S, Z, J],
            &[
                &[5, 4, 3, 2, 1, 0],
                &[0, 1, 2, 3, 4, 5],
                &[2, 0, 5, 1, 4, 3],
            ],
        );
        let repeated = relation(
            &mut kinds,
            &[I, I, O, O, T, S],
            &[
                &[0, 2, 1, 4, 3, 5],
                &[1, 0, 3, 5, 4, 2],
                &[0, 1, 2, 3, 4, 5],
            ],
        );
        let roots = [first, repeated];
        let mut bitmap = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                nodes: 2_048,
                members: 8_192,
                work: 100_000,
                ..resources()
            },
        )
        .unwrap();
        for query in [
            known_supply(&[6, 5, 4, 3, 2, 1], 0, false),
            known_supply(&[1, 2, 3, 4, 5, 6], 0, false),
            known_supply(&[3, 1, 6, 2, 5, 4], 0, false),
            known_supply(&[1, 2, 3, 4, 6, 5], 0, false),
            known_supply(&[1, 2, 1, 3, 2, 4], 0, true),
            known_supply(&[1, 1, 2, 2, 3, 4, 7], 0, true),
            known_supply(&[2, 1, 3, 2, 4, 7], 1, true),
        ] {
            assert_eq!(
                bitmap
                    .word_bitmap_membership(6, &roots, &query, &kinds, &control)
                    .unwrap(),
                Some(reference_known_membership(&kinds, &roots, query)),
                "query={query:?}",
            );
        }
        assert!(bitmap.word_bitmap_metrics()[6] >= 12);
        assert!(bitmap.word_bitmap_metrics()[4] > 6);
    }

    #[test]
    fn word_bitmap_unknown_roots_never_become_negative_but_allow_positive_witnesses() {
        use crate::backend::wasm_cpu::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
        use clearra_core_domain::piece::rotation::RotationState;
        use PieceKind::{I, O, T};
        let control = ExecutionControl::default();
        let specs = [
            (0, 2, 0, false),
            (2, 1, 1, false),
            (3, 1, 1, false),
            (4, 0, 2, true),
        ]
        .map(
            |(edge_start, edge_count, depth, accepting)| BuildOrderNodeSpec {
                edge_start,
                edge_count,
                depth,
                accepting,
            },
        );
        let edges = [(1, I), (2, I), (3, O), (3, T)].map(|(child, piece)| {
            BuildOrderGraph::edge(child, 0, piece, RotationState::Zero, 0, 0, 0)
        });
        let graph =
            BuildOrderGraph::from_topological_parts(specs.into(), edges.into(), 0, 0, false)
                .unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let mixed = kinds.canonicalize(&graph).unwrap();
        let known = relation(&mut kinds, &[O, O], &[&[0, 1]]);
        let mut bitmap = arena();
        assert_eq!(
            bitmap
                .word_bitmap_membership(
                    2,
                    &[mixed, known],
                    &known_supply(&[2, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(
                    2,
                    &[mixed, known],
                    &known_supply(&[1, 1], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            None
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(
                    2,
                    &[known],
                    &known_supply(&[1, 1], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(false)
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(
                    2,
                    &[mixed],
                    &known_supply(&[1, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            None
        );
        assert_eq!(bitmap.word_bitmap_metrics()[7], 2);
        assert!(reference_known_membership(
            &kinds,
            &[mixed],
            known_supply(&[1, 2], 0, false)
        ));

        let mut board = arena();
        let roots = [mixed, known];
        for (query, expected) in [
            (known_supply(&[2, 2], 0, false), Some(true)),
            (known_supply(&[1, 1], 0, false), None),
            (known_supply(&[1, 2], 0, false), None),
        ] {
            assert_eq!(
                board
                    .word_bitmap_membership_for_board(4, 2, &roots, &query, &kinds, &control)
                    .unwrap(),
                expected
            );
        }
        // A cached unknown Empty union stays unknown. A known Full subset
        // is nevertheless a sound positive witness for the same frozen set.
        assert_eq!(
            board
                .word_bitmap_membership_for_board(
                    4,
                    2,
                    &roots,
                    &known_supply(&[1, 1], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            None
        );
        assert_eq!(
            board
                .word_bitmap_membership_for_board(
                    4,
                    2,
                    &roots,
                    &known_supply(&[2, 2], 0, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            Some(true)
        );
        assert!(board.word_bitmap_board_metrics()[1] >= 2);
    }

    #[test]
    fn word_bitmap_optional_storage_keeps_union_and_group_member_bounds() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = [
            relation(&mut kinds, &[I], &[&[0]]),
            relation(&mut kinds, &[O], &[&[0]]),
        ];
        let query = known_supply(&[1], 0, false);
        let mut full = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 20,
                work: 0,
            },
            KindUnionResources {
                members: 20,
                ..resources()
            },
        )
        .unwrap();
        assert_eq!(
            full.word_bitmap_membership(1, &[roots[0]], &query, &kinds, &control)
                .unwrap(),
            None
        );
        assert!(full.metrics()[4] > 0);
        assert_eq!(full.word_bitmap_metrics()[5], 0);
        // Exact raw-query metadata has its own finite bound and does not use
        // payload credits. The refused payload is never published.
        assert!(full.word_bitmap_retained_bytes() > 0);
        assert!(full.word_bitmap_peak_bytes() >= full.word_bitmap_retained_bytes());
        assert_eq!(
            full.intern(1, &[roots[0]], &kinds, &control).unwrap(),
            roots[0] | KIND_SOURCE_TAG
        );

        let mut bitmap = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 14,
                work: 0,
            },
            KindUnionResources {
                members: 20,
                ..resources()
            },
        )
        .unwrap();
        assert_eq!(
            bitmap
                .word_bitmap_membership(1, &[roots[0]], &query, &kinds, &control)
                .unwrap(),
            Some(true)
        );
        let bytes = bitmap.word_bitmap_retained_bytes();
        let before = bitmap.word_bitmap_metrics();
        assert_eq!(before[5], 3); // query, terminal, and I, each one u64 word.
        bitmap.set_multiset_grouping_enabled(true);
        assert_eq!(
            bitmap
                .intern_for_supply(6, 1, &roots, [1, 0, 0, 0, 0, 0, 0], &kinds, &control)
                .unwrap(),
            Some(roots[0] | KIND_SOURCE_TAG)
        );
        assert_eq!(bitmap.multiset_group_metrics()[5], 1);
        assert_eq!(bitmap.multiset_group_metrics()[6], 0);
        assert_eq!(bitmap.word_bitmap_retained_bytes(), bytes);
        let union = bitmap.intern(1, &roots, &kinds, &control).unwrap();
        assert_eq!(
            lazy_words(&mut bitmap, &kinds, union),
            BTreeSet::from([vec![1], vec![2]])
        );
        assert_eq!(bitmap.word_bitmap_retained_bytes(), 0);
        let after = bitmap.word_bitmap_metrics();
        assert_eq!(&after[..8], &before[..8]);
        assert!(after[8] > before[8]);
        assert!(bitmap.word_bitmap_peak_bytes() >= bytes);
        assert!(bitmap.peak_bytes() >= bitmap.retained_bytes());
    }

    #[test]
    fn word_bitmap_query_cache_is_bounded_and_keeps_full_exact_keys() {
        use super::word_bitmap::MAX_QUERY_CACHE;
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let root = relation(&mut kinds, &[PieceKind::I], &[&[0]]);
        let mut bitmap = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                work: 1_000_000,
                ..resources()
            },
        )
        .unwrap();
        // Unused tail bytes do not affect words, but remain in the exact raw
        // profile key. Target canonicalization normalizes them separately.
        // Fill the actual finite profile cache without fabricating entries,
        // changing a limit, or generating long supply executions.
        for index in 0..MAX_QUERY_CACHE {
            let mut query = known_supply(&[], 0, true);
            query.queue[5] = index as u8;
            query.queue[6] = (index >> 8) as u8;
            assert_eq!(
                bitmap
                    .word_bitmap_membership(1, &[root], &query, &kinds, &control)
                    .unwrap(),
                Some(false)
            );
        }
        let before = bitmap.word_bitmap_metrics();
        assert_eq!(before[2], 0); // No length-one target exists for an empty source.
        let mut next = known_supply(&[], 0, true);
        next.queue[4] = 1;
        assert_eq!(
            bitmap
                .word_bitmap_membership(1, &[root], &next, &kinds, &control)
                .unwrap(),
            None
        );
        assert_eq!(bitmap.word_bitmap_metrics()[2], before[2]);
        assert_eq!(
            bitmap
                .word_bitmap_membership(1, &[root], &known_supply(&[], 0, true), &kinds, &control)
                .unwrap(),
            Some(false)
        );
        assert_eq!(bitmap.word_bitmap_metrics()[1], before[1]);
        assert!(bitmap.word_bitmap_metrics()[3] > before[3]);
        assert_eq!(bitmap.word_bitmap_metrics()[7], 1);
        assert_eq!(bitmap.word_bitmap_metrics()[4], 0);
        assert!(bitmap.word_bitmap_peak_bytes() >= bitmap.word_bitmap_retained_bytes());
    }

    #[test]
    fn word_bitmap_membership_scope_owner_work_and_cancellation_fail_closed() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let query = known_supply(&[1, 2, 1], 0, true);
        let mut bitmap = arena();
        assert_eq!(
            bitmap
                .word_bitmap_membership(0, &roots, &query, &kinds, &control)
                .unwrap(),
            None
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(7, &roots, &query, &kinds, &control)
                .unwrap(),
            None
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(
                    3,
                    &roots,
                    &known_supply(&[1, 2, 1], 1, false),
                    &kinds,
                    &control
                )
                .unwrap(),
            None
        );
        assert_eq!(
            bitmap
                .word_bitmap_membership(3, &roots, &query, &kinds, &control)
                .unwrap(),
            Some(true)
        );
        assert!(matches!(
            bitmap.word_bitmap_membership(
                2,
                &roots,
                &known_supply(&[1, 2], 0, true),
                &kinds,
                &control
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch"
            ))
        ));
        assert!(matches!(
            bitmap.intern(3, &roots, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));

        let mut bitmap = arena();
        bitmap
            .word_bitmap_membership(3, &roots, &query, &kinds, &control)
            .unwrap();
        let mut other = PieceOrderLanguageCache::default();
        let other_roots = fixture(&mut other);
        assert!(matches!(
            bitmap.word_bitmap_membership(3, &other_roots, &query, &other, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));

        let mut bounded = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 11,
            },
            KindUnionResources {
                work: 12,
                ..resources()
            },
        )
        .unwrap();
        assert!(matches!(
            bounded.word_bitmap_membership(3, &roots, &query, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(bounded.metrics()[4], 1);
        assert_eq!(bounded.word_bitmap_metrics()[8], 1);
        assert!(matches!(
            bounded.intern(3, &roots, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));

        let mut bitmap = arena();
        bitmap
            .word_bitmap_membership(3, &roots, &query, &kinds, &control)
            .unwrap();
        let before = bitmap.metrics()[4];
        let cancelled = ExecutionControl::default();
        cancelled.cancellation.handle().cancel();
        assert!(matches!(
            bitmap.word_bitmap_membership(3, &roots, &query, &kinds, &cancelled),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(bitmap.metrics()[4], before);
    }

    fn arena() -> KindUnionArena {
        KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            resources(),
        )
        .unwrap()
    }

    fn lazy_words(
        arena: &mut KindUnionArena,
        kinds: &PieceOrderLanguageCache,
        root: u32,
    ) -> BTreeSet<Vec<u8>> {
        let (accepting, edges) = if root & KIND_SOURCE_TAG != 0 {
            let root = root & !KIND_SOURCE_TAG;
            let accepting = kinds.node(root).unwrap().accepting;
            let edges = (0..kinds.node_edge_count(root).unwrap())
                .map(|index| {
                    let edge = kinds.edge_at(root, index).unwrap();
                    (edge.piece_code(), edge.child() | KIND_SOURCE_TAG)
                })
                .collect::<Vec<_>>();
            (accepting, edges)
        } else {
            arena
                .prepare_edges(root, 0x7f, kinds, &ExecutionControl::default())
                .unwrap();
            (
                arena.node(root).unwrap().1,
                (0..arena.edge_count(root).unwrap())
                    .map(|index| arena.edge(root, index).unwrap())
                    .collect(),
            )
        };
        let mut words = BTreeSet::new();
        if accepting {
            words.insert(Vec::new());
        }
        for (piece, child) in edges {
            for suffix in lazy_words(arena, kinds, child) {
                let mut word = vec![piece];
                word.extend(suffix);
                words.insert(word);
            }
        }
        words
    }

    fn eager_words(language: &RootedPieceLanguageUnion, root: u32) -> BTreeSet<Vec<u8>> {
        let mut words = BTreeSet::new();
        if language.node(root).unwrap().accepting {
            words.insert(Vec::new());
        }
        for index in 0..language.edge_count(root).unwrap() {
            let (piece, child) = language.edge(root, index).unwrap();
            for suffix in eager_words(language, child) {
                let mut word = vec![piece];
                word.extend(suffix);
                words.insert(word);
            }
        }
        words
    }

    #[test]
    fn lazy_union_preserves_all_same_kind_children_and_matches_eager_language() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        assert_eq!(kinds.edges_for_piece(roots[0], 1).unwrap().len(), 2);
        let eager = RootedPieceLanguageUnion::new(&kinds, &roots).unwrap();
        let mut lazy = arena();
        let root = lazy.intern(3, &roots, &kinds, &control).unwrap();
        assert_eq!(
            lazy.intern(3, &[roots[1], roots[0], roots[1]], &kinds, &control)
                .unwrap(),
            root
        );
        assert_eq!(lazy.edge_count(root), Some(0));
        lazy.prepare_edges(root, 1 << 2, &kinds, &control).unwrap();
        let first = lazy.edge(root, 0).unwrap();
        assert_eq!(first.0, 3);
        lazy.prepare_edges(root, 1, &kinds, &control).unwrap();
        assert_eq!(
            lazy.edge(root, 0),
            Some(first),
            "later preparation preserves stored edge indices"
        );
        assert_eq!(lazy.edge_count(root), Some(2));
        let metrics = lazy.metrics();
        lazy.prepare_edges(root, 1 | (1 << 2), &kinds, &control)
            .unwrap();
        assert_eq!(lazy.metrics(), metrics);
        let words = lazy_words(&mut lazy, &kinds, root);
        assert_eq!(
            words,
            BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1], vec![3, 2, 1]])
        );
        assert_eq!(words, eager_words(&eager, eager.root()));
        assert!(lazy.peak_bytes() >= lazy.retained_bytes());
        let singleton = lazy
            .intern(3, &[roots[0], roots[0]], &kinds, &control)
            .unwrap();
        assert_eq!(singleton, roots[0] | KIND_SOURCE_TAG);
        assert_eq!(
            lazy_words(&mut lazy, &kinds, singleton),
            BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1]])
        );
    }

    #[test]
    fn revealed_supply_filters_exact_multisets_and_keeps_alias_contexts_separate() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let mut lazy = arena();
        let iio = [2, 1, 0, 0, 0, 0, 0];
        let toi = [1, 1, 1, 0, 0, 0, 0];
        let first = lazy
            .intern_for_supply(7, 3, &roots, iio, &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(first, roots[0] | KIND_SOURCE_TAG);
        let metrics = lazy.supply_multiset_metrics();
        assert_eq!(&metrics[..2], &[2, 1]);
        assert!(metrics[2] > 0 && metrics[3] > 0);
        assert_eq!(metrics[4], 1);
        assert_eq!(
            lazy.intern_for_supply(7, 3, &roots, iio, &kinds, &control)
                .unwrap(),
            Some(first)
        );
        assert_eq!(lazy.supply_multiset_metrics(), metrics);
        let second = lazy
            .intern_for_supply(7, 3, &roots, toi, &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(second, roots[1] | KIND_SOURCE_TAG);
        assert_ne!(first, second);
        let reject = lazy
            .intern_for_supply(7, 3, &roots, [3, 0, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(reject & KIND_SOURCE_TAG, 0);
        assert_eq!(lazy.node(reject), Some((3, false)));
        assert!(lazy_words(&mut lazy, &kinds, reject).is_empty());
        assert_eq!(lazy.edge_count(reject), Some(0));
        assert_eq!(lazy.supply_multiset_metrics()[4], 3);
        let unfiltered = lazy.intern(3, &roots, &kinds, &control).unwrap();
        assert_eq!(
            lazy_words(&mut lazy, &kinds, unfiltered),
            BTreeSet::from([vec![1, 1, 2], vec![1, 2, 1], vec![3, 2, 1]])
        );
        assert!(lazy.peak_bytes() >= lazy.retained_bytes());
    }

    #[test]
    fn multiset_groups_match_scalar_root_unions_for_all_small_supplies() {
        use PieceKind::{I, O};

        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let [first, second] = fixture(&mut kinds);
        let third = relation(&mut kinds, &[I, I, O], &[&[2, 0, 1]]);
        let roots = [first, second, third];
        let mut scalar = arena();
        let mut grouped = arena();
        grouped.set_multiset_grouping_enabled(true);
        for i in 0..=3 {
            for o in 0..=3 {
                for t in 0..=3 {
                    let available = [i, o, t, 0, 0, 0, 0];
                    let expected = scalar
                        .intern_for_supply(12, 3, &roots, available, &kinds, &control)
                        .unwrap()
                        .unwrap();
                    let actual = grouped
                        .intern_for_supply(12, 3, &roots, available, &kinds, &control)
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        lazy_words(&mut grouped, &kinds, actual),
                        lazy_words(&mut scalar, &kinds, expected),
                        "available={available:?}",
                    );
                }
            }
        }
        let metrics = grouped.multiset_group_metrics();
        assert_eq!(metrics[0], 1);
        assert_eq!(metrics[1], roots.len());
        assert!(metrics[2] > 0 && metrics[3] > 0 && metrics[5] > 0);
        assert_eq!(metrics[4], 0);
        assert_eq!(metrics[6], roots.len());
        assert!(metrics[7] > 0);
        assert_eq!(metrics[8], 0);
        assert_eq!(scalar.multiset_group_metrics(), [0; 9]);

        // The established alias is still checked before the optional index.
        let before_work = grouped.metrics()[4];
        grouped
            .intern_for_supply(12, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap();
        assert_eq!(grouped.metrics()[4], before_work + 1);
        assert_eq!(grouped.multiset_group_metrics(), metrics);
        assert!(grouped.peak_bytes() >= grouped.retained_bytes());
    }

    #[test]
    fn multiset_groups_visit_only_seven_remainder_keys_and_keep_zero_depth() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots: Vec<_> = (0..7)
            .map(|missing| {
                let pieces: Vec<_> = PieceKind::STANDARD_TETROMINOES
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, piece)| (index != missing).then_some(piece))
                    .collect();
                relation(&mut kinds, &pieces, &[&[0, 1, 2, 3, 4, 5]])
            })
            .collect();
        let mut scalar = arena();
        let expected = scalar
            .intern_for_supply(2, 6, &roots, [1; 7], &kinds, &control)
            .unwrap()
            .unwrap();
        let mut grouped = arena();
        grouped.set_multiset_grouping_enabled(true);
        let actual = grouped
            .intern_for_supply(2, 6, &roots, [1; 7], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(grouped.multiset_group_metrics()[2], 7);
        assert_eq!(grouped.multiset_group_metrics()[3], 7);
        assert_eq!(
            lazy_words(&mut grouped, &kinds, actual),
            lazy_words(&mut scalar, &kinds, expected)
        );
        let rejected = grouped
            .intern_for_supply(2, 6, &roots, [0; 7], &kinds, &control)
            .unwrap()
            .unwrap();
        assert!(lazy_words(&mut grouped, &kinds, rejected).is_empty());

        let terminal = (0..kinds.node_count() as u32)
            .find(|&node| kinds.node(node).unwrap().accepting)
            .unwrap();
        let accepted = grouped
            .intern_for_supply(3, 0, &[terminal], [0; 7], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(accepted, terminal | KIND_SOURCE_TAG);
        assert_eq!(
            lazy_words(&mut grouped, &kinds, accepted),
            BTreeSet::from([Vec::new()])
        );
    }

    #[test]
    fn multiset_groups_preserve_unknown_nfa_roots_and_member_cap_fallback() {
        use crate::backend::wasm_cpu::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
        use clearra_core_domain::piece::rotation::RotationState;
        use PieceKind::{I, O, T};

        let control = ExecutionControl::default();
        let specs = [
            (0, 2, 0, false),
            (2, 1, 1, false),
            (3, 1, 1, false),
            (4, 0, 2, true),
        ]
        .map(
            |(edge_start, edge_count, depth, accepting)| BuildOrderNodeSpec {
                edge_start,
                edge_count,
                depth,
                accepting,
            },
        );
        let edges = [(1, I), (2, I), (3, O), (3, T)].map(|(child, piece)| {
            BuildOrderGraph::edge(child, 0, piece, RotationState::Zero, 0, 0, 0)
        });
        let graph =
            BuildOrderGraph::from_topological_parts(specs.into(), edges.into(), 0, 0, false)
                .unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let unknown = kinds.canonicalize(&graph).unwrap();
        let known = relation(&mut kinds, &[O, O], &[&[0, 1]]);
        let roots = [unknown, known];
        let mut scalar = arena();
        let mut grouped = arena();
        grouped.set_multiset_grouping_enabled(true);
        for available in [[1, 0, 1, 0, 0, 0, 0], [0; 7], [0, 2, 0, 0, 0, 0, 0]] {
            let expected = scalar
                .intern_for_supply(4, 2, &roots, available, &kinds, &control)
                .unwrap()
                .unwrap();
            let actual = grouped
                .intern_for_supply(4, 2, &roots, available, &kinds, &control)
                .unwrap()
                .unwrap();
            assert_eq!(
                lazy_words(&mut grouped, &kinds, actual),
                lazy_words(&mut scalar, &kinds, expected)
            );
        }
        assert_eq!(grouped.multiset_group_metrics()[4], 3);

        let mut bounded = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 2,
                work: 0,
            },
            KindUnionResources {
                members: 2,
                ..resources()
            },
        )
        .unwrap();
        bounded.set_multiset_grouping_enabled(true);
        let actual = bounded
            .intern_for_supply(4, 2, &roots, [1, 0, 1, 0, 0, 0, 0], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(actual, unknown | KIND_SOURCE_TAG);
        assert_eq!(bounded.multiset_group_metrics()[0], 0);
        assert_eq!(bounded.multiset_group_metrics()[5], 1);
        assert_eq!(bounded.multiset_group_metrics()[6], 0);
        assert_eq!(&bounded.supply_multiset_metrics()[..2], &[2, 1]);
    }

    #[test]
    fn multiset_groups_evict_optional_members_before_mandatory_union() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let mut grouped = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                members: 2,
                ..resources()
            },
        )
        .unwrap();
        grouped.set_multiset_grouping_enabled(true);
        let first = grouped
            .intern_for_supply(8, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(first, roots[0] | KIND_SOURCE_TAG);
        assert_eq!(grouped.multiset_group_metrics()[6], 2);
        let before = grouped.multiset_group_metrics()[7];
        let union = grouped
            .intern_for_supply(8, 3, &roots, [2, 1, 1, 0, 0, 0, 0], &kinds, &control)
            .unwrap()
            .unwrap();
        assert_eq!(grouped.node(union), Some((3, false)));
        assert_eq!(grouped.metrics()[2], 2);
        let metrics = grouped.multiset_group_metrics();
        assert_eq!(metrics[6], 0);
        assert_eq!(metrics[8], 1);
        assert!(metrics[7] > before);
        assert_eq!(grouped.intern(3, &roots, &kinds, &control).unwrap(), union);
        let before_work = grouped.metrics()[4];
        assert_eq!(
            grouped
                .intern_for_supply(8, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
                .unwrap(),
            Some(first)
        );
        assert_eq!(grouped.metrics()[4], before_work + 1);
        assert_eq!(grouped.multiset_group_metrics(), metrics);
        assert!(grouped.peak_bytes() >= grouped.retained_bytes());
    }

    #[test]
    fn multiset_groups_fail_closed_for_depth_owner_work_and_cancellation() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let mut depth = arena();
        depth.set_multiset_grouping_enabled(true);
        depth
            .intern_for_supply(7, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap();
        assert!(matches!(
            depth.intern_for_supply(7, 2, &roots, [2, 0, 0, 0, 0, 0, 0], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch"
            ))
        ));
        assert!(matches!(
            depth.intern(3, &roots, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));

        let mut owner = arena();
        owner.set_multiset_grouping_enabled(true);
        owner
            .intern_for_supply(7, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap();
        let mut other = PieceOrderLanguageCache::default();
        let other_roots = fixture(&mut other);
        assert!(matches!(
            owner.intern_for_supply(7, 3, &other_roots, [1, 1, 1, 0, 0, 0, 0], &other, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));

        let mut exhausted = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 100,
            },
            KindUnionResources {
                work: 104,
                ..resources()
            },
        )
        .unwrap();
        exhausted.set_multiset_grouping_enabled(true);
        assert!(matches!(
            exhausted.intern_for_supply(7, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(exhausted.metrics()[4], 4);
        let metrics = exhausted.multiset_group_metrics();
        assert_eq!(metrics[0], 0);
        assert_eq!(metrics[1], 1);
        assert_eq!(metrics[6], 0);
        assert_eq!(metrics[7], 3);
        assert!(exhausted.peak_bytes() >= exhausted.retained_bytes());

        let mut cancelled = arena();
        cancelled.set_multiset_grouping_enabled(true);
        cancelled
            .intern_for_supply(7, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control)
            .unwrap();
        let before = cancelled.multiset_group_metrics();
        control.cancellation.handle().cancel();
        assert!(matches!(
            cancelled.intern_for_supply(7, 3, &roots, [2, 1, 0, 0, 0, 0, 0], &kinds, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(cancelled.multiset_group_metrics(), before);
    }

    #[test]
    fn revealed_supply_preserves_nonuniform_nfa_and_bounded_cache_fallbacks() {
        use crate::backend::wasm_cpu::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
        use clearra_core_domain::piece::rotation::RotationState;
        use PieceKind::{I, O, T};
        let control = ExecutionControl::default();
        let specs = [
            (0, 2, 0, false),
            (2, 1, 1, false),
            (3, 1, 1, false),
            (4, 0, 2, true),
        ]
        .map(
            |(edge_start, edge_count, depth, accepting)| BuildOrderNodeSpec {
                edge_start,
                edge_count,
                depth,
                accepting,
            },
        );
        let edges = [(1, I), (2, I), (3, O), (3, T)].map(|(child, piece)| {
            BuildOrderGraph::edge(child, 0, piece, RotationState::Zero, 0, 0, 0)
        });
        let graph =
            BuildOrderGraph::from_topological_parts(specs.into(), edges.into(), 0, 0, false)
                .unwrap();
        let mut kinds = PieceOrderLanguageCache::default();
        let root = kinds.canonicalize(&graph).unwrap();
        assert_eq!(kinds.edges_for_piece(root, 1).unwrap().len(), 2);
        let mut lazy = arena();
        let filtered = lazy
            .intern_for_supply(3, 2, &[root], [1, 0, 1, 0, 0, 0, 0], &kinds, &control)
            .unwrap()
            .unwrap();
        // Looking only at the first I child (IO) would incorrectly discard IT.
        assert_eq!(filtered, root | KIND_SOURCE_TAG);
        assert_eq!(lazy.supply_multiset_metrics()[1], 0);
        assert_eq!(
            lazy_words(&mut lazy, &kinds, filtered),
            BTreeSet::from([vec![1, 2], vec![1, 3]])
        );
        assert_eq!(
            lazy.intern_for_supply(3, 2, &[root], [16, 0, 0, 0, 0, 0, 0], &kinds, &control)
                .unwrap(),
            None,
            "an unrepresentable count preserves the unfiltered language"
        );
        let mut bounded = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                nodes: 1,
                ..resources()
            },
        )
        .unwrap();
        assert_eq!(
            bounded
                .intern_for_supply(3, 2, &[root], [1, 0, 1, 0, 0, 0, 0], &kinds, &control)
                .unwrap(),
            Some(root | KIND_SOURCE_TAG)
        );
        assert_eq!(
            bounded
                .intern_for_supply(4, 2, &[root], [1, 0, 1, 0, 0, 0, 0], &kinds, &control)
                .unwrap(),
            None,
            "a full optional alias cache falls back without rejecting"
        );
        assert_eq!(bounded.supply_multiset_metrics()[4], 1);

        let mut exhausted = KindUnionArena::new(
            KindUnionResources {
                nodes: 0,
                edges: 0,
                members: 0,
                work: 0,
            },
            KindUnionResources {
                work: 1,
                ..resources()
            },
        )
        .unwrap();
        assert!(matches!(
            exhausted.intern_for_supply(3, 2, &[root], [1, 0, 1, 0, 0, 0, 0], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert!(matches!(
            exhausted.intern(2, &[root], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));

        let mut other_kinds = PieceOrderLanguageCache::default();
        let other_root = other_kinds.canonicalize(&graph).unwrap();
        assert!(matches!(
            lazy.intern_for_supply(
                4,
                2,
                &[other_root],
                [1, 0, 1, 0, 0, 0, 0],
                &other_kinds,
                &control
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_suffix_arena_changed"
            ))
        ));
        assert!(matches!(
            lazy.intern(2, &[root], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_failed"
            ))
        ));
    }

    #[test]
    fn lazy_union_includes_family_resources_and_refuses_invalid_languages() {
        let control = ExecutionControl::default();
        let mut kinds = PieceOrderLanguageCache::default();
        let roots = fixture(&mut kinds);
        let base = KindUnionResources {
            nodes: 8,
            edges: 9,
            members: 10,
            work: 100,
        };
        for (limits, expected) in [
            (
                KindUnionResources {
                    nodes: 8,
                    ..resources()
                },
                "wasm_observation_physical_node_limit",
            ),
            (
                KindUnionResources {
                    members: 11,
                    ..resources()
                },
                "wasm_observation_physical_union_limit",
            ),
            (
                KindUnionResources {
                    work: 102,
                    ..resources()
                },
                "wasm_observation_physical_work_limit",
            ),
        ] {
            let mut lazy = KindUnionArena::new(base, limits).unwrap();
            assert!(
                matches!(lazy.intern(3, &roots, &kinds, &control), Err(WasmExactSearchError::InvalidProblem(code)) if code == expected)
            );
            assert!(matches!(
                lazy.intern(3, &[roots[0]], &kinds, &control),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_kind_union_failed"
                ))
            ));
        }
        let mut lazy = KindUnionArena::new(
            base,
            KindUnionResources {
                edges: 9,
                ..resources()
            },
        )
        .unwrap();
        let root = lazy.intern(3, &roots, &kinds, &control).unwrap();
        assert!(matches!(
            lazy.prepare_edges(root, 1, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_edge_limit"
            ))
        ));
        assert!(matches!(
            arena().intern(2, &roots, &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_depth_mismatch"
            ))
        ));
        assert!(matches!(
            arena().intern(3, &[], &kinds, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_kind_union_empty"
            ))
        ));
    }
}
