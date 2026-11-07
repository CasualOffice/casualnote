# 23 — Storage format

**Status: design only. Nothing is built.** All sizes, windows and limits are proposals. Platform durability behaviour is from documentation and memory, is unverified, and is confirmed by `STO-01` and `STO-03`.

## Outcome

A live CasualNote notebook is a folder, `.cnote`, on a real disk or in a browser's origin-private storage. It meets these promises:

- **An edit is durable before it is shown as saved** (PR-1). A crash at any byte loses only what was not yet acknowledged.
- **Recovery is automatic, and never destroys bytes.** Anything cut from a log is first moved aside.
- **Damage is told apart from a crash,** and reported by name.
- **Attachments of large size resume** after interruption, and are checked by content hash (FR-08).
- **On desktop, the notes are also plain files** in a one-way mirror (PR-3). In a browser, walking away means export.
- **The format is versioned on its own** ([02](02-ARCHITECTURE.md)), and a newer format is refused and not guessed at.

This format is internal, and may change between major versions. The stable format for moving and leaving is [the package](36-EXPORT-FORMAT.md). `-store` owns this page's code, over a host seam.

## Design

### Layout

```
Notebook.cnote/
  manifest.json                       format version; no note content
  manifest.next.json                  only during a format migration
  tree/
    log/<device>-<sequence>.seg
    snapshot/<kind>-<device>-<seq>.snap
  pages/<k2>/<key>/
    doc.id                            the page id, UTF-8, for recovering orphans
    log/<device>-<sequence>.seg
    snapshot/<kind>-<device>-<seq>.snap
  databases/<k2>/<key>/
    doc.id
    log/...
    snapshot/...
  blobs/<b2>/<sha256>/
    index.part                        while incomplete
    index.cnb                         present and valid means complete
    chunks/00000000 ...               raw bytes, in order
  readable/                           the mirror; deleting it loses nothing
    Project/Meeting.md
    Project/Meeting.json
    Project/Meeting.svg
    files/...
  readable-edited/                    files a person changed in the mirror, set aside
  local/                              this device only; never synced
    device.json
    lock
    poison.marker
    mirror.idx
    verified.idx                      files whose whole-file hash has been checked once
    quarantine/
    reserve.bin
```

| Name | Rule |
| --- | --- |
| `<key>` | Lower-case hex of the first 16 bytes of `SHA-256(class byte, id bytes)`. **A document id is never a path,** so a hostile or long id cannot name one, and no case or Unicode-form collision can occur |
| `<k2>`, `<b2>` | First two hex characters, so no single directory grows past what file systems and OPFS handle well |
| `<device>` | 32 lower-case hex characters |
| `<sequence>` | 16 lower-case hex characters. Per device, per document, gapless, from 1 |
| `<sha256>` | The whole file's SHA-256, 64 lower-case hex characters, the same hash as in the package's `assets/` |

**The log directory holds every device's segments,** its own and those fetched from others. A device only ever creates files under its own device id (ADR-021).

**Who may write what.** Sync ([56](56-SYNC-DESIGN.md)) reads sealed segments, and adds other devices' segments. Only `-store` writes everything else. `local/` is never uploaded, and neither is `readable/` for an encrypted notebook.

### manifest.json

```json
{
  "format": "casualnote.notebook",
  "storage": { "major": 1, "minor": 0 },
  "workspace": "<workspace id>",
  "crdt_codec": 1,
  "op_schema": 1,
  "sync_epoch": 0
}
```

No titles, names or text, so a manifest exposed by a storage provider reveals nothing about the notes. `x-` keys are preserved. It is written once at creation and changes only in a migration, using the two-file method under Format versioning.

### Segment files

A segment is one append-only file of frames for one document from one device. **Sealed segments are immutable** and are the only form that leaves the device. An active segment grows, and only its unacknowledged tail can ever be cut.

All integers are little-endian. `crc32c` is the Castagnoli polynomial. Reserved bytes are zero, and are refused if not (`CN-STO-0004`).

#### Segment header, 128 bytes

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 8 | magic | `43 4E 53 45 47 0D 0A 1A` (`CNSEG`, CR, LF, 0x1A). Detects text-mode mangling, as PNG does |
| 8 | 2 | format_major | Storage major version, 1 |
| 10 | 2 | format_minor | 0. Minor versions only add frame types or use flags that a reader may skip |
| 12 | 2 | header_len | 128 |
| 14 | 1 | doc_class | 1 tree, 2 page, 3 database |
| 15 | 1 | crdt_codec | 1 = Loro. Names how frame payload updates are read, so framing is independent of the library |
| 16 | 2 | codec_version | Encoding version within the codec |
| 18 | 2 | flags | 0 in version 1 |
| 20 | 4 | reserved | 0 |
| 24 | 16 | device_id | Must match the file name |
| 40 | 8 | sequence | Must match the file name |
| 48 | 8 | created_ms | Signed. Host clock, informative only |
| 56 | 32 | doc_key | Full SHA-256 of class byte and id. Must match the directory, so a segment cannot be filed under another document |
| 88 | 32 | prev_segment_sha256 | Hash of the same device's previous sealed segment for this document; zeros at sequence 1 |
| 120 | 4 | reserved | 0 |
| 124 | 4 | header_crc32c | Over bytes 0 to 123 |

`prev_segment_sha256` chains a device's segments. Two different segments claiming the same device and sequence, or a broken chain, show that a device was forked by a restored backup or a copied folder (`CN-STO-0007`). Neither one is thrown away.

#### Frame, 32-byte header then payload

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 4 | frame_magic | `CNFR` |
| 4 | 1 | frame_type | 1 Batch, 2 Peer, 3 Seal. A reader skips unknown types only if the file's minor version permits it |
| 5 | 1 | frame_flags | 0 in version 1. Bit 0 reserved for compression |
| 6 | 2 | reserved | 0 |
| 8 | 8 | frame_index | From 0 within the segment, gapless |
| 16 | 4 | payload_len | Checked against the limit **before** any allocation |
| 20 | 4 | payload_crc32c | Over the payload |
| 24 | 4 | prev_crc32c | `payload_crc32c` of the previous frame, 0 for the first. Detects a missing or swapped frame |
| 28 | 4 | frame_crc32c | Over bytes 0 to 27 of this header |
| 32 | payload_len | payload | As below. No padding |

The CRCs catch tearing and bit rot. They are not security. Authenticity against a hostile party comes from `-crypto` sealing the whole segment for sync ([56](56-SYNC-DESIGN.md)).

#### Batch payload, type 1

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 2 | op_schema | Operation schema version |
| 2 | 1 | origin | 1 User, 2 Undo, 3 Redo, 4 Assistant, 5 Import, 6 Restore, 7 Review, 8 System |
| 3 | 1 | reserved | 0 |
| 4 | 16 | batch_id | From [`OpBatch`](24-OPERATIONS-AND-MERGE.md) |
| 20 | 8 | wall_ms | Signed. Host clock, informative only |
| 28 | 16 | group_id | Zeros if none |
| 44 | 2 | group_index | |
| 46 | 2 | group_total | 0 if none |
| 48 | 4 | summary_len | Up to 64 KiB |
| 52 | 4 | update_len | Bytes of CRDT update |
| 56 | summary_len | summary | `u16` kind count, then per kind `u16` op kind and `u32` count, then `u16` label length and a UTF-8 label (a tool name). No note text |
| then | update_len | update | Opaque, read only by `-merge` under `crdt_codec` |

`summary` and `update` must fill the payload exactly (`CN-STO-0005`). The summary is display data from an untrusted source: it is never used for merge decisions ([24](24-OPERATIONS-AND-MERGE.md)).

#### Peer payload, type 2

Written as the first frame of each segment, and whenever the peer id changes.

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 8 | peer_id | The CRDT peer id in use from here on |
| 8 | 1 | reason | 0 unchanged, 1 new device, 2 recovery truncated a tail, 3 install id changed, 4 fork detected, 5 previous segment abandoned after a failed flush |
| 9 | 7 | reserved | 0 |

#### Seal payload, type 3, 48 bytes

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 8 | frame_count | Frames before this one |
| 8 | 8 | bytes_before | File offset of this frame's header |
| 16 | 32 | segment_sha256 | SHA-256 over bytes 0 to `bytes_before - 1` |

Nothing may follow a seal (`CN-STO-0005`). **Foreign segments must be sealed.** A device refuses an unsealed segment from another device.

**A segment is sealed when:** it reaches 8 MiB; the notebook closes cleanly; or sync is on and the document has been idle for 30 seconds. On the next write, a new segment starts. Compaction bounds the local file count. How many small uploaded segments sync storage keeps is for [56](56-SYNC-DESIGN.md).

### Durability: fsync order

The rule: **nothing is acknowledged until every byte needed to read it back is durable,** including the directory entry that names the file.

**Creating a segment.**

| Step | Action | Crash here leaves |
| --- | --- | --- |
| 1 | Create the file, write the 128-byte header | Nothing referenced. Removed by recovery |
| 2 | `sync_file` the segment | The same |
| 3 | `sync_dir` the `log/` directory, and its parents if they were created in this call | A valid empty segment |
| 4 | Write the Peer frame, `sync_file` | A valid segment |

**Appending a batch.**

| Step | Action | Crash here leaves |
| --- | --- | --- |
| 1 | `MergeDoc::apply` has updated memory | Nothing durable. Not acknowledged |
| 2 | `write_at(tail, header + payload)` | A possible torn tail. Not acknowledged |
| 3 | `sync_file` | Maybe durable, maybe not. Not acknowledged, and either is correct |
| 4 | Only now: advance `durable_tail`, complete the `AckTicket` | Durable and acknowledged |

**Group commit.** Keystrokes do not each pay for a flush. A single writer runs: if no flush is in flight, it flushes now. Frames arriving during a flush wait, and all of them share the next one, and are acknowledged together. There is no timer on this path, so an idle system flushes immediately, and a slow disk naturally groups more. Coalescing a typing burst into one frame can be done before `write_at`, up to 50 ms, and is measured by `SPK-02`. This is why painting never waits for a flush, as explained in [24](24-OPERATIONS-AND-MERGE.md).

**Sealing.** Write the Seal frame, `sync_file`. Only then may sync upload the file. A seal is valid only if its hash matches, so no rename is needed.

**Ordering across documents.** A new document's files are durable (`sync_dir` included) before the tree or block that refers to it is acknowledged, as [24](24-OPERATIONS-AND-MERGE.md) orders. A blob's `index.cnb` is durable before any op that references it is applied.

**The commit point is always a valid new file, never a rename.** Snapshots, blobs and seals all become real by having a valid footer or hash. So OPFS and every native file system behave the same, and rename atomicity is not relied on. The only exception is the mirror, which is not a source of truth.

**When `sync_file` fails.** After a failed flush, the operating system may have dropped dirty pages, and retrying can falsely report success. So a failure is never retried on that file:

1. Stop acknowledging, and show "not saved" (`CN-STO-0010`).
2. Leave that segment as it is, and open a new segment with the next sequence, with `prev_segment_sha256` zeros and reason 5 in its Peer frame. The abandoned segment is sealed at the next open, after recovery has found its `good_end`, so its acknowledged frames can still be uploaded. If it cannot be read back then, its fate is decided by step 7 of recovery.
3. Write all unacknowledged frames, still in memory, to the new segment, and flush.
4. If that succeeds, acknowledge. If not, keep the edits in memory, offer a package export to another location, and keep retrying on a timer.

**Disk full** (`CN-STO-0011`). `local/reserve.bin`, 64 MiB on desktop, is released so that recovery, compaction and export can still run. Nothing is acknowledged until space returns, and the edit is kept in memory. The person is told, in words that say what is at risk.

### Crash recovery

Runs on open. It reads the whole of what it uses, is idempotent, and may itself crash and restart. Document-level steps run lazily when a document is opened. The tree is opened first.

| Step | Action | Outcome |
| --- | --- | --- |
| 1 | Take `local/lock` through the host | Held elsewhere: `CN-STO-0003`. Read-only or forward to the holder |
| 2 | Read `manifest.next.json`, if valid. Otherwise read `manifest.json` | Finish a migration if one was under way. Missing or invalid: `CN-STO-0001`. Newer major: `CN-STO-0002`, nothing written |
| 3 | Read `local/device.json`, and compare `install_id` with the host's | No file: new device. Mismatch: a copied folder. New device id and peer, `CN-STO-0018` reported as information. `local/` is otherwise reset |
| 4 | Check `local/poison.marker` | Present: the named segment is moved to `local/quarantine/`, `CN-STO-0016` reported, marker removed |
| 5 | **For each document opened:** list `log/`, and parse headers | A bad header on a foreign file: quarantined. On this device's own newest file: see step 6. Name and header disagree: `CN-STO-0004` |
| 6 | Scan this device's active segment frame by frame, checking every CRC and `frame_index` | Find `good_end`, the end of the last valid frame |
| 7 | **Decide tail or damage.** Search after `good_end` for a later valid frame: `frame_magic` with a valid `frame_crc32c` and valid payload | **None: a torn tail,** go to step 8. **Found: damage in the middle,** not a crash. Do not truncate. Copy the file to quarantine, read up to `good_end`, report `CN-STO-0005`, and mark for refetch from other devices. Start a new segment |
| 8 | Copy the bytes after `good_end` to `local/quarantine/`, then `set_len(good_end)`, then `sync_file` | **Bytes are moved aside before they are cut.** Only unacknowledged data can be here, because an acknowledged frame was flushed in full |
| 9 | If step 8 cut anything, or step 3 found a change: write a Peer frame with a new peer id | So counters that may have been used are never reused |
| 10 | Sealed segments: check `segment_sha256`, once per file, recorded in `local/verified.idx` | Mismatch: `CN-STO-0006`, quarantined, refetched. Other documents are unaffected |
| 11 | Check chains and sequence order per device | Gap: wait and report in the state panel (PR-2). Fork: `CN-STO-0007`. Both branches are imported, since CRDT import merges them |
| 12 | Choose the best snapshot: valid footer, then the most covered frames | Invalid snapshots ignored, and removed by compaction. Base snapshots checked first |
| 13 | `MergeDoc::load` the snapshot. On a snapshot's first use (`verified.idx`), project it and compare its state hash | Cache mismatch: `CN-STO-0012`, discard it and rebuild from the log. Base mismatch: `CN-STO-0012` and `CN-STO-0019`, refetch |
| 14 | Guarded import of every frame not covered, then project and repair | [24](24-OPERATIONS-AND-MERGE.md) |
| 15 | Remove stale `index.part` files only if older than 30 days, and any unreferenced empty segments | |
| 16 | Open for writing | First write starts a new segment if the last one is sealed |

**A limit worth stating.** Bit rot in the very last acknowledged frame looks like a torn tail. Bytes go to quarantine and the frame is lost locally. Other devices, or the quarantine file, are the only remedy, and a single-device notebook has no other copy. We do not claim to prevent this.

**Crash suite** (`STO-01`). An in-memory host records every I/O request. The test crashes at every point, applying any subset of unflushed writes, including torn and reordered ones, and asserts: every acknowledged batch is present, the state hash matches a replay of acknowledged batches plus possibly later ones in order, and nothing panics.

### Snapshots and compaction

| Kind | File prefix | Holds | Safe to delete? |
| --- | --- | --- | --- |
| Cache | `cache` | CRDT state with full history, a shortcut to load | **Yes,** while the segments it covers exist |
| Base | `base` | CRDT state with full history, replacing segments that were removed | **No.** It holds the only local copy of what it covers |
| Shallow | `shallow` | CRDT state with history cut at a trim point | **No.** Written only under the trim rules |

An earlier design note said that snapshots are always safe to delete. That is true only of cache snapshots.

#### Snapshot file

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 8 | magic | `43 4E 53 4E 50 0D 0A 1A` (`CNSNP`, CR, LF, 0x1A) |
| 8 | 2 | format_major | 1 |
| 10 | 2 | format_minor | 0 |
| 12 | 2 | header_len | 144 |
| 14 | 1 | doc_class | As for segments |
| 15 | 1 | crdt_codec | As for segments |
| 16 | 2 | codec_version | |
| 18 | 1 | kind | 1 cache, 2 base, 3 shallow |
| 19 | 5 | reserved | 0 |
| 24 | 32 | doc_key | |
| 56 | 8 | created_ms | Informative |
| 64 | 16 | device_id | Author |
| 80 | 8 | snapshot_seq | Per device, per document, ascending |
| 88 | 32 | state_hash | [The state hash](24-OPERATIONS-AND-MERGE.md) at this snapshot |
| 120 | 4 | covers_len | Bytes of coverage |
| 124 | 4 | version_len | Bytes of opaque version |
| 128 | 8 | payload_len | Bytes of CRDT state |
| 136 | 4 | reserved | 0 |
| 140 | 4 | header_crc32c | Over bytes 0 to 139 |
| 144 | covers_len | covers | Repeated: `device_id` (16), `sequence` (8), `frames` (8), one per segment covered in part or whole |
| then | version_len | version | Opaque |
| then | payload_len | payload | Opaque |
| then | 32 | content_sha256 | Over every byte before this field |
| then | 4 | end_magic | `CNEN` |
| then | 4 | footer_crc32c | Over the previous 36 bytes |

**When.** Off the typing path, and never during a burst of typing: when the uncovered tail exceeds 1 MiB or 1,000 frames; on closing a document whose tail exceeds 64 KiB; after a long catch-up.

**Taking a snapshot.** Write in full, `sync_file`, `sync_dir`. It counts only if the footer checks. Keep the previous one until the next successful load using the new one.

#### Compaction

Replaces covered segments with a base snapshot, to keep storage in bounds. Full history is kept, so **compaction loses no merge ability and no version history.** It is local: the uploaded copies on sync storage follow their own retention ([56](56-SYNC-DESIGN.md)).

| Step | Action | Crash here leaves |
| --- | --- | --- |
| 1 | Pick sealed segments for this document that are all imported | No change |
| 2 | Conditions: this device's own segments are uploaded if sync is on; no segment is quarantined; the document is not being read for a fork | No change |
| 3 | Write a `base` snapshot with `covers`, and verify by re-reading: load it into a fresh instance and compare `state_hash` | Extra file, harmless |
| 4 | `sync_file`, `sync_dir` on the snapshot | Both forms present |
| 5 | Delete covered segments, then `sync_dir` | Some segments left: harmless, since import is idempotent |
| 6 | Delete older cache and base snapshots covered by the new one, `sync_dir` | |

`STO-02`: the state hash after rebuild equals the original. Sequence numbers do not reset. The chain continues from `covers`, so `prev_segment_sha256` is checked only for segments still present.

#### Trimming

Trimming is separate from compaction and removes history. Loro's documentation says peers "can only sync if they have versions after the shallow snapshot point", and advises coordinating before trimming. Only `-sync` starts it, under rules in [56](56-SYNC-DESIGN.md) (`SYN-05`):

1. Never past the oldest registered device's acknowledged state.
2. Removing a device is an explicit action by a person.
3. Never past a `ReviewMarker` that stores only a version reference.
4. A device beyond any limit does not lose its edits. Its own copy is projected and arrives as a visible restored copy of those pages. Nothing is silently dropped (PR-1).

Whether to trim at all before `SPK-01` reports is open. Compaction alone may be enough.

### Blobs

Attachments, PDF backgrounds, extension files and large strokes. **Content-addressed, chunked, immutable.** File name and type live in the referring block, so identical bytes under two names are stored once.

#### index.cnb

| Offset | Size | Field | Meaning |
| --- | --- | --- | --- |
| 0 | 8 | magic | `43 4E 42 4C 42 0D 0A 1A` (`CNBLB`, CR, LF, 0x1A) |
| 8 | 2 | format_major | 1 |
| 10 | 2 | format_minor | 0 |
| 12 | 4 | chunk_size | 4 MiB. Every chunk but the last is this size |
| 16 | 8 | total_len | Bytes |
| 24 | 4 | chunk_count | Must equal `ceil(total_len / chunk_size)`, checked before allocation. An empty blob has 0 |
| 28 | 4 | reserved | 0 |
| 32 | 32 | blob_sha256 | Must match the directory name |
| 64 | 32 × count | chunk hashes | SHA-256 per chunk, in order |
| then | 4 | crc32c | Over every byte before this field |

`index.part` has the same format. **Chunks are raw bytes** with 8-hex-digit names, so `cat chunks/*` reconstructs the file with no CasualNote software (PR-3).

Fixed-size chunks were chosen over content-defined chunking. Most attachments are immutable and compressed, so sub-file deduplication would rarely help, and fixed chunks are simple to resume.

#### Adding a blob and resuming

| Step | Action | If interrupted |
| --- | --- | --- |
| 1 | Hash the source as a stream, recording the chunk hashes. The whole-file hash names the directory | Nothing written |
| 2 | If `index.cnb` is already valid, done | Deduplicated |
| 3 | Write `index.part`, `sync_file`, `sync_dir` | Resumable from here |
| 4 | For each chunk: if the file exists with the right length, skip it. Otherwise write, `sync_file` | Restart at step 4. Skipped chunks are verified at step 6 |
| 5 | `sync_dir` on `chunks/` | |
| 6 | Re-read every chunk, and check each hash and the whole hash. A bad chunk is rewritten | Detects a source that changed while being copied: `CN-STO-0013`, and the person is told |
| 7 | Write `index.cnb`, `sync_file`, `sync_dir`. **The blob now exists** | |
| 8 | Delete `index.part` | Harmless |
| 9 | Apply the op referring to it | The blob is unreferenced, and collected later |

Download from sync (`FIL-01`) follows the same steps: `index.cnb` is fetched first, as `index.part`, and each chunk is checked on arrival against its hash. Upload resumes per chunk ([56](56-SYNC-DESIGN.md), [57](57-SERVER-DESIGN.md)). A foreign index is hostile: length and count are checked against limits before allocating.

**Hashing is off the typing path.** On the web it runs in a worker.

**Collection.** Mark and sweep, off the typing path. Roots: every document's current state, trash, retained history, open review markers. A blob with no root for 30 days, after every registered device has acknowledged the removal, is deleted. It runs only at a person's request in a browser under quota pressure, since recovering a mistake there is harder.

**A missing blob is never a broken note.** The record stays and shows as missing (R-20, [22](22-DATA-MODEL.md)), and the state panel names it.

### The readable mirror

**One-way, regenerated, never read back** (ADR-010, `STO-04`). It is how PR-3 is kept on desktop: open the folder, and read the notes.

| File | Content |
| --- | --- |
| `readable/<path>/<title>.md` | The same Markdown as the package's `readable/`, with front-matter `lang` when the page has one |
| `readable/<path>/<title>.json` | Exactly the package's page JSON, per the same schema. A database page also gets its database's JSON, as `<title>.database.json` |
| `readable/<path>/<title>.svg` | How the ink and placed items look, when the page has a canvas |
| `readable/files/<name>` | Attachments under real names |

- **Paths** are made as for the package: NFC for the path only, unsafe characters replaced, the same `safeName` rule, and id-suffixed on a case-insensitive collision. Very long titles are shortened by bytes at a character boundary to fit a 255-byte name.
- **Trigger.** A `Delta` subscription, debounced by 2 seconds per page, off the typing path. **A mirror failure never blocks an acknowledgement** (PR-4), and shows in the state panel as `CN-STO-0020`.
- **Writing.** Temp file then rename. This is the one place rename is used. A crash leaves the old or new version, and a leftover temp is removed.
- **Large files.** Attachments are copied, which doubles their disk use. Attachments above 256 MiB are not copied by default, and a `.txt` beside the name says how to rebuild it from `blobs/`. A setting changes this.
- **Rebuild.** `local/mirror.idx` records path, size and hash per file. A background check repairs differences. Deleting `readable/` and reopening restores it (`STO-04`).

**"Never read back" and a person's edits.** A person may edit a mirror file. The app never imports it, because a mirror file shares no merge history with the notes ([24](24-OPERATIONS-AND-MERGE.md)). But overwriting it would silently destroy their work. So before replacing a file, the writer compares its size, modified time and hash with `mirror.idx`. If it differs, the file moves to `readable-edited/`, with a time in its name, and the state panel says so. The person can bring it in through normal import ([37](37-IMPORT-FORMATS.md)). Reading a hash for this comparison is the only read, and nothing from the file enters the notes.

**Encrypted notebooks.** `readable/` is plaintext on the device's own disk and is never uploaded. The folder must be excluded from any provider's sync, and the app warns if the notebook's folder appears to be inside one ([07](07-SECURITY-THREAT-MODEL.md)).

**The dependency.** `-store` does not depend on `-export`, which sits above it. The caller passes a `MirrorRender` value that turns a page into its files, so the layers in [02](02-ARCHITECTURE.md) hold. Settled in `FND-03`.

### The host I/O seam

`-store` has no I/O of its own and no `cfg` (19, `GOV-06`). It is **sans-I/O:** a state machine that issues requests and takes completions. This keeps one code path for native and wasm, needs no async runtime in `crates/`, and lets the crash suite simulate every I/O outcome.

```rust
pub enum IoRequest {
    Open { path: StorePath, mode: OpenMode },     // Read, ReadWrite, CreateNew
    ReadAt { file: FileHandle, at: u64, len: u32 },
    WriteAt { file: FileHandle, at: u64, data: Bytes },
    SyncFile { file: FileHandle },
    SyncDir { dir: StorePath },
    SetLen { file: FileHandle, len: u64 },
    Close { file: FileHandle },
    List { dir: StorePath },
    Remove { path: StorePath },
    CreateDir { path: StorePath },
    Stat { path: StorePath },
    Lock { name: LockName },
    Space,
}

pub struct Store { /* ... */ }
impl Store {
    pub fn poll_io(&mut self) -> Vec<(IoId, IoRequest)>;
    pub fn on_complete(&mut self, id: IoId, result: IoResult);
    pub fn append(&mut self, doc: DocKey, frame: Frame) -> AckTicket;
    pub fn poll_ack(&mut self) -> Vec<AckTicket>;
}
```

`StorePath` is a validated relative path built only from known directory names, hex keys and numbers. The host never receives a path derived from note content, except in the mirror.

| Request | Native desktop and server | Web, OPFS in a dedicated worker |
| --- | --- | --- |
| `Open` | `std::fs::OpenOptions` | `getFileHandle`, then `createSyncAccessHandle`. Handle creation is async, and a handle locks its file. Keep few open, closed by LRU |
| `ReadAt`, `WriteAt` | Positional I/O | Synchronous `read` and `write` with `at` |
| `SyncFile` | `File::sync_all`, or `sync_data` on a file that has not grown in metadata terms. On Apple platforms, Rust's std is documented to use `F_FULLFSYNC` (from memory; confirm at `STO-03`) | `flush()`. **Whether this survives power loss is browser-defined and unverified** |
| `SyncDir` | Open the directory, `sync_all`. On Windows there is no such call; NTFS journals metadata, so the seam returns success (unverified, `DEC-07`) | No-op. Creating an entry has no separate durability step exposed |
| `SetLen` | `File::set_len` | `truncate` |
| `List`, `CreateDir`, `Stat` | `std::fs` | Async directory iteration |
| `Remove` | `remove_file` | `removeEntry` |
| `Lock` | An OS advisory lock on `local/lock` | Web Locks API. A second tab takes a read-only role, or forwards to the holder ([42](42-EDITOR-AND-RENDERING.md)) |
| `Space` | Free disk space | `navigator.storage.estimate()` |

**The sync handle is available only in a dedicated worker,** so on the web the engine runs in a worker, and the tab talks to it over messages ([55](55-SDK-AND-EMBEDDING.md)). IndexedDB holds small per-browser settings only, never notes (the storage row for the web host in [02](02-ARCHITECTURE.md) lists both).

**Same bytes on native and web** (`STO-03`): given the same recorded `Update` bytes and the same ids and clock values as inputs, the files are byte-identical, by test. Whether the library's own encoding is the same on both is a question for `SPK-01`.

**A notebook placed in a third-party sync folder** (Dropbox, OneDrive, iCloud). Audit evidence shows this causes loss for other apps (Logseq, in [12](12-OPEN-SOURCE-AUDIT.md)). The lock and flush may be unreliable there, and files may be placed or replaced by a third party. The host warns when a path looks like one, and header, chain and hash checks catch what they can. ADR-021's way to use such a folder is as a **sync target**, not as the live notebook.

### Browser limits

**A browser's storage is private to the site. There is no folder that a person can open, so on the web, walking away means export.** The person is told this plainly, at first run and in settings.

| Limit | Handling |
| --- | --- |
| No readable mirror | No `readable/` on OPFS. PR-3 is kept through one-click export of the whole workspace ([36](36-EXPORT-FORMAT.md)), and the settings show when the last export was made |
| The browser may evict the site's storage | Call `navigator.storage.persist()` when the first notebook is created, and show the answer (`CN-STO-0017` if refused). Eviction rules differ by browser and are unverified. Refused persistence means the promise is weaker, and the app says so, recommending regular export or sync |
| Quota | Check `estimate()` before large blob writes. On a quota error: `CN-STO-0011`, stop acknowledging, keep the edit in memory, and offer export at once, since export needs no extra origin space |
| Clearing site data, private windows | Everything is gone, and we cannot prevent it. A private window is detected where possible and shown as temporary |
| OPFS missing | Notebook creation refused with `CN-STO-0017`. There is no in-memory mode that looks like saving. A plainly labelled scratch mode may be offered. Export still works |
| Flush semantics | Unverified. An acknowledgement on the web means the browser accepted the flush, and we say no more than that until `STO-03` reports |
| Multiple tabs | One writer, by Web Locks. A tab that loses the lock reads through the holder |
| Worker killed by the browser | Unacknowledged edits are lost; acknowledged ones recover through the steps above. "Saved" is never shown early |
| Size | Attachments are limited by quota and by the export file size. A notebook too large to export in one file is split by the export flow ([36](36-EXPORT-FORMAT.md) has no split form yet, so this is a gap) |

### Format versioning

Independent of the SDK, op schema, package and sync protocol versions ([02](02-ARCHITECTURE.md)).

| Change | Version | Rule |
| --- | --- | --- |
| New optional frame type, flag or `x-` manifest key that an old reader can skip | Minor | Old readers read on, and write segments with their own minor |
| Anything an old reader cannot safely read | Major | Refused with `CN-STO-0002`, a clear message, and no write of any kind |
| New CRDT encoding | `codec_version`, per segment | Old readers refuse that segment only (`CN-STO-0015`), and keep working on the rest. The state panel shows it |
| New library | `crdt_codec`, with `sync_epoch` | [How to replace Loro](24-OPERATIONS-AND-MERGE.md) |
| New op | Op schema, per batch | The CRDT update still imports. New fields appear as Raw data that repair leaves alone |

**Migration, for a major bump.**

1. Take a package export to `local/`, for a final safety net.
2. Write new-format files beside the old ones. Old files stay.
3. Write `manifest.next.json`, `sync_file`, `sync_dir`. **This is the commit point.**
4. Remove the old `manifest.json`, write it again from the next one, `sync_file`, `sync_dir`, and remove `manifest.next.json`.
5. Delete old-format files.

A crash before step 3 leaves the old notebook intact, and the leftovers are removed. A crash after it finishes on the next open (recovery step 2). All devices may not move together: a device can sync with others on an older format only if the sync protocol says so ([56](56-SYNC-DESIGN.md)).

## Failure modes and limits

### Codes

For [20](20-ERROR-CODE-REGISTRY.md).

| Code | Meaning | What the person sees |
| --- | --- | --- |
| `CN-STO-0001` | Not a notebook, or the manifest is invalid | The folder cannot be opened. Nothing changed |
| `CN-STO-0002` | Written by a newer major version | Update the app. Nothing changed |
| `CN-STO-0003` | Another process or tab holds the notebook | Read-only, or switch to it |
| `CN-STO-0004` | Segment header invalid, or name and contents disagree | The file is set aside, and named |
| `CN-STO-0005` | Frame damage that is not a torn tail, or a malformed payload | Damage reported per document. The rest is readable, and refetch is offered |
| `CN-STO-0006` | Sealed segment hash mismatch | As above |
| `CN-STO-0007` | Sequence gap or fork in a device's segments | State panel. Gaps wait, forks merge |
| `CN-STO-0008` | Frame over the limit | Refused before allocation |
| `CN-STO-0009` | Too many files or frames in a segment or document | Refused for foreign data; rolls for own data |
| `CN-STO-0010` | Flush failed | "Not saved", plus what to do |
| `CN-STO-0011` | No space or over quota | "Not saved", plus export and clean-up options |
| `CN-STO-0012` | Snapshot invalid, or its state hash did not match | Normally silent rebuild, logged |
| `CN-STO-0013` | Blob hash or size mismatch, or the source changed while copying | Named file. Retry |
| `CN-STO-0014` | Blob over the limit | Named file |
| `CN-STO-0015` | Unknown `crdt_codec` or `codec_version` in one file | Update the app, and the rest stays usable |
| `CN-STO-0016` | A segment crashed the import, and was quarantined | State panel, naming the device |
| `CN-STO-0017` | Persistent storage refused or unavailable | A plain warning, or creation refused |
| `CN-STO-0018` | `install_id` differs: a copied or restored folder | Information. A new device identity is registered |
| `CN-STO-0019` | A document is unavailable because every copy failed checks | Listed. Export of everything else still works |
| `CN-STO-0020` | Mirror write failed | State panel. Notes are unaffected |

### Limits

Proposals, for [21](21-PARSER-LIMITS.md). Foreign data is checked before allocation.

| Limit | Value | Code |
| --- | --- | --- |
| Frame payload | 64 MiB | `CN-STO-0008` |
| Summary in a frame | 64 KiB | `CN-STO-0005` |
| Own segment size before rolling | 8 MiB, or one larger frame | Rolls |
| Foreign segment size | 64 MiB sealed, 256 MiB opened, as [56](56-SYNC-DESIGN.md) | `CN-STO-0009` |
| Frames per segment | 1,000,000 | `CN-STO-0009` |
| Segments per document before compaction is forced | 10,000 | `CN-STO-0009` |
| Snapshot payload | 4 GiB | `CN-STO-0012` |
| Snapshot `covers` | 16 MiB | `CN-STO-0012` |
| Blob | 1 TiB | `CN-STO-0014` |
| Chunks per blob | Follows from the two values above: 262,144 | `CN-STO-0014` |
| Documents in a notebook | 1,000,000 | `CN-STO-0009` |
| `manifest.json` | 64 KiB | `CN-STO-0001` |
| `doc.id` | 1 KiB | `CN-STO-0004` |

**The export gap.** [36](36-EXPORT-FORMAT.md) limits a package to 2 GiB per entry and 20 GiB in total, while FR-08 says attachments of any size, included in export. A blob over 2 GiB can be stored but not exported in version 1. Today this design refuses that export, naming the file and offering the raw file from `blobs/`, and never silently drops it. The owner of 36 has to decide.

### Other failure modes

| Failure | Handling |
| --- | --- |
| Hostile segment file names (`..`, wrong length) | `List` results must match the exact pattern. Others are ignored and reported |
| Segment for an unknown document | A directory is created with its `doc_key`. Without a `doc.id` it can't be named, so it waits for the tree. Shown under Recovered (R-19) when it has a page's content |
| Edits arrive for a purged page | Its segments are kept as a restored copy, not refused, because a device that was offline may hold the only copy (`SYN-05`) |
| A backup is restored over the folder on the same machine | `install_id` matches, so this is not caught at step 3. The next sync sees a fork in the chain and rotates the peer. Until then, the device may reuse sequence numbers, and both branches are kept |
| Too many open files | Writers close by LRU. Only documents being edited hold a handle |
| A device's clock is wrong | No storage decision uses it |
| Slow disks and network drives | Group commit gives lower "saved" rates rather than a slow keyboard. A sustained slowdown appears in the state panel |

## Alternatives considered

| Alternative | Why not |
| --- | --- |
| SQLite as the store | Good durability, but a binary single file cannot be synced by "each device writes its own new files" (ADR-021). Trilium's one-database design has the same limit, and wasm SQLite on OPFS adds weight and a second engine |
| RocksDB or similar, as AppFlowy uses | C++ and heavy, and does not fit the web build |
| One log for all documents | Simpler flushing, but loading a page would mean reading others, and sync could not skip unchanged pages |
| Markdown files as the truth | Two imports share no ancestor, so merging breaks (ADR-010). The mirror keeps the readability |
| Rename as the commit point | OPFS rename support is uneven (unverified), and "a valid new file" works everywhere |
| Rewriting segments | Would break "each device only writes its own new files", and would confuse any storage provider |
| An `fsync` on a timer, with "saved" shown early | Breaks PR-1 |
| Syncing the mirror | It is derived, and would be a second truth |
| Reading mirror edits back automatically | No merge base, so it would silently overwrite notes |
| Content-defined chunking | Little benefit on compressed media, more code, and harder resume |
| Compressing frames by default | Costs CPU on the typing path. The flag bit is reserved, and `SPK-01` reports whether storage needs it |
| Blake3 for hashing | Faster, but the package's `assets/` uses SHA-256, and one hash lets export skip re-hashing |

## Acceptance gates

| Gate | Method | State |
| --- | --- | --- |
| Every acknowledged batch survives a crash at any I/O point | Crash suite, with torn and reordered unflushed writes (`STO-01`) | Not built |
| A bad frame in the middle is treated as damage, and at the tail as a crash | Fixtures per case | Not built |
| Truncated bytes reach quarantine first | Crash suite | Not built |
| Failed flushes are not retried on one file | Fault injection | Not built |
| Rebuild from segments equals base and cache snapshots | State hash (`STO-02`) | Not built |
| Deleting cache snapshots loses nothing; deleting `readable/` loses nothing | Tests (`STO-02`, `STO-04`) | Not built |
| Native and OPFS produce the same bytes from the same inputs | `STO-03` | Not built |
| Blob resume works after interruption at every step, and a bad chunk is caught | Injected failures (`FIL-01`) | Not built |
| A newer major is refused, and writes nothing | Fixture | Not built |
| Each `CN-STO` code has a fixture | Table-driven | Not built |
| Every parser has a fuzz target: segment, frame, snapshot, blob index, samples | `fuzz/` (`GOV-07`) | Not built |
| A mirror edit is set aside and never imported | Test (`STO-04`) | Not built |
| Mirror failure does not block saving | Fault injection | Not built |
| OPFS flush behaviour across browsers, reported as measured | `STO-03` | Not done |

## ADRs

ADR-010, ADR-006, ADR-005, ADR-017, ADR-021, ADR-018 (path handling for titles).

## Tracker ids

`STO-01`, `STO-02`, `STO-03`, `STO-04`, `STO-05`, `FIL-01`, `MRG-02`, `SPK-01`, `SPK-02`, `SYN-01`, `SYN-05`, `GOV-06`, `GOV-07`, `PKG-05`, `DEC-07`.
