# 37 — Import formats

**Status: design only, nothing built in Rust.** The [prototype](prototypes/casualnote-prototype.html) and `tools/cnotepack.py` read the CasualNote Package ([36](36-EXPORT-FORMAT.md)). Markdown, HTML and Evernote import do not exist anywhere. Limits are proposals.

## Outcome

A person can bring their notes in from a package, Markdown, HTML or an Evernote export. They see exactly what will be kept, changed or dropped before anything commits. A hostile file can do no more than be refused, and nothing it mentions is ever fetched.

Requirements: FR-14, QR-4, QR-7. Threats: SEC-F01 to SEC-F11 in [07](07-SECURITY-THREAT-MODEL.md). Crate: `-import` ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)), which has no network code and writes only through `-ops`.

| Order | Format | Row | Phase |
| --- | --- | --- | --- |
| 1 | CasualNote Package | `PKG-06` | 1 |
| 2 | Markdown, single file, folder or ZIP | `IMP-01` | 6 |
| 3 | HTML, single file, folder or ZIP | `IMP-01` | 6 |
| 4 | Evernote ENEX | `IMP-02` | 6 |
| Later | Others, by demand, from public documents only | `IMP-03` | 10 |

**OneNote is out of scope** by owner instruction 3 ([01](01-ORD.md)), and that includes its export forms, so no OneNote importer is planned.

## Design

### One pipeline

```
user picks a file → host opens it → detect → parse (bounded, in a worker) → ImportDraft
   → resolve links → build ImportReport → person reviews → commit as one transaction, or cancel
```

```rust
pub trait Importer {
    fn detect(head: &[u8], name: &str) -> Confidence;
    fn parse(&self, source: &mut dyn ImportSource, sink: &mut dyn DraftSink, limits: &Limits,
             report: &mut ReportBuilder) -> Result<(), ImportError>;
}
```

- **Host and engine.** The host supplies bytes through `ImportSource`. No importer touches a path or a socket. The whole thing runs in a worker or background thread, off the typing path.
- **Draft, not ops.** Parsers produce a neutral `ImportDraft` of pages, blocks, databases and assets, written to staging storage as it goes, so a big import is not held in memory. **Before commit, nothing has changed in the notebook.**
- **Detection** uses content first, and the extension second. A package is identified by its `mimetype` entry ([36](36-EXPORT-FORMAT.md)). A person can override a wrong guess, but not for a package, which either passes or is refused.
- **Cancelling** at any point before commit deletes the staging data and changes nothing.
- **Determinism.** The same bytes, options and version give the same draft and report. Native and wasm are compared in CI. Ids come from the host's random seam, so tests inject a seeded one.

### The transaction

Import is one transaction ([36](36-EXPORT-FORMAT.md)): after a crash, either all pages are there or none are.

ADR-006 places each page in its own CRDT document, so a many-page import crosses documents. `-ops` batches and `-store` must provide an atomic commit across them. Proposal: write every page under an `import_id`, kept hidden, and then append one commit record to the log as the single point of truth. Recovery drops anything under an uncommitted `import_id`. [23](23-STORAGE-FORMAT.md) and [24](24-OPERATIONS-AND-MERGE.md) must confirm this, since neither is written at the time of this draft (proposed row `FND-71`).

**Undo.** One import is one undo step in the session that made it, and appears in history as a named event. Undo moves the imported pages to the trash. **Import never merges** (from [36](36-EXPORT-FORMAT.md)): it always creates pages, or replaces if the person chose that for a package.

### The import report

The report exists to make nothing silent. It is built before commit, shown for review, and available afterwards.

```json
{
  "format": "enex", "source": "Work.enex", "bytes": 48211334,
  "counts": { "pages": 412, "blocks": 18210, "databases": 3, "assets": 96 },
  "summary": { "kept": 18120, "changed": 74, "dropped": 16, "refused": 1 },
  "items": [
    { "severity": "dropped", "code": "CN-IMP-0014", "where": "Trip plan", "what": "script element", "count": 1 },
    { "severity": "changed", "code": "CN-IMP-0015", "where": "Budget", "what": "Remote image kept as a link, not fetched", "url": "https://example.com/a.png" },
    { "severity": "refused", "code": "CN-IMP-0002", "where": "video.mov", "what": "File over the size limit" }
  ],
  "items_truncated": false
}
```

| Severity | Meaning |
| --- | --- |
| `kept` | Came through as it was. Counted, not listed |
| `changed` | Came through in another form. Listed |
| `dropped` | Left out, on purpose. Listed with a reason |
| `refused` | A file or item that failed a limit or check |

- **Identical items are grouped** with a count. A list longer than the limit is stated as cut and is saved in full to a file.
- **What it holds.** Titles, paths, URLs and codes, not note bodies. The labels are message ids, translated by [50](50-INTERNATIONALIZATION.md).
- **After commit.** It can be saved as JSON or plain text. Optionally "Keep as a page" (default off) stores it as a normal page.
- **A dangling link is not an error.** It is kept and listed ([36](36-EXPORT-FORMAT.md)).

### Package import

[36](36-EXPORT-FORMAT.md) defines the reader, codes (`CN-PKG-*`) and limits. This pipeline adds:

- **Into a new notebook:** ids are kept.
- **Id already exists:** a copy with new ids, links rewritten (default), or replace, by the person's choice for the whole import or per page. Never merge.
- **Unknown fields** are kept when the model supports it (`x-` fields, `extensions/`). If they cannot be kept in the live notebook, the report says so.

### Markdown

**Parser:** `pulldown-cmark`, MIT from memory, so confirm at the licence gate. Fuzzed and bounded. CommonMark with the GitHub tables, task list and strikethrough extensions.

| Markdown | Becomes | Report |
| --- | --- | --- |
| Paragraph, `#` to `###`, list, task list, quote, code fence, rule | `paragraph`, `heading`, `bulleted` or `numbered`, `todo`, `quote`, `code`, `divider` | kept |
| `####` to `######` | `heading` at `level` 3, since the model has 1 to 3 | changed |
| Nested list deeper than the model allows | Flattened to the limit | changed |
| Bold, italic, inline code | Marks | kept |
| Strikethrough | Plain text, if no such mark exists | changed |
| `[[Title]]`, or a relative link to another `.md` being imported | Page link | kept |
| `[text](https://...)` | **Needs a URL inline.** See the gap below | changed |
| `![alt](relative/path.png)` inside the import | **Needs an image block.** See the gap below | kept when available |
| `![alt](https://...)` | **Not fetched.** Kept as text with the URL | changed |
| Table | A new database with text columns: header to column names, first column to the row name, statuses defaulted | changed |
| Raw HTML | **Literal text.** Never parsed as markup | changed |
| Front matter | `lang` and `title` read. Other keys kept as a code block at the top, so nothing is lost | changed |
| `#tag` | Stays as text, and is a tag by [61](61-KNOWLEDGE-GRAPH.md) | kept |

**A folder or ZIP** becomes nested pages. A folder with a same-named `.md`, or an `index.md` or `README.md`, gives that page its body. Otherwise a folder is an empty page. Non-Markdown files referenced by a page become assets. Unreferenced ones are listed in the report and not imported. A ZIP uses `-package`, with the limits of [36](36-EXPORT-FORMAT.md). Symbolic links are refused. **Titles come from** front matter, else the first level-one heading only if it is the first block, else the file name.

### HTML

**Parser:** `html5ever`, MIT or Apache-2.0 from memory, so confirm at the licence gate. It parses as a browser would, within bounds. **This parser never touches a DOM, and no imported HTML is ever rendered.** The output is the block model, so there is no "sanitised HTML" stored at all (SEC-F09, QR-7).

**Rule:** walk the parsed tree with an **allow-list**. Anything not on the list contributes only its text, or is dropped if listed below. Nothing is ever kept as markup.

| Elements | Becomes |
| --- | --- |
| `p`, `div`, `section`, `article` with text | `paragraph` |
| `h1`, `h2`, `h3` | `heading`. `h4` to `h6` become level 3, changed |
| `ul`, `ol`, `li`, checkbox in `li` | `bulleted`, `numbered`, `todo` |
| `blockquote`, `pre`, `hr` | `quote`, `code`, `divider` |
| `strong`, `b`, `em`, `i`, `code`, `mark` | Marks |
| `a href` | Page link when it resolves inside the import. Otherwise a URL inline, for allowed schemes only (SEC-W05) |
| `img` | Local path or a `data:` raster inside the limits becomes an asset. **A remote URL is not fetched** and is kept as text. SVG is never inlined and stays an asset (SEC-F11) |
| `table` | A new database with text columns, as Markdown. Nested tables flatten |
| `span`, `font`, `center`, unknown elements | Their text, continuing the current block |

| **Dropped, with content, and counted** | |
| --- | --- |
| `script`, `style`, `template`, `noscript` | Scripts and styling, never meaningful text |
| `iframe`, `object`, `embed`, `applet`, `frame`, `frameset` | Active content |
| `form`, `input` except checkbox in a list, `button`, `select` | Form controls |
| `svg`, `math`, `canvas`, `audio`, `video` | Inline active or complex content. The URL for media is kept as text |
| `meta`, `link`, `base`, `head` apart from `title` | Metadata. `<base>` is ignored, so links can't be repointed |
| Comments | Not content |

**Every attribute is ignored** except `href`, `src`, `alt`, `lang`, `checked`, `start`, `colspan`, `rowspan` where relevant. `style`, `class`, `id`, `on*` and everything else never survive. `lang` carries to the block when valid BCP 47. `dir` is not stored because direction is derived from text ([36](36-EXPORT-FORMAT.md)), so a neutral-only passage can change direction. That is noted in the report. **Character references** are decoded by the parser, and the text stays text.

**No fetch at all.** No image, stylesheet, font, frame or redirect, on any route. A test asserts this by giving the host's I/O seam no network, and by watching for requests in the browser.

**Encoding.** The `meta charset` or byte-order mark is only a hint, shown in the preview.

### Encoding, in all text formats

UTF-8 is the default, with a byte-order mark stripped. UTF-16 by its mark. **Legacy encodings matter in a multilingual product** (GBK, Big5, Shift-JIS, EUC-KR, Windows-1256): invalid UTF-8 pauses that file, and the preview asks the person to choose an encoding, with a sample, through `encoding_rs` (licence from memory, confirm). **Guessing silently is not allowed,** and neither is silently replacing bytes with U+FFFD (`CN-IMP-0004`). Text is stored as written, with no normalisation ([36](36-EXPORT-FORMAT.md)). ZIP entry names without the UTF-8 flag use the same choice, and two names that collide after decoding are refused, the same as `CN-PKG-0005`.

### Evernote ENEX

Written **from Evernote's public DTDs and documents only**: `evernote-export` and ENML. Details here are from those documents as recalled while drafting, unverified against a real export. `IMP-02` must read the current public documents first, and record the source and date.

| ENEX | Becomes | Report |
| --- | --- | --- |
| `en-export` | A parent page named after the file | kept |
| `note/title` | Page title | kept |
| `note/content` | ENML inside CDATA, parsed by a second bounded XML pass into blocks, by the HTML element table | kept |
| `en-note` | The page body | kept |
| `en-todo checked` | `todo` | kept |
| `en-media hash type` | The matching `resource` as an asset, placed where it occurs | kept when available |
| `en-crypt` | **Cannot be decrypted.** A `code` block holding the ciphertext, so nothing is lost | changed |
| `note/tag` | Tag text, by [61](61-KNOWLEDGE-GRAPH.md), as a line at the top | changed |
| `created`, `updated`, `note-attributes` (author, source URL, location, reminders) | No field exists in [36](36-EXPORT-FORMAT.md). Written as a small metadata block at the top. A person can turn that off | changed |
| `resource/data` | Base64, decoded in a stream, to an asset by SHA-256 | kept |
| `resource/mime`, `file-name` | Asset type and name | kept |
| `resource/recognition`, `alternate-data` | Dropped | dropped |
| Resource no note refers to | Attached at the end of the page | changed |
| `en-media` with no matching resource | Placeholder text | changed |

**The MD5 in `en-media hash` is only a lookup key.** The body hash and size are checked against the decoded bytes, and any mismatch is reported (`CN-IMP-0007`). SHA-256 is the only hash stored. **MD5 here is never treated as integrity against an attacker.**

**XML rules.** Parser: `quick-xml`, MIT from memory, confirm at the licence gate. In a streaming form, and it does not fetch or expand. Real ENEX files contain a `DOCTYPE` naming a public DTD, so:

- A plain `DOCTYPE` with an external identifier is **ignored and never fetched**.
- **An internal subset or any entity declaration is refused** (`CN-IMP-0005`). That rules out entity-expansion bombs and external entities.
- Named entities in ENML (`&nbsp;` and so on) come from a fixed table. Anything else is left as literal text and reported.
- A note is parsed separately, so a bad one fails alone (`CN-IMP-0006`), and is listed, while the rest continue.

### Hostile files

Every file is hostile (QR-4). Limits are measured while reading, and never trusted from a header. Each parser has a fuzz target: `import_markdown`, `import_html`, `import_enex` (`GOV-07`). Each refusal code has a hostile fixture. Panics are contained at the worker boundary, and reported as `CN-IMP-0013` with nothing committed.

### The clean-room rule for other products' formats

ADR-023 applies, and is stricter here because importers are a natural place to copy from.

1. **Sources are public documents only:** a published format specification, a published DTD, or a format's own open standard.
2. **Never read** the importer or exporter of a GPL, AGPL or source-available project while writing ours. Joplin's Evernote importer, for one, sits under AGPL by default ([12](12-OPEN-SOURCE-AUDIT.md)). A contributor who has read one writes a different importer.
3. **Each importer's header** names the documents used, with dates.
4. **Fixtures are made by us:** written by hand, or generated from our own generator. A real user's export is never committed, and a sample lifted from another project's test suite is not allowed.
5. **No reverse engineering of undocumented binary formats.** If a format has no public documentation, it is not imported.
6. **Pull requests touching an importer** carry a statement of what was consulted.

This is an engineering rule, not legal advice, and `DEC-08` covers the legal review.

### Gaps this design needs filled

[36](36-EXPORT-FORMAT.md) has no block for an image or file, and no URL inline. Inline content is text or a page reference only, though `assets/` exists in its layout. So imported images and external links have nowhere to land. [22](22-DATA-MODEL.md) needs an attachment block and a link inline, with a minor package version (proposed row `FND-64`). Until then, the fallback is plain text with the URL or file name, reported as `changed`, and the asset kept only if the block exists. **IMP-01 should not ship on the fallback,** since images and links are common in real notes.

## Failure modes and limits

### Failure modes

| Failure | Code | Result |
| --- | --- | --- |
| Format not recognised | `CN-IMP-0001` | Refused, listing what is supported |
| A file or item over a limit | `CN-IMP-0002` | That file refused and listed. A limit on the whole import refuses the import |
| Nesting over the limit | `CN-IMP-0003` | That document refused for XML. HTML and Markdown flatten at the limit, reported |
| Invalid text encoding, not resolved | `CN-IMP-0004` | That file is held until the person chooses |
| XML has an internal subset or entity declaration | `CN-IMP-0005` | Whole ENEX refused |
| Malformed note in an ENEX | `CN-IMP-0006` | That note refused and listed |
| Resource hash or size mismatch | `CN-IMP-0007` | Asset kept under its real hash, and reported |
| Invalid base64 in a resource | `CN-IMP-0008` | That resource dropped, a placeholder placed, and reported |
| Path in a folder or ZIP escapes, or is a symlink | `CN-IMP-0009` | Refused, as `CN-PKG-0004`, `CN-PKG-0006` |
| Commit failed | `CN-IMP-0010` | Rolled back, and nothing changed. Report kept |
| Cancelled by the person | `CN-IMP-0011` | Staging deleted |
| Not enough storage | `CN-IMP-0012` | Stopped before commit |
| Parser panic contained | `CN-IMP-0013` | Refused, nothing changed |
| Element or feature dropped by the allow-list | `CN-IMP-0014` | Listed, the import continues |
| Remote resource not fetched | `CN-IMP-0015` | Listed, kept as text |
| Content converted to a poorer form | `CN-IMP-0016` | Listed |
| `CN-PKG-*` | From [36](36-EXPORT-FORMAT.md) | Package refused whole |

### Limits

Package and ZIP limits are those of [36](36-EXPORT-FORMAT.md). Others are proposals for [21](21-PARSER-LIMITS.md).

| Limit | Value |
| --- | --- |
| Whole import | 20 GiB, 100,000 pages, as a package |
| One Markdown or HTML file | 64 MiB, as a package's JSON limit |
| One ENEX file | 20 GiB, streamed |
| One ENML body, decoded | 64 MiB |
| One asset | 2 GiB, as a package entry |
| Compression ratio, per entry | 200:1 for entries over 1 MiB, as [36](36-EXPORT-FORMAT.md) |
| Tree depth, HTML and XML | 64, then flatten or refuse as above |
| Nested Markdown structure | 64 |
| Blocks per page | 100,000 |
| Attributes per element | 256, then ignored |
| Length of an XML name or attribute value | 1 MiB |
| `data:` URI | 16 MiB |
| Database from a table | 10,000 rows, 256 columns, then kept as text |
| Report items listed | 10,000, with the rest in the saved file |

## Alternatives considered

| Option | Decision | Why |
| --- | --- | --- |
| Keep sanitised HTML in a block | Rejected | Any HTML stored can reach the DOM someday. The block model alone meets QR-7 and SEC-F09 |
| A DOM-based sanitiser such as `ammonia` | Rejected | It outputs HTML, which we don't want. The allow-list walk is small and testable |
| Fetching remote images for convenience | Rejected | Leaks that a note was opened, and SSRF-like risk. An explicit "fetch these" action could come later as a host-run feature |
| Importing OneNote | Rejected | Owner instruction 3 |
| Merging into an existing page | Rejected | No shared history ([36](36-EXPORT-FORMAT.md)) |
| Remembering imports to avoid duplicates | Deferred | Adds state. For now, re-importing creates copies, and the preview warns on identical titles |
| Encoding detection by statistics | Rejected as automatic | Wrong guesses corrupt silently. A preview with a choice is safer |
| Reusing an existing converter | Rejected | Pandoc is GPL from memory, and the audited apps' importers are copyleft |

## Acceptance gates

| Gate | Row |
| --- | --- |
| Export, then import, then export gives identical bytes (engine) | `PKG-06` |
| Each `CN-PKG-*` has a fixture, with one fuzz target | `PKG-06` |
| Report names anything lost: each row of the mapping tables has a fixture and an expected report | `IMP-01` |
| Hostile HTML: script, event attributes, `javascript:` links, `base`, `meta refresh`, SVG, nested frames. No element reaches the DOM. No request is made | `IMP-01`, SEC-F09 |
| Text in every launch language comes through byte for byte, including Arabic, Hindi and legacy encodings | `IMP-01`, PR-5 |
| Crash at every step of commit: all or nothing | `IMP-01`, `FND-71` |
| Cancel before commit leaves notebook bytes unchanged | `IMP-01` |
| ENEX fixtures written from public docs: entities refused, DOCTYPE not fetched, base64 bomb bounded, `en-crypt` kept, bad note isolated | `IMP-02` |
| A source statement exists for every importer | `IMP-02`, `IMP-03` |
| One fuzz target per parser, run in CI | `GOV-07` |
| Native and wasm give the same draft and report | QR-3 |

## ADRs

ADR-005, ADR-006, ADR-017, ADR-018, ADR-023. Any parser crate is an engine dependency and needs recording per [08](08-ADR-REGISTER.md).

## Tracker ids

`PKG-06`, `IMP-01`, `IMP-02`, `IMP-03`, `GOV-07`, `DEC-08`. Proposed: `FND-64`, `FND-71`.
