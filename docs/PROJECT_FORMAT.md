# Project serialization and interchange

## Current project persistence

The egui app saves/loads the `daw_core::model::Project` struct as human-readable **JSON**, using `serde_json`. This is the current prototype format; it has no explicit magic/schema version or migration layer. Supported fields in current code:

```text
Project
  name
  tempo_map
  time_signature
  global_scale? (root and mode)
  tracks[]
    id, name, kind, volume, pan, muted, solo
    clips[]
      id, start (ticks), duration (ticks), scale_override?
      kind: Audio { source: PathBuf, warped: bool }
         or Midi { notes[]: pitch, start, duration, velocity, muted }
```

**Caveats:** Audio files are linked to external paths; bytes/caches are not embedded. A saved project cannot be assumed portable across machines. The `warped` model flag does **not** mean there is an implemented warp renderer. Hardware/plugin/mixer-rack state is not persisted. Do not hand-edit arbitrary files unless you validate expected fields. Older project files relied on serde defaults for `muted` and `solo`.

### Portable project proposal (NOT IMPLEMENTED)

- A versioned envelope with `format_id`, `schema_version`, `app_version`, `project`, `asset_manifest`.
- Relative media paths with checksums, optional copy-on-save to an assets directory and missing-file relinking.
- Atomic save via temp file + fsync + rename; recovery files and backward migration tests.
- Stable object UUIDs or clearly documented allocator with migration handling, clip/track/device references.
- Explicit plugin state manifests and persistence only after plugin host safety is established.

## Ableton .als considerations (FUTURE)

- `.als` files are commonly gzip-compressed XML, but a container format is not a documented compatibility guarantee.
- Versioned, proprietary structures and plugin state require a legally obtained, curated fixture set.
- Aim first for a **read-only limited subset** (tempo, tracks, basic clips, notes), recording omitted fields.
- Preserve unknown nodes only with a tested round-trip strategy and stable IDs; never promise lossless import/export by default.
- No redistribution of Ableton presets, test projects or proprietary art without rights.

## Test cases to add

1. Save/reopen synthetic 2 audio + 2 MIDI track project at non-default tempo and scale.
2. Preserve IDs, clip offsets, mute/solo, velocity and scale override.
3. Gracefully flag missing audio and offer relink.
4. Reject malformed JSON, duplicates and excessive track/clip counts without corrupting open state.
5. Test new schema migrations against older generated fixtures.
6. Security limits for untrusted paths, XML bombs and overlong documents before implementing interchange.
