# Documentation index

**Status: building (owner instruction 11).** The engine and web editor run. Everything else here is design. Numbers are targets until a prototype or test says otherwise, and a "Proposed" decision can still change. Read in this order if new: 01, 02, 19, then the area you care about.

## Product and planning

| Doc | What it is |
| --- | --- |
| [01 — Outcome requirements](01-ORD.md) | The product, who it is for, promises, requirements |
| [03 — Owner instructions](03-OWNER-INSTRUCTIONS.md) | Everything the owner has instructed. First in authority |
| [05 — Phases and tasks](05-PHASES-AND-TASKS.md) | How work opens and closes, Definition of Done |
| [06 — Roadmap and delivery](06-ROADMAP-AND-DELIVERY.md) | **The only roadmap,** with risk register |
| [14 — Execution tracker](14-EXECUTION-TRACKER.md) | **The only tracker,** with owner decisions |
| [12 — Open-source audit](12-OPEN-SOURCE-AUDIT.md) | Every repository checked, and what its licence allows |
| [10 — Enterprise requirements](10-ENTERPRISE-REQUIREMENTS.md) | The enterprise bar and when it is delivered |

## High-level design

| Doc | What it is |
| --- | --- |
| [02 — Architecture](02-ARCHITECTURE.md) | HLD: context, layers, data flows, trust boundaries |
| [19 — Workspace and layers](19-WORKSPACE-SCAFFOLD-DESIGN.md) | Crates, dependencies, boundary rules |
| [08 — ADR register](08-ADR-REGISTER.md) | Every decision and its status |
| [17 — Glossary](17-GLOSSARY.md) | Terms |

## Low-level design

| Doc | What it is |
| --- | --- |
| [22 — Data model](22-DATA-MODEL.md) | Types, invariants, repair, mapping to the package |
| [24 — Operations and merge](24-OPERATIONS-AND-MERGE.md) | Op set, undo, CRDT adapter, overlap detection, `SPK-01` |
| [23 — Storage format](23-STORAGE-FORMAT.md) | Live notebook, segments, durability, crash recovery, blobs, mirror |
| [36 — Export format](36-EXPORT-FORMAT.md) | CasualNote Package, version 1 draft |
| [37 — Import formats](37-IMPORT-FORMATS.md) | Package, Markdown, HTML, Evernote, import report |
| [42 — Editor and rendering](42-EDITOR-AND-RENDERING.md) | Op-driven DOM editing, headless output, `SPK-02` |
| [43 — Canvas and ink](43-CANVAS-AND-INK.md) | Two views of one page, pen pipeline, `SPK-03` |
| [44 — Desktop shell](44-DESKTOP-SHELL.md) | Tauri host, downloads, model process, updates |
| [47 — UX and feature map](47-UX-AND-FEATURE-MAP.md) | Screens, states, shortcuts, commands |
| [49 — Design system](49-DESIGN-SYSTEM.md) | Tokens, components, the no-slop rules |
| [50 — Internationalisation](50-INTERNATIONALIZATION.md) | Multilingual rules, right-to-left, input, translation |
| [55 — SDK and embedding](55-SDK-AND-EMBEDDING.md) | Rust, wasm and npm surfaces |
| [56 — Sync design](56-SYNC-DESIGN.md) | Any-storage sync, cryptography, trimming, `SPK-04` |
| [57 — Server design](57-SERVER-DESIGN.md) | Co-editing, sharing, workspace modes |
| [60 — Search and embeddings](60-SEARCH-AND-EMBEDDINGS.md) | Index, tokenising, vectors, `SPK-05`, `SPK-06` |
| [61 — Knowledge graph](61-KNOWLEDGE-GRAPH.md) | Derived nodes and edges, layout |
| [62 — AI assistant and providers](62-AI-ASSISTANT-AND-PROVIDERS.md) | Web provider and desktop local model modes, `SPK-07` |
| [63 — Agent tools and MCP](63-AGENT-TOOLS-AND-MCP.md) | One tool set, approval, MCP server, CLI |

## Quality and security

| Doc | What it is |
| --- | --- |
| [07 — Threat model](07-SECURITY-THREAT-MODEL.md) | Threats, controls, test status |
| [15 — CI and release gates](15-CI-AND-RELEASE-GATES.md) | Test levels, jobs, gates, release checklist |
| [20 — Error code registry](20-ERROR-CODE-REGISTRY.md) | Every refusal code |
| [21 — Limits](21-PARSER-LIMITS.md) | Every bound |
| [30 — Performance targets](30-PERFORMANCE-TARGETS.md) | Targets and how they are tested. None measured |

## Other

- [Clickable prototype](prototypes/casualnote-prototype.html): throwaway, for judging the product. No engine.
- Schema: `schemas/cnotepack-1.schema.json`. Reader: `tools/cnotepack.py`. Sample: `fixtures/sample-workspace.cnotepack`.
