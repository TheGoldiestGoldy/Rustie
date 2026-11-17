# Contributing to Slate Studio

Thanks for helping make a functional native Rust DAW. This is pre-release software and changes need a reproducible verification trail.

Before coding, read [DEVELOPMENT](docs/DEVELOPMENT.md), [PROJECT_STATUS](docs/PROJECT_STATUS.md), [AGENT_HANDOFF](docs/AGENT_HANDOFF.md), and [LEGAL_AND_ASSETS](docs/LEGAL_AND_ASSETS.md).

Use topic branches/worktrees. Don't overwrite a co-author's dirty tree. Provide a small PR: what changed, what is working, disabled or unfinished features, screenshots for UI changes, and tests.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Audio-thread changes must preserve real-time constraints and include allocation/timing reasoning. Report hardware details for audio smoke tests, but redact identifiers and personal data from logs.

For bugs, include minimal self-created repro, commit SHA, environment, expected vs actual behavior, and relevant diagnostics. Do not upload copyrighted sample packs, plugins, sessions or recordings.

License metadata currently says `MIT OR Apache-2.0`; before external public contributions the maintainer must confirm a complete licensing and contributor rights policy.
