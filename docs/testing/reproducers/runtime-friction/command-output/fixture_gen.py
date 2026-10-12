"""Deterministic stdout fixture for an actual Peritus command, with bounded working memory."""
import sys

size = 37_349_240
first = b"PERITUS_REAL_OUTPUT_BEGIN\n"
last = b"\nPERITUS_REAL_OUTPUT_END\n"
sys.stdout.buffer.write(first)
remaining = size - len(first) - len(last)
chunk = b"0123456789abcdef" * 4096
while remaining:
    count = min(remaining, len(chunk))
    sys.stdout.buffer.write(chunk[:count])
    remaining -= count
sys.stdout.buffer.write(last)
