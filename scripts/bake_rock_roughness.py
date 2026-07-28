"""Bake the OpenEXR roughness shipped with dark_rock_02 into a PNG.

The Blender reference ships a single-channel roughness map as
``dark_rock_02_rough_4k.exr``. Bevy 0.18's PNG/JPG pipeline cannot
load EXR at runtime, so the project ships a PNG sibling. This
script converts the EXR to a 1024x1024 RGBA PNG whose red channel
encodes the roughness (the others are zero). Bevy's
``metallic_roughness_texture`` accepts an RGB map where R carries
the metallic value and G carries the roughness; this output
uses the green channel (G) for roughness and zero on R, which
matches the GLTF convention.

The output is written next to the asteroid textures:

    assets/textures/celestial/asteroids/generic_rock_roughness_2k.png
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import OpenEXR
import Imath
import imageio.v2 as imageio


def read_exr_first_channel(path: Path) -> np.ndarray:
    """Return a single-channel float32 array from the first channel of an EXR."""
    f = OpenEXR.InputFile(str(path))
    header = f.header()
    width = int(header["dataWindow"].max.x) + 1
    height = int(header["dataWindow"].max.y) + 1
    pt = Imath.PixelType(Imath.PixelType.FLOAT)
    # Try common channel names in order; some EXRs use the original
    # layer name as the channel key.
    for candidate in ("R", "G", "B", "Y", "Z"):
        try:
            data = f.channel(candidate, pt)
        except Exception:
            continue
        f.close()
        return np.frombuffer(data, dtype=np.float32).reshape(height, width)
    # Fall back: enumerate channelSet.
    channels = header["channels"].keys()
    if not channels:
        f.close()
        raise SystemExit("EXR has no channels")
    first = next(iter(channels))
    data = f.channel(first, pt)
    f.close()
    return np.frombuffer(data, dtype=np.float32).reshape(height, width)


def resize_to(arr: np.ndarray, size: int) -> np.ndarray:
    src_h, src_w = arr.shape
    ys = (np.linspace(0, src_h - 1, size)).astype(np.int64)
    xs = (np.linspace(0, src_w - 1, size)).astype(np.int64)
    return arr[ys[:, None], xs[None, :]]


def to_uint8(arr: np.ndarray) -> np.ndarray:
    """Clamp to 0..1 and convert to uint8."""
    return np.clip(arr * 255.0, 0, 255).astype(np.uint8)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--src",
        type=Path,
        default=Path(
            r"G:\Eigene Dateien\Downloads\dark_rock_02_4k.blend\textures"
            r"\dark_rock_02_rough_4k.exr"
        ),
        help="Path to the OpenEXR roughness map (single channel).",
    )
    parser.add_argument(
        "--dst",
        type=Path,
        default=Path(
            "assets/textures/celestial/asteroids/generic_rock_roughness_2k.png"
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
    rough = np.clip(rough, 0.0, 1.0)

    # GLTF metallic_roughness convention: R = metallic, G = roughness.
    # We bake an RGB PNG with G carrying the roughness value.
    rgb = np.stack([np.zeros_like(rough), rough, np.zeros_like(rough)], axis=-1)
    img = to_uint8(rgb)

    args.dst.parent.mkdir(parents=True, exist_ok=True)
    imageio.imwrite(args.dst, img)
    print(f"Wrote {args.dst} ({img.shape[0]}x{img.shape[1]}, R=0, G=roughness)")


if __name__ == "__main__":
    main()
