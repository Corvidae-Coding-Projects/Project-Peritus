# Native web access

Implement keyless SearXNG search, ordinary HTTP page reading, immutable source paging and literal find, with the existing isolated Playwright runtime for JavaScript rendering. These tools serve conversation, design, writer, fixer, and review roles without changing provider adapters. A full autonomous deep-research coordinator is a later consumer.

Ownership: the new C4 `peritus-tools-web` owns HTTP, extraction, retained source files and SearXNG configuration/lifecycle. Product tools own permissions and dispatch; the existing command runtime owns optional browser processes. CLI exposes setup, status, stop and direct operations for people and diagnostics. Ordinary HTTP needs Read/Network, retained reads/find need Read, explicit rendering additionally needs Process. Browser rendering cannot attach to user profiles or read workspace files.

Prefer this separate component over putting HTTP into the GUI adapter: it keeps native reading independent of Python/Chromium and supports direct CLI consumers. Search uses an explicitly configured SearXNG endpoint or a loopback-only, resource-bounded managed container. No paid search key. Search outages must not prevent opening known URLs. Surface partial engine failures and non-2xx responses.

Sources are immutable, identified by digest, and retain requested/final URL, retrieval time, method, HTTP status, raw digest and extracted text. Pages have exact UTF-8 byte continuation offsets and line numbers. Network bodies and transient query caching have explicit resource bounds; context pages are independently bounded. Durable source evidence has no arbitrary total-size quota and is never silently evicted. Never silently truncate a document or claim a snippet is a read source. Cache reuse is explicit in output and refresh is available. Untrusted pages cannot grant authority.

## Real-interface scenarios, specified before execution

- Configure/start real SearXNG; search a public technical query and open a returned primary source. Observe partial/upstream failures honestly. Repeat setup and stop/start without orphaning or touching other containers.
- Read real HTTP/HTTPS HTML, redirects, plain text and relative links; preserve title/final URL and stable reference. Exclude script/style content while preserving useful code/preformatted text.
- Generate independently known Unicode documents and long lines; reconstruct the entire document with byte pages, then find literal tokens beyond the first page. Fresh process retrieval must preserve identity.
- Observe 404, unsupported scheme, bad references/offsets, unavailable search endpoint, malformed arguments, and request cancellation; subsequent valid operations still work.
- Render a JavaScript-populated page through actual Chromium and read the content omitted from its initial HTTP body. Close the temporary context, preserve other browser sessions, and reject local file navigation.
- Exercise web tools through the actual Peritus model/daemon conversation path, including read-only design/review availability and denied network/process permissions. Existing filesystem tools still work.
- Build current CLI/daemon with two Cargo jobs; inspect runtime errors and bounded resource ownership. Retain confirmed defect replays and final evidence in docs/testing.
