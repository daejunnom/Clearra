add_library(clearra_core STATIC ${CLEARRA_CORE_SOURCES})
target_link_libraries(clearra_core PRIVATE clearra_core_sanitizer_options)
target_include_directories(clearra_core
    PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
    PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src
)
if(CLEARRA_ENABLE_STAGE_PROFILING)
    target_compile_definitions(
        clearra_core PUBLIC CLEARRA_ENABLE_STAGE_PROFILING=1
    )
endif()
clearra_core_enable_strict_warnings(clearra_core)

option(
    CLEARRA_BUILD_TEST_ORACLE
    "Build the independent test-only correctness oracle"
    OFF
)

if(CLEARRA_BUILD_TEST_ORACLE)
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
