"""Generate a neutral tangent-space rock normal map for asteroid relief.

Run from the repository root with the project venv Python:

    .venv/Scripts/python.exe scripts/generate_asteroid_normal.py

Writes ``assets/textures/celestial/asteroids/generic_rock_normal_2k.png``.
The result is a tangent-space normal map (neutral base RGB ``128,128,255``)
with low-frequency crater / regolith relief and higher-frequency regolith
grain on top. The map is intentionally neutral so it can be reused across
all asteroid spectral classes (C, S, M, V, D, P) without biasing colour.

The class palette is already decided by the Rust material
(`asteroid_class_profile`); this map only contributes geometric relief.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import imageio.v2 as imageio


SIZE = 1024
OUT = Path("assets/textures/celestial/asteroids/generic_rock_normal_2k.png")


def value_noise(x: np.ndarray, y: np.ndarray, salt: int) -> np.ndarray:
    """Cheap value-noise field for a height map, vectorised in NumPy."""
    cell = 32.0
    ix = np.floor(x / cell).astype(np.int64)
    iy = np.floor(y / cell).astype(np.int64)
    fx = (x / cell) - ix
    fy = (y / cell) - iy

    rng = np.random.default_rng(salt)

    def corner(cx: int, cy: int) -> np.ndarray:
        # Deterministic per-tile random in [0, 1).
        bx = np.broadcast_to(ix + cx, fx.shape)
        by = np.broadcast_to(iy + cy, fy.shape)
        key = (bx * np.int64(73856093)) ^ (by * np.int64(19349663)) ^ np.int64(salt)
        key &= np.int64(0xFFFFFFFF)
        return (key * np.int64(2654435761) & np.int64(0xFFFFFFFF)).astype(np.float64) / float(0xFFFFFFFF)

    v00 = corner(0, 0)
    v10 = corner(1, 0)
    v01 = corner(0, 1)
    v11 = corner(1, 1)

    sx = fx * fx * (3.0 - 2.0 * fx)
    sy = fy * fy * (3.0 - 2.0 * fy)
    top = v00 * (1.0 - sx) + v10 * sx
    bot = v01 * (1.0 - sx) + v11 * sx
    return top * (1.0 - sy) + bot * sy


def height_field() -> np.ndarray:
    yy, xx = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)

    layer = np.zeros_like(xx)
    # Low-frequency craters and large boulders.
    layer += 0.6 * value_noise(xx, yy, salt=11)
    layer += 0.35 * value_noise(xx * 2.3, yy * 2.3, salt=29)
    # Regolith grain.
    layer += 0.18 * value_noise(xx * 6.5, yy * 6.5, salt=53)
    layer += 0.10 * value_noise(xx * 14.0, yy * 14.0, salt=97)

    rng = np.random.default_rng(20260728)
    # Drop a handful of shallow craters to break up the silhouette.
    for _ in range(8):
        cx = float(rng.integers(0, SIZE))
        cy = float(rng.integers(0, SIZE))
        r = float(rng.integers(60, 220))
        depth = float(rng.uniform(0.4, 0.7))
        dx = xx - cx
        dy = yy - cy
        dist = np.sqrt(dx * dx + dy * dy)
        crater = depth * np.exp(-((dist / r) ** 2))
        # Soft raised rim.
        rim = 0.15 * np.exp(-(((dist - r * 1.2) / (r * 0.18)) ** 2))
        layer = layer - crater + rim
    return layer


def main() -> None:
    h = height_field()

    # Sobel derivatives → tangent-space normal.
    gy, gx = np.gradient(h.astype(np.float32))
    # Scale the gradient so relief is visible without overpowering the
    # material's per-class tint.
    gx *= 2.5
    gy *= 2.5
    nz = np.ones_like(gx)
    norm = np.sqrt(gx * gx + gy * gy + nz * nz)
    nx = gx / norm
    ny = gy / norm
    nz = nz / norm

    # Encode (-1..1) → (0..255).
    enc = np.stack([nx, ny, nz], axis=-1) * 0.5 + 0.5
    img = np.clip(enc * 255.0, 0, 255).astype(np.uint8)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    imageio.imwrite(OUT, img)
    print(f"Wrote {OUT} ({SIZE}x{SIZE}, tangent-space normal)")


if __name__ == "__main__":
    main()
