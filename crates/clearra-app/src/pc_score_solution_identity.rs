//! One exact score-field identity. Compact identities stay inline; extended
//! rows reference one immutable canonical-key dictionary, never one key per
//! pattern/winner and never a truncated Board64 placeholder.
use std::{cmp::Ordering, fmt, sync::Arc};

use clearra_core_domain::solution::{
    normalized_tiling_solution::{NormalizedTilingSolutionKey, StandardBoard64TilingIdentity},
    ExtendedTilingSolutionKey,
};

#[derive(Clone, Debug)]
pub struct PcScoreSolutionIdentity(Storage);

#[derive(Clone, Debug)]
enum Storage {
    Compact(StandardBoard64TilingIdentity),
    Extended {
        keys: Arc<Vec<String>>,
        index: usize,
    },
}

impl PcScoreSolutionIdentity {
    pub(crate) const fn compact(identity: StandardBoard64TilingIdentity) -> Self {
        Self(Storage::Compact(identity))
    }

    pub const fn standard_board64_identity(&self) -> Option<StandardBoard64TilingIdentity> {
        match self.0 {
            Storage::Compact(identity) => Some(identity),
            Storage::Extended { .. } => None,
        }
    }

    pub fn extended_canonical_key(&self) -> Option<&str> {
        match &self.0 {
            Storage::Compact(_) => None,
            Storage::Extended { keys, index } => Some(&keys[*index]),
        }
    }

    pub fn normalized_solution_key(&self) -> NormalizedTilingSolutionKey {
        match &self.0 {
            Storage::Compact(identity) => {
                NormalizedTilingSolutionKey::from_standard_board64_identity(*identity)
            }
            Storage::Extended { keys, index } => {
                NormalizedTilingSolutionKey::parse_canonical(&keys[*index])
                    .expect("private construction validates the immutable canonical key")
            }
        }
    }

    pub(crate) fn write_canonical(&self, output: &mut dyn fmt::Write) -> fmt::Result {
        match &self.0 {
            Storage::Compact(identity) => {
                let mut output = output;
                identity.write_canonical(&mut output)
            }
            Storage::Extended { keys, index } => output.write_str(&keys[*index]),
        }
    }

    pub(crate) fn matches_result_candidate(
        &self,
        result: &clearra_core_executor::CoreExecutionResult,
        index: usize,
    ) -> bool {
        match &self.0 {
            Storage::Compact(identity) => {
                result.full_height_scoring_execution_batch().is_none()
                    && result.normalized_solution_identities().get(index) == Some(identity)
            }
            Storage::Extended {
                keys,
                index: own_index,
            } => {
                result.full_height_scoring_execution_batch().is_some()
                    && *own_index == index
                    && result.normalized_solution_keys().get(index) == keys.get(index)
            }
        }
    }

    /// Count the single dictionary backing shared by these rows. Reject mixed
    /// extended owners instead of granting unaccounted allocation credit.
    pub(crate) fn checked_shared_retained_bytes<'a>(
        identities: impl IntoIterator<Item = &'a Self>,
    ) -> Option<u128> {
        let mut owner: Option<&Arc<Vec<String>>> = None;
        for identity in identities {
            if let Storage::Extended { keys, .. } = &identity.0 {
                if owner.is_some_and(|previous| !Arc::ptr_eq(previous, keys)) {
                    return None;
                }
                owner = Some(keys);
            }
        }
        owner.map_or(Some(0), |keys| checked_dictionary_bytes(keys))
    }
}

pub(crate) enum PcScoreIdentitySource {
    Compact(Vec<StandardBoard64TilingIdentity>),
    Extended(Arc<Vec<String>>),
}

/// The legacy compact renderer borrows its existing identity slice. It must
/// not clone the dictionary merely to use the same field-average reducer.
pub(crate) trait PcScoreIdentityLookup {
    fn identity_count(&self) -> usize;
    fn identity(&self, index: usize) -> Option<PcScoreSolutionIdentity>;
}

impl PcScoreIdentityLookup for [StandardBoard64TilingIdentity] {
    fn identity_count(&self) -> usize {
        self.len()
    }
    fn identity(&self, index: usize) -> Option<PcScoreSolutionIdentity> {
        self.get(index)
            .copied()
            .map(PcScoreSolutionIdentity::compact)
    }
}

impl PcScoreIdentityLookup for PcScoreIdentitySource {
    fn identity_count(&self) -> usize {
        Self::len(self)
    }
    fn identity(&self, index: usize) -> Option<PcScoreSolutionIdentity> {
        Self::identity(self, index)
    }
}

impl PcScoreIdentitySource {
    pub(crate) fn from_result(
        result: &clearra_core_executor::CoreExecutionResult,
        mut guard: impl FnMut(u128) -> Result<(), clearra_core_executor::CoreExecutionError>,
    ) -> Result<Self, clearra_core_executor::CoreExecutionError> {
        use clearra_core_executor::CoreExecutionError;
        let invalid = || CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_identity_source_invalid",
        };
        let overflow = || CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_identity_memory_projection_overflow",
        };
        let allocation = || CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_identity_allocation_failed",
        };
        if let Some(batch) = result.full_height_scoring_execution_batch() {
            if !result.normalized_solution_identities().is_empty() {
                return Err(invalid());
            }
            let source = result.normalized_solution_keys();
            if !source.windows(2).all(|pair| pair[0] < pair[1])
                || source.iter().any(|key| {
                    !ExtendedTilingSolutionKey::parse_canonical(key).is_ok_and(|key| {
                        key.height() == batch.height() && key.initial_board() == batch.initial()
                    })
                })
            {
                return Err(invalid());
            }
            let header =
                (core::mem::size_of::<Vec<String>>() + 2 * core::mem::size_of::<usize>()) as u128;
            let projected = (source.len() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)
                .and_then(|bytes| bytes.checked_add(header))
                .ok_or_else(overflow)?;
            let strings = source
                .iter()
                .try_fold(0_u128, |bytes, key| bytes.checked_add(key.len() as u128))
                .ok_or_else(overflow)?;
            guard(projected.checked_add(strings).ok_or_else(overflow)?)?;
            let mut keys = Vec::new();
            keys.try_reserve_exact(source.len())
                .map_err(|_| allocation())?;
            let mut bytes = (keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)
                .and_then(|bytes| bytes.checked_add(header))
                .ok_or_else(overflow)?;
            guard(bytes.checked_add(strings).ok_or_else(overflow)?)?;
            for key in source {
                guard(bytes.checked_add(key.len() as u128).ok_or_else(overflow)?)?;
                let mut owned = String::new();
                owned
                    .try_reserve_exact(key.len())
                    .map_err(|_| allocation())?;
                bytes = bytes
                    .checked_add(owned.capacity() as u128)
                    .ok_or_else(overflow)?;
                guard(bytes)?;
                owned.push_str(key);
                keys.push(owned);
            }
            Ok(Self::Extended(Arc::new(keys)))
        } else {
            let source = result.normalized_solution_identities();
            guard(
                (source.len() as u128)
                    .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
                    .ok_or_else(overflow)?,
            )?;
            let mut identities = Vec::new();
            identities
                .try_reserve_exact(source.len())
                .map_err(|_| allocation())?;
            guard(
                (identities.capacity() as u128)
                    .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
                    .ok_or_else(overflow)?,
            )?;
            identities.extend_from_slice(source);
            identities.sort_unstable();
            identities.dedup();
            Ok(Self::Compact(identities))
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Compact(values) => values.len(),
            Self::Extended(keys) => keys.len(),
        }
    }

    pub(crate) fn identity(&self, index: usize) -> Option<PcScoreSolutionIdentity> {
        match self {
            Self::Compact(values) => values
                .get(index)
                .copied()
                .map(PcScoreSolutionIdentity::compact),
            Self::Extended(keys) => keys.get(index).map(|_| {
                PcScoreSolutionIdentity(Storage::Extended {
                    keys: Arc::clone(keys),
                    index,
                })
            }),
        }
    }

    pub(crate) fn compact_identities(&self) -> Option<&[StandardBoard64TilingIdentity]> {
        match self {
            Self::Compact(values) => Some(values),
            Self::Extended(_) => None,
        }
    }

    pub(crate) fn checked_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(values) => (values.capacity() as u128)
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128),
            Self::Extended(keys) => checked_dictionary_bytes(keys),
        }
    }

    pub(crate) fn shared_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(_) => Some(0),
            Self::Extended(keys) => checked_dictionary_bytes(keys),
        }
    }
}

// Capacity, not length, is the memory authority for the immutable owner.
#[allow(clippy::ptr_arg)]
fn checked_dictionary_bytes(keys: &Vec<String>) -> Option<u128> {
    let outer = (keys.capacity() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)?
        .checked_add(
            (core::mem::size_of::<Vec<String>>() + 2 * core::mem::size_of::<usize>()) as u128,
        )?;
    keys.iter().try_fold(outer, |bytes, key| {
        bytes.checked_add(key.capacity() as u128)
    })
}

impl PartialEq for PcScoreSolutionIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for PcScoreSolutionIdentity {}
impl PartialOrd for PcScoreSolutionIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PcScoreSolutionIdentity {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.0, &other.0) {
            (Storage::Compact(left), Storage::Compact(right)) => left.cmp(right),
            (
                Storage::Extended {
                    keys: left,
                    index: a,
                },
                Storage::Extended {
                    keys: right,
                    index: b,
                },
            ) => left[*a].cmp(&right[*b]),
            (Storage::Compact(_), Storage::Extended { .. }) => Ordering::Less,
            (Storage::Extended { .. }, Storage::Compact(_)) => Ordering::Greater,
        }
    }
}

impl PartialEq<StandardBoard64TilingIdentity> for PcScoreSolutionIdentity {
    fn eq(&self, other: &StandardBoard64TilingIdentity) -> bool {
        self.standard_board64_identity() == Some(*other)
    }
}

impl From<StandardBoard64TilingIdentity> for PcScoreSolutionIdentity {
    fn from(value: StandardBoard64TilingIdentity) -> Self {
        Self::compact(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(height: u8, column: u16) -> String {
        use clearra_core_domain::board::standard_pc_board::Board256Mask;
        let cells = (0..4).fold(Board256Mask::EMPTY, |cells, row| {
            cells.union(Board256Mask::singleton(row * 10 + column).unwrap())
        });
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(cells);
        let hex = |mask: Board256Mask| {
            let [a, b, c, d] = mask.words();
            format!("{d:016x}{c:016x}{b:016x}{a:016x}")
        };
        let key = format!(
            "ctk2|height={height}|initial={}|placements=I:{}",
            hex(initial),
            hex(cells)
        );
        ExtendedTilingSolutionKey::parse_canonical(&key).unwrap();
        key
    }

    #[test]
    fn extended_winners_reference_one_dictionary_and_preserve_all_words() {
        let source = PcScoreIdentitySource::Extended(Arc::new(vec![key(24, 0), key(24, 1)]));
        let first = source.identity(0).unwrap();
        let repeated = source.identity(0).unwrap();
        let second = source.identity(1).unwrap();
        assert_eq!(first, repeated);
        assert!(first.standard_board64_identity().is_none());
        assert_eq!(
            first.normalized_solution_key().as_str(),
            first.extended_canonical_key().unwrap()
        );
        let parsed =
            ExtendedTilingSolutionKey::parse_canonical(first.extended_canonical_key().unwrap())
                .unwrap();
        assert_eq!(parsed.height(), 24);
        assert_ne!(parsed.initial_board().words()[3], 0);
        assert_eq!(
            PcScoreSolutionIdentity::checked_shared_retained_bytes([&first, &repeated, &second]),
            source.shared_retained_bytes()
        );
        let mut written = String::new();
        first.write_canonical(&mut written).unwrap();
        assert_eq!(written, first.extended_canonical_key().unwrap());
    }

    #[test]
    fn mixed_dictionary_owners_do_not_gain_unaccounted_memory_credit() {
        let left = PcScoreIdentitySource::Extended(Arc::new(vec![key(8, 0)]));
        let right = PcScoreIdentitySource::Extended(Arc::new(vec![key(8, 0)]));
        let first = left.identity(0).unwrap();
        let second = right.identity(0).unwrap();
        assert_eq!(
            first, second,
            "result equality is semantic, not pointer identity"
        );
        assert_eq!(
            PcScoreSolutionIdentity::checked_shared_retained_bytes([&first, &second]),
            None
        );
    }

    #[test]
    fn compact_identities_keep_their_original_canonical_format_and_order() {
        let left = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty()).unwrap();
        let right = StandardBoard64TilingIdentity::from_placements(1, std::iter::empty()).unwrap();
        let compact = PcScoreSolutionIdentity::from(left);
        assert_eq!(compact.standard_board64_identity(), Some(left));
        assert!(compact.extended_canonical_key().is_none());
        assert_eq!(
            compact.normalized_solution_key(),
            NormalizedTilingSolutionKey::from_standard_board64_identity(left)
        );
        assert_eq!(compact.cmp(&right.into()), left.cmp(&right));
        assert_eq!(
            PcScoreSolutionIdentity::checked_shared_retained_bytes([&compact]),
            Some(0)
        );
    }
}
