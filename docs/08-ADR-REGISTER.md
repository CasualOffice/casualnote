# 08 — ADR register

ADR ids here are this repository's own, and OpenCalc's are cited as "its ADR-n". **Accepted** means the owner instructed it, or agreed. **Proposed** means it is the working assumption, and the named prototype or owner decision settles it. Changing an Accepted ADR needs a new one.

**Write an ADR when a decision touches:** a public API; a crate boundary; a stored or exchanged format; the op set or undo; a security or parser policy; a dependency in the engine; the AI trust boundary; a performance budget that constrains a lower layer.

| ADR | Decision | Status | Settled by | Reasoning in |
| --- | --- | --- | --- | --- |
| ADR-001 | Product name CasualNote, crates `casual-note-*` | Accepted, instruction 10. Trademark search outstanding | `DEC-08` | [01](01-ORD.md) |
| ADR-002 | Everything is Apache-2.0, with no enterprise edition, licence key or commercial folder in this repo | Accepted, instructions 3 and 10 | Done | [01](01-ORD.md) |
| ADR-003 | Layer division is fixed before code | Proposed | Owner review | [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) |
| ADR-004 | Follow OpenCalc's architecture; copy with provenance, never modify it | Accepted, instruction 1 | Done | [02](02-ARCHITECTURE.md) |
| ADR-005 | A closed op set is the only write path; a CRDT does the merging; there is no `transform` | Proposed | `SPK-01` | [24](24-OPERATIONS-AND-MERGE.md) |
| ADR-006 | One CRDT document per page, one per database, and one for the page tree, using Loro behind `-merge`. Databases have their own document because several pages share them (FR-05) | Proposed | `SPK-01` | [24](24-OPERATIONS-AND-MERGE.md) |
| ADR-007 | All text is edited in the real DOM, including text on canvas items; ink is drawn on canvas; the engine stays the source of truth | Proposed | `SPK-02` | [42](42-EDITOR-AND-RENDERING.md) |
| ADR-008 | Strokes are immutable records, with samples stored as compact blobs, never per-point CRDT values | Proposed | `SPK-03` | [43](43-CANVAS-AND-INK.md) |
| ADR-009 | Our own small search index, the same on native and web. Tantivy is rejected | Proposed | `SPK-05` | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| ADR-010 | The op log is the source of truth, and readable files are a one-way mirror | Proposed | `SPK-01` | [23](23-STORAGE-FORMAT.md) |
| ADR-011 | Tauri, not Electron or Flutter | Accepted, inherited under ADR-004 | Done | [44](44-DESKTOP-SHELL.md) |
| ADR-012 | Two assistant modes behind one provider interface: the user's provider on the web, a downloaded local model on desktop | Accepted, instruction 7 | Done | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| ADR-013 | One closed tool set for assistant, MCP and CLI. Every assistant write needs approval and is one undo step | Accepted for the tool set, instruction 6. Approval rule Proposed | Owner review | [63](63-AGENT-TOOLS-AND-MCP.md) |
| ADR-014 | The knowledge graph is derived on read, with no separate store | Proposed | Owner review | [61](61-KNOWLEDGE-GRAPH.md) |
| ADR-015 | Embeddings are computed on the device, stored as a rebuildable index, never synced. The default model is multilingual | Proposed for the storage rule. Multilingual Accepted, instruction 7 | `SPK-06` | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| ADR-016 | The local runtime is llama.cpp, run outside the engine, on loopback with a random key and restricted origins | Proposed | `SPK-07` | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| ADR-017 | The CasualNote Package is the stable export format, and holds no CRDT state | Accepted for having a format, instruction 8. Version 1 is a draft | Owner review | [36](36-EXPORT-FORMAT.md) |
| ADR-018 | Multilingual first: no hard-coded strings, logical CSS, ICU MessageFormat, per-script search | Accepted, instruction 7 | Done | [50](50-INTERNATIONALIZATION.md) |
| ADR-019 | Block model, with databases as first-class data referenced from pages | Accepted, direction from instruction 4. Schema Proposed | Owner review | [22](22-DATA-MODEL.md) |
| ADR-020 | Page and canvas are two views over one page's content | Accepted, direction from instruction 4. Details Proposed | Owner review | [43](43-CANVAS-AND-INK.md) |
| ADR-021 | Any-storage sync: each device writes only its own new encrypted segments | Proposed | `SPK-04` | [56](56-SYNC-DESIGN.md) |
| ADR-022 | Server: `axum`, WebSocket, Postgres, object storage, with end-to-end encrypted and managed workspace modes | Proposed | Owner review | [57](57-SERVER-DESIGN.md) |
| ADR-023 | Licence gate: Apache, MIT, BSD, ISC, Zlib. MPL by named exception. GPL and AGPL fail CI. No code or design copied from copyleft or source-available projects | Accepted, instruction 3 | Done | [12](12-OPEN-SOURCE-AUDIT.md) |
| ADR-024 | Toolchain as OpenCalc: MSRV 1.88.0, development 1.96.0 | Proposed | Owner review | [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) |
| ADR-025 | Web app in plain ES modules with no framework or bundler, as OpenCalc | Proposed | `SPK-02` | [42](42-EDITOR-AND-RENDERING.md) |
| ADR-026 | Icons from Phosphor (MIT), inlined. No emoji as interface icons | Proposed, follows instruction 5 | Owner review | [49](49-DESIGN-SYSTEM.md) |
| ADR-027 | Model weights and fonts are dependencies with their own licence review, shipped separately from the code | Proposed | Owner review | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| ADR-028 | The wasm engine loads in parts, with heavier features such as search models and PDF loaded on demand | Proposed | `SPK-02` | [42](42-EDITOR-AND-RENDERING.md) |
| ADR-029 | PDF backgrounds are parsed by PDF.js in a worker, after a licence check | Proposed | `FND-42`, `SPK-03` | [43](43-CANVAS-AND-INK.md) |
| ADR-030 | A fresh CRDT peer id per open, so counters are never reused. Forks salvage through a visible restored copy, never by trusting import to merge them | Proposed | `SPK-01` | [24](24-OPERATIONS-AND-MERGE.md) |
| ADR-031 | Ops address targets by CRDT anchors plus a client sequence, so a device's own in-flight edits and remote imports never discard typed text | Proposed | `SPK-02` | [24](24-OPERATIONS-AND-MERGE.md) |
| ADR-032 | The package holds ink in `ink/<page>.jsonl`, compact and streamed, apart from page JSON | Proposed | Owner review | [36](36-EXPORT-FORMAT.md) |
| ADR-033 | Shell commands that redirect data need a confirmation drawn natively, and the model runs sandboxed | Proposed | `SPK-07` | [44](44-DESKTOP-SHELL.md) |

## Reversals on record

- **"Not a database tool" and "not an AI product"** were early proposals, reversed by owner instructions 4 and 6.
- **Clone of OneNote** was instruction 1, reversed by instruction 3.
- **Server-ordered OT** was the first assumption, inherited from OpenCalc, and replaced by ADR-005 after the audit.
