# Contributing

Adapted from OpenPresent's `CONTRIBUTING.md`. **The project is at the design stage, and is not yet taking code contributions.** Design feedback is welcome once a repository is public.

Read [AGENTS.md](AGENTS.md) and [CLAUDE.md](CLAUDE.md) first.

## Before you implement

- [ ] The work has a row in [docs/14](docs/14-EXECUTION-TRACKER.md).
- [ ] The outcome is in a numbered design note, indexed in [docs/00](docs/00-README.md).
- [ ] An ADR trigger in [docs/08](docs/08-ADR-REGISTER.md) has an Accepted ADR.
- [ ] The acceptance test is named, and it can fail.
- [ ] Code that reads a file is bounded, with a hostile fixture per limit, a fuzz target and a code from [docs/20](docs/20-ERROR-CODE-REGISTRY.md).

## While you implement

- One coherent change per commit.
- Every edit is an operation with an inverse.
- A test counts only if seen to fail without the change. A bug fix starts with the failing test.
- Every string goes through the catalogue, and works right-to-left.
- Never read the source of a copyleft or source-available project for design ([docs/12](docs/12-OPEN-SOURCE-AUDIT.md)).

## Before you commit

```sh
python3 tools/check-docs.py
python3 tools/test_cnotepack.py
```

Once Rust exists, add the jobs in [docs/15](docs/15-CI-AND-RELEASE-GATES.md).

## Licensing of contributions

Contributions are under Apache-2.0, inbound as outbound, with a DCO sign-off (`git commit -s`) rather than a CLA. The DCO itself is proposed, and has not been separately confirmed by the owner.
