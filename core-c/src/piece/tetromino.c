#include "operation.h"

static const uint8_t STANDARD_PIECES[CLEARRA_STANDARD_TETROMINO_COUNT] = {
    CLR_PIECE_I,
    CLR_PIECE_O,
    CLR_PIECE_T,
    CLR_PIECE_S,
    CLR_PIECE_Z,
    CLR_PIECE_J,
    CLR_PIECE_L,
};

static const ClearraPieceShape STANDARD_SHAPES[CLEARRA_STANDARD_TETROMINO_COUNT]
                                                 [CLEARRA_ROTATION_STATE_COUNT] = {
    {
        {CLR_PIECE_I, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {2, 0}, {3, 0}}},
        {CLR_PIECE_I, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {0, 2}, {0, 3}}},
        {CLR_PIECE_I, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {2, 0}, {3, 0}}},
        {CLR_PIECE_I, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {0, 2}, {0, 3}}},
    },
    {
        {CLR_PIECE_O, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {0, 1}, {1, 1}}},
        {CLR_PIECE_O, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {0, 1}, {1, 1}}},
        {CLR_PIECE_O, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {0, 1}, {1, 1}}},
        {CLR_PIECE_O, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {0, 1}, {1, 1}}},
    },
    {
        {CLR_PIECE_T, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {2, 0}, {1, 1}}},
        {CLR_PIECE_T, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {0, 2}, {1, 1}}},
        {CLR_PIECE_T, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{0, 1}, {1, 1}, {2, 1}, {1, 0}}},
        {CLR_PIECE_T, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{1, 0}, {1, 1}, {1, 2}, {0, 1}}},
    },
    {
        {CLR_PIECE_S, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {1, 1}, {2, 1}}},
        {CLR_PIECE_S, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{1, 0}, {0, 1}, {1, 1}, {0, 2}}},
        {CLR_PIECE_S, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {1, 1}, {2, 1}}},
        {CLR_PIECE_S, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{1, 0}, {0, 1}, {1, 1}, {0, 2}}},
    },
    {
        {CLR_PIECE_Z, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{1, 0}, {2, 0}, {0, 1}, {1, 1}}},
        {CLR_PIECE_Z, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {1, 1}, {1, 2}}},
        {CLR_PIECE_Z, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{1, 0}, {2, 0}, {0, 1}, {1, 1}}},
        {CLR_PIECE_Z, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {1, 1}, {1, 2}}},
    },
    {
        {CLR_PIECE_J, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {2, 0}, {0, 1}}},
        {CLR_PIECE_J, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {0, 2}, {1, 2}}},
        {CLR_PIECE_J, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{2, 0}, {0, 1}, {1, 1}, {2, 1}}},
        {CLR_PIECE_J, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {1, 1}, {1, 2}}},
    },
    {
        {CLR_PIECE_L, CLEARRA_ROTATION_SPAWN, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {1, 0}, {2, 0}, {2, 1}}},
        {CLR_PIECE_L, CLEARRA_ROTATION_RIGHT, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {0, 2}, {1, 0}}},
        {CLR_PIECE_L, CLEARRA_ROTATION_REVERSE, CLEARRA_TETROMINO_AREA,
         {{0, 0}, {0, 1}, {1, 1}, {2, 1}}},
        {CLR_PIECE_L, CLEARRA_ROTATION_LEFT, CLEARRA_TETROMINO_AREA,
         {{0, 2}, {1, 0}, {1, 1}, {1, 2}}},
    },
};
static bool standard_piece_index(uint8_t piece, uint8_t *out_index) {
    if (piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
        return false;
    }
    if (out_index != 0) {
        *out_index = (uint8_t)(piece - CLR_PIECE_I);
    }
    return true;
}bool clearra_piece_is_standard_tetromino(uint8_t piece) {
    return standard_piece_index(piece, 0);
}const char *clearra_piece_name(uint8_t piece) {
    switch (piece) {
        case CLR_PIECE_I:
            return "I";
        case CLR_PIECE_O:
            return "O";
        case CLR_PIECE_T:
            return "T";
        case CLR_PIECE_S:
            return "S";
        case CLR_PIECE_Z:
            return "Z";
        case CLR_PIECE_J:
            return "J";
        case CLR_PIECE_L:
            return "L";
        default:
            return "unknown";
    }
}uint8_t clearra_piece_area(uint8_t piece) {
    return clearra_piece_is_standard_tetromino(piece) ? CLEARRA_TETROMINO_AREA : 0u;
}bool clearra_standard_tetromino_piece_at(uint8_t index, uint8_t *out_piece) {
    if (out_piece == 0 || index >= CLEARRA_STANDARD_TETROMINO_COUNT) {
        return false;
    }
    *out_piece = STANDARD_PIECES[index];
    return true;
}ClearraOperationStatus clearra_tetromino_shape(
    uint8_t piece,
    uint8_t rotation,
    const ClearraPieceShape **out_shape) {
    uint8_t index = 0;
    if (out_shape == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (!standard_piece_index(piece, &index)) {
        return CLEARRA_OPERATION_INVALID_PIECE;
    }
    if (!clearra_rotation_state_is_valid(rotation)) {
        return CLEARRA_OPERATION_INVALID_ROTATION;
    }
    *out_shape = &STANDARD_SHAPES[index][rotation];
    return CLEARRA_OPERATION_OK;
}
