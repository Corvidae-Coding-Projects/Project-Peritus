# Stage 6 logical sandbox contract

`stage6::sandbox` compares the public `NetworkContract` and `FilesystemContract` decisions to independent rule tables. Generated rules vary allow/deny overlap, exact and descendant/suffix matching, IP prefix lengths, TCP/UDP, inclusive port boundaries, all seven file operations, root paths, shared-prefix siblings, Unicode paths and Windows-style logical paths. Each generated contract is queried across a cross-product of hosts/paths, transports/operations and boundary ports. The model checks deny precedence and default denial, not just agreement between two production APIs.

The deliberate sandbox mutation turns a matching network deny into an allow in `calibration/stage6-sandbox.patch`. Fixed overlapping allow/deny examples and generated cases must expose it; exact restoration must recover their results.

These contracts are inert logical admission checks. They do not establish `NetworkPlan` requested-name/resolved-IP behavior, `compile_sandbox` seven-domain completeness, backend feature admission, or native Linux/macOS/Windows enforcement. Those are separate Stage 6 obligations and must be reported as gaps until actual qualified tests run. In particular, compilation and reference-backend simulation cannot substitute for an admitted and denied OS effect.
