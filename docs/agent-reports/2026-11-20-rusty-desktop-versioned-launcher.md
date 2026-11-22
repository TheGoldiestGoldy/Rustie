# Rusty desktop identity, versioning and installed launcher
**20 November 2026 UTC | Sole Integrator | Branch `beta/integration`**

## 1. What changed
- Rebranded the **visible** DAW identity from Slate Studio/Rustie to **Rusty**, including native window title, top-left transport branding, help tutorial, saved-project file dialogs, and user-facing diagnostics.
- Embedded `v0.1.0-dev` and the **exact compiled Git commit (12 characters)** using `crates/daw-ui/build.rs`. Both the title and visible transport show the version/build identifier, and `rusty --version` prints it headlessly.
- New default save-dialog name is `Untitled.rusty.json`; **all existing `*.slate.json` saves remain readable**. No project data/schema conversion, migration or deletion. Future user output directories use `~/Music/Rusty Recordings`, `Rusty Exports` and `Rusty Plugin Renders`; old audio stays where it is.
- Worker lookup prefers co-installed `rusty-plugin-host` and falls back to the original Cargo-produced `daw-plugin-host` for developer builds, preserving CLI worker tests.
- Added `scripts/install-rusty-local.sh` to compile/install a matched, checksummed GUI/worker pair under `~/.local/opt/rusty/versions/<SHA>` and atomically update the stable `current` symlink. The GNOME app menu points to `~/.local/bin/rusty` instead of old `master`; desktop aliases for Slate Studio and Rustie UX Review remain launchable but hidden to avoid duplicate menu icons. Prior entries are backed up under `~/.local/share/rusty/legacy-desktop/`. New canonical entries: `rusty.desktop` and `rusty-ux-review.desktop`.
- Legacy `scripts/start-slate-studio.sh` in this integration worktree now forwards to `scripts/start-rusty.sh`, never the old stale `target/release/daw-ui`. The **original master checkout was not modified**.
- Main repository, development crate names, audio formats, legacy personal files and specialist worktrees were intentionally not renamed; this preserves existing imports, Git worktree paths, plugin IPC and personal recordings.

## 2. Initial verification
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace --quiet`: PASS, **119 tests**, zero failed.
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS.
- `cargo build --release -p daw-ui -p daw-plugin-host`: PASS.
- Built binary `daw-ui --version`: printed `Rusty v0.1.0-dev` plus Git commit (initial pre-commit source build, later rebuilt from final committed SHA by installer).
- Existing review project `daw-ui --check-project '~/Music/Rustie UX Review/Rustie UX Review.slate.json'`: PASS, 4 tracks / 8 clips / 2 decoded WAV files.
- `bash -n` for installation and both launcher scripts: PASS.
- `git diff --check`: PASS.
- Validation log: `/home/developer/.cache/rusty-desktop-final-qa.log`.

## 3. Build-freshness regression found during real installation
The first installation run intentionally **failed closed**: after committing branding, Cargo reused the earlier executable and it still printed the previous commit `12c3a82e3d6d` although the branch had moved to `a1e6d2a382fc`. The desktop menu was not repointed. Corrective change: installer now exports `RUSTY_BUILD_REVISION=<HEAD short SHA>` and `build.rs` registers `cargo:rerun-if-env-changed=RUSTY_BUILD_REVISION` and rejects a mismatch; this forces and validates the exact revised build. Revalidate the final installed revision after the corrective commit.

## 4. User-facing installation and reproducibility
The installer refuses dirty worktrees. First successful deployment built commit `f57c2b0794d2` and installed `Rusty v0.1.0-dev (f57c2b0794d2)` under `~/.local/opt/rusty/versions/f57c2b0794d2`; `current` resolved to that directory, both installed binaries passed `sha256sum -c SHA256SUMS`, and all four canonical/legacy GNOME desktop entries passed `desktop-file-validate`. The canonical `gtk-launch rusty` started process 31934 from the installed Rusty executable; `gtk-launch slate-studio` independently started process 32203 from the same installed executable. Both test processes were closed afterward, and `~/.cache/rusty/launcher.log` recorded the correct commit. An installed CLAP limiter graph probe used the installed companion worker, produced nonzero PCM over 400 blocks and reported 1 dry fallback. A legacy four-track `.slate.json` project passed preflight. This proves real app-menu executable launch and plugin worker wiring, **not manual GUI mouse/speaker acceptance**. Re-run the version-pinned installer after committing this final deployment report so the installed SHA also matches the latest report commit.

Canonical development base remains the committed Wave 1 `INTEGRATED_SHA=e6b82a79feeac4158f1918b3f5598916d5bb177c`, with later sole-integrator fixes on `beta/integration`. The Session and UI specialist worktrees remain unchanged at the designated Wave 2 base. No release, external publication, audio profile change or GitHub push was performed.

## 5. Known limitations and next-agent handoff
- **Rusty v0.1.0-dev is not Beta 1 READY.** Session persistence, real physical recording/listening, plugin GUI state/long-run stability, and Wave 2 UI acceptance remain separate Beta 1 gates.
- The local installer installs *the latest tested, committed integration build at installation time*. When subsequent agent work is merged, the integrator **must rerun it**. It does not compile source on every click.
- The version ID proves the installed source commit, not the number of completed product features.
- Read `docs/RUSTY_DESKTOP_INSTALL.md` before future launcher work, keep `~/.local/bin/rusty --version` matching the installed commit and verify a real app startup before claiming installation.
