# Actual application screenshots

These are captures of compiled native Rusty windows. No generated concept art is presented as working UI. Images demonstrate appearance and visible state, not audio quality, measured latency or interaction correctness.

## 22 November: microphone workflow candidate

The mic agent captured these in a native GUI acceptance run against base `bf52dee0003b` plus its working-tree mic changes. The title's base SHA alone does **not** reproduce that candidate. These captures predate the final combined mic/seek snapshot and are labeled accordingly. The QA media itself is not published.

### Input signal before recording

![Input 1 mono selected and track meter showing signal while stopped](screenshots/2026-11-22/mic-input-meter.png)

### During capture

![Track REC timer, live input level and growing waveform during capture](screenshots/2026-11-22/mic-recording.png)

### Physical channel selection

![Physical input mono and stereo choices](screenshots/2026-11-22/mic-channel-selection.png)

### Saved take reopened

![Recorded take after saving and reopening](screenshots/2026-11-22/mic-take-reopened.png)

## 21 November: integrated baseline

These are native integration captures associated with source `028e2b1a8cd530af99fcc5e0910a930b075e1167`, before the 22 November recording/seek follow-ups. See the [baseline report](agent-reports/2026-11-21-beta-integration-followup-report.md).

### Arrangement and editing

![Arrangement with audio and MIDI clips](screenshots/beta1-arrangement-batch-1540.png)

### Session and mixer

![Independent Session scenes with mixer strips](screenshots/beta1-session-playing-1540.png)

### Compact native effects

![Native EQ and effects in compact Devices window](screenshots/beta1-fx-chain-980.png)

### SoundFont instrument controls

![SoundFont instrument panel](screenshots/beta1-soundfont-1540.png)

### Explicit export limitation

![Export guard for enabled live inserts](screenshots/beta1-export-guard-1540.png)

## Historical visuals

The `midnight-ember-*` captures date to the 17 November UI phase; `showcase/*2026-11-19.png` captures date to 19 November. They are retained for history, not used as proof of current feature completion. Original dimensions and pixels are retained; no decorative UI was painted into these captures.
