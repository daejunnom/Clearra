#include "realization_domain_propagation.h"

#include <string.h>

uint16_t clearra_realization_deleted_rows_for_state(
    const uint16_t contributors[16],
    uint16_t clearable_rows,
    uint16_t placed_operations) {
    uint16_t deleted_rows = 0u;
    uint16_t remaining_rows = clearable_rows;
    while (remaining_rows != 0u) {
        uint16_t row_bit = (uint16_t)(
            remaining_rows & (uint16_t)(~remaining_rows + UINT16_C(1)));
        uint8_t row = 0u;
        for (uint16_t cursor = row_bit;
             (cursor & UINT16_C(1)) == 0u;
             cursor >>= 1u) {
            row++;
        }
        if ((placed_operations & contributors[row]) == contributors[row]) {
            deleted_rows = (uint16_t)(deleted_rows | row_bit);
        }
        remaining_rows = (uint16_t)(remaining_rows & ~row_bit);
    }
    return deleted_rows;
}

bool clearra_realization_domain_supports_deleted_state(
    const ClearraGeometryCatalog *catalog,
    const ClearraRealizationCandidateDomain *domain,
    uint16_t deleted_rows) {
    if (catalog == 0 || domain == 0) {
        return false;
    }
    if (domain->compact != 0u) {
        return deleted_rows < 64u &&
               (domain->compact_deleted_states &
                (UINT64_C(1) << deleted_rows)) != 0u;
    }
    return clearra_geometry_catalog_skeleton_supports_clear_state(
        catalog, domain->skeleton_id, deleted_rows);
}

static bool domain_word_range_is_valid(
    const ClearraRealizationDomainPropagationInput *input,
    const ClearraRealizationCandidateDomain *domain) {
    return input != 0 && domain != 0 &&
           domain->active_word_count != 0u &&
           domain->active_word_offset <= input->realization_word_count &&
           domain->active_word_count <=
               input->realization_word_count - domain->active_word_offset;
}

bool clearra_realization_domain_value_is_active(
    const ClearraRealizationDomainPropagationInput *input,
    uint8_t operation,
    uint32_t local_realization_index) {
    if (input == 0 || input->domains == 0 ||
        input->active_realization_words == 0 ||
        operation >= input->operation_count) {
        return false;
    }
    const ClearraRealizationCandidateDomain *domain =
        &input->domains[operation];
    if (local_realization_index >= domain->realization_count ||
        !domain_word_range_is_valid(input, domain)) {
        return false;
    }
    size_t word_index = domain->active_word_offset +
                        local_realization_index / 64u;
    uint64_t bit = UINT64_C(1) << (local_realization_index % 64u);
    return (input->active_realization_words[word_index] & bit) != 0u;
}

static bool realization_required_predecessors(
    const ClearraRealizationDomainPropagationInput *input,
    uint8_t operation,
    uint32_t local_realization_index,
    uint16_t *out_predecessors) {
    const ClearraRealizationCandidateDomain *domain =
        &input->domains[operation];
    const ClearraInverseClearTemplate *template_value =
        clearra_geometry_catalog_template_at_index(
            input->catalog,
            domain->realization_begin + local_realization_index);
    if (template_value == 0 || out_predecessors == 0) {
        return false;
    }
    uint16_t required_rows = template_value->minimum_deleted_row_mask;
    if ((required_rows & ~input->clearable_rows) != 0u) {
        return false;
    }
    uint16_t predecessors = input->required_predecessors[operation];
    while (required_rows != 0u) {
        uint16_t row_bit = (uint16_t)(
            required_rows & (uint16_t)(~required_rows + UINT16_C(1)));
        uint8_t row = 0u;
        for (uint16_t cursor = row_bit;
             (cursor & UINT16_C(1)) == 0u;
             cursor >>= 1u) {
            row++;
        }
        predecessors = (uint16_t)(
            predecessors | input->contributors[row]);
        required_rows = (uint16_t)(required_rows & ~row_bit);
    }
    uint16_t operation_bit = (uint16_t)(UINT16_C(1) << operation);
    if ((predecessors & operation_bit) != 0u) {
        return false;
    }
    *out_predecessors = predecessors;
    return true;
}

static bool transition_has_realization_support(
    const ClearraRealizationDomainPropagationInput *input,
    uint16_t state,
    uint8_t operation,
    bool record_support) {
    const ClearraRealizationCandidateDomain *domain =
        &input->domains[operation];
    uint16_t deleted_rows = clearra_realization_deleted_rows_for_state(
        input->contributors, input->clearable_rows, state);
    bool supported = false;
    for (uint32_t local_index = 0u;
         local_index < domain->realization_count;
         ++local_index) {
        if (!clearra_realization_domain_value_is_active(
                input, operation, local_index)) {
            continue;
        }
        uint16_t predecessors = 0u;
        uint32_t realization_index =
            domain->realization_begin + local_index;
        if (!realization_required_predecessors(
                input, operation, local_index, &predecessors) ||
            (state & predecessors) != predecessors ||
            !clearra_geometry_catalog_realization_supports_clear_state(
                input->catalog, realization_index, deleted_rows)) {
            continue;
        }
        supported = true;
        if (record_support) {
            size_t word_index = domain->active_word_offset +
                                local_index / 64u;
            input->supported_realization_words[word_index] |=
                UINT64_C(1) << (local_index % 64u);
        } else {
            break;
        }
    }
    return supported;
}

bool clearra_realization_domain_common_predecessors(
    const ClearraRealizationDomainPropagationInput *input,
    uint16_t out_predecessors[CLR_BUILDUP_MAX_OPERATIONS]) {
    if (input == 0 || input->domains == 0 ||
        input->active_realization_words == 0 || out_predecessors == 0 ||
        input->operation_count == 0u ||
        input->operation_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return false;
    }
    for (uint8_t operation = 0u;
         operation < input->operation_count;
         ++operation) {
        bool found = false;
        uint16_t common = UINT16_MAX;
        const ClearraRealizationCandidateDomain *domain =
            &input->domains[operation];
        for (uint32_t local_index = 0u;
             local_index < domain->realization_count;
             ++local_index) {
            if (!clearra_realization_domain_value_is_active(
                    input, operation, local_index)) {
                continue;
            }
            uint16_t predecessors = 0u;
            if (!realization_required_predecessors(
                    input, operation, local_index, &predecessors)) {
                return false;
            }
            common = found ? (uint16_t)(common & predecessors)
                           : predecessors;
            found = true;
        }
        if (!found) {
            return false;
        }
        out_predecessors[operation] = common;
    }
    return true;
}

bool clearra_realization_structural_transition_allowed(
    const ClearraRealizationDomainPropagationInput *input,
    uint16_t state,
    uint8_t operation) {
    if (input == 0 || input->catalog == 0 || input->domains == 0 ||
        input->contributors == 0 || input->required_predecessors == 0 ||
        operation >= input->operation_count) {
        return false;
    }
    uint16_t operation_bit = (uint16_t)(UINT16_C(1) << operation);
    if ((state & operation_bit) != 0u ||
        (state & input->required_predecessors[operation]) !=
            input->required_predecessors[operation]) {
        return false;
    }
    return transition_has_realization_support(
        input, state, operation, false);
}

static uint32_t count_bits_u64(uint64_t value) {
    uint32_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

ClearraRealizationDomainPropagationStatus
clearra_realization_domain_propagate(
    const ClearraRealizationDomainPropagationInput *input,
    uint32_t *reachable_generations,
    uint32_t *live_generations,
    size_t state_capacity,
    uint32_t generation,
    ClearraRealizationDomainPropagationResult *out_result) {
    if (input == 0 || input->catalog == 0 || input->domains == 0 ||
        input->contributors == 0 || input->required_predecessors == 0 ||
        input->active_realization_words == 0 ||
        input->supported_realization_words == 0 ||
        input->realization_word_count == 0u ||
        reachable_generations == 0 || live_generations == 0 ||
        out_result == 0 || generation == 0u ||
        input->operation_count == 0u ||
        input->operation_count > CLR_BUILDUP_MAX_OPERATIONS ||
        input->terminal_state !=
            (uint16_t)(((uint32_t)UINT16_C(1) << input->operation_count) - 1u) ||
        state_capacity <= input->terminal_state) {
        return CLEARRA_REALIZATION_DOMAIN_INVALID;
    }
    for (uint8_t operation = 0u;
         operation < input->operation_count;
         ++operation) {
        const ClearraRealizationCandidateDomain *domain =
            &input->domains[operation];
        if (domain->realization_count == 0u ||
            !domain_word_range_is_valid(input, domain)) {
            return CLEARRA_REALIZATION_DOMAIN_INVALID;
        }
    }

    *out_result = (ClearraRealizationDomainPropagationResult){
        .complete = 1u,
    };
    memset(
        input->supported_realization_words,
        0,
        input->realization_word_count *
            sizeof(*input->supported_realization_words));
    reachable_generations[0] = generation;
    out_result->reachable_state_count = 1u;

    for (uint32_t state_value = 0u;
         state_value < input->terminal_state;
         ++state_value) {
        uint16_t state = (uint16_t)state_value;
        if (reachable_generations[state] != generation) {
            continue;
        }
        for (uint8_t operation = 0u;
             operation < input->operation_count;
             ++operation) {
            if (!clearra_realization_structural_transition_allowed(
                    input, state, operation)) {
                continue;
            }
            uint16_t next_state = (uint16_t)(
                state | (uint16_t)(UINT16_C(1) << operation));
            if (reachable_generations[next_state] != generation) {
                reachable_generations[next_state] = generation;
                out_result->reachable_state_count++;
            }
        }
    }

    if (reachable_generations[input->terminal_state] != generation) {
        return CLEARRA_REALIZATION_DOMAIN_INFEASIBLE;
    }

    live_generations[input->terminal_state] = generation;
    out_result->live_state_count = 1u;
    for (uint32_t state_value = input->terminal_state;
         state_value != 0u;
         --state_value) {
        uint16_t state = (uint16_t)(state_value - 1u);
        if (reachable_generations[state] != generation) {
            continue;
        }
        bool state_live = false;
        for (uint8_t operation = 0u;
             operation < input->operation_count;
             ++operation) {
            if ((state & (uint16_t)(UINT16_C(1) << operation)) != 0u) {
                continue;
            }
            uint16_t next_state = (uint16_t)(
                state | (uint16_t)(UINT16_C(1) << operation));
            if (live_generations[next_state] != generation ||
                !transition_has_realization_support(
                    input, state, operation, true)) {
                continue;
            }
            state_live = true;
        }
        if (state_live) {
            live_generations[state] = generation;
            out_result->live_state_count++;
        }
    }

    for (uint8_t operation = 0u;
         operation < input->operation_count;
         ++operation) {
        const ClearraRealizationCandidateDomain *domain =
            &input->domains[operation];
        uint32_t domain_active_count = 0u;
        for (uint32_t word = 0u;
             word < domain->active_word_count;
             ++word) {
            size_t word_index = domain->active_word_offset + word;
            uint64_t before = input->active_realization_words[word_index];
            uint64_t after = before &
                             input->supported_realization_words[word_index];
            input->active_realization_words[word_index] = after;
            out_result->removed_realization_count +=
                count_bits_u64(before & ~after);
            domain_active_count += count_bits_u64(after);
        }
        if (domain_active_count == 0u) {
            return CLEARRA_REALIZATION_DOMAIN_INFEASIBLE;
        }
        out_result->active_realization_count += domain_active_count;
    }

    return live_generations[0] == generation
        ? CLEARRA_REALIZATION_DOMAIN_SUPPORTED
        : CLEARRA_REALIZATION_DOMAIN_INFEASIBLE;
}
