#ifndef CLEARRA_GPU_BATCH_DESCRIPTOR_INTERNAL_H
#define CLEARRA_GPU_BATCH_DESCRIPTOR_INTERNAL_H

#include "gpu_backend.h"
uint64_t clearra_gpu_low_mask_for_cells(uint8_t cell_count);
clr_gpu_piece_multiset_window clearra_gpu_piece_multiset_window_from_pieces(
    const uint8_t *pieces,
    uint8_t piece_count);
bool clearra_gpu_piece_multiset_window_is_valid(
    const clr_gpu_piece_multiset_window *window);
clr_piece_multiset_window clearra_gpu_piece_multiset_window_to_c(
    clr_gpu_piece_multiset_window gpu_window);
ClearraGpuStatus clearra_gpu_piece_source_from_batch(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_piece_source_descriptor *out_source);
#endif
