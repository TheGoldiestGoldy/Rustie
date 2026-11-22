# Rustie | Sole Integrator post-Wave-1 safety hardening
20 November 2026 UTC. Branch `beta/integration`. **NOT** a Beta 1 GO decision.

## 1. Scope and exact change

After finalizing the immutable Wave 2 base `INTEGRATED_SHA=e6b82a79feeac4158f1918b3f5598916d5bb177c` and creating clean `beta/session` and `beta/ui` worktrees from that commit, integration independently reviewed plugin state restoration.

**Fix:** `crates/daw-ui/src/studio_live_plugins.rs` now checks recorded opaque state `plugin_uid` against the selected `ClapLiveInsert.class_id` **when both identities are present**, before passing state bytes to a worker. Wrong identity, format, malformed hex or excess size skips restoration and uses plugin defaults with diagnostic warning; old valid blobs with empty UID remain loadable. This is a narrowly bounded integration-only hardening change. It does not add a native plugin editor or turn state-recall GUI acceptance into PASS.

## 2. Independent validation

With `CARGO_TARGET_DIR=/home/developer/.cache/rustie-target-integration`:
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace --quiet`: **119 passed**, 0 failed (previous 118 + new UID mismatch regression in the UI crate).
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS.
- `cargo build --release -p daw-ui -p daw-plugin-host`: PASS.
- `git diff --check`: PASS.
- Negative installed-host startup CLI probes: missing CLAP bundle, invalid CLAP class, missing VST3 bundle all returned **exit 2** with error messages, not a hang (three 8-second bounded probes). Log `/home/developer/.cache/rustie-wave1-negative-probes.log`.
- QA log `/home/developer/.cache/rustie-integration-hardening-gate.log`. Positive live plugin and state roundtrip probes from prior report remain at source `ccacd30`; they were not recharacterized as physical hardware proof.

## 3. Impact on Wave 2 and remaining defects

Wave 2 worktrees **intentionally remain at `e6b82a7`**, the validated three-way base. This integration-only change is an additional later commit on `beta/integration` and will be part of the final merge; the Session/UI agents need not rebase or edit their worktrees. Each specialist must keep using `e6b82a79feeac4158f1918b3f5598916d5bb177c` as baseline and deliver committed code/reports separately.

Saved plugin state without a recorded UID cannot be authoritatively tied to a class and is accepted for legacy compatibility. VST3 state/GUI cold reopen and worker crash/hang still need acceptance. No hardware microphone/speaker testing or GitHub push performed.
