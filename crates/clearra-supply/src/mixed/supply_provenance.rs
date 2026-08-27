use crate::finite_allocation::{FiniteSupplyAllocationError, FiniteSupplyAllocationTransaction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyProvenance {
    supply_provenance_id: u64,
    bag_profile_id: String,
    piece_set_id: String,
    observed_window_id: Option<String>,
    bag_boundary_evidence: BagBoundaryEvidence,
    duplicate_witness: bool,
    ambiguity_report: bool,
}

impl SupplyProvenance {
    pub fn new(
        bag_profile_id: impl Into<String>,
        piece_set_id: impl Into<String>,
        observed_window_id: Option<String>,
        bag_boundary_evidence: BagBoundaryEvidence,
        duplicate_witness: bool,
        ambiguity_report: bool,
    ) -> Result<Self, SupplyProvenanceError> {
        let bag_profile_id = bag_profile_id.into();
        if bag_profile_id.trim().is_empty() {
            return Err(SupplyProvenanceError::EmptyBagProfileId);
        }
        let piece_set_id = piece_set_id.into();
        if piece_set_id.trim().is_empty() {
            return Err(SupplyProvenanceError::EmptyPieceSetId);
        }
        let supply_provenance_id = stable_supply_provenance_id(
            &bag_profile_id,
            &piece_set_id,
            observed_window_id.as_deref(),
            bag_boundary_evidence,
            duplicate_witness,
            ambiguity_report,
        );

        Ok(Self {
            supply_provenance_id,
            bag_profile_id,
            piece_set_id,
            observed_window_id,
            bag_boundary_evidence,
            duplicate_witness,
            ambiguity_report,
        })
    }

    pub(crate) fn checked_finite_provenance_id(
        bag_profile_id: &str,
        piece_set_id: &str,
        observed_window_id: Option<&str>,
        bag_boundary_evidence: BagBoundaryEvidence,
        duplicate_witness: bool,
        ambiguity_report: bool,
    ) -> Result<u64, SupplyProvenanceError> {
        if bag_profile_id.trim().is_empty() {
            return Err(SupplyProvenanceError::EmptyBagProfileId);
        }
        if piece_set_id.trim().is_empty() {
            return Err(SupplyProvenanceError::EmptyPieceSetId);
        }
        Ok(stable_supply_provenance_id(
            bag_profile_id,
            piece_set_id,
            observed_window_id,
            bag_boundary_evidence,
            duplicate_witness,
            ambiguity_report,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_validated_finite_parts(
        supply_provenance_id: u64,
        bag_profile_id: &str,
        piece_set_id: &str,
        observed_window_id: Option<&str>,
        bag_boundary_evidence: BagBoundaryEvidence,
        duplicate_witness: bool,
        ambiguity_report: bool,
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<Self, FiniteSupplyAllocationError> {
        let mut bag_profile = transaction.try_string_with_capacity(bag_profile_id.len())?;
        bag_profile.push_str(bag_profile_id);
        let mut piece_set = transaction.try_string_with_capacity(piece_set_id.len())?;
        piece_set.push_str(piece_set_id);
        let observed_window = match observed_window_id {
            Some(value) => {
                let mut duplicate = transaction.try_string_with_capacity(value.len())?;
                duplicate.push_str(value);
                Some(duplicate)
            }
            None => None,
        };

        Ok(Self {
            supply_provenance_id,
            bag_profile_id: bag_profile,
            piece_set_id: piece_set,
            observed_window_id: observed_window,
            bag_boundary_evidence,
            duplicate_witness,
            ambiguity_report,
        })
    }
}
impl SupplyProvenance {
    pub fn standard_7_bag() -> Self {
        Self::new(
            "standard-7-bag",
            "standard-tetrominoes",
            None,
            BagBoundaryEvidence::FixedBoundary,
            false,
            false,
        )
        .expect("standard provenance")
    }
}
impl SupplyProvenance {
    pub fn supply_provenance_id(&self) -> u64 {
        self.supply_provenance_id
    }
}
impl SupplyProvenance {
    pub fn bag_profile_id(&self) -> &str {
        &self.bag_profile_id
    }
}
impl SupplyProvenance {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl SupplyProvenance {
    pub fn observed_window_id(&self) -> Option<&str> {
        self.observed_window_id.as_deref()
    }
}
impl SupplyProvenance {
    pub fn bag_boundary_evidence(&self) -> BagBoundaryEvidence {
        self.bag_boundary_evidence
    }
}
impl SupplyProvenance {
    pub fn duplicate_witness(&self) -> bool {
        self.duplicate_witness
    }
}
impl SupplyProvenance {
    pub fn ambiguity_report(&self) -> bool {
        self.ambiguity_report
    }
}
impl SupplyProvenance {
    pub fn supply_provenance_in_cache_key(&self) -> u64 {
        self.supply_provenance_id
    }

    /// Returns only the heap payload retained by provenance strings, measured
    /// by `String` allocation capacity. The inline provenance owner is
    /// deliberately excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = checked_add_bytes(
            self.bag_profile_id.capacity() as u128,
            self.piece_set_id.capacity() as u128,
        )?;
        checked_add_bytes(
            bytes,
            self.observed_window_id
                .as_ref()
                .map_or(0, |value| value.capacity() as u128),
        )
    }
}

fn checked_add_bytes(left: u128, right: u128) -> Option<u128> {
    left.checked_add(right)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BagBoundaryEvidence {
    NotEvaluated,
    FixedBoundary,
    ObservedCompatible,
    ObservedAmbiguous,
    DuplicateRejected,
}

impl BagBoundaryEvidence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not-evaluated",
            Self::FixedBoundary => "fixed-boundary",
            Self::ObservedCompatible => "observed-compatible",
            Self::ObservedAmbiguous => "observed-ambiguous",
            Self::DuplicateRejected => "duplicate-rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupplyProvenanceError {
    EmptyBagProfileId,
    EmptyPieceSetId,
}

fn stable_supply_provenance_id(
    bag_profile_id: &str,
    piece_set_id: &str,
    observed_window_id: Option<&str>,
    bag_boundary_evidence: BagBoundaryEvidence,
    duplicate_witness: bool,
    ambiguity_report: bool,
) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for value in [
        "supply-provenance:v1",
        bag_profile_id,
        piece_set_id,
        observed_window_id.unwrap_or(""),
        bag_boundary_evidence.as_str(),
        if duplicate_witness {
            "duplicate"
        } else {
            "no-duplicate"
        },
        if ambiguity_report {
            "ambiguous"
        } else {
            "unambiguous"
        },
    ] {
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_provenance_in_cache_key() {
        let provenance = SupplyProvenance::new(
            "standard-7-bag",
            "standard-tetrominoes",
            Some("obs:IOT".to_owned()),
            BagBoundaryEvidence::ObservedAmbiguous,
            false,
            true,
        )
        .expect("provenance");

        assert_ne!(provenance.supply_provenance_in_cache_key(), 0);
        assert_eq!(provenance.observed_window_id(), Some("obs:IOT"));
        assert!(provenance.ambiguity_report());
    }

    #[test]
    fn retained_capacity_counts_each_owned_string_capacity() {
        let mut bag = String::with_capacity(64);
        bag.push_str("standard-7-bag");
        let bag_capacity = bag.capacity();
        let mut pieces = String::with_capacity(48);
        pieces.push_str("standard-tetrominoes");
        let pieces_capacity = pieces.capacity();
        let mut observed = String::with_capacity(32);
        observed.push_str("obs:IOT");
        let observed_capacity = observed.capacity();
        let provenance = SupplyProvenance::new(
            bag,
            pieces,
            Some(observed),
            BagBoundaryEvidence::ObservedCompatible,
            false,
            false,
        )
        .expect("provenance");

        assert_eq!(
            provenance.checked_retained_capacity_bytes(),
            (bag_capacity as u128)
                .checked_add(pieces_capacity as u128)
                .and_then(|bytes| bytes.checked_add(observed_capacity as u128))
        );
    }

    #[test]
    fn retained_capacity_addition_fails_closed_on_overflow() {
        assert_eq!(checked_add_bytes(u128::MAX, 1), None);
    }
}
