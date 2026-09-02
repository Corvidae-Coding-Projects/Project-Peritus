# peritus-product-state

`peritus-product-state` is the H-class G4 owner of pure, versioned, resumable product state. It
retains stable non-secret installation identities, the exact durable local-bootstrap phase,
provider/default/offline selection, and non-secret direct-route profiles containing only opaque
credential references. It also retains explicit, default-off automatic-failover consent and
requires at least two selected routes before that choice is valid. The executable transition
predicate is mirrored by a Verus refinement.

The crate performs no filesystem, process, terminal, network, credential, workspace, or daemon
effect. Effectful composition belongs to `peritus-launcher`; durable runtime authority remains in
the existing G0/C0/C1/B1 boundaries. State generations never persist live daemon readiness.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-product-state
```
