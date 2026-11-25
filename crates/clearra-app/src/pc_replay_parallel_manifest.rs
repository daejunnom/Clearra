//! Native driver for the same source-bound replay manifest used by the host.
//! Each wave owns only a bounded count receipt buffer. Immutable graphs are
//! shared, and every simultaneous language workspace enters one memory ledger.
use super::*;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

struct LiveMemory {
    bytes: u128,
    maximum: u128,
}

struct LanguageLease<'a> {
    owner: &'a Mutex<LiveMemory>,
    bytes: u128,
    failure: Option<PcReplayPageError>,
}

impl LanguageLease<'_> {
    fn admit(&mut self, bytes: u128) -> Result<(), ExactReplayMaterializationError> {
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| ExactReplayMaterializationError::InvalidEvidence)?;
        let required = owner
            .bytes
            .checked_sub(self.bytes)
            .and_then(|n| n.checked_add(bytes))
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        if required > owner.maximum {
            self.failure = Some(PcReplayPageError::MemoryLimit {
                required,
                maximum: owner.maximum,
            });
            return Err(ExactReplayMaterializationError::MemoryLimitExceeded {
                required_memory_bytes: required,
                max_memory_bytes: owner.maximum,
            });
        }
        owner.bytes = required;
        self.bytes = bytes;
        Ok(())
    }

    fn map<T>(&mut self, result: Result<T, ExactReplayMaterializationError>) -> ReplayResult<T> {
        result.map_err(|error| {
            self.failure
                .take()
                .unwrap_or_else(|| replay_engine_error(error, 0))
        })
    }
}

impl Drop for LanguageLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut owner) = self.owner.lock() {
            owner.bytes -= self.bytes;
        }
    }
}

fn parallel_error(error: ParallelWorkError<PcReplayPageError>) -> PcReplayPageError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => "complete_replay_cancelled".into(),
        ParallelWorkError::ThreadSpawnFailed => "complete_replay_allocation_failed".into(),
        ParallelWorkError::WorkerPanicked => "complete_replay_worker_failed".into(),
    }
}

impl PcReplaySourceBuildSession {
    /// The request thread only coordinates. The existing compute worker drives
    /// bounded waves and may use its own slot in a nested shared-budget call.
    pub(crate) fn complete_parallel(
        self,
        control: &ExecutionControl,
    ) -> ReplayResult<Arc<PcReplayPageSource>> {
        let session = Mutex::new(Some(self));
        let output = Mutex::new(None);
        try_for_each_chunk(
            control,
            1,
            1,
            || Ok::<_, PcReplayPageError>(()),
            |_, _| {
                let mut session = session
                    .lock()
                    .map_err(|_| PcReplayPageError::from("complete_replay_worker_failed"))?
                    .take()
                    .ok_or("complete_replay_worker_failed")?;
                session.drain_count_waves(control)?;
                *output
                    .lock()
                    .map_err(|_| PcReplayPageError::from("complete_replay_worker_failed"))? =
                    Some(session.complete()?);
                Ok(())
            },
        )
        .map_err(parallel_error)?;
        output
            .into_inner()
            .map_err(|_| PcReplayPageError::from("complete_replay_worker_failed"))?
            .ok_or_else(|| "complete_replay_worker_failed".into())
    }

    fn drain_count_waves(&mut self, control: &ExecutionControl) -> ReplayResult<()> {
        // An already-started cooperative cursor can finish its current cell
        // before entering a wave; no state or completed count is discarded.
        while self.language.is_some() {
            self.advance(1, control)?;
        }
        let patterns = self.source.materialized_pattern_count;
        let cells = self.pending.len().checked_mul(patterns).ok_or(OVERFLOW)?;
        let wave_capacity = control
            .parallelism()
            .max(1)
            .checked_mul(32)
            .ok_or(OVERFLOW)?;
        while self.next_geometry < self.pending.len() {
            if control.is_cancelled() {
                return Err("complete_replay_cancelled".into());
            }
            self.ready_counts = VecDeque::new();
            let first = self
                .next_geometry
                .checked_mul(patterns)
                .and_then(|n| n.checked_add(self.next_pattern))
                .ok_or(OVERFLOW)?;
            let capacity = wave_capacity.min(cells - first);
            let workspace_headers = bytes::<LanguageLease<'_>>(control.parallelism())?
                .checked_add(core::mem::size_of::<Mutex<LiveMemory>>() as u128)
                .and_then(|n| {
                    n.checked_add(core::mem::size_of::<Mutex<VecDeque<(usize, usize)>>>() as u128)
                })
                .ok_or(OVERFLOW)?;
            self.guard_additional(
                bytes::<(usize, usize)>(capacity)?
                    .checked_add(workspace_headers)
                    .ok_or(OVERFLOW)?,
            )?;
            let mut counts = VecDeque::new();
            counts
                .try_reserve_exact(capacity)
                .map_err(|_| "complete_replay_allocation_failed")?;
            let mut geometry = self.next_geometry;
            let mut pattern = self.next_pattern;
            while counts.len() < capacity && geometry < self.pending.len() {
                if control.is_cancelled() {
                    return Err("complete_replay_cancelled".into());
                }
                match self
                    .source
                    .next_possible_pattern(&self.pending[geometry].1, pattern)?
                {
                    Some(next) => {
                        counts.push_back((geometry * patterns + next, 0));
                        pattern = next + 1;
                    }
                    None => {
                        geometry += 1;
                        pattern = 0;
                    }
                }
            }
            if counts.is_empty() {
                // Every remaining cell was rejected by the original source's
                // necessary-supply proof. No successful queue was sampled away.
                while self.next_geometry < self.pending.len() {
                    self.finish_geometry()?;
                }
                break;
            }
            let count = counts.len();
            let proved_empty = counts.back().expect("nonempty wave").0 + 1 - first - count;
            let baseline = self
                .checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(self.source.retained_external_bytes()?))
                .and_then(|n| n.checked_add(bytes::<(usize, usize)>(counts.capacity()).ok()?))
                .and_then(|n| n.checked_add(workspace_headers))
                .ok_or(OVERFLOW)?;
            ensure_peak(baseline, self.source.maximum_bytes, &mut |_| true)?;
            let owner = Mutex::new(LiveMemory {
                bytes: baseline,
                maximum: self.source.maximum_bytes,
            });
            let counts = Mutex::new(counts);
            let completed = AtomicUsize::new(0);
            try_for_each_chunk(
                control,
                count,
                1,
                || {
                    Ok::<_, PcReplayPageError>(LanguageLease {
                        owner: &owner,
                        bytes: 0,
                        failure: None,
                    })
                },
                |lease, tasks| {
                    for offset in tasks {
                        let ordinal = counts.lock().map_err(|_| {
                            PcReplayPageError::from("complete_replay_worker_failed")
                        })?[offset]
                            .0;
                        let geometry = ordinal / patterns;
                        let pattern = ordinal % patterns;
                        let locations = &self.pending[geometry].1;
                        // The copied location index belongs to this one
                        // language. Its allocation is admitted before reserve.
                        let initial = bytes::<ExactReplayGraphLocation>(locations.len())?
                            .checked_add(core::mem::size_of::<ReplayLanguage>() as u128)
                            .ok_or(OVERFLOW)?;
                        let admission = lease.admit(initial);
                        lease.map(admission)?;
                        let mut owned_locations = Vec::new();
                        owned_locations
                            .try_reserve_exact(locations.len())
                            .map_err(|_| "complete_replay_allocation_failed")?;
                        owned_locations.extend_from_slice(locations);
                        let admission = lease.admit(
                            bytes::<ExactReplayGraphLocation>(owned_locations.capacity())?
                                .checked_add(core::mem::size_of::<ReplayLanguage>() as u128)
                                .ok_or(OVERFLOW)?,
                        );
                        lease.map(admission)?;
                        let language = self.source.batches.new_language(
                            owned_locations,
                            pattern,
                            ExactReplayMaterializationLimits::new(
                                MAX_GEOMETRY_EXECUTIONS,
                                256,
                                self.source.maximum_bytes,
                            ),
                            &mut |peak| lease.admit(peak),
                        );
                        let mut language = lease.map(language)?;
                        loop {
                            let advanced =
                                language.advance(MAX_ADVANCE_WORK, control, &mut |peak| {
                                    lease.admit(peak)
                                });
                            if lease.map(advanced)? {
                                break;
                            }
                        }
                        let witnesses = language
                            .count()
                            .ok_or("pc replay incomplete language count")?;
                        drop(language);
                        let admission = lease.admit(0);
                        lease.map(admission)?;
                        counts.lock().map_err(|_| {
                            PcReplayPageError::from("complete_replay_worker_failed")
                        })?[offset]
                            .1 = witnesses;
                        let done = completed.fetch_add(1, Ordering::AcqRel) + 1;
                        control.report_progress(
                            "execution-replay",
                            (first + proved_empty + done) as u64,
                            Some(cells as u64),
                        );
                    }
                    Ok(())
                },
            )
            .map_err(parallel_error)?;
            self.ready_counts = counts
                .into_inner()
                .map_err(|_| "complete_replay_worker_failed")?;
            // Ordered reduction reuses the cooperative manifest's exact hash,
            // pattern ordinals, first-page witnesses and ownership checks.
            while !self.ready_counts.is_empty() {
                self.advance(1, control)?;
            }
        }
        while !self.advance(1, control)? {}
        Ok(())
    }
}
