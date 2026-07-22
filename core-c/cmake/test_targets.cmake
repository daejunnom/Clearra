# Test Manifest

if(NOT TARGET clearra_core_test_oracle)
    add_library(
        clearra_core_test_oracle STATIC
        ${CLEARRA_CORE_TEST_ORACLE_SOURCES}
    )
    target_include_directories(clearra_core_test_oracle
        PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include ${CMAKE_CURRENT_SOURCE_DIR}/src
    )
    target_compile_definitions(clearra_core_test_oracle PRIVATE CLEARRA_CORE_TEST=1)
    target_link_libraries(
        clearra_core_test_oracle
        PUBLIC clearra_core
        PRIVATE clearra_core_sanitizer_options
    )
    clearra_core_enable_strict_warnings(clearra_core_test_oracle)
endif()

set(CLEARRA_CORE_TEST_NAMES
    clearra_core_version_tests memory_tests board64_tests test_board64
    board_backend_dispatch_tests field_tests operation_table_tests
    rule_profile_tests supply_tests cache_identity_tests candidate_tests
    reachability_tests problem_descriptor_tests packing_tests pruning_tests
    gpu_tests scheduler_tests buildup_tests coverage_tests scoring_event_tests
    external_pc_solution_tests
)
set(CLEARRA_CORE_TEST_SOURCES
    tests/version_tests.c tests/memory_tests.c tests/board64_tests.c
    tests/test_board64.c tests/board_backend_dispatch_tests.c tests/field_tests.c
    tests/operation_table_tests.c tests/rule_profile_tests.c tests/supply_tests.c
    tests/cache_identity_tests.c tests/candidate_tests.c tests/reachability_tests.c
    tests/problem_descriptor_tests.c tests/packing_tests.c tests/pruning_tests.c
    tests/gpu_tests.c tests/scheduler_tests.c tests/buildup_tests.c
    tests/coverage_tests.c tests/scoring_event_tests.c
    tests/external_pc_solution_tests.c
)

# Gpu Test Sources

set(gpu_tests_EXTRA_SOURCES
    tests/gpu_test_support.c
    tests/gpu_descriptor_tests.c
    tests/gpu_backend_adapter_tests.c
    tests/gpu_reference_tests.c
    tests/gpu_worker_tests.c
)

# Candidate Test Sources

set(candidate_tests_EXTRA_SOURCES
    tests/candidate_tests_support.c
    tests/candidate_harddrop_tests.c
    tests/candidate_locked_tests.c
    tests/candidate_kick_transition_tests.c
    tests/candidate_cache_dedupe_tests.c
)

# Packing Test Sources

set(packing_tests_EXTRA_SOURCES
    tests/packing_tests_support.c
    tests/packing_problem_tests.c
    tests/packing_window_tests.c
    tests/placement_candidate_tests.c
    tests/packing_buffer_hash_tests.c
    tests/packing_operation_set_tests.c
    tests/geometry_exact_cover_tests.c
)

# Scheduler Test Sources

set(scheduler_tests_EXTRA_SOURCES
    tests/hybrid_support/hybrid_scheduler.c
    tests/hybrid_support/hybrid_buildup_dispatch.c
    tests/hybrid_support/hybrid_candidate_queue.c
    tests/hybrid_support/hybrid_backpressure.c
    tests/hybrid_support/batch_planner.c
    tests/scheduler_tests_support.c
    tests/scheduler_gpu_product_tests.c
    tests/scheduler_backpressure_tests.c
    tests/scheduler_autotune_tests.c
    tests/scheduler_memory_fallback_tests.c
)

# Buildup Test Sources

set(buildup_tests_EXTRA_SOURCES
    tests/buildup_tests_support.c
    tests/buildup_enumeration_support.c
    tests/buildup_problem_tests.c
    tests/buildup_impossible_fixture_tests.c
    tests/buildup_enumeration_tests.c
    tests/buildup_hold_enumeration_tests.c
    tests/buildup_export_tests.c
)

# Configure Test Target

function(clearra_core_configure_test_target test_target)
    target_include_directories(${test_target}
        PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include ${CMAKE_CURRENT_SOURCE_DIR}/src
    )
    target_compile_definitions(${test_target} PRIVATE CLEARRA_CORE_TEST=1)
    if(CLEARRA_ENABLE_STAGE_PROFILING)
        target_compile_definitions(
            ${test_target} PRIVATE CLEARRA_ENABLE_STAGE_PROFILING=1
        )
    endif()
    target_link_libraries(${test_target} PRIVATE clearra_core_sanitizer_options)
    clearra_core_enable_strict_warnings(${test_target})
endfunction()

# Add Test Target

function(clearra_core_add_test test_name test_source)
    add_executable(${test_name} ${test_source} ${${test_name}_EXTRA_SOURCES})
    target_link_libraries(${test_name} PRIVATE clearra_core_test_oracle)
    clearra_core_configure_test_target(${test_name})
    add_test(NAME ${test_name} COMMAND ${test_name})
endfunction()

# Aggregate Tests

list(LENGTH CLEARRA_CORE_TEST_NAMES CLEARRA_CORE_TEST_COUNT)
math(EXPR CLEARRA_CORE_TEST_LAST_INDEX "${CLEARRA_CORE_TEST_COUNT} - 1")

set(CLEARRA_CORE_AGGREGATE_OBJECTS)
foreach(test_index RANGE 0 ${CLEARRA_CORE_TEST_LAST_INDEX})
    list(GET CLEARRA_CORE_TEST_NAMES ${test_index} test_name)
    list(GET CLEARRA_CORE_TEST_SOURCES ${test_index} test_source)
    set(test_main "${test_name}_main")
    add_library(${test_name}_object OBJECT ${test_source} ${${test_name}_EXTRA_SOURCES})
    clearra_core_configure_test_target(${test_name}_object)
    target_compile_definitions(${test_name}_object PRIVATE main=${test_main})
    list(APPEND CLEARRA_CORE_AGGREGATE_OBJECTS $<TARGET_OBJECTS:${test_name}_object>)
endforeach()

# Executed Adversarial Correctness Gate

option(CLEARRA_CORE_ADVERSARIAL_TESTS
    "Build the focused correctness-regression executable."
    OFF
)
if(CLEARRA_CORE_ADVERSARIAL_TESTS)
    add_executable(clearra_adversarial_tests
        tests/adversarial_tests.c
        $<TARGET_OBJECTS:packing_tests_object>
        $<TARGET_OBJECTS:pruning_tests_object>
        $<TARGET_OBJECTS:buildup_tests_object>
    )
    target_link_libraries(clearra_adversarial_tests PRIVATE clearra_core_test_oracle)
    clearra_core_configure_test_target(clearra_adversarial_tests)
    add_test(NAME clearra_adversarial_tests COMMAND clearra_adversarial_tests)
endif()

add_executable(
    clearra_core_all_tests
    tests/all_tests.c
    tools/geometry_benchmark.c
    ${CLEARRA_CORE_AGGREGATE_OBJECTS}
)
target_link_libraries(clearra_core_all_tests PRIVATE clearra_core_test_oracle)
clearra_core_configure_test_target(clearra_core_all_tests)
add_test(NAME clearra_core_all_tests COMMAND clearra_core_all_tests)

# Split Tests

option(CLEARRA_CORE_SPLIT_TESTS "Build each core-c test as a separate executable." OFF)
if(CLEARRA_CORE_SPLIT_TESTS)
    foreach(test_index RANGE 0 ${CLEARRA_CORE_TEST_LAST_INDEX})
        list(GET CLEARRA_CORE_TEST_NAMES ${test_index} test_name)
        list(GET CLEARRA_CORE_TEST_SOURCES ${test_index} test_source)
        clearra_core_add_test(${test_name} ${test_source})
    endforeach()
endif()
