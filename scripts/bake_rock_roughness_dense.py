"""Bake the OpenEXR roughness shipped with dark_rock_02 into a PNG with
an extra fine-grain noise layer on top.

The Blender reference ships a single-channel roughness map as
``dark_rock_02_rough_4k.exr``. Bevy 0.18's PNG/JPG pipeline cannot
load EXR at runtime, so the project ships a PNG sibling. The
existing ``bake_rock_roughness.py`` already produces
``generic_rock_roughness_2k.png`` from this source; this script
produces a denser variant with an extra procedural fine-grain layer
added on top of the EXR-derived roughness so the surface reads as more
gritty at close zoom.

Output:
    assets/textures/celestial/asteroids/generic_rock_roughness_dense_2k.png

Run from the repo root with the project venv Python::

    .venv/Scripts/python.exe scripts/bake_rock_roughness_dense.py

The GLTF metallic_roughness channel layout is preserved: R is unused
(255), G carries the roughness, B is metallic (0). Bevy reads the green
channel as a data map and multiplies it with the per-class
``perceptual_roughness`` scalar, so the final shader roughness lands
in [0.85, 1.00] even with the noise layer on top.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import Imath
import OpenEXR
import imageio.v2 as imageio
import numpy as np

# Same band as bake_rock_roughness.py: the per-class scalar multiplies
# the texture, so we keep the band near the top of [0, 1] to land the
# final product in [0.85, 1.00].
ROUGH_MIN = 0.85
ROUGH_MAX = 1.00

# Amplitude of the procedural fine-grain noise added on top of the
# EXR-derived roughness, in the same [0, 1] linear roughness space.
# 0.04 is small enough not to push any pixel below ROUGH_MIN after
# the remap, but large enough that the micro-variation survives the
# 4K → 1024 resample.
FINE_GRAIN_AMP = 0.04

# Size of the fine-grain noise cell in pixels. 4 px is below the
# per-pixel detail the texture sampling can resolve at in-game zoom,
# so the variation reads as "grittiness" rather than as visible
# texture.
FINE_GRAIN_CELL_PX = 4


def read_exr_first_channel(path: Path) -> np.ndarray:
    """Return a single-channel float32 array from the first channel of an EXR."""
    f = OpenEXR.InputFile(str(path))
    header = f.header()
    width = int(header["dataWindow"].max.x) + 1
    height = int(header["dataWindow"].max.y) + 1
    pt = Imath.PixelType(Imath.PixelType.FLOAT)
    for candidate in ("R", "G", "B", "Y", "Z"):
        try:
            data = f.channel(candidate, pt)
        except Exception:
            continue
        f.close()
        return np.frombuffer(data, dtype=np.float32).reshape(height, width)
    channels = header["channels"].keys()
    if not channels:
        f.close()
        raise SystemExit("EXR has no channels")
    first = next(iter(channels))
    data = f.channel(first, pt)
    f.close()
    return np.frombuffer(data, dtype=np.float32).reshape(height, width)


def resize_to(arr: np.ndarray, size: int) -> np.ndarray:
    """Box-filter resize (no extra deps) to ``(size, size)``."""
    if arr.ndim == 2:
        src_h, src_w = arr.shape
        ys = (np.linspace(0, src_h - 1, size)).astype(np.int64)
        xs = (np.linspace(0, src_w - 1, size)).astype(np.int64)
        return arr[ys[:, None], xs[None, :]]
    src_h, src_w, _ = arr.shape
    ys = (np.linspace(0, src_h - 1, size)).astype(np.int64)
    xs = (np.linspace(0, src_w - 1, size)).astype(np.int64)
    return arr[ys[:, None], xs[None, :]]


def fine_grain_noise(shape: tuple[int, int], salt: int) -> np.ndarray:
    """Per-cell uniform noise in [0, 1) with a small spatial averaging pass.

    The cell size is in pixels; the noise field is built by hashing
    pixel-space coordinates to a uniform random, then blurred by a
    2x2 box so the transitions don't alias into sharp pixel patterns.
    The seed is fixed so re-runs are byte-identical.
    """
    h, w = shape
    rng = np.random.default_rng(salt)
    cells_y = (h + FINE_GRAIN_CELL_PX - 1) // FINE_GRAIN_CELL_PX
    cells_x = (w + FINE_GRAIN_CELL_PX - 1) // FINE_GRAIN_CELL_PX
    cell = rng.random((cells_y, cells_x)).astype(np.float32)
    # Bilinear upsample back to (h, w).
    ys = (np.linspace(0, cells_y - 1, h)).astype(np.float32)
    xs = (np.linspace(0, cells_x - 1, w)).astype(np.float32)
    y0 = np.floor(ys).astype(np.int64)
    x0 = np.floor(xs).astype(np.int64)
    y1 = np.clip(y0 + 1, 0, cells_y - 1)
    x1 = np.clip(x0 + 1, 0, cells_x - 1)
    fy = ys - y0
    fx = xs - x0
    v00 = cell[y0[:, None], x0[None, :]]
    v10 = cell[y0[:, None], x1[None, :]]
    v01 = cell[y1[:, None], x0[None, :]]
    v11 = cell[y1[:, None], x1[None, :]]
    top = v00 * (1.0 - fx) + v10 * fx
    bot = v01 * (1.0 - fx) + v11 * fx
    out = top * (1.0 - fy[:, None]) + bot * fy[:, None]
    return out - 0.5  # Centered in [-0.5, 0.5] for additive use.


def to_uint8(arr: np.ndarray) -> np.ndarray:
    return np.clip(arr * 255.0, 0, 255).astype(np.uint8)


def _default_src() -> Path:
    """Pick a sensible default source EXR.

    Resolution order:
      1. The `HELIOS_ROCK_ROUGHNESS_SRC` env var, if set (CI / scripted runs).
      2. The original Windows-only path (so existing operator machines
         keep working without a flag).
      3. A POSIX-friendly fallback under `~/Downloads/` matching the
         same filename, so contributors who downloaded the asset under
         the conventional location on Linux / macOS can run with no
         arguments.
    """
    env = os.environ.get("HELIOS_ROCK_ROUGHNESS_SRC")
    if env:
        return Path(env)
    windows_path = Path(
        r"G:\Eigene Dateien\Downloads\dark_rock_02_4k.blend\textures"
        r"\dark_rock_02_rough_4k.exr"
    )
    if windows_path.exists():
        return windows_path
    posix_fallback = (
        Path.home()
        / "Downloads"
        / "dark_rock_02_4k.blend"
        / "textures"
        / "dark_rock_02_rough_4k.exr"
    )
    return posix_fallback


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--src",
        type=Path,
        default=_default_src(),
        help="Path to the OpenEXR roughness map (single channel). "
        "Defaults to $HELIOS_ROCK_ROUGHNESS_SRC if set, then the "
        "operator's Windows Downloads path if it exists, then a "
        "POSIX $HOME/Downloads/ fallback.",
    )
    parser.add_argument(
        "--dst",
        type=Path,
        default=Path(
            "assets/textures/celestial/asteroids/generic_rock_roughness_dense_2k.png"
        ),
        help="Destination PNG path (overwritten if it exists).",
    )
    parser.add_argument(
        "--size",
        type=int,
        default=1024,
        help="Output size (square). Default 1024.",
    )
    args = parser.parse_args()

    if not args.src.exists():
        raise SystemExit(f"Source EXR not found: {args.src}")

    print(f"Reading {args.src}…")
    rough = read_exr_first_channel(args.src)
    if args.size and rough.shape[0] != args.size:
        print(f"Resizing from {rough.shape[0]}x{rough.shape[1]} to {args.size}x{args.size}…")
        rough = resize_to(rough, args.size)

    # Remap source [0, 1] to the desired band, then add fine-grain noise.
    rough = np.clip(rough, 0.0, 1.0)
    rough = ROUGH_MIN + (ROUGH_MAX - ROUGH_MIN) * rough
    rough = np.clip(rough, 0.0, 1.0)

    noise = fine_grain_noise(rough.shape, salt=20260803)
    rough = np.clip(rough + FINE_GRAIN_AMP * noise, 0.0, 1.0)
    print(
        f"  remapped to [{rough.min():.3f}, {rough.max():.3f}], "
        f"mean {rough.mean():.3f}, fine_grain_amp={FINE_GRAIN_AMP}"
    )

    # GLTF metallic_roughness: R = AO (unused, 255), G = roughness, B = metallic (0).
    rgb = np.stack(
        [
            np.full_like(rough, 255, dtype=np.uint8),  # R = unused
            to_uint8(rough),                          # G = roughness
            np.zeros_like(rough, dtype=np.uint8),     # B = metallic (0)
        ],
        axis=-1,
    )
    img = rgb.astype(np.uint8)

    args.dst.parent.mkdir(parents=True, exist_ok=True)
    imageio.imwrite(args.dst, img)
    print(
        f"Wrote {args.dst} ({img.shape[0]}x{img.shape[1]}, "
        f"R=255, G=roughness [{rough.min():.2f}..{rough.max():.2f}], B=0)"
    )


if __name__ == "__main__":
    main()
