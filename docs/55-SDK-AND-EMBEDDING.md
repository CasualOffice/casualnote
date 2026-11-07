# 55 — SDK and embedding

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) has no SDK and is no stand-in for one. OpenCalc's SDK is the reference for its package layout and its lessons. Its code has not been copied, and nothing here has been tried in a host's build. `SDK-01` is Phase 1 and `SDK-02` is Phase 10.

## Outcome

A developer can add a workspace to their own product: a Rust crate on a server or in a native app, or an npm package and one HTML element in a web page. They can read it, drive it and listen to it through a small, stable surface, without taking on the engine's internals.

Requirements: FR-23. Principle 8 in [02](02-ARCHITECTURE.md): public surfaces are narrower than internals and versioned independently. Audience: developers embedding a workspace ([01](01-ORD.md)).

**Single user first.** Version 1 embeds one person's workspace in one browser or process. There are no accounts, no permissions and no co-editing through this surface. The shape below leaves room for them (see Multi-user, later).

## Design

### The layers

```
Host's page                       Host's Rust program
<casual-note-page>  (element)     │
   @casualnote/engine (JS API)    │
   casual-note-wasm               casual-note-sdk
          └────────── engine crates ──┘
```

`-sdk` and `-wasm` have **no logic of their own** ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)). A rule that lives in one but not the other is a bug. Every SDK method maps to one engine call. The test suite exercises both with the same scenarios (`WEB-01`).

### Rust surface: `casual-note-sdk`

Used by tests from Phase 1 (`SDK-01`), then by the desktop shell, the server and `-tools`.

```rust
pub struct Workspace { /* opaque */ }

impl Workspace {
    pub fn open(host: Host, opts: OpenOptions) -> Result<Workspace, SdkError>;
    pub fn create(host: Host, opts: CreateOptions) -> Result<Workspace, SdkError>;
    pub fn close(self) -> Result<(), SdkError>;

    // Read
    pub fn pages(&self, cursor: Option<Cursor>, limit: u32) -> Result<PageList, SdkError>;
    pub fn page(&self, id: PageId) -> Result<PageView, SdkError>;
    pub fn database(&self, id: DatabaseId) -> Result<DatabaseView, SdkError>;

    // Write: only through operations
    pub fn apply(&mut self, batch: OpBatch, origin: Origin) -> Result<Applied, SdkError>;
    pub fn undo(&mut self) -> Result<Option<Applied>, SdkError>;
    pub fn redo(&mut self) -> Result<Option<Applied>, SdkError>;

    // Derived
    pub fn search(&mut self, query: &Query, opts: &SearchOpts) -> Result<SearchResults, SdkError>;
    pub fn related_pages(&self, page: PageId, k: u32) -> Result<Vec<Related>, SdkError>;
    pub fn graph(&self, filter: &GraphFilter) -> Result<Graph, SdkError>;

    // Formats
    pub fn export_package(&self, scope: Scope, out: &mut dyn Write) -> Result<ExportReport, SdkError>;
    pub fn import(&mut self, source: &mut dyn ImportSource, opts: ImportOptions) -> Result<ImportPreview, SdkError>;

    // Change feed
    pub fn drain_events(&mut self) -> Vec<Event>;

    // Versions
    pub fn versions() -> Versions;
}
```

**Host seams** are passed in as one value, one per capability, so the engine has no platform code ([02](02-ARCHITECTURE.md) principle 5):

| Seam | Capability |
| --- | --- |
| `io` | Durable log, snapshots, blobs and derived files ([23](23-STORAGE-FORMAT.md), `STO-03`) |
| `clock` | Time, for timestamps only |
| `random` | Ids and keys. Tests inject a seed |
| `embedder` | Optional. The pull interface in [60](60-SEARCH-AND-EMBEDDINGS.md) |

**What is not there.** AI providers, keys and network: those stay with the host ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)). `-tools` sits above this surface.

**Rules for the surface.**

- No type from `loro` or any other third-party crate appears here. `-merge` keeps that secret (ADR-006).
- Ids are opaque strings at the edge, with typed wrappers in Rust.
- Public enums and structs are `#[non_exhaustive]`, so adding a variant or field is not a break.
- Errors carry the registered code: `SdkError { code: "CN-…", message, params }` ([20](20-ERROR-CODE-REGISTRY.md)).
- No panics reach the host. A contained panic returns an error.
- Long work is done in steps, as the [60](60-SEARCH-AND-EMBEDDINGS.md) pattern: the caller gives a budget. There is no hidden thread, because threads are a host decision.

**Operations.** `OpBatch` is the published form of the op set ([24](24-OPERATIONS-AND-MERGE.md)), and has its own schema version. Helpers build common batches. Hosts are not given raw CRDT access.

### Wasm bridge: `casual-note-wasm`

`wasm-bindgen`, exposing the same calls.

- **The engine runs in a dedicated worker,** with the storage I/O that OPFS requires in a worker. The JS API is thus promise-based, and the worker boundary is hidden. Where exactly the typing path sits is decided in `SPK-02` ([42](42-EDITOR-AND-RENDERING.md)).
- **Values cross as structured data.** Handles for long-lived things, with an explicit `free()`, and `FinalizationRegistry` only as a safety net.
- **Errors** become JS `Error` objects with `.code` and `.params`.
- **Bytes** cross as transferable `ArrayBuffer`s, so an export does not copy twice.
- **The host's CSP** needs `script-src 'wasm-unsafe-eval'` or the equivalent. The README and the startup error say exactly what is missing.

### npm packages

OpenCalc's split is the starting point, since its reasoning carries over. The `@casualnote` scope is not checked: `casualnote` itself was free on npm on 2026-11-07 (`DEC-01`). All names wait on `DEC-01`.

| Package | Holds | Depends on |
| --- | --- | --- |
| `@casualnote/engine` | Wasm, worker, JS API, type declarations. No DOM | Nothing |
| `@casualnote/element` | The custom element, styles and message catalogues | `engine` |
| `@casualnote/react` | A thin wrapper, once somebody needs it | `element`, with React as a peer |

**Shipping wasm into someone else's build is the hard part,** as OpenCalc's SDK design found. In order of preference:

1. **Default.** Resolve the `.wasm` and worker from `import.meta.url`, which works in bundlers that understand that pattern and in plain ES modules.
2. **Explicit.** `init({ wasmUrl, workerUrl })`, for a host that copies the assets itself or serves them under its own rules.
3. **No third-party CDN by default.** The real product loads nothing from a CDN (SEC-C03), and neither does this.

All ES modules, with no required build step, in keeping with ADR-025. Examples run in CI for plain HTML and at least one bundler (`SDK-02`). The size budget is PERF-14's, and unmeasured.

### JS API

```js
import { open } from "@casualnote/engine";

const ws = await open({
  storage: "opfs",             // "opfs" | "memory" | a custom object for the io seam
  name: "my-product-notes",    // separate per name
});

const page = await ws.page(id);
await ws.apply(batch, { source: "host" });
const results = await ws.search("导入");
const bytes = await ws.exportPackage({ scope: "workspace" });
const unlisten = ws.on("change", e => { /* e.pages, e.source */ });
await ws.close();
```

### Embedding element

```html
<script type="module" src="/vendor/casualnote/element.js"></script>
<casual-note-page workspace="my-product-notes" page="p-launch" view="page" locale="ar"></casual-note-page>
```

| Attribute | Values |
| --- | --- |
| `workspace` | Storage name, or set `.workspace` to an object from `open()` |
| `page` | Page id |
| `view` | `page` or `canvas` (ADR-020) |
| `mode` | `edit` or `read-only` |
| `locale` | BCP 47. Default follows the page, then the browser ([50](50-INTERNATIONALIZATION.md)) |
| `theme` | `light`, `dark` or `auto` |
| `chrome` | `full`, `minimal` or `none`: sidebar, toolbar, slash menu |

**Mounting.** OpenCalc offers shadow DOM and an iframe behind one API, and its lessons apply: fixed overlays and `100vh` behave differently inside a shadow root, and per-instance style blocks multiply. **CasualNote has a further reason for caution:** its text is in the real DOM (ADR-007), so selection, input methods, spell check and screen readers all cross this boundary. Selection in shadow roots is known from memory to vary across browsers, and that is unverified here. So:

- **`SPK-02` must include the editor inside a shadow root** on Chrome, Safari and Firefox, with input methods. Proposed row `FND-74`.
- **The iframe mount** gives a real boundary, at a cost in integration, and goes through the same API with `postMessage`. Its envelope has a `ready` handshake advertising versions and methods, and a strict origin check, following OpenCalc.
- **The default mount** waits for that result. Do not guess.

**Theming.** Set on the host element, as CSS custom properties with typed suffixes from [49](49-DESIGN-SYSTEM.md): `--cn-accent-color`, `--cn-spacing`. Custom properties cross a shadow boundary. **Logical CSS only,** so right-to-left works inside any host ([50](50-INTERNATIONALIZATION.md)).

**Languages.** Message catalogues load on demand from the package, and a host may override strings. The direction of each block comes from the text, not from the host's page.

**Several elements.** Two elements on one workspace name share one engine worker, and with it one undo stack per view, specified in `SDK-02`. A second tab of the same origin and name is the same multi-tab question as the app ([23](23-STORAGE-FORMAT.md)).

### Events

Names are stable and part of the API. Payloads hold ids and facts, and **no note text by default,** so a host's logging doesn't collect notes by accident. Fetching content is a deliberate call.

| Event | Fires when | Payload | Cancelable |
| --- | --- | --- | --- |
| `ready` | Engine and element loaded | `versions`, `capabilities` | No |
| `before-apply` | Before an op batch from the user | `batch summary`, `source` | **Yes** |
| `change` | Changes committed in memory | `pages`, `source`, `batchId` | No |
| `saved` | Durable (PR-1) | `upTo` | No |
| `selection` | Selection or current block changed | `page`, `block`, `collapsed` | No |
| `navigate` | User wants another page or a URL | `kind`, `target` | **Yes:** host may route it |
| `before-export` | Export asked for | `scope` | **Yes** |
| `index-state` | Search or vectors progress | State from [60](60-SEARCH-AND-EMBEDDINGS.md) | No |
| `error` | Anything failed | `code`, `message`, `params` | No |

**`source`** is on every change: `user`, `host`, `tool`, `import`, `remote`, `undo`. Without it a host echoing its own writes into its store loops forever. **`before-apply`** is the host's permission hook, and stays narrow: it can refuse, not rewrite. Veto runs before the op batch and never delays the durable write of allowed edits beyond the host's own handler, which must be synchronous and short. PR-4 holds only if host handlers behave, which is documented, and the benchmark runs with a no-op handler.

**In Rust,** `drain_events` returns the same events, with no callbacks, so nothing reenters the engine.

### What the host gets, and what it does not

| Capability | Default | How to change |
| --- | --- | --- |
| Assistant panel | **Off** | Host passes a provider object meeting [62](62-AI-ASSISTANT-AND-PROVIDERS.md). Keys are the host's responsibility |
| Search by meaning | Off | Host supplies an embedder, or allows the download |
| Sync | Off | Comes with `FR-19` work. Not in the first SDK |
| Network | **None** | The engine and element fetch only their own assets |
| Export | On | Host can veto with `before-export`, or hide it |
| Plugins | None | Out of scope until a sandbox exists ([01](01-ORD.md)) |

**Security for embedders.** Document strings reach the DOM only as text, whatever the host does (QR-7, SEC-W01). An element inside a host's page shares its origin, so **a compromised host page can read the notes,** and the docs say so. A host that needs a real boundary uses the iframe mount on a separate origin. **Browsers partition storage in third-party iframes,** so a cross-origin iframe may not see the same workspace, or keep it, across top-level sites. The exact behaviour is unverified and belongs in `SDK-02` tests. The minimum CSP is documented for hosts, with an example.

**Storage is the host's data.** `opfs` persists per origin. A host that wants a server calls `exportPackage` or supplies an `io` object. Our sync protocol is not part of this surface yet.

### Versioning

Each versioned thing is separate ([02](02-ARCHITECTURE.md)). `versions()` reports them all, and `ready` includes them.

| Version | Governs | Scheme |
| --- | --- | --- |
| `sdk` | Rust and JS API | Semver. npm and crates.io move together |
| `operations` | `OpBatch` schema | Integer ([24](24-OPERATIONS-AND-MERGE.md)) |
| `storage` | Live notebook | Integer ([23](23-STORAGE-FORMAT.md)) |
| `package` | Export format | `major.minor` ([36](36-EXPORT-FORMAT.md)) |
| `sync` | Sync protocol | [56](56-SYNC-DESIGN.md) |
| `toolSet` | Agent tools | [63](63-AGENT-TOOLS-AND-MCP.md) |

**Skew.** The wasm and its JS wrapper are built together and refuse to start if they differ. That error needs a registered code, and no `CN-SDK` prefix exists yet, so [20](20-ERROR-CODE-REGISTRY.md) must add one (proposed row `FND-75`). A notebook newer than the SDK refuses to open and is not changed. An older one is upgraded only with the host's consent, since that may be one-way: `OpenOptions.allow_upgrade`, default false in the SDK. Export remains the way out.

### Stability policy

All proposals, for owner review.

| Tier | Promise | Examples |
| --- | --- | --- |
| **Stable** | No breaking change without a major version | Methods and events documented without a tag, after 1.0 |
| **Preview** | May change in a minor version, with a changelog entry. Marked in docs and types | Anything new, the graph API at first |
| **Internal** | No promise. Not exported where the language allows it, otherwise marked `_` | CSS class names, DOM structure, worker messages, the iframe wire format except its handshake |

- **Before 1.0,** published as `0.x`, any minor version may break, with a changelog entry and a migration note. **1.0 needs a real embedder's use,** not just our own examples.
- **After 1.0,** something deprecated stays for at least one major version and at least six months, warns once per session, and carries its replacement in the docs.
- **Data outlives API.** Readers for package formats are kept permanently ([36](36-EXPORT-FORMAT.md)), whatever the SDK does.
- **Enforced.** CI diffs the public Rust API and `.d.ts` files against a committed snapshot. A difference in a stable item without a major bump fails (`SDK-02`, [15](15-CI-AND-RELEASE-GATES.md)).
- **Dependencies are not re-exported,** so a Loro upgrade can't be a breaking change.
- **Licence.** Apache-2.0, the same as everything else (ADR-002). The name is not granted (`DEC-01`).

### Multi-user, later

Not designed here, and only protected from being blocked:

- Every batch already carries an `Origin`, which can later name an actor.
- `before-apply` is where a local permission hook would sit. Real enforcement belongs on the server ([57](57-SERVER-DESIGN.md), `SRV-03`).
- An `identity` option and presence events would be added as Preview.
- No API takes a "current user" today, so nothing has to be unwound.

## Failure modes and limits

| Failure | What happens |
| --- | --- |
| Wasm or worker fails to load | `open()` rejects, naming the URL tried and a CSP hint. The element shows a plain localised message |
| Missing OPFS or a blocked storage call | `open()` rejects with the store's code. `memory` is only used if the host chose it: **never a silent fallback that would lose work** |
| Storage quota exceeded | Edits stop being acknowledged as saved. `error` fires, and "saved" never lies (PR-1) |
| Version skew | Refuses to start, with both versions named |
| Host's `before-apply` handler throws | Treated as no veto, `error` fires. A security-minded host should wrap it. **Documented,** with the alternative `onHandlerError: "refuse"` |
| Host's handler is slow | A warning in development builds. PR-4 is not guaranteed in that case |
| Element removed from the DOM | Listeners released, and the engine stays until `close()` if the host owns it |
| Host's CSS leaks in, without a boundary | Light DOM is not offered for this reason. See Mounting |
| Two tabs on one workspace | [23](23-STORAGE-FORMAT.md) decides. The SDK surfaces it and does not hide it |

Limits come from [21](21-PARSER-LIMITS.md), and the SDK has none of its own. All engine errors keep their registered codes ([20](20-ERROR-CODE-REGISTRY.md)).

## Alternatives considered

| Option | Decision | Why |
| --- | --- | --- |
| A framework component first | Rejected | ADR-025, and a custom element serves all frameworks |
| Exposing the CRDT to hosts | Rejected | Ties them to Loro, against ADR-005 and ADR-006 |
| Callbacks in the Rust API | Rejected | Reentrancy risk. Draining events is simple |
| Hidden threads in the Rust SDK | Rejected | A host owns policy, and the budget model works on both targets |
| Bundling provider code in the element | Rejected | Off by default, and keys belong to the host |
| Inlining wasm as base64 | Deferred | OpenCalc offers it. Costs size, and only worth adding if a host needs it |
| Light DOM mount | Rejected | Host CSS would reach the editor, and vice versa |
| A hosted embed through a script tag from our servers | Rejected | `DEC-03`, SEC-C03 |

## Acceptance gates

| Gate | Row |
| --- | --- |
| `-sdk` is used by the engine's tests | `SDK-01` |
| The same scenario set gives the same results through `-sdk` and `-wasm` | `WEB-01`, QR-3 |
| No third-party type in the public Rust API, by a check on the API snapshot | `SDK-01`, `GOV-04` |
| Examples build and run in CI: plain HTML with no bundler, and one bundler | `SDK-02` |
| Public API diff fails on an unmarked break | `SDK-02` |
| Every event has a test, including `source` and veto | `SDK-02` |
| Keystroke target PERF-01 holds with the element in a host page and a no-op `before-apply` | `SDK-02` |
| Input methods, screen reader and right-to-left pass in each mount that ships | `SDK-02`, `SPK-02` |
| A hostile note fixture inserts no element in the host's page | `SDK-02`, SEC-W01 |
| Without opt-in, a scripted embed makes no network request beyond its own assets | `SDK-02`, QR-6 |
| Version skew is refused | `SDK-02` |

## ADRs

ADR-002, ADR-003, ADR-005, ADR-006, ADR-007, ADR-012, ADR-020, ADR-025. A 1.0 stability commitment needs its own ADR.

## Tracker ids

`SDK-01`, `SDK-02`, `WEB-01`, `SPK-02`, `GOV-04`, `DEC-01`, `DEC-03`. Proposed: `FND-74`, `FND-75`.
