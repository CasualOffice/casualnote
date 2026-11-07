# 17 — Glossary

| Term | Meaning here |
| --- | --- |
| **Acknowledged edit** | An edit durably on this device's disk, and shown as saved. PR-1 protects these. Not the same as a device acknowledgement |
| **Device acknowledgement** | A device's record of which other devices' changes it has received. Trimming depends on these |
| **Approval card** | The prompt shown before an assistant's write, built from the tool's arguments |
| **Block** | One unit of a page: paragraph, heading, list item, to-do, database reference and so on |
| **Canvas view** | The same page's content on a free surface, alongside items and ink |
| **Compaction** | Rewriting a page's log into a snapshot, without losing what other devices still need |
| **CRDT** | A data structure that merges concurrent edits without a central order. Here, Loro behind `-merge` |
| **Database** | Rows and columns with table and board views, stored apart from any one page |
| **Derived data** | Anything rebuildable from the notes: search index, vectors, graph, readable mirror |
| **Device registry** | The list of a notebook's devices and what each has acknowledged |
| **E2EE** | End-to-end encrypted: the storage or server holds only ciphertext |
| **Embed** | A block that shows another page live |
| **Embedding** | A vector computed from text by a model, used for search by meaning |
| **Host** | What runs the engine and supplies I/O, clock, network and AI: web app, desktop, server, MCP |
| **Host seam** | A value or small interface through which a host supplies a capability |
| **Ink** | Pen strokes, kept as raw samples |
| **Inverse** | The operation that undoes another |
| **Knowledge graph** | Nodes and edges derived from pages, tags, rows and people |
| **Local model** | An AI model downloaded and run on the user's own computer |
| **Managed mode** | A server workspace where the server can read content |
| **MCP** | Model Context Protocol, how external AI clients use our tools |
| **Mirror** | Readable Markdown and JSON regenerated from the notes, never read back |
| **Notebook** | The storage: the live, synced form on disk (`.cnote`). One notebook holds exactly one workspace |
| **Op** | One validated change in the closed operation set |
| **Overlap** | Two devices changing the same block without seeing each other's edit. Triggers merge review |
| **Package** | The stable export file (`.cnotepack`) |
| **Page view** | The same page's content as a flowing document |
| **Projection** | Turning merged CRDT state into valid model state. Repairs are never written back |
| **Provider** | The service or runtime that answers the assistant: a user's API, or local llama.cpp |
| **Pseudo-locale** | A fake language used in tests to reveal untranslated strings |
| **Segment** | A sealed, append-only file of changes written by one device |
| **Shallow snapshot** | Loro's trimmed state. Peers can sync only from versions after its point |
| **Tool** | One function in the closed set the assistant and MCP clients can call |
| **Trimming** | Discarding old history, never past what the oldest device has seen |
| **Workspace** | The content: pages, databases and settings. Held in one notebook |
