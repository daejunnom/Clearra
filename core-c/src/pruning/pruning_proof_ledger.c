#include "clr_pruning.h"

#include <stdint.h>
#include <string.h>

static uint64_t saturating_add_u64(uint64_t left, uint64_t right) {
    return UINT64_MAX - left < right ? UINT64_MAX : left + right;
}

static uint64_t multiset_digest_term(uint64_t value) {
    value += UINT64_C(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27u)) * UINT64_C(0x94d049bb133111eb);
    value ^= value >> 31u;
    return value == 0u ? UINT64_C(1) : value;
}

static bool entry_is_valid(clr_pruning_proof_ledger_entry entry) {
    return entry.batch_id != 0u && entry.producer_id != 0u &&
           entry.catalog_identity_digest != 0u &&
           entry.affected_candidate_count != 0u &&
           entry.evidence_digest != 0u && entry.prune_reason > 0u &&
           entry.prune_reason < CLR_PRUNING_PRUNE_REASON_COUNT;
}

static clr_pruning_minimal_record *find_minimal_record(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    for (uint16_t index = 0u; index < ledger->minimal_record_count; ++index) {
        clr_pruning_minimal_record *record = &ledger->minimal_records[index];
        if (record->batch_id == entry.batch_id &&
            record->producer_id == entry.producer_id &&
            record->catalog_identity_digest == entry.catalog_identity_digest &&
            record->prune_reason == entry.prune_reason) {
            return record;
        }
    }
    return 0;
}

static bool minimal_record_available(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    return find_minimal_record(ledger, entry) != 0 ||
           ledger->minimal_record_count <
               CLR_PRUNING_MINIMAL_RECORD_MAX_ENTRIES;
}

static void record_minimal_evidence(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    clr_pruning_minimal_record *record = find_minimal_record(ledger, entry);
    if (record == 0) {
        record = &ledger->minimal_records[ledger->minimal_record_count++];
        *record = (clr_pruning_minimal_record){
            .batch_id = entry.batch_id,
            .producer_id = entry.producer_id,
            .catalog_identity_digest = entry.catalog_identity_digest,
            .prune_reason = entry.prune_reason,
        };
    }
    record->aggregate_evidence_digest +=
        multiset_digest_term(entry.evidence_digest);
    if (record->aggregate_evidence_digest == 0u) {
        record->aggregate_evidence_digest = UINT64_C(1);
    }
    record->affected_candidate_count = saturating_add_u64(
        record->affected_candidate_count, entry.affected_candidate_count);

}

static void record_kept_candidate(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    if (UINT32_MAX - ledger->candidates_kept_due_to_evidence_capacity <
        entry.affected_candidate_count) {
        ledger->candidates_kept_due_to_evidence_capacity = UINT32_MAX;
    } else {
        ledger->candidates_kept_due_to_evidence_capacity +=
            entry.affected_candidate_count;
    }
}

static void record_truncated_evidence(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    ledger->evidence_truncated = 1u;
    if (ledger->dropped_evidence_count < UINT32_MAX) {
        ledger->dropped_evidence_count++;
    }
    if (ledger->prune_reason_counts[entry.prune_reason] < UINT32_MAX) {
        ledger->prune_reason_counts[entry.prune_reason]++;
    }
}

void clr_pruning_proof_ledger_init(clr_pruning_proof_ledger *ledger) {
    (void)clr_pruning_proof_ledger_init_with_policy(
        ledger, CLR_PRUNING_EVIDENCE_BEST_EFFORT);
}

clr_pruning_status clr_pruning_proof_ledger_init_with_policy(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_evidence_policy policy) {
    if (ledger == 0 ||
        (policy != CLR_PRUNING_EVIDENCE_BEST_EFFORT &&
         policy != CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED)) {
        return CLR_PRUNING_INVALID_ARGUMENT;
    }
    memset(ledger, 0, sizeof(*ledger));
    ledger->capacity = CLR_PRUNING_LEDGER_MAX_ENTRIES;
    ledger->minimal_record_capacity =
        CLR_PRUNING_MINIMAL_RECORD_MAX_ENTRIES;
    ledger->evidence_policy = (uint8_t)policy;
    return CLR_PRUNING_OK;
}

clr_pruning_status clr_pruning_proof_ledger_record(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry) {
    if (ledger == 0 || !entry_is_valid(entry)) {
        return CLR_PRUNING_INVALID_ARGUMENT;
    }
    if (!minimal_record_available(ledger, entry)) {
        ledger->minimal_record_capacity_hit = 1u;
        record_kept_candidate(ledger, entry);
        return CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE;
    }
    if (ledger->count >= CLR_PRUNING_LEDGER_MAX_ENTRIES) {
        if (ledger->evidence_policy ==
            (uint8_t)CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED) {
            ledger->complete_required_capacity_hit = 1u;
            record_kept_candidate(ledger, entry);
            return CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE;
        }
        record_minimal_evidence(ledger, entry);
        record_truncated_evidence(ledger, entry);
        return CLR_PRUNING_OK;
    }

    record_minimal_evidence(ledger, entry);
    ledger->entries[ledger->count] = entry;
    ledger->count++;
    return CLR_PRUNING_OK;
}
