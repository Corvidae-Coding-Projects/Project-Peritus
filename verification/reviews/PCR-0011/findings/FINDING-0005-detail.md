# FINDING-0005: First frozen candidate exceeded the enforced 500-line source budget

Frozen predecessor 92766bf3c failed the repository source-layout gate because ten changed source files exceeded the hard 500-line limit. That candidate was rejected and its partial formal campaign was discarded.

The final candidate splits cohesive private modules and test modules while preserving public paths. Token-stream comparison established moved-code equivalence apart from scoped visibility and rustfmt normalization. The exact replacement passes source-layout policy; no 927 gate result is reused.
