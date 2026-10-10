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
    Compact(Arc<[ExactScoringExecutionBatch]>),
    FullHeight(Arc<[FullHeightExecutionBatch]>),
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

trait BatchStorage: Clone {
    fn clone_nested_bytes(&self) -> Option<u128>;
    fn nested_bytes(&self) -> Option<u128>;
    fn graph_count(&self) -> usize;
}
impl BatchStorage for ExactScoringExecutionBatch {
    fn clone_nested_bytes(&self) -> Option<u128> {
        self.checked_clone_nested_bytes()
    }
    fn nested_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()
    }
    fn graph_count(&self) -> usize {
        self.graphs().len()
    }
}
impl BatchStorage for FullHeightExecutionBatch {
    fn clone_nested_bytes(&self) -> Option<u128> {
        self.checked_clone_nested_bytes()
    }
    fn nested_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()
    }
    fn graph_count(&self) -> usize {
        self.execution().graphs().len()
    }
}

fn retained<B: BatchStorage>(input: &[B]) -> Option<u128> {
    input.iter().try_fold(
        bytes::<B>(input.len())
            .ok()?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)?,
        |n, b| n.checked_add(b.nested_bytes()?),
    )
}

fn copy<B: BatchStorage>(
    input: &[B],
    original: u128,
    maximum: u128,
    problem_id_bytes: usize,
) -> ReplayResult<(Arc<[B]>, u128, usize)> {
    let graphs = input
        .iter()
        .try_fold(0_usize, |n, b| n.checked_add(b.graph_count()))
        .ok_or(OVERFLOW)?;
    let clone_peak = input
        .iter()
        .try_fold(
            bytes::<B>(input.len())?.checked_mul(2).ok_or(OVERFLOW)?,
            |n, b| n.checked_add(b.clone_nested_bytes()?),
        )
        .ok_or(OVERFLOW)?;
    let fixed = (core::mem::size_of::<PcReplaySourceBuildSession>()
        + 2 * core::mem::size_of::<usize>()) as u128;
    let index = bytes::<(u64, ExactReplayGraphLocation)>(graphs)?
        .checked_add(bytes::<(u64, Vec<ExactReplayGraphLocation>)>(graphs)?)
        .and_then(|n| n.checked_add(bytes::<ExactReplayGraphLocation>(graphs).ok()?))
        .ok_or(OVERFLOW)?;
    ensure_peak(
        original
            .checked_add(clone_peak)
            .and_then(|n| n.checked_add(fixed))
            .and_then(|n| n.checked_add(index))
            .and_then(|n| n.checked_add(problem_id_bytes as u128))
            .ok_or(OVERFLOW)?,
        maximum,
        &mut |_| true,
    )?;
    let copied = input.to_vec();
    let copied_heap = copied
        .iter()
        .try_fold(bytes::<B>(copied.capacity())?, |n, b| {
            n.checked_add(b.nested_bytes()?)
        })
        .ok_or(OVERFLOW)?;
    // Vec -> Arc overlaps only outer backing stores. Nested graphs are moved,
    // not replicated once again for the shared reader owner.
    ensure_peak(
        original
            .checked_add(copied_heap)
            .and_then(|n| n.checked_add(bytes::<B>(copied.len()).ok()?))
            .and_then(|n| n.checked_add(fixed))
            .ok_or(OVERFLOW)?,
        maximum,
        &mut |_| true,
    )?;
    let copied: Arc<[B]> = copied.into();
    let owned = retained(&copied).ok_or(OVERFLOW)?;
    Ok((copied, owned, graphs))
}

impl ReplayBatches {
    pub(super) fn copy_from_result(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        projection: PcPathProjectionContext,
        patterns: usize,
        original: u128,
        maximum: u128,
    ) -> ReplayResult<(Self, u128, usize)> {
        if problem.visible_height() > 6 {
            let evidence = result
                .pc_full_height_replay_evidence()
                .ok_or("pc replay full-height producer proof missing")?;
            if !evidence.matches_result_source(problem, result.normalized_solution_keys())
                || patterns != evidence.batch().execution().patterns().len()
            {
                return Err("pc replay full-height source is not bound to the query".into());
            }
            let (copied, bytes, count) = copy(
                core::slice::from_ref(evidence.batch()),
                original,
                maximum,
                problem.problem_id().as_str().len(),
            )?;
            Ok((Self::FullHeight(copied), bytes, count))
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
            let (copied, bytes, count) = copy(
                input,
                original,
                maximum,
                problem.problem_id().as_str().len(),
            )?;
            Ok((Self::Compact(copied), bytes, count))
        }
    }

    pub(super) fn push_flat_index(&self, output: &mut Vec<(u64, ExactReplayGraphLocation)>) {
        match self {
            Self::Compact(batches) => {
                for (batch, source) in batches.iter().enumerate() {
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
            Self::Compact(b) => retained(b),
            Self::FullHeight(b) => retained(b),
        }
    }
    pub(super) fn source_hasher(&self, problem: &SearchProblem) -> ReplayResult<Sha256> {
        Ok(match self {
            Self::Compact(b) => {
                crate::pc_replay_source_digest::pc_replay_source_hasher(problem, b)?
            }
            Self::FullHeight(b) => {
                crate::pc_replay_source_digest::full_height_pc_replay_source_hasher(problem, b)?
            }
        })
    }
    pub(super) fn matches_result(&self, result: &CoreExecutionResult) -> bool {
        match self {
            Self::Compact(b) => b.as_ref() == result.exact_scoring_execution_batches(),
            Self::FullHeight(b) => result.pc_full_height_replay_evidence().is_some_and(|e| {
                b.as_ref() == core::slice::from_ref(e.batch())
                    && e.matches_source_keys(result.normalized_solution_keys())
            }),
        }
    }
    pub(super) fn new_language(
        &self,
        locations: Vec<ExactReplayGraphLocation>,
        pattern: usize,
        limits: ExactReplayMaterializationLimits,
        guard: &mut impl FnMut(u128) -> Result<(), ExactReplayMaterializationError>,
    ) -> Result<ReplayLanguage, ExactReplayMaterializationError> {
        match self {
            Self::Compact(b) => {
                let extra = cursor_extra::<ExactReplayLanguageSession>();
                let limits = cursor_limits(limits, extra)?;
                ExactReplayLanguageSession::new(
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
                FullHeightReplayLanguageSession::new(
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
