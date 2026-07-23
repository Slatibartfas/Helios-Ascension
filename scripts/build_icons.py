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

    Per-size crop policy. The source 667×677 composition has two
    recognisable elements:
      - the wordmark "HELIOS ASCENSION" + dial ring (top half of the
        canvas), and
      - the gimbal hook + outer ship ring (which becomes pixel
        mush below ~48×48).

    At 16 and 32 the outer decoration is below the readability
    threshold of a Windows taskbar icon, so we crop tight on the
    wordmark and dial instead of shrinking the whole composition.
    At 48 we include the gimbal (the small mechanical motif just
    below "ASCENSION" — a brand cue at taskbar-thumbnail size).
    At 64+ the full composition fits.
    """
    w, h = src.size
    if size <= 16:
        # 16×16 → 8px-tall wordmark caps. Crop just the "HELIOS"
        # cap-line — readable, no "ASCENSION" subline.
        side = round(h * 0.40)
        cx, cy = w / 2, h * 0.40
    elif size <= 32:
        # 32×32 → both lines of the wordmark + a sliver of the
        # dial ring below the HELIOS cap-line.
        side = round(h * 0.55)
        cx, cy = w / 2, h * 0.43
    elif size == 48:
        # 48×48 → wordmark + gimbal hook. The gimbal reads as a
        # brand mechanical motif at thumbnail size.
        side = round(h * 0.70)
        cx, cy = w / 2, h * 0.50
    else:
        # 64, 128, 256: full composition fits the bitmap.
        side = min(w, h)
        cx, cy = w / 2, h / 2

    left = max(0, int(cx - side / 2))
    top = max(0, int(cy - side / 2))
    right = min(w, left + side)
    bottom = min(h, top + side)
    side = min(right - left, bottom - top)
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