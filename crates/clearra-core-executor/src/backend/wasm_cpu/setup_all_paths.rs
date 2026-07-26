use std::collections::HashMap;

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SetupCompletionIdentity {
    placement_set_id: u32,
    deleted_rows: u16,
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
    let mut paths = HashMap::new();
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

    let mut paths = paths.into_values().collect::<Vec<_>>();
    paths.sort_unstable();
    Ok(paths)
}

fn enumerate_suffixes(
    graph: &PartialBuildGraph,
    node_index: usize,
    path: &mut Vec<SetupSolutionStep>,
    paths: &mut HashMap<SetupCompletionIdentity, SetupSolutionPath>,
    control: &ExecutionControl,
    cancellation_work: &mut usize,
) -> Result<(), WasmExactSearchError> {
    check_cancel(control, cancellation_work)?;
    let node = graph.nodes[node_index];
    if node.accepting() {
        insert_canonical_solution_path(
            paths,
            SetupCompletionIdentity {
                placement_set_id: node.placement_set_id(),
                deleted_rows: node.deleted_rows,
            },
            SetupSolutionPath {
                steps: path.clone(),
            },
        )?;
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

fn insert_canonical_solution_path(
    paths: &mut HashMap<SetupCompletionIdentity, SetupSolutionPath>,
    identity: SetupCompletionIdentity,
    candidate: SetupSolutionPath,
) -> Result<(), WasmExactSearchError> {
    if let Some(current) = paths.get_mut(&identity) {
        if candidate < *current {
            *current = candidate;
        }
        return Ok(());
    }
    reserve_path_slot(paths)?;
    paths.insert(identity, candidate);
    Ok(())
}

fn reserve_path_slot(
    paths: &mut HashMap<SetupCompletionIdentity, SetupSolutionPath>,
) -> Result<(), WasmExactSearchError> {
    if paths.len() == paths.capacity() {
        paths.try_reserve(paths.capacity().max(256)).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_completion_path_storage_unavailable")
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(piece: PieceKind, x: i8) -> SetupSolutionPath {
        SetupSolutionPath {
            steps: vec![SetupSolutionStep {
                piece,
                rotation: 0,
                x,
                y: 0,
                cleared_lines: 0,
            }],
        }
    }

    #[test]
    fn completion_paths_dedupe_by_final_exact_placement_state() {
        let identity = SetupCompletionIdentity {
            placement_set_id: 7,
            deleted_rows: 3,
        };
        let mut paths = HashMap::new();

        insert_canonical_solution_path(&mut paths, identity, path(PieceKind::T, 4))
            .expect("first path");
        insert_canonical_solution_path(&mut paths, identity, path(PieceKind::I, 1))
            .expect("replacement path");

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths.get(&identity).expect("canonical path"),
            &path(PieceKind::I, 1)
        );
    }

    #[test]
    fn completion_paths_retain_distinct_deleted_row_states() {
        let mut paths = HashMap::new();

        insert_canonical_solution_path(
            &mut paths,
            SetupCompletionIdentity {
                placement_set_id: 7,
                deleted_rows: 1,
            },
            path(PieceKind::I, 1),
        )
        .expect("first state");
        insert_canonical_solution_path(
            &mut paths,
            SetupCompletionIdentity {
                placement_set_id: 7,
                deleted_rows: 2,
            },
            path(PieceKind::I, 1),
        )
        .expect("second state");

        assert_eq!(paths.len(), 2);
    }
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
