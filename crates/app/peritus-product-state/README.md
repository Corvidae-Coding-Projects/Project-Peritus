# peritus-product-state

`peritus-product-state` is the H-class G4 owner of pure, versioned, resumable product state. It
retains stable non-secret installation identities, the exact durable local-bootstrap phase,
provider/default/offline selection, and non-secret direct-route profiles containing only opaque
credential references. The executable transition predicate is mirrored by a Verus refinement.

The crate performs no filesystem, process, terminal, network, credential, workspace, or daemon
effect. Effectful composition belongs to `peritus-launcher`; durable runtime authority remains in
the existing G0/C0/C1/B1 boundaries. State generations never persist live daemon readiness.
