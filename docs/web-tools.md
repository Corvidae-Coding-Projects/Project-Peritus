# Web tools

Peritus can search, read and cite web pages in Chat, Design, Review and coding runs, with any provider. Page reading is native HTTP(S) and requires no paid API, Python or browser. Search is compiled into Peritus using the MIT-licensed [metasearch-rs library](https://github.com/MikeLuu99/metasearch-rust), pinned to 0.3.1 (the library release before its unrelated TUI dependency). There is no service to install or start, no first-use download, and no Python, Docker, Podman, API key or separate account requirement.

Search immediately:

```sh
peritus web search 'Rust ownership official documentation'
peritus web status
```

Built-in search queries Yahoo, DuckDuckGo, Brave and Startpage concurrently and merges duplicate URLs with reciprocal-rank fusion. Per process, only one fan-out runs at a time; engines have an eight-second network deadline. Cancellation also interrupts waiting searches. No search child processes or listening ports are created. Upstream engines can rate-limit, block, or change their HTML. Results report failed and empty engines separately; all-empty/all-failed searches return an actionable error, are not cached as successes, and do not prevent opening known URLs. Status reports local readiness, not a claim that upstream engines are reachable.

Native search accepts a comma-separated subset of those engines and up to 30 results. It supports the first ranked result set only (`page=1`); refine queries for further sources. Include date/language constraints in the query: native date/language filter parameters and later pages are explicitly rejected rather than silently ignored.

An optional existing SearXNG JSON server can still be selected:

```sh
peritus web setup https://search.example.org/search
peritus web setup # return to built-in Rust search; no provisioning
```

The endpoint must support SearXNG JSON (`search.formats: [html, json]`). External endpoints retain page/language/time/engine parameters. Configuration lives under `web/settings.json` in the user's Peritus data directory. `PERITUS_WEB_SEARCH_URL` overrides saved selection for that process. Legacy automatically managed container endpoints migrate to built-in search; their identity is retained only for explicit `peritus web stop` cleanup, with the existing exact ownership check. Containers are never started automatically. Built-in search has no background service to stop; external services are never stopped.

The model receives four tools:

`web_read` returns text without repeating the outgoing link catalog by default. Set
`include_links=true` at offset zero to retrieve those retained links. `web_open` still includes
links by default; either tool accepts `include_links=false` for focused text reading.

| Tool | Behavior |
| --- | --- |
| `web_search` | Discover URLs/snippets immediately, with optional native engine selection; external SearXNG additionally supports pages/date/language filters. |
| `web_open` | Read a URL, extract HTML or textual content, retain an immutable source, and return its first page and links. |
| `web_read` | Read exact UTF-8 byte pages from that source, including after reconnect or a new process. |
| `web_find` | Find literal text throughout a retained source and return matching offsets/line numbers. |

Native opens reuse snapshots for 15 minutes and report cache reuse. `refresh=true` fetches new content; previous source references retain their exact original extracted text. Search query caching lasts two minutes with at most 128 entries. Durable source evidence is not silently evicted or limited by a total-size quota. It lives under `web/sources` in the Peritus data directory; archive it before removing old files if historical references matter.

`web_open render=true` uses the isolated Playwright runtime for JavaScript content. Native installation provisions it automatically on Linux, macOS, and Windows; portable and system-package installs prepare it on first interactive launch. `peritus automation setup` repairs the runtime. Rendering uses a temporary clean context, blocks non-HTTP(S) requests, closes unsolicited popups, dismisses dialogs and closes its browser afterward. A dedicated run-owned reader keeps web timeouts separate from interactive GUI sessions; a stalled reader is canceled at the controller RPC deadline and a later read opens a fresh one. It never attaches to personal profiles. The model can set `wait_ms` up to five seconds for delayed content. Prefer native reads for ordinary documents.

Network opens/search need Read and Network permissions. Retained reads/find need Read only. Rendering additionally needs Process. Revocation cannot be bypassed by a cached result. These read tools do not grant workspace writes, browser interactions, file uploads or profile access. Page text and search snippets are untrusted evidence, and the role instructions require actual source reads and citations.

Pages default to 32 KiB, can request up to 1 MiB, and expose `next_offset` until the whole document is read. Offsets count UTF-8 bytes; preserve the returned continuation rather than guessing character offsets. Metadata includes source identity, original/final URL, title, retrieval time, HTTP status and digests. Links are limited to the first 500; this does not truncate page text. Text decoding honors the HTTP charset. PDF and binary extraction are explicitly unsupported; use an HTML/text version. Native page transfers/rendered HTML have a 64 MiB resource bound, redirects a ten-hop bound, and HTTP requests a 30-second deadline. Errors do not become successful source records.

Direct CLI operations are useful for diagnostics and scripts:

```sh
peritus web open https://doc.rust-lang.org/book/
peritus web read SOURCE_REFERENCE 0 32768
peritus web find SOURCE_REFERENCE 'ownership'
```

The CLI prints JSON. JavaScript rendering is available to the model through its run-owned computer runtime; direct `web open` uses native HTTP. This is the foundation for research-assisted design and a later deep-research coordinator.
