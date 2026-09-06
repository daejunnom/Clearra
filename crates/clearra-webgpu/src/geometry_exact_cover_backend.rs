use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[cfg(not(target_family = "wasm"))]
use std::sync::{Mutex, OnceLock};
#[cfg(target_family = "wasm")]
use std::{cell::RefCell, rc::Rc};

use crate::{
    adapter_selection::{select_adapter, WebGpuAdapterSelection, WebGpuAdapterSummary},
    geometry_exact_cover_cpu_confirm::CpuReferenceSampler,
    geometry_exact_cover_dispatch::{
        byte_len, execute_layer, params_words, size_of_u32, storage_buffer,
        storage_buffer_copy_source, uniform_buffer, LayerScratch, WORKGROUP_SIZE,
    },
    geometry_exact_cover_model::{CERTIFIED_CONSTRAINT_WORDS, STATE_WORDS, TRACE_WORDS},
    geometry_exact_cover_reduce::{ExactFrontierReduceStage, ReducedFrontier, TraceLayer},
    geometry_exact_cover_result::{
        reduce_failure_outcome, resource_incomplete, WebGpuGeometrySolutionGraph,
    },
    geometry_exact_cover_timing::{WebGpuGeometryExactCoverTimings, WebGpuStageTimer},
    WebGpuShaderContract, WebGpuUnavailableResult,
};

pub use crate::geometry_exact_cover_model::{
    WebGpuExactCoverCatalog, WebGpuGeometryCatalogIdentity, WebGpuGeometryExactCoverBatch,
    WebGpuGeometryExactCoverConnected, WebGpuGeometryExactCoverIncomplete,
    WebGpuGeometryExactCoverInputError, WebGpuGeometryExactCoverOutcome, WebGpuPackingTrustState,
    WebGpuPlacementSkeleton,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebGpuGeometryExactCoverBackend;

pub struct WebGpuGeometryExactCoverSession {
    selection: WebGpuAdapterSelection,
    reused: bool,
    context: SharedWebGpuDeviceContext,
    static_buffers: Option<StaticPackingBuffers>,
    layer_scratch: Option<LayerScratch>,
}

struct WebGpuDeviceContext {
    auto_selected: AtomicBool,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    limits: wgpu::Limits,
    shader_version: &'static str,
    shader_hash: String,
    adapter: WebGpuAdapterSummary,
    zero_counter_buffer: wgpu::Buffer,
}

// Native sessions can cross worker threads and therefore share a Send + Sync
// context through Arc. Browser WebGPU handles are thread-affine and live in a
// thread-local cache, so Rc is the accurate ownership primitive on wasm.
#[cfg(not(target_family = "wasm"))]
type SharedWebGpuDeviceContext = Arc<WebGpuDeviceContext>;
#[cfg(target_family = "wasm")]
type SharedWebGpuDeviceContext = Rc<WebGpuDeviceContext>;

struct StaticPackingBuffers {
    catalog: Arc<dyn WebGpuExactCoverCatalog>,
    skeleton_mask_buffer: wgpu::Buffer,
    skeleton_piece_buffer: wgpu::Buffer,
    support_offset_buffer: wgpu::Buffer,
    support_operation_buffer: wgpu::Buffer,
    constraint_buffer: wgpu::Buffer,
    resident_bytes: u64,
}

// Keep the public session outcome unboxed so existing executor ownership stays stable.
#[allow(clippy::large_enum_variant)]
pub enum WebGpuGeometryExactCoverSessionOutcome {
    Connected(WebGpuGeometryExactCoverSession),
    Unavailable(WebGpuUnavailableResult),
}

impl WebGpuGeometryExactCoverSession {
    pub fn adapter(&self) -> &WebGpuAdapterSummary {
        &self.context.adapter
    }

    pub const fn reused(&self) -> bool {
        self.reused
    }

    pub fn recycle(self) {
        cache_session(self);
    }

    pub async fn run(
        &mut self,
        batch: &WebGpuGeometryExactCoverBatch,
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        WebGpuGeometryExactCoverBackend::run_connected(self, batch).await
    }

    pub async fn run_family(
        &mut self,
        batches: &[WebGpuGeometryExactCoverBatch],
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        WebGpuGeometryExactCoverBackend::run_family_connected(self, batches, 1, &|| false).await
    }

    pub async fn run_family_with_host_workers(
        &mut self,
        batches: &[WebGpuGeometryExactCoverBatch],
        host_workers: usize,
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        WebGpuGeometryExactCoverBackend::run_family_connected(self, batches, host_workers, &|| {
            false
        })
        .await
    }

    pub async fn run_family_with_host_workers_and_control(
        &mut self,
        batches: &[WebGpuGeometryExactCoverBatch],
        host_workers: usize,
        should_cancel: &(dyn Fn() -> bool + Sync),
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        WebGpuGeometryExactCoverBackend::run_family_connected(
            self,
            batches,
            host_workers,
            should_cancel,
        )
        .await
    }

    /// Uploads the immutable geometry catalog before the first dispatch.
    /// Dynamic frontier and readback scratch remain deferred until their exact
    /// dispatch sizes are known.
    pub fn prepare_family(
        &mut self,
        batches: &[WebGpuGeometryExactCoverBatch],
    ) -> Result<(), WebGpuGeometryExactCoverInputError> {
        let batch = batches
            .first()
            .ok_or(WebGpuGeometryExactCoverInputError::IncompatibleBatchFamily)?;
        if batches
            .iter()
            .skip(1)
            .any(|member| !batch.can_share_family_dispatch(member))
        {
            return Err(WebGpuGeometryExactCoverInputError::IncompatibleBatchFamily);
        }
        self.ensure_static_buffers(batch)
    }

    fn ensure_static_buffers(
        &mut self,
        batch: &WebGpuGeometryExactCoverBatch,
    ) -> Result<(), WebGpuGeometryExactCoverInputError> {
        let cache_matches = self
            .static_buffers
            .as_ref()
            .is_some_and(|buffers| Arc::ptr_eq(&buffers.catalog, batch.catalog()));
        if !cache_matches {
            self.layer_scratch = None;
            let constraint_words = padded_constraint_words(batch)?;
            let resident_bytes = byte_len(batch.skeleton_cell_masks())?
                .saturating_add(byte_len(batch.skeleton_piece_kinds())?)
                .saturating_add(byte_len(batch.support_offsets())?)
                .saturating_add(byte_len(batch.support_operations())?)
                .saturating_add(byte_len(&constraint_words)?);
            self.static_buffers = Some(StaticPackingBuffers {
                catalog: Arc::clone(batch.catalog()),
                skeleton_mask_buffer: storage_buffer(
                    &self.context.device,
                    "exact-cover-skeleton-masks",
                    batch.skeleton_cell_masks(),
                ),
                skeleton_piece_buffer: storage_buffer(
                    &self.context.device,
                    "exact-cover-skeleton-piece-kinds",
                    batch.skeleton_piece_kinds(),
                ),
                support_offset_buffer: storage_buffer(
                    &self.context.device,
                    "exact-cover-support-offsets",
                    batch.support_offsets(),
                ),
                support_operation_buffer: storage_buffer(
                    &self.context.device,
                    "exact-cover-support-rows",
                    batch.support_operations(),
                ),
                constraint_buffer: uniform_buffer(
                    &self.context.device,
                    "exact-cover-certified-constraints",
                    &constraint_words,
                ),
                resident_bytes,
            });
        }
        self.static_buffers
            .as_ref()
            .map(|_| ())
            .ok_or(WebGpuGeometryExactCoverInputError::StaticBufferCache)
    }
}

fn padded_constraint_words(
    batch: &WebGpuGeometryExactCoverBatch,
) -> Result<[u32; CERTIFIED_CONSTRAINT_WORDS], WebGpuGeometryExactCoverInputError> {
    let source = batch.certified_constraint_words();
    if source.len() > CERTIFIED_CONSTRAINT_WORDS {
        return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
    }
    let mut words = [0_u32; CERTIFIED_CONSTRAINT_WORDS];
    words[..source.len()].copy_from_slice(source);
    Ok(words)
}

impl WebGpuGeometryExactCoverBackend {
    pub async fn adapter_available() -> bool {
        Self::adapter_available_selected(WebGpuAdapterSelection::Auto).await
    }

    pub async fn adapter_available_selected(selection: WebGpuAdapterSelection) -> bool {
        select_adapter(selection).await.is_ok()
    }

    pub async fn run(
        batch: &WebGpuGeometryExactCoverBatch,
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        let mut outcomes = Self::run_many(std::slice::from_ref(batch)).await?;
        Ok(outcomes
            .pop()
            .expect("a one-batch WebGPU run must return one outcome"))
    }

    pub async fn run_many(
        batches: &[WebGpuGeometryExactCoverBatch],
    ) -> Result<Vec<WebGpuGeometryExactCoverOutcome>, WebGpuGeometryExactCoverInputError> {
        if batches.is_empty() {
            return Ok(Vec::new());
        }
        let mut session = match Self::connect().await {
            WebGpuGeometryExactCoverSessionOutcome::Connected(session) => session,
            WebGpuGeometryExactCoverSessionOutcome::Unavailable(unavailable) => {
                return Ok(vec![WebGpuGeometryExactCoverOutcome::Unavailable(
                    unavailable,
                )]);
            }
        };
        let mut outcomes = Vec::with_capacity(batches.len());
        for batch in batches {
            outcomes.push(Self::run_connected(&mut session, batch).await?);
        }
        session.recycle();
        Ok(outcomes)
    }

    pub async fn connect() -> WebGpuGeometryExactCoverSessionOutcome {
        Self::connect_selected(WebGpuAdapterSelection::Auto).await
    }

    pub async fn connect_selected(
        selection: WebGpuAdapterSelection,
    ) -> WebGpuGeometryExactCoverSessionOutcome {
        if let Some(session) = Self::take_prepared_session(selection) {
            return WebGpuGeometryExactCoverSessionOutcome::Connected(session);
        }
        match Self::connect_device_context(selection).await {
            Ok(session) => WebGpuGeometryExactCoverSessionOutcome::Connected(session),
            Err(unavailable) => WebGpuGeometryExactCoverSessionOutcome::Unavailable(unavailable),
        }
    }

    /// Returns only already-created device state and never performs adapter
    /// discovery, device creation, shader compilation, or pipeline creation.
    pub fn take_prepared_session(
        selection: WebGpuAdapterSelection,
    ) -> Option<WebGpuGeometryExactCoverSession> {
        take_cached_session(selection).or_else(|| {
            cached_device_context(selection)
                .map(|context| session_from_context(selection, context, true))
        })
    }

    async fn connect_device_context(
        selection: WebGpuAdapterSelection,
    ) -> Result<WebGpuGeometryExactCoverSession, WebGpuUnavailableResult> {
        let contract = WebGpuShaderContract::embedded_geometry_exact_cover();
        let selected = select_adapter(selection).await.map_err(|error| {
            WebGpuUnavailableResult::new(format!("webgpu_adapter_unavailable: {error}"))
        })?;
        let adapter = selected.adapter;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("clearra-geometry-exact-cover-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|error| {
                WebGpuUnavailableResult::new(format!("webgpu_device_unavailable: {error}"))
            })?;
        let limits = device.limits();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clearra-geometry-exact-cover-packing"),
            source: wgpu::ShaderSource::Wgsl(contract.shader_source().into()),
        });
        let shader_errors = shader
            .get_compilation_info()
            .await
            .messages
            .into_iter()
            .filter(|message| message.message_type == wgpu::CompilationMessageType::Error)
            .map(|message| message.message)
            .collect::<Vec<_>>();
        if !shader_errors.is_empty() {
            return Err(WebGpuUnavailableResult::new(format!(
                "webgpu_shader_compile_failed: {}",
                shader_errors.join(" | ")
            )));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let pipeline_error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("clearra-geometry-exact-cover-pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = pipeline_error_scope.pop().await {
            return Err(WebGpuUnavailableResult::new(format!(
                "webgpu_pipeline_creation_failed: {error}"
            )));
        }
        let zero_counter_buffer = storage_buffer_copy_source(
            &device,
            "exact-cover-zero-counter-template",
            &[0_u32; crate::geometry_exact_cover_model::COUNTER_WORDS],
        );
        let (context, reused) =
            cache_device_context(SharedWebGpuDeviceContext::new(WebGpuDeviceContext {
                auto_selected: AtomicBool::new(selection == WebGpuAdapterSelection::Auto),
                device,
                queue,
                pipeline,
                limits,
                shader_version: contract.shader_version(),
                shader_hash: contract.shader_hash(),
                adapter: selected.summary,
                zero_counter_buffer,
            }));
        Ok(session_from_context(selection, context, reused))
    }

    async fn run_connected(
        session: &mut WebGpuGeometryExactCoverSession,
        batch: &WebGpuGeometryExactCoverBatch,
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        Self::run_family_connected(session, std::slice::from_ref(batch), 1, &|| false).await
    }

    async fn run_family_connected(
        session: &mut WebGpuGeometryExactCoverSession,
        batches: &[WebGpuGeometryExactCoverBatch],
        host_workers: usize,
        should_cancel: &(dyn Fn() -> bool + Sync),
    ) -> Result<WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverInputError> {
        if should_cancel() {
            return Ok(WebGpuGeometryExactCoverOutcome::Cancelled);
        }
        let batch = batches
            .first()
            .ok_or(WebGpuGeometryExactCoverInputError::IncompatibleBatchFamily)?;
        if batches
            .iter()
            .skip(1)
            .any(|member| !batch.can_share_family_dispatch(member))
        {
            return Err(WebGpuGeometryExactCoverInputError::IncompatibleBatchFamily);
        }
        let maximum_binding = session.context.limits.max_storage_buffer_binding_size;
        if [
            byte_len(batch.skeleton_cell_masks())?,
            byte_len(batch.skeleton_piece_kinds())?,
            byte_len(batch.support_offsets())?,
            byte_len(batch.support_operations())?,
        ]
        .into_iter()
        .any(|bytes| bytes > maximum_binding)
        {
            return Ok(WebGpuGeometryExactCoverOutcome::Unavailable(
                WebGpuUnavailableResult::new("webgpu_storage_buffer_limit_exceeded"),
            ));
        }
        let constraint_bytes = (CERTIFIED_CONSTRAINT_WORDS * size_of_u32()) as u64;
        if constraint_bytes > session.context.limits.max_uniform_buffer_binding_size {
            return Ok(WebGpuGeometryExactCoverOutcome::Unavailable(
                WebGpuUnavailableResult::new("webgpu_uniform_buffer_limit_exceeded"),
            ));
        }
        let maximum_buffer = session.context.limits.max_buffer_size;
        let state_record_bytes = (STATE_WORDS * size_of_u32()) as u64;
        let trace_record_bytes = (TRACE_WORDS * size_of_u32()) as u64;
        let combined_record_bytes = state_record_bytes.saturating_add(trace_record_bytes);
        let state_binding_capacity = maximum_binding / state_record_bytes;
        let trace_binding_capacity = maximum_binding / trace_record_bytes;
        let readback_capacity = maximum_buffer / combined_record_bytes;
        let device_dispatch_capacity = u32::try_from(
            state_binding_capacity
                .min(trace_binding_capacity)
                .min(readback_capacity)
                .min(u64::from(batch.frontier_capacity())),
        )
        .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
        if device_dispatch_capacity == 0 {
            return Ok(WebGpuGeometryExactCoverOutcome::Unavailable(
                WebGpuUnavailableResult::new("webgpu_storage_buffer_limit_exceeded"),
            ));
        }
        session.ensure_static_buffers(batch)?;
        let static_buffers = session
            .static_buffers
            .as_ref()
            .ok_or(WebGpuGeometryExactCoverInputError::StaticBufferCache)?;

        let mut initial_words = Vec::with_capacity(batches.len().saturating_mul(STATE_WORDS));
        for member in batches {
            initial_words.extend_from_slice(&member.initial_state_words());
        }
        let mut current_segments = vec![initial_words];
        let mut trace_layers = Vec::<TraceLayer>::new();
        let mut peak_gpu_bytes = static_buffers.resident_bytes;
        let mut peak_host_reduce_bytes = 0_usize;
        let mut timings = WebGpuGeometryExactCoverTimings::default();
        let mut cpu_reference_sampler = CpuReferenceSampler::default();
        let mut ranges = Vec::<(u32, u32)>::new();
        for _ in 0..batch.target_depth() {
            if should_cancel() {
                return Ok(WebGpuGeometryExactCoverOutcome::Cancelled);
            }
            let current_state_count = current_segments
                .iter()
                .try_fold(0usize, |total, segment| {
                    total.checked_add(segment.len() / STATE_WORDS)
                })
                .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
            if current_state_count == 0 {
                break;
            }
            let current_count = u32::try_from(current_state_count)
                .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
            let operation_count = u32::try_from(batch.skeleton_cell_masks().len())
                .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
            let input_binding_capacity = maximum_binding / state_record_bytes;
            let workgroup_parent_capacity =
                u64::from(session.context.limits.max_compute_workgroups_per_dimension)
                    .saturating_mul(u64::from(WORKGROUP_SIZE));
            let atomic_counter_parent_capacity = u64::from(u32::MAX) / u64::from(operation_count);
            let max_dispatch_parent_count = u32::try_from(
                input_binding_capacity
                    .min(workgroup_parent_capacity)
                    .min(atomic_counter_parent_capacity)
                    .min(u64::from(u32::MAX)),
            )
            .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
            if max_dispatch_parent_count == 0 {
                return Ok(WebGpuGeometryExactCoverOutcome::Unavailable(
                    WebGpuUnavailableResult::new("webgpu_parent_dispatch_limit_exceeded"),
                ));
            }
            let expected_record_count = (current_count as usize)
                .saturating_mul(operation_count as usize)
                .min(batch.frontier_capacity() as usize);
            let mut reducer = match ExactFrontierReduceStage::<STATE_WORDS>::new(
                batch.frontier_capacity() as usize,
                host_workers,
                expected_record_count,
            ) {
                Ok(reducer) => reducer,
                Err(_) => {
                    return Ok(resource_incomplete(
                        current_count,
                        batch.frontier_capacity(),
                    ));
                }
            };
            let retained_input_bytes = state_segments_resident_bytes(&current_segments)
                .saturating_add(trace_layers.iter().fold(0usize, |total, layer| {
                    total.saturating_add(layer.resident_bytes())
                }));
            let mut parent_index_base = 0u32;
            for current_words in &current_segments {
                let segment_count = u32::try_from(current_words.len() / STATE_WORDS)
                    .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
                if segment_count == 0 {
                    continue;
                }
                ranges.clear();
                ranges.push((0_u32, segment_count));
                while let Some((begin, end)) = ranges.pop() {
                    if should_cancel() {
                        return Ok(WebGpuGeometryExactCoverOutcome::Cancelled);
                    }
                    if end - begin > max_dispatch_parent_count {
                        let middle = begin + max_dispatch_parent_count;
                        ranges.push((middle, end));
                        ranges.push((begin, middle));
                        continue;
                    }
                    let begin_word = begin as usize * STATE_WORDS;
                    let end_word = end as usize * STATE_WORDS;
                    let output_capacity = u32::try_from(
                        u64::from(end - begin)
                            .saturating_mul(u64::from(operation_count))
                            .min(u64::from(batch.frontier_capacity()))
                            .min(u64::from(device_dispatch_capacity)),
                    )
                    .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?
                    .max(1);
                    let global_parent_begin = parent_index_base
                        .checked_add(begin)
                        .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
                    let params =
                        params_words(batch, end - begin, global_parent_begin, output_capacity)?;
                    let mut dispatch_mismatch = None;
                    let mut dispatch_reduce_error = None;
                    let mut dispatch_reduce_ns = 0_u64;
                    let mut payload_consumed = false;
                    let layer = execute_layer(
                        &session.context.device,
                        &session.context.queue,
                        &session.context.pipeline,
                        &static_buffers.skeleton_mask_buffer,
                        &static_buffers.skeleton_piece_buffer,
                        &static_buffers.support_offset_buffer,
                        &static_buffers.support_operation_buffer,
                        &static_buffers.constraint_buffer,
                        &session.context.zero_counter_buffer,
                        &current_words[begin_word..end_word],
                        &params,
                        output_capacity,
                        &mut session.layer_scratch,
                        &mut |state_words, trace_words| {
                            payload_consumed = true;
                            if dispatch_mismatch.is_none() {
                                dispatch_mismatch = cpu_reference_sampler
                                    .confirm_dispatch(
                                        batch,
                                        &current_words[begin_word..end_word],
                                        global_parent_begin,
                                        state_words,
                                        trace_words,
                                    )
                                    .err();
                            }
                            let reduce_timer = WebGpuStageTimer::begin();
                            if dispatch_reduce_error.is_none() {
                                dispatch_reduce_error = reducer
                                    .extend_generated_words(state_words, trace_words)
                                    .err();
                            }
                            dispatch_reduce_ns =
                                dispatch_reduce_ns.saturating_add(reduce_timer.finish_ns());
                        },
                    )
                    .await?;
                    if !layer.overflow && !payload_consumed && dispatch_mismatch.is_none() {
                        dispatch_mismatch = cpu_reference_sampler
                            .confirm_dispatch(
                                batch,
                                &current_words[begin_word..end_word],
                                global_parent_begin,
                                &[],
                                &[],
                            )
                            .err();
                    }
                    if should_cancel() {
                        return Ok(WebGpuGeometryExactCoverOutcome::Cancelled);
                    }
                    timings.add_layer(layer.timing);
                    peak_gpu_bytes = peak_gpu_bytes.max(
                        static_buffers
                            .resident_bytes
                            .saturating_add(layer.gpu_bytes),
                    );
                    if layer.overflow {
                        if end - begin <= 1 {
                            return Ok(resource_incomplete(
                                layer.generated_count,
                                batch.frontier_capacity(),
                            ));
                        }
                        let middle = begin + (end - begin) / 2;
                        ranges.push((middle, end));
                        ranges.push((begin, middle));
                        continue;
                    }
                    if let Some(mismatch) = dispatch_mismatch {
                        return Ok(WebGpuGeometryExactCoverOutcome::RejectedTrustMismatch {
                            parent_index: mismatch.parent_index,
                            mismatch_kind: mismatch.kind,
                        });
                    }
                    timings.add_exact_host_reduce(dispatch_reduce_ns);
                    peak_host_reduce_bytes = peak_host_reduce_bytes
                        .max(retained_input_bytes.saturating_add(reducer.peak_host_bytes()));
                    if let Some(error) = dispatch_reduce_error {
                        return Ok(reduce_failure_outcome(
                            error,
                            reducer.state_count(),
                            batch.frontier_capacity(),
                        ));
                    }
                }
                parent_index_base = parent_index_base
                    .checked_add(segment_count)
                    .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
            }
            peak_host_reduce_bytes = peak_host_reduce_bytes
                .max(retained_input_bytes.saturating_add(reducer.peak_host_bytes()));
            let finish_reduce_timer = WebGpuStageTimer::begin();
            let reduced = reducer.finish();
            timings.add_exact_host_reduce(finish_reduce_timer.finish_ns());
            let ReducedFrontier {
                state_segments,
                trace_layer,
            } = match reduced {
                Ok(frontier) => frontier,
                Err(error) => {
                    return Ok(reduce_failure_outcome(
                        error,
                        current_count as usize,
                        batch.frontier_capacity(),
                    ));
                }
            };
            let next_frontier_bytes = state_segments_resident_bytes(&state_segments)
                .saturating_add(trace_layer.resident_bytes());
            peak_host_reduce_bytes = peak_host_reduce_bytes
                .max(retained_input_bytes.saturating_add(next_frontier_bytes));
            current_segments = state_segments;
            trace_layers.push(trace_layer);
            let retained_host_bytes = current_segments
                .iter()
                .map(|segment| {
                    segment
                        .capacity()
                        .saturating_mul(std::mem::size_of::<u32>())
                })
                .fold(0usize, usize::saturating_add)
                .saturating_add(trace_layers.iter().fold(0_usize, |total, layer| {
                    total.saturating_add(layer.resident_bytes())
                }));
            peak_host_reduce_bytes = peak_host_reduce_bytes.max(retained_host_bytes);
        }

        if should_cancel() {
            return Ok(WebGpuGeometryExactCoverOutcome::Cancelled);
        }

        let final_count = current_segments
            .iter()
            .map(|segment| segment.len() / STATE_WORDS)
            .sum();
        let trust_state = if cpu_reference_sampler.is_trusted() {
            WebGpuPackingTrustState::TrustedCpuSampleConfirmed
        } else {
            WebGpuPackingTrustState::NeedsCpuConfirm
        };
        Ok(WebGpuGeometryExactCoverOutcome::Connected(
            WebGpuGeometryExactCoverConnected {
                solution_graph: WebGpuGeometrySolutionGraph::new(
                    batches,
                    trace_layers,
                    final_count,
                ),
                shader_version: session.context.shader_version,
                shader_hash: session.context.shader_hash.clone(),
                peak_gpu_bytes,
                peak_host_reduce_bytes,
                timings,
                trust_state,
                cpu_confirmed_dispatches: cpu_reference_sampler.confirmed_dispatches(),
                cpu_confirmed_parents: cpu_reference_sampler.confirmed_parents(),
            },
        ))
    }
}

fn state_segments_resident_bytes(segments: &[Vec<u32>]) -> usize {
    segments.iter().fold(
        segments
            .len()
            .saturating_mul(std::mem::size_of::<Vec<u32>>()),
        |total, segment| {
            total.saturating_add(
                segment
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u32>()),
            )
        },
    )
}

#[cfg(not(target_family = "wasm"))]
fn device_context_pool() -> &'static Mutex<Vec<SharedWebGpuDeviceContext>> {
    static CONTEXTS: OnceLock<Mutex<Vec<SharedWebGpuDeviceContext>>> = OnceLock::new();
    CONTEXTS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(target_family = "wasm")]
std::thread_local! {
    static DEVICE_CONTEXT_POOL: RefCell<Vec<SharedWebGpuDeviceContext>> = const { RefCell::new(Vec::new()) };
    static SESSION_POOL: RefCell<Vec<WebGpuGeometryExactCoverSession>> = const { RefCell::new(Vec::new()) };
}

#[cfg(not(target_family = "wasm"))]
fn cached_device_context(selection: WebGpuAdapterSelection) -> Option<SharedWebGpuDeviceContext> {
    device_context_pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|context| device_context_matches(context, selection))
        .cloned()
}

#[cfg(target_family = "wasm")]
fn cached_device_context(selection: WebGpuAdapterSelection) -> Option<SharedWebGpuDeviceContext> {
    DEVICE_CONTEXT_POOL.with(|contexts| {
        contexts
            .borrow()
            .iter()
            .find(|context| device_context_matches(context, selection))
            .cloned()
    })
}

#[cfg(not(target_family = "wasm"))]
fn cache_device_context(context: SharedWebGpuDeviceContext) -> (SharedWebGpuDeviceContext, bool) {
    let mut contexts = device_context_pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache_device_context_in(&mut contexts, context)
}

#[cfg(target_family = "wasm")]
fn cache_device_context(context: SharedWebGpuDeviceContext) -> (SharedWebGpuDeviceContext, bool) {
    DEVICE_CONTEXT_POOL
        .with(|contexts| cache_device_context_in(&mut contexts.borrow_mut(), context))
}

fn cache_device_context_in(
    contexts: &mut Vec<SharedWebGpuDeviceContext>,
    context: SharedWebGpuDeviceContext,
) -> (SharedWebGpuDeviceContext, bool) {
    if let Some(existing) = contexts
        .iter()
        .find(|existing| existing.adapter.index() == context.adapter.index())
    {
        if context.auto_selected.load(Ordering::Relaxed) {
            existing.auto_selected.store(true, Ordering::Relaxed);
        }
        return (SharedWebGpuDeviceContext::clone(existing), true);
    }
    const MAX_CACHED_DEVICE_CONTEXTS: usize = 4;
    if contexts.len() == MAX_CACHED_DEVICE_CONTEXTS {
        contexts.remove(0);
    }
    contexts.push(SharedWebGpuDeviceContext::clone(&context));
    (context, false)
}

fn device_context_matches(
    context: &WebGpuDeviceContext,
    selection: WebGpuAdapterSelection,
) -> bool {
    match selection {
        WebGpuAdapterSelection::Auto => context.auto_selected.load(Ordering::Relaxed),
        WebGpuAdapterSelection::Index(index) => context.adapter.index() == index,
    }
}

fn session_from_context(
    selection: WebGpuAdapterSelection,
    context: SharedWebGpuDeviceContext,
    reused: bool,
) -> WebGpuGeometryExactCoverSession {
    WebGpuGeometryExactCoverSession {
        selection,
        reused,
        context,
        static_buffers: None,
        layer_scratch: None,
    }
}

#[cfg(not(target_family = "wasm"))]
fn session_pool() -> &'static Mutex<Vec<WebGpuGeometryExactCoverSession>> {
    static SESSIONS: OnceLock<Mutex<Vec<WebGpuGeometryExactCoverSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(not(target_family = "wasm"))]
fn cache_session(session: WebGpuGeometryExactCoverSession) {
    let mut sessions = session_pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache_session_in(&mut sessions, session);
}

#[cfg(target_family = "wasm")]
fn cache_session(session: WebGpuGeometryExactCoverSession) {
    SESSION_POOL.with(|sessions| cache_session_in(&mut sessions.borrow_mut(), session));
}

fn cache_session_in(
    sessions: &mut Vec<WebGpuGeometryExactCoverSession>,
    session: WebGpuGeometryExactCoverSession,
) {
    if let Some(index) = sessions
        .iter()
        .position(|existing| existing.selection == session.selection)
    {
        sessions.swap_remove(index);
    }
    const MAX_CACHED_SESSIONS: usize = 4;
    if sessions.len() == MAX_CACHED_SESSIONS {
        sessions.remove(0);
    }
    sessions.push(session);
}

#[cfg(not(target_family = "wasm"))]
fn take_cached_session(
    selection: WebGpuAdapterSelection,
) -> Option<WebGpuGeometryExactCoverSession> {
    let mut sessions = session_pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    sessions
        .iter()
        .position(|session| session.selection == selection)
        .map(|index| {
            let mut session = sessions.swap_remove(index);
            session.reused = true;
            session
        })
}

#[cfg(target_family = "wasm")]
fn take_cached_session(
    selection: WebGpuAdapterSelection,
) -> Option<WebGpuGeometryExactCoverSession> {
    SESSION_POOL.with(|sessions| take_cached_session_from(&mut sessions.borrow_mut(), selection))
}

#[cfg(target_family = "wasm")]
fn take_cached_session_from(
    sessions: &mut Vec<WebGpuGeometryExactCoverSession>,
    selection: WebGpuAdapterSelection,
) -> Option<WebGpuGeometryExactCoverSession> {
    sessions
        .iter()
        .position(|session| session.selection == selection)
        .map(|index| {
            let mut session = sessions.swap_remove(index);
            session.reused = true;
            session
        })
}
