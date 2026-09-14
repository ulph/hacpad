#include "PluginProcessor.h"

namespace
{
    constexpr int   kProtocolVersion = 1;
    constexpr int   kDefaultPort     = 8131;
    const char*     kDefaultHost     = "127.0.0.1";

    /** Port override, so several hosts can be probed at once without a rebuild. */
    int resolvePort()
    {
        if (auto env = std::getenv ("HACPAD_RPC_PORT"))
            if (auto p = juce::String (env).getIntValue(); p > 0 && p < 65536)
                return p;

        return kDefaultPort;
    }

    juce::String toHex (const juce::uint8* data, int n)
    {
        juce::String s;
        for (int i = 0; i < n; ++i)
            s += juce::String::toHexString ((int) data[i]).paddedLeft ('0', 2);
        return s;
    }
}

//==============================================================================
HacpadProbeProcessor::HacpadProbeProcessor()
    : juce::AudioProcessor (BusesProperties()
                                .withOutput ("Output", juce::AudioChannelSet::stereo(), true))
{
    probes.reserve (HACPAD_NUM_PARAMS);

    for (int i = 0; i < HACPAD_NUM_PARAMS; ++i)
    {
        auto* p = new ProbeParameter (i);
        probes.push_back (p);
        addParameter (p);          // AudioProcessor takes ownership
    }

    rpc = std::make_unique<RpcLink> (kDefaultHost, resolvePort(),
                                     [this] (const juce::String& line) { handleLine (line); });

    startTimer (25);
}

HacpadProbeProcessor::~HacpadProbeProcessor()
{
    stopTimer();
    rpc.reset();                   // joins the RPC thread before parameters die
}

void HacpadProbeProcessor::prepareToPlay (double, int) {}

void HacpadProbeProcessor::processBlock (juce::AudioBuffer<float>& audio,
                                          juce::MidiBuffer& midi)
{
    audio.clear();

    if (! tapMidi.load())
        return;

    // Copy raw bytes into the ring without allocating. Anything that does not
    // fit is counted, not silently lost -- a probe that quietly drops evidence
    // is worse than one that admits it.
    for (const auto meta : midi)
    {
        const auto n = meta.numBytes;
        if (n <= 0)
            continue;

        int start1, size1, start2, size2;
        midiFifo.prepareToWrite (n + 1, start1, size1, start2, size2);

        if (size1 + size2 < n + 1)
        {
            midiFifo.finishedWrite (0);
            midiDropped.fetch_add (1);
            continue;
        }

        const juce::uint8 header = (juce::uint8) juce::jmin (n, 255);
        auto put = [&] (int offset, juce::uint8 b) { midiRing[(size_t) offset] = b; };

        int written = 0;
        auto emit = [&] (juce::uint8 b)
        {
            const auto idx = written < size1 ? start1 + written
                                             : start2 + (written - size1);
            put (idx, b);
            ++written;
        };

        emit (header);
        for (int i = 0; i < n; ++i)
            emit (meta.data[i]);

        midiFifo.finishedWrite (written);
    }
}

//==============================================================================
void HacpadProbeProcessor::timerCallback()
{
    // Parameter-name changes must reach the host from the message thread.
    if (namesChangedPending.exchange (0) > 0)
        updateHostDisplay (juce::AudioProcessorListener::ChangeDetails{}
                               .withParameterInfoChanged (true));

    // Drain tapped MIDI.
    while (midiFifo.getNumReady() > 0)
    {
        int start1, size1, start2, size2;
        midiFifo.prepareToRead (1, start1, size1, start2, size2);
        if (size1 + size2 < 1)
            break;

        const int len = midiRing[(size_t) start1];
        midiFifo.finishedRead (1);

        if (len <= 0 || midiFifo.getNumReady() < len)
            break;

        std::vector<juce::uint8> msg ((size_t) len);
        midiFifo.prepareToRead (len, start1, size1, start2, size2);
        for (int i = 0; i < len; ++i)
            msg[(size_t) i] = midiRing[(size_t) (i < size1 ? start1 + i
                                                           : start2 + (i - size1))];
        midiFifo.finishedRead (len);

        rpc->send ("midi " + toHex (msg.data(), len));
    }

    if (const auto dropped = midiDropped.exchange (0); dropped > 0)
        rpc->send ("dropped " + juce::String (dropped));

    // Re-announce whenever the link comes back, so the sidecar never has to
    // guess what a reconnecting plugin currently holds.
    if (const auto now = rpc->isConnected(); now != wasConnected)
    {
        wasConnected = now;
        if (now)
            announce();
    }
}

void HacpadProbeProcessor::announce()
{
    rpc->send ("hello " + juce::String (kProtocolVersion)
                        + " " + juce::String ((int) probes.size())
                        + " " + getName());

    for (auto* p : probes)
        rpc->send ("named " + juce::String (p->index) + " " + p->getNameNow());
}

void HacpadProbeProcessor::tellHostNamesChanged()
{
    namesChangedPending.fetch_add (1);
}

void HacpadProbeProcessor::handleLine (const juce::String& line)
{
    const auto cmd = line.upToFirstOccurrenceOf (" ", false, false);
    const auto rest = line.fromFirstOccurrenceOf (" ", false, false);

    auto indexed = [&] (juce::String& tail) -> ProbeParameter*
    {
        const auto idx = tail.upToFirstOccurrenceOf (" ", false, false).getIntValue();
        tail = tail.fromFirstOccurrenceOf (" ", false, false);

        if (! juce::isPositiveAndBelow (idx, (int) probes.size()))
            return nullptr;

        return probes[(size_t) idx];
    };

    if (cmd == "ping")
    {
        rpc->send ("pong");
    }
    else if (cmd == "describe")
    {
        announce();
    }
    else if (cmd == "name")
    {
        auto tail = rest;
        if (auto* p = indexed (tail))
        {
            p->setNameNow (tail);
            tellHostNamesChanged();
            rpc->send ("named " + juce::String (p->index) + " " + tail);
        }
        else
        {
            rpc->send ("err bad index: " + line);
        }
    }
    else if (cmd == "names")
    {
        // Bulk rename with one marker prefix: `names AAAA` gives AAAA0000..
        // Cheapest way to find the name field -- every slot becomes greppable.
        for (auto* p : probes)
            p->setNameNow (rest + juce::String (p->index).paddedLeft ('0', 4));

        tellHostNamesChanged();
        announce();
    }
    else if (cmd == "value")
    {
        auto tail = rest;
        if (auto* p = indexed (tail))
        {
            p->setValueNotifyingHost (juce::jlimit (0.0f, 1.0f, tail.getFloatValue()));
            rpc->send ("valued " + juce::String (p->index) + " " + tail);
        }
    }
    else if (cmd == "tap")
    {
        tapMidi = rest.getIntValue() != 0;
        rpc->send ("tap " + juce::String (tapMidi.load() ? 1 : 0));
    }
    else
    {
        rpc->send ("err unknown: " + cmd);
    }
}

//==============================================================================
juce::String HacpadProbeProcessor::getStatus() const
{
    return (rpc != nullptr && rpc->isConnected() ? "connected to sidecar :"
                                                : "waiting for sidecar :")
           + juce::String (resolvePort());
}

juce::AudioProcessorEditor* HacpadProbeProcessor::createEditor()
{
    // The generic editor lists every parameter with its current name, which is
    // exactly what we want to eyeball while probing.
    return new juce::GenericAudioProcessorEditor (*this);
}

void HacpadProbeProcessor::getStateInformation (juce::MemoryBlock& dest)
{
    juce::ValueTree state ("HACPAD_PROBE");

    for (auto* p : probes)
    {
        juce::ValueTree n ("PARAM");
        n.setProperty ("i", p->index, nullptr);
        n.setProperty ("name", p->getNameNow(), nullptr);
        n.setProperty ("value", p->getValue(), nullptr);
        state.appendChild (n, nullptr);
    }

    juce::MemoryOutputStream out (dest, false);
    state.writeToStream (out);
}

void HacpadProbeProcessor::setStateInformation (const void* data, int size)
{
    auto state = juce::ValueTree::readFromData (data, (size_t) size);
    if (! state.hasType ("HACPAD_PROBE"))
        return;

    for (auto n : state)
    {
        const int i = n.getProperty ("i", -1);
        if (! juce::isPositiveAndBelow (i, (int) probes.size()))
            continue;

        probes[(size_t) i]->setNameNow (n.getProperty ("name", "").toString());
        probes[(size_t) i]->setValue ((float) n.getProperty ("value", 0.0));
    }

    tellHostNamesChanged();
}

//==============================================================================
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter()
{
    return new HacpadProbeProcessor();
}
