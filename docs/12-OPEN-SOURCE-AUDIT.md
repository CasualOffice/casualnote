# 12 — Open-source audit

Checked 2026-11-07 for an Apache-2.0 product. Licence conclusions are an engineer's reading, not legal advice.

**How this was checked.** Licences, READMEs, docs, manifests and issue trackers were read on GitHub. Permissive sync
engines were also read at source level. **No application was built, installed or run**, so nothing here says how an app
actually feels or performs. GPL, AGPL and source-available projects were studied through docs and behaviour only.

**Limits.** Issues were sampled by reactions and keywords, which favours old feature requests. Many cited issues are
closed and may be fixed. Transitive dependency licences were not audited. "None found" means not found, not proven absent.

## What an Apache-2.0 product may do

| Licence | Depend on it | Copy code | Take ideas |
| --- | --- | --- | --- |
| MIT, BSD, ISC, Apache-2.0 | Yes | Yes, keeping notices | Yes |
| MPL-2.0 | Yes | Copied files stay MPL | Yes |
| GPL, AGPL | No | No | Yes, from docs and behaviour |
| Source-available or proprietary | No, unless licensed separately | No | Yes, from docs and behaviour |

## Canvas and block workspaces

| Project | Licence | Use | Design | Sync | Ink | Evidence of pain |
| --- | --- | --- | --- | --- | --- | --- |
| [AFFiNE](https://github.com/toeverything/AFFiNE) | MIT, except `packages/backend` and `packages/common/native`, which are under commercial terms | MIT parts only | Block tree plus canvas elements | Yjs and y-octo | Pressure stored; [palm rejection reported poor](https://github.com/toeverything/AFFiNE/issues/13782) | [No local encryption](https://github.com/toeverything/AFFiNE/issues/5491) |
| [BlockSuite](https://github.com/toeverything/blocksuite) | MPL-2.0 | Allowed, but looks dormant since merging into AFFiNE | Yjs block tree, web components | Yjs | Not inspected | [RTL](https://github.com/toeverything/blocksuite/issues/7894) |
| [AppFlowy](https://github.com/AppFlowy-IO/AppFlowy), [Collab](https://github.com/AppFlowy-IO/AppFlowy-Collab) | AGPL-3.0 | Ideas | Flutter and Rust; typed layer over yrs | yrs, RocksDB | [Declined](https://github.com/AppFlowy-IO/AppFlowy/issues/2370) | [Bulk export](https://github.com/AppFlowy-IO/AppFlowy/issues/3334) |
| [Anytype](https://github.com/anyproto/anytype-ts) | Any Source Available 1.0 | Ideas | Typed objects of blocks | Encrypted CRDT DAG, P2P | Excalidraw embed only | [RTL](https://github.com/anyproto/anytype-ts/issues/1373) |
| [any-sync](https://github.com/anyproto/any-sync) | MIT | Allowed; Go, so a poor fit | Sync protocol | P2P and nodes | — | — |
| [tldraw](https://github.com/tldraw/tldraw) | Proprietary. Licence forbids production use without a key | Ideas | Shapes and record store | Server rooms | Pressure and pen mode, per docs | [Licence tracking](https://github.com/tldraw/tldraw/issues/10034) |
| [Excalidraw](https://github.com/excalidraw/excalidraw) | MIT | All, but requires React | Flat element list | Per-element version | Pressure stored; [stylus UX weak](https://github.com/excalidraw/excalidraw/issues/9705) | [Rich text](https://github.com/excalidraw/excalidraw/issues/5138) |

## Notes and knowledge apps

| Project | Licence | Use | Storage | Sync and conflicts | Encryption | Evidence of pain |
| --- | --- | --- | --- | --- | --- | --- |
| [Joplin](https://github.com/laurent22/joplin) | Composite: AGPL by default; server has a custom personal-use licence; some folders MIT or MPL | Ideas, plus named permissive folders | Local database, readable through export | Many targets; [line-based merge, otherwise a conflict note](https://github.com/laurent22/joplin/blob/dev/readme/dev/spec/conflict_auto_merge.md) | Optional E2EE | [Sync silently skips files](https://github.com/laurent22/joplin/issues/6517) |
| [SiYuan](https://github.com/siyuan-note/siyuan) | AGPL-3.0 | Ideas | JSON files and assets folder | Own data repo; third-party sync disks unsupported | Encrypted notebooks | [Sync data loss](https://github.com/siyuan-note/siyuan/issues/10205) |
| [Trilium](https://github.com/TriliumNext/Trilium) | AGPL-3.0 | Ideas | One SQLite database | Star topology; conflicts not surfaced | Per-note | [Typing lag](https://github.com/TriliumNext/Trilium/issues/7373) |
| [Logseq](https://github.com/logseq/logseq) | AGPL-3.0 | Ideas | SQLite for DB graphs; files split off | Alpha; README warns of data loss | Not verified | [Notes deleted with OneDrive](https://github.com/logseq/logseq/issues/8190) |
| [Memos](https://github.com/usememos/memos) | MIT | All; Go server | SQL database | None; server-centric | None found | [Unsaved text lost](https://github.com/usememos/memos/issues/5667) |
| [Notesnook](https://github.com/streetwriters/notesnook) | GPL-3.0 | Ideas | Encrypted local database | Marks a conflict; user chooses | Always-on E2EE | [Local notes disappearing](https://github.com/streetwriters/notesnook/issues/2502) |
| [Standard Notes](https://github.com/standardnotes/app) | AGPL-3.0 | Ideas | Encrypted local store | Duplicates conflicted notes | E2EE | [Dated forum evidence](https://github.com/standardnotes/forum/issues/3535) |

Worth learning from: Memos's versioned export spec with JSON Schemas, and Standard Notes's crypto documentation.

**A warning we must respect.** Joplin [tried word-level merging and dropped it because it could silently duplicate
text](https://github.com/laurent22/joplin/blob/dev/readme/dev/spec/conflict_auto_merge.md). A CRDT merges at a finer
grain still, so the same failure is available to us. [24](24-OPERATIONS-AND-MERGE.md) and [56](56-SYNC-DESIGN.md) answer it with visible review.

## Ink and handwriting apps

All three are GPL, so ideas only. The licence files give the version; "only" versus "or later" was not confirmed.

| Project | Licence | Format | Strength | Evidence of pain |
| --- | --- | --- | --- | --- |
| [Rnote](https://github.com/flxzt/rnote) | GPL-3.0 | Compressed JSON; declared unstable | Rust vector engine, infinite layout | [Slows on an 8-page file](https://github.com/flxzt/rnote/issues/1436), no mobile |
| [Xournal++](https://github.com/xournalpp/xournalpp) | GPL-2.0 | Gzipped XML; PDFs not embedded | Best PDF annotation of the three | [PDF not embedded](https://github.com/xournalpp/xournalpp/issues/937) |
| [Saber](https://github.com/saber-notes/saber) | GPL-3.0 | No format document found | Cross-device with encrypted sync | [Huge files](https://github.com/saber-notes/saber/issues/847) |

## Sync engines

All five are plain MIT, read from their licence files. MIT has no express patent grant.

| Project | Rust | Rich text | Tree and move | History trimming | Maturity | Wasm size |
| --- | --- | --- | --- | --- | --- | --- |
| [Yjs](https://github.com/yjs/yjs) | No | Yes | None found | None found | Stable | 69 KB JS |
| [yrs](https://github.com/y-crdt/y-crdt) | Yes; 69 unsafe sites | Yes | None found | None found | 0.28, pre-1.0 | 1.0 MB |
| [y-octo](https://github.com/y-crdt/y-octo) | Yes; 43 unsafe sites | Yes | None found | None found | 0.1 | No published build |
| [Loro](https://github.com/loro-dev/loro) | Yes; 24 core unsafe sites | Yes | **Native** | **Shallow snapshots** | 1.17 | 3.6 MB |
| [Automerge](https://github.com/automerge/automerge) | Yes; almost no unsafe | Yes, cites Peritext | None found | None found | JS 3.5 stable, Rust 0.12 | 3.64 MB |

Open defects to test rather than assume, all reported and not reproduced here:
[yrs use-after-free](https://github.com/y-crdt/y-crdt/issues/676), [Yjs memory growth](https://github.com/yjs/yjs/issues/741),
[Automerge memory](https://github.com/automerge/automerge/issues/946), and Loro's
[1216](https://github.com/loro-dev/loro/issues/1216), [1262](https://github.com/loro-dev/loro/issues/1262) and
[1254](https://github.com/loro-dev/loro/issues/1254). Benchmarks are stale or published by interested parties, and no
source covers years of real edits.

## Building blocks

| Project | Licence | Plan |
| --- | --- | --- |
| [Tauri](https://github.com/tauri-apps/tauri) | Apache-2.0 on GitHub | Desktop shell, as OpenCalc |
| [perfect-freehand](https://github.com/steveruizok/perfect-freehand) | MIT | Port the stroke algorithm; the existing Rust ports have little adoption |
| [ink-stroke-modeler](https://github.com/google/ink-stroke-modeler) | Apache-2.0 | Reference for smoothing and prediction |
| [ink-stroke-modeler-rs](https://github.com/flxzt/ink-stroke-modeler-rs) | MIT OR Apache-2.0 on crates.io | Candidate; confirm licence files first. Partial port |
| [tiny-skia](https://github.com/linebender/tiny-skia) | BSD-3-Clause | Headless raster, as OpenCalc |
| [kurbo](https://github.com/linebender/kurbo), [peniko](https://github.com/linebender/peniko) | Apache-2.0 OR MIT | Curves and paint types |
| [parley](https://github.com/linebender/parley), [harfrust](https://github.com/harfbuzz/harfrust) | Apache-2.0 OR MIT; MIT | Headless text layout |
| [Tantivy](https://github.com/quickwit-oss/tantivy) | MIT | **Rejected for the engine.** A maintainer calls browser use [out of scope](https://github.com/quickwit-oss/tantivy/issues/2855) |
| [Tesseract](https://github.com/tesseract-ocr/tesseract), [tesseract-wasm](https://github.com/robertknight/tesseract-wasm) | Apache-2.0; BSD-2-Clause | Optional printed-text OCR for images |
| [ocrs](https://github.com/robertknight/ocrs) | Code is Apache-2.0 | **Do not bundle its models:** they [inherit CC BY-SA 4.0](https://github.com/robertknight/ocrs-models) |

**Handwriting recognition:** no good permissive on-device option was found.

## Gaps none of them fill

1. **A permissive stack all the way through.** Engine, sync server and apps under one permissive licence: not found.
2. **Text, canvas and pen in one structured notes app, with a good pen.**
3. **Sync people can trust.** Silent loss recurs across SiYuan, Logseq, Notesnook, Memos and Joplin, and no app reviewed offers a per-device view of what is unsynced.
4. **Conflict handling that is neither a chore nor invisible.**
5. **Encrypted sync to plain storage, with notes still readable outside the app.** Each app gives up one of these.
6. **Attachments designed for sync,** large, resumable and included in export.
7. **A published, testable promise that export is complete.** Memos comes closest, for its own migrations.
8. **A light desktop app.** AFFiNE and Anytype use Electron; AppFlowy uses Flutter.
9. **Right-to-left text,** which recurs as a complaint in BlockSuite and Anytype.
