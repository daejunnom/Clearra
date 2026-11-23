//! Bounded observations of completed candidate verifiers in native diagnostics.
//! This module and its result hook are absent from ordinary product builds.

use clearra_core_domain::execution_cancellation::{ExecutionCancellationHandle, ExecutionControl};
use clearra_problem::SearchProblem;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use super::buildup::CandidateBuildResult;
use super::catalog::GeometryCatalog;
use super::geometry::GeometryCandidate;

const SAMPLES_PER_OUTCOME: usize = 16;
const OUTCOMES: [&str; 3] = [
    "buildable-under-actual-source",
    "realization-feasibility-rejected",
    "no-accepted-physical-source-order",
];
const WORK_FIELDS: [&str; 5] = [
    "graph_nodes",
    "reachability_states",
    "feasibility_states",
    "coverage_product_states",
    "coverage_product_edge_checks",
];

static ACTIVE: AtomicBool = AtomicBool::new(false);
static OWNER: Mutex<Option<Weak<FeedbackState>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorCandidateFeedbackError {
    AlreadyActive,
    EmptyWindow,
}

/// Owns one explicitly requested diagnostic window. Finish only after the
/// associated search and its workers have returned. Other cancellation owners
/// are excluded even when they execute concurrently in the same process.
pub struct ExecutorCandidateFeedbackSession {
    state: Arc<FeedbackState>,
}

impl ExecutorCandidateFeedbackSession {
    pub fn start(
        requested: u64,
        cancellation: ExecutionCancellationHandle,
    ) -> Result<Self, ExecutorCandidateFeedbackError> {
        if requested == 0 {
            return Err(ExecutorCandidateFeedbackError::EmptyWindow);
        }
        let mut owner = OWNER.lock().expect("candidate feedback owner");
        if owner.as_ref().and_then(Weak::upgrade).is_some() {
            return Err(ExecutorCandidateFeedbackError::AlreadyActive);
        }
        let state = Arc::new(FeedbackState {
            requested,
            cancellation,
            completed: AtomicU64::new(0),
            saturated: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            stopped_at: Mutex::new(None),
            outcomes: std::array::from_fn(|_| OutcomeTotals::default()),
        });
        *owner = Some(Arc::downgrade(&state));
        ACTIVE.store(true, Ordering::Release);
        Ok(Self { state })
    }

    pub fn cancellation_requested_at(&self) -> Option<Instant> {
        *self
            .state
            .stopped_at
            .lock()
            .expect("candidate feedback stop")
    }

    pub fn finish(self) -> Value {
        self.state.report()
    }
}

impl Drop for ExecutorCandidateFeedbackSession {
    fn drop(&mut self) {
        let mut owner = OWNER.lock().expect("candidate feedback owner");
        if owner
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|state| Arc::ptr_eq(&state, &self.state))
        {
            ACTIVE.store(false, Ordering::Release);
            *owner = None;
        }
    }
}

struct StoredSample {
    ordinal: u64,
    value: Value,
}

struct OutcomeTotals {
    completed: AtomicU64,
    work: [AtomicU64; WORK_FIELDS.len()],
    sample_ceiling: AtomicU64,
    samples: Mutex<Vec<StoredSample>>,
}

impl Default for OutcomeTotals {
    fn default() -> Self {
        Self {
            completed: AtomicU64::new(0),
            work: std::array::from_fn(|_| AtomicU64::new(0)),
            sample_ceiling: AtomicU64::new(u64::MAX),
            samples: Mutex::new(Vec::new()),
        }
    }
}

struct FeedbackState {
    requested: u64,
    cancellation: ExecutionCancellationHandle,
    completed: AtomicU64,
    saturated: AtomicBool,
    stop_requested: AtomicBool,
    stopped_at: Mutex<Option<Instant>>,
    outcomes: [OutcomeTotals; OUTCOMES.len()],
}

impl FeedbackState {
    fn record(
        &self,
        outcome: usize,
        work: [u64; WORK_FIELDS.len()],
        sample: impl FnOnce(u64) -> Value,
    ) {
        let ordinal = add_exact(&self.completed, 1, &self.saturated);
        let totals = &self.outcomes[outcome];
        add_exact(&totals.completed, 1, &self.saturated);
        for (counter, amount) in totals.work.iter().zip(work) {
            add_exact(counter, amount, &self.saturated);
        }
        // Keep the first ordinals even if a worker is descheduled between
        // receiving its ordinal and publishing its sample. Once full, later
        // completions avoid both row copies and the sample mutex.
        if ordinal <= totals.sample_ceiling.load(Ordering::Relaxed) {
            let mut samples = totals.samples.lock().expect("candidate feedback samples");
            let index = samples.partition_point(|entry| entry.ordinal < ordinal);
            if index < SAMPLES_PER_OUTCOME {
                if samples.len() == SAMPLES_PER_OUTCOME {
                    samples.pop();
                }
                samples.insert(
                    index,
                    StoredSample {
                        ordinal,
                        value: sample(ordinal),
                    },
                );
                if samples.len() == SAMPLES_PER_OUTCOME {
                    totals.sample_ceiling.store(
                        samples.last().expect("full diagnostic samples").ordinal,
                        Ordering::Relaxed,
                    );
                }
            }
        }
        if ordinal >= self.requested
            && self
                .stop_requested
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            *self.stopped_at.lock().expect("candidate feedback stop") = Some(Instant::now());
            self.cancellation.cancel();
        }
    }

    fn report(&self) -> Value {
        let completed = self.completed.load(Ordering::Relaxed);
        let saturated = self.saturated.load(Ordering::Relaxed);
        let outcomes: Vec<_> = self.outcomes.iter().enumerate().map(|(index, totals)| {
            json!({
                "outcome": OUTCOMES[index],
                "completed_verifier_calls": totals.completed.load(Ordering::Relaxed),
                "work": work_json(std::array::from_fn(|slot| totals.work[slot].load(Ordering::Relaxed))),
            })
        }).collect();
        let mut samples: Vec<_> = self
            .outcomes
            .iter()
            .flat_map(|totals| {
                totals
                    .samples
                    .lock()
                    .expect("candidate feedback samples")
                    .iter()
                    .map(|sample| (sample.ordinal, sample.value.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        samples.sort_unstable_by_key(|(ordinal, _)| *ordinal);
        json!({
            "requested_completed_verifier_calls": self.requested,
            "completed_verifier_calls": completed,
            "requested_window_reached": self.stop_requested.load(Ordering::Acquire),
            "inflight_completions_beyond_window": completed.saturating_sub(self.requested),
            "retained_samples_per_outcome": SAMPLES_PER_OUTCOME,
            "counters_saturated": saturated,
            "counts_exact": !saturated,
            "outcomes": outcomes,
            "samples": samples.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
            "complete": false,
            "counter_contract": if saturated {
                "one or more counters saturated; recorded unsampled counts and sums are lower bounds, not exact totals"
            } else {
                "exact unsampled counts and sums of completed verifier fields"
            },
            "acceptance_contract": "existing verifier verdict after actual physical/source/Hold/EOF rules; no-accepted-order does not isolate physical failure from source failure; feasibility rejection is a safe negative condition",
            "sampling_contract": "first completed calls in each verdict class, scheduling dependent; neither uniform random sampling nor family path rank; repeated canonical identities remain repeated verifier calls",
            "trace_contract": "only already-produced representative evidence is retained; an empty trace does not mean no witness and no extra BuildUp is requested for sampling",
            "work_contract": "unsampled sums of completed CandidateBuildResult fields; exact unless counters_saturated; these fields are stage-defined work, not instructions or elapsed time; optional feedback atomics and retained-row copies are diagnostic overhead",
        })
    }
}

fn add_exact(counter: &AtomicU64, amount: u64, saturated: &AtomicBool) -> u64 {
    let previous = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(amount))
        })
        .expect("saturating diagnostic update always succeeds");
    match previous.checked_add(amount) {
        Some(total) => total,
        None => {
            saturated.store(true, Ordering::Relaxed);
            u64::MAX
        }
    }
}

fn active_state(control: &ExecutionControl) -> Option<Arc<FeedbackState>> {
    if !ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let state = OWNER
        .lock()
        .expect("candidate feedback owner")
        .as_ref()?
        .upgrade()?;
    std::ptr::eq(
        state.cancellation.atomic_flag(),
        control.cancellation.atomic_flag(),
    )
    .then_some(state)
}

fn work_json(work: [u64; WORK_FIELDS.len()]) -> Value {
    Value::Object(
        WORK_FIELDS
            .into_iter()
            .zip(work)
            .map(|(name, amount)| (name.to_owned(), json!(amount)))
            .collect(),
    )
}

pub(super) fn observe_completed(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    result: &CandidateBuildResult,
    control: &ExecutionControl,
) {
    let Some(state) = active_state(control) else {
        return;
    };
    let outcome = if result.buildable {
        0
    } else if result.feasibility_rejected {
        1
    } else {
        2
    };
    let work = [
        result.graph_nodes,
        result.reachability_states,
        result.feasibility_states,
        result.coverage_product_states,
        result.coverage_product_edge_checks,
    ]
    .map(|value| {
        u64::try_from(value).unwrap_or_else(|_| {
            state.saturated.store(true, Ordering::Relaxed);
            u64::MAX
        })
    });
    state.record(outcome, work, |ordinal| {
        let placements: Vec<_> = candidate.row_ids().iter().map(|&id| {
            let row = catalog.skeleton(id);
            json!({ "catalog_row_id": id, "piece": row.piece.as_ascii().to_string(),
                "target_frame_cells": format!("0x{:016x}", row.cells) })
        }).collect();
        let original_queue = result.witness_pattern_id.and_then(|id| {
            let universe = problem.piece_source().materialized_universe()?;
            Some(universe.try_sequence_at(id as usize)?.iter().map(|piece| piece.as_ascii()).collect::<String>())
        });
        let trace: Vec<_> = result.representative_path.iter().map(|step| json!({
            "piece": step.piece().as_ascii().to_string(), "rotation": step.rotation(),
            "x": step.x(), "y": step.y(), "hold": step.hold(), "cleared_lines": step.cleared_lines(),
        })).collect();
        json!({
            "completed_ordinal": ordinal, "outcome": OUTCOMES[outcome],
            "candidate_count_complete": result.count_complete,
            "identity_bucket_hash": format!("{:016x}", candidate.identity.bucket_hash()),
            "target_index": candidate.target_index,
            "initial_board_mask": format!("0x{:016x}", catalog.initial_board()),
            "placements": placements, "witness_pattern_id": result.witness_pattern_id,
            "witness_original_queue": original_queue, "existing_representative_trace": trace,
            "work": work_json(work),
        })
    });
}

#[cfg(test)]
mod tests {
    use super::{
        active_state, ExecutorCandidateFeedbackError, ExecutorCandidateFeedbackSession,
        FeedbackState, OutcomeTotals, SAMPLES_PER_OUTCOME,
    };
    use clearra_core_domain::execution_cancellation::ExecutionControl;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    #[test]
    fn completed_feedback_is_request_owned_exact_and_bounded() {
        let control = ExecutionControl::default();
        let other = ExecutionControl::default();
        assert!(matches!(
            ExecutorCandidateFeedbackSession::start(0, control.cancellation.handle()),
            Err(ExecutorCandidateFeedbackError::EmptyWindow)
        ));
        let session =
            ExecutorCandidateFeedbackSession::start(200, control.cancellation.handle()).unwrap();
        assert!(matches!(
            ExecutorCandidateFeedbackSession::start(1, other.cancellation.handle()),
            Err(ExecutorCandidateFeedbackError::AlreadyActive)
        ));
        assert!(active_state(&other).is_none());
        let state = active_state(&control).unwrap();
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let state = &state;
                scope.spawn(move || {
                    for _ in 0..32 {
                        state.record(worker % 2, [1, 2, 3, 4, 5], |ordinal|
                            json!({ "completed_ordinal": ordinal, "outcome": worker % 2 }));
                    }
                });
            }
        });
        assert!(control.is_cancelled());
        assert!(!other.is_cancelled());
        assert!(session.cancellation_requested_at().is_some());
        let report = session.finish();
        assert_eq!(report["completed_verifier_calls"], 256);
        assert_eq!(report["inflight_completions_beyond_window"], 56);
        assert_eq!(report["requested_window_reached"], true);
        assert_eq!(report["counters_saturated"], false);
        assert_eq!(report["counts_exact"], true);
        assert_eq!(report["complete"], false);
        assert_eq!(
            report["samples"].as_array().unwrap().len(),
            2 * SAMPLES_PER_OUTCOME
        );
        for outcome in &report["outcomes"].as_array().unwrap()[..2] {
            assert_eq!(outcome["completed_verifier_calls"], 128);
            assert_eq!(outcome["work"]["coverage_product_edge_checks"], 640);
        }
        assert!(active_state(&control).is_none());
        let next = ExecutorCandidateFeedbackSession::start(1, other.cancellation.handle()).unwrap();
        assert_eq!(next.finish()["completed_verifier_calls"], 0);
    }

    #[test]
    fn saturated_diagnostic_counters_do_not_claim_exact_totals() {
        let state = FeedbackState {
            requested: 2,
            cancellation: ExecutionControl::default().cancellation.handle(),
            completed: AtomicU64::new(0),
            saturated: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            stopped_at: Mutex::new(None),
            outcomes: std::array::from_fn(|_| OutcomeTotals::default()),
        };
        state.outcomes[0].work[0].store(u64::MAX - 1, Ordering::Relaxed);
        state.record(
            0,
            [2, 0, 0, 0, 0],
            |ordinal| json!({ "completed_ordinal": ordinal }),
        );
        let report = state.report();
        assert_eq!(report["outcomes"][0]["work"]["graph_nodes"], u64::MAX);
        assert_eq!(report["counters_saturated"], true);
        assert_eq!(report["counts_exact"], false);
        assert!(report["counter_contract"]
            .as_str()
            .unwrap()
            .contains("not exact totals"));
    }
}
