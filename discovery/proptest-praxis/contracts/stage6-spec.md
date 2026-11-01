# Stage 6 acceptance gate graph contract

Actual public path: `peritus-spec::GateDefinition::new`, `GateGraph::new`,
`execution_order`, `get`, and `dependency_precedes`. The product command builder in
`crates/app/peritus-product-runner/src/developer_tools/command_runtime/contract.rs` constructs
a one-node graph; `peritus-gates` consumes the checked order. This target reaches multi-node
graphs through the public constructor that current one-node callers rarely exercise.

The discovery input is a bounded set of one to eight distinct gates with an explicit bitset
of dependencies for each gate. An independent bitset interpreter repeatedly chooses the
lowest-index ready gate; it reports cycles when no gate is ready. The checker compares the
accepted order, all declared dependencies, all pairwise order queries, and rejection of an
unknown gate. Constructed acyclic DAGs use independently generated ranks, so dependencies
can point in either identifier direction. Arbitrary graphs reach cycles and self-edges.

Fixed invalid cases cover duplicate and noncanonical dependency order, unknown dependency,
duplicate and noncanonical definitions, zero timeout, and empty graph. The oracle checks
rejection, and for self-edges and cycles also checks the specific error. All identifiers and
digests are synthetic. This target checks graph planning, not gate execution or acceptance
evidence evaluation; those are distinct callers and remain separate coverage rows.

Behavioral counters distinguish acyclic graphs, cyclic and self-edge rejection, multiple
simultaneously ready gates, declared edges, and each fixed invalid class. A deliberate
canonical-order mutation must be detected by both the fixed graph and generated cases.
