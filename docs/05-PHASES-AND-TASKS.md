# 05 — What a phase and a task are

Adapted from OpenPresent's docs/05. This is the rule for opening and closing work. [06](06-ROADMAP-AND-DELIVERY.md) applies it, and [14](14-EXECUTION-TRACKER.md) records the state.

## Levels

| Level | What it is | Defined by |
| --- | --- | --- |
| **Phase** | A capability a person or host gains | Its charter in 06 |
| **Prototype** (`SPK-*`) | A short experiment that settles an open decision | A row with a pass and a fail outcome |
| **Task** | One reviewable change with one acceptance test | A row in 14 |

## Phase lifecycle

```
Not opened ──entry met──▶ Open ──exit criteria met, owner signs──▶ Closed
                           │  ▲
                           ▼  │
                         Blocked
```

- **Entry criteria** say what must be true to start.
- **Exit criteria** must each be checkable by someone who did not do the work, by running a command or reading a named file. "Mostly done" is not a criterion.
- **Only the owner closes a phase.** An agent assembles the evidence and proposes closure.
- **Waivers.** Starting early is sometimes right. A waiver names the unmet criterion, the risk, and who accepted it. It never removes an exit criterion.
- **Reopening.** If a closed phase's gate later fails, it reopens with a row saying what regressed.

## Task lifecycle

| Status | Meaning | Needed to enter |
| --- | --- | --- |
| `Open` | Defined, not started | Id, scope, an acceptance test stated in advance |
| `In progress` | Being worked | An owner. Partial work stays here, and the row says what remains |
| `Blocked` | Cannot proceed | The blocking id or decision is named |
| `Done` | Proven | All of the Definition of Done |

**A prototype's result is not product code.** Clickable-prototype rows are `Done` when the prototype does the thing. The product feature has its own row.

## Definition of Done

1. **The acceptance test was seen to fail without the change,** and pass with it.
2. **Someone other than the author checks it by running it.** A worker's report is not evidence.
3. All local gates pass, and once CI exists, CI is green.
4. For parsers: limits have hostile fixtures, and a fuzz target exists.
5. For anything people read: every string goes through translation, and it works right-to-left ([50](50-INTERNATIONALIZATION.md)).
6. Documents changed in the same change agree with the code.
7. Anything found and not fixed has become a new row.

## What this forbids

- Starting work with no row.
- Marking `Done` on a report, or on a test never seen red.
- Closing a phase with an unmet criterion.
- Changing an exit criterion to fit what was built, without the owner.
- Claiming a number as measured when it was not.
