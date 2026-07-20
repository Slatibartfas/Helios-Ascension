"""Generate the Helios Ascension icon set from assets/logo/icon.png.

The source `assets/logo/icon.png` is the canonical 667x677 square crop
of the logo artwork (rockets, satellite, gimbal, dial, and wordmark)
already produced by the art team. We resize it (Lanczos) into the
per-size bitmaps Bevy's winit icon path needs.

Outputs (all under assets/icons/):
- icon.png        256x256 (Linux/desktop fallback)
- icon_16.png     16x16  (taskbar @ small sizes)
- icon_32.png     32x32  (taskbar)
- icon_48.png     48x48  (Explorer small)
- icon_64.png     64x64
- icon_128.png    128x128
- icon_256.png    256x256
- icon.ico        multi-resolution (16/32/48/64/128/256) hand-rolled
                  PNG-encoded entries inside the .ico container.
                  PNG-in-ICO is valid since Vista.

ICNS for macOS is intentionally NOT produced here because:
  - winit's runtime icon path (`Icon::from_rgba`) consumes RGBA
    bitmaps directly — ICNS is never read at runtime on any platform
    Bevy supports out-of-the-box.
  - The canonical macOS bundle workflow is `cargo-bundle` +
    `iconutil` (ImageMagick) at packaging time. Authors who want
    a macOS bundle should run `iconutil -c icns icons.iconset`
    on a separately-prepared .iconset folder.

The script is idempotent — running it twice produces the same files.
"""
from __future__ import annotations

import io
import struct
import sys
from pathlib import Path

from PIL import Image

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC = REPO_ROOT / "assets" / "logo" / "icon.png"
OUT = REPO_ROOT / "assets" / "icons"
OUT.mkdir(parents=True, exist_ok=True)


# ----------------------------------------------------------------------
# Resize helper
# ----------------------------------------------------------------------

def resize_icon(src: Image.Image, size: int) -> Image.Image:
    """Resize the source icon to `size`×`size` using Lanczos.

    The source is already a square (or near-square: 667×677)
    composition, so no crop is needed — only downscale. We
    force-fit to a square by centering the shorter dimension and
    trimming the longer one, which preserves the artwork's framing
    rather than stretching it.
    """
    w, h = src.size
    side = min(w, h)
    left = (w - side) // 2
    top = (h - side) // 2
    cropped = src.crop((left, top, left + side, top + side))
    return cropped.resize((size, size), Image.LANCZOS)


# ----------------------------------------------------------------------
# ICO writer (PNG-in-ICO)
# ----------------------------------------------------------------------
# Spec reference: https://en.wikipedia.org/wiki/ICO_(file_format)
# We emit a Vista+ "PNG-in-ICO" container — entries store raw PNG
# bytes, which Windows Vista and later read natively. Older Windows
# is not a target.

ICO_DIR_ENTRY_SIZE = 16
ICO_HEADER_SIZE = 6


def build_ico(entries: list[tuple[int, int, bytes]]) -> bytes:
    """Build an .ico file from (width, height, png_bytes) tuples.

    Width/height use 0 to mean 256 (the ICO format reserves 0 for
    sizes that don't fit in the single-byte width/height field).
    """
    out = io.BytesIO()
    # ICONDIR: reserved=0, type=1 (icon), count
    out.write(struct.pack("<HHH", 0, 1, len(entries)))
    offset = ICO_HEADER_SIZE + ICO_DIR_ENTRY_SIZE * len(entries)
    for width, height, png_bytes in entries:
        # ICONDIRENTRY:
        # width(1) height(1) color_count(1) reserved(1)
        # planes(2) bit_count(2) bytes_in_res(4) image_offset(4)
        b_w = 0 if width >= 256 else width
        b_h = 0 if height >= 256 else height
        out.write(struct.pack(
            "<BBBBHHII",
            b_w, b_h, 0, 0,
            1, 32,                # planes=1, bit_count=32
            len(png_bytes),
            offset,
        ))
        offset += len(png_bytes)
    for _, _, png_bytes in entries:
        out.write(png_bytes)
    return out.getvalue()


# ----------------------------------------------------------------------
# Main
# ----------------------------------------------------------------------

def encode_png(img: Image.Image) -> bytes:
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def main() -> int:
    if not SRC.exists():
        print(f"ERROR: source logo missing at {SRC}", file=sys.stderr)
        return 1

    src = Image.open(SRC).convert("RGBA")

    # Per-size: the source is already square-composed, so a single
    # Lanczos resize path works for every target size.
    sizes = [16, 32, 48, 64, 128, 256]
    per_size: dict[int, Image.Image] = {
        s: resize_icon(src, s) for s in sizes
    }

    # Canonical Linux / generic fallback: 256x256 PNG.
    png_256 = encode_png(per_size[256])
    (OUT / "icon.png").write_bytes(png_256)
    print(f"wrote icon.png        ({len(png_256):>7} bytes, 256x256)")

    # Write each PNG individually so modders can pick the size they
    # need without re-decoding the ICO container.
    for s in sizes:
        png = encode_png(per_size[s])
        (OUT / f"icon_{s}.png").write_bytes(png)
        print(f"wrote icon_{s:>3}.png      ({len(png):>7} bytes, {s}x{s})")

    # Write the multi-resolution .ico for Windows.
    ico_entries = [(s, s, encode_png(per_size[s])) for s in sizes]
    ico_bytes = build_ico(ico_entries)
    (OUT / "icon.ico").write_bytes(ico_bytes)
    print(f"wrote icon.ico        ({len(ico_bytes):>7} bytes, {sizes} PNG-in-ICO)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())