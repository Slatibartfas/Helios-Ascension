"""Step-by-step icon pipeline diagnostic.

Reads one source PNG and emits every intermediate step of the runtime
icon-processing pipeline as a side-by-side sheet:

  1. Original  -- raw asset as authored on disk
  2. Decoded   -- RGBA8, 1:1, no transforms
  3. Resized   -- down-sampled to the target display size (Lanczos)
  4. Legacy cubic  -- alpha = (1 - luminance)^3  (the OLD recipe)
  5. New threshold -- alpha = (0.86 - lum) / (0.86 - 0.42) clamped  (NEW)
  6. Final tinted  -- premultiplied white on transparent, tinted cyan
  7. Magnified     -- the final tinted result blown up 4x so detail is visible

This lets us see exactly which step introduces the speckle, the
smeared edges, or the loss of contrast the player is seeing.

Usage:
    python scripts/debug_icon_pipeline.py [PNG] [TARGET_PX]

The default PNG is the Construction category badge. TARGET_PX is the
display size the in-game UI asks for (the bar uses 28 px after our
latest tweak; the dropdown uses 30 px; the popup uses 16 px).
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parents[1]


def find_default_asset() -> Path:
    """Return the first existing category-construction.png under assets/."""
    candidates = list((REPO_ROOT / "assets").rglob("category-construction.png"))
    if not candidates:
        candidates = list((REPO_ROOT / "assets").rglob("*.png"))
    return candidates[0]


def decode_rgba(path: Path) -> Image.Image:
    return Image.open(path).convert("RGBA")


def resize_nearest(image: Image.Image, size: int) -> Image.Image:
    """1:1 nearest-neighbour so the per-pixel structure stays visible."""
    return image.resize((size, size), Image.NEAREST)


def resize_lanczos(image: Image.Image, size: int) -> Image.Image:
    return image.resize((size, size), Image.LANCZOS)


def legacy_cubic_alpha(image: Image.Image) -> Image.Image:
    """The old recipe: alpha = (1 - luminance)^3."""
    w, h = image.size
    src = image.load()
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    for y in range(h):
        for x in range(w):
            r, g, b, _ = src[x, y]
            luminance = 0.299 * r + 0.587 * g + 0.114 * b
            alpha = max(0.0, min(1.0, (1.0 - luminance / 255.0) ** 3))
            pa = int(round(alpha * 255))
            dst[x, y] = (pa, pa, pa, pa)
    return out


def new_threshold_alpha(image: Image.Image) -> Image.Image:
    """The new recipe: alpha = ((0.86 - lum) / (0.86 - 0.42)) clamped."""
    w, h = image.size
    src = image.load()
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    lo, hi = 0.42, 0.86
    for y in range(h):
        for x in range(w):
            r, g, b, _ = src[x, y]
            luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0
            alpha = max(0.0, min(1.0, (hi - luminance) / (hi - lo)))
            pa = int(round(alpha * 255))
            dst[x, y] = (pa, pa, pa, pa)
    return out


def tint_white(processed: Image.Image, tint_rgb: tuple[int, int, int]) -> Image.Image:
    """Apply the in-game tint to a premultiplied-white alpha texture.

    Mirrors egui's `Image::tint` which multiplies the (already
    premultiplied) colour by the tint colour.
    """
    w, h = processed.size
    src = processed.load()
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    tr, tg, tb = tint_rgb
    for y in range(h):
        for x in range(w):
            pr, pg, pb, pa = src[x, y]
            # Premultiplied white * tint = tint * (alpha/255)
            scale = pa / 255.0
            r = int(round(tr * scale))
            g = int(round(tg * scale))
            b = int(round(tb * scale))
            dst[x, y] = (r, g, b, pa)
    return out


def draw_caption(image: Image.Image, text: str, height: int = 18) -> Image.Image:
    """Add a thin black caption strip below the image."""
    band = Image.new("RGBA", (image.width, height), (10, 14, 22, 255))
    draw = ImageDraw.Draw(band)
    try:
        font = ImageFont.load_default()
    except Exception:
        font = None
    draw.text((3, 1), text, fill=(0, 255, 220, 255), font=font)
    sheet = Image.new("RGBA", (image.width, image.height + height), (10, 14, 22, 255))
    sheet.paste(image, (0, 0), image)
    sheet.paste(band, (0, image.height))
    return sheet


def magnify(image: Image.Image, factor: int) -> Image.Image:
    return image.resize(
        (image.width * factor, image.height * factor),
        Image.NEAREST,
    )


def main() -> int:
    if len(sys.argv) >= 2:
        asset = Path(sys.argv[1]).resolve()
    else:
        asset = find_default_asset()
    if not asset.exists():
        print(f"ERROR: asset not found: {asset}", file=sys.stderr)
        return 1

    target_px = int(sys.argv[2]) if len(sys.argv) >= 3 else 28
    cyan = (0x60, 0xC8, 0xD8)

    print(f"asset      : {asset}")
    print(f"target_px  : {target_px}")

    original = decode_rgba(asset)
    print(f"original   : {original.size} mode={original.mode}")

    # Step 2: 1:1 nearest view of the decoded asset (no transforms).
    decoded = resize_nearest(original, original.width)

    # Step 3: Lanczos-downsample to the target display size.
    resized = resize_lanczos(original, target_px)

    # Step 4: legacy cubic recipe (old behaviour).
    legacy = legacy_cubic_alpha(resized)

    # Step 5: new threshold recipe (current code).
    new_alpha = new_threshold_alpha(resized)

    # Step 6: tinted as egui would draw it.
    legacy_tinted = tint_white(legacy, cyan)
    new_tinted = tint_white(new_alpha, cyan)

    # Step 7: blow up the final so per-pixel artefacts are obvious.
    legacy_mag = magnify(legacy_tinted, 4)
    new_mag = magnify(new_tinted, 4)

    # Build a horizontal sheet so the steps can be diffed left-to-right.
    panels = [
        draw_caption(decoded, "1. original"),
        draw_caption(resized, f"2. resized {target_px}px"),
        draw_caption(legacy, "3. legacy cubic"),
        draw_caption(new_alpha, "4. new threshold"),
        draw_caption(legacy_tinted, "5. legacy + tint"),
        draw_caption(new_tinted, "6. new + tint"),
        draw_caption(legacy_mag, "7. legacy 4x"),
        draw_caption(new_mag, "8. new 4x"),
    ]

    panel_height = panels[0].height
    sheet = Image.new(
        "RGBA",
        (sum(p.width for p in panels) + (len(panels) - 1) * 4, panel_height),
        (10, 14, 22, 255),
    )
    x = 0
    for p in panels:
        sheet.paste(p, (x, 0), p)
        x += p.width + 4

    out_dir = REPO_ROOT / "target" / "icon_debug"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"pipeline_{asset.stem}_{target_px}px.png"
    sheet.save(out_path)
    print(f"wrote      : {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
