# FINDING-0017: Windows preview line submission sent the wrong terminator

The raw terminal API correctly promised exact bytes, but the `/preview play` line-submission interface sent line feed on Windows cooked ConPTY sessions. The final candidate keeps the raw API unchanged and makes only the line-submission layer emit carriage return on Windows and line feed elsewhere. Daemon, runner, and TUI fixtures now use the same terminal semantics.
