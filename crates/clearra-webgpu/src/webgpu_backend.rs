use std::fmt;

use futures_channel::oneshot;
use wgpu::util::DeviceExt;

use crate::{
    adapter_selection::{select_adapter, WebGpuAdapterSelection},
    WebGpuShaderContract,
};

const WORKGROUP_SIZE: u32 = 64;
const PARAM_WORD_COUNT: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuBitsetBatch {
    row_count: u32,
    word_count: u32,
    words: Vec<u32>,
}

impl WebGpuBitsetBatch {
    pub fn new(rows: &[Vec<u32>]) -> Result<Self, WebGpuBatchInputError> {
        let first = rows.first().ok_or(WebGpuBatchInputError::EmptyBatch)?;
        if first.is_empty() {
            return Err(WebGpuBatchInputError::EmptyRow);
        }
        if let Some((row_index, actual)) = rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| (row.len() != first.len()).then_some((index, row.len())))
        {
            return Err(WebGpuBatchInputError::RowLengthMismatch {
                row_index,
                expected: first.len(),
                actual,
            });
        }

        let row_count =
            u32::try_from(rows.len()).map_err(|_| WebGpuBatchInputError::DimensionOverflow)?;
        let word_count =
            u32::try_from(first.len()).map_err(|_| WebGpuBatchInputError::DimensionOverflow)?;
        let words = rows.iter().flat_map(|row| row.iter().copied()).collect();
        Ok(Self {
            row_count,
            word_count,
            words,
        })
    }

    pub fn row_count(&self) -> u32 {
        self.row_count
    }

    pub fn word_count(&self) -> u32 {
        self.word_count
    }

    fn expected_union(&self) -> Vec<u32> {
        let mut union = vec![0; self.word_count as usize];
        for row in self.words.chunks_exact(self.word_count as usize) {
            for (target, word) in union.iter_mut().zip(row) {
                *target |= *word;
            }
        }
        union
    }

    fn byte_len(&self) -> Result<u64, WebGpuBatchInputError> {
        u64::try_from(self.words.len())
            .ok()
            .and_then(|len| len.checked_mul(std::mem::size_of::<u32>() as u64))
            .ok_or(WebGpuBatchInputError::DimensionOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuTrustState {
    DeterministicReferenceMatched,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebGpuLimits {
    max_storage_buffer_binding_size: u64,
    max_compute_workgroup_storage_size: u32,
    max_compute_invocations_per_workgroup: u32,
}

impl WebGpuLimits {
    fn from_wgpu(limits: &wgpu::Limits) -> Self {
        Self {
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_compute_workgroup_storage_size: limits.max_compute_workgroup_storage_size,
            max_compute_invocations_per_workgroup: limits.max_compute_invocations_per_workgroup,
        }
    }

    pub fn max_storage_buffer_binding_size(self) -> u64 {
        self.max_storage_buffer_binding_size
    }

    pub fn max_compute_workgroup_storage_size(self) -> u32 {
        self.max_compute_workgroup_storage_size
    }

    pub fn max_compute_invocations_per_workgroup(self) -> u32 {
        self.max_compute_invocations_per_workgroup
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuConnectedResult {
    union_words: Vec<u32>,
    adapter_label_or_redacted: String,
    shader_version: &'static str,
    shader_hash: String,
    limits: WebGpuLimits,
    trust_state: WebGpuTrustState,
    cpu_confirmed: bool,
}

impl WebGpuConnectedResult {
    pub fn union_words(&self) -> &[u32] {
        &self.union_words
    }

    pub fn adapter_label_or_redacted(&self) -> &str {
        &self.adapter_label_or_redacted
    }

    pub fn shader_version(&self) -> &'static str {
        self.shader_version
    }

    pub fn shader_hash(&self) -> &str {
        &self.shader_hash
    }

    pub fn limits(&self) -> WebGpuLimits {
        self.limits
    }

    pub fn trust_state(&self) -> WebGpuTrustState {
        self.trust_state
    }

    pub fn cpu_confirmed(&self) -> bool {
        self.cpu_confirmed
    }

    pub fn can_claim_exact(&self) -> bool {
        self.cpu_confirmed && self.trust_state == WebGpuTrustState::DeterministicReferenceMatched
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuUnavailableResult {
    reason: String,
}

impl WebGpuUnavailableResult {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuRejectedMismatch {
    expected_digest: String,
    actual_digest: String,
}

impl WebGpuRejectedMismatch {
    pub fn expected_digest(&self) -> &str {
        &self.expected_digest
    }

    pub fn actual_digest(&self) -> &str {
        &self.actual_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebGpuBatchOutcome {
    Connected(WebGpuConnectedResult),
    Unavailable(WebGpuUnavailableResult),
    RejectedMismatch(WebGpuRejectedMismatch),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebGpuBackend;

impl WebGpuBackend {
    pub async fn run_bitset_union(
        batch: &WebGpuBitsetBatch,
    ) -> Result<WebGpuBatchOutcome, WebGpuBatchInputError> {
        Self::run_bitset_union_on(batch, WebGpuAdapterSelection::Auto).await
    }

    pub async fn run_bitset_union_on(
        batch: &WebGpuBitsetBatch,
        selection: WebGpuAdapterSelection,
    ) -> Result<WebGpuBatchOutcome, WebGpuBatchInputError> {
        let contract = WebGpuShaderContract::embedded_reviewed();
        let adapter = match select_adapter(selection).await {
            Ok(selected) => selected.adapter,
            Err(error) => {
                return Ok(WebGpuBatchOutcome::Unavailable(
                    WebGpuUnavailableResult::new(format!("webgpu_adapter_unavailable: {error}")),
                ));
            }
        };

        let adapter_limits = adapter.limits();
        let input_bytes = batch.byte_len()?;
        let output_bytes = u64::from(batch.word_count) * std::mem::size_of::<u32>() as u64;
        let max_storage = u64::from(adapter_limits.max_storage_buffer_binding_size);
        if input_bytes > max_storage || output_bytes > max_storage {
            return Ok(WebGpuBatchOutcome::Unavailable(
                WebGpuUnavailableResult::new("webgpu_storage_buffer_limit_exceeded"),
            ));
        }

        let (device, queue) = match adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("clearra-webgpu-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
        {
            Ok(pair) => pair,
            Err(error) => {
                return Ok(WebGpuBatchOutcome::Unavailable(
                    WebGpuUnavailableResult::new(format!("webgpu_device_unavailable: {error}")),
                ));
            }
        };

        let shader_error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clearra-pattern-bitset-union"),
            source: wgpu::ShaderSource::Wgsl(contract.shader_source().into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("clearra-pattern-bitset-union-pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = shader_error_scope.pop().await {
            return Ok(WebGpuBatchOutcome::Unavailable(
                WebGpuUnavailableResult::new(format!("webgpu_shader_compile_failed: {error}")),
            ));
        }

        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clearra-pattern-rows"),
            contents: bytemuck::cast_slice(&batch.words),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clearra-pattern-union"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params = [batch.word_count, batch.row_count, 0, 0];
        debug_assert_eq!(params.len(), PARAM_WORD_COUNT);
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clearra-pattern-union-params"),
            contents: bytemuck::cast_slice(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clearra-pattern-union-readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clearra-pattern-union-bind-group"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("clearra-pattern-union-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("clearra-pattern-union-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(batch.word_count.div_ceil(WORKGROUP_SIZE), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_bytes);
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (sender, receiver) = oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(error) = device.poll(wgpu::PollType::wait_indefinitely()) {
            return Ok(WebGpuBatchOutcome::Unavailable(
                WebGpuUnavailableResult::new(format!("webgpu_device_poll_failed: {error}")),
            ));
        }
        match receiver.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return Ok(WebGpuBatchOutcome::Unavailable(
                    WebGpuUnavailableResult::new(format!("webgpu_readback_failed: {error}")),
                ));
            }
            Err(_) => {
                return Ok(WebGpuBatchOutcome::Unavailable(
                    WebGpuUnavailableResult::new("webgpu_readback_callback_dropped"),
                ));
            }
        }

        let mapped = slice
            .get_mapped_range()
            .map_err(|_| WebGpuBatchInputError::ReadbackAlignment)?;
        let actual = bytemuck::cast_slice::<u8, u32>(&mapped).to_vec();
        drop(mapped);
        readback.unmap();

        let expected = batch.expected_union();
        if actual != expected {
            return Ok(WebGpuBatchOutcome::RejectedMismatch(
                WebGpuRejectedMismatch {
                    expected_digest: digest_words(&expected),
                    actual_digest: digest_words(&actual),
                },
            ));
        }

        Ok(WebGpuBatchOutcome::Connected(WebGpuConnectedResult {
            union_words: actual,
            adapter_label_or_redacted: "redacted".to_owned(),
            shader_version: contract.shader_version(),
            shader_hash: contract.shader_hash(),
            limits: WebGpuLimits::from_wgpu(&adapter_limits),
            trust_state: WebGpuTrustState::DeterministicReferenceMatched,
            cpu_confirmed: true,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebGpuBatchInputError {
    EmptyBatch,
    EmptyRow,
    RowLengthMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    DimensionOverflow,
    ReadbackAlignment,
}

impl fmt::Display for WebGpuBatchInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WebGpuBatchInputError {}

fn digest_words(words: &[u32]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytemuck::cast_slice::<u32, u8>(words) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}
