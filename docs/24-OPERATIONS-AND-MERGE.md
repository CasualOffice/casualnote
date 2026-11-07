# 24 — Operations and merge

**Status: design only. Nothing is built.** ADR-005 and ADR-006 are Proposed and `SPK-01` can overturn them. All numbers are proposals. Statements about Loro's API that were not checked against a pinned version are marked unverified.

## Outcome

Every edit to the notes is one op from a closed set. `-ops` validates it, records how to undo it, and compiles it to CRDT writes. `-merge` holds those writes in one CRDT document per page and one for the page tree, merges changes from other devices, and projects the result into [`-model`](22-DATA-MODEL.md) types. Two devices that have seen the same changes show the same notes, whatever order the changes arrived in. When two devices changed one block without seeing each other, the merge still happens, and the block is marked so a person can look at it.

This page also defines `SPK-01`, which decides whether any of this survives contact with years of real notes.

## Design

### Documents

| Class | One per | Holds | Why separate |
| --- | --- | --- | --- |
| Tree | Workspace | Page tree, titles, favourites, trash, database registry, workspace name | The sidebar opens without loading a page |
| Page | Page | Blocks, text, canvas, ink, reviews, checkpoints | Loading cost stays proportional to one page. A long history in one page cannot slow another |
| Database | Database | Statuses, columns, rows, reviews, checkpoints | Recorded in ADR-006. Databases are shared between pages (FR-05), so they cannot live in a page's document |

Keeping a database in its host page would mean opening another page's whole document to show an embedded table, and deleting the host would threaten the shared data. A database document behaves like a page document for sync, storage and undo.

**Atomicity is per document.** Sync delivers documents independently ([56](56-SYNC-DESIGN.md)), so atomicity across documents could not hold on other devices even if a local write had it. Cross-document work is made safe by ordering instead: **create new documents first, and make the visible reference last.** An unreferenced document is invisible, and harmless.

| Work | Order | If interrupted |
| --- | --- | --- |
| Create page | Page document, then tree node | Orphan page document with nothing in it. R-19 in [22](22-DATA-MODEL.md) hides it, and compaction removes it |
| Create database in a page | Database document, tree registry, then the block | Registered but unreferenced database, listed in settings |
| Import into a notebook | All new documents, then one tree batch | Nothing appears, and the leftovers are removed |
| Assistant write across pages | One batch per document, sharing a `group` | A prefix applied. See op batches |

### Container layout

The layout is named here in neutral terms and implemented only in `-merge`. "Text" means a CRDT text with marks, "mlist" a movable list, "map" a CRDT map with last-writer-wins keys.

| Document | Container | Contents |
| --- | --- | --- |
| Tree | `pages`: tree | Node metadata: `page_id`, `title` (Text, created with the node), `kind`, `database`, `favorite`, `trashed_at_ms`, `trash_base`, `x` |
| Tree | `purged`: map | `PageId` to `{ final, at_ms }`, the tombstone `PurgePage` leaves. `final` is the page document's version at purge |
| Tree | `workspace`: map | `id`, `name`, `languages`, `x` |
| Tree | `databases`: map | `DatabaseId` to `{ trashed_at_ms }` |
| Tree | `extensions`: map | Path to `sha256` |
| Page | `meta`: map | `view`, `lang`, `x` |
| Page | `order`: mlist | `BlockId` strings |
| Page | `blocks`: map | `BlockId` to map: `type`, `level`, `checked`, `lang`, `database`, `page`, `file`, `x`, `text` (Text), `del_after`, `del_fp`, `split_from`, `merged_into` |
| Page | `canvas`: map | `note_x`, `note_y`, `note_width`, `present`, `x` |
| Page | `item_order`: mlist, `items`: map | `ItemId` to map: geometry, `kind`, `color`, `x`, `text` (Text), `del_fp`, `del_after` |
| Page | `ink_order`: mlist, `ink`: map | `InkId` to one immutable value per record |
| Page, database | `reviews`: map | Target key to a map: `versions` (device to `ReviewVersion`) and `resolved` (device to the `Version` that device resolved through). An entry stays after it is resolved |
| Page, database | `versions`: list | Checkpoints |
| Database | `meta`: map, `name`: Text | `default_view`, `x` |
| Database | `status_order`, `column_order`: mlist; `statuses`, `columns`: map | Id to map: `name` (Text, created with the entry), `group` or `type`, `hidden`, `x` |
| Database | `row_order`: mlist, `rows`: map | `RowId` to map: `name` (Text), `status`, `dates` (map), `x`, `del_fp`, `del_after` |
| Database | `cell:<row>:<column>`: root Text | Text cells. See rule L-1 |

**Layout rules. These prevent silent loss, and each has a test.**

| Rule | Reason |
| --- | --- |
| **L-1: a child Text is created with its parent, never lazily.** Otherwise it must be addressed by a stable root name | If two devices lazily create a child under one key, one container wins and the other's text is hidden. A text cell cannot be created with the row, because a column can be added later, so cells use root containers named from their ids, which every peer resolves to the same container |
| L-2: every block gets a `text` at insert, including a divider | A concurrent change of kind cannot then race to create it |
| L-3: `order` holds ids, and `blocks` holds content | Delete against edit becomes detectable, which R-08 in [22](22-DATA-MODEL.md) depends on |
| L-4: `blocks` entries are not removed while history needs them | Undo and R-08 read them. Compaction may clear them after the trim rule allows ([23](23-STORAGE-FORMAT.md)) |

Unverified: that many root containers per document scale well, which `SPK-01` tests with a 100,000-cell database.

### Op batches

```rust
pub struct OpBatch {
    pub id: BatchId,                 // 16 random bytes, minted by the caller
    pub doc: DocRef,                 // Tree, Page(id) or Database(id)
    pub schema: u16,                 // operation schema version
    pub origin: Origin,
    pub ops: Vec<Op>,
    pub client_seq: u64,             // per editor session and document, from 1
    pub group: Option<GroupRef>,     // id, index, total, for batches across documents
    pub wall_ms: i64,                // from the host; informative only
    pub step: StepHint,              // Coalesce with the previous step, or start a new one
}

pub enum Origin { User, Undo, Redo, Assistant { tool: String }, Import, Restore, Review, System }

pub struct Applied { pub rev: ProjectionRev, pub delta: Delta, pub durable: AckTicket, pub undo: Option<StepId> }

/// Where an op lands in a text. Never a bare offset, which a remote import would move.
pub enum TextPos {
    Anchor(Anchor),                                // a stable cursor from MergeDoc::anchor
    Provisional { client_seq: u64, index: u32 },   // `index` scalars into the text that batch inserted
}
```

**Positions, and why batches carry no `base`.** An earlier draft carried a projection revision and refused an op when a remote import had moved its target. That lost text a person had typed, most of all across a long composition, so `base` is removed.

- **Text is addressed by `TextPos`, blocks and other entries by id.** Neither moves when a remote import lands. Offsets exist only inside `-merge` at compile time, in the same single-threaded turn that resolves them.
- **`client_seq` orders one session's batches.** They apply in that order, and a repeat returns the first result. A later batch can name text from an earlier one through a provisional anchor ([42](42-EDITOR-AND-RENDERING.md)), so **a device's own in-flight ops never make a later op stale.** It is not stored in the frame, and is unrelated to `client_seq` on the server's `Update` ([57](57-SERVER-DESIGN.md)).
- **A remote import never refuses an op whose anchors still resolve.** An anchor on a character that someone has deleted resolves to the nearest surviving position (from memory of Loro's stable cursors, unverified, `SPK-01`). A composition takes its anchor at `compositionstart`, so text typed for minutes lands there at `compositionend` whatever arrived meanwhile.
- **A range keeps what it did not see.** `DeleteText` and `SetMark` name both ends. If a remote insert landed inside the range, the delete takes only the characters that were in the view. This is the same identity question as U-1, with the same options, and `SPK-01` settles it.
- **Refusal is only for a target truly gone or invalid:** the page is purged or its document unavailable, the block's entry was cleared by compaction, an anchor predates a trim point, or the target now takes no text. The code is `CN-OPS-0002`.

**The recovery path. Typed text is never discarded.** A refused batch writes nothing to the notes, and its text goes to a recovery record on this device, shown where the person was typing, and kept across restarts until they choose ([42](42-EDITOR-AND-RENDERING.md)). Later batches that name its provisional anchors refuse with it, in order, and independent batches carry on. A tool write has its own path (`CN-OPS-0004`), since its text is not lost when it is refused.

**Applying a batch.**

| Step | Action | On failure |
| --- | --- | --- |
| 1 | Take the batch in `client_seq` order. Resolve every `TextPos`, `after` and id | Target truly gone or invalid: `CN-OPS-0002`, and typed text takes the recovery path |
| 2 | Validate every op, in order, against the current Valid state with the earlier ops of the batch applied to a scratch copy | Whole batch refused, nothing written. Typed text takes the recovery path |
| 3 | Compute inverses | Cannot fail after step 2 |
| 4 | Compile to `Write`s | Cannot fail after step 2. A failure is a bug: `CN-OPS-0099` |
| 5 | `MergeDoc::apply` runs all writes in one CRDT commit | The document instance is discarded and reloaded from the durable log. Nothing was acknowledged |
| 6 | `-store` appends the update as one frame ([23](23-STORAGE-FORMAT.md)) and returns a ticket | The edit stays in memory, and the state shows "not saved" |
| 7 | Project the delta, push the undo step | |
| 8 | When the ticket completes, the host may show "saved" (PR-1) | |

Painting does not wait for step 8. See "The keystroke order" under Failure modes.

**A batch is one document, one commit, one undo step.** Peers get the whole batch or none of it. A `group` links batches across documents and is one undo step locally, but its atomicity is not promised: after a crash, or part-way through sync, a device may hold a prefix. Each part is valid alone, and a `PartialGroup` version event says what happened.

Limits on a batch are in Failure modes. An import too big for one frame becomes several batches in a group, committed by its final tree batch.

### The closed op set

The set is closed. A new op is an operation schema version bump and an ADR. `TextEdit` is `Insert { at, text }` or `Delete { from, to }`, where each position is a `TextPos`.

**Common validation for all ops:** the document exists and is writable; ids have the accepted form; lengths are within limits; `after` names an entry this document has held, or is `None`, meaning first. An `after` that a remote delete has removed is followed along its `del_after` chain, as R-08 places a block, and is not refused. Every `TextPos` resolves. **Live** means visible in the projection, which includes a block, row or item kept by R-08 or R-17 (see Implicit restore). Each row lists only what it adds. In the inverse column, `prior` means a value read at step 3.

**Compiled writes notation.** `text(x)` is a Text, `mlist(x)` a movable list, `map(x).k = v` a map write. `i` is the index resolved from `after` against the current list. `at` and `len` in the writes are scalars resolved from `TextPos` at step 4.

#### Tree document

| Op | Fields | Validation | Inverse | CRDT writes |
| --- | --- | --- | --- | --- |
| `CreatePage` | `page, parent, after, kind, title` | Id unused; parent live; a database kind names a registered database | `TrashPage` | `tree.create(parent, i)`, then node meta and `title` Text |
| `MovePage` | `page, parent, after` | Page live; new parent not itself or a descendant | `MovePage` to `prior` parent and sibling | `tree.move(node, parent, i)` |
| `EditTitle` | `page, edits[]` | Page exists | Text inverse (see Undo) | `text(title)` insert, delete |
| `SetFavorite` | `page, value` | | `prior` | `node.favorite = v` |
| `TrashPage` | `page, at_ms` | Not trashed | `RestorePage` | `node.trashed_at_ms = at`, `node.trash_base = version(page document)` |
| `RestorePage` | `page` | Trashed | `TrashPage` | `node.trashed_at_ms = null`, `node.trash_base = null` |
| `PurgePage` | `page` | Trashed for 30 days or more; every registered device has acknowledged ([56](56-SYNC-DESIGN.md)); and no edit is concurrent with or later than the trash (see Edits after trash) | **None.** Not undoable | `tree.delete(node)`, `map(purged).page = { final, at_ms }`. File removal follows in [23](23-STORAGE-FORMAT.md) |
| `SetWorkspace` | `name?, languages?` | | `prior` | `map(workspace)` |
| `RegisterDatabase` | `database` | Id unused | `SetDatabaseTrashed(true)` | `map(databases).id = {}` |
| `SetDatabaseTrashed` | `database, at_ms?` | Registered | `prior` | `map(databases).id.trashed_at_ms` |
| `SetExtensionFile` | `path, sha256?` | Blob present; path rules as [36](36-EXPORT-FORMAT.md) | `prior` | `map(extensions)` |

#### Page document: blocks and text

| Op | Fields | Validation | Inverse | CRDT writes |
| --- | --- | --- | --- | --- |
| `SetPageView` | `view` | | `prior` | `map(meta).view` |
| `SetPageLang` | `lang?` | Pattern | `prior` | `map(meta).lang` |
| `InsertBlock` | `block, after, body, lang?, content, x?` | Id unused in `blocks`. Database or embed target has valid id form, and may dangle. An attachment's blob is complete | `DeleteBlock`, refused if others have since edited (see corners) | Child map and `text`, fields, `mlist(order).insert(i, id)` |
| `DeleteBlock` | `block` | Live | `RestoreBlock` | `b.del_after = prev`, `b.del_fp = fp(text)`, `mlist(order).delete(idx)` |
| `RestoreBlock` | `block, after` | In `blocks`, not in order. Only produced by undo, redo, review or implicit restore | `DeleteBlock` | `b.del_fp = null`, `mlist(order).insert(i, id)` |
| `MoveBlock` | `block, after` | Live; `after` is not the block | `MoveBlock` to `prior` predecessor | `mlist(order).move(from, i)` |
| `SetBlockKind` | `block, kind, level?, checked?` | A kind without text requires the text to be empty. A heading requires a level | `prior` kind and attrs | `b.type`, `b.level`, `b.checked` |
| `SetBlockAttr` | `block, attr, value` | One of `checked`, `level`, `lang`, `database`, `page`; right for the type | `prior` | `b.<attr> = v` |
| `InsertText` | `block, at, text, marks` | Block live and takes text; text non-empty | Delete those exact characters | `text(b).insert(at, s)`, then `mark` per bit |
| `DeleteText` | `block, from, to` | Block live; both ends resolve | Reinsert runs at an anchor | `text(b).delete(at, len)` |
| `SetMark` | `block, from, to, mark, on` | Block live; both ends resolve; non-empty | `prior` marks per sub-range | `text(b).mark` or `unmark` over the range |
| `InsertPageLink` | `block, at, page` | Block takes text; id form. The target may dangle | Delete that character | `text(b).insert(at, U+FFFC)`, then `mark(pagelink = id)` with no expansion |
| `SplitBlock` | `block, at, new_block, new_kind` | Block takes text; new id unused | `MergeBlocks(new_block, block)` | Read runs after `at`, `text(b).delete` them, and do `InsertBlock` with those runs after `block`, setting `new.split_from = block` |
| `MergeBlocks` | `block, into` | Both take text; `into` precedes `block` | Delete the appended characters from `into`, then `RestoreBlock(block)` | Append runs to `text(into)`, set `b.merged_into = into`, then as `DeleteBlock(block)` |
| `ResolveReview` | `target, choice` | A marker is open; `choice` is `KeepMerged` or `KeepVersion(i)` | Reverse text edits, and restore the marker | For a version: minimal text edits to reach that snapshot. For a block kept visible by R-08: keeping it also writes `RestoreBlock`, and removing it is `DeleteBlock`. Then `map(reviews).target.resolved[device] = version(document)`. The entry is not deleted (see Marker) |

**Implicit restore.** A block that R-08 keeps visible is live, but it is not in `order`, so an op that needs an index would otherwise have nothing to work with. So **the first op on a resurrected block compiles an implicit `RestoreBlock`:** in the same commit, `mlist(order).insert` at the place R-08 computed, and `b.del_fp = null`, then the op itself. `DeleteBlock` on such a block therefore restores and deletes in one commit, writing a new `del_fp` and `del_after`. An `after` that names such a block restores it too. If two devices do this at once, R-05 keeps the first position. Rows kept by R-17 and items follow the same rule, with `RestoreRow` and `RestoreItem`.

Split and merge copy text, because a block is a separate Text container. So a concurrent edit may end up in an unexpected block. The overlap detector flags every such case, since both devices touched one block.

#### Page document: canvas and ink

| Op | Fields | Validation | Inverse | CRDT writes |
| --- | --- | --- | --- | --- |
| `SetNotePlacement` | `x, y, width` | Width above 0, in range | `prior` | `map(canvas)` |
| `AddItem` | `item, after, kind, x, y, width, height, color?, text` | Id unused, sizes above 0 | `DeleteItem` | Child map and Text, `mlist(item_order).insert` |
| `DeleteItem` | `item` | In order | `RestoreItem` | `del_fp`, `del_after`, `mlist(item_order).delete` |
| `RestoreItem` | `item, after` | Exists, not in order | `DeleteItem` | Clear `del_fp`, `mlist(item_order).insert` |
| `TransformItem` | `item, x?, y?, width?, height?` | In range | `prior` fields | One map write per field |
| `SetItemColor` | `item, color?` | Length limit | `prior` | `item.color` |
| `EditItemText` | `item, edits[]` | Item exists | Text inverse | `text(item)` |
| `ReorderItem` | `item, after` | In order | `prior` predecessor | `mlist(item_order).move` |
| `AddInk` | `ink, after, record` | Id unused. Samples decode. If a blob, it is complete | `DeleteInk` | `map(ink).id = value` (one value), `mlist(ink_order).insert` |
| `DeleteInk` | `ink` | In order | `RestoreInk` | `mlist(ink_order).delete`. The value stays |
| `RestoreInk` | `ink, after` | Value present, not in order | `DeleteInk` | `mlist(ink_order).insert` |
| `ReplaceInk` | `ink, with[]` | `ink` in order; each new record valid | `ReplaceInk` back, the old record restored | Delete one, insert several, in place. Partial erase and moving ink are this op |
| `ReorderInk` | `ink, after` | In order | `prior` predecessor | `mlist(ink_order).move` |

Concurrent erasure of one stroke by two devices keeps both sets of fragments, so part of an erased stroke can come back. Nothing is lost, and `origin` lets [43](43-CANVAS-AND-INK.md) tidy this. No review fires, since ink has no text.

#### Database document

| Op | Fields | Validation | Inverse | CRDT writes |
| --- | --- | --- | --- | --- |
| `EditDatabaseName` | `edits[]` | | Text inverse | `text(name)` |
| `SetDefaultView` | `view` | | `prior` | `map(meta)` |
| `AddStatus` | `status, after, name, group` | Id unused; group known | `RemoveStatus` | Child map, `mlist(status_order).insert` |
| `UpdateStatus` | `status, name_edits?, group?` | Exists | Text inverse, `prior` group | `text(s.name)`, `s.group` |
| `RemoveStatus` | `status, fallback` | Not the last live status; fallback live | Re-show, and restore rows from `prior` | `s.hidden = true`, `mlist(status_order).delete`, then `row.status = fallback` per affected row |
| `MoveStatus` | `status, after` | Live | `prior` predecessor | `mlist(status_order).move` |
| `AddColumn` | `column, after, name, type` | Id unused; type known | `RemoveColumn` | Child map, `mlist(column_order).insert` |
| `EditColumnName` | `column, edits[]` | Exists | Text inverse | `text(c.name)` |
| `RemoveColumn` | `column` | Live | `RestoreColumn` | `c.hidden = true`, `mlist(column_order).delete`. Cells stay, and are exported ([22](22-DATA-MODEL.md)) |
| `RestoreColumn` | `column, after` | Exists and hidden. A concurrent duplicate is harmless | `RemoveColumn` | `c.hidden = false`, `mlist(column_order).insert` |
| `MoveColumn` | `column, after` | Live | `prior` predecessor | `mlist(column_order).move` |
| `AddRow` | `row, after, name, status` | Id unused; status live | `DeleteRow` | Child map and Text, `mlist(row_order).insert` |
| `DeleteRow` | `row` | Live | `RestoreRow` | `del_fp`, `del_after`, `mlist(row_order).delete` |
| `RestoreRow` | `row, after` | Exists, not in order | `DeleteRow` | Clear `del_fp`, `mlist(row_order).insert` |
| `MoveRow` | `row, after` | Live | `prior` predecessor | `mlist(row_order).move` |
| `EditRowName` | `row, edits[]` | Exists | Text inverse | `text(r.name)` |
| `SetRowStatus` | `row, status` | Status live | `prior` | `row.status` |
| `EditCellText` | `row, column, edits[]` | Column is text | Text inverse | `text(cell:<row>:<column>)` |
| `SetCellDate` | `row, column, value?` | Column is date; ISO 8601 | `prior` | `row.dates.<column>` |
| `ResolveReview` | As for pages | | | For a cell in a removed column, keeping the cell also writes `RestoreColumn`. Leaving the column removed only resolves the marker |

#### System ops

Written by the engine, with no undo step, and never accepted from `-tools` or the bridge.

| Op | Fields | Validation | Inverse | CRDT writes |
| --- | --- | --- | --- | --- |
| `MarkReview` | `target, versions[]` | Written only by the detector | None | `map(reviews).target.versions[device] = version` per version |
| `WriteCheckpoint` | `kind, label, frontier` | Written only by the history writer | None | `list(versions).push` |

**In a server workspace,** both are ordinary change classes that a device with Edit or Manage writes ([57](57-SERVER-DESIGN.md)). A session below Edit has nothing of its own to merge, so the engine compiles neither, and shows any marker it detects locally. So a returning device is never refused, and never sent to a restored copy, for its merge bookkeeping.

**Op kinds carry no text in the log summary.** The op list is compiled away, and the frame stores only the CRDT update and a count per kind ([23](23-STORAGE-FORMAT.md)). So the log is one truth, and not an op list and a CRDT list that could disagree.

### Undo over merged state

Undo is **local:** it reverts this device's own steps, and never travels as "undo". It compiles to a new ordinary batch with `origin: Undo`, and peers merge it like any edit. The CRDT is never rewound and the live document is never checked out to an old version. OpenCalc's docs/69 and OpenSlide's docs/24 shaped this, with one difference: last-writer-wins fields are not clobbered here.

```rust
pub struct UndoStep {
    pub id: StepId,
    pub group: Option<GroupId>,
    pub parts: Vec<(DocRef, Vec<Inverse>)>,
    pub label: StepLabel,
}

pub enum Inverse {
    DeleteOwnSpan { target: TextRef, span: SpanId },                // the characters this step created
    Reinsert { target: TextRef, at: Anchor, runs: Vec<Run> },       // what this step deleted
    SetField { path: FieldPath, prior: Value, mine: Value },        // only if still mine
    Restore { kind: EntryKind, id: Id, after: Option<Id>, fp: Fp }, // RestoreBlock and its kin
    Remove { kind: EntryKind, id: Id, fp: Fp },                     // the inverse of an insert
    Move { kind: EntryKind, id: Id, after: Option<Id>, token: PositionToken },
    ResolveBack { target: TargetKey, marker: ReviewMarker, edits: Vec<Inverse> },
}

pub enum UndoOutcome { Applied, Partial { skipped: Vec<Skip> }, Refused { why: RefuseReason } }
```

- **Scope.** One stack per document, in memory only, cleared on close, capped at 1,000 steps or 64 MiB. A `group` sits on each stack it touches, and undoing it from one removes it from the rest. The focused surface picks the stack: a page's own, or the tree's from the sidebar.
- **Coalescing.** `StepHint::Coalesce` joins a step when it is the same block, contiguous, and within 1 second. Word boundaries are not used, because they do not exist in the same way in Chinese, Japanese or Thai. A composition is one step: nothing is written until `compositionend` ([42](42-EDITOR-AND-RENDERING.md)).
- **Reporting.** `Partial` and `Refused` always tell the person what happened and why, with a message from [50](50-INTERNATIONALIZATION.md). Undo that does nothing silently is a known trap (OpenCalc docs/69). A step that did nothing is removed from the stack.
- **Redo** is a new intention, not a claim about the past, as OpenCalc reasons. It applies the inverse of the undo, keeping the original ids (for an insert, that is `RestoreBlock`), and refuses only if a target is gone.

**The corners.**

| # | Situation | Behaviour | Reason |
| --- | --- | --- | --- |
| U-1 | I insert text, a peer types inside it, I undo | Only my characters go; the peer's stay in place | Characters have identity. **Needs an adapter primitive, unverified:** see below |
| U-2 | I delete text, a peer edits around it, I undo | Reinserted at the anchor, with new character identity | The anchor sits on the deleted range's neighbours and survives |
| U-3 | I delete text, a peer also deleted part of it, I undo | All my deleted text returns, including the part the peer also deleted | Undo puts back what I removed. Nothing is lost, and the peer can delete it again |
| U-4 | I set a checkbox or status, a peer changes it later, I undo | Skipped and reported (`Partial`) | Reverting would silently destroy a peer's change that I never saw. **This differs from OpenCalc,** which clobbers a cell: after a 90-day merge, "someone else's" can be months of work, so we report instead |
| U-5 | I insert a block, a peer types in it, I undo | Refused and reported | `fp` differs from what I left. OpenCalc's structural rule: the peer's history has no copy of this |
| U-6 | I insert a block, a peer moves it, I undo | Removed | Its text is still mine alone |
| U-7 | I delete a block, a peer edits it, then R-08 keeps it visible, I undo | `Restore` finds it visible, clears `del_fp` and the marker, and it stays where it is | Idempotent |
| U-8 | I delete a block, and my anchor block is gone, I undo | Placed by the `del_after` chain, as R-08 | Same algorithm, so same result on every replica |
| U-9 | I move a block, a peer moves it later, I undo | Skipped and reported | `PositionToken` differs. Moving it back would override a newer intention |
| U-10 | I split a block, a peer typed in the tail, I undo | `MergeBlocks` runs and the peer's text survives in a merged block. It stays flagged if a marker was opened | Text kept over tidy result |
| U-11 | My step's page is in the trash | Refused, naming the page | Undo must not silently edit something invisible |
| U-12 | My step's page was purged, or its target is before a trim point | Refused. An anchor that cannot resolve means refuse, never guess | |
| U-13 | A step has several ops, some now stale | Stale last-writer-wins ops are skipped and reported. A stale structural op refuses the whole step | OpenCalc rejected "narrow" as a state nobody asked for. We allow it only for independent fields, because refusing whole steps after a long merge would make undo useless |
| U-14 | Undo of an assistant batch (`TOOL-02`) | One step, as any other, and it may be `Partial` | ADR-013 |
| U-15 | Undo of an import into an existing notebook | Trashes the new pages | The import was one step. Replacing a page is restored from its checkpoint |
| U-16 | Undo while a batch from a peer is waiting to import | The import runs first, then the undo | Undo always sees the merged state |
| U-17 | A restore of an old version | Undoable, as one step | It is an ordinary batch, see History |
| U-18 | Undo after `ResolveReview` | Text restored, and this device's `resolved` entry removed, so the marker reopens | Choosing is a normal edit |
| U-19 | Crash | The stack is gone, and the notes are not | Stacks are not persisted. Version history is the longer-term recovery |

**Why U-1 is the hard one.** Deleting an anchored range would also delete a peer's characters inside it. So `DeleteOwnSpan` must delete by character identity. `SpanId` wraps the peer and counter range the adapter recorded at insert. Whether Loro's public API allows deleting by that identity is **unverified**. If it does not, the options, tested in `SPK-01`, are:

1. Compare the text at the step's own version against now, using `fork_at`, and delete only characters that existed then. Slower, and needs no new primitive.
2. Use Loro's own `UndoManager`, described in its docs as handling local undo with remote changes. **Unverified here.** It would remove much of this section, and see the argument against the design below.

### The `-merge` adapter

This is the only crate that names the library (`GOV-04`). Nothing from Loro appears in its signatures. Its surface is deliberately small enough to re-implement.

```rust
pub trait MergeDoc: Sized {
    fn new(class: DocClass, peer: PeerId) -> Self;
    fn load(class: DocClass, peer: PeerId, base: Option<&[u8]>, limits: &ImportLimits)
        -> Result<Self, MergeError>;

    /// One commit. Returns the bytes for the log.
    fn apply(&mut self, writes: &[Write]) -> Result<Update, MergeError>;

    /// Never panics to the caller; see guarded import.
    fn import(&mut self, updates: &[&[u8]], limits: &ImportLimits) -> Result<ImportOutcome, MergeError>;

    fn version(&self) -> Version;           // opaque bytes
    fn has_ops_beyond(&self, v: &Version) -> bool;  // is any op here outside the causal past of v?
    fn updates_since(&self, v: &Version) -> Result<Update, MergeError>;
    fn snapshot(&self, kind: SnapshotKind) -> Result<Vec<u8>, MergeError>;

    fn read(&self) -> RawDoc;               // for full projection and repair
    fn read_changed(&self, since: &Version) -> RawDelta;

    fn at(&self, v: &Version) -> Result<Self, MergeError>;  // a read-only fork, never a checkout
    fn touched(&self, from: &Version, to: &Version) -> Vec<Touched>;
    fn anchor(&self, t: TextRef, at: u32, side: Side) -> Anchor;
    fn resolve(&self, a: &Anchor) -> Option<u32>;
    fn token(&self, list: ListRef, id: &str) -> Option<PositionToken>;

    fn trim_to(&mut self, v: &Version) -> Result<Vec<u8>, MergeError>;  // only -sync calls this
}

pub enum Write {
    MapSet { path: Path, key: Key, value: Value },
    MapDel { path: Path, key: Key },
    TextInsert { text: TextRef, at: u32, s: String },
    TextDelete { text: TextRef, at: u32, len: u32 },
    TextDeleteSpan { text: TextRef, span: SpanId },
    TextMark { text: TextRef, at: u32, len: u32, key: Key, value: Option<Value> },
    ListInsert { list: ListRef, at: u32, id: Id },
    ListMove { list: ListRef, from: u32, to: u32 },
    ListDelete { list: ListRef, at: u32 },
    TreeCreate { node: NodeKey, parent: Option<NodeKey>, at: u32 },
    TreeMove { node: NodeKey, parent: Option<NodeKey>, at: u32 },
    TreeDelete { node: NodeKey },
}
```

Rules:

- **`Update`, `Version`, `Anchor`, `PeerId` and `SpanId` are opaque** to everything above `-merge`.
- **`-merge` has no product meaning** beyond the layout, repair input, and overlap detection, which 19 assigns to it. "Block", "row" and so on appear only in `layout` and `touched`.
- **Marks are configured with no expansion for `pagelink`,** and with expansion after for `bold`, `italic`, `code` and `highlight`. Mark ranges for link characters must stay on one character. Unverified against Loro's text style configuration.
- **Peer ids.** Loro's peer id is a 64-bit value that must not be reused concurrently (from memory of its docs, unverified). **A fresh random peer id is minted each time a notebook is opened for writing,** so counters are never reused after a crash, a restored backup or a copied folder ([23](23-STORAGE-FORMAT.md)). This grows the peer table with every open, and `SPK-01` measures that cost. Import is **not** assumed to merge two branches that share a peer id and overlap in counters, which is unverified and is a named `SPK-01` check. A detected fork is salvaged as a restored copy ([56](56-SYNC-DESIGN.md)).

**Guarded import** (`MRG-02`). Every remote update is hostile (QR-4), and panics are reported in Loro.

| Step | Action | Result |
| --- | --- | --- |
| 1 | `-store` has checked framing and limits, and `-crypto` has authenticated it | |
| 2 | Size check against `ImportLimits` | Over: `CN-MRG-0002` |
| 3 | Add one attempt to the segment's count in the poison marker ([23](23-STORAGE-FORMAT.md)), durable | A crash or trap now names the segment on restart, and is counted |
| 4 | For updates above 1 MiB, import into a fork. Below that, into the live document | A bad large update cannot hurt the live state |
| 5 | Run under `catch_unwind` on native | Panic: instance dropped, reloaded from durable state, `CN-MRG-0003`. The attempt stays counted, and the segment is retried at the next open or round |
| 6 | On wasm, a panic traps by default and `catch_unwind` cannot help | The host restarts the worker. Step 3's count is read on restart. Same outcome, slower |
| 7 | Check the resulting projection against [22](22-DATA-MODEL.md) limits | Over: refuse the fork's result, `CN-MRG-0004`. Under 1 MiB, the limit trips later and is handled as a visible warning |
| 8 | Remove the segment's count | |

An update with missing dependencies stays pending inside the library (from memory, unverified). It is never an error: the state panel reports the document as waiting on a device (PR-2).

**Crashes are counted, not assumed to be the segment's fault.** A power cut during an import looks the same as a poisonous segment. So an import that dies leaves the segment in place, nothing is acknowledged, and recovery imports it again. That is safe only because import is idempotent, and whether Loro's is after a crash part-way through is unverified (`SPK-01`).

| Attempts on one segment | Whose segment | Result |
| --- | --- | --- |
| 1 or 2 | Any | Retried |
| 3 | Another device's | Quarantined, `CN-STO-0016` |
| 3 | **This device's own** | **Never quarantined automatically.** Its document blocks with a visible state, `CN-STO-0021`, and export is offered. Own segments may hold the only copy of an edit |

A quarantined segment is not deleted. It is visible in the sync panel ([56](56-SYNC-DESIGN.md)), with the device it came from, and a person can retry it. The rest of the notes keep working. `ImportOutcome` is per document, because a `MergeDoc` is one document, and [56](56-SYNC-DESIGN.md) keeps that outcome per document within a sync object.

### Projection

Projection turns CRDT state into Valid values, on every path. `-merge` reads Raw, and `-model`'s `repair_*` makes it Valid.

```rust
pub struct Delta {
    pub rev: ProjectionRev,
    pub blocks: Vec<BlockChange>,    // Inserted, Updated, Moved, Removed, each with its Valid value
    pub tree: Vec<TreeChange>,
    pub canvas: Vec<CanvasChange>,
    pub reviews: Vec<ReviewChange>,
}
```

| Path | When | Cost target |
| --- | --- | --- |
| Full | Page load, after a guarded fork swap, and after a reload | Proposal: under 100 ms for a typical page (`SPK-01`) |
| Incremental | After an op or an import. `read_changed` gives touched ids, and only those blocks are repaired | Must fit QR-1 |
| Positional rules (R-05, R-06, R-08) | When `order` or a block's delete record changes | Local to the order list |

`ProjectionRev` is a per-instance counter, not a CRDT version, and is valid only in this process. It tells the editor which projection a `Delta` follows. No op carries it.

**The state hash** is per document, and is defined in [`-model`](22-DATA-MODEL.md). It covers all CRDT-held state, read as Raw, and it is not package JSON. Two replicas with equal hashes for a document hold the same state for it. `SPK-01`, snapshots and the sync suite all compare this.

Derived work (mirror, search, vectors, graph) subscribes to `Delta` off the typing path and never blocks it (PR-4).

### The overlap detector

**Why it exists.** Joplin [tried word-level merging and dropped it because it could silently duplicate text](https://github.com/laurent22/joplin/blob/dev/readme/dev/spec/conflict_auto_merge.md). A CRDT merges at a finer grain still, and two devices rewriting one sentence offline will interleave. So the failure is available to us, and "the CRDT converged" is no comfort: converged and readable are different claims. We keep the merge, because PR-1 says offline edits are never dropped, and we make it visible.

**A block is flagged when two devices changed it without seeing each other's edits, as defined below.** Real-time typing never triggers it.

**Input.** Detection runs once per import session for a document, after all segments waiting for it have been imported, on the importing device (`MRG-03`).

```rust
pub enum Channel { Live, CatchUp { gap_ms: u64 } }
/// `before`: this document's version when the session began.
/// `remote`: the causal closure of what was imported, from `ImportOutcome`.
pub fn detect(doc: &impl MergeDoc, before: &Version, remote: &Version, channel: Channel) -> Vec<Overlap>;
```

**Algorithm.**

1. If `channel` is `Live`, stop. The live channel ([57](57-SERVER-DESIGN.md)) reports `Live` only after catch-up has completed, and both sides see each other's edits within moments.
2. If `CatchUp` with `gap_ms` under 30 seconds, stop. A brief network drop is not a person working unseen. `gap_ms` comes from the device's monotonic clock, and is never read from update metadata. The 30 seconds is a proposal, tuned by `SPK-01` (RISK-03).
3. `base` = the per-peer minimum of `before` and `remote`. Causally closed sets intersect to a causally closed set, so `base` is what both sides had seen.
4. `L = touched(base, before)`, what this device had that the other side had not seen. `R = touched(base, remote)`, what arrived that this device had not seen.
5. For each target in both `L` and `R`, apply the table.
6. Build each marker: the text at `before` and the text at `remote`, using `at()`. Cap and write as below.

Detection trusts only what `touched` derives from the CRDT history. A device's claims in frame metadata are never used, so a hostile peer cannot suppress a flag through its summary.

| This device | Other device | Flag? | Why |
| --- | --- | --- | --- |
| Text | Text | **Yes,** unless the merged text equals both versions | The case Joplin warned about. Same block, wherever in it. Two versions can be identical while the merge holds the text twice, so the merged text is what is compared |
| Text | Delete | **Yes** | R-08 has kept it visible, and a person decides |
| Delete | Delete | No | Same intention |
| Text | Marks only | No | Merges cleanly, and no text changes |
| Marks | Marks | No | Last writer per mark; nothing is hidden |
| Same field, different values (`checked`, `level`, `status`, date) | | **Yes** | One value lost as last writer. Show both |
| Same field, same value | | No | |
| Text or date cell | Removal of its column | **Yes,** as R-08 | The cell's value is kept, but it is hidden with its column. The marker shows it, and keeping it writes `RestoreColumn` |
| Different fields | | No | Both kept |
| Move | Anything except delete | No | No text at stake. Position is one writer's |
| Kind change | Text, into a kind without text | **Yes** | R-12 or R-13 applied |
| Split or merge | Text in the same block | **Yes** | Text was copied, so duplication is possible |
| Both split, or both merged | | **Yes** | This duplicates text, the exact Joplin failure. Detected as below |
| Ink, item geometry | Same | No | No text at stake |

The same table applies to titles, row names, text cells, date cells and canvas item text.

**Detecting both split or both merged.** The op list is compiled away, so this reads CRDT state. `SplitBlock` sets `split_from` on the new block, and `MergeBlocks` sets `merged_into` on the removed one.

| Case | Test, on `touched` |
| --- | --- |
| Both split | `L` holds a new block with `split_from = b`, and `R` holds a different new block with `split_from = b`. Flag `b` and both new blocks |
| Both merged | `L` and `R` both wrote `merged_into` on one block, whatever the values. This is an exception to "same field, same value". Flag the block and each `into` |

A hostile peer that leaves these fields out still edits `b`'s text, so the Text against Text row flags it anyway.

**Edits after trash.** The detector sees one document at a time, and trash is in the tree while edits are in the page. So this check compares the two: `TrashPage` records the page document's version as `trash_base`, and the page is **edited in the trash** when its document `has_ops_beyond(trash_base)`. That is any edit concurrent with or later than the trash, whatever the clocks say. It is derived, so every device computes it after importing either document, and a missing `trash_base` counts as edited.

- `PurgePage` refuses such a page with `CN-OPS-0009`, on every device, and without a timer that overrides it.
- Trash and the state panel surface the page, naming the devices that edited it. The person restores it, or trashes it again from what they can now see. That writes a new `trash_base` and starts the 30 days again.
- A device acknowledges the tree segment holding a trash only after it has written its own edits to that page ([56](56-SYNC-DESIGN.md)), so the check at purge sees them.

**Marker.**

```rust
pub struct ReviewMarker {
    pub target: TargetKey,                   // Block, Title, RowName, Cell, ItemText, Field
    pub versions: Vec<ReviewVersion>,        // up to 4
    pub opened_at_ms: i64,                   // informative
}
pub struct ReviewVersion { pub device: DeviceId, pub seen: Version, pub at: Option<Version>, pub runs: Option<Vec<Run>> }
```

- **The marker stores the text of each version, up to 64 KiB per version,** so it does not depend on history that may later be trimmed, and it is library-neutral. Above that it stores only `at`, and pins trimming until resolved (`SYN-05`).
- **It is synced as ordinary CRDT data,** written by `MarkReview`. Two devices detecting the same overlap write the same target. That is harmless, since each writes its own device key under `versions`. A device that had no concurrent edit sees the marker because the other device wrote it, which keeps it in view on every device.
- **A resolved review stays resolved.** Deleting the entry would let a late `MarkReview` from an import bring it back, so resolving records state instead: `resolved[device]` holds the version that device resolved through, and `seen` holds the version a `ReviewVersion` was detected at. A marker is **open** while some version's `seen` has ops beyond every `resolved` entry. So a late import of the same overlap stays closed, a new overlap opens it, and two devices resolving at once cannot clobber each other. `-merge` computes this for `RawPage`, because versions are opaque above it. Resolving clears `runs`, and compaction may clear the entry once the floor has passed it ([56](56-SYNC-DESIGN.md)).
- **It never blocks.** The merged text is already in the block, in the page, in search and in export. Review is a mark in the margin that can be left for ever ([47](47-UX-AND-FEATURE-MAP.md), `SYN-04`).
- **Choosing a version** is `ResolveReview`, a normal undoable edit. "Keep merged" only records the resolved state.

**A burst.** After a 90-day merge, the first 500 markers per document are written. The rest are counted in one page-level marker, "N more blocks changed on both devices", and listed in the merge event. They are never dropped silently. Detection runs off the typing path with progress in the state panel (PR-2).

Three devices: the versions list holds each. A chain A to B to C is flagged where the overlap is first seen, and later devices receive the marker. If device C also had an unseen edit in that block, it adds its version.

What the detector cannot catch: two devices typing the same sentence into different blocks. Nothing merged wrongly, so that is not a merge fault.

### Why there is no transform

ADR-005 has no `transform` function, and `-ops` must never own one (19). Reasons:

- A CRDT position is an identity, not a number, so an op from 90 days ago needs no rewriting to land correctly.
- OpenCalc's docs/56 retains operations for a bounded window, which it calls the definition of bounded offline, and names long offline as the condition that would supersede its OT choice. That condition is routine here.
- Two convergence mechanisms could disagree, and principle 3 of [02](02-ARCHITECTURE.md) rules that out.
- The server can relay and store without interpreting ops, which is what the end-to-end encrypted mode needs ([57](57-SERVER-DESIGN.md)).

**Three places that look as if they need one, and what they do instead.**

| Place | Instead |
| --- | --- |
| The editor maps a DOM change against a view that a remote import has just moved | The op carries anchors and `client_seq`, and lands by identity, so there is nothing to re-map. A remote patch still never lands inside an active composition; it waits ([42](42-EDITOR-AND-RENDERING.md), `I18N-03`). If the target is truly gone, `CN-OPS-0002` and the recovery path keep the text |
| The assistant holds a block while waiting for approval, and the block changes | The tool's write carries the block's fingerprint, and a mismatch refuses with `CN-OPS-0004`. The tool re-reads and re-asks ([63](63-AGENT-TOOLS-AND-MCP.md)). Text is never applied by position against moved content |
| Undo against a moved document | Anchors, identity and tokens, as above. If `SPK-01` finds this is really a hand-written transform, that counts against the design |

### History and version events

FR-12: version history per page with restore, where merges appear as named events (`STO-05`).

```rust
pub struct Checkpoint {
    pub id: CheckpointId,
    pub kind: EventKind,
    pub at_ms: i64,        // host clock, for display only
    pub device: DeviceId,
    pub frontier: Version,
    pub label: String,     // ICU message key and arguments, never translated text
}

pub enum EventKind {
    Session,                          // edits, coalesced
    Merge { from: Vec<DeviceId>, changed: u32, flagged: u32 },
    Restore { to: CheckpointId },
    Import, AssistantWrite { tool: String }, PartialGroup { group: GroupId },
    RepairNoted, Trim,
}
```

- **Written to the document's `versions` list,** so they sync, and survive log compaction. They are small and few: one `Session` per 5 idle minutes or on page close, and one per merge, restore or import.
- **Ordered by causal order, then device.** Host clocks are shown as a time and never used to order, since they can be wrong by years.
- **The labels are keys, not language.** So history reads in the viewer's interface language, not the writer's.
- **Viewing a version** is `at(frontier)`, a read-only fork, projected and repaired like any other. The current document is untouched.
- **Restore never rewinds.** It computes the difference between the current Valid page and the old one, by block id, and writes ordinary ops (`InsertText`, `DeleteText`, `RestoreBlock` and the like) as one batch with `origin: Restore`. So restoring merges cleanly with whatever a peer is doing, is undoable, and is a named event. Text differences use a Myers diff over scalars.
- **Versions before a trim point** are listed with their labels, and cannot be opened. The interface says so ([56](56-SYNC-DESIGN.md)).
- **A checkpoint's `frontier` is library-specific.** Replacing Loro invalidates old ones, handled below.

### How to replace Loro

The design tries to keep this a bounded job.

| What | Depends on Loro? | On replacement |
| --- | --- | --- |
| `-model`, `-ops`, the package, the mirror, search, graph | No | Unchanged, except `Inverse` internals if `SpanId` changes |
| `-merge` | Yes, by design | New implementation of `MergeDoc`, and the `layout` module |
| Segment framing | No. `crdt_codec` names the payload ([23](23-STORAGE-FORMAT.md)) | Unchanged |
| Update and snapshot payloads | Yes | Migrated, below |
| Checkpoint frontiers | Yes | Old ones become labelled, view-only through the old adapter |

**Procedure.**

1. Implement `MergeDoc` over the new library. Run the same conformance suite: convergence, every corner in Undo, the labelled overlap histories, the guarded import fixtures.
2. Add a migration: for each document, project through the old adapter, and replay the Valid state through `-ops` into a new document. This is the same path as package import, and needs no part of Loro's history. Write it as a new base snapshot with the new codec, on a new **sync epoch** recorded in `manifest.json`.
3. Old segments are archived and not deleted. The old adapter stays in the build, read-only, until every registered device has moved. It reads old history for "view old version" and old offline work.
4. **A device still on the old epoch with unsynced edits.** Its edits are diffed against the migration point, using the old adapter, and written as a batch on the new epoch, flagged for review by the same table. Character-level merging across the epoch is lost, and block-level review replaces it. No edit is dropped.
5. Bump the storage major version, and refuse older readers with `CN-STO-0002`.

**What is not preserved:** character-level history across the epoch as live merge state, and any peer ids.

**The fallback named by `SPK-01` is yrs with our own tree.** The audit found no native tree, move or trim in it ([12](12-OPEN-SOURCE-AUDIT.md)). So `TreeCreate`, `TreeMove`, `ListMove` and `trim_to` would have to be built: a move-safe tree in a map with cycle repair (R-24 is already there for this), fractional-index ordering for blocks, and either no trim or our own. That is a real cost, and is why `Write` is expressed in terms of operations rather than Loro's containers.

### Loro, as a dependency

| Fact | Consequence |
| --- | --- |
| MIT. No express patent grant ([12](12-OPEN-SOURCE-AUDIT.md)) | Passes the licence gate (ADR-023). Patents are for `DEC-08` |
| Native movable tree, movable list, text with marks | Why it was picked (ADR-006) |
| Shallow snapshots trim history. The docs say peers "can only sync if they have versions after the shallow snapshot point", and advise coordinating before trimming | Trim rules in [23](23-STORAGE-FORMAT.md) and [56](56-SYNC-DESIGN.md). `trim_to` is called by `-sync` alone |
| 24 unsafe sites in core crates | Our `forbid(unsafe_code)` does not reach it. Recorded in ADR-006 as 19 requires, and fuzzed through guarded import |
| Wasm build about 3.6 MB | Counts against the web app's load budget ([30](30-PERFORMANCE-TARGETS.md)) |
| `loro-internal` pins `wasm-bindgen =0.2.100` | `-wasm` must use that exact version, so one dependency fixes our whole web bridge's version. `GOV-05` flags a change |

The version is pinned exactly. An upgrade is a change with its own run of `SPK-01`'s corpus, since an upgrade could change encoded bytes or merge behaviour.

## Failure modes and limits

### The keystroke order

The keystroke flow in [02](02-ARCHITECTURE.md) shows the fsync before the projection and DOM patch. Read literally, that puts a disk flush inside QR-1's 16 ms, which cannot be promised on every device. This design decouples them, within PR-1: the text is already on screen because the browser put it there, projection follows at once, and only the word "saved" waits for the flush. Group commit bounds the flush rate ([23](23-STORAGE-FORMAT.md)). This is raised as a contradiction to resolve in 02, and `SPK-02` measures it.

### Refusal codes

| Code | Meaning |
| --- | --- |
| `CN-OPS-0001` | Validation failed; the details name the op and field |
| `CN-OPS-0002` | The op's target is gone or invalid. Typed text goes to the recovery path ([42](42-EDITOR-AND-RENDERING.md)) |
| `CN-OPS-0003` | Batch too large: more than 10,000 ops or 16 MiB of text. Split into a group |
| `CN-OPS-0004` | Stale fingerprint on a tool write |
| `CN-OPS-0005` | Op schema version newer than this build |
| `CN-OPS-0006` | System op from a caller that may not issue it |
| `CN-OPS-0007` | Undo refused; the reason is included |
| `CN-OPS-0008` | Purge refused: the 30 days are not up, or a device has not acknowledged |
| `CN-OPS-0009` | Purge refused: the page has an edit concurrent with or later than its trash |
| `CN-OPS-0099` | Compile failed after validation passed. A bug, reported, and the document is reloaded |
| `CN-MRG-0001` | Could not load a document. Its segments stay, and the document shows as unavailable |
| `CN-MRG-0002` | Update over the import limit: 256 MiB per update, proposal |
| `CN-MRG-0003` | Import panicked or trapped. Counted against the segment, retried, and handled after 3 attempts as in Guarded import |
| `CN-MRG-0004` | Merged state would break a [22](22-DATA-MODEL.md) limit |
| `CN-MRG-0005` | Update predates this document's trim point. Goes to the restored-copy path (`SYN-05`) |
| `CN-MRG-0006` | Library invariant failure or unexpected error |

### Other failure modes

| Failure | What happens |
| --- | --- |
| Two sessions share a peer id | With a fresh peer per open, this takes a process cloned while running, such as a resumed VM snapshot. The effect on Loro is unverified, and `SPK-01` injects it. The chain check notices, and the divergent branch becomes a restored copy ([56](56-SYNC-DESIGN.md)) |
| Divergence between replicas in the field | Devices compare state hashes, if [56](56-SYNC-DESIGN.md) adopts this. A mismatch is reported in the state panel and never papered over by another merge |
| Memory growth, as reported for Loro and others | Documents are unloaded when not in view, and wasm memory does not shrink, so the host recycles the worker at a threshold. `SPK-01` measures it |
| Review fires too often | Text against text, same block, is the rule. A tuned threshold or a sentence-level refinement is the response, based on `SPK-01` (RISK-03) |
| Review fires too rarely | Duplication stays unseen. Labelled histories test this |
| A long catch-up is slow | Runs off the typing path, with progress. Import batches yield between segments |
| A clock is wrong by years | Nothing depends on it |
| An older device reads a newer block type | `Unknown`, retained. It can still merge, since CRDT data is preserved |

## Alternatives considered

### The strongest argument against this design

It should be weighed seriously, and `SPK-01` must test it rather than assume it away.

1. **Validating ops says nothing about merged state.** Two valid ops can merge into an invalid page. So `-ops` validation guards only local entry, and the true guarantee comes from repair at read time. Validation could be argued to be duplicating effort, since repair must exist anyway.
2. **Undo inverts against state that has moved.** The inverse was computed against a state that no longer exists, and the corners table above is long because of it. It takes real care to avoid writing a hand-made transform under another name.
3. **Loro has open panic and leak reports,** including [1216](https://github.com/loro-dev/loro/issues/1216), [1262](https://github.com/loro-dev/loro/issues/1262) and [1254](https://github.com/loro-dev/loro/issues/1254). They are reported and not reproduced here, and no source covers years of real notes ([12](12-OPEN-SOURCE-AUDIT.md)). The library is on the only write path of a product whose first promise is that nothing is lost.
4. **If `-ops` turns out to be a thin rename of Loro calls, drop it.** A layer that adds no meaning adds code to maintain, a false sense of safety and a slower path. Loro's own `UndoManager` (unverified) might do undo better than the corners above.

**The test.** `SPK-01` implements a representative slice of `-ops` and reports:

- how many ops compile to more than one write or carry a check that repair could not make;
- how many undo corners pass with our inverses, and how many with Loro's `UndoManager`;
- whether `DeleteOwnSpan` needed library internals.

**The decision rule, a proposal.** If most ops are one write with a trivial check, and the library's undo passes the same corners, `-ops` shrinks to validation and semantic ops (`SplitBlock`, `MergeBlocks`, `DeleteBlock` with its fingerprint), and hand-written inverses go. Boundary rule 2 in 19 stays: there is still one write path.

What the layer is expected to earn, if it stays: `del_fp` and `del_after`, split and merge, status removal with fallback, approval and stale-write checks, limits, and a place for a replacement library to plug in.

### Others

| Alternative | Why not |
| --- | --- |
| Server-ordered OT, as OpenCalc | Long offline is routine, and its docs/56 names that as the case it does not fit |
| yrs now | None of native tree, move or trim was found, and it has 69 unsafe sites. It remains the fallback |
| Automerge | No native tree found, and no trim found ([12](12-OPEN-SOURCE-AUDIT.md)). Wasm size is comparable. Not ruled out as a second fallback, and its near-zero unsafe is attractive |
| One document for the whole workspace | Load time and memory scale with everything ever written |
| One document per block | Too many documents, and moving between blocks and pages gets harder, for a small benefit |
| Word-level merge with no marking | Joplin's experience |
| Conflict copies, as Standard Notes does | A chore, which is the gap [12](12-OPEN-SOURCE-AUDIT.md) names |
| Last writer wins per block | Silently drops one device's text, breaking PR-1 |
| Storing the semantic op log beside CRDT updates | Two truths. The CRDT is what merges (principle 3). Op counts are enough for history |
| A global undo stack across devices | Undo would undo someone else's work. Also needs a wire concept |
| Writing review markers only locally | Each device would have to detect separately, and a device with no concurrent edit could never see it |
| Blocking merge until reviewed | Against PR-1's "merged text is never hidden" and against local-first use |

## Acceptance gates

| Gate | Method | State |
| --- | --- | --- |
| Every op has a validation test and an inverse test | Table-driven from the tables above (`OPS-01`) | Not built |
| Any delivery order gives the same state hash | Generated histories with shuffled, duplicated and delayed updates | Not built |
| Every undo corner behaves as listed | One named test per U-n, plus generated concurrent undo (`OPS-02`) | Not built |
| Hostile updates are refused, and none panics past the guard | Fuzz target, plus the reported Loro cases (`MRG-02`) | Not built |
| Layout rule L-1 holds | Generated concurrency on every text-bearing field: no text vanishes | Not built |
| Only `-merge` names `loro` | `check-crate-deps.py` (`GOV-04`) | Not built |
| Detector matches labelled histories, and live typing never flags | `MRG-03` | Not built |
| Restore round trip | Restoring, then comparing the Valid page with the checkpoint's. The state hash differs by design, because restore writes new CRDT state (`STO-05`) | Not built |
| A long composition survives remote imports | Composition held while updates arrive, with no refusal and no loss; own in-flight ops never go stale (`OPS-01`, `I18N-03`) | Not built |
| A refused op's typed text reaches the recovery path, with later dependent batches refused in order | One test per kind of gone target | Not built |
| A crash during import quarantines nothing before the third attempt, and never an own segment | Injected crashes and traps (`MRG-02`) | Not built |
| A page edited in the trash cannot be purged | Generated histories across tree and page documents | Not built |
| A resolved review does not reopen on late import, and a new overlap does open it | Shuffled delivery of `MarkReview` and `ResolveReview` | Not built |
| Package written from any merged state passes the schema | Generated | Not built |

### SPK-01: does Loro survive years of notes?

**Settles** ADR-005, ADR-006 and ADR-010 (RISK-01, RISK-03). It comes before `MRG-01`, which is blocked on it. It is throwaway code in a scratch crate: it may name `loro` directly, and it must not become product code. No result below exists yet.

**Inputs.**

| Input | Definition (workload parameters, proposals) |
| --- | --- |
| Duration | 5 simulated years, with simulated clocks |
| Devices | 3: a desktop online most days, a laptop online on weekdays, a tablet running the wasm build |
| Offline periods | **The tablet is offline for 90 days,** editing pages others are also editing. A second window has the laptop offline for 14 days overlapping it, so three-way merges occur. Many short gaps of 1 minute to 3 days |
| Text | **Real notes in the launch languages** (`DEC-09` recommendation): English, Simplified and Traditional Chinese, Japanese, Korean, Spanish, French, German, Portuguese, Arabic, Hindi. Sources must have licences that let them be kept as test fixtures. Mixed-language pages included. Not lorem ipsum |
| Edit shape | Recorded or modelled human editing: typing in bursts, backspacing, corrections, cut and paste, CJK arriving as composition commits, Hindi and Arabic clusters |
| Scale | 500 pages, about 2,000 sessions a year per device, a typical page of 200 blocks, one large page of 5,000 blocks, one database with 10,000 rows and 20 columns, and canvases with 5,000 strokes |
| Structure | Page moves, page nesting, trash, restore, duplicates, split, merge, reorders, status removal, column changes |
| Faults | Duplicated, reordered and delayed updates; a killed process mid-commit; a restored backup; the peer id events in [23](23-STORAGE-FORMAT.md) |
| Seeds | Deterministic and committed, so any failure replays exactly |

**Measured**, on native and in Chrome's wasm, on a stated reference laptop, recording hardware and versions:

| Measure | Pass |
| --- | --- |
| **Divergence:** state hash per document across the 3 devices after each sync, under shuffled delivery, and native against wasm | **None.** Zero mismatches |
| **Panics:** over the full run, the three reported Loro issues attempted, and a fuzz run on imports | **None.** Any panic in normal history fails. A panic only on hostile input fails too, unless guarded import contains it with no state damage, and that is written up |
| **Load:** a typical page after 5 years, snapshot plus tail, to Valid projection, in wasm and native | **Under 100 ms** at the 95th percentile |
| **Storage:** on-disk bytes for all documents divided by the UTF-8 bytes of live text plus compact stroke samples | **Under 10x.** Attachments excluded. The denominator is live state, the strict reading. Bytes ever typed are reported beside it, and the owner may rule on which was meant |
| **Review precision and recall:** on a labelled set where people mark real overlaps, for the full histories and for live typing | **The marker fires only on real overlaps:** no false flag on the labelled set, and no flag at all on simulated live typing. Missed real overlaps are also counted as failures |

**Also reported.** These do not pass or fail alone, but decide the follow-on design:

| Question | Why |
| --- | --- |
| Memory over the run, including wasm's peak and growth after repeated import | The leak reports |
| Import cost for the 90-day merge | Whether the catch-up plan holds |
| Time for one commit plus exporting its update | Whether it fits the typing path |
| Peer table growth with a fresh peer per open | The cost of the peer id policy. A cost too high to carry goes back to the owner, with the fork handling unchanged |
| Root container scaling, on the text cells | Rule L-1 |
| `at()` fork cost, on old versions | Detector and history |
| Whether characters can be deleted by identity | U-1, and ranges that keep what they did not see |
| Stable cursors: whether an anchor survives remote edits and deletes, what it then resolves to, and the cost after years | Ops land by anchor |
| What import does with two branches sharing a peer id and overlapping in counters: merge, error or corruption | Not assumed to merge. The answer says how urgent the running-clone case is |
| Whether import stays idempotent after a crash part-way through | Crashes are retried, not quarantined |
| Whether reading Raw gives the same order on every replica | The state hash |
| All undo corners, with our inverses and with Loro's `UndoManager` | The argument against the design |
| Number of ops that are thin | The drop rule for `-ops` |
| Whether encoded updates are the same bytes on native and wasm | `STO-03` |
| How merged, interleaved text actually reads, judged by a person, per language | RISK-03. Numbers cannot answer this |
| Whether Loro's documented trim works with a coordinated 3-device flow | ADR-010 and `SYN-05` |
| The API assumptions marked unverified on this page | They must become facts or design changes |

**Outcome.**

| Result | Action |
| --- | --- |
| **Pass** on all five | ADR-005, ADR-006 and ADR-010 can move to Accepted, on the owner's sign-off. `MRG-01` unblocks |
| **Fail** on panics, divergence, load or storage, caused by Loro, and not fixable by a pinned upstream fix or a small layout change | **Run the same corpus with yrs and our own tree.** If that passes, a new ADR replaces ADR-006 |
| **Fail** on both, or a failure in the design rather than the library, such as review precision that cannot be tuned | **Redesign,** with the owner, before Phase 1 opens |
| A pass with a thin `-ops` | Apply the drop rule above, regardless of the rest |

**Time box:** proposal of two weeks. A partial result is reported as partial. Evidence goes in the tracker row, named, with its commit and seeds, and no number is claimed from elsewhere.

## ADRs

ADR-005, ADR-006 (needs to record the database document class and the unsafe sites), ADR-010, ADR-013, ADR-017, ADR-018, ADR-021, ADR-023.

## Tracker ids

`SPK-01`, `SPK-02`, `OPS-01`, `OPS-02`, `MRG-01`, `MRG-02`, `MRG-03`, `MOD-02`, `STO-05`, `DB-01`, `CNV-02`, `INK-03`, `TOOL-02`, `SYN-04`, `SYN-05`, `I18N-03`, `GOV-04`, `GOV-05`, `DEC-08`, `DEC-09`.
