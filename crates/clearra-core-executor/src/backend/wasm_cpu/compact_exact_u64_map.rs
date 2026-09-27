#[cfg(any(feature = "local-search-ab", test))]
use std::{collections::HashSet, hash::Hash};
use std::{
    collections::{HashMap, TryReserveError},
    hash::{BuildHasherDefault, Hasher},
};

#[cfg(any(feature = "local-search-ab", test))]
const COMPACT_SLOT_PAYLOAD_BYTES: usize = 12;

/// Identical hashing for the reference and compact exact memo tables. The
/// complete packed key is always compared; a hash never authorizes a hit.
#[derive(Default)]
struct ExactU64Hasher(u64);

impl Hasher for ExactU64Hasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut value = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            value ^= u64::from(*byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = splitmix64(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = splitmix64(value);
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Splitting the key removes tuple alignment padding without reducing its
/// precision. Fields remain naturally aligned; there are no packed/unsafe reads.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[cfg(any(feature = "local-search-ab", test))]
struct MemoEntry {
    key_low: u32,
    key_high: u32,
    value: u32,
}

#[cfg(any(feature = "local-search-ab", test))]
const _: () = assert!(core::mem::size_of::<MemoEntry>() == COMPACT_SLOT_PAYLOAD_BYTES);

#[cfg(any(feature = "local-search-ab", test))]
impl MemoEntry {
    const fn new(key: u64, value: u32) -> Self {
        Self {
            key_low: key as u32,
            key_high: (key >> 32) as u32,
            value,
        }
    }

    const fn key(self) -> u64 {
        self.key_low as u64 | (self.key_high as u64) << 32
    }
}

#[cfg(any(feature = "local-search-ab", test))]
impl Hash for MemoEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.key());
    }
}

#[cfg(any(feature = "local-search-ab", test))]
impl PartialEq for MemoEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key_low == other.key_low && self.key_high == other.key_high
    }
}

#[cfg(any(feature = "local-search-ab", test))]
impl Eq for MemoEntry {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ExactU64MemoStorage {
    /// Keep the current product representation until compact-storage A/B.
    #[default]
    Reference,
    #[cfg(any(feature = "local-search-ab", test))]
    Compact,
}

impl ExactU64MemoStorage {
    pub fn from_environment() -> Result<Self, &'static str> {
        #[cfg(feature = "local-search-ab")]
        {
            Self::from_environment_value(std::env::var("CLEARRA_STANDARD_BAG_MEMO"))
        }
        #[cfg(not(feature = "local-search-ab"))]
        {
            Ok(Self::Reference)
        }
    }

    #[cfg(any(feature = "local-search-ab", test))]
    fn from_environment_value(
        value: Result<String, std::env::VarError>,
    ) -> Result<Self, &'static str> {
        match value {
            Ok(label) => Self::from_label(Some(&label)),
            Err(std::env::VarError::NotPresent) => Self::from_label(None),
            Err(std::env::VarError::NotUnicode(_)) => Err("wasm_standard_bag_memo_storage_invalid"),
        }
    }

    #[cfg(any(feature = "local-search-ab", test))]
    fn from_label(label: Option<&str>) -> Result<Self, &'static str> {
        match label {
            Some("compact") => Ok(Self::Compact),
            None | Some("reference") => Ok(Self::Reference),
            Some(_) => Err("wasm_standard_bag_memo_storage_invalid"),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            #[cfg(any(feature = "local-search-ab", test))]
            Self::Compact => "compact",
        }
    }
}

/// Lossless storage-only candidate for worker-local product/union memo tables.
/// Both variants retain SwissTable growth/load semantics and useful cache
/// entries. No iterator is exposed: table placement cannot affect solution order.
pub(super) struct ExactU64MemoMap {
    storage: MemoStorage,
}

enum MemoStorage {
    Reference(HashMap<u64, u32, BuildHasherDefault<ExactU64Hasher>>),
    #[cfg(any(feature = "local-search-ab", test))]
    Compact(HashSet<MemoEntry, BuildHasherDefault<ExactU64Hasher>>),
}

impl ExactU64MemoMap {
    pub fn new(storage: ExactU64MemoStorage) -> Self {
        Self {
            storage: match storage {
                ExactU64MemoStorage::Reference => MemoStorage::Reference(HashMap::default()),
                #[cfg(any(feature = "local-search-ab", test))]
                ExactU64MemoStorage::Compact => MemoStorage::Compact(HashSet::default()),
            },
        }
    }

    pub const fn storage(&self) -> ExactU64MemoStorage {
        match &self.storage {
            MemoStorage::Reference(_) => ExactU64MemoStorage::Reference,
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(_) => ExactU64MemoStorage::Compact,
        }
    }

    #[inline]
    pub fn get(&self, key: &u64) -> Option<&u32> {
        match &self.storage {
            MemoStorage::Reference(entries) => entries.get(key),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries
                .get(&MemoEntry::new(*key, 0))
                .map(|entry| &entry.value),
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &u64) -> bool {
        self.get(key).is_some()
    }

    #[inline]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        match &mut self.storage {
            MemoStorage::Reference(entries) => entries.try_reserve(additional),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries.try_reserve(additional),
        }
    }

    #[inline]
    pub fn insert(&mut self, key: u64, value: u32) -> Option<u32> {
        match &mut self.storage {
            MemoStorage::Reference(entries) => entries.insert(key, value),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries
                .replace(MemoEntry::new(key, value))
                .map(|entry| entry.value),
        }
    }

    pub fn clear(&mut self) {
        match &mut self.storage {
            MemoStorage::Reference(entries) => entries.clear(),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries.clear(),
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            MemoStorage::Reference(entries) => entries.len(),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries.len(),
        }
    }

    pub fn capacity(&self) -> usize {
        match &self.storage {
            MemoStorage::Reference(entries) => entries.capacity(),
            #[cfg(any(feature = "local-search-ab", test))]
            MemoStorage::Compact(entries) => entries.capacity(),
        }
    }

    /// Logical capacity payload only, not allocator/SIMD control bytes or OS
    /// peak memory. Explicitly account for the reference tuple's padding.
    pub fn retained_payload_bytes(&self) -> usize {
        let slot_bytes = match self.storage() {
            ExactU64MemoStorage::Reference => core::mem::size_of::<(u64, u32)>(),
            #[cfg(any(feature = "local-search-ab", test))]
            ExactU64MemoStorage::Compact => COMPACT_SLOT_PAYLOAD_BYTES,
        };
        self.capacity().saturating_mul(slot_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_slots_preserve_full_keys_values_updates_and_recycling() {
        let mut reference = ExactU64MemoMap::new(ExactU64MemoStorage::Reference);
        let mut compact = ExactU64MemoMap::new(ExactU64MemoStorage::Compact);
        let mut keys = vec![0, u64::MAX, 1_u64 << 32, u32::MAX as u64];
        keys.extend((0..4096_u64).map(|index| splitmix64(index)));
        for (index, key) in keys.iter().copied().enumerate() {
            for value in [index as u32, u32::MAX.wrapping_sub(index as u32)] {
                reference.try_reserve(1).expect("reference reservation");
                compact.try_reserve(1).expect("compact reservation");
                assert_eq!(reference.insert(key, value), compact.insert(key, value));
                assert_eq!(reference.get(&key), compact.get(&key));
            }
        }
        assert_eq!(reference.len(), compact.len());
        assert_eq!(reference.capacity(), compact.capacity());
        assert_eq!(
            compact.get(&0xfeed_abcd_1234_5678),
            reference.get(&0xfeed_abcd_1234_5678)
        );
        assert_eq!(
            compact.retained_payload_bytes() * 4,
            reference.retained_payload_bytes() * 3
        );
        let capacity = compact.capacity();
        reference.clear();
        compact.clear();
        assert_eq!(compact.capacity(), capacity);
        assert_eq!(compact.len(), 0);
        for key in keys {
            assert!(!reference.contains_key(&key));
            assert!(!compact.contains_key(&key));
        }
        for key in [0, u64::MAX] {
            assert_eq!(
                reference.insert(key, u32::MAX),
                compact.insert(key, u32::MAX)
            );
        }
    }

    #[test]
    fn exact_key_hash_and_value_independence_match_reference() {
        for key in [0, u64::MAX, 0xffff_ffff, 0xffff_ffff_0000_0000] {
            let mut reference = ExactU64Hasher::default();
            key.hash(&mut reference);
            for value in [0, u32::MAX] {
                let mut compact = ExactU64Hasher::default();
                MemoEntry::new(key, value).hash(&mut compact);
                assert_eq!(reference.finish(), compact.finish());
                assert_eq!(MemoEntry::new(key, value), MemoEntry::new(key, 1));
            }
        }
        assert_ne!(MemoEntry::new(1, 0), MemoEntry::new(1_u64 << 32, 0));
    }

    #[test]
    fn reservation_overflow_keeps_existing_cache_entries() {
        for storage in [ExactU64MemoStorage::Reference, ExactU64MemoStorage::Compact] {
            let mut memo = ExactU64MemoMap::new(storage);
            memo.insert(u64::MAX, u32::MAX);
            assert!(memo.try_reserve(usize::MAX).is_err());
            assert_eq!(memo.get(&u64::MAX), Some(&u32::MAX));
        }
    }

    #[test]
    fn candidate_requires_explicit_opt_in() {
        assert_eq!(
            ExactU64MemoStorage::from_label(None),
            Ok(ExactU64MemoStorage::Reference)
        );
        assert_eq!(
            ExactU64MemoStorage::from_label(Some("reference")),
            Ok(ExactU64MemoStorage::Reference)
        );
        assert!(ExactU64MemoStorage::from_label(Some("invalid")).is_err());
        assert_eq!(
            ExactU64MemoStorage::from_label(Some("compact")),
            Ok(ExactU64MemoStorage::Compact)
        );
    }

    #[test]
    fn invalid_os_environment_value_is_rejected_without_global_environment_mutation() {
        assert_eq!(
            ExactU64MemoStorage::from_environment_value(Err(std::env::VarError::NotUnicode(
                std::ffi::OsString::new(),
            ))),
            Err("wasm_standard_bag_memo_storage_invalid"),
        );
        assert_eq!(
            ExactU64MemoStorage::from_environment_value(Err(std::env::VarError::NotPresent)),
            Ok(ExactU64MemoStorage::Reference),
        );
        assert_eq!(
            ExactU64MemoStorage::from_environment_value(Ok("compact".to_owned())),
            Ok(ExactU64MemoStorage::Compact),
        );
        assert_eq!(
            ExactU64MemoStorage::from_environment_value(Ok("invalid".to_owned())),
            Err("wasm_standard_bag_memo_storage_invalid"),
        );
    }
}
