# 15 — CI and release gates

CI is part of the architecture. Job names are a contract, and a change does not land until they pass. Adapted from OpenCalc's and OpenPresent's docs/15.

**Current state:** no repository, so no CI (`GOV-01`, `GOV-03`). Today `tools/check-docs.py` and `tools/test_cnotepack.py` run by hand. Everything else below is the target.

## Test levels

| Level | Where | Proves | Runs |
| --- | --- | --- | --- |
| Unit | Beside the code | One function, including each refusal | Every push |
| Integration | `crates/*/tests/` | A crate's public surface | Every push |
| Property | `-merge`, `-ops`, `-model` | Convergence and repair on generated histories | Every push, longer nightly |
| Crash | `-store` | No acknowledged edit is lost when killed at any step (PR-1) | Every push, longer nightly |
| Fault injection | `-sync` | Delay, drop, duplicate, reorder and tamper (PR-1, PR-2) | Nightly |
| Hostile | `fixtures/hostile/` | Every `SEC-*` and every limit in [21](21-PARSER-LIMITS.md) has an input that crosses it | Every push |
| Fuzz | `fuzz/`, one target per parser | Nothing panics or over-allocates | Build per push, campaign weekly |
| Walk-away | `tools/cnotepack.py` on engine output | A package is readable with none of our code (PR-3) | Every push |
| Determinism | Native and wasm | Same bytes and same search results | Every push |
| Browser | Chrome, Safari, Firefox | Editing, input methods, right-to-left, shortcuts | Every push |
| Language | Per launch language | Pseudo-locale, Arabic run, search sets, composition (PR-5) | Every push |
| Accessibility | Automated checks plus a scripted screen reader | QR-5 | Every push. Manual audit per release |
| Performance | [30](30-PERFORMANCE-TARGETS.md) | Budgets, against committed baselines (PR-4) | Every push on a named runner |
| AI safety | Prompt-injection fixtures with a fake and a real provider | No unapproved write (PR-6) | Every push with fake, nightly with real |
| AI off | Network capture | With AI off, no outbound request (PR-6) | Every push |
| Repository | `tools/check-*.py` | The documents and rules hold | Every push, pre-commit |

## Jobs

| Job | Does | Phase |
| --- | --- | --- |
| `checks` | `check-docs.py`, `check-crate-deps.py`, `check-host-seams.py`, `check-css-logical.py`, `check-i18n.py`, `check-dom-text.py` | 0 |
| `fmt-clippy` | `cargo fmt --check`, `clippy -D warnings` | 0 |
| `test` | Unit, integration, property and hostile on Linux, macOS, Windows | 0 |
| `wasm` | `cargo check --target wasm32-unknown-unknown` and determinism | 0 |
| `licence` | `cargo deny`, `cargo audit`, weights and fonts list | 0 |
| `fuzz-build` | Every fuzz target builds | 0 |
| `reader` | `python3 tools/test_cnotepack.py`, reader on the engine's output, schema check | 1 |
| `browser` | Interaction runs in three browsers | 2 |
| `language` | Language level above | 2 |
| `a11y` | Accessibility level above | 2 |
| `perf` | Performance level above | 2 |
| `ai-safety` | AI levels above | 5 |
| `mcp` | Client conformance, root-escape tests | 5 |
| `desktop` | Bundle on three platforms | 6 |
| `sync-faults` | Fault injection, nightly | 7 |
| `server` | Two browsers against the real server | 8 |

**The owner has no Windows machine** (`DEC-07`), so Windows confidence rests on CI runners until a release candidate.

## Gates proposed, and what each refuses

| Gate | Refuses |
| --- | --- |
| `check-docs.py` | Broken links; duplicate or unknown tracker ids; a doc missing from the index; unknown ADR or error code; the word "lossless"; a second tracker or roadmap |
| `check-crate-deps.py` | An upward dependency; any crate but `-merge` naming `loro`; `crates/` depending on `server/` |
| `check-host-seams.py` | `cfg(target_*)` in `crates/` |
| `check-css-logical.py` | Physical CSS direction properties in the app |
| `check-i18n.py` | A user-visible literal outside the catalogue; a missing key; string concatenation of messages |
| `check-dom-text.py` | `innerHTML` and similar on document data |
| `check-secrets.py` | A seeded key found in any export, log or sync output |

Each gate has a seeded violation seen failing, as OpenCalc's `check-gate-selftest.py` proves.

## Release gates

A release does not ship unless all of these pass:

1. All six promises in [01](01-ORD.md) are green, on the release commit.
2. Signed installers on three platforms, installed fresh and upgraded from the last release.
3. A package from every earlier version still imports.
4. SBOM published, with code, model and font licences.
5. Notes on anything preserved but not modelled, and known limits, in `CHANGELOG.md`.
6. For the first public release: `DEC-01` and `DEC-08`, and for any encryption claim, `CRY-02`.

## Commands today

```sh
python3 tools/check-docs.py
python3 tools/check-docs.py --selftest
python3 tools/test_cnotepack.py
python3 tools/cnotepack.py verify fixtures/sample-workspace.cnotepack
```
