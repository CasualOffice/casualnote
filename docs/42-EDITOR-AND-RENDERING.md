# 42 — Editor and rendering

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) shows the look and the interaction, but it is not op-driven (see below). Every number is a proposal. Statements about browser behaviour come from specifications and prior knowledge, are **unverified** here, and are what `SPK-02` exists to check.

## Outcome

A person types in any language on a page of any length, in Chrome, Safari or Firefox and in the desktop webview. Input methods, spell check, right-to-left text, native selection and screen readers work the way they do in a good native text field. The engine stays the only source of truth (ADR-007): the DOM is a projection, every change is an op, and no string from a document reaches the DOM as HTML (QR-7). The same content also prints to PDF and PNG with no browser (`RND-01`).

## What the prototype proves, and what it does not

| Prototype behaviour | Product |
| --- | --- |
| One `contenteditable` per block, with `role="textbox"` | Page-level editing host is the first candidate. See "The editing host" |
| `input` handler reads runs back from the DOM with `readRuns` | Reversed. `beforeinput` produces ops, and the DOM follows |
| Whole-document snapshots for undo, at most 80, with a 700 ms grouping window | Inverses in `-ops` (`OPS-02`) |
| `rerenderDoc` rebuilds the page on block changes | Keyed patching per block |
| `execCommand` for bold, italic and code-block newlines | Removed. No `execCommand` anywhere |
| Paste reads `text/plain` and collapses newlines | Full clipboard pipeline below |
| Runs triggers and Markdown shortcuts on every `input`, including during composition | Evaluated only on committed text |

So the prototype says nothing about `SPK-02`. It has never run on a long page or under a screen reader.

## Design

### Parts of the web editor

All under `webapp/editor/`, plain ES modules (ADR-025). The same code runs in the desktop webview.

| Module | Owns | Must never |
| --- | --- | --- |
| `engine-client.js` | The async interface to the engine: `apply(batch)`, events `ack`, `durable`, `projection` | Hold document state |
| `view-model.js` | A disposable cache of the projection: per block, `version`, `type`, attributes, runs | Be persisted, or be read by anything outside the editor |
| `input.js` | `beforeinput`, keydown and composition handling, intent to op batch | Touch the DOM directly |
| `predict.js` | Pure function: view-model plus op gives next view-model, for the small op subset below | Decide validity |
| `patch.js` | The only module that writes to the editing host | Use `innerHTML`, or touch a composing block |
| `selection.js` | DOM selection to and from model positions | Set the selection during composition |
| `virtual.js` | The virtualisation tiers | Unmount a block holding the anchor, focus or composition |
| `clipboard.js` | Copy, cut, paste, drop | Insert parsed nodes into the live document |
| `blocks/*.js` | DOM shape per block type, including islands such as the database | Write to the model except through ops |

### The edit protocol

The engine does not run on the UI thread. On the web it lives in a dedicated worker, because OPFS synchronous handles are only available there. On desktop it is native, behind Tauri commands (ADR-011, [44](44-DESKTOP-SHELL.md)). Both are asynchronous, so a `beforeinput` handler can never wait for the engine. The protocol is therefore asynchronous on every host.

```
beforeinput
  → input.js builds an op batch with clientSeq n and the selection before and after
  → predict.js updates the view-model          (only for the predictable subset)
  → patch.js updates the DOM, same task        ← the paint that QR-1 measures
  → engine-client posts the batch
       engine: -ops validates, records inverse → -merge writes
             → ack(n, revision, patches)
             → -store appends; group commit, fsync
             → durable(upToSeq n)              ← status shows "Saved on this device"
  → on ack: compare prediction with the projection. Equal: nothing to paint.
            Different: patch to the projection, count the mismatch
```

**Rules.**

1. **Paint never waits for fsync, and "saved" always does.** [02](02-ARCHITECTURE.md) draws fsync before the DOM patch in its keystroke flow. Read literally, that cannot meet QR-1. PR-1 only requires that an edit is durable before it *shows as saved*, and the glossary's "acknowledged edit" says the same. This design follows PR-1. Between paint and `durable` the status line says "Saving". The core doc needs correcting, and a row is proposed.
2. **Prediction is not a second truth.** The view-model can be dropped and reloaded from a projection at any moment. If engine and prediction disagree, the engine wins, always.
3. **Prediction is not transformation.** Remote changes are never transformed against in-flight local ops. A block with unacknowledged ops or an active composition is "held": projections for it are stored, and the latest is applied when the block becomes clean. This keeps ADR-005's "no `transform`" true, since the only merge is in the CRDT.
4. **The predictable subset is closed:** `text.insert`, `text.delete`, `text.replace`, `mark.set`, `mark.clear`, `block.split`, `block.merge`. Everything else (type changes through repair, moves, database ops) waits for `ack`, with no visible state change until then. A property test runs `predict.js` and `-ops` on generated inputs and requires the same result.

**Op names here are a working vocabulary.** The closed set belongs to [24](24-OPERATIONS-AND-MERGE.md), and the fields to [22](22-DATA-MODEL.md). If they differ, those documents win, and this table follows.

**Store failure.** If the store cannot accept a write (`CN-EDT-0005`), typing is not silently accepted as saved. The status line shows the failure, the page stays editable in memory, and the person can export. Text that was never `durable` is named as unsaved.

### The editing host

The decision that shapes everything else: one editing host per page, or one per block.

| | Page-level host (first candidate) | Per-block host (prototype) |
| --- | --- | --- |
| Selection across blocks | Native, including drag, Shift+arrows and touch handles | Browsers stop a native selection at the host boundary, so we would draw and manage our own |
| Up and down arrows | Native, respecting line layout and bidi | Hand-written, as in the prototype, and poor on wrapped bidi lines |
| Screen readers | One editable region with real structure inside | Each block is a separate text field, so structure is hard to reach |
| Non-editable blocks | Islands with `contenteditable="false"`, with awkward native caret behaviour we must handle | No issue |
| Cost of a keystroke | Unknown on 5,000 blocks. A browser may do work proportional to the host | Small host, likely cheaper |
| Browser quirks | Risk of native edits crossing our structure. Mitigated by cancelling nearly everything | Fewer |

`SPK-02` tests both. If the page-level host passes, it is the design. It costs more to get right in the browser and returns the selection and screen reader behaviour that QR-5 and PR-5 require.

**DOM shape.** The title and each block are direct children of the host. A block is an element keyed by `data-b="<id>"`.

| Block type ([36](36-EXPORT-FORMAT.md)) | Element and role | Editable |
| --- | --- | --- |
| `paragraph` | `p` | Yes |
| `heading` level 1 to 3 | `h2` to `h4`, with the page title as the only `h1` | Yes |
| `bulleted`, `numbered` | `div` with `role="listitem"`, inside a contiguous `role="list"` group, marker in a non-editable `span` | Yes |
| `todo` | As list item, with a real checkbox in a non-editable `span` | Yes |
| `quote` | `blockquote` | Yes |
| `callout` | `div` with `role="note"` and an icon | Yes |
| `code` | `pre`, `spellcheck="false"`, `dir="ltr"` by default | Yes |
| `divider` | `hr` in an island | No |
| `database`, `embed` | Island. Inner inputs are ordinary controls | No |

Each editable block carries `dir="auto"`, and `lang` from the block or the page if set ([50](50-INTERNATIONALIZATION.md)). `dir="auto"` is on the block row, not only on the text, so that list markers follow the text. In the prototype it is only on the text, and a bullet stays on the left of a right-aligned Arabic line.

**Inside a block.** One text node per run. Marks wrap it in a fixed order, outermost first: `strong`, `em`, `code`, `mark`. A page reference is a non-editable `span[data-ref]` holding the title as text, and it counts as one position in the model, not as the length of its title (the prototype counts its title length, which breaks when the title changes). Filler nodes, needed for empty blocks and for a caret after a reference at the end of a block, are marked `data-filler`, have length zero, and are never copied or read back.

### beforeinput to op

Default is **cancel, then apply**. `P` means `preventDefault`, `L` means the browser is allowed to act. An `inputType` not in this table is cancelled and does nothing (`CN-EDT-0002`, logged in development builds only), so a new browser feature cannot silently change the document.

| `inputType` | Action | Ops | Notes |
| --- | --- | --- | --- |
| `insertText` | P | Delete selection if any, then `text.insert` with `data` and the marks at the caret | `SPK-02` variant V3 lets this through if cancelling harms autocorrect or dead keys |
| `insertReplacementText` | P | `text.replace` on `getTargetRanges()` | Spell check and autocorrect. Text is in `data` or `dataTransfer` depending on the browser |
| `insertCompositionText` | L | None until commit | Not cancellable. See "Composition" |
| `deleteCompositionText`, `insertFromComposition` | L | None until commit | Safari's sequence, from Input Events Level 1 |
| `insertParagraph` | P | `block.split`. Empty list or to-do item: `block.set_type` to paragraph. In `code`: `text.insert` of `\n`. In the title: focus the first block, creating one if needed | Same rules as the prototype's `blockKeys` |
| `insertLineBreak` | P | `text.insert` of `\n`. In the title: ignored | Shift+Enter |
| `insertFromPaste`, `insertFromPasteAsQuotation` | P | See "Clipboard" | Handled in the `paste` event; this is the backstop |
| `insertFromDrop` | P | Fragment or attachment, then `block.move` when moving inside the page | See "Clipboard" |
| `insertFromYank` | P | `text.insert` | macOS Control+Y |
| `insertTranspose` | P | `text.replace` on the target range | macOS Control+T |
| `insertLink` | P | Link mark, if [22](22-DATA-MODEL.md) adopts one | Package schema has none today |
| `insertOrderedList`, `insertUnorderedList`, `insertHorizontalRule` | P | `block.set_type`, `block.insert` | Native menus only |
| `deleteContentBackward`, `deleteContentForward` | P | `text.delete` on `getTargetRanges()`. At the start of a non-paragraph: `block.set_type` to paragraph. At the start of a paragraph: `block.merge` with the previous, or select the previous island | Target ranges respect what the platform does. In Hindi and Arabic backward delete often removes one code point rather than one grapheme, and that is what the person expects. Falls back to `Intl.Segmenter` only when the ranges are empty |
| `deleteWordBackward`, `deleteWordForward`, `deleteSoftLineBackward`, `deleteSoftLineForward`, `deleteHardLineBackward`, `deleteHardLineForward`, `deleteEntireSoftLine`, `deleteContent` | P | `text.delete` on target ranges, or a multi-block batch if the range crosses blocks | Only the browser knows where a soft line or a word in Thai or Japanese ends |
| `deleteByCut`, `deleteByDrag` | P | Delete batch | Cut runs from the `cut` event |
| `historyUndo`, `historyRedo` | P | Engine undo and redo | The browser's history is never used |
| `formatBold`, `formatItalic` | P | `mark.set` or `mark.clear`. With a collapsed caret, sets pending marks for the next insert | |
| `formatRemove` | P | `mark.clear` for all marks in range | |
| `formatSetBlockTextDirection`, `formatSetInlineTextDirection` | P | `block.set_direction`, if adopted ([50](50-INTERNATIONALIZATION.md)). Ignored until then | Chrome on Windows has a native shortcut for this |
| `formatUnderline`, `formatStrikeThrough`, `formatSuperscript`, `formatSubscript`, `formatIndent`, `formatOutdent`, `formatJustify*`, `formatFontName`, `formatFontColor`, `formatBackColor` | P | None | Not in the model |

**A range that crosses blocks** always produces one batch, so one undo step: delete the tail of the first block, delete the head of the last, remove the blocks between, merge.

**Non-cancellable and unseen edits.** Some Android keyboards, browser extensions, and platform services change the DOM without a cancellable event. A `MutationObserver` on the host is always on. Any mutation that `patch.js` did not make and that is not part of a composition is reconciled in the same task: read the block's text, diff against the view-model with a common prefix and suffix, emit `text.replace`, and repaint the block's structure from the view-model. Foreign elements are removed, and their text is kept only if it was added to a text node the person typed in. The DOM can therefore never diverge from the engine for longer than one task.

### Keys not covered by beforeinput

`keydown` runs commands from the registry ([47](47-UX-AND-FEATURE-MAP.md)). It first returns if `event.isComposing` or `event.keyCode === 229`. The second test matters: Safari can deliver the Enter that confirms a candidate after `compositionend`, with `isComposing` false and code 229. Without it, confirming a Japanese word would split the block.

| Key | Behaviour |
| --- | --- |
| Arrow keys | Native. Horizontal movement is never computed by us, because visual order in bidi lines belongs to the browser. Only when the next position is an island do we intercept, and select the island as a block |
| Tab, Shift+Tab | In `code`: `text.insert` of a tab. Elsewhere: moves focus out of the host, so the keyboard is never trapped. Indent has no meaning until [22](22-DATA-MODEL.md) has nested lists, which the package schema lacks today |
| Escape | Closes a menu. Otherwise moves from text selection to block selection |
| Triggers `/` and `[[` | Evaluated on committed text only. Forms typed through an input method (`／`, `、`, `【【`, `「「`) are also accepted ([50](50-INTERNATIONALIZATION.md)) |

Shortcuts match on `event.key` when it is ASCII and fall back to `event.code` otherwise. The prototype's `e.key === "k"` never fires on an Arabic or Hindi layout.

### Composition

**Rule: nothing touches a block during its composition.** No patching, no selection writes, no re-render, no focus change, no scroll adjustment, no remote projection and no virtual tier change. Interrupting a composition commits or destroys it, and that is the most common way web editors fail people typing Chinese, Japanese and Korean.

| Event | Editor does |
| --- | --- |
| `compositionstart` | Marks the block as composing and held. Takes a snapshot of the block's text and the selection in model positions. If the selection spans text or blocks, emits the delete batch first, on the same task |
| `beforeinput` of composition types, `compositionupdate`, `input` | Nothing but updating the status to "Saving", and the candidate position for any menu that wants it |
| `compositionend` | Starts commit. Browsers order the last `input` differently, so commit runs in a task posted after `compositionend`, reading the DOM rather than the event |
| Commit | Diff the block's text with the snapshot, using common prefix and suffix. Emit **one** `text.insert` or `text.replace`, as a single undo step. Clear held state, apply any stored projection, re-check triggers and Markdown shortcuts |

**When the op is emitted:** at commit, and never before. Composing text is not an edit yet: the person has not chosen it.

**Consequences, stated plainly.**

- A crash during composition loses the uncommitted composition. PR-1 covers acknowledged edits, and the status said "Saving" throughout.
- Remote changes to that block appear only after commit. A long composition delays them, and never corrupts them.
- On Android, some keyboards compose every word, and a few compose for a very long time. Android is Phase 9, and `MOB-01` must revisit this.
- If the block is deleted remotely during composition, the composition is kept: on commit the text is inserted as a new block in the same place and the person sees it. Text the person typed is not dropped (PR-1).
- Switching tab or page during composition waits for `compositionend`, which the browser fires on blur.

**Dead keys** (Spanish, French, German, Portuguese) go through the same path where the platform uses composition events, and through `insertText` where it does not.

### Positions, caret and selection

**Model position:** `{ block, offset, affinity }`. `offset` is in UTF-16 code units at the editor boundary because that is what the DOM uses. The bridge converts if the stored unit differs, and [24](24-OPERATIONS-AND-MERGE.md) names that unit. A position is never placed inside a surrogate pair: if the DOM gives one, it snaps outward. `affinity` says which run a boundary position belongs to.

**DOM to model.** Find the block by walking up to `data-b`. Add the lengths of earlier text nodes and references using a per-block table of cumulative offsets, rebuilt when the block is patched. Filler nodes count zero. A position inside a reference snaps to its nearer edge.

**Model to DOM.** Reverse the walk. Selection is written only if it differs from the current one, and never during composition, because writing it restarts input method state in some browsers and can scroll.

**Marks at the caret.** Typing at a boundary takes marks from the left run, except marks that do not extend (`code`, and any link mark). Pending marks set with a collapsed caret apply to the next insert and clear on any caret move. Exact extension rules belong to [24](24-OPERATIONS-AND-MERGE.md), since the CRDT's mark behaviour has to match.

**Selection state.**

| Mode | Meaning | Painted by |
| --- | --- | --- |
| `text` | Anchor and focus are model positions, in any blocks | The native selection |
| `blocks` | A contiguous range of whole blocks | `aria-selected` and a token background, with the native selection collapsed |
| `all` | The whole page | As `blocks`. No DOM range is built, so it works whatever the virtual tier. Mod+A first selects the block's text, and the second press enters this mode |

The selection is sent to the engine with each batch and, debounced, on `selectionchange`. `-merge` holds it as CRDT cursors, so a remote edit before the caret moves it correctly. A projection carries the recomputed selection and the `clientSeq` it was based on. If the person has moved since, it is ignored.

**Bidi.** A position at a direction boundary has two visual carets. We do not draw carets, so the browser handles this. The selection may paint as several separate rectangles, which is correct.

### Projection patching

`patches` from the engine are per block, keyed by id: `{ id, version, type, attrs, runs }`, plus block order changes and removals.

1. **Skip** if the version matches the view-model. This is the usual case after a predicted keystroke.
2. **Hold** if the block is composing or has in-flight ops.
3. **Structure:** if the type or an element-defining attribute changed, replace the block's element and restore the selection by model position.
4. **Runs:** compare old and new runs by common prefix and suffix, and touch only the runs between. Where a text node's marks are unchanged, change its `data` with `replaceData` so that the node, and the caret in it, survive.
5. **Order:** move elements by the longest run already in order, so that moving one block does not move the 4,999 others.
6. **Frame:** local patches apply in the same task. Remote ones batch on `requestAnimationFrame`, and never take more than a proposed 4 ms in one frame before yielding. All of this is off the typing path (PR-4).

Everything derived runs after this and off the typing path: outline, backlinks, counts and search.

**Writes are text only.** `patch.js` uses `createElement`, `createTextNode`, `textContent`, `setAttribute` with an allow-list, and `replaceData`. `check-dom-text.py` refuses `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write` and `execCommand` in `webapp/`. There is no exception for icons: the prototype's `innerHTML` for icons becomes static SVG built with `createElementNS` (ADR-026). CSP is `default-src 'self'`, `script-src 'self' 'wasm-unsafe-eval'`, `object-src 'none'`, `base-uri 'none'`, with `require-trusted-types-for 'script'` where supported. Images load only from local blob URLs (SEC-W01, SEC-W03). [07](07-SECURITY-THREAT-MODEL.md) holds the threats.

### Virtualising long pages

Honesty is the constraint: find-in-page, screen readers and selection must keep working, or the limit must be stated. So the ladder starts with what keeps all three, and `SPK-02` moves up only when the budget fails.

| Tier | What happens off screen | Find in page | Screen reader | Selection | Cost |
| --- | --- | --- | --- | --- | --- |
| 0 | Nothing | Native | Full | Native | Full layout |
| 1 | `content-visibility: auto` with `contain-intrinsic-size: auto` and a per-type estimate | Native, per spec | Full, per spec | Native | Browser support to verify in `SPK-02` |
| 2 | **Hollow blocks:** element and plain text stay. Marks, references, gutter controls and islands are not built until near the viewport | Native, on plain text | Text and headings stay; inline meaning is lost until hydrated | Native. Hydration preserves it by model position | Much less DOM, all the text |
| 3 | **Windowed:** blocks outside a generous window become sized spacers | Own find bar only | Reads to the window edge. Headings stay mounted as landmarks, and each block has `aria-posinset` and `aria-setsize` | Anchor and focus blocks stay mounted; a selection between them is held in the model. `blocks` and `all` modes are unaffected | Least DOM. Least honest |

**Tier 1 is the starting design.** Tier 3 ships only if `SPK-02` forces it, and then with the limits above published in the accessibility statement. A block holding anchor, focus or composition, and the block being patched, are never hollowed or unmounted. Height estimates that change as a block hydrates are anchored on the first visible block, so that scrolling does not jump.

**Find.** An own find bar (`edit.find`) exists whatever the tier, because a desktop webview has no reliable native find interface. It searches page text from the engine using the search normaliser ([60](60-SEARCH-AND-EMBEDDINGS.md)) and paints with the CSS Custom Highlight API where supported, so that highlighting mutates nothing and cannot disturb a composition. Browser find still works in tiers 0 to 2.

### Undo

`-ops` owns the undo stack (`OPS-02`). It is per page and per device, covers local ops only, and never undoes another device's work. Canvas and ink ops share the page's stack ([43](43-CANVAS-AND-INK.md)).

- **Steps.** A batch is one step. Contiguous typing in one block groups together until a pause (proposed 700 ms, from the prototype), a caret jump, or any non-typing op. A composition commit is one step. An assistant write is one step (ADR-013).
- **Selection.** Each step stores model selection before and after, as stable positions, and undo restores it.
- **During composition,** undo keys go to the input method. We do nothing.
- **Refusal.** If the inverse cannot apply, for example because the block was removed on another device, the step is skipped with a visible message (`CN-EDT-0006`), and nothing is half-applied. [24](24-OPERATIONS-AND-MERGE.md) defines inverses over merged state.
- **Native history** is never used. Every `historyUndo` and `historyRedo` is cancelled, and Mod+Z is a registered command.

### Clipboard

**A clipboard is hostile input.** Any page can put anything on it. Stated as a property and tested: *pasting hostile markup produces no script execution and no network request* (SEC-W04, QR-7). The approach is OpenCalc's docs/68: the browser parses, and nothing parsed is ever inserted.

**Copy and cut.** In the `copy` and `cut` events, from the view-model:

| Flavour | Content |
| --- | --- |
| `text/plain` | Text with one newline between blocks. No Markdown symbols |
| `text/html` | Built by a fixed serialiser with escaping, from the same block-to-tag table. Carries a `data-cn-fragment` attribute on the root holding the fragment as base64 JSON, in the [36](36-EXPORT-FORMAT.md) block schema |

**Paste pipeline.**

```
paste event → read flavours
  internal fragment present and valid? → use it, as untrusted: schema and limits checked
  else text/html? → DOMParser (inert: no browsing context, no scripts, no fetches)
                  → walk with allow-list → Fragment (plain data, never nodes)
  else text/plain → one paragraph per line
  files? → attachments through the store (FIL-01), never via src
Fragment → engine: validated, one op batch, one undo step → projection → DOM
```

Rules: parsed nodes are never inserted. `textContent` and a fixed attribute allow-list are all that is read. `src`, `srcset`, `style` values with `url()`, and `on*` are never read, so there is nothing to sanitise there. The walk is bounded (`CN-EDT-0004`). Plain paste (`edit.paste_plain`) always uses `text/plain`. In `code` blocks all paste is plain, newlines kept. In the title, newlines become spaces. Text is stored as written, without normalising ([50](50-INTERNATIONALIZATION.md)).

| HTML | Result |
| --- | --- |
| `h1`, `h2`, `h3`, and `h4` to `h6` | Heading 1, 2, 3, and 3 |
| `p`, `div`, unknown block elements | Paragraph |
| `ul > li`, `ol > li` | Bulleted, numbered. Nesting flattens until the model has it |
| `blockquote`, `pre`, `hr` | Quote, code, divider |
| `strong`, `b`, `font-weight` of 600 or more | `bold` |
| `em`, `i`, `font-style: italic` | `italic` |
| `code`, `mark` | `code`, `highlight` |
| `a[href]` | Link mark if adopted, with schemes limited to `http`, `https`, `mailto` (SEC-W05). Until adopted, the text and then the URL as text. The URL is never dropped silently |
| `img` | Allowed raster type in a `data:` URL or clipboard file: attachment, if [22](22-DATA-MODEL.md) has an image block. Remote: alt text and the URL as text. **Never fetched** (SEC-W03) |
| `table` | One paragraph per row, with tab-separated cells. Conversion to a database is an explicit command |
| `script`, `style`, `template`, `noscript`, `iframe`, `object`, `embed`, `svg`, `math`, `head` | Subtree skipped |
| Anything else | Unwrapped, text kept |

The same mapping as a table of fixtures also drives HTML import ([37](37-IMPORT-FORMATS.md)), so paste and import agree. Which parser runs for import in a headless host is that document's decision.

**Fixtures.** Real clipboard captures from common sources (Word, Google Docs, browsers, Excel), and a hostile set: `<script>`, `onerror`, `<img src>` pointing to an observable URL, `javascript:` links, mutation-XSS strings, deep nesting, and an oversized fragment.

### Browser differences, as named risks

All unverified until `SPK-02`. Each has a test in the matrix.

| ID | Risk | Response |
| --- | --- | --- |
| BR-1 | `compositionend` and the last `input` arrive in different orders across browsers | Commit reads the DOM in a posted task |
| BR-2 | Safari's confirm-Enter has `isComposing` false and code 229 | Test both |
| BR-3 | Input Events Level 1 and Level 2 differ: Safari's extra composition types; what is cancellable | Table above accepts both; reconciliation is the backstop |
| BR-4 | `getTargetRanges()` is empty in some cases, such as at a block edge or in some Firefox versions | Fall back to `Selection.modify` on a clone, then `Intl.Segmenter` |
| BR-5 | Caret cannot rest after a non-editable element at the end of a line, in Chrome and Safari | Filler nodes |
| BR-6 | Firefox and Chrome differ on deleting across and into `contenteditable="false"` | All cancelled; own handling |
| BR-7 | Safari autocorrect, the text replacement bubble and dictation may rely on native insertion | Variant V3 |
| BR-8 | Dictation and emoji pickers may fire composition or `insertText` with long ranges | Included in the test script |
| BR-9 | Native spell check underlines are lost when a text node is replaced | `replaceData` on patches. Whole-block rebuilds are structural only |
| BR-10 | A big host may slow typing, spell check or hit testing, especially in Safari | Variant V2; virtual tiers |
| BR-11 | Screen readers track a live-mutated editable region badly (VoiceOver in Safari, NVDA in Firefox and Chrome) | In-place text node updates. Scripted and manual test |
| BR-12 | `content-visibility` support and its effect on find and accessibility differ by version | Tier fallbacks by feature detection |
| BR-13 | Windows input methods differ from macOS ones. **The owner has no Windows machine, and `DEC-07` targets Phase 6** | `SPK-02` needs Windows much earlier. Row proposed |
| BR-14 | Android keyboards send most typing as composition, with non-cancellable deletes | Reconciliation. Full treatment in `MOB-01` |
| BR-15 | Grammar and password-manager extensions inject nodes into editable regions | `MutationObserver` reconciliation |
| BR-16 | Webview engines differ from the browsers, and are updated by the OS | `SPK-02` adds WKWebView and WebView2 runs before ADR-007 is settled |

### Headless layout and render

`-layout` and `-render` draw pages without a browser, for PDF, PNG, thumbnails and the server (`RND-01`). They never drive the live editing surface ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)).

```
model page or canvas
  → -layout: block layout, itemise (bidi, script, language)
           → -text: font selection, shaping with harfrust
           → parley: line breaking and line boxes
           → -ink: stroke outlines
           → display list
  → -render: tiny-skia for PNG; PDF writer for PDF
```

**Display list.** Serialisable, in painter's order, with items `Path`, `GlyphRun` (face, size, glyph ids, positions and source range), `Image`, `Group` (transform, opacity, clip), `Blend` (for the highlighter). Shape taken from OpenCalc and OpenSlide docs/42, adapted here.

**Screen and print may break lines differently.** The browser lays out the editor and parley lays out headless output, and ADR-007 accepts this: notes do not need identical line breaks. A person never edits against engine metrics, so nothing depends on them matching.

**Determinism (QR-3).** The same model, the same version and the same font pack give the same bytes. `-render` uses a pinned font pack and never system fonts. The pack's hash is part of each golden. The PDF has no timestamps unless passed in by the host. Hash map order is not allowed in output.

**Fonts.** Faces are supplied through a host seam rather than compiled in, as OpenCalc's docs/64 and docs/101 decided. CJK slices are fetched by need: the writer reports which it wants, the host supplies them, and the export report names any character it could not draw, per script, with a sample. Nothing is drawn as an unexplained box. Rules in [50](50-INTERNATIONALIZATION.md).

**PDF.** Real text, with embedded subset fonts and `ToUnicode`, so that text extracts in every launch script, Arabic and Devanagari included. Document language and heading tags are written, because QR-5 should extend to exports. Vector ink. Canvas pages size to their content. PDF backgrounds are in [43](43-CANVAS-AND-INK.md). OpenCalc's PDF writer and subsetter are the obvious source, copied with `PROVENANCE.md`, once confirmed free of spreadsheet types. [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) lists only `-package`, `-text` and `-secrets` as copied today.

**Size on the web.** The typing path must not pay for shaping. OpenCalc's docs/64 records that embedded fonts were 72% of its wasm bundle. `-layout` and `-render` load as a separate wasm module, in a worker, on the first export. [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) describes one `-wasm` with the same set as `-sdk`, so this is a proposed boundary change and needs an ADR. The alternative of turning shaping off in the browser (OpenCalc's choice) would break PR-5 for a web PDF export of Arabic or Hindi.

**Licences**, per [12](12-OPEN-SOURCE-AUDIT.md): parley Apache-2.0 OR MIT, harfrust MIT, tiny-skia BSD-3-Clause, kurbo and peniko Apache-2.0 OR MIT. **`skrifa`, named in [19](19-WORKSPACE-SCAFFOLD-DESIGN.md), and any PDF writing dependency are not in the audit.** Whether parley uses harfrust directly, or how it shares fonts with `-text`, is unverified, and [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) does not list parley under `-layout`.

**Phase mismatch.** FR-13 lists PDF per page in Phase 1, while `-render` and `RND-01` are Phase 3. This document assumes PDF arrives in Phase 3.

## Failure modes and limits

Codes for [20](20-ERROR-CODE-REGISTRY.md), limits for [21](21-PARSER-LIMITS.md). All limit values are proposals.

| Code | Meaning | Person sees |
| --- | --- | --- |
| `CN-EDT-0001` | Engine refused an op batch. The DOM returns to the projection | The block returns to its last state, with a message |
| `CN-EDT-0002` | Unknown `inputType` cancelled | Nothing. Development log only |
| `CN-EDT-0003` | Prediction differed from the projection | Nothing, with a counter for tests. A failure in CI |
| `CN-EDT-0004` | Paste or drop exceeded a limit, or a fragment failed its schema | "Could not paste all of this", and what was kept |
| `CN-EDT-0005` | The store refused or failed a write | Status line shows unsaved state. Export stays available |
| `CN-EDT-0006` | Undo could not apply | Step skipped, with a message |
| `CN-EDT-0007` | Engine unreachable (worker died, IPC failed) | Editing pauses, unacknowledged text kept on screen and re-sent after restart; if that fails, offered for copying |
| `CN-EDT-0008` | Text with a lone surrogate reached the bridge | Replaced by U+FFFD at the boundary, and logged |

| Limit | Proposed value |
| --- | --- |
| Paste: clipboard HTML bytes | 16 MiB |
| Paste: elements walked | 200,000 |
| Paste: nesting depth | 64, matching the package's JSON limit |
| Paste: blocks in one batch | 20,000 |
| Internal fragment bytes | 16 MiB |
| Text in one block, when pasting | 1 MiB of UTF-8. Longer pasted text splits on newlines first. The model's hard limit is 4 MiB ([22](22-DATA-MODEL.md)) |

**Known limits.** A crash can lose uncommitted composition and edits not yet durable, which were never shown as saved. Long pages at tier 3 would weaken browser find and screen reader browsing. Line breaks differ between screen and export.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Engine-drawn text, as OpenSlide does | Loses native input methods, spell check, accessibility and right-to-left for free. Needed there for identical line breaks, which notes do not need. **Kept as the last resort** after `SPK-02` |
| Letting the browser edit, then reading the DOM back (the prototype) | No op per intent, no control over what the browser writes, and structure drift. Still used for composition, because there is no alternative |
| Hidden `contenteditable` and an own caret | The same input path as engine-drawn text, with the same loss of accessibility and native selection |
| Engine on the main thread, synchronous `beforeinput` | Tempting, because it removes prediction. OPFS sync handles need a worker anyway, desktop is IPC regardless, and a slow merge would freeze typing. `SPK-02` measures it as a data point for the web |
| Existing editor libraries (ProseMirror, Lexical, and others) | Would bring a document model competing with the engine's, against ADR-005 and ADR-025. They were not audited for this and may be read for ideas within licence |
| OT-style rebasing of in-flight ops | Forbidden by ADR-005 |

## SPK-02: can DOM editing be op-driven and fast?

**Decides ADR-007 and ADR-025.** The result is not product code ([05](05-PHASES-AND-TASKS.md)). It lives in a throwaway harness.

**Build.** The real edit protocol above with a real engine worker. If `SPK-01` has no `-merge` yet, Loro's wasm is called directly in the harness, so merge cost is real. Three variants:

| Variant | Host | `insertText` | Virtual tier |
| --- | --- | --- | --- |
| V1 | Page-level | Cancelled | Start at 1, go up as needed |
| V2 | Per block, with own cross-block selection | Cancelled | Same |
| V3 | Best of V1 and V2 | Allowed through, then reconciled | Same |

**Fixture.** The Long page workspace from [30](30-PERFORMANCE-TARGETS.md): 5,000 blocks of mixed types, in all launch languages, with marks and references. At least one mostly Arabic page and one mostly Japanese.

**Measure.** On a 60 Hz display, waiting for the next refresh alone can use 16 ms, so measuring to pixels on screen would fail any page. The spike reports two numbers, as [30](30-PERFORMANCE-TARGETS.md) defines:

- **Work time:** from the input event's `timeStamp` to the end of the frame's main-thread work. From a trace in Chrome, and from `requestAnimationFrame` followed by a posted task elsewhere. 95th percentile must be within 16 ms.
- **First-frame rate:** the share of keystrokes shown in the first frame after the event. Reported, and the owner judges.
- Also reported: PERF-04 (open to interactive), engine `ack` latency, `durable` latency (PERF-02), prediction mismatches, DOM node count, memory.

Keystrokes are scripted at realistic speed, and with a person typing. Keystrokes run at the top, middle and end of the page, in plain text and in marked text, in Latin and in complex scripts.

**Machines.** Named at the start of the spike, per [30](30-PERFORMANCE-TARGETS.md): a mid-range laptop and an old one. "Mid-range" has no definition until then, and the named model and price band go in the report.

**Matrix.**

| | Chrome | Safari | Firefox |
| --- | --- | --- | --- |
| Performance: 95% within 16 ms | Required | Required | Required |
| Chinese Simplified and Traditional: system pinyin, zhuyin or cangjie | Required | Required | Required |
| Japanese: system input method, Google Japanese Input where available | Required | Required | Required |
| Korean: two-set, syllable composition | Required | Required | Required |
| Hindi: InScript and a transliteration method | Required | Required | Required |
| Dead keys: Spanish, French, German, Portuguese | Required | Required | Required |
| Right-to-left: Arabic typing, mixed bidi lines, caret, selection, delete | Required | Required | Required |
| Screen reader | VoiceOver and NVDA | VoiceOver | NVDA |
| Native spell check and autocorrect | Required | Required | Required |
| Selection: drag, Shift+arrows, across islands, Mod+A, on a long page | Required | Required | Required |
| Find in page | Required | Required | Required |
| Paste: real and hostile fixtures | Required | Required | Required |

Windows runs need a machine or a remote one (BR-13, `DEC-07`). Automated composition uses Chrome's DevTools protocol (`Input.imeSetComposition`), which Firefox and WebKit do not offer in the same way, so real input methods are also tested by hand against a written script, and each result is recorded per browser and per method.

**Pass.** On a 5,000-block page, work time is within 16 ms for 95% of keystrokes on a mid-range laptop, and input methods, right-to-left and a screen reader all work in Chrome, Safari and Firefox.

**Fail: virtualise harder.** Move in order, and re-run after each step:

1. Tier 2, then tier 3, with the limits above accepted and written down.
2. Variant V2, if the page-level host is what is slow or broken.
3. Narrow the browser list, with owner agreement (RISK-02).
4. **Last resort: engine-drawn text,** with a hidden input element and a parallel accessible DOM, as OpenSlide does. This reopens ADR-007, puts shaping on the typing path, and changes the reasons this product exists. It needs an owner decision.

**Report.** `SPK-02` writes the variant chosen, the tier needed, each matrix cell, and the numbers, and settles ADR-007 and ADR-025. PERF-01 and PERF-04 then record a machine and a commit.

## Acceptance gates

| Gate | Proves | Tracker |
| --- | --- | --- |
| `SPK-02` report, every matrix cell recorded | Design is viable | `SPK-02` |
| PERF-01 on the named runner, against a committed baseline. A CI runner is not a laptop, so CI catches regressions, and the laptop figure is taken per release | QR-1 | `WEB-03` |
| Every row of the `inputType` table has a test in each browser | Mapping | `WEB-03` |
| Composition test: nothing writes to the host during composition, shown by a `MutationObserver` assertion, in Chinese, Japanese and Korean | No interruption | `I18N-03` |
| Remote edit arriving during composition neither breaks the composition nor loses the edit | Hold rule | `WEB-03` |
| `predict.js` agrees with `-ops` on generated inputs | Prediction | `WEB-03` |
| Crash injection between paint and `durable`: status never said saved for lost text | PR-1 | `STO-01`, `WEB-03` |
| Hostile clipboard fixtures: no script, no request, no raw node inserted | QR-7, SEC-W04 | `WEB-04` |
| `check-dom-text.py` seeded violation fails | QR-7 | `GOV-03` |
| Find, selection and screen reader script on the Long page at the chosen tier | Honest virtualisation | `A11Y-01` |
| Golden PDF and PNG per launch script, text extracted back from the PDF | Headless output | `RND-01` |
| Same bytes native and wasm, with the same font pack | QR-3 | `RND-01` |

## ADRs

ADR-005 (no `transform`), ADR-007 and ADR-025 (settled by `SPK-02`), ADR-013 (assistant writes are one undo step), ADR-018, ADR-026. **Proposed, not yet in [08](08-ADR-REGISTER.md):** separate lazy wasm modules for `-layout` and `-render`, and for `-ink` ([43](43-CANVAS-AND-INK.md)).

## Tracker ids

`SPK-02`, `WEB-01`, `WEB-03`, `WEB-04`, `WEB-05`, `OPS-02`, `I18N-02`, `I18N-03`, `A11Y-01`, `RND-01`, `FIL-01`, `MOB-01`, `STO-01`, `GOV-03`, and `RISK-02` in [06](06-ROADMAP-AND-DELIVERY.md).
