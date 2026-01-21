# CMake Toolchain File for OXIDE OS Cross-Compilation
#
# Usage:
#   cmake -DCMAKE_TOOLCHAIN_FILE=/path/to/oxide-toolchain.cmake ..
#
# Or set in CMakeLists.txt before project():
#   set(CMAKE_TOOLCHAIN_FILE /path/to/oxide-toolchain.cmake)

# Get the directory of this toolchain file
get_filename_component(OXIDE_TOOLCHAIN_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(OXIDE_ROOT "${OXIDE_TOOLCHAIN_DIR}/../.." ABSOLUTE)

# Target system
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR x86_64)
set(CMAKE_SYSTEM_VERSION 1)

# Specify the cross compiler
set(CMAKE_C_COMPILER "${OXIDE_ROOT}/toolchain/bin/oxide-cc")
set(CMAKE_CXX_COMPILER "${OXIDE_ROOT}/toolchain/bin/oxide-c++")
set(CMAKE_ASM_COMPILER "${OXIDE_ROOT}/toolchain/bin/oxide-as")
set(CMAKE_AR "${OXIDE_ROOT}/toolchain/bin/oxide-ar")
set(CMAKE_LINKER "${OXIDE_ROOT}/toolchain/bin/oxide-ld")
set(CMAKE_RANLIB ":")  # No-op, ar does this

# Sysroot
set(CMAKE_SYSROOT "${OXIDE_ROOT}/toolchain/sysroot")
set(CMAKE_FIND_ROOT_PATH "${CMAKE_SYSROOT}")

# Search for programs in the build host directories
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)

# Search for libraries and headers in the target directories
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)

# Compiler flags
set(CMAKE_C_FLAGS_INIT "-ffreestanding")
set(CMAKE_CXX_FLAGS_INIT "-ffreestanding -fno-exceptions -fno-rtti")

# Don't look for shared libraries (static only)
set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")

# Executable suffix (none for OXIDE)
set(CMAKE_EXECUTABLE_SUFFIX "")

# Set pkg-config
set(ENV{PKG_CONFIG} "${OXIDE_ROOT}/toolchain/bin/oxide-pkg-config")

message(STATUS "OXIDE Toolchain: ${OXIDE_ROOT}/toolchain")
message(STATUS "OXIDE Sysroot: ${CMAKE_SYSROOT}")
