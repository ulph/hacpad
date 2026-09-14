# CPM bootstrap. Pinned so a fresh clone configures with no extra steps and no
# vendored dependencies -- JUCE is fetched at configure time, not checked in.
set(CPM_DOWNLOAD_VERSION 0.40.2)
set(CPM_HASH_SUM "c8cdc32c03816538ce22781ed72964dc864b2a34a310d3b7104812a5ca2d835d")

if(CPM_SOURCE_CACHE)
  set(CPM_PATH "${CPM_SOURCE_CACHE}/cpm/CPM_${CPM_DOWNLOAD_VERSION}.cmake")
else()
  set(CPM_PATH "${CMAKE_BINARY_DIR}/cmake/CPM_${CPM_DOWNLOAD_VERSION}.cmake")
endif()

if(NOT EXISTS "${CPM_PATH}")
  message(STATUS "Downloading CPM.cmake ${CPM_DOWNLOAD_VERSION}")
  file(DOWNLOAD
    "https://github.com/cpm-cmake/CPM.cmake/releases/download/v${CPM_DOWNLOAD_VERSION}/CPM.cmake"
    "${CPM_PATH}"
    EXPECTED_HASH SHA256=${CPM_HASH_SUM})
endif()

include("${CPM_PATH}")
