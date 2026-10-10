# FINDING-0009: Retained PTY output reconstruction exceeded the real fixture deadline

Frozen predecessor 51274d823 rebuilt the retained 512 KiB output tail byte-by-byte after each small PTY read. A real >9 MiB web-console fixture exceeded its unchanged 30-second diagnostic deadline on hosted Linux.

The final candidate copies the VecDeque's two contiguous slices in bulk while preserving stream order and eviction. An allocator-independent wrap/interleave regression exercises both slices. The same-host microbenchmark fell from about 13.08 seconds to 0.105 seconds, and the real fresh PTY fixture fell from 12.74 seconds to 0.62 seconds locally without changing the deadline.
