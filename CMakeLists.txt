cmake_minimum_required(VERSION 3.20)

include("${CMAKE_CURRENT_LIST_DIR}/core-c/cmake/clearra_build_root.cmake")
project(clearra LANGUAGES C)

include(CTest)

add_subdirectory(core-c)
