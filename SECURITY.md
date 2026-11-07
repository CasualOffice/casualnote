# Security

**CasualNote is at the design stage, with no released product,** so no version is supported yet. The threat model is in [docs/07](docs/07-SECURITY-THREAT-MODEL.md).

## Reporting

Once the repository is public, report privately through GitHub's security advisories on the repository. Please do not open a public issue. Include steps to reproduce, and leave out any real notes or keys. This page will name a response time before the first release.

## In scope

The standalone package reader today. Later: the engine, apps, sync, server, MCP server, and how the assistant handles note content.

## What we promise

- Every file and remote update is treated as hostile.
- No code runs from a notebook or package.
- Nothing is fetched automatically because of note content.
- API keys never appear in exports, sync, logs or the mirror.
- We make no public encryption claim before an outside review.

## Known limits

- Encrypted sync does not hide file sizes, counts or timing from storage.
- A removed device keeps what it has already read.
- Packages are not encrypted.
