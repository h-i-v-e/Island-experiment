using System;
using System.Collections.Generic;
using System.Threading;
using Motu.Islands;
using UnityEngine;

namespace Motu.Streaming
{
    internal static partial class WheatTileGeometry
    {
        private const double PlantSpacing = 0.30d;
        // Match the canopy rows in field-local Z. Keep most of the jitter along each row.
        private const double RowSpacing = 0.28d;
        private const double PlantJitter = 0.10d;
        private const double RowJitter = 0.045d;
        private const double PlantInset = 0.15d;

        private static void AppendPlants(
            IslandPreparedMesh terrain, FieldSpace space, WheatTileBounds tileBounds,
            List<WheatPlant> plants, CancellationToken token)
        {
            var heights = new Dictionary<Vector2Int, float>();
            var indices = terrain.triangles;
            var vertices = terrain.vertices;
            for (int index = 0; index < indices.Length; index += 3)
            {
                if ((index & 255) == 0) token.ThrowIfCancellationRequested();
                var triangle = new GroundTriangle(
                    vertices[indices[index]], vertices[indices[index + 1]], vertices[indices[index + 2]]);
                if (triangle.Covers(space.Bounds))
                    SampleTriangle(triangle, space, tileBounds, heights, token);
            }

            // Stable row order is independent of the tile's triangle order and tessellation.
            var cells = new List<Vector2Int>(heights.Keys);
            cells.Sort((a, b) => a.y == b.y ? a.x.CompareTo(b.x) : a.y.CompareTo(b.y));
            for (int i = 0; i < cells.Count; i++)
            {
                if ((i & 255) == 0) token.ThrowIfCancellationRequested();
                var cell = cells[i];
                var seed = CellSeed(space.Field.Seed, cell.x, cell.y);
                var root = PlantPosition(space, cell.x, cell.y, seed, heights[cell]);
                if (space.Field.Excludes(root)) continue;
                var yaw = space.Field.Yaw + (Unit(seed ^ 0x7f4a7c15u) - 0.5f) * 0.82f;
                var height = CanopyHeight + (Unit(seed ^ 0xd1b54a35u) - 0.5f) * 0.36f;
                var width = 0.34f + Unit(seed ^ 0x9e3779b9u) * 0.08f;
                var variation = Unit(seed ^ 0x632be59bu);
                plants.Add(new WheatPlant(root, yaw, height, width, variation));
            }
        }

        private static void SampleTriangle(
            GroundTriangle triangle, FieldSpace space, WheatTileBounds tileBounds,
            Dictionary<Vector2Int, float> heights, CancellationToken token)
        {
            space.ToField(triangle.A, out var ax, out var az);
            space.ToField(triangle.B, out var bx, out var bz);
            space.ToField(triangle.C, out var cx, out var cz);
            var half = space.Field.HalfExtents;
            double minX = Math.Max(PlantInset - half.x, Math.Min(ax, Math.Min(bx, cx)));
            double maxX = Math.Min(half.x - PlantInset, Math.Max(ax, Math.Max(bx, cx)));
            double minZ = Math.Max(PlantInset - half.y, Math.Min(az, Math.Min(bz, cz)));
            double maxZ = Math.Min(half.y - PlantInset, Math.Max(az, Math.Max(bz, cz)));
            if (minX > maxX || minZ > maxZ) return;

            int firstX = (int)Math.Ceiling((minX - PlantJitter) / PlantSpacing - 0.5d);
            int lastX = (int)Math.Floor((maxX + PlantJitter) / PlantSpacing - 0.5d);
            int firstZ = (int)Math.Ceiling((minZ - RowJitter) / RowSpacing - 0.5d);
            int lastZ = (int)Math.Floor((maxZ + RowJitter) / RowSpacing - 0.5d);
            for (int z = firstZ; z <= lastZ; z++)
            {
                token.ThrowIfCancellationRequested();
                for (int x = firstX; x <= lastX; x++)
                {
                    if ((x & 255) == 0) token.ThrowIfCancellationRequested();
                    var seed = CellSeed(space.Field.Seed, x, z);
                    PlantFieldPosition(x, z, seed, out var fieldX, out var fieldZ);
                    if (space.Field.InwardDistance(fieldX, fieldZ) < PlantInset)
                        continue;

                    var root = space.ToIsland(fieldX, fieldZ, 0f);
                    // Half-open tile bounds give a shared-edge root one owner.
                    if (root.x < tileBounds.MinX || root.x >= tileBounds.MaxX
                        || root.z < tileBounds.MinZ || root.z >= tileBounds.MaxZ
                        || !triangle.TryGetHeight(root, out var height)) continue;

                    var cell = new Vector2Int(x, z);
                    if (!heights.TryGetValue(cell, out var previous) || height > previous)
                        heights[cell] = height;
                }
            }
        }

        private static Vector3 PlantPosition(FieldSpace space, int column, int row, uint seed, float height)
        {
            PlantFieldPosition(column, row, seed, out var x, out var z);
            return space.ToIsland(x, z, height);
        }

        private static void PlantFieldPosition(int column, int row, uint seed, out double x, out double z)
        {
            x = (column + 0.5d) * PlantSpacing + (2d * Unit(seed) - 1d) * PlantJitter;
            z = (row + 0.5d) * RowSpacing + (2d * Unit(seed ^ 0xb5297a4du) - 1d) * RowJitter;
        }

        private static uint CellSeed(uint fieldSeed, int column, int row)
        {
            unchecked { return fieldSeed ^ (uint)column * 0x8da6b343u ^ (uint)row * 0xd8163841u; }
        }
    }
}
