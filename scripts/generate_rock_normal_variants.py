"""Generate a pool of 4 distinct tangent-space rock normal-map variants
for asteroid relief.

Writes
``assets/textures/celestial/asteroids/generic_rock_normal_{a,b,c,d}_2k.png``.

Each variant has a different character (crater density, fracture pattern,
regolith grain) so different asteroids read as visually distinct surfaces
rather than one bump map recoloured by per-class tint. The Rust selector
in ``src/plugins/solar_system.rs`` picks one per body via
``hash(name) % 4``; the original ``generic_rock_normal_2k.png`` is kept
as the fallback if any new file is missing or a body lands outside the
pool. The fallback path is intentional — the new variants are more
cratery than the old one, so the old one becomes the "least bumpy" option
when the pool is unavailable.

Run from the repo root with the project venv Python::

    .venv/Scripts/python.exe scripts/generate_rock_normal_variants.py

The recipes are stable: every salt/seed below is pinned so the output
files are byte-identical across re-runs. Change the recipe at your own
risk — the existing assets are part of the saved-game visual contract.
"""

from __future__ import annotations

from pathlib import Path
from typing import Sequence, Tuple

import imageio.v2 as imageio
import numpy as np

# Output size (square). 1024 matches the existing fallback so the
# mip chain and texture budget stay constant.
SIZE = 1024

OUT_DIR = Path("assets/textures/celestial/asteroids")

# Relief gradient gain. The base pipeline (see generate_asteroid_normal.py)
# used 2.5; the new variants push to 3.5 to make the bump read at
# in-game zoom because Bevy 0.18 does not expose a public
# normal_map_strength knob. The values below stay below the threshold
# where the rim normals start to alias into a noisy silhouette.
GRADIENT_GAIN = 3.5

# Crater-radius threshold (pixels) above which the impact gets a
# central rebound peak. Real-world craters transition from bowl to
# peak morphology around D ~= 8 km on stony bodies; we are not
# simulating that, but the visual jump is welcome.
CENTRAL_PEAK_TRANSITION_PX = 90.0


# ---------------------------------------------------------------------------
# Height-field primitives
# ---------------------------------------------------------------------------


def value_noise(x: np.ndarray, y: np.ndarray, salt: int) -> np.ndarray:
    """Cheap value-noise field for a height map, vectorised in NumPy.

    Identical formulation to ``generate_asteroid_normal.py`` so the
    fallback and the variants share the same micro-grain character at
    matching frequencies.
    """
    cell = 32.0
    ix = np.floor(x / cell).astype(np.int64)
    iy = np.floor(y / cell).astype(np.int64)
    fx = (x / cell) - ix
    fy = (y / cell) - iy

    def corner(cx: int, cy: int) -> np.ndarray:
        bx = np.broadcast_to(ix + cx, fx.shape)
        by = np.broadcast_to(iy + cy, fy.shape)
        key = (bx * np.int64(73856093)) ^ (by * np.int64(19349663)) ^ np.int64(salt)
        key &= np.int64(0xFFFFFFFF)
        return (
            (key * np.int64(2654435761) & np.int64(0xFFFFFFFF)).astype(np.float64)
            / float(0xFFFFFFFF)
        )

    v00 = corner(0, 0)
    v10 = corner(1, 0)
    v01 = corner(0, 1)
    v11 = corner(1, 1)

    sx = fx * fx * (3.0 - 2.0 * fx)
    sy = fy * fy * (3.0 - 2.0 * fy)
    top = v00 * (1.0 - sx) + v10 * sx
    bot = v01 * (1.0 - sx) + v11 * sx
    return top * (1.0 - sy) + bot * sy


def crater(
    xx: np.ndarray,
    yy: np.ndarray,
    cx: float,
    cy: float,
    r: float,
    depth: float,
    with_central_peak: bool,
) -> np.ndarray:
    """A single impact crater with optional central rebound peak.

    Bowl:  negative Gaussian at ``(cx, cy)`` of width ``r`` and
           amplitude ``depth``.
    Rim:   positive Gaussian annulus at ``r * 1.25`` of width
           ``r * 0.18`` and amplitude ``0.18 * depth``.
    Peak:  positive Gaussian at the centre of width ``r * 0.18`` and
           amplitude ``0.30 * depth``, only when ``with_central_peak``
           is true (i.e. ``r > CENTRAL_PEAK_TRANSITION_PX``).
    """
    dx = xx - cx
    dy = yy - cy
    dist = np.sqrt(dx * dx + dy * dy)
    bowl = -depth * np.exp(-((dist / r) ** 2))
    rim = 0.18 * depth * np.exp(-(((dist - r * 1.25) / (r * 0.18)) ** 2))
    peak = 0.0
    if with_central_peak:
        peak = 0.30 * depth * np.exp(-((dist / (r * 0.18)) ** 2))
    return bowl + rim + peak


def crater_field(
    xx: np.ndarray,
    yy: np.ndarray,
    *,
    count: int,
    r_min: float,
    r_max: float,
    depth_min: float,
    depth_max: float,
    seed: int,
) -> np.ndarray:
    """Drop ``count`` randomly placed craters within a parameter box.

    The central peak is enabled per-crater based on
    ``CENTRAL_PEAK_TRANSITION_PX`` so we don't have to thread the
    threshold through the recipe tuples.
    """
    rng = np.random.default_rng(seed)
    field = np.zeros_like(xx, dtype=np.float64)
    for _ in range(count):
        cx = float(rng.integers(0, SIZE))
        cy = float(rng.integers(0, SIZE))
        r = float(rng.uniform(r_min, r_max))
        depth = float(rng.uniform(depth_min, depth_max))
        field = field + crater(
            xx,
            yy,
            cx,
            cy,
            r,
            depth,
            r > CENTRAL_PEAK_TRANSITION_PX,
        )
    return field


def fracture_field(
    xx: np.ndarray,
    yy: np.ndarray,
    *,
    count: int,
    length: float,
    depth: float,
    seed: int,
) -> np.ndarray:
    """A handful of linear ridges/depressions for the fractured variant.

    Each fracture is a Gaussian-enveloped line of length ``length``
    and ~6 px half-width. Half the fractures are raised ridges, half
    are depressions — random per fracture — so the field reads as
    genuinely broken rock instead of a regular comb.
    """
    rng = np.random.default_rng(seed)
    field = np.zeros_like(xx, dtype=np.float64)
    for _ in range(count):
        x0 = float(rng.uniform(0, SIZE))
        y0 = float(rng.uniform(0, SIZE))
        theta = float(rng.uniform(0.0, 2.0 * np.pi))
        dx = np.cos(theta)
        dy = np.sin(theta)
        t = (xx - x0) * dx + (yy - y0) * dy
        px = (xx - x0) - t * dx
        py = (yy - y0) - t * dy
        perp = np.sqrt(px * px + py * py)
        along = np.abs(t)
        envelope = np.exp(-((along / length) ** 2)) * np.exp(-((perp / 6.0) ** 2))
        sign = 1.0 if rng.random() > 0.5 else -1.0
        field = field + sign * depth * envelope
    return field


# ---------------------------------------------------------------------------
# Per-variant recipes
# ---------------------------------------------------------------------------


# Each recipe is (count, r_min, r_max, depth_min, depth_max, seed).
# Splitting a recipe into a "small" group and a "large" group gives
# the eye a size hierarchy on the surface, which is what real
# cratered bodies look like.
CraterRecipe = Tuple[int, float, float, float, float, int]
FractureRecipe = Tuple[int, float, float, int]

VARIANTS: dict[str, dict] = {
    # Heavily cratered (Bennu / Ryugu / Callisto character).
    # Many small and medium craters, all central-peakless. Regolith
    # grain layered on top.
    "a": {
        "noise_layers": [
            (0.45, 1.0, 11),    # large boulder-scale undulation
            (0.32, 2.5, 29),    # medium relief
            (0.18, 6.5, 53),    # regolith grain
            (0.10, 14.0, 97),   # micro-grain
        ],
        "craters": [
            (32, 18, 75, 0.50, 0.80, 7001),
            (10, 80, 140, 0.55, 0.80, 7002),
        ],
        "fractures": [],
    },
    # Sparse large craters with prominent central peaks (Mathilde / Eros).
    # Lower crater count, big impact features, less regolith.
    "b": {
        "noise_layers": [
            (0.32, 1.0, 11),
            (0.25, 2.5, 29),
            (0.12, 7.0, 53),
        ],
        "craters": [
            (3, 200, 320, 0.75, 0.95, 8001),
            (4, 120, 200, 0.60, 0.85, 8002),
        ],
        "fractures": [],
    },
    # Fractured rock faces (Itokawa character). A few medium craters
    # on top of a noisy base that's been cut by linear ridges and
    # depressions. Reads as angular, faceted, less "rounded".
    "c": {
        "noise_layers": [
            (0.55, 1.0, 11),
            (0.38, 2.5, 29),
            (0.16, 7.0, 53),
            (0.08, 14.0, 97),
        ],
        "craters": [
            (7, 50, 110, 0.45, 0.65, 9001),
        ],
        "fractures": [
            (6, 320.0, 0.22, 9010),
            (4, 200.0, 0.18, 9011),
            (3, 130.0, 0.15, 9012),
        ],
    },
    # Rolling regolith (Ceres / Vesta character). A gentle low-frequency
    # base with scattered medium craters. Reads as smoother, more
    # "geological", the look of a larger body where impacts have
    # partly relaxed.
    "d": {
        "noise_layers": [
            (0.60, 1.0, 11),    # gentle rolling base
            (0.30, 2.5, 29),
            (0.16, 6.5, 53),
            (0.08, 14.0, 97),
        ],
        "craters": [
            (12, 60, 140, 0.45, 0.70, 1001),
            (6, 30, 70, 0.35, 0.55, 1002),
        ],
        "fractures": [],
    },
}


# ---------------------------------------------------------------------------
# Compose + encode
# ---------------------------------------------------------------------------


def height_field(
    noise_layers: Sequence[Tuple[float, float, int]],
    craters: Sequence[CraterRecipe],
    fractures: Sequence[FractureRecipe] = (),
) -> np.ndarray:
    xx, yy = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    field = np.zeros_like(xx, dtype=np.float64)
    for amp, freq, salt in noise_layers:
        field = field + amp * value_noise(xx * freq, yy * freq, salt=salt)
    for count, r_min, r_max, depth_min, depth_max, seed in craters:
        field = field + crater_field(
            xx,
            yy,
            count=count,
            r_min=r_min,
            r_max=r_max,
            depth_min=depth_min,
            depth_max=depth_max,
            seed=seed,
        )
    for count, length, depth, seed in fractures:
        field = field + fracture_field(
            xx,
            yy,
            count=count,
            length=length,
            depth=depth,
            seed=seed,
        )
    return field


def to_normal_map(height: np.ndarray, gain: float = GRADIENT_GAIN) -> np.ndarray:
    """Convert a height field to a tangent-space normal map (uint8 RGB)."""
    gy, gx = np.gradient(height.astype(np.float32))
    gx *= gain
    gy *= gain
    nz = np.ones_like(gx)
    norm = np.sqrt(gx * gx + gy * gy + nz * nz)
    nx = gx / norm
    ny = gy / norm
    nz = nz / norm
    enc = np.stack([nx, ny, nz], axis=-1) * 0.5 + 0.5
    return np.clip(enc * 255.0, 0, 255).astype(np.uint8)


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for label, params in VARIANTS.items():
        h = height_field(
            noise_layers=params["noise_layers"],
            craters=params["craters"],
            fractures=params.get("fractures", ()),
        )
        n = to_normal_map(h)
        out = OUT_DIR / f"generic_rock_normal_{label}_2k.png"
        imageio.imwrite(out, n)
        crater_count = sum(c[0] for c in params["craters"])
        fracture_count = sum(f[0] for f in params.get("fractures", ()))
        print(
            f"Wrote {out} ({SIZE}x{SIZE}, variant={label}, "
            f"craters={crater_count}, fractures={fracture_count})"
        )


if __name__ == "__main__":
    main()
