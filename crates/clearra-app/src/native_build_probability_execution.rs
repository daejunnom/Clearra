use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    CoreExecutionError, CoreExecutionResult, WasmBuildProbabilityCandidateProducer,
    WasmBuildProbabilityDistributedVerifier, WasmCandidatePacket, WasmCandidateProducerAdvance,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, FinesseMetric, FinessePatternKnowledge,
    SearchProblem,
};

use crate::app_services::AppCoreExecutorService;

const NATIVE_BUILD_BATCH_CAPACITY: usize = 32;

enum WorkerRequest {
    Consume(Vec<WasmCandidatePacket>),
    Finish,
}

enum WorkerResult {
    Consumed(usize),
    Finished(Vec<CoreExecutionResult>),
}

struct WorkerCompletion {
    worker_index: usize,
    result: Result<WorkerResult, &'static str>,
}

pub(crate) fn run_native_build_probability_with_workers(
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    let total_workers = requested_workers.max(2);
    // The coordinator only produces and merges batches; it never verifies a
    // candidate. Requested worker count therefore maps one-to-one to verifier
    // threads instead of reserving a non-participating coordinator slot.
    let worker_threads = total_workers;
    let producer = WasmBuildProbabilityCandidateProducer::new_with_finesse(
        problem,
        field,
        aggregation,
        finesse_metric,
        finesse_pattern_knowledge,
    )
    .map_err(core_error)?;
    let (completion_sender, completion_receiver) = mpsc::channel::<WorkerCompletion>();
    let mut request_senders = Vec::with_capacity(worker_threads);
    let mut handles = Vec::with_capacity(worker_threads);
    for worker_index in 0..worker_threads {
        let verifier = WasmBuildProbabilityDistributedVerifier::new(problem, field, aggregation)
            .map_err(core_error)?;
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let worker_completion_sender = completion_sender.clone();
        let worker_control = control.clone();
        let handle = thread::Builder::new()
            .name(format!("clearra-finesse-build-{worker_index}"))
            .spawn(move || {
                build_probability_worker_main(
                    worker_index,
                    verifier,
                    request_receiver,
                    worker_completion_sender,
                    worker_control,
                );
            })
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_worker_pool_unavailable",
            })?;
        request_senders.push(request_sender);
        handles.push(handle);
    }
    drop(completion_sender);

    let result = drive_native_build_probability(
        service,
        producer,
        total_workers,
        &request_senders,
        &completion_receiver,
        control,
    );
    drop(request_senders);
    let join_result = join_workers(handles);
    match (result, join_result) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn drive_native_build_probability(
    service: AppCoreExecutorService,
    mut producer: WasmBuildProbabilityCandidateProducer,
    total_workers: usize,
    request_senders: &[SyncSender<WorkerRequest>],
    completion_receiver: &Receiver<WorkerCompletion>,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    let mut available = (0..request_senders.len()).collect::<VecDeque<_>>();
    let mut in_flight = 0_usize;
    let mut summary = None;
    while summary.is_none() || in_flight != 0 {
        while summary.is_none() {
            let Some(worker_index) = available.pop_front() else {
                break;
            };
            let mut batch = Vec::with_capacity(NATIVE_BUILD_BATCH_CAPACITY);
            while batch.len() < NATIVE_BUILD_BATCH_CAPACITY && summary.is_none() {
                match producer.advance(control).map_err(core_error)? {
                    WasmCandidateProducerAdvance::Pending => {}
                    WasmCandidateProducerAdvance::Candidate(candidate) => batch.push(candidate),
                    WasmCandidateProducerAdvance::Completed(completed) => {
                        summary = Some(completed);
                    }
                    WasmCandidateProducerAdvance::Cancelled => {
                        return Err(CoreExecutionError::Cancelled)
                    }
                }
            }
            if batch.is_empty() {
                available.push_front(worker_index);
                break;
            }
            let count = batch.len();
            request_senders[worker_index]
                .send(WorkerRequest::Consume(batch))
                .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_pool_unavailable",
                })?;
            in_flight = in_flight.saturating_add(1);
            control.report_progress("build-probability-candidates", count as u64, None);
        }
        if in_flight == 0 {
            if summary.is_some() {
                break;
            }
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_worker_pool_stalled",
            });
        }
        let completion =
            completion_receiver
                .recv()
                .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_pool_unavailable",
                })?;
        match completion.result.map_err(core_error)? {
            WorkerResult::Consumed(count) => {
                if count == 0 {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "wasm_cpu_worker_protocol_invalid",
                    });
                }
                in_flight = in_flight.saturating_sub(1);
                available.push_back(completion.worker_index);
            }
            WorkerResult::Finished(_) => {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_protocol_invalid",
                })
            }
        }
    }

    for sender in request_senders {
        sender
            .send(WorkerRequest::Finish)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_worker_pool_unavailable",
            })?;
    }
    let mut worker_results = Vec::with_capacity(request_senders.len());
    for _ in request_senders {
        let completion =
            completion_receiver
                .recv()
                .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_pool_unavailable",
                })?;
        match completion.result.map_err(core_error)? {
            WorkerResult::Finished(results) => {
                worker_results.push((completion.worker_index, results))
            }
            WorkerResult::Consumed(_) => {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_protocol_invalid",
                })
            }
        }
    }
    worker_results.sort_unstable_by_key(|(worker_index, _)| *worker_index);
    let mut merger = producer.into_merger().map_err(core_error)?;
    for (_, results) in worker_results {
        for result in results {
            let result = service.materialize_distributed_postprocess_partition(result, control)?;
            merger.absorb(&result).map_err(core_error)?;
        }
    }
    merger
        .finish_with_control(
            &summary.ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_worker_geometry_summary_missing",
            })?,
            total_workers,
            control,
        )
        .map(|result| {
            result.with_replaced_fields(vec![(
                "cpu_parallel_decision_reason".to_owned(),
                "native-ready-worker-build-probability-pipeline".to_owned(),
            )])
        })
        .map_err(core_error)
}

fn build_probability_worker_main(
    worker_index: usize,
    mut verifier: WasmBuildProbabilityDistributedVerifier,
    requests: Receiver<WorkerRequest>,
    completions: mpsc::Sender<WorkerCompletion>,
    control: ExecutionControl,
) {
    while let Ok(request) = requests.recv() {
        let result = match request {
            WorkerRequest::Consume(candidates) => {
                let count = candidates.len();
                let result = candidates
                    .iter()
                    .try_for_each(|candidate| verifier.consume(candidate, &control));
                result.map(|()| WorkerResult::Consumed(count))
            }
            WorkerRequest::Finish => verifier.finish().map(WorkerResult::Finished),
        };
        let terminal = matches!(result, Ok(WorkerResult::Finished(_))) || result.is_err();
        if completions
            .send(WorkerCompletion {
                worker_index,
                result,
            })
            .is_err()
            || terminal
        {
            return;
        }
    }
}

fn join_workers(handles: Vec<JoinHandle<()>>) -> Result<(), CoreExecutionError> {
    if handles.into_iter().any(|handle| handle.join().is_err()) {
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "wasm_cpu_worker_panicked",
        })
    } else {
        Ok(())
    }
}

fn core_error(reason: &'static str) -> CoreExecutionError {
    CoreExecutionError::Pc(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{BuildProbabilityFinesseRequest, ProblemCompiler};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    fn one_piece_problem(workers: usize) -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(workers)
                .with_worker_hardware_limit(workers),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece problem")
    }

    #[test]
    fn native_ready_worker_path_matches_serial_finesse_without_nested_searches() {
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target");
        let service = AppCoreExecutorService::wasm_cpu();
        let control = ExecutionControl::default();
        let serial = service
            .execute_build_probability_with_control(
                &one_piece_problem(1),
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Search {
                    pattern_knowledge: FinessePatternKnowledge::Both,
                },
                &control,
            )
            .expect("serial finesse");
        // Invoke the coordinator directly so this test still exercises two
        // workers in CI containers whose native hardware limit is one.
        let parallel = run_native_build_probability_with_workers(
            service,
            &one_piece_problem(2),
            field,
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            2,
            &control,
        )
        .expect("parallel finesse");

        assert_eq!(
            parallel.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(parallel.finesse_report(), serial.finesse_report());
        assert_eq!(parallel.usize_field("workers_used"), Some(2));
        assert_eq!(
            parallel.field("cpu_parallel_decision_reason"),
            Some("native-ready-worker-build-probability-pipeline")
        );
    }
}
