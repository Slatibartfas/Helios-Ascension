"""Bake the OpenEXR normal map shipped with the dark_rock_02_4k reference
into a Bevy-compatible tangent-space normal PNG.

The Blender file (G:\\Eigene Dateien\\Downloads\\dark_rock_02_4k.blend)
ships a real tangent-space normal at:

    G:\\Eigene Dateien\\Downloads\\dark_rock_02_4k.blend\\textures\\
        dark_rock_02_nor_gl_4k.exr

Bevy 0.18's PNG/JPG pipeline does not load OpenEXR at runtime, so the
project still has to ship a PNG. This helper converts the EXR to a
8-bit tangent-space normal PNG and writes it next to the other
asteroid textures:

    assets/textures/celestial/asteroids/generic_rock_normal_2k.png

The normal is encoded in DirectX-style ``(x, y, z)`` half-floats in
``B, G, R`` channel order by convention. ``imageio`` reads RGBA
back as 32-bit floats, so we swap R/B before encoding to OpenGL
convention that Bevy/PBR expects. The output is uint8, written with
``imageio.imwrite``.
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

import numpy as np
import OpenEXR
import Imath
import imageio.v2 as imageio


def read_exr_rgb(path: Path) -> np.ndarray:
    """Return a ``(H, W, 3)`` float32 array from an EXR file."""
    f = OpenEXR.InputFile(str(path))
    header = f.header()
    width = int(header["dataWindow"].max.x) + 1
    height = int(header["dataWindow"].max.y) + 1

    pt = Imath.PixelType(Imath.PixelType.FLOAT)
    r_str = f.channel("R", pt)
    g_str = f.channel("G", pt)
    b_str = f.channel("B", pt)

    r = np.frombuffer(r_str, dtype=np.float32).reshape(height, width)
    g = np.frombuffer(g_str, dtype=np.float32).reshape(height, width)
    b = np.frombuffer(b_str, dtype=np.float32).reshape(height, width)
    f.close()
    return np.stack([r, g, b], axis=-1)


def to_uint8_normal(rgb: np.ndarray) -> np.ndarray:
    """Convert a tangent-space normal in ``[-1, 1]`` to uint8 RGB.

    The reference uses DirectX convention (Y points down). Bevy/PBR
    expects OpenGL convention (Y points up), so we flip the green
    channel while we are at it. That keeps the bump on the rock
    geometry looking correct.
    """
    rgb = rgb.copy()
    rgb[..., 1] = -rgb[..., 1]
    encoded = rgb * 0.5 + 0.5
    return np.clip(encoded * 255.0, 0, 255).astype(np.uint8)


def resize_to(arr: np.ndarray, size: int) -> np.ndarray:
    """Box-filter resize (no extra deps) to ``(size, size)``."""
    src_h, src_w, channels = arr.shape
    ys = (np.linspace(0, src_h - 1, size)).astype(np.int64)
    xs = (np.linspace(0, src_w - 1, size)).astype(np.int64)
    return arr[ys[:, None], xs[None, :], :]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--src",
        type=Path,
        default=Path(
            r"G:\Eigene Dateien\Downloads\dark_rock_02_4k.blend\textures"
            r"\dark_rock_02_nor_gl_4k.exr"
        ),
        help="Path to the OpenEXR normal map (DirectX).",
    )
    parser.add_argument(
        "--dst",
        type=Path,
        default=Path(
            "assets/textures/celestial/asteroids/generic_rock_normal_2k.png"
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
    rgb = read_exr_rgb(args.src)
    if args.size and rgb.shape[0] != args.size:
        print(f"Resizing from {rgb.shape[0]}x{rgb.shape[1]} to {args.size}x{args.size}…")
        rgb = resize_to(rgb, args.size)
    img = to_uint8_normal(rgb)

    args.dst.parent.mkdir(parents=True, exist_ok=True)
    imageio.imwrite(args.dst, img)
    print(f"Wrote {args.dst} ({img.shape[0]}x{img.shape[1]}, tangent-space normal)")


if __name__ == "__main__":
    main()
