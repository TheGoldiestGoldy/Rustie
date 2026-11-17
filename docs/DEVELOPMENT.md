# Developer setup and workflow

## Baseline

Rust edition **2024**, `rust-version = "1.85"`. One Cargo workspace, six members. Primary tested host: native Linux on CachyOS/Arch (not a guarantee for arbitrary distros).

```bash
git clone <REPOSITORY_URL> slate-studio
cd slate-studio
cargo build -p daw-ui
cargo run -p daw-ui
cargo run -p daw-engine --bin daw-audio-probe
```

For production-like speed, use `cargo build --release -p daw-ui` and `cargo run --release -p daw-ui`. Ensure native development libraries for ALSA, JACK and PulseAudio, plus Wayland/X11/xkbcommon graphics dependencies. **Pick one compatible JACK provider:** on a PipeWire session use the distro's PipeWire JACK development compatibility package; on a standalone JACK server use its JACK development package. Avoid replacing the audio stack as part of a routine build.

### Portable project launcher

The checked-in baseline launcher contains an absolute home-directory path. Use the repository-relative launch script introduced by this documentation branch, or `cargo run -p daw-ui`. Do not commit hardcoded usernames/paths.

## Local validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build errors caused by missing native libraries are environment issues; record the exact dependency/error rather than claiming source compilation failed.

## Repository conventions

- Small PRs with baseline SHA, feature scope and tests.
- Keep core musical logic independent of GUI and hardware.
- Audio callback changes require a specific no-allocation test and a timeline/queue analysis.
- Do not mutate project data outside expected model/undo flow; identity invariants matter.
- Do not check in `target/`, binaries, render outputs, personal audio, samples, installed plugins, credentials or local project files.
- Record compatibility breaks, migrations and changes to system audio setup explicitly.
- Avoid ambiguous "working" or "verified" labels. Report code review, compile, unit tests and hardware tests separately.

## Branches and multiple agents

Each agent uses a separate branch or `git worktree add`. The source worktree may be actively edited: do not `git reset --hard`, `git clean -fd`, force-push, or overwrite another agent's `docs/` content.

Example:

```bash
git worktree add -b feat/session-clip-launch ../slate-session-clip-launch master
cd ../slate-session-clip-launch
# Make scoped changes; validate; commit.
```

Integration order: inspect `git status` and `git diff` -> rebase/merge on newest reviewed base -> run tests -> open PR -> merge after review. See [AGENT_HANDOFF](AGENT_HANDOFF.md).

## Debugging audio without disruption

```bash
cargo run -p daw-engine --bin daw-audio-probe -- --silent 48000 pulse
```

Only run the opt-in tone with speaker volume monitored. Do not change card profiles, kill audio services or terminate calls for test convenience.

## Handover checklist

Before posting "done": links or filenames, base/head commit SHAs, user-visible behavior, test command+output, limitations, screenshots if UI changed, whether code touched real-time path, any secret/IP audit findings, and rebase/merge risks.

## GitHub bootstrap

The Rustie GitHub repository now exists at https://github.com/TheGoldiestGoldy/Rustie and is public. GitHub content may be added through the authorized connector even though the local GitHub CLI was not authenticated during this audit. The commands below are optional for future *locally authenticated* Git publishing. Do **not** initialize the new remote with an unrelated starter commit if preserving the local history through a straightforward first push:

```bash
# Run only after reviewing local source/history and choosing a safe integration strategy.
cd /path/to/your/slate-studio-checkout
git remote add origin https://github.com/TheGoldiestGoldy/Rustie.git
gh auth login -h github.com
# Avoid replacing GitHub history: fetch first and merge carefully.
git fetch origin
```

The main application worktree's `master` branch also exists locally and may advance while agents work. Before choosing the public default branch, merge/rebase the documentation branch with those changes and verify there are no local identifiers, media or secrets. Replace placeholders with actual values, never guessed remote URLs.
