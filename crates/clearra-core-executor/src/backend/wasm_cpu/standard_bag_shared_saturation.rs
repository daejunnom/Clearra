//! Same-request shared completion may retire only a Boolean coverage owner.
//! Local coverage remains the exact partial set actually proved by this worker.

use std::sync::Arc;

use super::super::parallel_coverage::SharedStandardBagCoverageComplete;
use super::{
    ExecutionControl, StandardBagCoverage, StandardBagRequestTables, WasmExactSearchError,
};

/// Private construction follows both the shared bitmap proof and the actual
/// request-table identity check. The Arc pins that identity through recycling.
struct BoundSharedSaturation {
    _tables: Arc<StandardBagRequestTables>,
    _original_pattern_count: usize,
}

pub(super) struct SharedSaturationState {
    authority: Option<BoundSharedSaturation>,
    early_publications: usize,
    adoptions: usize,
    retire_transitions: usize,
    #[cfg(any(test, feature = "local-search-ab"))]
    enabled: bool,
}

#[cfg(any(test, feature = "local-search-ab"))]
fn parse_shared_saturation(
    value: Result<String, std::env::VarError>,
) -> Result<bool, &'static str> {
    match value {
        Ok(value) if value == "0" => Ok(false),
        Ok(value) if value == "1" => Ok(true),
        Err(std::env::VarError::NotPresent) => Ok(true),
        _ => Err("wasm_standard_bag_shared_saturation_mode_invalid"),
    }
}

impl SharedSaturationState {
    pub(super) fn from_environment() -> Result<Self, WasmExactSearchError> {
        #[cfg(feature = "local-search-ab")]
        let enabled =
            parse_shared_saturation(std::env::var("CLEARRA_STANDARD_BAG_SHARED_SATURATION"))
                .map_err(WasmExactSearchError::InvalidProblem)?;
        Ok(Self {
            authority: None,
            early_publications: 0,
            adoptions: 0,
            retire_transitions: 0,
            #[cfg(feature = "local-search-ab")]
            enabled,
            #[cfg(all(test, not(feature = "local-search-ab")))]
            enabled: true,
        })
    }

    pub(super) const fn accounting(&self) -> (usize, usize, usize) {
        (
            self.early_publications,
            self.adoptions,
            self.retire_transitions,
        )
    }
}

impl StandardBagCoverage {
    pub const fn shared_saturation_enabled(&self) -> bool {
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.shared_saturation.enabled
        }
        #[cfg(not(any(test, feature = "local-search-ab")))]
        {
            true
        }
    }

    #[cfg(test)]
    pub fn set_shared_saturation_enabled_for_test(&mut self, enabled: bool) {
        self.shared_saturation.enabled = enabled;
    }

    pub fn has_shared_boolean_saturation(&self) -> bool {
        self.shared_saturation.authority.is_some()
    }

    pub fn needs_early_shared_publication(&self) -> bool {
        self.boolean_membership_enabled()
            && self.shared_saturation_enabled()
            && self.shared_saturation.early_publications == 0
            && self.materialized_pattern_count != 0
            && self.can_use_first_pattern_witness()
    }

    pub fn record_early_shared_publication(&mut self) {
        self.shared_saturation.early_publications =
            self.shared_saturation.early_publications.saturating_add(1);
    }

    pub(super) fn record_shared_retirement(&mut self) {
        self.shared_saturation.retire_transitions =
            self.shared_saturation.retire_transitions.saturating_add(1);
    }

    /// Caller has finished the previous candidate's consumers and is in the
    /// immutable Summary/Boolean-only request scope. A count match alone is
    /// insufficient: independent requests allocate distinct table owners.
    pub fn adopt_shared_boolean_saturation(
        &mut self,
        proof: &SharedStandardBagCoverageComplete<'_>,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        if !self.boolean_membership_enabled() || !self.shared_saturation_enabled() {
            return Ok(false);
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.has_shared_boolean_saturation() || self.global_is_complete() {
            return Ok(false);
        }
        if self.materialized_pattern_count == 0
            || self.materialized_pattern_count > u32::MAX as usize
        {
            return Ok(false);
        }
        let snapshot =
            proof
                .request()
                .snapshot
                .get()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_shared_saturation_request_mismatch",
                ))?;
        let tables = snapshot.tables.as_ref().ok().and_then(Option::as_ref);
        if proof.pattern_count() != self.materialized_pattern_count
            || snapshot.identity.pattern_count != self.materialized_pattern_count
            || !tables.is_some_and(|tables| Arc::ptr_eq(tables, &self.tables))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_shared_saturation_request_mismatch",
            ));
        }
        let span = crate::performance::SearchStageSpan::begin(
            crate::performance::ExecutorSearchStage::WasmSharedCoverageRetire,
        );
        // Flush before storing the authority. Failure keeps full-root meaning
        // intact and cannot turn cancellation or allocation failure into a miss.
        self.flush_epoch_global()?;
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        self.shared_saturation.authority = Some(BoundSharedSaturation {
            _tables: Arc::clone(&self.tables),
            _original_pattern_count: self.materialized_pattern_count,
        });
        self.enter_boolean_membership_mode()?;
        self.shared_saturation.adoptions = self.shared_saturation.adoptions.saturating_add(1);
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        span.finish(1);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "standard_bag_shared_saturation_tests.rs"]
mod tests;
