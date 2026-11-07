# 60 — Search and embeddings

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) has a stand-in: `toks` splits text with regular expressions and `vectorize` hashes word counts into a small vector. That is word overlap, not an embedding model, and neither its tokeniser nor its thresholds carry over. No model named here has been run. Every number is a proposal until a prototype measures it ([30](30-PERFORMANCE-TARGETS.md)).

## Outcome

A person finds a note instantly and offline, in whatever language they wrote it. Exact search works with AI off, needs no download and returns the same results on native and web. Search by meaning is an optional download that finds a note written in different words or another language, and always says that it is a match by meaning.

Requirements: FR-09, FR-10, QR-2, QR-3, PR-4, PR-5. Crates: `-search` and `-embed` ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)). Both read the model and never write to it.

## Design

### Part 1: exact search

#### What gets indexed

The unit is a **search document**, one per piece of text with an edit identity. An edit re-tokenises only the documents it touched.

| Source | Field | Start weight |
| --- | --- | --- |
| Page title | `title` | 4 |
| Heading block | `heading` | 2 |
| Other text block, including code | `body` | 1 |
| Database row name | `row` | 2 |
| Database cell text | `cell` | 1 |
| Canvas sticky, shape or text item | `canvas` | 1 |
| Attachment file name | `file` | 1 |
| OCR text, if that option is installed | `ocr` | 0.5 |

The weights are starting values chosen for this draft and are not measured. `SPK-05` tunes them, then they freeze under the ranking version. Pages in the trash are removed from the index and added back on restore. Ink is not indexed, because handwriting recognition is out of scope ([01](01-ORD.md)).

#### Tokenising

Tokenising is pure, has no I/O, and is a function of the text and `tokeniser_version` alone. It ignores the interface language, so results do not change with it. A block's optional `lang` is not used in version 1.

**Step 1, the matching copy.** Take the stored text and produce a temporary copy. **The stored text is never changed** ([36](36-EXPORT-FORMAT.md) multilingual rules).

1. Normalise to NFC.
2. Apply full Unicode case folding, so `Straße` and `STRASSE` agree.
3. Remove default-ignorable characters such as U+200B, U+200C, U+200D, LRM and RLM from the copy. A right-to-left note with invisible direction marks is still findable. Keep an offset map to the original text, used for highlighting.
4. **Keep combining marks.** Hindi and Arabic words change meaning or break without them.

Case folding on its own never removes a mark. NFKC is not used, because it changes too much text. The width table in step 3 of the query rules below covers the part of NFKC that Japanese and Chinese users need.

**Step 2, split into runs by script class.** Common and Inherited characters take the class of the run before them. A change of class ends a run, so `iPhone手机` gives one Latin run and one Han run.

**Step 3, make terms for each run.**

| Class | Scripts | Terms |
| --- | --- | --- |
| Word | Latin, Greek, Cyrillic, Arabic, Hebrew, Devanagari and any script not named below | UAX #29 word boundaries. Numbers stay whole. Punctuation is not indexed |
| Hangul | Korean | The space-separated word, plus Hangul syllable bigrams, so a noun is still found when a particle is attached |
| No spaces | Han, Hiragana, Katakana, Thai, Lao, Khmer, Myanmar | The configured segmenter, or else overlapping bigrams of grapheme clusters |
| Symbol | Emoji | One term per grapheme cluster |

**Grapheme clusters, not code points,** form the bigrams, so a Thai vowel sign or tone mark is never separated from its consonant.

**Word segmentation or bigrams.** Bigrams come first, because they need no dictionary and are deterministic, and `SPK-05` starts from them. They also add positions and terms, which puts pressure on PERF-07. The tokeniser has a per-class `Segmenter` slot so that a dictionary segmenter can replace them. `SPK-05` compares bigrams against dictionary segmenters for Chinese, Japanese and Thai, recording findability, precision, index size and added wasm size. A dictionary segmenter is adopted only if bigrams fail the gate and its licence passes ADR-023. Candidates are below in Alternatives considered.

**No stop words and no stemming in version 1.** Stop-word lists and stemmers exist per language, and using them for English alone would make English better than the others (ADR-018). BM25 already discounts common words. Prefix matching on the last query term covers much of what stemming would. `SPK-05` may add a per-language stemmer only if a test set shows the need, and only for documents whose language is known.

**Secondary terms, for optional marks.** A primary term always keeps its marks. A narrow table can also emit one extra term, stored in the same posting list and ranked below the primary:

| Script | Extra term removes | Why |
| --- | --- | --- |
| Latin, Greek, Cyrillic | Marks after canonical decomposition | `ete` should find `été` |
| Arabic | Harakat (U+064B to U+0652) and tatweel (U+0640). Alef variants fold to bare alef | Vowel marks are usually omitted when typing, so a search without them should find text with them |
| Devanagari | Nothing | Vowel signs are part of the word |

This table is a proposal from the design author, not a linguist's review. The per-language test sets decide it, with native review under `I18N-05`. It adds to index size, so it is one of the first things cut if PERF-07 fails.

**Known limits, stated rather than hidden.**

- Simplified and Traditional Chinese are different terms: `软件` does not find `軟件` (proposed row `FND-68`).
- Japanese kana does not find the same word written in kanji.
- An Arabic word with an attached clitic (`والكتاب`) is not found by searching `كتاب` unless a segmenter or light stemmer is added. `SPK-05` tests this.

**Pinned Unicode data.** A newer Unicode table can change terms. The normalisation, segmentation and case-fold tables come from pinned crates, and any change bumps `tokeniser_version`, which triggers a rebuild. A golden file of terms per launch language catches an accidental change.

#### Index format

Everything is a rebuildable derived file ([23](23-STORAGE-FORMAT.md) owns the directory). It sits outside sync, export and the mirror, and has the same local protection as the notes it derives from. It is never treated as trusted: a parse failure leads to a rebuild, never a panic.

```
derived/search/
  manifest.json        format, tokeniser_version, ranking_version, segment list, file hashes
  pages.bin            page ordinal, page id, page version last indexed
  stats.bin            per-field document count and total length; document-frequency overlay
  seg-<n>.dict         sorted terms, front-coded in blocks; each entry: df, postings offset, positions offset
  seg-<n>.post         document ids and term frequencies, delta and varint coded, in blocks with a skip table
  seg-<n>.pos          token positions for phrase queries, read only when a phrase needs them
  seg-<n>.doc          document id to page ordinal, block id, field, length
  seg-<n>.fwd          term ordinals per document, so a delete can correct document frequency
  seg-<n>.live         deletion bitmap
```

Integers are little-endian. Within a segment, document ids sort by page ordinal, so all documents of one page are adjacent and postings can be grouped by page while merging. Only one sparse entry per dictionary block is kept in memory, and the rest is read on demand through the host I/O seam (`STO-03`: OPFS sync access in a worker, files on native).

**Why exact statistics.** BM25 depends on document frequency and average length. If those depended on how segments happened to be merged, the same notes could rank differently on two devices, which breaks QR-3. So `stats.bin` keeps exact live totals. The live document frequency of a term is the sum across segments plus the overlay in `stats.bin`, and `.fwd` tells a delete which terms to decrement. `.fwd` costs space, and `SPK-05` measures that.

#### Updating off the typing path

```
op commits → projection reports changed blocks → host queues BlockChange{page, block, new text}
   → (idle pause) → separate worker or thread → SearchIndex.enqueue → step(budget) → memtable → flush to a segment
```

- **Not on the typing path (PR-4, PERF-09).** The durable write and "saved" come first and never wait for the index. The index runs in its own worker on the web and its own thread on desktop.
- **Pause.** The host waits for an idle pause in typing, with a maximum wait, then sends the queued changes. Proposed: 1 second idle, 10 seconds maximum.
- **The engine has no clock.** `step` takes a budget in documents and bytes. The host measures time and chooses the budget, so results depend only on the input.
- **Queries see fresh text.** A search first drains pending changes, up to a budget, and searches the in-memory segment as well as the files. If changes are still pending, the result says how many (`CN-SCH-0006`).
- **Flush and merge.** The in-memory segment flushes when it is large or the app is idle. The manifest is replaced atomically. Tiered merging is also budgeted, and it drops deleted documents.
- **Crash or external change.** On open, compare page versions from the store with `pages.bin`. For a page that differs, delete all its documents and add the current ones. No index edit is ever part of the durability contract, so losing the in-memory segment loses nothing.
- **Rebuild.** Deleting `derived/search/` is always safe. A rebuild shows progress, and search answers meanwhile from what is ready, with that state stated.

```rust
pub struct SearchIndex { /* ... */ }
impl SearchIndex {
    pub fn open(io: &dyn DerivedIo) -> Result<Self, SearchError>;   // CN-SCH-0001, 0002 lead to a rebuild
    pub fn enqueue(&mut self, change: BlockChange);
    pub fn step(&mut self, budget: WorkBudget) -> StepOutcome;      // Idle, MoreWork, Flushed
    pub fn search(&mut self, query: &Query, opts: &SearchOpts) -> SearchResults;
    pub fn state(&self) -> IndexState;                              // Ready, Building{done, total}, Pending{n}
}
```

#### Querying

1. Run the same tokeniser over the query.
2. A run of a no-spaces class becomes a phrase of its bigrams. A single character in those classes scans the dictionary by prefix, bounded by the expansion limit.
3. Apply a small width table to fullwidth ASCII and halfwidth katakana, in both query and text, so `ＡＢＣ` finds `ABC`.
4. Syntax is small: words, `"a phrase"`, `-excluded`. The last unquoted term also matches by prefix, for search as you type.
5. All terms must appear somewhere **in the page**, not necessarily in one block. If nothing matches, retry with any term and label those results "some of your words".

#### Ranking

BM25 per search document, with `k1 = 1.2` and `b = 0.75` (the customary published values, unmeasured here), average length taken per field, and the field weights above.

```
idf(t)   = ln(1 + (N - df + 0.5) / (df + 0.5))
block(d) = weight(field) * sum over t of idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len / avg_len))
page(p)  = max block + 0.25 * (sum of its other matching blocks, capped at the max)
```

- A run of bigrams contributes the **mean** of its bigram scores, not the sum, or a Chinese query would dominate a mixed-language one simply by having more terms.
- A secondary term scores at half its primary. Starting value, tuned in `SPK-05`.
- **Recency and open-count do not affect rank.** They need a clock and per-device state, which would break QR-3.
- **Determinism.** Logarithms come from a pure Rust maths crate, not the platform's. Scores are rounded to fixed-point integers, and ties break on page id then block id. Native and wasm then agree. CI compares them (the Determinism job in [15](15-CI-AND-RELEASE-GATES.md)).

Evaluation is document-at-a-time with a top-k heap and the skip tables. Block-max pruning is added only if `SPK-05` needs it. It must not change the results, only the work.

**A result** is a page id, the best block id, the field, and a snippet of plain text with highlight ranges as offsets into the original. **Snippets are never HTML** (QR-7). They are re-tokenised at query time through the offset map, so no offsets are stored in the index.

### Part 2: search by meaning

Models are optional downloads (ADR-015, ADR-027). This feature is off until the person installs one. Nothing downloads without a click ([62](62-AI-ASSISTANT-AND-PROVIDERS.md) download flow). It is on-device in both modes, including the web, and needs no AI provider. The model runs on the device and no text leaves it, so turning the assistant off does not require turning this off.

#### Chunking

One chunk per block is the default, because a block already has an edit identity.

| Case | Rule |
| --- | --- |
| Ordinary text block | One chunk |
| Block over the model's input budget | Split at UAX #29 sentence boundaries, with no overlap. Chunk key is block id plus ordinal |
| Block with no letters or numbers, divider, database reference, empty block | No chunk |
| Database row | One chunk: the row name, then `column: value` for text cells |
| Canvas text item | One chunk |
| Very short block | Kept. Its input is prefixed with context, below, so it still means something |

**Context prefix.** The text sent to the model is the page title, the nearest heading and the block text, up to the model's budget. The prefix is never stored. The input budget per model is set in the catalogue after `SPK-06`, and is counted in graphemes as a conservative stand-in, because the tokeniser belongs to the host.

**Skip unchanged text.** Each chunk stores a hash of its input, `chunker_version` and the model tag. An edit that restores earlier text, or a block move, costs nothing.

#### The model is a host seam

`-embed` owns chunking, the store and nearest-neighbour search, and has no inference or network code ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md) rule 4). Inference in the browser ends in a worker message, so the seam is a pull interface with no async in the crate:

```rust
pub struct ModelTag { pub id: String, pub revision: String, pub file_sha256: [u8; 32],
                      pub dims: u16, pub quant: Quant, pub chunker_version: u16 }

impl VectorIndex {
    pub fn open(io: &dyn DerivedIo, tag: ModelTag) -> Result<Self, EmbedError>;
    pub fn enqueue(&mut self, change: BlockChange);
    pub fn next_batch(&mut self, max: usize) -> Option<Batch>;      // chunk keys and input text, kind = Passage
    pub fn submit(&mut self, batch: BatchId, vectors: &[f32]) -> Result<(), EmbedError>; // CN-EMB-0003, 0004
    pub fn nearest(&self, query: &[f32], k: usize, filter: &Filter) -> Vec<Hit>;
    pub fn related_pages(&self, page: PageId, k: usize) -> Vec<Hit>;
}
```

The host's embedder takes text and a kind, `Query` or `Passage`, because some models want a different prefix for each. That prefix is the host adapter's job and is unverified until `SPK-06` reads each model card.

**Where the runtime lives.** A compiled inference runtime belongs in no crate under `crates/`. The web needs a separately built wasm module loaded only after the model is installed, in its own worker, and desktop needs the same runtime in the shell. [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) gives this no home (proposed row `FND-60`). Proposal: `webapp/embedder/` and `desktop/`, built from one shared source outside `crates/`.

**Throttling.** The host only calls `next_batch` when no typing has happened recently, and on desktop can pause on battery. Embedding has lower priority than the search index. Proposed idle pause: 30 seconds per changed block, with an initial build running continuously at low priority and pausing during typing (PERF-09).

#### Vector store

```
derived/vectors/<hash of model tag>/
  manifest.json   the model tag, count, chunker_version
  vectors.bin     fixed-width records of dims values
  chunks.bin      fixed-width: page ordinal, block id, ordinal, input hash
  live.bin        deletion bitmap
  pages.bin       one mean vector per page
```

- **Append only** with a deletion bitmap, compacted in budgeted steps when a large share is dead. A trailing partial record after a crash is ignored.
- **Tagged by model.** A query vector is only ever compared with vectors of the same tag. An index with a different tag is never read for results (`CN-EMB-0002`). Switching model keeps the old index working until the new one is complete, then deletes it.
- **Never synced, never exported** ([36](36-EXPORT-FORMAT.md)), so vectors can't conflict and encrypted sync is unaffected.
- **Rebuildable.** Deleting the folder loses nothing except compute time. A page vector is the length-weighted mean of its chunk vectors, normalised, recomputed when its chunks change.

**Size is arithmetic, not a measurement.** If a model has 384 dimensions (from memory of the model cards, unverified), 100,000 chunks take 38.4 MB at one byte per dimension or 153.6 MB at four. `SPK-06` measures whether 8-bit scalar quantisation costs meaningful quality. A workspace of 100,000 pages can hold far more than 100,000 blocks, which is why the store limit is stated separately below.

#### Nearest-neighbour search

**Brute force first.** Scan live vectors with a dot product and a top-k heap, ties broken by chunk key. It is exact, small, has no extra file to keep in step, and rebuilds trivially. Plain loops that the compiler can vectorise are used, built for wasm with SIMD enabled. No `cfg(target_arch)` is allowed in `crates/`, and hand-written intrinsics would need it.

**HNSW only if `SPK-06` shows brute force misses the target at a realistic size.** Candidate: `instant-distance`, Apache-2.0 per GitHub, pure Rust. Check first whether it supports incremental insertion and deterministic builds; both are unverified. An HNSW graph is an approximate, additional derived file, and then needs its own rebuild path.

#### How results reach the person

- **Search panel:** exact matches first, then a separate group labelled as matches by meaning. They are not blended into one ranked list. A person should always be able to tell whether their words are in the note.
- **Related pages:** cosine of page vectors above a threshold. The threshold belongs to the model and is in the catalogue. The prototype's `.06`, `.08` and `.12` are for its stand-in only.
- **Graph:** similarity edges come from `related_pages` ([61](61-KNOWLEDGE-GRAPH.md)).
- **Assistant and tools:** `search_notes` and `related_pages` ([63](63-AGENT-TOOLS-AND-MCP.md)).

#### Embedding candidates

Licences and sizes read from Hugging Face on 2026-11-07. **None has been run.**

| Model | Licence | Size | Position |
| --- | --- | --- | --- |
| paraphrase-multilingual-MiniLM-L12-v2 | Apache-2.0 | 118 MB, quantised | Default candidate. `SPK-06` picks between this and the next |
| multilingual-e5-small | MIT | 118 MB | Default candidate |
| bge-m3 | MIT | 2.27 GB | Desktop-only option |
| all-MiniLM-L6-v2 | Apache-2.0 | 23 MB | English only, so never the default (ADR-015). May be offered, labelled as such |
| jina-embeddings-v3 | CC-BY-NC | | **Rejected:** non-commercial licence |

#### Runtime candidates

| Runtime | Licence | Position |
| --- | --- | --- |
| `tract` | GitHub reports NOASSERTION, README says MIT or Apache. **Confirm from the licence files first** | Rust, targets wasm. Runs in both hosts if it performs, which also helps QR-3 |
| `candle` | Apache-2.0 or MIT | Rust, targets wasm. Same appeal |
| `fastembed-rs` | Apache-2.0, through `ort` | Desktop only, since ONNX Runtime is native. A different runtime on desktop could produce different vectors from the web |

Running one runtime in both hosts is preferred. `SPK-06` must report whether vectors differ between native and wasm builds of the same model.

### Part 3: optional OCR for printed text

Optional, and off by default (`OCR-01`, Phases 9 and 10). The text extracted from an image goes to the `ocr` field and is derived data. It is tagged by engine and language data, and is rebuildable, not synced and not exported. It is never written into the note.

**Tesseract** (Apache-2.0) and **tesseract-wasm** (BSD-2-Clause) are the candidates. Licences of the language data are not verified here. **`ocrs` code is Apache-2.0 but its models inherit CC BY-SA 4.0, so they are not bundled** ([12](12-OPEN-SOURCE-AUDIT.md)). Images are hostile input, so OCR runs in a worker with size limits. Handwriting recognition is `HWR-01`, with no permissive option found.

## Failure modes and limits

### Failure modes

| Failure | What happens | Code |
| --- | --- | --- |
| Index format or tokeniser version differs | Delete and rebuild, with progress shown. Notes are untouched | `CN-SCH-0001` |
| Index file fails its hash or parse | Same. Never a panic. One fuzz target (`GOV-07`) covers segments | `CN-SCH-0002` |
| Query over a limit | Refused, naming the limit | `CN-SCH-0003` |
| Too many query terms | Refused, naming the limit | `CN-SCH-0004` |
| Prefix expansion over the limit | Results returned, marked as partial | `CN-SCH-0005` |
| Index still building or changes pending | Results returned, with how far along | `CN-SCH-0006` |
| Block text over the indexed length | The start is indexed, and the page is marked as partly indexed | `CN-SCH-0007` |
| Meaning search asked for, no model installed | Exact search still answers; the UI offers the download | `CN-EMB-0001` |
| Index tag differs from the active model | Not read. Rebuild starts | `CN-EMB-0002` |
| Vector count or dimensions wrong from the host | Batch refused | `CN-EMB-0003` |
| Non-finite value in a vector | Batch refused, so one bad value cannot poison the top-k | `CN-EMB-0004` |
| Vector file damaged | Delete and rebuild | `CN-EMB-0005` |
| Embedder fails, crashes or runs out of memory | Embedding pauses, state shows it, exact search is unaffected | `CN-EMB-0006` |
| Vector store over its limit | Remaining chunks are not embedded, and the state says so | `CN-EMB-0007` |

Search and meaning search can always fail independently of each other, and neither can affect notes (rule 5 in [19](19-WORKSPACE-SCAFFOLD-DESIGN.md)).

### Limits

All are proposals for [21](21-PARSER-LIMITS.md).

| Limit | Value |
| --- | --- |
| Query length | 4 KiB |
| Query terms, after tokenising | 64 |
| Prefix expansions per term | 256 |
| Results per request, and total | 50, and 1,000 |
| Snippet length | 240 graphemes |
| Indexed text per block | 1 MiB |
| Term length, longer is cut | 64 graphemes |
| Chunks in a vector store | 1,000,000, pending `SPK-06` |
| Chunks embedded per block | 64 |

## Alternatives considered

| Option | Decision | Why |
| --- | --- | --- |
| **Tantivy** (MIT) | **Rejected** (ADR-009) | A maintainer calls browser use [out of scope](https://github.com/quickwit-oss/tantivy/issues/2855). Native-only would give different results on desktop and web, breaking QR-3 |
| A database's full-text feature in wasm | Rejected | A second storage engine beside ours, and its tokenisers would not meet the per-script rules. Not evaluated in depth |
| `Intl.Segmenter` | Rejected for the index | Varies by browser and version, is unavailable on native, and would break QR-3 |
| ICU4X segmenter | `SPK-05` candidate | Has data for these scripts. Its licence is Unicode-3.0 from memory, unverified, which is not on the ADR-023 list, so it would need a named exception (proposed row `FND-70`) |
| `jieba-rs`, `lindera` | `SPK-05` candidates | Licences and dictionary size unverified. Dictionaries may dominate the wasm size |
| Stripping all marks | Rejected | Breaks Hindi, and changes meaning in others (ADR-018) |
| Blending exact and meaning results, such as reciprocal rank fusion | Rejected for version 1 | Hides why a result appeared. Worth revisiting once there is a labelled set |
| USearch | Rejected for the engine | C++ core, which `-embed` cannot carry in wasm under our dependency policy |
| Storing vectors in sync to save compute | Rejected (ADR-015) | Conflicts, leaks, and ties sync to a model |
| Unigrams for all CJK | `SPK-05` baseline only | High recall, but long posting lists and poor precision |

## Acceptance gates

### `SPK-05`: our own search index

Decides ADR-009. Builds a throwaway `-search` and runs it in wasm on the reference machines from [30](30-PERFORMANCE-TARGETS.md).

**Corpus:** the Large workspace, 100,000 pages in mixed launch languages (`DEC-09`). **Query sets:** a labelled set per launch language in `fixtures/`, including right-to-left, Han, kana and Thai.

| Check | Pass | Row |
| --- | --- | --- |
| Query time in wasm | 95% of queries under 50 ms | PERF-06, QR-2 |
| Findability | Every labelled query in Chinese, Japanese, Korean, Thai, Arabic and Hindi finds its target, with and without invisible direction marks | PR-5 |
| Index size | Under 25% of the text size | PERF-07 |
| Typing | PERF-01 holds while indexing runs | PERF-09 |
| Same results | Native and wasm rank identically on every query. So do an index built incrementally and one rebuilt from scratch | QR-3 |
| Comparison recorded | Bigrams against at least one dictionary segmenter, on the sets above | |

**If it fails, narrow the scope.** In this order: drop secondary terms; drop `.fwd` for an old-text path; drop positions for word-class scripts, losing phrases there; index titles and recent pages fully and other pages on demand. Publish the reduced promise, and amend QR-2. Do not ship a target that was missed. Moving to a native-only index is not an option unless QR-3 is reopened.

### `SPK-06`: a real embedding model on the device

Decides ADR-015 and picks the model. Compares the two default candidates on `tract` and `candle` in wasm, and the desktop options natively.

| Check | Pass | Row |
| --- | --- | --- |
| Off the typing path | Embedding 100,000 blocks leaves PERF-01 intact throughout | PERF-09 |
| Query time | A query within 100 ms in wasm, reporting query embedding and vector scan separately | PERF-08 |
| Quality | Related pages beat the word-overlap baseline on a labelled set in at least English and Chinese. Include a query in one language with its target in another | EMB-02 |
| Quantisation | Quality with 8-bit vectors against 32-bit, reported | |
| Rebuild | Delete and rebuild gives the same results on the same build | EMB-01 |
| Decision recorded | The model, the runtime, its input budget, the thresholds and download size. Licence files confirmed | ADR-027 |

The labelled sets and their size are decided at the start of the spike and written down before running, so the result cannot be chosen to fit. **PERF-08 names the Large workspace (100,000 pages) while this spike names 100,000 blocks. These differ by the blocks per page** (proposed row `FND-63`). The spike runs at 100,000 blocks, reports its scaling, and states the largest size at which the target holds.

**If it fails.** Try HNSW if only scan time failed. If query embedding alone exceeds the target in wasm, meaning search stays desktop-only until that changes, with web keeping exact search. FR-10 is a should, so it can move without blocking a release.

### Standing gates

| Gate | Row |
| --- | --- |
| Per-language test sets pass, every push | `SRCH-01` |
| QR-2 benchmark, with a regression limit | `SRCH-02` |
| One fuzz target per index parser | `GOV-07` |
| Rebuild gives the same results | `EMB-01` |
| With AI off and no model installed, a network capture of a scripted session shows nothing sent (PR-6) | `AST-03` |

**QR-3 and vectors.** QR-3 and the Determinism job in [15](15-CI-AND-RELEASE-GATES.md) say "same search results" without separating exact from meaning search. This design promises identical results for exact search only. Vector results depend on the runtime and floating-point paths, and are not promised to match between native and wasm unless `SPK-06` shows it (proposed row `FND-62`).

## ADRs

ADR-009, ADR-015, ADR-018, ADR-023, ADR-027. Touches QR-2, QR-3. A secondary-term table and any segmenter dependency need recording in ADR-009 once decided.

## Tracker ids

`SPK-05`, `SPK-06`, `SRCH-01`, `SRCH-02`, `EMB-01`, `EMB-02`, `OCR-01`, `HWR-01`, `STO-03`, `GOV-07`, `I18N-05`. Proposed: `FND-60`, `FND-62`, `FND-63`, `FND-68`, `FND-70`.
