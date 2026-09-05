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
    #[cfg(feature = "webgpu-search")]
    PackingGpuBatchPlan,
    #[cfg(feature = "webgpu-search")]
    PackingGpuConnect,
    #[cfg(feature = "webgpu-search")]
    // Native WebGPU combines dispatch and readback in this stage; the browser
    // backend emits the split host/dispatch/payload stages below.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PackingGpuDispatchReadback,
    #[cfg(feature = "webgpu-search")]
    PackingGpuHostPrepareSubmit,
    #[cfg(feature = "webgpu-search")]
    PackingGpuDispatchCounterWait,
    #[cfg(feature = "webgpu-search")]
    PackingGpuPayloadReadback,
    #[cfg(feature = "webgpu-search")]
    PackingGpuHostReduce,
    #[cfg(feature = "webgpu-search")]
    PackingGpuTraceEnumeration,
    #[cfg(feature = "webgpu-search")]
    // Canonicalization is currently emitted by the native geometry backend;
    // retain the stage identity so cross-target profiler schemas stay stable.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PackingGpuCanonicalize,
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
    WasmGeometryAdvance,
    WasmCandidateProjection,
    WasmCandidateFeasibility,
    WasmBuildOrderReachability,
    WasmCoverageLanguageProduct,
    WasmWitnessSearch,
    WasmCandidateResultReduce,
    WasmFinalCoverage,
    WasmMinimumCoverProof,
    WasmSpinExecutionGraphPrepare,
    WasmResultCanonicalize,
    FinesseGeometry,
    FinesseTargetGrouping,
    FinesseMovementBfs,
    FinesseAnnotationPrune,
    FinesseProductDp,
    FinesseAggregation,
    FinesseWitness,
}

impl ExecutorSearchStage {
    #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
    const COUNT: usize = Self::FinesseWitness as usize + 1;

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
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuBatchPlan,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuConnect,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuDispatchReadback,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuHostPrepareSubmit,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuDispatchCounterWait,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuPayloadReadback,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuHostReduce,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuTraceEnumeration,
        #[cfg(feature = "webgpu-search")]
        Self::PackingGpuCanonicalize,
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
        Self::WasmGeometryAdvance,
        Self::WasmCandidateProjection,
        Self::WasmCandidateFeasibility,
        Self::WasmBuildOrderReachability,
        Self::WasmCoverageLanguageProduct,
        Self::WasmWitnessSearch,
        Self::WasmCandidateResultReduce,
        Self::WasmFinalCoverage,
        Self::WasmMinimumCoverProof,
        Self::WasmSpinExecutionGraphPrepare,
        Self::WasmResultCanonicalize,
        Self::FinesseGeometry,
        Self::FinesseTargetGrouping,
        Self::FinesseMovementBfs,
        Self::FinesseAnnotationPrune,
        Self::FinesseProductDp,
        Self::FinesseAggregation,
        Self::FinesseWitness,
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
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuBatchPlan => "rust.packing.gpu.batch_plan",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuConnect => "rust.packing.gpu.connect",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuDispatchReadback => "rust.packing.gpu.dispatch_readback",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuHostPrepareSubmit => "rust.packing.gpu.host_prepare_submit",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuDispatchCounterWait => "rust.packing.gpu.dispatch_counter_wait",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuPayloadReadback => "rust.packing.gpu.payload_readback",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuHostReduce => "rust.packing.gpu.host_exact_reduce",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuTraceEnumeration => "rust.packing.gpu.trace_enumeration",
            #[cfg(feature = "webgpu-search")]
            Self::PackingGpuCanonicalize => "rust.packing.gpu.canonicalize",
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
            Self::WasmGeometryAdvance => "wasm.geometry.advance",
            Self::WasmCandidateProjection => "wasm.candidate.projection",
            Self::WasmCandidateFeasibility => "wasm.candidate.feasibility",
            Self::WasmBuildOrderReachability => "wasm.candidate.build_order_reachability",
            Self::WasmCoverageLanguageProduct => "wasm.candidate.coverage_language_product",
            Self::WasmWitnessSearch => "wasm.candidate.witness_search",
            Self::WasmCandidateResultReduce => "wasm.candidate.result_reduce",
            Self::WasmFinalCoverage => "wasm.finalize.coverage",
            Self::WasmMinimumCoverProof => "wasm.finalize.minimum_cover_proof",
            Self::WasmSpinExecutionGraphPrepare => "wasm.finalize.spin_execution_graph_prepare",
            Self::WasmResultCanonicalize => "wasm.finalize.result_canonicalize",
            Self::FinesseGeometry => "finesse.geometry",
            Self::FinesseTargetGrouping => "finesse.target_grouping",
            Self::FinesseMovementBfs => "finesse.movement_bfs",
            Self::FinesseAnnotationPrune => "finesse.annotation_prune",
            Self::FinesseProductDp => "finesse.product_dp",
            Self::FinesseAggregation => "finesse.aggregation",
            Self::FinesseWitness => "finesse.witness",
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
thread_local! {
    static ACTIVE_PROFILE: RefCell<Option<[StageTotals; ExecutorSearchStage::COUNT]>> =
        const { RefCell::new(None) };
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

    #[cfg(feature = "webgpu-search")]
    pub(crate) fn record_elapsed(
        stage: ExecutorSearchStage,
        elapsed: Option<std::time::Duration>,
        work_items: u64,
    ) {
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        if let Some(elapsed) = elapsed {
            ACTIVE_PROFILE.with(|profile| {
                let mut profile = profile.borrow_mut();
                let Some(totals) = profile.as_mut() else {
                    return;
                };
                let stage = &mut totals[stage as usize];
                stage.duration = stage.duration.saturating_add(elapsed);
                stage.invocation_count = stage.invocation_count.saturating_add(1);
                stage.work_item_count = stage.work_item_count.saturating_add(work_items);
            });
        }
        #[cfg(not(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling")))]
        {
            let _ = (stage, elapsed, work_items);
        }
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
            let Some(totals) = profile.as_mut() else {
                return;
            };
            let stage = &mut totals[self.stage as usize];
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
    pub duration_ns: u64,
    pub invocation_count: u64,
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
            *profile = Some([StageTotals::default(); ExecutorSearchStage::COUNT]);
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
        self.active = false;
        let totals = ACTIVE_PROFILE
            .with(|profile| profile.borrow_mut().take())
            .unwrap_or([StageTotals::default(); ExecutorSearchStage::COUNT]);
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
