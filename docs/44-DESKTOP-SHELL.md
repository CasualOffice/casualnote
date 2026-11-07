# 44 — Desktop shell

**Status: design only, nothing built.** No `desktop/` directory exists. Every limit is a proposal. Tauri behaviour is taken from its documentation, or marked unverified, and nothing here has run. ADR-011 is Accepted by inheritance.

## Outcome

One small signed desktop app on macOS, Windows and Linux. It is the web app's interface with the engine running as native Rust. It offers a live readable folder, opens `.cnotepack` files from the operating system, and can download and run a local AI model. With no optional feature in use, nothing leaves the computer.

The shell adds no engine logic. It owns the window, the disk, the network, secrets and child processes, as [02](02-ARCHITECTURE.md) gives the host those jobs.

**The owner has no Windows machine (`DEC-07`, RISK-10).** Windows relies on CI runners until a release candidate, then on a borrowed or rented machine. This limits what can be claimed, and the section on Windows says exactly how.

## Design

### Shape

```
┌───────────────────────── desktop app, one process ───────────────────────┐
│  Webview, one per window: webapp/ ES modules, same as the web app         │
│    editor, canvas, panels · host/desktop.js is the only Tauri-aware file  │
│        │ invoke, Channel                       ▲ events                   │
│        ▼                                       │                          │
│  Rust: Tauri commands, thin and typed                                     │
│    engine actor per workspace ── casual-note-sdk (native, no wasm)        │
│    sync host: folder, WebDAV, S3 adapters                               │
│    mirror watcher · download manager · model supervisor · keychain        │
└───────┬────────────────────────────────────────────────────┬──────────────┘
        ▼                                                    ▼
   local disk: notebooks, mirror                 llama-server child process
                                                 127.0.0.1, random port and key
```

**In its own Cargo workspace.** `desktop/` is a binary depending on `casual-note-sdk` and Tauri 2, as [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) states. Tauri's dependency tree would otherwise weigh on every workspace build. OpenCalc names the hazard in its doc 81: a separate workspace can break unnoticed, as its `fuzz/` did (its `CI-014`). So the `desktop` CI job and a local `cargo check` exist **from the day the directory does**.

**No `casual-note-tauri` crate.** A crate earns its place by having more than one consumer, which is OpenCalc's reasoning under its ADR-023.

**Native storage adapters** have one problem: the `SPK-04` harness and the CLI may also want them, and they cannot enter `crates/` (boundary rule 4). Proposed row `FND-22` decides their home.

### Native engine, and no wasm on the hot path

As OpenCalc's docs 44 and 81 decided, the desktop app does not run the engine as WebAssembly in the webview. Wasm would keep a browser's memory limits, threads and storage, and lose direct disk access.

**One editor, one binding, two implementations.** OpenCalc's options were: bundle wasm, reimplement the whole call surface separately, or make the single engine handle swappable. It chose the third (its `TAURI-003`), and so does this design.

```js
// webapp/src/host/engine.js. One interface, chosen at startup.
export const engine = isDesktop()
  ? (await import('./desktop.js')).engine   // invoke to native -sdk
  : (await import('./wasm.js')).engine;     // -wasm in this page
```

Three things differ from OpenCalc, and need care.

**1. The interface is asynchronous on both hosts from the first day.** Tauri's `invoke` returns a promise, and a wasm call does not. If `WEB-01` builds a synchronous editor API, the desktop port becomes a rewrite. So every call through `engine` returns a promise, even in `wasm.js`. This is a constraint on [42](42-EDITOR-AND-RENDERING.md) and `WEB-01`, and it must land in Phase 2, four phases before this shell.

**2. The typing path crosses IPC.** Text is edited in the real DOM (ADR-007), unlike OpenCalc's canvas grid. The flow in [02](02-ARCHITECTURE.md) becomes:

```
beforeinput (webview)
  → editor applies or lets the browser apply the visible change; sends op with local sequence n
  → invoke("apply_ops") → engine actor: -ops validates, -merge writes, -store fsyncs
  → reply { n, saved: true, patch } → "saved" indicator; patch reconciled by sequence
```

The webview never waits on the reply before painting. The saved indicator does wait, so PR-1 still holds: nothing shows as saved before the fsync. If the engine refuses an op, the patch corrects the DOM. Input method composition follows [42](42-EDITOR-AND-RENDERING.md): nothing is touched during composition.

**IPC cost on this path is unmeasured.** QR-1's 16 ms budget may or may not survive `invoke` plus fsync plus patch, on three webviews, and OpenCalc's figures are for a different boundary. Proposed row `FND-24` runs a typing-path prototype in a Tauri shell, along with `SPK-02`.

**3. `SPK-02` does not name desktop webviews.** Its list is Chrome, Safari and Firefox. Desktop adds WKWebView (close to Safari, not the same), WebView2 (close to Chrome) and **WebKitGTK, which none of them represents.** Input methods, right-to-left text, pen events and accessibility on WebKitGTK are unverified. `FND-24` covers this too.

**Commands.** A transport with no logic. Illustrative:

| Command | Purpose |
| --- | --- |
| `open_workspace(handle)` | Opens, takes the workspace lock, starts the actor |
| `apply_ops(session, seq, ops)` | Returns `{ seq, saved, patch }` |
| `undo`, `redo` | Local undo ([24](24-OPERATIONS-AND-MERGE.md)) |
| `project_page(session, page, range)` | Blocks for a viewport |
| `subscribe(session, channel)` | Streams changes through a Tauri `Channel` |
| `export_package(session, scope, handle)`, `import_preview(handle)`, `import_commit(id)` | [36](36-EXPORT-FORMAT.md), [37](37-IMPORT-FORMATS.md) |
| `search`, `graph` | [60](60-SEARCH-AND-EMBEDDINGS.md), [61](61-KNOWLEDGE-GRAPH.md) |
| `sync_status`, `sync_now`, `sync_connect`, `sync_remove_device` | [56](56-SYNC-DESIGN.md) |
| `pick_file`, `pick_folder`, `save_as` | Native dialogs. Return an opaque handle, never a path to pass back |
| `mirror_status`, `mirror_set_location` | Below |
| `download_start/pause/resume/cancel`, `download_status` | Below |
| `model_start/stop/status`, `assistant_chat` | Below, [62](62-AI-ASSISTANT-AND-PROVIDERS.md) |
| `system_check` | RAM, disk, GPU hints |
| `open_external(url)` | User click only, scheme allow-list (SEC-W05) |
| `check_update`, `install_update` | Below |

**Paths never come from the webview.** Dialogs and association handlers give the webview an opaque handle. A script that reaches the IPC cannot name an arbitrary file, which keeps the "engine receives bytes, not paths" boundary from OpenCalc. **Command names and argument shapes are checked by a contract test** that runs the same fixture through `wasm.js` and `desktop.js`, since OpenCalc warns that the risk of a swappable binding is two implementations that must agree (QR-3).

**Engine threading.** One actor thread per open workspace serialises access to `-sdk`, which is not assumed thread-safe. Long jobs take a cancel token. Derived refreshes run off the typing path (PR-4). A panic in the actor closes that workspace with a visible error and reopens from the log. The log is authoritative, and only derived data may need a rebuild.

### Windows and tabs

**One process, one engine per workspace, many windows.** The single-instance plugin routes later launches to the running process.

| Behaviour | Design |
| --- | --- |
| Tabs | Drawn in the DOM, by the web app's shell (`WEB-02`). Native tabs are not used, so web and desktop share behaviour and right-to-left mirroring |
| Windows | One webview each. A tab moves to a new window through a menu or `Ctrl/Cmd+Shift+N`. Drag-out is later |
| Same page in two windows | Both subscribe to one session, and updates reach both. Undo stack is per page in the actor, so both see the same stack |
| Same workspace in two processes | Refused by a lock file with a stale check (`CN-DSK-0003`). Two apps on one log are out of scope |
| Restore | Windows, tabs, size and position are restored per workspace, clamped to screens that exist now |
| Close window with edits | **No "save?" dialog,** since everything is saved already. The window closes after in-flight replies return, bounded to 2 s (proposal), then warns if a write failed |
| Quit | Actor flush, stop the model, stop watchers, close. Sync is not awaited. If `unsynced` is above zero, the next launch resumes and the panel states it |
| macOS | App stays alive with no windows, and Dock click reopens. Linux and Windows quit on last window, with no tray in the first release |
| Crash recovery | Log replay ([23](23-STORAGE-FORMAT.md)), then windows restored. Crash reports are never sent (QR-6) and stay local |
| Accessibility | The tab strip follows the same pattern as the web. Native dialogs and menus come from the OS. VoiceOver and NVDA on a webview are unverified (`A11Y-01`) |

Window title holds the page title. Titles can contain anything, so they are passed as plain strings to the OS and bidirectional control characters are stripped for this purpose only.

### Menus

The OS draws the menu from ids the editor supplies, as OpenCalc's `TAURI-004` does. There is no second menu definition.

```
webapp command registry → setMenu([{ id, labelKey, accel, role, enabled, checked }])
  → shell builds native menu → click → event { id } → same command path as a keyboard shortcut
```

| Menu | Items |
| --- | --- |
| App (macOS) | About, Preferences, Check for Updates, Services, Hide, Quit |
| File | New page, New window, Open workspace, Import, Export, Close tab, Close window |
| Edit | Undo, Redo, Cut, Copy, Paste, Paste as plain text, Find |
| View | Page or canvas, Sidebar, Assistant, Sync panel, Zoom, Full screen |
| Go | Back, Forward, Quick switcher, Search everything |
| Window | Minimise, Tabs, list of windows |
| Help | Keyboard shortcuts, Report a problem (opens a local draft, sends nothing), About licences |

- **Languages.** Labels come from the same ICU catalogue (ADR-018). The menu is rebuilt on a language change. Right-to-left: macOS and Windows draw their own, and behaviour with mixed direction is unverified.
- **Edit commands use native roles where possible,** so input methods, spell check and platform services keep working in the DOM. Undo and Redo do not: they go to the engine, as the DOM's own undo stack is not the truth (ADR-007). Interaction of that with an input method is part of `FND-24`.
- **Enabled state** is pushed on focus changes, and is advisory. The command path checks again.

### Live readable folder

The mirror is derived, one-way and regenerated (ADR-010, `STO-04`). The format is in [23](23-STORAGE-FORMAT.md). This is what the shell adds.

| Rule | Detail |
| --- | --- |
| Off the typing path | Regenerated after edits, debounced at 2 s (proposal), per page |
| Default location | Under the user's Documents, in a folder named for the workspace, shown in Settings. The person may move it |
| **Never read back** | A file there is never imported automatically |
| **User edits are not destroyed** | The shell keeps a hash per mirror file. Before overwriting, it compares. If the file on disk differs, the edited file moves to `_edited/<date>/` inside the mirror, and the app offers to import it as a new page, as a notice that stays until dismissed. Deleting a mirror file just means it returns |
| Watching | `notify` crate (licence to confirm under `GOV-05`) watches only for those notices. A missed event is caught by the hash check. Watchers and network drives are unreliable, so correctness never depends on them |
| Writes | Temp file, fsync, rename |
| **Encrypted notebooks** | The mirror stays on this device only, and never reaches sync storage ([56](56-SYNC-DESIGN.md)). **Inside the sync root it is refused** (`CN-DSK-0006`). Inside a known cloud-client folder it warns that plaintext will reach that provider, names which, and needs confirmation. Detection is by known paths, is incomplete, and the docs say so |
| Names in every language | Titles become file names through one function in `-store`, shared with [36](36-EXPORT-FORMAT.md): NFC, control and bidirectional override characters removed, reserved Windows names escaped, path length bounded, collisions numbered. CJK and Arabic titles must work on all three platforms. macOS may present a different normal form on some volumes (unverified), so comparison is always after normalising |
| Deleted or trashed pages | Removed from the mirror, or moved to `_trash/`. [23](23-STORAGE-FORMAT.md) decides |
| Unavailable location | Drive removed or permission lost: mirror pauses (`CN-DSK-0005`). The notes are unaffected, and it regenerates fully on return |
| Disabled | The setting turns it off and offers to delete the folder. Deleting it loses nothing (`STO-04`) |

### File associations: `.cnotepack`

Declared under `bundle.fileAssociations`. Only `.cnotepack` is associated. `.cnotepack.zip` from the prototype and web downloads can be chosen through Import. The extension depends on the final name (`DEC-01`). The MIME type and macOS UTI are proposals: `application/vnd.casualnote.package+zip`, and a UTI under the final bundle identifier.

| Platform | How the path arrives | Notes |
| --- | --- | --- |
| macOS | `RunEvent::Opened` | Also at cold start, before any window exists |
| Windows | `argv`, then single-instance forwarding | Registered by installer |
| Linux | `argv` from the `.desktop` file, `%F`, then forwarding | Depends on packaging format |

**Flow.**

```
OS event → queue (holds until the webview reports ready, bounded at 32)
  → canonicalise: must be a regular file, size within the 21 limit, not a symlink out
  → handle → -import opens with bounded reader, all CN-PKG-* apply (ADR-017)
  → preview: name, pages, size, languages, anything not understood
  → person chooses a target: new workspace, or into the open one
  → one transaction, one undo step, report (37)
```

**Opening a file never changes anything without a click, and no content runs.** A double-clicked package is hostile input, the same as any other (QR-4, SEC-F*). It can arrive while a workspace is locked, in which case it waits. Several at once queue. A package that fails shows its code and message, without a crash.

### CSP and webview security

**Do not copy OpenCalc's security config: its `desktop/tauri.conf.json` has `"csp": null`.** This product puts note text in a real DOM, so the CSP is an important control (SEC-W01). Proposed policy:

```
default-src 'none';
script-src 'self';
style-src 'self';
img-src 'self' blob: cnasset:;
media-src 'self' blob: cnasset:;
font-src 'self';
connect-src ipc: http://ipc.localhost cnasset: http://cnasset.localhost;
worker-src 'self';
object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'
```

Notes on it:

- **No provider or model address in `connect-src`.** On desktop, all AI traffic goes through Rust. A script injected into the webview cannot post notes or keys anywhere (SEC-W02). The web app's different policy is in [62](62-AI-ASSISTANT-AND-PROVIDERS.md).
- **No `'wasm-unsafe-eval'`,** since the engine is native. If a search or embedding component later needs wasm in the webview, the row that adds it must justify the change.
- **No `'unsafe-inline'`.** Tauri adds hashes or nonces for its own injected scripts, per its documentation. Inline `style` attributes are blocked under this policy while CSSOM property writes are not, so the editor uses the CSSOM. Whether WKWebView, WebView2 and WebKitGTK all behave alike here is unverified, and a CSP violation in testing fails the job.
- **`dangerousDisableAssetCspModification` stays off.**

**Other controls.**

| Control | Design |
| --- | --- |
| Capabilities | Allow only our commands and the minimum core events. **The `fs`, `shell`, `http` and `process` plugin APIs are never exposed to the webview** |
| Attachments | Served by a custom `cnasset:` protocol by content hash, per session, with `Content-Type` from an allow-list and `nosniff`. SVG only through an image element (SEC-F11). `convertFileSrc` is not used, since it maps paths |
| Navigation | `on_navigation` denies anything but the app's own origin. `window.open` denied. Links go through `open_external` with a scheme allow-list |
| Global object | With no bundler (ADR-025), `withGlobalTauri: true` is the plain route, as OpenCalc. Only `host/desktop.js` may read it, enforced by a gate |
| Drag and drop | Tauri's window drop handler is reported to intercept HTML5 drag on Windows (unverified), and the editor needs drag to move blocks. It is set off, and files arrive through the web `File` API. The handle rule needs testing with that route |
| Devtools | Release builds without them |
| Remote content | None loaded. No CDN, no remote fonts (QR-6, SEC-C03) |
| Secrets | Provider keys, sync credentials, session tokens and the optional unlocked key sit in the OS keychain, through the `keyring` crate or a copy of OpenCalc's `-secrets`. Linux without a secret service: refuse to store and ask each time (`CN-DSK-0013`). The webview never reads a secret; it can only ask Rust to use one |

### Local model process

**The design, catalogue and provider contract are owned by [62](62-AI-ASSISTANT-AND-PROVIDERS.md)** (ADR-012, ADR-016, ADR-027, `SPK-07`). The shell supervises the process. If `SPK-07` changes the runtime, this section changes with it.

**Bundled as a Tauri sidecar,** per target triple, not downloaded. Running a downloaded executable would need a second signature path, and macOS quarantine would complicate it. Bundling adds installer size, which is unmeasured, and which the "light desktop app" claim has to survive (PERF-15). Which GPU builds (Metal, Vulkan, CPU) ship is decided by `SPK-07`.

**States.**

```
Absent ── model downloaded and verified ──▶ Installed
Installed ── first assistant use, or Start ──▶ Checking
Checking ── RAM and disk fine, or user overrides warning ──▶ Starting
Checking ── refused ──▶ Installed (CN-DSK-0010)
Starting ── /health ok ──▶ Ready
Starting ── exit or 180 s timeout (proposal) ──▶ Failed
Ready ── request ──▶ Busy ── done ──▶ Ready
Ready ── idle 10 min (proposal, setting) ──▶ Stopping
Any running ── quit, model switch, update, memory pressure ──▶ Stopping
Stopping ── exit, else kill after 5 s ──▶ Installed
Ready, Busy ── unexpected exit ──▶ Crashed
Crashed ── attempt < 3 in 10 min ──▶ Starting (backoff)
Crashed ── otherwise ──▶ Failed (CN-DSK-0011; shown; Start retries)
```

| Rule | Detail |
| --- | --- |
| **Never at launch** | Started on first use. A person not using it pays nothing |
| Network | `--host 127.0.0.1`. Port chosen by the OS and read back, or picked then bound. Unverified which the runtime supports |
| Key | 256 random bits per run, passed so it does not show in a process list where the runtime allows (unverified: environment or file versus an argument). SEC-L02's limit applies: another program with this user's rights is out of scope |
| Origins | Restricted, by flag as `SPK-07` confirms, since the default allows any (SEC-L01). A test sends a cross-origin request, and it must fail |
| **The webview never talks to it** | All requests go through Rust, streamed back by `Channel`. The port and key never reach JavaScript, and the CSP would refuse them anyway. So an injected script cannot use the model directly |
| Model file | Hash re-checked if size or mtime changed since last verify. A parser bug in the runtime stays in a separate process (SEC-L04) |
| Lifetime | **The child must not outlive the app.** Windows: job object with kill-on-close. Linux: `PR_SET_PDEATHSIG`. macOS: child watch through a pipe closed at parent exit, or a small supervisor, since macOS has no direct equal (unverified route). At startup, a stale PID file is checked against process identity and killed only if it matches |
| Logs | Runtime stdout and stderr captured to a bounded file. **Prompts are never logged,** and the verbosity flag is chosen with that in mind. Confirm in `SPK-07` |
| Air gap and off | With AI off the sidecar never runs. A network capture stays silent (PR-6) |
| Memory pressure | macOS and Linux signals, where available (unverified on each). On warning while idle it stops, and while busy it finishes and stops. Otherwise it is just a status line |

### Download manager

For models and any later optional data (ADR-027). Nothing downloads without a click (`DSK-02`).

**Before starting,** the person sees file size, licence, source host and required disk. The catalogue is signed with the app and updated with it: fetching it separately would be a background network call.

```
Queued ── slot free, disk check ok ──▶ Connecting ── 200/206 ──▶ Downloading
Downloading ── pause ──▶ Paused ── resume ──▶ Connecting
Downloading ── transient error ──▶ Backoff ── retry ──▶ Connecting
Downloading ── complete ──▶ Verifying ── hash matches ──▶ Installing ── rename ──▶ Done
Verifying ── mismatch ──▶ Failed (part file deleted, CN-DSK-0008)
Any ── cancel ──▶ Cancelled (part file deleted)
Any ── app quits ──▶ Paused on next launch; never auto-resumed without the setting
```

| Aspect | Design |
| --- | --- |
| Resume | `Range` with `If-Range` and the stored `ETag`. If the server ignores the range or the validator changed, restart and say so. Mirror behaviour of model hosts is unverified, and `SPK-07` tests it |
| Integrity | Pinned SHA-256 from the catalogue. Hash state is saved periodically, so resume does not re-read the whole file |
| Part file | `<name>.part` beside a small JSON state. Atomic rename after verifying |
| Transport | HTTPS only. Redirects only to HTTPS and to hosts in that catalogue entry. System proxy settings respected, unverified on each OS |
| Concurrency | One large file at a time, to keep the app quiet. Bandwidth limit as a setting |
| Disk | Requires file size plus 1 GiB free (proposal) at start. Re-checked as it runs; `ENOSPC` pauses it with a clear message |
| Custom files | A local GGUF can be added with a warning (SEC-L04). Same process isolation |
| Status | Bytes, rate, remaining, source. Visible in Settings and a small indicator. A failure stays until dismissed |
| Metered networks | No reliable cross-platform signal (unverified). Size is always shown first |

### RAM and disk checks

`system_check` reports through `sysinfo` (licence to confirm). Figures are advice unless stated.

| Check | When | Rule (proposals) | Result |
| --- | --- | --- | --- |
| Model RAM | Before download and before start | `required_ram` from catalogue ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)). Below it: warn, and offer a smaller model. Below 75% of it: start needs an explicit override | `CN-DSK-0010` |
| Unified memory | macOS, Apple silicon | The figure counts against system memory, and GPU limits are unverified | Message wording |
| Model disk | Before download | Size plus 1 GiB | Refused (`CN-DSK-0007`) |
| Workspace disk, warning | At launch and periodically | Under 2 GiB free | Banner |
| Workspace disk, low | Same | Under 256 MiB free | Mirror, index and downloads pause. **The log keeps writing** |
| Disk full on write | The log write fails | **"Saved" does not show.** A persistent banner names the unsaved state, and text stays in memory and in the editor with retry. Never a silent drop (PR-1) | `CN-DSK-0004` |
| Import or export | Before starting | Estimate plus margin | Refused with the size |
| App memory | Not enforced | Measured against an Electron peer later (PERF-15) | None |

All figures come from a seam and are never read in `crates/`. Hardware needs for models have not been measured ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)), so any required figure in the first catalogue is an estimate and is labelled as one.

### Updates and signing

**Update checks are opt-in.** PR-6 and the Phase 5 gate say that with AI off a network capture shows nothing, and QR-6 says usable air-gapped. A silent check at launch would break both. So:

- First run shows one quiet choice, "Check for updates automatically", defaulting to off and sending nothing until chosen.
- Help has "Check for Updates".
- The check sends only the version and platform, needed to fetch the feed.
- A security-only notice cannot reach someone who opted out. That is a real cost of the promise, and the docs say so. Owner may wish to revisit it.

**The updater.** `tauri-plugin-updater`. Per [Tauri's documentation](https://v2.tauri.app/plugin/updater/), a signature is required and cannot be disabled, using a public key in the app's config (SEC-C04). The feed is static, hosted with releases (`DEC-12`), and is pinned to HTTPS.

| Platform | Package | Signing | Update route | Status |
| --- | --- | --- | --- | --- |
| macOS | `.app` in `.dmg` | Developer ID, hardened runtime, notarised, stapled. The sidecar is signed too | Tauri updater | Needs Apple Developer enrolment |
| Windows | NSIS or MSI (chosen at `DSK-04`) | Authenticode. Cloud or hardware-held certificate. SmartScreen reputation is unverified, and a new certificate may warn at first | Tauri updater | Certificate cost and custody open |
| Linux | AppImage, `.deb`, `.rpm` | Updater signature for AppImage. GPG for packages | Updater for AppImage. Package manager for others, unverified | Flatpak and Snap later |

Proposed row `FND-25` covers enrolment, certificates, cost and who holds the keys, since `DSK-04` assumes they exist.

**Key custody.** The updater key signs everything every install will trust, and losing it stops updates to all installs. Kept offline or in CI's secret store with a documented rotation route. Two people or one owner with a sealed backup is an owner decision.

**Order of an update.** Download, verify signature, stop the model, flush the actor, install, relaunch. If the new version's storage format is newer, `-store` migrates after taking a copy of the manifest and any rewritten files ([23](23-STORAGE-FORMAT.md)). An older app opening a newer format refuses with a message, and never guesses. A failed update leaves the old version in place. Release notes are shown in the app's language where translated.

### Windows without a Windows machine

| Verified by CI runners (once they exist) | Not provable there until a release candidate |
| --- | --- |
| Builds, bundles, and tests run on `windows-latest` (`desktop` job, [15](15-CI-AND-RELEASE-GATES.md)) | Real installer experience, SmartScreen and antivirus reaction |
| App launches and a scripted webview smoke test runs, if WebDriver works there (unverified) | Input methods: Chinese, Japanese and Korean IME composition |
| Path handling tests: reserved names, long paths, NTFS rename | Pen, palm rejection and touch |
| Job object kills the child | NVDA and Narrator with the editor |
| `MoveFileEx` semantics in the sync folder adapter | High DPI and multi-monitor |
| WebView2 present on the runner | WebView2 version spread, and machines missing it |
| Upgrade from the last installer, scripted, if a runner allows | Real cloud clients on Windows ([56](56-SYNC-DESIGN.md)) |
| | GPU in the local model |

Before any release: a borrowed or rented machine (`DEC-07`) runs a written checklist covering the right-hand column. **The app is not described as supported on Windows until it has passed.** Until then, release notes call Windows builds "CI-tested only". Native review of Arabic and Hindi input on Windows belongs to this too (`I18N-05`).

## Failure modes and limits

| Failure | Behaviour | Code |
| --- | --- | --- |
| Webview missing or too old (WebView2 absent, WebKitGTK old) | Native dialog before any window, with what to install | `CN-DSK-0001` |
| Webview crash or hang | Reload the view from the engine, nothing lost. Engine state is in Rust | `CN-DSK-0002` |
| Second process on a workspace | Refused, and forwarded to the running instance | `CN-DSK-0003` |
| Stale lock after crash | PID and start-time check, then cleared | None |
| Disk full | "Saved" withheld, banner | `CN-DSK-0004` |
| Mirror location gone | Paused, visible | `CN-DSK-0005` |
| Mirror in sync root | Refused | `CN-DSK-0006` |
| IPC reply late or lost during webview reload | Patch reconciled by sequence on resubscribe | None |
| Actor panic | Workspace closes with a message, log replay on reopen | `CN-DSK-0012` |
| Download: no disk, hash mismatch, host refused | Refused, visible | `CN-DSK-0007`, `0008`, `0009` |
| Model: not enough RAM | Warn or refuse | `CN-DSK-0010` |
| Model: crash loop | Stops after 3 | `CN-DSK-0011` |
| Model: child orphaned after a hard kill | Removed at next launch if identity matches | None |
| Keychain unavailable | Ask each time, store nothing | `CN-DSK-0013` |
| Update: signature fails | Refused, old app stays | `CN-DSK-0014` |
| Bad association path | Refused with reason | `CN-DSK-0015` |
| Network drive for notebooks | fsync and watch semantics are unknown. Warned against, and not a supported location | `CN-DSK-0016` |
| Webview rendering differences | Text is real DOM, so line breaks differ across OSes (ADR-007). Export is headless, so unaffected | None |
| Pen on WebView2 or WebKitGTK | Unverified (`SPK-03`) | None |

### Error codes and limits

Proposed for [20](20-ERROR-CODE-REGISTRY.md) and [21](21-PARSER-LIMITS.md).

| Code | Meaning |
| --- | --- |
| `CN-DSK-0001` | Webview missing or unsupported |
| `CN-DSK-0002` | Webview crashed and was reloaded |
| `CN-DSK-0003` | Workspace already open in another process |
| `CN-DSK-0004` | Disk full: edit is not saved |
| `CN-DSK-0005` | Mirror location unavailable |
| `CN-DSK-0006` | Mirror refused inside the sync root |
| `CN-DSK-0007` | Not enough disk for a download |
| `CN-DSK-0008` | Download hash mismatch |
| `CN-DSK-0009` | Download host or redirect refused |
| `CN-DSK-0010` | Not enough memory for this model |
| `CN-DSK-0011` | Local model keeps crashing |
| `CN-DSK-0012` | Engine stopped unexpectedly |
| `CN-DSK-0013` | Secure key storage unavailable |
| `CN-DSK-0014` | Update signature refused |
| `CN-DSK-0015` | File from the operating system refused |
| `CN-DSK-0016` | Location not supported for notebooks |

| Limit (proposal) | Value |
| --- | --- |
| Queued association events | 32 |
| Window close wait | 2 s |
| Model start timeout | 180 s |
| Model stop grace | 5 s |
| Model restarts | 3 in 10 min |
| Model idle stop | 10 min |
| Runtime log file | 16 MiB, rotated |
| Concurrent large downloads | 1 |

## Alternatives considered

| Alternative | Why not |
| --- | --- |
| Electron, Flutter | ADR-011. Weight, and reimplementing the web interface |
| Wasm engine in the webview | Keeps browser limits. OpenCalc rejected it for the same reason |
| Separate command surface, written for desktop | Two editors in practice. OpenCalc's `TAURI-002` shows the cost |
| `casual-note-tauri` crate | One consumer, and it can be extracted later |
| In-process llama.cpp through bindings | C++ inside the engine's process, so a model parser bug or GPU crash takes notes with it. ADR-016, `SPK-07` |
| Webview calls the model directly | Port and key in JavaScript, and in the CSP. A proxy through Rust costs a little and removes the path |
| Runtime downloaded on demand | Smaller installer, but an executable fetched at runtime needs its own signing and quarantine story. Can revisit after measuring size |
| Automatic update checks by default | Breaks PR-6's plain statement and air-gapped use |
| Native tabs | Differ per platform, no right-to-left consistency, not available everywhere |
| One process per window | More memory, and two owners of one log |
| Reading the mirror back | ADR-010. A second writer of truth |

## Acceptance gates

| Gate | Evidence required |
| --- | --- |
| `DSK-01` | CI bundles and launches on three platforms. `cargo check` of `desktop/` from its first day |
| No wasm path | `desktop/` runs the engine natively: a test asserts the wasm module is not loaded |
| One binding | Contract fixtures give the same results through `wasm.js` and `desktop.js` (QR-3) |
| Typing | `FND-24` report on all three webviews against QR-1 |
| Saved means saved | Kill the process right after "saved": the edit is there on relaunch (PR-1). `ENOSPC` injected: "saved" does not show |
| CSP | Policy in `tauri.conf.json`, gate fails on `null`, `unsafe-inline` or `unsafe-eval`. Injected script cannot reach an outside address, on each webview |
| Capabilities | Gate lists exposed commands and plugins. Seeded `fs` permission fails |
| Paths | Webview cannot name a path: fuzzed handles refused |
| Associations | Double-clicked package reaches preview, cold and warm, on each platform. Hostile fixtures from [36](36-EXPORT-FORMAT.md) show codes and change nothing |
| Mirror | Edited file is kept and offered, not overwritten. Deleting the mirror loses nothing. Titles in each launch script, on each platform |
| `DSK-02` | Resume after a kill. Hash mismatch refused. Licence and size appear first |
| `DSK-03` | Other-origin request refused. Port and key absent from webview, logs and process lists where feasible. Child dead after app kill, each platform |
| Silence | With AI, updates and sync off: network capture shows nothing (PR-6, QR-6) |
| `DSK-04` | Signed, notarised installers. Fresh install and upgrade from the last release. Tampered update refused |
| Windows | Written checklist passed on a real machine before Windows is called supported (`DEC-07`) |

## ADRs

- **ADR-011** (Accepted, inherited): Tauri.
- **ADR-007, ADR-025:** real DOM, plain ES modules. Both constrain the bridge.
- **ADR-010:** the mirror is one-way.
- **ADR-012, ADR-016, ADR-027:** local model and weights.
- **ADR-017:** package association.
- **ADR-018:** menus and messages in every language.
- **ADR-023:** Tauri's and every plugin's licence tree is to be confirmed, as [12](12-OPEN-SOURCE-AUDIT.md) did not audit transitive licences.

No new ADR is needed unless `FND-24` overturns the binding design.

## Tracker ids

`DSK-01`, `DSK-02`, `DSK-03`, `DSK-04`, `STO-04`, `WEB-01`, `WEB-02`, `SPK-02`, `SPK-03`, `SPK-07`, `DEC-01`, `DEC-07`, `DEC-10`, `DEC-12`, `A11Y-01`, `I18N-05`. Requirements: FR-22, PR-1, PR-6, QR-1, QR-6. Threats: SEC-W01, SEC-W02, SEC-W05, SEC-F11, SEC-L01 to SEC-L05, SEC-C03, SEC-C04. Performance: PERF-15. Risk: RISK-10. Proposed rows: `FND-22` (native adapter home), `FND-24` (typing path over IPC, and WebKitGTK and WKWebView input), `FND-25` (signing identities, cost, custody).

Related: [15](15-CI-AND-RELEASE-GATES.md), [20](20-ERROR-CODE-REGISTRY.md), [21](21-PARSER-LIMITS.md), [22](22-DATA-MODEL.md), [23](23-STORAGE-FORMAT.md), [24](24-OPERATIONS-AND-MERGE.md), [30](30-PERFORMANCE-TARGETS.md), [36](36-EXPORT-FORMAT.md), [37](37-IMPORT-FORMATS.md), [42](42-EDITOR-AND-RENDERING.md), [43](43-CANVAS-AND-INK.md), [47](47-UX-AND-FEATURE-MAP.md), [49](49-DESIGN-SYSTEM.md), [50](50-INTERNATIONALIZATION.md), [55](55-SDK-AND-EMBEDDING.md), [56](56-SYNC-DESIGN.md), [57](57-SERVER-DESIGN.md), [60](60-SEARCH-AND-EMBEDDINGS.md), [61](61-KNOWLEDGE-GRAPH.md), [62](62-AI-ASSISTANT-AND-PROVIDERS.md), [63](63-AGENT-TOOLS-AND-MCP.md), [07](07-SECURITY-THREAT-MODEL.md).
