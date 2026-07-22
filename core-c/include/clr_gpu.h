#ifndef CLR_GPU_H
#define CLR_GPU_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum ClearraGpuStatus {
    CLEARRA_GPU_OK = 0,
    CLEARRA_GPU_INVALID_ARGUMENT = 1,
    CLEARRA_GPU_UNAVAILABLE = 2,
    CLEARRA_GPU_PACKING_ERROR = 3
} ClearraGpuStatus;

typedef enum ClearraGpuUnavailableReason {
    CLEARRA_GPU_UNAVAILABLE_NONE = 0,
    CLEARRA_GPU_UNAVAILABLE_FEATURE_DISABLED = 1,
    CLEARRA_GPU_UNAVAILABLE_DEVICE_NOT_FOUND = 2,
    CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE = 3
} ClearraGpuUnavailableReason;

typedef enum ClearraGpuBackendKind {
    CLEARRA_GPU_BACKEND_NATIVE_COMPUTE = 1,
    CLEARRA_GPU_BACKEND_DISABLED = 255
} ClearraGpuBackendKind;

typedef struct ClearraGpuDeviceRequest {
    uint8_t device_kind;
    uint8_t device_index;
} ClearraGpuDeviceRequest;

typedef struct ClearraGpuBackendCapability {
    ClearraGpuBackendKind backend_kind;
    uint8_t available;
    uint8_t connected;
    uint8_t exact_supported;
    uint8_t accepts_user_shader_path;
    ClearraGpuUnavailableReason unavailable_reason;
} ClearraGpuBackendCapability;

ClearraGpuStatus clearra_gpu_device_capability_query(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability);

#define CLEARRA_GPU_BATCH_MAX_PIECES 15u
#define CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_VERSION 5u
#define CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_SIZE 112u
#define CLEARRA_GPU_PIECE_SOURCE_UNKNOWN 0u
#define CLEARRA_GPU_PIECE_SOURCE_FIXED_SEQUENCE 1u
#define CLEARRA_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN 2u
#define CLEARRA_GPU_PIECE_SOURCE_OBSERVED_WINDOW 3u

typedef struct clr_gpu_piece_multiset_window {
    uint8_t counts[8];
    uint8_t total_count;
    uint8_t exact_count;
    uint8_t reserved[6];
} clr_gpu_piece_multiset_window;

typedef struct ClearraGpuPackingBatchDescriptor {
    uint64_t batch_id;
    uint8_t board_width;
    uint8_t board_height;
    uint8_t active_packing_rows;
    uint8_t goal_clear_lines_hint;
    uint8_t piece_window;
    uint8_t piece_count;
    uint8_t exact_piece_count;
    uint8_t piece_source_kind;
    uint64_t piece_source_id;
    clr_gpu_piece_multiset_window piece_multiset_window;
    uint64_t initial_board_mask;
    uint64_t operation_table_id;
    uint64_t rule_profile_id;
    uint64_t kick_profile_id;
    uint32_t candidate_capacity;
    uint32_t max_frontier_states;
    uint32_t pattern_count;
    uint64_t shape_hash_seed;
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
} ClearraGpuPackingBatchDescriptor;

#ifdef __cplusplus
}
#endif

#endif
