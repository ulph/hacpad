#include "RpcLink.h"

RpcLink::RpcLink (juce::String hostIn, int portIn, LineHandler onLineIn)
    : juce::Thread ("hacpad-rpc"),
      host (std::move (hostIn)),
      port (portIn),
      onLine (std::move (onLineIn))
{
    startThread();
}

RpcLink::~RpcLink()
{
    // Generous but bounded: the socket read below wakes every 200ms, so this
    // never actually waits the full timeout.
    stopThread (2000);
}

void RpcLink::send (const juce::String& line)
{
    const juce::ScopedLock sl (outboxLock);

    // Bound the buffer. If the sidecar is gone, a plugin left running for hours
    // must not grow this without limit.
    if (outbox.size() < 4096)
        outbox.add (line);
}

void RpcLink::run()
{
    while (! threadShouldExit())
    {
        juce::StreamingSocket socket;

        if (! socket.connect (host, port, 1000))
        {
            connected = false;
            wait (1000);          // sidecar not up yet; keep trying
            continue;
        }

        connected = true;
        inbox.clear();

        while (! threadShouldExit() && socket.isConnected())
            if (! pump (socket))
                break;

        connected = false;
        socket.close();
    }
}

bool RpcLink::pump (juce::StreamingSocket& socket)
{
    drainOutbox (socket);

    const auto ready = socket.waitUntilReady (true, 200);
    if (ready < 0)
        return false;             // socket error
    if (ready == 0)
        return true;              // timed out; loop so we can flush and re-check exit

    char buffer[2048];
    const auto got = socket.read (buffer, (int) sizeof (buffer), false);
    if (got <= 0)
        return false;             // peer closed

    inbox += juce::String::fromUTF8 (buffer, got);

    for (;;)
    {
        const auto nl = inbox.indexOfChar ('\n');
        if (nl < 0)
            break;

        auto line = inbox.substring (0, nl).trimEnd();
        inbox = inbox.substring (nl + 1);

        if (line.isNotEmpty() && onLine != nullptr)
            onLine (line);
    }

    // A peer that sends no newlines must not be able to exhaust memory here.
    if (inbox.length() > 1 << 20)
        inbox.clear();

    return true;
}

void RpcLink::drainOutbox (juce::StreamingSocket& socket)
{
    juce::StringArray pending;

    {
        const juce::ScopedLock sl (outboxLock);
        pending.swapWith (outbox);
    }

    for (const auto& line : pending)
    {
        const auto payload = (line + "\n").toRawUTF8();
        const auto len = (int) std::strlen (payload);

        if (socket.write (payload, len) != len)
            return;               // dropped; the reconnect loop will notice
    }
}
