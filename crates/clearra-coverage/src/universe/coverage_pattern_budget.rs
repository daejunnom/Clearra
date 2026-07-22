use crate::matrix::coverage_matrix_error::CoverageMatrixError;

pub const C_COVERAGE_DEFAULT_PATTERN_BUDGET: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoveragePatternBudget {
    max_pattern_count: Option<usize>,
}

impl CoveragePatternBudget {
    pub const fn product_unbounded() -> Self {
        Self {
            max_pattern_count: None,
        }
    }
}
impl CoveragePatternBudget {
    pub const fn c_bridge_default() -> Self {
        Self {
            max_pattern_count: Some(C_COVERAGE_DEFAULT_PATTERN_BUDGET),
        }
    }
}
impl CoveragePatternBudget {
    pub const fn custom(max_pattern_count: usize) -> Self {
        Self {
            max_pattern_count: Some(max_pattern_count),
        }
    }
}
impl CoveragePatternBudget {
    pub const fn max_pattern_count(self) -> Option<usize> {
        self.max_pattern_count
    }
}
impl CoveragePatternBudget {
    pub fn check(self, pattern_count: usize) -> Result<(), CoverageMatrixError> {
        if let Some(max_pattern_count) = self.max_pattern_count {
            if pattern_count > max_pattern_count {
                return Err(CoverageMatrixError::PatternBitSetCapacityExceeded {
                    pattern_count,
                    max_pattern_count,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_fixed_1024_limit_is_default_budget_not_product_invariant() {
        assert_eq!(
            CoveragePatternBudget::c_bridge_default().max_pattern_count(),
            Some(1024)
        );
        assert_eq!(
            CoveragePatternBudget::product_unbounded().check(4096),
            Ok(())
        );
    }
}
