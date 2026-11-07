# 57 — casual-note-server

**Status: design only, nothing built.** No server code exists. Every limit and load figure is a proposal, and none has been measured. ADR-022 is Proposed.

## Outcome

A team self-hosts one Apache-2.0 binary that gives them real-time co-editing, presence, sharing, comments and large attachments, on Postgres and object storage they run themselves. It works air-gapped (ENT-R04).

Each workspace chooses one of two modes when it is created:

| | **End-to-end encrypted (E2EE)** | **Managed** |
| --- | --- | --- |
| Server sees content | No, ciphertext only | Yes |
| Server stores | Sealed updates, checkpoints, blobs | CRDT updates, checkpoints, blobs |
| Search, preview | Client only | Server search and previews |
| Repair, admin recovery | Impossible | Possible |
| Retention, legal hold | Impossible (ENT-R08) | Yes |
| Compaction | Clients | Server |
| Permissions | Admission and publish right only | Per operation |
| Sharing with others | Not in the first release, except a labelled shared-passphrase group (below) | Yes |

The product stays complete with no server. This is an optional host ([02](02-ARCHITECTURE.md)).

## Design

### A host, not a layer

`server/casual-note-server` sits outside `crates/`, and nothing in `crates/` depends on it ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)). It owns transport, identity, permissions data, persistence and fan-out. It owns **no merge logic and no note semantics**. In managed mode it asks `-sdk`, which holds them. In E2EE mode the request path does not call content APIs of `-sdk` at all, and a gate enforces that.

| Concern | Owner |
| --- | --- |
| WebSocket and HTTP, sessions, rate limits | Server |
| Who may do what, stored as data | Server |
| What an update does (managed) | `-sdk`, through `-merge` and `-ops` |
| Guarded import, panics, size limits | `-merge`, called with isolation by the server |
| Search index (managed) | `-search`, run by the server off the request path |
| Keys (E2EE) | Clients only |
| Provider keys, relay (optional) | Server, under admin policy |

### What carries from OpenCalc

**OpenCalc's collaboration server cannot be copied.** It imports spreadsheet crates, and its core is a server-ordered OT session. CasualNote merges with a CRDT (ADR-005). Patterns carry, code does not. Only the shape below is reused, and anything copied gets a `PROVENANCE.md`.

| OpenCalc pattern (its docs 57, 59, 61, 72 and 77) | Here |
| --- | --- |
| WebSocket over `axum`, with `ClientMessage` and `ServerMessage` | Carried. SSE and gRPC are rejected for the same reasons |
| **Append before acknowledging** | Carried unchanged. A receipt that is sometimes false is silent loss |
| `Refused { reason: NotSaving }` instead of silence when the log cannot append | Carried |
| A client-generated resume key, mapped to a server identity scoped to the user | Carried, though simpler, because replaying a CRDT update is harmless |
| Conditional append, so a stale writer is rejected | Carried as a unique constraint in Postgres |
| `/healthz` and `/readyz` as different questions, and `--healthcheck`, `--readycheck` | Carried |
| Plaintext-exposure warnings, refused insecure combinations at startup | Carried |
| Deny-by-default access check: a new op class is refused until someone classifies it | Carried, and central |
| Identity shown to others comes from the server, never the client | Carried |
| Session override that can only reduce access, with a ceiling above it | Carried as page locks |
| JWKS rotation with `kid`-triggered and timed refresh | Carried for OIDC and later host-signed tokens |
| `min-replicas-to-write` thinking: turn silent loss into a visible refusal | Carried, as `synchronous_commit` guidance for Postgres |

**What deliberately differs.**

| OpenCalc | Here | Why |
| --- | --- | --- |
| Server is the order, transforms each op | Server only assigns a log cursor. Order has no meaning to the merge | CRDT |
| Document of record stays with the integrator | Server stores notebooks | Teams need shared storage ([02](02-ARCHITECTURE.md)) |
| Host decides access, token is the contract | Server stores grants | No integrator in the self-hosted case. A host-signed token path can return under [55](55-SDK-AND-EMBEDDING.md) |
| Leader per document, Redis leases, fencing | Not needed at first | Nothing depends on a single orderer. See Scaling |
| Resume needs careful duplicate suppression, or inserts double | Duplicate is a no-op at the CRDT, and also stopped by a unique key | CRDT import is idempotent, subject to `SPK-01` |

### Why per-operation permissions cannot be enforced in E2EE

Three independent reasons, each sufficient.

1. **The server cannot see the operation.** To refuse "this person may comment but not edit", something must read the change and classify it. The server holds ciphertext, and a sealed comment and a sealed page edit look the same.
2. **Clients cannot do it for the server without new cryptography and agreement.** Every key holder can create a valid sealed update. Having each client reject a bad update would need a signature per person, plus an authenticated, ordered membership log that all clients read identically. Otherwise one client accepts what another rejects, and the replicas diverge for good. Reject-on-read also leaves the bad data stored and relayed, which is not what "refused" means. None of this exists, and team key-sharing is deferred ([56](56-SYNC-DESIGN.md)).
3. **Read permission is key possession.** Anyone who can decrypt a page can read it. A workspace key opens every page, and the server withholding a channel is a courtesy rather than a boundary, since a client that ever received the ciphertext keeps it.

**What the server can still enforce in E2EE,** and only this:

| Control | Enforced by | What it means |
| --- | --- | --- |
| Admission | Sign-in and membership | Whether this account can connect to this workspace at all |
| Publish right | The connection's role | A read-only connection's `Update` frames are refused. It cannot stop someone with the key from writing through another account or to other storage |
| Size, rate, quota | Counting | Same as in managed mode |
| Removal | Stops relay | A removed member receives nothing new. **They keep their keys and everything they already received** |

The UI never shows view, comment and edit choices on an E2EE workspace, because those would promise what cannot be delivered. This matches SEC-V01 in [07](07-SECURITY-THREAT-MODEL.md).

### Per-operation enforcement in managed mode

**The problem.** A CRDT update is not an op. An `-ops` batch compiles down to low-level CRDT writes ([24](24-OPERATIONS-AND-MERGE.md)), and what the client sends is the result. A hostile client can write any update, so a client's claim about what it did is worthless.

**The approach.** The server applies the update to a validation replica through `-sdk` and asks what really changed.

```rust
// Proposed -sdk surface, row FND-23. Lives in -merge and -model, not the server.
pub fn classify_update(
    doc: &PageReplica,
    update: &[u8],
    limits: &ImportLimits,
) -> Result<UpdateClassification, ImportRefusal>;

pub struct UpdateClassification {
    pub classes: BTreeSet<ChangeClass>,   // closed enum, see matrix
    pub unknown: bool,                    // anything unclassifiable: refused
    pub touched_blocks: BlockSummary,     // for audit, without text
}
```

**Pipeline for `Update`.** Steps 1 to 6 happen before the update reaches anyone else.

| # | Step | On failure |
| --- | --- | --- |
| 1 | Frame size, per-connection and per-workspace rate | `CN-SRV-0006`, `CN-SRV-0007` |
| 2 | Session still valid, membership current | `CN-SRV-0002` |
| 3 | Duplicate check on `(workspace, doc, device, client_seq)`. If found and bytes match, return the original `Committed` | Different bytes: `CN-SRV-0014` |
| 4 | `classify_update` on a blocking thread, panic isolated | Hostile: `CN-SRV-0008`. Missing dependencies: `CN-SRV-0015`, client catches up first |
| 5 | Every class allowed for this person's effective access. `unknown` is always refused | `CN-SRV-0003` |
| 6 | In one transaction: check `grants_version` unchanged, insert log row, advance head | `NotSaving`, `CN-SRV-0010` |
| 7 | Apply to replica | Replica dropped and rebuilt from the log |
| 8 | `Committed` to sender | |
| 9 | Fan out `Broadcast` | Resume covers a crash here |
| 10 | Queue search and audit work | Outbox, retried |

The `grants_version` check in step 6 means that a revocation committed before the append wins, however the race falls.

**Refusal has a consequence the client must handle.** CRDT updates depend on earlier ones. Once one is refused, everything that peer later wrote in that document depends on something no one else has. So on `Refused` with a permission reason:

1. The client stops sending for that page.
2. It snapshots the changed page content locally as a **restored copy** in the person's private space, with a note saying why ([56](56-SYNC-DESIGN.md)). PR-1 applies: nothing is dropped silently.
3. It discards its replica for that page and reloads from the server under a new peer id.

The most common path is someone losing edit access while offline: they come back, their edits are refused, and they get a restored copy they can share with an editor.

**Access matrix.** Deny by default. A new `ChangeClass` is refused for everyone below `Manage` until a person places it here, as with OpenCalc's `Access::permits`.

| Change class | View | Comment | Edit | Manage |
| --- | --- | --- | --- | --- |
| Read page and subscribe | Yes | Yes | Yes | Yes |
| Add, edit own, delete own comment; react | No | Yes | Yes | Yes |
| Resolve or delete any comment | No | No | Yes | Yes |
| Block content, formatting, order | No | No | Yes | Yes |
| Canvas items, ink | No | No | Yes | Yes |
| Database rows | No | No | Yes | Yes |
| Database schema | No | No | Yes | Yes |
| Attachment add | No | No | Yes | Yes |
| Resolve merge review | No | No | Yes | Yes |
| Merge bookkeeping: mark a review (`MarkReview`) | No | No | Yes | Yes |
| Merge bookkeeping: write a checkpoint (`WriteCheckpoint`) | No | Only in the comment document | Yes | Yes |
| Move or rename page | No | No | Yes, with Edit on source and destination parents | Yes |
| Delete page, restore page | No | No | No | Yes |
| Grant, share, lock | No | No | No | Yes |
| Export (policy, ENT-R09) | Policy | Policy | Policy | Policy |
| `unknown` | No | No | No | No |

**Why bookkeeping has its own classes.** `MarkReview` and `WriteCheckpoint` are written by the engine ([24](24-OPERATIONS-AND-MERGE.md)), not by a person, and they travel in the same updates as edits. Unclassified, they would be `unknown`, so a device with Edit returning from offline would be refused for its own merge records and pushed into a restored copy. A forged marker costs a glance, since a review never blocks. A device below Edit has nothing of its own to merge, so its engine writes neither to the page, and shows any marker locally. This stops it publishing an update it would be refused for.

**Granularity limits, stated plainly.**

| Limit | Why |
| --- | --- |
| **Read access is per page, never per block** | A CRDT document cannot be partly sent |
| **A person with View on a page receives what is in its CRDT document, which may include deleted text** | History is in the document. Mitigation: viewers and external links get a projection or, where `SPK-01` shows it works, a shallow snapshot. Until then it is a documented limit |
| **The page tree document holds all titles** | So only people with workspace-wide View subscribe to it. Others get `GET /shared-with-me`, a server-made list, and no tree document |
| **Embeds and links do not grant access** | An embed of an unshared page shows a "no access" placeholder, and titles in links resolve through a permission check |
| **Revocation does not reach what was already received** | Nothing can |

**Effective access** is the most specific grant on the page's ancestry, capped by workspace role, then reduced by any page lock. A lock can only reduce, as in OpenCalc's session override, but here it is durable and audited because this server owns permission data.

### Managed and E2EE paths compared

| Step | Managed | E2EE |
| --- | --- | --- |
| Doc id on the wire | Page id | `HMAC(DK, notebook id ‖ page id)`, so the server never learns page ids. DK is independent of epoch keys ([56](56-SYNC-DESIGN.md)), so a rotation does not change a document's id and strand its log |
| Payload | CRDT update bytes | Sealed envelope using the [56](56-SYNC-DESIGN.md) format and key hierarchy |
| Validation | `classify_update` | Size and rate only |
| Presence payload | Cursor, name | Sealed cursor. Connection presence itself is visible |
| Comments | Separate comment document per page | Same, as another opaque sealed document |
| Search | Server index (`-search`) | Client only ([60](60-SEARCH-AND-EMBEDDINGS.md)) |
| Notification | Can include a preview | "Someone mentioned you", no content, and mentions are client-declared |
| Compaction | Server worker | Clients (below) |
| Admin recovery | Yes | Impossible |
| Corrupt document | Server can inspect and repair | Only clients can. An admin can only restore a backup |

**Choosing a mode is permanent for that workspace.** Converting from E2EE to managed means a client uploads plaintext, and from managed to E2EE cannot erase what the server and its backups already hold. Each conversion is therefore "create a new workspace and import a package" ([36](36-EXPORT-FORMAT.md)), and the old workspace's fate is stated separately.

**The E2EE sign-in and the passphrase are unrelated.** The server's identity flow never sees the passphrase, and setup warns that a company IdP password must not be reused as one. The client can compare only what it was given locally.

**Sharing in E2EE, first release.** Key-sharing between people is deferred ([56](56-SYNC-DESIGN.md)). An E2EE workspace is therefore one person's devices, plus one labelled option: a **shared-passphrase group**, where everyone who has been told the passphrase is an equal editor. There is no invitation button, no individual revocation of key access, and setup says so. Removing someone stops relay and needs a rotation with a new passphrase given to everyone else out of band. Whether to offer this option at all is an owner call, and it is off by default.

**E2EE compaction.** A client that holds the key writes a sealed checkpoint and offers it.

```
POST /api/v1/workspaces/{w}/docs/{doc}/checkpoints   { covers_cursor, parts, size }
```

The server cannot check the content, so a broken or hostile client could offer garbage. Hence the same rule as [56](56-SYNC-DESIGN.md): the log below `covers_cursor` stays until **another device has reported the checkpoint verified**, or a single-device workspace's own read-back. The previous checkpoint is retained until then. If no client compacts, storage grows and the admin sees bytes per workspace, with no way to act on content.

### Transport: WebSocket

`GET /api/v1/ws`, binary frames, CBOR, one socket per device multiplexing many documents of one workspace, named in `Hello`. So `doc` in a message means `(workspace, doc)`. Limit: 2 MiB per frame (proposal). A larger update uses several frames, as below. Browsers cannot set headers on a WebSocket, so the session token goes in the first frame and never in the URL, since URLs reach logs. The server closes a socket that has not sent `Hello` within 10 s, and it checks `Origin` on upgrade against a configured allow-list.

**Client to server.**

| Message | Fields | Notes |
| --- | --- | --- |
| `Hello` | `protocol_min`, `protocol_max`, `session_token`, `workspace`, `device_id`, `resume_key` | First frame only |
| `Subscribe` | `doc`, `have: { log_epoch, cursor }?` | `have` absent means start from the newest checkpoint |
| `Unsubscribe` | `doc` | |
| `Update` | `doc`, `client_seq`, `part?: { index, total, sha256 }`, `payload` | Must be durable locally first ([23](23-STORAGE-FORMAT.md)) |
| `Awareness` | `doc`, `payload` | Ephemeral, never stored |
| `CursorAck` | `doc`, `cursor`, `verified_checkpoints?`, `rejected?: [{ cursor, code }]` | The device's durable position; feeds trimming. `rejected` means as in [56](56-SYNC-DESIGN.md), and a rejected update does not hold the cursor back |
| `Ping` | `nonce` | Every 25 s (proposal) |

**Server to client.**

| Message | Fields | Notes |
| --- | --- | --- |
| `Welcome` | `protocol`, `mode`, `member`, `limits`, `heartbeat`, `resumed` | |
| `Subscribed` | `doc`, `log_epoch`, `head`, `access`, `checkpoint?` | `checkpoint` is a blob reference when needed |
| `Catchup` | `doc`, `from`, `to`, `items`, `more` | Bounded batches. An item above the frame limit uses the part form |
| `Committed` | `doc`, `client_seq`, `cursor` | **Sent only after the durable append** |
| `Refused` | `doc`, `client_seq`, `code`, `retryable`, `detail` | Includes `NotSaving` |
| `Broadcast` | `doc`, `cursor`, `origin_device`, `part?`, `payload` | Not sent to the origin device |
| `PeerRejected` | `doc`, `cursor`, `device`, `code` | Another device rejected the update at `cursor`. Sent to its origin, which runs the restored copy flow for that document ([56](56-SYNC-DESIGN.md)) |
| `Awareness` | `doc`, `origin`, `payload` | Identity from the server |
| `PresenceList` | `doc`, `members` | On subscribe and on change |
| `AccessChanged` | `doc`, `access`, `grants_version` | Reduction takes effect at once. Client stops or reloads |
| `Draining` | `retry_after` | Graceful shutdown |
| `Goodbye` | `code` | Before a deliberate close |
| `Pong` | `nonce` | |

**An `Update` above 2 MiB.** The client splits it into parts that each fit a frame, all with one `client_seq`, in order from 0. `total` and the `sha256` of the whole are the same on every part. The server holds the parts until the last one, checks the hash, and only then runs the pipeline on the whole update, so nothing is classified, committed or relayed in pieces. `Committed` or `Refused` answers the whole. A whole update is at most 64 MiB, the frame limit in [23](23-STORAGE-FORMAT.md), in at most 64 parts. One socket holds at most one unfinished update per document, and other messages may come between parts. Bad numbering or a bad hash is `CN-SRV-0005`, and over a limit is `CN-SRV-0006`. A lost socket drops the parts, and resume sends them again from 0, which is safe because `client_seq` is unchanged. `Broadcast` and `Catchup` use the same form downstream.

**Connection states on the client.**

```
Disconnected ── dial, backoff with jitter ──▶ Connecting
Connecting ── Welcome ──▶ Connected ── Subscribe ──▶ CatchingUp ── more=false ──▶ Live
Live ── socket lost ──▶ Disconnected (local edits keep being saved)
Live ── Refused(retryable) ──▶ Live, resend later
Live ── Refused(permission) ──▶ restored-copy flow for that page
Any ── Goodbye(auth) ──▶ NeedsSignIn
Any ── protocol mismatch ──▶ UpdateNeeded
```

The state panel shows this per server connection, on the same `SyncStatus` shape as [56](56-SYNC-DESIGN.md), and PR-2 applies to server workspaces too.

**Resume.** A client generates a `resume_key` once per installed device, and the server maps it to a device record scoped to the user. A key presented by a different user is refused, not adopted. On reconnect the client subscribes with its `have` values and resends everything unacknowledged, with the same `client_seq` and bytes. The unique constraint makes the resend safe. Two tabs in one browser share one device's store through a leader tab ([55](55-SDK-AND-EMBEDDING.md) and [23](23-STORAGE-FORMAT.md) decide how), and if they cannot, each tab has its own resume key.

**Log epoch.** `log_epoch` changes when the server's log is rewound, by restoring a backup. A client whose cursor is ahead of the head, or whose epoch differs, resends every update it holds that the server no longer has. **This makes a backup restore heal from clients:** any edit still on a device returns. Managed mode classifies them against current grants as normal, so a revoked person's old edits return as restored copies on their device, not to the page.

**Merge review.** Live co-editing never creates review markers. A device returning from offline runs the overlap detector on its own catch-up, on the client, in both modes. Its `MarkReview` and `WriteCheckpoint` writes are allowed classes in the matrix above. The flow is [56](56-SYNC-DESIGN.md).

**Protocol version.** Server protocol is independent of the sync protocol, and starts at 1. The server supports a window of versions. A client outside it gets `UpdateNeeded` and can keep working locally.

**Presence.** `Awareness` is rate-limited to 10 per second per connection (proposal), dropped first under load, and never stored. Colour and name come from the server. A hostile client can send a nonsense cursor in E2EE, since only clients can read it, so clients bound what they render ([42](42-EDITOR-AND-RENDERING.md)). Who is online is visible to the server in both modes.

### Comments

A comment thread needs a different write permission from the page it sits on. **Comments therefore live in a separate CRDT document per page,** not in the page document. A person with Comment access subscribes to the page for reading and publishes only to its comment document, which gives the server a simple per-channel check on top of `classify_update`.

| Aspect | Design |
| --- | --- |
| Anchor | Block id plus a stable text position from `-merge`, surviving edits. If the anchored text is deleted, the comment stays and is shown as "on deleted text" |
| Model | `Comment { id, thread, anchor, author, body_inlines, created, edited, resolved_by }`, proposed here because [22](22-DATA-MODEL.md) has no comment type yet (proposed row `FND-28`). Author id is set by the server in managed mode |
| Local comments | The same document type works without a server, and in export ([36](36-EXPORT-FORMAT.md)) |
| Mentions | Managed: server parses and notifies. E2EE: client declares recipients, and the server sees only that |
| Author in E2EE | Client-asserted, and any key holder can forge it. The UI labels this |

### Blob service

Attachments of any size, resumable, content-addressed (FR-08, `FIL-01`, [23](23-STORAGE-FORMAT.md)). The chunk model matches [56](56-SYNC-DESIGN.md), so a client has one upload engine.

| Step | Route | Notes |
| --- | --- | --- |
| Start | `POST /api/v1/workspaces/{w}/blobs` `{ id, size, chunk_size, chunks, sha256? }` | `id` is a content hash in managed mode, a keyed tag in E2EE. Returns existing state, so restart is the same call |
| Status | `GET /api/v1/workspaces/{w}/blobs/{id}/status` | Which chunks are held. This is how a client resumes after any failure |
| Upload | `PUT /api/v1/workspaces/{w}/blobs/{id}/chunks/{n}` | Idempotent. Header gives the chunk hash, checked on receipt |
| Finish | `POST /api/v1/workspaces/{w}/blobs/{id}/complete` | Managed: whole hash and size checked, then available. E2EE: count and size only |
| Download | `GET /api/v1/workspaces/{w}/blobs/{id}` with `Range`, or `/chunks/{n}` | Permission checked per request, `Content-Disposition: attachment`, `X-Content-Type-Options: nosniff`, never served inline from the app origin |
| Abort | `DELETE /api/v1/workspaces/{w}/blobs/{id}/upload` | Also expires after 7 days idle (proposal) |

Chunks stream through the server to object storage at first, so authentication stays in one place. Presigned object URLs may come later if throughput demands them, once measured. Resumable by chunk is deliberately simpler than adopting the tus protocol; it could be mapped onto later.

Chunk limit: 16 MiB, default 4 MiB. Quota is checked at start and enforced as chunks arrive. Managed mode offers an optional scan hook before availability. E2EE cannot scan, and the docs say so. Blobs are reference-counted through what the page and history refer to, then deleted after a grace period. E2EE cannot compute references, so clients report and the server only deletes after an explicit client request or workspace deletion. Reference tracking in managed mode is an open detail for [23](23-STORAGE-FORMAT.md).

### Identity

ENT-R01 says no password store of our own. So:

| Mode | When | How |
| --- | --- | --- |
| **OIDC** | Phase 8 | Any standards-compliant issuer, with a JWKS refresh as OpenCalc's. Small teams can run a small permissive IdP |
| **Invite tokens** | Phase 8, small installs and air-gapped | `casual-note-server admin invite <name>` prints a one-time link. Redeeming binds the device and creates a session. Only a hash is stored |
| **SAML** | Phase 10 (`ENT-01`) | Maps to the same accounts |
| **SCIM provisioning** | Later, on demand | Not designed here |

First run prints a one-time admin invite to the console, valid for one hour (proposal). Proposed row `FND-26` tracks this, since the OIDC-only reading of ENT-R01 leaves Phase 8 with no simple sign-in.

**Sessions.** Opaque 256-bit random tokens, stored hashed, so revocation is immediate (SEC-V03), with no signing key for a compromised node to misuse. Browser: `HttpOnly`, `Secure`, `SameSite=Strict` cookie for HTTP plus a short-lived socket ticket for `Hello`. Desktop: bearer token in the OS keychain ([44](44-DESKTOP-SHELL.md)). A session lasts 30 days idle (proposal), and admins can shorten it.

### Audit hook

ENT-R03 is delivered in Phase 10, and the hook exists from Phase 8. Events go through a transactional outbox table in the same transaction as the change, so nothing audited can be missed by a crash.

```rust
pub trait AuditSink { async fn deliver(&self, batch: &[AuditEvent]) -> Result<(), AuditError>; }
// Shipped: JSON lines to stdout, file, HTTPS webhook with HMAC signature.
```

| Event family | Managed includes | E2EE includes |
| --- | --- | --- |
| Sign-in, session, token | Actor, time, source | Same |
| Member and grant changes, share links, locks | Full detail | Same |
| Page opened, edited | Page, classes, block counts, **no text** | Opaque doc, byte count |
| Export, admin recovery | Actor, scope | Export happens on the client and is not seen |
| Blob | Id, size | Tag, size |
| AI relay (ENT-R06) | Provider, model, counts, **no text** | Same, if opted in |

Sink failure: a retained outbox with alerting, by default. An operator can set `audit.fail_closed=true`, so that writes stop when the outbox passes a size limit, which is a visible outage they chose.

### Optional provider relay

Design and policy are in [62](62-AI-ASSISTANT-AND-PROVIDERS.md) (`AST-04`, ENT-R05, ENT-R06). This server only supplies the following.

| Rule | Detail |
| --- | --- |
| Off by default | `ai.relay.enabled=false`. With it off, the routes do not exist |
| Endpoints | From admin config only. **A client never supplies a URL,** which rules out request forgery to internal services |
| Keys | Held server-side in secrets the operator provides, never returned to clients |
| Route | `POST /api/v1/ai/relay/{provider}/chat`, streaming through, with size and rate limits per user |
| Logging | Counts only. Bodies are never logged or stored |
| Workspace policy | Admin allow-list. Managed: enforced here |
| E2EE | A relay means plaintext passes through this server, which breaks the mode's main promise for those requests. So it is refused (`CN-SRV-0013`) unless opted in per notebook (SEC-A05), and the panel says the server can see those requests |

A browser that calls a provider directly does not involve this server.

### Data model

Sketch for `SRV-01`. Plain SQL through a pinned migration tool. Text columns hold no note text in E2EE, by test.

**Every document and blob is keyed by `(workspace, id)`.** Ids are unique only within a workspace, and an imported package keeps its ids ([36](36-EXPORT-FORMAT.md)), so one package imported into two workspaces gives two documents with one id. A global key would make the second import collide with, or reach, the first.

```sql
CREATE TABLE workspaces (
  id uuid PRIMARY KEY, mode text NOT NULL CHECK (mode IN ('e2ee','managed')),
  name_enc bytea, name text,           -- name is NULL in e2ee
  log_epoch bigint NOT NULL DEFAULT 1, created_at timestamptz NOT NULL,
  CHECK ((mode = 'e2ee') = (name IS NULL))
);
CREATE TABLE accounts (id uuid PRIMARY KEY, issuer text, subject text, display_name text,
  UNIQUE (issuer, subject));
CREATE TABLE members (workspace uuid REFERENCES workspaces, account uuid REFERENCES accounts,
  role text NOT NULL, grants_version bigint NOT NULL DEFAULT 0, removed_at timestamptz,
  PRIMARY KEY (workspace, account));
CREATE TABLE devices (id uuid PRIMARY KEY, account uuid NOT NULL REFERENCES accounts,
  resume_key_hash bytea NOT NULL UNIQUE, last_seen timestamptz);
CREATE TABLE sessions (token_hash bytea PRIMARY KEY, account uuid NOT NULL,
  device uuid, expires_at timestamptz NOT NULL, revoked_at timestamptz);

CREATE TABLE docs (workspace uuid NOT NULL REFERENCES workspaces, id bytea NOT NULL,
  kind smallint,                       -- NULL in e2ee: the server does not know
  head bigint NOT NULL DEFAULT 0, floor_cursor bigint NOT NULL DEFAULT 0,
  PRIMARY KEY (workspace, id));
CREATE TABLE doc_log (workspace uuid NOT NULL, doc bytea NOT NULL, cursor bigint NOT NULL,
  device uuid NOT NULL, client_seq bigint NOT NULL, payload_hash bytea NOT NULL,
  payload bytea, payload_ref text,     -- inline up to 64 KiB, else object storage
  account uuid NOT NULL, created_at timestamptz NOT NULL,
  PRIMARY KEY (workspace, doc, cursor), UNIQUE (workspace, doc, device, client_seq))
  PARTITION BY HASH (workspace, doc);
CREATE TABLE doc_device_cursor (workspace uuid, doc bytea, device uuid, cursor bigint NOT NULL,
  PRIMARY KEY (workspace, doc, device));
CREATE TABLE checkpoints (id uuid PRIMARY KEY, workspace uuid NOT NULL, doc bytea NOT NULL, covers_cursor bigint NOT NULL,
  object_key text NOT NULL, size bigint NOT NULL, created_by uuid,
  verified_by uuid[] NOT NULL DEFAULT '{}', created_at timestamptz NOT NULL,
  FOREIGN KEY (workspace, doc) REFERENCES docs (workspace, id));

CREATE TABLE grants (workspace uuid, doc bytea, subject_kind text, subject uuid,
  access text NOT NULL, created_by uuid, created_at timestamptz,
  PRIMARY KEY (workspace, doc, subject_kind, subject));      -- managed only
CREATE TABLE page_locks (workspace uuid, doc bytea, locked_by uuid, max_access text, reason text,
  PRIMARY KEY (workspace, doc));
CREATE TABLE share_links (token_hash bytea PRIMARY KEY, workspace uuid NOT NULL, doc bytea NOT NULL, access text NOT NULL
  CHECK (access IN ('view','comment')), expires_at timestamptz, revoked_at timestamptz);

CREATE TABLE blobs (id bytea, workspace uuid, size bigint, chunk_size int, state text,
  created_by uuid, PRIMARY KEY (workspace, id));
CREATE TABLE blob_chunks (workspace uuid, blob bytea, n int, hash bytea, object_key text,
  PRIMARY KEY (workspace, blob, n));
CREATE TABLE audit_outbox (id bigserial PRIMARY KEY, event jsonb NOT NULL, delivered_at timestamptz);
```

**Cursor assignment.** `UPDATE docs SET head = head + 1 … RETURNING head` inside the append transaction. This serialises appends per document on one row, which is simple and may bottleneck a very busy page. That cost is unmeasured.

**Managed-mode trimming** follows [56](56-SYNC-DESIGN.md): `floor_cursor` is the lowest `doc_device_cursor` over the workspace's registered devices. Log rows below both the floor and a checkpoint may go. History trimming obeys the same rule. A device admin-removed by explicit act stops counting, and if it returns it gets a restored copy. A retention policy that an admin set, with a stated device-expiry period, counts as an explicit act, and is shown as such in the admin screen.

### HTTP API

Prefix `/api/v1`, JSON unless noted. Errors: `{ "code": "CN-SRV-nnnn", "message_key", "detail" }`, with messages from the catalogue and never hard-coded (ADR-018, ENT-R11). "E" and "M" show which modes offer a route.

| Method and path | Purpose | Requires | Mode |
| --- | --- | --- | --- |
| `GET /healthz`, `GET /readyz` | Probes, unauthenticated, no prefix | None | Both |
| `GET /api/v1/meta` | Version, protocols, auth methods, limits | None | Both |
| `GET /auth/oidc/start`, `/auth/oidc/callback` | OIDC | None | Both |
| `POST /auth/invite/redeem` | Invite token | Token | Both |
| `POST /auth/logout`, `GET/DELETE /sessions` | Session list and revoke | Session | Both |
| `POST /ws-ticket` | One-time socket ticket | Session | Both |
| `POST/GET /workspaces` | Create (mode is fixed here), list | Account | Both |
| `GET/PATCH/DELETE /workspaces/{w}` | Settings, delete | Admin | Both |
| `GET/POST/DELETE /workspaces/{w}/members` | Members | Admin | Both |
| `GET /workspaces/{w}/devices`, `DELETE …/{d}` | Device list, explicit removal | Own, or Admin | Both |
| `GET/PUT/DELETE /workspaces/{w}/docs/{d}/grants` | Page sharing | Manage | M |
| `PUT/DELETE /workspaces/{w}/docs/{d}/lock` | Lock, reduce only | Manage | M |
| `POST/DELETE /workspaces/{w}/docs/{d}/share-links` | External link | Manage | M |
| `GET /shared-with-me` | Pages available to me, each as workspace and id | Account | M |
| `POST /workspaces/{w}/docs/{d}/checkpoints` | Client compaction | Publish right | E |
| `GET /workspaces/{w}/docs/{d}/checkpoints/latest` | Checkpoint reference | Subscribe right | Both |
| Blob routes above | Upload, download | Edit to write, View to read | Both |
| `GET /workspaces/{w}/search?q=` | Organisation search | Filtered by read access | M |
| `GET /workspaces/{w}/docs/{d}/history`, `POST …/restore` | Admin recovery | Admin | M |
| `POST /workspaces/{w}/export` | Server-side package ([36](36-EXPORT-FORMAT.md)) | Policy | M |
| `GET/PUT /workspaces/{w}/retention` | Retention and hold (ENT-R08) | Admin | M |
| `POST /ai/relay/{provider}/chat` | Provider relay | Policy | M; E only if opted in |
| `GET /metrics` | On a separate listener by default | Operator | Both |

Search results are filtered by read access at query time, since an index can otherwise leak titles and snippets. A route not offered in a mode returns `CN-SRV-0004`, so a client can explain why rather than appearing broken.

### Deployment

| Aspect | Design |
| --- | --- |
| Artefact | One binary, `casual-note-server`, in a small container with no `curl`. `--healthcheck` and `--readycheck` as OpenCalc |
| Dependencies | Postgres, S3-compatible object storage. Versions and products tested are unverified until `SRV-06`. Nothing else |
| Compose | Server, Postgres, one permissive S3-compatible store chosen under ADR-023 |
| Helm | Same, with external stores, and probes mapped liveness to `--healthcheck`, readiness to `--readycheck` |
| TLS | Server can terminate, or a reverse proxy can. Plaintext listener off loopback warns at startup. Mixed certificate configs are refused |
| Air gap | No external calls, CDN or fonts (ENT-R04, QR-6). The web app is served from the binary or a static path |
| Web app CSP | Strict, and `connect-src` lists this server only, plus provider endpoints that admin policy allows ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)) |
| Encryption at rest | Managed: operator's disk and object-storage keys (ENT-R07). E2EE adds nothing here because content is already sealed |
| Migrations | Forward-only with `--migrate`, run explicitly. Tested across versions (ENT-R13) |
| Backup | Postgres first, then object storage, object versioning advised. Restoring raises `log_epoch` with `casual-note-server admin after-restore`, and clients heal (above). The package is the portable last resort (ENT-R10) |
| Observability | Metrics and structured logs with **no note text, no titles in E2EE, no tokens** (ENT-R14), checked by a canary test |
| Config | Environment variables, `CASUALNOTE_*`, plus a file. Secrets from files or the environment, never in `/meta` or logs |
| Durability setting | `synchronous_commit=on` is required for the receipt promise. The server checks it at startup and refuses to start without `--unsafe-allow-async-commit`. With a replica, `synchronous_standby_names` decides what survives a failover, and asynchronous replication can lose an acknowledged append: in that case clients heal it through `log_epoch` if the operator raises it |

Crate candidates, with no licence audit done: `axum`, `tokio`, `sqlx` or `tokio-postgres`, `object_store`, `openidconnect`. They must pass `GOV-05`. The server sits in the main workspace or its own, as decided at `SRV-01`.

### Scaling, honestly

**Nothing here has been measured.** No server exists, and the unknowns below may change the design.

**Phase 8 ships one node.** One process with Postgres and object storage is the only supported deployment until a load test says otherwise. This is a real limit, and the docs state it.

| Unknown | Why it matters | How it gets known |
| --- | --- | --- |
| Memory per resident page replica in managed mode | Validation holds a replica per active page. Loro's real footprint on long pages is untested | `SPK-01`, then the `SRV-02` load test |
| `classify_update` cost | Runs per update. If it is slow, typing latency for others suffers | Benchmark once `FND-23` exists |
| Appends per second on one page, given the `head` row | A busy page serialises | Load test |
| Connections per node | Fan-out cost grows with subscribers | Load test |
| Postgres storage growth from per-keystroke rows | Client batching interval, proposed as 100 to 250 ms, trades latency against rows | Load test |
| E2EE overhead | Envelope on every small update, no compaction by the server | Load test |
| Long-offline client catch-up | Could need a large read | Checkpoints plus bounded batches, untested |
| Postgres `LISTEN`/`NOTIFY` under load | Possible multi-node fan-out path | Only if multi-node is pursued |

**Proposed load targets for [30](30-PERFORMANCE-TARGETS.md),** labelled as proposals: 50 editors on one page with 500 ms end-to-end relay at the 95th percentile on stated hardware, and 1,000 connected devices on one node. `SRV-02`'s "convergence under load" and Phase 8's "a load test meets its target" need such a row, which 30 does not yet have.

**Path to several nodes, not committed.** Because a CRDT needs no single orderer, any node can append with the same unique constraints, and any node can validate by catching its replica up from the log. Cross-node fan-out would carry a pointer through `LISTEN`/`NOTIFY`, with payload read from the log. Redis, and OpenCalc's leader, lease and fencing machinery, are left out unless measurement requires them, and OpenCalc's own warnings about asynchronous failover in its doc 77 would then apply. A socket pins a connection to a node and not a client to a node, so no sticky sessions are planned.

## Failure modes and limits

| Failure | Behaviour | Code |
| --- | --- | --- |
| Postgres unavailable | `/readyz` 503. `Refused { NotSaving }` on updates. Clients keep edits locally and show it | `CN-SRV-0010` |
| Object storage unavailable | Inline updates still commit. Large ones, blobs and checkpoints refused. Download errors are visible | `CN-SRV-0011` |
| Crash after append, before `Committed` | Client resends, unique constraint returns the same cursor | None |
| Crash after `Committed`, before fan-out | Others catch up from the log on reconnect | None |
| Hostile update panics in import | Caught on the blocking thread (`panic=unwind` required in the server build). Page replica dropped, update refused, sender rate-limited, event logged. Repeat offenders' sessions are closed | `CN-SRV-0008` |
| Import hangs or uses excess memory | Time and size limits. A hard limit needs a process boundary, which is unverified as to need. Isolating imports in a worker process is the fallback | `CN-SRV-0008` |
| Replica grows too large | Eviction, rebuilt on demand. A page over limit refuses new writes with a visible message | `CN-SRV-0012` |
| Slow consumer | Bounded queue per connection. On overflow, `Goodbye`, and it resumes by cursor. Drop presence first | `CN-SRV-0016` |
| Permission revoked while offline | Edits refused on return, restored copy on that device | `CN-SRV-0003` |
| Permission race | `grants_version` in the append transaction decides | `CN-SRV-0003` |
| Client's update needs a missing dependency | Refused retryable, client catches up and resends | `CN-SRV-0015` |
| Backup restored | `log_epoch` rises, clients resend what was lost | None; panel notes it |
| Asynchronous Postgres failover loses appends | Clients show saved locally and committed, then heal when the operator raises `log_epoch`. Without that step, the gap persists until cursors disagree | Named as a residual risk |
| Quota exceeded | New blobs refused. Text continues within a limit | `CN-SRV-0007` |
| Audit sink down | Outbox grows, alert. Optional fail closed | `CN-SRV-0017` |
| OIDC issuer unreachable | Existing sessions continue. New sign-ins refused | `CN-SRV-0018` |
| Session expires mid-edit | `NeedsSignIn`. Local edits continue being saved | `CN-SRV-0002` |
| Protocol mismatch | `UpdateNeeded`. Local work continues | `CN-SRV-0001` |
| E2EE: corrupt or garbage checkpoint | Previous checkpoint and log remain until verified. Server cannot detect it, clients do | None at server |
| E2EE: lost passphrase | Server cannot help. No admin recovery | Client side `CN-CRY-0001` |
| E2EE: all members lose keys | Ciphertext only. Admin can delete it | None |
| Admin in managed mode | Can read all content by design, and the mode label says so. Audit is the only control | None |
| Co-edit storm of merge conflicts | Not applicable live, since CRDT merges each update. Merged text quality is RISK-03 | None |

### Error codes and limits

Proposed for [20](20-ERROR-CODE-REGISTRY.md) and [21](21-PARSER-LIMITS.md).

| Code | Meaning |
| --- | --- |
| `CN-SRV-0001` | Protocol outside the server's window |
| `CN-SRV-0002` | Not signed in, or session expired or revoked |
| `CN-SRV-0003` | Permission refused for this change |
| `CN-SRV-0004` | Not available in this workspace's mode |
| `CN-SRV-0005` | Malformed frame |
| `CN-SRV-0006` | Frame or body too large |
| `CN-SRV-0007` | Rate or quota exceeded |
| `CN-SRV-0008` | Update rejected by guarded import |
| `CN-SRV-0009` | Workspace mode is permanent |
| `CN-SRV-0010` | Not saving: log unavailable |
| `CN-SRV-0011` | Object storage unavailable |
| `CN-SRV-0012` | Page over its size limit |
| `CN-SRV-0013` | AI relay refused by workspace mode or policy |
| `CN-SRV-0014` | Same `client_seq`, different bytes |
| `CN-SRV-0015` | Update depends on state not yet held |
| `CN-SRV-0016` | Client too slow; reconnect to resume |
| `CN-SRV-0017` | Audit delivery failing |
| `CN-SRV-0018` | Identity provider unreachable |

| Limit (proposal) | Value |
| --- | --- |
| WebSocket frame | 2 MiB |
| Inline log payload | 64 KiB |
| Subscriptions per socket | 512 |
| Sockets per account | 32 |
| Outbound queue per socket | 4 MiB |
| `Catchup` batch | 1 MiB or 1,000 items |
| Awareness rate | 10 per second |
| Whole `Update` across parts | 64 MiB, 64 parts |
| `Hello` deadline | 10 s |
| Blob chunk | 16 MiB, default 4 MiB |
| Blob idle expiry | 7 days |
| Relay request body | 8 MiB |

## Alternatives considered

| Alternative | Why not |
| --- | --- |
| Copy OpenCalc's server | Imports spreadsheet crates, and its OT core has no role here |
| Server as a layer, with merge logic inside | Forks engine logic. A boundary rule forbids it |
| Trust a client's op description next to the update | A hostile client can send a harmless description with a harmful update |
| Send ops rather than CRDT updates and have the server compile them | Would make the server the order for offline work, which is what ADR-005 rejected |
| Per-block read permissions | A CRDT document cannot be partly delivered |
| Enforce E2EE permissions by client verification | Divergence without ordered membership and personal signatures. See above |
| One passphrase-derived server login | Would tie authentication to content keys |
| JWT sessions | Immediate revocation needs state anyway, and a signing key becomes a target. Kept for host-signed embed tokens later ([55](55-SDK-AND-EMBEDDING.md)) |
| Redis from day one | No demonstrated need, and a second durable-looking store with asynchronous failover |
| Separate relay and store services | More operations for no known benefit at this size |
| tus for blobs | Standard, but chunk-addressed idempotent upload is simpler and matches the sync model |
| Presigned URLs from day one | Scatters authentication and needs per-store CORS. Later, on evidence |
| Convertible workspace modes | Cannot erase what the server and backups already hold, and the promise would be false |

## Acceptance gates

| Gate | Evidence required |
| --- | --- |
| `SRV-01` | Two browsers in CI against a real server, Postgres and object storage |
| `SRV-02` | Two to fifty clients converge under injected disconnects. Load target from [30](30-PERFORMANCE-TARGETS.md) |
| `SRV-03` | Every `ChangeClass` has an allow and a refuse test. A hand-crafted update claiming to be harmless is refused. **A refused op never reaches another client.** A new class fails CI until placed in the matrix |
| `SRV-04` | Canary text in an E2EE workspace is absent from all Postgres tables, objects, logs and metrics. A gate proves the E2EE request path never calls content APIs of `-sdk` |
| Refusal | Offline edits with revoked access appear as a restored copy, none lost |
| Append before ack | Injected crash at each step: any `Committed` update survives restart |
| Backup | Restore an old backup: clients heal, none lost |
| Resume | Socket killed at each message boundary, and between the parts of a large `Update`: no loss, no duplicate |
| Bookkeeping | A device with Edit returning with `MarkReview` and `WriteCheckpoint` in its updates is committed. A device below Edit writes neither, and is not refused (`SRV-03`) |
| Rotation | Document ids in E2EE are the same before and after a key rotation (`SRV-04`) |
| Ids | One package imported into two workspaces gives two independent documents. A route with the wrong workspace is refused (`SRV-03`) |
| Hostile input | Fuzzed frames, fuzzed updates: no crash of the process (SEC-S01, SEC-V02). One fixture per limit |
| Identity | `resume_key` from another user refused. Token never in a URL or log. Cross-origin socket refused |
| `SRV-05` | Comment access cannot publish to the page document |
| `SRV-06` | Fresh install from the docs on Compose and Helm. Upgrade across versions |
| Air gap | Network capture in an isolated install shows no external call |
| Public claim | No E2EE claim before `CRY-02` |

## ADRs

- **ADR-022** (Proposed): this design.
- **ADR-005, ADR-006:** CRDT per page, which is why there is no server order.
- **ADR-021:** the sealed format, shared in E2EE mode.
- **ADR-002, ADR-023:** no enterprise edition, and the licence gate applies to the server's dependencies.
- **ADR-012, ADR-013:** relay is a provider path, and approval rules remain client-side.
- **ADR-004:** patterns from OpenCalc, without modifying it.

## Tracker ids

`SRV-01` to `SRV-06`, `AST-04`, `ENT-01`, `FIL-01`, `MRG-02`, `CRY-02`, `SPK-01`, `GOV-05`, `DEC-03`. Threats: SEC-V01 to SEC-V03, SEC-S01, SEC-A05. Requirements: FR-20, FR-21, ENT-R01 to ENT-R14. Proposed rows: `FND-23` (`classify_update` in `-sdk`), `FND-26` (Phase 8 sign-in without a password store), `FND-27` (server load targets in 30), `FND-28` (comment model in 22, and in export).

Related: [10](10-ENTERPRISE-REQUIREMENTS.md), [22](22-DATA-MODEL.md), [23](23-STORAGE-FORMAT.md), [24](24-OPERATIONS-AND-MERGE.md), [36](36-EXPORT-FORMAT.md), [42](42-EDITOR-AND-RENDERING.md), [44](44-DESKTOP-SHELL.md), [55](55-SDK-AND-EMBEDDING.md), [56](56-SYNC-DESIGN.md), [60](60-SEARCH-AND-EMBEDDINGS.md), [62](62-AI-ASSISTANT-AND-PROVIDERS.md), [07](07-SECURITY-THREAT-MODEL.md), [15](15-CI-AND-RELEASE-GATES.md), [20](20-ERROR-CODE-REGISTRY.md), [21](21-PARSER-LIMITS.md), [30](30-PERFORMANCE-TARGETS.md).
