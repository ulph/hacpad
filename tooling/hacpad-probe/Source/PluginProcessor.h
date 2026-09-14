#pragma once

#include <juce_audio_processors/juce_audio_processors.h>

#include "RpcLink.h"

#ifndef HACPAD_NUM_PARAMS
 #define HACPAD_NUM_PARAMS 16
#endif

/**
    A parameter whose name can be changed at runtime.

    This is the entire point of the plugin. A host that renders plugin
    parameters onto hardware -- Akai's VIP, driving the Advance 25's screen --
    will re-read the names when told they changed, and push them to the panel.
    Setting a name to a known marker and then finding those bytes on the wire
    turns protocol work into a chosen-plaintext exercise instead of guesswork.
*/
class ProbeParameter final : public juce::AudioProcessorParameter
{
public:
    explicit ProbeParameter (int indexIn)
        : index (indexIn),
          name ("Param " + juce::String (indexIn + 1))
    {
    }

    float getValue() const override                       { return value.load(); }
    void  setValue (float newValue) override              { value = newValue; }
    float getDefaultValue() const override                { return 0.0f; }

    juce::String getName (int maximumStringLength) const override
    {
        const juce::ScopedLock sl (nameLock);
        return name.substring (0, maximumStringLength);
    }

    juce::String getLabel() const override                { return {}; }
    int getNumSteps() const override                      { return 0x7fffffff; }
    bool isDiscrete() const override                      { return false; }
    bool isAutomatable() const override                   { return true; }

    juce::String getText (float v, int len) const override
    {
        return juce::String (v, 3).substring (0, len > 0 ? len : 5);
    }

    float getValueForText (const juce::String& text) const override
    {
        return juce::jlimit (0.0f, 1.0f, text.getFloatValue());
    }

    /** Not thread-safe against getName by design of juce::String; the lock
        keeps the two from tearing, since the host reads names on its own
        thread while the RPC thread writes them. */
    void setNameNow (const juce::String& newName)
    {
        const juce::ScopedLock sl (nameLock);
        name = newName;
    }

    juce::String getNameNow() const
    {
        const juce::ScopedLock sl (nameLock);
        return name;
    }

    const int index;

private:
    std::atomic<float> value { 0.0f };

    mutable juce::CriticalSection nameLock;
    juce::String name;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (ProbeParameter)
};


/**
    Probe plugin: exposes renameable parameters and a line protocol out to a
    sidecar on the host.

    This is an instrument for reverse-engineering VIP, not a shipping component.
    We control the parameter names; VIP renders them onto the Advance's screen;
    so a name set from the sidecar and then found on the wire tells us the field
    layout and encoding directly.
*/
class HacpadProbeProcessor final : public juce::AudioProcessor,
                                    private juce::Timer
{
public:
    HacpadProbeProcessor();
    ~HacpadProbeProcessor() override;

    void prepareToPlay (double sampleRate, int maximumExpectedSamplesPerBlock) override;
    void releaseResources() override {}
    void processBlock (juce::AudioBuffer<float>&, juce::MidiBuffer&) override;

    juce::AudioProcessorEditor* createEditor() override;
    bool hasEditor() const override                       { return true; }

    const juce::String getName() const override           { return "hacpad probe"; }
    bool acceptsMidi() const override                     { return true; }
    bool producesMidi() const override                    { return true; }
    bool isMidiEffect() const override                    { return false; }
    double getTailLengthSeconds() const override          { return 0.0; }

    int getNumPrograms() override                         { return 1; }
    int getCurrentProgram() override                      { return 0; }
    void setCurrentProgram (int) override                 {}
    const juce::String getProgramName (int) override      { return "Default"; }
    void changeProgramName (int, const juce::String&) override {}

    void getStateInformation (juce::MemoryBlock&) override;
    void setStateInformation (const void*, int) override;

    /** Status line for the editor. */
    juce::String getStatus() const;

private:
    void timerCallback() override;
    void handleLine (const juce::String& line);
    void announce();
    void tellHostNamesChanged();

    std::vector<ProbeParameter*> probes;   // owned by the AudioProcessor
    std::unique_ptr<RpcLink> rpc;

    // Incoming MIDI is captured on the audio thread without allocating, then
    // drained on the timer. Off unless the sidecar asks for it.
    std::atomic<bool> tapMidi { false };
    juce::AbstractFifo midiFifo { 8192 };
    std::vector<juce::uint8> midiRing = std::vector<juce::uint8> (8192);

    // Per instance, message thread only. Several probe instances can be loaded
    // at once, and each must re-announce on its own reconnect.
    bool wasConnected = false;

    std::atomic<int> namesChangedPending { 0 };
    std::atomic<juce::int64> midiDropped { 0 };

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (HacpadProbeProcessor)
};
