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
| DEC-12 | `git init` and the GitHub repo | Done | Owner, 2026-11-08: create and push. `CasualOffice/casualnote`, private until the owner chooses public | CI and the site need a remote | Done |

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
| DOC-008 | Independent review of the design set | Done | P1 | Two fresh readers found 30 defects, 7 of them P0. Their quotes were checked against the text, and all were real. Fixes are `REV-*` below. A second pass over the fixes is `DOC-009` | Every finding dispositioned |
| DOC-009 | Second review, of the fixes themselves | Open | P1 | The corrected designs have not been reviewed afresh | Findings dispositioned |
| PROTO-01 | Clickable prototype | Done | P1 | [Prototype](prototypes/casualnote-prototype.html). Blocks, databases, canvas, graph, assistant, settings, six interface languages. Runs in a browser with no Rust | Scripted checks in Chrome, 2026-11-07. Hand dragging not tested |
| PROTO-02 | Prototype assistant against the live model | Open | P2 | Tested only with a stand-in model | Owner tries it |
| PROTO-04 | Prototype export still writes `canvas.ink` | Open | P3 | The package format moved ink to `ink/`. The sample fixture was converted. The throwaway prototype was not | Prototype retired or updated |
| PROTO-03 | Prototype translations reviewed | Open | P2 | Drafts by the design author, not native-reviewed | Native review |
| PKG-01 | Package format, version 1 draft | Done | P0 | [36](36-EXPORT-FORMAT.md), `schemas/cnotepack-1.schema.json` | Sample package passes schema and reader |
| PKG-02 | Standalone reader | Done | P0 | `tools/cnotepack.py`, standard library only | `tools/test_cnotepack.py`: 24 tests. Path and hash checks removed and tests seen red. Byte-flip sweep first failed on raw crashes, then fixed |
| PKG-03 | Format review by someone outside the project | Open | P1 | Not done | Findings dispositioned |

## Phase 0 — Foundation

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| GOV-01 | `git init`, hooks, commit rules | Done | P0 | `.githooks/pre-commit` runs `check-docs.py` and the reader tests. `.githooks/commit-msg` requires Conventional Commits. Enable with `git config core.hooksPath .githooks` | A broken doc and a bad message were each seen refused |
| GOV-02 | Cargo workspace, toolchain, lints | Done | P0 | Cargo workspace, pinned toolchain, lints. Builds to native and wasm32 | `cargo check` on both targets |
| GOV-03 | CI on three platforms | In progress | P0 | `.github/workflows/ci.yml`. Seeded failures per job not yet proven | Seeded violation fails each job |
| GOV-04 | Crate dependency gate | Open | P0 | `check-crate-deps.py`: layer rules, only `-merge` names `loro` | Seeded edge fails |
| GOV-05 | Licence gate | In progress | P0 | `deny.toml`. Named MPL exceptions: im, bitmaps, sized-chunks, through Loro | Seeded GPL crate fails |
| WEB-07 | Marketing site and hosted demo | In progress | P1 | `site/`, `.github/workflows/pages.yml`, real app at `/app/`. Pages needs a public repository on this plan | Page live |
| GOV-06 | Host-seam gate | Open | P1 | Copied from OpenCalc's `check-host-seams.py` | Seeded `cfg` fails |
| GOV-07 | Fuzz workspace | Open | P0 | Separate workspace, pinned nightly | One target runs |
| SPK-01 | Merge: does Loro survive years of notes? | Open | P0 | 5-year, 3-device simulated history, including 90 days offline, and real notes in the launch languages. Decides ADR-005, ADR-006, ADR-010 | Pass and fail in [24](24-OPERATIONS-AND-MERGE.md) |
| SPK-02 | Editor: can DOM editing be op-driven and fast? | Open | P0 | 5,000-block page, Chrome, Safari, Firefox, input methods, right-to-left, screen reader. Includes desktop webviews and the Tauri bridge: WKWebView locally, WebKitGTK and WebView2 through CI where possible. Decides ADR-007, ADR-025, ADR-028 | Pass and fail in [42](42-EDITOR-AND-RENDERING.md) |

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
| MOD-01 | `-model` types and invariants | In progress | P0 | Pages, blocks, marks, links, embeds, databases, canvas, ink. Attachments, comments and nesting remain | Property tests |
| MOD-02 | Schema repair as a pure projection | Open | P0 | Repairs never write back | Generated invalid states repair the same way on every replica |
| OPS-01 | Closed op set | In progress | P0 | Closed op set covering pages, blocks, text, links, canvas, ink and databases, with inverses, tested. Text ops use UTF-16 offsets, not anchors yet (ADR-031 remains) | Every op has a validation test |
| OPS-02 | Undo with inverses over merged state | In progress | P0 | Undo replays inverses. Concurrent-undo cases not tested | Generated concurrent undo tests |
| MRG-01 | `-merge` adapter over Loro | In progress | P0 | `casual-note-merge` over Loro 1.17.1, one document per page plus a tree. Three-device convergence tested. `SPK-01` still not run | Only crate naming `loro` |
| MRG-02 | Guarded CRDT import | In progress | P0 | Bounded container, byte-flip test. Size limits and panic isolation remain | Hostile updates refused, none panic |
| MRG-03 | Overlap detection for merge review | Open | P1 | [24](24-OPERATIONS-AND-MERGE.md) | Labelled histories |
| STO-01 | Durable log with fsync before ack | In progress | P0 | **Stopgap:** whole snapshots in IndexedDB, with "saved" shown only after commit. The log in docs/23 remains | Crash suite |
| STO-02 | Snapshots and compaction | Open | P0 | | Rebuild equals original |
| STO-03 | Host I/O seam, native and OPFS | Open | P0 | | Same bytes on both |
| STO-04 | Readable mirror | Open | P1 | One-way, regenerated | Deleting it loses nothing |
| STO-05 | Version history and restore | Open | P1 | | Restore round trip |
| PKG-04 | `-package` copied from OpenCalc | Open | P0 | Provenance recorded | Hostile ZIP fixtures |
| PKG-05 | Engine package writer | Done | P0 | `crates/casual-note-package`. `tools/cnotepack.py` accepts its output, and export, import, export gives identical bytes. Whole-package native and wasm parity not tested | `tools/cnotepack.py` and schema pass. Bytes match on native and wasm |
| PKG-06 | Engine package reader and import report | Done | P0 | Import with report and copies on a clash. Assets and extensions are reported, not imported | Fixture per code, fuzz target |
| SDK-01 | `-sdk` facade, first surface | Open | P1 | [55](55-SDK-AND-EMBEDDING.md) | Used by tests |

## Phase 2 — Web editor

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| WEB-01 | `-wasm` bridge | Done | P0 | `casual-note-wasm`, 2.46 MB wasm, 823 KB gzipped, unoptimised | Same results as native |
| WEB-02 | App shell: sidebar, tabs, panels | In progress | P0 | Sidebar, tabs, outline, settings. Real browser test, no accessibility audit yet | Keyboard and screen reader pass |
| WEB-03 | Block editor on the real DOM | In progress | P0 | Op-driven DOM editing, including composition handling. Not measured against QR-1, and real input methods not tested (`SPK-02`) | QR-1 |
| WEB-04 | Slash menu, Markdown shortcuts, formatting | In progress | P1 | Slash menu, Markdown shortcuts, marks work. Links remain | Interaction tests |
| WEB-05 | Page links, backlinks, outline, tags | Done | P1 | `features/links.js`: `[[` picker, backlinks, tags. Outline in the app | Interaction tests |
| WEB-06 | Service worker, offline start | Open | P1 | | Loads with the network off |
| PAG-01 | Reorder pages, and delete forever | Open | P2 | No op yet. Nesting works | Merge test |
| AST-06 | Try the assistant against real providers | Open | P0 | Only a fake provider so far, so real streaming and browser access are unverified, read with `FND-65` | Owner or test key |
| FIL-01 | Attachments: chunks, resume, content hash. Moved from Phase 4, since paste and canvas need it | Open | P0 | [23](23-STORAGE-FORMAT.md) | Resume after injected failure |
| DB-01 | Databases: model and operations | Done | P0 | `crates/casual-note-merge`, `-ops`. Cells from two devices both survive. Removing a column hides it | Merge tests |
| DB-02 | Table and board views | Done | P1 | `webapp/features/database.js`: table and board, with drag, keyboard and rename. Real browser tests | Interaction tests |
| I18N-01 | Message catalogue with ICU MessageFormat | In progress | P0 | Six interface languages from a simple table, not ICU MessageFormat yet. Drafts, unreviewed | Pseudo-locale finds no raw string |
| I18N-02 | Logical CSS, right-to-left | In progress | P0 | Logical CSS, Arabic mirrors. No gate yet | CSS gate refuses physical properties. Arabic run passes |
| I18N-03 | Input-method support | Open | P0 | | Composition tests in Chinese, Japanese, Korean |
| I18N-04 | Fonts by script | Open | P1 | | Every launch script renders |
| I18N-05 | Translation workflow and native review | Open | P1 | | A language is called supported only after review |
| A11Y-01 | Accessibility baseline | In progress | P0 | Contrast meets AA by calculation, with tree and menu roles. No real screen reader test or audit | Audit passes |

## Phase 3 — Canvas and ink

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| CNV-01 | Canvas view over a page | Done | P0 | `features/canvas.js`: page and canvas switch, note card edits real blocks, pan and zoom | Switching views loses nothing |
| CNV-02 | Sticky notes, shapes, arrows, text | Done | P1 | Sticky notes, shapes, arrows, text. Real pointer event tests | Merge tests |
| INK-01 | Stroke model and outline | Done | P1 | Stroke outline ported from the prototype, samples stored compactly | Golden outlines |
| INK-02 | Pen input, pressure, palm rejection | In progress | P1 | Pressure, palm rule written. Not judged on any real pen device (`DEC-05`) | Owner judges on a device |
| INK-03 | Partial erase | Done | P2 | A partial erase replaces a stroke with its pieces, one undo step | Merge tests |
| INK-04 | PDF as an annotatable background | Open | P2 | | PDF is embedded, never linked |
| RND-01 | Headless layout and PDF and PNG | Open | P1 | [42](42-EDITOR-AND-RENDERING.md) | Golden output per script |

## Phase 4 — Knowledge

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| SRCH-01 | Tokenisers per script | Done | P0 | `crates/casual-note-search`: tests in seven languages | Per-language test sets |
| SRCH-02 | Inverted index | In progress | P0 | BM25 index behind Ctrl K, incremental. `SPK-05` scale not measured | QR-2 |
| EMB-01 | Vector index and rebuild | Blocked | P1 | Waits on `SPK-06` | Rebuild gives same results |
| EMB-02 | Search by meaning, related notes | Open | P1 | | Beats baseline on a labelled set |
| GRF-01 | Derived graph | Done | P1 | `crates/casual-note-graph`. Similar pages use word overlap, not embeddings | Golden graphs |
| GRF-02 | Graph view and local graph | Done | P1 | `features/knowledge.js`: graph view, Related, local graph | Interaction tests |
| BLK-01 | Embedded pages | Done | P2 | `features/links.js`: live read-only embeds, with self and cycle refusal | Cycles refused |

## Phase 5 — Assistant and agents

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| TOOL-01 | `-tools` closed tool set | Done | P0 | `crates/casual-note-tools`: nine tools with schemas and refusals | Every tool has a schema and a refusal test |
| TOOL-02 | Approval flow, one undo step per write | Done | P0 | Approval card from real arguments; declined writes change nothing; one undo step | Declined write changes nothing |
| AST-01 | Assistant panel | Done | P1 | `features/assistant.js`, streaming, Stop, 8-round limit | Interaction tests |
| AST-02 | Provider seam, tool-calling adapters | Done | P0 | Anthropic and OpenAI-compatible adapters, tested with a fake provider. **Not tested against any real provider** | Contract tests with a fake provider |
| AST-03 | Web providers and key handling | In progress | P0 | Keys in memory by default, explicit remember option. Stored-script test (`FND-67`) remains | Key absent from export, logs, sync, by test |
| AST-04 | Relay through the server for providers that refuse browsers | Open | P2 | Needs Phase 8 | Optional |
| AST-05 | Prompt-injection suite | In progress | P0 | An injection fixture causes no unapproved write. A full suite remains | No unapproved write in any fixture |
| MCP-01 | `casual-note-mcp` | Done | P1 | `server/casual-note-mcp` over real stdio, root escape and read-only refused. No writer lock (`FND-69`) | Client conformance, `--root` escapes refused |
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
| REL-01 | First public release checklist | Blocked | P0 | Waits on `DEC-08` | Every promise that applies to a local-only release is green |

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

## Review findings

From `DOC-008`. **Done means the design was corrected and the documents agree. No code proves any of this yet,** so each still needs its test when built.

| ID | Title | St | Sev | Mechanism | Gate |
| --- | --- | --- | --- | --- | --- |
| REV-01 | Stale base could discard typed text | Done | P0 | Ops address by anchors, ADR-031. 24, 42 | Test when built |
| REV-02 | Phase 6 and 7 formed a cycle over the promises | Done | P0 | First release scoped to local promises. 01, 06, 15 | Test when built |
| REV-03 | Desktop IPC bypassed the CSP | Done | P0 | Native confirmation, keys bound to origin, ADR-033, SEC-W06. 44, 62 | Test when built |
| REV-04 | A crash during import quarantined good segments | Done | P0 | Attempt count; own segments never auto-quarantined. 23, 24, 56 | Test when built |
| REV-05 | Forks assumed the CRDT merges same-peer branches | Done | P0 | Fresh peer per open, ADR-030. 23, 56 | Test when built |
| REV-06 | Moving a notebook stranded saved edits | Done | P0 | Old id sealed and republished. 23 | Test when built |
| REV-07 | Purge destroyed unseen offline edits | Done | P0 | Purge blocked by concurrent edits. 24, 23 | Test when built |
| REV-08 | A separate process contained nothing | Done | P1 | OS sandbox, nonce on the port. SEC-L04, SEC-L06. 44, 62 | Test when built |
| REV-09 | Partial-download hash could be bypassed | Done | P1 | Full hash before install and start. 44, 62 | Test when built |
| REV-10 | Web CSP read attacker-writable input | Done | P1 | Not treated as a control. Session-only keys. SEC-W07. 62 | Test when built |
| REV-11 | Composition broke on Korean and selections | Done | P1 | Snapshot at compositionend; to confirm in `SPK-02`. 42 | Test when built |
| REV-12 | Phase 3 depended on Phase 4; paste needed `FIL-01` | Done | P1 | `FIL-01` moved to Phase 2, `SPK-03` uses a harness. 06, 14 | Test when built |
| REV-13 | Desktop typing path tested too late | Done | P1 | Phase 0 exit. 06, 42 | Test when built |
| REV-14 | Encrypted notebook opt-in could not be tested and had a bypass | Done | P1 | Enforced in `-tools`, Phase 7 gate. 62, 63, 07 | Test when built |
| REV-15 | Approved write could copy private text to a shared page | Done | P1 | Audience and sources on the card, SEC-A08. **Reduced, not removed.** 62, 63 | Test when built |
| REV-16 | Offline cell edits vanished with a removed column | Done | P1 | Flagged, exported hidden, RestoreColumn. 22, 24 | Test when built |
| REV-17 | Damaged segments stranded later edits | Done | P1 | Good-prefix copy sealed, peer rotated. 23, 56 | Test when built |
| REV-18 | Quarantine had no exit | Done | P1 | Per-document outcome, acks report rejects, restored copy. 56 | Test when built |
| REV-19 | Two incompatible segment models | Done | P1 | One mapping specified. 23, 56 | Test when built |
| REV-20 | State hash could not be computed as defined | Done | P1 | Per-document hash in `-model`. 24, 23 | Test when built |
| REV-21 | Key rotation did not stop forged writes | Done | P1 | Per-device cutoff, SEC-S08. 56 | Test when built |
| REV-22 | E2EE document ids changed on rotation | Done | P1 | Stable key. 57 | Test when built |
| REV-23 | Merge bookkeeping triggered permission refusal | Done | P1 | Added to the matrix. 57 | Test when built |
| REV-24 | Server ids were global | Done | P1 | Keyed by workspace. 57 | Test when built |
| REV-25 | Valid notes could not be exported or re-read | Done | P1 | Ink moved to `ink/`, entry limit raised, ADR-032. 36, schema, reader, prototype, fixture | Test when built |
| REV-26 | MCP read exfiltration had no threat row | Done | P2 | SEC-M04. **Not fully preventable.** 63, 07 | Test when built |
| REV-27 | CSP blocked required features, and policies differed | Done | P2 | One table in 62. 42, 57 | Test when built |
| REV-28 | Web fonts could fetch because of note content | Done | P2 | SEC-W08. 50 | Test when built |
| REV-29 | Gates were not wired | Done | P2 | `check-secrets.py` in `ai-safety`, Phase 5 exit, SEC-F10 test. 15, 06, 07 | Test when built |
| REV-30 | Resurrected blocks could not be operated on, and smaller merge items | Done | P2 | Implicit RestoreBlock, others. 22, 24, 57 | Test when built |

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
| FND-20 | Loro import status and peer-id behaviour | Blocked | P0 | Waits on `SPK-01`. Review found that same-peer branches may silently lose edits, so ADR-030 stops relying on this. Must be confirmed anyway | Confirmed against Loro |
| FND-21 | Which storage each host can reach | Open | P1 | Browsers cannot reach every storage, because of CORS and file access | Table in 56, tested in `SPK-04` |
| FND-22 | Home for native storage adapters | Open | P2 | They need network and files, so not `crates/` | Decision in 19 |
| FND-23 | `classify_update` for remote updates | Open | P1 | Remote updates are not ops, but permissions need to classify them | Design in 24 and 57 |
| FND-24 | IPC typing cost and webview input | Open | P0 | QR-1 is unknown through Tauri's bridge. Now a Phase 0 exit criterion | `SPK-02` on desktop |
| FND-25 | Signing identities | Open | P1 | `DSK-04` assumes Apple and Windows certificates exist | Owner obtains them |
| FND-26 | Phase 8 sign-in | Open | P1 | ENT-R01 forbids a password store, so 57 proposes OIDC plus a first-run invite | Owner review |
| FND-27 | Server load targets | Open | P2 | 30 has none | Rows added to 30 |
| FND-28 | Comment model | Open | P2 | `SRV-05` needs a model 22 lacks | Added to 22 |
| FND-40 | Keystroke order, from the editor review | Done | P0 | Same as `FND-02` | Documents agree |
| FND-41 | Define the QR-1 metric | Done | P0 | A 60 Hz refresh alone uses 16 ms. Now main-thread work, with first-frame share reported | 01, 30, 42 agree |
| FND-42 | Licence audit: PDF.js, PDF writer, skrifa, fonts, messageformat | Open | P1 | Not covered in 12. Phosphor's MIT licence is confirmed | Added to 12 |
| FND-43 | Package gaps from the editor: links, image and file blocks, PDF items, direction, tilt, time | Open | P1 | Read with `FND-04` | Schema and reader updated |
| FND-44 | Concurrent erase projection | Open | P1 | Erased ink can return when another device replaces the same stroke. 43 and 24 must agree | Merge test |
| FND-45 | Contrast of `--faint` and small targets | Done | P0 | `--faint` 2.60 to 4.51 light, 3.20 to 4.60 dark. A test fails if tokens drop below AA | Tokens changed, contrast check in CI |
| FND-46 | ADRs for lazy wasm and PDF worker, reword ADR-007 | Done | P1 | ADR-028, ADR-029, ADR-007 | 08 updated |
| FND-47 | Add `check-design` and `check-commands` gates | Open | P2 | To enforce 49 and 47 | Seeded violations fail |
| FND-60 | Homes for the assistant loop, embedder and CLI | Open | P1 | 19 names none | Decision in 19 |
| FND-61 | `-wasm` depending on `-tools` | Open | P1 | The web assistant needs it | Decision in 19 |
| FND-62 | Scope QR-3 for vectors | Done | P1 | QR-3 now promises keyword search only | 01 updated |
| FND-63 | PERF-08 size | Open | P2 | 100,000 blocks is tested, 100,000 pages may be far more | Measured in `SPK-06` |
| FND-64 | Model gaps for the assistant | Open | P2 | Listed in 62 | Added to 22 |
| FND-65 | Verify browser access per provider | Open | P1 | Only Anthropic and OpenRouter have been looked at, through secondary sources | Tested per provider |
| FND-66 | Who hosts model files, against `DEC-03` | Open | P1 | Downloads come from third parties unless we mirror them | Owner decision |
| FND-67 | Web key protection | Open | P0 | Phase 5 exit criterion. A CSP read from `localStorage` is not a defence, as the review showed. Session-only keys by default; separate-origin connector to study | Stored-script test |
| FND-68 | Simplified and Traditional Chinese search | Open | P1 | Not handled yet | Search test set |
| FND-69 | MCP default and writer lock | Open | P1 | 63 commits unless `--read-only`. Owner may prefer read-only as the default | Owner decision |
| FND-70 | Segmenter licence | Open | P1 | ICU4X or similar not yet audited | Added to 12 |
| FND-71 | Atomic import | Open | P1 | One transaction across many documents needs a design | Crash test during import |
| FND-72 | Agents in live documents | Open | P3 | Later, as OpenCalc's AGENT-03 | Design note |
| FND-73 | `-secrets` missing from 19 | Done | P2 | Added as `server/casual-note-secrets` | 19 updated |
| FND-74 | Editor inside a shadow root | Open | P2 | Selection behaviour varies, and the SDK wants isolation | Tested in `SPK-02` |
| FND-75 | `CN-SDK` code prefix | Open | P3 | 55 needs it when the SDK has errors | Registry entry |
