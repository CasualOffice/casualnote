# 56 — Any-storage sync and encryption

**Status: design only, nothing built.** No sync code, cryptography or storage adapter exists. Every limit and timing here is a proposal. Statements about third-party storage are marked as documented (read, not tested) or unverified. `SPK-04` decides whether this design stands (ADR-021).

## Outcome

A person with two or more devices keeps one workspace in step through storage they already have: a folder, WebDAV or S3. No server of ours is involved. The storage holds ciphertext.

The design has to hold five promises at once:

1. **Each device writes only its own new files.** Two devices never write one file, so there is nothing to overwrite.
2. **Delay, drop, duplicate and reorder are all tolerated.** Any of them costs time and never costs an edit.
3. **Nothing is silently dropped.** A device beyond any limit gets a visible restored copy (PR-1).
4. **Every failure can be seen, per device** (PR-2).
5. **Absence is never an instruction.** A missing file, an empty listing or an unmounted drive never removes anything from a device.

This covers one person's devices. Sharing between people is [57](57-SERVER-DESIGN.md).

## Design

### Words used here

"Acknowledged" has two meanings in this project, so this document keeps them apart.

| Term | Meaning |
| --- | --- |
| **Saved** | Durably on local disk and shown as saved. [17](17-GLOSSARY.md) calls this an acknowledged edit, and it is what PR-1 and `SPK-04` protect |
| **Written** | A storage adapter reported that the object is stored. This is not evidence that any other device can see it |
| **Device ack** | A sealed record in which device D states that it has durably imported a range of another device's segments. This is the only evidence of delivery |
| **Registered device** | A device with an announce record and no removal record |
| **Floor** | For one notebook, the lowest device ack across registered devices |

A **workspace** has one sync root. A **notebook** is the unit of keys, acks and trimming. The core documents disagree on how the two relate: [17](17-GLOSSARY.md) has a workspace containing notebooks, and [22](22-DATA-MODEL.md) has one notebook holding exactly one workspace. This design works under either. If the relation is one to one, each device has one `n/<notebook>/` prefix, one notebook key under the workspace key, and subscriptions never vary. If a workspace can hold several, a device may subscribe to some only, and a device not subscribed to a notebook does not hold back its trimming.

### Where the code lives

| Part | Owner | Rule from [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) |
| --- | --- | --- |
| Exchange state machine, registry, acks, trim decisions, panel state | `-sync` | Owns no transport, names no platform |
| Key hierarchy, sealing and opening | `-crypto` | Owns no key storage |
| Local outbox and inbox, cursors, log export | `-store` over the host I/O seam ([23](23-STORAGE-FORMAT.md)) | No network |
| Guarded import, overlap detection | `-merge` ([24](24-OPERATIONS-AND-MERGE.md)) | The only crate naming the CRDT library |
| Folder, WebDAV and S3 adapters, timers, credentials, keychain | Host: `desktop/` ([44](44-DESKTOP-SHELL.md)) and `webapp/` | Network code never enters `crates/` |

**`-sync` has no I/O of its own.** It is a state machine: the host asks it for effects, performs them, and feeds back the outcome.

```rust
pub struct SyncEngine { /* no I/O, no clock, no RNG of its own */ }

impl SyncEngine {
    /// Effects the host must perform now. Each carries a request id.
    pub fn poll(&mut self, now: HostTime, rng: &mut dyn HostRng) -> Vec<Effect>;
    /// Feed back the outcome of one effect.
    pub fn on_outcome(&mut self, id: RequestId, outcome: StorageOutcome);
    /// Local log has new closed segments, the user acted, or a timer fired.
    pub fn on_trigger(&mut self, trigger: Trigger);
    /// Snapshot for the panel. Cheap, and never touches storage.
    pub fn status(&self) -> SyncStatus;
}

pub enum Effect { Storage(RequestId, StorageRequest), WakeAt(HostTime) }
```

This shape gives the same engine on native and web with no `cfg`, and it lets the fault-injection suite run the real engine under a simulated storage with a fixed seed. The adapter is a thin translator, so most of the logic is exercised without a network.

### Storage adapter interface

Four verbs. Nothing else is assumed.

```rust
pub enum StorageRequest {
    List { prefix: Key, after: Option<Key>, limit: u32 },
    Get { key: Key, max_len: u64 },
    PutIfAbsent { key: Key, body: BodyRef, len: u64 },
    Delete { key: Key },
}

pub enum StorageOutcome {
    Listed { entries: Vec<Entry>, next: Option<Key> },  // Entry { key, len }
    Got(BodyRef),
    Created,
    AlreadyExists,
    Deleted,                       // includes "was not there"
    NotFound,
    Transient { retry_after: Option<Duration>, detail: AdapterDetail },
    Unauthorised,
    Forbidden,
    QuotaFull,
    Unreachable,
    TooLarge,
}
```

| Verb | Contract the engine relies on | Contract it does **not** rely on |
| --- | --- | --- |
| `list` | Returns some of what is there, in any order, and may miss recent files | Completeness, or a listing that reflects a just-finished put |
| `get` | Returns some bytes or an error. The engine checks them by opening them | That the bytes are complete or current |
| `put-if-absent` | `Created` means the adapter handed the object to storage. `AlreadyExists` means something is under that name | Atomic create on every backend, or any delivery to another device |
| `delete` | Removes the object, eventually, perhaps | That it stays gone: a sync client or a backup can bring it back |

**Why weak contracts are enough.** Only one device ever writes a given name, and object identity comes from the sealed header rather than from the name (below). `put-if-absent` therefore does not arbitrate between devices. It catches two rarer things: a retry after a crash, and a cloned device. If a backend cannot do it atomically, those two cases fall back to the hash chain, which is slower to notice but still notices.

**Key grammar.** Every key is lower-case ASCII hex, digits, `/` and `.`, so case folding, Unicode normalisation and Windows reserved names cannot collide or change a name. No key begins with a dot, because some sync clients are reported to skip dotfiles (unverified).

### What each storage guarantees

Read this table as "what we may assume". Anything not stated as documented is unverified, and no entry has been tested by this project.

| Property | Plain local folder | Cloud client folder (Dropbox, OneDrive, iCloud Drive, Google Drive and similar) | WebDAV | S3 (AWS) | S3-compatible |
| --- | --- | --- | --- | --- | --- |
| Atomic create | `O_EXCL` temp file, then `link(2)`, is the POSIX route. Windows route is `MoveFileEx` without replace. Unverified on SMB, NFS and FUSE | Local step as plain folder. **No atomicity across computers** | `If-None-Match: *` is defined by HTTP. `MOVE` with `Overwrite: F` may return 412, per RFC 4918 §9.9. Whether a given server honours either: unverified | **Documented:** [`If-None-Match: *` returns 412 if the key exists, and 409 on a conflicting concurrent operation, which should be retried](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html) | Unverified per product. Probed at setup |
| Read after write | Yes on one machine | **No.** Other computers see it when the client gets round to it | Unverified per server | **Documented:** [strong read-after-write for PUT, DELETE and LIST](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html) | Unverified |
| Partial file visible | Not when using temp and link | **Assumed yes** on other computers while a file arrives | Unverified. Some servers are reported to use part files | Documented as never partial for a single key | Unverified |
| Conflict copies | No | **Yes, assume so.** Exact naming varies and is unverified | Unverified | No | Unverified |
| Files hidden or dehydrated on demand | No | **Assume yes.** A read may block, fail or return a placeholder. Unverified per client | No | Archive storage classes can refuse reads (documented class names, behaviour untested here) | Unverified |
| Deleted file can come back | Only from a user restore | **Assume yes** | Unverified | Versioned buckets keep deleted versions | Unverified |
| Listing depth | Any | Any, after the client has fetched it | One level is safe. RFC 4918 §9.1 allows servers to refuse `Depth: infinity` | Prefix listing with `start-after` | Unverified |
| Available from a browser | Chromium's File System Access API only, as far as checked (unverified for other browsers) | Same | Only if the server sends CORS headers, which most may not (unverified) | Only with a bucket CORS rule | Same as S3 |

**Setup probe.** On connecting storage, the adapter runs a probe under its own prefix: create, create again and expect refusal, list, get, delete, list again, then large and small object round trips. The result is stored and shown, for example "Create-once: confirmed at setup on 2026-11-07". A failed create-once probe is a warning and not a refusal, for the reason above. A failed round trip refuses the storage (`CN-SYN-0002`). A probe says what happened once, and is not a guarantee.

**Web host limit.** On the web, "any folder, WebDAV or S3" (FR-19) narrows in practice to whatever the browser can reach, so the web app shows a per-storage explanation and never fails silently. Proposed row `FND-21` decides the published list per host. Storage credentials are host secrets and are never synced, exported or placed in the mirror (`check-secrets.py`, [15](15-CI-AND-RELEASE-GATES.md)).

### Cloud folder clients and conflict copies

A cloud client sits between us and the real store and has its own opinions. The rule is:

> **A name is a hint. The sealed header is the identity.**

| What the client may do | How the reader responds |
| --- | --- |
| Create a conflict copy, such as `000…12.cnss (conflicted copy)` or `000…12 (1).cnss` | A file under `d/` whose name fails the grammar is a **candidate**. It is size-checked and opened. If it opens, its identity comes from the sealed header. Same content hash as a known object: a duplicate, ignored and counted. Different content for the same device and sequence: a fork, handled below |
| Present a file that is still arriving | Open fails. It stays `Incomplete` and is retried with backoff. It becomes `Corrupt` only after 5 attempts and 24 hours (proposal), and even then it is kept |
| Present a placeholder or zero-length file | Same as incomplete, and shown as "Waiting for your storage client" |
| Block a read for a long time | The adapter has a timeout, proposed as 120 s, and reports `Transient` |
| Sync our temp files | Temp names start `tmp-` and readers ignore them. The owner removes its own stale temp files |
| Bring a deleted file back | Harmless: known hash, ignored. The owner deletes it again, at most once per day (proposal) |
| Restore the whole folder to an older state | `StorageRegressed`, below |
| Not sync at all, because it is paused or signed out | The storage looks healthy and other devices go quiet. Only device acks reveal this, so the panel never says "synced" on the basis of `Written` |
| Rename or mangle a file | Candidate handling, as for a conflict copy |
| Leave files that have no valid seal | Retried at most once per day, counted as foreign, never deleted by us. Limit below |

Conflict copies are never deleted automatically unless they are byte-identical to the canonical object and owned by this device.

**The readable mirror must not be in the sync root.** If the live readable folder ([44](44-DESKTOP-SHELL.md)) lies inside a cloud client's folder, plaintext reaches that provider regardless of this design. The desktop host refuses a mirror inside the sync root (`CN-DSK-0006`) and warns if it sits inside a known cloud folder.

### Layout on storage

```
<root>/
  casualnote-sync.json              plaintext, write-once: { format, sync_id, created_protocol }
  d/<device>/                       32 hex. Only this device writes here
    r/<seq16>.cnsr                  registry records
    k/<seq16>.cnsk                  key records
    p/tmp-<random>                  setup probe files
    n/<notebook>/                   32 hex, random, not derived from any title
      s/<shard4>/<seq16>.cnss       segments; shard = seq >> 12
      a/<seq16>.cnsa                device acks
      c/<id32>/m.cnsc               checkpoint manifest
      c/<id32>/<part8>.cnsc         checkpoint parts
      b/<tag32>/m.cnsb              blob manifest
      b/<tag32>/<index8>.cnsb       blob chunks
```

Page ids, titles, tags and languages appear nowhere outside the sealed body. This layout is the sync protocol's own, and is separate from the local layout in [23](23-STORAGE-FORMAT.md). `casualnote-sync.json` contains no key, and creating it needs an explicit user action: an empty root is never initialised automatically (see `RootMissing`).

### Sealed objects

Every object except `casualnote-sync.json` and key records has this 128-byte header, followed by ciphertext and a 16-byte tag.

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 4 | magic | `CNS1` |
| 4 | 2 | `protocol` | Sync protocol version of this object, little-endian |
| 6 | 1 | `kind` | 1 segment, 2 ack, 3 registry, 4 checkpoint manifest, 5 checkpoint part, 6 blob manifest, 7 blob chunk |
| 7 | 1 | `flags` | Reserved; must be 0 in version 1 |
| 8 | 16 | `key_id` | Which workspace key opens this |
| 24 | 16 | `device` | Writer |
| 40 | 16 | `notebook` | All zero for workspace-level records |
| 56 | 8 | `seq` | Writer's sequence within this kind and notebook, or part or chunk index |
| 64 | 32 | `salt` | Random, per object |
| 96 | 24 | `nonce` | Random, per object |
| 120 | 8 | `body_len` | Ciphertext length. Must equal the file length minus the header, or the object is refused before any work |

The whole header is the AEAD's additional data, so moving a file to another name, device or notebook gains nothing: the reader trusts the header, and a tampered header fails to open.

The plaintext body is canonical CBOR (RFC 8949 §4.2), padded with zeros to the next 4 KiB multiple (proposal). Padding reduces what keystroke-sized segments reveal and removes nothing else; see Metadata.

**Segment body.**

```
SegmentBody {
  prev_hash:   [u8; 32],     // SHA-256 of this device's previous sealed segment; zeros for seq 1
  sealed_at:   u64,          // writer's clock, advisory, used for display only
  merge_codec: u16,          // opaque, from -merge; identifies the CRDT encoding
  docs: [ { doc_id, update: bytes, end_version: bytes } ],  // tree and page documents
  blobs_added: [ blob_tag ],
  review_seen: [ marker_id ],
}
```

A sync segment wraps one or more closed local log segments ([23](23-STORAGE-FORMAT.md)). `-sync` treats `update` and `end_version` as opaque bytes from `-merge`. `prev_hash` gives each device a hash chain, which is how a fork or a tampered history is noticed.

**Ack body.**

```
AckBody {
  sealed_at: u64,
  protocol_max: u16,
  acked: { device: { seq: u64, chain_hash: [u8; 32] } },  // contiguous, durably imported
  extra: { device: [ (seq_from, seq_to) ] },              // imported past a gap
  checkpoints_verified: [ checkpoint_id ],
  open_reviews: u32,
}
```

An entry means "I hold every segment of this device from 1 to `seq`, or a checkpoint covering them, on disk after fsync".

**Registry records.**

| Record | Fields | Effect |
| --- | --- | --- |
| `Announce` | device, display name, platform label, `protocol_min`, `protocol_max`, subscribed notebooks | Registers the device |
| `Subscribe`, `Unsubscribe` | notebook | Changes whose acks count for that notebook |
| `Remove` | target device, removed by, reason, the remover's view of the target's ack | Explicit removal. Monotonic: a removed id never returns |
| `Retire` | old device id, new device id | A forked or restored device moved to a new id |
| `ProtocolFloor` | notebook-independent minimum protocol | See versioning |

The registry is the union of all records seen. It is a grow-only set with "removed beats active", so no ordering or clock is needed and two devices can write at once. **Each device also keeps its own persistent copy of every device it has ever seen,** and a device missing from a listing is never treated as unregistered.

### Lifecycle of an object

**Outbound segment.**

```
Pending ── seal, persist sealed bytes in outbox ──▶ Sealed
Sealed ── put-if-absent ──▶ Uploading
Uploading ── Created ──▶ Written
Uploading ── AlreadyExists, same hash ──▶ Written
Uploading ── AlreadyExists, different hash ──▶ Fork (CN-SYN-0006)
Uploading ── Transient / Unreachable ──▶ Sealed, backoff
Uploading ── QuotaFull / Forbidden / Unauthorised ──▶ Blocked, needs user
Written ── another device acks ──▶ Seen(some)
Seen(some) ── all registered devices ack ──▶ Seen(all)
Seen(all) ── covered by verified checkpoint ──▶ Trimmable
Trimmable ── delete ──▶ Deleted (remote only)
```

The sealed bytes are persisted before the first put, and a retry always sends the same bytes. A segment leaves the **local outbox only at `Seen(all)` and checkpoint covered**, which is what lets a device republish after storage has lost something.

**Inbound object.**

```
Listed ── name and size within limits ──▶ Fetching
Fetching ── Got ──▶ Fetched
Fetched ── open succeeds ──▶ Opened
Fetched ── open fails ──▶ Incomplete ── retries exhausted ──▶ Corrupt (kept, visible)
Opened ── protocol or codec too new ──▶ TooNew (kept, not acked)
Opened ── key unknown ──▶ NeedsKey (kept, not acked)
Opened ── guarded import: Applied ──▶ Imported
Opened ── guarded import: Pending ──▶ Waiting (deps missing, held in inbox)
Opened ── guarded import: Rejected ──▶ Quarantined (CN-SYN-0009, visible)
Imported ── fsync ──▶ Durable ── included in next ack ──▶ Acked
```

**Contract needed from `-merge`** (proposed row `FND-20`, for [24](24-OPERATIONS-AND-MERGE.md)):

- Import is idempotent and commutative, and is told apart as `Applied`, `Pending { missing }`, `BeforeTrimPoint`, or `Rejected`. The idempotence and pending behaviour of Loro is unverified here and is for `SPK-01`.
- **A peer id is never reused after a restore or clone.** Two replicas writing under one CRDT peer id with the same counters could diverge in a way the CRDT cannot repair. Allocating a fresh peer id on each open, or on any detected restore, is the likely answer, and its cost on long histories is unmeasured.

### Exchange algorithm

One round, triggered by a closed local segment (debounced, proposed 5 s), a timer, focus, a folder change notification, or the "Sync now" button. Polling backs off from 30 s to 15 min (proposals), and each adapter may set its own floor to control request cost.

```
round(root):
  1. PREFLIGHT   get casualnote-sync.json
                 missing: RootMissing, stop, change nothing locally
                 sync_id differs: WrongRoot, stop
  2. PUSH        for each Sealed outbox object in seq order: put-if-absent
                 for each blob with pending chunks: continue upload, after segments
  3. DISCOVER    list d/, union with persisted known devices
                 for each device, notebook, kind: list after persisted cursor
                 candidates that fail the name grammar go to candidate handling
  4. KEYS        new key records: try with held keys; else NeedsKey
  5. REGISTRY    new registry records: apply; if this device is removed, go to Removed flow
  6. FETCH       fetch new objects, bounded concurrency (proposal 4), size checked first
  7. OPEN        open each; verify hash chain; route by kind
  8. IMPORT      segments through guarded import; retry Waiting; fsync
  9. OVERLAP     run overlap detector on the imported range; create review markers
 10. ACK         if acked vector changed and 60 s since last (proposal): write new ack
 11. CHECKPOINT  if thresholds passed and none is in flight: write checkpoint
 12. TRIM        apply trimming rule to this device's own objects
 13. STATUS      recompute SyncStatus; emit change event
```

Each step records its own outcome. A failed step does not stop later steps that do not depend on it: failed uploads do not stop imports, and a corrupt object from one device does not stop another device's.

**Gaps and reordering.** `seq` 9 arriving before `seq` 8 is imported anyway if `-merge` can take it, and is otherwise `Waiting`. The ack's contiguous `seq` stays at 7, with 9 in `extra`, so the trim floor cannot pass a gap. After a gap has been open for 24 hours (proposal) the panel shows it under that device, and other devices' acks show whether they received the missing segment.

**Self-check for dropped writes.** Storage may report success and lose the object. Each round, a device compares its own listing with its outbox, and also reads other devices' acks of its own segments. A `Written` segment that is absent from the listing, or that stays missing from acks while later segments appear in `extra`, goes back to `Sealed` and is put again with the same bytes. A lagging listing makes this a harmless repeat.

**The hash chain.** When `seq` n+1 arrives, its `prev_hash` must equal the hash of `seq` n if that is held. A mismatch is a fork. A device also compares other devices' acks of its own segments against its own chain.

**Crash points.** All are safe, and `SPK-04` injects each.

| Crash between | On restart |
| --- | --- |
| Local fsync and seal | Log has unsealed segments; seal them |
| Seal and put | Outbox holds sealed bytes; put them, same bytes |
| Put and recording `Written` | Put again, get `AlreadyExists`, compare hash, `Written` |
| Fetch and import | Inbox entry is reprocessed; import is idempotent |
| Import and fsync | Lost locally, cursor did not advance, fetched again |
| Fsync and ack | Ack is written later. Others wait a little longer |
| Ack and delete of older ack | Two acks exist, the higher wins |
| Checkpoint part and manifest | No manifest means no checkpoint; owner removes stale parts |
| Remove record and cleanup | Cleanup resumes from the record |

### Forks: cloned and restored devices

A device copied by a disk image, a VM snapshot or a backup restore believes it is still the original.

| Signal | Meaning |
| --- | --- |
| This device lists its own prefix and finds a `seq` it has no record of | It was restored from a backup, or a clone is running |
| `AlreadyExists` with different bytes | Same |
| Another device's ack names this device with a different `chain_hash` | Same, and the only signal on storage without create-once |
| A reader holds two valid objects for one device and `seq` | Same |

Response, in order:

1. Stop writing under this id. Import the unknown segments, since they are this person's edits.
2. Mint a new device id and write `Retire { old, new }`.
3. Republish every outbox segment not yet seen by all, under the new id, as new segments. The CRDT import makes the repeat harmless.
4. Show the event in the panel (`CN-SYN-0006`). The old id stops holding back trimming only after the user confirms that the original device is gone, because the two may both be alive.

A reader holding two valid objects keeps and imports both, so neither is lost.

### Device registry and sync state

The panel reads `SyncStatus`, and nothing else. `CN-SYN-*` codes carry the message key ([20](20-ERROR-CODE-REGISTRY.md), [50](50-INTERNATIONALIZATION.md)). The surface is in [47](47-UX-AND-FEATURE-MAP.md).

```rust
pub struct SyncStatus {
    pub root: RootState,            // Connected, Unreachable, RootMissing, WrongRoot,
                                    // StorageRegressed, AuthFailed, QuotaFull, Locked, NeedsKey
    pub last_round: Option<RoundSummary>,
    pub this_device: DeviceId,
    pub devices: Vec<DeviceRow>,
    pub reviews_open: u32,
    pub restored_copies: Vec<RestoredCopy>,
    pub foreign_files: u32,
    pub storage_held_back: Option<HeldBack>,   // bytes, and the device holding the floor
}

pub struct DeviceRow {
    pub id: DeviceId,
    pub name: String,
    pub platform: String,
    pub state: DeviceState,
    pub last_seen: Option<LastSeen>,           // { observed_at, claimed_at }
    pub unsynced: Unsynced,                    // { segments, bytes, oldest_edit, blobs, blob_bytes }
    pub failed: Vec<Failure>,                  // { code, object_class, first_at, last_at, attempts, next_retry, action }
    pub restorable: Restorable,                // { outbox_segments, checkpoints, restored_copies }
}
```

| Field | For this device | For another device |
| --- | --- | --- |
| `unsynced` | Sealed or pending and not yet `Written`, plus `Written` and not yet acked, shown as two numbers | Segments this device has written that the other has not acked: "not yet seen by Tablet" |
| `failed` | Every open failure on outbound or local import | Failures while reading that device's objects: corrupt, too new, gap, fork |
| `restorable` | Outbox and checkpoints held here, restored copies made | What this device still holds that could rebuild that device: segments retained for it, newest verified checkpoint |
| `last_seen` | Time of the last completed round | `observed_at`: when this device first saw that device's newest record, by the local clock. `claimed_at`: that device's own clock, shown only in detail |

**Clocks are display only.** Nothing in merge, trim or registry logic reads a clock.

**Device states.**

```
         announce              no record for 30 days (display only)
  (new) ─────────▶ Active ─────────────────────────▶ Stale
                     ▲  │                               │
                     └──┼───────── new record ──────────┘
                        │ user removes (from Active or Stale)
                        ▼
                    Removing ── cleanup complete ──▶ Removed
                                                        │ that device returns
                                                        ▼
                                                 Restored copy flow
```

`Stale` only changes how the row looks. **No time limit ever removes a device.**

**Fault to panel table.** This table is the PR-2 gate: `SYN-03` asserts that each row produces the state shown.

| Fault | State in `SyncStatus` | Code | User action |
| --- | --- | --- | --- |
| Delay | `unsynced` counts rise, `last_seen` stays old | None | None |
| Dropped segment, gap | `failed` on that device: gap | `CN-SYN-0005` | None at first; owner republishes when it sees the gap in acks |
| Duplicate | Counted in round summary | None | None |
| Reorder | `Waiting` count in round summary | None | None |
| Bit flip or truncation | `Incomplete`, then `Corrupt` | `CN-SYN-0004` | Owner republishes; else shown |
| Tampered header or body | Same, and never imported | `CN-SYN-0004` | Shown |
| Conflict copy, identical | Counted | None | None |
| Conflict copy, differing | Fork | `CN-SYN-0006` | Confirm which device is alive |
| Hostile CRDT update | `Quarantined` | `CN-SYN-0009` | Export for a report; never imported |
| Storage unreachable | `root: Unreachable` | `CN-SYN-0001` | Check connection or mount |
| Root file missing | `root: RootMissing` | `CN-SYN-0003` | Reconnect, or explicitly reseed |
| Objects vanished, cursor ahead of listing | `root: StorageRegressed` | `CN-SYN-0010` | Explicitly reseed |
| Credentials refused | `root: AuthFailed` | `CN-SYN-0011` | Re-enter |
| Storage full | `root: QuotaFull` | `CN-SYN-0012` | Free space or trim |
| Delete refused (for example Object Lock) | `storage_held_back`, trim paused | `CN-SYN-0013` | Informational |
| Object too new | Device `failed`, not acked | `CN-SYN-0007` | Update this app |
| Key unavailable | `root: NeedsKey` | `CN-CRY-0005` | Enter passphrase |
| Over a limit | Device `failed` | `CN-SYN-0008` | Shown; not imported |
| Device never acks | `storage_held_back` names it | `CN-SYN-0014` | Wait, or remove the device |
| Device beyond limit | `restored_copies` gains an entry | `CN-SYN-0015` | Review the copy |
| Peer stops responding, storage fine | Row stale, `unsynced` for that device rising | None | Check that device |

### Merge review

The detector and marker model belong to [24](24-OPERATIONS-AND-MERGE.md) (`MRG-03`). The surface belongs to [47](47-UX-AND-FEATURE-MAP.md) (`SYN-04`). Sync owns the flow.

1. **The merge always happens first.** The merged block is the current content. Review never holds text back, and merged text is never hidden (PR-1).
2. Step 9 of a round finds blocks changed by two devices without either having seen the other, and creates a marker: `{ marker_id, page, block, versions: [{ device, version_ref, claimed_at }], merged_version }`.
3. The block gets a quiet marker, the page and panel show a count, and PR-2 applies: an open review is part of the sync state.
4. Opening a marker shows each device's version read-only, from history, with the merged result.
5. **Keep this one** produces an ordinary op batch through `-ops`, one undo step ([24](24-OPERATIONS-AND-MERGE.md)). **Keep merged** changes no content.
6. The resolution is recorded in the page document, so it syncs and clears on other devices.

```
Open ──▶ Viewed ──▶ Resolved(KeepVersion | KeepMerged)
  │        │
  └────────┴──▶ Superseded (a later overlap on the same block, new marker links back)
```

Markers never expire. **An open marker pins the versions it names,** so history trimming cannot pass them. If a deliberate limit ever forces that, the versions are first copied as plain text into the marker. Real-time co-editing does not create markers; a device returning from offline does, in both this design and [57](57-SERVER-DESIGN.md). How often this fires is unmeasured and is part of `SPK-01` (RISK-03).

### Checkpoints and trimming

There are two kinds of trimming, and they carry different risks.

| | T1: storage trim | T2: history trim |
| --- | --- | --- |
| Removes | Segments on storage that a checkpoint covers | CRDT history older than a chosen point, using shallow snapshots |
| Lagging device | Imports the checkpoint and carries on, because a full-history checkpoint keeps everything | **Cannot merge** if its frontier is older than the trim point |
| Default | On | **Off,** until `SPK-01` shows it is needed |

[Loro's documentation](https://github.com/loro-dev/loro-docs/blob/main/pages/docs/concepts/shallow_snapshots.mdx) says peers "can only sync if they have versions after the shallow snapshot point", which is why T2 is treated as dangerous.

**Checkpoints.** Any device may write one under its own prefix when uncovered segments exceed 64 MiB or 2,000 segments (proposals). It waits a random 0 to 10 minutes first and skips if another appears. Two at once only waste storage. A checkpoint holds the state of the notebook's documents, the sync vector it covers (`{device: seq, chain_hash}`), the registry, and for T2 the trim point. It uses the blob manifest shape: parts first, manifest last. A checkpoint counts as **verified** when another device lists it in `checkpoints_verified`, or, with a single registered device, when that device has read it back, opened it, imported it into a scratch replica and compared the result to its own state.

**The trimming rule.** For notebook N and device X:

```
floor(N, X) = min over registered subscribers D of N: acked[D][X].seq
              (a device counts as having acked everything it wrote itself)

Device X may delete its own segment s only if all hold:
  s.seq <= floor(N, X)
  a verified checkpoint C exists with C.vector[X] >= s.seq, and this device has read C back
  every registered subscriber's protocol_max can read C
```

T2 may move its point only up to the version given by the floor, and never past a version pinned by an open review.

Three properties make this safe under stale information:

- **Stale acks are low,** so a stale view only delays trimming.
- **A registered device with no ack has a floor of zero.** It holds trimming back until it acks.
- **A new device that nobody has seen yet needs nothing old.** It starts from a checkpoint.

**A device deletes only under its own prefix.** The one exception is explicit removal, below. Old acks: keep the newest two. Old checkpoints: the owner deletes one when a newer verified checkpoint covers it and the floor has passed it.

**Holding back.** A silent device holds trimming back indefinitely, by design. The panel names it, the bytes held and the choice: wait, or remove it. The product never removes it for the user, and no timer does.

### Removing a device

An explicit act, with a confirmation that says what follows. Steps:

1. **Show the cost.** Segments from that device not yet held here are listed, as are edits only it may hold. Its edits never written to storage cannot be reached from here, and the dialog says so.
2. Import everything of the target's that is available.
3. Write `Remove { target, … }` under the current key, before any rotation, so the removed device can still read why it was removed.
4. Offer key rotation (below). If the device was lost or stolen the offer is the default.
5. Write a checkpoint that covers the target's segments, and upload under this device's prefix any blob still referenced that only the target held.
6. After that checkpoint is verified, delete the target's objects, the one case where a device deletes under another's prefix.
7. Tell the user to change the storage credentials. **We do not control them,** so a removed device that keeps them can still list and delete files, and rotation does not change that.

Two devices removing each other both take effect. Both keep their local data, and each can rejoin under a new id through the flow below.

### Device beyond any limit: restored copy

A device is beyond the limit when it cannot rejoin normally. Causes:

| Cause | Detected by |
| --- | --- |
| It was removed and has returned | A `Remove` record names it, readable after unlock |
| Its state is older than the T2 trim point | Guarded import returns `BeforeTrimPoint` |
| Storage lost what it needs and no device can supply it | Gap that no device can fill, confirmed by the user |
| Its CRDT or protocol version is no longer supported by the others | Protocol check |

Flow:

```
1. Freeze     stop publishing this notebook. Keep the local replica read-only
2. Bootstrap  build a fresh replica from the newest verified checkpoint
3. Diff       -merge lists pages changed locally since the last point known shared
4. Project    each changed page to model state, as -export does, with no CRDT state
5. Create     through -ops, as one batch, new pages under
              "Restored copies / <device name> / <date>", new ids, each linked to its original
6. Publish    under a new device id; blobs referenced by the copies are re-uploaded
7. Show       panel entry, CN-SYN-0015, with a count of pages. It stays until dismissed
8. Keep       the frozen replica stays until the person deletes it explicitly
```

A restored copy is never merged automatically, because that is exactly what could not be done safely. The page view offers a comparison with the original. The diff in step 3 can be coarse: a page is included whole if the precise change cannot be computed. A false copy costs a glance, and a missed one costs an edit.

### Storage emptied or regressed

| State | Trigger | Behaviour |
| --- | --- | --- |
| `RootMissing` | `casualnote-sync.json` absent | Could be an unmounted drive, a signed-out client or real loss. **Nothing changes locally and nothing is written.** Actions: retry, choose another location, or "Rebuild storage from this device" |
| `StorageRegressed` | A device's listed `seq` is below a persisted cursor, or a known checkpoint has vanished | Same. Reads continue. New writes continue under new `seq`, because they harm nothing |
| Reseed | User confirms | Write root descriptor if absent. Republish outbox, under the same names with the same bytes. Write a fresh checkpoint. Anything no surviving device holds is listed as unavailable |

### Blobs

Attachments and stroke data ([23](23-STORAGE-FORMAT.md), `FIL-01`, ADR-008). Text is never held up for a blob: a segment can name a blob before it has arrived, and the page shows a placeholder.

| Aspect | Design |
| --- | --- |
| Tag | `tag = HMAC-SHA-256(BK, plaintext SHA-256)`, truncated to 128 bits. Storage cannot compare it with a public hash |
| Deduplication | Within a notebook only. Identical attachments in one notebook are visible to storage as one tag; across notebooks they are not linkable |
| Where | Under the uploading device's own prefix. If a blob's manifest is already visible from another device, it is not uploaded again. Two simultaneous uploads produce two copies, with one tag, which is harmless |
| Chunks | Fixed 4 MiB (proposal). Each is one AEAD message whose header carries the index |
| Manifest | Written last. Holds plaintext hash, length, media type, chunk count, per-chunk hash, and is what marks a blob complete. Tampering with chunk order or dropping the final chunk fails |
| Resume | List the blob's prefix, upload the missing chunks. Fetch skips chunks already held |
| Verify | After the final chunk, plaintext hash must match the content hash in [23](23-STORAGE-FORMAT.md) or it is `Corrupt` |
| Download policy | Desktop: all, by default. Web: on open, within browser quota, with a visible count. A device may ack text without holding a blob |
| Deletion | The owner deletes a blob only when no retained version references it, the floor has passed the segment that removed the last reference, and trash has expired (FR-01). Before removal, referenced blobs are re-homed |

"Any size" in FR-08 is subject to the manifest limit below and the storage's own limits.

### Cryptography

**This section is a design that has had no outside review.** No public encryption claim may be made until `CRY-02` is done and its findings are dispositioned. That covers the website, the README, the app and release notes. The product describes this as "encrypted before upload, not yet independently reviewed" until then.

**Primitives** (ADR-021 cites the design; a separate ADR is proposed once `CRY-01` starts).

| Purpose | Algorithm | Candidate crate |
| --- | --- | --- |
| Passphrase stretching | Argon2id | `argon2` |
| Encryption | XChaCha20-Poly1305 | `chacha20poly1305` |
| Key derivation | HKDF-SHA-256 | `hkdf`, `sha2` |
| Tags | HMAC-SHA-256 | `hmac` |
| Memory clearing | Best effort | `zeroize` |

**The licences of these RustCrypto crates and their dependencies have not been audited by this project.** They must pass `cargo deny` under ADR-023 (`GOV-05`) before `-crypto` joins the workspace, and any independent review of them must be confirmed, not assumed. Our `forbid(unsafe_code)` does not cover them ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)).

**Key hierarchy.**

```
passphrase (NFC)        recovery key (256 random bits)
     │ Argon2id              │ HKDF
     ▼                       ▼
    KEK-P                  KEK-R
     └────── each wraps ─────┘
                 ▼
     WK: workspace key, 256 random bits, per epoch
                 │ HKDF, info = "casualnote/v1/notebook" ‖ notebook id
                 ▼
     NK: notebook key            also: WOK, for workspace-level records
                 │ HKDF
       ┌─────────┴───────────┐
       ▼                     ▼
   OK: per-object key     BK: blob tag key, info "casualnote/v1/blob-tag"
   salt = header.salt
   info = "casualnote/v1/object" ‖ kind ‖ device ‖ notebook ‖ seq
```

Each object has its own key from its random salt, so a random nonce under that key carries no realistic reuse risk. Nonces stay random rather than zero, so a mistake in salt handling does not immediately become a catastrophic reuse.

`key_id` is the first 16 bytes of `HKDF(WK, "casualnote/v1/key-id")`, so it identifies a key without revealing it.

**Argon2id parameters.** Proposed default: 128 MiB, t = 3, p = 1. RFC 9106 §4's recommendations are t = 1, p = 4, 2 GiB, and, for low memory, t = 3, p = 4, 64 MiB. p = 1 is proposed because `wasm32` may run single-threaded, and the parameter changes the result. The timing on phones and in wasm is unmeasured, and `CRY-01` must time it on the lowest supported device before the defaults are fixed.

**Parameter bounds protect against hostile storage.** Parameters come from storage, so the client checks them before deriving anything: below a 64 MiB or t = 2 floor is refused (`CN-CRY-0003`), which stops a downgrade, and above a 1 GiB or t = 10 ceiling is refused, which stops a memory-exhaustion attack.

**Passphrases in any language.** The passphrase is normalised to NFC and encoded as UTF-8 before use, in the manner of the PRECIS OpaqueString profile (RFC 8265), with no trimming or case change. The same phrase typed on a Mac and on Windows, or through an input method, has to derive the same key. Test fixtures cover each launch script (ADR-018, [50](50-INTERNATIONALIZATION.md)). Length minimum: 12 Unicode scalar values (proposal). A strength meter must not assume Latin text.

**Recovery key.** 256 random bits shown once as 52 Crockford base32 characters in groups of four, with a 2-character check. It is ASCII only by design, so no word list needs translating. Input is folded before checking: lower case, `I`, `L` and `O` as Crockford defines, and full-width forms and non-Latin digits that an input method or keyboard may produce become their ASCII equivalents. Setup requires retyping it. The UI says plainly: with neither the passphrase nor this key the notes in storage cannot be recovered by anyone (SEC-S06). No backdoor exists.

**Key records,** in `d/<device>/k/`, are not sealed with the header above, since they are what a new device opens first.

```
KeyRecord {
  epoch: u64, wk_id: [u8; 16], created_by: DeviceId,
  wraps: [
    { kind: passphrase, argon2: { m, t, p, salt }, nonce, ciphertext },
    { kind: recovery, nonce, ciphertext },
  ],
  prev: [ { wk_id, nonce, ciphertext } ],   // earlier WK, wrapped under this WK
}
```

Each wrap's additional data binds `epoch`, `wk_id`, `kind` and, for a passphrase, the Argon2 parameters. Current key is the highest `(epoch, wk_id)` seen, so two devices that rotate at once settle on one by rule, and both remain readable because objects name their `key_id`.

Bootstrap on a new device: list key records, take up to 8 newest distinct salts (limit, to bound Argon2 work), try the passphrase, follow `prev` for older keys. A device remembers the highest epoch it has seen and refuses to write under anything lower (`CN-CRY-0006`).

**Unlock state.**

```
Locked ── passphrase or recovery key ──▶ Unlocking ── ok ──▶ Unlocked
Unlocking ── wrong ──▶ Locked (CN-CRY-0001; no lockout, since the attacker holds the file anyway)
Unlocked ── unknown key_id seen ──▶ NeedsKey ── passphrase ──▶ Unlocked
Unlocked ── sign out or policy ──▶ Locked
```

The host stores the unlocked key: the OS keychain on desktop if the person chooses to stay unlocked ([44](44-DESKTOP-SHELL.md)), and memory only by default on the web, since browser storage protects it poorly. A passphrase and KEK are never stored. Wasm memory cannot be locked and `zeroize` is best effort, and we say so.

**What is visible to storage and to anyone who can read it.**

| Visible | Detail |
| --- | --- |
| Sizes | Object sizes, rounded up to 4 KiB for segments. Blob size to within a chunk |
| Counts | Number of devices, notebooks, segments, blobs and chunks |
| Timing | When each device writes, from file times and access logs: when someone works, and how much |
| Structure | Device ids and notebook ids, which are random but stable, so activity can be linked over time |
| Equality | Repeated attachments within one notebook |
| Software | That this is CasualNote, and the protocol version |

Hidden: titles, text, page ids and structure, tags, languages, media types, file names. This is SEC-S04, an accepted risk. The product and docs say so in the setup flow and the panel.

**Limits of this design, stated plainly.**

| Not provided | Consequence |
| --- | --- |
| Per-device signatures | Every device holds one key, so any key holder can write as any device. This suits one person's devices and is part of what team sharing would have to add |
| Forward secrecy | A stolen WK opens every object of that epoch and, through `prev`, earlier ones |
| Protection from a compromised device | Malware on an unlocked device sees everything |
| Proof that storage is complete | Storage can withhold. Sequence gaps, hash chains and other devices' acks reveal much of it, and a stale row shows the rest, but a storage that hides everything looks the same as a device that went quiet (SEC-S03) |
| Hiding an epoch from a new device | Storage that hides the newest key record can steer a brand-new device onto an old key. It will then meet `key_id`s it cannot open, and shows `NeedsKey` |

### Rotation

| Change | What happens | Protects against | Does not protect against |
| --- | --- | --- | --- |
| **Change passphrase** | New key record with new wrap of the same WK. Needs the current key. The owner of each older record deletes it on seeing the new one, and until then the old passphrase still opens it | A weak old passphrase, going forward | Anyone who already has the old record and passphrase. Storage may keep old versions, so **a suspected leak needs rotation** |
| **New recovery key** | New record, same WK | A lost or exposed recovery key, with the same caveat | Same |
| **Rotate workspace key** | Generate WK at epoch + 1, wrap the old one in `prev`, write the record. New objects use it. Old objects stay as they are until checkpoints replace them | A removed device or stolen key reading **future** data | Reading what it already holds, or old objects still on storage |

Rotating the key needs a human: **no device stores the passphrase or the recovery key,** so the person types the passphrase, and either types the existing recovery key or receives a new one. A removed device therefore cannot follow a rotation unless it learns the passphrase. If that passphrase may be known to whoever holds the device, it has to change at the same time, and the dialog says so.

A device that was offline during a rotation meets an unknown `key_id`, shows `NeedsKey`, and continues after the passphrase. If it has been removed, it goes through the restored copy flow. Re-encryption of old data happens over time through checkpoints and T1; "rotation finished" is reported only when no object under the old epoch remains, and that is shown as a state.

### Why team key-sharing is deferred

Sharing a key among people is a different problem from sharing among one person's devices:

- **Distribution** needs per-person public keys and a way to verify them, or anyone running the storage could substitute their own.
- **Removal** needs a rotation that reaches every remaining member without a shared passphrase, in storage with no order and no server, while members are offline. Anything like MLS (RFC 9420) needs a delivery service with ordering, which this storage model rejects on purpose.
- **Permissions** do not exist without signatures per person, and even then a reader who holds the key holds the content ([57](57-SERVER-DESIGN.md) explains).
- **A shared passphrase** gives no individual removal and no accountability.

The honest first release is one person's devices. Any later design needs its own document, ADR and outside review. Encrypted packages (`PKG-07`) are also separate.

### Sync protocol versioning

The sync protocol version is independent of the others listed in [02](02-ARCHITECTURE.md). It starts at 1.

| Rule | Detail |
| --- | --- |
| Reader | Accepts `protocol_min..=protocol_max`, compiled in |
| Writer | Writes the highest version that every registered device can read, from their announces |
| Too new | Bytes are kept and not acked (`CN-SYN-0007`). The floor stays put, which is the safe direction. Panel says "Update this app to read edits from Laptop" |
| Unknown `kind`, in a known protocol | Kept, ignored, counted. Never deleted by a non-owner |
| Unknown CBOR field | Ignored; a field that must be understood needs a protocol bump |
| Held back | An old offline device keeps the writer on an old version, visibly. The person can update or remove it |
| Raising the floor | `ProtocolFloor` is an explicit act after every registered device supports it |
| CRDT encoding | `merge_codec` is separate, so a library upgrade uses the same refusal path. `-sync` never names the library |
| Key derivation labels | The `v1` in labels changes only with a new `CNS` magic |

### Limits

Proposals for [21](21-PARSER-LIMITS.md). Each is checked before allocation and before decryption where possible, and exceeding one gives `CN-SYN-0008`.

| Limit | Value |
| --- | --- |
| Sealed segment | 64 MiB, so one maximum frame from [23](23-STORAGE-FORMAT.md) always fits with its seal |
| Decompressed or opened body per segment | 256 MiB, the update limit in [24](24-OPERATIONS-AND-MERGE.md) |
| Documents per segment | 4,096 |
| Registry or ack record | 1 MiB |
| Devices per workspace | 64 |
| Checkpoint part | 16 MiB |
| Checkpoint parts | 65,536 |
| Blob chunk | 4 MiB plus overhead |
| Blob manifest entries | 1,048,576 (4 TiB at 4 MiB per chunk) |
| Objects per listing page | 1,000 |
| Foreign files examined per round | 256 |
| Key records tried | 8 |
| Argon2 memory | 64 MiB to 1 GiB |
| CBOR nesting depth | 32 |

Each CBOR body and the header is a parser with a fuzz target (`GOV-07`, QR-4). CRDT updates go through guarded import with panic isolation (SEC-S01, SEC-S07, `MRG-02`).

### SPK-04: fault-injected runs on real storage

**Question.** Does segment sync survive real storage? This decides ADR-021.

**Passes when.** 10,000 fault-injected runs across a folder, WebDAV and S3: no acknowledged edit lost, every failure visible.

**Fails, then.** Narrow the supported storage list. A backend that fails is dropped from the list, and the others stand on their own results. If the failure is in the design rather than a backend, ADR-021 reopens.

**A run** is one seeded scenario: three simulated devices, a random edit stream including offline periods, restarts and crash points, one schedule of faults, then a quiet period with faults lifted, then the oracle. Everything is reproducible from the seed, and a failing seed becomes a permanent regression test.

| Backend | Proposed share | How |
| --- | --- | --- |
| Plain local folder | 3,000 | Real file system, faults injected through a wrapper |
| Cloud client behaviour on a folder | 1,000 | A simulator of the conflict-copy, partial-file, placeholder, delay and resurrection behaviour in this document |
| WebDAV | 3,000 | At least two server products through a fault proxy. Products chosen under ADR-023 for test use |
| S3 | 3,000 | At least AWS or a compatible server, through a fault proxy |

**Real cloud clients can't be run 10,000 times.** A separate manual campaign runs at least Dropbox, OneDrive, iCloud Drive and Google Drive, on real accounts and two computers, with the owner's platforms and a borrowed or rented Windows machine (`DEC-07`). Its results decide which clients are named. A client not tested is not named as supported, and the simulator's assumptions are corrected from what the campaign sees.

**Fault menu.**

| Class | Faults |
| --- | --- |
| Timing | Delay, listing lag, long read block |
| Loss | Drop write after reporting success, drop read, delete a file, lose a suffix of a device's files, empty the root, roll the root back |
| Multiplicity | Duplicate delivery, conflict copy, resurrected file |
| Order | Reorder listing, reorder delivery |
| Corruption | Bit flip, truncation, zero-length, placeholder, header edit, moved file, swapped files |
| Refusal | 401, 403, 404, 409, 412, 429, 500, 503, reset, timeout, quota full, delete refused |
| Device | Crash at each point in the crash table, restore from backup, clone, long offline, removal and return |
| Keys | Rotation during a run, offline during rotation |

**Oracle.** Failing any is a failed run.

| ID | Invariant |
| --- | --- |
| I1 | Every edit shown as saved on any device is present on every registered device after the quiet period, or in a visible restored copy, unless storage was destroyed and no device holds it |
| I2 | All registered devices converge to the same projected state |
| I3 | Every injected fault with lasting effect appears in `SyncStatus` with the expected state, from the table above |
| I4 | No panic, hang or unbounded memory |
| I5 | Nothing was deleted from storage in breach of the trimming rule |
| I6 | No tampered or foreign object was imported |
| I7 | Panel says "seen by" a device only if that device really acked |

I1 and I3 are the published pass condition, and the others are there to explain a failure. Destructive faults with no surviving copy cannot pass I1 by any design, so those runs instead require the loss to be shown.

Until `MRG-01` lands, the spike can use a stand-in document type with grow-only semantics, and must be re-run against `-merge` before ADR-021 is accepted. The nightly `sync-faults` job ([15](15-CI-AND-RELEASE-GATES.md)) then runs a smaller selection against simulated storage on every night. No result exists yet.

## Failure modes and limits

| Failure or limit | Effect | Handling |
| --- | --- | --- |
| Device lost before its edits were written | Those edits exist nowhere else | Cannot be solved here. Panel shows unsynced counts while the device is alive, so the risk is visible |
| Web device loses browser storage | Same, and its registration holds back trimming | New id on return, stale row, user removes the old one ([23](23-STORAGE-FORMAT.md)) |
| Passphrase and recovery key both lost | Storage is unreadable. Any unlocked device can export or set new credentials | Plain warning at setup |
| Storage hides everything from one device | Looks like the others have gone quiet | Stale rows. Nothing is lost locally |
| Silent device held for years | Storage grows | Named in the panel. Only the user may remove it |
| Sync client paused without telling us | `Written` everywhere, nobody acks | The panel shows "not yet seen", never "synced" |
| Large first sync | Slow, on metered connections too | Progress, pause, resume. Costs are unmeasured |
| S3 or WebDAV request cost from polling | Money or rate limits | Backoff, cost floor per adapter. Real numbers are unmeasured |
| Backend list incomplete for hours | Delay only | Cursor is not advanced past a gap |
| Storage full | Writes stop. Local edits are still saved | `QuotaFull`. Text is uploaded before blobs |
| Two sync roots for one workspace | Not supported in version 1 | Setup refuses a second root |
| Two devices create a root at once in one location | Possible on a cloud folder, where create-once does not span computers | Joining never creates a root, so this needs two explicit setups. The lowest `sync_id` among valid descriptors is the root, and the other device shows `WrongRoot` with its data intact |
| Fresh peer ids on each open | Possible metadata growth | Left to `SPK-01` |
| Size and timing analysis | Reveals activity | Accepted (SEC-S04), stated |

### Error codes

Proposed for [20](20-ERROR-CODE-REGISTRY.md).

| Code | Meaning |
| --- | --- |
| `CN-SYN-0001` | Storage unreachable |
| `CN-SYN-0002` | Setup probe failed a round trip |
| `CN-SYN-0003` | Sync root missing, or belongs to another workspace |
| `CN-SYN-0004` | Object failed to open: corrupt, truncated or tampered |
| `CN-SYN-0005` | Sequence gap from a device |
| `CN-SYN-0006` | Device fork: clone or restored backup |
| `CN-SYN-0007` | Object or encoding is newer than this app |
| `CN-SYN-0008` | Object exceeds a limit |
| `CN-SYN-0009` | CRDT update rejected by guarded import, quarantined |
| `CN-SYN-0010` | Storage has gone backwards |
| `CN-SYN-0011` | Storage refused the credentials |
| `CN-SYN-0012` | Storage full |
| `CN-SYN-0013` | Storage refused a delete; trimming is paused |
| `CN-SYN-0014` | A device is holding back trimming |
| `CN-SYN-0015` | Restored copy created |
| `CN-SYN-0016` | This device was removed |
| `CN-CRY-0001` | Wrong passphrase or recovery key |
| `CN-CRY-0002` | Recovery key has a bad check or length |
| `CN-CRY-0003` | Key derivation parameters outside bounds |
| `CN-CRY-0004` | Key record is damaged |
| `CN-CRY-0005` | Object needs a key this device does not have |
| `CN-CRY-0006` | Key epoch is lower than one already seen |
| `CN-CRY-0007` | Passphrase below the minimum length |

## Alternatives considered

| Alternative | Why not |
| --- | --- |
| One shared log or database file on storage | Two writers on one file is the failure this design exists to prevent. Reports in the audit ([12](12-OPEN-SOURCE-AUDIT.md)) show what shared-folder sync can do |
| Locks on storage | Not available on every backend, and useless on cloud folders. A stale lock also blocks a person's work |
| A central manifest listing all segments | Needs a single writer or locks. Listing each device's own prefix avoids it |
| Trust file names | Conflict copies and moved files make names unreliable |
| Deterministic (convergent) encryption for blobs | Reveals equality with any known file. Keyed tags are limited to a notebook |
| Blobs in a shared namespace | Cleaner deduplication, but it breaks "two devices never write one file" |
| Auto-remove a device after N days | Silent loss by another name. Refused |
| Last-writer-wins on a conflict | Silently drops text |
| Peer-to-peer | Needs discovery and both devices online. Possible later as another transport |
| AES-GCM | Random 96-bit nonces need more care than XChaCha20's 192 bits, and hardware support varies, including in wasm |
| Async storage trait inside `-sync` | Pulls a runtime question into `crates/`. Effects keep it deterministic and testable |
| Per-device signatures now | Worth little while all devices hold one key and one person owns them. Comes with team sharing |

## Acceptance gates

| Gate | Evidence required |
| --- | --- |
| ADR-021 accepted | `SPK-04` report: 10,000 runs, seeds, per-backend results, supported list |
| No acknowledged edit lost | Oracle I1 over the suite, nightly on `sync-faults` |
| Every fault visible | Each row of the fault table has a test that fails if its state is removed (PR-2, `SYN-03`) |
| Tamper | Bit flip across every byte of each object kind is refused, with no state change (`CRY-01`) |
| Key test vectors | Pinned vectors for each derivation and seal, the same on native and wasm |
| Languages | Passphrase fixtures in each launch script, NFC and NFD forms open the same key |
| Limits | One hostile fixture per limit, one fuzz target per parser |
| Trimming | Generated histories: no segment deleted while a registered device needs it. Removed-then-returned, cloned and restored devices each lose nothing (`SYN-05`) |
| Restored copy | Device beyond the limit: every locally saved edit appears in a visible copy |
| Licences | RustCrypto crates and dependencies pass `cargo deny` and are recorded (`GOV-05`) |
| Secrets | Seeded passphrase, key, and storage credential absent from logs, export, sync and mirror (`check-secrets.py`) |
| Public claim | None before `CRY-02` findings are dispositioned |

## ADRs

- **ADR-021** (Proposed): this design, settled by `SPK-04`.
- **ADR-005, ADR-006:** CRDT per page. Sync assumes import is idempotent and commutative.
- **ADR-008:** stroke data travels as blobs.
- **ADR-010:** the op log is the source of truth, and the mirror never reaches storage.
- **ADR-015:** embeddings are never synced.
- **ADR-018:** passphrases and messages are multilingual.
- **ADR-023:** licence gate, which the RustCrypto crates have not yet been checked against.
- **New ADR proposed at `CRY-01`:** cryptographic primitives and parameters.

## Tracker ids

`SPK-04`, `SYN-01`, `SYN-02`, `SYN-03`, `SYN-04`, `SYN-05`, `CRY-01`, `CRY-02`, `MRG-02`, `MRG-03`, `FIL-01`, `GOV-05`, `GOV-07`, `DEC-07`, `PKG-07`. Threats: SEC-S01 to SEC-S07. Proposed rows: `FND-20` (merge contract: import status and peer id rule), `FND-21` (supported storage per host), `FND-22` (a home for native storage adapters shared by desktop and test harness).

Related: [22](22-DATA-MODEL.md), [23](23-STORAGE-FORMAT.md), [24](24-OPERATIONS-AND-MERGE.md), [36](36-EXPORT-FORMAT.md), [44](44-DESKTOP-SHELL.md), [47](47-UX-AND-FEATURE-MAP.md), [50](50-INTERNATIONALIZATION.md), [57](57-SERVER-DESIGN.md), [07](07-SECURITY-THREAT-MODEL.md), [15](15-CI-AND-RELEASE-GATES.md), [17](17-GLOSSARY.md), [20](20-ERROR-CODE-REGISTRY.md), [21](21-PARSER-LIMITS.md).
