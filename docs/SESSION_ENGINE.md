# Session grid and playback

## Project format

A project owns an ordered list of scene IDs and its launch grid. A playable track owns a sparse map from scene ID to a complete MIDI or audio clip. A cell’s stable identity is the pair of track ID and scene ID. Track, Arrangement clip and Session clip IDs are unique within the project. Scene IDs have their own namespace. Group, return and master tracks cannot own cells. Group routing, mute, solo and the dedicated MIDI monitor continue to use their track nodes.

For example, a layout with `"session": {"scenes": [1, 2, 4], "quantization": "Quarter"}` can have a MIDI track whose `session_slots` contains just key `"4"`. Its first two cells stay empty when saved. Each value uses the normal clip schema. Audio cells reference the same source file as any copy, while offsets, fades, duration and gain belong to each clip independently. Arrangement playback and export read `Track.clips`.

The GUI loader and `--check-project` both run `Project::prepare_session`. A missing Session layout imports the former per-track row order once, gives each copy a new clip ID, and sets its start to zero. Original clip IDs, timeline positions and source files are preserved. An explicit layout with empty cells stays empty. The editor supports 48 tracks, 48 scenes and 48 clips per surface.

## Controls

- Click an empty MIDI or audio cell to create or import at that row. Add a scene with **＋ Scene**. The clip editor, duplicate, copy, paste, delete and project history work with Session clips. Paste and duplicate fill the first available cell on the destination track.
- Choose **Sixteenth**, **Quarter** or **Bar**. A running Session uses the first grid line strictly after the render callback reads the request. A scene addresses every playable track. Empty cells stop their tracks. A target that is already active keeps its loop phase.
- Track stop and **Stop all** use the same launch grid and keep the transport running. The main transport Stop closes playback. A later launch replaces pending changes on the tracks it addresses. **Cancel queued** clears pending changes across all tracks.
- Switching the visible view leaves the running graph and transport position alone. Starting a Session performance from an Arrangement playback graph requires a rebuild. Structural edits and explicit playback restarts also rebuild the graph. Active clips and pending launches are performance state and are transient.

## Render contract

`Controller::launch_session(SessionLaunch)` publishes one fixed-size command to a dedicated 16-entry SPSC ring. Its 48 lane actions correspond to track positions in the compiled graph. A full ring rejects the entire item and increments the existing overflow counter. The UI displays the rejection and the performer can retry. The callback accepts at most 16 items per block and applies them in publication order. Pending state is held by the preloaded clip voices. The general command ring and its pending-event storage are outside the scene admission path.

Each voice owns its compiled MIDI event list, optional target and a shared read-only tempo-segment table. Events and loop boundaries use absolute musical ticks converted to project frames. The next cycle is derived from the previous musical tick position. This avoids adding a rounded frame duration repeatedly. Same-tick note-offs precede note-ons. Switching away sends all-notes-off to the built-in source and the live effects chain. Atomic meters expose the rendered voice state and cycle progress to the GUI. Session MIDI and launches continue while the GUI is idle.

Audio plays at the source sample rate after decode/resampling. Tempo changes move the next loop boundary without time-stretching the source. Short sources leave silence for the remaining cycle. Clip gates change over 128 samples, and imported loops receive a short edge fade in addition to respecting user fade settings. Physical output latency, click audibility and external instrument behavior require listening tests.
