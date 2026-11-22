using System;
using System.Collections.Generic;
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
        internal readonly WheatFieldOutline Outline;

        internal WheatField(Vector2 centre, Vector2 halfExtents, float yaw, uint seed, WheatFieldOutline outline = null)
        {
            Centre = centre;
            HalfExtents = halfExtents;
            Yaw = yaw;
            Seed = seed;
            Outline = outline;
        }

        internal double InwardDistance(double x, double z)
        {
            return Outline == null ? Math.Min(HalfExtents.x - Math.Abs(x), HalfExtents.y - Math.Abs(z))
                : Outline.InwardDistance(x, z);
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
            if (town == null) return null;
            var fields = new List<WheatField>();
            var scale = worldSize / 2000f;
            var seed = unchecked((uint)townSeed ^ (uint)(townSeed >> 32));
            foreach (var plot in town.wheat_fields ?? Array.Empty<TownTrackBlocker>())
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (plot?.centre?.Length != 2 || plot.half_extents?.Length != 2)
                    throw new InvalidOperationException("The native wheat field bounds are invalid.");
                var x = plot.centre[0];
                var y = plot.centre[1];
                var fieldSeed = unchecked(seed ^ ((uint)BitConverter.SingleToInt32Bits(x) * 0x9e3779b9u)
                    ^ ((uint)BitConverter.SingleToInt32Bits(y) * 0x85ebca6bu));
                fields.Add(new WheatField(new Vector2(x - 1000f, y - 1000f) * scale,
                    new Vector2(plot.half_extents[0], plot.half_extents[1]) * scale, plot.yaw, fieldSeed));
            }
            var roadsideFields = town.roadsideFields?.fields;
            if (roadsideFields != null)
            {
                foreach (var plot in roadsideFields)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    if (plot.wheat)
                        fields.Add(WheatFieldOutline.FromNative(plot.outline, scale,
                            unchecked(seed ^ plot.id * 0x9e3779b9u)));
                }
            }
            if (fields.Count == 0) return null;
            var prepared = fields.ToArray();
            return new IslandPreparedWheat(prepared, WheatGroundMask.Build(prepared, cancellationToken));
        }
    }
}
