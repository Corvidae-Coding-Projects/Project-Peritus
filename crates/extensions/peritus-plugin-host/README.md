# peritus-plugin-host

Discovers strict plugin manifests, verifies exact manifest/artifact trust, launches executable or
WASI-compatible Wasm plugins outside the daemon, negotiates the versioned SDK protocol, and owns
bounded request lifecycle, quotas, cancellation, shutdown, and failure classification. Authority
is supplied by a G0/B1 mediator; the host cannot mint or widen it.
