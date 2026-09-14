#!/usr/bin/env python3
"""Speak text aloud through the speakers. Usage: python3 say.py "text to speak" """
import subprocess
import sys
import tempfile

from gtts import gTTS


def say(text: str) -> None:
    with tempfile.NamedTemporaryFile(suffix=".mp3") as f:
        gTTS(text).save(f.name)
        subprocess.run(
            ["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet", f.name],
            check=False,
        )


if __name__ == "__main__":
    say(" ".join(sys.argv[1:]) or "no text given")
