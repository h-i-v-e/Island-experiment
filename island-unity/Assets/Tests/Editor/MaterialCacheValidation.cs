using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using UnityEngine;
using Motu.Interop;
using Motu.Islands;

using Motu.Streaming;
using Motu.World;
using Motu.Rendering;
using static UnityEngine.Object;
using static Motu.Rendering.IslandMaterialTextureCache;
namespace Motu.Editor
{
    internal static class MaterialCacheValidation
    {
        private static byte[] Sequence(int length, int offset)
        {
            var result = new byte[length];
            for (var index = 0; index < length; index++)
            {
                result[index] = (byte)(index + offset);
            }
            return result;
        }
        private static bool Same(
            IslandPreparedMaterialTexture expected,
            IslandPreparedMaterialTexture actual)
        {
            return actual != null
                && expected.width == actual.width
                && expected.height == actual.height
                && expected.physicalTileWidthMetres.Equals(actual.physicalTileWidthMetres)
                && expected.physicalTileHeightMetres.Equals(actual.physicalTileHeightMetres)
                && expected.minimumHeight.Equals(actual.minimumHeight)
                && expected.maximumHeight.Equals(actual.maximumHeight)
                && expected.baseHeight.Equals(actual.baseHeight)
                && System.Linq.Enumerable.SequenceEqual(expected.albedoRgb, actual.albedoRgb)
                && System.Linq.Enumerable.SequenceEqual(expected.normalRgb, actual.normalRgb)
                && System.Linq.Enumerable.SequenceEqual(expected.heightR16, actual.heightR16)
                && System.Linq.Enumerable.SequenceEqual(expected.occlusion, actual.occlusion);
        }

        internal static void ValidateRoundTrip()
        {
            var directory = Path.Combine(
                Path.GetTempPath(),
                "motu-material-cache-validation-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(directory);
            try
            {
                const int resolution = 2;
                var colours = new IslandMaterialColours(
                    new Color(0.11f, 0.07f, 0.03f),
                    new Color(0.24f, 0.26f, 0.28f),
                    new Color(0.62f, 0.48f, 0.21f));
                var texture = new IslandPreparedMaterialTexture(
                    resolution,
                    resolution,
                    2f,
                    3f,
                    -0.01f,
                    0.03f,
                    0f,
                    Sequence(resolution * resolution * 3, 7),
                    Sequence(resolution * resolution * 3, 23),
                    Sequence(resolution * resolution * 2, 41),
                    Sequence(resolution * resolution, 59));
                var expected = new IslandPreparedMaterialTextures(
                    colours,
                    texture,
                    texture,
                    texture,
                    texture,
                    texture,
                    texture,
                    texture);
                TrySave(expected, 1024L * 1024L, directory);
                var actual = TryLoad(colours, resolution, 1024L * 1024L, directory);
                if (actual == null
                    || !actual.loadedFromCache
                    || !Same(texture, actual.dirt)
                    || !Same(texture, actual.forestFloor)
                    || !Same(texture, actual.rock)
                    || !Same(texture, actual.riverBed)
                    || !Same(texture, actual.beach)
                    || !Same(texture, actual.fallenStones)
                    || !Same(texture, actual.treeBark))
                {
                    throw new InvalidOperationException(
                        "The content-addressed material cache did not round-trip its maps.");
                }
            }
            finally
            {
                Directory.Delete(directory, true);
            }
        }
    }
}
