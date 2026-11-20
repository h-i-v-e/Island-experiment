using System;
using System.Threading;
using Motu.Streaming;
using Motu.Towns;
using UnityEngine;

namespace Motu.Islands
{
    internal readonly struct WheatField
    {
        internal readonly Vector2 Centre;
        internal readonly Vector2 HalfExtents;
        // Native XY radians. On the Unity ground plane the row vector is (cos, sin) in XZ.
        internal readonly float Yaw;
        internal readonly uint Seed;

        internal WheatField(Vector2 centre, Vector2 halfExtents, float yaw, uint seed)
        {
            Centre = centre;
            HalfExtents = halfExtents;
            Yaw = yaw;
            Seed = seed;
        }
    }

    internal readonly struct WheatPlant
    {
        internal readonly Vector3 Root;
        internal readonly float Yaw, Height, Width, Variation;

        internal WheatPlant(Vector3 root, float yaw, float height, float width, float variation)
        {
            Root = root;
            Yaw = yaw;
            Height = height;
            Width = width;
            Variation = variation;
        }
    }

    internal sealed class IslandPreparedWheat
    {
        internal readonly WheatField[] Fields;
        internal readonly WheatGroundMask Mask;

        private IslandPreparedWheat(WheatField[] fields, WheatGroundMask mask)
        {
            Fields = fields;
            Mask = mask;
        }

        internal static IslandPreparedWheat Prepare(TownPreparedData town, ulong townSeed,
            float worldSize, CancellationToken cancellationToken)
        {
            var plots = town?.wheat_fields;
            if ((plots?.Length ?? 0) == 0) return null;
            var fields = new WheatField[plots.Length];
            var scale = worldSize / 2000f;
            for (var i = 0; i < fields.Length; i++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var plot = plots[i];
                if (plot?.centre?.Length != 2 || plot.half_extents?.Length != 2)
                    throw new InvalidOperationException("The native wheat field bounds are invalid.");
                var x = plot.centre[0];
                var y = plot.centre[1];
                var seed = unchecked((uint)townSeed ^ (uint)(townSeed >> 32)
                    ^ ((uint)BitConverter.SingleToInt32Bits(x) * 0x9e3779b9u)
                    ^ ((uint)BitConverter.SingleToInt32Bits(y) * 0x85ebca6bu));
                fields[i] = new WheatField(new Vector2(x - 1000f, y - 1000f) * scale,
                    new Vector2(plot.half_extents[0], plot.half_extents[1]) * scale, plot.yaw, seed);
            }
            return new IslandPreparedWheat(fields, WheatGroundMask.Build(fields, cancellationToken));
        }
    }
}
