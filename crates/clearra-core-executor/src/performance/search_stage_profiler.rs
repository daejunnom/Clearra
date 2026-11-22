#[cfg(all(
    not(target_family = "wasm"),
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
))]
use std::sync::{Arc, Mutex};
#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
use std::{cell::RefCell, marker::PhantomData, rc::Rc, time::Duration};

#[cfg(all(
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"),
    not(target_arch = "wasm32")
))]
use std::time::Instant;
#[cfg(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now_ms() -> f64;
}

#[cfg(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))]
type StageInstant = f64;
#[cfg(all(
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"),
    not(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))
))]
type StageInstant = Instant;

#[cfg(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))]
#[inline]
fn stage_now() -> StageInstant {
    performance_now_ms()
}

#[cfg(all(
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"),
    not(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))
))]
#[inline]
fn stage_now() -> StageInstant {
    Instant::now()
}

#[cfg(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))]
#[inline]
fn stage_elapsed(started: StageInstant) -> Duration {
    Duration::from_secs_f64(((performance_now_ms() - started).max(0.0)) / 1_000.0)
}

#[cfg(all(
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"),
    not(all(feature = "wasm-stage-profiling", target_arch = "wasm32"))
))]
#[inline]
fn stage_elapsed(started: StageInstant) -> Duration {
    started.elapsed()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum ExecutorSearchStage {
    PcPacking,
    PcBuildUp,
    PcOutput,
    PackingUniverseAndFamily,
    PackingBackendAndPatternIndex,
    PackingGpuCapabilityQuery,
    // These stages are emitted only by the optional native geometry-graph backend.
    #[cfg_attr(not(feature = "native-c-core"), allow(dead_code))]
    PackingCpuCatalogCompile,
    #[cfg_attr(not(feature = "native-c-core"), allow(dead_code))]
    PackingCpuGeometryGraph,
    #[cfg_attr(not(feature = "native-c-core"), allow(dead_code))]
    PackingCpuTaskSplit,
    #[cfg_attr(not(feature = "native-c-core"), allow(dead_code))]
    PackingCpuBuildabilityReduce,
    BuildUpNativeExecution,
    BuildUpGeometryLanguageExport,
    BuildUpLanguageIntersection,
    BuildUpTraceMaterialization,
    BuildUpPatternBinding,
    BuildUpProblemLowering,
    BuildUpNativeCall,
    BuildUpVariantCopy,
    BuildUpWitness,
    BuildUpAcceptedCandidates,
    BuildUpSolutionSet,
    BuildUpTraceMaterial,
    BuildUpCoverageRows,
    BuildUpPostProcess,
    BuildUpPatternWeights,
    BuildUpObjective,
    BuildUpResultAssembly,
    WasmSessionCatalogCompile,
    WasmSessionSupplyCompile,
    WasmSessionGeometryPrepare,
    WasmSetupGeometryCompile,
    WasmSetupPartialBuild,
    WasmSetupCoverageGraphCompile,
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    WasmParallelGeometryCompileAndSplit,
    WasmGeometryTargetPreparation,
    WasmGeometryFamilyCompile,
    WasmGeometryConstructionIndexRelease,
    WasmGeometryPathCount,
    WasmGeometryTaskSplit,
    WasmGeometryResidualLookup,
    WasmGeometryFeasiblePieces,
    WasmGeometryCheapProjection,
    WasmGeometryMinimumDomain,
    WasmGeometryAdvancedDomain,
    WasmGeometryAdvancedPrune,
    WasmGeometryComponentPlan,
    WasmGeometryFamilyCommit,
    WasmGeometrySupportRows,
    WasmGeometryPrefixClosure,
    WasmGeometryPrefixPrunedSubtrees,
    WasmGeometryRowOrderPrepare,
    WasmGeometryRowOrderPrunedSubtrees,
    WasmGeometrySourcePrefix,
    WasmGeometrySourcePrefixPrunedSubtrees,
    WasmGeometrySourcePrefixRowStates,
    WasmGeometrySourcePrefixRootFastPasses,
    WasmGeometryEmittedCandidates,
    WasmGeometryAdvance,
    WasmCandidateProjection,
    WasmCandidateFeasibility,
    WasmBuildOrderReachability,
    WasmCoverageLanguageProduct,
    WasmWitnessSearch,
    WasmCandidateResultReduce,
    WasmCandidateMemoryAccounting,
    WasmStandardBagMemoPromote,
    WasmFinalCoverage,
    WasmMinimumCoverProof,
    WasmSpinExecutionGraphPrepare,
    WasmPcPathExecutionGraphs,
    WasmPcPathPatternRows,
    AppPcPathManifestPrepare,
    AppPcPathManifestCount,
    WasmResultCanonicalize,
    FinesseGeometry,
    FinesseTargetGrouping,
    FinesseMovementBfs,
    FinesseAnnotationPrune,
    FinesseProductDp,
    FinesseAggregation,
    FinesseWitness,
    #[cfg(any(
        feature = "placement-dependency-pc",
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    StrictPrefixUpper,
    #[cfg(any(
        feature = "placement-dependency-pc",
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    StrictPrefixReverse,
    #[cfg(any(
        feature = "placement-dependency-pc",
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    StrictPrefixLanguage,
    #[cfg(all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    ))]
    StrictReferenceLanguage,
    #[cfg(all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    ))]
    StrictStandardBagProduct,
    #[cfg(feature = "placement-dependency-pc")]
    PlacementDependencyPc,
    #[cfg(feature = "placement-dependency-pc")]
    PlacementDependencyPcLanguage,
    #[cfg_attr(
        not(all(feature = "parallel", not(target_family = "wasm"))),
        allow(dead_code)
    )]
    ExtendedPcCandidateVerifyAndReduce,
}

impl ExecutorSearchStage {
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    const COUNT: usize = Self::ExtendedPcCandidateVerifyAndReduce as usize + 1;

    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    const ALL: [Self; Self::COUNT] = [
        Self::PcPacking,
        Self::PcBuildUp,
        Self::PcOutput,
        Self::PackingUniverseAndFamily,
        Self::PackingBackendAndPatternIndex,
        Self::PackingGpuCapabilityQuery,
        Self::PackingCpuCatalogCompile,
        Self::PackingCpuGeometryGraph,
        Self::PackingCpuTaskSplit,
        Self::PackingCpuBuildabilityReduce,
        Self::BuildUpNativeExecution,
        Self::BuildUpGeometryLanguageExport,
        Self::BuildUpLanguageIntersection,
        Self::BuildUpTraceMaterialization,
        Self::BuildUpPatternBinding,
        Self::BuildUpProblemLowering,
        Self::BuildUpNativeCall,
        Self::BuildUpVariantCopy,
        Self::BuildUpWitness,
        Self::BuildUpAcceptedCandidates,
        Self::BuildUpSolutionSet,
        Self::BuildUpTraceMaterial,
        Self::BuildUpCoverageRows,
        Self::BuildUpPostProcess,
        Self::BuildUpPatternWeights,
        Self::BuildUpObjective,
        Self::BuildUpResultAssembly,
        Self::WasmSessionCatalogCompile,
        Self::WasmSessionSupplyCompile,
        Self::WasmSessionGeometryPrepare,
        Self::WasmSetupGeometryCompile,
        Self::WasmSetupPartialBuild,
        Self::WasmSetupCoverageGraphCompile,
        Self::WasmParallelGeometryCompileAndSplit,
        Self::WasmGeometryTargetPreparation,
        Self::WasmGeometryFamilyCompile,
        Self::WasmGeometryConstructionIndexRelease,
        Self::WasmGeometryPathCount,
        Self::WasmGeometryTaskSplit,
        Self::WasmGeometryResidualLookup,
        Self::WasmGeometryFeasiblePieces,
        Self::WasmGeometryCheapProjection,
        Self::WasmGeometryMinimumDomain,
        Self::WasmGeometryAdvancedDomain,
        Self::WasmGeometryAdvancedPrune,
        Self::WasmGeometryComponentPlan,
        Self::WasmGeometryFamilyCommit,
        Self::WasmGeometrySupportRows,
        Self::WasmGeometryPrefixClosure,
        Self::WasmGeometryPrefixPrunedSubtrees,
        Self::WasmGeometryRowOrderPrepare,
        Self::WasmGeometryRowOrderPrunedSubtrees,
        Self::WasmGeometrySourcePrefix,
        Self::WasmGeometrySourcePrefixPrunedSubtrees,
        Self::WasmGeometrySourcePrefixRowStates,
        Self::WasmGeometrySourcePrefixRootFastPasses,
        Self::WasmGeometryEmittedCandidates,
        Self::WasmGeometryAdvance,
        Self::WasmCandidateProjection,
        Self::WasmCandidateFeasibility,
        Self::WasmBuildOrderReachability,
        Self::WasmCoverageLanguageProduct,
        Self::WasmWitnessSearch,
        Self::WasmCandidateResultReduce,
        Self::WasmCandidateMemoryAccounting,
        Self::WasmStandardBagMemoPromote,
        Self::WasmFinalCoverage,
        Self::WasmMinimumCoverProof,
        Self::WasmSpinExecutionGraphPrepare,
        Self::WasmPcPathExecutionGraphs,
        Self::WasmPcPathPatternRows,
        Self::AppPcPathManifestPrepare,
        Self::AppPcPathManifestCount,
        Self::WasmResultCanonicalize,
        Self::FinesseGeometry,
        Self::FinesseTargetGrouping,
        Self::FinesseMovementBfs,
        Self::FinesseAnnotationPrune,
        Self::FinesseProductDp,
        Self::FinesseAggregation,
        Self::FinesseWitness,
        #[cfg(any(
            feature = "placement-dependency-pc",
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        Self::StrictPrefixUpper,
        #[cfg(any(
            feature = "placement-dependency-pc",
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        Self::StrictPrefixReverse,
        #[cfg(any(
            feature = "placement-dependency-pc",
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        Self::StrictPrefixLanguage,
        #[cfg(all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        ))]
        Self::StrictReferenceLanguage,
        #[cfg(all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        ))]
        Self::StrictStandardBagProduct,
        #[cfg(feature = "placement-dependency-pc")]
        Self::PlacementDependencyPc,
        #[cfg(feature = "placement-dependency-pc")]
        Self::PlacementDependencyPcLanguage,
        Self::ExtendedPcCandidateVerifyAndReduce,
    ];

    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::PcPacking => "rust.pc.packing",
            Self::PcBuildUp => "rust.pc.buildup",
            Self::PcOutput => "rust.pc.output",
            Self::PackingUniverseAndFamily => "rust.packing.universe_and_family",
            Self::PackingBackendAndPatternIndex => "rust.packing.backend_and_pattern_index",
            Self::PackingGpuCapabilityQuery => "rust.packing.gpu.capability_query",
            Self::PackingCpuCatalogCompile => "rust.packing.cpu.catalog_compile",
            Self::PackingCpuGeometryGraph => "rust.packing.cpu.geometry_graph",
            Self::PackingCpuTaskSplit => "rust.packing.cpu.task_split",
            Self::PackingCpuBuildabilityReduce => "rust.packing.cpu.buildability_reduce",
            Self::BuildUpNativeExecution => "rust.buildup.native_execution",
            Self::BuildUpGeometryLanguageExport => "rust.buildup.geometry_language_export",
            Self::BuildUpLanguageIntersection => "rust.buildup.language_intersection",
            Self::BuildUpTraceMaterialization => "rust.buildup.trace_materialization",
            Self::BuildUpPatternBinding => "rust.buildup.pattern_binding",
            Self::BuildUpProblemLowering => "rust.buildup.problem_lowering",
            Self::BuildUpNativeCall => "rust.buildup.native_call",
            Self::BuildUpVariantCopy => "rust.buildup.variant_copy",
            Self::BuildUpWitness => "rust.buildup.witness",
            Self::BuildUpAcceptedCandidates => "rust.buildup.accepted_candidates",
            Self::BuildUpSolutionSet => "rust.buildup.solution_set",
            Self::BuildUpTraceMaterial => "rust.buildup.trace_material",
            Self::BuildUpCoverageRows => "rust.buildup.coverage_rows",
            Self::BuildUpPostProcess => "rust.buildup.postprocess",
            Self::BuildUpPatternWeights => "rust.buildup.pattern_weights",
            Self::BuildUpObjective => "rust.buildup.objective",
            Self::BuildUpResultAssembly => "rust.buildup.result_assembly",
            Self::WasmSessionCatalogCompile => "wasm.session.catalog_compile",
            Self::WasmSessionSupplyCompile => "wasm.session.supply_compile",
            Self::WasmSessionGeometryPrepare => "wasm.session.geometry_prepare",
            Self::WasmSetupGeometryCompile => "wasm.setup.geometry_compile",
            Self::WasmSetupPartialBuild => "wasm.setup.partial_build",
            Self::WasmSetupCoverageGraphCompile => "wasm.setup.coverage_graph_compile",
            Self::WasmParallelGeometryCompileAndSplit => "wasm.geometry.parallel_compile_and_split",
            Self::WasmGeometryTargetPreparation => "wasm.geometry.target_preparation",
            Self::WasmGeometryFamilyCompile => "wasm.geometry.family_compile",
            Self::WasmGeometryConstructionIndexRelease => {
                "wasm.geometry.construction_index_release"
            }
            Self::WasmGeometryPathCount => "wasm.geometry.path_count",
            Self::WasmGeometryTaskSplit => "wasm.geometry.task_split",
            Self::WasmGeometryResidualLookup => "wasm.geometry.residual_lookup",
            Self::WasmGeometryFeasiblePieces => "wasm.geometry.feasible_pieces",
            Self::WasmGeometryCheapProjection => "wasm.geometry.cheap_projection",
            Self::WasmGeometryMinimumDomain => "wasm.geometry.minimum_domain",
            Self::WasmGeometryAdvancedDomain => "wasm.geometry.advanced_domain",
            Self::WasmGeometryAdvancedPrune => "wasm.geometry.advanced_prune",
            Self::WasmGeometryComponentPlan => "wasm.geometry.component_plan",
            Self::WasmGeometryFamilyCommit => "wasm.geometry.family_commit",
            Self::WasmGeometrySupportRows => "wasm.geometry.support_rows",
            Self::WasmGeometryPrefixClosure => "wasm.geometry.prefix_closure",
            Self::WasmGeometryPrefixPrunedSubtrees => "wasm.geometry.prefix_pruned_subtrees",
            Self::WasmGeometryRowOrderPrepare => "wasm.geometry.row_order_prepare",
            Self::WasmGeometryRowOrderPrunedSubtrees => "wasm.geometry.row_order_pruned_subtrees",
            Self::WasmGeometrySourcePrefix => "wasm.geometry.source_prefix",
            Self::WasmGeometrySourcePrefixPrunedSubtrees => {
                "wasm.geometry.source_prefix_pruned_subtrees"
            }
            Self::WasmGeometrySourcePrefixRowStates => "wasm.geometry.source_prefix_row_states",
            Self::WasmGeometrySourcePrefixRootFastPasses => {
                "wasm.geometry.source_prefix_root_fast_passes"
            }
            Self::WasmGeometryEmittedCandidates => "wasm.geometry.emitted_candidates",
            Self::WasmGeometryAdvance => "wasm.geometry.advance",
            Self::WasmCandidateProjection => "wasm.candidate.projection",
            Self::WasmCandidateFeasibility => "wasm.candidate.feasibility",
            Self::WasmBuildOrderReachability => "wasm.candidate.build_order_reachability",
            Self::WasmCoverageLanguageProduct => "wasm.candidate.coverage_language_product",
            Self::WasmWitnessSearch => "wasm.candidate.witness_search",
            Self::WasmCandidateResultReduce => "wasm.candidate.result_reduce",
            Self::WasmCandidateMemoryAccounting => "wasm.candidate.memory_accounting",
            Self::WasmStandardBagMemoPromote => "wasm.coverage.standard_bag_memo_promote",
            Self::WasmFinalCoverage => "wasm.finalize.coverage",
            Self::WasmMinimumCoverProof => "wasm.finalize.minimum_cover_proof",
            Self::WasmSpinExecutionGraphPrepare => "wasm.finalize.spin_execution_graph_prepare",
            Self::WasmPcPathExecutionGraphs => "wasm.pc.path.execution_graphs",
            Self::WasmPcPathPatternRows => "wasm.pc.path.pattern_rows",
            Self::AppPcPathManifestPrepare => "app.pc.path.manifest_prepare",
            Self::AppPcPathManifestCount => "app.pc.path.manifest_count",
            Self::WasmResultCanonicalize => "wasm.finalize.result_canonicalize",
            Self::FinesseGeometry => "finesse.geometry",
            Self::FinesseTargetGrouping => "finesse.target_grouping",
            Self::FinesseMovementBfs => "finesse.movement_bfs",
            Self::FinesseAnnotationPrune => "finesse.annotation_prune",
            Self::FinesseProductDp => "finesse.product_dp",
            Self::FinesseAggregation => "finesse.aggregation",
            Self::FinesseWitness => "finesse.witness",
            #[cfg(feature = "placement-dependency-pc")]
            Self::PlacementDependencyPc => "pc.placement_dependency.proof_and_coverage",
            #[cfg(feature = "placement-dependency-pc")]
            Self::PlacementDependencyPcLanguage => "pc.placement_dependency.language",
            Self::ExtendedPcCandidateVerifyAndReduce => {
                "rust.extended_pc.candidate_verify_and_reduce"
            }
            #[cfg(any(
                feature = "placement-dependency-pc",
                test,
                all(
                    feature = "local-search-ab",
                    feature = "parallel",
                    not(target_family = "wasm")
                )
            ))]
            Self::StrictPrefixUpper => "wasm.candidate.strict_prefix.upper",
            #[cfg(any(
                feature = "placement-dependency-pc",
                test,
                all(
                    feature = "local-search-ab",
                    feature = "parallel",
                    not(target_family = "wasm")
                )
            ))]
            Self::StrictPrefixReverse => "wasm.candidate.strict_prefix.reverse",
            #[cfg(any(
                feature = "placement-dependency-pc",
                test,
                all(
                    feature = "local-search-ab",
                    feature = "parallel",
                    not(target_family = "wasm")
                )
            ))]
            Self::StrictPrefixLanguage => "wasm.candidate.strict_prefix.language",
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            Self::StrictReferenceLanguage => "wasm.candidate.strict_reference.language",
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            Self::StrictStandardBagProduct => "wasm.candidate.strict.standard_bag_product",
        }
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
#[derive(Clone, Copy, Debug, Default)]
struct StageTotals {
    duration: Duration,
    invocation_count: u64,
    work_item_count: u64,
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
type SearchStageTotals = [StageTotals; ExecutorSearchStage::COUNT];

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
struct ThreadSearchProfile {
    totals: SearchStageTotals,
    #[cfg(not(target_family = "wasm"))]
    worker_totals: Option<Arc<Mutex<SearchStageTotals>>>,
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl Default for ThreadSearchProfile {
    fn default() -> Self {
        Self {
            totals: [StageTotals::default(); ExecutorSearchStage::COUNT],
            #[cfg(not(target_family = "wasm"))]
            worker_totals: None,
        }
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
thread_local! {
    static ACTIVE_PROFILE: RefCell<Option<ThreadSearchProfile>> = const { RefCell::new(None) };
}

/// A token captured by the submitting thread, not by a process-wide profile.
/// Measure only the job body, and publish its completion after this returns.
/// A pooled thread then leaves with no residual state from the request.
#[cfg(feature = "parallel")]
pub struct WorkerSearchProfile {
    #[cfg(all(
        not(target_family = "wasm"),
        any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
    ))]
    totals: Option<Arc<Mutex<SearchStageTotals>>>,
}

#[cfg(feature = "parallel")]
impl WorkerSearchProfile {
    #[inline]
    pub fn capture() -> Self {
        Self {
            #[cfg(all(
                not(target_family = "wasm"),
                any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
            ))]
            totals: ACTIVE_PROFILE.with(|slot| {
                let mut slot = slot.borrow_mut();
                let profile = slot.as_mut()?;
                Some(Arc::clone(profile.worker_totals.get_or_insert_with(|| {
                    Arc::new(Mutex::new(
                        [StageTotals::default(); ExecutorSearchStage::COUNT],
                    ))
                })))
            }),
        }
    }

    #[inline]
    pub fn measure<R>(self, job: impl FnOnce() -> R) -> R {
        #[cfg(all(
            not(target_family = "wasm"),
            any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
        ))]
        let _guard = self.totals.map(|totals| {
            // Keep the parent accumulator in the active TLS for any worker jobs
            // that this job itself submits. Its own hot counters remain private.
            let previous = ACTIVE_PROFILE.with(|slot| {
                slot.replace(Some(ThreadSearchProfile {
                    worker_totals: Some(Arc::clone(&totals)),
                    ..ThreadSearchProfile::default()
                }))
            });
            WorkerSearchProfileGuard { totals, previous }
        });
        job()
    }
}

#[cfg(all(
    feature = "parallel",
    not(target_family = "wasm"),
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
))]
struct WorkerSearchProfileGuard {
    totals: Arc<Mutex<SearchStageTotals>>,
    previous: Option<ThreadSearchProfile>,
}

#[cfg(all(
    feature = "parallel",
    not(target_family = "wasm"),
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
))]
impl Drop for WorkerSearchProfileGuard {
    fn drop(&mut self) {
        let Some(job) = ACTIVE_PROFILE.with(|slot| slot.replace(self.previous.take())) else {
            return;
        };
        // At most one merge per logical job, outside every timed span. Duration
        // is accumulated work time; overlapping stages can exceed wall time.
        let mut totals = self.totals.lock().expect("worker profile totals");
        add_stage_totals(&mut totals, job.totals);
    }
}

#[cfg(all(
    not(target_family = "wasm"),
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
))]
fn add_stage_totals(totals: &mut SearchStageTotals, additional: SearchStageTotals) {
    for (total, next) in totals.iter_mut().zip(additional) {
        total.duration = total.duration.saturating_add(next.duration);
        total.invocation_count = total.invocation_count.saturating_add(next.invocation_count);
        total.work_item_count = total.work_item_count.saturating_add(next.work_item_count);
    }
}

#[inline]
pub(crate) const fn profile_sample_scale(ordinal: usize) -> u64 {
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    {
        if ordinal <= 4_096 {
            1
        } else if ordinal & 1_023 == 0 {
            1_024
        } else {
            0
        }
    }
    #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
    {
        let _ = ordinal;
        0
    }
}

/// A coarse App-side stage in the same opt-in profile as the Core PC replay stages.
///
/// With profiling disabled this is a no-op. Dropping an active span counts a
/// single manifest preparation or bounded count slice; it does not count pairs
/// of queues and candidates.
pub struct PcReplayProfileSpan(SearchStageSpan);

impl PcReplayProfileSpan {
    /// Measures one grouped replay manifest preparation.
    #[inline]
    pub fn manifest_prepare() -> Self {
        Self(SearchStageSpan::begin(
            ExecutorSearchStage::AppPcPathManifestPrepare,
        ))
    }

    /// Measures one bounded grouped replay manifest count slice.
    #[inline]
    pub fn manifest_count() -> Self {
        Self(SearchStageSpan::begin(
            ExecutorSearchStage::AppPcPathManifestCount,
        ))
    }

    /// Records the number of completed units instead of counting a single slice.
    #[inline]
    pub fn finish(self, work_items: u64) {
        self.0.finish(work_items);
    }
}

pub(crate) struct SearchStageSpan {
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    stage: ExecutorSearchStage,
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    started: Option<StageInstant>,
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    scale: u64,
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    finished: bool,
}

impl SearchStageSpan {
    #[inline]
    pub(crate) fn begin(stage: ExecutorSearchStage) -> Self {
        Self::begin_scaled(stage, 1)
    }

    /// Starts a profiling-only sampled span. `scale == 0` is an inactive span;
    /// otherwise elapsed time and counters are scaled to represent the skipped
    /// calls. Product builds compile this method down to a no-op.
    #[inline]
    pub(crate) fn begin_scaled(stage: ExecutorSearchStage, scale: u64) -> Self {
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        {
            let active = scale != 0 && ACTIVE_PROFILE.with(|profile| profile.borrow().is_some());
            Self {
                stage,
                started: active.then(stage_now),
                scale: if active { scale } else { 0 },
                finished: false,
            }
        }
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        {
            let _ = (stage, scale);
            Self {}
        }
    }

    #[inline]
    pub(crate) fn finish(self, work_items: u64) {
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        {
            let mut span = self;
            span.record(work_items);
        }
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        let _ = work_items;
    }

    /// Publish an already accumulated, exact work count without sampling it
    /// or inventing duration/call counts. Hot loops accumulate locally and
    /// flush once at their owner boundary. Timed spans for the same work must
    /// use `finish(0)` so the weighted estimate is not added a second time.
    #[inline]
    pub(crate) fn record_exact_work(stage: ExecutorSearchStage, work_items: u64) {
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        if work_items != 0 {
            ACTIVE_PROFILE.with(|profile| {
                let mut profile = profile.borrow_mut();
                if let Some(profile) = profile.as_mut() {
                    let total = &mut profile.totals[stage as usize];
                    total.work_item_count = total.work_item_count.saturating_add(work_items);
                }
            });
        }
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        let _ = (stage, work_items);
    }

    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    fn record(&mut self, work_items: u64) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.scale == 0 {
            return;
        }
        let elapsed = self
            .started
            .take()
            .map(|started| {
                stage_elapsed(started).saturating_mul(self.scale.min(u64::from(u32::MAX)) as u32)
            })
            .unwrap_or_default();
        ACTIVE_PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            let Some(profile) = profile.as_mut() else {
                return;
            };
            let stage = &mut profile.totals[self.stage as usize];
            stage.duration = stage.duration.saturating_add(elapsed);
            stage.invocation_count = stage.invocation_count.saturating_add(self.scale);
            stage.work_item_count = stage
                .work_item_count
                .saturating_add(work_items.saturating_mul(self.scale));
        });
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl Drop for SearchStageSpan {
    fn drop(&mut self) {
        self.record(1);
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorSearchProfileError {
    AlreadyActive,
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutorSearchProfileStage {
    pub name: &'static str,
    /// Accumulated stage time, not end-to-end wall time. Sampled spans scale
    /// their durations and counters; overlapping stages may count the same
    /// interval on one thread or across workers.
    pub duration_ns: u64,
    pub invocation_count: u64,
    /// Stage-defined work units. Ordinary sampled spans scale this field;
    /// stages using `record_exact_work` publish an unsampled owner total.
    pub work_item_count: u64,
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
pub struct ExecutorSearchProfileSession {
    active: bool,
    _thread_bound: PhantomData<Rc<()>>,
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl ExecutorSearchProfileSession {
    pub fn start() -> Result<Self, ExecutorSearchProfileError> {
        let installed = ACTIVE_PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            if profile.is_some() {
                return false;
            }
            *profile = Some(ThreadSearchProfile::default());
            true
        });
        if !installed {
            return Err(ExecutorSearchProfileError::AlreadyActive);
        }
        Ok(Self {
            active: true,
            _thread_bound: PhantomData,
        })
    }

    pub fn finish(mut self) -> Vec<ExecutorSearchProfileStage> {
        let profile = self.take_profile();
        let totals = profile.totals;
        #[cfg(not(target_family = "wasm"))]
        let totals = {
            let mut totals = totals;
            if let Some(worker_totals) = profile.worker_totals {
                add_stage_totals(
                    &mut totals,
                    *worker_totals.lock().expect("worker profile totals"),
                );
            }
            totals
        };
        Self::stage_rows(totals)
    }

    /// A non-consuming diagnostic snapshot of this thread's finished spans.
    /// Running spans and other workers are excluded. Reading a checkpoint
    /// neither resets counters nor grants completion to the search.
    pub fn snapshot_current_thread() -> Option<Vec<ExecutorSearchProfileStage>> {
        ACTIVE_PROFILE.with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|profile| Self::stage_rows(profile.totals))
        })
    }

    /// Return the submitting thread and pooled worker aggregates separately.
    /// The submitting thread may also run a native worker. Neither list is
    /// exclusive wall time: different spans can overlap or be nested.
    #[cfg(not(target_family = "wasm"))]
    pub fn finish_partitioned(
        mut self,
    ) -> (
        Vec<ExecutorSearchProfileStage>,
        Vec<ExecutorSearchProfileStage>,
    ) {
        let profile = self.take_profile();
        let workers = profile.worker_totals.map_or(
            [StageTotals::default(); ExecutorSearchStage::COUNT],
            |totals| *totals.lock().expect("worker profile totals"),
        );
        (Self::stage_rows(profile.totals), Self::stage_rows(workers))
    }

    fn take_profile(&mut self) -> ThreadSearchProfile {
        self.active = false;
        ACTIVE_PROFILE
            .with(|slot| slot.borrow_mut().take())
            .unwrap_or_default()
    }

    fn stage_rows(totals: SearchStageTotals) -> Vec<ExecutorSearchProfileStage> {
        ExecutorSearchStage::ALL
            .into_iter()
            .zip(totals)
            .map(|(stage, totals)| ExecutorSearchProfileStage {
                name: stage.name(),
                duration_ns: totals.duration.as_nanos().min(u128::from(u64::MAX)) as u64,
                invocation_count: totals.invocation_count,
                work_item_count: totals.work_item_count,
            })
            .collect()
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl Drop for ExecutorSearchProfileSession {
    fn drop(&mut self) {
        if self.active {
            ACTIVE_PROFILE.with(|profile| {
                profile.borrow_mut().take();
            });
        }
    }
}

#[cfg(all(
    test,
    feature = "parallel",
    not(target_family = "wasm"),
    any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")
))]
mod tests {
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{mpsc, Barrier},
        thread,
    };

    use super::{
        ExecutorSearchProfileSession, ExecutorSearchProfileStage, ExecutorSearchStage,
        PcReplayProfileSpan, SearchStageSpan, WorkerSearchProfile, ACTIVE_PROFILE,
    };

    const STAGE: ExecutorSearchStage = ExecutorSearchStage::WasmGeometryAdvance;

    fn stage(stages: &[ExecutorSearchProfileStage]) -> &ExecutorSearchProfileStage {
        stages
            .iter()
            .find(|stage| stage.name == STAGE.name())
            .expect("fixed stage")
    }

    #[test]
    fn diagnostic_snapshot_preserves_active_profile_and_terminal_totals() {
        assert!(ExecutorSearchProfileSession::snapshot_current_thread().is_none());
        let session = ExecutorSearchProfileSession::start().unwrap();
        SearchStageSpan::record_exact_work(ExecutorSearchStage::WasmGeometrySupportRows, 7);
        let snapshot = ExecutorSearchProfileSession::snapshot_current_thread().unwrap();
        assert_eq!(
            snapshot
                .iter()
                .find(|row| row.name == "wasm.geometry.support_rows")
                .unwrap()
                .work_item_count,
            7,
        );
        SearchStageSpan::record_exact_work(ExecutorSearchStage::WasmGeometrySupportRows, 11);
        let snapshot = ExecutorSearchProfileSession::snapshot_current_thread().unwrap();
        assert_eq!(
            snapshot
                .iter()
                .find(|row| row.name == "wasm.geometry.support_rows")
                .unwrap()
                .work_item_count,
            18,
        );
        assert_eq!(session.finish(), snapshot);
        assert!(ExecutorSearchProfileSession::snapshot_current_thread().is_none());
    }

    #[test]
    fn worker_profile_exact_work_is_not_scaled_or_counted_as_a_timed_call() {
        SearchStageSpan::record_exact_work(STAGE, 99);
        let session = ExecutorSearchProfileSession::start().unwrap();
        SearchStageSpan::begin_scaled(STAGE, 1024).finish(0);
        SearchStageSpan::record_exact_work(STAGE, 7);
        let worker = WorkerSearchProfile::capture();
        worker.measure(|| {
            SearchStageSpan::begin_scaled(STAGE, 2048).finish(0);
            SearchStageSpan::record_exact_work(STAGE, 11);
            SearchStageSpan::record_exact_work(ExecutorSearchStage::WasmGeometrySupportRows, 31);
        });
        let (caller, pooled) = session.finish_partitioned();
        assert_eq!(stage(&caller).invocation_count, 1024);
        assert_eq!(stage(&caller).work_item_count, 7);
        assert_eq!(stage(&pooled).invocation_count, 2048);
        assert_eq!(stage(&pooled).work_item_count, 11);
        let exact_only = pooled
            .iter()
            .find(|row| row.name == ExecutorSearchStage::WasmGeometrySupportRows.name())
            .unwrap();
        assert_eq!(exact_only.duration_ns, 0);
        assert_eq!(exact_only.invocation_count, 0);
        assert_eq!(exact_only.work_item_count, 31);
        let subsequent = ExecutorSearchProfileSession::start().unwrap();
        assert_eq!(stage(&subsequent.finish()).work_item_count, 0);
    }

    #[test]
    fn worker_profile_pc_path_stages_keep_work_counts_separate() {
        let session = ExecutorSearchProfileSession::start().expect("caller profile");
        SearchStageSpan::begin(ExecutorSearchStage::WasmPcPathExecutionGraphs).finish(3);
        SearchStageSpan::begin(ExecutorSearchStage::WasmPcPathPatternRows).finish(7);
        {
            let _span = PcReplayProfileSpan::manifest_prepare();
        }
        PcReplayProfileSpan::manifest_count().finish(1);
        {
            let _span = PcReplayProfileSpan::manifest_count();
        }

        let stages = session.finish();
        for (name, calls, items) in [
            ("wasm.pc.path.execution_graphs", 1, 3),
            ("wasm.pc.path.pattern_rows", 1, 7),
            ("app.pc.path.manifest_prepare", 1, 1),
            ("app.pc.path.manifest_count", 2, 2),
        ] {
            let stage = stages
                .iter()
                .find(|stage| stage.name == name)
                .expect("named PC path stage");
            assert_eq!(
                (stage.invocation_count, stage.work_item_count),
                (calls, items)
            );
        }
    }

    #[test]
    fn worker_profile_is_merged_before_completion_is_published() {
        let session = ExecutorSearchProfileSession::start().expect("caller profile");
        SearchStageSpan::begin(STAGE).finish(3);
        let (sender, receiver) = mpsc::channel();
        let stages = thread::scope(|scope| {
            for items in [5, 7] {
                let profile = WorkerSearchProfile::capture();
                let sender = sender.clone();
                scope.spawn(move || {
                    profile.measure(|| SearchStageSpan::begin_scaled(STAGE, 2).finish(items));
                    sender.send(()).expect("completion receiver");
                });
            }
            drop(sender);
            receiver.recv().expect("first completion");
            receiver.recv().expect("second completion");
            // Match the real pool protocol: finish after acknowledgements, not
            // after joining the operating-system threads.
            session.finish()
        });
        assert_eq!(stage(&stages).invocation_count, 5);
        assert_eq!(stage(&stages).work_item_count, 27);
    }

    #[test]
    fn diagnostic_profile_distinguishes_caller_from_pool_worker_work() {
        let session = ExecutorSearchProfileSession::start().expect("caller profile");
        SearchStageSpan::begin(STAGE).finish(3);
        let worker = WorkerSearchProfile::capture();
        thread::scope(|scope| {
            scope
                .spawn(|| worker.measure(|| SearchStageSpan::begin_scaled(STAGE, 2).finish(5)))
                .join()
                .expect("worker job");
        });
        let (caller, pool) = session.finish_partitioned();
        assert_eq!(stage(&caller).invocation_count, 1);
        assert_eq!(stage(&caller).work_item_count, 3);
        assert_eq!(stage(&pool).invocation_count, 2);
        assert_eq!(stage(&pool).work_item_count, 10);
    }

    #[test]
    fn worker_profile_sessions_are_isolated_on_one_reused_worker() {
        let both_submitted = Barrier::new(2);
        let (jobs, jobs_rx) = mpsc::channel::<(WorkerSearchProfile, u64, mpsc::Sender<()>)>();
        let run_request = |items| {
            let profile = ExecutorSearchProfileSession::start().expect("request profile");
            let (done, wait) = mpsc::channel();
            jobs.send((WorkerSearchProfile::capture(), items, done))
                .expect("reused worker");
            both_submitted.wait();
            wait.recv().expect("job completion");
            let stages = profile.finish();
            assert_eq!(stage(&stages).invocation_count, 1);
            assert_eq!(stage(&stages).work_item_count, items);
        };
        thread::scope(|scope| {
            scope.spawn(move || {
                for (profile, items, done) in jobs_rx.iter().take(2) {
                    profile.measure(|| SearchStageSpan::begin(STAGE).finish(items));
                    assert!(ACTIVE_PROFILE.with(|slot| slot.borrow().is_none()));
                    done.send(()).expect("completion receiver");
                }
            });
            scope.spawn(|| run_request(19));
            run_request(23);
        });
    }

    #[test]
    fn worker_profile_restores_thread_state_when_a_job_panics() {
        let session = ExecutorSearchProfileSession::start().expect("caller profile");
        let profile = WorkerSearchProfile::capture();
        thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                profile.measure(|| {
                    SearchStageSpan::begin(STAGE).finish(4);
                    panic!("synthetic worker panic");
                });
            }));
            assert!(result.is_err());
            assert!(ACTIVE_PROFILE.with(|slot| slot.borrow().is_none()));
            let separate = ExecutorSearchProfileSession::start().expect("fresh worker profile");
            assert_eq!(stage(&separate.finish()).work_item_count, 0);
        })
        .join()
        .expect("worker handled panic");
        assert_eq!(stage(&session.finish()).work_item_count, 4);
    }
}
