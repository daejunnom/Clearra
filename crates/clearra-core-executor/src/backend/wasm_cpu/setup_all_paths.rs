use std::collections::HashSet;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_problem::SetupPathDetail;

use crate::CorePathStep;

use super::{setup_partial_build::PartialBuildGraph, WasmExactSearchError};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionStep {
    pub(super) piece: PieceKind,
    pub(super) rotation: u8,
    pub(super) x: i8,
    pub(super) y: i8,
    pub(super) cleared_lines: u8,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionPath {
    pub(super) steps: Vec<SetupSolutionStep>,
}

impl SetupSolutionPath {
    pub(super) fn into_core_path(self) -> Vec<CorePathStep> {
        self.steps
            .into_iter()
            .map(|step| {
                CorePathStep::new(
                    step.piece,
                    step.rotation,
                    i32::from(step.x),
                    i32::from(step.y),
                    "placement-only",
                    step.cleared_lines,
                )
            })
            .collect()
    }
}

pub(super) fn enumerate_setup_completion_paths(
    graph: &PartialBuildGraph,
    detail: &SetupPathDetail,
    control: &ExecutionControl,
) -> Result<Vec<SetupSolutionPath>, WasmExactSearchError> {
    let target_shape_index = u32::try_from(graph.shape_index_for_detail(detail).ok_or(
        WasmExactSearchError::InvalidProblem("setup_path_detail_shape_not_found"),
    )?)
    .map_err(|_| WasmExactSearchError::InvalidProblem("setup_path_detail_shape_index_overflow"))?;
    let mut paths = HashSet::new();
    paths.try_reserve(256).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_completion_path_storage_unavailable")
    })?;
    let mut path = Vec::with_capacity(10);
    let mut cancellation_work = 0_usize;

    for (node_index, node) in graph.nodes.iter().copied().enumerate() {
        let matches_target = node.shape_index() == Some(target_shape_index);
        if !node.live() || !matches_target {
            continue;
        }
        enumerate_suffixes(
            graph,
            node_index,
            &mut path,
            &mut paths,
            control,
            &mut cancellation_work,
        )?;
    }

    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort_unstable();
    Ok(paths)
}

fn enumerate_suffixes(
    graph: &PartialBuildGraph,
    node_index: usize,
    path: &mut Vec<SetupSolutionStep>,
    paths: &mut HashSet<SetupSolutionPath>,
    control: &ExecutionControl,
    cancellation_work: &mut usize,
) -> Result<(), WasmExactSearchError> {
    check_cancel(control, cancellation_work)?;
    let node = graph.nodes[node_index];
    if node.accepting() {
        reserve_path_slot(paths)?;
        paths.insert(SetupSolutionPath {
            steps: path.clone(),
        });
        return Ok(());
    }
    if !node.live() {
        return Ok(());
    }

    let edge_start = node.edge_start as usize;
    let edge_end = edge_start + node.edge_count as usize;
    for edge in graph.edges[edge_start..edge_end].iter().copied() {
        if !graph.nodes[edge.to as usize].live() {
            continue;
        }
        path.push(SetupSolutionStep {
            piece: edge.piece,
            rotation: edge.rotation(),
            x: edge.x,
            y: edge.y,
            cleared_lines: edge.cleared_lines(),
        });
        enumerate_suffixes(
            graph,
            edge.to as usize,
            path,
            paths,
            control,
            cancellation_work,
        )?;
        path.pop();
    }
    Ok(())
}

fn reserve_path_slot(paths: &mut HashSet<SetupSolutionPath>) -> Result<(), WasmExactSearchError> {
    if paths.len() == paths.capacity() {
        paths.try_reserve(paths.capacity().max(256)).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_completion_path_storage_unavailable")
        })?;
    }
    Ok(())
}

#[inline]
fn check_cancel(control: &ExecutionControl, work: &mut usize) -> Result<(), WasmExactSearchError> {
    *work = work.wrapping_add(1);
    if *work & 4095 == 0 && control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}
