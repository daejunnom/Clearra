use clearra_build_coverage::coverage::BuildCoverageResult;

#[derive(Clone, Debug, PartialEq)]
pub struct BuildCoverageSummarySchema {
    pattern_count: usize,
    covered_pattern_count: usize,
    probability: f64,
    packing_candidate_count: usize,
    build_variant_count: usize,
}

impl BuildCoverageSummarySchema {
    pub fn from_result(result: &BuildCoverageResult) -> Self {
        let covered = result.union_coverage().covered_patterns();
        Self {
            pattern_count: covered.pattern_count(),
            covered_pattern_count: covered.count_ones() as usize,
            probability: result.probability().get(),
            packing_candidate_count: 0,
            build_variant_count: 0,
        }
    }
}
impl BuildCoverageSummarySchema {
    pub fn empty() -> Self {
        Self {
            pattern_count: 0,
            covered_pattern_count: 0,
            probability: 0.0,
            packing_candidate_count: 0,
            build_variant_count: 0,
        }
    }
}
impl BuildCoverageSummarySchema {
    pub fn with_core_counts(
        mut self,
        packing_candidate_count: usize,
        build_variant_count: usize,
    ) -> Self {
        self.packing_candidate_count = packing_candidate_count;
        self.build_variant_count = build_variant_count;
        self
    }
}
impl BuildCoverageSummarySchema {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl BuildCoverageSummarySchema {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}
impl BuildCoverageSummarySchema {
    pub fn probability(&self) -> f64 {
        self.probability
    }
}
impl BuildCoverageSummarySchema {
    pub fn coverage_probability(&self) -> f64 {
        self.probability
    }
}
impl BuildCoverageSummarySchema {
    pub fn packing_candidate_count(&self) -> usize {
        self.packing_candidate_count
    }
}
impl BuildCoverageSummarySchema {
    pub fn build_variant_count(&self) -> usize {
        self.build_variant_count
    }
}
