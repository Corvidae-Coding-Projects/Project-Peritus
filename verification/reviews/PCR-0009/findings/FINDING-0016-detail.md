# FINDING-0016: Git history output depended on user encoding, notes, signatures, and color configuration

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

Local Git configuration could rewrite the accepted history subject or inject presentation data without changing the request identity. In particular, `i18n.logOutputEncoding=SHIFT-JIS` changed `yen-¥` into different bytes. The repair forces UTF-8 log output and disables notes, signature display, color, and patch output for the canonical history invocation.
