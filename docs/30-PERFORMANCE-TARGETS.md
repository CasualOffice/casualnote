# 30 — Performance targets

**Nothing here is measured.** Every number is a proposal to be tested by a prototype or benchmark, and may change with evidence. Once measured, a target records the machine and commit. **The web path sets the budget;** native must produce identical results and may be faster.

## Reference machines

To be named in `SPK-02`: a mid-range laptop and an old laptop, both on the Chrome, Safari and Firefox current at the time. The owner's Mac is available. Windows waits on `DEC-07`.

## Reference workspaces

| Name | Shape |
| --- | --- |
| Small | 100 pages, a few thousand blocks |
| Large | 100,000 pages, in mixed languages |
| Long page | One page with 5,000 blocks |
| Old history | A simulated 5 years from 3 devices |
| Ink page | A canvas with 20,000 strokes |

## Targets

| ID | What | Target | Workspace | Tested by |
| --- | --- | --- | --- | --- |
| PERF-01 | Input event to the end of main-thread work, 95th percentile (QR-1, PR-4). Measured this way because a 60 Hz refresh alone can use 16 ms. The share shown in the first frame is reported alongside | 16 ms | Long page | `SPK-02` |
| PERF-02 | Durable write of one edit, 95th percentile | Under 16 ms on native; browser to be measured | Long page | `STO-01` |
| PERF-03 | Open a typical page | 100 ms | Old history | `SPK-01` |
| PERF-04 | Open the Long page to interactive | 500 ms | Long page | `SPK-02` |
| PERF-05 | Storage of history | Under 10 times the text and strokes | Old history | `SPK-01` |
| PERF-06 | Search, 95th percentile (QR-2) | 50 ms in wasm | Large | `SPK-05` |
| PERF-07 | Search index size | Under 25% of text | Large | `SPK-05` |
| PERF-08 | Query by meaning | 100 ms in wasm | 100,000 blocks. Size at 100,000 pages is open, `FND-63` | `SPK-06` |
| PERF-09 | Indexing and embedding | Never on the typing path; PERF-01 holds while they run | Large | `SPK-06` |
| PERF-10 | Graph view open | 500 ms | Small; Large by filtering | `GRF-02` |
| PERF-11 | Canvas pan and zoom | 60 frames per second | Ink page | `CNV-01` |
| PERF-12 | Pen latency | Judged on devices; no number until `SPK-03` | Ink page | `SPK-03` |
| PERF-13 | Package export | Linear in size, off the main thread | Large | `PKG-05` |
| PERF-14 | Web app first load, cold | Measured and budgeted once the wasm size is known. Loro's published wasm is about 3.6 MB on its own | Small | `WEB-01` |
| PERF-15 | Desktop memory, idle, no model | Measured against an Electron peer, to test the "light desktop app" claim | Small | `DSK-01` |
| PERF-16 | Local model first reply | No target until `SPK-07` | Small | `SPK-07` |

## How regressions are caught

As OpenCalc: versioned JSON reports, committed baselines per named machine, and a regression threshold in CI. A claim such as "fast" or "light" cannot appear in public text without a row here with a measured result.

## Measured, 2026-11-08

Apple M4, 16 GB, Chrome only, on a loaded machine and uncommitted code based on `7d473f0`. Treat as indicative.

| Measure | Target | Result | State |
| --- | --- | --- | --- |
| Typing, 5,000-block page, p95 | 16 ms | 174 ms; 89 ms with fixes not applied; 11.7 ms at 300 blocks | Fail |
| Opening that page | | 2.7 to 3.5 s | |
| Search, 100,000 pages, wasm, p95 | 50 ms | 223 ms | Fail |
| 5-year history, 3 devices | No panic | Loro panics on snapshot import, 2 of 2 seeds | Fail |
| Hostile imports | No panic | 0 of 3,000 | Pass |
| First load | | 0.46 to 1.4 s, wasm 4.28 MB | |
| Meaning search, MiniLM on tract in wasm | | 57 ms query, 41 ms scan | |
| Log write per keystroke, dev build | | 168 bytes, 23 to 54 ms in the worker | |

Not measured: load time, size against content and memory at 5 years; other browsers; an idle machine.
