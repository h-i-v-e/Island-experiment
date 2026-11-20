using System;
using System.Collections.Generic;
using System.Threading;
using Motu.Islands;
using UnityEngine;

namespace Motu.Streaming
{
    internal sealed class WheatTileData
    {
        internal readonly IslandPreparedMesh Canopy;
        internal readonly WheatPlant[] Plants;

        internal WheatTileData(IslandPreparedMesh canopy, WheatPlant[] plants)
        {
            Canopy = canopy;
            Plants = plants ?? Array.Empty<WheatPlant>();
        }
    }

    internal readonly struct WheatTileBounds
    {
        internal readonly float MinX, MinZ, MaxX, MaxZ;

        internal WheatTileBounds(float minX, float minZ, float maxX, float maxZ)
        {
            MinX = minX;
            MinZ = minZ;
            MaxX = maxX;
            MaxZ = maxZ;
        }

        internal bool Overlaps(WheatTileBounds other)
        {
            return MaxX > other.MinX && MinX < other.MaxX && MaxZ > other.MinZ && MinZ < other.MaxZ;
        }
    }

    internal static partial class WheatTileGeometry
    {
        internal const float CanopyHeight = 0.75f;

        internal static WheatTileData Build(
            IslandPreparedMesh terrain, WheatField[] fields, int lod, CancellationToken token,
            WheatTileBounds? tileBounds = null)
        {
            token.ThrowIfCancellationRequested();
            if (lod < 0 || lod > 2 || fields == null || fields.Length == 0
                || terrain == null || terrain.triangles.Length == 0)
                return null;

            var bounds = tileBounds ?? MeshBounds(terrain.vertices, token);
            CanopyBuilder canopy = null;
            List<WheatPlant> plants = null;
            foreach (var field in fields)
            {
                token.ThrowIfCancellationRequested();
                var space = new FieldSpace(field);
                if (!bounds.Overlaps(space.Bounds)) continue;

                if (lod == 0)
                {
                    plants ??= new List<WheatPlant>();
                    AppendPlants(terrain, space, bounds, plants, token);
                }
                else
                {
                    canopy ??= new CanopyBuilder();
                    canopy.Append(terrain, space, token);
                }
            }

            var mesh = canopy?.ToMesh();
            if (mesh == null && (plants == null || plants.Count == 0)) return null;
            return new WheatTileData(mesh, plants?.ToArray());
        }

        private static WheatTileBounds MeshBounds(Vector3[] vertices, CancellationToken token)
        {
            var min = new Vector2(vertices[0].x, vertices[0].z);
            var max = min;
            for (int index = 1; index < vertices.Length; index++)
            {
                if ((index & 1023) == 0) token.ThrowIfCancellationRequested();
                var vertex = new Vector2(vertices[index].x, vertices[index].z);
                min = Vector2.Min(min, vertex);
                max = Vector2.Max(max, vertex);
            }
            return new WheatTileBounds(min.x, min.y, max.x, max.y);
        }

        private readonly struct FieldSpace
        {
            internal readonly WheatField Field;
            internal readonly WheatTileBounds Bounds;
            private readonly double cosine;
            private readonly double sine;

            internal FieldSpace(WheatField field)
            {
                Field = field;
                cosine = Math.Cos(field.Yaw);
                sine = Math.Sin(field.Yaw);
                var halfX = Math.Abs(cosine) * field.HalfExtents.x + Math.Abs(sine) * field.HalfExtents.y;
                var halfZ = Math.Abs(sine) * field.HalfExtents.x + Math.Abs(cosine) * field.HalfExtents.y;
                Bounds = new WheatTileBounds(
                    (float)(field.Centre.x - halfX), (float)(field.Centre.y - halfZ),
                    (float)(field.Centre.x + halfX), (float)(field.Centre.y + halfZ));
            }

            internal void ToField(Vector3 ground, out double x, out double z)
            {
                double groundX = (double)ground.x - Field.Centre.x;
                double groundZ = (double)ground.z - Field.Centre.y;
                x = groundX * cosine + groundZ * sine;
                z = -groundX * sine + groundZ * cosine;
            }

            internal Vector3 ToIsland(double x, double z, double height)
            {
                return new Vector3(
                    (float)(Field.Centre.x + x * cosine - z * sine),
                    (float)height,
                    (float)(Field.Centre.y + x * sine + z * cosine));
            }
        }

        private readonly struct GroundTriangle
        {
            internal readonly Vector3 A;
            internal readonly Vector3 B;
            internal readonly Vector3 C;
            internal readonly double Area;

            internal GroundTriangle(Vector3 a, Vector3 b, Vector3 c)
            {
                A = a;
                B = b;
                C = c;
                Area = Edge(a, b, c);
            }

            internal bool Covers(WheatTileBounds bounds)
            {
                // Discard vertical and downward-facing faces of cliffs and openings.
                return Area > 0d
                    && Math.Min(A.x, Math.Min(B.x, C.x)) < bounds.MaxX
                    && Math.Max(A.x, Math.Max(B.x, C.x)) > bounds.MinX
                    && Math.Min(A.z, Math.Min(B.z, C.z)) < bounds.MaxZ
                    && Math.Max(A.z, Math.Max(B.z, C.z)) > bounds.MinZ;
            }

            internal bool TryGetHeight(Vector3 root, out float height)
            {
                height = 0f;
                double a = Edge(B, C, root);
                double b = Edge(C, A, root);
                double c = Edge(A, B, root);
                if (a < 0d || b < 0d || c < 0d) return false;
                height = (float)((a * A.y + b * B.y + c * C.y) / Area);
                return true;
            }

            private static double Edge(Vector3 a, Vector3 b, Vector3 p)
            {
                return ((double)b.z - a.z) * ((double)p.x - a.x)
                    - ((double)b.x - a.x) * ((double)p.z - a.z);
            }
        }

        private static float Unit(uint value)
        {
            unchecked
            {
                value ^= value >> 16;
                value *= 0x7feb352d;
                value ^= value >> 15;
                value *= 0x846ca68b;
                value ^= value >> 16;
                return (value >> 8) * (1f / 16777216f);
            }
        }
    }
}
