#include "clr_piece.h"
clr_piece_window_descriptor clearra_piece_window_descriptor(
    uint16_t max_pieces,
    uint16_t exact_pieces,
    bool has_exact_pieces) {
    clr_piece_window_descriptor window = {0};
    window.max_pieces = max_pieces;
    window.exact_pieces = exact_pieces;
    window.has_exact_pieces = has_exact_pieces ? 1u : 0u;
    return window;
}bool clearra_piece_window_has_exact(const clr_piece_window_descriptor *window) {
    return window != 0 && window->has_exact_pieces != 0u;
}clr_piece_multiset_window clearra_piece_multiset_window_empty(void) {
    clr_piece_multiset_window window = {0};
    return window;
}clr_piece_multiset_window clearra_piece_multiset_window_from_pieces(
    const uint8_t *pieces,
    uint16_t piece_count) {
    clr_piece_multiset_window window = clearra_piece_multiset_window_empty();
    if (pieces == 0) {
        return window;
    }
    uint16_t count = piece_count;
    if (count > CLR_PIECE_MULTISET_WINDOW_CAPACITY) {
        count = CLR_PIECE_MULTISET_WINDOW_CAPACITY;
    }
    window.total_count = (uint8_t)count;
    window.exact_count = (uint8_t)count;
    for (uint16_t index = 0u; index < count; ++index) {
        uint8_t piece = pieces[index];
        if (piece >= CLR_PIECE_I && piece <= CLR_PIECE_L) {
            window.counts[piece]++;
        }
    }
    return window;
}bool clearra_piece_multiset_window_is_valid(
    const clr_piece_multiset_window *window) {
    uint16_t counted = 0u;
    if (window == 0 || window->total_count == 0u ||
        window->total_count > CLR_PIECE_MULTISET_WINDOW_CAPACITY ||
        window->counts[CLR_PIECE_NONE] != 0u ||
        window->exact_count > window->total_count) {
        return false;
    }
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        counted = (uint16_t)(counted + window->counts[piece]);
    }
    return counted == window->total_count;
}

clr_piece_multiset_family clearra_piece_multiset_family_empty(void) {
    clr_piece_multiset_family family = {0};
    return family;
}

static bool member_fits_envelope(
    const clr_piece_multiset_window *member,
    const clr_piece_multiset_window *envelope) {
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        if (member->counts[piece] > envelope->counts[piece]) {
            return false;
        }
    }
    return member->total_count <= envelope->total_count;
}

bool clearra_piece_multiset_family_is_valid(
    const clr_piece_multiset_family *family,
    const clr_piece_multiset_window *envelope) {
    if (family == 0 || envelope == 0 ||
        family->count > CLR_PIECE_MULTISET_FAMILY_CAPACITY) {
        return false;
    }
    if (family->count == 0u) {
        return true;
    }
    if (family->complete == 0u ||
        !clearra_piece_multiset_window_is_valid(envelope)) {
        return false;
    }
    for (uint16_t index = 0u; index < family->count; ++index) {
        const clr_piece_multiset_window *member = &family->members[index];
        if (!clearra_piece_multiset_window_is_valid(member) ||
            member->exact_count != member->total_count ||
            !member_fits_envelope(member, envelope)) {
            return false;
        }
    }
    return true;
}

static bool counts_match_member(
    const clr_piece_multiset_window *member,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT],
    bool exact) {
    uint16_t used_total = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint8_t used = used_piece_counts[piece];
        used_total = (uint16_t)(used_total + used);
        if (used > member->counts[piece] ||
            (exact && used != member->counts[piece])) {
            return false;
        }
    }
    return exact ? used_total == member->total_count
                 : used_total <= member->total_count;
}

bool clearra_piece_multiset_family_can_complete_prefix(
    const clr_piece_multiset_family *family,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    if (family == 0 || used_piece_counts == 0 || family->count == 0u) {
        return true;
    }
    for (uint16_t index = 0u; index < family->count; ++index) {
        if (counts_match_member(&family->members[index], used_piece_counts, false)) {
            return true;
        }
    }
    return false;
}

bool clearra_piece_multiset_family_contains_exact(
    const clr_piece_multiset_family *family,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    if (family == 0 || used_piece_counts == 0 || family->count == 0u) {
        return true;
    }
    for (uint16_t index = 0u; index < family->count; ++index) {
        if (counts_match_member(&family->members[index], used_piece_counts, true)) {
            return true;
        }
    }
    return false;
}

uint64_t clearra_piece_multiset_family_digest(
    const clr_piece_multiset_family *family) {
    uint64_t hash = UINT64_C(1469598103934665603);
    if (family == 0) {
        return hash;
    }
    hash ^= family->count;
    hash *= UINT64_C(1099511628211);
    for (uint16_t index = 0u; index < family->count; ++index) {
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            hash ^= family->members[index].counts[piece];
            hash *= UINT64_C(1099511628211);
        }
    }
    return hash == 0u ? UINT64_C(1) : hash;
}
