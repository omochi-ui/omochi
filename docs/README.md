# Documentation map

User instructions are maintained in the documentation site, in
[Japanese](../apps/site/src/content/docs/docs/index.md) and
[English](../apps/site/src/content/docs/en/docs/index.md). Keep corresponding
pages aligned. This directory holds architecture, development, deployment and
historical design material; the [specification index](../spec/README.md) identifies
current contracts and experimental specifications.

Development documentation in `docs/` and `spec/` is maintained in English only.
Repository READMEs use `README.md` for English and `README.ja.md` for Japanese
translations. The user-facing site's existing Japanese and English structure
is maintained separately.

## Current implementation

| Topic | Start here |
| --- | --- |
| Repository setup and verification | [Contributing](../CONTRIBUTING.md) |
| Whole-codebase implementation specification: runtime, modules, protocols, storage and verification | [Architecture and implementation specification](architecture.md) |
| Gateway, owner and shared-client connections | [Remote access overview](dashboard-remote-access.md) |
| Hosted dashboard sharing and recovery | [Sharing guide](dashboard-sharing.md) |
| Dashboard layout and board view | [Dashboard UI](dashboard-ui.md) |
| Flutter app, embedded Hub and external Hub connections | [Mobile app](../apps/mobile/README.md) |
| SDK/browser packages | [SDK releases](sdk-releasing.md) |
| 2026-11-06 release notes and upgrade instructions | [SDK/browser 0.2.1](releases/sdk-0.2.1.md), [native 0.3.1](releases/native-0.3.1.md) |
| Cargo crates, native binaries and npm launchers | [Native releases](native-releasing.md), [distribution](../crates/DISTRIBUTION.md) |
| Static websites, dashboard hosting, Docker and relay | [Deployment](../deploy/README.md) |
| Shared interface tokens and components | [Design system](design-system.md) |

The normal hosted dashboard connects to a native Hub. The Flutter app can use
its embedded Rust Hub or connect to an external Hub. Direct browser-to-Gateway
aggregation is an opt-in experiment.

## Evidence and history

| Material | Scope |
| --- | --- |
| [Dashboard sharing acceptance](dashboard-sharing-acceptance.md) | Dated local browser/relay evidence; public-host and separate-network acceptance remain separate |
| [Mobile validation](../apps/mobile/docs/validation.md), [remote connections](../apps/mobile/docs/remote-connections-validation.md) | Platform-specific checks and remaining limits |
| [Architecture diagram exports](diagrams/README.md) | Historical TypeScript server diagrams and reproducible source revision |
| [Native benchmarks](benchmarks/native-local.md) | Historical measurements against the former TypeScript server |
| [Browser dashboard experiment](../experiments/browser-dashboard/README.md) | Current opt-in commands and relocated direct-Gateway implementation |
| [Browser experiment plan](dashboard-remote-access.browser-experiment.md) | Archived implementation plan with original paths and commands |
| [Remote-access reviews](reviews/dashboard-remote-access/review-state.md) | Historical proposals, review snapshots and findings |

`dashboard-remote-access.browser-v*.md`,
`dashboard-remote-access.server-hub-v1.md` and `spec/*.review-v*.md` are historical
snapshots. Their future-tense instructions and test counts apply to the recorded
revision. Use current guides and package manifests for commands, and dated
acceptance records for what was actually verified.
