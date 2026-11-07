# 22 — Data model

**Status: design only. Nothing is built.** All limits are proposals. The Rust below is a sketch of `casual-note-model`, not final code.

## Outcome

`-model` defines what a note is, in plain Rust values with no CRDT types, no I/O and no platform. Every other crate reads these types. An engineer can implement `-model` from this page, including:

- every type behind the [package format](36-EXPORT-FORMAT.md), so that a workspace can always be written out as a package with no CRDT concept in it;
- the invariants, and for each one what happens when a merge breaks it;
- schema repair, as pure functions that give the same result on every replica and never write back;
- how text in any language is stored, and how tags and page links are represented.

How these values are written and merged is in [24](24-OPERATIONS-AND-MERGE.md). How they reach the disk is in [23](23-STORAGE-FORMAT.md).

## Design

### Terms and layers of data

One `.cnote` notebook holds exactly one workspace. "Notebook" means the storage, "workspace" means the content.

| Form | Type | Comes from | Guarantees |
| --- | --- | --- | --- |
| Raw | `RawPage`, `RawDatabase`, `RawTree` | `-merge` reads CRDT state into these. They are defined in `-model` with no library type in them | None. Any merge result can be expressed, including an invalid one |
| Valid | `Page`, `Database`, `Workspace` | `repair_*` over Raw | Every invariant below holds |
| Package | JSON per [`cnotepack-1.schema.json`](../schemas/cnotepack-1.schema.json) | `-export` over Valid | Matches the schema |

Validating an op says nothing about merged state ([24](24-OPERATIONS-AND-MERGE.md)), so **Valid can only be reached through repair**. There is no other constructor from CRDT state.

### Ids

```rust
pub struct Id(Box<str>);          // validated; see below
pub struct PageId(Id);     pub struct DatabaseId(Id);
pub struct BlockId(Id);    pub struct ItemId(Id);     pub struct InkId(Id);
pub struct RowId(Id);      pub struct ColumnId(Id);   pub struct StatusId(Id);
pub struct WorkspaceId(Id);
pub struct DeviceId([u8; 16]);    // never appears in the package
pub struct Sha256([u8; 32]);      // blobs; lower-case hex in JSON
```

| Id | Minted form | Must be unique in | Extra rule |
| --- | --- | --- | --- |
| `PageId`, `DatabaseId`, `WorkspaceId` | 16 random bytes, 26 characters of lower-case base32 | The workspace | NFC, and unique under Unicode case folding |
| `BlockId`, `ItemId`, `InkId`, `RowId`, `ColumnId`, `StatusId` | 8 random bytes, 13 characters of lower-case base32 | The page or database | None |
| `DeviceId` | 16 random bytes | The notebook | See [23](23-STORAGE-FORMAT.md) |

Rules:

- **Accepted form** follows the schema: 1 to 128 Unicode scalars, none of them `/`, `\` or U+0000 to U+001F. Older ids such as `p-launch` and `faekwcto` are valid and kept verbatim on import.
- **The extra rule for page and database ids exists because of the package.** They become file names, and [36](36-EXPORT-FORMAT.md) refuses non-NFC paths (`CN-PKG-0004`) and paths differing only by case or form (`CN-PKG-0005`). If the model allowed them, a workspace could not be exported.
- **The caller mints ids and the op carries them,** with randomness from a host seam. So replaying an op is deterministic (QR-3), and `-model` has no random source.
- **Ids never appear in the interface and never derive from content.** On disk they are hashed before they become a directory name ([23](23-STORAGE-FORMAT.md)), so a hostile id cannot name a path.

### Workspace and page

```rust
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,                // may be empty
    pub languages: Vec<Lang>,        // an author's hint, never used for logic
    pub pages: Vec<PageEntry>,       // depth-first, in sibling order
    pub databases: Vec<DatabaseId>,  // the registry; sorted by id
    pub extensions: Vec<ExtensionFile>,
    pub extras: Extras,
}

pub struct PageEntry {               // lives in the tree document
    pub id: PageId,
    pub title: String,
    pub parent: Option<PageId>,
    pub kind: PageKind,
    pub favorite: bool,
    pub trashed_at_ms: Option<i64>,
    pub extras: Extras,
}

pub enum PageKind { Document, Database(DatabaseId) }

pub struct Page {                    // lives in the page's own document
    pub id: PageId,
    pub view: PageView,              // Page or Canvas: which view opens by default
    pub lang: Option<Lang>,
    pub blocks: Vec<Block>,
    pub canvas: Option<Canvas>,
    pub reviews: Vec<ReviewMarker>,  // see 24
    pub extras: Extras,
}

pub type Extras = BTreeMap<String, JsonValue>;  // only keys starting x-
```

- **The title is in the tree document,** so the sidebar never opens a page document to draw itself.
- **`kind` is fixed when a page is created.** A database page has no text of its own in the prototype, and its blocks are normally empty, though the schema allows them.
- **Not in the model, because they are per device or per window:** the canvas viewport (`canvas.view` in the prototype), selection, open tabs, the interface language. None of them is exported, and they are never written through ops.

### Blocks

Blocks are a **flat, ordered list per page.** The package has no children field and neither does the prototype, so a nested tree could not be expressed. See Open gaps.

```rust
pub struct Block {
    pub id: BlockId,
    pub lang: Option<Lang>,
    pub body: BlockBody,
    pub extras: Extras,
}

pub enum BlockBody {
    Paragraph(Inline),
    Heading { level: HeadingLevel, content: Inline },  // 1..=3
    Bulleted(Inline),
    Numbered(Inline),                                  // the number is computed
    Todo { checked: bool, content: Inline },
    Quote(Inline),
    Callout(Inline),
    Code(Inline),
    Divider,
    Database { database: DatabaseId },
    Embed { page: PageId },
    Attachment { file: FileRef, caption: Inline },
    Unknown { type_name: String, raw: JsonValue },     // from a newer version; read-only
}

pub struct FileRef { pub sha256: Sha256, pub name: String, pub media_type: String, pub bytes: u64 }
```

- **List numbers are computed:** a `Numbered` block is 1 unless the block before it is `Numbered`, as in the prototype. Nothing stores them.
- **`Unknown` is kept so that a newer version's blocks survive an older device.** It renders as a placeholder, accepts no edits and is exported as described in the mapping table.

### Inline runs and marks

```rust
pub type Inline = Vec<Run>;

pub enum Run {
    Text { text: String, marks: MarkSet, extras: Extras },
    PageLink { page: PageId, extras: Extras },
}

pub struct MarkSet(u8);  // bold = 1, italic = 2, code = 4, highlight = 8
```

**Canonical form,** which `normalise_runs` produces and every Valid value obeys:

1. No empty `Text` run.
2. Adjacent `Text` runs with equal marks and equal extras are joined.
3. Marks serialise in the fixed order `bold, italic, code, highlight`, which matches the prototype.
4. `PageLink` has no marks, since the schema has nowhere to put them.

**Offsets** count Unicode scalars in the block's text, and a `PageLink` counts as one. Ops land by `TextPos` ([24](24-OPERATIONS-AND-MERGE.md)), so an offset is used only to mint an anchor, and as the index of a provisional one. They do not count bytes, UTF-16 units or grapheme clusters. Scalars are correct here because a person deleting one combining mark in Hindi or Arabic must be able to do so. The DOM reports UTF-16, and the `-wasm` bridge converts. Whether an offset lands on a sensible caret position is the editor's concern ([42](42-EDITOR-AND-RENDERING.md)).

**Representation inside CRDT text.** A page link is the character U+FFFC with a `pagelink` mark holding the target id. A U+FFFC that has no `pagelink` mark is literal text the person pasted, and stays literal. This is a storage detail: `Run` never exposes U+FFFC to its readers.

### Language handling

**Text is stored exactly as written.** No normalisation, transliteration, translation or trimming happens anywhere in the write path. That includes bidi control characters and zero-width joiners that a person typed or pasted. How they are displayed safely is in [07](07-SECURITY-THREAT-MODEL.md).

```rust
pub struct Lang(Box<str>);  // pattern ^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$, kept as written

pub enum Dir { Ltr, Rtl, Neutral }

/// First strong character, per Unicode bidi rules P2 and P3. Pure.
pub fn base_direction(runs: &[Run]) -> Dir;

/// For matching only: NFC, then default case folding. Never stored, never exported.
pub fn fold_key(text: &str) -> String;
```

| Rule | Detail |
| --- | --- |
| `lang` is optional, on a page or a block | A BCP 47 tag used as a hint for tokenising, hyphenation and the `lang` attribute. Absent means unknown. It is never inferred and written back |
| `lang` is not canonicalised | `zh-hans` stays `zh-hans`, so a package round trip gives the same bytes |
| **Direction is never stored** | There is no `dir` field in the model, the CRDT or the package. It is computed per block, title, canvas item and cell |
| `lang` never decides direction | A block tagged `ar` containing only English is left-to-right |
| A `PageLink` is neutral for direction | The DOM isolates the title, so a link cannot change the block's direction |
| `Neutral` | An empty or digits-only block. What to show is decided by [42](42-EDITOR-AND-RENDERING.md) and [50](50-INTERNATIONALIZATION.md), and is not recorded |
| The interface language never reaches the model | An untitled page's title is `""`, and the word "Untitled" exists only in the interface |
| Unicode data is pinned | `base_direction` and `fold_key` change only with a declared Unicode version bump, because they affect output compared across native and web (QR-3) |

`base_direction` is for headless output and the graph. The live editor uses `dir="auto"`. A test compares the two per language, since any difference between them is a bug in one of them (`I18N-02`).

### Tags and page links

**A tag is text.** The model stores nothing for a tag beyond the characters the person typed. The package has no tag field, and the prototype derives tags from text with a regular expression. So a tag survives export to Markdown and a walk-away with no help from us.

```rust
/// Derived on read, never stored. Used by the outline, search and the graph.
pub fn tags_of(page_text: impl Iterator<Item = TextSource>) -> Vec<Tag>;

pub struct Tag { pub key: String /* fold_key */, pub display: String /* first seen */ }
```

| Question | Answer |
| --- | --- |
| Pattern | `(?:^\|\s)#([\p{L}\p{M}\p{N}_/-]+)`, from the prototype. `\p{M}` matters for Hindi and Arabic |
| Scanned | The title, inline text of every block, canvas item text, row names, in that order |
| Not scanned | Runs marked `code` and `Code` blocks, which is stricter than the prototype, so `#include` in code is not a tag |
| Across runs | Text is joined before scanning, so a partly bold tag still counts. A `PageLink` breaks the scan |
| Matching | By `fold_key`. `#Ａ` and `#ａ` are the same tag if they fold the same, and each occurrence keeps its own spelling |
| Renaming a tag | A batch of ordinary text ops. There is no tag op |

**Open for [61](61-KNOWLEDGE-GRAPH.md) and [50](50-INTERNATIONALIZATION.md).** Chinese and Japanese text is often written with no space before `#`, so `这是#研究` is not recognised by this pattern. Allowing it has a cost, since with no closing mark the tag would run on to the end of the sentence. Unresolved.

**A page link** is `Run::PageLink`, created by `[[` in the editor. It stores the target id and no title, so renaming a page never rewrites other pages and titles in any language are safe.

```rust
pub enum Reference {
    Link { from: BlockId, to: PageId },
    Embed { from: BlockId, to: PageId },
    Database { from: BlockId, to: DatabaseId },
}
pub fn references_of(page: &Page) -> Vec<Reference>;  // backlinks and the graph derive from this
```

- **Backlinks are derived,** never stored (FR-03, ADR-014).
- **A dangling link is kept** and shown as a removed page, as [36](36-EXPORT-FORMAT.md) requires. A link to a page in the trash displays the same way.
- **An embed cycle** can arise from a merge even if each op was valid. Rendering carries a chain of page ids, and stops at a repeat or at depth 8 (`BLK-01`).

### Database

A database is first-class data, referenced from pages (ADR-019, FR-05). It is in its own CRDT document so that several pages can share it. See [24](24-OPERATIONS-AND-MERGE.md) for why that goes beyond ADR-006's wording.

```rust
pub struct Database {
    pub id: DatabaseId,
    pub name: String,
    pub default_view: DbView,        // Table or Board
    pub statuses: Vec<Status>,       // never empty
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub reviews: Vec<ReviewMarker>,
    pub extras: Extras,
}

pub struct Status { pub id: StatusId, pub name: String, pub group: StatusGroup, pub extras: Extras }
pub enum StatusGroup { Todo, Active, Review, Done }

pub struct Column { pub id: ColumnId, pub name: String, pub ty: ColumnType, pub hidden: bool, pub extras: Extras }
pub enum ColumnType { Text, Date }

pub struct Row {
    pub id: RowId,
    pub name: String,
    pub status: StatusId,
    pub values: BTreeMap<ColumnId, String>,  // an absent key means empty
    pub extras: Extras,
}
```

- **A status's colour is not stored.** The group decides it, as in the prototype: `todo` grey, `active` blue, `review` amber, `done` green. Software reads progress from `group` and never from the user's own name for the status, which may be in any language.
- **An empty value is an absent key.** The prototype omits empty values on export, so `""` from an imported package is normalised away.
- **Date values are strings.** Ops require ISO 8601 (`2026-11-20`, or a date and time). A package may hold any string in a date column, and it is kept as written and shown as text. It is not coerced and not dropped.
- **A removed column is hidden, not deleted.** Its cells stay, it can be restored (`RestoreColumn`), and it is exported marked hidden. `columns` holds live columns in order, then hidden ones by id.
- **Name and status are built-in,** not entries in `columns`, matching the schema.
- **Column type cannot change in version 1.** There is no op for it.

### Canvas

Page and canvas are two views of one page's content (ADR-020). `blocks` is the same list in both, and on the canvas it is placed as one column by `note`.

```rust
pub struct Milli(i64);  // thousandths of a canvas unit

pub struct Canvas {
    pub note: NotePlacement,     // x, y, width: Milli
    pub items: Vec<CanvasItem>,  // array order is z-order, lowest first
    pub ink: Vec<InkRecord>,     // array order is z-order, drawn above items
    pub extras: Extras,
}

pub struct CanvasItem {
    pub id: ItemId,
    pub kind: ItemKind,          // Sticky, Rect, Ellipse, Text
    pub x: Milli, pub y: Milli, pub width: Milli, pub height: Milli,
    pub color: Option<String>,   // a token such as "amber", kept as written
    pub text: String,            // plain: the schema has no runs here
    pub extras: Extras,
}
```

**Coordinates are fixed-point, not floats.** The prototype already rounds to three decimals on export. Integer milli-units make that exact, so printing `833.007` or `252` is the same on native and web with no dependence on a float formatter (QR-3). A package with finer precision is rounded on import, and the import report says how many values changed. Values outside ±10^9 units are refused (`CN-MOD-0005`).

`color` values are tokens and never reach CSS as text. An unknown token draws the default colour and is kept as written.

### Ink records

Strokes are immutable, and samples are never per-point CRDT values (ADR-008). Geometry algorithms are in [43](43-CANVAS-AND-INK.md).

```rust
pub enum InkRecord {
    Stroke {
        id: InkId,
        kind: StrokeKind,          // Pen, Highlighter
        color: String,
        width: Milli,
        bbox: [Milli; 4],          // derived at write time, for hit-testing without decoding
        samples: SampleRef,
        origin: Option<InkId>,     // the stroke this was cut from by a partial erase
        extras: Extras,
    },
    Arrow { id: InkId, from: [Milli; 2], to: [Milli; 2], extras: Extras },
}

pub enum SampleRef {
    Inline(Bytes),                 // one opaque CRDT value, up to 16 KiB
    Blob { sha256: Sha256, count: u32 },
}
```

**Moving ink or erasing part of a stroke replaces records.** Nothing mutates samples. The 16 KiB threshold is a proposal, settled by `SPK-03`.

**Encoded samples** (all integers little-endian, varints are unsigned LEB128, signed values zigzag-encoded first):

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 4 | magic | `CNIK` |
| 4 | 1 | version | 1 |
| 5 | 1 | flags | 0 in version 1 |
| 6 | 2 | reserved | 0 |
| 8 | 4 | count | Number of samples, 1 or more |
| 12 | variable | first sample | `x`, `y` as signed varints in milli-units; pressure as varint in thousandths, 0 to 1000 |
| then | variable | later samples | `dx`, `dy`, `dp` as signed varints |

Decoding is bounded: `count` is checked against the limit before any allocation, and trailing bytes are refused (`CN-MOD-0007`). Decoding then re-encoding gives the same bytes, and the same encoding on native and web.

Export expands samples to `[x, y, pressure]`, one line per record in `ink/<page-id>.jsonl`, as the schema requires. A pressure outside 0 to 1 in an imported package breaks the schema, and is refused (`CN-PKG-0010`). A stroke at the sample limit fits a line: 65,536 samples at about 40 bytes each is about 2.6 MiB, against a line limit of 16 MiB.

### Invariants

Each one is checked by op validation where possible. **Merges can still break most of them,** so the last column is what stops that reaching the rest of the engine.

| # | Invariant | Can a merge break it? | Restored by |
| --- | --- | --- | --- |
| I-1 | Block ids are unique within a page | Yes, with a hostile or replayed update | R-05 |
| I-2 | Every entry in the block order has a block | Yes | R-04 |
| I-3 | A block with unseen edits is never hidden by a delete | Yes, delete against edit | R-08 |
| I-4 | A heading has a level from 1 to 3 | Yes | R-02 |
| I-5 | A divider, database or embed block has no text | Yes, kind change against typing | R-12, R-13 |
| I-6 | Runs are canonical | Yes | R-09, R-10, R-23 |
| I-7 | `pagelink` covers exactly one U+FFFC | Yes | R-09 |
| I-8 | A database has at least one status | Yes, two concurrent removals | R-16 |
| I-9 | Every row's status exists | Yes | R-15 |
| I-10 | The page tree has no cycle, and each `PageId` appears once | Library guarantees no cycle. Duplicates, yes | R-18, R-24 |
| I-11 | Sizes are positive, coordinates in range | Only with hostile input | R-14 |
| I-12 | Item and ink ids are unique within a page | Only with hostile input | R-25 |
| I-13 | `kind` of a page never changes | Only with hostile input | R-21 |
| I-14 | Sibling order is total and the same on every replica | No, the library orders them | Test only |
| I-15 | A page's package JSON stays within 64 MiB, and a database's | No hard guarantee, since additions from two devices can together exceed it | `CN-MOD-0009` on write, and a visible warning |

### Schema repair

```rust
pub struct Repaired<T> { pub value: T, pub notes: Vec<RepairNote> }

pub fn repair_page(raw: &RawPage) -> Repaired<Page>;
pub fn repair_database(raw: &RawDatabase) -> Repaired<Database>;
pub fn repair_tree(raw: &RawTree) -> Repaired<Workspace>;
pub fn normalise_runs(runs: Vec<Run>) -> Vec<Run>;
```

**Rules for these functions:**

1. **Pure.** No clock, no randomness, no locale, no I/O, no hash-map iteration order.
2. **Never write back** (`MOD-02`). Writing a repair into the CRDT would be a new concurrent edit on each replica, which can fight forever. A repair is recomputed on each projection.
3. **Same Raw, same Valid,** on every replica and on native and web.
4. **Idempotent.** Lowering Valid to Raw and repairing again gives the same value.
5. **Text is never discarded.** If a rule cannot place text, it creates a place for it. Numbers and flags may be defaulted.
6. **Every repair adds a `RepairNote`,** which is diagnostic and appears in history tooling. Repairs are not errors.

Applied in the order listed.

| Rule | Condition | Result |
| --- | --- | --- |
| R-01 | Block `type` is not known to this version | `Unknown`, retained and read-only |
| R-02 | Heading level missing or outside 1 to 3 | Missing becomes 1; otherwise clamped, as the prototype does |
| R-03 | `checked` is not a boolean | `false` |
| R-04 | An order entry has no block | Entry ignored |
| R-05 | The same id appears twice in the order | First position wins |
| R-06 | A block is not in the order and has no delete record | Placed at the end of the page, sorted by id. Its text must not vanish |
| R-07 | A block is not in the order, and `del_fp` equals the fingerprint of its text | Deleted. Hidden, kept for undo |
| R-08 | A block is not in the order, and `del_fp` differs from its text's fingerprint | **Someone edited it without seeing the delete.** It stays visible, after its `del_after` anchor. The detector in [24](24-OPERATIONS-AND-MERGE.md) opens the review marker, since repair writes nothing. See the algorithm below |
| R-09 | `pagelink` mark over anything except a single U+FFFC, or with an invalid id | Mark ignored; the characters are kept as text |
| R-10 | A page-link character also carries other marks | Other marks ignored for it |
| R-11 | `Database` or `Embed` target is missing or unknown | Block kept, shown as removed |
| R-12 | A `Divider` has non-empty text | Becomes `Paragraph` |
| R-13 | A `Database` or `Embed` block has non-empty text | Block kept. A derived `Paragraph` with id `<block-id>~t` follows it, holding the text |
| R-14 | A size is zero or negative; a coordinate is out of range | Size becomes 1 unit; coordinate clamped |
| R-15 | A row's status does not exist | Shown under the first live status. The stored value is unchanged |
| R-16 | No live status | The hidden status with the lowest id is used. If none ever existed, a constant `{id: "s0", name: "", group: todo}` |
| R-17 | A row is deleted and `del_fp` differs from the fingerprint of its name and text cells | As R-08, for rows, using row order |
| R-18 | Two tree nodes claim one `PageId` | The node with the lowest neutral node key keeps the id. The other gets `<id>~<node-key>` as a derived id, and stays visible |
| R-19 | A page document exists with no tree node | In order: if its id is in the tree's `purged` map, it is **not shown and never resurrected,** and any edit past the tombstone goes to a restored copy through `-sync` ([56](56-SYNC-DESIGN.md)). If it holds no block and no text, it is hidden: this is the orphan an interrupted create leaves ([24](24-OPERATIONS-AND-MERGE.md)), and compaction removes it. Otherwise it is shown under a virtual `Recovered` parent, since its tree node may still be on the way. A tree node with no document yet is an empty page marked pending |
| R-20 | An ink or attachment blob is not present | Record kept, drawn as missing. It is never removed |
| R-21 | A database page has no database id | Kept, shown as removed. A `kind` overwritten by a hostile write cannot be told from a real value and is read as written |
| R-22 | `lang` does not match the pattern | Treated as absent, value left in place |
| R-23 | Runs are not canonical | `normalise_runs` |
| R-24 | A parent chain has a cycle, or a parent is unknown | Broken at the lowest id, which goes to the root. This guards a future library, since Loro's tree should prevent cycles |
| R-25 | Duplicate item or ink id | First in array order wins. A later duplicate with text is given the derived id `<id>~<index>` |

**R-08 algorithm.** `del_fp` is the first 8 bytes of the SHA-256 of the block's text at the moment of delete, written by the delete op together with `del_after`, the id of the block before it. Because both are ordinary values in CRDT state, the check needs no causal history, and every replica reaches the same answer.

```
place(block):
    a = block.del_after
    seen = {block.id}
    while a is not None and a not in live_order:
        if a in seen: a = None; break
        seen.add(a)
        a = blocks[a].del_after if a in blocks else None
    insert after a, or at the start of the page when a is None
    ties at one anchor: sorted by block id
```

Formatting and attribute changes do not change `del_fp`, so a concurrent `bold` does not undo a delete. Only text can resurrect a block. The result is deliberate: PR-1 says merged text is never hidden, so a delete loses to an unseen edit and the person decides in review.

### The state hash

Replicas, snapshots and the test suites compare documents through one hash, defined here because it needs no library. It is **not** the package JSON, which leaves out most of what a merge can get wrong, and which would have to read blobs.

```rust
pub const STATE_HASH_VERSION: u16 = 1;
/// One document. Pure, and subject to the same rules as repair.
pub fn state_hash(doc: &RawDocument) -> Sha256;   // RawTree, RawPage or RawDatabase
```

| Rule | Detail |
| --- | --- |
| Scope | One document. A page's title lives in the tree document, so the tree's hash covers it. Nothing is hashed across documents |
| Input | **Raw, before repair,** so a new repair rule cannot look like divergence between two versions of the app |
| Covers | All CRDT-held state: order lists, entries not in order, `del_fp`, `del_after`, `split_from`, `merged_into`, hidden statuses and columns, cells of removed columns, reviews with their `resolved` entries, the `versions` list, `trash_base`, `purged`, and `x-` values |
| Samples | **By hash, never by content.** `Inline` contributes the SHA-256 of its bytes, and `Blob` its `sha256` and `count`. No blob is opened or fetched, so a missing blob gives no false divergence |
| Leaves out | CRDT history, peer ids and counters, `ProjectionRev`, and anything per device |
| Empty | An empty Text and a Text that was never written hash the same, by being omitted, so a cell's root container that exists on one replica and not yet on another differs in nothing |
| Encoding | SHA-256 over canonical CBOR (RFC 8949 §4.2) of `[STATE_HASH_VERSION, class, id, body]`. Map keys sort as CBOR says, lists keep their order, text is its runs in order, and opaque versions are byte strings |
| Change | A new encoding bumps `STATE_HASH_VERSION`. A snapshot with another version is not compared, and is rebuilt ([23](23-STORAGE-FORMAT.md)) |

Equal hashes mean equal state, and so the same repair result from the same build. They say nothing about history, so the compaction check compares versions as well ([23](23-STORAGE-FORMAT.md)).

### Mapping to the package

Version 1 of the schema is the constraint. Rows marked **x-** use the extension mechanism in [36](36-EXPORT-FORMAT.md) because the schema has no field, and each is raised under Open gaps.

| Model | Package JSON | Prototype | Notes |
| --- | --- | --- | --- |
| `Workspace.languages` | `manifest.languages` | Empty | As BCP 47, as written |
| `Workspace.id`, `.name` | **x-** `manifest["x-casualnote-workspace"]` | None | `{ id, name }`. Required to keep a workspace's identity |
| `Workspace.extensions` | `extensions/<name>/...`, listed in `manifest.files` | None | Kept as blobs, listed in the tree document |
| `PageEntry.id` | `page.id`, `manifest.pages[].id` | `id` | |
| `PageEntry.title` | `page.title`, `manifest.pages[].title` | `title` | `""` when untitled |
| `PageEntry.parent` | `page.parent` | `parent` | `null` at the root |
| Sibling order | **x-** `page["x-casualnote-order"]` | Array order, lost on export | Integer position among siblings. `manifest.pages` is sorted by id, so it cannot carry order |
| `PageEntry.kind` | `page.kind`, plus `page.database` | `kind`, `db` | |
| `PageEntry.favorite` | `page.favorite` | `fav` | |
| `PageEntry.trashed_at_ms` | Not exported | `trash` | The prototype leaves trashed pages out. Links to them become dangling and are listed in the report |
| `Page.view` | `page.view` | `mode` | |
| `Page.lang` | `page.lang` | None | |
| `Page.blocks` | `page.blocks` | `blocks` | |
| `Page.reviews` | Not exported | None | The merged text is exported. Review is a live concept |
| `Block.id`, `.lang` | `block.id`, `block.lang` | `id` | |
| `Paragraph` | `type: paragraph`, `content` | `p` | |
| `Heading` | `type: heading`, `level`, `content` | `h1` to `h3` | |
| `Bulleted`, `Numbered` | `bulleted`, `numbered`, `content` | `ul`, `ol` | |
| `Todo` | `type: todo`, `checked`, `content` | `todo`, `done` | |
| `Quote`, `Callout`, `Code` | Same names, `content` | Same | |
| `Divider` | `type: divider`, no `content` | `hr` | |
| `Database` | `type: database`, `database` | `db` | |
| `Embed` | `type: embed`, `page` | `embed` | |
| `Attachment` | **x-** `type: paragraph`, `content` holds the caption or file name, plus `x-casualnote-attachment: { sha256, name, mediaType, bytes }`. File at `assets/<sha256>.<ext>` | None | A reader that ignores `x-` still sees the name as text and the file in `assets/` |
| `Unknown` | Written back as read: its own `type` and the fields in `raw` | None | The schema allows a type it does not list, and a reader keeps and reports it ([36](36-EXPORT-FORMAT.md)). A newer writer may have newer rules |
| `Run::Text` | `{ "text", "marks" }`, with `marks` omitted when empty | `{ t, b, i, c, h }` | |
| `Run::PageLink` | `{ "page": id }` | `{ ref }` | |
| `Canvas` | `page.canvas`, omitted when `None` | `canvas` | New pages get the default note placement, x 80, y 60, width 620 |
| `Canvas.extras` | `canvas["x-..."]` | | |
| `NotePlacement` | `canvas.note` `{x, y, width}` | `note` `{x, y, w}` | `note` allows no `x-` fields |
| `CanvasItem` | `canvas.items[]`, in z-order | `{ x, y, w, h, c, text }` | `text` is always present, `""` when empty |
| `InkRecord::Stroke` | One line of `ink/<page-id>.jsonl`, named by `canvas.inkFile`: `{ id, kind, color, width, points }`, in z-order | `{ kind: pen or hi, color, w, points }` | `points` expanded from `samples`. `origin` and `bbox` are not exported, so a re-imported fragment stands alone |
| `InkRecord::Arrow` | One line of the same file: `{ id, kind: arrow, from, to }` | `{ arrow: 1, a, b }` | |
| PDF background (`INK-04`) | **x-** `canvas["x-casualnote-background"]`, a `FileRef` | None | Not yet modelled beyond this. [43](43-CANVAS-AND-INK.md) owns it |
| `Milli` | JSON number, up to 3 decimals, no trailing zeros | Rounded by `num()` | Integers print with no point |
| `Database` | `databases/<id>.json` | `S.dbs` | All registered databases on a workspace export; only referenced ones on a page export |
| `Database.name`, `default_view` | `name`, `defaultView` | `name`, `view` | |
| `Status` | `statuses[]`: `{ id, name, group }` | `{ id, name, c }` | |
| `Column` | `columns[]`: `{ id, name, type, hidden }`, with `hidden` omitted when false | `{ id, name, type? }` | Hidden columns are exported, marked hidden. Import restores them hidden, as `AddColumn` then `RemoveColumn` |
| `Row` | `rows[]`: `{ id, name, status, values }` | `{ id, name, status, <col>: value }` | Keys for hidden columns are exported. On import, keys naming no column are listed in the report and dropped |
| `Extras` | `x-` keys on the same object | None | Preserved on rewrite, as [36](36-EXPORT-FORMAT.md) requires |
| `DeviceId`, CRDT state, versions, undo | Not exported | | ADR-017 |

**Import direction.** Fields in the schema that mean nothing for a block's type (`level` on a paragraph) are dropped and listed in the import report, so nothing goes silently. Importing runs through `-ops` as described in [37](37-IMPORT-FORMATS.md), and non-canonical runs are normalised on the way in. The text does not change, only the run boundaries.

**The round trip.** Package written by the engine, imported into a new notebook and exported again with the same `created`: identical bytes. A package written by another tool is normalised once, and is then stable.

### Open gaps

Each needs the owner of [36](36-EXPORT-FORMAT.md) or a new row. Until decided, the `x-` forms above keep the model expressible.

| Gap | Effect | Recommendation |
| --- | --- | --- |
| No attachment block in the schema, though `assets/` exists and FR-08 is M | No page can reference a file in a package 1.0 reader | Add `attachment` to the draft before version 1 freezes |
| No sibling order in the schema | Export then import reorders the sidebar. The byte-identity gate in [36](36-EXPORT-FORMAT.md) cannot see this, because the order is not in the bytes | Add an optional `order` field |
| No workspace name or id | A workspace cannot name itself in a package | Add to the manifest |
| No nesting of blocks | FR-02 says "lists", and nested lists cannot be represented | Decide before `-ops` freezes. Adding it later is a minor version plus a new op |
| Trashed pages are not exported | "Nothing lost on export" does not cover the 30-day trash | State it in [36](36-EXPORT-FORMAT.md), or export them |
| FR-08 says attachments of any size; [36](36-EXPORT-FORMAT.md) limits an entry to 2 GiB | A bigger file is in the notebook but cannot be exported | See `CN-STO-0014` in [23](23-STORAGE-FORMAT.md). A decision is needed |

## Failure modes and limits

All numbers are proposals. Op refusals use `CN-OPS-*` ([24](24-OPERATIONS-AND-MERGE.md)). Limits on types use `CN-MOD-*`, for [20](20-ERROR-CODE-REGISTRY.md) and [21](21-PARSER-LIMITS.md).

| Code | Refused or reported when | Limit |
| --- | --- | --- |
| `CN-MOD-0001` | An id breaks the accepted form | 1 to 128 scalars |
| `CN-MOD-0002` | A page or database id is non-NFC, or collides under case folding | |
| `CN-MOD-0003` | Block text is too long | 4 MiB of UTF-8 per block |
| `CN-MOD-0004` | A page has too many blocks | 200,000 |
| `CN-MOD-0005` | A coordinate or size is out of range or not finite | ±10^9 units |
| `CN-MOD-0006` | A string attribute is too long | Title, names, cell: 1 MiB. `lang`: 64 bytes. `color`: 64 bytes |
| `CN-MOD-0007` | Encoded samples are malformed | |
| `CN-MOD-0008` | A stroke or page has too much ink | 65,536 samples per stroke, with longer gestures split into several strokes ([43](43-CANVAS-AND-INK.md)); 200,000 ink records per page |
| `CN-MOD-0009` | A page or database's package JSON would exceed 64 MiB, the [36](36-EXPORT-FORMAT.md) limit | Warned at 48 MiB, refused at 64 MiB. Ink does not count, since it has its own file |
| `CN-MOD-0012` | A page's exported ink file would exceed 2 GiB, the [36](36-EXPORT-FORMAT.md) entry limit | Warned at 1.5 GiB. Export refuses by name and never drops the ink |
| `CN-MOD-0010` | A database has too many columns, statuses or rows | 512 columns, 128 statuses, 1,000,000 rows |
| `CN-MOD-0011` | A canvas has too many items | 100,000 |

**Other failure modes.**

| Failure | Handling |
| --- | --- |
| A lone surrogate in imported JSON | Rust strings cannot hold one. The import refuses the file with `CN-PKG-0010` |
| NUL or control characters in text | Stored as written. Ids refuse them, text does not |
| A hostile `x-` value | Size-limited by the file limit, stored as an opaque JSON string, and never interpreted or rendered |
| R-08 could resurrect a block a person wanted gone | Deleting it again goes through the implicit `RestoreBlock` ([24](24-OPERATIONS-AND-MERGE.md)) and writes a new `del_fp`, and it goes. It is visible once, by design. Any other first op on it restores it the same way |
| The 8-byte fingerprint collides | A concurrent edit could then be hidden. Odds are about 1 in 2^64 per delete against edit. Accepted, and its text remains in CRDT state and in version history |
| Direction differs between `base_direction` and `dir="auto"` | A failing test, not a runtime concern |

## Alternatives considered

| Alternative | Why not |
| --- | --- |
| Nested block tree | The schema and the prototype are flat. It needs a tree per page, and more merge cases. Could return as a minor version |
| Tags as a stored list on a page | Two sources of truth that can disagree, a new merge case, and tags would be lost in Markdown. A tag in text survives everything |
| Page link as a mark over the title text | Renames would rewrite other pages, and titles would be copied across documents |
| Storing direction per block | It goes wrong as soon as the text changes, and mixed-language notes need it computed anyway |
| NFC-normalising text on write | "Stored exactly as written" is a promise. Some scripts have meaningful differences in form, and a round trip must be exact |
| Floats for coordinates | Printing would depend on the float formatter, and native and web must give the same bytes |
| Writing repairs into the CRDT | Each replica would write its own repair, and those fight. `MOD-02` forbids it |
| Text cells as a last-writer-wins register | One device's text would vanish on merge, against PR-1 |
| A generic property bag on blocks | Hides invalid states from the compiler, and this model is meant to be closed |
| Storing a colour on statuses | The prototype derives it from the group, and the schema has no field |

## Acceptance gates

| Gate | Method | State |
| --- | --- | --- |
| Every invariant holds after repair | Property test over generated Raw values, including invalid ones | Not built |
| Same Raw gives the same Valid on native and wasm | The same generated corpus on both | Not built |
| Repair is idempotent and never writes | Property test; a counting fake of `-merge` sees no writes | Not built |
| No rule discards text | Property test: every scalar in Raw text appears in Valid, except the page-link character | Not built |
| Equal Raw gives an equal state hash, on native and wasm, with blobs present or missing | Generated values | Not built |
| Every Valid value exports to JSON that passes the schema and `tools/cnotepack.py` | Generated values, reader run in CI | Not built |
| Round trip gives identical bytes | Sample package, then generated workspaces | Not built |
| Text in each launch language comes back unchanged, and direction is right | Per-language test sets (`DEC-09`) | Not built |
| Sample decoding is bounded | Fuzz target | Not built |

## ADRs

ADR-005, ADR-006 (needs a database-document addition, see [24](24-OPERATIONS-AND-MERGE.md)), ADR-008, ADR-014, ADR-017, ADR-018, ADR-019, ADR-020.

## Tracker ids

`MOD-01`, `MOD-02`, `DB-01`, `CNV-01`, `CNV-02`, `INK-01`, `INK-03`, `INK-04`, `BLK-01`, `WEB-05`, `I18N-02`, `PKG-05`, `PKG-06`, `SPK-03`, `DEC-09`.
