// SRP rationale: this module has one change reason: bounded canonical storage of normalized tiling solutions.
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, NormalizedTilingSolutionSetHasher, PiecePlacementMask,
    StandardBoard64TilingIdentity, STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
};

pub(crate) const PACKED_TILING_ROW_BITS: usize = 12;
pub(crate) const PACKED_TILING_ROW_MASK: u64 = (1_u64 << PACKED_TILING_ROW_BITS) - 1;
pub(crate) const PACKED_TILING_MAX_ROW_ID: u32 = PACKED_TILING_ROW_MASK as u32 - 1;
pub(crate) const PACKED_TILING_WORD_COUNT: usize =
    (STANDARD_BOARD64_TILING_MAX_PLACEMENTS * PACKED_TILING_ROW_BITS).div_ceil(u64::BITS as usize);
const PACKED_TILING_BYTE_COUNT: usize = PACKED_TILING_WORD_COUNT * core::mem::size_of::<u64>();
const PACKED_TILING_RADIX_THRESHOLD: usize = 1024;
const MAX_TILING_PROJECTION_UNIQUE_NODES: usize = 1_024;
const MAX_TILING_PROJECTION_POINTER_COMPARISONS: u128 = 4_194_304;

pub type PackedTilingRows = [u64; PACKED_TILING_WORD_COUNT];

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackedTilingMergeIndex {
    words: Vec<u64>,
    entry_bits: u8,
    root_bits: u8,
}

impl PackedTilingMergeIndex {
    fn new(
        root_count: usize,
        maximum_run_len: usize,
        entry_count: usize,
    ) -> Result<Self, &'static str> {
        let root_bits = bit_width(root_count.saturating_sub(1));
        let local_bits = bit_width(maximum_run_len.saturating_sub(1));
        let entry_bits = root_bits
            .checked_add(local_bits)
            .ok_or("wasm_tiling_merge_index_width_overflow")?;
        if (root_count > 1 && entry_bits == 0) || entry_bits > u64::BITS as usize {
            return Err("wasm_tiling_merge_index_width_invalid");
        }
        let bit_count = entry_count
            .checked_mul(entry_bits)
            .ok_or("wasm_tiling_merge_index_size_overflow")?;
        let word_count = bit_count.div_ceil(u64::BITS as usize);
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| "wasm_tiling_merge_index_storage_unavailable")?;
        words.resize(word_count, 0);
        Ok(Self {
            words,
            entry_bits: entry_bits as u8,
            root_bits: root_bits as u8,
        })
    }

    fn write(&mut self, index: usize, root: usize, local: usize) -> Result<(), &'static str> {
        let root_bits = usize::from(self.root_bits);
        let entry_bits = usize::from(self.entry_bits);
        let root = u64::try_from(root).map_err(|_| "wasm_tiling_merge_root_overflow")?;
        let local = u64::try_from(local).map_err(|_| "wasm_tiling_merge_local_overflow")?;
        let value = local
            .checked_shl(root_bits as u32)
            .and_then(|local| local.checked_add(root))
            .ok_or("wasm_tiling_merge_entry_overflow")?;
        write_packed_value(&mut self.words, index, entry_bits, value)
            .ok_or("wasm_tiling_merge_index_invalid")
    }

    fn read(&self, index: usize) -> Result<(usize, usize), &'static str> {
        let root_bits = usize::from(self.root_bits);
        let value = read_packed_value(&self.words, index, usize::from(self.entry_bits))
            .ok_or("wasm_tiling_merge_index_invalid")?;
        let root_mask = if root_bits == 0 {
            0
        } else {
            (1_u64 << root_bits) - 1
        };
        let root =
            usize::try_from(value & root_mask).map_err(|_| "wasm_tiling_merge_root_overflow")?;
        let local =
            usize::try_from(value >> root_bits).map_err(|_| "wasm_tiling_merge_local_overflow")?;
        Ok((root, local))
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        (self.words.capacity() as u128).checked_mul(core::mem::size_of::<u64>() as u128)
    }
}

#[cfg(test)]
pub(crate) fn pack_tiling_row_ids(rows: &[u32]) -> Option<PackedTilingRows> {
    if rows.is_empty() || rows.len() > STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        return None;
    }
    let mut packed = PackedTilingRows::default();
    let mut previous = None;
    for (index, row_id) in rows.iter().copied().enumerate() {
        if row_id > PACKED_TILING_MAX_ROW_ID || previous.is_some_and(|previous| row_id <= previous)
        {
            return None;
        }
        previous = Some(row_id);
        write_packed_tiling_row(&mut packed, index, u64::from(row_id) + 1)?;
    }
    Some(packed)
}

pub(crate) fn pack_canonical_tiling_row_ids(
    source_rows: &[u32],
    canonical_rank_by_source: &[u32],
) -> Option<PackedTilingRows> {
    if source_rows.is_empty() || source_rows.len() > STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        return None;
    }
    let mut packed = PackedTilingRows::default();
    let mut previous = None;
    for (index, source_row) in source_rows.iter().copied().enumerate() {
        if previous.is_some_and(|previous| source_row <= previous) {
            return None;
        }
        previous = Some(source_row);
        let canonical_rank = canonical_rank_by_source.get(source_row as usize).copied()?;
        if canonical_rank > PACKED_TILING_MAX_ROW_ID {
            return None;
        }
        write_packed_tiling_row(&mut packed, index, u64::from(canonical_rank) + 1)?;
    }
    Some(packed)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TilingSolutionPageStore {
    initial_board_mask: u64,
    catalog_rows: Arc<[PiecePlacementMask]>,
    identity_runs: Vec<Vec<PackedTilingRows>>,
    composite_children: Vec<Arc<TilingSolutionPageStore>>,
    merge_index: Option<PackedTilingMergeIndex>,
    identity_count: usize,
    normalized_hash: String,
}

impl TilingSolutionPageStore {
    /// Conservative allocation peak for constructing one canonical store from
    /// compact identities that are already retained by the search session.
    ///
    /// The projection deliberately counts every catalog work buffer together,
    /// the immutable catalog allocation, the distributed-run outer buffer,
    /// merge scratch/index storage, the optional radix stack, and the final
    /// store/`Arc` allocation. Moving the packed identity buffers into the
    /// store needs no additional credit, but one full packed copy is included
    /// so allocator implementation details cannot invalidate the admission.
    pub(crate) fn checked_canonical_construction_peak_upper_bound(
        catalog_count: usize,
        identity_count: usize,
        run_count: usize,
    ) -> Option<u128> {
        let catalog_count = catalog_count as u128;
        let identity_count = identity_count as u128;
        let run_count = run_count as u128;
        let catalog_row_bytes =
            catalog_count.checked_mul(core::mem::size_of::<PiecePlacementMask>() as u128)?;
        let indexed_catalog_bytes = catalog_count
            .checked_mul(core::mem::size_of::<(usize, PiecePlacementMask)>() as u128)?;
        let catalog_rank_bytes = catalog_count.checked_mul(core::mem::size_of::<u32>() as u128)?;
        let packed_identity_bytes =
            identity_count.checked_mul(core::mem::size_of::<PackedTilingRows>() as u128)?;
        // The packed merge index never needs more than one u64 word per
        // identity because an entry is rejected above 64 bits.
        let merge_index_bytes = identity_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
        let run_outer_bytes =
            run_count.checked_mul(core::mem::size_of::<Vec<PackedTilingRows>>() as u128)?;
        let merge_cursor_bytes = run_count.checked_mul(core::mem::size_of::<usize>() as u128)?;
        let merge_heap_bytes = run_count
            .checked_mul(core::mem::size_of::<Reverse<(PackedTilingRows, usize)>>() as u128)?;
        let radix_stack_bytes = if identity_count > PACKED_TILING_RADIX_THRESHOLD as u128 {
            ((PACKED_TILING_BYTE_COUNT * (u8::MAX as usize) + 1) as u128)
                .checked_mul(core::mem::size_of::<(usize, usize, usize)>() as u128)?
        } else {
            0
        };
        let final_store_bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add((core::mem::size_of::<usize>() as u128).checked_mul(2)?)?
            .checked_add(64)?;

        // Input catalog + indexed catalog + canonical catalog + Arc slice.
        catalog_row_bytes
            .checked_mul(3)?
            .checked_add(indexed_catalog_bytes)?
            .checked_add(catalog_rank_bytes)?
            .checked_add(packed_identity_bytes)?
            .checked_add(run_outer_bytes.checked_mul(2)?)?
            .checked_add(merge_index_bytes)?
            .checked_add(merge_cursor_bytes)?
            .checked_add(merge_heap_bytes)?
            .checked_add(radix_stack_bytes)?
            .checked_add(final_store_bytes)
    }

    /// Conservative peak for materializing the public initial page and its
    /// canonical string keys while the immutable family store remains live.
    pub(crate) fn checked_initial_page_build_peak_upper_bound(page_count: usize) -> Option<u128> {
        const NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND: u128 = 512;
        let page_count = page_count as u128;
        let identities =
            page_count.checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?;
        let keys = page_count.checked_mul(
            (core::mem::size_of::<String>() as u128)
                .checked_add(NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND)?,
        )?;
        identities.checked_add(keys)
    }

    /// Builds the immutable paging authority from already validated standard
    /// board identities. The resulting store owns a compact placement catalog
    /// plus packed row identities; callers do not have to retain the much
    /// larger `StandardBoard64TilingIdentity` vector after construction.
    pub(crate) fn from_standard_identities(
        initial_board_mask: u64,
        identities: Vec<StandardBoard64TilingIdentity>,
    ) -> Result<Self, &'static str> {
        let mut placement_count = 0_usize;
        for identity in &identities {
            if identity.initial_board_mask() != initial_board_mask {
                return Err("tiling_page_initial_board_mismatch");
            }
            placement_count = placement_count
                .checked_add(identity.placement_count())
                .ok_or("tiling_page_placement_count_overflow")?;
        }

        let mut catalog_rows = Vec::new();
        catalog_rows
            .try_reserve_exact(placement_count)
            .map_err(|_| "tiling_page_catalog_storage_unavailable")?;
        for identity in &identities {
            for index in 0..identity.placement_count() {
                catalog_rows.push(
                    identity
                        .placement(index)
                        .ok_or("tiling_page_identity_placement_missing")?,
                );
            }
        }
        catalog_rows.sort_unstable_by(|left, right| {
            left.piece()
                .as_ascii()
                .cmp(&right.piece().as_ascii())
                .then_with(|| left.cells_mask().cmp(&right.cells_mask()))
        });
        catalog_rows.dedup_by(|left, right| {
            left.piece() == right.piece() && left.cells_mask() == right.cells_mask()
        });
        if catalog_rows.len() > PACKED_TILING_MAX_ROW_ID as usize + 1 {
            return Err("tiling_page_catalog_too_large");
        }

        let mut packed_identities = Vec::new();
        packed_identities
            .try_reserve_exact(identities.len())
            .map_err(|_| "tiling_page_identity_storage_unavailable")?;
        for identity in identities {
            let mut packed = PackedTilingRows::default();
            for index in 0..identity.placement_count() {
                let placement = identity
                    .placement(index)
                    .ok_or("tiling_page_identity_placement_missing")?;
                let row_index = catalog_rows
                    .binary_search_by(|row| {
                        row.piece()
                            .as_ascii()
                            .cmp(&placement.piece().as_ascii())
                            .then_with(|| row.cells_mask().cmp(&placement.cells_mask()))
                    })
                    .map_err(|_| "tiling_page_catalog_row_missing")?;
                write_packed_tiling_row(&mut packed, index, row_index as u64 + 1)
                    .ok_or("tiling_page_identity_pack_failed")?;
            }
            packed_identities.push(packed);
        }

        Self::from_canonical_rows(initial_board_mask, catalog_rows, packed_identities)
    }

    #[cfg(test)]
    pub(crate) fn new(
        initial_board_mask: u64,
        catalog_rows: Vec<PiecePlacementMask>,
        mut identities: Vec<PackedTilingRows>,
    ) -> Result<Self, &'static str> {
        let (catalog_rows, canonical_rank_by_source) = canonicalize_catalog_rows(catalog_rows)?;
        for identity in &mut identities {
            *identity = remap_packed_rows(identity, &canonical_rank_by_source)?;
        }
        Self::from_canonical_rows(initial_board_mask, catalog_rows, identities)
    }

    pub(crate) fn new_canonical(
        initial_board_mask: u64,
        catalog_rows: Vec<PiecePlacementMask>,
        identities: Vec<PackedTilingRows>,
    ) -> Result<Self, &'static str> {
        let (catalog_rows, _) = canonicalize_catalog_rows(catalog_rows)?;
        Self::from_canonical_rows(initial_board_mask, catalog_rows, identities)
    }

    fn from_canonical_rows(
        initial_board_mask: u64,
        catalog_rows: Vec<PiecePlacementMask>,
        mut identities: Vec<PackedTilingRows>,
    ) -> Result<Self, &'static str> {
        sort_packed_tiling_rows(&mut identities)?;
        identities.dedup();
        Self::from_sorted_canonical_runs(initial_board_mask, catalog_rows, vec![identities])
    }

    pub(crate) fn new_canonical_runs(
        initial_board_mask: u64,
        catalog_rows: Vec<PiecePlacementMask>,
        identity_runs: Vec<Vec<PackedTilingRows>>,
    ) -> Result<Self, &'static str> {
        let (catalog_rows, _) = canonicalize_catalog_rows(catalog_rows)?;
        Self::from_sorted_canonical_runs(initial_board_mask, catalog_rows, identity_runs)
    }

    fn from_sorted_canonical_runs(
        initial_board_mask: u64,
        catalog_rows: Vec<PiecePlacementMask>,
        mut identity_runs: Vec<Vec<PackedTilingRows>>,
    ) -> Result<Self, &'static str> {
        identity_runs.retain(|run| !run.is_empty());
        let mut identity_count = 0_usize;
        for run in &identity_runs {
            if run.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err("wasm_tiling_run_order_invalid");
            }
            identity_count = identity_count
                .checked_add(run.len())
                .ok_or("wasm_tiling_solution_count_overflow")?;
        }

        let catalog_rows: Arc<[PiecePlacementMask]> = catalog_rows.into();
        let (merge_index, normalized_hash) = build_merge_index_and_hash(
            initial_board_mask,
            catalog_rows.as_ref(),
            &identity_runs,
            identity_count,
        )?;
        Ok(Self {
            initial_board_mask,
            catalog_rows,
            identity_runs,
            composite_children: Vec::new(),
            merge_index,
            identity_count,
            normalized_hash,
        })
    }

    pub(crate) fn merge_canonical_stores(
        mut stores: Vec<Arc<Self>>,
    ) -> Result<Arc<Self>, &'static str> {
        stores.retain(|store| !store.is_empty());
        if stores.len() == 1 {
            return Ok(stores.pop().expect("one retained tiling store"));
        }
        if stores.is_empty() {
            return Ok(Arc::new(Self {
                initial_board_mask: 0,
                catalog_rows: Arc::from([]),
                identity_runs: Vec::new(),
                composite_children: Vec::new(),
                merge_index: None,
                identity_count: 0,
                normalized_hash: NormalizedTilingSolutionSetHasher::default().finish(),
            }));
        }

        let maximum_run_len = stores.iter().map(|store| store.len()).max().unwrap_or(0);
        let maximum_entry_count = stores.iter().try_fold(0_usize, |total, store| {
            total
                .checked_add(store.len())
                .ok_or("wasm_tiling_solution_count_overflow")
        })?;
        let mut merge_index =
            PackedTilingMergeIndex::new(stores.len(), maximum_run_len, maximum_entry_count)?;
        let mut cursors = vec![0_usize; stores.len()];
        let mut heap = BinaryHeap::new();
        heap.try_reserve(stores.len())
            .map_err(|_| "wasm_tiling_merge_heap_storage_unavailable")?;
        for (child, store) in stores.iter().enumerate() {
            if let Some(identity) = store.first_identity()? {
                heap.push(Reverse((identity, child)));
            }
        }

        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        let mut previous = None;
        let mut output_index = 0_usize;
        while let Some(Reverse((identity, child))) = heap.pop() {
            let local = cursors[child];
            if previous != Some(identity) {
                merge_index.write(output_index, child, local)?;
                hash_standard_identity(&mut hasher, identity);
                output_index = output_index
                    .checked_add(1)
                    .ok_or("wasm_tiling_merge_count_overflow")?;
                previous = Some(identity);
            }
            cursors[child] = local
                .checked_add(1)
                .ok_or("wasm_tiling_merge_cursor_overflow")?;
            if cursors[child] < stores[child].len() {
                heap.push(Reverse((stores[child].identity_at(cursors[child])?, child)));
            }
        }

        Ok(Arc::new(Self {
            initial_board_mask: 0,
            catalog_rows: Arc::from([]),
            identity_runs: Vec::new(),
            composite_children: stores,
            merge_index: Some(merge_index),
            identity_count: output_index,
            normalized_hash: hasher.finish(),
        }))
    }

    pub fn len(&self) -> usize {
        self.identity_count
    }

    pub fn is_empty(&self) -> bool {
        self.identity_count == 0
    }

    pub fn normalized_hash(&self) -> &str {
        &self.normalized_hash
    }

    pub fn page_keys(&self, offset: usize, limit: usize) -> Result<Vec<String>, &'static str> {
        self.page_identities(offset, limit)?
            .into_iter()
            .map(|identity| {
                Ok(
                    NormalizedTilingSolutionKey::from_standard_board64_identity(identity)
                        .as_str()
                        .to_owned(),
                )
            })
            .collect()
    }

    pub fn page_identities(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<StandardBoard64TilingIdentity>, &'static str> {
        let begin = offset.min(self.identity_count);
        let end = begin.saturating_add(limit).min(self.identity_count);
        let mut identities = Vec::new();
        identities
            .try_reserve_exact(end - begin)
            .map_err(|_| "wasm_tiling_solution_page_storage_unavailable")?;
        if begin == end {
            return Ok(identities);
        }
        self.for_each_page_identity(begin, end - begin, |identity| identities.push(identity))?;
        Ok(identities)
    }

    /// Visits one bounded canonical page without allocating a page vector.
    ///
    /// The immutable store retains ownership of paging and index validation;
    /// transports can independently count or encode identities without
    /// materializing competing `Vec<String>` allocations.
    pub fn for_each_page_identity(
        &self,
        offset: usize,
        limit: usize,
        mut visit: impl FnMut(StandardBoard64TilingIdentity),
    ) -> Result<(), &'static str> {
        let begin = offset.min(self.identity_count);
        let end = begin.saturating_add(limit).min(self.identity_count);
        for index in begin..end {
            visit(self.identity_at(index)?);
        }
        Ok(())
    }

    /// Allocation-free comparison of a canonical prefix page against public
    /// key strings. This is used by the publication gate after the page has
    /// already been materialized under the terminal memory authority.
    pub(crate) fn initial_page_keys_match(&self, keys: &[String]) -> bool {
        keys.len() <= self.identity_count
            && keys.iter().enumerate().all(|(index, key)| {
                self.identity_at(index)
                    .is_ok_and(|identity| canonical_identity_matches_key(identity, key))
            })
    }

    pub fn first_identity(&self) -> Result<Option<StandardBoard64TilingIdentity>, &'static str> {
        Ok(self.page_identities(0, 1)?.into_iter().next())
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        usize::try_from(
            self.checked_owned_graph_retained_bytes()
                .unwrap_or(u128::MAX),
        )
        .unwrap_or(usize::MAX)
    }

    /// Returns the producer-authoritative backing retained by this immutable
    /// page-store graph.
    ///
    /// Downstream ownership boundaries use this allocation-free projection
    /// when an `Arc<TilingSolutionPageStore>` crosses crates. The graph walk
    /// counts shared store nodes and catalog slices once and fails closed when
    /// its fixed projection workspace is exhausted.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.checked_owned_graph_retained_bytes()
    }

    /// Inline workspace retained while `checked_retained_capacity_bytes`
    /// projects the immutable DAG. Callers enforcing a process-wide hard cap
    /// must authorize this stack carrier before invoking the projection.
    pub const fn checked_retained_capacity_projection_workspace_inline_bytes() -> Option<u128> {
        let node_table = core::mem::size_of::<
            [Option<&TilingSolutionPageStore>; MAX_TILING_PROJECTION_UNIQUE_NODES],
        >() as u128;
        let Some(node_tables) = node_table.checked_mul(2) else {
            return None;
        };
        let Some(counters) = (core::mem::size_of::<u128>() as u128).checked_mul(2) else {
            return None;
        };
        let Some(cursors) = (core::mem::size_of::<usize>() as u128).checked_mul(2) else {
            return None;
        };
        let Some(references) =
            (core::mem::size_of::<&TilingSolutionPageStore>() as u128).checked_mul(2)
        else {
            return None;
        };
        let Some(flags) = (core::mem::size_of::<bool>() as u128).checked_mul(3) else {
            return None;
        };
        let Some(with_counters) = node_tables.checked_add(counters) else {
            return None;
        };
        let Some(with_cursors) = with_counters.checked_add(cursors) else {
            return None;
        };
        let Some(with_references) = with_cursors.checked_add(references) else {
            return None;
        };
        with_references.checked_add(flags)
    }

    /// Checked backing owned by the complete immutable page-store DAG.
    ///
    /// The walk is allocation-free and counts pointer-identical store nodes and
    /// catalog slices once, while still counting every `Arc` slot physically
    /// present in each parent's `composite_children` buffer. Fixed-size pending
    /// and visited tables keep duplicate-child DAGs linear in their unique
    /// nodes without allocating before admission. Oversized node sets or
    /// adversarial comparison work fail closed.
    pub(crate) fn checked_owned_graph_retained_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        let mut comparisons = 0_u128;
        let mut pending = [None; MAX_TILING_PROJECTION_UNIQUE_NODES];
        let mut pending_len = 1_usize;
        pending[0] = Some(self);
        let mut visited = [None; MAX_TILING_PROJECTION_UNIQUE_NODES];
        let mut visited_len = 0_usize;

        while pending_len != 0 {
            pending_len -= 1;
            let store = pending[pending_len].take()?;
            if Self::checked_store_pointer_in(&visited[..visited_len], store, &mut comparisons)? {
                continue;
            }
            let catalog_seen = Self::checked_catalog_pointer_in(
                &visited[..visited_len],
                &store.catalog_rows,
                &mut comparisons,
            )?;
            if visited_len == visited.len() {
                return None;
            }
            visited[visited_len] = Some(store);
            visited_len += 1;

            bytes = bytes.checked_add(core::mem::size_of::<Self>() as u128)?;
            bytes = bytes.checked_add(
                (store.identity_runs.capacity() as u128)
                    .checked_mul(core::mem::size_of::<Vec<PackedTilingRows>>() as u128)?,
            )?;
            for run in &store.identity_runs {
                bytes = bytes.checked_add(
                    (run.capacity() as u128)
                        .checked_mul(core::mem::size_of::<PackedTilingRows>() as u128)?,
                )?;
            }
            bytes = bytes.checked_add(
                (store.composite_children.capacity() as u128)
                    .checked_mul(core::mem::size_of::<Arc<Self>>() as u128)?,
            )?;
            bytes = bytes.checked_add(
                store
                    .merge_index
                    .as_ref()
                    .map_or(Some(0), PackedTilingMergeIndex::checked_retained_bytes)?,
            )?;
            bytes = bytes.checked_add(store.normalized_hash.capacity() as u128)?;
            if !catalog_seen {
                bytes = bytes.checked_add(
                    (store.catalog_rows.len() as u128)
                        .checked_mul(core::mem::size_of::<PiecePlacementMask>() as u128)?,
                )?;
            }

            for child in store.composite_children.iter().rev() {
                let child = child.as_ref();
                let already_visited = Self::checked_store_pointer_in(
                    &visited[..visited_len],
                    child,
                    &mut comparisons,
                )?;
                let already_pending = Self::checked_store_pointer_in(
                    &pending[..pending_len],
                    child,
                    &mut comparisons,
                )?;
                if already_visited || already_pending {
                    continue;
                }
                if pending_len == pending.len() {
                    return None;
                }
                pending[pending_len] = Some(child);
                pending_len += 1;
            }
        }
        Some(bytes)
    }

    /// Cloning the public `Arc<TilingSolutionPageStore>` creates no nested
    /// backing; the immutable graph above remains shared and counted once.
    pub const fn checked_arc_clone_nested_bytes(&self) -> Option<u128> {
        Some(0)
    }

    fn checked_store_pointer_in(
        stores: &[Option<&Self>],
        target: &Self,
        comparisons: &mut u128,
    ) -> Option<bool> {
        for store in stores.iter().flatten() {
            *comparisons = comparisons.checked_add(1)?;
            if *comparisons > MAX_TILING_PROJECTION_POINTER_COMPARISONS {
                return None;
            }
            if core::ptr::eq(*store, target) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn checked_catalog_pointer_in(
        stores: &[Option<&Self>],
        target: &Arc<[PiecePlacementMask]>,
        comparisons: &mut u128,
    ) -> Option<bool> {
        for store in stores.iter().flatten() {
            *comparisons = comparisons.checked_add(1)?;
            if *comparisons > MAX_TILING_PROJECTION_POINTER_COMPARISONS {
                return None;
            }
            if Arc::ptr_eq(&store.catalog_rows, target) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn identity_at(&self, index: usize) -> Result<StandardBoard64TilingIdentity, &'static str> {
        if index >= self.identity_count {
            return Err("wasm_tiling_solution_index_out_of_range");
        }
        if !self.composite_children.is_empty() {
            let (child, local) = self
                .merge_index
                .as_ref()
                .ok_or("wasm_tiling_solution_merge_index_missing")?
                .read(index)?;
            return self
                .composite_children
                .get(child)
                .ok_or("wasm_tiling_solution_merge_child_invalid")?
                .identity_at(local);
        }
        if let Some(merge_index) = &self.merge_index {
            let (root, local) = merge_index.read(index)?;
            let rows = self
                .identity_runs
                .get(root)
                .and_then(|run| run.get(local))
                .ok_or("wasm_tiling_solution_merge_entry_invalid")?;
            return identity_from_packed(self.initial_board_mask, &self.catalog_rows, rows);
        }
        let rows = self
            .identity_runs
            .first()
            .and_then(|run| run.get(index))
            .ok_or("wasm_tiling_solution_run_missing")?;
        identity_from_packed(self.initial_board_mask, &self.catalog_rows, rows)
    }
}

fn canonical_identity_matches_key(identity: StandardBoard64TilingIdentity, key: &str) -> bool {
    const PREFIX: &[u8] = b"ctk1|initial=";
    const PLACEMENTS: &[u8] = b"|placements=";
    let bytes = key.as_bytes();
    let mut cursor = 0_usize;
    if bytes.get(..PREFIX.len()) != Some(PREFIX) {
        return false;
    }
    cursor += PREFIX.len();
    if !lower_hex_u64_matches(bytes, cursor, identity.initial_board_mask()) {
        return false;
    }
    cursor += 16;
    if bytes.get(cursor..cursor + PLACEMENTS.len()) != Some(PLACEMENTS) {
        return false;
    }
    cursor += PLACEMENTS.len();
    for index in 0..identity.placement_count() {
        if index != 0 {
            if bytes.get(cursor) != Some(&b',') {
                return false;
            }
            cursor += 1;
        }
        let Some(placement) = identity.placement(index) else {
            return false;
        };
        if bytes.get(cursor).copied() != Some(placement.piece().as_ascii() as u8)
            || bytes.get(cursor + 1) != Some(&b':')
            || !lower_hex_u64_matches(bytes, cursor + 2, placement.cells_mask())
        {
            return false;
        }
        cursor += 18;
    }
    cursor == bytes.len()
}

fn lower_hex_u64_matches(bytes: &[u8], offset: usize, value: u64) -> bool {
    let Some(actual) = bytes.get(offset..offset + 16) else {
        return false;
    };
    actual.iter().enumerate().all(|(index, byte)| {
        let shift = (15 - index) * 4;
        let digit = ((value >> shift) & 0xf) as u8;
        *byte
            == if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            }
    })
}

fn hash_standard_identity(
    hasher: &mut NormalizedTilingSolutionSetHasher,
    identity: StandardBoard64TilingIdentity,
) {
    hasher.update_canonical_placements(
        identity.initial_board_mask(),
        (0..identity.placement_count()).map(|index| {
            identity
                .placement(index)
                .expect("validated tiling identity placement index")
        }),
    );
}

fn build_merge_index_and_hash(
    initial_board_mask: u64,
    catalog_rows: &[PiecePlacementMask],
    identity_runs: &[Vec<PackedTilingRows>],
    identity_count: usize,
) -> Result<(Option<PackedTilingMergeIndex>, String), &'static str> {
    let mut hasher = NormalizedTilingSolutionSetHasher::default();
    if identity_runs.len() <= 1 {
        if let Some(run) = identity_runs.first() {
            for rows in run {
                hash_packed_tiling_rows(&mut hasher, initial_board_mask, catalog_rows, rows)?;
            }
        }
        return Ok((None, hasher.finish()));
    }

    let maximum_run_len = identity_runs.iter().map(Vec::len).max().unwrap_or(0);
    let mut merge_index =
        PackedTilingMergeIndex::new(identity_runs.len(), maximum_run_len, identity_count)?;
    let mut cursors = Vec::new();
    cursors
        .try_reserve_exact(identity_runs.len())
        .map_err(|_| "wasm_tiling_merge_cursor_storage_unavailable")?;
    cursors.resize(identity_runs.len(), 0_usize);
    let mut heap = BinaryHeap::new();
    heap.try_reserve(identity_runs.len())
        .map_err(|_| "wasm_tiling_merge_heap_storage_unavailable")?;
    for (root, run) in identity_runs.iter().enumerate() {
        if let Some(rows) = run.first() {
            heap.push(Reverse((*rows, root)));
        }
    }

    let mut output_index = 0_usize;
    let mut previous = None;
    while let Some(Reverse((rows, root))) = heap.pop() {
        if previous.is_some_and(|previous| previous >= rows) {
            return Err("wasm_tiling_merge_order_invalid");
        }
        let local = cursors[root];
        if identity_runs[root].get(local) != Some(&rows) {
            return Err("wasm_tiling_merge_cursor_invalid");
        }
        merge_index.write(output_index, root, local)?;
        hash_packed_tiling_rows(&mut hasher, initial_board_mask, catalog_rows, &rows)?;
        output_index = output_index
            .checked_add(1)
            .ok_or("wasm_tiling_merge_count_overflow")?;
        cursors[root] = local
            .checked_add(1)
            .ok_or("wasm_tiling_merge_cursor_overflow")?;
        if let Some(next) = identity_runs[root].get(cursors[root]) {
            heap.push(Reverse((*next, root)));
        }
        previous = Some(rows);
    }
    if output_index != identity_count {
        return Err("wasm_tiling_merge_count_mismatch");
    }
    Ok((Some(merge_index), hasher.finish()))
}

fn hash_packed_tiling_rows(
    hasher: &mut NormalizedTilingSolutionSetHasher,
    initial_board_mask: u64,
    catalog_rows: &[PiecePlacementMask],
    rows: &PackedTilingRows,
) -> Result<(), &'static str> {
    hasher.begin_canonical_identity(initial_board_mask);
    for index in 0..STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        let encoded = read_packed_tiling_row(rows, index);
        if encoded == 0 {
            break;
        }
        let row_index =
            usize::try_from(encoded - 1).map_err(|_| "wasm_tiling_row_identity_invalid")?;
        let placement = catalog_rows
            .get(row_index)
            .copied()
            .ok_or("wasm_tiling_row_identity_out_of_range")?;
        hasher.update_canonical_placement(placement, index == 0);
    }
    hasher.end_canonical_identity();
    Ok(())
}

pub(crate) fn canonicalize_catalog_rows(
    catalog_rows: Vec<PiecePlacementMask>,
) -> Result<(Vec<PiecePlacementMask>, Vec<u32>), &'static str> {
    let mut indexed_rows = Vec::new();
    indexed_rows
        .try_reserve_exact(catalog_rows.len())
        .map_err(|_| "wasm_tiling_page_catalog_index_unavailable")?;
    indexed_rows.extend(catalog_rows.into_iter().enumerate());
    indexed_rows.sort_unstable_by(|left, right| {
        left.1
            .piece()
            .as_ascii()
            .cmp(&right.1.piece().as_ascii())
            .then_with(|| left.1.cells_mask().cmp(&right.1.cells_mask()))
    });

    let mut canonical_rows = Vec::new();
    canonical_rows
        .try_reserve_exact(indexed_rows.len())
        .map_err(|_| "wasm_tiling_page_catalog_unavailable")?;
    let mut canonical_rank_by_source = Vec::new();
    canonical_rank_by_source
        .try_reserve_exact(indexed_rows.len())
        .map_err(|_| "wasm_tiling_page_catalog_map_unavailable")?;
    canonical_rank_by_source.resize(indexed_rows.len(), 0);
    for (rank, (source_index, row)) in indexed_rows.into_iter().enumerate() {
        canonical_rank_by_source[source_index] =
            u32::try_from(rank).map_err(|_| "wasm_tiling_page_catalog_too_large")?;
        canonical_rows.push(row);
    }
    Ok((canonical_rows, canonical_rank_by_source))
}

#[cfg(test)]
fn remap_packed_rows(
    source: &PackedTilingRows,
    canonical_rank_by_source: &[u32],
) -> Result<PackedTilingRows, &'static str> {
    let mut remapped = PackedTilingRows::default();
    for index in 0..STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        let encoded = read_packed_tiling_row(source, index);
        if encoded == 0 {
            break;
        }
        let source_index =
            usize::try_from(encoded - 1).map_err(|_| "wasm_tiling_row_identity_invalid")?;
        let rank = canonical_rank_by_source
            .get(source_index)
            .copied()
            .ok_or("wasm_tiling_row_identity_out_of_range")?;
        write_packed_tiling_row(&mut remapped, index, u64::from(rank) + 1)
            .ok_or("wasm_tiling_row_identity_invalid")?;
    }
    Ok(remapped)
}

fn sort_packed_tiling_rows(rows: &mut [PackedTilingRows]) -> Result<(), &'static str> {
    if rows.len() <= PACKED_TILING_RADIX_THRESHOLD {
        rows.sort_unstable();
        return Ok(());
    }

    let mut pending = Vec::new();
    pending
        .try_reserve_exact(PACKED_TILING_BYTE_COUNT * (u8::MAX as usize) + 1)
        .map_err(|_| "wasm_tiling_radix_stack_unavailable")?;
    pending.push((0_usize, rows.len(), 0_usize));
    let mut offsets = [0_usize; u8::MAX as usize + 2];
    let mut next = [0_usize; u8::MAX as usize + 1];

    while let Some((begin, end, byte_index)) = pending.pop() {
        let len = end - begin;
        if len <= 1 {
            continue;
        }
        if len <= PACKED_TILING_RADIX_THRESHOLD || byte_index == PACKED_TILING_BYTE_COUNT {
            rows[begin..end].sort_unstable();
            continue;
        }

        offsets.fill(0);
        for row in &rows[begin..end] {
            offsets[usize::from(packed_tiling_byte(row, byte_index)) + 1] += 1;
        }
        for index in 1..offsets.len() {
            offsets[index] += offsets[index - 1];
        }
        for bucket in 0..next.len() {
            next[bucket] = begin + offsets[bucket];
        }

        for bucket in 0..next.len() {
            let bucket_end = begin + offsets[bucket + 1];
            while next[bucket] < bucket_end {
                let source = next[bucket];
                let destination_bucket = usize::from(packed_tiling_byte(&rows[source], byte_index));
                if destination_bucket == bucket {
                    next[bucket] += 1;
                } else {
                    let destination = next[destination_bucket];
                    rows.swap(source, destination);
                    next[destination_bucket] += 1;
                }
            }
        }

        let next_byte = byte_index + 1;
        for bucket in 0..next.len() {
            let child_begin = begin + offsets[bucket];
            let child_end = begin + offsets[bucket + 1];
            if child_end - child_begin > 1 {
                pending.push((child_begin, child_end, next_byte));
            }
        }
    }
    Ok(())
}

fn packed_tiling_byte(rows: &PackedTilingRows, byte_index: usize) -> u8 {
    let word = byte_index / core::mem::size_of::<u64>();
    let byte_in_word = byte_index % core::mem::size_of::<u64>();
    ((rows[word] >> ((core::mem::size_of::<u64>() - 1 - byte_in_word) * u8::BITS as usize))
        & u64::from(u8::MAX)) as u8
}

fn bit_width(maximum_value: usize) -> usize {
    (usize::BITS - maximum_value.leading_zeros()) as usize
}

fn write_packed_value(words: &mut [u64], index: usize, width: usize, value: u64) -> Option<()> {
    if width == 0 || width > u64::BITS as usize || value.checked_shr(width as u32).unwrap_or(0) != 0
    {
        return None;
    }
    let bit = index.checked_mul(width)?;
    let word = bit / u64::BITS as usize;
    let offset = bit % u64::BITS as usize;
    if offset + width <= u64::BITS as usize {
        let shift = u64::BITS as usize - offset - width;
        *words.get_mut(word)? |= value << shift;
        return Some(());
    }

    let high_bits = u64::BITS as usize - offset;
    let low_bits = width - high_bits;
    let low_mask = (1_u64 << low_bits) - 1;
    *words.get_mut(word)? |= value >> low_bits;
    *words.get_mut(word + 1)? |= (value & low_mask) << (u64::BITS as usize - low_bits);
    Some(())
}

fn read_packed_value(words: &[u64], index: usize, width: usize) -> Option<u64> {
    if width == 0 || width > u64::BITS as usize {
        return None;
    }
    let bit = index.checked_mul(width)?;
    let word = bit / u64::BITS as usize;
    let offset = bit % u64::BITS as usize;
    if offset + width <= u64::BITS as usize {
        let shift = u64::BITS as usize - offset - width;
        let mask = if width == u64::BITS as usize {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        return Some((words.get(word)? >> shift) & mask);
    }

    let high_bits = u64::BITS as usize - offset;
    let low_bits = width - high_bits;
    let high_mask = (1_u64 << high_bits) - 1;
    let low_mask = (1_u64 << low_bits) - 1;
    let high = (words.get(word)? & high_mask) << low_bits;
    let low = (words.get(word + 1)? >> (u64::BITS as usize - low_bits)) & low_mask;
    Some(high | low)
}

pub(crate) fn write_packed_tiling_row(
    words: &mut PackedTilingRows,
    index: usize,
    value: u64,
) -> Option<()> {
    if index >= STANDARD_BOARD64_TILING_MAX_PLACEMENTS || value > PACKED_TILING_ROW_MASK {
        return None;
    }

    // Keep the first row in the most-significant bits of the first word.
    // Array ordering then matches row-by-row lexicographic ordering, so
    // millions of identities can be sorted without unpacking every row.
    let bit = index.checked_mul(PACKED_TILING_ROW_BITS)?;
    let word = bit / u64::BITS as usize;
    let offset = bit % u64::BITS as usize;
    if offset + PACKED_TILING_ROW_BITS <= u64::BITS as usize {
        let shift = u64::BITS as usize - offset - PACKED_TILING_ROW_BITS;
        words[word] |= value << shift;
    } else {
        let high_bits = u64::BITS as usize - offset;
        let low_bits = PACKED_TILING_ROW_BITS - high_bits;
        let low_mask = (1_u64 << low_bits) - 1;
        words[word] |= value >> low_bits;
        words[word + 1] |= (value & low_mask) << (u64::BITS as usize - low_bits);
    }
    Some(())
}

pub(crate) fn read_packed_tiling_row(words: &PackedTilingRows, index: usize) -> u64 {
    if index >= STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        return 0;
    }
    let bit = index * PACKED_TILING_ROW_BITS;
    let word = bit / u64::BITS as usize;
    let offset = bit % u64::BITS as usize;
    if offset + PACKED_TILING_ROW_BITS <= u64::BITS as usize {
        let shift = u64::BITS as usize - offset - PACKED_TILING_ROW_BITS;
        return (words[word] >> shift) & PACKED_TILING_ROW_MASK;
    }

    let high_bits = u64::BITS as usize - offset;
    let low_bits = PACKED_TILING_ROW_BITS - high_bits;
    let low_mask = (1_u64 << low_bits) - 1;
    let high = (words[word] & ((1_u64 << high_bits) - 1)) << low_bits;
    let low = (words[word + 1] >> (u64::BITS as usize - low_bits)) & low_mask;
    high | low
}

fn identity_from_packed(
    initial_board_mask: u64,
    catalog_rows: &[PiecePlacementMask],
    rows: &PackedTilingRows,
) -> Result<StandardBoard64TilingIdentity, &'static str> {
    let mut masks = [0_u64; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
    let mut packed_piece_codes = 0_u64;
    let mut count = 0;
    for (index, mask) in masks.iter_mut().enumerate() {
        let encoded = read_packed_tiling_row(rows, index);
        if encoded == 0 {
            break;
        }
        let row_id =
            usize::try_from(encoded - 1).map_err(|_| "wasm_tiling_row_identity_invalid")?;
        let row = catalog_rows
            .get(row_id)
            .copied()
            .ok_or("wasm_tiling_row_identity_out_of_range")?;
        *mask = row.cells_mask();
        packed_piece_codes |= u64::from(piece_code(row.piece())) << (index * 3);
        count += 1;
    }
    StandardBoard64TilingIdentity::from_compact_parts(
        initial_board_mask,
        packed_piece_codes,
        &masks[..count],
    )
    .map_err(|_| "wasm_tiling_identity_materialization_invalid")
}

const fn piece_code(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> u8 {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
            NormalizedTilingSolutionKey, NormalizedTilingSolutionSet, PiecePlacementMask,
            StandardBoard64TilingIdentity,
        },
    };

    use super::{
        write_packed_tiling_row, PackedTilingRows, TilingSolutionPageStore,
        MAX_TILING_PROJECTION_UNIQUE_NODES, PACKED_TILING_WORD_COUNT,
    };

    #[test]
    fn compact_page_store_preserves_exact_keys_count_and_hash() {
        let catalog = vec![
            PiecePlacementMask::new(PieceKind::I, 0b1111),
            PiecePlacementMask::new(PieceKind::O, 0b1111_0000),
            PiecePlacementMask::new(PieceKind::J, 0b1111_0000_0000),
        ];
        let mut first = PackedTilingRows::default();
        let mut second = PackedTilingRows::default();
        write_packed_tiling_row(&mut first, 0, 2).expect("O row");
        write_packed_tiling_row(&mut second, 0, 3).expect("J row");

        let store =
            TilingSolutionPageStore::new(0, catalog, vec![first, second]).expect("page store");
        let identities = store.page_identities(0, 10).expect("identities");
        assert_eq!(
            identities[0].placement(0).expect("first placement").piece(),
            PieceKind::J
        );
        assert_eq!(
            identities[1]
                .placement(0)
                .expect("second placement")
                .piece(),
            PieceKind::O
        );
        let expected_keys = identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(store.len(), 2);
        assert_eq!(store.page_keys(0, 10).expect("keys"), expected_keys);
        assert_eq!(
            store.normalized_hash(),
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities
            )
        );
        assert_eq!(
            store.normalized_hash(),
            NormalizedTilingSolutionSet::new(
                identities
                    .iter()
                    .copied()
                    .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            )
            .hash()
        );
        assert_eq!(store.page_keys(1, 1).expect("second page").len(), 1);
        assert!(store.page_keys(2, 1).expect("empty page").is_empty());
        let mut visited = Vec::new();
        store
            .for_each_page_identity(1, 1, |identity| visited.push(identity))
            .expect("allocation-free second page");
        assert_eq!(visited, identities[1..].to_vec());
    }

    #[test]
    fn standard_identity_constructor_canonicalizes_deduplicates_and_pages() {
        let first = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::O, 0x000f)],
        )
        .expect("first identity");
        let second = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0x00f0)],
        )
        .expect("second identity");
        let mut expected = vec![first, second];
        expected.sort_unstable();

        let store =
            TilingSolutionPageStore::from_standard_identities(0, vec![first, second, first])
                .expect("standard identity page store");

        assert_eq!(
            store.page_identities(0, 10).expect("identity page"),
            expected
        );
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.normalized_hash(),
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&expected)
        );
        assert_eq!(
            TilingSolutionPageStore::from_standard_identities(1, vec![first]),
            Err("tiling_page_initial_board_mismatch")
        );
    }

    #[test]
    fn canonical_root_runs_merge_without_copying_identity_storage() {
        let catalog = vec![
            PiecePlacementMask::new(PieceKind::I, 0b1111),
            PiecePlacementMask::new(PieceKind::I, 0b1111_0000),
            PiecePlacementMask::new(PieceKind::J, 0b1111_0000_0000),
        ];
        let mut first = PackedTilingRows::default();
        let mut second = PackedTilingRows::default();
        let mut third = PackedTilingRows::default();
        write_packed_tiling_row(&mut first, 0, 1).expect("first I row");
        write_packed_tiling_row(&mut first, 1, 3).expect("J row");
        write_packed_tiling_row(&mut second, 0, 2).expect("second I row");
        write_packed_tiling_row(&mut second, 1, 3).expect("J row");
        write_packed_tiling_row(&mut third, 0, 1).expect("first I row");
        write_packed_tiling_row(&mut third, 1, 2).expect("second I row");

        let store = TilingSolutionPageStore::new_canonical_runs(
            0,
            catalog,
            vec![vec![first, second], vec![third]],
        )
        .expect("merged root page store");
        let identities = store.page_identities(0, 10).expect("merged identities");
        let keys = identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(store.len(), 3);
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            store.normalized_hash(),
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities
            )
        );
        assert_eq!(
            store.page_keys(1, 2).expect("paged merged identities"),
            keys[1..].to_vec()
        );
    }

    #[test]
    fn canonical_store_union_deduplicates_across_symmetry_passes() {
        let catalog = vec![
            PiecePlacementMask::new(PieceKind::I, 0b1111),
            PiecePlacementMask::new(PieceKind::O, 0b1111_0000),
            PiecePlacementMask::new(PieceKind::J, 0b1111_0000_0000),
        ];
        let mut first = PackedTilingRows::default();
        let mut shared = PackedTilingRows::default();
        let mut last = PackedTilingRows::default();
        write_packed_tiling_row(&mut first, 0, 1).expect("I row");
        write_packed_tiling_row(&mut shared, 0, 2).expect("O row");
        write_packed_tiling_row(&mut last, 0, 3).expect("J row");
        let left = Arc::new(
            TilingSolutionPageStore::new(0, catalog.clone(), vec![first, shared])
                .expect("left store"),
        );
        let right = Arc::new(
            TilingSolutionPageStore::new(0, catalog, vec![shared, last]).expect("right store"),
        );

        let merged = TilingSolutionPageStore::merge_canonical_stores(vec![left, right])
            .expect("merged stores");
        let identities = merged.page_identities(0, 10).expect("merged identities");

        assert_eq!(merged.len(), 3);
        assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            merged.normalized_hash(),
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities
            )
        );
    }

    #[test]
    fn retained_projection_counts_shared_store_and_catalog_backing_once() {
        let mut normalized_hash = String::with_capacity(43);
        normalized_hash.push_str("leaf");
        let catalog_rows: Arc<[PiecePlacementMask]> = Arc::from([
            PiecePlacementMask::new(PieceKind::I, 0b1111),
            PiecePlacementMask::new(PieceKind::O, 0b1111_0000),
        ]);
        let mut run = Vec::with_capacity(5);
        run.push(PackedTilingRows::default());
        let mut identity_runs = Vec::with_capacity(3);
        identity_runs.push(run);
        let leaf = Arc::new(TilingSolutionPageStore {
            initial_board_mask: 0,
            catalog_rows,
            identity_runs,
            composite_children: Vec::new(),
            merge_index: None,
            identity_count: 1,
            normalized_hash,
        });

        let mut root_hash = String::with_capacity(47);
        root_hash.push_str("root");
        let mut children = Vec::with_capacity(7);
        children.push(Arc::clone(&leaf));
        children.push(Arc::clone(&leaf));
        let root = TilingSolutionPageStore {
            initial_board_mask: 0,
            catalog_rows: Arc::from([]),
            identity_runs: Vec::new(),
            composite_children: children,
            merge_index: None,
            identity_count: 1,
            normalized_hash: root_hash,
        };

        let expected = 2 * core::mem::size_of::<TilingSolutionPageStore>()
            + root.composite_children.capacity()
                * core::mem::size_of::<Arc<TilingSolutionPageStore>>()
            + root.normalized_hash.capacity()
            + leaf.catalog_rows.len() * core::mem::size_of::<PiecePlacementMask>()
            + leaf.identity_runs.capacity() * core::mem::size_of::<Vec<PackedTilingRows>>()
            + leaf.identity_runs[0].capacity()
                * PACKED_TILING_WORD_COUNT
                * core::mem::size_of::<u64>()
            + leaf.normalized_hash.capacity();
        assert_eq!(
            root.checked_owned_graph_retained_bytes(),
            Some(expected as u128)
        );
        assert_eq!(root.retained_bytes(), expected);
        assert_eq!(root.checked_arc_clone_nested_bytes(), Some(0));
    }

    #[test]
    fn retained_projection_visits_duplicate_dag_nodes_once_and_bounds_unique_nodes() {
        let mut store = Arc::new(TilingSolutionPageStore {
            initial_board_mask: 0,
            catalog_rows: Arc::from([]),
            identity_runs: Vec::new(),
            composite_children: Vec::new(),
            merge_index: None,
            identity_count: 0,
            normalized_hash: String::new(),
        });
        let mut expected = core::mem::size_of::<TilingSolutionPageStore>();
        for _ in 0..20 {
            let children = vec![Arc::clone(&store), Arc::clone(&store)];
            expected += core::mem::size_of::<TilingSolutionPageStore>()
                + children.capacity() * core::mem::size_of::<Arc<TilingSolutionPageStore>>();
            store = Arc::new(TilingSolutionPageStore {
                initial_board_mask: 0,
                catalog_rows: Arc::from([]),
                identity_runs: Vec::new(),
                composite_children: children,
                merge_index: None,
                identity_count: 0,
                normalized_hash: String::new(),
            });
        }
        assert_eq!(
            store.checked_owned_graph_retained_bytes(),
            Some(expected as u128)
        );

        for _ in 20..MAX_TILING_PROJECTION_UNIQUE_NODES {
            store = Arc::new(TilingSolutionPageStore {
                initial_board_mask: 0,
                catalog_rows: Arc::from([]),
                identity_runs: Vec::new(),
                composite_children: vec![store],
                merge_index: None,
                identity_count: 0,
                normalized_hash: String::new(),
            });
        }
        assert_eq!(store.checked_owned_graph_retained_bytes(), None);
    }

    #[test]
    fn retained_projection_workspace_reports_its_exact_inline_carrier() {
        let node_tables = core::mem::size_of::<
            [Option<&TilingSolutionPageStore>; MAX_TILING_PROJECTION_UNIQUE_NODES],
        >() * 2;
        let scalar_state = core::mem::size_of::<u128>() * 2
            + core::mem::size_of::<usize>() * 2
            + core::mem::size_of::<&TilingSolutionPageStore>() * 2
            + core::mem::size_of::<bool>() * 3;
        let exact = (node_tables + scalar_state) as u128;

        assert_eq!(
            TilingSolutionPageStore::checked_retained_capacity_projection_workspace_inline_bytes(),
            Some(exact)
        );
        assert!(exact > exact - 1);
    }
}
