use clearra_coverage::pattern::pattern_bitset::{PatternBitSet, PatternBitSetError};

use super::{
    build_order_language::BuildOrderLanguage, hold_reachable_language::HoldReachableLanguage,
};

pub struct LanguageIntersection;

impl LanguageIntersection {
    pub fn non_empty(
        build_orders: &BuildOrderLanguage,
        hold_orders: &HoldReachableLanguage,
    ) -> bool {
        hold_orders
            .orders()
            .iter()
            .any(|order| build_orders.accepts_order(order))
    }
}

impl LanguageIntersection {
    pub fn coverage_bits_for_candidate(
        build_orders: &BuildOrderLanguage,
        hold_orders: &[HoldReachableLanguage],
        pattern_count: usize,
    ) -> Result<PatternBitSet, PatternBitSetError> {
        let mut coverage = PatternBitSet::new(pattern_count);
        for hold_language in hold_orders {
            if Self::non_empty(build_orders, hold_language) {
                coverage.insert(hold_language.pattern_id)?;
            }
        }
        Ok(coverage)
    }
}

#[cfg(test)]
#[path = "language_intersection_tests.rs"]
mod tests;
