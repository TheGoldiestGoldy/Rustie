# Known limitations

**22 November 2026: development preview; Beta 1 remains NO-GO.**

- PulseAudio startup sometimes failed to advance in one-second probes. Longer baseline retries first advanced after roughly 0.85–2.0 seconds. New seek behavior must not be described as fixing initial device startup without a specific retest.
- Physical headphone acceptance, reference-transient recording alignment, analogue round-trip latency and backend XRUN/deadline coverage remain separate gates.
- Live-insert Arrangement export is guarded: enabled inserts block export. Use verified printed audio and bypass inserts; native-state recall does not mean offline export renders that chain.
- Plug-in compatibility is limited to named combinations. The installed LSP VST3 bundle has a class-metadata failure. Worker isolation is process fault containment, not an OS security sandbox or a promise of inaudible failure.
- Long audio is decoded into RAM; high-quality conversion and disk streaming are future work.
- SoundFont functionality needs a local bank. The repository's tiny self-created test fixture is not a musical sound library.
- Full production automation, warp/time stretch, comprehensive routing/PDC, broad interchange and long-session recovery need further work. Reserved parser/client crates do not imply completed `.als` support.
- Autonatural is a separate offline research workflow. Its synthetic evidence does not establish natural-sounding vocals; its experimental CLAP latency is unsuitable for live vocal monitoring at the reported 400 ms setting.
- Distribution notices and contributor-rights confirmation remain open. This source publication does not declare Beta 1 shippable.

[Release decision](BETA1_RELEASE_DECISION.md) · [Current follow-ups](PROJECT_STATUS.md) · [Verification](VERIFICATION.md)
