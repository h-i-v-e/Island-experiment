#!/usr/bin/env python3
"""Bake the native palm's fine leaflet chart for shared middle/far cutout cards.

Run from any directory; requires Pillow. The native chart in trees/palm.rs is
U=(lateral_metres+1)/2 and V=axial_metres/(2.9*1.03), with V=0 at the root.
PNG rows run top to bottom, so their Y direction is reversed from texture V.
All outlines mirror the 18-pair fine palm's root staggering, swept direction,
diamond width and 0.43 fold position. No random noise or external images.
"""

import argparse
import hashlib
import math
from pathlib import Path

from PIL import Image, ImageDraw

REFERENCE_LENGTH = 2.9
CHART_HALF_WIDTH = 1.0
CHART_LENGTH = REFERENCE_LENGTH * 1.03
PAIRS = 18


def chart_point(point, pixels):
    lateral, axial = point
    return (
        (lateral / (2 * CHART_HALF_WIDTH) + 0.5) * (pixels - 1),
        (1 - axial / CHART_LENGTH) * (pixels - 1),
    )


def leaf_points(pair, side):
    t = 0.12 + 0.83 * (pair + 0.28 + side * 0.10) / PAIRS
    length = 0.88 * math.sin(math.pi * t) ** 0.65 * 0.94
    sweep = 0.28 + t * 0.32
    root = (0.0, t * REFERENCE_LENGTH)
    direction = (side, sweep)
    tip = (side * length, root[1] + sweep * length)
    magnitude = math.hypot(*direction)
    across = (direction[1] / magnitude, -direction[0] / magnitude)
    width = 0.058 * math.sqrt(math.sin(math.pi * t))
    middle = (tip[0] * 0.43, root[1] + (tip[1] - root[1]) * 0.43)
    leading = tuple(middle[i] + across[i] * width for i in range(2))
    trailing = tuple(middle[i] - across[i] * width for i in range(2))
    return root, leading, tip, trailing


def bake(size, supersample):
    pixels = size * supersample
    # White transparent padding avoids dark fringes in every mip/filter mode.
    image = Image.new("RGBA", (pixels, pixels), (255, 255, 255, 0))
    draw = ImageDraw.Draw(image)
    for pair in range(PAIRS):
        for side in (-1, 1):
            root, leading, tip, trailing = leaf_points(pair, side)
            points = [chart_point(p, pixels) for p in (root, leading, tip, trailing)]
            draw.polygon(points[:3], fill=(247, 247, 247, 255))
            draw.polygon([points[0], points[2], points[3]], fill=(217, 217, 217, 255))
            draw.line([points[0], points[2]], fill=(255, 255, 255, 255), width=max(1, supersample))
    # Match the tapered near rachis, including the un-leafed root and fine tip.
    stem = [(-0.028, 0), (0.028, 0), (0.004, REFERENCE_LENGTH), (-0.004, REFERENCE_LENGTH)]
    draw.polygon([chart_point(p, pixels) for p in stem], fill=(235, 235, 235, 255))
    result = image.resize((size, size), Image.Resampling.LANCZOS)
    transparent = result.getchannel("A").point(lambda alpha: 255 if alpha == 0 else 0)
    result.paste((255, 255, 255, 0), mask=transparent)
    return result


def main():
    repo = Path(__file__).resolve().parents[1]
    default_output = repo / "packages/com.motu.runtime/Runtime/Resources/Motu/PalmFrond.png"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=default_output)
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--supersample", type=int, default=3)
    args = parser.parse_args()
    if args.size < 64 or args.supersample < 1:
        parser.error("size must be at least 64 and supersample at least 1")
    image = bake(args.size, args.supersample)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.output, optimize=False, compress_level=9)
    covered = sum(image.getchannel("A").histogram()[128:])
    digest = hashlib.sha256(args.output.read_bytes()).hexdigest()
    print(f"{args.output}: {args.size}x{args.size} RGBA; alpha>=0.5 {covered / args.size**2:.3%}; sha256 {digest}")


if __name__ == "__main__":
    main()
