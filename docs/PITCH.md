# The Rusty pitch

## One sentence

Rusty is a native Linux music workstation exploring a faster, clearer path from a real performance to a saved song—with vocal expression as a research priority and verification as part of the product's public record.

## Thirty seconds

We are building a DAW in Rust because we want a music-making environment we can understand, inspect and shape around actual recording sessions on Linux. Rusty already combines an Arrangement, an independent Session grid, MIDI editing, audio capture, native effects and tested external instruments in a native desktop application. Alongside it, the Autonatural Lab explores tuning a vocal note's center while keeping the singer's movement and expression. We are publishing the code, working UI and evidence as we go, including the failures that still prevent a production release.

## Why try this?

The motivating question is practical: can we make it easier to capture an idea, understand what the system is doing, edit it and keep a result worth listening to?

Recent owner testing made that question concrete. Imported WAVs played and sessions reopened, but a mic take could show a waveform while remaining hard to hear. The track gave too little feedback before and during capture. Clicking the place to audition was also tedious. Those observations led to specific channel-routing, input-meter, recording-preview and seeking work. They are product feedback, not hypothetical feature requests.

Linux is the starting environment, with native audio and no Wine requirement for Rusty itself. Familiar Arrangement and Session conventions reduce the amount a musician has to relearn. The opportunity is to make the entire workflow coherent and trustworthy rather than make every individual control novel.

## The product hypothesis

A singer should be able to record, see that capture is working, hear a centered take, correct pitch with restraint, compare against the original and keep the version they prefer without losing the performance or managing a chain of disconnected experiments.

The main DAW supplies the foundation. Autonatural currently explores the vocal part in a separate offline alpha. This is a hypothesis to test, not a claim that the integrated experience already exists or outperforms established tools.

## What makes the experiment worth watching

- A native Rust desktop implementation with real render/capture paths.
- Musician feedback that directly changes the interface and audio workflow.
- A separate, reversible research lane for expressive vocal correction.
- AI-assisted implementation accompanied by source review, regression checks, runtime evidence and honest release gates.
- An inspectable development history that separates plans from delivered behavior.

## What success would mean

First: reliably make, save, reopen and export a real song on supported hardware. Then: show that the vocal workflow helps people reach a result they prefer, with fewer confusing steps. Finally: demonstrate that reliability across machines, sessions and supported plug-ins is repeatable.

We do not yet have listening-panel results, workflow timing studies, adoption data, revenue claims or a committed v1 date. [Evidence](VERIFICATION.md) · [Roadmap](ROADMAP.md) · [Competitive context](DIFFERENTIATION.md)
