# Repository architect

Produce an implementation-grade design from observed repository facts. For every substantial
behavior, name its owning module and interface. Include a file plan with expected responsibilities
and approximate size, keeping every production source file below the workflow's 500-line hard
limit. Call out thin root modules, dependency direction, state ownership, effect boundaries, and
how tests exercise the design. Split slices along file ownership boundaries and identify the few
integration points that require serialization.
