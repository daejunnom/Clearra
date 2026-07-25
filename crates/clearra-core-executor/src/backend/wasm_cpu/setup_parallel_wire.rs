use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{SetupCycleResetBorrowPolicy, SetupLimits, SetupSearchQuery};

use super::{
    setup_coverage_graph::{SetupCoverageEdge, SetupCoverageGraph, SetupCoverageNode},
    setup_partial_build::SetupShape,
    WasmExactSearchError,
};

const INITIALIZATION_MAGIC: [u8; 4] = *b"CSP2";
const TASK_MAGIC: [u8; 4] = *b"CST2";
const RESULT_MAGIC: [u8; 4] = *b"CSR2";
const NO_WITNESS: u32 = u32::MAX;

pub(super) fn is_setup_parallel_initialization(input: &[u8]) -> bool {
    input.starts_with(&INITIALIZATION_MAGIC)
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SetupParallelShapeResult {
    pub(super) shape_index: u32,
    pub(super) build_covered_patterns: u32,
    pub(super) joint_covered_patterns: u32,
    pub(super) build_weight: f64,
    pub(super) joint_weight: f64,
    pub(super) witness_pattern_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SetupParallelTask {
    pub(super) task_index: u32,
    pub(super) condition_index: u32,
    pub(super) word_start: u32,
    pub(super) word_end: u32,
}

#[derive(Debug)]
pub(super) struct SetupParallelTaskResult {
    pub(super) task_index: u32,
    pub(super) condition_index: u32,
    pub(super) word_start: u32,
    pub(super) word_end: u32,
    pub(super) global_pattern_count: u32,
    pub(super) covered_shapes: Vec<SetupParallelShapeResult>,
    pub(super) peak_segment_pages: u32,
}

pub(super) struct SetupParallelInitialization {
    pub(super) query: SetupSearchQuery,
    pub(super) graph: Arc<SetupCoverageGraph>,
    pub(super) shape_count: usize,
}

pub(super) fn encode_initialization(
    query: &SetupSearchQuery,
    graph: &SetupCoverageGraph,
    shapes: &[SetupShape],
) -> Result<Vec<u8>, WasmExactSearchError> {
    let limits = query.limits();
    let mut output = Vec::new();
    let estimated = 128_usize
        .checked_add(graph.nodes.len().saturating_mul(12))
        .and_then(|value| value.checked_add(graph.edges.len().saturating_mul(4)))
        .ok_or(WasmExactSearchError::InvalidProblem(
            "setup_parallel_initialization_size_overflow",
        ))?;
    output.try_reserve(estimated).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_initialization_storage_unavailable")
    })?;
    output.extend_from_slice(&INITIALIZATION_MAGIC);
    push_u32(
        &mut output,
        u32::try_from(query.residue().pieces().len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_residue_count_overflow")
        })?,
    );
    for piece in query.residue().pieces() {
        output.push(piece_code(*piece));
    }
    output.push(u8::from(
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
    ));
    for value in [
        limits.max_shape_families(),
        limits.max_tiling_variants_per_family(),
        limits.max_build_variants_per_tiling(),
        limits.max_results(),
        limits.max_patterns(),
        limits.post_pc_retained_trace_limit(),
    ] {
        push_u64(
            &mut output,
            u64::try_from(value).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_parallel_limit_overflow")
            })?,
        );
    }
    push_u32(&mut output, graph.root);
    push_u32(
        &mut output,
        u32::try_from(graph.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_node_count_overflow")
        })?,
    );
    push_u32(
        &mut output,
        u32::try_from(graph.edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_edge_count_overflow")
        })?,
    );
    push_u32(
        &mut output,
        u32::try_from(shapes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_shape_count_overflow")
        })?,
    );
    for node in &graph.nodes {
        push_u32(&mut output, node.edge_start);
        push_u32(&mut output, node.shape_index().unwrap_or(u32::MAX));
        push_u16(&mut output, node.edge_count);
        output.push(node.depth);
        output.push(node.flags());
    }
    for edge in &graph.edges {
        push_u32(&mut output, edge.raw());
    }
    Ok(output)
}

pub(super) fn decode_initialization(
    input: &[u8],
) -> Result<SetupParallelInitialization, WasmExactSearchError> {
    let mut reader = Reader::new(input);
    reader.expect_magic(INITIALIZATION_MAGIC)?;
    let residue_count = reader.usize_from_u32("setup_parallel_residue_count_overflow")?;
    if !(1..=8).contains(&residue_count) {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_parallel_residue_count_invalid",
        ));
    }
    let mut residue = Vec::new();
    residue.try_reserve_exact(residue_count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_residue_storage_unavailable")
    })?;
    for _ in 0..residue_count {
        residue.push(piece_from_code(reader.u8()?)?);
    }
    let borrow_policy = match reader.u8()? {
        0 => SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse,
        1 => SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_borrow_policy_invalid",
            ));
        }
    };
    let mut limits = [0_usize; 6];
    for value in &mut limits {
        *value = reader.usize_from_u64("setup_parallel_limit_overflow")?;
    }
    let limits = SetupLimits::new(
        limits[0], limits[1], limits[2], limits[3], limits[4], limits[5],
    )
    .map_err(|_| WasmExactSearchError::InvalidProblem("setup_parallel_limits_invalid"))?;
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(residue)
        .with_cycle_reset_borrow_policy(borrow_policy)
        .with_limits(limits);

    let root = reader.u32()?;
    let node_count = reader.usize_from_u32("setup_parallel_node_count_overflow")?;
    let edge_count = reader.usize_from_u32("setup_parallel_edge_count_overflow")?;
    let shape_count = reader.usize_from_u32("setup_parallel_shape_count_overflow")?;
    let mut nodes = Vec::new();
    nodes.try_reserve_exact(node_count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_node_storage_unavailable")
    })?;
    for _ in 0..node_count {
        let edge_start = reader.u32()?;
        let shape_index = reader.u32()?;
        let edge_count = reader.u16()?;
        let depth = reader.u8()?;
        let flags = reader.u8()?;
        nodes.push(SetupCoverageNode::from_wire(
            edge_start,
            edge_count,
            shape_index,
            depth,
            flags,
        )?);
    }
    let mut edges = Vec::new();
    edges.try_reserve_exact(edge_count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_edge_storage_unavailable")
    })?;
    for _ in 0..edge_count {
        edges.push(SetupCoverageEdge::from_raw(reader.u32()?)?);
    }
    reader.finish()?;
    if nodes.iter().any(|node| {
        node.shape_index()
            .is_some_and(|index| index as usize >= shape_count)
    }) {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_parallel_node_shape_out_of_range",
        ));
    }
    Ok(SetupParallelInitialization {
        query,
        graph: Arc::new(SetupCoverageGraph::from_wire_parts(nodes, edges, root)?),
        shape_count,
    })
}

pub(super) fn encode_tasks(tasks: &[SetupParallelTask]) -> Vec<u8> {
    let mut output = Vec::with_capacity(8 + tasks.len() * 16);
    output.extend_from_slice(&TASK_MAGIC);
    push_u32(&mut output, u32::try_from(tasks.len()).unwrap_or(u32::MAX));
    for task in tasks {
        push_u32(&mut output, task.task_index);
        push_u32(&mut output, task.condition_index);
        push_u32(&mut output, task.word_start);
        push_u32(&mut output, task.word_end);
    }
    output
}

pub(super) fn decode_tasks(input: &[u8]) -> Result<Vec<SetupParallelTask>, WasmExactSearchError> {
    let mut reader = Reader::new(input);
    reader.expect_magic(TASK_MAGIC)?;
    let count = reader.usize_from_u32("setup_parallel_task_count_overflow")?;
    let mut tasks = Vec::new();
    tasks.try_reserve_exact(count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_task_storage_unavailable")
    })?;
    for _ in 0..count {
        let task = SetupParallelTask {
            task_index: reader.u32()?,
            condition_index: reader.u32()?,
            word_start: reader.u32()?,
            word_end: reader.u32()?,
        };
        if task.word_start >= task.word_end {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_task_word_range_invalid",
            ));
        }
        tasks.push(task);
    }
    reader.finish()?;
    Ok(tasks)
}

pub(super) fn encode_results(
    results: &[SetupParallelTaskResult],
) -> Result<Vec<u8>, WasmExactSearchError> {
    let mut output = Vec::new();
    output.try_reserve(8).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_result_storage_unavailable")
    })?;
    output.extend_from_slice(&RESULT_MAGIC);
    push_u32(
        &mut output,
        u32::try_from(results.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_result_count_overflow")
        })?,
    );
    for result in results {
        push_u32(&mut output, result.task_index);
        push_u32(&mut output, result.condition_index);
        push_u32(&mut output, result.word_start);
        push_u32(&mut output, result.word_end);
        push_u32(&mut output, result.global_pattern_count);
        push_u32(&mut output, result.peak_segment_pages);
        push_u32(
            &mut output,
            u32::try_from(result.covered_shapes.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_parallel_covered_shape_count_overflow")
            })?,
        );
        for shape in &result.covered_shapes {
            push_u32(&mut output, shape.shape_index);
            push_u32(&mut output, shape.build_covered_patterns);
            push_u32(&mut output, shape.joint_covered_patterns);
            push_u64(&mut output, shape.build_weight.to_bits());
            push_u64(&mut output, shape.joint_weight.to_bits());
            push_u32(&mut output, shape.witness_pattern_id);
        }
    }
    Ok(output)
}

pub(super) fn decode_results(
    input: &[u8],
) -> Result<Vec<SetupParallelTaskResult>, WasmExactSearchError> {
    let mut reader = Reader::new(input);
    reader.expect_magic(RESULT_MAGIC)?;
    let count = reader.usize_from_u32("setup_parallel_result_count_overflow")?;
    let mut results = Vec::new();
    results.try_reserve_exact(count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_parallel_result_storage_unavailable")
    })?;
    for _ in 0..count {
        let task_index = reader.u32()?;
        let condition_index = reader.u32()?;
        let word_start = reader.u32()?;
        let word_end = reader.u32()?;
        if word_start >= word_end {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_result_word_range_invalid",
            ));
        }
        let global_pattern_count = reader.u32()?;
        let peak_segment_pages = reader.u32()?;
        let shape_count = reader.usize_from_u32("setup_parallel_covered_shape_count_overflow")?;
        let mut covered_shapes = Vec::new();
        covered_shapes.try_reserve_exact(shape_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_covered_shape_storage_unavailable")
        })?;
        for _ in 0..shape_count {
            let shape = SetupParallelShapeResult {
                shape_index: reader.u32()?,
                build_covered_patterns: reader.u32()?,
                joint_covered_patterns: reader.u32()?,
                build_weight: f64::from_bits(reader.u64()?),
                joint_weight: f64::from_bits(reader.u64()?),
                witness_pattern_id: reader.u32()?,
            };
            if !shape.build_weight.is_finite()
                || !shape.joint_weight.is_finite()
                || shape.build_weight < 0.0
                || shape.joint_weight < 0.0
                || shape.joint_weight > shape.build_weight + f64::EPSILON
                || shape.joint_covered_patterns > shape.build_covered_patterns
                || (shape.joint_covered_patterns == 0 && shape.witness_pattern_id != NO_WITNESS)
                || (shape.joint_covered_patterns != 0 && shape.witness_pattern_id == NO_WITNESS)
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_shape_result_invalid",
                ));
            }
            covered_shapes.push(shape);
        }
        results.push(SetupParallelTaskResult {
            task_index,
            condition_index,
            word_start,
            word_end,
            global_pattern_count,
            covered_shapes,
            peak_segment_pages,
        });
    }
    reader.finish()?;
    Ok(results)
}

fn piece_code(piece: PieceKind) -> u8 {
    PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|candidate| *candidate == piece)
        .map_or(u8::MAX, |index| index as u8)
}

fn piece_from_code(code: u8) -> Result<PieceKind, WasmExactSearchError> {
    PieceKind::STANDARD_TETROMINOES
        .get(code as usize)
        .copied()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "setup_parallel_piece_code_invalid",
        ))
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct Reader<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, cursor: 0 }
    }

    fn expect_magic(&mut self, expected: [u8; 4]) -> Result<(), WasmExactSearchError> {
        let actual = self.take(4)?;
        if actual != expected {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_wire_magic_invalid",
            ));
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, WasmExactSearchError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, WasmExactSearchError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| WasmExactSearchError::InvalidProblem("setup_parallel_wire_truncated"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, WasmExactSearchError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| WasmExactSearchError::InvalidProblem("setup_parallel_wire_truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, WasmExactSearchError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| WasmExactSearchError::InvalidProblem("setup_parallel_wire_truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn usize_from_u32(&mut self, reason: &'static str) -> Result<usize, WasmExactSearchError> {
        usize::try_from(self.u32()?).map_err(|_| WasmExactSearchError::InvalidProblem(reason))
    }

    fn usize_from_u64(&mut self, reason: &'static str) -> Result<usize, WasmExactSearchError> {
        usize::try_from(self.u64()?).map_err(|_| WasmExactSearchError::InvalidProblem(reason))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], WasmExactSearchError> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_parallel_wire_length_overflow",
            ))?;
        let value =
            self.input
                .get(self.cursor..end)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_wire_truncated",
                ))?;
        self.cursor = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), WasmExactSearchError> {
        if self.cursor == self.input.len() {
            Ok(())
        } else {
            Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_wire_trailing_bytes",
            ))
        }
    }
}
