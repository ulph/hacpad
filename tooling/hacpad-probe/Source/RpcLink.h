#pragma once

#include <juce_core/juce_core.h>

#include <functional>

/**
    A line-oriented TCP link from the plugin out to a sidecar on the host.

    The plugin is the *client*, not the server. Two reasons: a listening socket
    inside a Wine prefix is the kind of thing that quietly fails, and making the
    sidecar the stable endpoint means plugin instances can come and go with the
    host's plugin scanning without anything needing to track them.

    Reconnects on its own, so start order does not matter -- the sidecar can be
    started before or after the host.

    Lines are UTF-8, newline-terminated, and deliberately readable so the whole
    protocol can be driven by hand with netcat while debugging.
*/
class RpcLink final : private juce::Thread
{
public:
    using LineHandler = std::function<void (const juce::String&)>;

    RpcLink (juce::String host, int port, LineHandler onLine);
    ~RpcLink() override;

    /** Queue a line for the sidecar. Safe from any thread, including audio:
        it only appends to a locked buffer and never blocks on the socket. */
    void send (const juce::String& line);

    bool isConnected() const noexcept   { return connected.load(); }

private:
    void run() override;
    bool pump (juce::StreamingSocket&);
    void drainOutbox (juce::StreamingSocket&);

    const juce::String host;
    const int port;
    LineHandler onLine;

    std::atomic<bool> connected { false };

    juce::CriticalSection outboxLock;
    juce::StringArray outbox;

    juce::String inbox;   // partial line carried between reads

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (RpcLink)
};
