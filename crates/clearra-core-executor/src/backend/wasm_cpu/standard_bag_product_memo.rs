use super::compact_exact_u64_map::{ExactU64MemoMap, ExactU64MemoStorage};

#[cfg(any(feature = "local-search-ab", test))]
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
};

const FULL_STANDARD_BAG: u8 = 0x7f;
const _: () = assert!(core::mem::size_of::<(u32, u32)>() == 8);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum StandardBagProductMemoLayout {
    /// The accepted product representation and its existing compact A/B arm.
    #[default]
    Flat,
    #[cfg(any(feature = "local-search-ab", test))]
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
            Ok(Self::Flat)
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
            None | Some("flat") => Ok(Self::Flat),
            Some("state-major") => Ok(Self::StateMajor),
            Some(_) => Err("wasm_standard_bag_product_memo_layout_invalid"),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            #[cfg(any(feature = "local-search-ab", test))]
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProductMemoAdmissionError {
    Capacity,
    #[cfg(any(feature = "local-search-ab", test))]
    OutOfScope,
}

pub(super) struct StandardBagProductMemo {
    storage: ProductMemoStorage,
    #[cfg(test)]
    failed_admissions_remaining: usize,
}

enum ProductMemoStorage {
    Flat(ExactU64MemoMap),
    #[cfg(any(feature = "local-search-ab", test))]
    StateMajor(StateMajorProductMemo),
}

impl StandardBagProductMemo {
    pub fn new(
        layout: StandardBagProductMemoLayout,
        flat_storage: ExactU64MemoStorage,
        max_source_depth: u8,
    ) -> Self {
        #[cfg(not(any(feature = "local-search-ab", test)))]
        let _ = max_source_depth;
        Self {
            storage: match layout {
                StandardBagProductMemoLayout::Flat => {
                    ProductMemoStorage::Flat(ExactU64MemoMap::new(flat_storage))
                }
                #[cfg(any(feature = "local-search-ab", test))]
                StandardBagProductMemoLayout::StateMajor => {
                    ProductMemoStorage::StateMajor(StateMajorProductMemo {
                        max_source_depth,
                        rows: Vec::new(),
                        entry_count: 0,
                    })
                }
            },
            #[cfg(test)]
            failed_admissions_remaining: 0,
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn layout(&self) -> StandardBagProductMemoLayout {
        match &self.storage {
            ProductMemoStorage::Flat(_) => StandardBagProductMemoLayout::Flat,
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(_) => StandardBagProductMemoLayout::StateMajor,
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn storage_label(&self) -> &'static str {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.storage().label(),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(_) => "state-major",
        }
    }

    #[inline]
    pub fn get(&self, key: &StandardBagProductMemoKey) -> Option<&u32> {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.get(&key.packed()),
            #[cfg(any(feature = "local-search-ab", test))]
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
            #[cfg(any(feature = "local-search-ab", test))]
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
        #[cfg(test)]
        if self.failed_admissions_remaining != 0 {
            self.failed_admissions_remaining -= 1;
            return Err(ProductMemoAdmissionError::Capacity);
        }
        self.try_reserve_for(key, 1)?;
        match &mut self.storage {
            ProductMemoStorage::Flat(memo) => Ok(memo.insert(key.packed(), value)),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => {
                let row_index = memo
                    .row_index(key)
                    .ok_or(ProductMemoAdmissionError::OutOfScope)?;
                let row = memo.rows[row_index]
                    .as_mut()
                    .ok_or(ProductMemoAdmissionError::Capacity)?;
                let previous = row.insert(key.language_node, value);
                if previous.is_none() {
                    memo.entry_count += 1;
                }
                Ok(previous)
            }
        }
    }

    /// Deterministic admission-failure fixture, absent from every product and
    /// local A/B binary. It exercises exact fallback without inducing an OOM.
    #[cfg(test)]
    pub fn fail_admissions_for_test(&mut self, count: usize) {
        self.failed_admissions_remaining = count;
    }

    fn try_reserve_for(
        &mut self,
        key: StandardBagProductMemoKey,
        additional: usize,
    ) -> Result<(), ProductMemoAdmissionError> {
        #[cfg(not(any(feature = "local-search-ab", test)))]
        let _ = key;
        match &mut self.storage {
            ProductMemoStorage::Flat(memo) => memo
                .try_reserve(additional)
                .map_err(|_| ProductMemoAdmissionError::Capacity),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => {
                let row_index = memo
                    .row_index(key)
                    .ok_or(ProductMemoAdmissionError::OutOfScope)?;
                if memo.rows.is_empty() {
                    let row_count = usize::from(memo.max_source_depth)
                        .checked_add(1)
                        .and_then(|depths| depths.checked_mul(128 * 8))
                        .ok_or(ProductMemoAdmissionError::Capacity)?;
                    memo.rows
                        .try_reserve_exact(row_count)
                        .map_err(|_| ProductMemoAdmissionError::Capacity)?;
                    memo.rows.resize_with(row_count, || None);
                }
                memo.rows[row_index]
                    .get_or_insert_with(|| {
                        StateMajorRow::with_hasher(StateMajorRowHasher {
                            source_prefix: key.packed() & !u64::from(u32::MAX),
                        })
                    })
                    .try_reserve(additional)
                    .map_err(|_| ProductMemoAdmissionError::Capacity)
            }
        }
    }

    pub fn clear(&mut self) {
        match &mut self.storage {
            ProductMemoStorage::Flat(memo) => memo.clear(),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => {
                for row in memo.rows.iter_mut().flatten() {
                    row.clear();
                }
                memo.entry_count = 0;
            }
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.len(),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => memo.entry_count,
        }
    }

    /// Logical entry capacity payload, excluding row directory and hash control
    /// bytes. Directory payload is reported separately and charged exactly once.
    pub fn retained_payload_bytes(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.retained_payload_bytes(),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(_) => self
                .capacity()
                .saturating_mul(core::mem::size_of::<(u32, u32)>()),
        }
    }

    #[cfg(any(feature = "parallel", feature = "local-search-ab", test))]
    pub fn capacity(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(memo) => memo.capacity(),
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => memo
                .rows
                .iter()
                .flatten()
                .fold(0_usize, |total, row| total.saturating_add(row.capacity())),
        }
    }

    pub fn directory_retained_bytes(&self) -> usize {
        match &self.storage {
            ProductMemoStorage::Flat(_) => 0,
            #[cfg(any(feature = "local-search-ab", test))]
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
            #[cfg(any(feature = "local-search-ab", test))]
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
            #[cfg(any(feature = "local-search-ab", test))]
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
            #[cfg(any(feature = "local-search-ab", test))]
            ProductMemoStorage::StateMajor(memo) => memo.rows.capacity(),
        }
    }
}

#[cfg(any(feature = "local-search-ab", test))]
type StateMajorRow = HashMap<u32, u32, StateMajorRowHasher>;

#[cfg(any(feature = "local-search-ab", test))]
struct StateMajorProductMemo {
    max_source_depth: u8,
    rows: Vec<Option<StateMajorRow>>,
    entry_count: usize,
}

#[cfg(any(feature = "local-search-ab", test))]
impl StateMajorProductMemo {
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

/// Match the accepted flat hasher's full packed-key SplitMix64 input, although
/// only a u32 language reference is resident in each entry. The source prefix
/// lives once per row and its bytes belong to the directory payload.
#[cfg(any(feature = "local-search-ab", test))]
#[derive(Clone, Copy)]
struct StateMajorRowHasher {
    source_prefix: u64,
}

#[cfg(any(feature = "local-search-ab", test))]
impl BuildHasher for StateMajorRowHasher {
    type Hasher = LanguageNodeHasher;

    fn build_hasher(&self) -> Self::Hasher {
        LanguageNodeHasher {
            source_prefix: self.source_prefix,
            hash: 0,
        }
    }
}

#[cfg(any(feature = "local-search-ab", test))]
struct LanguageNodeHasher {
    source_prefix: u64,
    hash: u64,
}

#[cfg(any(feature = "local-search-ab", test))]
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

#[cfg(any(feature = "local-search-ab", test))]
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
        first.try_insert(key, 2).expect("first private root");
        second.try_insert(key, 3).expect("second private root");
        assert!(first.try_reserve_for(key, usize::MAX).is_err());
        assert_eq!(first.get(&key), Some(&2));
        assert_eq!(second.get(&key), Some(&3));
        let before = (
            first.capacity(),
            first.directory_retained_bytes(),
            first.allocated_rows(),
        );
        first.clear();
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
    }

    #[test]
    fn v081_state_major_product_memo_selector_preserves_reference_and_compact_defaults() {
        assert_eq!(
            StandardBagProductMemoLayout::from_label(None),
            Ok(StandardBagProductMemoLayout::Flat)
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
}
