# 61 — Knowledge graph

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) has a stand-in: `buildGraph` scans every page on demand, `settle` lays it out, and its "similar" edges come from a word-overlap vector rather than a model. Its behaviour informed this design. Its cost does not scale, as shown below.

## Outcome

A person can see how their notes connect, in any language, and always tell a connection they made from one the computer guessed. The graph cannot disagree with the notes, because it is computed from them (ADR-014).

Requirements: FR-11, PERF-10. Crate: `-graph` ([19](19-WORKSPACE-SCAFFOLD-DESIGN.md)), which depends on `-model` and `-embed`, reads and never writes, and holds no stored graph.

## Design

### Node kinds

Node ids are strings, stable across rebuilds and independent of language.

| Kind | Id | Label | Comes from |
| --- | --- | --- | --- |
| Page | `page:<page id>` | The title as written. An empty title shows the localised word for "Untitled" and stays empty in data | Each live page, including database pages |
| Tag | `tag:<tag key>` | The most used spelling | `#` tags in text, below |
| Database row | `row:<row id>` | Row name | A named row of a database hosted on a page |
| Person | `person:<person key>` | The name as written | A person value on a row. Key rule as tags |

Pages in the trash are not nodes. A row with an empty name is not a node, as in the prototype.

**What "person" means needs the data model.** [36](36-EXPORT-FORMAT.md) has text columns such as `owner` and no person type, and the prototype treats `owner` as a person by its column name, which cannot work in other languages. So a person node comes only from a column explicitly typed as person in [22](22-DATA-MODEL.md), or from an explicit mention if the model adds one. Until then, FR-11's person nodes have no source (proposed row `FND-64`). There is no account or identity behind a person in version 1: `Mei` and `mei` are one node, and two people called Mei are also one node.

### Edge types

Direction is `from` to `to`. Forward and reverse labels are message ids, translated in [50](50-INTERNATIONALIZATION.md). The English below is the prototype's `REL` table.

| Type | From to | Forward | Reverse | Origin | Derived from |
| --- | --- | --- | --- | --- | --- |
| `link` | page to page | links to | linked from | Made | Inline page link in a block, row or canvas item |
| `embed` | page to page | embeds | embedded in | Made | `embed` block. Also a `database` block pointing at a database hosted on another page |
| `child` | page to page | has subpage | subpage of | Made | Page tree parent |
| `tag` | page to tag | tagged | tag on | Made | Tag in the page's text |
| `row` | page to row | contains | row in | Made | Row of a database on that page |
| `owner` | row to person | involves | owner or author of | Made | Person value on the row |
| `similar` | page to page, **undirected** | similar to | similar to | **Computed** | Nearest neighbours from [60](60-SEARCH-AND-EMBEDDINGS.md) |

Between any pair, at most one edge per type, so ten links from A to B are one edge with `count: 10`. A self-link is dropped. A link to a removed page is not an edge, and it counts as a `dangling` figure on the page's details, matching [36](36-EXPORT-FORMAT.md), which keeps such links and shows them as removed.

### Similarity edges are always visibly different

This is a rule, not a style choice. The type system carries it:

```rust
pub enum Origin { Made, Computed { score: Score, model: ModelTagId } }
pub struct Edge { pub from: NodeId, pub to: NodeId, pub kind: EdgeKind, pub origin: Origin, pub count: u32 }
```

| Where | Rule |
| --- | --- |
| Graph view | Computed edges are dashed and thin, and never depend on colour alone (QR-5). The legend always explains them. One filter turns them all off |
| Details panel | Listed in a separate group: "Similar, computed", with the score, never mixed with links |
| Backlinks (FR-03) | Never include computed edges |
| Tool results | Each connection carries `"origin": "made"` or `"computed"`, and a score for computed ([63](63-AGENT-TOOLS-AND-MCP.md)) |
| Screen readers | The text alternative says "similar, computed" in words |
| Promotion | A person can choose "Add link", which runs an ordinary op and creates a real `link`. A computed edge is never written into the notes on its own |
| Pair rule | No `similar` edge is drawn when a made edge already joins the pair, as in the prototype |
| No model installed | No computed edges at all. The graph is complete without them, and the filter explains why it is empty |

### Multilingual tags

**Syntax.** `#` followed by letters, marks, numbers, `_`, `/` or `-` in any script, the same character classes as the prototype's regular expression. The `#` must follow the start of text, whitespace or punctuation. Text in `code` blocks and code marks is not scanned. Fullwidth `＃` is accepted, because Chinese and Japanese input methods produce it. The note is never rewritten to use the other form.

**The end of a tag in unspaced text.** `#标签之后继续写` would take the whole run as a tag, since Chinese has no space to end it. So a closing `#` also ends a tag, as in `#标签#之后继续写`, a form already familiar to Chinese users. This is a proposal, to be checked with native reviewers (`I18N-05`). A cleaner alternative is an explicit tag chip in the editor, if [22](22-DATA-MODEL.md) and [42](42-EDITOR-AND-RENDERING.md) add one. [36](36-EXPORT-FORMAT.md) has no tag field, so text remains the source for now (proposed row `FND-64`).

**Identity.** The key is the tag text after NFC and full case folding, used for matching only. Marks are kept, as in [60](60-SEARCH-AND-EMBEDDINGS.md). The label is the spelling used most, ties broken by code point order, so the choice is deterministic.

**Tags are never translated or merged across languages.** `#import` and `#导入` are two nodes. Aliases are a possible later feature that would have to be stored as data, so they are out of this version. `a/b` is one flat tag whose label contains a slash. Nested tags are not modelled.

### Derivation

Derivation is a pure function of the notes, so golden tests can compare exact output.

```rust
pub struct PageFacts {            // everything one page contributes
    pub page: PageId, pub version: PageVersion, pub title: String, pub parent: Option<PageId>,
    pub links: Vec<(PageId, u32)>, pub embeds: Vec<PageId>, pub tags: Vec<TagUse>,
    pub rows: Vec<RowFacts>, pub dangling: u32,
}
pub fn page_facts(page: &Page, dbs: &DatabaseView) -> PageFacts;
pub fn assemble(facts: impl Iterator<Item = PageFacts>) -> Graph;   // sorted nodes and edges
pub fn similar_edges(graph: &Graph, within: &NodeSet, source: &dyn SimilarSource, policy: &SimilarPolicy) -> Vec<Edge>;
pub fn neighbours(graph: &Graph, node: &NodeId, depth: u8, filter: &Filter) -> Subgraph;
```

1. `page_facts` walks one page's blocks, canvas items and hosted database. It is linear in the page's size.
2. `assemble` adds nodes, then edges. A database block points at its host page if another page hosts that database, which gives `embed`, as in the prototype.
3. `similar_edges` runs last and only within the set being shown. For each page it takes the top neighbours above the model's threshold, skipping pairs already joined. Proposed: 2 per page, as in the prototype. **It takes a filter so it can never compute all pairs of a big workspace.** The prototype's loop over all pages is fine for its sample and would not survive 100,000 pages.
4. Nodes and edges are sorted by id and type. That order is the graph's only order, so output is byte-stable.

A graph needs no view to exist. The tools use the same functions without a layout.

### Caching and invalidation

**Derived on read,** as ADR-014 says, does not have to mean re-reading every page on every read. There is an in-memory cache, in the same worker as the search index, off the typing path.

| Layer | Key | Invalidated by |
| --- | --- | --- |
| Page facts | Page id and page version | That page's change event: replace only that page's facts |
| Assembled graph | `graph_version`, a counter raised on any fact change | Any fact change. Rebuilt on the next read only, never eagerly |
| Similar edges | `graph_version` of the set, model tag and vector version | New vectors, model switch, or a set change |
| Layout | `graph_version` and filter | Recomputed when the view asks |

An invalidated layer is rebuilt on next read, so a person typing with the graph closed pays only for replacing facts. Remote merges arrive the same way, through the same change events. Nothing is written to disk.

**Cold start is the real cost.** The first read after opening must have facts for every page. In the Large workspace that means projecting up to 100,000 page documents, and nobody has measured that. The cold scan runs in the background, in steps with budgets. The view opens with what is ready, filtered, and says that it is still reading. `GRF-02` measures it. **If the scan is too slow,** the fix is a persisted facts file, as rebuildable as the search index. That would conflict with "must never own any stored graph" in [19](19-WORKSPACE-SCAFFOLD-DESIGN.md) and with ADR-014's wording, so it needs a new ADR. The principle would still hold: a file you can delete, never authoritative.

**Backlinks** (FR-03, Phase 2) arrive two phases before this crate (Phase 4). They read the same link facts, so [22](22-DATA-MODEL.md) should define link extraction in `-model`, where both can use it.

### Layout

A force-directed layout that is deterministic. **The same graph and filter give the same positions on any device,** so golden tests can compare them and a screenshot is reproducible.

```rust
pub struct LayoutParams { /* per-edge-type spring length and strength, repulsion, gravity, iterations */ }
pub struct Layout { /* ... */ }
impl Layout {
    pub fn new(graph: &Subgraph, params: &LayoutParams) -> Self;
    pub fn step(&mut self, iterations: u32) -> LayoutState;   // MoreWork, Settled, IterationLimit
    pub fn positions(&self) -> &[(NodeId, Point)];
    pub fn pin(&mut self, node: &NodeId, at: Point);
}
```

- **Start positions.** Nodes sorted by id, placed on a golden-angle spiral. No random numbers and no clock.
- **Forces.** Repulsion between nodes, a spring per edge, weak gravity to the centre, velocity damping, and cooling across iterations. **Spring lengths start from the prototype's `settle` table,** where `row` is shortest and `similar` is longest and weakest, so a computed edge barely pulls the shape of what people made. All values are the prototype's guesses. `GRF-02` tunes them on real graphs.
- **Scale.** Repulsion is exact pairs up to a small graph, then a Barnes-Hut tree. The prototype's pair loop is quadratic.
- **Number rules.** `f64` with pure Rust maths for anything beyond square root, no parallel steps that could change order, and positions rounded when returned. Native and wasm can then be compared, as with search ranking.
- **Off the main thread.** The layout runs in the worker in budgeted steps. The view draws positions as they arrive. With reduced motion set, it shows only the settled result.
- **Warm starts break purity, on purpose and only in a session.** When a graph changes under an open view, a warm start from current positions avoids a jarring jump. Golden tests only cover cold starts. A dragged node is pinned for the session and may be kept per device as view state, never in the notes, never synced.

### The views

**Whole graph** (`GRF-02`). The filters in the prototype: kinds, computed edges and a text query. Canvas drawing follows [49](49-DESIGN-SYSTEM.md) and [47](47-UX-AND-FEATURE-MAP.md). Labels use the browser's text shaping, so any script and direction draw correctly, and a right-to-left title is drawn as an isolated run.

**Local graph.** A side panel for the open page. It shows the neighbours within a depth of 1 by default, with a choice of up to 3, ordered with made edges before computed edges. It shows at most a small number of nodes (the prototype uses 14), with "and N more" linking to the whole graph. It reads from the same cache and adds nothing to the page's open time, since it loads after the page paints.

**Text alternative (QR-5).** Every graph view has a list form: node, kind and connections grouped by relation, keyboard navigable and usable by screen readers. A canvas alone would not meet WCAG 2.2 AA. A graph must not be the only route to any information: everything on it is also on the page's details.

### Why no separate store

- **It cannot go wrong.** A stored graph is a second copy of links and tags, and it can disagree with the notes after a crash, a merge or a bug. A derived one cannot.
- **Merging stays in one place.** A CRDT merges the notes, and a stored graph would need its own merge or its own repair (principle 3 in [02](02-ARCHITECTURE.md)).
- **Encrypted sync is unaffected.** Nothing extra leaves the device.
- **Export is simple.** The graph is not in the package and is rebuilt on import ([36](36-EXPORT-FORMAT.md)).
- **Schema changes are free.** A new edge type needs code, not a migration.

The price is the cold scan above. A graph database, with its query language and paths across many hops, is also rejected: nothing in the product needs more than a depth of 3.

### Future typed relations

Today's edges have fixed types, and people cannot say "A contradicts B". A later version could add typed relations as data: a database column of type relation, or a page property with a label the person writes. That would be stored as ordinary model data through `-ops`, exported in a minor version of the package, and merged as any other value. In this design it stays derived: it becomes `EdgeKind::Relation(RelationId)` with `Origin::Made` and the person's own label, in their own language, shown untranslated. No graph store is needed for it. It needs an ADR and a [22](22-DATA-MODEL.md) change first, and is not in the first version.

## Failure modes and limits

### Failure modes

| Failure | What happens | Code |
| --- | --- | --- |
| Graph larger than the draw limit | Draw the top nodes by made-edge degree, ties by id. The view states "Showing N of M" and offers filters. Never silently partial | `CN-GRF-0001` |
| Tool asks for an unknown node | Refused, naming what was searched | `CN-GRF-0002` |
| Name matches several nodes | Refused, with candidates | `CN-GRF-0003` |
| Layout hits its iteration limit | Draw the positions reached, stated in the view's state | `CN-GRF-0004` |
| Depth over the limit | Refused, naming the limit | `CN-GRF-0005` |
| Embedding unavailable or still building | Made edges are complete, and computed edges are missing with a note | `CN-EMB-0001` |
| Cold scan unfinished | Partial graph, marked as such, with progress | `CN-GRF-0006` |
| A page's projection fails | That page is omitted and reported on the view. The graph otherwise continues | `CN-GRF-0007` |

The graph never writes, so none of these can affect notes.

### Limits

Proposals for [21](21-PARSER-LIMITS.md) and [30](30-PERFORMANCE-TARGETS.md). PERF-10 asks for the view open within 500 ms on the Small workspace, and on Large only through filtering.

| Limit | Value |
| --- | --- |
| Nodes drawn | 2,000 |
| Edges drawn | 10,000 |
| Local graph depth | 3 |
| Local graph nodes in the panel | 14 before "and N more" |
| Similar edges per page | 2 |
| Pages per `similar_edges` call | The nodes being drawn, never the whole workspace |
| Connections per tool result | 100, truncation stated |
| Tags per page counted | 1,000 |
| Tag length | 128 graphemes |

## Alternatives considered

| Option | Decision | Why |
| --- | --- | --- |
| A stored graph or graph database | Rejected (ADR-014) | See above |
| Similarity as the same edge in another colour | Rejected | Confuses guesses with facts, and fails for colour-blind readers and screen readers |
| Promoting strong similarity to a link automatically | Rejected | Writes the computer's guess into a person's notes, against PR-6 |
| Random seed for layout | Rejected | Not testable, and the shape changes every visit |
| Precomputing all similar pairs | Rejected | Quadratic in pages. Filtered to what is shown |
| WebGL or a graph library in the view | Deferred | A dependency before need. Canvas first, measured by `GRF-02`. A plain-module library would still have to obey ADR-025 |
| Person identity from accounts | Deferred | Single user first, [57](57-SERVER-DESIGN.md) comes later |
| Tag translation or merging across languages | Rejected | Guessing changes meaning. Aliases would be explicit, if ever |

## Acceptance gates

| Gate | Row |
| --- | --- |
| Golden graphs from fixture notebooks in the launch languages, byte-stable, with nodes and edges exactly as listed | `GRF-01` |
| Rebuild from the notes after discarding every cache gives the same graph | `GRF-01` |
| Incremental result after a generated edit sequence equals a fresh derivation | `GRF-01` |
| Native and wasm give the same graph and the same cold layout | `GRF-01`, QR-3 |
| Tag fixtures: Arabic, Hindi, Chinese with and without the closing `#`, fullwidth `＃`, mixed case, combining marks, tags inside code | `GRF-01`, PR-5 |
| Every computed edge has `Origin::Computed`, and none reaches backlinks | `GRF-01` |
| Without a model, no computed edge, and the graph is otherwise identical | `GRF-01` |
| View tests: filters, local graph, keyboard, screen-reader list form, and Arabic | `GRF-02`, QR-5 |
| PERF-10 on Small. Cold-scan time on Large measured and recorded, and the persisted-facts decision made from it | `GRF-02` |
| Typing stays within PERF-01 while facts refresh and layout runs | `GRF-02`, PERF-09 |

## ADRs

ADR-014 and ADR-015. Typed relations and any persisted facts file would need new ADRs.

## Tracker ids

`GRF-01`, `GRF-02`, `EMB-02`, `WEB-05`, `I18N-05`. Proposed: `FND-64`.
