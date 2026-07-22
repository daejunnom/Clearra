#ifndef CLEARRA_GEOMETRY_CATALOG_INTERNAL_H
#define CLEARRA_GEOMETRY_CATALOG_INTERNAL_H

#include "packing_problem.h"
#include "../pruning/geometry_projection_reachability.h"

#define CLEARRA_REALIZATION_CHUNK_CAPACITY 128u
#define CLEARRA_SKELETON_PROJECTION_VERSION UINT64_C(2)

typedef struct ClearraInverseClearTemplate {
    uint64_t canonical_cell_ownership;
    uint32_t realization_id;
    uint16_t minimum_deleted_row_mask;
    uint16_t using_row_mask;
    uint16_t inverse_template_id;
    uint16_t operation_id;
    uint64_t rule_capability;
    int8_t target_x;
    int8_t target_anchor_y;
    uint8_t piece;
    uint8_t rotation;
} ClearraInverseClearTemplate;

typedef struct ClearraConcreteRealization {
    uint64_t world_cell_mask;
    uint64_t canonical_cell_ownership;
    uint64_t projection_evidence_digest;
    uint64_t forward_replay_evidence_digest;
    uint32_t realization_id;
    uint16_t clear_state_deleted_row_mask;
    uint16_t inserted_row_mask;
    uint16_t need_deleted_mask;
    uint16_t using_row_mask;
    uint16_t completed_row_mask;
    uint16_t inverse_template_id;
    uint16_t operation_id;
    uint64_t rule_capability;
    int8_t lock_x;
    int8_t lock_y;
    int8_t target_x;
    int8_t target_anchor_y;
    uint8_t piece;
    uint8_t rotation;
} ClearraConcreteRealization;

typedef struct ClearraRealizationChunk {
    struct ClearraRealizationChunk *next;
    uint32_t count;
    ClearraInverseClearTemplate items[CLEARRA_REALIZATION_CHUNK_CAPACITY];
} ClearraRealizationChunk;

typedef struct ClearraGeometryPieceProjection {
    ClearraGeometryColumnSignature signature;
    uint8_t piece;
    uint8_t reserved[3];
} ClearraGeometryPieceProjection;

struct ClearraGeometryCatalog {
    ClearraGeometryCatalogIdentity identity;
    ClearraBoard64Layout layout;
    uint64_t initial_board;
    uint64_t goal_region_mask;
    uint64_t required_fill_mask;
    uint64_t forbidden_mask;
    uint32_t skeleton_count;
    uint32_t realization_count;
    uint32_t realization_payload_count;
    uint32_t support_entry_count;
    uint32_t *skeleton_piece_kind;
    uint64_t *skeleton_cell_mask;
    uint32_t *skeleton_realization_offset;
    uint32_t *skeleton_realization_count;
    uint32_t *skeleton_parent_row_id;
    uint64_t *skeleton_deleted_state_bits;
    uint64_t *realization_deleted_state_bits;
    uint16_t *skeleton_using_row_mask;
    uint16_t *skeleton_required_deleted_rows;
    uint8_t *skeleton_additive_signatures;
    uint8_t *skeleton_apdp_support_flags;
    uint64_t *skeleton_column_projection_low;
    uint32_t *skeleton_column_projection_high;
    ClearraGeometryPieceProjection *piece_column_projections;
    uint32_t piece_projection_offsets[CLR_STANDARD_PIECE_KIND_COUNT + 1u];
    uint32_t piece_column_projection_count;
    ClearraInverseClearTemplate **realization_refs;
    uint32_t cell_support_offsets[65];
    uint32_t *cell_support_row_ids;
    ClearraRealizationChunk *realization_chunks;
    ClearraRealizationChunk *realization_tail;
    size_t resident_bytes;
    size_t max_resident_bytes;
};

bool clearra_geometry_catalog_matches_problem(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem);

static inline const ClearraInverseClearTemplate *
clearra_geometry_catalog_template_at_index(
    const ClearraGeometryCatalog *catalog,
    uint32_t realization_index) {
    return catalog == 0 || realization_index >= catalog->realization_count
        ? 0
        : catalog->realization_refs[realization_index];
}

static inline const ClearraInverseClearTemplate *
clearra_geometry_catalog_representative_template(
    const ClearraGeometryCatalog *catalog,
    uint32_t skeleton_id) {
    if (catalog == 0 || skeleton_id >= catalog->skeleton_count ||
        catalog->skeleton_realization_count[skeleton_id] == 0u) {
        return 0;
    }
    return clearra_geometry_catalog_template_at_index(
        catalog, catalog->skeleton_realization_offset[skeleton_id]);
}

bool clearra_geometry_catalog_find_skeleton(
    const ClearraGeometryCatalog *catalog,
    uint8_t piece,
    uint64_t canonical_cell_ownership,
    uint32_t *out_skeleton_id);

bool clearra_geometry_catalog_instantiate_realization(
    const ClearraGeometryCatalog *catalog,
    const ClearraInverseClearTemplate *template_value,
    uint16_t deleted_row_mask,
    ClearraConcreteRealization *out_realization);

bool clearra_geometry_catalog_skeleton_supports_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint32_t skeleton_id,
    uint16_t deleted_row_mask);

bool clearra_geometry_catalog_realization_supports_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint32_t realization_index,
    uint16_t deleted_row_mask);

ClearraPackingStatus clearra_geometry_catalog_realizations_for_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint8_t piece,
    uint64_t canonical_cell_ownership,
    uint16_t deleted_row_mask,
    ClearraPlacementCandidate
        out_variants[CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS],
    uint8_t *out_count);

#endif
