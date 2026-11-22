# License and originality review checklist

**Engineering notes, not a legal opinion.** Similar workflow conventions are not authorization to reproduce proprietary branding, source files, sounds or a confusingly similar trade dress.

## What was actually used
- All UI and loop code written in this pass is original Rust for the existing project and egui API.
- No code, icons, samples, presets, screenshots, commercial assets, font files, DAW binary or project files were copied from Ableton.
- No code was copied from Ardour, LMMS or Zrythm. Their GPL/copyleft status means copying implementations into a differently licensed project needs independent compatibility review.
- The existing Rust workspace declares `MIT OR Apache-2.0` in Cargo metadata; **the repository still needs complete license files and a contributor rights check before public publication**.
- Audio/plugin discovery is only filesystem inspection. Discovery does not imply redistribution rights to the detected plugins.

## Verified official references (checked 2026-11-17)
- Ableton trademark guidance: https://www.ableton.com/en/legal/branding-trademark-guidelines/
- Ableton Live manual (workflow concepts, *not* source/assets): https://www.ableton.com/en/manual/live-concepts/
- egui source & MIT OR Apache-2.0: https://github.com/emilk/egui
- CLAP MIT license: https://github.com/free-audio/clap/blob/main/LICENSE
- Steinberg VST 3 current MIT SDK and trademark distinction: https://www.steinberg.net/developers/vstsdk/
- Steinberg SDK license detail: https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Licensing.html
- Ardour GPLv2 statement: https://ardour.org/building
- LMMS GPL-2.0 repository: https://github.com/LMMS/lmms
- GitHub repository licensing: https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository

## Pre-publication gates
1. Confirm authors/owners can license every original source file as declared.
2. Add unmodified standard LICENSE-MIT and LICENSE-APACHE files if dual licensing is intended; review third-party dependencies with `cargo license` or an SBOM, including transitive libraries and runtime assets.
3. Do not copy Ableton names/logos or make product affiliation claims. Keep 'Ableton-inspired workflow' only as truthful comparative prose, not the app's branding.
4. Verify effects, CLAP, VST3 and LV2 dependencies independently before integrating hosting. VST as a trademark is separate from permissive SDK source licensing.
5. Do not publish private saved projects, absolute user paths, samples from commercial packs, plugin binaries, user email, audio device logs, tokens or traces.
6. Review `git diff --cached` before every public commit; no license choice makes private data safe to publish.
