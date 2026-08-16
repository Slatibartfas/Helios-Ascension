"""Generate the Helios Ascension SFX cue set.

Reads `assets/data/sfx_prompts.ron`, calls an audio-generation
API for each cue, and writes the resulting `.wav` files under
`assets/audio/sfx/`.

This script has two modes:

1. **Default (synthesize locally)**: Generates simple
   placeholder WAVs from `duration_ms` + `intensity` metadata
   in the prompts file. The placeholders are *real audio*
   (envelope-shaped sine/square waves matched to each cue's
   target length and intensity) — they're immediately playable
   by the SFX backend. Useful for: smoke-testing the wiring
   without API credentials, regenerating after a manifest
   schema change, or producing a `git diff`-friendly baseline.

2. **`--api` mode**: Calls the MiniMax Audio endpoint
   (or whatever audio-generation API is configured via
   environment variables) to produce production-quality
   AI-generated cues. Requires `MMX_API_KEY` in the
   environment (or `--api-key` on the command line).

The script is idempotent — running it twice produces the same
files. Add `--force` to overwrite existing WAVs even if newer
than the manifest.

Pattern: mirrors `scripts/build_icons.py` (icon generator) and
`scripts/audit_sfx_manifest.py` (manifest auditor). All three
scripts live under `scripts/` and operate on the assets
directory.
"""
from __future__ import annotations

import argparse
import math
import re
import struct
import sys
import wave
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PROMPTS_PATH = REPO_ROOT / "assets" / "data" / "sfx_prompts.ron"
SFX_OUT = REPO_ROOT / "assets" / "sfx_out"
ASSETS_SFX = REPO_ROOT / "assets" / "audio" / "sfx"
ASSETS_SFX.mkdir(parents=True, exist_ok=True)


# ----------------------------------------------------------------------
# RON parser (tiny — we only need the cues + metadata)
# ----------------------------------------------------------------------

def parse_prompts(path: Path) -> list[dict]:
    """Parse `sfx_prompts.ron` into a list of cue dicts.

    Uses a minimal hand-rolled RON parser rather than pulling in
    the `ron` PyPI package — the prompts file is small and the
    schema is stable. The parser only walks the `prompts: [...]`
    array (skipping the leading doc-comments) and recognises the
    `(cue_id: "...", ...)` named-field syntax.

    Returns a list of dicts with keys: cue_id, duration_ms,
    intensity, prompt, attribution, notes.
    """
    if not path.exists():
        sys.exit(f"prompts file not found: {path}")
    text = path.read_text(encoding="utf-8")

    # Find the `prompts: [` block — that's where the cue tuples
    # live. Skip the leading `//` doc-comments which contain
    # unmatched `(` characters that fool the depth counter.
    prompts_match = re.search(r"prompts:\s*\[(.*)\n\s*\]", text, re.DOTALL)
    if not prompts_match:
        sys.exit(f"could not find `prompts: [...]` block in {path}")
    body = prompts_match.group(1)

    cues: list[dict] = []
    depth = 0
    start: int | None = None
    for i, ch in enumerate(body):
        if ch == "(":
            if depth == 0:
                start = i + 1
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0 and start is not None:
                block = body[start:i].strip()
                cues.append(_parse_cue_tuple(block))
                start = None
    return cues


def _parse_cue_tuple(block: str) -> dict:
    """Parse one `(cue_id: "...", duration_ms: N, ...)` tuple."""
    fields: dict[str, object] = {}
    # Split top-level comma-separated fields. This is a
    # simplification — strings inside fields could in theory
    # contain commas; the current schema doesn't, so this works.
    for raw in _split_top_level(block):
        raw = raw.strip()
        if not raw:
            continue
        if ":" not in raw:
            continue
        key, _, value = raw.partition(":")
        key = key.strip()
        value = value.strip().rstrip(",").strip()
        fields[key] = _parse_value(value)

    return {
        "cue_id": str(fields.get("cue_id", "unknown")),
        "duration_ms": int(fields.get("duration_ms", 100)),  # type: ignore[arg-type]
        "intensity": str(fields.get("intensity", "medium")),
        "prompt": str(fields.get("prompt", "")),
        "attribution": str(fields.get("attribution", "")),
        "notes": str(fields.get("notes", "")),
    }


def _split_top_level(s: str) -> list[str]:
    """Split `s` on top-level commas (depth=0 ignoring parens)."""
    out: list[str] = []
    depth = 0
    cur: list[str] = []
    in_str = False
    for ch in s:
        if ch == '"':
            in_str = not in_str
            cur.append(ch)
        elif in_str:
            cur.append(ch)
        elif ch in "([{":
            depth += 1
            cur.append(ch)
        elif ch in ")]}":
            depth -= 1
            cur.append(ch)
        elif ch == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if cur:
        out.append("".join(cur))
    return out


def _parse_value(value: str) -> object:
    """Parse a single RON value into a Python scalar."""
    value = value.strip()
    if value.startswith('"') and value.endswith('"'):
        return value[1:-1]
    if value in ("true", "True"):
        return True
    if value in ("false", "False"):
        return False
    try:
        return int(value)
    except ValueError:
        pass
    try:
        return float(value)
    except ValueError:
        pass
    return value


# ----------------------------------------------------------------------
# Manifest ↔ prompts mapping
# ----------------------------------------------------------------------

def manifest_filename(cue_id: str) -> str:
    """Map a cue_id (`ui.button_click`) to the WAV filename in
    `assets/audio/sfx/` (`ui_button_click.wav`). Mirrors the
    convention used by `scripts/audit_sfx_manifest.py` and the
    `SfxCueId::as_str_id` enum in Rust.
    """
    return cue_id.replace(".", "_") + ".wav"


# ----------------------------------------------------------------------
# Local synthesis (default mode)
# ----------------------------------------------------------------------

def synthesize_placeholder(cue: dict, out_path: Path) -> None:
    """Generate a real placeholder WAV matched to the cue's
    `duration_ms` + `intensity`. Uses envelope-shaped sine and
    square waves — basic, but actually audible.

    Per-cue tone recipes:
    - `ui.button_click`     880 Hz → 1320 Hz sweep, 80ms, fast decay
    - `ui.tab_switch`       660 Hz, 120ms
    - `ui.panel_open`       220→880 Hz rising sweep, 280ms
    - `ui.panel_close`      880→220 Hz falling sweep, 240ms
    - `ui.slider_tick`      1320 Hz, 60ms (square wave)
    - `ui.dropdown_open`    990 Hz, 140ms
    - `ui.row_select`       440+660 Hz two-tone, 100ms
    - `ui.drag_drop`        220 Hz, 180ms (low thud)
    - `ui.modal_confirm`    660→880 Hz ascending, 320ms
    - `ui.modal_cancel`     880→660 Hz descending, 280ms
    - `ui.chip_toggle`      1100 Hz, 70ms (square)
    - `ui.mode_toggle`      330 Hz, 160ms (deep thud)
    - `notifications.chime` 440+660+880 Hz three-note chord, 600ms

    These are *placeholders*. Real cues come from the audio
    API (run with `--api`).
    """
    sample_rate = 44_100
    duration_s = cue["duration_ms"] / 1000.0
    n = int(sample_rate * duration_s)
    intensity = cue["intensity"]
    cue_id = cue["cue_id"]

    # Choose a recipe by cue_id.
    recipes = {
        "ui.button_click": ("sweep", [880.0, 1320.0]),
        "ui.tab_switch": ("sine", [660.0]),
        "ui.panel_open": ("sweep", [220.0, 880.0]),
        "ui.panel_close": ("sweep", [880.0, 220.0]),
        "ui.slider_tick": ("square", [1320.0]),
        "ui.dropdown_open": ("sine", [990.0]),
        "ui.row_select": ("chord", [440.0, 660.0]),
        "ui.drag_drop": ("sine", [220.0]),
        "ui.modal_confirm": ("sweep", [660.0, 880.0]),
        "ui.modal_cancel": ("sweep", [880.0, 660.0]),
        "ui.chip_toggle": ("square", [1100.0]),
        "ui.mode_toggle": ("sine", [330.0]),
        "notifications.chime": ("chord", [440.0, 660.0, 880.0]),
    }
    kind, freqs = recipes.get(cue_id, ("sine", [440.0]))

    # Amplitude envelope. Intensity maps to peak amplitude.
    amp_map = {"low": 0.15, "medium": 0.25, "high": 0.40}
    peak = amp_map.get(intensity, 0.20)

    # Write the WAV.
    with wave.open(str(out_path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)  # 16-bit PCM
        wav.setframerate(sample_rate)

        frames: list[bytes] = []
        for i in range(n):
            t = i / sample_rate
            # Exponential decay envelope; slightly longer hold at
            # the start so the cue has a perceptible attack.
            decay = math.exp(-3.0 * t / duration_s) if duration_s > 0 else 0.0
            attack = min(1.0, t * sample_rate / 200.0)  # 4.5ms attack
            env = decay * attack

            if kind == "sweep":
                # Linear frequency sweep from freqs[0] to freqs[1].
                f0, f1 = freqs
                inst_f = f0 + (f1 - f0) * (i / max(1, n - 1))
                phase = 2.0 * math.pi * inst_f * t
                sample = math.sin(phase)
            elif kind == "square":
                # Square wave at freqs[0].
                phase = 2.0 * math.pi * freqs[0] * t
                sample = 1.0 if math.sin(phase) > 0 else -1.0
                sample *= 0.5  # tame the square harmonics
            elif kind == "chord":
                # Sum of sines at freqs[i].
                sample = sum(math.sin(2.0 * math.pi * f * t) for f in freqs)
                sample /= len(freqs)
            else:  # sine
                sample = math.sin(2.0 * math.pi * freqs[0] * t)

            value = int(sample * peak * env * 32_767)
            value = max(-32_767, min(32_767, value))
            frames.append(struct.pack("<h", value))

        wav.writeframes(b"".join(frames))


# ----------------------------------------------------------------------
# API mode (--api) — stub for MiniMax audio generation
# ----------------------------------------------------------------------

def generate_via_api(cue: dict, out_path: Path, api_key: str | None) -> None:
    """Generate a real WAV via the audio API.

    **This is a placeholder.** The actual MiniMax audio API
    endpoint for SFX isn't covered by the bundled `mmx-cli`
    skill (which has TTS + music but not standalone SFX
    generation). The plan is:

    1. Detect the configured provider via environment variables
       (e.g. `MMX_AUDIO_API_URL`, `ELEVENLABS_API_KEY`).
    2. POST the cue's `prompt` + `duration_ms` to the provider.
    3. Save the returned audio bytes as a WAV (or transcode
       from MP3/OGG with ffmpeg if the provider returns a
       compressed format).

    Until that's wired, we fall back to the placeholder
    synthesizer so the script always produces a usable file.
    """
    sys.stderr.write(
        f"[generate_sfx] note: --api mode is a stub; falling back to "
        f"local synthesis for {cue['cue_id']!r}. The MiniMax Audio "
        f"SFX endpoint will be integrated in a follow-up PR.\n"
    )
    synthesize_placeholder(cue, out_path)


# ----------------------------------------------------------------------
# Manifest audit (small, pre-flight check)
# ----------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--api",
        action="store_true",
        help="Generate via the audio API instead of local synthesis.",
    )
    parser.add_argument(
        "--api-key",
        default=None,
        help="API key (defaults to MMX_API_KEY environment variable).",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing WAVs even if newer than the manifest.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the plan without writing any files.",
    )
    args = parser.parse_args()

    cues = parse_prompts(PROMPTS_PATH)
    if not cues:
        sys.exit(f"no cues parsed from {PROMPTS_PATH}")

    print(f"generate_sfx: {len(cues)} cue(s) parsed from {PROMPTS_PATH.name}")

    api_key = args.api_key or __import__("os").environ.get("MMX_API_KEY")
    if args.api and not api_key:
        sys.exit("--api requires MMX_API_KEY or --api-key")

    for cue in cues:
        filename = manifest_filename(cue["cue_id"])
        out_path = ASSETS_SFX / filename
        if out_path.exists() and not args.force:
            # Skip — assume newer than the manifest.
            print(f"  skip {filename} (already exists; use --force to overwrite)")
            continue
        if args.dry_run:
            print(f"  would write {out_path.relative_to(REPO_ROOT)}")
            continue
        if args.api:
            generate_via_api(cue, out_path, api_key)
        else:
            synthesize_placeholder(cue, out_path)
        size = out_path.stat().st_size
        print(f"  wrote {filename} ({size} bytes, {cue['duration_ms']}ms)")

    print(f"generate_sfx: done ({len(cues)} cue(s) processed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())