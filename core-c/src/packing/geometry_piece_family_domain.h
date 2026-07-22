#ifndef CLEARRA_GEOMETRY_PIECE_FAMILY_DOMAIN_H
#define CLEARRA_GEOMETRY_PIECE_FAMILY_DOMAIN_H

#include "clr_piece.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT \
    ((CLR_PIECE_MULTISET_FAMILY_CAPACITY + 63u) / 64u)
#define CLEARRA_PIECE_FAMILY_COUNT_LIMIT \
    (CLR_PIECE_MULTISET_WINDOW_CAPACITY + 1u)

typedef struct ClearraActivePieceFamily {
    uint64_t words[CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT];
} ClearraActivePieceFamily;

typedef struct ClearraGeometryPieceFamilyDomain {
    uint64_t at_least_count[CLR_STANDARD_PIECE_KIND_COUNT]
                            [CLEARRA_PIECE_FAMILY_COUNT_LIMIT + 1u]
                            [CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT];
    ClearraActivePieceFamily initial;
    uint8_t constrained;
} ClearraGeometryPieceFamilyDomain;

bool clearra_geometry_piece_family_domain_compile(
    const clr_piece_multiset_family *family,
    uint16_t family_begin,
    uint16_t family_end,
    ClearraGeometryPieceFamilyDomain *out_domain);

bool clearra_geometry_piece_family_advance(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    uint8_t piece,
    uint8_t next_piece_count,
    ClearraActivePieceFamily *out_active);

bool clearra_geometry_piece_family_exact_match(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]);

uint16_t clearra_geometry_piece_family_remaining_count_mask(
    const ClearraGeometryPieceFamilyDomain *domain,
    const ClearraActivePieceFamily *active,
    uint8_t piece,
    uint8_t current_count,
    uint8_t maximum_count);

#endif
