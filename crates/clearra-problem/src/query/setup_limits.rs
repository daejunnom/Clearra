use clearra_profiles::search::search_defaults::SearchDefaults;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetupLimits {
    max_shape_families: usize,
    max_tiling_variants_per_family: usize,
    max_build_variants_per_tiling: usize,
    max_results: usize,
    max_patterns: usize,
    post_pc_retained_trace_limit: usize,
}

impl SetupLimits {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_shape_families: usize,
        max_tiling_variants_per_family: usize,
        max_build_variants_per_tiling: usize,
        max_results: usize,
        max_patterns: usize,
        post_pc_retained_trace_limit: usize,
    ) -> Result<Self, SetupLimitsError> {
        if max_shape_families == 0 {
            return Err(SetupLimitsError::ZeroMaxShapeFamilies);
        }
        if max_tiling_variants_per_family == 0 {
            return Err(SetupLimitsError::ZeroMaxTilingVariantsPerFamily);
        }
        if max_build_variants_per_tiling == 0 {
            return Err(SetupLimitsError::ZeroMaxBuildVariantsPerTiling);
        }
        if max_results == 0 {
            return Err(SetupLimitsError::ZeroMaxResults);
        }
        if max_patterns == 0 {
            return Err(SetupLimitsError::ZeroMaxPatterns);
        }
        if post_pc_retained_trace_limit == 0 {
            return Err(SetupLimitsError::ZeroPostPcRetainedTraceLimit);
        }

        Ok(Self {
            max_shape_families,
            max_tiling_variants_per_family,
            max_build_variants_per_tiling,
            max_results,
            max_patterns,
            post_pc_retained_trace_limit,
        })
    }
}
impl SetupLimits {
    pub fn max_shape_families(self) -> usize {
        self.max_shape_families
    }
}
impl SetupLimits {
    pub fn max_tiling_variants_per_family(self) -> usize {
        self.max_tiling_variants_per_family
    }
}
impl SetupLimits {
    pub fn max_build_variants_per_tiling(self) -> usize {
        self.max_build_variants_per_tiling
    }
}
impl SetupLimits {
    pub fn max_results(self) -> usize {
        self.max_results
    }
}
impl SetupLimits {
    pub fn max_patterns(self) -> usize {
        self.max_patterns
    }
}
impl SetupLimits {
    pub fn post_pc_retained_trace_limit(self) -> usize {
        self.post_pc_retained_trace_limit
    }
}

impl Default for SetupLimits {
    fn default() -> Self {
        SearchDefaults::MVP1.into()
    }
}

impl From<SearchDefaults> for SetupLimits {
    fn from(defaults: SearchDefaults) -> Self {
        Self {
            max_shape_families: defaults.setup_max_shape_families(),
            max_tiling_variants_per_family: defaults.setup_max_tiling_variants_per_family(),
            max_build_variants_per_tiling: defaults.setup_max_build_variants_per_tiling(),
            max_results: defaults.setup_max_results(),
            max_patterns: defaults.setup_max_patterns(),
            post_pc_retained_trace_limit: defaults.scenario_retained_trace_limit(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupLimitsError {
    ZeroMaxShapeFamilies,
    ZeroMaxTilingVariantsPerFamily,
    ZeroMaxBuildVariantsPerTiling,
    ZeroMaxResults,
    ZeroMaxPatterns,
    ZeroPostPcRetainedTraceLimit,
}
