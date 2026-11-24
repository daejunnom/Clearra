use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    minimal,
    model::{
        LayerMetrics, SpinStructureOutcome, SpinStructureQuery, SpinStructureReport,
        SpinStructureStageMetrics, SpinStructureTimingMetrics, StructureOperation,
    },
    operation_catalog::LogicalOperationCatalog,
    stage_timer::StageTimer,
    structural_expand::{expand_and_verify_with_control, StructuralExpansionMetrics},
    structural_fill::{enumerate_fill_seeds_with_control, FillSeedMetrics},
    structural_verify::StructuralBuildVerifier,
    SpinStructureError,
};

pub(crate) fn compile_catalog(query: &SpinStructureQuery) -> Arc<LogicalOperationCatalog> {
    Arc::new(
        LogicalOperationCatalog::compile(query.height, query.initial_board, query.inventory)
            .expect("validated bounded structure query has a finite operation catalog"),
    )
}

pub(crate) fn target_operations(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
) -> Vec<StructureOperation> {
    if query.mode.t_only() {
        if query.inventory.count(PieceKind::T) == 0 {
            Vec::new()
        } else {
            catalog.operations_for_piece(PieceKind::T).to_vec()
        }
    } else {
        query
            .inventory
            .available()
            .flat_map(|piece| catalog.operations_for_piece(piece).iter().copied())
            .collect()
    }
}

pub(crate) fn run_target(
    query: SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
) -> SpinStructureReport {
    run_target_with_control(query, catalog, target, &ExecutionControl::default())
        .expect("serial target search has no cancellation or worker failure")
}

pub(crate) fn run_target_with_control(
    query: SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    control: &ExecutionControl,
) -> Result<SpinStructureReport, SpinStructureError> {
    if control.is_cancelled() {
        return Err(SpinStructureError::Cancelled);
    }
    let mut fill_metrics = FillSeedMetrics::default();
    let fill_started = StageTimer::start();
    let fill_seeds = enumerate_fill_seeds_with_control(
        &query,
        catalog,
        target,
        Some(&mut fill_metrics),
        control,
    );
    let fill_ns = fill_started.elapsed_ns();
    if control.is_cancelled() {
        return Err(SpinStructureError::Cancelled);
    }
    let seeds = fill_seeds
        .iter()
        .map(|seed| seed.operations().to_vec())
        .collect::<Vec<_>>();

    let mut verifier = StructuralBuildVerifier::new_with_control(&query, control);
    let mut expansion_metrics = StructuralExpansionMetrics::default();
    let expansion_started = StageTimer::start();
    let outcomes = expand_and_verify_with_control(
        &query,
        catalog,
        target,
        seeds,
        &mut verifier,
        &mut expansion_metrics,
        control,
    );
    let expansion_ns = expansion_started.elapsed_ns();
    if control.is_cancelled() {
        return Err(SpinStructureError::Cancelled);
    }
    let verifier_metrics = verifier.metrics();

    let mut report = SpinStructureReport {
        query: Some(query.clone()),
        workers_used: 1,
        complete: true,
        stages: SpinStructureStageMetrics {
            build_states: verifier_metrics.build_states,
            fill_checks: fill_metrics.search_nodes,
            support_locks: expansion_metrics.support_candidates + expansion_metrics.roof_candidates,
            corner_checks: expansion_metrics.blocker_candidates,
            entry_states: verifier_metrics.entry_states,
            verification_checks: expansion_metrics.verification_candidates,
            exact_state_deduplications: fill_metrics.duplicate_seeds
                + expansion_metrics.exact_duplicates,
            exact_outcome_deduplications: 0,
        },
        timings: SpinStructureTimingMetrics {
            fill_ns,
            expansion_ns,
            finalization_ns: 0,
            layer_ns: expansion_metrics.elapsed_ns_by_depth,
        },
        ..SpinStructureReport::default()
    };
    partition_outcomes(&mut report, outcomes);
    report.layers = layers(&query, &expansion_metrics);
    let finalization_started = StageTimer::start();
    let mut report = minimal::finalize_with_control(report, control)?;
    refresh_layer_acceptance(&mut report);
    report.timings.finalization_ns = report
        .timings
        .finalization_ns
        .saturating_add(finalization_started.elapsed_ns());
    Ok(report)
}

fn partition_outcomes(
    report: &mut SpinStructureReport,
    outcomes: impl IntoIterator<Item = SpinStructureOutcome>,
) {
    for outcome in outcomes {
        if outcome.is_mini() {
            report.mini.push(outcome);
        } else {
            report.regular.push(outcome);
        }
    }
}

pub(crate) fn layers(
    query: &SpinStructureQuery,
    metrics: &StructuralExpansionMetrics,
) -> Vec<LayerMetrics> {
    (1..=usize::from(query.placement_limit()))
        .filter(|depth| {
            metrics.candidates_by_depth[*depth] != 0
                || metrics.generated_by_depth[*depth] != 0
                || metrics.verified_by_depth[*depth] != 0
        })
        .map(|depth| LayerMetrics {
            depth: depth as u8,
            input_states: metrics.candidates_by_depth[depth],
            piece_choices: metrics.piece_choices_by_depth[depth],
            reachable_locks: metrics.reachable_locks_by_depth[depth],
            generated_states: metrics.generated_by_depth[depth],
            exact_duplicates: metrics.duplicates_by_depth[depth],
            terminal_candidates: metrics.verified_by_depth[depth],
            accepted_regular: metrics.accepted_regular_by_depth[depth],
            accepted_mini: metrics.accepted_mini_by_depth[depth],
        })
        .collect()
}

pub(crate) fn refresh_layer_acceptance(report: &mut SpinStructureReport) {
    for layer in &mut report.layers {
        layer.accepted_regular = 0;
        layer.accepted_mini = 0;
    }
    for (mini, outcomes) in [(false, &report.regular), (true, &report.mini)] {
        for outcome in outcomes {
            let depth = outcome.placement_count() as u8;
            let Some(layer) = report.layers.iter_mut().find(|layer| layer.depth == depth) else {
                continue;
            };
            if mini {
                layer.accepted_mini += 1;
            } else {
                layer.accepted_regular += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PieceInventory, SpinLineRequirement, SpinStructureMode};

    #[test]
    fn target_partition_is_t_only_or_inventory_wide_by_mode() {
        let inventory =
            PieceInventory::from_pieces([PieceKind::I, PieceKind::T]).expect("inventory");
        let mut t_query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        t_query.height = 4;
        t_query.fill_top = 4;
        t_query.line_requirement = SpinLineRequirement::Any;
        let catalog = compile_catalog(&t_query);
        assert!(target_operations(&t_query, &catalog)
            .iter()
            .all(|operation| operation.piece() == PieceKind::T));

        let mut all_query = t_query.clone();
        all_query.mode = SpinStructureMode::AllSpin;
        let pieces = target_operations(&all_query, &catalog)
            .into_iter()
            .map(StructureOperation::piece)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(pieces, [PieceKind::I, PieceKind::T].into_iter().collect());
    }
}
