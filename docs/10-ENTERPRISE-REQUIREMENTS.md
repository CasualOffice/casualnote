# 10 — Enterprise requirements

These gate the phases that deliver them, and shape design from the first crate. Certifications such as SOC 2 are organisational, so the product is built to support them and does not claim them. **Under `DEC-02`, all of this stays Apache-2.0 in this repo, with no enterprise edition.**

| ID | Area | Requirement | Designed in | Delivered |
| --- | --- | --- | --- | --- |
| ENT-R01 | Identity | SSO through OIDC and SAML; no password store of our own | 8 | 10 |
| ENT-R02 | Access | View, comment and edit, enforced per operation on the server in managed mode | 1 | 8 |
| ENT-R03 | Audit | Append-only events: who opened, edited, shared, exported. Export to a SIEM | 8 | 10 |
| ENT-R04 | Self-hosting | Runs entirely on-premises, including air-gapped: no external calls, CDN, fonts or telemetry | 0 | 8 |
| ENT-R05 | AI policy | An admin can disable AI, allow-list providers and models, or require a local or internal endpoint | 5 | 10 |
| ENT-R06 | AI data | Workspace policy decides whether notes may reach a provider. Use of AI is audited, without recording note text | 5 | 10 |
| ENT-R07 | Encryption | In transit always. At rest with host-managed keys in managed mode. End-to-end in encrypted mode | 7 | 8 |
| ENT-R08 | Retention | Retention rules and legal hold in managed mode. **Not possible in end-to-end encrypted mode,** and the docs say so | 8 | 10 |
| ENT-R09 | Export control | Admins can restrict export in managed mode. This cannot stop a determined user who can read the screen, and the docs say so | 8 | 10 |
| ENT-R10 | Backup | Documented backup and restore. The package format is a portable last resort | 1 | 8 |
| ENT-R11 | Languages | All launch languages for admin screens too | 2 | 10 |
| ENT-R12 | Accessibility | WCAG 2.2 AA, with a published conformance statement | 2 | 6 |
| ENT-R13 | Deployment | Compose and Helm. Upgrades tested across versions | 8 | 8 |
| ENT-R14 | Observability | Metrics, structured logs with no note text | 8 | 8 |
| ENT-R15 | Licensing clarity | SBOM per release, with licences for code, models and fonts | 0 | 6 |
| ENT-R16 | Security process | Published disclosure policy and supported versions | 0 | 6 |

Delivery rows are tracked as `ENT-01` in [14](14-EXECUTION-TRACKER.md), split into tasks when Phase 10 opens.
