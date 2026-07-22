#include <stdio.h>
#include <string.h>

int clearra_core_version_tests_main(void);
int memory_tests_main(void);
int board64_tests_main(void);
int test_board64_main(void);
int board_backend_dispatch_tests_main(void);
int field_tests_main(void);
int operation_table_tests_main(void);
int rule_profile_tests_main(void);
int supply_tests_main(void);
int cache_identity_tests_main(void);
int candidate_tests_main(void);
int reachability_tests_main(void);
int problem_descriptor_tests_main(void);
int packing_tests_main(void);
int pruning_tests_main(void);
int gpu_tests_main(void);
int scheduler_tests_main(void);
int buildup_tests_main(void);
int coverage_tests_main(void);
int scoring_event_tests_main(void);
int external_pc_solution_tests_main(void);
int geometry_benchmark_main(int argc, char **argv);
static int run_core_test(const char *name, int (*test_main)(void)) {
    printf("%s\n", name);
    fflush(stdout);
    int status = test_main();
    if (status != 0) {
        fprintf(stderr, "%s failed with status %d\n", name, status);
        return 1;
    }
    return 0;
}

static int should_run_test(int argc, char **argv, const char *name) {
    if (argc <= 1) {
        return 1;
    }
    for (int index = 1; index < argc; ++index) {
        if (strcmp(argv[index], name) == 0) {
            return 1;
        }
    }
    return 0;
}

#define RUN_SELECTED_TEST(name, test_main) \
    do { \
        if (should_run_test(argc, argv, name)) { \
            failures += run_core_test(name, test_main); \
        } \
    } while (0)

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "geometry_benchmark") == 0) {
        return geometry_benchmark_main(argc - 1, argv + 1);
    }
    int failures = 0;

    RUN_SELECTED_TEST("clearra_core_version_tests", clearra_core_version_tests_main);
    RUN_SELECTED_TEST("memory_tests", memory_tests_main);
    RUN_SELECTED_TEST("board64_tests", board64_tests_main);
    RUN_SELECTED_TEST("test_board64", test_board64_main);
    RUN_SELECTED_TEST("board_backend_dispatch_tests", board_backend_dispatch_tests_main);
    RUN_SELECTED_TEST("field_tests", field_tests_main);
    RUN_SELECTED_TEST("operation_table_tests", operation_table_tests_main);
    RUN_SELECTED_TEST("rule_profile_tests", rule_profile_tests_main);
    RUN_SELECTED_TEST("supply_tests", supply_tests_main);
    RUN_SELECTED_TEST("cache_identity_tests", cache_identity_tests_main);
    RUN_SELECTED_TEST("candidate_tests", candidate_tests_main);
    RUN_SELECTED_TEST("reachability_tests", reachability_tests_main);
    RUN_SELECTED_TEST("problem_descriptor_tests", problem_descriptor_tests_main);
    RUN_SELECTED_TEST("packing_tests", packing_tests_main);
    RUN_SELECTED_TEST("pruning_tests", pruning_tests_main);
    RUN_SELECTED_TEST("gpu_tests", gpu_tests_main);
    RUN_SELECTED_TEST("scheduler_tests", scheduler_tests_main);
    RUN_SELECTED_TEST("buildup_tests", buildup_tests_main);
    RUN_SELECTED_TEST("coverage_tests", coverage_tests_main);
    RUN_SELECTED_TEST("scoring_event_tests", scoring_event_tests_main);
    RUN_SELECTED_TEST("external_pc_solution_tests", external_pc_solution_tests_main);

    if (failures != 0) {
        return 1;
    }

    puts("core-c aggregate tests passed");
    return 0;
}

#undef RUN_SELECTED_TEST
