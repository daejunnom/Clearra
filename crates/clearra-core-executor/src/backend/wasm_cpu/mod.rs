mod build_stage_domain;
pub use build_stage_domain::{BuildStageDomain, BuildStageDomainError};
#[cfg(test)]
mod build_clear_row_pruning_tests;
mod build_clear_rows;
mod build_probability;
mod build_probability_distributed;
mod buildup;
pub use buildup::generic_board64_search;
pub use buildup::{
    CompactBuildUpEdge, CompactBuildUpNode, CompactBuildUpSnapshot, CompactBuildUpSnapshotError,
    CompactBuildUpTransitionMode, RustCompactBuildUpWorkspace,
};
#[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
mod candidate_feedback;
mod catalog;
#[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
pub use candidate_feedback::{ExecutorCandidateFeedbackError, ExecutorCandidateFeedbackSession};
mod compact_exact_u64_map;
mod compact_kick_authority;
#[cfg(test)]
mod compact_pc2_completion_ledger;
mod coverage_product;
#[cfg(all(test, feature = "placement-dependency-pc"))]
mod dependency_consumers_tests;
mod distributed;
#[cfg(test)]
mod distributed_pc2_replay_proof;
mod exact_collections;
mod extended_board;
mod extended_build_probability;
mod extended_buildup;
mod extended_geometry;
mod extended_geometry_component;
mod extended_geometry_dense;
mod extended_geometry_domain;
mod extended_inverse_catalog;
mod extended_pc_failed_queue;
mod extended_pc_observation;
mod extended_pc_search;
mod extended_pc_tiling;
mod extended_reachability;
mod finesse_score;
pub mod generic_geometry;
mod geometry;
mod geometry_apdp;
mod geometry_component;
mod geometry_domain;
mod geometry_family;
#[cfg(all(test, feature = "parallel"))]
mod geometry_parallel_tests;
#[cfg(feature = "symbolic-family")]
mod geometry_partial_family;
mod geometry_projection;
mod geometry_separator;
#[cfg(test)]
mod graph_probability_integration_tests;
mod inverse_projection;
mod kick_profiles;
#[cfg(feature = "parallel")]
mod parallel_coverage;
#[cfg(all(feature = "local-search-ab", feature = "parallel", target_os = "linux"))]
mod parallel_memory_peak;
#[cfg(all(
    test,
    feature = "parallel",
    feature = "placement-dependency-pc",
    not(target_family = "wasm")
))]
mod parallel_observation_tests;
#[cfg(all(test, feature = "placement-dependency-pc"))]
mod parametric_kick_consumer_tests;
#[cfg(all(feature = "local-search-ab", feature = "parallel", target_os = "linux"))]
pub use parallel_memory_peak::observe_native_pc_memory;
#[cfg(feature = "parallel")]
mod parallel_search;
#[cfg(feature = "parallel")]
mod parallel_worker;
mod pc4_graph_materializer;
mod piece_order_language;
#[cfg(feature = "local-search-ab")]
pub use piece_order_language::{count_distinct_no_hold_one_bag_poset, DistinctNoHoldOneBagCount};
#[cfg(any(test, feature = "local-search-ab", feature = "placement-dependency-pc"))]
mod placement_conditions;
#[cfg(any(test, feature = "local-search-ab"))]
mod placement_dependency;
#[cfg(test)]
mod placement_dependency_integration_tests;
mod placement_poset;
mod queue_observation_policy;
mod reachability;
mod realization_feasibility;
mod result;
mod setup_all_paths;
mod setup_coverage_graph;
mod setup_finder;
mod setup_graph_builder;
mod setup_observation_language;
mod setup_parallel;
mod setup_parallel_segmented;
mod setup_parallel_wire;
mod setup_partial_build;
mod setup_representative;
mod setup_suffix_coverage;
mod source_prefix;
mod standard_bag_coverage;
mod standard_bag_product_memo;
#[cfg(feature = "symbolic-family")]
mod symbolic_boolean;
#[cfg(feature = "symbolic-family")]
mod symbolic_family;
#[cfg(feature = "symbolic-family")]
mod symbolic_family_bridge;
#[cfg(feature = "symbolic-family")]
mod symbolic_family_supply;
#[cfg(feature = "symbolic-family")]
pub use symbolic_family::{
    solve_symbolic_family, SymbolicFamilyError, SymbolicFamilyLimits, SymbolicFamilyRankReport,
    SymbolicFamilyReport, SymbolicFamilyResult,
};
#[cfg(test)]
mod static_influence_integration_tests;
#[cfg(test)]
mod structural_graph_probability_tests;
#[cfg(any(test, feature = "local-search-ab"))]
mod structural_order;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
mod symbolic_prefix_domain;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
mod symbolic_prefix_lanes;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
mod symbolic_prefix_orders;
#[cfg(all(
    feature = "local-search-ab",
    feature = "parallel",
    not(target_family = "wasm")
))]
mod symbolic_prefix_parallel;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
mod symbolic_prefix_subsumption;
mod terminal_parallel;
#[cfg(all(
    feature = "local-search-ab",
    feature = "parallel",
    not(target_family = "wasm")
))]
pub use symbolic_prefix_parallel::{
    with_catalog_guard_strict_parallel_prefix_experiment,
    with_legacy_strict_parallel_prefix_experiment, with_strict_parallel_prefix_experiment,
    StrictParallelPrefixReport,
};
#[cfg(all(
    feature = "local-search-ab",
    feature = "parallel",
    not(target_family = "wasm")
))]
pub(super) fn strict_parallel_prefix_requested() -> bool {
    symbolic_prefix_parallel::strict_parallel_prefix_requested()
}
#[cfg(test)]
pub(super) fn strict_prefix_experiment_active() -> bool {
    symbolic_prefix_orders::in_strict_prefix_order()
}
#[cfg(feature = "local-search-ab")]
pub use buildup::{geometric_edge_reuse_experiment_totals, GeometricEdgeReuseMetrics};
#[cfg(all(feature = "local-search-ab", feature = "parallel"))]
pub use buildup::{with_operation_mask_experiment, OperationMaskExperimentReport};
#[cfg(feature = "local-search-ab")]
pub use geometry::{geometry_domain_experiment_totals, GeometryDomainExperimentTotals};
#[cfg(feature = "local-search-ab")]
pub use structural_order::{
    run_conditional_ordered_work, structural_order_experiment_totals, ConditionalOrderedWorkMode,
    StructuralOrderMetrics,
};
mod terminal_hold_projection;
mod tiling_parallel;

pub(crate) use build_probability::{BuildProbabilityAdvance, WasmBuildProbabilitySession};
pub use build_probability_distributed::{
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier,
};
pub use distributed::{
    canonical_wasm_candidate_packet_batch_sha256, encode_canonical_wasm_candidate_packet_batch,
    WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuCandidateProducer,
    WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    WasmDistributedResultMerger, WasmDistributedVerifier,
};
pub use extended_build_probability::pc_distributed::{
    WasmFullHeightPcProducer, WasmFullHeightPcResultMerger, WasmFullHeightPcVerifier,
};
pub use extended_pc_failed_queue::{WasmPcFailedQueueAdvance, WasmPcFailedQueueSession};
pub(crate) use extended_pc_search::ExtendedPcSearchSession;
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
pub use extended_pc_search::{NativePcCooperativeAdvance, NativePcCooperativeResult};
pub use extended_pc_tiling::distributed::{
    WasmFullHeightTilingProducer, WasmFullHeightTilingResultMerger, WasmFullHeightTilingWorker,
};
pub(crate) use extended_pc_tiling::ExtendedPcTilingSession;
pub use pc4_graph_materializer::{
    any_pc4_ilc_target_field, enumerate_pc4_ilc_geometric_predecessor_fields,
    enumerate_pc4_ilc_predecessor_fields, enumerate_pc4_ilc_target_fields,
    materialize_pc4_ilc_transition, Pc4IlcForwardMembershipWorkspace, Pc4IlcMaterializationError,
    Pc4IlcPlacement,
};
#[cfg(any(test, feature = "qualification-reference"))]
pub(crate) use reachability::exact_solver_local_relation_spawn_entries;
pub(crate) use reachability::{
    exact_entry_lock_anchors, exact_local_relation, exact_spawn_lock_anchors,
    DocumentLockReachability, DocumentReachabilityEngine,
};
#[cfg(any(test, feature = "local-search-ab"))]
pub use result::CapturedSolutionCoverage;
#[cfg(any(test, feature = "local-search-ab"))]
pub(crate) use result::SolutionCoverageCapture;
pub use result::WasmCompactPcFailedQueueSession;
pub(crate) use result::{ExactSearchAdvance, WasmExactSearchSession};
pub(crate) use setup_finder::{WasmSetupSearchAdvance, WasmSetupSearchSession};
#[cfg(not(target_family = "wasm"))]
pub(crate) use setup_parallel::{
    execute_setup_parallel_native, execute_setup_parallel_native_with_complete_candidates,
};
pub(crate) use setup_parallel::{
    WasmSetupParallelCoordinator, WasmSetupParallelProduce, WasmSetupParallelWorker,
    WasmSetupParallelWorkerAdvance,
};
#[cfg(feature = "local-search-ab")]
pub use standard_bag_coverage::P7P4CoverageBlocks;
pub use tiling_parallel::{
    WasmPackedTilingIdentity, WasmPcRootProducer, WasmPcRootResultMerger, WasmTilingRootAdvance,
    WasmTilingRootChunk, WasmTilingRootProducer, WasmTilingRootResultMerger, WasmTilingRootWorker,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum WasmExactSearchError {
    InvalidProblem(&'static str),
    ResourceAdmission(Box<clearra_core_domain::resource::ResourceReport>),
    Cancelled,
}

pub(super) const PC_PLACEMENT_DEPENDENCY_UNSUPPORTED: &str =
    "wasm_placement_dependency_request_unsupported";

#[cfg(test)]
thread_local! {
    static PC_PLACEMENT_DEPENDENCY_REFERENCE: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
}

/// Serial differential tests explicitly retain the former concrete verifier.
/// The override never exists in product builds or propagates to new workers.
#[cfg(test)]
pub(super) fn with_pc_placement_dependency_reference<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            PC_PLACEMENT_DEPENDENCY_REFERENCE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(PC_PLACEMENT_DEPENDENCY_REFERENCE.with(|mode| mode.replace(true)));
    run()
}

fn pc_placement_dependency_reference_requested() -> bool {
    #[cfg(test)]
    if PC_PLACEMENT_DEPENDENCY_REFERENCE.with(std::cell::Cell::get) {
        return true;
    }
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    {
        // The native diagnostic validates this existing process-wide 0/1
        // axis. Only its explicit zero arm may select the BuildUp reference.
        static REFERENCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *REFERENCE.get_or_init(|| std::env::var("CLEARRA_STRICT_PREFIX").as_deref() == Ok("0"))
    }
    #[cfg(not(all(feature = "local-search-ab", not(target_family = "wasm"))))]
    false
}

fn uses_pc_placement_dependency(problem: &clearra_problem::SearchProblem) -> bool {
    cfg!(feature = "placement-dependency-pc")
        && problem.pc_placement_dependency_eligible()
        && !pc_placement_dependency_reference_requested()
}

fn ensure_pc_placement_dependency_mode(
    problem: &clearra_problem::SearchProblem,
) -> Result<(), WasmExactSearchError> {
    if problem.backend_policy().precompute_build_dependencies()
        && !(cfg!(feature = "placement-dependency-pc")
            && problem.pc_placement_dependency_requested())
    {
        return Err(WasmExactSearchError::InvalidProblem(
            PC_PLACEMENT_DEPENDENCY_UNSUPPORTED,
        ));
    }
    Ok(())
}

fn ensure_no_pc_placement_dependency_mode(
    problem: &clearra_problem::SearchProblem,
) -> Result<(), WasmExactSearchError> {
    if problem.backend_policy().precompute_build_dependencies() {
        return Err(WasmExactSearchError::InvalidProblem(
            PC_PLACEMENT_DEPENDENCY_UNSUPPORTED,
        ));
    }
    Ok(())
}

impl WasmExactSearchError {
    pub(super) fn resource_admission(
        report: clearra_core_domain::resource::ResourceReport,
    ) -> Self {
        Self::ResourceAdmission(Box::new(report))
    }

    pub(super) fn reason(&self) -> &'static str {
        use clearra_core_domain::resource::{
            ExecutionAvailabilityReason as Reason, ExecutionAvailabilityState as State,
        };
        match self {
            Self::InvalidProblem(reason) => reason,
            Self::Cancelled => "wasm_cpu_search_cancelled",
            Self::ResourceAdmission(report) => match (
                report.execution_availability().state(),
                report.execution_availability().reason(),
            ) {
                (State::Unavailable, Some(Reason::PatternCountAddressSpaceExceeded)) => {
                    "pattern_count_address_space_unavailable"
                }
                (State::Unavailable, Some(Reason::DensePatternRepresentationUnavailable)) => {
                    "dense_pattern_representation_unavailable"
                }
                (State::Deferred, Some(Reason::SharedResourceContention)) => {
                    "shared_execution_resource_deferred"
                }
                (State::Exhausted, Some(Reason::MemoryBudgetExceeded)) => {
                    "shared_execution_memory_exhausted"
                }
                (State::Exhausted, Some(Reason::ComputeBudgetExceeded)) => {
                    "shared_execution_compute_exhausted"
                }
                _ => "shared_execution_resource_unavailable",
            },
        }
    }
}

const MAX_BOARD64_PIECES: usize = 15;

fn packing_hold_projection(
    problem: &clearra_problem::SearchProblem,
) -> clearra_supply::pattern_universe::PackingHoldProjection {
    if problem.supply().projects_unplaced_lookahead()
        && !problem.supply().projects_standard_bag_lookahead()
    {
        clearra_supply::pattern_universe::PackingHoldProjection::ReleaseHeldAtTerminal
    } else {
        clearra_supply::pattern_universe::PackingHoldProjection::PreserveFinalHoldLanguage
    }
}

fn ensure_connected_kick_profile(
    problem: &clearra_problem::SearchProblem,
) -> Result<(), WasmExactSearchError> {
    if dependency_kick_authority(problem)?.is_some() {
        return Ok(());
    }
    ensure_builtin_kick_profile(problem, true)
}

/// Consumers without a bound ordered-table owner may only use the complete
/// canonical table. In particular, their graph can include placements outside
/// the submitted fixed queue, so the PC-only playable-piece projection is not
/// sufficient authority for these consumers.
fn ensure_canonical_kick_profile(
    problem: &clearra_problem::SearchProblem,
) -> Result<(), WasmExactSearchError> {
    ensure_builtin_kick_profile(problem, false)
}

fn ensure_builtin_kick_profile(
    problem: &clearra_problem::SearchProblem,
    allow_fixed_scenario_projection: bool,
) -> Result<(), WasmExactSearchError> {
    let Some(builtin) = kick_profiles::builtin_kick_profile(problem.kick_profile().profile_id())
    else {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_exact_backend_requires_connected_kick_profile",
        ));
    };
    let rule_selection = problem.rule_profile();
    if let Some(verified) = rule_selection.verified_kick_profile() {
        if !matches_builtin_order(rule_selection.rule().id(), verified, builtin)
            && !(allow_fixed_scenario_projection
                && fixed_compact_scenario_has_same_playable_kicks(
                    problem,
                    verified.profile(),
                    builtin,
                ))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_exact_backend_requires_canonical_ordered_kick_profile",
            ));
        }
    }
    Ok(())
}

/// Bind a submitted table to the Boolean dependency contract by its complete
/// ordered contents. A profile tag is never authority for an imported table.
/// Other consumers retain their existing admission and evidence contracts.
fn dependency_kick_authority(
    problem: &clearra_problem::SearchProblem,
) -> Result<Option<compact_kick_authority::CompactKickAuthority>, WasmExactSearchError> {
    if !cfg!(feature = "placement-dependency-pc") || !problem.pc_placement_dependency_eligible() {
        return Ok(None);
    }
    let height = u8::try_from(problem.compact_pc_geometry_height()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_ordered_kick_height_out_of_range")
    })?;
    ordered_kick_authority(
        problem,
        u8::try_from(problem.initial_board().width()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_ordered_kick_width_out_of_range")
        })?,
        height,
    )
}

/// Admit the submitted ordered contents for a consumer's actual geometry.
/// The caller must establish its Boolean scope and bind the returned owner to
/// its reachability workspace before it can use that imported table.
fn ordered_kick_authority(
    problem: &clearra_problem::SearchProblem,
    width: u8,
    height: u8,
) -> Result<Option<compact_kick_authority::CompactKickAuthority>, WasmExactSearchError> {
    use compact_kick_authority::{CompactKickAdmission, CompactKickAuthority};

    let rule = problem.rule_profile();
    let Some(verified) = rule.verified_kick_profile() else {
        return Ok(None);
    };
    CompactKickAuthority::admit_profile(width, height, rule.rule().id(), verified)
        .map(Some)
        .map_err(|error| {
            WasmExactSearchError::InvalidProblem(match error {
                CompactKickAdmission::Unsupported => "wasm_ordered_kick_profile_unsupported",
                CompactKickAdmission::IndexOrCoordinateOverflow => {
                    "wasm_ordered_kick_bounds_exceeded"
                }
                CompactKickAdmission::BudgetExceeded => "wasm_ordered_kick_compile_budget",
            })
        })
}

/// In a whole fixed no-hold Scenario only the submitted queue pieces can
/// move or appear in a replay. If their ordered tables agree exactly, a
/// difference elsewhere in a verified import cannot alter that search.
/// This is not a profile-wide proof and never qualifies imported assets.
fn fixed_compact_scenario_has_same_playable_kicks(
    problem: &clearra_problem::SearchProblem,
    supplied: &clearra_rules::kicks::KickTableProfile,
    builtin: &clearra_rules::kicks::KickTableProfile,
) -> bool {
    use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend, WorkerPolicy};
    use clearra_problem::{
        goal::SearchGoal, SearchOutputPolicy, SearchProblemKind, SearchProblemPreset,
    };

    let backend = problem.backend_policy();
    let supply = problem.supply();
    if supplied.id() != builtin.id()
        || supplied.source_rule() != builtin.source_rule()
        || supplied.source_rule() != problem.rule_profile().rule().id()
        || supplied.supports_180() != builtin.supports_180()
        || problem.preset() != SearchProblemPreset::ScenarioPc
        || problem.problem_kind() != SearchProblemKind::ScenarioPc
        || !matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
        || problem.initial_board().width() != 10
        || !(1..=6).contains(&problem.visible_height())
        || !(1..=6).contains(&problem.compact_pc_geometry_height())
        || backend.requested_backend() != RequestedSearchBackend::Cpu
        || backend.worker_policy() != WorkerPolicy::Fixed(1)
        || backend.tablebase_requested()
        || backend.precompute_build_dependencies()
        || !matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        || problem.output_policy() != SearchOutputPolicy::Trace
        || problem.objective().score().requested()
        || problem.objective().execution_constraints().requested()
        || problem.solution_probability_policy().requested()
        || problem
            .queue_observation_policy()
            .requires_observation_policy()
        || supply.hold_enabled()
        || supply.hold_piece().is_some()
        || supply.projects_unplaced_lookahead()
        || problem.continuation_policy().min_remaining_queue() != 0
    {
        return false;
    }
    let Some(sequence) = problem.piece_source().fixed_sequence() else {
        return false;
    };
    if sequence.is_empty() || problem.exact_pieces() != Some(sequence.len()) {
        return false;
    }
    let playable = |entry: &&clearra_rules::kicks::KickTableEntry| {
        sequence.pieces().contains(&entry.transition().piece())
    };
    let mut actual = supplied.entries().iter().filter(playable);
    let expected = builtin.entries().iter().filter(playable);
    // A verified profile has unique keys. Equality of each supplied
    // sequence and the relevant entry counts also excludes missing keys.
    actual.clone().count() == expected.count()
        && actual.all(|entry| builtin.sequence_for(entry.transition()) == Some(entry.sequence()))
}

pub(super) fn builtin_accelerators_match_rule(
    rule: clearra_rules::profile::rule_profile::RuleProfileId,
    verified: Option<&clearra_rules::kicks::VerifiedKickTableProfile>,
) -> bool {
    use clearra_rules::profile::rule_profile::RuleProfileId;

    let Some(verified) = verified else {
        return matches!(
            rule,
            RuleProfileId::Srs
                | RuleProfileId::SrsPlus
                | RuleProfileId::SrsX
                | RuleProfileId::Jstris180
                | RuleProfileId::NoKick
        );
    };
    kick_profiles::builtin_kick_profile(verified.profile().id())
        .is_some_and(|builtin| matches_builtin_order(rule, verified, builtin))
}

fn matches_builtin_order(
    selected_rule: clearra_rules::profile::rule_profile::RuleProfileId,
    verified: &clearra_rules::kicks::VerifiedKickTableProfile,
    builtin: &clearra_rules::kicks::KickTableProfile,
) -> bool {
    let supplied = verified.profile();
    // Verification excludes duplicate transition keys. Record order is
    // irrelevant, but each transition's first-success order is authority.
    supplied.id() == builtin.id()
        && supplied.source_rule() == selected_rule
        && supplied.source_rule() == builtin.source_rule()
        && supplied.transition_count() == builtin.transition_count()
        && supplied
            .entries()
            .iter()
            .all(|entry| builtin.sequence_for(entry.transition()) == Some(entry.sequence()))
}

const fn piece_index(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> usize {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn mix_digest(mut hash: u64, value: u64) -> u64 {
    if hash == 0 {
        hash = 0xcbf2_9ce4_8422_2325;
    }
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Combines one member into a stable multiset digest. Unlike `mix_digest`,
/// this operation is associative and commutative, so independently scheduled
/// geometry roots produce the same candidate-set evidence as serial search.
fn mix_order_independent_candidate_digest(digest: u64, value: u64) -> u64 {
    let mut contribution = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    contribution = (contribution ^ (contribution >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    contribution = (contribution ^ (contribution >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    contribution ^= contribution >> 31;
    digest.wrapping_add(contribution)
}

fn uses_order_independent_pc_candidate_digest(problem: &clearra_problem::SearchProblem) -> bool {
    let Some(universe) = problem.piece_source().materialized_universe() else {
        return false;
    };
    problem.objective().kind()
        == clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique
        && problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountUnique
        && !problem.objective().score().requested()
        && !problem.objective().execution_constraints().requested()
        && !problem.solution_probability_policy().requested()
        && !problem
            .queue_observation_policy()
            .requires_observation_policy()
        && !problem.backend_policy().tablebase_requested()
        && standard_bag_coverage::StandardBagCoverage::supports(universe, problem.initial_hold())
}
