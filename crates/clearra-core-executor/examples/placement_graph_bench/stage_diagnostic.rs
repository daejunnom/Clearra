//! Admission for diagnostic timing. Keep it separate from the uninstrumented
//! benchmark and from the strict-prefix experiment's different execution path.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchStageDiagnostic {
    ProductDefault,
    ProductCutoffExperiment,
    Reference,
    OptimizedUpper,
    LegacyUpper,
}

impl SearchStageDiagnostic {
    pub(super) const fn is_product(self) -> bool {
        matches!(self, Self::ProductDefault | Self::ProductCutoffExperiment)
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
                "CLEARRA_SEARCH_STAGE_PROFILE requires the ordinary global-only product route; only labeled product optimizations and the minimum-domain cutoff may vary"
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
        validate_product_memo_environment, validate_product_memo_observation,
        SearchStageDiagnostic::{
            self, LegacyUpper, OptimizedUpper, ProductCutoffExperiment, ProductDefault, Reference,
        },
    };

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
