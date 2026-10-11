# Final candidate proof-impact inventory

This audit-only bundle records the exact proof-impact comparison from protected base
`a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8` (tree
`2902edba7095a6ebd53b02bbf6f19005193d6deb`) to reviewed implementation candidate
`5d739de2902de0a5379f23f32b0c9f0ae562b28c` (tree
`3ca8f9e7fa5388ae3baa127c51c4bb5bd5c4fed7`).

The source-derived inventory contains 3,640 base sources, 3,731 candidate sources, and 155
transitions: 91 additions, 64 changes, and no removals. The affected projection contains all 66
packages because the architecture registry changed. Their classes are 53 H, 12 V, and one T.
The resulting plan contains 66 ordinary test commands and 66 Verus commands.

`artifact-sha256.txt` binds the generated artifacts and passes native `sha256sum --check`. The
bundle is evidence for the reviewed source and required verification plan. It is not a PCR
authorization, approval verdict, obligation discharge, protected-base trust record, or permission
to merge. Historical review and obligation records remain unchanged.

The gate list retains generator order, pairing each package's ordinary and Verus commands. A future
authorization verdict must apply the checker's canonical ordering by gate kind and then package.
That authorization must already exist on the protected base before a later application can pass
`verify-trust`; this draft pull request does not claim that protected-base state.
