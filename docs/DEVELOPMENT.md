# Developer setup

Rust edition 2024; declared minimum Rust 1.89. Use a current compatible Linux Rust toolchain. The native development baseline is CachyOS/Arch; CI uses Ubuntu. Other environments require validation.

```bash
git clone https://github.com/TheGoldiestGoldy/Rustie.git
cd Rustie
cargo build --locked --release -p daw-ui -p daw-plugin-host
./target/release/daw-ui
```

Build the UI and worker together. Runtime file dialogs use `zenity`; plug-in inspection uses GNU `timeout`. Install native development libraries for ALSA, JACK/PipeWire-JACK, PulseAudio and X11/Wayland/OpenGL. Choose the JACK provider appropriate to the existing audio stack; do not replace the audio stack just to compile.

The [CI workflow](../.github/workflows/ci.yml) lists the Ubuntu development packages. Verify the actual workflow filename if adapting these instructions to another branch.

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For a versioned local installation from clean committed source, use `./scripts/install-rusty-local.sh`; see [installation](RUSTY_DESKTOP_INSTALL.md). Source publication does not automatically upgrade the desktop app.

Use a separate branch/worktree per change. Inspect existing edits first. Preserve realtime bounds, project IDs, source media and old saves. Report source/automated/native/hardware/listening evidence separately. See [contributing](../CONTRIBUTING.md), [agent instructions](../AGENTS.md) and [handoff](AGENT_HANDOFF.md).
