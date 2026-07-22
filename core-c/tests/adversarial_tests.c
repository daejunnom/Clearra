#include <stdio.h>

void frontier_hash_collision_does_not_merge_distinct_partial_states(void);
void failed_memo_hash_collision_does_not_merge_distinct_hold_states(void);
void reachability_capacity_exceeded_is_incomplete_not_impossible(void);
void buildup_exports_actual_success_operation_order(void);
int pruning_tests_main(void);

int main(void) {
    frontier_hash_collision_does_not_merge_distinct_partial_states();
    failed_memo_hash_collision_does_not_merge_distinct_hold_states();
    reachability_capacity_exceeded_is_incomplete_not_impossible();
    buildup_exports_actual_success_operation_order();
    if (pruning_tests_main() != 0) {
        return 1;
    }
    puts("core-c adversarial correctness tests passed");
    return 0;
}
