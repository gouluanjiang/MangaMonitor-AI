"""Render public/favicon.svg's existing geometry to a dependency-free Windows ICO."""

from pathlib import Path
import struct


POLYGON = [(9, 28), (9, 12), (14, 12), (20, 22), (26, 12), (31, 12),
           (31, 28), (26, 28), (26, 19), (20, 28), (14, 19), (14, 28)]


def inside_polygon(x, y):
    inside = False
    previous = POLYGON[-1]
    for current in POLYGON:
        ax, ay = previous
        bx, by = current
        if (ay > y) != (by > y) and x < (bx - ax) * (y - ay) / (by - ay) + ax:
            inside = not inside
        previous = current
    return inside


def color_at(x, y):
    # The 40 x 40 rectangle has rx=11, matching public/favicon.svg exactly.
    dx, dy = max(11 - x, 0, x - 29), max(11 - y, 0, y - 29)
    if dx * dx + dy * dy > 121:
        return (0, 0, 0, 0)
    return (32, 32, 34, 255) if inside_polygon(x, y) else (217, 155, 114, 255)


def dib(size):
    pixels = bytearray()
    samples = 4
    for y in reversed(range(size)):
        for x in range(size):
            colors = [color_at((x + (sx + 0.5) / samples) * 40 / size,
                               (y + (sy + 0.5) / samples) * 40 / size)
                      for sy in range(samples) for sx in range(samples)]
            alpha_sum = sum(c[3] for c in colors)
            rgb = [round(sum(c[i] * c[3] for c in colors) / alpha_sum)
                   if alpha_sum else 0 for i in range(3)]
            pixels.extend([rgb[2], rgb[1], rgb[0], round(alpha_sum / len(colors))])
    mask = bytes(((size + 31) // 32) * 4 * size)
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0,
                         len(pixels) + len(mask), 0, 0, 0, 0)
    return header + pixels + mask


def main():
    sizes = [32, 48, 128, 256]
    images = [dib(size) for size in sizes]
    offset = 6 + 16 * len(sizes)
    entries = bytearray()
    for size, data in zip(sizes, images):
        entries.extend(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0,
                                   1, 32, len(data), offset))
        offset += len(data)
    destination = Path(__file__).resolve().parent.parent / "icons" / "icon.ico"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(struct.pack("<HHH", 0, 1, len(images)) + entries + b"".join(images))
    print(f"Generated {destination.name}: {destination.stat().st_size} bytes")


if __name__ == "__main__":
    main()
