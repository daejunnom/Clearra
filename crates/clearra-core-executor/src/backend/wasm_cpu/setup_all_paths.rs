use std::collections::HashMap;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_problem::SetupPathDetail;

use crate::CorePathStep;

use super::{setup_partial_build::PartialBuildGraph, WasmExactSearchError};

const MAX_PC_PIECES: usize = 10;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionStep {
    pub(super) piece: PieceKind,
    pub(super) rotation: u8,
    pub(super) x: i8,
    pub(super) y: i8,
    pub(super) cleared_lines: u8,
    placement_row: u16,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SetupSolutionPath {
    pub(super) steps: Vec<SetupSolutionStep>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SetupCompletionIdentity {
    placements: [u16; MAX_PC_PIECES],
    placement_count: u8,
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

impl SetupCompletionIdentity {
    fn from_path(
        path: &[SetupSolutionStep],
        deleted_rows: u16,
    ) -> Result<Self, WasmExactSearchError> {
        if path.len() > MAX_PC_PIECES {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_completion_path_piece_count_overflow",
            ));
        }
        let mut placements = [0_u16; MAX_PC_PIECES];
        for (index, step) in path.iter().enumerate() {
            placements[index] = step.placement_row;
        }
        placements[..path.len()].sort_unstable();
        Ok(Self {
            placements,
            placement_count: path.len() as u8,
            deleted_rows,
        })
    }
}

pub(super) fn enumerate_setup_completion_paths(
    graph: &PartialBuildGraph,
    detail: &SetupPathDetail,
    control: &ExecutionControl,
) -> Result<Vec<SetupSolutionPath>, WasmExactSearchError> {
    let target_shape_index =
        graph
            .shape_index_for_detail(detail)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_path_detail_shape_not_found",
            ))?;
    let target_node =
        graph
            .shape_target_node(target_shape_index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_path_detail_target_node_missing",
            ))?;
    let mut paths = HashMap::new();
    paths.try_reserve(256).map_err(|_| {
        WasmExactSearchError::InvalidProblem("setup_completion_path_storage_unavailable")
    })?;
    let mut path = Vec::with_capacity(10);
    let mut cancellation_work = 0_usize;

    let node = graph.nodes.get(target_node as usize).copied().ok_or(
        WasmExactSearchError::InvalidProblem("setup_path_detail_target_node_invalid"),
    )?;
    if node.live() {
        enumerate_suffixes(
            graph,
            target_node as usize,
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
        let identity = SetupCompletionIdentity::from_path(path, node.deleted_rows)?;
        insert_canonical_solution_path(
            paths,
            identity,
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
    for edge_index in edge_start..edge_end {
        let edge = graph.edges[edge_index];
        if !graph.nodes[edge.to as usize].live() {
            continue;
        }
        let placement_row =
            graph
                .edge_row_id(edge_index)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_completion_edge_row_missing",
                ))?;
        path.push(SetupSolutionStep {
            piece: edge.piece,
            rotation: edge.rotation(),
            x: edge.x,
            y: edge.y,
            cleared_lines: edge.cleared_lines(),
            placement_row,
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

    fn path(piece: PieceKind, x: i8, placement_row: u16) -> SetupSolutionPath {
        SetupSolutionPath {
            steps: vec![SetupSolutionStep {
                piece,
                rotation: 0,
                x,
                y: 0,
                cleared_lines: 0,
                placement_row,
            }],
        }
    }

    #[test]
    fn completion_paths_retain_distinct_exact_placement_sets() {
        let mut paths = HashMap::new();
        let first = path(PieceKind::T, 4, 7);
        let second = path(PieceKind::I, 1, 11);

        insert_canonical_solution_path(
            &mut paths,
            SetupCompletionIdentity::from_path(&first.steps, 3).expect("first identity"),
            first,
        )
        .expect("first path");
        insert_canonical_solution_path(
            &mut paths,
            SetupCompletionIdentity::from_path(&second.steps, 3).expect("second identity"),
            second,
        )
        .expect("second path");

        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn completion_paths_dedupe_order_variants_of_the_same_placement_set() {
        let mut paths = HashMap::new();
        let forward = SetupSolutionPath {
            steps: vec![
                path(PieceKind::T, 4, 7).steps[0].clone(),
                path(PieceKind::I, 1, 11).steps[0].clone(),
            ],
        };
        let reverse = SetupSolutionPath {
            steps: forward.steps.iter().cloned().rev().collect(),
        };
        let identity =
            SetupCompletionIdentity::from_path(&forward.steps, 3).expect("forward identity");
        assert_eq!(
            identity,
            SetupCompletionIdentity::from_path(&reverse.steps, 3).expect("reverse identity")
        );

        insert_canonical_solution_path(&mut paths, identity, forward.clone())
            .expect("forward path");
        insert_canonical_solution_path(&mut paths, identity, reverse.clone())
            .expect("reverse path");

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths.get(&identity).expect("canonical path"),
            std::cmp::min(&forward, &reverse)
        );
    }

    #[test]
    fn completion_paths_dedupe_rotation_aliases_of_the_same_inverse_lock_row() {
        let mut paths = HashMap::new();
        let first = path(PieceKind::O, 3, 19);
        let mut alias = first.clone();
        alias.steps[0].rotation = 1;
        let identity = SetupCompletionIdentity::from_path(&first.steps, 0).expect("first identity");

        insert_canonical_solution_path(&mut paths, identity, first).expect("first path");
        insert_canonical_solution_path(
            &mut paths,
            SetupCompletionIdentity::from_path(&alias.steps, 0).expect("alias identity"),
            alias,
        )
        .expect("alias path");

        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn completion_paths_retain_distinct_deleted_row_states() {
        let candidate = path(PieceKind::I, 1, 11);
        let first =
            SetupCompletionIdentity::from_path(&candidate.steps, 1).expect("first identity");
        let second =
            SetupCompletionIdentity::from_path(&candidate.steps, 2).expect("second identity");

        assert_ne!(first, second);
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
