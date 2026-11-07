# 06 — Roadmap and delivery

**The only roadmap.** Rules are in [05](05-PHASES-AND-TASKS.md), and tasks are in [14](14-EXECUTION-TRACKER.md). **No dates:** none would be honest until Phase 0 has measured throughput.

## Where things stand

| Phase | Gives | State |
| --- | --- | --- |
| **D** Design | An agreed design | **Open.** Documents written. Fresh review running (`DOC-008`), owner sign-off outstanding |
| **0** Foundation | A repo where a broken change cannot land; the riskiest decisions tested | Not opened |
| **1** Engine | Notes model, operations, merge, durable store, export. No UI | Not opened |
| **2** Web editor | Blocks, databases and languages in a browser | Not opened |
| **3** Canvas and ink | Page and canvas switch; pen | Not opened |
| **4** Knowledge | Search, embeddings, graph, embeds, attachments | Not opened |
| **5** Assistant and agents | Web provider mode, tools, MCP | Not opened |
| **6** First release | Desktop app, local models, import, signed builds | Not opened |
| **7** Sync | Encrypted any-storage sync, state panel, merge review | Not opened |
| **8** Server | Co-editing, sharing, permissions | Not opened |
| **9** Tablet and mobile | Tablet-first interface, mobile shells | Not opened |
| **10** Platform and enterprise | SDK, more importers, SSO, audit | Not opened |

**The order is deliberate.** The engine comes before UI, as the owner set for OpenPresent. Sync ships late, but its seams (segments, device ids, the merge layer) exist from Phase 1. Languages start in Phase 2 and are never added later.

## Phase D — Design

- **Gives:** an agreed design, so later phases need no do-over.
- **Opens:** project start.
- **Closes when:**
  - [x] Requirements, HLD, LLDs, threat model, ADRs, roadmap and tracker exist. Evidence: [00](00-README.md).
  - [x] `tools/check-docs.py` passes.
  - [ ] Each `DEC-*` is answered, or marked as not blocking with the phase that needs it.
  - [ ] Each Proposed ADR is Accepted, or names a prototype that settles it.
  - [ ] Owner sign-off.

## Phase 0 — Foundation

- **Gives:** a repository where a broken change cannot land unnoticed, and answers to the two prototypes that could sink the design.
- **Opens:** Phase D closed, or a waiver.
- **In:** workspace, CI on three platforms, repo gates, fuzz workspace, `SPK-01`, `SPK-02`.
- **Out:** product features.
- **Closes when:**
  - [ ] CI is green on Linux, macOS and Windows, and a seeded violation is seen failing each job.
  - [ ] `SPK-01` reported, and ADR-005, ADR-006 and ADR-010 settled.
  - [ ] `SPK-02` reported in the launch languages, and ADR-007 and ADR-025 settled.
  - [ ] Licence gate fails on a seeded GPL dependency.
  - [ ] Owner sign-off.

## Phase 1 — Engine

- **Gives:** a headless engine that keeps notes safely and exports them.
- **Opens:** Phase 0 closed.
- **In:** `-model`, `-merge`, `-ops`, `-store`, `-package`, `-export`, the package reader in `-import`, `-sdk`. Version history.
- **Out:** UI, search, networking.
- **Closes when:**
  - [ ] Crash suite: killing the process at every step loses no acknowledged edit (PR-1).
  - [ ] Simulated devices converge on every generated history.
  - [ ] The engine's export passes `tools/cnotepack.py` and the schema, and an export, import, export round trip gives identical bytes (PR-3).
  - [ ] It compiles to `wasm32-unknown-unknown`, and native and wasm give identical bytes.
  - [ ] Fuzz targets exist for every parser.
  - [ ] Owner sign-off.

## Phase 2 — Web editor

- **Gives:** a person can write and organise in a browser, in any launch language.
- **Opens:** Phase 1 closed.
- **In:** app shell, all block types, slash menu, links, backlinks, outline, databases, undo, translation system, right-to-left.
- **Closes when:**
  - [ ] QR-1 holds in CI.
  - [ ] The pseudo-locale finds no untranslated string, and the Arabic run passes (PR-5).
  - [ ] Input-method tests pass for Chinese, Japanese and Korean.
  - [ ] An accessibility audit passes, screen reader included (QR-5).
  - [ ] Owner uses it for real notes, and signs off.

## Phase 3 — Canvas and ink

- **Gives:** the same page as a canvas, with pen.
- **Opens:** Phase 2 closed; `SPK-03` run on real devices.
- **Closes when:**
  - [ ] Switching views loses nothing, by a round-trip test.
  - [ ] Merge tests include ink and canvas.
  - [ ] `-render` produces PDF and PNG in every launch script.
  - [ ] Owner judges the pen on real hardware, and signs off.

## Phase 4 — Knowledge

- **Gives:** finding anything, and seeing how notes connect.
- **Opens:** Phase 2 closed; `SPK-05` and `SPK-06` reported.
- **Closes when:**
  - [ ] QR-2 holds, and each launch language has a passing search test set.
  - [ ] Related notes beat the word-overlap baseline on a labelled set, in English and Chinese at least.
  - [ ] Deleting every index and vector and rebuilding gives the same results.
  - [ ] Attachments resume after injected failures, and appear in export.
  - [ ] Owner sign-off.

## Phase 5 — Assistant and agents

- **Gives:** an assistant using the user's own provider, and agent access.
- **Opens:** Phase 4 closed.
- **Closes when:**
  - [ ] The prompt-injection suite passes: no note text causes an unapproved write (PR-6).
  - [ ] With AI off, a network capture shows no outbound request.
  - [ ] The MCP server passes a client conformance run, and `--root` escapes are refused.
  - [ ] API keys appear in no export, log, sync or mirror, by test.
  - [ ] Owner sign-off.

## Phase 6 — First release

- **Gives:** a desktop app and web app people can install and trust.
- **Opens:** Phase 5 closed; `SPK-07` reported.
- **Closes when:**
  - [ ] Signed installers on three platforms.
  - [ ] Model download resumes, checks hashes, and shows licence and size first.
  - [ ] The local server refuses a request from another origin, by test.
  - [ ] Import from Markdown, HTML and Evernote, with a report.
  - [ ] All six promises are green.
  - [ ] Licence and name review done (`DEC-08`).
  - [ ] Owner sign-off.

## Phase 7 — Sync

- **Gives:** encrypted sync through storage the person already has.
- **Opens:** Phase 6 closed; `SPK-04` reported.
- **Closes when:**
  - [ ] The fault-injection suite loses no acknowledged edit, across a folder, WebDAV and S3.
  - [ ] A device offline past any limit gets a visible restored copy (PR-1).
  - [ ] Every fault appears in the state panel (PR-2).
  - [ ] An outside review of the cryptography, before any public claim.
  - [ ] Owner sign-off.

## Phase 8 — Server

- **Gives:** real-time teams on a self-hosted server.
- **Opens:** Phase 7 closed.
- **Closes when:**
  - [ ] Two browsers co-edit in CI, and a load test meets its target.
  - [ ] Permissions are enforced per operation on the server.
  - [ ] It deploys from documented Compose and Helm.
  - [ ] Owner sign-off.

## Phase 9 — Tablet and mobile

- **Gives:** a good tablet and pen experience, and mobile shells if a prototype supports them.
- **Opens:** Phase 3 and Phase 6 closed.
- **Closes when:** a named device matrix passes, and the owner signs off.

## Phase 10 — Platform and enterprise

- **Gives:** a published SDK, more importers, enterprise requirements in [10](10-ENTERPRISE-REQUIREMENTS.md).
- **Opens:** Phase 8 closed.
- **Closes when:** a stable API with example embeds in CI, SSO and audit delivered, and the owner signs off.

## Risk register

| ID | Risk | Effect | Response | Tested by |
| --- | --- | --- | --- | --- |
| RISK-01 | Loro fails on long histories, or has unfixable defects | Merge design changes | Adapter, format-neutral export, fallback to yrs with our own tree | `SPK-01` |
| RISK-02 | Op-driven DOM editing is unreliable across browsers | Editor design changes | Prototype first, narrow browser list, last resort engine-drawn text | `SPK-02` |
| RISK-03 | Merged text reads badly, and review fires too often or too rarely | Trust is damaged | Measure, tune the threshold | `SPK-01` |
| RISK-04 | Small local models call tools poorly | Desktop assistant is weak | Start with read-only tools, recommend larger models | `SPK-07` |
| RISK-05 | Model weights or fonts carry licence traps | Legal exposure | ADR-027, review before listing | `DEC-08` |
| RISK-06 | Prompt injection through notes | Unwanted changes, data leaving | Approval on writes, tool allow-list, injection suite | `SEC-*`, Phase 5 |
| RISK-07 | API key theft in the browser | User's account is abused | CSP, no HTML from documents, one-click removal | [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| RISK-08 | Scope: this is many hard products at once | Nothing ships | Each phase is useful alone, strict order, later phases can be cut | Owner review |
| RISK-09 | AFFiNE covers the same ground with more people | Little reason to switch | Compete on licence, trust, languages, local AI and weight | Owner review |
| RISK-10 | No Windows machine with the owner | Windows issues found late | CI runners; a paid or borrowed machine before release | `DEC-07` |
| RISK-11 | Pen quality cannot be judged without devices | Phase 3 cannot close | Owner's devices, or borrowing | `DEC-05` |
| RISK-12 | Translation quality is unreviewed | Poor first impression in those languages | Native review before a language is called supported | `I18N-05` |
| RISK-13 | Legal status of MIT dependencies, which carry no express patent grant, and of the name | Enterprise objection | Counsel before first public release | `DEC-08` |
