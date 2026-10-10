# FINDING-0003: Nested Git registration could copy dirty child bytes into the parent candidate

The reviewed pre-freeze nested-repository implementation accepted a registered child whose files were ordinary parent-index entries. Parent staging could then copy the child's private dirty bytes into the parent candidate; registered ignored children could also disappear instead of becoming exact HEAD gitlinks.

The final implementation rejects index collisions before mutation, records exact child HEAD values before staging, writes explicit mode-160000 links, and preserves byte-native paths. An external public-API fixture reproduces the original overlap and now requires pre-mutation rejection; repaired nested and safety suites pass.
