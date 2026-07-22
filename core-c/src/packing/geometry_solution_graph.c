#include "geometry_solution_graph_internal.h"

#include "clr_execution_control.h"

#include <stdlib.h>
#include <string.h>

static bool catalog_identity_matches(
    const ClearraGeometryCatalogIdentity *left,
    const ClearraGeometryCatalogIdentity *right) {
    return memcmp(left, right, sizeof(*left)) == 0;
}

void clearra_geometry_solution_graph_release(
    ClearraGeometrySolutionGraph **graph) {
    if (graph == 0 || *graph == 0) {
        return;
    }
    clearra_geometry_solution_family_release(&(*graph)->family);
    free(*graph);
    *graph = 0;
}

size_t clearra_geometry_solution_graph_resident_bytes(
    const ClearraGeometrySolutionGraph *graph) {
    return graph == 0 ? 0u : graph->resident_bytes;
}

uint32_t clearra_geometry_solution_graph_node_count(
    const ClearraGeometrySolutionGraph *graph) {
    return graph == 0 ? 0u : graph->family.node_count;
}

bool clearra_geometry_solution_graph_matches_catalog(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometryCatalogIdentity *catalog_identity) {
    return graph != 0 && graph->complete != 0u && catalog_identity != 0 &&
           catalog_identity_matches(&graph->catalog_identity, catalog_identity);
}

static bool advance_task_to_branch(
    const ClearraGeometrySolutionGraph *graph,
    ClearraGeometrySolutionTask *task) {
    for (;;) {
        if (task->family_ref == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            return false;
        }
        if (task->family_ref == CLEARRA_GEOMETRY_FAMILY_EMPTY) {
            if (task->continuation_count != 0u) {
                task->family_ref = task->continuation_family_refs[
                    --task->continuation_count];
                continue;
            }
            return task->prefix_count == graph->target_depth;
        }
        const ClearraGeometryFamilyNode *node =
            clearra_geometry_solution_family_node(
                &graph->family, task->family_ref);
        if (node == 0) {
            return false;
        }
        if (node->kind == CLEARRA_GEOMETRY_FAMILY_UNION) {
            return true;
        }
        if (node->kind == CLEARRA_GEOMETRY_FAMILY_PRODUCT) {
            if (task->continuation_count >= CLEARRA_PACKING_MAX_PIECES) {
                return false;
            }
            task->continuation_family_refs[task->continuation_count++] =
                node->right;
            task->family_ref = node->left;
            continue;
        }
        if (node->kind != CLEARRA_GEOMETRY_FAMILY_APPEND ||
            task->prefix_count >= graph->target_depth ||
            node->row_id >= graph->skeleton_count) {
            return false;
        }
        task->prefix_row_ids[task->prefix_count++] = node->row_id;
        task->family_ref = node->left;
    }
}

static uint64_t saturated_path_add(uint64_t left, uint64_t right) {
    return left > UINT64_MAX - right ? UINT64_MAX : left + right;
}

static uint64_t saturated_path_multiply(uint64_t left, uint64_t right) {
    return left != 0u && right > UINT64_MAX / left
        ? UINT64_MAX
        : left * right;
}

static uint64_t *build_family_path_counts(
    const ClearraGeometrySolutionGraph *graph) {
    size_t count = (size_t)graph->family.node_count + 2u;
    if (count > SIZE_MAX / sizeof(uint64_t)) {
        return 0;
    }
    uint64_t *path_counts = (uint64_t *)malloc(count * sizeof(uint64_t));
    if (path_counts == 0) {
        return 0;
    }
    path_counts[CLEARRA_GEOMETRY_FAMILY_INVALID] = 0u;
    path_counts[CLEARRA_GEOMETRY_FAMILY_EMPTY] = 1u;
    for (ClearraGeometryFamilyRef reference = 2u;
         reference < count;
         ++reference) {
        const ClearraGeometryFamilyNode *node =
            clearra_geometry_solution_family_node(
                &graph->family, reference);
        if (node == 0 || node->left >= reference ||
            (node->kind != CLEARRA_GEOMETRY_FAMILY_APPEND &&
             node->right >= reference)) {
            free(path_counts);
            return 0;
        }
        if (node->kind == CLEARRA_GEOMETRY_FAMILY_APPEND) {
            path_counts[reference] = path_counts[node->left];
        } else if (node->kind == CLEARRA_GEOMETRY_FAMILY_UNION) {
            path_counts[reference] = saturated_path_add(
                path_counts[node->left], path_counts[node->right]);
        } else if (node->kind == CLEARRA_GEOMETRY_FAMILY_PRODUCT) {
            path_counts[reference] = saturated_path_multiply(
                path_counts[node->left], path_counts[node->right]);
        } else {
            free(path_counts);
            return 0;
        }
    }
    return path_counts;
}

static uint64_t task_path_count(
    const ClearraGeometrySolutionTask *task,
    const uint64_t *path_counts) {
    if (path_counts == 0) {
        return 1u;
    }
    uint64_t count = path_counts[task->family_ref];
    for (uint8_t index = 0u; index < task->continuation_count; ++index) {
        count = saturated_path_multiply(
            count, path_counts[task->continuation_family_refs[index]]);
    }
    return count;
}

ClearraPackingStatus clearra_geometry_solution_graph_split_tasks(
    const ClearraGeometrySolutionGraph *graph,
    ClearraGeometrySolutionTask *tasks,
    uint32_t task_capacity,
    uint32_t *out_task_count,
    size_t *out_peak_scratch_bytes) {
    if (graph == 0 || graph->complete == 0u || tasks == 0 ||
        task_capacity == 0u || out_task_count == 0 ||
        out_peak_scratch_bytes == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_task_count = 0u;
    *out_peak_scratch_bytes = 0u;
    if (graph->root == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_PACKING_OK;
    }

    tasks[0] = (ClearraGeometrySolutionTask){.family_ref = graph->root};
    if (task_capacity == 1u) {
        if (!advance_task_to_branch(graph, &tasks[0])) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        *out_task_count = 1u;
        return CLEARRA_PACKING_OK;
    }
    uint32_t task_count = 1u;
    uint64_t *path_counts = build_family_path_counts(graph);
    if (path_counts != 0) {
        *out_peak_scratch_bytes =
            ((size_t)graph->family.node_count + 2u) * sizeof(*path_counts);
    }
    while (task_count < task_capacity) {
        uint32_t split_index = UINT32_MAX;
        uint64_t split_path_count = 0u;
        for (uint32_t index = 0u; index < task_count; ++index) {
            if (!advance_task_to_branch(graph, &tasks[index])) {
                free(path_counts);
                return CLEARRA_PACKING_INVALID_ARGUMENT;
            }
            const ClearraGeometryFamilyNode *node =
                clearra_geometry_solution_family_node(
                    &graph->family, tasks[index].family_ref);
            uint64_t candidate_path_count =
                task_path_count(&tasks[index], path_counts);
            if (node != 0 && node->kind == CLEARRA_GEOMETRY_FAMILY_UNION &&
                (split_index == UINT32_MAX ||
                 candidate_path_count > split_path_count)) {
                split_index = index;
                split_path_count = candidate_path_count;
            }
        }
        if (split_index == UINT32_MAX) {
            break;
        }

        ClearraGeometrySolutionTask right = tasks[split_index];
        const ClearraGeometryFamilyNode *node =
            clearra_geometry_solution_family_node(
                &graph->family, tasks[split_index].family_ref);
        tasks[split_index].family_ref = node->left;
        right.family_ref = node->right;
        memmove(
            &tasks[split_index + 2u],
            &tasks[split_index + 1u],
            (task_count - split_index - 1u) * sizeof(*tasks));
        tasks[split_index + 1u] = right;
        task_count++;
    }
    for (uint32_t index = 0u; index < task_count; ++index) {
        if (!advance_task_to_branch(graph, &tasks[index])) {
            free(path_counts);
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
    }
    free(path_counts);
    *out_task_count = task_count;
    return CLEARRA_PACKING_OK;
}

typedef struct ClearraGeometryPathTraversal {
    const ClearraGeometrySolutionGraph *graph;
    const ClearraGeometryPathSink *sink;
    uint32_t rows[CLEARRA_PACKING_MAX_PIECES];
    uint64_t emitted_count;
    uint32_t cancellation_poll_counter;
} ClearraGeometryPathTraversal;

static ClearraPackingStatus stream_family(
    ClearraGeometryPathTraversal *traversal,
    ClearraGeometryFamilyRef family,
    uint8_t depth,
    ClearraGeometryFamilyRef *continuations,
    uint8_t continuation_count) {
    if (clr_execution_control_poll(&traversal->cancellation_poll_counter)) {
        return CLEARRA_PACKING_CANCELLED;
    }
    if (family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_PACKING_OK;
    }
    if (family == CLEARRA_GEOMETRY_FAMILY_EMPTY) {
        if (continuation_count != 0u) {
            return stream_family(
                traversal,
                continuations[continuation_count - 1u],
                depth,
                continuations,
                (uint8_t)(continuation_count - 1u));
        }
        if (depth != traversal->graph->target_depth) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        ClearraGeometryPathView path = {
            .skeleton_row_ids = traversal->rows,
            .operation_count = depth,
        };
        ClearraPackingStatus status =
            traversal->sink->consume(traversal->sink->context, &path);
        if (status == CLEARRA_PACKING_OK) {
            traversal->emitted_count++;
        }
        return status;
    }

    const ClearraGeometryFamilyNode *node =
        clearra_geometry_solution_family_node(&traversal->graph->family, family);
    if (node == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (node->kind == CLEARRA_GEOMETRY_FAMILY_APPEND) {
        if (depth >= traversal->graph->target_depth ||
            node->row_id >= traversal->graph->skeleton_count) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        traversal->rows[depth] = node->row_id;
        return stream_family(
            traversal,
            node->left,
            (uint8_t)(depth + 1u),
            continuations,
            continuation_count);
    }
    if (node->kind == CLEARRA_GEOMETRY_FAMILY_PRODUCT) {
        if (continuation_count >= CLEARRA_PACKING_MAX_PIECES) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        continuations[continuation_count] = node->right;
        return stream_family(
            traversal,
            node->left,
            depth,
            continuations,
            (uint8_t)(continuation_count + 1u));
    }
    if (node->kind != CLEARRA_GEOMETRY_FAMILY_UNION) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraPackingStatus status = stream_family(
        traversal,
        node->left,
        depth,
        continuations,
        continuation_count);
    return status == CLEARRA_PACKING_OK
        ? stream_family(
              traversal,
              node->right,
              depth,
              continuations,
              continuation_count)
        : status;
}

ClearraPackingStatus clearra_geometry_solution_graph_stream_task_paths(
    const ClearraGeometrySolutionGraph *graph,
    const ClearraGeometrySolutionTask *task,
    const ClearraGeometryPathSink *sink,
    uint64_t *out_emitted_count) {
    if (graph == 0 || graph->complete == 0u || task == 0 || sink == 0 ||
        sink->consume == 0 || out_emitted_count == 0 ||
        task->prefix_count > graph->target_depth ||
        task->continuation_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraGeometryPathTraversal traversal = {
        .graph = graph,
        .sink = sink,
    };
    memcpy(
        traversal.rows,
        task->prefix_row_ids,
        (size_t)task->prefix_count * sizeof(*traversal.rows));
    ClearraGeometryFamilyRef continuations[CLEARRA_PACKING_MAX_PIECES];
    memcpy(
        continuations,
        task->continuation_family_refs,
        (size_t)task->continuation_count * sizeof(*continuations));
    ClearraPackingStatus status = stream_family(
        &traversal,
        task->family_ref,
        task->prefix_count,
        continuations,
        task->continuation_count);
    *out_emitted_count = traversal.emitted_count;
    return status;
}
