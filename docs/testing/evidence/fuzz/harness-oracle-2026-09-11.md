# Fuzz harness oracle correction

- Source under test: `b576da334ec6dd6efa228761cb0a902b505fa854`
- Host: Linux x86_64
- Classification: harness defect, not a product defect
- Target: `sse`, seed 881

The first bounded SSE campaign stopped after three executions because the harness required invalid
input to produce the same error category under every chunk partition. The generated input was
rejected in both cases: whole-input parsing reached invalid UTF-8 and returned `MalformedStream`,
while one-byte chunks exceeded the configured one-byte frame limit first and returned
`LimitExceeded`. Error priority for already-invalid input is not part of the documented framing
contract.

The original 13-byte artifact remains in the local campaign evidence. LibFuzzer reduced the
logical signature to five bytes (`00 00 b9 f2 0a`) before LeakSanitizer failed under the sandbox's
ptrace restrictions. Stable replay independently confirmed that the five-byte case fails the old
oracle and an empty input does not, preventing the sanitizer failure from being mistaken for a
zero-byte reproducer. The five-byte case is retained as `corpus/sse/invalid-priority-partition`.

The corrected oracle requires identical output for inputs accepted as valid by whole-input
parsing. Inputs rejected as invalid must remain rejected under the generated partition, while the
specific error category may reflect the first violation observed. No production parser behavior
or limit was changed for this harness correction.
