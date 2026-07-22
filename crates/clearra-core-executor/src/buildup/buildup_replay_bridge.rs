mod native_replay_trace {
    use clearra_core_ffi::{CBuildVariantView, CPackingCandidate};
    use clearra_problem::SearchProblem;
    use clearra_replay::ReplayTrace;

    use crate::{
        buildup::buildup_replay_bridge::replay_layout::replay_layout,
        spin::{build_variant_mapper::BuildVariantMapper, BuildVariantReplayEvidence},
    };

    pub(super) fn native_replay_trace_from_build_variant(
        problem: &SearchProblem,
        accepted_candidates: &[CPackingCandidate],
        variant: &CBuildVariantView,
    ) -> Option<ReplayTrace> {
        let candidate = accepted_candidates
            .iter()
            .find(|candidate| candidate.candidate_id == variant.candidate_id())?;
        let layout = replay_layout(problem)?;
        let replay_evidence = BuildVariantReplayEvidence::from_build_variant_and_candidate(
            variant.clone(),
            layout,
            problem.initial_board().occupied_mask(),
            candidate,
        )
        .ok()?;
        BuildVariantMapper::to_replay_trace_with_marker(
            replay_evidence.variant(),
            &replay_evidence,
            true,
            true,
        )
        .ok()
    }
}
mod native_trace_material {
    use clearra_core_ffi::{CBuildVariantView, CPackingCandidate};
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::{
            buildup_replay_bridge::{
                native_replay_trace::native_replay_trace_from_build_variant,
                path_steps::path_steps_from_candidate,
                representative_trace_selection::RepresentativeTraceSelection,
                trace_material::BuildUpTraceMaterial,
            },
            buildup_trace_retention::{retained_trace_count, trace_key_for_build_variant},
        },
        packing::scenario_packing_witness::ScenarioPackingWitness,
    };

    pub(super) fn native_build_variant_trace_material(
        problem: &SearchProblem,
        accepted_candidates: &[CPackingCandidate],
        build_variants: &[CBuildVariantView],
        witness: ScenarioPackingWitness,
    ) -> BuildUpTraceMaterial {
        let selected_variant = RepresentativeTraceSelection::select(build_variants)
            .and_then(|selection| selection.selected_variant(build_variants));
        let selected_candidate = selected_variant.and_then(|variant| {
            accepted_candidates
                .iter()
                .find(|candidate| candidate.candidate_id == variant.candidate_id())
        });
        let path_steps = selected_candidate
            .map(|candidate| path_steps_from_candidate(candidate, witness))
            .unwrap_or_default();
        let sample_replay_trace = selected_variant.and_then(|variant| {
            native_replay_trace_from_build_variant(problem, accepted_candidates, variant)
        });
        let retained_trace_count = if path_steps.is_empty() && sample_replay_trace.is_none() {
            0
        } else {
            retained_trace_count(problem, witness, build_variants.len())
        };
        let trace_key = (retained_trace_count > 0)
            .then(|| selected_variant.map(|variant| trace_key_for_build_variant(variant, 0)))
            .flatten();

        BuildUpTraceMaterial {
            path_steps,
            sample_replay_trace,
            trace_key,
            retained_trace_count,
        }
    }
}
mod postprocess_execution_batch {
    use clearra_core_ffi::{CBuildVariantView, CPackingCandidate};
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::{
            buildup_replay_bridge::native_replay_trace::native_replay_trace_from_build_variant,
            buildup_trace_retention::trace_key_for_build_variant,
        },
        CorePostProcessExecution,
    };

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub(crate) struct BuildUpPostProcessBatch {
        pub(crate) executions: Vec<CorePostProcessExecution>,
        pub(crate) all_variants_materialized: bool,
    }

    pub(crate) fn postprocess_executions_from_build_variants(
        problem: &SearchProblem,
        accepted_candidates: &[CPackingCandidate],
        build_variants: &[CBuildVariantView],
    ) -> BuildUpPostProcessBatch {
        let mut executions = Vec::with_capacity(build_variants.len());
        let mut all_variants_materialized = true;

        for variant in build_variants {
            let Some(replay_trace) =
                native_replay_trace_from_build_variant(problem, accepted_candidates, variant)
            else {
                all_variants_materialized = false;
                continue;
            };
            let Ok(pattern_id) = usize::try_from(variant.coverage_pattern_id()) else {
                all_variants_materialized = false;
                continue;
            };
            executions.push(CorePostProcessExecution::new(
                variant.candidate_id(),
                pattern_id,
                trace_key_for_build_variant(variant, variant.candidate_id()),
                replay_trace,
            ));
        }

        BuildUpPostProcessBatch {
            executions,
            all_variants_materialized,
        }
    }
}
mod path_steps {
    use clearra_core_ffi::CPackingCandidate;

    use crate::{
        buildup::buildup_replay_bridge::piece_code::piece_from_code,
        core_execution_result::CorePathStep,
        packing::scenario_packing_witness::ScenarioPackingWitness,
    };

    pub(super) fn path_steps_from_candidate(
        candidate: &CPackingCandidate,
        witness: ScenarioPackingWitness,
    ) -> Vec<CorePathStep> {
        let operation_count = usize::from(candidate.operation_count);
        candidate
            .operations
            .iter()
            .take(operation_count)
            .enumerate()
            .filter_map(|(index, operation)| {
                let piece = piece_from_code(operation.piece)?;
                let cleared_lines = if index + 1 == operation_count {
                    witness.cleared_lines
                } else {
                    0
                };
                Some(CorePathStep::new(
                    piece,
                    operation.rotation,
                    i32::from(operation.x),
                    i32::from(operation.y),
                    "none",
                    cleared_lines,
                ))
            })
            .collect()
    }
}
mod piece_code {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    pub(super) fn piece_from_code(code: u8) -> Option<PieceKind> {
        match code {
            clearra_core_ffi::problem::C_PIECE_I => Some(PieceKind::I),
            clearra_core_ffi::problem::C_PIECE_O => Some(PieceKind::O),
            clearra_core_ffi::problem::C_PIECE_T => Some(PieceKind::T),
            clearra_core_ffi::problem::C_PIECE_S => Some(PieceKind::S),
            clearra_core_ffi::problem::C_PIECE_Z => Some(PieceKind::Z),
            clearra_core_ffi::problem::C_PIECE_J => Some(PieceKind::J),
            clearra_core_ffi::problem::C_PIECE_L => Some(PieceKind::L),
            _ => None,
        }
    }
}
mod replay_layout {
    use clearra_core_domain::board::board_size::BoardSize;
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_problem::SearchProblem;

    pub(super) fn replay_layout(problem: &SearchProblem) -> Option<Board64Layout> {
        let width = problem.initial_board().width();
        let max_height = (64 / usize::from(width)).max(1) as u16;
        let board_size =
            BoardSize::new(width, problem.search_height().min(max_height).max(1)).ok()?;
        Board64Layout::new(board_size).ok()
    }
}
#[path = "buildup_replay_bridge/representative_trace_selection.rs"]
pub(crate) mod representative_trace_selection;
mod trace_material {
    use clearra_replay::ReplayTrace;

    use crate::core_execution_result::CorePathStep;

    #[derive(Clone, Debug, Default, PartialEq)]
    pub(crate) struct BuildUpTraceMaterial {
        pub(crate) path_steps: Vec<CorePathStep>,
        pub(crate) sample_replay_trace: Option<ReplayTrace>,
        pub(crate) trace_key: Option<String>,
        pub(crate) retained_trace_count: usize,
    }
}
mod trace_material_selector {
    use clearra_core_ffi::{CBuildVariantView, CPackingCandidate};
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::buildup_replay_bridge::{
            native_trace_material::native_build_variant_trace_material,
            trace_material::BuildUpTraceMaterial,
        },
        packing::scenario_packing_witness::ScenarioPackingWitness,
    };

    pub(crate) fn trace_material_for_execution(
        problem: &SearchProblem,
        accepted_candidates: &[CPackingCandidate],
        build_variants: &[CBuildVariantView],
        witness: ScenarioPackingWitness,
    ) -> BuildUpTraceMaterial {
        if !witness.solution_found {
            return BuildUpTraceMaterial::default();
        }
        native_build_variant_trace_material(problem, accepted_candidates, build_variants, witness)
    }
}
pub(crate) use postprocess_execution_batch::{
    postprocess_executions_from_build_variants, BuildUpPostProcessBatch,
};
pub(crate) use trace_material::BuildUpTraceMaterial;
pub(crate) use trace_material_selector::trace_material_for_execution;
