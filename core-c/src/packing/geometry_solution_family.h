#ifndef CLEARRA_GEOMETRY_SOLUTION_FAMILY_H
#define CLEARRA_GEOMETRY_SOLUTION_FAMILY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLEARRA_GEOMETRY_FAMILY_INVALID UINT32_C(0)
#define CLEARRA_GEOMETRY_FAMILY_EMPTY UINT32_C(1)

typedef uint32_t ClearraGeometryFamilyRef;

typedef enum ClearraGeometryFamilyNodeKind {
    CLEARRA_GEOMETRY_FAMILY_APPEND = 1,
    CLEARRA_GEOMETRY_FAMILY_UNION = 2,
    CLEARRA_GEOMETRY_FAMILY_PRODUCT = 3
} ClearraGeometryFamilyNodeKind;

typedef struct ClearraGeometryFamilyNode {
    uint32_t left;
    uint32_t right;
    uint32_t row_id;
    uint8_t kind;
    uint8_t reserved[3];
} ClearraGeometryFamilyNode;

typedef struct ClearraGeometryFamilyChunk ClearraGeometryFamilyChunk;
typedef struct ClearraGeometryFamilyDirectoryBlock
    ClearraGeometryFamilyDirectoryBlock;
typedef struct ClearraGeometryFamilyInternTable
    ClearraGeometryFamilyInternTable;

typedef struct ClearraGeometrySolutionFamily {
    ClearraGeometryFamilyChunk *chunks;
    ClearraGeometryFamilyChunk *tail;
    ClearraGeometryFamilyDirectoryBlock *directories;
    ClearraGeometryFamilyDirectoryBlock *directory_tail;
    ClearraGeometryFamilyDirectoryBlock **directory_index;
    ClearraGeometryFamilyInternTable *intern_table;
    size_t resident_bytes;
    size_t max_bytes;
    uint32_t node_count;
    uint32_t directory_block_count;
    bool allocation_failed;
    bool interning_disabled;
} ClearraGeometrySolutionFamily;

typedef struct ClearraGeometrySolutionFamilyCheckpoint {
    ClearraGeometryFamilyChunk *tail;
    ClearraGeometryFamilyDirectoryBlock *directory_tail;
    ClearraGeometryFamilyDirectoryBlock **directory_index;
    size_t resident_bytes;
    uint32_t node_count;
    uint32_t directory_block_count;
    uint32_t tail_count;
    uint32_t directory_count;
    bool allocation_failed;
    bool interning_disabled;
} ClearraGeometrySolutionFamilyCheckpoint;

void clearra_geometry_solution_family_init(
    ClearraGeometrySolutionFamily *family,
    size_t max_bytes);
void clearra_geometry_solution_family_release(
    ClearraGeometrySolutionFamily *family);
void clearra_geometry_solution_family_checkpoint_begin(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometrySolutionFamilyCheckpoint *checkpoint);
void clearra_geometry_solution_family_checkpoint_commit(
    ClearraGeometrySolutionFamily *family,
    const ClearraGeometrySolutionFamilyCheckpoint *checkpoint);
void clearra_geometry_solution_family_checkpoint_rollback(
    ClearraGeometrySolutionFamily *family,
    const ClearraGeometrySolutionFamilyCheckpoint *checkpoint);
ClearraGeometryFamilyRef clearra_geometry_solution_family_append(
    ClearraGeometrySolutionFamily *family,
    uint32_t row_id,
    ClearraGeometryFamilyRef suffix);
ClearraGeometryFamilyRef clearra_geometry_solution_family_union(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef left,
    ClearraGeometryFamilyRef right);
ClearraGeometryFamilyRef clearra_geometry_solution_family_product(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef left,
    ClearraGeometryFamilyRef right);
const ClearraGeometryFamilyNode *clearra_geometry_solution_family_node(
    const ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef reference);

#endif
