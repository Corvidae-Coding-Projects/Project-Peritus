# Task candidate workspace runtime

`managed.rs` and `managed/` own exact task preimages, Git/index/content fingerprints, nested repository patch export, and crash-safe discard transactions. `discard_recovery.rs` defines read-only transaction state. The parent `task_candidate.rs` exposes the stable candidate identity and error API shared by clients and the product runner.

These functions operate only on caller-authorized paths; they do not admit work, run models, or grant execution permissions. The original workspace creation API remains in `../candidate.rs`.
