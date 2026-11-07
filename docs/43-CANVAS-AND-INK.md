# 43 — Canvas and ink

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) shows the page and canvas switch, a note card, items, pan and zoom, a simple ink stroke, partial erase and pen-first touch handling. It has not been tried on a real pen device. Numbers are proposals, and browser behaviour here is **unverified** until `SPK-03`.

## Outcome

One page has two views over one content (ADR-020). A person switches between a flowing document and a free surface with one control, and nothing is lost in either direction. On the surface they can place notes, shapes, arrows and text, write with a pen that responds to pressure, highlight, erase part of a stroke, and annotate a PDF that is embedded in the workspace. Ink never bloats the CRDT (ADR-008), and it merges without conflict.

## Design

### Two views, one content

```
page
├── blocks[]            shown in both views
├── view                "page" or "canvas": the last chosen view
└── canvas
    ├── note            { x, y, width }: where the blocks sit on the surface
    ├── items[]         stickies, shapes, text, arrows, images, PDF pages
    └── strokes[]       ink records
```

Field names belong to [22](22-DATA-MODEL.md). The package form in [36](36-EXPORT-FORMAT.md) matches this, apart from the gaps listed below.

| Rule | Detail |
| --- | --- |
| Switching writes only `view` | One op, one undo step. Blocks, items and strokes are untouched, which is what makes `CNV-01` easy to prove |
| One editor | Blocks in canvas view are the same editor as in page view ([42](42-EDITOR-AND-RENDERING.md)), mounted inside the note card. Same input path, same composition rules, same undo stack |
| The camera is not content | Pan and zoom are per device, kept with host preferences. They are never synced and never exported. The prototype stores the camera on the page, and that does not carry over |
| Page view tells the truth | If the canvas holds items or strokes, the page view shows one plain line at its end, such as "3 items and 12 strokes on the canvas", with a button to switch. The prototype shows nothing, and content that a person cannot tell exists goes against the spirit of PR-1 |
| Search crosses views | A hit in an item opens canvas view centred on it. A hit in a block opens the page's last view ([60](60-SEARCH-AND-EMBEDDINGS.md)) |
| Export | Per-page Markdown and HTML hold the blocks and say that canvas detail was not included. PDF draws the chosen view. The package holds everything ([36](36-EXPORT-FORMAT.md)) |

**Ink attached to a block is out of scope.** An earlier idea was ink that moves with a paragraph. Strokes here live in canvas coordinates, and ink over the note card does not follow the text if it reflows. That limit is stated in the interface help. The strokes stay valid if anchoring is designed later.

### The surface

**Coordinates.** World space is in canvas units, y down, and **always left to right**. The world does not mirror in right-to-left interfaces, because a drawing has no reading direction. The tools around it do mirror: the dock and zoom control use logical properties. The prototype fixes them with `left` and `right`, and that must change ([50](50-INTERNATIONALIZATION.md)).

**Layers, bottom to top.**

| Layer | Holds | Drawn with |
| --- | --- | --- |
| Grid | Dots at 22 units, hidden when they would be closer than a proposed 8 pixels | CSS background on the surface, as in the prototype |
| Backgrounds | PDF pages and images | Bitmaps from a worker |
| Items and note card | The note card, stickies, shapes, text | **DOM**, in a transformed world element |
| Dry ink | Finished strokes, arrows, highlighter | `<canvas>`, viewport-sized |
| Wet ink | The stroke being drawn | A second `<canvas>`, viewport-sized |
| Tools | Dock, zoom control | DOM, outside the world |

Ink is above items, so a pen can annotate anything. Within the dry layer, highlighter strokes draw first and pen strokes after, so that highlighting never dulls writing.

**Items are DOM. This differs from the wording of ADR-007,** which says placed items are drawn on canvas. A sticky holds text, and everything that ADR-007 gives as a reason for DOM text (input methods, spell check, screen readers, right-to-left) applies to it. Drawing item text on canvas would need an engine text path with an overlay editor, for no gain at note-taking item counts. The prototype already does it this way. **ADR-007 needs rewording to "ink is on canvas; all text, including text on items, is in the DOM".** Arrows have no text, and they draw with ink.

**Culling.** Only items intersecting the viewport plus a margin are mounted. An item being edited is never unmounted. The Long page's virtual tiers ([42](42-EDITOR-AND-RENDERING.md)) apply inside the note card.

### Note card

The page's blocks in a card at `note.x`, `note.y`, with `note.width` in canvas units and height from content. A grip moves it, and an end handle resizes its width, which reflows the text. Its content follows the same reading measure rule as the page ([49](49-DESIGN-SYSTEM.md)). There is one per page. The card's text keeps its own direction and alignment.

With the select tool the pointer edits the text as usual. With any other tool, the world ignores pointers (the prototype's `.canvas.draw .world { pointer-events: none }`), so that a pen can write across the text without moving a caret.

Zoom uses a CSS transform on the world. Layer promotion happens only during a gesture and is released when it ends, so that the text re-rasterises sharply. Editing and input method candidates under a scale transform are in the `SPK-03` script.

### Items

| Kind | Fields beyond id, x, y, width, height | In package schema |
| --- | --- | --- |
| `sticky` | `color` (`amber`, `grey`), `text` | Yes |
| `rect`, `ellipse` | `text` | Yes |
| `text` | `text` | Yes |
| `arrow` | `from`, `to` | Yes, under `ink`. See below |
| `image` | Attachment hash | **No** |
| `pdf-page` | Attachment hash, page index | **No** |

- **Text is plain** in version 1, with a mini editor following the same composition rules. `dir="auto"` on each. Rich text on items is not planned here.
- **Colour names, not values.** `amber` resolves through `--sticky` per theme. A stored hex value would break dark mode. Foreign values from imported files are kept, shown as a neutral colour, and written back unchanged.
- **Gestures.** Move and resize are previewed locally and commit one op when the pointer lifts, so that dragging does not write per frame. Concurrent moves resolve by the CRDT's register rule, and the review marker is not raised for geometry ([24](24-OPERATIONS-AND-MERGE.md)).
- **Order** among items is a movable list, in the CRDT (`CNV-02`). Bring forward and send back are commands.
- **Arrows are items in the model,** because they are edited, not immutable like ink. The package schema keeps them in `ink` with `kind: "arrow"`, and the package writer and reader map between the two. Binding an arrow to an item is later work.
- **Sizes.** From the prototype: sticky 170 by 130, rectangle 170 by 80, ellipse 160 by 90, text 190 by 36, minimum width 100, minimum height 36. An arrow under 8 units long is discarded on release.

### Pan and zoom

Camera `{ x, y, z }`: `screen = world × z + (x, y)`. Zoom range 0.25 to 3, from the prototype, is a proposal.

| Input | Action |
| --- | --- |
| Wheel or trackpad scroll | Pan |
| Ctrl or Cmd with wheel, or trackpad pinch | Zoom around the pointer |
| Select tool, drag on empty surface | Pan |
| One finger, after a pen has been seen, or under select tool | Pan |
| Two fingers | Pan and zoom, always |
| Keyboard | Arrows pan when nothing is focused. Commands in [47](47-UX-AND-FEATURE-MAP.md) for zoom in, out, reset, fit |
| Zoom control | Minus, percent as `Intl.NumberFormat`, plus, Fit |

`touch-action: none` on the surface, so that all gestures are ours. Frame target PERF-11, on the Ink page workspace of 20,000 strokes, unmeasured.

### Ink pipeline

```
pointerdown  → route by pointer type (palm rules below), capture pointer, freeze the camera
pointerrawupdate or pointermove
             → getCoalescedEvents(): every sample since the last event
             → append to raw sample buffer: x, y, pressure, tilt, time
             → getPredictedEvents(): drawn on the wet layer only, never stored
requestAnimationFrame
             → -ink extends the outline for new samples; wet layer redraws its tail
pointerup    → final outline from all samples
             → ink.add op with the encoded samples, through the normal edit protocol
             → dry layer paints it; only then is the wet layer cleared
pointercancel → pen: commit what exists. Touch: discard
```

**Pointer events.** Pointer events on all hosts. `pointerrawupdate` where present, because it fires before the frame. `getCoalescedEvents` and `getPredictedEvents` are feature-detected, since support differs by browser and has changed over time. Without them, the code uses plain events and accepts lower quality. The prototype already calls coalesced events.

**Samples.** Stored as they arrived, in world units using the camera frozen at `pointerdown`. Exact duplicates are dropped, and nothing else. The prototype drops samples under one screen pixel, and that is a rendering decision which belongs in the outline step. Mouse and touch have no real pressure, so the pressure channel is left absent and the outline derives thinning from speed at draw time. The prototype stores simulated pressure. The reason for the change: the raw record should say only what the device reported.

**Wet ink.** `getContext("2d", { desynchronized: true })` where supported, which can skip the compositor and cut latency. Only the changed tail redraws, plus a discarded tail of predicted samples. Availability and tearing per device are `SPK-03` items.

**Dry ink.** Each stroke has a cached `Path2D` built from its outline, plus a spatial index from `-ink` to find the visible ones. Panning composites cached tiles of the dry layer, rendered per zoom bucket in an `OffscreenCanvas` worker where possible. At low zoom, a simplified outline is used. No flash between wet and dry: the wet layer clears only after the dry layer has painted the stroke.

**Where it runs.** The engine is off the main thread ([42](42-EDITOR-AND-RENDERING.md)), and a round trip per sample would ruin latency. `-ink` depends only on `-model` and `kurbo`, and it can load as its own small wasm module on the main thread, when the canvas first opens. That module is the one implementation for wet, dry and headless drawing, so that nothing shifts visually on pen up. [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) assumes a single `-wasm`, so this needs an ADR along with the render module.

**Rejected: SVG for ink,** as in the prototype. Every stroke is a DOM node, and 20,000 of them would cost too much on pan and zoom. **Rejected for now: WebGL.** Canvas 2D first. It returns if `SPK-03` or PERF-11 shows a need.

### Smoothing and outlines

Smoothing is **always derived, never stored.** The stroke record keeps raw samples, so that a better algorithm later improves old notes without any migration. Each record stores the outline algorithm's version, so that old notes do not change shape without a decision.

| | perfect-freehand | ink-stroke-modeler |
| --- | --- | --- |
| Licence | MIT, per [12](12-OPEN-SOURCE-AUDIT.md) | Apache-2.0 |
| Language | TypeScript. Existing Rust ports have little adoption, so we port it | C++. `ink-stroke-modeler-rs` is a partial Rust port, MIT OR Apache-2.0 on crates.io, licence files not yet confirmed |
| Gives | Pressure-varying outline polygon, with streamline smoothing | A physical model of the pen, for position smoothing and prediction |
| Does not give | Prediction, or a physical model | An outline |
| Size of the work | Small | Larger, with more to verify |

**Decision: port perfect-freehand into `-ink`** as tracker row `INK-01` says, keeping its licence notice and a `PROVENANCE.md`. The two are not really rivals: the modeler sits before an outline step. It is evaluated in `SPK-03` as an optional filter ahead of the outline, and adopted only if the owner can see the difference on a device, because a second dependency needs to earn its place. The prototype's `outline()` is a throwaway ribbon and is not a starting point. Golden outlines (`INK-01`) lock the behaviour. Output uses fixed rounding, so that native and wasm agree (QR-3).

### Immutable strokes

A stroke record is written once and never edited (ADR-008).

| Field | Meaning |
| --- | --- |
| `id` | Stable id |
| `kind` | `pen` or `highlighter` |
| `color` | Name: `ink`, `red`, `blue`. Resolved per theme through `--ink`, `--ink-red`, `--ink-blue`. The highlighter uses `--hi` |
| `width` | Base width in canvas units. 3 for pen, 18 for highlighter in the prototype |
| `algo` | Outline algorithm version |
| `samples` | Encoded bytes (next section). Fragments have none |
| `root`, `range` | Fragments only: the record holding the bytes, and the sample range this fragment shows |
| `parent`, `eraseId` | Fragments only: which record this came from, and in which erase |

**Merge.** Strokes are keys in a map in the page's CRDT document. Two devices drawing at the same time add two records, and there is nothing to merge. Samples are one binary value per record, never per-point CRDT values.

**Bytes in the document, not the blob store.** A stroke must arrive whole with its record, and strokes are small next to attachments, which are chunked and resumable ([23](23-STORAGE-FORMAT.md)). `SPK-03` and `SPK-01` check what this does to storage (PERF-05) on the Ink page workspace. If history grows too fast, large strokes move to the blob store by hash, without changing anything above this layer.

### Sample encoding

Struct of arrays, delta coded, in integers, canonical so that the same samples always give the same bytes (QR-3). This is a proposal, with no size measured.

```
magic    "CNI1"     4 bytes
flags    u8         bit 0 pressure, bit 1 tilt, bit 2 time
count    varuint    number of samples
x[]      first value zigzag varint, then zigzag varint deltas, in 1/100 canvas unit
y[]      same
p[]      if flag 0: value 0 to 1023, delta coded
tx[]     if flag 1: tilt x in whole degrees, -90 to 90, delta coded
ty[]     if flag 1: tilt y, same
dt[]     if flag 2: milliseconds since the previous sample, varuint. First is 0
```

Rules: no compression here, since the segment and snapshot layers compress ([23](23-STORAGE-FORMAT.md)). No absolute time, which reveals nothing useful and makes bytes depend on the clock. Altitude and azimuth from a browser convert to tilt x and y. A reader refuses non-minimal varints, trailing bytes and any unknown flag (`CN-INK-0001`). **A stroke's bytes are hostile input,** like any synced update (QR-4), and a fuzz target is required (`GOV-07`).

**Package form.** `points: [[x, y, pressure], ...]`, where x and y are the fixed value over 100, and pressure is the value over 1023 rounded to 4 places. Absent pressure writes a value derived from speed, as a plain number, because the schema requires one. This round trip is exact for x, y and pressure, so export, import, export stays byte-identical. **Tilt and time are not in package version 1,** so they would be lost by leaving and coming back. That is a gap in [36](36-EXPORT-FORMAT.md): an optional field is allowed by its versioning rule, and a row is proposed.

### Partial erase

Erasing part of a stroke creates new strokes from what is left. Usually two (a cut through the middle), one if the erase took an end, more if the eraser crossed it several times. The original record's bytes do not change.

```
stroke S, samples 0..100, eraser crosses 40..60

  add record A  { root: S, range: 0..40,   parent: S, eraseId: e1 }
  add record B  { root: S, range: 60..100, parent: S, eraseId: e1 }
  add erasure e1 { parent: S }
```

| Property | How |
| --- | --- |
| Immutable | S is unchanged. Records are added |
| No copied samples | Fragments point into the root's bytes. Erased samples stay in the file until compaction, at which point bytes no fragment refers to can go |
| One undo step | An erase gesture is one batch, committed on pointer up, with live preview. Undo removes `e1` and its fragments, and S is visible again |
| Radius | In screen pixels, divided by zoom. 11 in the prototype. Cuts fall at sample boundaries, and a very sparse stroke cuts slightly wider |
| Whole-stroke erase | A setting. An erasure with no fragments |
| Export | Writes visible samples only. **Erased ink does not leave in a package** |

**Erase is not secure deletion.** Erased samples remain in history until trimmed, like any deleted text. The interface must not suggest otherwise.

**Concurrent erases.** With plain "replace one stroke with two", device A erasing the middle and device B erasing the end would each keep what the other erased, and erased ink would return. Instead, visibility is a pure projection (like `MOD-02`, never written back):

```
visible(record) =
    if no live erasure of this record: its own range
    else: the intersection, over each live erasure e of this record,
          of the union of visible(child) for the children in e
```

So A's `0..40, 60..100` and B's `0..80` show `0..40` and `60..80`, the same on every device. Rendering sees plain fragments. **This needs agreement with [24](24-OPERATIONS-AND-MERGE.md)** and a row is proposed. The simple form remains the fallback. It loses nothing, but it can bring back erased ink.

### Palm rejection

Rules run per pointer at `pointerdown`.

| State | Pen | Touch | Mouse |
| --- | --- | --- | --- |
| No pen ever seen on this device | Draws. Sets "pen seen" | Draws with a drawing tool, pans under select | As tool |
| **Pen seen** | Draws | **Pans.** One finger pans, two pan and zoom, and touch never draws | As tool |
| Pen in contact or just lifted | Draws | Ignored for a grace period, proposed 500 ms | As tool |
| Touch stroke in progress when a pen arrives | Draws | That stroke is discarded | |

"Pen seen" is remembered per device in host preferences, never synced. A setting ("Draw with finger") overrides it. The prototype ignores touch after a pen has been seen, and does not pan: **product behaviour differs from it on purpose**, because the owner's rule is that touch pans. Contact size (`width` and `height` on the event) may help reject a palm that arrives first, and `SPK-03` decides whether it is reliable. [12](12-OPEN-SOURCE-AUDIT.md) notes that AFFiNE's palm rejection has been reported as poor, so this deserves real testing.

### Highlighter

Flat width, no pressure in the outline. Pressure is still stored if reported. Colour `--hi`, composited at 0.55 opacity with `--hi-blend` (`multiply` in light, `screen` in dark). The whole stroke fills once into an offscreen buffer and is then composited, so that a stroke crossing itself does not darken. Separate overlapping strokes do build up, as a real one does. In PDF output, a `Blend` display-list item ([42](42-EDITOR-AND-RENDERING.md)) maps to the PDF blend mode.

### PDF as background

**Embedded, never linked** (`INK-04`). This settles SEC-F12.

**Importing.** The file goes through the attachment store by content hash ([23](23-STORAGE-FORMAT.md), `FIL-01`). Each page becomes a `pdf-page` item, stacked vertically with a gap, at the page's size in points. Pages lock their aspect ratio. Ink goes on top in canvas coordinates, with no reference to the PDF.

**Viewing.** PDF.js (Apache-2.0, **not yet in [12](12-OPEN-SOURCE-AUDIT.md)**) in its own dedicated worker, on web and desktop alike, configured with eval off, annotations and scripts off, no fetching of any external resource, and streaming off. **Only bitmaps cross to the page,** and no PDF.js DOM layer is used. A bitmap is drawn for the visible pages at the current zoom bucket, with a bounded cache. A page that fails or times out shows a placeholder naming the page (`CN-INK-0007`), and the ink over it remains.

| Option considered | Result |
| --- | --- |
| Browser's built-in viewer in an `iframe` | Cannot sit under an ink layer in all browsers, and WKWebView differs. Rejected |
| A Rust parser in the engine | Large attack surface in the engine. Rejected for viewing |
| PDF.js in a worker | **Chosen.** Its parser runs inside the browser sandbox, in a worker with no DOM, under the page's CSP ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)) |

**Searching.** Text extracted by the same worker goes to search as derived data ([60](60-SEARCH-AND-EMBEDDINGS.md)). Selectable PDF text is later work. Scanned pages have no text, and OCR is `OCR-01`.

**Exporting.** A package holds the PDF in `assets/`. A PDF export should keep original pages as vectors under the ink, which needs a PDF page-copy library in Rust (candidates to audit: `lopdf`, `pdf-writer`, **none audited**). The fallback is pages rasterised by the host's worker, at a stated resolution, and the export report says so. On a server with no browser, the fallback is unavailable, and the report says that too.

**Package gap.** The schema's `canvasItem.kind` is a closed enum, without `pdf-page` or `image`. [36](36-EXPORT-FORMAT.md) only allows optional fields in a minor version, so these must land **before version 1.0 is frozen**. Row proposed.

### Accessibility

Ink has no text equivalent, and handwriting is not recognised, so the canvas states a count ("12 strokes") and does not pretend otherwise. Items are reachable by keyboard from the dock, in a list ordered by position, with each one's text and kind announced. Arrow keys move a selected item, and every tool is a registered command. All of this is tested under `A11Y-01`.

### Handwriting recognition

**Deferred** (`HWR-01`). No good permissive on-device option was found ([12](12-OPEN-SOURCE-AUDIT.md)). Raw samples are kept so it can be added later. Interface copy and search say plainly that handwriting is not searchable. Re-survey before starting.

## Failure modes and limits

Codes for [20](20-ERROR-CODE-REGISTRY.md), limits for [21](21-PARSER-LIMITS.md). Values are proposals.

| Code | Refuse or report when | Result |
| --- | --- | --- |
| `CN-INK-0001` | Sample bytes malformed: bad magic, unknown flag, truncated, non-minimal varint, trailing bytes | Stroke skipped, named in the report. Never a panic |
| `CN-INK-0002` | Sample count disagrees with the data, or exceeds the limit | Same |
| `CN-INK-0003` | A coordinate is non-finite or out of range | Op refused locally. Skipped from remote, and reported |
| `CN-INK-0004` | Fragment range out of bounds, unknown root, or a `parent` cycle | Fragment not drawn, and reported. Projection stays total |
| `CN-INK-0005` | A file offered as a PDF is not one, or exceeds limits | Import refused with the reason |
| `CN-INK-0006` | PDF is encrypted | Refused in version 1, with the reason |
| `CN-INK-0007` | A PDF page failed or timed out in the worker | Placeholder, with ink kept |

| Limit | Proposed value | Beyond it |
| --- | --- | --- |
| Samples per stroke | 65,536 | The stroke ends and a new one starts in the same gesture, invisibly. Nothing is lost |
| Sample bytes per stroke | 1 MiB | Refused from outside (`CN-INK-0002`) |
| Fragments from one erase of one record | 1,024 | Whole-stroke erase is used instead, with a message |
| Erasure depth along `parent` | 64 | Compacted into one level at the next snapshot |
| Canvas coordinate | ±10^9 units, as [22](22-DATA-MODEL.md) | `CN-INK-0003` |
| PDF file | 2 GiB, the package's per-entry limit | `CN-INK-0005` |
| PDF pages | 10,000 | `CN-INK-0005` |
| PDF page render | 30 seconds in the worker | `CN-INK-0007` |

**Known limits.** No handwriting search. Ink over a note card does not follow reflowed text. Erased ink stays in history until trimmed. Web pen latency depends on the browser and the device, and it may simply be worse than native. Pen quality has no number until `SPK-03` (PERF-12).

## Alternatives considered

| Option | Why not |
| --- | --- |
| Per-point CRDT values | Grows without limit, for no benefit since a stroke is never edited by two people. ADR-008. Rnote and Saber report slow or huge files ([12](12-OPEN-SOURCE-AUDIT.md)) |
| Erased ranges as a mutable set on one stroke | Merges neatly, and is close to what the projection achieves, but breaks "immutable records" and makes each renderer interpret it. Fragments let everything below the projection see plain strokes |
| Excalidraw as the canvas | MIT, but requires React, against ADR-025. Its flat element list and per-element version were read as ideas |
| tldraw | Proprietary licence. Ideas only |
| AFFiNE's canvas code | The MIT parts are allowed, but they bring a different data model. Studied from docs only. Behaviour not tried |
| Rnote, Xournal++, Saber | GPL. Ideas only. Xournal++'s linked PDFs are the warning behind "embedded, never linked" |
| Separate canvas page type | Against ADR-020. The switch is the product's point |

## SPK-03: pen in a browser and webview

**Blocked on `DEC-05`, the pen devices available.** Decides ADR-008. Must run on **real devices**: a simulator or a mouse proves nothing here. The owner's Mac has no pen (RISK-11).

| Device, if available | Runs in | Why |
| --- | --- | --- |
| iPad with Apple Pencil | Safari, and WKWebView if a shell is planned | The strongest case, as `DEC-05` notes |
| Windows stylus device | Chrome, Edge, Firefox, WebView2 | Desktop app's webview. Also needs `DEC-07` |
| Android stylus tablet | Chrome | Second tablet platform |

**Harness.** The real pipeline, `-ink` module, wet and dry layers, on the Ink page workspace.

**Recorded on each device.** Sample rate and coalesced count. Whether `pointerrawupdate`, `getPredictedEvents` and `desynchronized` exist and work. Pressure and tilt ranges reported. Palm rejection: a written script of strokes with a palm resting, count of false marks and missed strokes. A slow-motion video of the pen tip against the line, for an honest latency view. Stored bytes per stroke against PERF-05, and pan and zoom frame rate (PERF-11). The modeler filter on and off. Input method and caret inside the zoomed note card.

**Pass.** The owner judges writing a page of notes and annotating a PDF on the real device: latency and palm rejection are acceptable, and the storage result supports ADR-008. No latency number is set beforehand (PERF-12). The measurements become the numbers afterwards.

**Fail.** A native shell for the pen surface on the failing platform, with `MOB-02` as its route, and the web pen labelled as limited there. Storage failing alone moves stroke bytes to the blob store.

Phase 3 cannot open without this (`INK-02` is blocked). `INK-01` and `INK-03` are pure algorithm work that can proceed before it.

## Acceptance gates

| Gate | Proves | Tracker |
| --- | --- | --- |
| Generated pages: switching views any number of times, then export, gives identical bytes apart from `view` | Nothing lost | `CNV-01` |
| Merge tests: concurrent add, move, delete of items | Items merge | `CNV-02` |
| Golden outlines on native and wasm | `-ink` is deterministic | `INK-01` |
| Encoding round trip on generated samples; bytes are canonical; fuzz target finds no panic | Encoding | `INK-01`, `GOV-07` |
| Package round trip with ink gives identical bytes | Package form | `PKG-05` |
| Merge tests: concurrent partial erases give the same visible ranges on all replicas, with erased ink staying erased | Erase | `INK-03` |
| Undo restores the stroke after a partial erase, including after a merge | Undo | `INK-03`, `OPS-02` |
| Scripted touch after pen pans and never draws. Owner checks on a device | Palm rules | `INK-02` |
| Hostile PDF fixtures: no script, no network request, no main-thread crash | SEC-F12 | `INK-04` |
| Package with a PDF contains the PDF, with no link | Embedding | `INK-04` |
| PDF and PNG of a canvas page in each launch script, with ink | Headless output | `RND-01` |
| PERF-11 on the Ink page, against a committed baseline | Frame rate | `CNV-01` |

## ADRs

ADR-007 (needs rewording, see above), ADR-008 (settled by `SPK-03`), ADR-020, ADR-023 (only ideas from GPL ink apps), ADR-027 (no recognition model is bundled). **Proposed, not yet in [08](08-ADR-REGISTER.md):** a separate `-ink` wasm module; PDF.js in a worker as the PDF path.

## Tracker ids

`SPK-03`, `DEC-05`, `DEC-07`, `CNV-01`, `CNV-02`, `INK-01`, `INK-02`, `INK-03`, `INK-04`, `RND-01`, `FIL-01`, `GOV-07`, `OPS-02`, `A11Y-01`, `PKG-05`, `OCR-01`, `HWR-01`, `MOB-02`, `MOD-02`, and `RISK-11` in [06](06-ROADMAP-AND-DELIVERY.md).
