#include "gpu/gpu_backend.h"

ClearraGpuStatus clearra_cpu_packing_reference_generate(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer) {
    clr_resource_report resource_report;
    return clearra_cpu_packing_reference_generate_with_resource_report(
        batch, out_buffer, &resource_report);
}

ClearraGpuStatus clearra_cpu_packing_reference_generate_with_resource_report(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    return clearra_cpu_packing_reference_generate_with_resource_report_and_pruning_ledger(
        batch, out_buffer, out_resource_report, &pruning_ledger);
}

ClearraGpuStatus
clearra_cpu_packing_reference_generate_with_resource_report_and_pruning_ledger(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    if (batch == 0 || out_buffer == 0 || out_resource_report == 0 ||
        out_pruning_ledger == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clr_packing_problem problem;
    if (clearra_gpu_batch_descriptor_to_packing_problem(batch, &problem) !=
        CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraPackingStatus status =
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
            &problem,
            out_buffer,
            out_resource_report,
            out_pruning_ledger);
    return status == CLEARRA_PACKING_OK ||
                   status == CLEARRA_PACKING_CAPACITY_EXCEEDED
        ? CLEARRA_GPU_OK
        : CLEARRA_GPU_PACKING_ERROR;
}
