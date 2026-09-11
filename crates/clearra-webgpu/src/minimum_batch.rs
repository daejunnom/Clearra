//! Feature-gated minimum-cover experiment using the existing adapter and wgpu path.
//! Results are propagation proposals, never product UNSAT / canonical receipts.
use clearra_coverage::cover::experimental_batch::{
    BatchCoverMatrix, CoverPropagationRound, PackedCoverStates,
};
use futures_channel::oneshot;
use wgpu::util::DeviceExt;

use crate::adapter_selection::{WebGpuAdapterSelection, WebGpuAdapterSummary};
use crate::geometry_exact_cover_backend::WebGpuGeometryExactCoverBackend;

pub const MINIMUM_BATCH_SHADER: &str = include_str!("embedded_minimum_batch.wgsl");

pub struct WebGpuMinimumBatchSession {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    matrix: wgpu::Buffer,
    candidate_count: usize,
    constraint_count: usize,
    adapter: WebGpuAdapterSummary,
    scratch: Option<BatchScratch>,
}

struct BatchScratch {
    state_count: usize,
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    // Retain uniforms together with the bind groups that reference them.
    params: Vec<wgpu::Buffer>,
    groups: Vec<wgpu::BindGroup>,
    output_bytes: u64,
}

impl WebGpuMinimumBatchSession {
    pub async fn connect(
        matrix: &BatchCoverMatrix,
        selection: WebGpuAdapterSelection,
    ) -> Result<Self, String> {
        let words = matrix.device_words();
        let bytes = (words.len() as u64)
            .checked_mul(4)
            .ok_or("matrix overflow")?;
        if bytes > wgpu::Limits::default().max_storage_buffer_binding_size {
            return Err("matrix exceeds storage limit".into());
        }
        let (device, queue, adapter) =
            WebGpuGeometryExactCoverBackend::minimum_device_handles(selection)
                .await
                .map_err(|error| format!("device: {}", error.reason()))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clearra-minimum-batch-ab"),
            source: wgpu::ShaderSource::Wgsl(MINIMUM_BATCH_SHADER.into()),
        });
        // Match the existing geometry backend: browser compilation diagnostics
        // remain explicit, while native-only scopes avoid wgpu's web error
        // conversion panic after an otherwise valid pipeline compilation.
        let errors = shader
            .get_compilation_info()
            .await
            .messages
            .into_iter()
            .filter(|message| message.message_type == wgpu::CompilationMessageType::Error)
            .map(|message| message.message)
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(format!("shader: {}", errors.join(" | ")));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("clearra-minimum-batch-ab"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = scope.pop().await {
            return Err(format!("pipeline: {error}"));
        }
        let matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clearra-minimum-immutable-matrix"),
            contents: bytemuck::cast_slice(&words),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            matrix: matrix_buffer,
            candidate_count: matrix.candidate_count(),
            constraint_count: matrix.constraint_count(),
            adapter,
            scratch: None,
        })
    }

    pub fn adapter(&self) -> &WebGpuAdapterSummary {
        &self.adapter
    }

    fn prepare(&mut self, states: &PackedCoverStates) -> Result<(), String> {
        if states.candidate_count() != self.candidate_count {
            return Err("matrix/state identity mismatch".into());
        }
        if self
            .scratch
            .as_ref()
            .is_some_and(|s| s.state_count == states.state_count())
        {
            return Ok(());
        }
        let span = self.candidate_count * states.groups();
        let input_bytes = ((2 * span + states.state_count()) as u64)
            .checked_mul(4)
            .ok_or("input overflow")?;
        let output_bytes = ((2 * span + states.groups()) as u64)
            .checked_mul(4)
            .ok_or("output overflow")?;
        if input_bytes.max(output_bytes) > self.device.limits().max_storage_buffer_binding_size {
            return Err("batch exceeds storage limit".into());
        }
        let input = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("minimum-batch-input"),
            size: input_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("minimum-batch-output"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("minimum-batch-readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut params = Vec::new();
        let mut groups = Vec::new();
        for mode in 0..3_u32 {
            let words = [
                self.candidate_count as u32,
                states.state_count() as u32,
                states.groups() as u32,
                self.constraint_count as u32,
                mode,
                0,
                0,
                0,
            ];
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("minimum-batch-params"),
                    contents: bytemuck::cast_slice(&words),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
            groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("minimum-batch-binding"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.matrix.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: input.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: output.as_entire_binding(),
                    },
                ],
            }));
            params.push(uniform);
        }
        self.scratch = Some(BatchScratch {
            state_count: states.state_count(),
            input,
            output,
            readback,
            params,
            groups,
            output_bytes,
        });
        Ok(())
    }

    /// Persistent device + immutable matrix + reusable scratch; upload and readback are included.
    pub async fn propagate_round(
        &mut self,
        states: &PackedCoverStates,
        bit_sliced: bool,
    ) -> Result<CoverPropagationRound, String> {
        self.propagate_round_with_zero_atomic_guard(states, bit_sliced, false)
            .await
    }

    /// Diagnostic switch only; each arm uses the same shader and state snapshot.
    pub async fn propagate_round_with_zero_atomic_guard(
        &mut self,
        states: &PackedCoverStates,
        bit_sliced: bool,
        skip_zero_atomic: bool,
    ) -> Result<CoverPropagationRound, String> {
        self.propagate_round_with_layout(states, bit_sliced, skip_zero_atomic, false)
            .await
    }

    /// The group-major layout puts adjacent GPU lanes on contiguous state words.
    pub async fn propagate_round_with_layout(
        &mut self,
        states: &PackedCoverStates,
        bit_sliced: bool,
        skip_zero_atomic: bool,
        group_major_lanes: bool,
    ) -> Result<CoverPropagationRound, String> {
        let dimensions = if bit_sliced && group_major_lanes {
            (states.groups().div_ceil(64), self.constraint_count)
        } else if bit_sliced {
            (self.constraint_count.div_ceil(64), states.groups())
        } else {
            (states.state_count().div_ceil(64), 1)
        };
        let maximum = self.device.limits().max_compute_workgroups_per_dimension as usize;
        if dimensions.0 > maximum
            || dimensions.1 > maximum
            || states.state_count().div_ceil(64) > maximum
        {
            return Err("batch exceeds dispatch dimensions".into());
        }
        self.prepare(states)?;
        let scratch = self.scratch.as_ref().ok_or("missing batch scratch")?;
        let guard = [u32::from(skip_zero_atomic), u32::from(group_major_lanes)];
        // The first five words include `mode`; experimental flags start at
        // reserved0 (byte 20). Keep the dispatch mode immutable.
        self.queue.write_buffer(
            &scratch.params[usize::from(bit_sliced)],
            20,
            bytemuck::cast_slice(&guard),
        );
        self.queue
            .write_buffer(&scratch.params[2], 20, bytemuck::cast_slice(&guard));
        let input_words = states.device_words();
        self.queue
            .write_buffer(&scratch.input, 0, bytemuck::cast_slice(&input_words));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("minimum-batch-round"),
            });
        let assignment_bytes = 2 * self.candidate_count as u64 * states.groups() as u64 * 4;
        encoder.copy_buffer_to_buffer(&scratch.input, 0, &scratch.output, 0, assignment_bytes);
        encoder.clear_buffer(
            &scratch.output,
            assignment_bytes,
            Some(states.groups() as u64 * 4),
        );
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("minimum-constraints"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &scratch.groups[usize::from(bit_sliced)], &[]);
            pass.dispatch_workgroups(dimensions.0 as u32, dimensions.1 as u32, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("minimum-round-seal"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &scratch.groups[2], &[]);
            pass.dispatch_workgroups((states.state_count() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &scratch.output,
            0,
            &scratch.readback,
            0,
            scratch.output_bytes,
        );
        self.queue.submit([encoder.finish()]);
        let slice = scratch.readback.slice(..);
        let (sender, receiver) = oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        #[cfg(not(target_arch = "wasm32"))]
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("poll: {e}"))?;
        receiver
            .await
            .map_err(|e| format!("readback callback: {e}"))?
            .map_err(|e| format!("readback: {e}"))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| format!("mapped range: {e}"))?;
        let words = bytemuck::cast_slice::<u8, u32>(&mapped).to_vec();
        drop(mapped);
        scratch.readback.unmap();
        Ok(CoverPropagationRound { words })
    }
}
