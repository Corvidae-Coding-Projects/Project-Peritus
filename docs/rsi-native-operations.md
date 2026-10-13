# Native candidate evaluation and evolution

The daemon owns local executable candidate campaigns through its serialized authority and durable
E1/E2/E3/D2/F0 owners. These operations require actual materialization receipts, immutable input
artifacts, observed candidate/evaluator processes, and published evidence. A decoded report or
selected candidate does not authorize production activation.

## Host configuration

Linux native evaluation requires host-provisioned isolation components and a writable delegated
cgroup. Declare their absolute paths in the daemon configuration:

```toml
[rsi]
bubblewrap = "/usr/bin/bwrap"
linux_helper = "/absolute/path/to/peritus-linux-sandbox-helper"
cgroup_root = "/absolute/path/to/delegated/cgroup"
```

Use the helper built with the daemon. Its actual fingerprint is frozen in the execution profile;
replace it between campaigns and create fresh profiles. Missing components or unsupported isolation
produce explicit campaign infrastructure failure. There is no unrestricted execution fallback.
Configure bounded process resources and one worker when exercising a disposable campaign.

## Operator APIs

`AuthorityHandle::start_evaluation` admits a complete `FrozenEvaluationProfile` through the sole
writable owner. Local profiles use `EvaluationExecutionMode::LocalProcess`, real E1 arm bindings,
and `LocalProcessCatalog` artifacts. Public candidate task inputs and sealed evaluator inputs are
separate; the candidate cannot receive the sealed task or a program/support artifact that aliases
it. Command arguments remain literal data. Scheduling and process reservation commit before child
effects. `evaluation` reads the replayed state; `cancel_evaluation` durably cancels before signalling
an owned worker. Shutdown and restart retain physical observations and reconcile exact owned
process identities instead of launching an already consumed attempt again.

Full frozen profiles and rollout records have versioned canonical storage APIs. Legacy Debug text
cannot be recovered into execution authority. Input, raw output, terminal, trace, diagnostic and
report roots are retained through new committed artifact dependencies, including late observations
after cancellation. Complete analysis and publication can resume after the last worker directive
has already been acknowledged.

The public `rsi::evolution::EvolutionOperator` borrows the journal, artifact store and evidence
store from an exclusive owner. Close the daemon before using a standalone operator on its state;
do not open a second journal writer against a running daemon. The operator validates actual E1
materializations, published E2/E3 evidence, completed independent D2 review, and the unchanged
protected promotion policy. Selection is inert. Promotion and rollback each require their own
fresh signed approval, exact kernel/capability target, and atomic approve-once consumption with
the production-pointer compare-and-swap. Ordinary raw transition submission cannot activate a
pointer or manufacture derived review/selection evidence.

`rsi::production::current_program` performs a fresh authoritative pointer/materialization lookup
and returns an immutable executable reference. Submit that input through a new admitted local
operation to execute it. Previously captured references retain their historical binding. This
native lookup does not replace the ordinary model prompt graph or establish model-provider
quality. LocalProcess campaigns make no provider requests; unavailable native measurements remain
unavailable rather than becoming zero.

The [real campaign report](testing/rsi-production-real-2026-10-13.md) records the executed scope,
confirmed defects, negative cases and reproducible public consumers.
