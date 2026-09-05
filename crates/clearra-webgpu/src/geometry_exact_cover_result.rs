use std::{fmt, mem::size_of, sync::Arc};

use crate::{
    geometry_exact_cover_model::{
        WebGpuGeometryExactCoverBatch, WebGpuGeometryExactCoverIncomplete,
        WebGpuGeometryExactCoverOutcome, MAX_PACKING_OPERATIONS,
    },
    geometry_exact_cover_reduce::{FrontierReduceError, TraceLayer, TraceRecord},
};

pub struct WebGpuGeometrySolutionGraph {
    batches: Box<[WebGpuGeometryExactCoverBatch]>,
    layers: Box<[TraceLayer]>,
    final_state_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebGpuGeometryCandidatePath {
    batch_index: u32,
    operation_indices: [u32; MAX_PACKING_OPERATIONS],
    operation_count: u8,
}

impl WebGpuGeometryCandidatePath {
    pub const fn batch_index(&self) -> u32 {
        self.batch_index
    }

    pub fn operation_indices(&self) -> &[u32] {
        &self.operation_indices[..usize::from(self.operation_count)]
    }
}

#[derive(Debug)]
struct PathCursorFrame {
    layer_index: usize,
    edges: Vec<TraceRecord>,
    next_edge_index: usize,
}

#[derive(Debug)]
pub struct WebGpuGeometryPathCursor {
    graph: Arc<WebGpuGeometrySolutionGraph>,
    final_state_index: usize,
    frames: Vec<PathCursorFrame>,
    reverse_path: [u32; MAX_PACKING_OPERATIONS],
    invalid: bool,
}

impl WebGpuGeometrySolutionGraph {
    pub(crate) fn new(
        batches: &[WebGpuGeometryExactCoverBatch],
        layers: Vec<TraceLayer>,
        final_state_count: usize,
    ) -> Self {
        Self {
            batches: batches.to_vec().into_boxed_slice(),
            layers: layers.into_boxed_slice(),
            final_state_count,
        }
    }

    pub const fn final_state_count(&self) -> usize {
        self.final_state_count
    }

    pub fn peak_frontier_state_count(&self) -> usize {
        self.layers
            .iter()
            .map(TraceLayer::state_count)
            .max()
            .unwrap_or(0)
    }

    pub fn resident_bytes(&self) -> usize {
        self.batches
            .len()
            .saturating_mul(size_of::<WebGpuGeometryExactCoverBatch>())
            .saturating_add(
                self.layers
                    .iter()
                    .map(TraceLayer::resident_bytes)
                    .fold(0usize, usize::saturating_add),
            )
    }

    pub fn path_cursor(self: &Arc<Self>) -> WebGpuGeometryPathCursor {
        WebGpuGeometryPathCursor {
            graph: Arc::clone(self),
            final_state_index: 0,
            frames: Vec::with_capacity(self.layers.len()),
            reverse_path: [0; MAX_PACKING_OPERATIONS],
            invalid: false,
        }
    }

    pub fn stream_partition_paths<E>(
        &self,
        partition_index: usize,
        partition_count: usize,
        sink: &mut impl FnMut(&[u32]) -> Result<(), E>,
    ) -> Result<u64, WebGpuGeometryPathStreamError<E>> {
        if partition_count == 0 || partition_index >= partition_count {
            return Err(WebGpuGeometryPathStreamError::InvalidGraph {
                final_state_index: 0,
            });
        }
        let mut emitted = 0u64;
        let mut reverse_path = [0u32; MAX_PACKING_OPERATIONS];
        for final_state_index in (partition_index..self.final_state_count).step_by(partition_count)
        {
            if !stream_candidate_paths(
                &self.batches,
                &self.layers,
                self.layers.len().checked_sub(1),
                final_state_index,
                &mut reverse_path,
                0,
                sink,
                &mut emitted,
            )? {
                return Err(WebGpuGeometryPathStreamError::InvalidGraph { final_state_index });
            }
        }
        Ok(emitted)
    }
}

impl WebGpuGeometryPathCursor {
    pub fn next_path(&mut self) -> Result<Option<WebGpuGeometryCandidatePath>, usize> {
        if self.invalid {
            return Err(self.final_state_index.saturating_sub(1));
        }
        loop {
            if self.frames.is_empty() {
                if self.final_state_index >= self.graph.final_state_count {
                    return Ok(None);
                }
                let state_index = self.final_state_index;
                self.final_state_index += 1;
                let Some(layer_index) = self.graph.layers.len().checked_sub(1) else {
                    self.invalid = true;
                    return Err(state_index);
                };
                if !self.push_frame(layer_index, state_index) {
                    self.invalid = true;
                    return Err(state_index);
                }
            }

            let depth = self.frames.len() - 1;
            let (layer_index, edge) = {
                let Some(frame) = self.frames.last_mut() else {
                    continue;
                };
                let layer_index = frame.layer_index;
                let Some(edge) = frame.edges.get(frame.next_edge_index).copied() else {
                    self.frames.pop();
                    continue;
                };
                frame.next_edge_index += 1;
                (layer_index, edge)
            };
            let Some(operation_batch) = self.graph.batches.first() else {
                self.invalid = true;
                return Err(self.final_state_index.saturating_sub(1));
            };
            if edge.operation_index as usize >= operation_batch.skeleton_cell_masks().len()
                || depth >= self.reverse_path.len()
            {
                self.invalid = true;
                return Err(self.final_state_index.saturating_sub(1));
            }
            self.reverse_path[depth] = edge.operation_index;
            if layer_index != 0 {
                if !self.push_frame(layer_index - 1, edge.parent_index as usize) {
                    self.invalid = true;
                    return Err(self.final_state_index.saturating_sub(1));
                }
                continue;
            }

            let batch_index = edge.parent_index as usize;
            let Some(batch) = self.graph.batches.get(batch_index) else {
                self.invalid = true;
                return Err(self.final_state_index.saturating_sub(1));
            };
            let operation_count = depth + 1;
            let mut canonical = [0u32; MAX_PACKING_OPERATIONS];
            canonical[..operation_count].copy_from_slice(&self.reverse_path[..operation_count]);
            canonical[..operation_count].sort_unstable();
            if !validate_candidate(batch, &canonical[..operation_count]) {
                self.invalid = true;
                return Err(self.final_state_index.saturating_sub(1));
            }
            return Ok(Some(WebGpuGeometryCandidatePath {
                batch_index: edge.parent_index,
                operation_indices: canonical,
                operation_count: operation_count as u8,
            }));
        }
    }

    fn push_frame(&mut self, layer_index: usize, state_index: usize) -> bool {
        let Some(layer) = self.graph.layers.get(layer_index) else {
            return false;
        };
        let Some(edges) = layer.incoming_edges(state_index) else {
            return false;
        };
        let edges = edges.collect::<Vec<_>>();
        if edges.is_empty() {
            return false;
        }
        self.frames.push(PathCursorFrame {
            layer_index,
            edges,
            next_edge_index: 0,
        });
        true
    }
}

impl fmt::Debug for WebGpuGeometrySolutionGraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebGpuGeometrySolutionGraph")
            .field("batch_count", &self.batches.len())
            .field("layer_count", &self.layers.len())
            .field("final_state_count", &self.final_state_count)
            .field("resident_bytes", &self.resident_bytes())
            .finish()
    }
}

#[derive(Debug)]
pub enum WebGpuGeometryPathStreamError<E> {
    InvalidGraph { final_state_index: usize },
    Consumer(E),
}

// Traversal state is passed separately so recursion reuses the caller-owned buffers.
#[allow(clippy::too_many_arguments)]
fn stream_candidate_paths<E>(
    batches: &[WebGpuGeometryExactCoverBatch],
    layers: &[TraceLayer],
    layer_index: Option<usize>,
    state_index: usize,
    reverse_path: &mut [u32; MAX_PACKING_OPERATIONS],
    depth: usize,
    sink: &mut impl FnMut(&[u32]) -> Result<(), E>,
    emitted: &mut u64,
) -> Result<bool, WebGpuGeometryPathStreamError<E>> {
    let Some(operation_batch) = batches.first() else {
        return Ok(false);
    };
    let Some(layer_index) = layer_index else {
        return Ok(false);
    };
    let Some(edges) = layers[layer_index].incoming_edges(state_index) else {
        return Ok(false);
    };
    if depth >= reverse_path.len() {
        return Ok(false);
    }
    let mut found_edge = false;
    for edge in edges {
        found_edge = true;
        if edge.operation_index as usize >= operation_batch.skeleton_cell_masks().len() {
            return Ok(false);
        }
        reverse_path[depth] = edge.operation_index;
        if layer_index == 0 {
            let Some(batch) = batches.get(edge.parent_index as usize) else {
                return Ok(false);
            };
            let operation_count = depth + 1;
            let mut canonical = [0u32; MAX_PACKING_OPERATIONS];
            canonical[..operation_count].copy_from_slice(&reverse_path[..operation_count]);
            canonical[..operation_count].sort_unstable();
            if !validate_candidate(batch, &canonical[..operation_count]) {
                return Ok(false);
            }
            sink(&canonical[..operation_count]).map_err(WebGpuGeometryPathStreamError::Consumer)?;
            *emitted = emitted.saturating_add(1);
        } else if !stream_candidate_paths(
            batches,
            layers,
            Some(layer_index - 1),
            edge.parent_index as usize,
            reverse_path,
            depth + 1,
            sink,
            emitted,
        )? {
            return Ok(false);
        }
    }
    Ok(found_edge)
}

pub(crate) fn resource_incomplete(
    generated_state_count: u32,
    capacity: u32,
) -> WebGpuGeometryExactCoverOutcome {
    WebGpuGeometryExactCoverOutcome::ResourceIncomplete(WebGpuGeometryExactCoverIncomplete {
        generated_state_count,
        capacity,
    })
}

pub(crate) fn reduce_failure_outcome(
    error: FrontierReduceError,
    retained_state_count: usize,
    capacity: u32,
) -> WebGpuGeometryExactCoverOutcome {
    let generated_state_count = match error {
        FrontierReduceError::CapacityExceeded {
            generated_state_count,
        } => generated_state_count,
        FrontierReduceError::AllocationFailed => {
            u32::try_from(retained_state_count.saturating_add(1)).unwrap_or(u32::MAX)
        }
        FrontierReduceError::InvalidInput => {
            return WebGpuGeometryExactCoverOutcome::RejectedInvalidResult {
                candidate_index: retained_state_count,
            };
        }
    };
    resource_incomplete(generated_state_count, capacity)
}

fn validate_candidate(batch: &WebGpuGeometryExactCoverBatch, indices: &[u32]) -> bool {
    if indices.len() != usize::from(batch.target_depth()) {
        return false;
    }
    let mut occupied = batch.initial_mask();
    let mut used = [0u8; 7];
    for index in indices {
        let skeleton_index = *index as usize;
        let Some(mask) = batch.skeleton_cell_masks().get(skeleton_index).copied() else {
            return false;
        };
        let Some(piece) = batch.skeleton_piece_kinds().get(skeleton_index).copied() else {
            return false;
        };
        if !(1..=7).contains(&piece)
            || occupied & mask != 0
            || mask & batch.forbidden_mask() != 0
            || mask & !batch.goal_mask() != 0
        {
            return false;
        }
        occupied |= mask;
        used[piece as usize - 1] = used[piece as usize - 1].saturating_add(1);
    }
    used == batch.desired_piece_counts()
        && batch.goal_mask() & !occupied == 0
        && (occupied & !batch.initial_mask()) & !batch.required_fill_mask() == 0
}
