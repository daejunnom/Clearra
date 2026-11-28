//! Finite same-binary A/B entrypoint for the placement-condition experiment.
//! Build with --release --features parallel,local-search-ab. Large runs require
//! the repository supervisor; A/B selection is fixed by the process environment.
//! Add wasm-stage-profiling and CLEARRA_SEARCH_STAGE_PROFILE=1 for the ordinary
//! product route, or CLEARRA_STRICT_STAGE_PROFILE=1 for strict-prefix diagnostics.
//! Product diagnostics also require CLEARRA_STANDARD_BAG_UNION_MEMO=compact;
//! minimum-domain cutoff remains opt-in (default 0); the visited-first and
//! complete-global-union optimizations default to 1 with independent off arms.
//! CLEARRA_ROW_STACK_GEOMETRY=1 selects experimental geometry with the ordinary
//! verifier; CLEARRA_APDP_PARENT_RECHECK=1 restores the redundant APDP reference.
//! Both axes default to 0 and are labeled separately from the product route.
//! Diagnostic reports cannot serve as uninstrumented timing adoption evidence.
//! With --summary and ordinary stage profiling, setting
//! CLEARRA_SEARCH_DIAGNOSTIC_CANCEL_MS to 1..60000 emits a separate, bounded
//! diagnostic report even when cooperative cancellation interrupts the search.

#[cfg(not(all(feature = "parallel", feature = "local-search-ab")))]
fn main() {
    eprintln!("placement_graph_bench requires features parallel,local-search-ab");
    std::process::exit(2);
}

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
fn main() {
    // The public CLI uses this stack contract too. Keeping the example small
    // avoids linking App/UI while executing the same Rust CPU core.
    let result = std::thread::Builder::new()
        .name("placement-graph-benchmark".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run::main)
        .expect("benchmark product thread")
        .join()
        .expect("benchmark product thread completed");
    if let Err(error) = result {
        eprintln!("placement_graph_bench: {error}");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
#[path = "placement_graph_bench/solution_coverage_compare.rs"]
mod solution_coverage_compare;
#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
#[path = "placement_graph_bench/solution_coverage_spool.rs"]
mod solution_coverage_spool;
#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
#[path = "placement_graph_bench/stage_diagnostic.rs"]
mod stage_diagnostic;

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
mod run {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs::{self, File, OpenOptions},
        io::{self, BufReader, BufWriter, Read, Write},
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        time::Instant,
    };

    use super::solution_coverage_compare::{
        self, CoverageComparison, CoverageUniverse, CoverageWriter, P7_INITIAL,
    };
    use super::solution_coverage_spool::{CoverageFragment, CoverageSpool, CoverageSpoolLimits};
    #[cfg(feature = "wasm-stage-profiling")]
    use super::stage_diagnostic::DiagnosticCancellation;
    use super::stage_diagnostic::{
        parse_cancel_diagnostic_ms, validate_cancel_diagnostic, validate_product_memo_environment,
        validate_product_memo_observation, SearchStageDiagnostic, CANCEL_DIAGNOSTIC_ENV,
    };

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
        runtime_cpu_capacity::CpuCapacity,
        solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };
    use clearra_core_executor::{
        backend::{
            with_catalog_guard_strict_parallel_prefix_experiment,
            with_legacy_strict_parallel_prefix_experiment, with_operation_mask_experiment,
            with_strict_parallel_prefix_experiment, CapturedSolutionCoverage,
            GeometricEdgeReuseMetrics, GeometryDomainExperimentTotals,
            OperationMaskExperimentReport, StrictParallelPrefixReport,
        },
        CoreExecutionResult, WasmCpuSearchBackend,
    };
    #[cfg(feature = "wasm-stage-profiling")]
    use clearra_core_executor::{
        ExecutorSearchProfileSession, ExecutorSearchProfileStage, WasmCpuSearchError,
    };
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
        PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
        RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
    use clearra_rules::profile::builtin_rules::{jstris_180, srs, srs_plus};
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    const LARGE_COUNT: usize = 456_459;
    const LARGE_HASH: &str = "cts1:2fcbc950acdffa31";
    const P7_CANDIDATE_COUNT: usize = 2_260;
    const GIB: u64 = 1024 * 1024 * 1024;
    const STRICT_SPILL_WRITE_BYTES: u64 = 5 * GIB;
    const STRICT_FINAL_OUTPUT_BYTES: u64 = 3 * GIB;
    const LEGACY_FINAL_OUTPUT_BYTES: u64 = 8 * GIB;
    const EIGHT_I_INITIAL: u64 = 0xc030_0c03;
    const GROUNDED_O_CELLS: u64 = 0x0c03;
    const GROUNDED_O_INITIAL: u64 = 0x0f_ffff & !GROUNDED_O_CELLS;
    const EIGHT_I_CAPTURE_ERROR: &str =
        "--nonempty-eight-i supports global-only output; --coverage-slot is unavailable";
    const TEN_I_CAPTURE_ERROR: &str =
        "--empty-ten-i supports global-only output; --coverage-slot is unavailable";
    const GROUNDED_O_CAPTURE_ERROR: &str =
        "grounded two-line O fixtures use one fixed queue and support only global-only output";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Fixture {
        OneI,
        NonemptyEightI,
        EmptyTenI,
        GroundedOTwoLinesAll,
        GroundedOTwoLinesUnique,
        P7,
        P7P4,
    }

    impl Fixture {
        const fn name(self) -> &'static str {
            match self {
                Self::OneI => "one-I-preflight",
                Self::NonemptyEightI => "nonempty-4L-eight-I-jstris180",
                Self::EmptyTenI => "empty-4L-ten-I-jstris180",
                Self::GroundedOTwoLinesAll => "grounded-o-two-lines-countall-srs90",
                Self::GroundedOTwoLinesUnique => "grounded-o-two-lines-countunique-srs90",
                Self::P7 => "nonempty-4L-P7-srs-plus",
                Self::P7P4 => "empty-4L-P7P4-jstris180",
            }
        }

        const fn height(self) -> u16 {
            match self {
                Self::OneI => 1,
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => 2,
                Self::NonemptyEightI | Self::EmptyTenI | Self::P7 | Self::P7P4 => 4,
            }
        }

        const fn placed_pieces(self) -> usize {
            match self {
                Self::OneI | Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => 1,
                Self::NonemptyEightI => 8,
                Self::EmptyTenI | Self::P7P4 => 10,
                Self::P7 => 6,
            }
        }

        const fn fixed_board(self) -> Option<u64> {
            match self {
                Self::OneI => Some(0x3f0),
                Self::NonemptyEightI => Some(EIGHT_I_INITIAL),
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => {
                    Some(GROUNDED_O_INITIAL)
                }
                Self::EmptyTenI | Self::P7 | Self::P7P4 => None,
            }
        }

        fn input(self) -> Value {
            let (board, supply, supplied_pieces, original_supply_count, hold) = match self {
                Self::OneI => ("0x3f0", "I", 1, 1, "disabled"),
                Self::NonemptyEightI => ("0xc0300c03", "IIIIIIII", 8, 1, "disabled"),
                Self::EmptyTenI => ("0", "IIIIIIIIII", 10, 1, "disabled"),
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => {
                    ("0xff3fc", "O", 1, 1, "disabled")
                }
                Self::P7 => ("0x3c0f03c0f", "P7", 7, 5_040, "enabled-empty"),
                Self::P7P4 => ("0", "P7P4", 11, 4_233_600, "enabled-empty"),
            };
            let rule = match self {
                Self::P7 => "srs-plus",
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => "srs90",
                _ => "jstris-180",
            };
            json!({
                "board": board, "height": self.height(), "placed_pieces": self.placed_pieces(),
                "supply": supply, "supplied_pieces": supplied_pieces,
                "original_supply_count": original_supply_count, "hold": hold,
                "rule": rule,
                "count": if self.count_policy() == PcCountPolicy::CountAll { "all" } else { "unique" },
                "queue_knowledge": "full-future-oracle"
            })
        }

        fn capture_universe(self) -> Result<CoverageUniverse, String> {
            match self {
                Self::OneI => Ok(CoverageUniverse::SmallI),
                Self::P7 => Ok(CoverageUniverse::P7),
                Self::P7P4 => Ok(CoverageUniverse::P7P4),
                Self::NonemptyEightI => Err(EIGHT_I_CAPTURE_ERROR.into()),
                Self::EmptyTenI => Err(TEN_I_CAPTURE_ERROR.into()),
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique => {
                    Err(GROUNDED_O_CAPTURE_ERROR.into())
                }
            }
        }

        const fn is_grounded_o(self) -> bool {
            matches!(
                self,
                Self::GroundedOTwoLinesAll | Self::GroundedOTwoLinesUnique
            )
        }

        const fn count_policy(self) -> PcCountPolicy {
            match self {
                Self::GroundedOTwoLinesAll => PcCountPolicy::CountAll,
                _ => PcCountPolicy::CountUnique,
            }
        }
    }

    fn parse_fixture(args: &[String]) -> Result<(Fixture, Option<&str>), String> {
        match args {
            [] => Ok((Fixture::P7P4, None)),
            [flag] if flag == "--summary" => Ok((Fixture::P7P4, None)),
            [flag] if flag == "--small" => Ok((Fixture::OneI, None)),
            [flag] if flag == "--p7" => Ok((Fixture::P7, None)),
            [flag] if flag == "--nonempty-eight-i" => Ok((Fixture::NonemptyEightI, None)),
            [flag] if flag == "--empty-ten-i" => Ok((Fixture::EmptyTenI, None)),
            [flag] if flag == "--grounded-o-two-lines" => Ok((Fixture::GroundedOTwoLinesAll, None)),
            [flag] if flag == "--grounded-o-two-lines-unique" => {
                Ok((Fixture::GroundedOTwoLinesUnique, None))
            }
            [flag, slot] if flag == "--coverage-slot" => Ok((Fixture::P7P4, Some(slot.as_str()))),
            [small, flag, slot] if small == "--small" && flag == "--coverage-slot" => {
                Ok((Fixture::OneI, Some(slot.as_str())))
            }
            [p7, flag, slot] if p7 == "--p7" && flag == "--coverage-slot" => {
                Ok((Fixture::P7, Some(slot.as_str())))
            }
            [fixture, flag, _] if fixture == "--nonempty-eight-i" && flag == "--coverage-slot" => {
                Err(EIGHT_I_CAPTURE_ERROR.into())
            }
            [fixture, flag, _] if fixture == "--empty-ten-i" && flag == "--coverage-slot" => {
                Err(TEN_I_CAPTURE_ERROR.into())
            }
            [fixture, flag, _]
                if matches!(fixture.as_str(), "--grounded-o-two-lines" | "--grounded-o-two-lines-unique")
                    && flag == "--coverage-slot" =>
            {
                Err(GROUNDED_O_CAPTURE_ERROR.into())
            }
            _ => Err(
                "usage: placement_graph_bench [--summary | [--small | --p7] [--coverage-slot SLOT] | --nonempty-eight-i | --empty-ten-i | --grounded-o-two-lines | --grounded-o-two-lines-unique | --scalar-demo | --compare FILE FILE ... | --compare-coverage FILE FILE]"
                    .into(),
            ),
        }
    }

    fn validate_fixture_diagnostic_route(
        fixture: Fixture,
        diagnostic: Option<SearchStageDiagnostic>,
        strict_arm: Option<bool>,
        minimum_domain_cutoff: bool,
        geometric_edge_reuse_arm: Option<bool>,
        grounded_o_other_axes: bool,
    ) -> Result<(), String> {
        let product_diagnostic = diagnostic.is_some_and(SearchStageDiagnostic::is_product);
        if fixture == Fixture::P7 && strict_arm.is_none() && !product_diagnostic {
            return Err(
                "--p7 requires an explicit CLEARRA_STRICT_PREFIX arm or a product stage diagnostic"
                    .into(),
            );
        }
        if fixture.is_grounded_o() {
            if grounded_o_other_axes {
                return Err("grounded two-line O fixtures isolate geometric edge reuse".into());
            }
            if product_diagnostic && geometric_edge_reuse_arm == Some(true) {
                return Err("product stage diagnostics keep geometric edge reuse disabled".into());
            }
            // Plain fixture runs use the product cutoff default. Explicit
            // geometric-edge A/B arms retain the historical cutoff=0 baseline;
            // neither arm may silently acquire a second changed mechanism.
            if !product_diagnostic && geometric_edge_reuse_arm.is_some() && minimum_domain_cutoff {
                return Err(
                    "grounded geometric-edge reuse arms require CLEARRA_MINIMUM_DOMAIN_CUTOFF=0; omit CLEARRA_GEOMETRIC_EDGE_REUSE for a default fixture run"
                        .into(),
                );
            }
        }
        Ok(())
    }

    fn validate_row_stack_fixture(fixture: Fixture, row_stack: bool) -> Result<(), String> {
        if row_stack && fixture.is_grounded_o() {
            return Err(
                "row-stack geometry is not qualified for grounded O fixtures: they request per-solution probabilities, and CountAll is also outside the experimental scope"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn main() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|arg| arg == "--compare") {
            return compare(&args[1..]);
        }
        if let [flag, reference, candidate] = args.as_slice() {
            if flag == "--compare-coverage" {
                return compare_coverage(Path::new(reference), Path::new(candidate));
            }
        }
        if args.as_slice() == ["--scalar-demo"] {
            return scalar_demo();
        }
        let (fixture, coverage_slot) = parse_fixture(&args)?;
        let summary_requested = args.as_slice() == ["--summary"];
        let diagnostic_cancel_ms = match std::env::var(CANCEL_DIAGNOSTIC_ENV) {
            Ok(value) => parse_cancel_diagnostic_ms(Some(&value))?,
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(format!("{CANCEL_DIAGNOSTIC_ENV} must be a Unicode integer"));
            }
        };
        if ((fixture != Fixture::OneI && !fixture.is_grounded_o()) || coverage_slot.is_some())
            && (std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
                || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search"))
        {
            return Err(
                "P7/P7P4, four-row I rectangles and coverage capture require the supervised benchmark-search profile".into(),
            );
        }
        let mode = std::env::var("CLEARRA_PLACEMENT_DEPENDENCY").unwrap_or_else(|_| "0".into());
        if !matches!(mode.as_str(), "0" | "1") {
            return Err("CLEARRA_PLACEMENT_DEPENDENCY must be 0 or 1 at process start".into());
        }
        let structural_mode =
            std::env::var("CLEARRA_STRUCTURAL_ORDER").unwrap_or_else(|_| "0".into());
        if !matches!(structural_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_STRUCTURAL_ORDER must be 0 or 1 at process start".into());
        }
        let static_influence_mode =
            std::env::var("CLEARRA_STATIC_INFLUENCE").unwrap_or_else(|_| "0".into());
        if !matches!(static_influence_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_STATIC_INFLUENCE must be 0 or 1 at process start".into());
        }
        let translation_interval_mode =
            std::env::var("CLEARRA_TRANSLATION_INTERVAL").unwrap_or_else(|_| "0".into());
        if !matches!(translation_interval_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_TRANSLATION_INTERVAL must be 0 or 1 at process start".into());
        }
        let movement_clear_certificates_mode =
            experiment_mode("CLEARRA_MOVEMENT_CLEAR_CERTIFICATES")?;
        let incremental_domain_mode = experiment_mode("CLEARRA_INCREMENTAL_DOMAIN")?;
        let minimum_domain_cutoff_mode = experiment_mode("CLEARRA_MINIMUM_DOMAIN_CUTOFF")?;
        let minimum_domain_work_mode = experiment_mode("CLEARRA_MINIMUM_DOMAIN_WORK")?;
        let row_stack_geometry_mode = experiment_mode("CLEARRA_ROW_STACK_GEOMETRY")?;
        let apdp_parent_recheck_mode = experiment_mode("CLEARRA_APDP_PARENT_RECHECK")?;
        validate_row_stack_fixture(fixture, row_stack_geometry_mode == "1")?;
        let reachability_visited_first_mode =
            experiment_mode_with_default("CLEARRA_REACHABILITY_VISITED_FIRST", "1")?;
        let standard_bag_global_saturation_mode =
            experiment_mode_with_default("CLEARRA_STANDARD_BAG_GLOBAL_SATURATION", "1")?;
        let physical_prefix_pruning_mode =
            experiment_mode_with_default("CLEARRA_PHYSICAL_PREFIX_PRUNING", "1")?;
        let candidate_ilc_pruning_mode =
            experiment_mode_with_default("CLEARRA_CANDIDATE_ILC_PRUNING", "1")?;
        let standard_bag_boolean_membership_mode =
            experiment_mode_with_default("CLEARRA_STANDARD_BAG_BOOLEAN_MEMBERSHIP", "1")?;
        let standard_bag_shared_saturation_mode =
            experiment_mode_with_default("CLEARRA_STANDARD_BAG_SHARED_SATURATION", "1")?;
        let first_kick_interval_mode = experiment_mode("CLEARRA_FIRST_KICK_INTERVAL")?;
        let ordered_kick_interval_mode = experiment_mode("CLEARRA_ORDERED_KICK_INTERVAL")?;
        let ordered_kick_guard_reuse_mode = experiment_mode("CLEARRA_ORDERED_KICK_GUARD_REUSE")?;
        let geometric_edge_reuse_arm = optional_experiment_mode("CLEARRA_GEOMETRIC_EDGE_REUSE")?;
        let geometric_edge_reuse_mode = if geometric_edge_reuse_arm == Some(true) {
            "1"
        } else {
            "0"
        }
        .to_owned();
        let ilc_lazy_frame_mode = experiment_mode("CLEARRA_ILC_LAZY_FRAME")?;
        let no_entry_components_mode = experiment_mode("CLEARRA_NO_ENTRY_COMPONENTS")?;
        let same_tile_parent_reuse_mode = experiment_mode("CLEARRA_SAME_TILE_PARENT_REUSE")?;
        let conditional_order_mode = experiment_mode("CLEARRA_CONDITIONAL_ORDER")?;
        let terminal_identity_move_mode = experiment_mode("CLEARRA_TERMINAL_IDENTITY_MOVE")?;
        let operation_mask_mode = optional_experiment_mode("CLEARRA_OPERATION_MASKS")?;
        let strict_prefix_mode = optional_experiment_mode("CLEARRA_STRICT_PREFIX")?;
        if summary_requested && (strict_prefix_mode.is_some() || operation_mask_mode.is_some()) {
            return Err("--summary uses the ordinary product route without strict-prefix or operation-mask overrides".into());
        }
        let catalog_guard_mode = optional_experiment_mode("CLEARRA_STRICT_CATALOG_GUARDS")?;
        let frontier_mode = optional_experiment_mode("CLEARRA_STRICT_LANGUAGE_FRONTIER")?;
        let subsumption_mode = optional_experiment_mode("CLEARRA_STRICT_SUBSUMPTION")?;
        if [catalog_guard_mode, frontier_mode, subsumption_mode].contains(&Some(true))
            && strict_prefix_mode != Some(true)
        {
            return Err("strict optimization requires an enabled strict-prefix request".into());
        }
        let stage_diagnostic = SearchStageDiagnostic::select_strict(
            optional_experiment_mode("CLEARRA_STRICT_STAGE_PROFILE")? == Some(true),
            optional_experiment_mode("CLEARRA_STRICT_STAGE_LEGACY_UPPER")? == Some(true),
            strict_prefix_mode,
            coverage_slot.is_some(),
            cfg!(feature = "wasm-stage-profiling"),
        )?;
        if catalog_guard_mode == Some(true)
            && stage_diagnostic == Some(SearchStageDiagnostic::LegacyUpper)
        {
            return Err("catalog guards and legacy upper diagnostics cannot be combined".into());
        }
        let auxiliary_axis_enabled = [
            &mode,
            &structural_mode,
            &static_influence_mode,
            &translation_interval_mode,
            &movement_clear_certificates_mode,
            &incremental_domain_mode,
            &minimum_domain_work_mode,
            &first_kick_interval_mode,
            &ordered_kick_interval_mode,
            &ordered_kick_guard_reuse_mode,
            &geometric_edge_reuse_mode,
            &ilc_lazy_frame_mode,
            &no_entry_components_mode,
            &same_tile_parent_reuse_mode,
            &conditional_order_mode,
            &terminal_identity_move_mode,
        ]
        .into_iter()
        .any(|mode| mode != "0");
        // Geometry experiments retain their explicit all-zero reference arms.
        // The ordinary physical-path diagnostic can ablate the two product
        // optimizations independently. The minimum-support cutoff remains an
        // opt-in experiment after its measured end-to-end regression.
        let other_axis_enabled = auxiliary_axis_enabled
            || minimum_domain_cutoff_mode != "0"
            || row_stack_geometry_mode != "0"
            || apdp_parent_recheck_mode != "0";
        let product_optimization_enabled =
            reachability_visited_first_mode != "0" || standard_bag_global_saturation_mode != "0";
        let stage_diagnostic = SearchStageDiagnostic::select_product(
            optional_experiment_mode("CLEARRA_SEARCH_STAGE_PROFILE")? == Some(true),
            stage_diagnostic,
            auxiliary_axis_enabled
                || operation_mask_mode.is_some()
                || strict_prefix_mode.is_some()
                || catalog_guard_mode.is_some()
                || frontier_mode.is_some()
                || subsumption_mode.is_some(),
            coverage_slot.is_some(),
            cfg!(feature = "wasm-stage-profiling"),
            valid_experiment_source(option_env!("CLEARRA_SOURCE_COMMIT")),
            minimum_domain_cutoff_mode == "1",
        )?
        .map(|diagnostic| {
            diagnostic.with_geometry_experiments(
                row_stack_geometry_mode == "1",
                apdp_parent_recheck_mode == "1",
            )
        });
        validate_cancel_diagnostic(
            diagnostic_cancel_ms,
            stage_diagnostic,
            summary_requested,
            coverage_slot.is_some(),
        )?;
        if stage_diagnostic.is_some_and(SearchStageDiagnostic::is_product) {
            validate_product_memo_environment(|name| std::env::var(name))?;
        }
        let memory_profile =
            optional_experiment_mode("CLEARRA_SEARCH_MEMORY_PROFILE")? == Some(true);
        if memory_profile
            && (!stage_diagnostic.is_some_and(SearchStageDiagnostic::is_product)
                || !cfg!(target_os = "linux"))
        {
            return Err(
                "CLEARRA_SEARCH_MEMORY_PROFILE requires the Linux product stage diagnostic".into(),
            );
        }
        if operation_mask_mode.is_some()
            && (fixture != Fixture::P7P4
                || !valid_experiment_source(option_env!("CLEARRA_SOURCE_COMMIT"))
                || other_axis_enabled
                || product_optimization_enabled
                || strict_prefix_mode.is_some())
        {
            return Err("operation masks require a source-bound P7P4 run with all other experiment axes off".into());
        }
        if strict_prefix_mode.is_some()
            && (!matches!(fixture, Fixture::P7 | Fixture::P7P4)
                || !valid_experiment_source(option_env!("CLEARRA_SOURCE_COMMIT"))
                || other_axis_enabled
                || product_optimization_enabled)
        {
            return Err(
                "strict prefix requires a source-bound P7/P7P4 run with all other experiment axes off"
                    .into(),
            );
        }
        let grounded_o_other_axes = [
            &mode,
            &structural_mode,
            &static_influence_mode,
            &translation_interval_mode,
            &movement_clear_certificates_mode,
            &incremental_domain_mode,
            &minimum_domain_work_mode,
            &first_kick_interval_mode,
            &ordered_kick_interval_mode,
            &ordered_kick_guard_reuse_mode,
            &ilc_lazy_frame_mode,
            &no_entry_components_mode,
            &same_tile_parent_reuse_mode,
            &conditional_order_mode,
            &terminal_identity_move_mode,
        ]
        .into_iter()
        .any(|mode| mode != "0");
        validate_fixture_diagnostic_route(
            fixture,
            stage_diagnostic,
            strict_prefix_mode,
            minimum_domain_cutoff_mode == "1",
            geometric_edge_reuse_arm,
            grounded_o_other_axes,
        )?;
        if fixture.is_grounded_o()
            && geometric_edge_reuse_arm.is_some()
            && (row_stack_geometry_mode == "1" || apdp_parent_recheck_mode == "1")
        {
            return Err(
                "grounded geometric-edge reuse arms require row-stack geometry and APDP parent recheck disabled"
                    .into(),
            );
        }
        if fixture == Fixture::EmptyTenI
            && [
                &mode,
                &structural_mode,
                &static_influence_mode,
                &translation_interval_mode,
                &movement_clear_certificates_mode,
                &incremental_domain_mode,
                &first_kick_interval_mode,
                &ordered_kick_interval_mode,
                &ordered_kick_guard_reuse_mode,
                &geometric_edge_reuse_mode,
                &ilc_lazy_frame_mode,
                &no_entry_components_mode,
                &same_tile_parent_reuse_mode,
                &conditional_order_mode,
                &terminal_identity_move_mode,
            ]
            .into_iter()
            .any(|mode| mode != "0")
        {
            return Err("--empty-ten-i isolates minimum-domain, row-stack geometry and APDP parent-recheck axes".into());
        }
        let binary_sha = file_sha(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        let capacity = CpuCapacity::current();
        if strict_prefix_mode.is_some() && capacity.hard_limit() < 2 {
            return Err("strict prefix comparison requires at least two native workers".into());
        }
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_policy(WorkerPolicy::Auto)
            .with_worker_hardware_limit(capacity.hard_limit())
            .with_use_all_logical_processors(true)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false);
        let compile_started = Instant::now();
        let problem = compile_problem(fixture, policy, strict_prefix_mode.is_some())?;
        let problem = if summary_requested {
            product_summary(problem)?
        } else {
            problem
        };
        if problem.count_policy() != fixture.count_policy() {
            return Err("compiled fixture changed the requested count policy".into());
        }
        let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "placement_graph_bench starting: case={} mode={} structural_mode={} static_influence_mode={} translation_interval_mode={} auto_cpu_limit={}",
            fixture.name(),
            mode,
            structural_mode,
            static_influence_mode,
            translation_interval_mode,
            capacity.hard_limit(),
        );
        let capture = coverage_slot
            .map(|slot| -> Result<_, String> {
                let universe = fixture.capture_universe()?;
                Ok((new_capture_directory(slot)?, universe))
            })
            .transpose()?;
        let strict_capture_budgets = strict_prefix_mode.is_some();
        let spool_limits = if strict_capture_budgets {
            CoverageSpoolLimits {
                disk_bytes: STRICT_SPILL_WRITE_BYTES,
                spill_write_bytes: Some(STRICT_SPILL_WRITE_BYTES),
                ..CoverageSpoolLimits::default()
            }
        } else {
            CoverageSpoolLimits::default()
        };
        #[cfg(feature = "wasm-stage-profiling")]
        let stage_profile = stage_diagnostic
            .map(|_| ExecutorSearchProfileSession::start())
            .transpose()
            .map_err(|error| format!("cannot start search stage profile: {error:?}"))?;
        let search_started = Instant::now();
        let control = ExecutionControl::default();
        #[cfg(feature = "wasm-stage-profiling")]
        if let Some(cancel_ms) = diagnostic_cancel_ms {
            // This branch never enters result/proof validation or the normal
            // benchmark schema. Profile ownership stays on the submitting
            // thread; workers publish their existing TLS totals when joined,
            // including when the backend returns Cancelled.
            let diagnostic = stage_diagnostic
                .ok_or("bounded cancellation diagnostic lost its admitted stage profile")?;
            let timer = DiagnosticCancellation::arm(&control, cancel_ms)?;
            let execute = || WasmCpuSearchBackend::execute_with_control(&problem, &control);
            #[cfg(target_os = "linux")]
            let (outcome, memory_observation) = if memory_profile {
                let (outcome, observation) =
                    clearra_core_executor::backend::observe_native_pc_memory(execute).map_err(
                        |error| format!("cannot finish cancellation memory diagnostic: {error}"),
                    )?;
                (outcome, Some(observation))
            } else {
                (execute(), None)
            };
            #[cfg(not(target_os = "linux"))]
            let (outcome, memory_observation) = (execute(), None::<Value>);
            let search_ms = search_started.elapsed().as_secs_f64() * 1000.0;
            let cancellation_requested = timer.finish()?;
            let (caller, workers) = stage_profile
                .ok_or("bounded cancellation diagnostic has no profiler session")?
                .finish_partitioned();
            let (status, execution_error) = match &outcome {
                Ok(_) => ("completed", None),
                Err(WasmCpuSearchError::Cancelled) => ("cancelled", None),
                Err(error) => ("failed", Some(format!("{error:?}"))),
            };
            let row_stack = clearra_core_executor::backend::row_stack_experiment_totals();
            let output = join_report_sections([
                json!({
                    "schema": "clearra.placement-graph-cancel-diagnostic.v1",
                    "diagnostic_only": true,
                    "timing_adoption_authority": false,
                    "solution_parity_checked": false,
                    "search_result_validated": false,
                    "search_status": status,
                    "execution_error": execution_error,
                    "cancellation_requested": cancellation_requested,
                    "cancel_after_ms": cancel_ms,
                    "cancellation_scope": "cooperative backend cancellation; the finite outer supervisor remains the hard execution bound",
                    "case": fixture.name(),
                    "input": fixture.input(),
                    "binary_sha256": binary_sha,
                    "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
                    "supervisor_run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
                    "search_output_policy": format!("{:?}", problem.output_policy()),
                    "requested_geometry_candidate_domain": if row_stack_geometry_mode == "1" { "row-stack-noncrossing" } else { "raw" },
                    "row_stack_geometry_mode": row_stack_geometry_mode,
                    "apdp_parent_recheck_mode": apdp_parent_recheck_mode,
                    "minimum_domain_cutoff_mode": minimum_domain_cutoff_mode,
                    "cpu": { "hard_limit": capacity.hard_limit(), "worker_policy": "auto", "use_all_logical_processors": true },
                    "timing_ms": { "problem_compile": compile_ms, "observed_search": search_ms },
                    "result_scope": "completed means the backend returned before cancellation; no solution, coverage, parity, or uninstrumented timing result is exported or validated by this diagnostic",
                }),
                diagnostic.report(
                    caller,
                    workers,
                    reachability_visited_first_mode == "1",
                    standard_bag_global_saturation_mode == "1",
                    physical_prefix_pruning_mode == "1",
                    candidate_ilc_pruning_mode == "1",
                    standard_bag_boolean_membership_mode == "1",
                    standard_bag_shared_saturation_mode == "1",
                ),
                json!({ "row_stack_geometry": {
                    "scope": "dispatch counters are published at dispatch; expanded_nodes and emitted_candidates count only completed generator partitions, excluding every still-live or cancelled partition",
                    "dispatch_searches": row_stack.dispatch_searches,
                    "dispatch_private_bytes": row_stack.dispatch_private_bytes,
                    "dispatch_private_bytes_scope": "sum of private generator backing at dispatch; excludes shared catalog, targets and verifier; not an RSS peak",
                    "completed_searches": row_stack.completed_searches,
                    "expanded_nodes_completed_partitions_only": row_stack.expanded_nodes,
                    "emitted_candidates_completed_partitions_only": row_stack.emitted_candidates,
                }}),
                memory_observation.map_or_else(
                    || json!({}),
                    |observation| json!({ "product_memory_profile": observation }),
                ),
            ])?;
            println!(
                "{}",
                serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
            );
            if status == "failed" {
                return Err("bounded search diagnostic failed; see diagnostic JSON".into());
            }
            return Ok(());
        }
        let execute = || -> Result<_, String> {
            if let Some((dir, universe)) = &capture {
                let universe = *universe;
                let spool = Arc::new(Mutex::new(
                    CoverageSpool::create(&dir.join("runs"), universe, spool_limits)
                        .map_err(|e| format!("coverage spool: {e}"))?,
                ));
                let owner = Arc::clone(&spool);
                let failure = Arc::new(Mutex::new(None::<String>));
                let capture_failure = Arc::clone(&failure);
                let result = WasmCpuSearchBackend::execute_with_solution_coverage_capture(
                    &problem,
                    &control,
                    move |identity, evidence| {
                        let save = || -> Result<(), String> {
                            let fragment = match evidence {
                                CapturedSolutionCoverage::Bitmap(bits) => {
                                    CoverageFragment::from_bitset(universe, bits)
                                        .map_err(|e| e.to_string())?
                                }
                                CapturedSolutionCoverage::P7P4(blocks)
                                    if universe == CoverageUniverse::P7P4 =>
                                {
                                    CoverageFragment::from_p7p4_blocks(blocks)
                                }
                                CapturedSolutionCoverage::P7P4(_) => {
                                    return Err("unexpected P7P4 blocks in short capture".into());
                                }
                            };
                            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(
                                identity,
                            );
                            owner
                                .lock()
                                .map_err(|_| "spool lock poisoned".to_owned())?
                                .push_fragment(key.as_str(), fragment)
                                .map_err(|e| e.to_string())
                        };
                        save().map_err(|error| {
                            if let Ok(mut first) = capture_failure.lock() {
                                if first.is_none() {
                                    *first = Some(error);
                                }
                            }
                            "solution_coverage_spool_write_failed"
                        })
                    },
                )
                .map_err(|e| {
                    let cause = failure.lock().ok().and_then(|mut first| first.take());
                    let stats = spool.lock().ok().map(|owner| owner.stats());
                    format!(
                    "exact capture execution failed: {e:?}; spool diagnostic: {cause:?}; {stats:?}"
                )
                })?;
                let spool = Arc::try_unwrap(spool)
                    .map_err(|_| "coverage workers still hold the spool")?
                    .into_inner()
                    .map_err(|_| "coverage spool lock poisoned")?;
                Ok((result, Some(spool)))
            } else if strict_prefix_mode.is_some() {
                // Capture activates the same complete ordinal path and identity
                // custody as the proof arm. Timing arms do not serialize rows.
                WasmCpuSearchBackend::execute_with_solution_coverage_capture(
                    &problem,
                    &control,
                    |_, _| Ok(()),
                )
                .map(|result| (result, None))
                .map_err(|e| format!("strict prefix timing execution failed: {e:?}"))
            } else {
                Ok((
                    WasmCpuSearchBackend::execute_with_control(&problem, &control)
                        .map_err(|e| format!("exact core execution failed: {e:?}"))?,
                    None,
                ))
            }
        };
        let execute_variant = || match (operation_mask_mode, strict_prefix_mode) {
            (Some(enabled), None) => {
                let (outcome, report) = with_operation_mask_experiment(enabled, execute);
                (outcome, Some((enabled, report)), None)
            }
            (None, Some(true)) => {
                let (outcome, report) = if catalog_guard_mode == Some(true) {
                    with_catalog_guard_strict_parallel_prefix_experiment(execute)
                } else if stage_diagnostic == Some(SearchStageDiagnostic::LegacyUpper) {
                    with_legacy_strict_parallel_prefix_experiment(execute)
                } else {
                    with_strict_parallel_prefix_experiment(execute)
                };
                (outcome, None, Some(report))
            }
            _ => (execute(), None, None),
        };
        #[cfg(target_os = "linux")]
        let ((outcome, operation_masks, strict_prefix), memory_observation) = if memory_profile {
            let (outcome, observation) =
                clearra_core_executor::backend::observe_native_pc_memory(execute_variant)
                    .map_err(|error| format!("cannot finish product memory diagnostic: {error}"))?;
            (outcome, Some(observation))
        } else {
            (execute_variant(), None)
        };
        #[cfg(not(target_os = "linux"))]
        let ((outcome, operation_masks, strict_prefix), memory_observation) =
            (execute_variant(), None::<Value>);
        let (result, spool) = outcome?;
        let search_ms = search_started.elapsed().as_secs_f64() * 1000.0;
        #[cfg(feature = "wasm-stage-profiling")]
        let profiled_stages = stage_profile.map(ExecutorSearchProfileSession::finish_partitioned);
        let structural_totals =
            clearra_core_executor::backend::structural_order_experiment_totals();
        let geometry_domains = clearra_core_executor::backend::geometry_domain_experiment_totals();
        let row_stack_totals = clearra_core_executor::backend::row_stack_experiment_totals();
        let geometric_edges =
            clearra_core_executor::backend::geometric_edge_reuse_experiment_totals();
        let evidence_started = Instant::now();
        let mut errors = validate(
            &result,
            fixture,
            capacity.hard_limit(),
            geometry_domains,
            DomainValidationModes {
                incremental: incremental_domain_mode == "1",
                same_tile_reuse: same_tile_parent_reuse_mode == "1",
                minimum_cutoff: minimum_domain_cutoff_mode == "1",
                minimum_work: minimum_domain_work_mode == "1",
                row_stack_geometry: row_stack_geometry_mode == "1",
            },
        );
        if row_stack_geometry_mode == "1" && fixture == Fixture::P7P4 {
            if row_stack_totals.dispatch_searches == 0
                || row_stack_totals.completed_searches != row_stack_totals.dispatch_searches
            {
                errors
                    .push("P7P4 row-stack must complete every dispatched generator search".into());
            }
            if result
                .usize_field("packing_candidate_count")
                .and_then(|count| u64::try_from(count).ok())
                != Some(row_stack_totals.emitted_candidates)
            {
                errors.push(
                    "P7P4 row-stack emitted candidates must match the reported packing count"
                        .into(),
                );
            }
        }
        let union_storage = std::env::var("CLEARRA_STANDARD_BAG_UNION_MEMO").ok();
        if stage_diagnostic.is_some_and(SearchStageDiagnostic::is_product) {
            if let Err(error) = validate_product_memo_observation(
                result.field("standard_bag_product_memo_policy"),
                result.field("standard_bag_product_memo_layout"),
                result.field("standard_bag_product_memo_storage"),
                result.field("standard_bag_union_memo_storage"),
            ) {
                errors.push(error);
            }
        } else if let Some(requested) = &union_storage {
            if result.field("standard_bag_union_memo_storage") != Some(requested.as_str()) {
                errors.push("the requested union memo storage was not used".into());
            }
        }
        if let Some(enabled) = strict_prefix_mode {
            validate_strict_prefix(
                &result,
                fixture,
                capacity.hard_limit(),
                enabled,
                strict_prefix,
                &mut errors,
            );
        }
        if let Some((enabled, report)) = operation_masks {
            if let Err(message) = validate_operation_mask_evidence(
                enabled,
                &operation_mask_evidence(report),
                result.usize_field("workers_used"),
                result.usize_field("total_build_order_nodes"),
                result.usize_field("worker_graph_projection_retained_bytes"),
            ) {
                errors.push(message);
            }
            if result.path_steps().len() != fixture.placed_pieces() {
                errors
                    .push("operation-mask run lost its full concrete representative trace".into());
            }
            if result.normalized_solution_identities().len() != LARGE_COUNT {
                errors.push(
                    "operation-mask run did not retain the complete identity sequence".into(),
                );
            }
        }
        if fixture.is_grounded_o() {
            validate_grounded_o(
                &result,
                fixture,
                geometric_edges,
                geometric_edge_reuse_mode == "1",
                &mut errors,
            );
        }
        let solution_coverage = match (capture, spool) {
            (Some((dir, universe)), Some(spool)) if errors.is_empty() => finish_capture(
                dir,
                spool,
                universe,
                &result,
                spool_limits.spill_write_bytes,
            )?,
            _ => Value::Null,
        };
        let fields: Vec<_> = result.summary_field_entries().collect();
        let summary: BTreeMap<_, _> = fields.iter().copied().collect();
        let words = result.coverage_pattern_words();
        let mut coverage_sha = Sha256::new();
        coverage_sha.update(b"clearra-coverage-words-le-v1\0");
        coverage_sha.update((words.len() as u64).to_le_bytes());
        for word in words {
            coverage_sha.update(word.to_le_bytes());
        }
        let identities = result.normalized_solution_identities();
        let mut identity_sha = Sha256::new();
        identity_sha.update(b"clearra-materialized-ordered-ctk1-v1\0");
        identity_sha.update((identities.len() as u64).to_le_bytes());
        for identity in identities {
            // The public key API owns the canonical CTK1 encoding; the
            // identity's private formatter is not part of this contract.
            let canonical = NormalizedTilingSolutionKey::from_standard_board64_identity(*identity);
            let bytes = canonical.as_str().as_bytes();
            identity_sha.update((bytes.len() as u64).to_le_bytes());
            identity_sha.update(bytes);
        }
        let total_count = result.usize_field("normalized_unique_solution_count");
        let mut covered_ordinal_sample = Vec::new();
        for (index, &word) in words.iter().enumerate() {
            let mut pending = word;
            while pending != 0 && covered_ordinal_sample.len() < 16 {
                covered_ordinal_sample.push(index * 64 + pending.trailing_zeros() as usize);
                pending &= pending - 1;
            }
            if covered_ordinal_sample.len() == 16 {
                break;
            }
        }
        // Keep report sections separate so json!'s token recursion is
        // bounded independently of the full published report's field count.
        let metadata = json!({
            // Neither the proof runner nor the adoption checker accepts
            // profiled measurements as uninstrumented timing evidence.
            "schema": SearchStageDiagnostic::schema(stage_diagnostic),
            "case": fixture.name(),
            "binary_sha256": binary_sha,
            "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
            "supervisor_run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
            "output_mode": if coverage_slot.is_some() { "solution-coverage" } else { "global-only" },
            "search_output_policy": format!("{:?}", problem.output_policy()),
            "solution_coverage": solution_coverage,
            "mode": mode,
            "structural_mode": structural_mode,
            "static_influence_mode": static_influence_mode,
            "translation_interval_mode": translation_interval_mode,
            "requested_geometry_candidate_domain": if row_stack_geometry_mode == "1" { "row-stack-noncrossing" } else { "raw" },
            "observed_geometry_candidate_domain": result.field("packing_candidate_domain"),
            "variant": format!("{},movement_clear={movement_clear_certificates_mode},incremental_domain={incremental_domain_mode},first_kick={first_kick_interval_mode},ordered_kick={ordered_kick_interval_mode},ordered_kick_guard_reuse={ordered_kick_guard_reuse_mode},geometric_edge_reuse={geometric_edge_reuse_mode},ilc_lazy_frame={ilc_lazy_frame_mode},no_entry_components={no_entry_components_mode},same_tile_parent_reuse={same_tile_parent_reuse_mode},conditional_order={conditional_order_mode},terminal_identity_move={terminal_identity_move_mode},minimum_domain_cutoff={minimum_domain_cutoff_mode},minimum_domain_work={minimum_domain_work_mode},row_stack_geometry={row_stack_geometry_mode},apdp_parent_recheck={apdp_parent_recheck_mode}",
                variant(&mode, &structural_mode, &static_influence_mode, &translation_interval_mode)),
        });
        let follow_on_axes = json!({
            "movement_clear_certificates_mode": movement_clear_certificates_mode,
            "incremental_domain_mode": incremental_domain_mode,
            "minimum_domain_cutoff_mode": minimum_domain_cutoff_mode,
            "minimum_domain_work_mode": minimum_domain_work_mode,
            "row_stack_geometry_mode": row_stack_geometry_mode,
            "apdp_parent_recheck_mode": apdp_parent_recheck_mode,
            "reachability_visited_first_mode": reachability_visited_first_mode,
            "standard_bag_global_saturation_mode": standard_bag_global_saturation_mode,
            "physical_prefix_pruning_mode": physical_prefix_pruning_mode,
            "candidate_ilc_pruning_mode": candidate_ilc_pruning_mode,
            "standard_bag_boolean_membership_mode": standard_bag_boolean_membership_mode,
            "standard_bag_shared_saturation_mode": standard_bag_shared_saturation_mode,
            "first_kick_interval_mode": first_kick_interval_mode,
            "ordered_kick_interval_mode": ordered_kick_interval_mode,
            "ordered_kick_guard_reuse_mode": ordered_kick_guard_reuse_mode,
            "geometric_edge_reuse_mode": geometric_edge_reuse_mode,
            "ilc_lazy_frame_mode": ilc_lazy_frame_mode,
            "no_entry_components_mode": no_entry_components_mode,
            "same_tile_parent_reuse_mode": same_tile_parent_reuse_mode,
            "conditional_order_mode": conditional_order_mode,
            "terminal_identity_move_mode": terminal_identity_move_mode,
        });
        let structural_metrics = json!({
            "static_influence": {
                "scope": "completed search, workspace counters aggregated after worker joins; null means metric not exposed",
                "queries": result.usize_field("reachability_static_influence_queries"),
                "reachable": result.usize_field("reachability_static_influence_reachable"),
                "unreachable": result.usize_field("reachability_static_influence_unreachable"),
                "row_steps": result.usize_field("reachability_static_influence_row_steps"),
                "cache_hits": result.usize_field("reachability_static_influence_cache_hits"),
                "masked_queries": result.usize_field("reachability_static_influence_masked_queries"),
                "ignored_cells": result.usize_field("reachability_static_influence_ignored_cells"),
                "mask_compilations": result.usize_field("reachability_static_influence_mask_compilations")
            },
            "structural_order": {
                "scope": "process lifetime, one completed search; sampled after worker joins",
                "skipped_build_graphs": structural_totals.skipped_build_graphs,
                "language_residual_states": structural_totals.language_residual_states,
                "exchangeable_operations_removed": structural_totals.exchangeable_operations_removed,
                "cached_interval_guards": structural_totals.cached_interval_guards,
                "algebraic_interval_attempts": structural_totals.algebraic_interval_attempts,
                "algebraic_interval_guards": structural_totals.algebraic_interval_guards,
                "algebraic_interval_row_steps": structural_totals.algebraic_interval_row_steps,
                "translation_interval_attempts": structural_totals.translation_interval_attempts,
                "translation_interval_guards": structural_totals.translation_interval_guards,
                "translation_interval_row_steps": structural_totals.translation_interval_row_steps
            },
            "movement_clear": {
                "frame_pose_attempts": structural_totals.movement_clear.frame_pose_attempts,
                "frame_pose_proofs": structural_totals.movement_clear.frame_pose_proofs,
                "reused_positive_locks": structural_totals.movement_clear.reused_positive_locks,
                "reused_closed_locks": structural_totals.movement_clear.reused_closed_locks,
                "point_pose_attempts": structural_totals.movement_clear.point_pose_attempts,
                "point_pose_proofs": structural_totals.movement_clear.point_pose_proofs
            },
            "first_kick_interval": {
                "attempts": structural_totals.first_kick_interval.attempts,
                "proofs": structural_totals.first_kick_interval.proofs,
                "selected_locks": structural_totals.first_kick_interval.selected_locks
            },
            "ordered_kick_interval": {
                "attempts": structural_totals.ordered_kick_interval.attempts,
                "proofs": structural_totals.ordered_kick_interval.proofs,
                "selected_locks": structural_totals.ordered_kick_interval.selected_locks
            },
            "conditional_order": {
                "attempted_candidates": structural_totals.conditional_order.attempted_candidates,
                "certified_candidates": structural_totals.conditional_order.certified_candidates,
                "empty_certificates": structural_totals.conditional_order.empty_certificates,
                "unknown_candidates": structural_totals.conditional_order.unknown_candidates,
                "budget_candidates": structural_totals.conditional_order.budget_candidates,
                "checked_transitions": structural_totals.conditional_order.checked_transitions,
                "residual_states": structural_totals.conditional_order.residual_states,
                "graph_bypasses": structural_totals.conditional_order.graph_bypasses,
                "reused_pose_proofs": structural_totals.conditional_order.reused_pose_proofs
            },
        });
        let edge_validation_metrics = json!({
            "ordered_kick_guard_reuse": {
                "constructed": structural_totals.ordered_kick_guard_reuse.constructed,
                "stored": structural_totals.ordered_kick_guard_reuse.stored,
                "reused_selected": structural_totals.ordered_kick_guard_reuse.reused_selected,
                "conflicts": structural_totals.ordered_kick_guard_reuse.conflicts,
                "exact_retries": structural_totals.ordered_kick_guard_reuse.exact_retries
            },
            "geometric_edges": {
                "scope": "completed compact BuildOrderGraph passes; pose feasibility is not reused",
                "footprint_validations": geometric_edges.footprint_validations,
                "reused_validations": geometric_edges.reused_validations,
                "line_clear_computations": geometric_edges.line_clear_computations,
                "projection_validations": geometric_edges.projection_validations,
            },
        });
        let domains = json!({
            "geometry_domain": {
                "scope": "completed compact family roots; not memo/column exits, partial roots or extended",
                "completed_families": geometry_domains.completed_families,
                "minimum_compiles": geometry_domains.minimum_compiles,
                "minimum_resource_authoritative": geometry_domains.minimum_resource_authoritative,
                "minimum_below_seven_pieces": geometry_domains.minimum_below_seven_pieces,
                "minimum_empty_origin": geometry_domains.minimum_empty_origin,
                "minimum_profiled_compiles": geometry_domains.minimum_profiled_compiles,
                "minimum_incidence_rows_scanned": geometry_domains.minimum_incidence_rows_scanned,
                "minimum_incidence_rows_skipped": geometry_domains.minimum_incidence_rows_skipped,
                "minimum_cells_cut_off": geometry_domains.minimum_cells_cut_off,
                "advanced_full_compiles": geometry_domains.advanced_full_compiles,
                "advanced_incremental_compiles": geometry_domains.advanced_incremental_compiles,
                "support_cells_reused": geometry_domains.support_cells_reused,
                "support_cells_recomputed": geometry_domains.support_cells_recomputed,
                "same_tile_parent_scans": geometry_domains.same_tile_parent_scans,
                "same_tile_parent_reuses": geometry_domains.same_tile_parent_reuses,
                "apdp_static_checks": geometry_domains.apdp_static_checks,
                "apdp_static_complete": geometry_domains.apdp_static_complete,
                "apdp_pivot_tightenings": geometry_domains.apdp_pivot_tightenings
            },
        });
        let memory_lifetimes = json!({
            "core_memory_lifetimes": {
                "scope": result.field("parallel_memory_lifetime_scope"),
                "interpretation": "selected logical byte estimates at named checkpoints; checkpoints overlap, do not sum or interpret as allocator or RSS peaks; null means not exposed",
                "worker_exit": {
                    "samples": result.usize_field("parallel_worker_exit_snapshot_count"),
                    "scratch_bytes_sum": result.usize_field("parallel_worker_exit_scratch_bytes_sum"),
                    "scratch_bytes_max": result.usize_field("parallel_worker_exit_scratch_bytes_max"),
                    "outbound_payload_bytes_sum": result.usize_field("parallel_worker_exit_outbound_payload_bytes_sum"),
                    "branch_slots_bytes_sum": result.usize_field("parallel_worker_exit_branch_slots_bytes_sum")
                },
                "coordinator_handoff": {
                    "catalog_payload_bytes": result.usize_field("parallel_handoff_catalog_payload_bytes"),
                    "result_slots_bytes": result.usize_field("parallel_handoff_result_slots_bytes"),
                    "global_coverage_storage_bytes": result.usize_field("parallel_handoff_global_coverage_storage_bytes")
                },
                "terminal_assembly": {
                    "result_slots_bytes": result.usize_field("parallel_assembly_result_slots_bytes")
                },
                "shared_request": {
                    "standard_bag_retained_bytes": result.usize_field("shared_standard_bag_request_retained_bytes"),
                    "reachability_template_retained_bytes": result.usize_field("shared_reachability_template_retained_bytes")
                }
            }
        });
        let mut process_configuration = json!({
            "CLEARRA_STANDARD_BAG_MEMO": std::env::var("CLEARRA_STANDARD_BAG_MEMO").ok(),
            "CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT": std::env::var("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT").ok(),
            "CLEARRA_REACHABILITY_VISITED_FIRST": reachability_visited_first_mode,
            "CLEARRA_STANDARD_BAG_GLOBAL_SATURATION": standard_bag_global_saturation_mode,
            "CLEARRA_PHYSICAL_PREFIX_PRUNING": physical_prefix_pruning_mode,
            "CLEARRA_CANDIDATE_ILC_PRUNING": candidate_ilc_pruning_mode,
            "CLEARRA_STANDARD_BAG_BOOLEAN_MEMBERSHIP": standard_bag_boolean_membership_mode,
            "CLEARRA_STANDARD_BAG_SHARED_SATURATION": standard_bag_shared_saturation_mode
        });
        if let Some(storage) = union_storage {
            process_configuration["CLEARRA_STANDARD_BAG_UNION_MEMO"] = json!(storage);
        }
        for key in [
            "CLEARRA_STRICT_CATALOG_GUARDS",
            "CLEARRA_STANDARD_BAG_ABSORB",
            "CLEARRA_STRICT_LANGUAGE_FRONTIER",
            "CLEARRA_STRICT_SUBSUMPTION",
        ] {
            if let Ok(value) = std::env::var(key) {
                process_configuration[key] = json!(value);
            }
        }
        let evidence = json!({
            "other_process_configuration": process_configuration,
            "backend_contract": "native-host Rust WasmCpuSearchBackend; no C core; no tablebase",
            "input": fixture.input(),
            "cpu": { "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
                "recommended_parallelism": capacity.recommended_parallelism(),
                "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
                "hard_limit": capacity.hard_limit(), "worker_policy": "auto", "use_all_logical_processors": true },
            "timing_ms": { "problem_compile": compile_ms, "search": search_ms,
                "evidence_hashing": evidence_started.elapsed().as_secs_f64() * 1000.0 },
            "coverage": { "word_count": words.len(),
                "set_bit_count": words.iter().map(|word| u64::from(word.count_ones())).sum::<u64>(),
                "sha256_le_v1": format!("{:x}", coverage_sha.finalize()),
                "covered_ordinal_sample": covered_ordinal_sample,
                "ordinal_sample_scope": "at most 16 original-universe bit indices; not a full queue export" },
            "materialized_identities": { "count": identities.len(), "reported_total_count": total_count,
                "covers_reported_family": total_count == Some(identities.len()),
                "sha256_ordered_ctk1_v1": format!("{:x}", identity_sha.finalize()),
                "scope": "only the identities actually exposed by this result; a partial page is never a full-set proof" },
            "materialized_solution_coverage_count": result.normalized_solution_coverages().len(),
            "materialized_solution_probability_count": result.solution_probabilities().len(),
            "summary": summary, "summary_field_entries": fields,
            "valid": errors.is_empty(), "validation_errors": errors,
        });
        let operation_mask_section = operation_masks.map_or_else(
            || json!({}),
            |(enabled, report)| {
                json!({
                    "operation_mask_mode": if enabled { "1" } else { "0" },
                    "operation_masks": operation_mask_evidence(report),
                    "operation_mask_representative": result.path_steps().iter().map(|step| {
                        json!([step.piece().as_ascii().to_string(), step.rotation(), step.x(), step.y(), step.hold(), step.cleared_lines()])
                    }).collect::<Vec<_>>(),
                })
            },
        );
        let strict_prefix_section = strict_prefix_mode.map_or_else(
            || json!({}),
            |enabled| {
                json!({
                    "strict_prefix_mode": if enabled { "1" } else { "0" },
                    "strict_prefix": strict_prefix.map(strict_prefix_evidence),
                    "coverage_presentation": "coverage-summary-omit",
                    "coverage_capture_sink": if coverage_slot.is_some() { "exact-spool" } else { "discard" },
                    "materialized_path_step_count": result.path_steps().len(),
                })
            },
        );
        #[cfg(feature = "wasm-stage-profiling")]
        let diagnostic_section = stage_diagnostic.zip(profiled_stages).map_or_else(
            || json!({}),
            |(diagnostic, (caller, workers))| {
                diagnostic.report(
                    caller,
                    workers,
                    reachability_visited_first_mode == "1",
                    standard_bag_global_saturation_mode == "1",
                    physical_prefix_pruning_mode == "1",
                    candidate_ilc_pruning_mode == "1",
                    standard_bag_boolean_membership_mode == "1",
                    standard_bag_shared_saturation_mode == "1",
                )
            },
        );
        #[cfg(not(feature = "wasm-stage-profiling"))]
        let diagnostic_section = json!({});
        let row_stack_metrics = json!({
            "row_stack_geometry": {
                "scope": "process-lifetime counters for this one-search process; published at partition dispatch/completion, without hot-path atomics",
                "dispatch_searches": row_stack_totals.dispatch_searches,
                "dispatch_private_bytes": row_stack_totals.dispatch_private_bytes,
                "dispatch_private_bytes_scope": "sum of private generator backing at dispatch; excludes shared catalog, targets and verifier; not an RSS peak",
                "completed_searches": row_stack_totals.completed_searches,
                "expanded_nodes": row_stack_totals.expanded_nodes,
                "emitted_candidates": row_stack_totals.emitted_candidates,
            }
        });
        let output = join_report_sections([
            metadata,
            follow_on_axes,
            structural_metrics,
            edge_validation_metrics,
            domains,
            memory_lifetimes,
            evidence,
            operation_mask_section,
            strict_prefix_section,
            diagnostic_section,
            row_stack_metrics,
            memory_observation.map_or_else(
                || json!({}),
                |observation| json!({ "product_memory_profile": observation }),
            ),
        ])?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        if output["valid"] != true {
            return Err("completed result failed benchmark validation; see JSON evidence".into());
        }
        Ok(())
    }

    impl SearchStageDiagnostic {
        #[cfg(feature = "wasm-stage-profiling")]
        fn report(
            self,
            caller: Vec<ExecutorSearchProfileStage>,
            workers: Vec<ExecutorSearchProfileStage>,
            reachability_visited_first: bool,
            standard_bag_global_saturation: bool,
            physical_prefix_pruning: bool,
            candidate_ilc_pruning: bool,
            standard_bag_boolean_membership: bool,
            standard_bag_shared_saturation: bool,
        ) -> Value {
            let stages: Vec<_> = caller
                .into_iter()
                .zip(workers)
                .filter(|(caller, workers)| {
                    caller.invocation_count != 0 || workers.invocation_count != 0
                })
                .map(|(caller, workers)| {
                    json!({
                        "stage": caller.name,
                        "caller_ns": caller.duration_ns,
                        "pool_workers_ns": workers.duration_ns,
                        "caller_measured_ns": caller.measured_duration_ns,
                        "pool_workers_measured_ns": workers.measured_duration_ns,
                        "caller_measured_samples": caller.sample_count,
                        "pool_worker_measured_samples": workers.sample_count,
                        "caller_invocations": caller.invocation_count,
                        "pool_worker_invocations": workers.invocation_count,
                        "caller_work_items": caller.work_item_count,
                        "pool_worker_work_items": workers.work_item_count,
                    })
                })
                .collect();
            let key = if self.is_product() {
                "product_stage_profile"
            } else {
                "strict_stage_profile"
            };
            json!({
                (key): {
                    "diagnostic_only": true,
                    "timing_adoption_authority": false,
                    "execution_route": self.execution_route(),
                    "configuration": match self {
                        Self::ProductDefault if reachability_visited_first && standard_bag_global_saturation && physical_prefix_pruning && candidate_ilc_pruning && standard_bag_boolean_membership && standard_bag_shared_saturation => "product-default",
                        Self::ProductDefault => "product-ablation",
                        Self::ProductCutoffExperiment => "product-cutoff-experiment",
                        Self::ProductRowStackExperiment { .. } => "product-row-stack-experiment",
                        Self::ProductApdpReference { .. } => "product-apdp-reference",
                        Self::Reference => "strict-reference",
                        Self::OptimizedUpper => "strict-optimized-upper",
                        Self::LegacyUpper => "strict-legacy-upper",
                    },
                    "upper_strategy": match self {
                        Self::ProductDefault | Self::ProductCutoffExperiment | Self::ProductRowStackExperiment { .. } | Self::ProductApdpReference { .. } | Self::Reference => "not-applicable",
                        Self::OptimizedUpper => "optimized",
                        Self::LegacyUpper => "legacy",
                    },
                    "product_optimizations": self.is_product().then(|| json!({
                        "minimum_domain_cutoff": self.minimum_domain_cutoff(),
                        "row_stack_geometry": self.row_stack_geometry(),
                        "apdp_parent_recheck": self.apdp_parent_recheck(),
                        "reachability_visited_first": reachability_visited_first,
                        "standard_bag_global_saturation": standard_bag_global_saturation,
                        "physical_prefix_pruning": physical_prefix_pruning,
                        "candidate_ilc_pruning": candidate_ilc_pruning,
                        "standard_bag_boolean_membership": standard_bag_boolean_membership,
                        "standard_bag_shared_saturation": standard_bag_shared_saturation,
                    })),
                    "geometry_domain_scope": if self.row_stack_geometry() { "bypassed-by-row-stack; minimum-domain and APDP parent-recheck settings are not exercised" } else { "residual-domain" },
                    "scope": "submitting-thread and pool-worker accumulated stage times; spans may nest; do not sum as wall time",
                    "sampling": "*_ns and *_invocations include sample expansion; *_measured_ns and *_measured_samples contain only observed spans; neither is end-to-end wall time",
                    "stages": stages,
                },
            })
        }
    }

    fn join_report_sections(sections: impl IntoIterator<Item = Value>) -> Result<Value, String> {
        let mut report = serde_json::Map::new();
        for section in sections {
            let Value::Object(fields) = section else {
                return Err("benchmark report section must be an object".into());
            };
            for (key, value) in fields {
                match report.entry(key) {
                    serde_json::map::Entry::Occupied(entry) => {
                        return Err(format!("duplicate benchmark report key: {}", entry.key()));
                    }
                    serde_json::map::Entry::Vacant(entry) => {
                        entry.insert(value);
                    }
                }
            }
        }
        Ok(Value::Object(report))
    }

    fn experiment_mode(name: &str) -> Result<String, String> {
        experiment_mode_with_default(name, "0")
    }

    fn experiment_mode_with_default(name: &str, default: &str) -> Result<String, String> {
        let value = match std::env::var(name) {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => default.into(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(format!("{name} must be Unicode 0 or 1 at process start"));
            }
        };
        if matches!(value.as_str(), "0" | "1") {
            Ok(value)
        } else {
            Err(format!("{name} must be 0 or 1 at process start"))
        }
    }

    fn optional_experiment_mode(name: &str) -> Result<Option<bool>, String> {
        match std::env::var(name).as_deref() {
            Ok("0") => Ok(Some(false)),
            Ok("1") => Ok(Some(true)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            _ => Err(format!("{name} must be 0 or 1 at process start")),
        }
    }

    fn valid_experiment_source(source: Option<&str>) -> bool {
        source.is_some_and(|sha| {
            sha.len() == 40
                && sha
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    }

    fn operation_mask_evidence(report: OperationMaskExperimentReport) -> Value {
        json!({
            "scope": "one native search, successful worker exits; concrete representative replay is a separate owner",
            "workers": report.workers,
            "requested_workers": report.requested_workers,
            "executing_workers": report.executing_workers,
            "graph_passes": report.graph_passes,
            "mask_passes": report.mask_passes,
            "mask_live_passes": report.mask_live_passes,
            "visited_subsets": report.visited_subsets,
            "selected_edges": report.selected_edges,
            "graph_node_bytes": report.graph_node_bytes,
            "graph_edge_bytes": report.graph_edge_bytes,
            "graph_piece_edge_bytes": report.graph_piece_edge_bytes,
            "mask_bytes": report.mask_bytes,
            "graph_projection_bytes": report.graph_projection_bytes,
        })
    }

    fn strict_prefix_evidence(report: StrictParallelPrefixReport) -> Value {
        json!({
            "workers": report.workers,
            "executing_workers": report.executing_workers,
            "candidates": report.candidates,
            "reverse_relations": report.reverse_relations,
            "unresolved_lanes": report.unresolved_lanes,
            "pruned_lanes": report.pruned_lanes,
            "subsumed_lanes": report.subsumed_lanes,
            "frontier_compilations": report.frontier_compilations,
            "pose_work": report.pose_work,
            "worklist_pops": report.worklist_pops,
            "pose_domains": report.pose_domains,
            "ordered_kick_terms": report.ordered_kick_terms,
            "peak_relation_logical_bytes": report.peak_relation_logical_bytes,
            "catalog_static_guard_hits": report.catalog_static_guard_hits,
            "catalog_static_guard_bytes": report.catalog_static_guard_bytes,
            "sealed": report.sealed,
        })
    }

    fn validate_strict_prefix(
        result: &CoreExecutionResult,
        fixture: Fixture,
        worker_cap: usize,
        enabled: bool,
        report: Option<StrictParallelPrefixReport>,
        errors: &mut Vec<String>,
    ) {
        let universe = match fixture {
            Fixture::P7 => CoverageUniverse::P7,
            Fixture::P7P4 => CoverageUniverse::P7P4,
            _ => {
                errors.push("unsupported strict-prefix fixture".into());
                return;
            }
        };
        let workers = result.usize_field("workers_used").unwrap_or_default();
        let active = result
            .usize_field("parallel_active_workers")
            .unwrap_or_default();
        if result.bool_field("cpu_parallel_execution") != Some(true)
            || !(2..=worker_cap).contains(&workers)
            || !(2..=workers).contains(&active)
        {
            errors.push("strict prefix did not use the required native parallel topology".into());
        }
        let draws = if fixture == Fixture::P7 { 7 } else { 11 };
        let queues = universe.queue_count() as usize;
        let words = result.coverage_pattern_words();
        let complete_words = words.len() == queues.div_ceil(64)
            && words.iter().enumerate().all(|(index, word)| {
                let remaining = queues - index * 64;
                let expected = if remaining >= 64 {
                    u64::MAX
                } else {
                    (1_u64 << remaining) - 1
                };
                *word == expected
            });
        if result.bool_field("coverage_calculated") != Some(true)
            || result.usize_field("source_sequence_length") != Some(draws)
            || result.usize_field("coverage_pattern_count") != Some(queues)
            || result.usize_field("covered_pattern_count") != Some(queues)
            || result
                .field("coverage_probability")
                .and_then(|value| value.parse::<f64>().ok())
                != Some(1.0)
            || !complete_words
        {
            errors.push("strict prefix did not cover the complete original queue universe".into());
        }
        if result.normalized_solution_identities().len() != universe.solution_count() as usize
            || !result.path_steps().is_empty()
            || !result.solution_coverages().is_empty()
            || !result.normalized_solution_coverages().is_empty()
            || !result.solution_probabilities().is_empty()
        {
            errors.push("strict prefix must retain all identities without materializing replay or presentation rows".into());
        }
        let candidates = result.usize_field("packing_candidate_count");
        if candidates.is_none_or(|count| count == 0)
            || (fixture == Fixture::P7 && candidates != Some(P7_CANDIDATE_COUNT))
        {
            errors.push("strict prefix candidate universe differs from the fixture".into());
        }
        let graph_nodes = result.usize_field("total_build_order_nodes");
        let pose_states = result.usize_field("total_reachability_states");
        if enabled {
            if report.is_none_or(|work| {
                !work.sealed
                    || work.workers != workers
                    || work.executing_workers != active
                    || Some(work.candidates) != candidates
                    || work.reverse_relations == 0
                    || work.unresolved_lanes == 0
            }) || graph_nodes != Some(0)
                || pose_states != Some(0)
            {
                errors.push(
                    "strict prefix worker evidence is incomplete or used the old BuildUp path"
                        .into(),
                );
            }
        } else if report.is_some()
            || graph_nodes.is_none_or(|nodes| nodes == 0)
            || pose_states.is_none_or(|states| states == 0)
        {
            errors.push("reference arm did not use concrete BuildUp".into());
        }
    }

    fn validate_operation_mask_evidence(
        enabled: bool,
        evidence: &Value,
        actual_workers: Option<usize>,
        actual_nodes: Option<usize>,
        actual_graph_bytes: Option<usize>,
    ) -> Result<(), String> {
        let metric = |key: &str| {
            evidence[key]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| format!("operation-mask evidence is missing {key}"))
        };
        let workers = metric("workers")?;
        let requested = metric("requested_workers")?;
        let executing = metric("executing_workers")?;
        let graph = metric("graph_passes")?;
        let masks = metric("mask_passes")?;
        let live = metric("mask_live_passes")?;
        let nodes = metric("visited_subsets")?;
        let mask_bytes = metric("mask_bytes")?;
        let concrete_bytes = metric("graph_node_bytes")?
            .saturating_add(metric("graph_edge_bytes")?)
            .saturating_add(metric("graph_piece_edge_bytes")?);
        let graph_bytes = metric("graph_projection_bytes")?;
        if workers < 2
            || executing < 2
            || executing > workers
            || actual_workers != Some(workers)
            || actual_nodes != Some(nodes)
            || actual_graph_bytes != Some(graph_bytes)
            || nodes == 0
            || metric("selected_edges")? == 0
            || graph_bytes < concrete_bytes.saturating_add(mask_bytes)
        {
            return Err(
                "operation-mask worker evidence is incomplete or does not match the core result"
                    .into(),
            );
        }
        if enabled {
            if requested != workers {
                return Err(
                    "operation-mask request was not installed and used across actual workers"
                        .into(),
                );
            }
            if graph != 0 || concrete_bytes != 0 {
                return Err("operation-mask workers fell back to concrete graph storage".into());
            }
            if live == 0 || masks < live || mask_bytes == 0 {
                return Err(
                    "operation-mask workers did not complete a live bounded traversal".into(),
                );
            }
        } else if requested != 0
            || masks != 0
            || live != 0
            || mask_bytes != 0
            || graph == 0
            || concrete_bytes == 0
        {
            return Err("operation-mask control was not purely concrete".into());
        }
        Ok(())
    }

    fn new_capture_directory(slot: &str) -> Result<PathBuf, String> {
        if slot.is_empty()
            || slot.len() > 64
            || !slot
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("coverage slot must be an ASCII alphanumeric/hyphen leaf".into());
        }
        let mut path = std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|e| e.to_string())?;
        if !path.join("config/clearra-management.v1.json").is_file() {
            return Err("run coverage capture from the Clearra repository root".into());
        }
        for component in [
            "_local",
            "artifacts",
            "benchmarks",
            "placement-graph-coverage",
        ] {
            path.push(component);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("coverage output root: {error}")),
            }
            let resolved = path.canonicalize().map_err(|e| e.to_string())?;
            if resolved != path || !path.is_dir() {
                return Err("coverage output root cannot follow symlinks or junctions".into());
            }
        }
        path.push(slot);
        fs::create_dir(&path).map_err(|e| format!("coverage slot must be new: {e}"))?;
        Ok(path)
    }

    fn finish_capture(
        dir: PathBuf,
        spool: CoverageSpool,
        universe: CoverageUniverse,
        result: &CoreExecutionResult,
        spill_write_limit_bytes: Option<u64>,
    ) -> Result<Value, String> {
        let (output_limit_bytes, output_limit_error) = if spill_write_limit_bytes.is_some() {
            (
                STRICT_FINAL_OUTPUT_BYTES,
                "CLEARRA_STRICT_COVERAGE_DISK_LIMIT: final output exceeds 3 GiB",
            )
        } else {
            (
                LEGACY_FINAL_OUTPUT_BYTES,
                "coverage output exceeded its 8 GiB quota",
            )
        };
        let path = dir.join("solution-coverage.clrcov");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let buffer = BufWriter::new(file);
        let output = CoverageWriter::new(
            LimitedOutput {
                inner: buffer,
                remaining: output_limit_bytes,
                limit_error: output_limit_error,
            },
            universe,
        )
        .map_err(|e| e.to_string())?;
        let mut result_keys: Vec<_> = result
            .normalized_solution_identities()
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .collect();
        result_keys.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut expected = result_keys.iter();
        let (summary, spool_stats) = spool.finish_checked(output, |key| {
            if expected.next().is_some_and(|next| next.as_str() == key) {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "captured solution identity differs from exact product result",
                ))
            }
        });
        let summary = summary.map_err(|e| format!("coverage evidence: {e}; {spool_stats:?}"))?;
        if expected.next().is_some() {
            return Err("captured solution family did not cover the exact product result".into());
        }
        drop(result_keys);
        Ok(json!({
            "sha256": file_sha(&path)?, "solution_count": summary.solutions,
            "total_weight": universe.queue_count(), "covered_weight": summary.covered_queues,
            "covered_pairs": summary.covered_pairs, "complete": true,
            "output_bytes": fs::metadata(&path).map_err(|e| e.to_string())?.len(),
            "output_limit_bytes": output_limit_bytes,
            "spool": {
                "fragments_fed": spool_stats.fragments_fed,
                "spill_bytes_written": spool_stats.spill_bytes_written,
                "spill_write_limit_bytes": spill_write_limit_bytes,
                "peak_live_spill_bytes": spool_stats.peak_live_spill_bytes,
                "peak_live_run_count": spool_stats.peak_live_run_count
            }
        }))
    }

    struct LimitedOutput<W> {
        inner: W,
        remaining: u64,
        limit_error: &'static str,
    }

    impl<W: Write> Write for LimitedOutput<W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() as u64 > self.remaining {
                return Err(io::Error::other(self.limit_error));
            }
            let written = self.inner.write(bytes)?;
            self.remaining -= written as u64;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    struct HashingReader<R> {
        inner: R,
        sha: Sha256,
    }

    impl<R: Read> Read for HashingReader<R> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let count = self.inner.read(bytes)?;
            self.sha.update(&bytes[..count]);
            Ok(count)
        }
    }

    fn compare_coverage(reference: &Path, candidate: &Path) -> Result<(), String> {
        let open = |path: &Path| {
            File::open(path)
                .map(BufReader::new)
                .map(|inner| HashingReader {
                    inner,
                    sha: Sha256::new(),
                })
                .map_err(|e| e.to_string())
        };
        let mut reference = open(reference)?;
        let mut candidate = open(candidate)?;
        let comparison = solution_coverage_compare::compare(&mut reference, &mut candidate)
            .map_err(|e| format!("coverage comparison: {e}"))?;
        let mut report = json!({
            "schema": "clearra.placement-graph-solution-coverage-comparison.v1",
        });
        match comparison {
            CoverageComparison::Equal(summary) => {
                report["left_sha256"] = json!(format!("{:x}", reference.sha.finalize()));
                report["right_sha256"] = json!(format!("{:x}", candidate.sha.finalize()));
                report["equivalent"] = json!(true);
                report["complete"] = json!(true);
                report["universe"] = json!(match summary.universe {
                    CoverageUniverse::P7P4 => "p7p4",
                    CoverageUniverse::SmallI => "small-i",
                    CoverageUniverse::P7 => "p7",
                    #[cfg(test)]
                    CoverageUniverse::Tiny => "synthetic",
                });
                report["solution_count"] = json!(summary.solutions);
                report["total_weight"] = json!(summary.universe.queue_count());
                report["covered_weight"] = json!(summary.covered_queues);
                report["covered_pairs"] = json!(summary.covered_pairs);
            }
            CoverageComparison::Different(diff) => {
                report["equivalent"] = json!(false);
                report["complete"] = json!(false);
                report["difference"] = json!({
                    "reference_identity": diff.reference_identity,
                    "candidate_identity": diff.candidate_identity,
                    "first_missing_queue": diff.first_missing_queue,
                    "first_added_queue": diff.first_added_queue,
                    "reference_numerator": diff.reference_numerator,
                    "candidate_numerator": diff.candidate_numerator,
                    "denominator": diff.denominator,
                });
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        if report["equivalent"] == true {
            Ok(())
        } else {
            Err("per-solution coverage differs".into())
        }
    }

    fn product_summary(problem: SearchProblem) -> Result<SearchProblem, String> {
        if problem.count_policy() != PcCountPolicy::CountUnique
            || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
        {
            return Err(
                "ordinary Summary timing requires CountUnique and omitted per-solution coverage"
                    .into(),
            );
        }
        Ok(problem.with_output_policy(SearchOutputPolicy::Summary))
    }

    fn compile_problem(
        fixture: Fixture,
        policy: PcExecutionPolicy,
        coverage_summary: bool,
    ) -> Result<SearchProblem, String> {
        let compiled = if fixture == Fixture::P7 {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(fixture.height(), P7_INITIAL),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(fixture.placed_pieces()),
            )
            .with_rule(srs_plus())
            .with_allow_hold(true)
            .with_exact_pieces(Some(fixture.placed_pieces()))
            .with_supply_window_size(SupplyWindowSize::new(7))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
            .with_retained_trace_limit(0)
            .with_execution_policy(policy);
            ProblemCompiler::compile_scenario_pc(&query)
        } else if let Some(board) = fixture.fixed_board() {
            let pieces = fixture.placed_pieces();
            let piece = if fixture.is_grounded_o() {
                PieceKind::O
            } else {
                PieceKind::I
            };
            let rule = if fixture.is_grounded_o() {
                srs()
            } else {
                jstris_180()
            };
            let mut query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(fixture.height(), board),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece; pieces])),
                PieceWindow::new(pieces),
            )
            .with_rule(rule)
            .with_allow_hold(false)
            .with_exact_pieces(Some(pieces))
            .with_count_policy(fixture.count_policy())
            .with_execution_policy(policy);
            if fixture.is_grounded_o() {
                // Include prevents the unique negative control from taking
                // the exact fixed-queue single-witness route, which has no
                // BuildOrderGraph or per-realization geometry loop.
                query =
                    query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
            }
            ProblemCompiler::compile_scenario_pc(&query)
        } else {
            let (queue, hold) = if fixture == Fixture::EmptyTenI {
                (
                    PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 10])),
                    PcHoldPolicy::Disabled,
                )
            } else {
                (PcQueueInput::standard_7_bag(), PcHoldPolicy::EnabledEmpty)
            };
            let mut query = OpeningPcSearchQuery::new(PcTarget::four_lines())
                .with_rule(jstris_180())
                .with_queue(queue)
                .with_hold_policy(hold)
                .with_objective(ObjectivePolicy::unique())
                .with_execution_policy(policy);
            if coverage_summary {
                query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit);
            }
            ProblemCompiler::compile_opening_pc(&query)
        };
        let compiled = compiled.map_err(|e| format!("{} compile: {e:?}", fixture.name()))?;
        if coverage_summary {
            let compiled = compiled.with_output_policy(SearchOutputPolicy::CoverageSummary);
            if compiled.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit {
                return Err("strict prefix requires the CoverageSummary+Omit projection".into());
            }
            Ok(compiled)
        } else {
            Ok(compiled)
        }
    }

    /// Independent tiling enumeration on a four-row rectangle. Its left edge
    /// is either one vertical I column or a four-column slab of four
    /// horizontal I pieces. Any tiling is a sequence of those blocks,
    /// achievable by filling each horizontal layer after the vertical pieces.
    fn four_row_i_identity_oracle(
        initial_board: u64,
        first_open_col: u8,
    ) -> BTreeSet<StandardBoard64TilingIdentity> {
        fn visit(
            col: u8,
            initial_board: u64,
            placed: &mut Vec<PiecePlacementMask>,
            identities: &mut BTreeSet<StandardBoard64TilingIdentity>,
        ) {
            if col == 10 {
                identities.insert(
                    StandardBoard64TilingIdentity::from_placements(
                        initial_board,
                        placed.iter().copied(),
                    )
                    .expect("oracle's placements are disjoint"),
                );
                return;
            }
            let vertical = (0..4).fold(0_u64, |mask, row| mask | (1_u64 << (row * 10 + col)));
            placed.push(PiecePlacementMask::new(PieceKind::I, vertical));
            visit(col + 1, initial_board, placed, identities);
            placed.pop();
            if col <= 6 {
                let checkpoint = placed.len();
                for row in 0..4 {
                    placed.push(PiecePlacementMask::new(
                        PieceKind::I,
                        0xf_u64 << (row * 10 + col),
                    ));
                }
                visit(col + 4, initial_board, placed, identities);
                placed.truncate(checkpoint);
            }
        }
        let mut identities = BTreeSet::new();
        visit(
            first_open_col,
            initial_board,
            &mut Vec::with_capacity(usize::from(10 - first_open_col)),
            &mut identities,
        );
        identities
    }

    #[derive(Clone, Copy)]
    struct DomainValidationModes {
        incremental: bool,
        same_tile_reuse: bool,
        minimum_cutoff: bool,
        minimum_work: bool,
        row_stack_geometry: bool,
    }

    fn validate_empty_minimum_work(
        geometry: GeometryDomainExperimentTotals,
        cutoff: bool,
        work: bool,
        errors: &mut Vec<String>,
    ) {
        if geometry.completed_families == 0
            || geometry.minimum_empty_origin == 0
            || geometry.advanced_full_compiles != 0
            || geometry.advanced_incremental_compiles != 0
        {
            errors.push("empty ten-I must actually use the empty-origin minimum route".into());
        }
        if work {
            if geometry.minimum_profiled_compiles != geometry.minimum_compiles
                || geometry.minimum_profiled_compiles == 0
                || geometry.minimum_incidence_rows_scanned == 0
            {
                errors.push("empty ten-I work mode must account for every minimum compile".into());
            }
            if cutoff != (geometry.minimum_incidence_rows_skipped > 0)
                || cutoff != (geometry.minimum_cells_cut_off > 0)
            {
                errors
                    .push("empty ten-I work counters did not match the selected cutoff arm".into());
            }
        } else if geometry.minimum_profiled_compiles != 0
            || geometry.minimum_incidence_rows_scanned != 0
            || geometry.minimum_incidence_rows_skipped != 0
            || geometry.minimum_cells_cut_off != 0
        {
            errors.push("empty ten-I timing mode requires the work counters to be disabled".into());
        }
    }

    fn validate_grounded_o(
        result: &CoreExecutionResult,
        fixture: Fixture,
        work: GeometricEdgeReuseMetrics,
        reuse: bool,
        errors: &mut Vec<String>,
    ) {
        let all = fixture == Fixture::GroundedOTwoLinesAll;
        // The only four unoccupied cells form an O on the floor. In the
        // SRS90 table each of its four spawn rotations descends in the
        // unobstructed two columns and is already grounded on the floor.
        // CountAll retains all four states; Unique takes the first harddrop
        // without reaching the other three equal-footprint validations.
        let identity = StandardBoard64TilingIdentity::from_placements(
            GROUNDED_O_INITIAL,
            [PiecePlacementMask::new(PieceKind::O, GROUNDED_O_CELLS)],
        )
        .expect("the literal one-O tiling forms a canonical identity");
        if result.normalized_solution_identities() != [identity]
            || result.coverage_pattern_words() != [1]
            || result.usize_field("coverage_pattern_count") != Some(1)
            || result.usize_field("covered_pattern_count") != Some(1)
            || result.bool_field("solution_probability_complete") != Some(true)
            || result
                .field("coverage_probability")
                .and_then(|value| value.parse::<f64>().ok())
                != Some(1.0)
        {
            errors.push(
                "grounded O must return the unique literal tiling and its fixed queue".into(),
            );
        }
        if result.field("build_variant_count") != Some(if all { "4" } else { "0" })
            || result.bool_field("build_variant_count_exact") != Some(all)
            || result.usize_field("total_build_order_nodes") != Some(2)
        {
            errors.push("grounded O must build exactly two subset nodes and preserve concrete rotation count semantics".into());
        }
        let expected = if reuse {
            GeometricEdgeReuseMetrics {
                footprint_validations: 1,
                reused_validations: if all { 3 } else { 0 },
                line_clear_computations: 1,
                projection_validations: 1,
            }
        } else {
            GeometricEdgeReuseMetrics::default()
        };
        if work != expected {
            errors.push(format!(
                "grounded O geometry work differs from the per-pose oracle: expected={expected:?}, actual={work:?}"
            ));
        }
    }

    fn validate(
        result: &CoreExecutionResult,
        fixture: Fixture,
        workers: usize,
        geometry: GeometryDomainExperimentTotals,
        modes: DomainValidationModes,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if modes.row_stack_geometry
            && result.field("packing_candidate_domain") != Some("row-stack-noncrossing")
        {
            errors.push(format!(
                "row-stack geometry was requested but the result reported packing_candidate_domain={:?}",
                result.field("packing_candidate_domain"),
            ));
        }
        for key in ["count_complete", "probability_complete"] {
            if result.bool_field(key) != Some(true) {
                errors.push(format!("{key} must be true"));
            }
        }
        if result.bool_field("resource_truncated") != Some(false) {
            errors.push("resource_truncated must be false".into());
        }
        let expected_count = match fixture {
            Fixture::OneI | Fixture::GroundedOTwoLinesAll | Fixture::GroundedOTwoLinesUnique => 1,
            Fixture::NonemptyEightI => 7,
            Fixture::EmptyTenI => 14,
            Fixture::P7 => CoverageUniverse::P7.solution_count() as usize,
            Fixture::P7P4 => LARGE_COUNT,
        };
        if result.usize_field("normalized_unique_solution_count") != Some(expected_count) {
            errors.push("completed solution count differs from fixture oracle".into());
        }
        if fixture == Fixture::P7P4 {
            if result.field("normalized_solution_set_hash") != Some(LARGE_HASH) {
                errors.push("completed solution-set hash differs from Jstris180 oracle".into());
            }
            if result.usize_field("workers_used") != Some(workers) {
                errors.push(format!(
                    "workers_used must equal detected auto limit {workers}"
                ));
            }
            if workers > 1 && result.bool_field("cpu_parallel_execution") != Some(true) {
                errors.push("large run must use the detected parallel topology".into());
            }
            if workers > 1 && result.usize_field("parallel_active_workers") != Some(workers) {
                errors.push(format!(
                    "parallel_active_workers must equal detected auto limit {workers}"
                ));
            }
        } else if matches!(fixture, Fixture::NonemptyEightI | Fixture::EmptyTenI) {
            let (board, first_open_col) = if fixture == Fixture::NonemptyEightI {
                (EIGHT_I_INITIAL, 2)
            } else {
                (0, 0)
            };
            let label = fixture.name();
            let actual_identities = result.normalized_solution_identities();
            if actual_identities.len() != expected_count
                || actual_identities.iter().copied().collect::<BTreeSet<_>>()
                    != four_row_i_identity_oracle(board, first_open_col)
            {
                errors.push(format!(
                    "{label}: canonical identities differ from the rectangle oracle"
                ));
            }
            // This compact CPU result exposes coverage_calculated and
            // probability_complete (checked above), not coverage_complete.
            // Its exact universe and bitmap prove completion of coverage.
            let calculated = result.bool_field("coverage_calculated");
            let patterns = result.usize_field("coverage_pattern_count");
            let covered = result.usize_field("covered_pattern_count");
            let probability = result
                .field("coverage_probability")
                .and_then(|value| value.parse::<f64>().ok());
            let words = result.coverage_pattern_words();
            if calculated != Some(true)
                || patterns != Some(1)
                || covered != Some(1)
                || probability != Some(1.0)
                || words != [1]
            {
                errors.push(format!(
                    "{label} must completely cover its only fixed queue: calculated={calculated:?}, patterns={patterns:?}, covered={covered:?}, probability={probability:?}, words={}, first word={:?}",
                    words.len(),
                    words.first(),
                ));
            }
            // The compact fixture permits automatic topology admission to
            // use fewer workers than P7P4. --compare checks actual topology.
            if !result
                .usize_field("workers_used")
                .is_some_and(|used| (1..=workers).contains(&used))
            {
                errors.push(format!(
                    "{label} must report an actual worker count within the Auto cap"
                ));
            }
            if modes.row_stack_geometry {
                // Identity, coverage and worker-topology checks above still
                // apply. Only residual-domain work is absent in this route.
                if geometry.minimum_compiles != 0
                    || geometry.advanced_full_compiles != 0
                    || geometry.advanced_incremental_compiles != 0
                {
                    errors
                        .push("row-stack geometry must bypass residual-domain compilation".into());
                }
                return errors;
            }
            if fixture == Fixture::EmptyTenI {
                validate_empty_minimum_work(
                    geometry,
                    modes.minimum_cutoff,
                    modes.minimum_work,
                    &mut errors,
                );
                return errors;
            }
            let (expected, excluded) = if modes.incremental {
                (
                    geometry.advanced_incremental_compiles,
                    geometry.advanced_full_compiles,
                )
            } else {
                (
                    geometry.advanced_full_compiles,
                    geometry.advanced_incremental_compiles,
                )
            };
            if geometry.completed_families == 0 || expected == 0 || excluded != 0 {
                errors.push("eight-I must actually execute its selected advanced route".into());
            }
            if modes.incremental != (geometry.support_cells_reused > 0) {
                errors.push("eight-I support-cell reuse did not match the incremental arm".into());
            }
            if modes.same_tile_reuse != (geometry.same_tile_parent_reuses > 0) {
                errors.push("eight-I parent reuse did not match the SameTile arm".into());
            }
            if !modes.same_tile_reuse && geometry.same_tile_parent_scans == 0 {
                errors
                    .push("eight-I reference arm must perform actual SameTile parent scans".into());
            }
        }
        errors
    }

    fn file_sha(path: &Path) -> Result<String, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    fn scalar_demo() -> Result<(), String> {
        let pieces = PieceKind::STANDARD_TETROMINOES;
        let cases: [(&str, &[PieceKind], &[u16], u128, u128, bool); 4] = [
            (
                "seven independent pieces",
                &pieces,
                &[0; 7],
                5040,
                5040,
                false,
            ),
            (
                "seven-piece chain",
                &pieces,
                &[0, 1, 2, 4, 8, 16, 32],
                1,
                5040,
                false,
            ),
            (
                "three-piece chain and four independent pieces",
                &pieces,
                &[0, 1, 2, 0, 0, 0, 0],
                840,
                5040,
                false,
            ),
            (
                "four-piece irreducible N order",
                &pieces[..4],
                &[0, 0, 3, 2],
                5,
                840,
                true,
            ),
        ];
        let mut rows = Vec::new();
        let mut errors = Vec::new();
        for (name, kinds, predecessors, expected_count, expected_total, requires_ideals) in cases {
            let count = clearra_core_executor::backend::count_distinct_no_hold_one_bag_poset(
                kinds,
                predecessors,
            )
            .ok_or_else(|| format!("{name}: supported distinct-kind poset was rejected"))?;
            if count.accepted_queue_count != expected_count
                || count.total_queue_count != expected_total
            {
                errors.push(format!(
                    "{name}: exact queue counts differ from the fixture oracle"
                ));
            }
            if (count.ideal_states > 0) != requires_ideals {
                errors.push(format!("{name}: unexpected decomposition/ideal-count path"));
            }
            rows.push(json!({
                "case": name,
                "pieces_in_operation_order": kinds.iter().map(|piece| piece.as_ascii().to_string()).collect::<Vec<_>>(),
                "predecessor_masks": predecessors,
                "mask_convention": "bit i names concrete operation i in pieces_in_operation_order",
                "accepted_queue_count": count.accepted_queue_count,
                "total_queue_count": count.total_queue_count,
                "exact_probability": {
                    "numerator": count.accepted_queue_count,
                    "denominator": count.total_queue_count,
                    "fraction": format!("{}/{}", count.accepted_queue_count, count.total_queue_count)
                },
                "probability_approximate": count.accepted_queue_count as f64 / count.total_queue_count as f64,
                "decomposed_subproblems": count.decomposed_subproblems,
                "ideal_states": count.ideal_states,
                "oracle": { "accepted_queue_count": expected_count, "total_queue_count": expected_total }
            }));
        }
        let binary_sha = file_sha(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        println!("{}", serde_json::to_string_pretty(&json!({
            "schema": "clearra.poset-scalar-demonstration.v1",
            "binary_sha256": binary_sha,
            "scope": "explicit fixed posets supplied directly; no field or placement certificate is claimed",
            "assumptions": {
                "hold": "disabled",
                "piece_kinds": "distinct, at most seven",
                "source": "fresh standard seven-piece bag",
                "draw_count": "exactly k draws, where k is the operation count",
                "denominator": "7Pk; every target piece must occur among those first k draws",
                "four_piece_N_probability": "5/840 unconditional; 5/24 would condition on drawing precisely its four piece kinds"
            },
            "field_search_performed": false,
            "placement_certificate_generated": false,
            "materialized_queue_count": 0,
            "cases": rows,
            "valid": errors.is_empty(),
            "validation_errors": errors
        })).map_err(|e| e.to_string())?);
        if !errors.is_empty() {
            return Err("scalar demonstration failed its exact-count validation".into());
        }
        Ok(())
    }

    fn report_experiment_mode<'a>(report: &'a Value, key: &str) -> Result<&'a str, String> {
        match report.get(key) {
            None => Ok("0"),
            Some(value) => value
                .as_str()
                .filter(|value| matches!(*value, "0" | "1"))
                .ok_or_else(|| format!("{key} must be a string containing 0 or 1")),
        }
    }

    fn report_candidate_domain(report: &Value) -> Result<&str, String> {
        fn domain_field(value: &Value) -> Result<Option<&str>, String> {
            if value.is_null() {
                Ok(None)
            } else {
                value.as_str().map(Some).ok_or_else(|| {
                    "packing candidate domain must be a string when present".to_owned()
                })
            }
        }
        let summary = domain_field(&report["summary"]["packing_candidate_domain"])?;
        let observed = domain_field(&report["observed_geometry_candidate_domain"])?;
        if summary
            .zip(observed)
            .is_some_and(|(left, right)| left != right)
        {
            return Err(
                "packing candidate domain disagrees with observed geometry metadata".into(),
            );
        }
        // Old reports predate this axis and therefore describe the raw family.
        // A declared row-stack arm must provide observed noncrossing evidence.
        let domain = summary.or(observed).unwrap_or("raw");
        let row_stack = report_experiment_mode(report, "row_stack_geometry_mode")?;
        match (domain, row_stack) {
            ("raw", "0") | ("row-stack-noncrossing", "1") => Ok(domain),
            _ => Err(format!(
                "declared row-stack mode {row_stack} does not match packing candidate domain {domain}"
            )),
        }
    }

    fn comparable_candidate_domains(first: &Value, other: &Value) -> Result<bool, String> {
        if report_candidate_domain(first)? == report_candidate_domain(other)? {
            return Ok(true);
        }
        validate_isolated_row_stack_axes(first, other)?;
        Ok(false)
    }

    fn validate_isolated_row_stack_axes(first: &Value, other: &Value) -> Result<(), String> {
        // Changing the candidate universe is legitimate only for an isolated,
        // declared row-stack switch. APDP-only A/B keeps exact raw comparison.
        for key in [
            "mode",
            "structural_mode",
            "static_influence_mode",
            "translation_interval_mode",
            "movement_clear_certificates_mode",
            "incremental_domain_mode",
            "minimum_domain_cutoff_mode",
            "minimum_domain_work_mode",
            "apdp_parent_recheck_mode",
            "first_kick_interval_mode",
            "ordered_kick_interval_mode",
            "ordered_kick_guard_reuse_mode",
            "geometric_edge_reuse_mode",
            "ilc_lazy_frame_mode",
            "no_entry_components_mode",
            "same_tile_parent_reuse_mode",
            "conditional_order_mode",
            "terminal_identity_move_mode",
            "operation_mask_mode",
            "strict_prefix_mode",
        ] {
            if report_experiment_mode(first, key)? != report_experiment_mode(other, key)? {
                return Err(format!(
                    "different candidate domains require only the declared row-stack axis to change; {key} also changed"
                ));
            }
        }
        Ok(())
    }

    fn compare(paths: &[String]) -> Result<(), String> {
        if paths.len() < 2 {
            return Err("--compare requires at least two result files".into());
        }
        let reports: Vec<Value> = paths
            .iter()
            .map(|path| {
                let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
                serde_json::from_reader(file).map_err(|e| format!("{path}: {e}"))
            })
            .collect::<Result<_, String>>()?;
        let first = &reports[0];
        let mut candidate_domains_differ = false;
        let operation_mask_comparison = reports
            .iter()
            .any(|report| report.get("operation_mask_mode").is_some());
        let rectangle_fixture = [Fixture::NonemptyEightI, Fixture::EmptyTenI]
            .into_iter()
            .any(|fixture| first["case"] == fixture.name());
        let grounded_o_fixture = [
            Fixture::GroundedOTwoLinesAll,
            Fixture::GroundedOTwoLinesUnique,
        ]
        .into_iter()
        .any(|fixture| first["case"] == fixture.name());
        let mut differences = Vec::new();
        let summary_keys = [
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "build_variant_count",
            "build_variant_count_exact",
            "coverage_pattern_count",
            "covered_pattern_count",
            "coverage_probability",
            "materialized_probability_mass",
            "count_complete",
            "coverage_complete",
            "probability_complete",
            "solution_probability_complete",
            "resource_truncated",
        ];
        for (index, report) in reports.iter().enumerate() {
            if report["valid"] != true {
                differences.push(format!("{}: invalid source result", paths[index]));
            }
            for key in [
                "schema",
                "case",
                "binary_sha256",
                "input",
                "cpu",
                "other_process_configuration",
                "output_mode",
                "coverage",
                "materialized_identities",
            ] {
                if report[key] != first[key] {
                    differences.push(format!("{}: {key} mismatch", paths[index]));
                }
            }
            for key in summary_keys {
                if report["summary"][key] != first["summary"][key] {
                    differences.push(format!("{}: summary.{key} mismatch", paths[index]));
                }
            }
            let same_candidate_domain = comparable_candidate_domains(first, report)?;
            candidate_domains_differ |= !same_candidate_domain;
            if same_candidate_domain {
                for key in ["packing_candidate_count", "packing_candidate_set_digest"] {
                    if report["summary"][key] != first["summary"][key] {
                        differences.push(format!("{}: summary.{key} mismatch", paths[index]));
                    }
                }
            }
            if rectangle_fixture || grounded_o_fixture {
                if report["output_mode"] != "global-only" {
                    differences.push(format!("{}: fixture must be global-only", paths[index]));
                }
            }
            if rectangle_fixture || grounded_o_fixture || operation_mask_comparison {
                for key in ["workers_used", "cpu_parallel_execution"] {
                    if report["summary"][key].is_null()
                        || report["summary"][key] != first["summary"][key]
                    {
                        differences
                            .push(format!("{}: actual topology {key} mismatch", paths[index]));
                    }
                }
            }
            if operation_mask_comparison {
                let proof = &report["operation_masks"];
                let reference_proof = &first["operation_masks"];
                let enabled = match report["operation_mask_mode"].as_str() {
                    Some("0") => false,
                    Some("1") => true,
                    _ => {
                        return Err(
                            "operation-mask comparison needs explicit modes in all arms".into()
                        )
                    }
                };
                let summary_metric =
                    |key: &str| report["summary"][key].as_str()?.parse::<usize>().ok();
                if let Err(error) = validate_operation_mask_evidence(
                    enabled,
                    proof,
                    summary_metric("workers_used"),
                    summary_metric("total_build_order_nodes"),
                    summary_metric("worker_graph_projection_retained_bytes"),
                ) {
                    differences.push(format!("{}: {error}", paths[index]));
                }
                if report["case"] != Fixture::P7P4.name()
                    || !valid_experiment_source(report["compiled_source_commit"].as_str())
                    || report["compiled_source_commit"] != first["compiled_source_commit"]
                    || report["materialized_identities"]["covers_reported_family"] != true
                    || report["materialized_identities"]["count"].as_u64()
                        != Some(LARGE_COUNT as u64)
                    || summary_metric("coverage_pattern_count") != Some(4_233_600)
                    || report["summary"]["normalized_solution_set_hash"] != LARGE_HASH
                {
                    differences.push(format!(
                        "{}: source-bound full P7P4 result evidence is required",
                        paths[index]
                    ));
                }
                if ["visited_subsets", "selected_edges"]
                    .into_iter()
                    .any(|key| proof[key] != reference_proof[key])
                {
                    differences.push(format!(
                        "{}: selected physical traversal work differs",
                        paths[index]
                    ));
                }
                let pass_count = |evidence: &Value| {
                    evidence["graph_passes"]
                        .as_u64()?
                        .checked_add(evidence["mask_passes"].as_u64()?)
                };
                if pass_count(proof) != pass_count(reference_proof) {
                    differences.push(format!(
                        "{}: number of physical traversal passes differs",
                        paths[index]
                    ));
                }
                if report["operation_mask_representative"]
                    .as_array()
                    .is_none_or(|steps| steps.len() != Fixture::P7P4.placed_pieces())
                    || report["operation_mask_representative"]
                        != first["operation_mask_representative"]
                    || report["summary"]["representative_pattern_id"]
                        != first["summary"]["representative_pattern_id"]
                {
                    differences.push(format!(
                        "{}: concrete representative replay differs",
                        paths[index]
                    ));
                }
            }
        }
        if candidate_domains_differ {
            // Do not admit an additional APDP or other treatment merely
            // because that report shares the first report's raw domain.
            for report in &reports {
                validate_isolated_row_stack_axes(first, report)?;
            }
        }
        // Missing experimental axes in old reports mean disabled. Keep their
        // original grouping when every report predates that axis. A binary
        // mismatch remains a parity error, even for otherwise compatible JSON.
        let has_static_influence_mode = reports
            .iter()
            .any(|report| report.get("static_influence_mode").is_some());
        let has_translation_interval_mode = reports
            .iter()
            .any(|report| report.get("translation_interval_mode").is_some());
        let legacy_modes = reports
            .iter()
            .all(|report| report.get("structural_mode").is_none())
            && !has_static_influence_mode
            && !has_translation_interval_mode;
        let report_variant = |dependency: &str,
                              structural: &str,
                              static_influence: &str,
                              translation_interval: &str| {
            if legacy_modes {
                dependency.to_owned()
            } else if has_translation_interval_mode {
                variant(
                    dependency,
                    structural,
                    static_influence,
                    translation_interval,
                )
            } else if has_static_influence_mode {
                format!("dependency={dependency},structural={structural},static_influence={static_influence}")
            } else {
                format!("dependency={dependency},structural={structural}")
            }
        };
        let mut times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut all_axes_times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut domain_work: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        let mut operation_mask_times = [Vec::new(), Vec::new()];
        for report in &reports {
            let dependency_mode = report["mode"].as_str().ok_or("mode missing")?;
            let structural_mode = match report.get("structural_mode") {
                None => "0",
                Some(value) => value.as_str().ok_or("structural_mode must be a string")?,
            };
            let static_influence_mode = match report.get("static_influence_mode") {
                None => "0",
                Some(value) => value
                    .as_str()
                    .ok_or("static_influence_mode must be a string")?,
            };
            let translation_interval_mode = match report.get("translation_interval_mode") {
                None => "0",
                Some(value) => value
                    .as_str()
                    .ok_or("translation_interval_mode must be a string")?,
            };
            if !matches!(dependency_mode, "0" | "1")
                || !matches!(structural_mode, "0" | "1")
                || !matches!(static_influence_mode, "0" | "1")
                || !matches!(translation_interval_mode, "0" | "1")
            {
                return Err("result mode flags must be 0 or 1".into());
            }
            let mode = report_variant(
                dependency_mode,
                structural_mode,
                static_influence_mode,
                translation_interval_mode,
            );
            let ms = report["timing_ms"]["search"]
                .as_f64()
                .ok_or("search time missing")?;
            if !ms.is_finite() || ms <= 0.0 {
                return Err("search time must be finite and positive".into());
            }
            let extra_mode = |key: &str| report_experiment_mode(report, key);
            let movement = extra_mode("movement_clear_certificates_mode")?;
            let incremental = extra_mode("incremental_domain_mode")?;
            let minimum_domain_cutoff = extra_mode("minimum_domain_cutoff_mode")?;
            let minimum_domain_work = extra_mode("minimum_domain_work_mode")?;
            let row_stack_geometry = extra_mode("row_stack_geometry_mode")?;
            let apdp_parent_recheck = extra_mode("apdp_parent_recheck_mode")?;
            let first_kick = extra_mode("first_kick_interval_mode")?;
            let ordered_kick = extra_mode("ordered_kick_interval_mode")?;
            let ordered_kick_guard_reuse = extra_mode("ordered_kick_guard_reuse_mode")?;
            let geometric_edge_reuse = extra_mode("geometric_edge_reuse_mode")?;
            let ilc_lazy_frame = extra_mode("ilc_lazy_frame_mode")?;
            let no_entry_components = extra_mode("no_entry_components_mode")?;
            let same_tile_parent_reuse = extra_mode("same_tile_parent_reuse_mode")?;
            let conditional = extra_mode("conditional_order_mode")?;
            let terminal_move = extra_mode("terminal_identity_move_mode")?;
            let mut all_axes = format!(
                "{mode},movement_clear={movement},incremental_domain={incremental},first_kick={first_kick},ordered_kick={ordered_kick},ordered_kick_guard_reuse={ordered_kick_guard_reuse},geometric_edge_reuse={geometric_edge_reuse},ilc_lazy_frame={ilc_lazy_frame},no_entry_components={no_entry_components},same_tile_parent_reuse={same_tile_parent_reuse},conditional_order={conditional},terminal_identity_move={terminal_move},minimum_domain_cutoff={minimum_domain_cutoff},minimum_domain_work={minimum_domain_work},row_stack_geometry={row_stack_geometry},apdp_parent_recheck={apdp_parent_recheck}"
            );
            if operation_mask_comparison {
                let op = extra_mode("operation_mask_mode")?;
                if [
                    dependency_mode,
                    structural_mode,
                    static_influence_mode,
                    translation_interval_mode,
                    movement,
                    incremental,
                    minimum_domain_cutoff,
                    minimum_domain_work,
                    row_stack_geometry,
                    apdp_parent_recheck,
                    first_kick,
                    ordered_kick,
                    ordered_kick_guard_reuse,
                    geometric_edge_reuse,
                    ilc_lazy_frame,
                    no_entry_components,
                    same_tile_parent_reuse,
                    conditional,
                    terminal_move,
                ]
                .into_iter()
                .any(|mode| mode != "0")
                {
                    return Err(
                        "operation-mask comparison may not mix other experiment axes".into(),
                    );
                }
                all_axes.push_str(&format!(",operation_masks={op}"));
                operation_mask_times[usize::from(op == "1")].push(ms);
            }
            if rectangle_fixture {
                let work = report
                    .get("geometry_domain")
                    .filter(|work| work.is_object())
                    .ok_or("I rectangle result is missing geometry_domain work counters")?;
                domain_work.entry(all_axes.clone()).or_default().push(work);
            }
            all_axes_times.entry(all_axes).or_default().push(ms);
            if !operation_mask_comparison
                && movement == "0"
                && incremental == "0"
                && minimum_domain_cutoff == "0"
                && minimum_domain_work == "0"
                && row_stack_geometry == "0"
                && apdp_parent_recheck == "0"
                && first_kick == "0"
                && ordered_kick == "0"
                && ordered_kick_guard_reuse == "0"
                && geometric_edge_reuse == "0"
                && ilc_lazy_frame == "0"
                && no_entry_components == "0"
                && same_tile_parent_reuse == "0"
                && conditional == "0"
                && terminal_move == "0"
            {
                times.entry(mode).or_default().push(ms);
            }
        }
        let means: BTreeMap<String, f64> = times
            .iter()
            .map(|(mode, values)| {
                (
                    mode.clone(),
                    values.iter().sum::<f64>() / values.len() as f64,
                )
            })
            .collect();
        let dependency_baseline = report_variant("0", "0", "0", "0");
        let dependency_treatment = report_variant("1", "0", "0", "0");
        let structural_treatment = report_variant("1", "1", "0", "0");
        let static_influence_treatment = report_variant("1", "0", "1", "0");
        let static_structural_treatment = report_variant("1", "1", "1", "0");
        let translation_treatment = report_variant("1", "1", "0", "1");
        let all_experiments_treatment = report_variant("1", "1", "1", "1");
        let time_change = |baseline: &str, treatment: &str| {
            means
                .get(baseline)
                .zip(means.get(treatment))
                .map(|(a, b)| (b / a - 1.0) * 100.0)
        };
        let dependency_change = time_change(&dependency_baseline, &dependency_treatment);
        let structural_change = (!legacy_modes)
            .then(|| time_change(&dependency_treatment, &structural_treatment))
            .flatten();
        let combined_change = (!legacy_modes)
            .then(|| time_change(&dependency_baseline, &structural_treatment))
            .flatten();
        let static_influence_change = has_static_influence_mode
            .then(|| time_change(&dependency_treatment, &static_influence_treatment))
            .flatten();
        let static_influence_over_structural_change = has_static_influence_mode
            .then(|| time_change(&structural_treatment, &static_structural_treatment))
            .flatten();
        let translation_change = has_translation_interval_mode
            .then(|| time_change(&structural_treatment, &translation_treatment))
            .flatten();
        let translation_over_static_change = has_translation_interval_mode
            .then(|| time_change(&static_structural_treatment, &all_experiments_treatment))
            .flatten();
        let all_experiments_change = (has_static_influence_mode || has_translation_interval_mode)
            .then(|| time_change(&dependency_baseline, &all_experiments_treatment))
            .flatten();
        let (primary_baseline, primary_treatment, change) = if translation_change.is_some() {
            (
                &structural_treatment,
                &translation_treatment,
                translation_change,
            )
        } else if translation_over_static_change.is_some() {
            (
                &static_structural_treatment,
                &all_experiments_treatment,
                translation_over_static_change,
            )
        } else if static_influence_change.is_some() {
            (
                &dependency_treatment,
                &static_influence_treatment,
                static_influence_change,
            )
        } else if static_influence_over_structural_change.is_some() {
            (
                &structural_treatment,
                &static_structural_treatment,
                static_influence_over_structural_change,
            )
        } else if structural_change.is_some() {
            (
                &dependency_treatment,
                &structural_treatment,
                structural_change,
            )
        } else {
            (
                &dependency_baseline,
                &dependency_treatment,
                dependency_change,
            )
        };
        let mut comparison = json!({
            "schema": "clearra.placement-graph-comparison.v1", "files": paths,
            "equivalent": differences.is_empty(), "differences": differences,
            "raw_candidate_comparison": if candidate_domains_differ { "not-equivalent-domains" } else { "exact-matching-domain" },
            "mode_grouping": if legacy_modes { "dependency-only (legacy reports)" }
                else if has_translation_interval_mode { "dependency-structural-static-influence-translation-interval" }
                else if has_static_influence_mode { "dependency-structural-static-influence" }
                else { "dependency-and-structural" },
            "search_times_ms_by_mode": times, "mean_search_ms_by_mode": means,
            "search_times_ms_by_all_axes": all_axes_times,
            "treatment_time_change_percent": change,
            "primary_comparison": { "baseline": primary_baseline, "treatment": primary_treatment },
            "time_change_percent": {
                "dependency_vs_disabled": dependency_change,
                "structural_vs_dependency": structural_change,
                "combined_vs_disabled": combined_change,
                "static_influence_vs_dependency": static_influence_change,
                "static_influence_vs_structural": static_influence_over_structural_change,
                "translation_interval_vs_structural": translation_change,
                "translation_interval_vs_static_structural": translation_over_static_change,
                "all_experiments_vs_disabled": all_experiments_change
            },
            "scope": "same binary/input/CPU, exact reported full-set hash and global coverage; no p95 claim"
        });
        if rectangle_fixture {
            comparison["geometry_domain_work_samples_by_all_axes"] = json!(domain_work);
        }
        if operation_mask_comparison {
            let [control, treatment] = operation_mask_times.each_ref();
            if control.is_empty() || treatment.is_empty() {
                return Err(
                    "operation-mask comparison needs at least one actual control and treatment"
                        .into(),
                );
            }
            let mean = |times: &[f64]| times.iter().sum::<f64>() / times.len() as f64;
            comparison["primary_comparison"] = json!({
                "baseline": "concrete_graph",
                "treatment": "operation_masks",
            });
            comparison["mode_grouping"] = json!("operation-masks");
            comparison["operation_mask_search_times_ms"] = json!({
                "concrete_graph": control,
                "operation_masks": treatment,
            });
            comparison["treatment_time_change_percent"] = if differences.is_empty() {
                json!((mean(treatment) / mean(control) - 1.0) * 100.0)
            } else {
                Value::Null
            };
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&comparison).map_err(|e| e.to_string())?
        );
        if !differences.is_empty() {
            return Err("benchmark result parity failed".into());
        }
        Ok(())
    }

    fn variant(
        dependency_mode: &str,
        structural_mode: &str,
        static_influence_mode: &str,
        translation_interval_mode: &str,
    ) -> String {
        format!(
            "dependency={dependency_mode},structural={structural_mode},static_influence={static_influence_mode},translation_interval={translation_interval_mode}"
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn candidate_comparison_requires_observed_domains_and_an_isolated_row_stack_switch() {
            let raw = json!({
                "row_stack_geometry_mode": "0",
                "apdp_parent_recheck_mode": "0",
                "summary": {"packing_candidate_domain": "raw"},
            });
            let row_stack = json!({
                "row_stack_geometry_mode": "1",
                "apdp_parent_recheck_mode": "0",
                "observed_geometry_candidate_domain": "row-stack-noncrossing",
                "summary": {"packing_candidate_domain": "row-stack-noncrossing"},
            });
            assert_eq!(comparable_candidate_domains(&raw, &row_stack), Ok(false));
            let mut apdp_reference = raw.clone();
            apdp_reference["apdp_parent_recheck_mode"] = json!("1");
            assert_eq!(
                comparable_candidate_domains(&raw, &apdp_reference),
                Ok(true)
            );
            assert!(comparable_candidate_domains(&apdp_reference, &row_stack).is_err());
            assert!(validate_isolated_row_stack_axes(&raw, &apdp_reference).is_err());

            let mut undeclared = row_stack.clone();
            undeclared["row_stack_geometry_mode"] = json!("0");
            assert!(report_candidate_domain(&undeclared).is_err());
            let mut unobserved = raw.clone();
            unobserved["row_stack_geometry_mode"] = json!("1");
            assert!(report_candidate_domain(&unobserved).is_err());
            let mut inconsistent = row_stack.clone();
            inconsistent["observed_geometry_candidate_domain"] = json!("raw");
            assert!(report_candidate_domain(&inconsistent).is_err());
            assert_eq!(report_candidate_domain(&json!({})), Ok("raw"));
            for invalid in [json!("yes"), json!(2), json!(true), Value::Null] {
                let report = json!({"row_stack_geometry_mode": invalid});
                assert!(report_experiment_mode(&report, "row_stack_geometry_mode").is_err());
            }
        }

        #[test]
        fn row_stack_admission_rejects_unqualified_grounded_probability_fixtures() {
            for fixture in [
                Fixture::GroundedOTwoLinesAll,
                Fixture::GroundedOTwoLinesUnique,
            ] {
                assert!(validate_row_stack_fixture(fixture, true).is_err());
                assert!(validate_row_stack_fixture(fixture, false).is_ok());
            }
            for fixture in [
                Fixture::OneI,
                Fixture::P7P4,
                Fixture::NonemptyEightI,
                Fixture::EmptyTenI,
            ] {
                assert!(validate_row_stack_fixture(fixture, true).is_ok());
            }
        }

        #[test]
        fn product_diagnostic_p7_and_small_admit_labeled_geometry_arms() {
            for flag in ["--p7", "--small"] {
                let args = vec![flag.to_owned()];
                let (fixture, capture) = parse_fixture(&args).unwrap();
                assert!(capture.is_none());
                for cutoff in [false, true] {
                    let diagnostic = SearchStageDiagnostic::select_product(
                        true,
                        None,
                        false,
                        capture.is_some(),
                        true,
                        true,
                        cutoff,
                    )
                    .unwrap();
                    assert_eq!(
                        diagnostic,
                        Some(if cutoff {
                            SearchStageDiagnostic::ProductCutoffExperiment
                        } else {
                            SearchStageDiagnostic::ProductDefault
                        })
                    );
                    assert!(validate_fixture_diagnostic_route(
                        fixture, diagnostic, None, cutoff, None, false,
                    )
                    .is_ok());
                    assert!(validate_product_memo_environment(|name| {
                        if name == "CLEARRA_STANDARD_BAG_UNION_MEMO" {
                            Ok("compact".to_owned())
                        } else {
                            Err(std::env::VarError::NotPresent)
                        }
                    })
                    .is_ok());
                    for row_stack in [false, true] {
                        for apdp_recheck in [false, true] {
                            let labeled = diagnostic.map(|diagnostic| {
                                diagnostic.with_geometry_experiments(row_stack, apdp_recheck)
                            });
                            assert!(validate_fixture_diagnostic_route(
                                fixture, labeled, None, cutoff, None, false,
                            )
                            .is_ok());
                        }
                    }
                }
            }
            assert!(
                validate_fixture_diagnostic_route(Fixture::P7, None, None, true, None, false,)
                    .is_err()
            );
            // Historical strict A/B still admits both explicit reference arms.
            for strict_arm in [false, true] {
                assert!(validate_fixture_diagnostic_route(
                    Fixture::P7,
                    None,
                    Some(strict_arm),
                    false,
                    None,
                    false,
                )
                .is_ok());
            }
        }

        #[test]
        fn grounded_o_defaults_and_product_cutoff_profiles_preserve_reuse_isolation() {
            for fixture in [
                Fixture::GroundedOTwoLinesAll,
                Fixture::GroundedOTwoLinesUnique,
            ] {
                assert!(
                    validate_fixture_diagnostic_route(fixture, None, None, false, None, false,)
                        .is_ok()
                );
                for cutoff in [false, true] {
                    let diagnostic = SearchStageDiagnostic::select_product(
                        true, None, false, false, true, true, cutoff,
                    )
                    .unwrap();
                    for disabled in [None, Some(false)] {
                        assert!(validate_fixture_diagnostic_route(
                            fixture, diagnostic, None, cutoff, disabled, false,
                        )
                        .is_ok());
                    }
                    assert!(validate_fixture_diagnostic_route(
                        fixture,
                        diagnostic,
                        None,
                        cutoff,
                        Some(true),
                        false,
                    )
                    .is_err());
                    assert!(validate_fixture_diagnostic_route(
                        fixture, diagnostic, None, cutoff, None, true,
                    )
                    .is_err());
                }
                for reuse in [false, true] {
                    assert!(validate_fixture_diagnostic_route(
                        fixture,
                        None,
                        None,
                        false,
                        Some(reuse),
                        false,
                    )
                    .is_ok());
                    assert!(validate_fixture_diagnostic_route(
                        fixture,
                        None,
                        None,
                        true,
                        Some(reuse),
                        false,
                    )
                    .is_err());
                }
            }
        }

        #[cfg(feature = "wasm-stage-profiling")]
        #[test]
        fn product_default_and_cutoff_experiment_reports_keep_product_labels() {
            for (diagnostic, label) in [
                (SearchStageDiagnostic::ProductDefault, "product-default"),
                (
                    SearchStageDiagnostic::ProductCutoffExperiment,
                    "product-cutoff-experiment",
                ),
            ] {
                let report =
                    diagnostic.report(Vec::new(), Vec::new(), true, true, true, true, true, true);
                assert!(report.get("strict_stage_profile").is_none());
                assert_eq!(
                    report["product_stage_profile"]["execution_route"],
                    "ordinary-product"
                );
                assert_eq!(report["product_stage_profile"]["configuration"], label);
                assert_eq!(
                    report["product_stage_profile"]["timing_adoption_authority"],
                    false
                );
                assert_eq!(
                    report["product_stage_profile"]["upper_strategy"],
                    "not-applicable"
                );
            }
            for (
                visited_first,
                global_saturation,
                physical_prefix,
                candidate_ilc,
                boolean_membership,
                shared_saturation,
            ) in [
                (false, false, true, true, true, true),
                (true, false, true, true, true, true),
                (false, true, true, true, true, true),
                (true, true, false, true, true, true),
                (true, true, true, false, true, true),
                (true, true, true, true, false, true),
                (true, true, true, true, true, false),
            ] {
                let report = SearchStageDiagnostic::ProductDefault.report(
                    Vec::new(),
                    Vec::new(),
                    visited_first,
                    global_saturation,
                    physical_prefix,
                    candidate_ilc,
                    boolean_membership,
                    shared_saturation,
                );
                let profile = &report["product_stage_profile"];
                assert_eq!(profile["configuration"], "product-ablation");
                assert_eq!(
                    profile["product_optimizations"],
                    json!({
                        "minimum_domain_cutoff": false,
                        "row_stack_geometry": false,
                        "apdp_parent_recheck": false,
                        "reachability_visited_first": visited_first,
                        "standard_bag_global_saturation": global_saturation,
                        "physical_prefix_pruning": physical_prefix,
                        "candidate_ilc_pruning": candidate_ilc,
                        "standard_bag_boolean_membership": boolean_membership,
                        "standard_bag_shared_saturation": shared_saturation,
                    })
                );
                assert_eq!(profile["timing_adoption_authority"], false);
            }
        }

        #[cfg(feature = "wasm-stage-profiling")]
        #[test]
        fn row_stack_and_apdp_reference_reports_label_the_actual_geometry_route() {
            for cutoff in [false, true] {
                for row_stack in [false, true] {
                    for apdp_recheck in [false, true] {
                        let diagnostic = SearchStageDiagnostic::select_product(
                            true, None, false, false, true, true, cutoff,
                        )
                        .unwrap()
                        .unwrap()
                        .with_geometry_experiments(row_stack, apdp_recheck);
                        let report = diagnostic.report(
                            Vec::new(),
                            Vec::new(),
                            true,
                            true,
                            true,
                            true,
                            true,
                            true,
                        );
                        let profile = &report["product_stage_profile"];
                        assert!(report.get("strict_stage_profile").is_none());
                        assert_eq!(profile["execution_route"], diagnostic.execution_route());
                        assert_eq!(
                            profile["product_optimizations"]["minimum_domain_cutoff"],
                            cutoff
                        );
                        assert_eq!(
                            profile["product_optimizations"]["row_stack_geometry"],
                            row_stack
                        );
                        assert_eq!(
                            profile["product_optimizations"]["apdp_parent_recheck"],
                            apdp_recheck
                        );
                        assert_eq!(profile["timing_adoption_authority"], false);
                        assert_eq!(profile["upper_strategy"], "not-applicable");
                        assert!(profile["scope"]
                            .as_str()
                            .unwrap()
                            .contains("submitting-thread and pool-worker"));
                        if row_stack {
                            assert_eq!(profile["configuration"], "product-row-stack-experiment");
                            assert!(profile["geometry_domain_scope"]
                                .as_str()
                                .unwrap()
                                .starts_with("bypassed-by-row-stack"));
                        } else if apdp_recheck {
                            assert_eq!(profile["configuration"], "product-apdp-reference");
                        }
                    }
                }
            }
        }

        #[test]
        fn report_sections_preserve_flat_keys_and_reject_collisions() {
            let report = join_report_sections([
                json!({"schema": "fixture"}),
                json!({"geometry_domain": {"completed_families": 1}}),
                json!({"valid": true}),
            ])
            .unwrap();
            assert_eq!(
                report,
                json!({"schema": "fixture", "geometry_domain": {"completed_families": 1}, "valid": true})
            );
            assert_eq!(
                join_report_sections([json!({"case": "a"}), json!({"case": "b"})]).unwrap_err(),
                "duplicate benchmark report key: case"
            );
        }

        #[test]
        fn fixture_selection_keeps_capture_universes_separate() {
            let parse = |arguments: &[&str]| {
                let arguments: Vec<String> =
                    arguments.iter().map(|value| (*value).to_owned()).collect();
                parse_fixture(&arguments).map(|(fixture, slot)| (fixture, slot.map(str::to_owned)))
            };
            assert_eq!(parse(&[]).unwrap(), (Fixture::P7P4, None));
            assert_eq!(parse(&["--summary"]).unwrap(), (Fixture::P7P4, None));
            assert_eq!(parse(&["--small"]).unwrap(), (Fixture::OneI, None));
            assert_eq!(parse(&["--p7"]).unwrap(), (Fixture::P7, None));
            assert_eq!(
                parse(&["--nonempty-eight-i"]).unwrap(),
                (Fixture::NonemptyEightI, None)
            );
            assert_eq!(
                parse(&["--empty-ten-i"]).unwrap(),
                (Fixture::EmptyTenI, None)
            );
            for (flag, fixture) in [
                ("--grounded-o-two-lines", Fixture::GroundedOTwoLinesAll),
                (
                    "--grounded-o-two-lines-unique",
                    Fixture::GroundedOTwoLinesUnique,
                ),
            ] {
                assert_eq!(parse(&[flag]).unwrap(), (fixture, None));
                assert_eq!(
                    parse(&[flag, "--coverage-slot", "sample"]).unwrap_err(),
                    GROUNDED_O_CAPTURE_ERROR
                );
                assert!(fixture.capture_universe().is_err());
            }
            assert_eq!(
                parse(&["--small", "--coverage-slot", "sample"]).unwrap(),
                (Fixture::OneI, Some("sample".to_owned()))
            );
            assert_eq!(
                parse(&["--p7", "--coverage-slot", "sample"]).unwrap(),
                (Fixture::P7, Some("sample".to_owned()))
            );
            assert_eq!(Fixture::P7.capture_universe(), Ok(CoverageUniverse::P7));
            assert_eq!(
                parse(&["--nonempty-eight-i", "--coverage-slot", "sample"]).unwrap_err(),
                EIGHT_I_CAPTURE_ERROR
            );
            assert!(Fixture::NonemptyEightI.capture_universe().is_err());
            assert_eq!(
                parse(&["--empty-ten-i", "--coverage-slot", "sample"]).unwrap_err(),
                TEN_I_CAPTURE_ERROR
            );
            assert!(Fixture::EmptyTenI.capture_universe().is_err());
        }

        #[test]
        fn grounded_o_control_compiles_the_same_fixed_queue_with_distinct_count_policies() {
            assert_eq!(GROUNDED_O_INITIAL, 0xff3fc);
            assert_eq!(GROUNDED_O_CELLS & 0x3ff, 3);
            assert_eq!(GROUNDED_O_CELLS >> 10, 3);
            for (fixture, count) in [
                (Fixture::GroundedOTwoLinesAll, PcCountPolicy::CountAll),
                (Fixture::GroundedOTwoLinesUnique, PcCountPolicy::CountUnique),
            ] {
                let problem =
                    compile_problem(fixture, PcExecutionPolicy::mvp_default(), false).unwrap();
                assert_eq!(problem.count_policy(), count);
                assert!(problem.solution_probability_policy().requested());
                assert!(!problem.supply().hold_enabled());
                let universe = problem.piece_source().materialized_universe().unwrap();
                assert_eq!(universe.pattern_count(), 1);
                assert_eq!(universe.sequence_at(0).as_ref(), &[PieceKind::O]);
            }
        }

        #[test]
        fn ordinary_summary_keeps_the_full_original_p7p4_request() {
            let trace =
                compile_problem(Fixture::P7P4, PcExecutionPolicy::mvp_default(), false).unwrap();
            assert_eq!(trace.output_policy(), SearchOutputPolicy::Trace);
            let summary = product_summary(trace).unwrap();
            assert_eq!(summary.output_policy(), SearchOutputPolicy::Summary);
            assert_eq!(summary.count_policy(), PcCountPolicy::CountUnique);
            assert_eq!(
                summary.solution_probability_policy(),
                PcSolutionProbabilityPolicy::Omit
            );
            assert!(summary.supply().hold_enabled());
            let universe = summary.piece_source().materialized_universe().unwrap();
            assert!(universe.complete());
            assert_eq!(universe.pattern_count(), 4_233_600);
            assert_eq!(universe.sequence_at(0).len(), 11);
            assert_eq!(universe.sequence_at(4_233_599).len(), 11);
            let richer = compile_problem(
                Fixture::GroundedOTwoLinesUnique,
                PcExecutionPolicy::mvp_default(),
                false,
            )
            .unwrap();
            assert!(product_summary(richer).is_err());
        }

        #[test]
        fn strict_p7_compiles_the_original_uniform_supply_without_replay() {
            let problem =
                compile_problem(Fixture::P7, PcExecutionPolicy::mvp_default(), true).unwrap();
            assert_eq!(problem.output_policy(), SearchOutputPolicy::CoverageSummary);
            assert_eq!(problem.count_policy(), PcCountPolicy::CountUnique);
            assert_eq!(
                problem.solution_probability_policy(),
                PcSolutionProbabilityPolicy::Omit
            );
            assert!(problem.supply().hold_enabled());
            let universe = problem.piece_source().materialized_universe().unwrap();
            assert!(universe.complete());
            assert_eq!(universe.pattern_count(), 5_040);
            assert_eq!(universe.sequence_at(0).len(), 7);
            assert_eq!(universe.sequence_at(5_039).len(), 7);
        }

        #[test]
        fn eight_i_rectangle_oracle_derives_all_seven_placements() {
            assert_eq!(EIGHT_I_INITIAL.count_ones(), 8);
            assert_eq!(four_row_i_identity_oracle(EIGHT_I_INITIAL, 2).len(), 7);
        }

        #[test]
        fn empty_ten_i_rectangle_oracle_derives_all_fourteen_placements() {
            use clearra_core_domain::solution::normalized_tiling_solution as normalized;

            let identities = four_row_i_identity_oracle(0, 0);
            assert_eq!(identities.len(), 14);
            assert_eq!(
                normalized::normalized_tiling_solution_set_hash_from_sorted_standard_board64_identity_iter(
                    identities
                ),
                "cts1:66d4ef8ac39a1839"
            );
        }

        #[test]
        fn empty_ten_i_work_modes_require_active_route_and_accurate_counts() {
            let timing = GeometryDomainExperimentTotals {
                completed_families: 1,
                minimum_compiles: 12,
                minimum_empty_origin: 12,
                ..GeometryDomainExperimentTotals::default()
            };
            let baseline = GeometryDomainExperimentTotals {
                minimum_profiled_compiles: 12,
                minimum_incidence_rows_scanned: 20,
                ..timing
            };
            let cutoff = GeometryDomainExperimentTotals {
                minimum_incidence_rows_scanned: 15,
                minimum_incidence_rows_skipped: 5,
                minimum_cells_cut_off: 2,
                ..baseline
            };
            let valid = |metrics, cutoff, work| {
                let mut errors = Vec::new();
                validate_empty_minimum_work(metrics, cutoff, work, &mut errors);
                errors.is_empty()
            };
            assert!(valid(timing, false, false));
            assert!(valid(timing, true, false));
            assert!(valid(baseline, false, true));
            assert!(valid(cutoff, true, true));
            assert!(!valid(timing, false, true));
            assert!(!valid(cutoff, true, false));
            assert!(!valid(baseline, true, true));
            assert!(!valid(cutoff, false, true));
            assert!(!valid(
                GeometryDomainExperimentTotals {
                    minimum_empty_origin: 0,
                    minimum_below_seven_pieces: 12,
                    ..timing
                },
                false,
                false
            ));
        }
    }
}
