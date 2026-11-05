#!/usr/bin/env python3
"""Draws the crescent icons with the standard library only.

Writes src-tauri/icons/source.png (app icon, feed it to `pnpm tauri icon`) and
src-tauri/icons/tray.png (menu bar template image: black plus alpha).
"""
import pathlib
import struct
import zlib

ICONS = pathlib.Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
SAMPLES = 4


def png(path, size, pixel):
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            rows.extend(pixel(x, y, size))

    def chunk(kind, data):
        checksum = zlib.crc32(kind + data)
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", checksum)

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + chunk(b"IEND", b"")
    )


def coverage(x, y, size, inside):
    hits = 0
    for sy in range(SAMPLES):
        for sx in range(SAMPLES):
            u = (x + (sx + 0.5) / SAMPLES) / size
            v = (y + (sy + 0.5) / SAMPLES) / size
            hits += inside(u, v)
    return hits / SAMPLES**2


def crescent(u, v):
    in_outer = (u - 0.5) ** 2 + (v - 0.5) ** 2 <= 0.40**2
    in_bite = (u - 0.63) ** 2 + (v - 0.44) ** 2 <= 0.33**2
    return in_outer and not in_bite


def rounded_square(u, v):
    margin, radius = 0.1, 0.18
    low, high = margin + radius, 1 - margin - radius
    dx = max(low - u, 0, u - high)
    dy = max(low - v, 0, v - high)
    return dx**2 + dy**2 <= radius**2


def app_pixel(x, y, size):
    plate = coverage(x, y, size, rounded_square)
    moon = coverage(x, y, size, lambda u, v: crescent(0.5 + (u - 0.5) / 0.62, 0.5 + (v - 0.5) / 0.62))
    r, g, b = [round(a + (c - a) * moon) for a, c in zip((14, 62, 66), (244, 226, 176))]
    return r, g, b, round(255 * plate)


def tray_pixel(x, y, size):
    return 0, 0, 0, round(255 * coverage(x, y, size, crescent))


png(ICONS / "source.png", 512, app_pixel)
png(ICONS / "tray.png", 44, tray_pixel)
