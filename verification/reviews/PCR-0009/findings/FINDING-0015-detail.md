# FINDING-0015: Git history identity changed under replace, graft, and shallow metadata

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

History ran through repository-global replacement and shallow metadata. The same accepted repository digest, start commit, and continuation request could return different parents, subjects, cursors, and digests after `git replace`, graft changes, or `.git/shallow` changes. The repair disables replacement and graft interpretation and supplies a null shallow file for the read-only history command.
