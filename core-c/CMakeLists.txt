cmake_minimum_required(VERSION 3.20)
project(clearra_core_c LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)

option(
    CLEARRA_ENABLE_STAGE_PROFILING
    "Enable opt-in high-detail search stage profiling"
    OFF
)

include(cmake/sanitizer_options.cmake)
include(cmake/compiler_warnings.cmake)
include(CTest)

set(CLEARRA_CORE_SOURCES)
include(cmake/source_manifest.cmake)
include(cmake/library_target.cmake)

if(BUILD_TESTING)
    include(cmake/test_targets.cmake)
endif()
