//! Identity validation for the complete source dictionary consumed by the same
//! exact minimum-cover reducer. Four-word fields never acquire Board64 authority.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        ExtendedTilingSolutionKey, NormalizedTilingSolutionKey, NormalizedTilingSolutionSetHasher,
        StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{CoreExecutionResult, PcChanceCoverageEvidence};
use clearra_problem::SearchProblem;

pub(crate) enum PcMinimumCoverSourceIdentities {
    Compact(Vec<StandardBoard64TilingIdentity>),
    // Extended keys already have an owner in the portfolio dictionary. This
    // domain binding permits borrowing them without a second 60-placement copy.
    Extended { height: u8, initial: Board256Mask },
}

impl PcMinimumCoverSourceIdentities {
    pub(crate) fn validate(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        producer: &PcChanceCoverageEvidence,
        keys: &[String],
    ) -> Result<Self, &'static str> {
        let rows = result.normalized_solution_coverages();
        if result.normalized_solution_keys() != keys || rows.len() != keys.len() {
            return Err("pc minimals deferred source identity evidence count mismatch");
        }
        if problem.visible_height() <= 6 {
            if result.normalized_solution_identities().len() != keys.len()
                || result.solution_coverages().len() != keys.len()
            {
                return Err("pc minimals deferred source identity evidence count mismatch");
            }
            for (((key, row), identity), coverage) in keys
                .iter()
                .zip(rows)
                .zip(result.normalized_solution_identities())
                .zip(result.solution_coverages())
            {
                let parsed = NormalizedTilingSolutionKey::parse_canonical(key)
                    .map_err(|_| "pc minimals deferred source key is not canonical")?;
                if parsed.standard_board64_identity().ok() != Some(*identity)
                    || coverage.identity() != *identity
                    || row.covered_patterns() != coverage.covered_patterns()
                {
                    return Err(
                        "pc minimals deferred source identity and coverage evidence mismatch",
                    );
                }
            }
            return Ok(Self::Compact(
                result.normalized_solution_identities().to_vec(),
            ));
        }
        if !(7..=24).contains(&problem.visible_height())
            || !result.normalized_solution_identities().is_empty()
            || !result.solution_coverages().is_empty()
            || producer.rows().len() != keys.len()
            || !producer.matches_extended_minimum_source_keys(keys)
        {
            return Err("pc minimals extended source identity evidence count mismatch");
        }
        let domain = Self::Extended {
            height: problem.visible_height() as u8,
            initial: Board256Mask::from_words(problem.initial_board().occupied_words()),
        };
        for (index, ((key, row), evidence)) in
            keys.iter().zip(rows).zip(producer.rows()).enumerate()
        {
            let identity = domain.validate_extended_key(key)?;
            if Some(identity.placement_count()) != problem.exact_pieces()
                || evidence.candidate_id()
                    != (index as u64)
                        .checked_add(1)
                        .ok_or("pc minimals extended source candidate index overflow")?
                || evidence.coverage_bits() != row.covered_patterns()
            {
                return Err("pc minimals extended source identity and coverage evidence mismatch");
            }
        }
        Ok(domain)
    }

    pub(crate) fn compact(&self) -> Option<&[StandardBoard64TilingIdentity]> {
        match self {
            Self::Compact(identities) => Some(identities),
            Self::Extended { .. } => None,
        }
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(identities) => (identities.capacity() as u128)
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128),
            Self::Extended { .. } => Some(0),
        }
    }

    pub(crate) fn source_hash(&self, keys: &[String]) -> Result<String, &'static str> {
        match self {
            Self::Compact(identities) => {
                if identities.len() != keys.len() {
                    return Err("pc minimals source identity count mismatch");
                }
                Ok(
                    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                        identities,
                    ),
                )
            }
            Self::Extended { .. } => self.extended_hash(keys),
        }
    }

    pub(crate) fn selected_hash(
        &self,
        keys: &[String],
        compact_identities: &[StandardBoard64TilingIdentity],
    ) -> Result<String, &'static str> {
        match self {
            Self::Compact(_) => {
                if keys.len() != compact_identities.len() {
                    return Err("pc minimals selected identity count mismatch");
                }
                Ok(
                    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                        compact_identities,
                    ),
                )
            }
            Self::Extended { .. } => {
                if !compact_identities.is_empty() {
                    return Err("pc minimals extended source acquired compact authority");
                }
                self.extended_hash(keys)
            }
        }
    }

    fn extended_hash(&self, keys: &[String]) -> Result<String, &'static str> {
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        for key in keys {
            hasher.update_extended_canonical_key(self.validate_extended_key(key)?);
        }
        Ok(hasher.finish())
    }

    fn validate_extended_key<'a>(
        &self,
        key: &'a str,
    ) -> Result<ExtendedTilingSolutionKey<'a>, &'static str> {
        let Self::Extended { height, initial } = self else {
            return Err("pc minimals compact source acquired extended authority");
        };
        let identity = ExtendedTilingSolutionKey::parse_canonical(key)
            .map_err(|_| "pc minimals extended source key is not canonical")?;
        let full = Board256Mask::all_cells(u16::from(*height) * 10)
            .map_err(|_| "pc minimals extended source height invalid")?;
        if identity.height() != *height
            || identity.initial_board() != *initial
            || identity.placements().fold(*initial, |occupied, placement| {
                occupied.union(placement.cells())
            }) != full
        {
            return Err("pc minimals extended source field partition mismatch");
        }
        Ok(identity)
    }
}
