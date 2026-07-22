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

#[derive(Clone, Debug, Eq, PartialEq)]
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
}
