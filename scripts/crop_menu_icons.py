#!/usr/bin/env python3
"""
Crop the baked-in text label out of each menu icon.

The egui top menu bar already renders the menu name as a text label
below the icon. The icons themselves also have the name baked in at
the bottom, which after the post-process (dark navy -> white-on-
transparent) shows up as white text inside the 80x80 button on the
dark menu bar, flickering at sub-pixel boundaries.

This script finds the gap between the pictogram and the baked-in
text label by scanning rows from the top, and crops each icon to
end at the last "line art" row before the gap. The result is a
clean, roughly square icon with only the pictogram.

Run from repo root:
    python scripts/crop_menu_icons.py
"""

import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    print("ERROR: Pillow not installed. Run: pip install Pillow", file=sys.stderr)
    sys.exit(1)


MENU_DIR = Path("assets/textures/ui/menu")

# Pixels darker than this (in grayscale luminance) are considered
# "line art or text". The pictogram and the baked-in label are both
# the same dark navy color, so a single threshold covers both.
DARK_THRESHOLD = 200

# A "gap" is a run of consecutive rows with zero dark pixels. The
# gap separates the pictogram (top) from the text label (bottom).
# We crop to the row just above the first such gap.
MIN_GAP_ROWS = 4

# Padding around the bounding box so the line art isn't pressed
# against the icon edge.
PADDING = 12


def find_pictogram_bottom(img: Image.Image) -> int:
    """Return the y-coordinate of the last pictogram row.

    Scans top-to-bottom starting from the first dark row (skipping
    the top padding), looking for the first run of >= MIN_GAP_ROWS
    consecutive empty rows. Returns the y of the row just before the
    gap starts. If no gap is found, returns the image height.
    """
    gray = img.convert("L")
    w, h = gray.size
    pixels = gray.load()

    # Find the first dark row (skip top padding).
    first_dark = 0
    while first_dark < h:
        has_dark = False
        for x in range(w):
            if pixels[x, first_dark] < DARK_THRESHOLD:
                has_dark = True
                break
        if has_dark:
            break
        first_dark += 1
    if first_dark >= h:
        return h  # No dark pixels at all

    # From the first dark row onward, look for the first gap.
    gap_run = 0
    last_dark_row = first_dark
    for y in range(first_dark, h):
        has_dark = False
        for x in range(w):
            if pixels[x, y] < DARK_THRESHOLD:
                has_dark = True
                break
        if has_dark:
            last_dark_row = y
            gap_run = 0
        else:
            gap_run += 1
            if gap_run >= MIN_GAP_ROWS:
                # Gap found. The pictogram ends just above it.
                return max(0, y - gap_run)
    return last_dark_row + 1


def crop_to_pictogram(img: Image.Image) -> Image.Image:
    """Crop img to the pictogram area (top) plus a small padding."""
    w, h = img.size
    pictogram_bottom = find_pictogram_bottom(img)
    # Add bottom padding, but don't extend past the original height.
    crop_y1 = min(h, pictogram_bottom + PADDING)
    if crop_y1 >= h:
        return img
    return img.crop((0, 0, w, crop_y1))


def main() -> int:
    if not MENU_DIR.is_dir():
        print(f"ERROR: {MENU_DIR} not found", file=sys.stderr)
        return 1

    pngs = sorted(MENU_DIR.glob("*.png"))
    # Skip preview/diagnostic files.
    pngs = [p for p in pngs if not p.name.startswith("_")]

    for path in pngs:
        img = Image.open(path)
        orig_size = img.size
        cropped = crop_to_pictogram(img)
        if cropped.size != orig_size:
            cropped.save(path, format="PNG", optimize=True)
            print(
                f"  {path.name:24s}  {orig_size[0]}x{orig_size[1]}  ->  "
                f"{cropped.size[0]}x{cropped.size[1]}"
            )
        else:
            print(f"  {path.name:24s}  {orig_size[0]}x{orig_size[1]}  (no change)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
