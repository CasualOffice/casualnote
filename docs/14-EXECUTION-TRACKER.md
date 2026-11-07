# 14 — Execution tracker

**The only tracker.** Ids are stable and never reused. Status: `Open`, `In progress`, `Blocked`, `Done`, defined in [05](05-PHASES-AND-TASKS.md). Phases and risks are in [06](06-ROADMAP-AND-DELIVERY.md). **Nothing is `Done` unless evidence is named.** A clickable-prototype row being `Done` says nothing about the product.

## Owner decisions

| ID | Decision | St | Recommendation | Why it matters | Blocks |
| --- | --- | --- | --- | --- | --- |
| DEC-01 | Product name | Done | Owner, 2026-11-07: **CasualNote.** `casualnote` was free on crates.io and npm that day. **No trademark search done,** which stays with `DEC-08` | Name, file extension and domains | Done |
| DEC-02 | No enterprise edition, licence key or commercial folder in this repo | Done | Owner, 2026-11-07: **yes.** Revenue, if any, comes from hosting and support | Rules out selling closed features from here | Done |
| DEC-03 | Whether this project ever runs a hosted service | Open | Not now. Users bring their own provider and storage | Cost, and privacy promises | Not blocking until Phase 8 |
| DEC-04 | Which Chinese repo the owner meant | Open | Assumed SiYuan. Another project may change what can be reused | The audit scope | Not blocking |
| DEC-05 | Pen devices available | Open | An iPad with Pencil helps most | `SPK-03` | Phase 3 |
| DEC-06 | Working rules, carried from OpenPresent: direct to `main`, parallel workers, decisions without asking | Done | Owner, 2026-11-07: **yes** | How work lands | Done |
| DEC-07 | Windows access | Open | CI runners until a release candidate, then a borrowed or rented machine. `SPK-02` also wants Windows input methods, which CI can only partly cover | The owner has no Windows machine | Partly `SPK-02`; Phase 6 |
| DEC-08 | Legal review: name, dependency patents, model and font licences | Open | Needs a lawyer | Enterprise buyers will ask | First public release |
| DEC-09 | Launch languages | Done | Owner, 2026-11-07: **English, Simplified and Traditional Chinese, Japanese, Korean, Spanish, French, German, Portuguese, Arabic, Hindi** | Translation and testing cost | Done |
| DEC-10 | Default local model | Open | Qwen3 4B, pending `SPK-07` | First impression on desktop | Phase 6 |
| DEC-11 | Accent colour | Open | Keep the prototype's cobalt, `#1f47d6` | Distinct from OpenCalc and OpenPresent | Phase 2 |
| DEC-12 | `git init` and the GitHub repo | In progress | Owner, 2026-11-07: `git init` **yes,** done locally. The remote is not created yet. Proposed: `CasualOffice/casualnote` | CI needs a remote | `GOV-03` |

## Phase D — Design

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| DOC-001 | Open-source audit | Done | P1 | [12](12-OPEN-SOURCE-AUDIT.md). Read licences, docs and issues. No app was run | Document exists |
| DOC-002 | Requirements and HLD | Done | P0 | [01](01-ORD.md), [02](02-ARCHITECTURE.md), [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) | `check-docs.py` |
| DOC-003 | Low-level designs | Done | P0 | Listed in [00](00-README.md) | `check-docs.py` |
| DOC-004 | Threat model, limits, error registry | Done | P0 | [07](07-SECURITY-THREAT-MODEL.md), [21](21-PARSER-LIMITS.md), [20](20-ERROR-CODE-REGISTRY.md) | `check-docs.py` |
| DOC-005 | Roadmap, phases, tracker, ADRs | Done | P0 | [06](06-ROADMAP-AND-DELIVERY.md), [05](05-PHASES-AND-TASKS.md), this page, [08](08-ADR-REGISTER.md) | `check-docs.py` |
| DOC-006 | Project files | Done | P1 | README, AGENTS, CLAUDE, CONTRIBUTING, SECURITY, GOVERNANCE, CODE_OF_CONDUCT, NOTICE, CHANGELOG | `check-docs.py` |
| DOC-007 | Document consistency gate | Done | P1 | `tools/check-docs.py`: links, tracker ids, index, ADR references, error codes, banned claims. Seeded violations were seen failing | `check-docs.py --selftest` |
| DOC-008 | Independent review of the design set | In progress | P1 | Owner agreed, 2026-11-07. Fresh readers audit for contradictions and unstated assumptions | Findings dispositioned |
| PROTO-01 | Clickable prototype | Done | P1 | [Prototype](prototypes/casualnote-prototype.html). Blocks, databases, canvas, graph, assistant, settings, six interface languages. Runs in a browser with no Rust | Scripted checks in Chrome, 2026-11-07. Hand dragging not tested |
| PROTO-02 | Prototype assistant against the live model | Open | P2 | Tested only with a stand-in model | Owner tries it |
| PROTO-03 | Prototype translations reviewed | Open | P2 | Drafts by the design author, not native-reviewed | Native review |
| PKG-01 | Package format, version 1 draft | Done | P0 | [36](36-EXPORT-FORMAT.md), `schemas/cnotepack-1.schema.json` | Sample package passes schema and reader |
| PKG-02 | Standalone reader | Done | P0 | `tools/cnotepack.py`, standard library only | `tools/test_cnotepack.py`: 24 tests. Path and hash checks removed and tests seen red. Byte-flip sweep first failed on raw crashes, then fixed |
| PKG-03 | Format review by someone outside the project | Open | P1 | Not done | Findings dispositioned |

## Phase 0 — Foundation

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| GOV-01 | `git init`, hooks, commit rules | Done | P0 | `.githooks/pre-commit` runs `check-docs.py` and the reader tests. `.githooks/commit-msg` requires Conventional Commits. Enable with `git config core.hooksPath .githooks` | A broken doc and a bad message were each seen refused |
| GOV-02 | Cargo workspace, toolchain, lints | Open | P0 | [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) | `cargo check` on both targets |
| GOV-03 | CI on three platforms | Blocked | P0 | Waits on `DEC-12`: no remote exists. [15](15-CI-AND-RELEASE-GATES.md) | Seeded violation fails each job |
| GOV-04 | Crate dependency gate | Open | P0 | `check-crate-deps.py`: layer rules, only `-merge` names `loro` | Seeded edge fails |
| GOV-05 | Licence gate | Open | P0 | `cargo deny`, weights and fonts list | Seeded GPL crate fails |
| GOV-06 | Host-seam gate | Open | P1 | Copied from OpenCalc's `check-host-seams.py` | Seeded `cfg` fails |
| GOV-07 | Fuzz workspace | Open | P0 | Separate workspace, pinned nightly | One target runs |
| SPK-01 | Merge: does Loro survive years of notes? | Open | P0 | 5-year, 3-device simulated history, including 90 days offline, and real notes in the launch languages. Decides ADR-005, ADR-006, ADR-010 | Pass and fail in [24](24-OPERATIONS-AND-MERGE.md) |
| SPK-02 | Editor: can DOM editing be op-driven and fast? | Open | P0 | 5,000-block page, Chrome, Safari, Firefox, input methods, right-to-left, screen reader. Repeated in desktop webviews (WKWebView, WebView2, WebKitGTK) before Phase 6. Decides ADR-007, ADR-025, ADR-028 | Pass and fail in [42](42-EDITOR-AND-RENDERING.md) |

## Later prototypes

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| SPK-03 | Pen in a browser and webview | Blocked | P1 | Waits on `DEC-05`. Decides ADR-008 | [43](43-CANVAS-AND-INK.md) |
| SPK-04 | Segment sync on real storage | Open | P1 | Fault injection across a folder, WebDAV, S3. Decides ADR-021 | [56](56-SYNC-DESIGN.md) |
| SPK-05 | Our own search index | Open | P1 | 100,000 pages in wasm, all launch scripts. Decides ADR-009 | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| SPK-06 | Real embedding model on the device | Open | P1 | Compare multilingual candidates, runtimes and size in wasm. Decides ADR-015 | [60](60-SEARCH-AND-EMBEDDINGS.md) |
| SPK-07 | Local model from the desktop shell | Open | P1 | Download, run, tool calling, languages, port security. Decides ADR-016, `DEC-10` | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |

## Phase 1 — Engine

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| MOD-01 | `-model` types and invariants | Open | P0 | [22](22-DATA-MODEL.md) | Property tests |
| MOD-02 | Schema repair as a pure projection | Open | P0 | Repairs never write back | Generated invalid states repair the same way on every replica |
| OPS-01 | Closed op set | Open | P0 | [24](24-OPERATIONS-AND-MERGE.md) | Every op has a validation test |
| OPS-02 | Undo with inverses over merged state | Open | P0 | Local undo only | Generated concurrent undo tests |
| MRG-01 | `-merge` adapter over Loro | Blocked | P0 | Waits on `SPK-01` | Only crate naming `loro` |
| MRG-02 | Guarded CRDT import | Open | P0 | Size limits, verify on load | Hostile updates refused, none panic |
| MRG-03 | Overlap detection for merge review | Open | P1 | [24](24-OPERATIONS-AND-MERGE.md) | Labelled histories |
| STO-01 | Durable log with fsync before ack | Open | P0 | [23](23-STORAGE-FORMAT.md) | Crash suite |
| STO-02 | Snapshots and compaction | Open | P0 | | Rebuild equals original |
| STO-03 | Host I/O seam, native and OPFS | Open | P0 | | Same bytes on both |
| STO-04 | Readable mirror | Open | P1 | One-way, regenerated | Deleting it loses nothing |
| STO-05 | Version history and restore | Open | P1 | | Restore round trip |
| PKG-04 | `-package` copied from OpenCalc | Open | P0 | Provenance recorded | Hostile ZIP fixtures |
| PKG-05 | Engine package writer | Open | P0 | | `tools/cnotepack.py` and schema pass. Bytes match on native and wasm |
| PKG-06 | Engine package reader and import report | Open | P0 | All `CN-PKG-*` | Fixture per code, fuzz target |
| SDK-01 | `-sdk` facade, first surface | Open | P1 | [55](55-SDK-AND-EMBEDDING.md) | Used by tests |

## Phase 2 — Web editor

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| WEB-01 | `-wasm` bridge | Open | P0 | | Same results as native |
| WEB-02 | App shell: sidebar, tabs, panels | Open | P0 | [47](47-UX-AND-FEATURE-MAP.md) | Keyboard and screen reader pass |
| WEB-03 | Block editor on the real DOM | Blocked | P0 | Waits on `SPK-02` | QR-1 |
| WEB-04 | Slash menu, Markdown shortcuts, formatting | Open | P1 | | Interaction tests |
| WEB-05 | Page links, backlinks, outline, tags | Open | P1 | | Interaction tests |
| WEB-06 | Service worker, offline start | Open | P1 | | Loads with the network off |
| DB-01 | Databases: model and operations | Open | P0 | [22](22-DATA-MODEL.md) | Merge tests |
| DB-02 | Table and board views | Open | P1 | | Interaction tests |
| I18N-01 | Message catalogue with ICU MessageFormat | Open | P0 | [50](50-INTERNATIONALIZATION.md) | Pseudo-locale finds no raw string |
| I18N-02 | Logical CSS, right-to-left | Open | P0 | | CSS gate refuses physical properties. Arabic run passes |
| I18N-03 | Input-method support | Open | P0 | | Composition tests in Chinese, Japanese, Korean |
| I18N-04 | Fonts by script | Open | P1 | | Every launch script renders |
| I18N-05 | Translation workflow and native review | Open | P1 | | A language is called supported only after review |
| A11Y-01 | Accessibility baseline | Open | P0 | QR-5 | Audit passes |

## Phase 3 — Canvas and ink

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| CNV-01 | Canvas view over a page | Open | P0 | [43](43-CANVAS-AND-INK.md) | Switching views loses nothing |
| CNV-02 | Sticky notes, shapes, arrows, text | Open | P1 | | Merge tests |
| INK-01 | Stroke model and outline | Open | P1 | Port the MIT stroke algorithm | Golden outlines |
| INK-02 | Pen input, pressure, palm rejection | Blocked | P1 | Waits on `SPK-03` | Owner judges on a device |
| INK-03 | Partial erase | Open | P2 | | Merge tests |
| INK-04 | PDF as an annotatable background | Open | P2 | | PDF is embedded, never linked |
| RND-01 | Headless layout and PDF and PNG | Open | P1 | [42](42-EDITOR-AND-RENDERING.md) | Golden output per script |

## Phase 4 — Knowledge

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| SRCH-01 | Tokenisers per script | Open | P0 | [60](60-SEARCH-AND-EMBEDDINGS.md) | Per-language test sets |
| SRCH-02 | Inverted index | Blocked | P0 | Waits on `SPK-05` | QR-2 |
| EMB-01 | Vector index and rebuild | Blocked | P1 | Waits on `SPK-06` | Rebuild gives same results |
| EMB-02 | Search by meaning, related notes | Open | P1 | | Beats baseline on a labelled set |
| GRF-01 | Derived graph | Open | P1 | [61](61-KNOWLEDGE-GRAPH.md) | Golden graphs |
| GRF-02 | Graph view and local graph | Open | P1 | | Interaction tests |
| BLK-01 | Embedded pages | Open | P2 | | Cycles refused |
| FIL-01 | Attachments: chunks, resume, content hash | Open | P0 | [23](23-STORAGE-FORMAT.md) | Resume after injected failure |

## Phase 5 — Assistant and agents

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| TOOL-01 | `-tools` closed tool set | Open | P0 | [63](63-AGENT-TOOLS-AND-MCP.md) | Every tool has a schema and a refusal test |
| TOOL-02 | Approval flow, one undo step per write | Open | P0 | | Declined write changes nothing |
| AST-01 | Assistant panel | Open | P1 | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) | Interaction tests |
| AST-02 | Provider seam, tool-calling adapters | Open | P0 | | Contract tests with a fake provider |
| AST-03 | Web providers and key handling | Open | P0 | Keys are per browser, never synced | Key absent from export, logs, sync, by test |
| AST-04 | Relay through the server for providers that refuse browsers | Open | P2 | Needs Phase 8 | Optional |
| AST-05 | Prompt-injection suite | Open | P0 | [07](07-SECURITY-THREAT-MODEL.md) | No unapproved write in any fixture |
| MCP-01 | `casual-note-mcp` | Open | P1 | Pattern from `casual-calc-mcp` | Client conformance, `--root` escapes refused |
| MCP-02 | CLI over the same tools | Open | P2 | | Same results as MCP |

## Phase 6 — First release

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| DSK-01 | Tauri shell | Open | P0 | [44](44-DESKTOP-SHELL.md) | Runs on three platforms in CI |
| DSK-02 | Model catalogue, download, hash, licence | Open | P0 | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) | Resume, hash failure refused |
| DSK-03 | Local model runtime | Blocked | P0 | Waits on `SPK-07` | Other-origin request refused |
| DSK-04 | Signing and updates | Open | P0 | | Signed installers |
| IMP-01 | Markdown and HTML import | Open | P0 | [37](37-IMPORT-FORMATS.md) | Report names anything lost |
| IMP-02 | Evernote import | Open | P1 | | Fixtures, written from public docs only |
| REL-01 | First public release checklist | Blocked | P0 | Waits on `DEC-01`, `DEC-08` | All six promises green |

## Phase 7 — Sync

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| SYN-01 | Device registry and segment exchange | Open | P0 | [56](56-SYNC-DESIGN.md) | Fault-injection suite |
| SYN-02 | Storage adapters: folder, WebDAV, S3 | Open | P1 | | Same suite on each |
| SYN-03 | Sync state panel | Open | P0 | PR-2 | Each fault appears |
| SYN-04 | Merge review interface | Open | P1 | | Labelled histories |
| SYN-05 | Trimming rules and restored copies | Open | P0 | | Device past any limit loses nothing |
| CRY-01 | `-crypto`: keys, sealed segments, recovery | Open | P0 | | Tamper tests |
| CRY-02 | Outside cryptography review | Open | P0 | Before any public claim | Findings dispositioned |

## Phase 8 — Server

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| SRV-01 | Server with sync and blobs | Open | P1 | [57](57-SERVER-DESIGN.md) | Two browsers in CI |
| SRV-02 | Real-time co-editing and presence | Open | P1 | | Convergence under load |
| SRV-03 | Sharing and per-operation permissions | Open | P0 | | Refused op never reaches another client |
| SRV-04 | End-to-end encrypted and managed modes | Open | P1 | | Server cannot read an encrypted workspace, by test |
| SRV-05 | Comments | Open | P2 | | Interaction tests |
| SRV-06 | Compose and Helm deployment | Open | P1 | | Fresh install from the docs |

## Phases 9 and 10

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| MOB-01 | Tablet-first interface | Open | P2 | | Device matrix |
| MOB-02 | Mobile shells prototype | Open | P2 | Tauri mobile, unmeasured | Prototype reports |
| SDK-02 | Published SDK and examples | Open | P2 | [55](55-SDK-AND-EMBEDDING.md) | Example embeds in CI |
| IMP-03 | Further importers | Open | P3 | Written from public docs, by demand | Fixtures |
| ENT-01 | Enterprise delivery | Open | P2 | [10](10-ENTERPRISE-REQUIREMENTS.md) | SSO and audit gates |
| OCR-01 | Printed-text OCR for images | Open | P3 | Optional download. Licence review first | Offered only after review |
| HWR-01 | Handwriting recognition | Open | P3 | No good permissive option found | Re-survey before starting |
| PKG-07 | Encrypted packages | Open | P2 | [36](36-EXPORT-FORMAT.md) open question | Design note first |

## Found during design

Rows found by the low-level design authors are added here.

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| FND-01 | Record the database document class in ADR-006 | Done | P1 | Databases are shared between pages. ADR-006 and 19 updated | Documents agree |
| FND-02 | Correct the keystroke order in 02 | Done | P0 | Paint first, then group commit, then "saved". Same finding as `FND-40` | Documents agree |
| FND-03 | Remove `-store` to `-export` dependency | Done | P1 | `MirrorRender` value from above. 19 and 23 updated | `check-crate-deps.py` once built |
| FND-04 | Package gaps: attachments, sibling order, workspace name, trashed pages | Open | P1 | Stopgap `x-` fields exist in 22. Needs a 36 revision before 1.0 | Schema and reader updated |
| FND-05 | Block nesting | Open | P1 | The schema is flat, while FR-02 lists can nest | Decision in 22 and 36 |
| FND-06 | Attachment export limit | Open | P1 | 36 allows 2 GiB per entry, while a notebook blob may reach 1 TiB | Decision in 36 |
| FND-07 | Splitting exports under browser quota | Open | P2 | A browser may be unable to hold one large package | Design note |
| FND-20 | Loro import status and peer-id rule | Blocked | P0 | Waits on `SPK-01`. Sync depends on both | Confirmed against Loro |
| FND-21 | Which storage each host can reach | Open | P1 | Browsers cannot reach every storage, because of CORS and file access | Table in 56, tested in `SPK-04` |
| FND-22 | Home for native storage adapters | Open | P2 | They need network and files, so not `crates/` | Decision in 19 |
| FND-23 | `classify_update` for remote updates | Open | P1 | Remote updates are not ops, but permissions need to classify them | Design in 24 and 57 |
| FND-24 | IPC typing cost and webview input | Open | P1 | QR-1 is unknown through Tauri's bridge | `SPK-02` on desktop |
| FND-25 | Signing identities | Open | P1 | `DSK-04` assumes Apple and Windows certificates exist | Owner obtains them |
| FND-26 | Phase 8 sign-in | Open | P1 | ENT-R01 forbids a password store, so 57 proposes OIDC plus a first-run invite | Owner review |
| FND-27 | Server load targets | Open | P2 | 30 has none | Rows added to 30 |
| FND-28 | Comment model | Open | P2 | `SRV-05` needs a model 22 lacks | Added to 22 |
| FND-40 | Keystroke order, from the editor review | Done | P0 | Same as `FND-02` | Documents agree |
| FND-41 | Define the QR-1 metric | Done | P0 | A 60 Hz refresh alone uses 16 ms. Now main-thread work, with first-frame share reported | 01, 30, 42 agree |
| FND-42 | Licence audit: PDF.js, PDF writer, skrifa, fonts, messageformat | Open | P1 | Not covered in 12. Phosphor's MIT licence is confirmed | Added to 12 |
| FND-43 | Package gaps from the editor: links, image and file blocks, PDF items, direction, tilt, time | Open | P1 | Read with `FND-04` | Schema and reader updated |
| FND-44 | Concurrent erase projection | Open | P1 | Erased ink can return when another device replaces the same stroke. 43 and 24 must agree | Merge test |
| FND-45 | Contrast of `--faint` and small targets | Open | P0 | By calculation, `--faint` gives 2.84 to 4.03:1, and small targets fail QR-5. The prototype has this too | Tokens changed, contrast check in CI |
| FND-46 | ADRs for lazy wasm and PDF worker, reword ADR-007 | Done | P1 | ADR-028, ADR-029, ADR-007 | 08 updated |
| FND-47 | Add `check-design` and `check-commands` gates | Open | P2 | To enforce 49 and 47 | Seeded violations fail |
| FND-60 | Homes for the assistant loop, embedder and CLI | Open | P1 | 19 names none | Decision in 19 |
| FND-61 | `-wasm` depending on `-tools` | Open | P1 | The web assistant needs it | Decision in 19 |
| FND-62 | Scope QR-3 for vectors | Done | P1 | QR-3 now promises keyword search only | 01 updated |
| FND-63 | PERF-08 size | Open | P2 | 100,000 blocks is tested, 100,000 pages may be far more | Measured in `SPK-06` |
| FND-64 | Model gaps for the assistant | Open | P2 | Listed in 62 | Added to 22 |
| FND-65 | Verify browser access per provider | Open | P1 | Only Anthropic and OpenRouter have been looked at, through secondary sources | Tested per provider |
| FND-66 | Who hosts model files, against `DEC-03` | Open | P1 | Downloads come from third parties unless we mirror them | Owner decision |
| FND-67 | CSP for a person-chosen endpoint | Open | P0 | A static app cannot send a per-person header. 62 has a proposal | Test with a seeded endpoint |
| FND-68 | Simplified and Traditional Chinese search | Open | P1 | Not handled yet | Search test set |
| FND-69 | MCP default and writer lock | Open | P1 | 63 commits unless `--read-only`. Owner may prefer read-only as the default | Owner decision |
| FND-70 | Segmenter licence | Open | P1 | ICU4X or similar not yet audited | Added to 12 |
| FND-71 | Atomic import | Open | P1 | One transaction across many documents needs a design | Crash test during import |
| FND-72 | Agents in live documents | Open | P3 | Later, as OpenCalc's AGENT-03 | Design note |
| FND-73 | `-secrets` missing from 19 | Done | P2 | Added as `server/casual-note-secrets` | 19 updated |
| FND-74 | Editor inside a shadow root | Open | P2 | Selection behaviour varies, and the SDK wants isolation | Tested in `SPK-02` |
| FND-75 | `CN-SDK` code prefix | Open | P3 | 55 needs it when the SDK has errors | Registry entry |
