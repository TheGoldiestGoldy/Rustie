# Agent 06 | Sole Integrator, Regression Tester and Beta Release Gate

**Only one agent may work in the integration worktree.** Use `/home/developer/Projects/rust-daw-agent-integration`, branch `beta/integration`. The verified tip before this documentation refresh was `fc54b668c32f56ea500d686e564f1d9fd6f66e31`; always record the actual current HEAD and status before acting. Never use dirty `master`.

## Entry and merge sequence

1. Read `AGENTS.md`, canonical `docs/beta-plan/00-ROADMAP.md`, `08-AGENT-CONTRACT.md`, both Wave 2 prompts, all specialist reports, `docs/FEATURE_MATRIX.md`, `docs/BETA1_RELEASE_DECISION.md` and the latest source.
2. Confirm `beta/integration` is clean. Confirm Session and UI branches descend from `e6b82a79feeac4158f1918b3f5598916d5bb177c`, have committed changes/reports and are clean. Do not reset/rebase them to hide conflicts.
3. Review Session and UI diffs independently, including JSON migration, stable IDs, event timing, plugin status display, UI accessibility and shared-file edits. Merge one branch at a time with traceable no-ff merges into the **latest** `beta/integration` tip. Resolve collisions intentionally; never delete another agent's feature to silence a conflict.
4. If a merge changes project schema or plugin IPC, add compatibility tests and build UI + worker from the same source revision. Keep old `.slate.json` projects readable.
5. Update `AGENTS.md`, `README.md`, `docs/FEATURE_MATRIX.md`, `docs/SESSION_ENGINE.md` and the beta reports only where the integrated source proves the change.

## Integrated verification gate

Use a dedicated integration target directory and capture full logs. Run:
```sh
export CARGO_TARGET_DIR=/home/developer/.cache/rusty-target-integration
export CARGO_BUILD_JOBS=4
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p daw-ui -p daw-plugin-host
git diff --check
```
Also run the existing project preflight, synthetic recording writer, live CLAP/VST3 effect and instrument probes, plugin state save/restore probes, realtime no-allocation tests and negative missing/invalid plugin startup tests. Record exact binary paths, SHA, plugin class IDs, block counts, missed/fallback counts and exit codes. Do not reuse another branch's binary or claim a probe was run at the current SHA if it was not.

## End-to-end and reliability audit

1. On a disposable project/media copy: create/open/save/reopen; 3 MIDI tracks + imported stereo WAV; play/seek/BPM/signature; edit/undo/redo; Session scenes; device reorder/bypass; CLAP/VST3 FX and external instrument; record; export.
2. Verify worker kill/crash/hang, malformed or wrong-UID state, missing plugin, queue saturation and child cleanup. No callback block, stale notes, zombie or project corruption.
3. Stress at least 10 minutes and test supported sample rates/buffer sizes. Report XRUNs/deadlines, queue loss and CPU/RAM. Synthetic callback timing is not acoustic latency.
4. Check UI at 1540×940 and 980×640, real screenshots, mouse/focus/keyboard flow, EQ3/EQ8/reverbs, help and tutorial skip. No screenshots from older builds as current proof.
5. Re-run `scripts/install-rusty-local.sh` from a clean committed integration tree. Verify `~/.local/bin/rusty --version`, matched UI/worker checksums, canonical GNOME launcher and real app startup. The launcher installer refuses dirty worktrees.
6. Audit license notices, dependencies, sample project, user-data paths and public assets. Do not include commercial plugin binaries, personal recordings, credentials or private screenshots in public artifacts.

## Owner acceptance and decision

With explicit owner permission, perform a real 10+ second AIR 192|6 mic/guitar take, listen through headphones/speakers, inspect waveform and export, save/quit/reopen, and measure/report latency with a real reference. Do not change OS audio defaults. If this cannot be done, mark NOT TESTED and keep Beta 1 NO-GO.

Update `docs/BETA1_RELEASE_DECISION.md` with PASS / FAIL / NOT TESTED for every roadmap gate. Deliver `docs/agent-reports/YYYY-MM-DD-beta-integration-report.md`, current source SHA, test logs, compatibility matrix, screenshots, release artifact/checksum if eligible, and the exact remaining owner actions. Only this agent may issue GO/NO-GO. No GitHub push or public release without explicit owner approval.
