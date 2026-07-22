#include "operation.h"

ClearraOperationStatus clearra_operation_table_generate(
    ClearraOperationTable *out_table) {
    if (out_table == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }

    out_table->count = 0;
    for (uint8_t piece_index = 0; piece_index < CLEARRA_STANDARD_TETROMINO_COUNT;
         piece_index++) {
        uint8_t piece = CLR_PIECE_NONE;
        if (!clearra_standard_tetromino_piece_at(piece_index, &piece)) {
            return CLEARRA_OPERATION_INVALID_PIECE;
        }
        for (uint8_t rotation = 0; rotation < CLEARRA_ROTATION_STATE_COUNT;
             rotation++) {
            ClearraOperation operation;
            ClearraOperationStatus status =
                clearra_operation_from_shape(piece, rotation, &operation);
            if (status != CLEARRA_OPERATION_OK) {
                return status;
            }
            if (operation.operation_id != out_table->count) {
                return CLEARRA_OPERATION_INVALID_ARGUMENT;
            }
            out_table->operations[out_table->count] = operation;
            out_table->count++;
        }
    }

    return CLEARRA_OPERATION_OK;
}
