#!/usr/bin/env python3
"""Generate a reproducible, synthetic Slate Studio UX review project.

No packages, network, account data, or proprietary loops required. Never
replace a reviewer's existing project or media; personal edits are preserved.
"""
import json
import math
import struct
import sys
import wave
from pathlib import Path

SAMPLE_RATE = 48_000
BPM = 120
PPQ = 960
BARS_PER_SCENE = 4
SCENE_TICKS = BARS_PER_SCENE * 4 * PPQ
DURATION_S = BARS_PER_SCENE * 4 * 60 / BPM


def make_loop(path: Path, variant: int):
    if path.exists():
        return False
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(SAMPLE_RATE)
        block = bytearray()
        count = int(DURATION_S * SAMPLE_RATE)
        for i in range(count):
            t = i / SAMPLE_RATE
            beat_phase = (t * BPM / 60) % 1.0
            offbeat = (t * BPM / 60 + 0.5) % 1.0
            kick = 0.14 * math.exp(-beat_phase * 17) * math.sin(2 * math.pi * (63 + (1 - beat_phase) * 22) * t)
            hat = 0.023 * math.exp(-offbeat * 35) * math.sin(2 * math.pi * 8100 * t)
            chord = 0.036 * math.sin(2 * math.pi * (220 if variant == 0 else 196) * t)
            if variant:
                chord += 0.019 * math.sin(2 * math.pi * 294 * t)
            sample = (kick + hat + chord) * min(1.0, t * 24, (DURATION_S - t) * 24)
            v = max(-32768, min(32767, int(sample * 32767)))
            block.extend(struct.pack('<hh', v, v))
            if len(block) >= 16384:
                out.writeframes(block)
                block.clear()
        if block:
            out.writeframes(block)
    return True


def midi_note(pitch, beat, duration=0.8, velocity=91):
    return {"pitch": pitch, "start": round(beat * PPQ),
            "duration": round(duration * PPQ), "velocity": velocity, "muted": False}


def midi_clip(id_, scene, notes):
    return {"id": id_, "start": scene * SCENE_TICKS, "duration": SCENE_TICKS,
            "kind": {"Midi": {"notes": notes}}, "scale_override": None}


def audio_clip(id_, scene, path):
    return {"id": id_, "start": scene * SCENE_TICKS, "duration": SCENE_TICKS,
            "kind": {"Audio": {"source": str(path), "warped": False}},
            "scale_override": None}


def track(id_, name, kind, clips, volume=0.5):
    return {"id": id_, "name": name, "kind": kind, "clips": clips,
            "volume": volume, "pan": 0.0, "muted": False, "solo": False}


def project_for(folder: Path, files):
    melody_a = [midi_note(p, beat) for beat, p in enumerate(
        [60, 64, 67, 72, 71, 67, 64, 62, 60, 65, 69, 72, 67, 64, 62, 60])]
    melody_b = [midi_note(p, beat, .82, 86) for beat, p in enumerate(
        [72, 71, 69, 67, 74, 72, 71, 69, 67, 65, 64, 62, 60, 62, 64, 67])]
    chord_a = [midi_note(p, beat, 3.65, 67) for beat, ps in
               [(0, [60, 64, 67]), (4, [57, 60, 64]), (8, [53, 57, 60]), (12, [55, 59, 62])]
               for p in ps]
    chord_b = [midi_note(p, beat, 3.65, 67) for beat, ps in
               [(0, [62, 65, 69]), (4, [60, 64, 67]), (8, [59, 62, 67]), (12, [60, 64, 67])]
               for p in ps]
    bass_a = [midi_note(p, beat, 1.65, 76) for beat, p in
              [(0, 36), (2, 36), (4, 33), (6, 33), (8, 29), (10, 29), (12, 31), (14, 31)]]
    bass_b = [midi_note(p, beat, 1.65, 76) for beat, p in
              [(0, 38), (2, 38), (4, 36), (6, 36), (8, 35), (10, 35), (12, 36), (14, 36)]]
    return {
        "name": "Rustie UX Review | Safe Test Song",
        "tempo_map": {"changes": {"0": {"micro_bpm": BPM * 1_000_000}}},
        "time_signature": {"numerator": 4, "denominator": 4},
        "global_scale": {"root": 0, "mode": "Major"},
        "tracks": [
            track(1, "01 | Melody - edit notes", "Midi", [midi_clip(2, 0, melody_a), midi_clip(3, 1, melody_b)], 0.44),
            track(4, "02 | Chords - edit harmony", "Midi", [midi_clip(5, 0, chord_a), midi_clip(6, 1, chord_b)], 0.30),
            track(7, "03 | Bass - test mixing", "Midi", [midi_clip(8, 0, bass_a), midi_clip(9, 1, bass_b)], 0.29),
            track(10, "04 | Original audio loop", "Audio", [audio_clip(11, 0, files[0]), audio_clip(12, 1, files[1])], 0.32),
        ],
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: generate-ux-review.py TARGET_DIRECTORY")
    folder = Path(sys.argv[1]).expanduser().resolve()
    folder.mkdir(parents=True, exist_ok=True)
    a, b = folder / "original-beat-A.wav", folder / "original-beat-B.wav"
    for variant, path in enumerate([a, b]):
        if make_loop(path, variant):
            print(f"Created audio: {path}")
    project_file = folder / "Rustie UX Review.slate.json"
    if not project_file.exists():
        project_file.write_text(json.dumps(project_for(folder, (a, b)), indent=2) + "\n")
        print(f"Created project: {project_file}")
    else:
        print(f"Preserved existing review project: {project_file}")
    print(f"REVIEW_PROJECT={project_file}")


if __name__ == "__main__":
    main()
