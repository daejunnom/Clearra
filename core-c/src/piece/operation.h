#ifndef CLEARRA_CORE_C_PIECE_OPERATION_H
#define CLEARRA_CORE_C_PIECE_OPERATION_H

#include "clr_piece.h"
#include "../board/board64.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_STANDARD_TETROMINO_COUNT 7u
#define CLEARRA_TETROMINO_AREA 4u
#define CLEARRA_ROTATION_STATE_COUNT 4u
#define CLEARRA_STANDARD_OPERATION_COUNT \
    (CLEARRA_STANDARD_TETROMINO_COUNT * CLEARRA_ROTATION_STATE_COUNT)
#define CLEARRA_STANDARD_OPERATION_TABLE_VERSION 1u
typedef enum ClearraOperationStatus {
    CLEARRA_OPERATION_OK = 0,
    CLEARRA_OPERATION_INVALID_ARGUMENT = 1,
    CLEARRA_OPERATION_INVALID_PIECE = 2,
    CLEARRA_OPERATION_INVALID_ROTATION = 3,
    CLEARRA_OPERATION_OUT_OF_BOUNDS = 4
} ClearraOperationStatus;typedef enum ClearraRotationState {
    CLEARRA_ROTATION_SPAWN = 0,
    CLEARRA_ROTATION_RIGHT = 1,
    CLEARRA_ROTATION_REVERSE = 2,
    CLEARRA_ROTATION_LEFT = 3
} ClearraRotationState;typedef struct ClearraCellOffset {
    int8_t x;
    int8_t y;
} ClearraCellOffset;typedef struct ClearraOperationBounds {
    int8_t min_x;
    int8_t min_y;
    int8_t max_x;
    int8_t max_y;
    uint8_t width;
    uint8_t height;
} ClearraOperationBounds;typedef struct ClearraPieceShape {
    uint8_t piece;
    uint8_t rotation;
    uint8_t area;
    ClearraCellOffset cells[CLEARRA_TETROMINO_AREA];
} ClearraPieceShape;typedef struct ClearraOperation {
    uint16_t operation_id;
    uint8_t piece;
    uint8_t rotation;
    uint8_t area;
    ClearraCellOffset cells[CLEARRA_TETROMINO_AREA];
    ClearraOperationBounds bounds;
    uint64_t shape_mask;
} ClearraOperation;typedef struct ClearraOperationTable {
    ClearraOperation operations[CLEARRA_STANDARD_OPERATION_COUNT];
    uint16_t count;
} ClearraOperationTable;typedef struct ClearraOperationSet {
    ClearraOperation operations[CLEARRA_STANDARD_OPERATION_COUNT];
    uint16_t count;
} ClearraOperationSet;bool clearra_piece_is_standard_tetromino(uint8_t piece);
const char *clearra_piece_name(uint8_t piece);
uint8_t clearra_piece_area(uint8_t piece);
bool clearra_standard_tetromino_piece_at(uint8_t index, uint8_t *out_piece);
ClearraOperationStatus clearra_tetromino_shape(
    uint8_t piece,
    uint8_t rotation,
    const ClearraPieceShape **out_shape);bool clearra_rotation_state_is_valid(uint8_t rotation);
const char *clearra_rotation_state_name(uint8_t rotation);
uint8_t clearra_rotation_count_for_piece(uint8_t piece);ClearraOperationStatus clearra_operation_id(
    uint8_t piece,
    uint8_t rotation,
    uint16_t *out_operation_id);
ClearraOperationStatus clearra_operation_from_shape(
    uint8_t piece,
    uint8_t rotation,
    ClearraOperation *out_operation);
ClearraOperationStatus clearra_operation_mask(
    ClearraBoard64Layout layout,
    const ClearraOperation *operation,
    int8_t x,
    int8_t y,
    uint64_t *out_mask);ClearraOperationStatus clearra_operation_table_generate(
    ClearraOperationTable *out_table);
void clearra_operation_set_clear(ClearraOperationSet *set);
ClearraOperationStatus clearra_operation_set_push(
    ClearraOperationSet *set,
    ClearraOperation operation);
uint16_t clearra_operation_set_count_piece(
    const ClearraOperationSet *set,
    uint8_t piece);
ClearraOperationStatus clearra_operation_set_from_table_for_piece(
    const ClearraOperationTable *table,
    uint8_t piece,
    ClearraOperationSet *out_set);
#endif
