# Contributing to Rusty

Rusty is a development preview. Start with the [current status](docs/PROJECT_STATUS.md), [architecture](docs/ARCHITECTURE.md), [developer setup](docs/DEVELOPMENT.md) and [agent instructions](AGENTS.md).

Use a topic branch or isolated worktree and preserve other contributors' changes. Explain the concrete user behavior, source baseline, verification and remaining limits. Include real native screenshots for UI changes and a dated report in `docs/agent-reports/`.

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --locked --release -p daw-ui -p daw-plugin-host
```

Audio callback changes must preserve bounded work and document allocation/timing implications. Test migration and recovery when changing saved data. A compile or nonzero PCM result is not headphone acceptance. Do not upload personal recordings/projects, secrets, installed plug-ins or proprietary sample banks.

The existing license declaration is MIT OR Apache-2.0. Contributor-rights confirmation and complete distribution notices remain release gates; see [asset and license notes](docs/LEGAL_AND_ASSETS.md).
