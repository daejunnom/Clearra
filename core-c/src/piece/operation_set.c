#include "operation.h"
void clearra_operation_set_clear(ClearraOperationSet *set) {
    if (set != 0) {
        set->count = 0;
    }
}ClearraOperationStatus clearra_operation_set_push(
    ClearraOperationSet *set,
    ClearraOperation operation) {
    if (set == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (!clearra_piece_is_standard_tetromino(operation.piece)) {
        return CLEARRA_OPERATION_INVALID_PIECE;
    }
    if (!clearra_rotation_state_is_valid(operation.rotation)) {
        return CLEARRA_OPERATION_INVALID_ROTATION;
    }
    if (set->count >= CLEARRA_STANDARD_OPERATION_COUNT) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }

    set->operations[set->count] = operation;
    set->count++;
    return CLEARRA_OPERATION_OK;
}uint16_t clearra_operation_set_count_piece(
    const ClearraOperationSet *set,
    uint8_t piece) {
    if (set == 0 || !clearra_piece_is_standard_tetromino(piece)) {
        return 0;
    }

    uint16_t count = 0;
    for (uint16_t index = 0; index < set->count; index++) {
        if (set->operations[index].piece == piece) {
            count++;
        }
    }
    return count;
}ClearraOperationStatus clearra_operation_set_from_table_for_piece(
    const ClearraOperationTable *table,
    uint8_t piece,
    ClearraOperationSet *out_set) {
    if (table == 0 || out_set == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (!clearra_piece_is_standard_tetromino(piece)) {
        return CLEARRA_OPERATION_INVALID_PIECE;
    }

    clearra_operation_set_clear(out_set);
    for (uint16_t index = 0; index < table->count; index++) {
        if (table->operations[index].piece == piece) {
            ClearraOperationStatus status =
                clearra_operation_set_push(out_set, table->operations[index]);
            if (status != CLEARRA_OPERATION_OK) {
                return status;
            }
        }
    }
    return CLEARRA_OPERATION_OK;
}