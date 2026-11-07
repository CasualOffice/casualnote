# 19 — Workspace and layer division

**The layer-division contract.** Changing a boundary needs an ADR. Nothing here exists yet, and **a crate joins the workspace only when it has a first consumer,** so this is the target rather than a to-do list for empty crates.

Prefix: `casual-note-`.

## Crates

| Crate | Owns | May depend on | Must never own | Phase |
| --- | --- | --- | --- | --- |
| `-model` | Workspace, page, block, inline, database, canvas, ink types. Schema rules as pure functions. Stable ids. See [22](22-DATA-MODEL.md) | Nothing internal | I/O, CRDT types, UI, network | 1 |
| `-merge` | The adapter over the CRDT library, per-page and tree documents, projection to `-model`, overlap detection. See [24](24-OPERATIONS-AND-MERGE.md) | `-model`, `loro` | Product semantics. No other crate may name `loro` | 1 |
| `-ops` | The closed operation set, validation, inverses, undo, op batches. See [24](24-OPERATIONS-AND-MERGE.md) | `-model`, `-merge` | Transform functions, layout | 1 |
| `-package` | Bounded ZIP reading and writing, path safety. Copied from OpenCalc | None | Notes semantics | 1 |
| `-store` | Durable log, snapshots, blobs, readable mirror, over a host I/O seam. See [23](23-STORAGE-FORMAT.md) | `-model`, `-merge`. The mirror's rendering comes in as a `MirrorRender` value from above | Network | 1 |
| `-crypto` | Key hierarchy, sealed segments. See [56](56-SYNC-DESIGN.md) | RustCrypto crates | Key storage, which is the host's | 7 |
| `-text` | Fonts and shaping for headless output. Copied from OpenCalc | `harfrust`, `skrifa` | Editing | 3 |
| `-ink` | Stroke model, smoothing, outlines, hit-testing. See [43](43-CANVAS-AND-INK.md) | `-model`, `kurbo` | Pointer events | 3 |
| `-layout` | Headless layout of pages and canvases, display list. See [42](42-EDITOR-AND-RENDERING.md) | `-model`, `-text`, `-ink`, `parley` | The live text editing surface | 3 |
| `-render` | PNG and PDF from the display list | `-layout`, `tiny-skia` | Geometry | 3 |
| `-export` | Package writer, Markdown, HTML. See [36](36-EXPORT-FORMAT.md) | `-model`, `-package` | Reading files | 1 |
| `-import` | Package reader and format importers, report. See [37](37-IMPORT-FORMATS.md) | `-model`, `-ops`, `-package` | Network | 1 |
| `-search` | Tokenising per script, inverted index. See [60](60-SEARCH-AND-EMBEDDINGS.md) | `-model` | Vectors | 4 |
| `-embed` | Chunking, vector store, nearest-neighbour search. The model is a host seam. See [60](60-SEARCH-AND-EMBEDDINGS.md) | `-model` | Model inference, network | 4 |
| `-graph` | Derives the knowledge graph. See [61](61-KNOWLEDGE-GRAPH.md) | `-model`, `-embed` | Any stored graph | 4 |
| `-sync` | Device registry, segment exchange, sync state, trimming. See [56](56-SYNC-DESIGN.md) | `-store`, `-crypto`, `-merge` | Transport | 7 |
| `-sdk` | The stable Rust facade | All above | Logic of its own | 1 |
| `-wasm` | The `wasm-bindgen` bridge | Same set as `-sdk` | Logic of its own | 2 |
| `-tools` | The agent tool set, with schemas. See [63](63-AGENT-TOOLS-AND-MCP.md) | `-sdk` | Transport, AI provider | 5 |

## Outside `crates/`

| Path | What | Phase |
| --- | --- | --- |
| `webapp/` | The web app: plain ES modules, no framework or bundler | 2 |
| `desktop/` | Tauri 2 shell, in its own Cargo workspace. Runs model downloads and the local model ([44](44-DESKTOP-SHELL.md)) | 6 |
| `server/casual-note-server` | Sync, sharing, co-editing ([57](57-SERVER-DESIGN.md)) | 8 |
| `server/casual-note-mcp` | MCP over stdio ([63](63-AGENT-TOOLS-AND-MCP.md)) | 5 |
| `server/casual-note-secrets` | Secret handling, copied from OpenCalc's `casual-calc-secrets` | 8 |
| `sdk/` | npm packages and examples ([55](55-SDK-AND-EMBEDDING.md)) | 10 |
| `schemas/` | Published JSON Schemas | Started |
| `tools/` | Repo gates, standalone package reader | Started |
| `fixtures/` | Sample and hostile packages, per-language test sets | Started |
| `fuzz/` | Separate workspace, one target per parser | 0 |

**Open placement questions:** the assistant loop, the embedder and the CLI (`FND-60`), `-wasm` depending on `-tools` (`FND-61`), native storage adapters (`FND-22`), and how much of the engine loads lazily (ADR-028).

## Boundary rules

1. **Only `-merge` names the CRDT library.** Replacing Loro touches one crate and the storage format. Checked by `check-crate-deps.py`.
2. **Only `-ops` writes to the model.** Import, tools and sync all go through it, except that merged remote changes arrive through `-merge`.
3. **No crate under `crates/` names a platform.** Copied from OpenCalc: no `cfg(target_os)`, `cfg(target_arch)` or `cfg(target_family)`.
4. **No crate under `crates/` has network or AI code.** Hosts provide them.
5. **Derived crates are one-way.** `-search`, `-embed` and `-graph` read the model and never write to it.
6. **Nothing under `crates/` depends on `server/` or `desktop/`.**

## Dependency policy

`unsafe_code = "forbid"` in our crates, `overflow-checks = true` in release, Rust 2024, resolver 3. Toolchain as OpenCalc: MSRV 1.88.0, development 1.96.0 (ADR-024). `cargo deny` allows Apache-2.0, MIT, BSD, ISC and Zlib; MPL only by named exception; GPL and AGPL fail CI (ADR-023).

**Dependencies carrying `unsafe`.** `forbid` covers our code only. `loro` has unsafe sites, and desktop's llama.cpp is C++, which is one reason it runs outside the engine. Each is recorded in the ADR that admits it.

## Provenance

Each copied crate carries a `PROVENANCE.md` naming the OpenCalc commit and local changes, as OpenPresent does.
