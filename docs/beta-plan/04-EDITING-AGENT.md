# Agent 03 | Arrangement, Audio/MIDI Editing and History | WAVE 1 COMPLETE

> Wave 1 editing work is merged into `beta/integration` at `b04d81c`. Do not rerun this prompt unchanged. Agent 07 may dispatch a narrow editing follow-up from a fresh integration SHA if integrated QA identifies a reproducible defect; GUI gesture and playback-continuity acceptance remain open.

You are Rustie's editing agent. Work ONLY inside:
  /home/developer/Projects/rust-daw-agent-editing (branch beta/editing).
Confirm the coordinator's BASE_SHA; read AGENTS.md, docs/beta-plan/{00-ROADMAP,08-AGENT-CONTRACT}.md, docs/FEATURE_MATRIX.md and UI source before editing.

## Mission
Make editing one complete song practical: predictable selection/drag, multi-clip audio actions, MIDI velocity editing and real undo/redo without losing existing clips or breaking playback. This is an implementation assignment, not a mockup.

## Owned files
- crates/daw-ui/src/studio_audio_editor.rs, studio_clip_management.rs, studio_piano.rs, studio_history.rs, focused UI tests and related docs.
- studio_views.rs and studio.rs are shared HOTSPOTS. Keep changes to minimal entry points or add new focused helper modules for integrator to wire in; document exact insertion points.
- Do not edit recording, plugin host or Session modules. Avoid core project schema changes unless the interface is essential and backward compatible.

## Ordered implementation
1. Investigate current selection, focus, egui pointer capture and scroll behavior. Reproduce previous drift issue (Library panel growing right on repaint) as a regression guard; do not undo the existing explicit fixed browser width.
2. Ensure Arrangement clip trim, split at playhead, move, duplicate, delete, copy/paste (track compatible) and quantized snap work on **real audio and MIDI clips** with correct source offset and no silent time shift. Keep original WAV unchanged, preserve sample boundaries and undo history.
3. Make click-in-empty-lane seek, clip-select and clip-drag mutually exclusive. Prevent a seek on selection; support zoom and keyboard navigation without accidental background pan. Confirm time signature and BPM input remain editable and persisted.
4. Make MIDI velocity bars draggable with bounded velocity and accurate note IDs; retain current 0-127 pitch range, fold/scale mode, length edit, piano selection, note audition and in-key restrictions.
5. Route all destructive/transform operations through bounded undo/redo transactions (including multistep operations), with redo invalidation, selection restoration and no data races.
6. Test corner cases: empty tracks, clip at frame zero, split at boundaries, overlapping clips, mouse drag release outside panel, 4/4 vs 6/8 bar rulers, different sample rates, large projects, mixed grouped tracks.
7. Add keyboard shortcut and hover-help mapping for new editor actions; coordinate UI final polish with Agent 05 (who will run in Wave 2).

## Acceptance
- User can import an audio track, see waveform, split/trim/move/copy and undo/redo; save/reopen retains exact regions and unchanged source WAV.
- User can create/adjust MIDI notes and velocities and undo/redo, with no phantom notes or surprising horizontal movement.
- Editing while stopped and during playback does not freeze engine or lose selection.
- Tests assert exact frame/tick offsets and history invariants. Run all contract checks. Record any manual UI interactions not actually tested.

## Deliverables
Commit to beta/editing and report docs/agent-reports/YYYY-MM-DD-beta-editing-report.md with before/after behavior, regressions, test commands, GUI captures if possible and minimal shared-file integration hooks. No merge or push.
