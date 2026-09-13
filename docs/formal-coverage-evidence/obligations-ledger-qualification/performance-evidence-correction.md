# Obligations performance evidence correction

Date: 2026-09-12

The source and proof claims in `/tmp/peritus-sol-obligations-performance-source-01` remain frozen. Two descriptions in the original performance report are withdrawn:

1. The new probe did not use a materially different early-exit shape. It is semantically the same late-failure fixture as `/tmp/peritus-parent-obligations-scaling-probe.rs`: direct evidence satisfies every required entry except the last member of each alternative branch. Its sizes, repeat counts, and assertions differ.
2. The phrase `no evidence` in `/tmp/peritus-sol-obligations-scaling-comparison-02.json` is incorrect; the fixture supplies direct evidence.

Parent's fresh A/B/C run, with exact source-manifest checks and separate never-used Cargo targets for each variant, establishes a bounded runtime regression in frozen325 and recovery in frozen356 on this fixture:

- foundation: 256 = 0.271 ms, 512 = 0.581 ms
- frozen325: 256 = 194.291 ms, 512 = 1,444.791 ms
- frozen356: 256 = 0.308 ms, 512 = 0.647 ms

Evidence: `/tmp/peritus-parent-obligations-scaling-independent3.json`, the corresponding `...-{foundation,candidate325,fixed356}.log` files, and `/tmp/peritus-parent-obligations-scaling-independent3.py`.

These measurements establish the bounded reproduced regression and recovery. Their doubling ratios support the repeated-member evaluation diagnosis; they are not an asymptotic theorem. The earlier fast frozen325 measurement remains unexplained, with source/build-cache swap only a possibility rather than an established cause.
