#ifndef CLEARRA_PACKING_PROBLEM_H
#define CLEARRA_PACKING_PROBLEM_H

#include "../board/board64.h"
#include "../cache/cache_identity.h"
#include "../piece/operation.h"

#include "clr_problem.h"
#include "clr_pruning.h"
#include "clr_resource_budget.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLEARRA_PACKING_MAX_PIECES 15u
#define CLEARRA_PACKING_MAX_PLACEMENT_CANDIDATES 2048u
#define CLEARRA_PACKING_MAX_CANDIDATES 8192u
#define CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS 4u
typedef enum ClearraPackingStatus {
    CLEARRA_PACKING_OK = 0,
    CLEARRA_PACKING_INVALID_ARGUMENT = 1,
    CLEARRA_PACKING_INVALID_LAYOUT = 2,
    CLEARRA_PACKING_INVALID_PIECE = 3,
    CLEARRA_PACKING_OUT_OF_BOUNDS = 4,
    CLEARRA_PACKING_COLLISION = 5,
    CLEARRA_PACKING_CAPACITY_EXCEEDED = 6,
    CLEARRA_PACKING_CANCELLED = 7
} ClearraPackingStatus;typedef struct ClearraPlacementCandidate {
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint16_t operation_id;
    uint16_t required_deleted_row_mask;
    uint64_t mask;
} ClearraPlacementCandidate;

typedef ClearraPackingStatus (*ClearraPlacementCandidateVisitor)(
    void *context,
    const ClearraPlacementCandidate *candidate);

typedef struct ClearraPlacementCandidateList {
    ClearraPlacementCandidate candidates[CLEARRA_PACKING_MAX_PLACEMENT_CANDIDATES];
    uint16_t count;
} ClearraPlacementCandidateList;typedef struct ClearraPackingCandidateView {
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    uint64_t final_board;
    uint64_t shape_mask;
    uint64_t shape_key;
    uint64_t tiling_key;
    uint64_t operation_set_key;
    uint8_t placed_count;
    uint8_t cleared_lines;
    uint16_t geometry_variant_domains;
    uint8_t pieces[CLEARRA_PACKING_MAX_PIECES];
    uint8_t rotations[CLEARRA_PACKING_MAX_PIECES];
    int8_t xs[CLEARRA_PACKING_MAX_PIECES];
    int8_t ys[CLEARRA_PACKING_MAX_PIECES];
    uint16_t operation_ids[CLEARRA_PACKING_MAX_PIECES];
    uint16_t operation_deleted_row_masks[CLEARRA_PACKING_MAX_PIECES];
    uint64_t operation_masks[CLEARRA_PACKING_MAX_PIECES];
} ClearraPackingCandidateView;

typedef ClearraPackingStatus (*ClearraPackingCandidateConsumer)(
    void *context,
    const ClearraPackingCandidateView *candidate,
    size_t accepted_candidate_count,
    size_t engine_resident_bytes,
    size_t max_candidate_rows,
    size_t max_total_bytes,
    uint8_t *out_inserted,
    uint16_t *out_truncation_reason,
    size_t *out_host_resident_bytes);

typedef struct ClearraPackingCandidateSink {
    void *context;
    /* Producers leave candidate identity unset. The sink owns exact
       deduplication and deterministic public identity assignment. */
    ClearraPackingCandidateConsumer consume;
} ClearraPackingCandidateSink;

typedef struct ClearraPackingCandidateBuffer {
    uint16_t count;
    uint64_t final_boards[CLEARRA_PACKING_MAX_CANDIDATES];
    uint64_t shape_masks[CLEARRA_PACKING_MAX_CANDIDATES];
    uint64_t shape_keys[CLEARRA_PACKING_MAX_CANDIDATES];
    uint64_t tiling_keys[CLEARRA_PACKING_MAX_CANDIDATES];
    uint64_t operation_set_keys[CLEARRA_PACKING_MAX_CANDIDATES];
    uint8_t placed_counts[CLEARRA_PACKING_MAX_CANDIDATES];
    uint8_t cleared_lines[CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t geometry_variant_domains[CLEARRA_PACKING_MAX_CANDIDATES];
    uint8_t pieces[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
    uint8_t rotations[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
    int8_t xs[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
    int8_t ys[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t operation_ids[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t operation_deleted_row_masks[CLEARRA_PACKING_MAX_PIECES]
                                        [CLEARRA_PACKING_MAX_CANDIDATES];
    uint64_t operation_masks[CLEARRA_PACKING_MAX_PIECES][CLEARRA_PACKING_MAX_CANDIDATES];
} ClearraPackingCandidateBuffer;typedef struct ClearraCanonicalPackingTable {
    ClearraPackingCandidateBuffer candidates;
    uint16_t raw_count;
    uint16_t candidate_ids[CLEARRA_PACKING_MAX_CANDIDATES];
    uint16_t raw_to_canonical_ids[CLEARRA_PACKING_MAX_CANDIDATES];
} ClearraCanonicalPackingTable;void clearra_placement_candidate_list_clear(ClearraPlacementCandidateList *list);
ClearraPackingStatus clearra_placement_candidate_list_push(
    ClearraPlacementCandidateList *list,
    ClearraPlacementCandidate candidate);
ClearraPackingStatus clearra_placement_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    ClearraPlacementCandidateList *out_list);
ClearraPackingStatus clearra_placement_candidates_generate_with_pruning_ledger(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    const clr_static_prune_context *base_prune_context,
    clr_pruning_proof_ledger *ledger,
    ClearraPlacementCandidateList *out_list);
ClearraPackingStatus clearra_placement_candidates_visit_with_pruning_ledger(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    const clr_static_prune_context *base_prune_context,
    clr_pruning_proof_ledger *ledger,
    ClearraPlacementCandidateVisitor visitor,
    void *visitor_context);
ClearraPackingStatus clearra_placement_geometry_variants_at_deleted_rows(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    ClearraPlacementCandidate
        out_variants[CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS],
    uint8_t *out_count);
void clearra_packing_candidate_view_clear(ClearraPackingCandidateView *candidate);
void clearra_packing_candidate_buffer_clear(ClearraPackingCandidateBuffer *buffer);
ClearraPackingStatus clearra_packing_candidate_buffer_push(
    ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate,
    uint16_t *out_index);
ClearraPackingStatus clearra_packing_candidate_buffer_candidate_at(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    ClearraPackingCandidateView *out_candidate);
ClearraPackingStatus clearra_packing_target_mask_for_lines(
    ClearraBoard64Layout layout,
    uint8_t target_lines,
    uint64_t *out_mask);
ClearraPackingStatus clearra_packing_pruner_accepts_static_candidate_with_ledger(
    ClearraBoard64Layout layout,
    uint64_t occupied_board,
    uint64_t target_mask,
    uint64_t placement_mask,
    const clr_static_prune_context *context,
    clr_pruning_proof_ledger *ledger,
    bool *out_accepts);
bool clearra_packing_prune_context_is_valid(
    const clr_static_prune_context *context);
ClearraPackingStatus clearra_packing_prune_context_from_problem(
    const clr_packing_problem *problem,
    clr_static_prune_context *out_context);
ClearraPackingStatus clearra_packing_prune_context_for_geometry(
    ClearraBoard64Layout layout,
    uint64_t occupied_board,
    uint64_t target_mask,
    clr_static_prune_context *out_context);
uint64_t clearra_packing_shape_key(ClearraBoard64Layout layout, uint64_t shape_mask);
uint64_t clearra_packing_cell_partition_key(
    ClearraBoard64Layout layout,
    const uint64_t *operation_masks,
    uint8_t operation_count);
uint64_t clearra_packing_tiling_key_with_piece_identity(
    ClearraBoard64Layout layout,
    const uint8_t *pieces,
    const uint8_t *rotations,
    const uint64_t *operation_masks,
    const uint16_t *operation_deleted_row_masks,
    uint8_t operation_count);
uint64_t clearra_packing_geometry_tiling_key(
    ClearraBoard64Layout layout,
    const uint8_t *pieces,
    const uint64_t *operation_masks,
    uint8_t operation_count);
uint64_t clearra_packing_operation_set_key(
    const ClearraPackingCandidateView *candidate);uint64_t clearra_packing_candidate_identity_key(
    const ClearraPackingCandidateView *candidate);
uint16_t clearra_packing_hash_bucket(uint64_t key, uint16_t bucket_count);
bool clearra_packing_hash_confirm_exact(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    const ClearraPackingCandidateView *candidate);
bool clearra_packing_hash_confirm_same_operation_set(
    const ClearraPackingCandidateBuffer *buffer,
    uint16_t index,
    const ClearraPackingCandidateView *candidate);
bool clearra_packing_candidate_buffer_exactly_matches(
    const ClearraPackingCandidateBuffer *left,
    const ClearraPackingCandidateBuffer *right);
ClearraPackingStatus clearra_packing_deduper_push_unique(
    ClearraPackingCandidateBuffer *buffer,
    const ClearraPackingCandidateView *candidate,
    uint16_t *out_index,
    bool *out_inserted);
void clearra_canonical_packing_table_clear(ClearraCanonicalPackingTable *table);
ClearraPackingStatus clearra_packing_host_reduce(
    const ClearraPackingCandidateBuffer *raw_buffer,
    ClearraCanonicalPackingTable *out_table);
ClearraPackingStatus clearra_buildup_problem_from_packing_candidate(
    const clr_packing_problem *packing,
    const ClearraPackingCandidateView *candidate,
    uint32_t coverage_pattern_id,
    clr_buildup_problem *out_problem);
ClearraPackingStatus clearra_buildup_problem_apply_packing_candidate(
    clr_buildup_problem *problem,
    const ClearraPackingCandidateView *candidate,
    uint32_t coverage_pattern_id);
#if defined(CLEARRA_CORE_TEST)
ClearraPackingStatus clearra_packing_enumerator_cpu_generate(
    ClearraBoard64Layout layout,
    uint64_t initial_board,
    uint8_t target_lines,
    const uint8_t *pieces,
    uint8_t piece_count,
    ClearraPackingCandidateBuffer *out_buffer);
ClearraPackingStatus clearra_packing_enumerator_cpu_generate_problem(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer);
ClearraPackingStatus clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report);
ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_with_resource_report_pruning_policy_and_ledger(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_partition_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    uint16_t root_partition_index,
    uint16_t root_partition_count,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_prefix_partition_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
#endif

typedef struct ClearraGeometryCatalog ClearraGeometryCatalog;
typedef struct ClearraGeometrySolutionGraph ClearraGeometrySolutionGraph;
typedef struct clr_buildup_workspace clr_buildup_workspace;

typedef struct ClearraGeometryCatalogIdentity {
    uint64_t board_layout_id;
    uint64_t compact_universe_digest;
    uint64_t target_geometry_digest;
    uint64_t piece_catalog_id;
    uint64_t skeleton_projection_version;
    uint64_t rule_capability_id;
    uint64_t realization_table_digest;
    uint64_t support_table_digest;
} ClearraGeometryCatalogIdentity;

typedef struct ClearraGeometryCatalogView {
    ClearraGeometryCatalogIdentity identity;
    const uint64_t *skeleton_cell_masks;
    const uint32_t *skeleton_piece_kinds;
    const uint32_t *skeleton_realization_offsets;
    const uint32_t *skeleton_realization_counts;
    const uint32_t *cell_support_offsets;
    const uint32_t *cell_support_row_ids;
    uint32_t skeleton_count;
    uint32_t realization_count;
    uint32_t support_entry_count;
    uint32_t cell_count;
} ClearraGeometryCatalogView;

ClearraPackingStatus clearra_geometry_catalog_compile(
    const clr_packing_problem *problem,
    clr_resource_report *out_resource_report,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraGeometryCatalog **out_catalog);
void clearra_geometry_catalog_release(ClearraGeometryCatalog **catalog);
const ClearraGeometryCatalogIdentity *clearra_geometry_catalog_identity(
    const ClearraGeometryCatalog *catalog);
size_t clearra_geometry_catalog_resident_bytes(
    const ClearraGeometryCatalog *catalog);
uint32_t clearra_geometry_catalog_skeleton_count(
    const ClearraGeometryCatalog *catalog);
uint32_t clearra_geometry_catalog_realization_count(
    const ClearraGeometryCatalog *catalog);
bool clearra_geometry_catalog_borrow_view(
    const ClearraGeometryCatalog *catalog,
    ClearraGeometryCatalogView *out_view);

typedef struct ClearraGeometrySolutionTask {
    uint32_t family_ref;
    uint32_t prefix_row_ids[CLEARRA_PACKING_MAX_PIECES];
    uint32_t continuation_family_refs[CLEARRA_PACKING_MAX_PIECES];
    uint8_t prefix_count;
    uint8_t continuation_count;
    uint8_t reserved[2];
} ClearraGeometrySolutionTask;

typedef struct ClearraGeometryPathView {
    const uint32_t *skeleton_row_ids;
    uint8_t operation_count;
    uint8_t reserved[7];
} ClearraGeometryPathView;

typedef ClearraPackingStatus (*ClearraGeometryPathConsumer)(
    void *context,
    const ClearraGeometryPathView *path);

typedef struct ClearraGeometryPathSink {
    void *context;
    ClearraGeometryPathConsumer consume;
} ClearraGeometryPathSink;

typedef struct ClearraBuildableGeometryStreamReport {
    uint64_t generated_count;
    uint64_t buildable_count;
    size_t workspace_retained_bytes;
    size_t host_resident_bytes;
    int32_t buildup_status;
    uint16_t truncation_reason;
    uint8_t complete;
    uint8_t candidate_buildable;
} ClearraBuildableGeometryStreamReport;

ClearraPackingStatus clearra_geometry_exact_cover_search_graph(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report);
ClearraPackingStatus
clearra_geometry_exact_cover_search_graph_with_pruning_ledger(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    clr_pruning_evidence_policy evidence_policy,
    ClearraGeometrySolutionGraph **out_graph,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
void clearra_geometry_solution_graph_release(
    ClearraGeometrySolutionGraph **graph);
size_t clearra_geometry_solution_graph_resident_bytes(
    const ClearraGeometrySolutionGraph *graph);
uint32_t clearra_geometry_solution_graph_node_count(
    const ClearraGeometrySolutionGraph *graph);
bool clearra_geometry_solution_graph_matches_catalog(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometryCatalogIdentity *catalog_identity);
ClearraPackingStatus clearra_geometry_solution_graph_split_tasks(
    const ClearraGeometrySolutionGraph *graph,
    ClearraGeometrySolutionTask *tasks,
    uint32_t task_capacity,
    uint32_t *out_task_count,
    size_t *out_peak_scratch_bytes);
ClearraPackingStatus clearra_geometry_solution_graph_stream_task_paths(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometrySolutionTask *task,
    const ClearraGeometryPathSink *sink,
    uint64_t *out_emitted_count);
ClearraPackingStatus clearra_geometry_solution_graph_stream_buildable_task(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometryCatalog *catalog,
    const ClearraGeometrySolutionTask *task,
    const clr_packing_problem *packing_problem,
    clr_buildup_problem *buildup_scratch,
    clr_buildup_workspace *buildup_workspace,
    const ClearraPackingCandidateSink *sink,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraBuildableGeometryStreamReport *out_report);

ClearraPackingStatus clearra_geometry_catalog_rows_buildable_to_sink(
    const ClearraGeometryCatalog *catalog,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    const clr_packing_problem *packing_problem,
    clr_buildup_problem *buildup_scratch,
    clr_buildup_workspace *buildup_workspace,
    const ClearraPackingCandidateSink *sink,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraBuildableGeometryStreamReport *out_report);

ClearraPackingStatus clearra_packing_materialize_catalog_row_ids(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraPackingCandidateView *out_candidate);

ClearraPackingStatus clearra_geometry_exact_cover_search_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report);
ClearraPackingStatus clearra_geometry_exact_cover_search_family_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t family_begin,
    uint16_t family_end,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report);
ClearraPackingStatus clearra_geometry_exact_cover_search_to_buffer(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report);
#endif
