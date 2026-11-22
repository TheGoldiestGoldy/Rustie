# SoundFont banks for the built-in instrument rack

Drop one or more **SoundFont `.sf2`** files in this folder (or in `~/Music`,
`~/Downloads`, or next to the installed executable). The built-in instrument
engine parses them with [`rustysynth`](https://github.com/sinshu/rustysynth) and
maps the curated rack presets to **General MIDI (GM)** bank/program numbers, so
a single GM-compatible bank can serve every instrument in the browser:
**Pianos & Keys**, **Bass**, **Strings & Brass**, **Classic Synths**, and
**Drums & Percussion**.

The app discovers `.sf2` files here at startup; press **Rescan SoundFonts** in
the Library (or reopen the app) after adding a new bank. If no `.sf2` is
present, MIDI tracks fall back to the built-in sine synth and the rack shows a
clear "no bank" message — nothing crashes.

## Recommended open-license banks

| Bank | License | Notes |
| --- | --- | --- |
| [GeneralUser GS](https://schristiancollins.com/generaluser.php) | Free (no-commercial use allowed; see site) | Compact (~30 MB), excellent GM coverage, best first choice for the rack. |
| [FluidR3 GM](https://github.com/FluidSynth/fluidsynth/wiki/SoundFont) | MIT | ~145 MB, well-balanced General MIDI set. |
| [Salamander Grand Piano](https://github.com/sfzinstruments/SalamanderGrandPiano) | CC0 | ~40 MB grand piano only (not GM); assign the "Acoustic Piano" preset to hear it, but other GM presets may be missing. |
| [Musyng Kite](https://github.com/RenderedSmall/MusyngKite) | CC0 | Large, detailed GM set (multi-hundred MB). |

For the full curated rack (all five categories), prefer a **GM-compatible**
bank such as **GeneralUser GS** or **FluidR3 GM**. `rustysynth` falls back to
the closest available preset when a specific GM program or drum kit is missing
from a bank.

## How the rack maps presets to GM

- Melodic presets are sent on MIDI channel 1 (index 0) with their GM program
  number (for example, Acoustic Piano = program 0, Rhodes = program 4, Trumpet
  = program 56, Analog Lead = program 81).
- **Drums & Percussion** presets use MIDI channel 10 (index 9) and a GM
  percussion kit (Standard = program 0, Room = 8, Power = 16, Electronic = 24,
  TR-808 = 25).

## License reminder

Only place `.sf2` files here whose license permits your use (including
redistribution, if you ship the project). The app itself contains no bundled
SoundFont data; banks are user-supplied.
