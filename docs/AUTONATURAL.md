# Autonatural: the vocal research lane

Autonatural explores pitch correction that moves a contiguous vocal note's center toward a musical target while retaining expression around that center. A tolerance band is intended to avoid unnecessary correction. Unvoiced or low-confidence material is intended to stay dry.

## What exists

A separate Rust DSP project provides analysis, region planning and experimental TD-PSOLA resynthesis. A CLI renders new WAVs. An isolated Rustie Autonatural Lab invokes that CLI in a worker process, imports the result at the corresponding timeline position and offers Original/Tuned A/B controls through track mute states. Source audio is read rather than overwritten.

The 21 November Lab port reported 90 workspace tests, strict Clippy, release build and two synthetic end-to-end smoke cases. Each produced 96,000 stereo frames at 48 kHz and verified unchanged input bytes. Those are Lab results, separate from the main DAW's test count.

## What has not been demonstrated

Naturalness on real singers; timbre/formant preservation; consonant and sibilant handling; vibrato preservation across registers; reliable automatic key detection; comprehensive quality gates G1–G11; useful live monitoring latency; owner-operated A/B acceptance on the updated Lab.

The separate experimental CLAP wrapper reports 400 ms latency at 48 kHz. The Lab's vocal workflow is offline and does not load that wrapper. Main DAW CLAP hosting does not establish working live Autonatural integration.

## Next experiment

Use consented dry monophonic vocal recordings covering different ranges, vibrato and transitions. Preserve originals, render fixed settings, level-match A/B comparisons, inspect pitch and timing, and ask listeners which take they prefer and why. Measure time-to-result separately from sound quality. Publish failures as well as successful examples before positioning this as a reason to switch DAWs.

## Source provenance

The Lab and DSP are separate local projects; this publication includes their documentation for context, not their executables, personal recordings or an implied merge into Rusty. Historical snapshots: [Lab overview](research/AUTONATURAL_LAB.md), [Lab port evidence](research/AUTONATURAL_LAB_PORT_REPORT.md), [DSP overview and limitations](research/AUTONATURAL_DSP.md). Paths in those snapshots describe the original development environment and are not installation instructions for this repository.
