# peritus-sandbox

`peritus-sandbox` is Project Peritus's platform-neutral execution-isolation contract. It validates
default-deny filesystem, process, environment, network, secret, resource, and terminal policy;
compiles deterministic inert plans; admits only complete backend support; and supplies an
executable reference backend for conformance.

The crate never spawns an operating-system process and never grants authority. Real launch is
owned by `peritus-process`, whose target gateway binds a checked plan and backend admission to an
opaque authorized launch. Later C3 native backends implement that process-owned boundary without
changing or weakening the values defined here.

Canonical plan and backend bytes are versioned and content addressed. Secret references and
delivery destinations are represented, but secret values are deliberately absent from every plan,
error, observation, and debug representation.
