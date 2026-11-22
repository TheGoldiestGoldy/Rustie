# Desktop build and AIR 192|6 check

## Changes

Promoted the previously verified UI and worker pair for source revision 028e2b1a8cd5 from the isolated QA installation to the workstation's versioned Rusty installation. Switched the current symlink after checking both executable hashes. The previous version remains available. This update contains the integrated recording input selection and editing changes already documented in the November 21 report.

## Current state

The desktop launcher now opens the verified revision. The existing Beta 1 NO-GO decision still applies. This deployment did not include UI or logo changes.

## Verification

- Both executables passed their original SHA-256 manifest.
- `rusty --version` reported v0.1.0-dev (028e2b1a8cd5). Launched via `gtk-launch rusty` and checked the process executable.
- `rusty --probe-input` enumerated AIR 192 6 Analog Stereo through PulseAudio at 44.1 kHz. It did not capture microphone audio.
- Three consecutive silent runs of `RUSTIE_PROBE_RATE=44100 rusty --probe-transport 128 1000` passed on the AIR 192|6 PulseAudio sink. Callback advancement took 1.05 to 1.08 ms after Play. These measurements exclude acoustic output.

## Open issues and next checks

The owner reported no usable input and no sound on the earlier desktop build, with ALSA open errors in its launcher log. Physical input selection and recording through the GUI, audible playback, monitoring, alignment and the ALSA path remain unverified. Check the simple Audio track workflow on the matched PulseAudio build first. Keep the earlier intermittent startup defect open until it passes independent regression checks. The owner also reported rough UI and weak logo treatment.

## Git and handoff

The integration branch was at bf52dee, with 028e2b1 as the verified executable source. This report is the only checkout change. No commit or push was made. The installed pair and manifest are in `~/.local/opt/rusty/versions/028e2b1a8cd5/`. Record the visible build ID, selected input, and whether the track meter and output move when checking any follow-up failure.
