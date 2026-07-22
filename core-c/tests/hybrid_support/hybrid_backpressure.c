#include "hybrid_scheduler.h"
static uint16_t fallback_u16(uint16_t preferred, uint16_t fallback) {
    return preferred != 0u ? preferred : fallback;
}static uint16_t u16_from_u32(uint32_t value) {
    return value > UINT16_MAX ? UINT16_MAX : (uint16_t)value;
}static uint16_t in_flight_batches(const ClearraHybridBackendMetrics *metrics) {
    uint32_t pending;
    if (metrics == 0) {
        return 0u;
    }

    pending = metrics->gpu_readback_pending;
    if (metrics->gpu_batches_submitted > metrics->gpu_batches_completed) {
        pending += metrics->gpu_batches_submitted - metrics->gpu_batches_completed;
    }
    return u16_from_u32(pending);
}ClearraHybridBackpressureReport clearra_hybrid_backpressure_report_for(
    const ClearraHybridBatchPlan *plan,
    const ClearraHybridBackendMetrics *metrics) {
    ClearraHybridBackpressureReport report = {0};
    if (plan == 0 || metrics == 0) {
        report.throttle_reason = CLEARRA_HYBRID_THROTTLE_NONE;
        return report;
    }

    report.gpu_queue_depth = metrics->fallback_used
        ? 0u
        : fallback_u16(metrics->gpu_queue_depth,
                       metrics->hybrid_candidate_count);
    report.cpu_worker_queue_depth =
        fallback_u16(metrics->cpu_buildup_backlog, plan->cpu_worker_count);
    report.readback_pending_batches = metrics->fallback_used
        ? 0u
        : fallback_u16(metrics->readback_pending_batches,
                       metrics->gpu_readback_overlap_steps);
    report.build_variant_buffer_pressure =
        fallback_u16(metrics->cpu_buildup_backlog,
                     metrics->hybrid_build_variant_count);
    report.coverage_row_buffer_pressure =
        fallback_u16(metrics->coverage_row_buffer_pressure,
                     metrics->hybrid_build_variant_count);
    report.throttled_backend =
        metrics->fallback_used ? 1u : (plan->gpu_large_packing_batch ? 2u : 1u);
    report.candidate_queue_len =
        u16_from_u32(metrics->candidate_buffer_pressure != 0u
                         ? metrics->candidate_buffer_pressure
                         : metrics->hybrid_candidate_count);
    report.candidate_queue_capacity = plan->large_batch_threshold;
    report.cpu_worker_backlog = report.cpu_worker_queue_depth;
    report.gpu_readback_backlog = report.readback_pending_batches;
    report.gpu_batch_in_flight =
        metrics->fallback_used ? 0u : in_flight_batches(metrics);
    report.memory_pressure_level = metrics->memory_pressure_level;

    if (report.readback_pending_batches > 0u) {
        report.throttle_reason = CLEARRA_HYBRID_THROTTLE_READBACK_PENDING;
    } else if (report.gpu_queue_depth > plan->large_batch_threshold) {
        report.throttle_reason = CLEARRA_HYBRID_THROTTLE_GPU_QUEUE_DEPTH;
    } else if (report.cpu_worker_queue_depth > plan->cpu_worker_count) {
        report.throttle_reason =
            CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH;
    } else if (report.build_variant_buffer_pressure > 0u) {
        report.throttle_reason =
            CLEARRA_HYBRID_THROTTLE_BUILD_VARIANT_BUFFER_PRESSURE;
    } else if (report.coverage_row_buffer_pressure > 0u) {
        report.throttle_reason =
            CLEARRA_HYBRID_THROTTLE_COVERAGE_ROW_BUFFER_PRESSURE;
    } else {
        report.throttle_reason = CLEARRA_HYBRID_THROTTLE_NONE;
        report.throttled_backend = 0u;
    }
    report.backpressure_active =
        (uint8_t)(report.throttle_reason != CLEARRA_HYBRID_THROTTLE_NONE ||
                  report.memory_pressure_level ==
                      CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH);
    if (report.backpressure_active &&
        report.throttle_reason ==
            CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH) {
        report.deferred_batch_count =
            report.cpu_worker_backlog > plan->cpu_worker_count
                ? (uint16_t)(report.cpu_worker_backlog - plan->cpu_worker_count)
                : 1u;
    }
    if (report.memory_pressure_level == CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH ||
        report.throttle_reason ==
            CLEARRA_HYBRID_THROTTLE_COVERAGE_ROW_BUFFER_PRESSURE) {
        report.truncated_batch_count = 1u;
    }

    return report;
}

#include "hybrid_scheduler.h"
static uint32_t autotune_cpu_backlog(
    const ClearraHybridAutotuneMetrics *metrics) {
    if (metrics == 0) {
        return 0u;
    }
    return metrics->cpu_confirm_queue_depth + metrics->cpu_buildup_queue_depth;
}ClearraHybridAutotuneBudget clearra_hybrid_autotune_budget_default(void) {
    ClearraHybridAutotuneBudget budget = {0};
    budget.min_batch_size = 16u;
    budget.max_batch_size = 256u;
    budget.max_readback_pending = 2u;
    budget.max_cpu_backlog = 8u;
    budget.max_memory_pressure = 75u;
    budget.max_coverage_buffer_pressure = 75u;
    return budget;
}ClearraHybridAutotuneDecision clearra_hybrid_autotune_evaluate(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics) {
    ClearraHybridAutotuneDecision decision = {0};
    uint8_t readback_high;
    uint8_t cpu_backlog_high;
    uint8_t coverage_pressure_high;
    uint8_t memory_pressure_high;

    if (budget == 0 || metrics == 0) {
        return decision;
    }

    decision.selected_batch_size =
        clearra_hybrid_batch_size_for(budget, metrics);
    decision.memory_pressure =
        clearra_hybrid_memory_pressure_report_for(budget, metrics);

    readback_high =
        (uint8_t)(metrics->gpu_readback_pending > budget->max_readback_pending);
    cpu_backlog_high =
        (uint8_t)(autotune_cpu_backlog(metrics) > budget->max_cpu_backlog);
    coverage_pressure_high =
        (uint8_t)(metrics->coverage_row_buffer_pressure >
                  budget->max_coverage_buffer_pressure);
    memory_pressure_high =
        (uint8_t)(decision.memory_pressure.level ==
                  CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH);

    decision.throttle_gpu_submission = readback_high;
    decision.prioritize_dedupe = cpu_backlog_high;
    decision.defer_low_priority_candidates = cpu_backlog_high;
    decision.reduce_trace_retention = memory_pressure_high;
    decision.batch_scope_early_release = memory_pressure_high;
    decision.throttle_coverage_row_emission = coverage_pressure_high;
    decision.count_only_mode_allowed = coverage_pressure_high;

    if (readback_high) {
        decision.throttle_reason = CLEARRA_HYBRID_THROTTLE_READBACK_PENDING;
    } else if (cpu_backlog_high) {
        decision.throttle_reason =
            CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH;
    } else if (coverage_pressure_high) {
        decision.throttle_reason =
            CLEARRA_HYBRID_THROTTLE_COVERAGE_ROW_BUFFER_PRESSURE;
    } else {
        decision.throttle_reason = CLEARRA_HYBRID_THROTTLE_NONE;
    }

    if (memory_pressure_high) {
        decision.partial_result_diagnostic_required = 1u;
        decision.truncation_reason = "memory_pressure_truncated";
    } else if (coverage_pressure_high) {
        decision.partial_result_diagnostic_required = 1u;
        decision.truncation_reason =
            "coverage_row_buffer_pressure_truncated";
    }

    return decision;
}

#include "hybrid_scheduler.h"
static uint32_t batch_sizer_cpu_backlog(
    const ClearraHybridAutotuneMetrics *metrics) {
    if (metrics == 0) {
        return 0u;
    }
    return metrics->cpu_confirm_queue_depth + metrics->cpu_buildup_queue_depth;
}static uint32_t clamp_batch_size(
    const ClearraHybridAutotuneBudget *budget,
    uint32_t requested) {
    if (budget == 0) {
        return requested;
    }
    if (requested < budget->min_batch_size) {
        return budget->min_batch_size;
    }
    if (requested > budget->max_batch_size) {
        return budget->max_batch_size;
    }
    return requested;
}uint32_t clearra_hybrid_batch_size_for(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics) {
    uint32_t selected;
    ClearraHybridMemoryPressureReport memory_pressure;

    if (budget == 0 || metrics == 0) {
        return 0u;
    }

    selected = budget->max_batch_size;
    if (batch_sizer_cpu_backlog(metrics) > budget->max_cpu_backlog) {
        selected /= 2u;
    }

    memory_pressure =
        clearra_hybrid_memory_pressure_report_for(budget, metrics);
    if (memory_pressure.level == CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH) {
        selected /= 2u;
    }

    return clamp_batch_size(budget, selected);
}

#include "hybrid_scheduler.h"
static uint32_t max_u32(uint32_t left, uint32_t right) {
    return left > right ? left : right;
}ClearraHybridMemoryPressureReport clearra_hybrid_memory_pressure_report_for(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics) {
    ClearraHybridMemoryPressureReport report = {0};
    uint32_t moderate_threshold;

    if (budget == 0 || metrics == 0) {
        report.level = CLEARRA_HYBRID_MEMORY_PRESSURE_LOW;
        return report;
    }

    report.memory_ticket_live_count = metrics->memory_ticket_live_count;
    report.pending_release_queue_depth = metrics->pending_release_queue_depth;
    report.pressure_score =
        max_u32(report.memory_ticket_live_count,
                report.pending_release_queue_depth);

    moderate_threshold = budget->max_memory_pressure / 2u;
    if (report.pressure_score >= budget->max_memory_pressure) {
        report.level = CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH;
    } else if (report.pressure_score >= moderate_threshold) {
        report.level = CLEARRA_HYBRID_MEMORY_PRESSURE_MODERATE;
    } else {
        report.level = CLEARRA_HYBRID_MEMORY_PRESSURE_LOW;
    }

    return report;
}
