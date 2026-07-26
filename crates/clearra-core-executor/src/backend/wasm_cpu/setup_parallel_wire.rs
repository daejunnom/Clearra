use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{
    SetupCandidatePriority, SetupCycleResetBorrowPolicy, SetupLengthPreference, SetupLimits,
    SetupPathDetail, SetupSearchMode, SetupSearchQuery,
};

use super::{
    setup_all_paths::{
        SetupAllPathEdge, SetupAllPathGraph, SetupAllPathNode, SetupSolutionPath, SetupSolutionStep,
    },
    setup_coverage_graph::{SetupCoverageEdge, SetupCoverageGraph, SetupCoverageNode},
    setup_finder::SetupHoldAction,
    setup_partial_build::{PartialBuildGraph, SetupShape},
    WasmExactSearchError,
};

const INITIALIZATION_MAGIC: [u8; 4] = *b"CSP6";
const TASK_MAGIC: [u8; 4] = *b"CST2";
const RESULT_MAGIC: [u8; 4] = *b"CSR4";
const NO_WITNESS: u32 = u32::MAX;

pub(super) fn is_setup_parallel_initialization(input: &[u8]) -> bool {
    input.starts_with(&INITIALIZATION_MAGIC)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SetupParallelShapeResult {
    pub(super) shape_index: u32,
    pub(super) build_covered_patterns: u32,
    pub(super) joint_covered_patterns: u32,
    pub(super) build_weight: f64,
    pub(super) joint_weight: f64,
    pub(super) min_covered_locks: u8,
    pub(super) max_covered_locks: u8,
    pub(super) witness_pattern_id: u32,
}

impl Default for SetupParallelShapeResult {
    fn default() -> Self {
        Self {
            shape_index: 0,
            build_covered_patterns: 0,
            joint_covered_patterns: 0,
            build_weight: 0.0,
            joint_weight: 0.0,
            min_covered_locks: u8::MAX,
            max_covered_locks: 0,
            witness_pattern_id: NO_WITNESS,
        }
    }
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
    pub(super) solution_paths: Vec<SetupSolutionPath>,
    pub(super) peak_segment_pages: u32,
}

pub(super) struct SetupParallelInitialization {
    pub(super) query: SetupSearchQuery,
    pub(super) graph: Arc<SetupCoverageGraph>,
    pub(super) shape_count: usize,
    pub(super) path_graph: Option<Arc<SetupAllPathGraph>>,
    pub(super) path_target_shape_index: Option<u32>,
}

pub(super) fn encode_initialization(
    query: &SetupSearchQuery,
    graph: &SetupCoverageGraph,
    source_graph: &PartialBuildGraph,
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
    output.push(match query.search_mode() {
        SetupSearchMode::ShapeOracle => 0,
        SetupSearchMode::QueueBased => 1,
    });
    let queue_based_pieces = match query.search_mode() {
        SetupSearchMode::ShapeOracle => &[][..],
        SetupSearchMode::QueueBased => query
            .queue()
            .as_fixed_sequence()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_parallel_queue_based_queue_missing",
            ))?
            .pieces(),
    };
    push_u32(
        &mut output,
        u32::try_from(queue_based_pieces.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_queue_based_count_overflow")
        })?,
    );
    for piece in queue_based_pieces {
        output.push(piece_code(*piece));
    }
    output.push(u8::from(
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
    ));
    output.push(match query.candidate_priority() {
        SetupCandidatePriority::All => 0,
        SetupCandidatePriority::BuildProbabilityFirst => 1,
        SetupCandidatePriority::PcProbabilityFirst => 2,
    });
    output.push(match query.length_preference() {
        SetupLengthPreference::Auto => 0,
        SetupLengthPreference::Longer => 1,
        SetupLengthPreference::Shorter => 2,
    });
    if let Some(detail) = query.path_detail() {
        output.push(1);
        push_u64(&mut output, detail.board_mask());
        push_string(&mut output, detail.condition_id())?;
    } else {
        output.push(0);
    }
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
    if let Some(detail) = query.path_detail() {
        let target_shape_index = u32::try_from(
            shapes
                .iter()
                .position(|shape| shape.board == detail.board_mask())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_path_detail_shape_missing",
                ))?,
        )
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_path_detail_shape_index_overflow")
        })?;
        let path_graph = SetupAllPathGraph::from_partial(source_graph);
        push_u32(&mut output, target_shape_index);
        push_u32(&mut output, path_graph.root);
        push_u32(
            &mut output,
            u32::try_from(path_graph.nodes.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_all_paths_node_count_overflow")
            })?,
        );
        push_u32(
            &mut output,
            u32::try_from(path_graph.edges.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_all_paths_edge_count_overflow")
            })?,
        );
        for node in &path_graph.nodes {
            push_u64(&mut output, node.board);
            push_u32(&mut output, node.edge_start);
            push_u32(&mut output, node.edge_count);
            push_u32(&mut output, node.shape_index);
            output.push(node.depth);
            output.push(u8::from(node.live) | (u8::from(node.accepting) << 1));
        }
        for edge in &path_graph.edges {
            push_u32(&mut output, edge.to);
            output.push(piece_code(edge.piece));
            output.push(edge.rotation);
            output.push(edge.x as u8);
            output.push(edge.y as u8);
            output.push(edge.cleared_lines);
        }
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
    let search_mode = match reader.u8()? {
        0 => SetupSearchMode::ShapeOracle,
        1 => SetupSearchMode::QueueBased,
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_search_mode_invalid",
            ));
        }
    };
    let queue_based_count = reader.usize_from_u32("setup_parallel_queue_based_count_overflow")?;
    if (search_mode == SetupSearchMode::ShapeOracle && queue_based_count != 0)
        || (search_mode == SetupSearchMode::QueueBased
            && (queue_based_count == 0 || residue_count + queue_based_count > 7))
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "setup_parallel_queue_based_count_invalid",
        ));
    }
    let mut queue_based_pieces = Vec::new();
    queue_based_pieces
        .try_reserve_exact(queue_based_count)
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_queue_based_storage_unavailable")
        })?;
    for _ in 0..queue_based_count {
        queue_based_pieces.push(piece_from_code(reader.u8()?)?);
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
    let candidate_priority = match reader.u8()? {
        0 => SetupCandidatePriority::All,
        1 => SetupCandidatePriority::BuildProbabilityFirst,
        2 => SetupCandidatePriority::PcProbabilityFirst,
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_candidate_priority_invalid",
            ));
        }
    };
    let length_preference = match reader.u8()? {
        0 => SetupLengthPreference::Auto,
        1 => SetupLengthPreference::Longer,
        2 => SetupLengthPreference::Shorter,
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_length_preference_invalid",
            ));
        }
    };
    let path_detail = match reader.u8()? {
        0 => None,
        1 => {
            let board_mask = reader.u64()?;
            let condition_id = reader.string("setup_parallel_path_condition_invalid")?;
            Some(SetupPathDetail::new(board_mask, condition_id).ok_or(
                WasmExactSearchError::InvalidProblem("setup_parallel_path_detail_invalid"),
            )?)
        }
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_path_detail_flag_invalid",
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
    let mut query = SetupSearchQuery::default().with_remaining_pieces(residue);
    if search_mode == SetupSearchMode::QueueBased {
        query = query.with_queue_based_pieces(queue_based_pieces);
    }
    let mut query = query
        .with_cycle_reset_borrow_policy(borrow_policy)
        .with_candidate_priority(candidate_priority)
        .with_length_preference(length_preference)
        .with_limits(limits);
    if let Some(detail) = path_detail {
        query = query.with_path_detail(detail);
    }

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
    let (path_graph, path_target_shape_index) = if query.path_detail().is_some() {
        let target_shape_index = reader.u32()?;
        if target_shape_index as usize >= shape_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_path_target_shape_out_of_range",
            ));
        }
        let path_root = reader.u32()?;
        let path_node_count = reader.usize_from_u32("setup_all_paths_node_count_overflow")?;
        let path_edge_count = reader.usize_from_u32("setup_all_paths_edge_count_overflow")?;
        let mut path_nodes = Vec::new();
        path_nodes.try_reserve_exact(path_node_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_node_storage_unavailable")
        })?;
        for _ in 0..path_node_count {
            let board = reader.u64()?;
            let edge_start = reader.u32()?;
            let edge_count = reader.u32()?;
            let shape_index = reader.u32()?;
            let depth = reader.u8()?;
            let flags = reader.u8()?;
            if flags & !0b11 != 0 {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_node_flags_invalid",
                ));
            }
            path_nodes.push(SetupAllPathNode {
                board,
                edge_start,
                edge_count,
                shape_index,
                depth,
                live: flags & 1 != 0,
                accepting: flags & 2 != 0,
            });
        }
        let mut path_edges = Vec::new();
        path_edges.try_reserve_exact(path_edge_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_edge_storage_unavailable")
        })?;
        for _ in 0..path_edge_count {
            path_edges.push(SetupAllPathEdge {
                to: reader.u32()?,
                piece: piece_from_code(reader.u8()?)?,
                rotation: reader.u8()?,
                x: reader.u8()? as i8,
                y: reader.u8()? as i8,
                cleared_lines: reader.u8()?,
            });
        }
        (
            Some(Arc::new(SetupAllPathGraph::from_wire_parts(
                path_nodes, path_edges, path_root,
            )?)),
            Some(target_shape_index),
        )
    } else {
        (None, None)
    };
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
        path_graph,
        path_target_shape_index,
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
            output.push(shape.min_covered_locks);
            output.push(shape.max_covered_locks);
            push_u32(&mut output, shape.witness_pattern_id);
        }
        push_u32(
            &mut output,
            u32::try_from(result.solution_paths.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_all_paths_count_overflow")
            })?,
        );
        for path in &result.solution_paths {
            output.push(u8::try_from(path.steps.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_all_paths_step_count_overflow")
            })?);
            for step in &path.steps {
                output.push(piece_code(step.piece));
                output.push(step.rotation);
                output.push(step.x as u8);
                output.push(step.y as u8);
                output.push(step.hold_action.code());
                output.push(step.cleared_lines);
            }
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
                min_covered_locks: reader.u8()?,
                max_covered_locks: reader.u8()?,
                witness_pattern_id: reader.u32()?,
            };
            if !shape.build_weight.is_finite()
                || !shape.joint_weight.is_finite()
                || shape.build_weight < 0.0
                || shape.joint_weight < 0.0
                || shape.joint_weight > shape.build_weight + f64::EPSILON
                || shape.joint_covered_patterns > shape.build_covered_patterns
                || (shape.joint_covered_patterns == 0
                    && (shape.min_covered_locks != u8::MAX || shape.max_covered_locks != 0))
                || (shape.joint_covered_patterns != 0
                    && (shape.min_covered_locks == 0
                        || shape.min_covered_locks > shape.max_covered_locks
                        || shape.max_covered_locks > 8))
                || (shape.joint_covered_patterns == 0 && shape.witness_pattern_id != NO_WITNESS)
                || (shape.joint_covered_patterns != 0 && shape.witness_pattern_id == NO_WITNESS)
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_shape_result_invalid",
                ));
            }
            covered_shapes.push(shape);
        }
        let path_count = reader.usize_from_u32("setup_all_paths_count_overflow")?;
        let mut solution_paths = Vec::new();
        solution_paths.try_reserve_exact(path_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_all_paths_result_storage_unavailable")
        })?;
        for _ in 0..path_count {
            let step_count = reader.u8()? as usize;
            if !(1..=8).contains(&step_count) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_all_paths_step_count_invalid",
                ));
            }
            let mut steps = Vec::new();
            steps.try_reserve_exact(step_count).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_all_paths_step_storage_unavailable")
            })?;
            for _ in 0..step_count {
                let step = SetupSolutionStep {
                    piece: piece_from_code(reader.u8()?)?,
                    rotation: reader.u8()?,
                    x: reader.u8()? as i8,
                    y: reader.u8()? as i8,
                    hold_action: SetupHoldAction::from_code(reader.u8()?).ok_or(
                        WasmExactSearchError::InvalidProblem("setup_all_paths_hold_action_invalid"),
                    )?,
                    cleared_lines: reader.u8()?,
                };
                if step.rotation > 3 || step.cleared_lines > 7 {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "setup_all_paths_step_invalid",
                    ));
                }
                steps.push(step);
            }
            solution_paths.push(SetupSolutionPath { steps });
        }
        results.push(SetupParallelTaskResult {
            task_index,
            condition_index,
            word_start,
            word_end,
            global_pattern_count,
            covered_shapes,
            solution_paths,
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

fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), WasmExactSearchError> {
    push_u32(
        output,
        u32::try_from(value.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_string_length_overflow")
        })?,
    );
    output.extend_from_slice(value.as_bytes());
    Ok(())
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

    fn string(&mut self, reason: &'static str) -> Result<String, WasmExactSearchError> {
        let len = self.usize_from_u32(reason)?;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| WasmExactSearchError::InvalidProblem(reason))
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
