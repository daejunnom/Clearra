//! Bounded scheduling of the existing boundary DFS. Each task owns an exact
//! source prefix; reduction follows that prefix order, never completion order.
use super::*;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

/// One finite state allowance spans sibling passes, horizons and population
/// members. A request never receives N copies of its declared state cap.
pub(crate) struct StateBudget {
    limit: Option<usize>,
    used: Mutex<usize>,
    parent: Option<Arc<StateBudget>>,
}

impl StateBudget {
    pub(crate) fn new(limit: Option<usize>, parent: Option<Arc<Self>>) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: Mutex::new(0),
            parent,
        })
    }
    pub(super) fn acquire(&self) -> bool {
        let mut used = self
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.limit.is_some_and(|limit| *used >= limit) || *used == usize::MAX {
            return false;
        }
        if self.parent.as_ref().is_some_and(|parent| !parent.acquire()) {
            return false;
        }
        *used += 1;
        true
    }
    pub(crate) fn used(&self) -> usize {
        *self
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(super) fn map_error(error: ParallelWorkError<BoundaryRecoveryError>) -> BoundaryRecoveryError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => BoundaryRecoveryError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed | ParallelWorkError::WorkerPanicked => {
            BoundaryRecoveryError::ParallelWorkerFailed
        }
    }
}

pub(super) fn run_pass(
    mut producer: Pass<'_>,
    initial: State,
) -> Result<(PassResult, usize), BoundaryRecoveryError> {
    if producer.control.is_cancelled() {
        return Err(BoundaryRecoveryError::Cancelled);
    }
    if !producer.budget.acquire() {
        return Ok((PassResult::Incomplete, 0));
    }
    producer.seen.insert(initial.clone());
    let mut frame = DfsFrame::new(initial);
    supply_choices(producer.query, &frame.state, &mut frame.choices);
    frame.entered = true;
    let window = producer.control.parallelism().max(1).saturating_mul(4);
    let budget = Arc::clone(&producer.budget);
    loop {
        let mut jobs = Vec::new();
        while jobs.len() < window {
            match producer.next_child(&mut frame, 0)? {
                Some(job) => jobs.push(job),
                None => break,
            }
        }
        if jobs.is_empty() {
            return Ok((PassResult::NoPath, budget.used()));
        }
        let first_found = Arc::new(AtomicUsize::new(usize::MAX));
        let results: Vec<_> = (0..jobs.len()).map(|_| Mutex::new(None)).collect();
        try_for_each_chunk(
            producer.control,
            jobs.len(),
            1,
            || {
                let mut worker =
                    Pass::new(producer.query, producer.control, producer.max_borrowed)?;
                worker.budget = Arc::clone(&budget);
                Ok(worker)
            },
            |worker, range| {
                for ordinal in range {
                    let result = if ordinal > first_found.load(Ordering::Acquire) {
                        PassResult::NoPath
                    } else {
                        worker.seen.clear();
                        worker.incomplete = false;
                        worker.cutoff = Some((Arc::clone(&first_found), ordinal));
                        let (state, step) = &jobs[ordinal];
                        let mut path = vec![step.clone()];
                        let found = worker.visit(state.clone(), &mut path)?;
                        if found.is_some() {
                            first_found.fetch_min(ordinal, Ordering::AcqRel);
                        }
                        finish_pass(found, path, worker.incomplete)
                    };
                    *results[ordinal]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
                }
                Ok(())
            },
        )
        .map_err(map_error)?;
        producer
            .control
            .report_progress("boundary-recovery-states", budget.used() as u64, None);
        for result in results {
            let result = result
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ok_or(BoundaryRecoveryError::ParallelWorkerFailed)?;
            if !matches!(result, PassResult::NoPath) {
                return Ok((result, budget.used()));
            }
        }
    }
}

pub(super) fn finish_pass(
    found: Option<State>,
    path: Vec<BoundaryRecoveryStep>,
    incomplete: bool,
) -> PassResult {
    found.map_or_else(
        || {
            if incomplete {
                PassResult::Incomplete
            } else {
                PassResult::NoPath
            }
        },
        |state| PassResult::Found {
            steps: path,
            checkpoint_step: usize::from(state.checkpoint_step.unwrap_or(0)),
            checkpoint_is_pc: state.checkpoint_is_pc,
            borrowed_count: state.borrowed_count,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse;

    fn query() -> BoundaryRecoveryQuery {
        BoundaryRecoveryQuery {
            initial_board: Board256Mask::from_words([0x3f0, 0, 0, 0]),
            stage_one_target: Board256Mask::EMPTY,
            final_board: Board256Mask::from_words([0xc030, 0, 0, 0]),
            height: 4,
            queue: vec![PieceKind::I, PieceKind::O],
            stage_one_queue_len: 1,
            required_placements: Some(2),
            placement_role_masks: Vec::new(),
            placement_role_pieces: Vec::new(),
            max_early_placements: Some(0),
            early_placement: EarlyPlacementPolicy::AnyStageTwoRole,
            hold_enabled: false,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            preserve_b2b_by_stage: [false; 2],
            preserve_b2b_bag_mask: 0,
            initial_b2b: true,
            max_states: None,
        }
    }

    fn assert_same_product(actual: &BoundaryRecoveryReport, expected: &BoundaryRecoveryReport) {
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.steps, expected.steps);
        assert_eq!(
            actual.stage_one_checkpoint_step,
            expected.stage_one_checkpoint_step
        );
        assert_eq!(actual.checkpoint_is_pc, expected.checkpoint_is_pc);
        assert_eq!(
            actual.borrowed_stage_two_count,
            expected.borrowed_stage_two_count
        );
    }

    #[test]
    fn fixed_passes_and_automatic_horizons_keep_canonical_witnesses_at_every_width() {
        let normal = query();
        let mut failed = normal.clone();
        failed.queue = vec![PieceKind::O; 2];
        let mut recovered = normal.clone();
        recovered.queue.push(PieceKind::T);
        recovered.hold_enabled = true;
        recovered.max_early_placements = Some(1);
        recovered.placement_role_masks = vec![
            Board256Mask::from_words([0xf, 0, 0, 0]),
            Board256Mask::from_words([0x300c000, 0, 0, 0]),
        ];
        recovered.early_placement = EarlyPlacementPolicy::SelectedRole {
            index: 1,
            placement: recovered.placement_role_masks[1],
        };
        let mut automatic = normal.clone();
        automatic.initial_board = Board256Mask::EMPTY;
        automatic.stage_one_target = Board256Mask::from_words([0xf, 0, 0, 0]);
        automatic.final_board = Board256Mask::from_words([0xc03f, 0, 0, 0]);
        automatic.height = 2;
        automatic.queue = vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::I,
            PieceKind::O,
        ];
        automatic.required_placements = None;
        for source in [normal, failed, recovered, automatic] {
            let expected = source
                .search(&ExecutionControl::default().with_parallelism(1))
                .unwrap();
            for workers in [2, 4, 8] {
                let actual = source
                    .search(&ExecutionControl::default().with_parallelism(workers))
                    .unwrap();
                assert_same_product(&actual, &expected);
            }
        }
    }

    #[test]
    fn parallel_population_preserves_original_probability_fold_and_global_state_cap() {
        let sequences = vec![
            vec![PieceKind::I, PieceKind::O],
            vec![PieceKind::O, PieceKind::I],
            vec![PieceKind::I, PieceKind::I],
        ];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(2),
            sequences,
            [0.1, 0.2, 0.7]
                .map(|weight| ProbabilityValue::new(weight).unwrap())
                .to_vec(),
            3,
            true,
            None,
        )
        .unwrap();
        let run = |workers, max_total_states| {
            crate::search_boundary_recovery_population(
                &universe,
                &ExecutionControl::default().with_parallelism(workers),
                crate::BoundaryRecoveryPopulationLimits {
                    max_pattern_evaluations: None,
                    max_total_states,
                },
                |_, sequence| {
                    Ok(Some(BoundaryRecoveryQuery {
                        queue: sequence.to_vec(),
                        ..query()
                    }))
                },
            )
            .unwrap()
        };
        let baseline = run(1, None);
        for workers in [1, 2, 4, 8] {
            let actual = run(workers, None);
            assert!(actual.complete);
            assert_eq!(actual.normal_probability, baseline.normal_probability);
            assert_eq!(actual.no_path_probability, baseline.no_path_probability);
            assert_eq!(actual.unknown_probability, baseline.unknown_probability);
            assert_eq!(actual.normal_count, baseline.normal_count);
            assert_eq!(actual.no_path_count, baseline.no_path_count);
            assert_eq!(
                actual
                    .normal_example
                    .as_ref()
                    .map(|x| (&x.0, &x.1, &x.2.steps)),
                baseline
                    .normal_example
                    .as_ref()
                    .map(|x| (&x.0, &x.1, &x.2.steps))
            );
            let limited = run(workers, Some(1));
            assert!(!limited.complete);
            assert_eq!(limited.state_count, 1);
            assert_eq!(limited.evaluated_pattern_count, 1);
            assert_eq!(limited.no_path_count, 0);
            assert_eq!(limited.incomplete_count, 1);
        }
    }
}
