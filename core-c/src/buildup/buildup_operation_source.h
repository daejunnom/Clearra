#ifndef CLEARRA_BUILDUP_OPERATION_SOURCE_H
#define CLEARRA_BUILDUP_OPERATION_SOURCE_H

#include "buildup_internal.h"

typedef enum ClearraBuildUpOperationSourceKind {
    CLEARRA_BUILDUP_OPERATION_SOURCE_OWNED = 0,
    CLEARRA_BUILDUP_OPERATION_SOURCE_CATALOG_ROWS = 1
} ClearraBuildUpOperationSourceKind;

typedef struct ClearraBuildUpOperationSource {
    const clr_buildup_problem *problem;
    const ClearraGeometryCatalog *catalog;
    const uint32_t *catalog_row_ids;
    const uint8_t *representative_order_hint;
    const uint16_t *required_predecessors;
    uint16_t operation_count;
    uint8_t kind;
    uint8_t reserved;
} ClearraBuildUpOperationSource;

clr_buildup_status clearra_buildup_operation_source_from_problem(
    const clr_buildup_problem *problem,
    ClearraBuildUpOperationSource *out_source);
clr_buildup_status clearra_buildup_operation_source_from_catalog_rows(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    ClearraBuildUpOperationSource *out_source);
clr_buildup_status clearra_buildup_operation_source_operation_at(
    const ClearraBuildUpOperationSource *source,
    uint16_t operation_index,
    clr_buildup_operation *out_operation);
clr_buildup_status clearra_buildup_operation_source_order(
    const ClearraBuildUpOperationSource *source,
    ClearraBuildUpOrder *out_order);
bool clearra_buildup_operation_source_has_geometry_domain(
    const ClearraBuildUpOperationSource *source,
    uint16_t operation_index);
bool clearra_buildup_operation_source_may_match_clear_state(
    const ClearraBuildUpOperationSource *source,
    const ClearraBuildUpState *state,
    uint16_t operation_index);

#endif
