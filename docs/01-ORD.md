# 01 — Outcome requirements

**Status: proposed. Remaining owner decisions are in [14](14-EXECUTION-TRACKER.md).** No product code exists. Numbers are targets, not measurements.

## The product

**CasualNote** is a multilingual, local-first workspace for documents, databases, canvases and pen notes, with a knowledge graph and an optional AI assistant. The engine, the sync server and the apps are all Apache-2.0.

It is an original product in the family of SiYuan, AppFlowy and AFFiNE, which the owner admires ([03](03-OWNER-INSTRUCTIONS.md) instruction 4). It is built on the OpenCalc architecture and joins CasualOffice.

## Why it should exist

Evidence is in [12](12-OPEN-SOURCE-AUDIT.md).

| Opening | Evidence |
| --- | --- |
| Permissive all the way through | AFFiNE's backend has commercial terms. AppFlowy, SiYuan, Trilium and Logseq are AGPL. Anytype and tldraw are restricted |
| Sync people can trust | Silent loss recurs across five audited apps, and conflicts are either a chore or invisible |
| Multilingual as the starting point | Right-to-left text recurs as a complaint in BlockSuite and Anytype |
| AI that the user controls | Their own provider on the web, a local model on desktop, and off by default |
| Notes that outlive the app | A published export format, plus a reader that needs nothing from us |
| A light desktop app | AFFiNE and Anytype use Electron, AppFlowy uses Flutter |

**Competitive risk.** AFFiNE is active, popular and much of it is MIT. The product now overlaps it heavily on features, so trust, licence, languages, weight and local AI have to carry the case.

## Who it is for

1. **People who keep serious notes for years,** in any language, who want privacy and no lock-in.
2. **Teams and organisations that must self-host** without AGPL review or per-seat licence terms.
3. **Developers embedding a workspace** in their own product.

## The promises

Each is a CI test before it can be claimed ([15](15-CI-AND-RELEASE-GATES.md)).

| ID | Promise | Proven by |
| --- | --- | --- |
| PR-1 | **Nothing you write is lost.** An edit is durable before it shows as saved. Offline edits are never dropped. Merged text is never hidden | Crash and fault-injection suite |
| PR-2 | **You can always see your sync state,** per device | Each injected fault appears in the state the panel reads |
| PR-3 | **You can walk away.** Everything is readable without us | `tools/cnotepack.py`, which shares no code with the app |
| PR-4 | **Typing stays fast** at any size | Benchmarks with limits that fail CI |
| PR-5 | **Every language is first class** | Pseudo-locale, Arabic run, per-language search and typing tests |
| PR-6 | **The assistant never changes a note without asking, and AI is optional** | With AI off, nothing leaves the device. Every assistant write needs approval. An MCP client is a separate program the person chose to connect; its writes can be switched off with `--read-only` ([63](63-AGENT-TOOLS-AND-MCP.md), `FND-69`) |

## Functional requirements

Priorities: **M** needed for the first release, **S** soon after, **L** later. Phases are in [06](06-ROADMAP-AND-DELIVERY.md).

| ID | Area | Requirement | Pri | Phase | Design |
| --- | --- | --- | --- | --- | --- |
| FR-01 | Organise | Workspaces, nested pages to any depth, favourites, tabs, trash for 30 days | M | 2 | [47](47-UX-AND-FEATURE-MAP.md) |
| FR-02 | Blocks | Text, headings, lists, to-dos, quotes, callouts, code, dividers; slash menu, Markdown shortcuts, drag to move, inline formatting | M | 2 | [42](42-EDITOR-AND-RENDERING.md) |
| FR-03 | Links | Page links with `[[`, backlinks, outline, tags | M | 2 | [22](22-DATA-MODEL.md) |
| FR-04 | Embeds | One page shown live inside another | S | 4 | [22](22-DATA-MODEL.md) |
| FR-05 | Databases | Rows and columns with table and board views, shared between pages | M | 2 | [22](22-DATA-MODEL.md) |
| FR-06 | Canvas | One switch between page and canvas, over the same content; sticky notes, shapes, arrows, text | M | 3 | [43](43-CANVAS-AND-INK.md) |
| FR-07 | Pen | Pressure, palm rejection, highlighter, partial erase; PDF annotation | S | 3 | [43](43-CANVAS-AND-INK.md) |
| FR-08 | Attachments | Large files (proposed limit 1 TiB in a notebook, and 2 GiB per file in a package until `FND-06` is settled), resumable, content-addressed, included in sync and export | M | 4 | [23](23-STORAGE-FORMAT.md) |
| FR-09 | Search | Instant, offline, every language, across pages, databases and canvases | M | 4 | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| FR-10 | Meaning | Search by meaning and related notes from an on-device multilingual model | S | 4 | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| FR-11 | Graph | Knowledge graph of pages, tags, rows and people, with links and similarity kept visibly apart | M | 4 | [61](61-KNOWLEDGE-GRAPH.md) |
| FR-12 | History | Version history per page with restore; merges appear as named events | M | 1 | [24](24-OPERATIONS-AND-MERGE.md) |
| FR-13 | Export | CasualNote Package, Markdown and HTML in Phase 1. PDF per page in Phase 3, when `-render` exists | M | 1, 3 | [36](36-EXPORT-FORMAT.md) |
| FR-14 | Import | Package, Markdown, HTML first; then Evernote, then others from public docs | M | 6 | [37](37-IMPORT-FORMATS.md) |
| FR-15 | Assistant, web | Connect the user's own AI provider. Providers that refuse browser calls need the relay, in Phase 8 | M | 5 | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| FR-16 | Assistant, desktop | Download and run a local model | M | 6 | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| FR-17 | Agents | One tool set for the assistant, an MCP server and a CLI | M | 5 | [63](63-AGENT-TOOLS-AND-MCP.md) |
| FR-18 | Languages | The launch languages throughout, right-to-left included | M | 2 | [50](50-INTERNATIONALIZATION.md) |
| FR-19 | Sync | Encrypted sync through any folder, WebDAV or S3, as each host allows (`FND-21`: browsers cannot reach every storage); visible state; merge review | S | 7 | [56](56-SYNC-DESIGN.md) |
| FR-20 | Collaboration | Real-time co-editing, comments, sharing, permissions through a self-hosted server | S | 8 | [57](57-SERVER-DESIGN.md) |
| FR-21 | Enterprise | SSO, audit, retention, admin | L | 10 | [10](10-ENTERPRISE-REQUIREMENTS.md) |
| FR-22 | Hosts | Web and desktop first; tablets through the web; mobile shells after a prototype | M | 6, 9 | [44](44-DESKTOP-SHELL.md) |
| FR-23 | Embedding | Published SDK for developers | L | 10 | [55](55-SDK-AND-EMBEDDING.md) |

## Quality requirements

Details and test method are in [30](30-PERFORMANCE-TARGETS.md) and [07](07-SECURITY-THREAT-MODEL.md). All targets are proposals.

| ID | Requirement |
| --- | --- |
| QR-1 | On a 5,000-block page, main-thread work for a keystroke finishes within 16 ms at the 95th percentile, measured from the input event. Durability follows it and never holds up the keyboard |
| QR-2 | Search within 50 ms across 100,000 pages in WebAssembly |
| QR-3 | Same input and version gives the same model, export bytes and keyword search results, native and web. Vector results depend on the model and runtime, and are not promised to match bit for bit (`FND-62`) |
| QR-4 | Every file and remote update is hostile: parsers are bounded and fuzzed |
| QR-5 | WCAG 2.2 AA, including screen readers in the editor |
| QR-6 | No telemetry by default, and fully usable air-gapped |
| QR-7 | No document string reaches the DOM as HTML unsanitised |

## Out of scope

- **OneNote compatibility** (instruction 3).
- **A hosted service run by this project,** for now. It would be a separate decision (`DEC-03`).
- **Plugins** that run third-party code, until a sandbox is designed.
- **Handwriting recognition** at first: no good permissive on-device option was found ([12](12-OPEN-SOURCE-AUDIT.md)). Strokes are kept so it can be added later.

## What this replaces

This document, [02](02-ARCHITECTURE.md) and the low-level designs replace the earlier single design note. Its "not a database tool" and "not an AI product" lines were reversed by owner instructions 4 and 6.
