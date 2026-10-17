# Web tools

Peritus can search, read and cite web pages in Chat, Design, Review and coding runs, with any provider. Page reading is native HTTP(S) and requires no paid API, Python or browser. Search uses [SearXNG](https://docs.searxng.org/), a self-hosted metasearch service.

Start local keyless search with a working Docker or Podman installation:

```sh
peritus web setup
peritus web status
peritus web search 'Rust ownership official documentation'
```

Setup creates only a Peritus-owned SearXNG container, bound to an available loopback port, with two CPUs, 512 MiB memory and a PID bound. Its settings enable JSON and only keyless DuckDuckGo, Bing, Google, Brave, Wikipedia, Yahoo and Mwmbl engines. Search engines can still block requests or rate-limit; responses report engine failures separately from results. `peritus web stop` stops only the exact owned container, and `peritus web setup` restarts it. Setup does not install a container engine or change unrelated containers.

To use an existing server instead:

```sh
peritus web setup https://search.example.org/search
```

The endpoint must support SearXNG JSON (`search.formats: [html, json]`). Many public instances disable JSON. Configuration is saved in the user's Peritus data directory under `web/settings.json`. `PERITUS_WEB_SEARCH_URL` overrides it for the current process. A missing search service does not prevent opening known URLs. Status checks service health; an actual search is needed to establish engine availability.

The model receives four tools:

| Tool | Behavior |
| --- | --- |
| `web_search` | Discover URLs/snippets, with page, language, time range and optional engine selection. |
| `web_open` | Read a URL, extract HTML or textual content, retain an immutable source, and return its first page and links. |
| `web_read` | Read exact UTF-8 byte pages from that source, including after reconnect or a new process. |
| `web_find` | Find literal text throughout a retained source and return matching offsets/line numbers. |

Native opens reuse snapshots for 15 minutes and report cache reuse. `refresh=true` fetches new content; previous source references retain their exact original extracted text. Search query caching lasts two minutes with at most 128 entries. Durable source evidence is not silently evicted or limited by a total-size quota. It lives under `web/sources` in the Peritus data directory; archive it before removing old files if historical references matter.

`web_open render=true` uses the existing isolated Playwright runtime for JavaScript content. Install it with `peritus automation setup`. Rendering uses a temporary clean context, blocks non-HTTP(S) requests, closes unsolicited popups, dismisses dialogs and closes its browser afterward. A dedicated run-owned reader keeps web timeouts separate from interactive GUI sessions; a stalled reader is canceled at the controller RPC deadline and a later read opens a fresh one. It never attaches to personal profiles. The model can set `wait_ms` up to five seconds for delayed content. Prefer native reads for ordinary documents.

Network opens/search need Read and Network permissions. Retained reads/find need Read only. Rendering additionally needs Process. Revocation cannot be bypassed by a cached result. These read tools do not grant workspace writes, browser interactions, file uploads or profile access. Page text and search snippets are untrusted evidence, and the role instructions require actual source reads and citations.

Pages default to 32 KiB, can request up to 1 MiB, and expose `next_offset` until the whole document is read. Offsets count UTF-8 bytes; preserve the returned continuation rather than guessing character offsets. Metadata includes source identity, original/final URL, title, retrieval time, HTTP status and digests. Links are limited to the first 500; this does not truncate page text. Text decoding honors the HTTP charset. PDF and binary extraction are explicitly unsupported; use an HTML/text version. Transfers/rendered HTML have a 64 MiB resource bound, redirects a ten-hop bound, and HTTP requests a 30-second deadline. Errors do not become successful source records.

Direct CLI operations are useful for diagnostics and scripts:

```sh
peritus web open https://doc.rust-lang.org/book/
peritus web read SOURCE_REFERENCE 0 32768
peritus web find SOURCE_REFERENCE 'ownership'
```

The CLI prints JSON. JavaScript rendering is available to the model through its run-owned computer runtime; direct `web open` uses native HTTP. This is the foundation for research-assisted design and a later deep-research coordinator.
