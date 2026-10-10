# FINDING-0008: A Unix-only range mutation failed strict Windows lint

Frozen predecessor 51274d823 declared a mutable file identity on every platform although mutation occurs only in the Unix branch. Strict Windows Clippy rejected the unused mutability and blocked the app-shell shard.

The final candidate keeps the common identity immutable and shadows it as mutable only under cfg(unix). Version and range calculations are unchanged. Native range tests and strict Linux/Windows checking pass.
