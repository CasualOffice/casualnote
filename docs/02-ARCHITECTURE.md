# 02 — Architecture (high-level design)

**Status: proposed, nothing here is built.** Decisions are in [08](08-ADR-REGISTER.md), and those marked Proposed can be overturned by a prototype (`SPK-*`). Crate-level detail is in [19](19-WORKSPACE-SCAFFOLD-DESIGN.md).

CasualNote is a **headless, deterministic Rust engine with host adapters**, the shape OpenCalc uses. The engine owns the notes, the rules and the derived indexes. The host owns the window, the disk, the network, the AI provider and the policy.

## System context

```
                 Person                     Any MCP client
                   │                               │
     ┌─────────────┴────────────┐         casual-note-mcp (stdio)
     │                          │                  │
  Web app                  Desktop app             │
  (browser)                (Tauri 2)               │
     │                          │                  │
  casual-note-wasm        casual-note-sdk ◀────────┘
     └────────────┬─────────────┘
              Engine crates
     ┌────────────┼────────────────┬─────────────────┐
     ▼            ▼                ▼                 ▼
 Browser or    Any storage     casual-note-     AI provider, or
 local disk    folder, WebDAV, server           local model
               S3 (encrypted)  (self-hosted)    (llama.cpp)
```

All the outward arrows are optional. With none of them, the product is complete and nothing leaves the device.

## The layers

```
 Hosts      webapp · desktop · server · mcp · cli
 ───────────────────────────────────────────────────────
 Bridges    -wasm · -sdk · -tools
 ───────────────────────────────────────────────────────
 Features   -search · -embed · -graph · -sync · -import · -export · -layout · -render
 ───────────────────────────────────────────────────────
 Core       -ops ── -store ── -crypto ── -package ── -ink ── -text
 ───────────────────────────────────────────────────────
 Merge      -merge    the only crate that names the CRDT library
 ───────────────────────────────────────────────────────
 Bedrock    -model
```

Layers depend downward only, and nothing under `crates/` depends on `server/`. Enforced by `tools/check-crate-deps.py` (`GOV-04`).

## Principles

1. **The engine is authoritative.** The DOM and canvas project engine state and never hold the truth.
2. **Every edit is an operation.** `-ops` is the only write path, and validates, records an inverse, and compiles down to CRDT writes (ADR-005).
3. **The CRDT merges; nothing else does.** There is no second convergence mechanism, such as OT beside it.
4. **Derived data is never authoritative.** Search indexes, vectors, the graph and the readable mirror can all be deleted and rebuilt (ADR-010, ADR-014, ADR-015).
5. **The host owns policy.** Clock, I/O, network, secrets and AI come in as values or small seams, one per capability. No `cfg(target_*)` in `crates/`.
6. **Hostile by default.** Every file, remote update, note and AI reply is untrusted.
7. **Languages are not a layer.** They are a rule on every layer (ADR-018).
8. **Public surfaces are narrower than internals,** and versioned independently.

## Hosts

| Host | Bridge | Storage | AI | Notes |
| --- | --- | --- | --- | --- |
| **Web app** | `-wasm` | OPFS and IndexedDB | The user's own provider ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)) | Plain ES modules, no framework or bundler (ADR-025) |
| **Desktop app** | `-sdk` from Tauri commands | The local disk, including the live readable folder | A downloaded local model, or a provider | [44](44-DESKTOP-SHELL.md). Engine runs natively |
| **Server** | `-sdk` | Postgres and object storage | None of its own | [57](57-SERVER-DESIGN.md). A host, not a layer |
| **MCP and CLI** | `-tools` | Under `--root` only | The client brings it | [63](63-AGENT-TOOLS-AND-MCP.md) |

## Data flows

**A keystroke.**

```
beforeinput → editor maps it to an op → -ops validates, records inverse
   → -merge writes CRDT change → projection returns changed blocks → DOM patched
   → -store appends to the log, group commit, fsync → ticket completes → "saved"
   → (later, off the typing path) mirror, search index, vectors, graph refresh
```

The screen updates at once, and "saved" appears only after the durable write (PR-1). Keystrokes share flushes through group commit ([23](23-STORAGE-FORMAT.md)), so a slow disk delays "saved" rather than the keyboard (PR-4).

**Sync, through any storage.**

```
local segment → -crypto seals → host uploads under this device's own name
other devices' segments → host downloads → -crypto opens → guarded import → -merge
   → schema projection → overlap detector → review marker if two devices touched one block unseen
```

**The assistant.**

```
person asks → provider (their API, or local llama-server)
   → tool call → -tools read: runs and returns data
              → -tools write: approval card → person allows → one op batch, one undo step
```

**Export.** The model is projected to a CasualNote Package with no CRDT state ([36](36-EXPORT-FORMAT.md)). Importing is the reverse, as one transaction.

## Trust boundaries

| Boundary | Untrusted side | Control |
| --- | --- | --- |
| File input | Packages, imports, attachments | Bounded parsers, fuzzing, refusal codes ([20](20-ERROR-CODE-REGISTRY.md), [21](21-PARSER-LIMITS.md)) |
| Sync input | Segments and CRDT updates from storage or the server | Authenticated decryption, guarded import, verify on load |
| Rendered content | Note text, imported HTML | No HTML from documents, strict CSP, no automatic fetch |
| AI output | Model replies and tool arguments | Tool arguments validated, writes approved. Note text is data |
| AI input | Notes sent to a provider | Off by default, stated in the panel, local option |
| Local model server | Other sites and apps on the same computer | Loopback only, random key, restricted origins |
| Agent access | MCP clients | `--root`, read-only flag, no network |

Threats and tests are in [07](07-SECURITY-THREAT-MODEL.md).

## Independent versions

These change separately so one does not falsely imply another: **SDK API**, **operation schema**, **live storage format** ([23](23-STORAGE-FORMAT.md)), **export format** ([36](36-EXPORT-FORMAT.md)), **sync protocol** ([56](56-SYNC-DESIGN.md)), **tool set** ([63](63-AGENT-TOOLS-AND-MCP.md)).

## Relationship to the sister repos

**Carried from OpenCalc:** the layer shape, operations with inverses, host seams, bounded parsers, deterministic output, pinned toolchain, plain web stack, Tauri shell, MCP pattern, document and gate discipline. **Copied with provenance when the time comes:** `-package`, `-text`, `server/casual-note-secrets`, repo gates and CI, after confirming they carry no spreadsheet types.

**Where this differs, and why:**

| OpenCalc | CasualNote | Why |
| --- | --- | --- |
| Server-ordered OT | CRDT per page (ADR-005) | Long offline is routine here. Its docs/56 names that condition as what would supersede its choice |
| `.xlsx` is the file format | Own format, with a stable export (ADR-010, ADR-017) | No external standard to be faithful to |
| Display list for everything | Real DOM for all text, including text on canvas items. Canvas drawing for ink (ADR-007) | Input methods, spell check, accessibility and right-to-left matter more than identical line breaks |
| Server does not store files | Server stores notebooks in its deployment | Teams need shared storage |
| AI is an external client | Assistant built in, optional (ADR-012, ADR-013) | Owner instructions 6 and 7 |

## Risks that shape the design

Full list in [06](06-ROADMAP-AND-DELIVERY.md). The three that could change this page: Loro failing `SPK-01`, op-driven DOM editing failing `SPK-02`, and small local models calling tools poorly in `SPK-07`.
