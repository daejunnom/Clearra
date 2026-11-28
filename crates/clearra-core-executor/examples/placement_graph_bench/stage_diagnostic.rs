//! Admission for diagnostic timing. Keep it separate from the uninstrumented
//! benchmark and from the strict-prefix experiment's different execution path.

pub(super) const CANCEL_DIAGNOSTIC_ENV: &str = "CLEARRA_SEARCH_DIAGNOSTIC_CANCEL_MS";
const MAX_CANCEL_DIAGNOSTIC_MS: u64 = 60_000;

/// A finite, explicitly requested diagnostic deadline. Missing is different
/// from zero; zero must not accidentally turn a bounded diagnostic unbounded.
pub(super) fn parse_cancel_diagnostic_ms(value: Option<&str>) -> Result<Option<u64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let millis = (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<u64>().ok())
        .flatten()
        .filter(|millis| (1..=MAX_CANCEL_DIAGNOSTIC_MS).contains(millis))
        .ok_or_else(|| {
            format!(
                "{CANCEL_DIAGNOSTIC_ENV} must be an integer from 1 to {MAX_CANCEL_DIAGNOSTIC_MS}"
            )
        })?;
    Ok(Some(millis))
}

pub(super) fn validate_cancel_diagnostic(
    cancel_ms: Option<u64>,
    diagnostic: Option<SearchStageDiagnostic>,
    summary_requested: bool,
    captures_coverage: bool,
) -> Result<(), String> {
    if cancel_ms.is_some()
        && (!summary_requested
            || captures_coverage
            || !diagnostic.is_some_and(SearchStageDiagnostic::is_product))
    {
        return Err(format!(
            "{CANCEL_DIAGNOSTIC_ENV} requires --summary and the ordinary source-bound CLEARRA_SEARCH_STAGE_PROFILE=1 route without coverage capture"
        ));
    }
    Ok(())
}

/// The timer owns only a cancellation handle. Stopping or dropping the guard
/// wakes and joins it, so a completed/failed diagnostic never leaves a timer
/// that can act on a later request. The search itself stays on its caller.
#[cfg(any(test, feature = "wasm-stage-profiling"))]
pub(super) struct DiagnosticCancellation {
    stop: Option<std::sync::mpsc::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
    fired: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(any(test, feature = "wasm-stage-profiling"))]
impl DiagnosticCancellation {
    pub(super) fn arm(
        control: &clearra_core_domain::execution_cancellation::ExecutionControl,
        millis: u64,
    ) -> Result<Self, String> {
        use std::sync::{atomic::Ordering, mpsc, Arc};

        if !(1..=MAX_CANCEL_DIAGNOSTIC_MS).contains(&millis) {
            return Err("diagnostic cancellation deadline is outside its finite range".into());
        }
        let handle = control.cancellation.handle();
        let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = Arc::clone(&fired);
        let (stop, receive) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("clearra-diagnostic-cancel".to_owned())
            .spawn(move || {
                if matches!(
                    receive.recv_timeout(std::time::Duration::from_millis(millis)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    observed.store(true, Ordering::Release);
                    handle.cancel();
                }
            })
            .map_err(|error| format!("cannot start diagnostic cancellation timer: {error}"))?;
        Ok(Self {
            stop: Some(stop),
            worker: Some(worker),
            fired,
        })
    }

    pub(super) fn finish(mut self) -> Result<bool, String> {
        self.stop_and_join()?;
        Ok(self.fired.load(std::sync::atomic::Ordering::Acquire))
    }

    fn stop_and_join(&mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "diagnostic cancellation timer panicked".to_owned())?;
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "wasm-stage-profiling"))]
impl Drop for DiagnosticCancellation {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchStageDiagnostic {
    ProductDefault,
    ProductCutoffExperiment,
    ProductRowStackExperiment {
        minimum_domain_cutoff: bool,
        apdp_parent_recheck: bool,
    },
    ProductApdpReference {
        minimum_domain_cutoff: bool,
    },
    Reference,
    OptimizedUpper,
    LegacyUpper,
}

impl SearchStageDiagnostic {
    pub(super) const fn is_product(self) -> bool {
        matches!(
            self,
            Self::ProductDefault
                | Self::ProductCutoffExperiment
                | Self::ProductRowStackExperiment { .. }
                | Self::ProductApdpReference { .. }
        )
    }

    /// These axes retain the ordinary verifier but change geometry work.
    /// Keep the selection explicit even when row-stack bypasses the domain
    /// where an enabled APDP reference would otherwise run.
    pub(super) const fn with_geometry_experiments(
        self,
        row_stack_geometry: bool,
        apdp_parent_recheck: bool,
    ) -> Self {
        if !self.is_product() {
            return self;
        }
        let minimum_domain_cutoff = self.minimum_domain_cutoff();
        if row_stack_geometry {
            Self::ProductRowStackExperiment {
                minimum_domain_cutoff,
                apdp_parent_recheck,
            }
        } else if apdp_parent_recheck {
            Self::ProductApdpReference {
                minimum_domain_cutoff,
            }
        } else if minimum_domain_cutoff {
            Self::ProductCutoffExperiment
        } else {
            Self::ProductDefault
        }
    }

    pub(super) const fn minimum_domain_cutoff(self) -> bool {
        match self {
            Self::ProductCutoffExperiment => true,
            Self::ProductRowStackExperiment {
                minimum_domain_cutoff,
                ..
            }
            | Self::ProductApdpReference {
                minimum_domain_cutoff,
            } => minimum_domain_cutoff,
            _ => false,
        }
    }

    pub(super) const fn row_stack_geometry(self) -> bool {
        matches!(self, Self::ProductRowStackExperiment { .. })
    }

    pub(super) const fn apdp_parent_recheck(self) -> bool {
        match self {
            Self::ProductApdpReference { .. } => true,
            Self::ProductRowStackExperiment {
                apdp_parent_recheck,
                ..
            } => apdp_parent_recheck,
            _ => false,
        }
    }

    pub(super) const fn execution_route(self) -> &'static str {
        match self {
            Self::ProductDefault | Self::ProductCutoffExperiment => "ordinary-product",
            Self::ProductRowStackExperiment { .. } => "row-stack-experiment",
            Self::ProductApdpReference { .. } => "ordinary-product-apdp-reference",
            _ => "strict-prefix-experiment",
        }
    }

    pub(super) const fn schema(diagnostic: Option<Self>) -> &'static str {
        match diagnostic {
            Some(_) => "clearra.placement-graph-stage-diagnostic.v1",
            None => "clearra.placement-graph-benchmark.v1",
        }
    }

    pub(super) fn select_strict(
        profile: bool,
        legacy_upper: bool,
        strict_arm: Option<bool>,
        captures_coverage: bool,
        profiler_available: bool,
    ) -> Result<Option<Self>, String> {
        if legacy_upper && !(profile && strict_arm == Some(true)) {
            return Err(
                "CLEARRA_STRICT_STAGE_LEGACY_UPPER requires CLEARRA_STRICT_STAGE_PROFILE=1 and CLEARRA_STRICT_PREFIX=1"
                    .into(),
            );
        }
        if !profile {
            return Ok(None);
        }
        let Some(arm) = strict_arm.filter(|_| !captures_coverage) else {
            return Err(
                "CLEARRA_STRICT_STAGE_PROFILE requires the global-only strict-prefix experiment"
                    .into(),
            );
        };
        if !profiler_available {
            return Err(
                "CLEARRA_STRICT_STAGE_PROFILE requires feature wasm-stage-profiling".into(),
            );
        }
        Ok(Some(match (arm, legacy_upper) {
            (false, _) => Self::Reference,
            (true, true) => Self::LegacyUpper,
            (true, false) => Self::OptimizedUpper,
        }))
    }

    pub(super) fn select_product(
        profile: bool,
        strict_diagnostic: Option<Self>,
        has_experiment: bool,
        captures_coverage: bool,
        profiler_available: bool,
        source_bound: bool,
        minimum_domain_cutoff: bool,
    ) -> Result<Option<Self>, String> {
        if !profile {
            return Ok(strict_diagnostic);
        }
        if strict_diagnostic.is_some() || has_experiment || captures_coverage {
            return Err(
                "CLEARRA_SEARCH_STAGE_PROFILE requires the ordinary global-only verifier; only labeled product optimizations, minimum-domain cutoff, row-stack geometry and APDP parent recheck may vary"
                    .into(),
            );
        }
        if !profiler_available {
            return Err(
                "CLEARRA_SEARCH_STAGE_PROFILE requires feature wasm-stage-profiling".into(),
            );
        }
        if !source_bound {
            return Err("CLEARRA_SEARCH_STAGE_PROFILE requires a compiled source commit".into());
        }
        Ok(Some(if minimum_domain_cutoff {
            Self::ProductCutoffExperiment
        } else {
            Self::ProductDefault
        }))
    }
}

/// The diagnostic binary enables local-search-ab, whose historical union
/// default differs from the product. Require the product selection explicitly;
/// never mutate process-wide environment while worker threads may exist.
pub(super) fn validate_product_memo_environment(
    mut read: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> Result<(), String> {
    for (name, expected, allow_unset) in [
        ("CLEARRA_STANDARD_BAG_MEMO", "reference", true),
        ("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT", "adaptive", true),
        ("CLEARRA_STANDARD_BAG_UNION_MEMO", "compact", false),
        ("CLEARRA_STANDARD_BAG_ABSORB", "0", true),
    ] {
        match read(name) {
            Ok(value) if value == expected => {}
            Err(std::env::VarError::NotPresent) if allow_unset => {}
            _ => {
                return Err(format!(
                    "product stage diagnostics require {name}={expected}{}",
                    if allow_unset {
                        " or unset"
                    } else {
                        " explicitly"
                    },
                ));
            }
        }
    }
    if !matches!(
        read("CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES"),
        Err(std::env::VarError::NotPresent)
    ) {
        return Err(
            "product stage diagnostics require CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES unset"
                .into(),
        );
    }
    Ok(())
}

/// Parallel worker accounting reports the effective layout after adaptive
/// promotion. A serial path has no such snapshot, and fixed queues may never
/// create StandardBag. Neither case proves that a memo allocation was used.
pub(super) fn validate_product_memo_observation(
    policy: Option<&str>,
    layout: Option<&str>,
    product_storage: Option<&str>,
    union_storage: Option<&str>,
) -> Result<(), String> {
    if [policy, layout, product_storage, union_storage]
        .into_iter()
        .all(|value| {
            matches!(
                value,
                None | Some("not-used" | "not-measured" | "not-applicable")
            )
        })
    {
        return Ok(());
    }
    if policy == Some("adaptive")
        && union_storage == Some("compact")
        && matches!(
            (layout, product_storage),
            (Some("flat"), Some("reference"))
                | (Some("state-major"), Some("state-major"))
                | (Some("mixed"), Some("mixed"))
        )
    {
        Ok(())
    } else {
        Err("product stage diagnostic observed a non-product StandardBag memo configuration".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_cancel_diagnostic_ms, validate_cancel_diagnostic, validate_product_memo_environment,
        validate_product_memo_observation, DiagnosticCancellation,
        SearchStageDiagnostic::{
            self, LegacyUpper, OptimizedUpper, ProductCutoffExperiment, ProductDefault, Reference,
        },
    };

    #[test]
    fn cancellation_diagnostic_deadline_is_strict_and_finite() {
        assert_eq!(parse_cancel_diagnostic_ms(None), Ok(None));
        for (input, expected) in [("1", 1), ("20000", 20_000), ("60000", 60_000)] {
            assert_eq!(parse_cancel_diagnostic_ms(Some(input)), Ok(Some(expected)));
        }
        for input in [
            "",
            "0",
            "60001",
            "-1",
            "+1",
            " 20",
            "20 ",
            "1.5",
            "NaN",
            "18446744073709551616",
        ] {
            assert!(
                parse_cancel_diagnostic_ms(Some(input)).is_err(),
                "accepted {input:?}"
            );
        }
    }

    #[test]
    fn cancellation_diagnostic_admission_requires_ordinary_profiled_summary() {
        assert!(validate_cancel_diagnostic(None, None, false, true).is_ok());
        for diagnostic in [
            ProductDefault,
            ProductCutoffExperiment,
            ProductDefault.with_geometry_experiments(true, false),
            ProductDefault.with_geometry_experiments(false, true),
        ] {
            assert!(
                validate_cancel_diagnostic(Some(20_000), Some(diagnostic), true, false).is_ok()
            );
            assert!(
                validate_cancel_diagnostic(Some(20_000), Some(diagnostic), false, false).is_err()
            );
            assert!(
                validate_cancel_diagnostic(Some(20_000), Some(diagnostic), true, true).is_err()
            );
        }
        for diagnostic in [
            None,
            Some(Reference),
            Some(OptimizedUpper),
            Some(LegacyUpper),
        ] {
            assert!(validate_cancel_diagnostic(Some(20_000), diagnostic, true, false).is_err());
        }
    }

    #[test]
    fn cancellation_timer_fires_and_joins_without_a_search() {
        let control = clearra_core_domain::execution_cancellation::ExecutionControl::default();
        let mut timer = DiagnosticCancellation::arm(&control, 1).unwrap();
        // Wait only for the finite one-millisecond timer, not a solver task.
        timer.worker.take().unwrap().join().unwrap();
        assert!(timer.finish().unwrap());
        assert!(control.is_cancelled());
    }

    #[test]
    fn cancellation_timer_stops_on_finish_and_drop() {
        let control = clearra_core_domain::execution_cancellation::ExecutionControl::default();
        assert!(!DiagnosticCancellation::arm(&control, 60_000)
            .unwrap()
            .finish()
            .unwrap());
        assert!(!control.is_cancelled());
        drop(DiagnosticCancellation::arm(&control, 60_000).unwrap());
        assert!(!control.is_cancelled());
        assert!(DiagnosticCancellation::arm(&control, 0).is_err());
        assert!(DiagnosticCancellation::arm(&control, 60_001).is_err());
    }

    #[test]
    fn strict_profiles_keep_their_execution_route_and_cannot_capture_proof_output() {
        let select = SearchStageDiagnostic::select_strict;
        assert_eq!(select(false, false, Some(true), true, false), Ok(None));
        assert_eq!(
            select(true, false, Some(false), false, true),
            Ok(Some(Reference))
        );
        assert_eq!(
            select(true, false, Some(true), false, true),
            Ok(Some(OptimizedUpper))
        );
        assert_eq!(
            select(true, true, Some(true), false, true),
            Ok(Some(LegacyUpper))
        );
        for (profile, legacy, arm, capture, available) in [
            (true, false, None, false, true),
            (true, false, Some(true), true, true),
            (true, false, Some(false), false, false),
            (false, true, Some(true), false, true),
            (true, true, Some(false), false, true),
        ] {
            assert!(select(profile, legacy, arm, capture, available).is_err());
        }
    }

    #[test]
    fn product_profile_requires_ordinary_source_bound_execution() {
        let select = SearchStageDiagnostic::select_product;
        assert_eq!(
            select(true, None, false, false, true, true, true),
            Ok(Some(ProductCutoffExperiment))
        );
        assert_eq!(
            select(true, None, false, false, true, true, false),
            Ok(Some(ProductDefault))
        );
        for (strict, experiment, capture, available, source) in [
            (Some(Reference), false, false, true, true),
            (Some(OptimizedUpper), false, false, true, true),
            (None, true, false, true, true),
            (None, false, true, true, true),
            (None, false, false, false, true),
            (None, false, false, true, false),
        ] {
            for cutoff in [false, true] {
                assert!(
                    select(true, strict, experiment, capture, available, source, cutoff).is_err()
                );
            }
        }
        assert_eq!(
            select(false, Some(LegacyUpper), true, false, true, true, true),
            Ok(Some(LegacyUpper))
        );
        assert_eq!(
            select(false, None, true, true, false, false, false),
            Ok(None)
        );
    }

    #[test]
    fn ordinary_verifier_geometry_axes_preserve_explicit_experiment_labels() {
        for cutoff in [false, true] {
            let base =
                SearchStageDiagnostic::select_product(true, None, false, false, true, true, cutoff)
                    .unwrap()
                    .unwrap();
            for row_stack in [false, true] {
                for apdp_recheck in [false, true] {
                    let diagnostic = base.with_geometry_experiments(row_stack, apdp_recheck);
                    assert!(diagnostic.is_product());
                    assert_eq!(diagnostic.minimum_domain_cutoff(), cutoff);
                    assert_eq!(diagnostic.row_stack_geometry(), row_stack);
                    assert_eq!(diagnostic.apdp_parent_recheck(), apdp_recheck);
                    assert_eq!(
                        diagnostic.execution_route(),
                        if row_stack {
                            "row-stack-experiment"
                        } else if apdp_recheck {
                            "ordinary-product-apdp-reference"
                        } else {
                            "ordinary-product"
                        },
                    );
                }
            }
        }
        for strict in [Reference, OptimizedUpper, LegacyUpper] {
            assert_eq!(strict.with_geometry_experiments(true, true), strict);
            assert!(!strict.is_product());
            assert_eq!(strict.execution_route(), "strict-prefix-experiment");
        }
    }

    #[test]
    fn product_memo_environment_requires_exact_defaults_without_mutation() {
        let validate = |entries: &[(&str, &str)]| {
            validate_product_memo_environment(|name| {
                entries
                    .iter()
                    .find_map(|(key, value)| (*key == name).then(|| (*value).to_owned()))
                    .ok_or(std::env::VarError::NotPresent)
            })
        };
        let compact_union = [("CLEARRA_STANDARD_BAG_UNION_MEMO", "compact")];
        assert!(validate(&compact_union).is_ok());
        assert!(validate(&[])
            .unwrap_err()
            .contains("UNION_MEMO=compact explicitly"));
        assert!(validate(&[
            ("CLEARRA_STANDARD_BAG_MEMO", "reference"),
            ("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT", "adaptive"),
            ("CLEARRA_STANDARD_BAG_UNION_MEMO", "compact"),
            ("CLEARRA_STANDARD_BAG_ABSORB", "0"),
        ])
        .is_ok());
        for (key, value) in [
            ("CLEARRA_STANDARD_BAG_MEMO", "compact"),
            ("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT", "flat"),
            ("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT", "state-major"),
            ("CLEARRA_STANDARD_BAG_UNION_MEMO", "reference"),
            ("CLEARRA_STANDARD_BAG_ABSORB", "1"),
            ("CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES", "0"),
            ("CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES", "4096"),
        ] {
            let mut entries = vec![(key, value)];
            if key != "CLEARRA_STANDARD_BAG_UNION_MEMO" {
                entries.extend(compact_union);
            }
            assert!(validate(&entries).is_err(), "must reject {key}={value}");
        }
        assert!(validate_product_memo_environment(|_| {
            Err(std::env::VarError::NotUnicode(std::ffi::OsString::new()))
        })
        .is_err());
    }

    #[test]
    fn product_memo_observation_preserves_adaptive_and_unused_paths() {
        for absent in [
            None,
            Some("not-used"),
            Some("not-measured"),
            Some("not-applicable"),
        ] {
            assert!(validate_product_memo_observation(absent, absent, absent, absent).is_ok());
        }
        for (layout, storage) in [
            ("flat", "reference"),
            ("state-major", "state-major"),
            ("mixed", "mixed"),
        ] {
            assert!(validate_product_memo_observation(
                Some("adaptive"),
                Some(layout),
                Some(storage),
                Some("compact"),
            )
            .is_ok());
        }
        for (policy, layout, storage, union) in [
            (
                Some("adaptive"),
                Some("flat"),
                Some("reference"),
                Some("reference"),
            ),
            (
                Some("flat"),
                Some("flat"),
                Some("reference"),
                Some("compact"),
            ),
            (
                Some("adaptive"),
                Some("flat"),
                Some("compact"),
                Some("compact"),
            ),
            (
                Some("adaptive"),
                Some("not-used"),
                Some("reference"),
                Some("compact"),
            ),
            (None, None, None, Some("compact")),
        ] {
            assert!(validate_product_memo_observation(policy, layout, storage, union).is_err());
        }
    }

    #[test]
    fn every_profile_has_a_schema_that_timing_adoption_rejects() {
        for diagnostic in [
            ProductDefault,
            ProductCutoffExperiment,
            SearchStageDiagnostic::ProductRowStackExperiment {
                minimum_domain_cutoff: false,
                apdp_parent_recheck: false,
            },
            SearchStageDiagnostic::ProductApdpReference {
                minimum_domain_cutoff: false,
            },
            Reference,
            OptimizedUpper,
            LegacyUpper,
        ] {
            assert_ne!(
                SearchStageDiagnostic::schema(Some(diagnostic)),
                SearchStageDiagnostic::schema(None),
            );
        }
    }
}
