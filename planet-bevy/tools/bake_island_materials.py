#!/usr/bin/env python3
"""Bake Unity Island's six ground recipes into portable, stacked PNG arrays.

Uses the existing Island native ABI; Planet never needs that library at runtime.
Run after island-rs/deploy-unity.sh, passing the Island checkout as the argument.
"""
import ctypes as c
import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib


class Inputs(c.Structure):
    _fields_ = [(name, c.c_float * 3) for name in ("dirt", "stone", "sand")]


class Options(c.Structure):
    _fields_ = [("width", c.c_uint32), ("height", c.c_uint32),
                ("normal", c.c_uint8), ("mask", c.c_uint8), ("reserved", c.c_uint8 * 2)]


class Bytes(c.Structure):
    _fields_ = [("data", c.POINTER(c.c_uint8)), ("length", c.c_int32)]

    def read(self):
        return c.string_at(self.data, self.length)


class Texture(c.Structure):
    _fields_ = [("width", c.c_int32), ("height", c.c_int32),
                ("tile_width", c.c_float), ("tile_height", c.c_float),
                ("minimum", c.c_float), ("maximum", c.c_float), ("neutral", c.c_float),
                ("albedo", Bytes), ("normal", Bytes), ("height_map", Bytes), ("occlusion", Bytes)]


LAYERS = ("dirt", "forest_floor", "rock", "river_bed", "beach", "fallen_stones")


class TextureSet(c.Structure):
    _fields_ = [("handle", c.c_void_p)] + [(name, Texture) for name in (*LAYERS, "bark")]


def png(width, height, rgba):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def linear(value):
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def main():
    root = Path(sys.argv[1]).resolve()
    output = Path(__file__).resolve().parents[1] / "assets/terrain"
    library = root / "packages/com.motu.runtime/Runtime/Plugins/macOS/libmotu.dylib"
    native = c.CDLL(str(library))
    native.BakeMotuMaterialTextures.argtypes = [c.POINTER(Inputs), c.POINTER(Options), c.POINTER(TextureSet)]
    native.BakeMotuMaterialTextures.restype = c.c_uint8
    native.ReleaseMaterialTextureSet.argtypes = [c.POINTER(TextureSet)]
    palette = ((0.09, 0.055, 0.026), (0.30, 0.32, 0.29), (0.62, 0.57, 0.34))
    inputs = Inputs(*[(c.c_float * 3)(*(linear(v) for v in colour)) for colour in palette])
    size = 512
    options = Options(size, size, 0, 0x3f, (c.c_uint8 * 2)(0, 0))
    textures = TextureSet()
    if native.BakeMotuMaterialTextures(c.byref(inputs), c.byref(options), c.byref(textures)) != 1:
        raise RuntimeError("Island native material bake failed")
    try:
        maps = {name: bytearray() for name in ("albedo", "normal", "mask")}
        metadata = []
        for name in LAYERS:
            texture = getattr(textures, name)
            assert texture.width == size and texture.height == size
            for channel in ("albedo", "normal"):
                rgb = getattr(texture, channel).read()
                rgba = bytearray([255]) * (size * size * 4)
                for component in range(3):
                    rgba[component::4] = rgb[component::3]
                maps[channel].extend(rgba)
            heights, occlusion = texture.height_map.read(), texture.occlusion.read()
            mask = bytearray([255]) * (size * size * 4)
            mask[0::4] = heights[1::2]
            mask[1::4] = occlusion
            mask[2::4] = bytes(size * size)
            maps["mask"].extend(mask)
            metadata.append({"name": name, "tile_metres": [texture.tile_width, texture.tile_height],
                             "neutral_height": (texture.neutral - texture.minimum) / (texture.maximum - texture.minimum)})
        output.mkdir(parents=True, exist_ok=True)
        for name, data in maps.items():
            (output / f"{name}.png").write_bytes(png(size, size * len(LAYERS), data))
        source = root / "packages/com.motu.runtime/Runtime/Shaders"
        provenance = {"resolution": size, "normal_convention": "OpenGL", "palette_srgb": palette,
                      "layers": metadata, "native_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
                      "recipe_sha256": {name: hashlib.sha256((root / "island-rs/texture-recipes" / name).read_bytes()).hexdigest()
                                        for name in ("Dirt.json", "ForestFloor.json", "cracked-stone.json", "rounded-river-stones.json", "Beach.json", "FallenStones.json")},
                      "source_sha256": {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
                                        for name in ("TerrainDetail.shader", "TerrainCoverageCommon.cginc", "TerrainRockCommon.cginc")}}
        (output / "source.json").write_text(json.dumps(provenance, indent=2) + "\n")
        print(f"Baked {len(LAYERS)} Unity terrain layers to {output}")
    finally:
        native.ReleaseMaterialTextureSet(c.byref(textures))


if __name__ == "__main__":
    main()
