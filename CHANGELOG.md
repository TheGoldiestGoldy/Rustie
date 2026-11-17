# Changelog

This repository is in **pre-release** status. It has no semantic-versioned public release.

## 2026-11-17: documentation scaffold (branch `docs/github-scaffold-20261117`)

- Collected actual Rust workspace crate map and committed project history.
- Wrote evidence-based feature inventory, UI target spec, audio notes, test matrix, bug register, roadmap, agent handoff and legal/privacy policy.
- Created GitHub-friendly issue/PR/CI scaffolding in a separate worktree.
- Preserved ongoing UI and documentation agent edits in the original worktree.

## 2026-11-17: `f578b9a` (historical local commit)

- Commit message: `Improve Slate Studio arrangement UI and low-latency playback`.
- Existing code includes arrangement editing, Session overview, piano roll and native audio driver selection.
- The reported visual/audio onset delay requires independent post-change verification.

## 2026-11-16: `48a1eb8`

- Commit message: `Route audio probe through native PulseAudio and expose output level`.

## 2026-11-16: `007a15d`

- Commit message: `Implement native Rust DAW core and Phase 2 audio engine`.

Exact changes should be reviewed using `git show <sha>`; these notes are a navigation aid, not an asserted release certification.
