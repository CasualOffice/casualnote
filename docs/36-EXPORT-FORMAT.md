# 36 — Export format: CasualNote Package

**Status: version 1 draft, proposed to the owner.** Implemented in the [prototype](prototypes/casualnote-prototype.html)
for export and import, and in [`tools/cnotepack.py`](../tools/cnotepack.py) for reading. The Rust engine does not exist
yet. Schema: [`schemas/cnotepack-1.schema.json`](../schemas/cnotepack-1.schema.json).

## Outcome

Anyone can take a whole workspace, or one page, out of CasualNote as one file. They can bring it back without losing
anything the format models, move it to another install, or read it with no CasualNote software at all.

## What it is, and what it is not

A CasualNote Package is a ZIP file with the extension `.cnotepack`. (The prototype saves `.cnotepack.zip`, because its host only allows known extensions.) It holds the current state of the notes, in two forms:
structured JSON for software, and Markdown for people.

| | Package (this document) | Live notebook (`.cnote`, [23](23-STORAGE-FORMAT.md)) |
| --- | --- | --- |
| Purpose | Moving, sharing, backup, leaving | Daily use and sync |
| Holds | Current state | Operation logs and snapshots |
| Merging | Not a merge format | Yes |
| Stability | Promised, versioned | Internal, and may change |
| Depends on the CRDT library | **No** | Yes |

The package is what limits our dependence on Loro (D1): it is defined without any CRDT concept, and the engine must be
able to write it and rebuild a notebook from it.

## Layout

```
mimetype                        first entry, stored uncompressed
manifest.json                   what is inside, with a hash of every file
README.txt                      how to read this file without any software
pages/<page-id>.json            structured page
databases/<database-id>.json    structured database
assets/<sha256>.<ext>           attachments, named by content hash
readable/<path>/<title>.md      the same page as Markdown
extensions/<name>/...           optional, see Extending
```

Paths use `/`, are UTF-8 with the ZIP language flag set, and are NFC-normalised. Titles in any language become file
names under `readable/`, so a Chinese or Arabic title stays readable in a file manager.

`mimetype` is the first entry, stored without compression and containing exactly
`application/vnd.casualnote.package+zip`, so a tool can identify the file from its first bytes. The same trick is used by EPUB and ODF.

## manifest.json

```json
{
  "format": "casualnote.package",
  "version": "1.0",
  "created": "2026-11-07T12:00:00Z",
  "generator": "CasualNote 0.1",
  "scope": "workspace",
  "languages": ["en", "zh-Hans"],
  "pages": [{ "id": "p-launch", "title": "Beta launch plan", "path": "pages/p-launch.json", "readable": "readable/Beta launch plan.md" }],
  "databases": [{ "id": "d-launch", "name": "Beta tracker", "path": "databases/d-launch.json" }],
  "files": [{ "path": "pages/p-launch.json", "sha256": "...", "bytes": 1024 }]
}
```

`files` lists every entry except `mimetype` and `manifest.json`. `languages` is a hint from the author, as BCP 47 tags.

## Pages

```json
{
  "id": "p-launch", "title": "Beta launch plan", "parent": null, "kind": "document", "view": "page", "favorite": true,
  "blocks": [
    { "id": "b1", "type": "paragraph", "lang": "en", "content": [
      { "text": "Open once " }, { "text": "import", "marks": ["bold"] }, { "text": " works. See " }, { "page": "p-import" }
    ] },
    { "id": "b2", "type": "heading", "level": 2, "content": [{ "text": "Goals" }] },
    { "id": "b3", "type": "todo", "checked": false, "content": [{ "text": "Ship it" }] },
    { "id": "b4", "type": "database", "database": "d-launch" },
    { "id": "b5", "type": "embed", "page": "p-format" }
  ],
  "canvas": {
    "note": { "x": 40, "y": 30, "width": 430 },
    "items": [{ "id": "i1", "kind": "sticky", "x": 530, "y": 40, "width": 180, "height": 120, "color": "amber", "text": "Show the report" }],
    "ink": [
      { "id": "s1", "kind": "pen", "color": "red", "width": 3, "points": [[720, 240, 0.5], [722, 243, 0.6]] },
      { "id": "s2", "kind": "arrow", "from": [252, 362], "to": [376, 362] }
    ]
  }
}
```

**Block types:** `paragraph`, `heading` (`level` 1 to 3), `bulleted`, `numbered`, `todo` (`checked`), `quote`, `callout`,
`code`, `divider`, `database`, `embed`. **Marks:** `bold`, `italic`, `code`, `highlight`. An inline `{ "page": id }` is a link to a page.

**Ink** keeps raw `[x, y, pressure]` samples, with pressure from 0 to 1, so it can be redrawn at any size and recognised later. Canvas coordinates are numbers in canvas units.

## Databases

```json
{
  "id": "d-launch", "name": "Beta tracker", "defaultView": "board",
  "statuses": [{ "id": "s1", "name": "To do", "group": "todo" }],
  "columns": [{ "id": "owner", "name": "Owner", "type": "text" }, { "id": "due", "name": "Due", "type": "date" }],
  "rows": [{ "id": "r1", "name": "Write the release notes", "status": "s1", "values": { "owner": "Mei", "due": "2026-11-20" } }]
}
```

Dates are ISO 8601. Status names are the user's own text, in any language. `group` is `todo`, `active`, `review` or `done`, so software understands progress without reading the name.

## Multilingual rules

- All text is UTF-8. **Text is stored exactly as written:** it is never transliterated, translated or re-normalised. Only paths are normalised.
- An optional `lang` on a page or block is a BCP 47 tag. Direction is not stored: it is worked out from the text, so a mixed-language note stays correct.
- Markdown files carry a front-matter `lang` when the page has one. They contain no direction marks beyond what the author typed.
- Nothing depends on the interface language. Exporting under Arabic and under English gives the same bytes, and an untitled page stays an empty string in the JSON.

## What is left out, on purpose

| Not exported | Why |
| --- | --- |
| API keys and any provider settings | Secrets must never travel in a file a person might share |
| Assistant conversations | Private, and not part of the notes |
| Embeddings and the search index | Rebuilt on import. They depend on the model and would go stale |
| The knowledge graph | Derived from the notes |
| Edit history | Version 1 holds current state. Reserved as `extensions/history` |
| CRDT state | Internal, see above |
| Downloaded models | Large, and separately licensed |

A sensitive-sharing warning is shown on export, since a package contains the notes in plain text. An encrypted form is left for a later version (Open questions).

## Determinism

The same notes and the same `created` value produce the same bytes: entries sorted by path after `mimetype` and
`manifest.json`, JSON with sorted keys and two-space indent, a fixed ZIP timestamp of 1980-01-01, and no extra ZIP fields.
Writers may use stored or deflated entries, and a given writer must always make the same choice.

## Reading it

**A package is hostile input.** Readers must:

| Code | Refuse when |
| --- | --- |
| `CN-PKG-0001` | It is not a ZIP, or `mimetype` is not the first entry with the exact value, stored uncompressed |
| `CN-PKG-0002` | `manifest.json` is missing, invalid, or is not this format |
| `CN-PKG-0003` | The major version is newer than the reader supports |
| `CN-PKG-0004` | A path is absolute, has `..`, a backslash, a drive letter, a NUL or control character, or is not NFC |
| `CN-PKG-0005` | Two entries have the same path, or differ only by case or Unicode form |
| `CN-PKG-0006` | An entry is encrypted, or a symbolic link |
| `CN-PKG-0007` | Limits are exceeded (below). Sizes are measured while decompressing, never trusted from the header |
| `CN-PKG-0008` | A file's hash or size differs from the manifest, or a listed file is missing |
| `CN-PKG-0009` | A file exists that the manifest does not list |
| `CN-PKG-0010` | A page or database file does not match its schema, or its id differs from the manifest |

**Limits in version 1:** 100,000 entries; 2 GiB per entry; 20 GiB in total; a compression ratio of 200:1 per entry for any entry over 1 MiB; 64 MiB per JSON file; JSON nesting of 64 levels.

**Also required:**

- **Never run or render as HTML anything from a package.** Text reaches the interface as text.
- **Never fetch anything** because a package mentions it.
- A dangling link to a page or database is not an error: it is kept, and shown as a removed page. Nothing is dropped silently, and the import report lists it.

## Importing

Import is a single transaction, and the user sees a report before it commits.

- **Into a new notebook:** ids are kept.
- **If an id already exists:** the user chooses to import as a copy with new ids (the default, with links rewritten), or to replace the existing page. **Import never merges** two versions of a page, because a package has no shared history.

## Versioning

`version` is `major.minor`. A minor version only adds optional fields. A reader **must keep fields it does not understand** when
rewriting a package, and may ignore them otherwise. A newer major version is refused with a clear message (`CN-PKG-0003`).
**We keep a reader for every major version we have shipped, permanently.**

## Extending

Other tools may add data under `extensions/<reverse-domain-name>/` and add `x-` prefixed fields anywhere in JSON. These must be
listed in `files`, are preserved, and must never be required in order to read the notes.

## Other exports

The package is the complete form. The per-page exports in FR-13 of [01](01-ORD.md), Markdown, HTML and PDF, are conveniences that can lose canvas detail, and say so at export.

## Licence

The format specification and schema are Apache-2.0, like everything else here, and anyone may implement them. The "CasualNote" name is not granted by that licence.

## Open questions

**Before 1.0, the format needs what the design work found missing** (`FND-04`, `FND-43`): attachments and image blocks in page JSON, links to URLs, sibling order, workspace name, block nesting, person and tag fields, and trashed pages. Version 1 stays a draft until those are settled.

1. **Encrypted packages.** Likely `age`-style passphrase encryption wrapping the whole file. Not in version 1.
2. **History.** Whether to specify `extensions/history` before 1.0.
3. **The file extension.** `.cnotepack` follows the product name (`DEC-01`). I searched GitHub code for it and found no use, which is not a full check.

## Acceptance gates

| Gate | State |
| --- | --- |
| Export then import then export gives identical bytes | Passing in the prototype |
| A reader with no CasualNote code verifies and extracts a package | Passing, `tools/cnotepack.py` |
| Each refusal code has a hostile fixture | Passing, `tools/test_cnotepack.py`. Removing the path and hash checks turns the matching tests red |
| Damage is refused by name and never silently changes notes | Passing. Bytes flipped across the sample package, in both the Python and the prototype reader. This test first failed, finding raw crashes, now fixed |
| The sample package and its JSON match the schema | Passing, using a validator only in the test environment |
| Text in every launch language survives unchanged | Passing for Arabic, Hindi, Japanese, Spanish and Chinese samples |
| Exporting under another interface language gives identical bytes | Passing in the prototype |
| Attachments, in the prototype | **Not covered.** The prototype has no attachment store for pages. The reader test covers `assets/` |
| A fuzz target for the Rust reader | **Not built.** No Rust yet |
| A format review by someone outside the project | **Not done** |
