use futures_channel::oneshot;
use wgpu::util::DeviceExt;

use crate::{
    geometry_exact_cover_model::{
        WebGpuGeometryExactCoverBatch, WebGpuGeometryExactCoverInputError, COUNTER_WORDS,
        PARAM_WORDS, STATE_WORDS, TRACE_WORDS,
    },
    geometry_exact_cover_timing::{WebGpuLayerTiming, WebGpuStageTimer},
};

pub(crate) const WORKGROUP_SIZE: u32 = 64;

pub(crate) struct LayerScratch {
    frontier: wgpu::Buffer,
    next_frontier: wgpu::Buffer,
    next_trace: wgpu::Buffer,
    counters: wgpu::Buffer,
    params: wgpu::Buffer,
    counter_readback: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    frontier_bytes: u64,
    state_bytes: u64,
    trace_bytes: u64,
    readback_bytes: u64,
}

impl LayerScratch {
    // Buffer roles and sizes are independent inputs to this private GPU allocation boundary.
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &wgpu::Device,
        pipeline: &wgpu::ComputePipeline,
        skeleton_mask_buffer: &wgpu::Buffer,
        skeleton_piece_buffer: &wgpu::Buffer,
        support_offset_buffer: &wgpu::Buffer,
        support_operation_buffer: &wgpu::Buffer,
        constraint_buffer: &wgpu::Buffer,
        frontier_bytes: u64,
        state_bytes: u64,
        trace_bytes: u64,
        readback_bytes: u64,
    ) -> Self {
        // Scratch is intentionally not host-initialized. execute_layer writes
        // every range it later reads; only atomic counters need an explicit
        // zero before each dispatch.
        let frontier = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exact-cover-current-frontier-scratch"),
            size: frontier_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let next_frontier = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exact-cover-next-frontier-scratch"),
            size: state_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let next_trace = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exact-cover-next-family-edge-scratch"),
            size: trace_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let counters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exact-cover-output-counters-scratch"),
            size: (COUNTER_WORDS * size_of_u32()) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("geometry-exact-cover-params-scratch"),
            size: (PARAM_WORDS * size_of_u32()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let counter_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("geometry-exact-cover-counter-readback-scratch"),
            size: (COUNTER_WORDS * size_of_u32()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("geometry-exact-cover-readback-scratch"),
            size: readback_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry-exact-cover-bind-group"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                binding(0, &frontier),
                binding(1, skeleton_mask_buffer),
                binding(2, skeleton_piece_buffer),
                binding(3, support_offset_buffer),
                binding(4, support_operation_buffer),
                binding(5, &next_frontier),
                binding(6, &next_trace),
                binding(7, &counters),
                binding(8, &params),
                binding(9, constraint_buffer),
            ],
        });
        Self {
            frontier,
            next_frontier,
            next_trace,
            counters,
            params,
            counter_readback,
            readback,
            bind_group,
            frontier_bytes,
            state_bytes,
            trace_bytes,
            readback_bytes,
        }
    }

    fn accommodates(
        &self,
        frontier_bytes: u64,
        state_bytes: u64,
        trace_bytes: u64,
        readback_bytes: u64,
    ) -> bool {
        self.frontier_bytes >= frontier_bytes
            && self.state_bytes >= state_bytes
            && self.trace_bytes >= trace_bytes
            && self.readback_bytes >= readback_bytes
    }

    fn resident_bytes(&self) -> u64 {
        self.frontier_bytes
            .saturating_add(self.state_bytes)
            .saturating_add(self.trace_bytes)
            .saturating_add((COUNTER_WORDS * size_of_u32()) as u64)
            .saturating_add((PARAM_WORDS * size_of_u32()) as u64)
            .saturating_add((COUNTER_WORDS * size_of_u32()) as u64)
            .saturating_add(self.readback_bytes)
    }
}

pub(crate) struct LayerDispatchReport {
    pub(crate) generated_count: u32,
    pub(crate) overflow: bool,
    pub(crate) gpu_bytes: u64,
    pub(crate) timing: WebGpuLayerTiming,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_layer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    skeleton_mask_buffer: &wgpu::Buffer,
    skeleton_piece_buffer: &wgpu::Buffer,
    support_offset_buffer: &wgpu::Buffer,
    support_operation_buffer: &wgpu::Buffer,
    constraint_buffer: &wgpu::Buffer,
    zero_counter_buffer: &wgpu::Buffer,
    current_words: &[u32],
    params: &[u32; PARAM_WORDS],
    capacity: u32,
    scratch_slot: &mut Option<LayerScratch>,
    consume_payload: &mut impl FnMut(&[u32], &[u32]),
) -> Result<LayerDispatchReport, WebGpuGeometryExactCoverInputError> {
    let prepare_timer = WebGpuStageTimer::begin();
    let current_count = u32::try_from(current_words.len() / STATE_WORDS)
        .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
    let state_bytes = u64::from(capacity) * (STATE_WORDS * size_of_u32()) as u64;
    let trace_bytes = u64::from(capacity) * (TRACE_WORDS * size_of_u32()) as u64;
    let counter_bytes = (COUNTER_WORDS * size_of_u32()) as u64;
    let payload_capacity_bytes = state_bytes
        .checked_add(trace_bytes)
        .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
    let readback_bytes = payload_capacity_bytes;
    let frontier_bytes = byte_len(current_words)?;
    let needs_scratch = scratch_slot.as_ref().is_none_or(|scratch| {
        !scratch.accommodates(frontier_bytes, state_bytes, trace_bytes, readback_bytes)
    });
    if needs_scratch {
        *scratch_slot = Some(LayerScratch::new(
            device,
            pipeline,
            skeleton_mask_buffer,
            skeleton_piece_buffer,
            support_offset_buffer,
            support_operation_buffer,
            constraint_buffer,
            frontier_bytes,
            state_bytes,
            trace_bytes,
            readback_bytes,
        ));
    }
    let scratch = scratch_slot
        .as_ref()
        .ok_or(WebGpuGeometryExactCoverInputError::LayerScratch)?;
    queue.write_buffer(&scratch.frontier, 0, bytemuck::cast_slice(current_words));
    queue.write_buffer(&scratch.params, 0, bytemuck::cast_slice(params));
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("geometry-exact-cover-encoder"),
    });
    encoder.copy_buffer_to_buffer(zero_counter_buffer, 0, &scratch.counters, 0, counter_bytes);
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("geometry-exact-cover-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &scratch.bind_group, &[]);
        pass.dispatch_workgroups(current_count.div_ceil(WORKGROUP_SIZE), 1, 1);
    }
    encoder.copy_buffer_to_buffer(
        &scratch.counters,
        0,
        &scratch.counter_readback,
        0,
        counter_bytes,
    );
    queue.submit([encoder.finish()]);
    let host_prepare_submit_ns = prepare_timer.finish_ns();

    let counter_timer = WebGpuStageTimer::begin();
    map_readback(device, &scratch.counter_readback, counter_bytes).await?;
    let counter_mapped = scratch
        .counter_readback
        .slice(..counter_bytes)
        .get_mapped_range()
        .map_err(|_| WebGpuGeometryExactCoverInputError::ReadbackAlignment)?;
    let counter_words = bytemuck::cast_slice::<u8, u32>(&counter_mapped[..counter_bytes as usize]);
    let generated_count = counter_words[0];
    let overflow = counter_words[1] != 0 || generated_count > capacity;
    let dispatch_counter_wait_ns = counter_timer.finish_ns();
    let base_timing = WebGpuLayerTiming {
        host_prepare_submit_ns,
        dispatch_counter_wait_ns,
        payload_readback_ns: 0,
        generated_record_count: generated_count,
    };

    if overflow {
        drop(counter_mapped);
        scratch.counter_readback.unmap();
        return Ok(LayerDispatchReport {
            generated_count,
            overflow: true,
            gpu_bytes: scratch.resident_bytes(),
            timing: base_timing,
        });
    }

    let retained_count = generated_count.min(capacity) as usize;
    if retained_count == 0 {
        drop(counter_mapped);
        scratch.counter_readback.unmap();
        return Ok(LayerDispatchReport {
            generated_count,
            overflow: false,
            gpu_bytes: scratch.resident_bytes(),
            timing: base_timing,
        });
    }
    let retained_state_bytes = (retained_count * STATE_WORDS * size_of_u32()) as u64;
    let retained_trace_bytes = (retained_count * TRACE_WORDS * size_of_u32()) as u64;
    let retained_readback_bytes = retained_state_bytes
        .checked_add(retained_trace_bytes)
        .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)?;

    drop(counter_mapped);
    scratch.counter_readback.unmap();
    let payload_timer = WebGpuStageTimer::begin();
    let mut readback_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("geometry-exact-cover-readback-encoder"),
    });
    readback_encoder.copy_buffer_to_buffer(
        &scratch.next_frontier,
        0,
        &scratch.readback,
        0,
        retained_state_bytes,
    );
    readback_encoder.copy_buffer_to_buffer(
        &scratch.next_trace,
        0,
        &scratch.readback,
        retained_state_bytes,
        retained_trace_bytes,
    );
    queue.submit([readback_encoder.finish()]);

    map_readback(device, &scratch.readback, retained_readback_bytes).await?;
    let mapped = scratch
        .readback
        .slice(..retained_readback_bytes)
        .get_mapped_range()
        .map_err(|_| WebGpuGeometryExactCoverInputError::ReadbackAlignment)?;
    let state_word_count = retained_count * STATE_WORDS;
    let state_words = bytemuck::cast_slice::<u8, u32>(&mapped[..state_word_count * size_of_u32()]);
    let trace_offset = usize::try_from(retained_state_bytes)
        .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?;
    let trace_words = bytemuck::cast_slice::<u8, u32>(
        &mapped[trace_offset..trace_offset + retained_count * TRACE_WORDS * size_of_u32()],
    );
    let payload_readback_ns = payload_timer.finish_ns();
    consume_payload(state_words, trace_words);
    drop(mapped);
    scratch.readback.unmap();

    Ok(LayerDispatchReport {
        generated_count,
        overflow,
        gpu_bytes: scratch.resident_bytes(),
        timing: WebGpuLayerTiming {
            payload_readback_ns,
            ..base_timing
        },
    })
}

async fn map_readback(
    _device: &wgpu::Device,
    readback: &wgpu::Buffer,
    readback_bytes: u64,
) -> Result<(), WebGpuGeometryExactCoverInputError> {
    let slice = readback.slice(..readback_bytes);
    let (sender, receiver) = oneshot::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    #[cfg(not(target_arch = "wasm32"))]
    _device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|_| WebGpuGeometryExactCoverInputError::DevicePoll)?;
    match receiver.await {
        Ok(Ok(())) => Ok(()),
        _ => Err(WebGpuGeometryExactCoverInputError::ReadbackFailed),
    }
}

pub(crate) fn params_words(
    batch: &WebGpuGeometryExactCoverBatch,
    current_count: u32,
    parent_index_base: u32,
    output_capacity: u32,
) -> Result<[u32; PARAM_WORDS], WebGpuGeometryExactCoverInputError> {
    Ok([
        current_count,
        u32::try_from(batch.skeleton_cell_masks().len())
            .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?,
        output_capacity,
        batch.cell_count(),
        u32::from(batch.width()),
        0,
        u32::from(batch.target_depth()),
        0,
        batch.required_fill_mask() as u32,
        (batch.required_fill_mask() >> 32) as u32,
        batch.goal_mask() as u32,
        (batch.goal_mask() >> 32) as u32,
        batch.forbidden_mask() as u32,
        (batch.forbidden_mask() >> 32) as u32,
        parent_index_base,
        0,
    ])
}

pub(crate) fn storage_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &'static str,
    values: &[T],
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(values),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

pub(crate) fn uniform_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &'static str,
    values: &[T],
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(values),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(crate) fn storage_buffer_copy_source<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &'static str,
    values: &[T],
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(values),
        usage: wgpu::BufferUsages::COPY_SRC,
    })
}

fn binding(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

pub(crate) fn byte_len<T>(values: &[T]) -> Result<u64, WebGpuGeometryExactCoverInputError> {
    u64::try_from(values.len())
        .ok()
        .and_then(|len| len.checked_mul(std::mem::size_of::<T>() as u64))
        .ok_or(WebGpuGeometryExactCoverInputError::DimensionOverflow)
}

pub(crate) const fn size_of_u32() -> usize {
    std::mem::size_of::<u32>()
}
