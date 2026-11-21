//! One immutable compact OR four-word graph owner for the shared pager.
//! Dispatch is per cell/member, never a larger enum in solver-hot-loop states.
use super::*;
use crate::pc_path_result::{
    checked_full_height_execution_projection_peak_bytes, project_full_height_execution,
};
use clearra_postprocess::{FullHeightCandidateExecution, FullHeightReplayLanguageSession};
use clearra_replay::FullHeightExecutionBatch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ReplayBatches {
    Compact(Option<Arc<Vec<ExactScoringExecutionBatch>>>),
    FullHeight(Arc<[FullHeightExecutionBatch]>),
}

pub(super) struct PreparedReplayBatches {
    pub batches: ReplayBatches,
    pub retained_graph_bytes: u128,
    pub graph_count: usize,
    /// Already counted in the exact source Core. No credit for separately
    /// retained request/host owners.
    pub shared_input_graph_bytes: u128,
}

#[derive(Debug)]
pub(super) enum ReplayLanguage {
    Compact(ExactReplayLanguageSession),
    FullHeight(FullHeightReplayLanguageSession),
}

// A single selected trace is already admitted by the language cursor. Keep the
// move inline and include this enum's size in the subsequent projection guard.
#[allow(clippy::large_enum_variant)]
pub(super) enum SelectedReplay {
    Compact(CorePostProcessExecution),
    FullHeight {
        candidate: u64,
        execution: FullHeightCandidateExecution,
    },
}

impl ReplayBatches {
    pub(super) fn single_compact_batch(&self) -> Option<&ExactScoringExecutionBatch> {
        let [batch] = compact_slice(match self {
            Self::Compact(batches) => batches,
            Self::FullHeight(_) => return None,
        }) else {
            return None;
        };
        Some(batch)
    }

    pub(super) fn prepare_from_result(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        projection: PcPathProjectionContext,
        patterns: usize,
    ) -> ReplayResult<PreparedReplayBatches> {
        if problem.visible_height() > 6 {
            let evidence = result
                .pc_full_height_replay_evidence()
                .ok_or("pc replay full-height producer proof missing")?;
            if !evidence.matches_result_source(problem, result.normalized_solution_keys())
                || patterns != evidence.batch().execution().patterns().len()
            {
                return Err("pc replay full-height source is not bound to the query".into());
            }
            let bytes = evidence
                .checked_shared_batch_retained_bytes()
                .ok_or(OVERFLOW)?;
            Ok(PreparedReplayBatches {
                batches: Self::FullHeight(Arc::clone(evidence.shared_batch_owner())),
                retained_graph_bytes: bytes,
                graph_count: evidence.batch().execution().graphs().len(),
                shared_input_graph_bytes: bytes,
            })
        } else {
            let input = result.exact_scoring_execution_batches();
            if result.pc_full_height_replay_evidence().is_some()
                || (input.is_empty()
                    && !(result.bool_field("count_complete") == Some(true)
                        && result.usize_field("solution_count") == Some(0)
                        && result.bool_field("solution_found") == Some(false)))
            {
                return Err(
                    "pc replay execution graph is missing or its empty result is unproven".into(),
                );
            }
            for batch in input {
                if !batch.complete()
                    || Some(batch.initial_occupied()) != projection.initial_board.compact()
                    || usize::from(batch.initial_cursor()) != projection.initial_cursor
                    || batch.initial_hold() != projection.initial_hold
                    || batch.patterns().len() != patterns
                {
                    return Err("pc replay graph source does not match the query".into());
                }
            }
            let bytes = result
                .checked_exact_scoring_execution_batches_retained_bytes()
                .ok_or(OVERFLOW)?;
            let count = input
                .iter()
                .try_fold(0_usize, |count, batch| {
                    count.checked_add(batch.graphs().len())
                })
                .ok_or(OVERFLOW)?;
            Ok(PreparedReplayBatches {
                batches: Self::Compact(result.exact_scoring_execution_batch_owner().cloned()),
                retained_graph_bytes: bytes,
                graph_count: count,
                shared_input_graph_bytes: bytes,
            })
        }
    }

    pub(super) fn push_flat_index(&self, output: &mut Vec<(u64, ExactReplayGraphLocation)>) {
        match self {
            Self::Compact(batches) => {
                for (batch, source) in compact_slice(batches).iter().enumerate() {
                    for (graph, value) in source.graphs().iter().enumerate() {
                        output.push((
                            value.candidate_id(),
                            ExactReplayGraphLocation { batch, graph },
                        ));
                    }
                }
            }
            Self::FullHeight(batches) => {
                for (batch, source) in batches.iter().enumerate() {
                    for (graph, value) in source.execution().graphs().iter().enumerate() {
                        output.push((
                            value.candidate_id(),
                            ExactReplayGraphLocation { batch, graph },
                        ));
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(None) => Some(0),
            Self::Compact(Some(b)) => b.iter().try_fold(
                bytes::<ExactScoringExecutionBatch>(b.capacity())
                    .ok()?
                    .checked_add(
                        (2 * core::mem::size_of::<usize>()
                            + core::mem::size_of::<Vec<ExactScoringExecutionBatch>>())
                            as u128,
                    )?,
                |sum, batch| sum.checked_add(batch.checked_nested_retained_bytes()?),
            ),
            Self::FullHeight(b) => b.iter().try_fold(
                bytes::<FullHeightExecutionBatch>(b.len())
                    .ok()?
                    .checked_add((2 * core::mem::size_of::<usize>()) as u128)?,
                |sum, batch| sum.checked_add(batch.checked_nested_retained_bytes()?),
            ),
        }
    }
    pub(super) fn source_hasher(&self, problem: &SearchProblem) -> ReplayResult<Sha256> {
        Ok(match self {
            Self::Compact(b) => {
                crate::pc_replay_source_digest::pc_replay_source_hasher(problem, compact_slice(b))?
            }
            Self::FullHeight(b) => {
                crate::pc_replay_source_digest::full_height_pc_replay_source_hasher(problem, b)?
            }
        })
    }
    pub(super) fn matches_result(&self, result: &CoreExecutionResult) -> bool {
        match self {
            Self::Compact(b) => match (b, result.exact_scoring_execution_batch_owner()) {
                (Some(page), Some(core)) => Arc::ptr_eq(page, core),
                (None, None) => true,
                _ => false,
            },
            Self::FullHeight(b) => result.pc_full_height_replay_evidence().is_some_and(|e| {
                // The staged pager's memory reservation credited this exact
                // allocation. Equal content from a separately retained batch
                // cannot inherit that overlap credit at promotion.
                Arc::ptr_eq(b, e.shared_batch_owner())
                    && e.matches_source_keys(result.normalized_solution_keys())
            }),
        }
    }
    pub(super) fn new_language(
        &self,
        locations: ExactReplayGraphLocations,
        pattern: usize,
        limits: ExactReplayMaterializationLimits,
        guard: &mut impl FnMut(u128) -> Result<(), ExactReplayMaterializationError>,
    ) -> Result<ReplayLanguage, ExactReplayMaterializationError> {
        match self {
            Self::Compact(None) => Err(ExactReplayMaterializationError::InvalidEvidence),
            Self::Compact(Some(b)) => {
                let extra = cursor_extra::<ExactReplayLanguageSession>();
                let limits = cursor_limits(limits, extra)?;
                ExactReplayLanguageSession::new_shared_batches(
                    Arc::clone(b),
                    locations,
                    pattern,
                    limits,
                    &mut |peak| cursor_guard(peak, extra, guard),
                )
                .map(ReplayLanguage::Compact)
            }
            Self::FullHeight(b) => {
                let extra = cursor_extra::<FullHeightReplayLanguageSession>();
                let limits = cursor_limits(limits, extra)?;
                FullHeightReplayLanguageSession::new_with_locations(
                    Arc::clone(b),
                    locations,
                    pattern,
                    limits,
                    &mut |peak| cursor_guard(peak, extra, guard),
                )
                .map(ReplayLanguage::FullHeight)
            }
        }
    }
}

fn compact_slice(
    batches: &Option<Arc<Vec<ExactScoringExecutionBatch>>>,
) -> &[ExactScoringExecutionBatch] {
    batches.as_ref().map_or(&[], |batches| batches.as_slice())
}

fn cursor_extra<C>() -> u128 {
    (core::mem::size_of::<ReplayLanguage>() - core::mem::size_of::<C>()) as u128
}
fn cursor_limits(
    limits: ExactReplayMaterializationLimits,
    extra: u128,
) -> Result<ExactReplayMaterializationLimits, ExactReplayMaterializationError> {
    let cap = limits.max_retained_bytes().checked_sub(extra).ok_or(
        ExactReplayMaterializationError::MemoryLimitExceeded {
            required_memory_bytes: extra,
            max_memory_bytes: limits.max_retained_bytes(),
        },
    )?;
    Ok(ExactReplayMaterializationLimits::new(
        limits.max_executions(),
        limits.max_path_steps(),
        cap,
    ))
}
fn cursor_guard(
    peak: u128,
    extra: u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactReplayMaterializationError>,
) -> Result<(), ExactReplayMaterializationError> {
    guard(
        peak.checked_add(extra)
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?,
    )
}

impl ReplayLanguage {
    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        let cursor = match self {
            Self::Compact(l) => l
                .checked_retained_bytes()?
                .checked_sub(core::mem::size_of::<ExactReplayLanguageSession>() as u128)?,
            Self::FullHeight(l) => l
                .checked_retained_bytes()?
                .checked_sub(core::mem::size_of::<FullHeightReplayLanguageSession>() as u128)?,
        };
        cursor.checked_add(core::mem::size_of::<Self>() as u128)
    }
    pub(super) fn count(&self) -> Option<usize> {
        match self {
            Self::Compact(l) => l.count(),
            Self::FullHeight(l) => l.count(),
        }
    }
    #[cfg(test)]
    pub(super) fn work_units(&self) -> u64 {
        match self {
            Self::Compact(l) => l.work_units(),
            Self::FullHeight(l) => l.work_units(),
        }
    }
    pub(super) fn advance(
        &mut self,
        work: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> Result<(), ExactReplayMaterializationError>,
    ) -> Result<bool, ExactReplayMaterializationError> {
        match self {
            Self::Compact(l) => l.advance(work, control, &mut |p| {
                cursor_guard(p, cursor_extra::<ExactReplayLanguageSession>(), guard)
            }),
            Self::FullHeight(l) => l.advance(work, control, &mut |p| {
                cursor_guard(p, cursor_extra::<FullHeightReplayLanguageSession>(), guard)
            }),
        }
    }
    pub(super) fn select(
        &self,
        candidate: u64,
        rank: usize,
        control: &ExecutionControl,
        external: u128,
        guard: &mut impl FnMut(u128) -> Result<(), ExactReplayMaterializationError>,
    ) -> ReplayResult<SelectedReplay> {
        Ok(match self {
            Self::Compact(l) => SelectedReplay::Compact(into_core(
                candidate,
                l.select(rank, control, &mut |p| {
                    cursor_guard(p, cursor_extra::<ExactReplayLanguageSession>(), guard)
                })
                .map_err(|e| replay_engine_error(e, external))?,
            )?),
            Self::FullHeight(l) => SelectedReplay::FullHeight {
                candidate,
                execution: l
                    .select(rank, control, &mut |p| {
                        cursor_guard(p, cursor_extra::<FullHeightReplayLanguageSession>(), guard)
                    })
                    .map_err(|e| replay_engine_error(e, external))?,
            },
        })
    }
}

impl SelectedReplay {
    pub(super) fn checked_owned_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128).checked_add(match self {
            Self::Compact(e) => e.checked_nested_retained_bytes()?,
            Self::FullHeight { execution, .. } => execution.checked_nested_retained_bytes()?,
        })
    }
    pub(super) fn checked_projection_bytes(&self) -> Option<u128> {
        match self {
            Self::Compact(e) => checked_execution_projection_peak_bytes(e),
            Self::FullHeight { execution, .. } => {
                checked_full_height_execution_projection_peak_bytes(execution)
            }
        }
    }
    pub(super) fn project(
        &self,
        context: PcPathProjectionContext,
        patterns: usize,
        candidate: u64,
    ) -> ReplayResult<PcPathWitnessV2> {
        Ok(match self {
            Self::Compact(e) => project_execution_with_context(context, e, patterns, candidate)?,
            Self::FullHeight {
                candidate: producer,
                execution,
            } => project_full_height_execution(context, *producer, execution, patterns, candidate)?,
        })
    }
}
