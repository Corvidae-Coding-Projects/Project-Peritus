# FINDING-0027: Required lifecycle discovery depended on anonymous Docker Hub capacity

The hosted lifecycle gate failed before exercising any scenario because the pinned Docker Hub image pull hit the anonymous rate limit. The final candidate changes only the registry prefix to the Docker-maintained image on Amazon ECR Public while retaining the Alpine name, tag, and immutable OCI index digest. Independent index, child-manifest, config, and layer hashing matched, and a real isolated pull plus all 12 lifecycle scenarios and all four replay targets completed with the workspace census unchanged.
