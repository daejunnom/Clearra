#ifndef CLR_PIECE_H
#define CLR_PIECE_H

#include <stdbool.h>
#include <stdint.h>

#define CLR_PIECE_NONE 0u
#define CLR_PIECE_I 1u
#define CLR_PIECE_O 2u
#define CLR_PIECE_T 3u
#define CLR_PIECE_S 4u
#define CLR_PIECE_Z 5u
#define CLR_PIECE_J 6u
#define CLR_PIECE_L 7u

#define CLR_PIECE_MULTISET_WINDOW_CAPACITY 15u
#define CLR_PIECE_MULTISET_FAMILY_CAPACITY 256u
#define CLR_STANDARD_PIECE_KIND_COUNT 8u

typedef struct clr_piece_window_descriptor {
    uint16_t max_pieces;
    uint16_t exact_pieces;
    uint8_t has_exact_pieces;
    uint8_t reserved[3];
} clr_piece_window_descriptor;typedef struct clr_piece_multiset_window {
    uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT];
    uint8_t total_count;
    uint8_t exact_count;
    uint8_t reserved[6];
} clr_piece_multiset_window;
typedef struct clr_piece_multiset_family {
    clr_piece_multiset_window members[CLR_PIECE_MULTISET_FAMILY_CAPACITY];
    uint16_t count;
    uint8_t complete;
    uint8_t reserved[5];
} clr_piece_multiset_family;
clr_piece_window_descriptor clearra_piece_window_descriptor(
    uint16_t max_pieces,
    uint16_t exact_pieces,
    bool has_exact_pieces);
bool clearra_piece_window_has_exact(const clr_piece_window_descriptor *window);
clr_piece_multiset_window clearra_piece_multiset_window_empty(void);
clr_piece_multiset_window clearra_piece_multiset_window_from_pieces(
    const uint8_t *pieces,
    uint16_t piece_count);
bool clearra_piece_multiset_window_is_valid(
    const clr_piece_multiset_window *window);
clr_piece_multiset_family clearra_piece_multiset_family_empty(void);
bool clearra_piece_multiset_family_is_valid(
    const clr_piece_multiset_family *family,
    const clr_piece_multiset_window *envelope);
bool clearra_piece_multiset_family_can_complete_prefix(
    const clr_piece_multiset_family *family,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]);
bool clearra_piece_multiset_family_contains_exact(
    const clr_piece_multiset_family *family,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]);
uint64_t clearra_piece_multiset_family_digest(
    const clr_piece_multiset_family *family);
#endif
