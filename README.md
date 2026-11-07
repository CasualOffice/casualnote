# CasualNote

**A multilingual, local-first workspace for documents, databases, canvases and pen notes, with a knowledge graph and an optional AI assistant you control. Apache-2.0 all the way through.**

> **Early, and it runs.** A Rust engine and a web editor work today. Databases, canvas, the knowledge graph, the assistant, sync and the desktop app are not built yet. See [Status](#status).

## Why

- **Permissive throughout.** Engine, sync server and apps under one licence, with no enterprise edition or licence key.
- **Multilingual first.** Right-to-left, input methods and every script are requirements from the first commit.
- **AI on your terms.** Your own provider on the web, a downloaded local model on desktop, and off by default. It never changes a note without asking.
- **Sync you can trust.** Encrypted through storage you already have, with visible state, and overlapping edits marked for review.
- **Walk away any time.** A [published export format](docs/36-EXPORT-FORMAT.md) with Markdown inside, and a reader that shares no code with the app.

## Status

| Area | State |
| --- | --- |
| Requirements, architecture, low-level designs, threat model, roadmap, tracker | Written, awaiting owner review |
| Clickable prototype | Works in a browser. No engine. Sync, provider connection and model download are previews |
| Export format, version 1 | Draft spec and schema |
| Standalone package reader | Works, with tests |
| Engine: pages, blocks, undo, merging | **Works, with tests** |
| Web editor: blocks, slash menu, marks, search, six interface languages, saves in the browser | **Works, tested in Chrome** |
| Databases, canvas, links, knowledge graph, assistant, MCP, package export from the engine | Next |
| Desktop app, sync, server | Not started |

Nothing has been measured. Performance figures in the docs are targets.

## Run it

```sh
tools/run-web.sh
```

Then open <http://127.0.0.1:8741/>. Needs Rust and wasm-pack.

## Check it

```sh
cargo test --workspace
node tools/e2e.mjs
python3 tools/cnotepack.py verify fixtures/sample-workspace.cnotepack
python3 tools/cnotepack.py extract fixtures/sample-workspace.cnotepack ./out
python3 tools/test_cnotepack.py
python3 tools/check-docs.py
```

Open `docs/prototypes/casualnote-prototype.html` in a browser for the prototype. The assistant in it only works when hosted on claude.ai.

## Documentation

Start at the [index](docs/00-README.md). The most useful pages: [requirements](docs/01-ORD.md), [architecture](docs/02-ARCHITECTURE.md), [roadmap](docs/06-ROADMAP-AND-DELIVERY.md), [tracker](docs/14-EXECUTION-TRACKER.md).

## Family

Part of [CasualOffice](https://github.com/CasualOffice), with [OpenCalc](https://github.com/CasualOffice/opencalc), whose architecture it follows, and OpenPresent.

## Licence

[Apache-2.0](LICENSE). See [NOTICE](NOTICE). The licence grants no trademark rights.
