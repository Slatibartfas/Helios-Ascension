#!/usr/bin/env python3
"""
Create a 1x1 transparent PNG at assets/textures/ui/buildings/missing.png.

Used as a placeholder for buildings whose icon is being authored but
hasn't landed yet. The build_icons pipeline + map_icons_by_id.py
normally maps every entry to a real PNG, so missing.png is a
fallback of last resort.
"""
import struct
import zlib
from pathlib import Path

OUT = Path(r"G:\Repositories\Helios-Ascension\assets\textures\ui\buildings\missing.png")


def make_png(width: int, height: int, rgba_bytes: bytes) -> bytes:
    def chunk(typ: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + typ + data + struct.pack(
            ">I", zlib.crc32(typ + data) & 0xFFFFFFFF
        )

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    # Add filter byte (0) at start of each scanline.
    raw = b""
    for _ in range(height):
        raw += b"\x00" + rgba_bytes[: width * 4]
    idat = zlib.compress(raw, 9)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


# 36x36 transparent placeholder with a single dim cyan pixel at center
# so it's not entirely invisible (debugging).
SIZE = 36
pixels = bytearray(SIZE * SIZE * 4)
for y in range(SIZE):
    for x in range(SIZE):
        i = (y * SIZE + x) * 4
        # Border: dim cyan
        if x == 0 or y == 0 or x == SIZE - 1 or y == SIZE - 1:
            pixels[i + 0] = 0x60
            pixels[i + 1] = 0xC8
            pixels[i + 2] = 0xD8
            pixels[i + 3] = 0x40
        else:
            pixels[i + 3] = 0  # fully transparent inside

png = make_png(SIZE, SIZE, bytes(pixels))
OUT.write_bytes(png)
print(f"Wrote {OUT} ({len(png)} bytes)")
