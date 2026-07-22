#ifndef CLEARRA_GPU_TEST_SUPPORT_H
#define CLEARRA_GPU_TEST_SUPPORT_H

#include "../src/gpu/gpu_backend.h"
#include "../src/gpu/gpu_backend_adapter.h"
#include "../include/clr_gpu_worker.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define EXPECT_GPU_STATUS(EXPR, EXPECTED)                                                \
    do {                                                                                 \
        ClearraGpuStatus actual_status = (EXPR);                                         \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected gpu status %d but got %d\n", __FILE__,       \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_PACKING_STATUS(EXPR, EXPECTED)                                            \
    do {                                                                                 \
        ClearraPackingStatus actual_status = (EXPR);                                     \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected packing status %d but got %d\n", __FILE__,   \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                \
    do {                                                                                 \
        if (!(EXPR)) {                                                                   \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);               \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                               \
    do {                                                                                 \
        if ((EXPR)) {                                                                    \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);              \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                       \
    do {                                                                                 \
        uint64_t actual_value = (uint64_t)(EXPR);                                        \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                  \
        if (actual_value != expected_value) {                                            \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__,         \
                    __LINE__, (unsigned long long)expected_value,                        \
                    (unsigned long long)actual_value);                                   \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)
ClearraBoard64Layout two_line_layout(void);
ClearraGpuPackingBatchDescriptor standard_batch(void);
ClearraGpuPackingBatchDescriptor mixed_piece_batch(void);
ClearraGpuPackingBatchDescriptor collision_batch(void);
ClearraGpuPackingBatchDescriptor c_abi_batch_descriptor(void);
uint64_t shape_hash_for(const ClearraPackingCandidateBuffer *buffer);
uint64_t tiling_hash_for(const ClearraPackingCandidateBuffer *buffer);
uint64_t operation_set_hash_for(const ClearraPackingCandidateBuffer *buffer);
void cpu_reference_for_gpu_batch(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_reference);
void expect_candidate_buffers_match_canonical(
    const ClearraPackingCandidateBuffer *left,
    const ClearraPackingCandidateBuffer *right);
void canonical_hashes_for(
    const ClearraPackingCandidateBuffer *buffer,
    uint64_t *out_shape_hash,
    uint64_t *out_tiling_hash,
    uint64_t *out_operation_set_hash);
#endif
