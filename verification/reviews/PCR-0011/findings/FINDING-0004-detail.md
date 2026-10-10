# FINDING-0004: Updater extraction and installation ownership ended before all consumers

The reviewed pre-freeze updater released the download lock before installation consumed a reusable extraction path. A second update could overwrite those inputs. Installation and verification also lacked one cross-version owner; Windows helper paths could collide and parent-PID-only waiting was vulnerable to PID reuse.

The final implementation uses immutable attempt extraction, retains a Unix installation flock through verification and cancellation cleanup, uses an external Windows installation owner, unique helper/outcome paths, and exact parent process creation identity. Focused extraction and owner replays pass; current candidate launcher tests also cover competing release and descendant custody.
