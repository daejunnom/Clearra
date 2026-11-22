use super::compact_exact_u64_map::{
    checked_hash_reserve_future_bytes, ExactU64MemoMap, ExactU64MemoStorage,
};
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
};

const FULL_STANDARD_BAG: u8 = 0x7f;
const _: () = assert!(core::mem::size_of::<(u32, u32)>() == 8);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum StandardBagProductMemoLayout {
    /// Start flat; promote only after the worker's live memo amortizes row costs.
    #[default]
    Adaptive,
    /// Explicit reference and compact A/B controls, never inferred from workers.
    #[cfg(any(feature = "parallel", feature = "local-search-ab", test))]
    Flat,
    #[cfg(any(feature = "parallel", feature = "local-search-ab", test))]
    StateMajor,
}

impl StandardBagProductMemoLayout {
    pub fn from_environment() -> Result<Self, &'static str> {
        #[cfg(feature = "local-search-ab")]
        {
            Self::from_environment_value(std::env::var("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT"))
        }
        #[cfg(not(feature = "local-search-ab"))]
        {
            Ok(Self::Adaptive)
        }
    }

    #[cfg(any(feature = "local-search-ab", test))]
    fn from_environment_value(
        value: Result<String, std::env::VarError>,
    ) -> Result<Self, &'static str> {
        match value {
            Ok(label) => Self::from_label(Some(&label)),
            Err(std::env::VarError::NotPresent) => Self::from_label(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err("wasm_standard_bag_product_memo_layout_invalid")
            }
        }
    }

    #[cfg(any(feature = "local-search-ab", test))]
    fn from_label(label: Option<&str>) -> Result<Self, &'static str> {
        match label {
            None | Some("adaptive") => Ok(Self::Adaptive),
            Some("flat") => Ok(Self::Flat),
            Some("state-major") => Ok(Self::StateMajor),
            Some(_) => Err("wasm_standard_bag_product_memo_layout_invalid"),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Flat => "flat",
            Self::StateMajor => "state-major",
        }
    }
}

/// Exact product identity. Moving its source tuple into a row does not reduce
/// the width of either language references or decision roots. The owner is one
/// worker's request/epoch; no memo entry is portable across that namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StandardBagProductMemoKey {
    language_node: u32,
    depth: u8,
    bag_remainder: u8,
    hold_code: u8,
}

impl StandardBagProductMemoKey {
    pub const fn new(language_node: u32, depth: u8, bag_remainder: u8, hold_code: u8) -> Self {
        Self {
            language_node,
            depth,
            bag_remainder: if bag_remainder == 0 {
                FULL_STANDARD_BAG
            } else {
                bag_remainder
            },
            hold_code,
        }
    }

    const fn packed(self) -> u64 {
        self.language_node as u64
            | (self.depth as u64) << 32
            | (self.bag_remainder as u64) << 40
            | (self.hold_code as u64) << 48
    }

    fn from_packed(value: u64) -> Option<Self> {
        let key = Self::new(
            value as u32,
            (value >> 32) as u8,
            (value >> 40) as u8,
            (value >> 48) as u8,
        );
        // Never truncate unknown high bits or silently renormalize a stored key.
        (key.packed() == value).then_some(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProductMemoAdmissionError {
    Capacity,
    OutOfScope,
}

pub(super) struct StandardBagProductMemo {
    storage: ProductMemoStorage,
    policy: StandardBagProductMemoLayout,
    adaptive: Option<AdaptiveProductMemo>,
    #[cfg(test)]
    failed_admissions_remaining: usize,
    #[cfg(test)]
    fail_promotion: bool,
}

struct AdaptiveProductMemo {
    max_source_depth: u8,
    next_check_len: usize,
    promotion_attempts: usize,
    promotions: usize,
}

enum ProductMemoStorage {
    Flat(ExactU64MemoMap),
    StateMajor(StateMajorProductMemo),
}

impl StandardBagProductMemo {
    pub fn new(
        layout: StandardBagProductMemoLayout,
        flat_storage: ExactU64MemoStorage,
        max_source_depth: u8,
    ) -> Self {
        Self {
            storage: match layout {
                StandardBagProductMemoLayout::Adaptive => {
                    ProductMemoStorage::Flat(ExactU64MemoMap::new(flat_storage))
                }
                #[cfg(any(feature = "parallel", feature = "local-search-ab", test))]
                StandardBagProductMemoLayout::Flat => {
                    ProductMemoStorage::Flat(ExactU64MemoMap::new(flat_storage))
                }
                #[cfg(any(feature = "parallel", feature = "local-search-ab", test))]
                StandardBagProductMemoLayout::StateMajor => {
                    ProductMemoStorage::StateMajor(StateMajorProductMemo::new(max_source_depth))
                }
            },
            policy: layout,
            adaptive: (layout == StandardBagProductMemoLayout::Adaptive).then(|| {
                AdaptiveProductMemo {
                    max_source_depth,
                    next_check_len: adaptive_first_check_len(max_source_depth, flat_storage),
                    promotion_attempts: 0,
                    promotions: 0,
                }
            }),
            #[cfg(test)]
            failed_admissions_remaining: 0,
            #[cfg(test)]
            fail_promotion: false,
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn layout(&self) -> StandardBagProductMemoLayout {
        match &self.storage {
            ProductMemoStorage::Flat(_) => StandardBagProductMemoLayout::Flat,
            ProductMemoStorage::StateMajor(_) => StandardBagProductMemoLayout::StateMajor,
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn storage_label(&self) -> &'static str {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.storage().label(),
            ProductMemoStorage::StateMajor(_) => "state-major",
        }
    }

    #[inline]
    pub fn get(&self, key: &StandardBagProductMemoKey) -> Option<&u32> {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.get(&key.packed()),
            ProductMemoStorage::StateMajor(memo) => {
                let row_index = memo.row_index(*key)?;
                memo.rows.get(row_index)?.as_ref()?.get(&key.language_node)
            }
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &StandardBagProductMemoKey) -> bool {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.contains_key(&key.packed()),
            ProductMemoStorage::StateMajor(_) => self.get(key).is_some(),
        }
    }

    /// A cache admission failure is never a solver failure or a negative proof.
    /// Callers retain the computed exact root and merely forgo this cache entry.
    #[inline]
    pub fn try_insert(
        &mut self,
        key: StandardBagProductMemoKey,
        value: u32,
    ) -> Result<Option<u32>, ProductMemoAdmissionError> {
        self.try_insert_guarded(key, value, |_, _| true)
    }

    /// `admit` sees this memo's currently retained payload/directory and the
    /// whole new allocation while its old storage remains live. Rejection
    /// only declines a cache entry or optional layout promotion.
    pub fn try_insert_guarded(
        &mut self,
        key: StandardBagProductMemoKey,
        value: u32,
        mut admit: impl FnMut(usize, u128) -> bool,
    ) -> Result<Option<u32>, ProductMemoAdmissionError> {
        #[cfg(test)]
        if self.failed_admissions_remaining != 0 {
            self.failed_admissions_remaining -= 1;
            return Err(ProductMemoAdmissionError::Capacity);
        }
        let previous = match &mut self.storage {
            ProductMemoStorage::Flat(memo) => {
                let future = memo
                    .checked_reserve_future_bytes(1)
                    .ok_or(ProductMemoAdmissionError::Capacity)?;
                if !admit(memo.retained_payload_bytes(), future) {
                    return Err(ProductMemoAdmissionError::Capacity);
                }
                memo.try_reserve(1)
                    .map_err(|_| ProductMemoAdmissionError::Capacity)?;
                memo.insert(key.packed(), value)
            }
            ProductMemoStorage::StateMajor(memo) => {
                memo.try_insert_guarded(key, value, &mut admit)?
            }
        };
        if previous.is_none() {
            self.maybe_promote(&mut admit);
        }
        Ok(previous)
    }

    #[cfg(test)]
    fn retained_bytes(&self) -> usize {
        self.retained_payload_bytes()
            .saturating_add(self.directory_retained_bytes())
    }

    /// Deterministic admission-failure fixture, absent from every product and
    /// local A/B binary. It exercises exact fallback without inducing an OOM.
    #[cfg(test)]
    pub fn fail_admissions_for_test(&mut self, count: usize) {
        self.failed_admissions_remaining = count;
    }

    #[cfg(test)]
    fn try_reserve_for(
        &mut self,
        key: StandardBagProductMemoKey,
        additional: usize,
    ) -> Result<(), ProductMemoAdmissionError> {
        match &mut self.storage {
            ProductMemoStorage::Flat(memo) => memo
                .try_reserve(additional)
                .map_err(|_| ProductMemoAdmissionError::Capacity),
            ProductMemoStorage::StateMajor(memo) => {
                let row_index = memo
                    .row_index(key)
                    .ok_or(ProductMemoAdmissionError::OutOfScope)?;
                memo.prepare_row(row_index, key)?;
                memo.mutate_row(row_index, |row| row.try_reserve(additional))
                    .ok_or(ProductMemoAdmissionError::Capacity)?
                    .map_err(|_| ProductMemoAdmissionError::Capacity)
            }
        }
    }

    pub fn clear(&mut self) {
        match &mut self.storage {
            ProductMemoStorage::Flat(memo) => memo.clear(),
            ProductMemoStorage::StateMajor(memo) => {
                if memo.entry_count != 0 {
                    for row in memo.rows.iter_mut().flatten() {
                        row.clear();
                    }
                    memo.entry_count = 0;
                }
                // HashMap::clear retains capacity; the cached sum stays exact.
            }
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.len(),
            ProductMemoStorage::StateMajor(memo) => memo.entry_count,
        }
    }

    /// Logical entry capacity payload, excluding row directory and hash control
    /// bytes. Directory payload is reported separately and charged exactly once.
    pub fn retained_payload_bytes(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.retained_payload_bytes(),
            ProductMemoStorage::StateMajor(memo) => memo.retained_payload_bytes(),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn capacity(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.capacity(),
            ProductMemoStorage::StateMajor(memo) => memo.row_capacity,
        }
    }

    pub fn directory_retained_bytes(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(_) => 0,
            ProductMemoStorage::StateMajor(memo) => memo
                .rows
                .capacity()
                .saturating_mul(core::mem::size_of::<Option<StateMajorRow>>()),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn active_rows(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(_) => 0,
            ProductMemoStorage::StateMajor(memo) => memo
                .rows
                .iter()
                .flatten()
                .filter(|row| !row.is_empty())
                .count(),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn allocated_rows(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(_) => 0,
            ProductMemoStorage::StateMajor(memo) => memo
                .rows
                .iter()
                .flatten()
                .filter(|row| row.capacity() != 0)
                .count(),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn row_slots(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(_) => 0,
            ProductMemoStorage::StateMajor(memo) => memo.rows.capacity(),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn policy(&self) -> StandardBagProductMemoLayout {
        self.policy
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn promotion_counts(&self) -> (usize, usize) {
        self.adaptive.as_ref().map_or((0, 0), |policy| {
            (policy.promotion_attempts, policy.promotions)
        })
    }

    /// Geometric checks keep failed cost probes amortized, not per-insert scans.
    /// Copy first, then atomically replace: even a late allocation failure keeps
    /// every exact entry and the newly computed root in the flat source.
    #[inline]
    fn maybe_promote(&mut self, admit: &mut impl FnMut(usize, u128) -> bool) {
        if self.policy != StandardBagProductMemoLayout::Adaptive {
            return;
        }
        if let (Some(policy), ProductMemoStorage::Flat(source)) = (&self.adaptive, &self.storage) {
            if source.len() >= policy.next_check_len {
                self.promote_flat(admit);
            }
        }
    }

    #[cold]
    fn promote_flat(&mut self, admit: &mut impl FnMut(usize, u128) -> bool) {
        let Some(policy) = &mut self.adaptive else {
            return;
        };
        let ProductMemoStorage::Flat(source) = &self.storage else {
            return;
        };
        let span = SearchStageSpan::begin(ExecutorSearchStage::WasmStandardBagMemoPromote);
        policy.next_check_len = source.len().saturating_mul(2);
        policy.promotion_attempts = policy.promotion_attempts.saturating_add(1);
        #[cfg(test)]
        if self.fail_promotion {
            policy.next_check_len = usize::MAX;
            return;
        }
        // copy_from retains the flat source while building a row directory,
        // its population scratch, and all destination row maps. Sum the whole
        // new surfaces; no post-allocation measurement authorizes a promotion.
        let Some(future) =
            StateMajorProductMemo::checked_copy_future_bytes(source.len(), policy.max_source_depth)
        else {
            return;
        };
        if !admit(source.retained_payload_bytes(), future) {
            return;
        }
        let candidate = match StateMajorProductMemo::copy_from(source, policy.max_source_depth) {
            Ok(candidate) => candidate,
            Err(_) => {
                // No allocation retry or changed resources after admission failure.
                policy.next_check_len = usize::MAX;
                return;
            }
        };
        let target_bytes = candidate
            .retained_payload_bytes()
            .saturating_add(candidate.directory_retained_bytes());
        // Require >=12.5% retained logical payload savings, including directory.
        // This is not a claim about allocator overhead or OS peak memory.
        let cost_limit = source
            .retained_payload_bytes()
            .saturating_sub(source.retained_payload_bytes() / 8);
        if target_bytes > cost_limit {
            return;
        }
        policy.promotions = policy.promotions.saturating_add(1);
        let promoted_slots = candidate.rows.capacity() as u64;
        self.storage = ProductMemoStorage::StateMajor(candidate);
        span.finish(promoted_slots);
    }
}

type StateMajorRow = HashMap<u32, u32, StateMajorRowHasher>;

struct StateMajorProductMemo {
    max_source_depth: u8,
    rows: Vec<Option<StateMajorRow>>,
    entry_count: usize,
    // Exact sum of the row capacities, not their lengths or the directory's
    // capacity. Browsers request retained bytes repeatedly between candidates.
    row_capacity: usize,
}

impl StateMajorProductMemo {
    const fn new(max_source_depth: u8) -> Self {
        Self {
            max_source_depth,
            rows: Vec::new(),
            entry_count: 0,
            row_capacity: 0,
        }
    }

    fn row_count(&self) -> usize {
        (usize::from(self.max_source_depth) + 1) * 128 * 8
    }

    fn retained_payload_bytes(&self) -> usize {
        self.row_capacity
            .saturating_mul(core::mem::size_of::<(u32, u32)>())
    }

    fn prepare_row(
        &mut self,
        index: usize,
        key: StandardBagProductMemoKey,
    ) -> Result<(), ProductMemoAdmissionError> {
        if self.rows.is_empty() {
            let row_count = self.row_count();
            self.rows
                .try_reserve_exact(row_count)
                .map_err(|_| ProductMemoAdmissionError::Capacity)?;
            self.rows.resize_with(row_count, || None);
        }
        self.rows[index].get_or_insert_with(|| {
            StateMajorRow::with_hasher(StateMajorRowHasher {
                source_prefix: key.packed() & !u64::from(u32::MAX),
            })
        });
        Ok(())
    }

    fn try_insert_guarded(
        &mut self,
        key: StandardBagProductMemoKey,
        value: u32,
        admit: &mut impl FnMut(usize, u128) -> bool,
    ) -> Result<Option<u32>, ProductMemoAdmissionError> {
        let index = self
            .row_index(key)
            .ok_or(ProductMemoAdmissionError::OutOfScope)?;
        let directory_future = if self.rows.is_empty() {
            (self.row_count() as u128)
                .checked_mul(core::mem::size_of::<Option<StateMajorRow>>() as u128)
                .ok_or(ProductMemoAdmissionError::Capacity)?
        } else {
            0
        };
        let row = self.rows.get(index).and_then(Option::as_ref);
        let needs_row = row.is_none();
        let row_future = checked_hash_reserve_future_bytes(
            row.map_or(0, StateMajorRow::capacity),
            row.map_or(0, StateMajorRow::len),
            1,
            core::mem::size_of::<(u32, u32)>(),
        )
        .ok_or(ProductMemoAdmissionError::Capacity)?;
        let future = directory_future
            .checked_add(row_future)
            .ok_or(ProductMemoAdmissionError::Capacity)?;
        let retained = self
            .retained_payload_bytes()
            .saturating_add(self.directory_retained_bytes());
        if !admit(retained, future) {
            return Err(ProductMemoAdmissionError::Capacity);
        }
        if needs_row {
            self.prepare_row(index, key)?;
        }
        let row = self.rows[index]
            .as_mut()
            .ok_or(ProductMemoAdmissionError::Capacity)?;
        if row_future != 0 {
            let previous_capacity = row.capacity();
            let reserved = row.try_reserve(1);
            // Only growth can change capacity. Preserve exact accounting even
            // if reserve fails; ordinary inserts reuse the admitted allocation.
            self.row_capacity = self.row_capacity - previous_capacity + row.capacity();
            reserved.map_err(|_| ProductMemoAdmissionError::Capacity)?;
        }
        let previous = row.insert(key.language_node, value);
        if previous.is_none() {
            self.entry_count += 1;
        }
        Ok(previous)
    }

    #[cfg(test)]
    #[inline]
    fn mutate_row<T>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut StateMajorRow) -> T,
    ) -> Option<T> {
        let row = self.rows.get_mut(index)?.as_mut()?;
        let previous_capacity = row.capacity();
        let result = f(row);
        // Reserve can fail; measure the actual post-call capacity either way.
        // Each capacity is backed by a disjoint allocation, so their sum
        // cannot overflow the address space.
        self.row_capacity = self.row_capacity - previous_capacity + row.capacity();
        Some(result)
    }

    fn directory_retained_bytes(&self) -> usize {
        self.rows
            .capacity()
            .saturating_mul(core::mem::size_of::<Option<StateMajorRow>>())
    }

    fn copy_from(
        source: &ExactU64MemoMap,
        max_source_depth: u8,
    ) -> Result<Self, ProductMemoAdmissionError> {
        let mut target = Self::new(max_source_depth);
        let row_count = target.row_count();
        let mut counts = Vec::new();
        counts
            .try_reserve_exact(row_count)
            .map_err(|_| ProductMemoAdmissionError::Capacity)?;
        counts.resize(row_count, 0_usize);
        source.try_visit_entries(|packed, _| {
            let key = StandardBagProductMemoKey::from_packed(packed)
                .ok_or(ProductMemoAdmissionError::OutOfScope)?;
            let index = target
                .row_index(key)
                .ok_or(ProductMemoAdmissionError::OutOfScope)?;
            counts[index] += 1;
            Ok(())
        })?;
        target
            .rows
            .try_reserve_exact(row_count)
            .map_err(|_| ProductMemoAdmissionError::Capacity)?;
        target.rows.resize_with(row_count, || None);
        // Reserve each row once from its exact population, avoiding per-key
        // rehashing and insertion-order-dependent capacity during migration.
        for (index, count) in counts
            .into_iter()
            .enumerate()
            .filter(|(_, count)| *count != 0)
        {
            let hold = index % 8;
            let bag = (index / 8) % 128;
            let depth = index / (128 * 8);
            let prefix = (depth as u64) << 32 | (bag as u64) << 40 | (hold as u64) << 48;
            let mut row = StateMajorRow::with_hasher(StateMajorRowHasher {
                source_prefix: prefix,
            });
            row.try_reserve(count)
                .map_err(|_| ProductMemoAdmissionError::Capacity)?;
            target.row_capacity += row.capacity();
            target.rows[index] = Some(row);
        }
        source.try_visit_entries(|packed, value| {
            let key = StandardBagProductMemoKey::from_packed(packed)
                .ok_or(ProductMemoAdmissionError::OutOfScope)?;
            let index = target
                .row_index(key)
                .ok_or(ProductMemoAdmissionError::OutOfScope)?;
            let row = target.rows[index]
                .as_mut()
                .ok_or(ProductMemoAdmissionError::Capacity)?;
            // The exact row population was reserved above. Copying an entry
            // cannot grow the row or change its already accumulated capacity.
            debug_assert!(row.len() < row.capacity());
            row.insert(key.language_node, value);
            Ok(())
        })?;
        target.entry_count = source.len();
        Ok(target)
    }

    fn checked_copy_future_bytes(entries: usize, max_source_depth: u8) -> Option<u128> {
        let rows = (usize::from(max_source_depth) + 1).checked_mul(128 * 8)?;
        let directory_and_counts = (rows as u128).checked_mul(
            (core::mem::size_of::<Option<StateMajorRow>>() + core::mem::size_of::<usize>()) as u128,
        )?;
        // The sum of nonempty row populations is exactly entries. Applying
        // the per-row SwissTable upper bound is linear in that population;
        // there can be at most min(entries, rows) trailing control groups.
        directory_and_counts
            .checked_add(
                (entries as u128)
                    .checked_mul(4)?
                    .checked_mul(core::mem::size_of::<(u32, u32)>() as u128 + 1)?,
            )?
            .checked_add((entries.min(rows) as u128).checked_mul(16)?)
    }

    #[inline]
    fn row_index(&self, key: StandardBagProductMemoKey) -> Option<usize> {
        if key.depth > self.max_source_depth
            || key.bag_remainder > FULL_STANDARD_BAG
            || key.hold_code > 7
        {
            return None;
        }
        Some(
            (usize::from(key.depth) * 128 + usize::from(key.bag_remainder)) * 8
                + usize::from(key.hold_code),
        )
    }
}

fn adaptive_first_check_len(max_source_depth: u8, storage: ExactU64MemoStorage) -> usize {
    let directory_bytes = (usize::from(max_source_depth) + 1)
        * 128
        * 8
        * core::mem::size_of::<Option<StateMajorRow>>();
    let flat_slot_bytes = storage.slot_payload_bytes();
    let savings_per_entry = flat_slot_bytes.saturating_sub(core::mem::size_of::<(u32, u32)>());
    // Do not even allocate a row directory until twice its cost can amortize.
    // Works with pointer-width-dependent row sizes on native and WASM alike.
    directory_bytes
        .saturating_mul(2)
        .div_ceil(savings_per_entry.max(1))
        .max(1)
        .checked_next_power_of_two()
        .unwrap_or(usize::MAX)
}

/// Match the accepted flat hasher's full packed-key SplitMix64 input, although
/// only a u32 language reference is resident in each entry. The source prefix
/// lives once per row and its bytes belong to the directory payload.
#[derive(Clone, Copy)]
struct StateMajorRowHasher {
    source_prefix: u64,
}

impl BuildHasher for StateMajorRowHasher {
    type Hasher = LanguageNodeHasher;

    fn build_hasher(&self) -> Self::Hasher {
        LanguageNodeHasher {
            source_prefix: self.source_prefix,
            hash: 0,
        }
    }
}

struct LanguageNodeHasher {
    source_prefix: u64,
    hash: u64,
}

impl Hasher for LanguageNodeHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut value = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            value ^= u64::from(*byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.hash = mix_language_node(value);
    }

    fn write_u32(&mut self, value: u32) {
        self.hash = mix_language_node(self.source_prefix | u64::from(value));
    }
}

fn mix_language_node(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(max_source_depth: u8) -> StandardBagProductMemo {
        StandardBagProductMemo::new(
            StandardBagProductMemoLayout::StateMajor,
            ExactU64MemoStorage::Reference,
            max_source_depth,
        )
    }

    fn assert_exact_row_capacity(memo: &StandardBagProductMemo) {
        let ProductMemoStorage::StateMajor(state) = &memo.storage else {
            panic!("state-major capacity test requires a row directory");
        };
        let walked: usize = state
            .rows
            .iter()
            .flatten()
            .map(StateMajorRow::capacity)
            .sum();
        assert_eq!(memo.capacity(), walked);
        assert_eq!(
            memo.retained_payload_bytes(),
            walked * core::mem::size_of::<(u32, u32)>()
        );
    }

    #[test]
    fn v081_state_major_product_memo_matches_full_reference_keys_and_values() {
        let mut reference = StandardBagProductMemo::new(
            StandardBagProductMemoLayout::Flat,
            ExactU64MemoStorage::Reference,
            3,
        );
        let mut treatment = candidate(3);
        for depth in 0..=3 {
            for bag_remainder in 0..=FULL_STANDARD_BAG {
                for hold_code in 0..=7 {
                    for language_node in [0, 1, u32::MAX] {
                        let key = StandardBagProductMemoKey::new(
                            language_node,
                            depth,
                            bag_remainder,
                            hold_code,
                        );
                        for value in [0, u32::MAX, key.packed() as u32] {
                            assert_eq!(
                                reference.try_insert(key, value),
                                treatment.try_insert(key, value),
                            );
                            assert_eq!(reference.get(&key), treatment.get(&key));
                        }
                    }
                }
            }
        }
        assert_eq!(reference.len(), treatment.len());
        assert_eq!(treatment.active_rows(), 4 * 127 * 8);
        assert!(treatment.row_slots() >= 4 * 128 * 8);
        assert_eq!(
            treatment.retained_payload_bytes(),
            treatment.capacity() * core::mem::size_of::<(u32, u32)>(),
        );
        assert!(treatment.directory_retained_bytes() > 0);
        assert_eq!(reference.directory_retained_bytes(), 0);
    }

    #[test]
    fn v081_state_major_product_memo_normalization_bounds_and_full_depth_are_exact() {
        let mut memo = candidate(u8::MAX);
        let zero = StandardBagProductMemoKey::new(u32::MAX, u8::MAX, 0, 7);
        let full = StandardBagProductMemoKey::new(u32::MAX, u8::MAX, FULL_STANDARD_BAG, 7);
        assert_eq!(zero, full);
        assert_eq!(memo.try_insert(zero, u32::MAX), Ok(None));
        assert_eq!(memo.get(&full), Some(&u32::MAX));
        for key in [
            StandardBagProductMemoKey::new(u32::MAX, u8::MAX - 1, FULL_STANDARD_BAG, 7),
            StandardBagProductMemoKey::new(u32::MAX, u8::MAX, FULL_STANDARD_BAG - 1, 7),
            StandardBagProductMemoKey::new(u32::MAX, u8::MAX, FULL_STANDARD_BAG, 6),
            StandardBagProductMemoKey::new(u32::MAX - 1, u8::MAX, FULL_STANDARD_BAG, 7),
        ] {
            assert_eq!(memo.get(&key), None);
        }
        for key in [
            StandardBagProductMemoKey::new(0, 0, 0x80, 0),
            StandardBagProductMemoKey::new(0, 0, 1, 8),
        ] {
            assert_eq!(
                memo.try_insert(key, 3),
                Err(ProductMemoAdmissionError::OutOfScope)
            );
            assert_eq!(memo.get(&key), None);
        }
        let mut bounded = candidate(2);
        let outside = StandardBagProductMemoKey::new(0, 3, 1, 0);
        assert_eq!(
            bounded.try_insert(outside, 3),
            Err(ProductMemoAdmissionError::OutOfScope)
        );
        assert_eq!(bounded.directory_retained_bytes(), 0);
    }

    #[test]
    fn v081_state_major_product_memo_workers_epochs_and_failed_admission_stay_private() {
        let key = StandardBagProductMemoKey::new(37, 0, FULL_STANDARD_BAG, 0);
        let mut first = candidate(2);
        let mut second = candidate(2);
        assert_exact_row_capacity(&first);
        first.try_insert(key, 2).expect("first private root");
        second.try_insert(key, 3).expect("second private root");
        assert_exact_row_capacity(&first);
        let old_capacity = first.capacity();
        first
            .try_reserve_for(key, old_capacity + 1)
            .expect("grow an existing row");
        assert!(first.capacity() > old_capacity);
        assert_exact_row_capacity(&first);
        assert!(first.try_reserve_for(key, usize::MAX).is_err());
        // Failure on a new valid row must not charge unallocated capacity.
        let another_row = StandardBagProductMemoKey::new(37, 1, FULL_STANDARD_BAG, 0);
        assert!(first.try_reserve_for(another_row, usize::MAX).is_err());
        assert_exact_row_capacity(&first);
        assert_eq!(first.get(&key), Some(&2));
        assert_eq!(second.get(&key), Some(&3));
        let before = (
            first.capacity(),
            first.directory_retained_bytes(),
            first.allocated_rows(),
        );
        first.clear();
        assert_exact_row_capacity(&first);
        assert_eq!(first.len(), 0);
        assert_eq!(first.active_rows(), 0);
        assert_eq!(first.get(&key), None);
        assert_eq!(second.get(&key), Some(&3));
        assert_eq!(
            (
                first.capacity(),
                first.directory_retained_bytes(),
                first.allocated_rows()
            ),
            before
        );
        first.try_insert(key, u32::MAX).expect("new epoch root");
        assert_eq!(first.get(&key), Some(&u32::MAX));
        assert_exact_row_capacity(&first);
    }

    #[test]
    fn v081_state_major_product_memo_selector_defaults_to_adaptive_with_explicit_controls() {
        assert_eq!(
            StandardBagProductMemoLayout::from_label(None),
            Ok(StandardBagProductMemoLayout::Adaptive)
        );
        assert_eq!(
            StandardBagProductMemoLayout::from_label(Some("adaptive")),
            Ok(StandardBagProductMemoLayout::Adaptive)
        );
        assert_eq!(
            StandardBagProductMemoLayout::from_label(Some("flat")),
            Ok(StandardBagProductMemoLayout::Flat)
        );
        assert_eq!(
            StandardBagProductMemoLayout::from_label(Some("state-major")),
            Ok(StandardBagProductMemoLayout::StateMajor)
        );
        assert!(StandardBagProductMemoLayout::from_label(Some("compact")).is_err());
        assert!(StandardBagProductMemoLayout::from_environment_value(Err(
            std::env::VarError::NotUnicode(std::ffi::OsString::new())
        ))
        .is_err());
        for storage in [ExactU64MemoStorage::Reference, ExactU64MemoStorage::Compact] {
            let memo = StandardBagProductMemo::new(StandardBagProductMemoLayout::Flat, storage, 2);
            assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
            assert_eq!(memo.storage_label(), storage.label());
            assert_eq!(memo.directory_retained_bytes(), 0);
            assert_eq!(memo.row_slots(), 0);
        }
    }

    #[test]
    fn v081_state_major_product_memo_hash_matches_flat_full_packed_key() {
        use std::hash::Hash;
        for depth in [0, 3, u8::MAX] {
            for mask in [0, 1, FULL_STANDARD_BAG] {
                for hold_code in [0, 1, 7] {
                    for language_node in [0, 1, u32::MAX] {
                        let key =
                            StandardBagProductMemoKey::new(language_node, depth, mask, hold_code);
                        let mut hasher = StateMajorRowHasher {
                            source_prefix: key.packed() & !u64::from(u32::MAX),
                        }
                        .build_hasher();
                        language_node.hash(&mut hasher);
                        assert_eq!(hasher.finish(), mix_language_node(key.packed()));
                    }
                }
            }
        }
    }

    fn adaptive(max_source_depth: u8) -> StandardBagProductMemo {
        StandardBagProductMemo::new(
            StandardBagProductMemoLayout::Adaptive,
            ExactU64MemoStorage::Reference,
            max_source_depth,
        )
    }

    #[test]
    fn bounded_product_memo_rejects_growth_before_allocating_and_keeps_live_keys() {
        let first = StandardBagProductMemoKey::new(4, 0, FULL_STANDARD_BAG, 0);
        for layout in [
            StandardBagProductMemoLayout::Flat,
            StandardBagProductMemoLayout::StateMajor,
        ] {
            let mut memo = StandardBagProductMemo::new(layout, ExactU64MemoStorage::Compact, 1);
            assert_eq!(
                memo.try_insert_guarded(first, 19, |retained, future| {
                    assert_eq!(retained, 0);
                    assert!(future > 0);
                    false
                }),
                Err(ProductMemoAdmissionError::Capacity)
            );
            assert_eq!(memo.len(), 0);
            assert_eq!(memo.retained_bytes(), 0);
            memo.try_insert_guarded(first, 19, |_, _| true).unwrap();
            let before = memo.retained_bytes();
            // Existing capacity can cache another exact key with no growth.
            let second = StandardBagProductMemoKey::new(5, 0, FULL_STANDARD_BAG, 0);
            memo.try_insert_guarded(second, 23, |retained, future| {
                assert_eq!(retained, before);
                future == 0
            })
            .unwrap();
            assert_eq!(memo.get(&first), Some(&19));
            assert_eq!(memo.get(&second), Some(&23));
            assert_eq!(memo.retained_bytes(), before);
        }
    }

    #[test]
    fn state_major_reuses_capacity_but_keeps_admission_at_growth_and_recycle_boundaries() {
        let mut memo = candidate(1);
        let key_for = |language| StandardBagProductMemoKey::new(language, 1, 7, 3);
        memo.try_insert(key_for(0), 11)
            .expect("allocate the first row entry");
        let initial_capacity = memo.capacity();
        let retained = memo.retained_bytes();
        for language in 1..initial_capacity as u32 {
            memo.try_insert_guarded(key_for(language), language + 11, |bytes, future| {
                assert_eq!(bytes, retained);
                assert_eq!(future, 0);
                true
            })
            .expect("reuse the row's already admitted capacity");
        }
        assert_exact_row_capacity(&memo);
        let next = key_for(initial_capacity as u32);
        assert_eq!(
            memo.try_insert_guarded(next, 27, |bytes, future| {
                assert_eq!(bytes, retained);
                assert!(future > 0);
                false
            }),
            Err(ProductMemoAdmissionError::Capacity)
        );
        assert_eq!(memo.get(&next), None);
        assert_eq!(memo.len(), initial_capacity);
        assert_eq!(memo.retained_bytes(), retained);
        memo.try_insert(next, 27)
            .expect("grow the row after admitting its new allocation");
        assert!(memo.capacity() > initial_capacity);
        assert_exact_row_capacity(&memo);
        for language in 0..initial_capacity as u32 {
            assert_eq!(memo.get(&key_for(language)), Some(&(language + 11)));
        }
        let capacity = memo.capacity();
        let retained = memo.retained_bytes();
        for _ in 0..3 {
            memo.clear();
            memo.clear();
            assert_eq!(memo.len(), 0);
            assert_eq!(memo.get(&next), None);
            assert_eq!(memo.capacity(), capacity);
            assert_eq!(memo.retained_bytes(), retained);
            assert_exact_row_capacity(&memo);
            // Zero allocation does not bypass the caller's cache admission.
            assert_eq!(
                memo.try_insert_guarded(next, 31, |bytes, future| {
                    assert_eq!(bytes, retained);
                    assert_eq!(future, 0);
                    false
                }),
                Err(ProductMemoAdmissionError::Capacity)
            );
            assert_eq!(memo.get(&next), None);
            memo.try_insert(next, 31)
                .expect("reuse the cleared row in the new cache epoch");
            assert_eq!(memo.get(&next), Some(&31));
            assert_exact_row_capacity(&memo);
        }
    }

    #[test]
    fn bounded_product_memo_declines_promotion_before_copying_the_flat_owner() {
        let key = StandardBagProductMemoKey::new(0, 0, FULL_STANDARD_BAG, 0);
        let mut memo = adaptive(1);
        memo.adaptive.as_mut().unwrap().next_check_len = 1;
        let mut calls = 0;
        assert_eq!(
            memo.try_insert_guarded(key, 13, |retained, future| {
                calls += 1;
                if calls == 1 {
                    assert_eq!(retained, 0);
                    assert!(future > 0);
                    true
                } else {
                    assert!(retained > 0, "old flat owner is still charged");
                    assert!(
                        future >= StateMajorProductMemo::checked_copy_future_bytes(1, 1).unwrap()
                    );
                    false
                }
            }),
            Ok(None)
        );
        assert_eq!(calls, 2);
        assert_eq!(memo.get(&key), Some(&13));
        assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
        assert_eq!(memo.directory_retained_bytes(), 0);
        assert_eq!(memo.promotion_counts(), (1, 0));
    }

    #[test]
    fn v081_adaptive_product_memo_small_requests_have_no_directory_or_promotion() {
        let mut memo = adaptive(10);
        let threshold = memo.adaptive.as_ref().unwrap().next_check_len;
        assert_eq!(
            threshold,
            adaptive_first_check_len(10, ExactU64MemoStorage::Reference)
        );
        for language in 0..1024 {
            let key = StandardBagProductMemoKey::new(language, 2, 3, 4);
            assert_eq!(memo.try_insert(key, u32::MAX - language), Ok(None));
            assert_eq!(memo.get(&key), Some(&(u32::MAX - language)));
        }
        assert_eq!(memo.policy(), StandardBagProductMemoLayout::Adaptive);
        assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
        assert_eq!(memo.promotion_counts(), (0, 0));
        assert_eq!(memo.directory_retained_bytes(), 0);
        assert_eq!(memo.row_slots(), 0);
        memo.clear();
        assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
    }

    #[test]
    fn v081_adaptive_product_memo_migrates_all_exact_keys_then_recycles_without_oscillation() {
        for storage in [ExactU64MemoStorage::Reference, ExactU64MemoStorage::Compact] {
            let mut memo =
                StandardBagProductMemo::new(StandardBagProductMemoLayout::Adaptive, storage, 1);
            let mut reference =
                StandardBagProductMemo::new(StandardBagProductMemoLayout::Flat, storage, 1);
            let threshold = memo.adaptive.as_ref().unwrap().next_check_len;
            let key_for = |index: usize| {
                StandardBagProductMemoKey::new(
                    (index as u32).wrapping_add(u32::MAX / 2),
                    (index % 2) as u8,
                    if index % 3 == 0 {
                        0
                    } else {
                        FULL_STANDARD_BAG - 1
                    },
                    (index % 8) as u8,
                )
            };
            for index in 0..threshold {
                let key = key_for(index);
                assert_eq!(
                    memo.try_insert(key, index as u32),
                    reference.try_insert(key, index as u32)
                );
                if index + 1 < threshold {
                    assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
                }
            }
            assert_eq!(memo.layout(), StandardBagProductMemoLayout::StateMajor);
            assert_eq!(memo.promotion_counts(), (1, 1));
            assert_exact_row_capacity(&memo);
            assert_eq!(memo.len(), reference.len());
            assert!(
                memo.retained_payload_bytes() + memo.directory_retained_bytes()
                    <= reference.retained_payload_bytes() - reference.retained_payload_bytes() / 8
            );
            for index in 0..threshold {
                let key = key_for(index);
                assert_eq!(memo.get(&key), reference.get(&key));
                assert_eq!(
                    memo.try_insert(key, u32::MAX),
                    reference.try_insert(key, u32::MAX)
                );
            }
            assert_eq!(memo.promotion_counts(), (1, 1));
            assert_exact_row_capacity(&memo);
            let retained = (memo.capacity(), memo.directory_retained_bytes());
            memo.clear();
            assert_exact_row_capacity(&memo);
            assert_eq!(memo.len(), 0);
            assert_eq!(memo.get(&key_for(0)), None);
            assert_eq!(memo.layout(), StandardBagProductMemoLayout::StateMajor);
            assert_eq!((memo.capacity(), memo.directory_retained_bytes()), retained);
            assert_eq!(memo.try_insert(key_for(0), 7), Ok(None));
            assert_eq!(memo.get(&key_for(0)), Some(&7));
            assert_exact_row_capacity(&memo);
            // An epoch in a large request retains its proven layout; a NEW small
            // request does not inherit either its directory or worker-local roots.
            assert_eq!(adaptive(1).layout(), StandardBagProductMemoLayout::Flat);
        }
    }

    #[test]
    fn v081_adaptive_product_memo_cost_probe_is_geometric_and_failed_copy_keeps_flat() {
        let key = StandardBagProductMemoKey::new(0, 0, 0, 0);
        let mut cost_rejected = adaptive(0);
        cost_rejected.adaptive.as_mut().unwrap().next_check_len = 1;
        assert_eq!(cost_rejected.try_insert(key, 9), Ok(None));
        assert_eq!(cost_rejected.layout(), StandardBagProductMemoLayout::Flat);
        assert_eq!(cost_rejected.promotion_counts(), (1, 0));
        assert_eq!(cost_rejected.adaptive.as_ref().unwrap().next_check_len, 2);
        assert_eq!(cost_rejected.directory_retained_bytes(), 0);
        assert_eq!(cost_rejected.try_insert(key, 10), Ok(Some(9)));
        assert_eq!(cost_rejected.promotion_counts(), (1, 0));

        let mut failed = adaptive(0);
        failed.adaptive.as_mut().unwrap().next_check_len = 1;
        failed.fail_promotion = true;
        assert_eq!(failed.try_insert(key, u32::MAX), Ok(None));
        assert_eq!(failed.get(&key), Some(&u32::MAX));
        assert_eq!(failed.layout(), StandardBagProductMemoLayout::Flat);
        failed.fail_promotion = false;
        assert_eq!(
            failed.try_insert(StandardBagProductMemoKey::new(1, 0, 0, 0), 3),
            Ok(None)
        );
        assert_eq!(failed.promotion_counts(), (1, 0));
        assert_eq!(failed.directory_retained_bytes(), 0);
    }

    #[test]
    fn v081_adaptive_product_memo_out_of_scope_copy_preserves_full_flat_keys() {
        let mut memo = adaptive(0);
        memo.adaptive.as_mut().unwrap().next_check_len = 1;
        let outside = StandardBagProductMemoKey::new(u32::MAX, 1, 128, 8);
        assert_eq!(memo.try_insert(outside, u32::MAX), Ok(None));
        assert_eq!(memo.get(&outside), Some(&u32::MAX));
        assert_eq!(memo.layout(), StandardBagProductMemoLayout::Flat);
        assert_eq!(memo.promotion_counts(), (1, 0));
        let mut source = ExactU64MemoMap::new(ExactU64MemoStorage::Reference);
        source.insert(u64::MAX, u32::MAX);
        assert!(matches!(
            StateMajorProductMemo::copy_from(&source, u8::MAX),
            Err(ProductMemoAdmissionError::OutOfScope)
        ));
        assert_eq!(source.get(&u64::MAX), Some(&u32::MAX));
    }

    #[cfg(not(feature = "local-search-ab"))]
    #[test]
    fn v081_adaptive_product_memo_is_the_product_default_without_ab_selectors() {
        assert_eq!(
            StandardBagProductMemoLayout::from_environment(),
            Ok(StandardBagProductMemoLayout::Adaptive)
        );
    }
}
