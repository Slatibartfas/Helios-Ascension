"""Bake the OpenEXR roughness shipped with dark_rock_02 into a PNG.

The Blender reference ships a single-channel roughness map as
``dark_rock_02_rough_4k.exr``. Bevy 0.18's PNG/JPG pipeline cannot
load EXR at runtime, so the project ships a PNG sibling. This
script converts the EXR to a 1024x1024 RGBA PNG whose green channel
encodes the roughness (GLTF convention: B = metallic, G = roughness).

The source roughness lives in the Blinn ``[0, 1]`` range. We remap
it into a narrow upper band (``[0.85, 1.0]``) so that, after Bevy
multiplies the per-class ``perceptual_roughness`` scalar by the
texture's green channel, the shader still sees a clearly matte rock
surface. Rock is rough, never satin-smooth.

    assets/textures/celestial/asteroids/generic_rock_roughness_2k.png
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import OpenEXR
import Imath
import imageio.v2 as imageio

# Linear roughness range we want the texture to encode. The
# per-class roughness scalar multiplies this in the shader, so we
# keep the texture in the upper band so the product lands in
# [0.85, 1.0] regardless of which class is active.
ROUGH_MIN = 0.85
ROUGH_MAX = 1.0


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

    # Source encodes Blinn roughness in [0, 1]. Remap into the
    # narrow band we want the multiplied final roughness to live in.
    # Mid-roughness rock (the real reference) sits near the top of
    # the band, so ROUGH_MIN/ROUGH_MAX keeps the variation but
    # never lets any pixel fall below the per-class scalar's
    # contribution.
    rough = np.clip(rough, 0.0, 1.0)
    rough = ROUGH_MIN + (ROUGH_MAX - ROUGH_MIN) * rough
    rough = np.clip(rough, 0.0, 1.0)
    print(f"  remapped to [{rough.min():.3f}, {rough.max():.3f}], mean {rough.mean():.3f}")

    # GLTF metallic_roughness: R = AO (unused, 255), G = roughness, B = metallic (0).
    # Bevy treats the green channel as a data map, so we store the
    # *raw* byte value we want the shader to see (the linear
    # multiplier). Combined with the per-class `perceptual_roughness`
    # scalar at ~0.85..0.95, the final roughness lands in
    # [0.72, 0.95] which reads as a clearly matte rock.
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
