# Run before project()/compiler probes for both root and standalone core-c builds.
# The authority belongs to this checkout, not CMAKE_SOURCE_DIR: try_compile may
# use a generated source directory while its binary directory remains managed.
if(NOT DEFINED ENV{CLEARRA_BUILD_SESSION_ID} OR "$ENV{CLEARRA_BUILD_SESSION_ID}" STREQUAL "")
    message(FATAL_ERROR "Clearra CMake requires an active managed build session; use invoke-clearra-build.")
endif()

get_filename_component(_clearra_cmake_authority "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
find_program(_clearra_cmake_node NAMES node REQUIRED)
execute_process(
    COMMAND "${_clearra_cmake_node}"
        "${_clearra_cmake_authority}/scripts/tools/clearra-build-paths.mjs"
        --source-root "${_clearra_cmake_authority}" --field transaction
    RESULT_VARIABLE _clearra_owner_status
    OUTPUT_VARIABLE _clearra_cmake_transaction
    ERROR_VARIABLE _clearra_owner_error
    OUTPUT_STRIP_TRAILING_WHITESPACE
)
if(NOT _clearra_owner_status EQUAL 0 OR _clearra_cmake_transaction STREQUAL "")
    message(FATAL_ERROR "Clearra CMake build owner validation failed: ${_clearra_owner_error}")
endif()

set_property(GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_AUTHORITY "${_clearra_cmake_authority}")
set_property(GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_TRANSACTION "${_clearra_cmake_transaction}")
set_property(GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_NODE "${_clearra_cmake_node}")

function(_clearra_assert_cmake_output base value label)
    if("${value}" STREQUAL "")
        return()
    endif()
    # Output generator expressions cannot be proved contained before generation.
    if("${value}" MATCHES "\\$<|;")
        message(FATAL_ERROR "Clearra cannot validate an indirect CMake output path: ${label}")
    endif()
    get_filename_component(_clearra_output "${value}" ABSOLUTE BASE_DIR "${base}")
    get_property(_clearra_authority GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_AUTHORITY)
    get_property(_clearra_transaction GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_TRANSACTION)
    get_property(_clearra_node GLOBAL PROPERTY CLEARRA_CMAKE_BUILD_NODE)
    execute_process(
        COMMAND "${_clearra_node}" --input-type=module -e
            "import { pathToFileURL } from 'node:url'; const p = await import(pathToFileURL(process.argv[1])); p.assertBuildPathWithin(process.argv[2], process.argv[3]); p.assertNoBuildLinks(process.argv[2]);"
            "${_clearra_authority}/scripts/tools/clearra-build-policy.mjs"
            "${_clearra_output}" "${_clearra_transaction}"
        RESULT_VARIABLE _clearra_output_status
        ERROR_VARIABLE _clearra_output_error
    )
    if(NOT _clearra_output_status EQUAL 0)
        message(FATAL_ERROR "Clearra CMake output is outside its managed transaction or follows a link (${label}): ${_clearra_output_error}")
    endif()
endfunction()

function(_clearra_check_cmake_directory_outputs)
    _clearra_assert_cmake_output("${CMAKE_CURRENT_BINARY_DIR}" "${CMAKE_BINARY_DIR}" "CMAKE_BINARY_DIR")
    _clearra_assert_cmake_output("${CMAKE_CURRENT_BINARY_DIR}" "${CMAKE_CURRENT_BINARY_DIR}" "CMAKE_CURRENT_BINARY_DIR")
    get_cmake_property(_clearra_variables VARIABLES)
    foreach(_clearra_variable IN LISTS _clearra_variables)
        if(_clearra_variable MATCHES "^CMAKE_(ARCHIVE|LIBRARY|RUNTIME|PDB|COMPILE_PDB)_OUTPUT_DIRECTORY(_[A-Za-z0-9_]+)?$"
           OR _clearra_variable MATCHES "^(EXECUTABLE_OUTPUT_PATH|LIBRARY_OUTPUT_PATH|CMAKE_Fortran_MODULE_DIRECTORY)$")
            _clearra_assert_cmake_output("${CMAKE_CURRENT_BINARY_DIR}" "${${_clearra_variable}}" "${_clearra_variable}")
        endif()
    endforeach()
    # Config names can be appended to output directories by multi-config generators.
    foreach(_clearra_configuration IN LISTS CMAKE_CONFIGURATION_TYPES CMAKE_BUILD_TYPE CMAKE_TRY_COMPILE_CONFIGURATION)
        if(_clearra_configuration MATCHES "[/\\\\]" OR _clearra_configuration STREQUAL "." OR _clearra_configuration STREQUAL "..")
            message(FATAL_ERROR "Clearra CMake configuration must not escape its output directory")
        endif()
    endforeach()
endfunction()

# This check intentionally does not skip IN_TRY_COMPILE. Nested compiler probes
# are permitted below the same transaction; an unrelated binary root is not.
_clearra_check_cmake_directory_outputs()

function(_clearra_check_cmake_target_outputs)
    _clearra_check_cmake_directory_outputs()
    get_property(_clearra_targets DIRECTORY PROPERTY BUILDSYSTEM_TARGETS)
    set(_clearra_configurations DEBUG RELEASE RELWITHDEBINFO MINSIZEREL ${CMAKE_CONFIGURATION_TYPES} ${CMAKE_BUILD_TYPE})
    foreach(_clearra_target IN LISTS _clearra_targets)
        get_target_property(_clearra_target_base "${_clearra_target}" BINARY_DIR)
        foreach(_clearra_property ARCHIVE_OUTPUT_DIRECTORY LIBRARY_OUTPUT_DIRECTORY RUNTIME_OUTPUT_DIRECTORY PDB_OUTPUT_DIRECTORY COMPILE_PDB_OUTPUT_DIRECTORY Fortran_MODULE_DIRECTORY)
            set(_clearra_properties "${_clearra_property}")
            foreach(_clearra_configuration IN LISTS _clearra_configurations)
                string(TOUPPER "${_clearra_configuration}" _clearra_configuration)
                list(APPEND _clearra_properties "${_clearra_property}_${_clearra_configuration}")
            endforeach()
            foreach(_clearra_selected IN LISTS _clearra_properties)
                get_target_property(_clearra_value "${_clearra_target}" "${_clearra_selected}")
                if(NOT _clearra_value STREQUAL "_clearra_value-NOTFOUND")
                    _clearra_assert_cmake_output("${_clearra_target_base}" "${_clearra_value}" "${_clearra_target}.${_clearra_selected}")
                endif()
            endforeach()
        endforeach()
    endforeach()
endfunction()

# Installation/export prefixes are not compiler caches. Check target output
# overrides again before generation, after this directory's targets are defined.
if(NOT CMAKE_SCRIPT_MODE_FILE)
    cmake_language(DEFER CALL _clearra_check_cmake_target_outputs)
endif()
