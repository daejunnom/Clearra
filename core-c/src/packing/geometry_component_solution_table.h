#ifndef CLEARRA_GEOMETRY_COMPONENT_SOLUTION_TABLE_H
#define CLEARRA_GEOMETRY_COMPONENT_SOLUTION_TABLE_H

#include "geometry_solution_family.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct ClearraGeometryComponentSolutionEntry {
    struct ClearraGeometryComponentSolutionEntry *next_in_bucket;
    uint32_t piece_count_signature;
    ClearraGeometryFamilyRef family_ref;
} ClearraGeometryComponentSolutionEntry;

_Static_assert(
    sizeof(ClearraGeometryComponentSolutionEntry) == 16u,
    "component solution entries must remain cache-compact");

typedef struct ClearraGeometryComponentSolutionChunk
    ClearraGeometryComponentSolutionChunk;

typedef struct ClearraGeometryComponentSolutionTable {
    ClearraGeometryComponentSolutionEntry **buckets;
    size_t bucket_count;
    ClearraGeometryComponentSolutionChunk *chunks;
    ClearraGeometryComponentSolutionChunk *tail;
    size_t resident_bytes;
    size_t max_bytes;
    size_t entry_count;
    bool allocation_failed;
    bool bucket_growth_disabled;
} ClearraGeometryComponentSolutionTable;

typedef enum ClearraGeometryComponentInsertStatus {
    CLEARRA_GEOMETRY_COMPONENT_INSERT_OK = 0,
    CLEARRA_GEOMETRY_COMPONENT_TABLE_UNAVAILABLE = 1,
    CLEARRA_GEOMETRY_COMPONENT_FAMILY_UNAVAILABLE = 2
} ClearraGeometryComponentInsertStatus;

typedef struct ClearraGeometryComponentSolutionIterator {
    const ClearraGeometryComponentSolutionChunk *chunk;
    uint32_t index;
} ClearraGeometryComponentSolutionIterator;

bool clearra_geometry_component_solution_table_init(
    ClearraGeometryComponentSolutionTable *table,
    size_t expected_signatures,
    size_t max_bytes);
void clearra_geometry_component_solution_table_release(
    ClearraGeometryComponentSolutionTable *table);
ClearraGeometryComponentInsertStatus
clearra_geometry_component_solution_table_insert(
    ClearraGeometryComponentSolutionTable *table,
    ClearraGeometrySolutionFamily *family,
    uint32_t piece_count_signature,
    ClearraGeometryFamilyRef family_ref);
void clearra_geometry_component_solution_iterator_begin(
    const ClearraGeometryComponentSolutionTable *table,
    ClearraGeometryComponentSolutionIterator *iterator);
const ClearraGeometryComponentSolutionEntry *
clearra_geometry_component_solution_iterator_next(
    ClearraGeometryComponentSolutionIterator *iterator);

#endif
