# Agent 05 | Devices, Browser, Effects UX and Onboarding | WAVE 2, PARALLEL

## Exact worktree and base

Use only `/home/developer/Projects/rust-daw-agent-ui`, branch `beta/ui`. Its verified clean starting SHA is `e6b82a79feeac4158f1918b3f5598916d5bb177c`. This is the deliberate Wave 2 base. **Do not rebase or reset it.** Agent 07 will merge your commit into the latest integration tip.

Read the canonical instructions from `/home/developer/Projects/rust-daw-agent-integration/docs/beta-plan/`, plus `AGENTS.md`, `docs/LIQUID_UI_DESIGN.md`, `docs/UX_REVIEW_ROUND3.md`, `docs/FEATURE_MATRIX.md`, current theme/UI source and the Devices pre-integration audit.

## Preflight findings to resolve

The preflight audit found inaccurate Effects-browser descriptions, CLAP discovery/insertion that appears disabled or unclear, imprecise EQ8 drag hit detection, and unclear plugin worker states. Existing Devices access, effect bypass/reorder and tutorial are present but need usability review. No implementation, build, screenshot or commit was produced by that audit. Reconfirm all findings against the latest integration source.

## Mission and ownership

Deliver a compact, coherent Rusty interface that makes existing working functionality discoverable. Own `studio_devices.rs`, `studio_browser.rs`, `studio_help.rs`, `theme.rs`, UI-only helpers and UX documentation. Do not change Session scheduling, plugin worker backend, DSP algorithms or recording model. Keep shared edits to `studio_views.rs` and `studio_plugin_render.rs` narrow and document them for Agent 07.

## Ordered implementation

1. Inspect the actual compiled app in Arrangement, Session, Mixer and Devices at 1540×940 and 980×640. Use real screenshots and pointer/keyboard interaction; if remote GUI capture is unavailable, record the exact blocker and require owner sign-off.
2. Correct Effects/Instrument/Plugin browser categories, search and discovery. Make CLAP/VST3 format, vendor/class, availability, loading, worker failure and unsupported state explicit. Never show success for a failed insert.
3. Improve compact grouping and button sizes. Keep Devices easy to reach near the top-left File/Edit area. Preserve existing Mixer/Piano workflows and the fixed-width Library behavior.
4. Fix EQ8 pointer hit testing and feedback. Make EQ3/EQ8/Vocal Reverb/Guitar Reverb labels and controls accurate; only expose parameters that are connected to persisted DSP values.
5. Make bypass, reorder, plugin insertion/removal and visible status consistent with the actual selected track/device chain. Do not invent native plugin editors or preset controls if the backend does not provide them.
6. Improve context menus, focus/hover descriptions, keyboard navigation, minimum-window behavior and the skippable first-run tutorial. No tutorial may block normal use.
7. Test repeated repaints, window resize, focus changes, disabled states and pointer release outside a panel. Avoid expensive blur or new audio-thread work.

## Acceptance and delivery

- Browser text matches real actions and supported formats; no disabled-looking or fake CLAP controls.
- EQ8 controls select the intended band reliably; visible effect parameters change persisted DSP values.
- Device order/bypass and project state survive save/reopen where already supported.
- No layout creep after 120 repaints; primary controls remain reachable at 980×640.
- Current compiled screenshots and interaction evidence are included, or the GUI limitation is clearly documented.
- Run shared checks and focused UI tests with a dedicated `CARGO_TARGET_DIR`.
- Commit to `beta/ui`; report `docs/agent-reports/YYYY-MM-DD-beta-ui-report.md` with screenshots, interaction steps, test output, unverified items and integrator notes. Do not merge or push.
