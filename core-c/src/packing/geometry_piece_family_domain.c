#include "geometry_piece_family_domain.h"

#include <string.h>

static bool active_is_empty(const ClearraActivePieceFamily *active) {
    for (uint16_t word = 0u;
         word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
         ++word) {
        if (active->words[word] != 0u) {
            return false;
        }
    }
    return true;
}

bool clearra_geometry_piece_family_domain_compile(
    const clr_piece_multiset_family *family,
    uint16_t family_begin,
    uint16_t family_end,
    ClearraGeometryPieceFamilyDomain *out_domain) {
    if (family == 0 || out_domain == 0 ||
        family->count > CLR_PIECE_MULTISET_FAMILY_CAPACITY ||
        (family->count == 0u && (family_begin != 0u || family_end != 0u)) ||
        (family->count != 0u &&
         (family_begin >= family_end || family_end > family->count))) {
        return false;
    }

    memset(out_domain, 0, sizeof(*out_domain));
    if (family->count == 0u) {
        return true;
    }
    out_domain->constrained = 1u;
    for (uint16_t member_index = family_begin;
         member_index < family_end;
         ++member_index) {
        uint16_t word = (uint16_t)(member_index / 64u);
        uint64_t bit = UINT64_C(1) << (member_index % 64u);
        out_domain->initial.words[word] |= bit;
        const clr_piece_multiset_window *member = &family->members[member_index];
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            uint8_t maximum = member->counts[piece];
            for (uint8_t count = 0u; count <= maximum; ++count) {
                out_domain->at_least_count[piece][count][word] |= bit;
            }
        }
    }
    return !active_is_empty(&out_domain->initial);
}

bool clearra_geometry_piece_family_advance(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    uint8_t piece,
    uint8_t next_piece_count,
    ClearraActivePieceFamily *out_active) {
    if (domain == 0 || active == 0 || out_active == 0 ||
        piece < CLR_PIECE_I || piece > CLR_PIECE_L ||
        next_piece_count > CLR_PIECE_MULTISET_WINDOW_CAPACITY) {
        return false;
    }
    if (domain->constrained == 0u) {
        *out_active = *active;
        return true;
    }
    for (uint16_t word = 0u;
         word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
         ++word) {
        out_active->words[word] =
            active->words[word] & domain->at_least_count[piece][next_piece_count][word];
    }
    return !active_is_empty(out_active);
}

bool clearra_geometry_piece_family_exact_match(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    if (domain == 0 || active == 0 || used_piece_counts == 0) {
        return false;
    }
    if (domain->constrained == 0u) {
        return true;
    }

    ClearraActivePieceFamily exact = *active;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint8_t count = used_piece_counts[piece];
        if (count > CLR_PIECE_MULTISET_WINDOW_CAPACITY) {
            return false;
        }
        for (uint16_t word = 0u;
             word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
             ++word) {
            uint64_t at_least_next = count == CLR_PIECE_MULTISET_WINDOW_CAPACITY
                ? UINT64_C(0)
                : domain->at_least_count[piece][count + 1u][word];
            exact.words[word] &=
                domain->at_least_count[piece][count][word] & ~at_least_next;
        }
        if (active_is_empty(&exact)) {
            return false;
        }
    }
    return true;
}

uint16_t clearra_geometry_piece_family_remaining_count_mask(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    uint8_t piece,
    uint8_t current_count,
    uint8_t maximum_count) {
    if (domain == 0 || active == 0 || piece < CLR_PIECE_I ||
        piece > CLR_PIECE_L || current_count > maximum_count ||
        maximum_count > CLR_PIECE_MULTISET_WINDOW_CAPACITY) {
        return 0u;
    }
    if (domain->constrained == 0u) {
        return (uint16_t)(UINT16_C(1) << (maximum_count - current_count));
    }

    uint16_t remaining_counts = 0u;
    for (uint8_t final_count = current_count;
         final_count <= maximum_count;
         ++final_count) {
        bool present = false;
        for (uint16_t word = 0u;
             word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
             ++word) {
            uint64_t at_least_next =
                final_count == CLR_PIECE_MULTISET_WINDOW_CAPACITY
                ? UINT64_C(0)
                : domain->at_least_count[piece][final_count + 1u][word];
            uint64_t exact = active->words[word] &
                             domain->at_least_count[piece][final_count][word] &
                             ~at_least_next;
            if (exact != 0u) {
                present = true;
                break;
            }
        }
        if (present) {
            remaining_counts = (uint16_t)(
                remaining_counts |
                (uint16_t)(UINT16_C(1) << (final_count - current_count)));
        }
    }
    return remaining_counts;
}
