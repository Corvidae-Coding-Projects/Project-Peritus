# FINDING-0022: Persisted behavior evidence could not qualify after host-loss recovery

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

A behavior check could be durably admitted while the launch was running, then a crash or host loss restored the launch as failed before goal publication. The qualifier rejected failed launches forever and could not use already validated evidence, while rescanning was unavailable after output ownership disappeared. The repair separates durable admission from retryable goal publication and treats validated persisted evidence as authoritative for the same launch and goal binding.
