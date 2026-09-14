cmake_minimum_required(VERSION 3.22)
project(HacpadProbe VERSION 0.1.0 LANGUAGES C CXX)

# JUCE rather than nih-plug for one reason: this plugin's whole purpose is
# renaming its parameters at runtime so the host re-reads them. JUCE exposes
# that directly (updateHostDisplay + withParameterInfoChanged); frameworks built
# around statically-declared parameters would have to be fought.
include(cmake/get_cpm.cmake)

CPMAddPackage(
    NAME JUCE
    GITHUB_REPOSITORY juce-framework/JUCE
    GIT_TAG 8.0.4
    GIT_SHALLOW TRUE)

# Number of exposed parameters. 16 rather than 8 so paging behaviour is
# observable -- VIP shows knobs in banks of 8.
set(HACPAD_NUM_PARAMS 16 CACHE STRING "Parameters the plugin exposes")

juce_add_plugin(HacpadProbe
    COMPANY_NAME            "hacpad"
    PLUGIN_MANUFACTURER_CODE Hcpd
    PLUGIN_CODE              Hprb
    FORMATS                  VST3
    IS_SYNTH                 TRUE
    NEEDS_MIDI_INPUT         TRUE
    NEEDS_MIDI_OUTPUT        TRUE
    EDITOR_WANTS_KEYBOARD_FOCUS FALSE
    COPY_PLUGIN_AFTER_BUILD  FALSE
    PRODUCT_NAME             "hacpad probe")

target_sources(HacpadProbe PRIVATE
    Source/PluginProcessor.cpp
    Source/RpcLink.cpp)

target_compile_features(HacpadProbe PRIVATE cxx_std_17)

target_compile_definitions(HacpadProbe PUBLIC
    JUCE_WEB_BROWSER=0
    JUCE_USE_CURL=0
    JUCE_VST3_CAN_REPLACE_VST2=0
    JUCE_DISPLAY_SPLASH_SCREEN=0
    HACPAD_NUM_PARAMS=${HACPAD_NUM_PARAMS})

target_link_libraries(HacpadProbe
    PRIVATE juce::juce_audio_utils
    PUBLIC  juce::juce_recommended_config_flags
            juce::juce_recommended_warning_flags)

# juce_core already pulls in the platform socket libraries, so juce::StreamingSocket
# needs no extra linkage here -- which also keeps this target buildable natively
# on Linux too, should a probe ever need to run outside Wine.
