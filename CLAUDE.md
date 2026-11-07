# CLAUDE.md

Working rules for this repository. [AGENTS.md](AGENTS.md) holds the contract and applies to every agent.

## Owner instructions come first

**[docs/03](docs/03-OWNER-INSTRUCTIONS.md) records everything the owner has instructed.** Record each new instruction there in the same turn. If something seems wrong or conflicts, raise it rather than quietly choosing.

## One roadmap, one tracker

**[docs/06](docs/06-ROADMAP-AND-DELIVERY.md) is the only roadmap, and [docs/14](docs/14-EXECUTION-TRACKER.md) the only tracker.** No parallel plans. No work without a row. Read the id space first, because ids are never reused. Rules are in [docs/05](docs/05-PHASES-AND-TASKS.md). Only the owner closes a phase.

## Update every document

A change is not done until every document it affects is true, in the same commit.

| If you changed | Update |
| --- | --- |
| Status of any work | docs/14, and the phase checkbox in docs/06 |
| A crate or its dependencies | docs/19 |
| A decision | docs/08 and the owning design note |
| The model, ops or storage | docs/22, 24, 23 |
| The export format | docs/36, the schema, `tools/cnotepack.py`, its tests and the sample fixture |
| A threat, limit or error code | docs/07, 21, 20 |
| A user-visible string | The catalogue, per docs/50 |
| A new document | docs/00 |
| Phase state | AGENTS.md Current state, README Status |

## The cycle

**Design, execute, check, find issues, fix, commit, with tests.** Check means running it. Find issues asks what would have to be true for this to be wrong. **A test counts only if it was seen to fail without the change.** A worker's report is not evidence: revert the fix, watch the test go red, restore. Substantial designs are discussed first, and an ADR trigger needs an Accepted ADR.

## Engineering standards

**Rust.** `cargo fmt` and `clippy -D warnings` clean. `unsafe` forbidden in our crates. Toolchain pinned. Must compile to `wasm32-unknown-unknown`, with no `#[cfg]` platform forks in the engine. **Untrusted input never panics:** no `unwrap`, `expect`, unchecked indexing or arithmetic on values from a file. Errors are typed, with stable codes from docs/20. Messages carry counts and ids, never note text. Time and randomness are supplied. No new crate without a first consumer.

**Parsers.** Bounded, a hostile fixture per limit, a fuzz target, a refusal code.

**Multilingual.** Every string in the catalogue, logical CSS only, text stored exactly as written, tests in the launch languages.

**JavaScript** (`webapp/`). ES modules, no framework or bundler. `textContent` for document strings. Strict CSP. Every verb is a registered command.

**Python** (`tools/`). Standard library where possible. Tools fail non-zero and name the problem.

**Design.** Follow docs/49. No emoji icons, tutorial pages or decoration.

**Licences.** Apache, MIT, BSD, ISC, Zlib. MPL by named exception. No GPL or AGPL. Models and fonts count as dependencies.

**Git.** Conventional Commits citing tracker ids. **Work lands directly on `main`, with no pull requests for now** (instruction 10). Parallel workers use their own worktree branches, and the main session verifies and merges them. Never force-push `main` or skip hooks. Enable hooks with `git config core.hooksPath .githooks`.

## Gates today

```sh
python3 tools/check-docs.py
python3 tools/test_cnotepack.py
```

Full list: [docs/15](docs/15-CI-AND-RELEASE-GATES.md). Start reading at [docs/00](docs/00-README.md).
