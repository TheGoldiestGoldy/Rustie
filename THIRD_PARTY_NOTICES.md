# Third-party dependency and attribution register

Rustie / Slate Studio uses Rust dependencies declared in Cargo manifests and pinned by `Cargo.lock`. This is a **starting register**, not a completed release license audit.

| Direct dependency | Role | Defined in |
| --- | --- | --- |
| `serde` | Model serialization | Workspace |
| `serde_json` | JSON project documents | `daw-ui` |
| `eframe / egui` | Native desktop interface | `daw-ui` |
| `cpal` | Native ALSA/JACK/PulseAudio output | `daw-engine` |
| `rtrb` | Bounded real-time command queue | `daw-engine` |
| `symphonia` | Audio decoding | `daw-engine` |
| `libc`, `zbus` | Linux real-time scheduling | `daw-engine` |
| `assert_no_alloc` | Test-time allocation checks | `daw-engine` dev dependency |

At release time, generate a complete inventory of transitive dependencies and license texts based on `Cargo.lock`. Validate all external icons, fonts, audio and any code snippets used by future agents. **Do not ship installed commercial plugins, copyrighted sample packs, or proprietary assets.**

No copied Ableton assets were found in the committed code snapshot reviewed for this publication.
