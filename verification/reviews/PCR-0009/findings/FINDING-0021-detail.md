# FINDING-0021: Graphical qualification incorrectly required note and behavior evidence from one stream

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The first typed-evidence repair required the behavior match and graphical-note match to use the same stream or artifact. Valid pipe launches can emit the observed behavior on stdout and the qualifying note on stderr. The repair requires one process and one compatible I/O mode while retaining each stream's independent exact range and source binding.
