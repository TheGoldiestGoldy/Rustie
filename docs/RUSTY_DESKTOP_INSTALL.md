# Rusty | Native Linux DAW desktop installation

**Status:** Development preview. See the [Beta 1 release decision](BETA1_RELEASE_DECISION.md).

## The application you actually click

On the Linux test workstation, the canonical GNOME application is **Rusty** (`rusty.desktop`), running `~/.local/bin/rusty`. The launcher runs the *installed, versioned bundle*, never `~/Projects/rust-daw/target/release/daw-ui` or another stale main-checkout build. The installer creates:

- `~/.local/opt/rusty/versions/<12-character Git SHA>/rusty`: app executable (Cargo builds it as `daw-ui` internally).
- `~/.local/opt/rusty/versions/<12-character Git SHA>/rusty-plugin-host`: companion plugin worker (Cargo builds it as `daw-plugin-host`).
- `~/.local/opt/rusty/versions/<SHA>/SHA256SUMS`: checksums of that installed pair.
- `~/.local/opt/rusty/current`: atomic symlink to the selected version.
- `~/.local/bin/rusty`: stable launcher, logs to `~/.cache/rusty/launcher.log`.
- `~/.local/share/applications/rusty.desktop`: application-menu entry called **Rusty**.

The historical `slate-studio.desktop` app ID remains as a hidden compatibility alias that calls the *same* stable Rusty launcher. This protects any old pinned shortcuts without giving the user two visible entries. There is also a **Rusty (UX Review)** app that opens the existing test project using the same current build; its old launcher ID is hidden. Original menu entries were backed up before replacement under `~/.local/share/rusty/legacy-desktop/`.

## Every release's exact version

Rusty displays `Rusty v0.1.0-dev (<12-character Git commit>)` in the native window title and `v0.1.0-dev · <commit>` in the top-left transport bar. The same version is printed by:

```sh
~/.local/bin/rusty --version
```

The commit is embedded **at build time**, not read from the current repository when opening the app. Thus old and new installed binaries report their own true source identifiers, even after further Git commits. `-dev` is intentional: the product does not meet Beta 1 release gates yet.

## Installing the latest tested integration

After coding and QA in `/home/developer/Projects/rust-daw-agent-integration`, **commit all intended source changes**, then run:

```sh
cd /home/developer/Projects/rust-daw-agent-integration
./scripts/install-rusty-local.sh
```

The installer refuses a dirty source tree, builds the UI and worker together, confirms the compiled Git commit matches `HEAD`, checks installed file hashes and desktop-entry syntax, then atomically switches `current`. Old version directories are retained for safe rollback; it does not modify the dirty original `master` checkout, install system packages, change audio defaults or push to GitHub. After future integration merges, re-run this installer; do not tell the user an older desktop shortcut is already current without verifying `rusty --version`.

The 21 November QA installed source `028e2b1a8cd5` in an isolated profile. The launcher opened the native application and both executable SHA-256 checks passed. The locked Linux graph requires Rust 1.89 or later, including `uuid 1.29.2`. QA used Rust 1.100.0. Project dialogs use `zenity` and plug-in inspection uses GNU `timeout`. Release packaging still needs contributor-rights confirmation and license and dependency notices.

Rusty accepts `--project <path>`, `--check-project <path>` and `--version`. New save-dialog filenames default to `Untitled.rusty.json`, while existing `*.slate.json` and other JSON projects remain supported. The source repository path and persisted schemas are intentionally not renamed. Audio export/record/render destinations use new `~/Music/Rusty ...` folders for future output, without moving existing personal recordings.

Troubleshooting: inspect `~/.cache/rusty/launcher.log` and `~/.local/opt/rusty/current/SHA256SUMS`, then compare `~/.local/bin/rusty --version` with the intended integration commit. No claims of physical speaker sound or GUI gestures are implied by CLI smoke tests.
