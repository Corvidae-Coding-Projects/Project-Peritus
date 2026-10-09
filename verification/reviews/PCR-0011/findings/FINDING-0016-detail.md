# FINDING-0016: Directory inventories were not bound to their project root

The directory cache keyed an inventory without the canonical accepted project root, allowing nested projects or equivalent aliases to reuse a cursor with the wrong relative-path base. The final candidate canonicalizes the accepted project root before resolution and keys the inventory by both canonical project root and canonical directory. Traversal confinement, alias reuse, and nested-project isolation regressions pass.
