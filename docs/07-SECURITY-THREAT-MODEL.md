# 07 — Security threat model

**Every file, remote update, note and AI reply is hostile until proven otherwise.** Each threat has a control and a named test. **Status:** `Tested` means a test exists and was seen to fail without the control. `Planned n` names the phase. Only Tested rows may be relied on. No Rust exists, so only the standalone package reader has any Tested rows.

## Assets

Note content; API keys; sync passphrases and keys; the integrity of notes; the user's provider account; the user's computer.

## Attackers

A stranger's file; a shared note or imported page containing instructions for the assistant; a compromised or nosy storage provider; a malicious website the user visits while the app is open; a malicious MCP client; another local program; a dishonest co-editor.

## Files and packages

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-F01 | Zip bomb by per-entry ratio | Ratio limit measured while decompressing | Tested, reader: `test_zip_bomb_ratio` |
| SEC-F02 | ZIP header lies about size | Measure while reading, never trust the header | Tested, reader, by the byte-flip sweep. Engine Planned 1 |
| SEC-F03 | Path traversal, absolute or drive paths | Refused, `CN-PKG-0004` | Tested, reader. Engine Planned 1 |
| SEC-F04 | Paths equal after case or Unicode folding: a scanner approves one, the app opens the other | Refused, `CN-PKG-0005` | Tested, reader. Engine Planned 1 |
| SEC-F05 | Symbolic link or encrypted entry | Refused, `CN-PKG-0006` | Tested, reader. Engine Planned 1 |
| SEC-F06 | Damaged file silently changes notes | Hash of every file in the manifest | Tested, reader and prototype: byte-flip sweep, notes never changed |
| SEC-F07 | Smuggled files alongside the manifest | Refused, `CN-PKG-0009` | Tested, reader. Engine Planned 1 |
| SEC-F08 | Deep JSON exhausts the stack | Nesting limit | Tested, reader. Engine Planned 1 |
| SEC-F09 | Imported HTML runs script, or loads tracking content | Sanitised to a block model. No HTML reaches the DOM. No fetch | Planned 6 |
| SEC-F10 | Attachment opened as code | Attachments are never run. Saving is explicit. Test: a script attachment has no execution path, `FIL-01` | Planned 2 |
| SEC-F11 | SVG attachment carries script | Rendered only through an image element under CSP | Planned 4 |
| SEC-F12 | PDF used as a background exploits a viewer | Runs in the browser's sandboxed path or a bounded parser. Decided in [43](43-CANVAS-AND-INK.md) | Planned 3 |

## Sync and CRDT input

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-S01 | Malformed CRDT update panics or corrupts. Loro has open reports (its issues 1262, 1254, 1216) | Size limits, verify on load, panic isolation at the `-merge` boundary | Planned 1 |
| SEC-S02 | Storage reads notes | Authenticated encryption before upload | Planned 7 |
| SEC-S03 | Storage tampers, replays or withholds | Authenticated segments, sequence checks. Withholding shows as a stale device in the state panel | Planned 7 |
| SEC-S04 | Metadata leak: sizes, counts, timing | **Not prevented.** Stated plainly in the product and docs | Accepted risk |
| SEC-S08 | Removed or stolen device keeps writing under the old key | The new key record carries a per-device cutoff. Later old-key objects, acks and removal records are refused | Planned 7 |
| SEC-S05 | Removed device keeps reading | Key rotation on removal. It can still read what it already had, and the product says so | Planned 7 |
| SEC-S06 | Lost passphrase | Recovery key, with a plain warning. No backdoor | Planned 7 |
| SEC-S07 | Update bomb: huge history | Size limits per segment and per page | Planned 7 |

## Web app

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-W01 | Script injection through note text. **Text is in the real DOM (ADR-007), so this is the main risk** | Document strings set as text only. Strict CSP. A gate refuses raw HTML insertion on document data | Planned 2 |
| SEC-W02 | API key stolen by an injected script | SEC-W01, plus CSP limiting connections to the configured endpoint | Planned 5 |
| SEC-W03 | Automatic fetch leaks that a note was opened | No remote images or links load automatically | Planned 2 |
| SEC-W04 | Clipboard paste brings script | Paste is parsed and sanitised to blocks | Planned 2 |
| SEC-W05 | Link to a dangerous URL scheme | Allow-list of schemes | Planned 2 |
| SEC-W06 | **Injected script in the desktop webview calls shell commands** to redirect notes, keys, sync or a model | Native confirmation drawn by the shell for endpoint, storage and model changes. Keys bound to their origin in Rust ([44](44-DESKTOP-SHELL.md)) | Planned 6 |
| SEC-W07 | **Stored script reads a remembered web API key.** A CSP read from `localStorage` is not a defence | Sanitising is the defence. Keys are session-only by default, and remembering is an explicit choice with a warning ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)) | Planned 5 |
| SEC-W08 | A note in a script without local fonts triggers a font request | Slices precached, or system fonts. Font-less test ([50](50-INTERNATIONALIZATION.md)) | Planned 2 |

## AI

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-A01 | **Prompt injection:** note text tells the assistant to change or send things | Tool results are marked as data. Every write needs approval. The assistant has no tool that reaches the network | Planned 5. Prototype has the approval flow, tested with a stand-in model |
| SEC-A02 | Approval fatigue, or a misleading approval card | The card shows the real change, from the tool arguments, never the model's words | Planned 5 |
| SEC-A03 | Exfiltration through an embedded link or image in a reply | Replies render as text. Links are not auto-fetched | Planned 5 |
| SEC-A04 | Notes reach a provider unexpectedly | Off by default. The panel names the provider. No background sending | Planned 5 |
| SEC-A05 | Encrypted notebook content goes to a provider or MCP client | Explicit opt-in per notebook, enforced in `-tools` for every surface. Tested again once encryption exists | Planned 7 |
| SEC-A06 | Key appears in export, sync, logs or mirror | Keys live in a separate store with no path to those. A test searches all output for a seeded key | Planned 5 |
| SEC-A07 | Runaway tool loop costs money | Round limit, stop button, usage shown | Planned 5 |
| SEC-A08 | **An approved write copies private text to a shared page.** A shared note tells the assistant to do this, and the person clicks Allow | The approval card shows audience and sources read, from engine data. Private-to-shared copies get a distinct warning and full preview. **This reduces the risk and cannot remove it** | Planned 5 |

## Local model

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-L01 | **A website calls the local model server.** `llama-server` defaults to allowing any origin | Loopback only, random key per run, origins restricted. Test sends a cross-origin request | Planned 6 |
| SEC-L02 | Another local program uses the server | The random key. A determined local program with the same user's rights is out of scope, and the docs say so | Planned 6 |
| SEC-L03 | Tampered model download, including a tampered partial file | Pinned SHA-256 over the whole final file before install and before each start, reviewed list | Planned 6 |
| SEC-L04 | A malicious model file exploits the parser in the runtime | Reviewed list by default, warning on custom files, OS sandbox with no network or notebook access. A separate process alone contains nothing. Where a platform has no sandbox, the interface says so | Planned 6 |
| SEC-L06 | Another program takes the port and receives the key and prompts | Bind first, then a nonce challenge proves the identity of the server before anything is sent | Planned 6 |
| SEC-L05 | Licence trap in a model | Licence shown first; reviewed before listing (ADR-027) | Planned 6 |

## Agent access

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-M01 | An MCP client reads outside the workspace | `--root` allow-list, canonical paths, symlink escapes refused. No root, no access | Planned 5 |
| SEC-M02 | A client writes unexpectedly | `--read-only` flag. Each write is one undo step | Planned 5 |
| SEC-M03 | Tool output too large | Bounds per call, truncation stated | Planned 5 |
| SEC-M04 | **A note injects instructions into an MCP client, which reads everything and sends it out through its own tools.** `--read-only` does not help, and we do not control the client | Setup text says so. `--root` per notebook narrows what is reachable. **Not fully preventable from our side** | Planned 5 |

## Server

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-V01 | A client exceeds its permission | Per-operation enforcement in managed mode. Encrypted mode can only restrict by key holding, which [57](57-SERVER-DESIGN.md) explains | Planned 8 |
| SEC-V02 | Resource exhaustion | Quotas, per-connection limits | Planned 8 |
| SEC-V03 | Stolen token | Short-lived tokens, revocation | Planned 8 |

## Supply chain

| ID | Threat | Control | Status |
| --- | --- | --- | --- |
| SEC-C01 | A copyleft or source-available dependency enters | `cargo deny`, reviewed weights and fonts list | Planned 0 |
| SEC-C02 | Known-vulnerable dependency | `cargo audit` in CI | Planned 0 |
| SEC-C03 | A CDN is tampered with | The real product loads nothing from a CDN. The prototype loads fonts from Google Fonts only | Planned 2 |
| SEC-C04 | Tampered update | Signed updates | Planned 6 |

## Never

- No macro or script execution from a notebook, package or model reply.
- No automatic network fetch caused by note content.
- No secret in code, logs, export, sync or the mirror.
- No telemetry by default.
- No public encryption claim before an outside review (`CRY-02`).
