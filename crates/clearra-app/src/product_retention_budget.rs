//! Host-owned admission for result construction, distinct from search and wire limits.

use clearra_core_executor::CoreExecutionResult;

/// Bounds the live source plus result-projection owners of one result product.
/// This is not a promise that the entire search or all workers fit this budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductRetentionBudget {
    maximum_bytes: u64,
}

impl ProductRetentionBudget {
    pub const MAXIMUM_BYTES: u64 = 1024 * 1024 * 1024;

    pub const fn new(maximum_bytes: u64) -> Option<Self> {
        if maximum_bytes == 0 || maximum_bytes > Self::MAXIMUM_BYTES {
            None
        } else {
            Some(Self { maximum_bytes })
        }
    }

    pub const fn maximum_bytes(self) -> u64 {
        self.maximum_bytes
    }
}

pub(crate) fn result_product_memory_limit(
    result: &CoreExecutionResult,
    host: Option<ProductRetentionBudget>,
    portable_fallback: u128,
) -> Result<u128, &'static str> {
    let execution = match (
        result.field_occurrence_count("execution_max_memory_mib"),
        result.unique_field("execution_max_memory_mib"),
    ) {
        (0, None) | (1, Some("none")) => None,
        (1, Some(value)) => Some(
            value
                .parse::<u128>()
                .ok()
                .and_then(|mib| mib.checked_mul(1024 * 1024))
                .ok_or("product_memory_authority_invalid")?,
        ),
        _ => return Err("product_memory_authority_invalid"),
    };
    Ok(match (host, execution) {
        (Some(host), Some(execution)) => u128::from(host.maximum_bytes()).min(execution),
        (Some(host), None) => u128::from(host.maximum_bytes()),
        (None, Some(execution)) => execution,
        (None, None) => portable_fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_retention_budget_is_separate_and_preserves_tighter_execution_limits() {
        let host = ProductRetentionBudget::new(256 * 1024 * 1024);
        for (value, expected_mib) in [("none", 256), ("1", 1), ("64", 64), ("512", 256)] {
            let result = CoreExecutionResult::new(
                vec![("execution_max_memory_mib".to_owned(), value.to_owned())],
                Vec::new(),
            );
            assert_eq!(
                result_product_memory_limit(&result, host, 16 * 1024 * 1024),
                Ok(expected_mib * 1024_u128 * 1024)
            );
        }
        assert!(ProductRetentionBudget::new(0).is_none());
        assert!(ProductRetentionBudget::new(ProductRetentionBudget::MAXIMUM_BYTES + 1).is_none());
    }
}
