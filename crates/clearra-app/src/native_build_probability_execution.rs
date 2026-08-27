use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::mpsc::{Receiver, SyncSender},
    thread::JoinHandle,
};
#[cfg(test)]
use std::{sync::mpsc, thread};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    core_execution_result::CoreResultFieldReplacementError, CoreExecutionError,
    CoreExecutionResult, WasmBuildProbabilityCandidateProducer,
    WasmBuildProbabilityDistributedResultMerger, WasmBuildProbabilityDistributedVerifier,
    WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuSearchError,
    WasmDistributedGeometrySummary,
};
#[cfg(test)]
use clearra_problem::{
    BuildProbabilityAggregation, FinesseMetric, FinessePatternKnowledge, SearchProblem,
};
use clearra_problem::{BuildProbabilityField, BuildSolutionProbabilityPolicy};

use crate::app_services::AppCoreExecutorService;

#[path = "native_durable_build_probability_execution.rs"]
mod durable;
#[path = "native_build_probability_host_runtime.rs"]
pub(crate) mod host_runtime;
#[path = "native_build_probability_system_provider.rs"]
mod system_provider;

const NATIVE_BUILD_BATCH_CAPACITY: usize = 32;
const NATIVE_BUILD_WORKER_STACK_BYTES: usize = 1024 * 1024;
const NATIVE_BUILD_WORKER_THREAD_NAME_PREFIX: &str = "clearra-finesse-build-";
const NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY: usize = 1;
const NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY: usize = 0;

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

struct NativeBuildProbabilityWorkerOutput {
    summary: WasmDistributedGeometrySummary,
    worker_results: Vec<(usize, Vec<CoreExecutionResult>)>,
}

/// Allocation-free upper bound for memory owned by the native coordinator.
///
/// The producer and delegated verifier states remain charged to their
/// aggregate child admissions. This projection charges only coordinator-owned
/// backing allocations, public channel owners/slots, worker stacks, and thread
/// name payloads. Opaque implementation-private `std::sync::mpsc` control
/// blocks are intentionally not guessed; the application-owned request slot is
/// bounded to one value and completion delivery is rendezvous-only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeCoordinatorMemoryProjection {
    worker_count: usize,
    batch_capacity: usize,
    row_ids_per_candidate: usize,
    candidate_packet_backing_bytes: u128,
    candidate_row_id_payload_bytes: u128,
    worker_stack_bytes: u128,
    request_sender_backing_bytes: u128,
    worker_handle_backing_bytes: u128,
    available_queue_backing_bytes: u128,
    worker_result_backing_bytes: u128,
    request_channel_slot_bytes: u128,
    request_receiver_owner_bytes: u128,
    completion_sender_owner_bytes: u128,
    completion_receiver_owner_bytes: u128,
    completion_channel_slot_bytes: u128,
    container_owner_bytes: u128,
    thread_name_payload_bytes: u128,
    required_peak_bytes: u128,
}

impl NativeCoordinatorMemoryProjection {
    fn checked(field: BuildProbabilityField, worker_count: usize) -> Option<Self> {
        if worker_count == 0 {
            return None;
        }

        let workers = worker_count as u128;
        let row_ids_per_candidate = field.target_piece_count();
        let per_batch_packet_backing =
            checked_heap_backing_bytes::<WasmCandidatePacket>(NATIVE_BUILD_BATCH_CAPACITY)?;
        let per_candidate_row_id_payload =
            checked_heap_backing_bytes::<u32>(row_ids_per_candidate)?;
        let per_batch_row_id_payload =
            (NATIVE_BUILD_BATCH_CAPACITY as u128).checked_mul(per_candidate_row_id_payload)?;
        let candidate_packet_backing_bytes = workers.checked_mul(per_batch_packet_backing)?;
        let candidate_row_id_payload_bytes = workers.checked_mul(per_batch_row_id_payload)?;
        let worker_stack_bytes = workers.checked_mul(NATIVE_BUILD_WORKER_STACK_BYTES as u128)?;
        let request_sender_backing_bytes =
            checked_heap_backing_bytes::<SyncSender<WorkerRequest>>(worker_count)?;
        let worker_handle_backing_bytes =
            checked_heap_backing_bytes::<JoinHandle<()>>(worker_count)?;
        let available_queue_backing_bytes = checked_heap_backing_bytes::<usize>(worker_count)?;
        let worker_result_backing_bytes =
            checked_heap_backing_bytes::<(usize, Vec<CoreExecutionResult>)>(worker_count)?;
        let request_channel_slot_bytes = workers.checked_mul(
            (NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY as u128)
                .checked_mul(core::mem::size_of::<WorkerRequest>() as u128)?,
        )?;
        let request_receiver_owner_bytes =
            workers.checked_mul(core::mem::size_of::<Receiver<WorkerRequest>>() as u128)?;
        let completion_sender_count = worker_count.checked_add(1)?;
        let completion_sender_owner_bytes = (completion_sender_count as u128)
            .checked_mul(core::mem::size_of::<SyncSender<WorkerCompletion>>() as u128)?;
        let completion_receiver_owner_bytes =
            core::mem::size_of::<Receiver<WorkerCompletion>>() as u128;
        let completion_channel_slot_bytes = workers.checked_mul(
            (NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY as u128)
                .checked_mul(core::mem::size_of::<WorkerCompletion>() as u128)?,
        )?;
        let container_owner_bytes = (core::mem::size_of::<Vec<SyncSender<WorkerRequest>>>()
            as u128)
            .checked_add(core::mem::size_of::<Vec<JoinHandle<()>>>() as u128)?
            .checked_add(core::mem::size_of::<VecDeque<usize>>() as u128)?
            .checked_add(core::mem::size_of::<Vec<(usize, Vec<CoreExecutionResult>)>>() as u128)?;
        let thread_name_payload_bytes =
            checked_native_worker_thread_name_payload_bytes(worker_count)?;

        let persistent_bytes = worker_stack_bytes
            .checked_add(request_sender_backing_bytes)?
            .checked_add(worker_handle_backing_bytes)?
            .checked_add(request_channel_slot_bytes)?
            .checked_add(request_receiver_owner_bytes)?
            .checked_add(completion_sender_owner_bytes)?
            .checked_add(completion_receiver_owner_bytes)?
            .checked_add(completion_channel_slot_bytes)?
            .checked_add(container_owner_bytes)?
            .checked_add(thread_name_payload_bytes)?;
        let drive_peak_bytes = persistent_bytes
            .checked_add(available_queue_backing_bytes)?
            .checked_add(candidate_packet_backing_bytes)?
            .checked_add(candidate_row_id_payload_bytes)?;
        let terminal_peak_bytes = persistent_bytes
            .checked_add(available_queue_backing_bytes)?
            .checked_add(worker_result_backing_bytes)?;

        Some(Self {
            worker_count,
            batch_capacity: NATIVE_BUILD_BATCH_CAPACITY,
            row_ids_per_candidate,
            candidate_packet_backing_bytes,
            candidate_row_id_payload_bytes,
            worker_stack_bytes,
            request_sender_backing_bytes,
            worker_handle_backing_bytes,
            available_queue_backing_bytes,
            worker_result_backing_bytes,
            request_channel_slot_bytes,
            request_receiver_owner_bytes,
            completion_sender_owner_bytes,
            completion_receiver_owner_bytes,
            completion_channel_slot_bytes,
            container_owner_bytes,
            thread_name_payload_bytes,
            required_peak_bytes: drive_peak_bytes.max(terminal_peak_bytes),
        })
    }
}

fn checked_heap_backing_bytes<T>(capacity: usize) -> Option<u128> {
    let bytes = capacity.checked_mul(core::mem::size_of::<T>())?;
    (bytes <= isize::MAX as usize).then_some(bytes as u128)
}

fn checked_native_worker_thread_name_payload_bytes(worker_count: usize) -> Option<u128> {
    let count = worker_count as u128;
    let prefix_bytes = count.checked_mul(NATIVE_BUILD_WORKER_THREAD_NAME_PREFIX.len() as u128)?;
    let mut digit_bytes = 0_u128;
    let mut lower = 0_u128;
    let mut upper = 10_u128;
    let mut digits = 1_u128;
    while lower < count {
        let end = count.min(upper);
        digit_bytes = digit_bytes.checked_add(end.checked_sub(lower)?.checked_mul(digits)?)?;
        lower = end;
        upper = upper.checked_mul(10)?;
        digits = digits.checked_add(1)?;
    }
    prefix_bytes.checked_add(digit_bytes)
}

fn native_worker_thread_name(worker_index: usize) -> Result<String, CoreExecutionError> {
    let mut remaining = worker_index;
    let mut digits = 1_usize;
    while remaining >= 10 {
        remaining /= 10;
        digits = digits
            .checked_add(1)
            .ok_or_else(native_coordinator_allocation_unavailable)?;
    }
    let expected_bytes = NATIVE_BUILD_WORKER_THREAD_NAME_PREFIX
        .len()
        .checked_add(digits)
        .ok_or_else(native_coordinator_allocation_unavailable)?;
    let mut name = String::new();
    name.try_reserve_exact(expected_bytes)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    name.push_str(NATIVE_BUILD_WORKER_THREAD_NAME_PREFIX);
    write!(&mut name, "{worker_index}").map_err(|_| native_coordinator_allocation_unavailable())?;
    debug_assert_eq!(name.len(), expected_bytes);
    Ok(name)
}

fn native_coordinator_allocation_unavailable() -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable {
        component: "wasm_cpu_worker_coordinator_allocation_unavailable",
    }
}

#[cfg(test)]
pub(crate) fn run_native_build_probability_with_workers(
    service: AppCoreExecutorService,
    problem: &SearchProblem,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse_metric: FinesseMetric,
    finesse_pattern_knowledge: FinessePatternKnowledge,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    let total_workers = requested_workers.max(2);
    // One unit remains with the producer/merger while every verifier receives
    // an explicit child of the aggregate parent admission.
    let worker_threads = total_workers.saturating_sub(1).max(1);
    let coordinator_reserved_bytes =
        native_coordinator_reserved_bytes(field, worker_threads).unwrap_or(u128::MAX);
    let producer = WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
        problem,
        field,
        aggregation,
        finesse_metric,
        finesse_pattern_knowledge,
        worker_threads,
        coordinator_reserved_bytes,
    )
    .map_err(WasmCpuSearchError::into_core_execution_error)?;

    // All coordinator allocations begin only after the aggregate admission
    // above has reserved `coordinator_reserved_bytes` for their full lifetime.
    let (completion_sender, completion_receiver) =
        mpsc::sync_channel::<WorkerCompletion>(NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY);
    let mut request_senders = Vec::new();
    request_senders
        .try_reserve_exact(worker_threads)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    let mut handles = Vec::new();
    handles
        .try_reserve_exact(worker_threads)
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    for worker_index in 0..worker_threads {
        let verifier = match producer.new_delegated_verifier(field, aggregation) {
            Ok(verifier) => verifier,
            Err(error) => {
                drop(request_senders);
                drop(completion_sender);
                let _ = join_workers(handles);
                return Err(error.into_core_execution_error());
            }
        };
        let (request_sender, request_receiver) =
            mpsc::sync_channel(NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY);
        let worker_completion_sender = completion_sender.clone();
        let worker_control = control.clone();
        let thread_name = match native_worker_thread_name(worker_index) {
            Ok(name) => name,
            Err(error) => {
                drop(request_senders);
                drop(completion_sender);
                drop(completion_receiver);
                let _ = join_workers(handles);
                return Err(error);
            }
        };
        let handle = match thread::Builder::new()
            .name(thread_name)
            .stack_size(NATIVE_BUILD_WORKER_STACK_BYTES)
            .spawn(move || {
                build_probability_worker_main(
                    worker_index,
                    verifier,
                    request_receiver,
                    worker_completion_sender,
                    worker_control,
                );
            }) {
            Ok(handle) => handle,
            Err(_) => {
                drop(request_senders);
                drop(completion_sender);
                let _ = join_workers(handles);
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_pool_unavailable",
                });
            }
        };
        request_senders.push(request_sender);
        handles.push(handle);
    }
    drop(completion_sender);

    let mut producer = producer;
    let drive_result = drive_native_build_probability(
        &mut producer,
        &request_senders,
        &completion_receiver,
        control,
    );
    drop(request_senders);
    // A rendezvous sender can be blocked when another worker reports an error.
    // Drop the receiver before joining so every pending send fails closed and
    // every worker can leave its loop.
    drop(completion_receiver);
    let join_result = join_workers(handles);
    match (drive_result, join_result) {
        (Ok(output), Ok(())) => finish_native_build_probability(
            service,
            producer,
            output,
            solution_probability_policy,
            total_workers,
            control,
        ),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn native_coordinator_reserved_bytes(
    field: BuildProbabilityField,
    worker_threads: usize,
) -> Option<u128> {
    native_coordinator_reserved_bytes_with_cap(field, worker_threads, u128::MAX)
}

fn native_coordinator_reserved_bytes_with_cap(
    field: BuildProbabilityField,
    worker_threads: usize,
    memory_cap_bytes: u128,
) -> Option<u128> {
    let required =
        NativeCoordinatorMemoryProjection::checked(field, worker_threads)?.required_peak_bytes;
    (required <= memory_cap_bytes).then_some(required)
}

fn drive_native_build_probability(
    producer: &mut WasmBuildProbabilityCandidateProducer,
    request_senders: &[SyncSender<WorkerRequest>],
    completion_receiver: &Receiver<WorkerCompletion>,
    control: &ExecutionControl,
) -> Result<NativeBuildProbabilityWorkerOutput, CoreExecutionError> {
    let mut available = VecDeque::new();
    available
        .try_reserve_exact(request_senders.len())
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    available.extend(0..request_senders.len());
    let mut in_flight = 0_usize;
    let mut summary = None;
    while summary.is_none() || in_flight != 0 {
        while summary.is_none() {
            let Some(worker_index) = available.pop_front() else {
                break;
            };
            let mut batch = Vec::new();
            batch
                .try_reserve_exact(NATIVE_BUILD_BATCH_CAPACITY)
                .map_err(|_| native_coordinator_allocation_unavailable())?;
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
    let mut worker_results = Vec::new();
    worker_results
        .try_reserve_exact(request_senders.len())
        .map_err(|_| native_coordinator_allocation_unavailable())?;
    for _ in request_senders {
        let completion =
            completion_receiver
                .recv()
                .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_pool_unavailable",
                })?;
        match completion.result.map_err(core_error)? {
            WorkerResult::Finished(results) => {
                worker_results.push((completion.worker_index, results));
                let (container_bytes, payload_bytes) =
                    checked_worker_results_memory(&worker_results, worker_results.capacity())
                        .ok_or_else(|| {
                            producer
                                .validate_external_result_memory(u128::MAX)
                                .expect_err("overflow-sized worker results are unavailable")
                                .into_core_execution_error()
                        })?;
                producer
                    .validate_external_result_memory(
                        container_bytes
                            .checked_add(payload_bytes)
                            .and_then(|bytes| {
                                bytes.checked_add(
                                    core::mem::size_of::<WasmDistributedGeometrySummary>() as u128,
                                )
                            })
                            .ok_or_else(|| {
                                producer
                                    .validate_external_result_memory(u128::MAX)
                                    .expect_err("overflow-sized worker results are unavailable")
                                    .into_core_execution_error()
                            })?,
                    )
                    .map_err(WasmCpuSearchError::into_core_execution_error)?;
            }
            WorkerResult::Consumed(_) => {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "wasm_cpu_worker_protocol_invalid",
                })
            }
        }
    }
    worker_results.sort_unstable_by_key(|(worker_index, _)| *worker_index);
    Ok(NativeBuildProbabilityWorkerOutput {
        summary: summary.ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "wasm_cpu_worker_geometry_summary_missing",
        })?,
        worker_results,
    })
}

fn finish_native_build_probability(
    service: AppCoreExecutorService,
    producer: WasmBuildProbabilityCandidateProducer,
    output: NativeBuildProbabilityWorkerOutput,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
    total_workers: usize,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    let (external_container_bytes, mut external_payload_bytes) =
        checked_worker_output_memory(&output).ok_or_else(|| {
            producer
                .validate_external_result_memory(u128::MAX)
                .expect_err("overflow-sized external results are unavailable")
                .into_core_execution_error()
        })?;
    producer
        .validate_external_result_memory(
            external_container_bytes
                .checked_add(external_payload_bytes)
                .ok_or_else(|| {
                    producer
                        .validate_external_result_memory(u128::MAX)
                        .expect_err("overflow-sized external results are unavailable")
                        .into_core_execution_error()
                })?,
        )
        .map_err(WasmCpuSearchError::into_core_execution_error)?;
    let mut merger = producer.into_merger().map_err(core_error)?;
    let NativeBuildProbabilityWorkerOutput {
        summary,
        worker_results,
    } = output;
    for (_, results) in worker_results {
        for result in results {
            let raw_payload_bytes =
                checked_public_result_payload_bytes(&result).ok_or_else(|| {
                    merger
                        .validate_external_result_memory(u128::MAX, 0)
                        .expect_err("overflow-sized result payload is unavailable")
                        .into_core_execution_error()
                })?;
            external_payload_bytes = external_payload_bytes
                .checked_sub(raw_payload_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_external_result_accounting_invalid",
                })?;
            let external_without_current = external_container_bytes
                .checked_add(external_payload_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_external_result_accounting_overflow",
                })?;
            let result = service.materialize_distributed_postprocess_partition_with_memory_guard(
                result,
                solution_probability_policy,
                control,
                |stage_result, checked_future_bytes| {
                    let stage_bytes =
                        WasmBuildProbabilityDistributedResultMerger::public_result_retained_bytes(
                            stage_result,
                        )
                        .ok_or_else(|| {
                            merger
                                .validate_external_result_memory(u128::MAX, 0)
                                .expect_err("overflow-sized postprocess result is unavailable")
                                .into_core_execution_error()
                        })?;
                    merger
                        .validate_external_result_memory(
                            external_without_current,
                            stage_bytes.checked_add(checked_future_bytes).ok_or(
                                CoreExecutionError::RuntimeUnavailable {
                                    component:
                                        "build_probability_postprocess_memory_projection_overflow",
                                },
                            )?,
                        )
                        .map_err(WasmCpuSearchError::into_core_execution_error)
                },
            )?;
            let materialized_payload_bytes = checked_public_result_payload_bytes(&result).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_result_memory_projection_overflow",
                },
            )?;
            let external_with_current = external_without_current
                .checked_add(materialized_payload_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_external_result_accounting_overflow",
                })?;
            merger
                .absorb_with_external_retained(&result, external_with_current)
                .map_err(WasmCpuSearchError::into_core_execution_error)?;
        }
    }
    merger.finish_with_control_and_terminal(
        &summary,
        total_workers,
        control,
        |result, authority| {
            let result = replace_native_parallel_decision_field_with_memory_guard(
                result.map_err(core_error)?,
                |stage_result, checked_future_bytes| {
                    authority
                        .validate_public_result_memory_with_future(
                            stage_result,
                            checked_future_bytes,
                        )
                        .map_err(core_error)
                },
            )?;
            service.materialize_build_probability_public_result_with_memory_guard(
                result,
                solution_probability_policy,
                control,
                |stage_result, checked_future_bytes| {
                    authority
                        .validate_public_result_memory_with_future(
                            stage_result,
                            checked_future_bytes,
                        )
                        .map_err(core_error)
                },
            )
        },
    )
}

fn replace_native_parallel_decision_field_with_memory_guard(
    result: CoreExecutionResult,
    mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    const KEY: &str = "cpu_parallel_decision_reason";
    const VALUE: &str = "native-ready-worker-build-probability-pipeline";

    // The first guard is intentionally allocation-free. It covers the owned
    // replacement strings and every field/report allocation that the guarded
    // replacement will make while the merger and the old result remain live.
    let borrowed = [(KEY, VALUE)];
    let projection = result
        .checked_borrowed_field_replacement_projection(&borrowed)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_native_decision_memory_projection_overflow",
        })?;
    memory_guard(&result, projection.required_future_bytes)?;

    let mut fields = Vec::new();
    fields
        .try_reserve_exact(1)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_native_decision_field_allocation_failed",
        })?;
    let mut key = String::new();
    key.try_reserve_exact(KEY.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_native_decision_field_allocation_failed",
        })?;
    key.push_str(KEY);
    let mut value = String::new();
    value
        .try_reserve_exact(VALUE.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_native_decision_field_allocation_failed",
        })?;
    value.push_str(VALUE);
    fields.push((key, value));
    result
        .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
            memory_guard(live, future)
        })
        .map_err(|error| match error {
            CoreResultFieldReplacementError::ProjectionOverflow => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_native_decision_memory_projection_overflow",
                }
            }
            CoreResultFieldReplacementError::AllocationFailed { .. } => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_native_decision_field_allocation_failed",
                }
            }
            CoreResultFieldReplacementError::MemoryGuard(error) => error,
        })
}

fn checked_public_result_payload_bytes(result: &CoreExecutionResult) -> Option<u128> {
    WasmBuildProbabilityDistributedResultMerger::public_result_retained_bytes(result)?
        .checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
}

fn checked_worker_output_memory(
    output: &NativeBuildProbabilityWorkerOutput,
) -> Option<(u128, u128)> {
    let (container_bytes, payload_bytes) =
        checked_worker_results_memory(&output.worker_results, output.worker_results.capacity())?;
    Some((
        container_bytes
            .checked_add(core::mem::size_of::<WasmDistributedGeometrySummary>() as u128)?,
        payload_bytes,
    ))
}

fn checked_worker_results_memory(
    worker_results: &[(usize, Vec<CoreExecutionResult>)],
    outer_capacity: usize,
) -> Option<(u128, u128)> {
    let mut container_bytes = (outer_capacity as u128)
        .checked_mul(core::mem::size_of::<(usize, Vec<CoreExecutionResult>)>() as u128)?;
    let mut payload_bytes = 0u128;
    for (_, results) in worker_results {
        container_bytes = container_bytes.checked_add(
            (results.capacity() as u128)
                .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)?,
        )?;
        for result in results {
            payload_bytes =
                payload_bytes.checked_add(checked_public_result_payload_bytes(result)?)?;
        }
    }
    Some((container_bytes, payload_bytes))
}

fn build_probability_worker_main(
    worker_index: usize,
    mut verifier: WasmBuildProbabilityDistributedVerifier,
    requests: Receiver<WorkerRequest>,
    completions: SyncSender<WorkerCompletion>,
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
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow,
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
                .with_worker_hardware_limit(workers)
                .with_use_all_logical_processors(true)
                .with_max_candidates(1_024),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece problem")
    }

    fn one_piece_probability_problem(workers: usize) -> SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        )
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(workers)
                .with_worker_hardware_limit(workers),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece probability problem")
    }

    fn native_decision_test_result() -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![("search_kind".to_owned(), "build-probability".to_owned())],
            Vec::new(),
        )
    }

    #[test]
    fn native_decision_field_replacement_accepts_the_exact_peak_and_rejects_peak_minus_one() {
        let peak = std::cell::Cell::new(0_u128);
        let result = replace_native_parallel_decision_field_with_memory_guard(
            native_decision_test_result(),
            |live, future| {
                let observed = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_projection_overflow",
                    })?;
                peak.set(peak.get().max(observed));
                Ok(())
            },
        )
        .expect("measure guarded decision replacement");
        assert_eq!(
            result.field("cpu_parallel_decision_reason"),
            Some("native-ready-worker-build-probability-pipeline")
        );
        let exact_peak = peak.get();

        replace_native_parallel_decision_field_with_memory_guard(
            native_decision_test_result(),
            |live, future| {
                let observed = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_projection_overflow",
                    })?;
                (observed <= exact_peak).then_some(()).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_budget_exceeded",
                    },
                )
            },
        )
        .expect("exact peak remains admissible");

        let error = replace_native_parallel_decision_field_with_memory_guard(
            native_decision_test_result(),
            |live, future| {
                let observed = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_projection_overflow",
                    })?;
                (observed < exact_peak).then_some(()).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_budget_exceeded",
                    },
                )
            },
        )
        .expect_err("peak minus one must fail closed");
        assert!(matches!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_budget_exceeded"
            }
        ));
    }

    #[test]
    fn app_service_rejects_parallel_request_without_host_memory_authority() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target");
        let error = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &one_piece_problem(2),
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Search {
                    pattern_knowledge: FinessePatternKnowledge::Both,
                },
                BuildSolutionProbabilityPolicy::Omit,
                &ExecutionControl::default(),
            )
            .expect_err("parallel request must not downgrade without provider authority");

        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "native_build_probability_host_provider_not_registered",
            }
        );
    }

    #[test]
    fn native_ready_worker_path_matches_serial_finesse_without_nested_searches() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
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
                BuildSolutionProbabilityPolicy::Omit,
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
            BuildSolutionProbabilityPolicy::Omit,
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

    #[test]
    fn native_ready_worker_path_matches_serial_per_solution_probability_reports() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target");
        let service = AppCoreExecutorService::wasm_cpu();
        let control = ExecutionControl::default();
        let finesse = BuildProbabilityFinesseRequest::Search {
            pattern_knowledge: FinessePatternKnowledge::Both,
        };
        let serial = service
            .execute_build_probability_with_control(
                &one_piece_probability_problem(1),
                field,
                BuildProbabilityAggregation::Buildability,
                finesse,
                BuildSolutionProbabilityPolicy::Include,
                &control,
            )
            .expect("serial per-solution probabilities");
        let parallel = run_native_build_probability_with_workers(
            service,
            &one_piece_probability_problem(2),
            field,
            BuildProbabilityAggregation::Buildability,
            FinesseMetric::Inputs,
            FinessePatternKnowledge::Both,
            BuildSolutionProbabilityPolicy::Include,
            2,
            &control,
        )
        .expect("parallel per-solution probabilities");

        assert_eq!(
            parallel.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            parallel.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(
            parallel.solution_probabilities(),
            serial.solution_probabilities()
        );
        assert_eq!(
            serial.bool_field("execution_constraint_materialized"),
            Some(true)
        );
        assert_eq!(
            parallel.bool_field("execution_constraint_materialized"),
            Some(true)
        );
        for key in [
            "solution_probabilities_requested",
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(
                parallel.unique_field(key),
                serial.unique_field(key),
                "{key}"
            );
        }
        assert_eq!(
            parallel.bool_field("solution_probability_complete"),
            Some(true)
        );
        assert_eq!(parallel.solution_probabilities().len(), 1);
    }

    #[test]
    fn native_coordinator_projection_is_fieldwise_for_one_two_and_four_workers() {
        let field = BuildProbabilityField::from_words_preserving_height(
            24,
            [0; 4],
            [u64::MAX, u64::MAX, u64::MAX, (1_u64 << 48) - 1],
        )
        .expect("extended field");
        for worker_count in [1_usize, 2, 4] {
            let projection = NativeCoordinatorMemoryProjection::checked(field, worker_count)
                .expect("checked coordinator projection");
            let workers = worker_count as u128;
            let expected_packet_backing = workers
                * NATIVE_BUILD_BATCH_CAPACITY as u128
                * core::mem::size_of::<WasmCandidatePacket>() as u128;
            let expected_row_payload = workers
                * NATIVE_BUILD_BATCH_CAPACITY as u128
                * field.target_piece_count() as u128
                * core::mem::size_of::<u32>() as u128;
            let expected_thread_names = (0..worker_count)
                .map(|worker_index| {
                    (NATIVE_BUILD_WORKER_THREAD_NAME_PREFIX.len() + worker_index.to_string().len())
                        as u128
                })
                .sum::<u128>();

            assert_eq!(projection.worker_count, worker_count);
            assert_eq!(projection.batch_capacity, NATIVE_BUILD_BATCH_CAPACITY);
            assert_eq!(projection.row_ids_per_candidate, field.target_piece_count());
            assert_eq!(
                projection.candidate_packet_backing_bytes,
                expected_packet_backing
            );
            assert_eq!(
                projection.candidate_row_id_payload_bytes,
                expected_row_payload
            );
            assert_eq!(
                projection.worker_stack_bytes,
                workers * NATIVE_BUILD_WORKER_STACK_BYTES as u128
            );
            assert_eq!(
                projection.request_sender_backing_bytes,
                workers * core::mem::size_of::<SyncSender<WorkerRequest>>() as u128
            );
            assert_eq!(
                projection.worker_handle_backing_bytes,
                workers * core::mem::size_of::<JoinHandle<()>>() as u128
            );
            assert_eq!(
                projection.available_queue_backing_bytes,
                workers * core::mem::size_of::<usize>() as u128
            );
            assert_eq!(
                projection.worker_result_backing_bytes,
                workers * core::mem::size_of::<(usize, Vec<CoreExecutionResult>)>() as u128
            );
            assert_eq!(
                projection.request_channel_slot_bytes,
                workers
                    * NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY as u128
                    * core::mem::size_of::<WorkerRequest>() as u128
            );
            assert_eq!(
                projection.request_receiver_owner_bytes,
                workers * core::mem::size_of::<Receiver<WorkerRequest>>() as u128
            );
            assert_eq!(
                projection.completion_sender_owner_bytes,
                (workers + 1) * core::mem::size_of::<SyncSender<WorkerCompletion>>() as u128
            );
            assert_eq!(
                projection.completion_receiver_owner_bytes,
                core::mem::size_of::<Receiver<WorkerCompletion>>() as u128
            );
            assert_eq!(projection.completion_channel_slot_bytes, 0);
            assert_eq!(
                projection.container_owner_bytes,
                (core::mem::size_of::<Vec<SyncSender<WorkerRequest>>>()
                    + core::mem::size_of::<Vec<JoinHandle<()>>>()
                    + core::mem::size_of::<VecDeque<usize>>()
                    + core::mem::size_of::<Vec<(usize, Vec<CoreExecutionResult>)>>())
                    as u128
            );
            assert_eq!(projection.thread_name_payload_bytes, expected_thread_names);

            let persistent_bytes = projection.worker_stack_bytes
                + projection.request_sender_backing_bytes
                + projection.worker_handle_backing_bytes
                + projection.request_channel_slot_bytes
                + projection.request_receiver_owner_bytes
                + projection.completion_sender_owner_bytes
                + projection.completion_receiver_owner_bytes
                + projection.completion_channel_slot_bytes
                + projection.container_owner_bytes
                + projection.thread_name_payload_bytes;
            let drive_peak_bytes = persistent_bytes
                + projection.available_queue_backing_bytes
                + projection.candidate_packet_backing_bytes
                + projection.candidate_row_id_payload_bytes;
            let terminal_peak_bytes = persistent_bytes
                + projection.available_queue_backing_bytes
                + projection.worker_result_backing_bytes;
            assert_eq!(
                projection.required_peak_bytes,
                drive_peak_bytes.max(terminal_peak_bytes)
            );
        }
    }

    #[test]
    fn native_coordinator_projection_overflow_and_cap_gate_fail_closed() {
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-row target");
        let required = native_coordinator_reserved_bytes(field, 4).expect("checked reservation");
        assert_eq!(
            native_coordinator_reserved_bytes_with_cap(field, 4, required),
            Some(required)
        );
        assert_eq!(
            native_coordinator_reserved_bytes_with_cap(field, 4, required - 1),
            None
        );
        assert!(NativeCoordinatorMemoryProjection::checked(field, 0).is_none());
        assert!(NativeCoordinatorMemoryProjection::checked(field, usize::MAX).is_none());
    }

    #[test]
    fn native_channels_bound_requests_and_rendezvous_completion_drop_errors() {
        let (request_sender, request_receiver) =
            mpsc::sync_channel(NATIVE_BUILD_REQUEST_CHANNEL_CAPACITY);
        request_sender
            .try_send(WorkerRequest::Finish)
            .expect("one request slot");
        assert!(matches!(
            request_sender.try_send(WorkerRequest::Finish),
            Err(mpsc::TrySendError::Full(WorkerRequest::Finish))
        ));
        drop(request_receiver);

        let (completion_sender, completion_receiver) =
            mpsc::sync_channel(NATIVE_BUILD_COMPLETION_CHANNEL_CAPACITY);
        assert!(matches!(
            completion_sender.try_send(WorkerCompletion {
                worker_index: 0,
                result: Err("expected-worker-error"),
            }),
            Err(mpsc::TrySendError::Full(WorkerCompletion {
                worker_index: 0,
                result: Err("expected-worker-error"),
            }))
        ));
        let blocked_sender = thread::spawn(move || {
            completion_sender
                .send(WorkerCompletion {
                    worker_index: 0,
                    result: Ok(WorkerResult::Consumed(1)),
                })
                .is_err()
        });
        drop(completion_receiver);
        assert!(blocked_sender
            .join()
            .expect("receiver drop releases a rendezvous sender"));
    }

    #[test]
    fn worker_result_container_projection_uses_reserved_capacities_fieldwise() {
        let worker_results: Vec<(usize, Vec<CoreExecutionResult>)> = Vec::with_capacity(4);
        let (container_bytes, payload_bytes) =
            checked_worker_results_memory(&worker_results, worker_results.capacity())
                .expect("checked empty result projection");
        assert_eq!(
            container_bytes,
            4 * core::mem::size_of::<(usize, Vec<CoreExecutionResult>)>() as u128
        );
        assert_eq!(payload_bytes, 0);
    }
}
