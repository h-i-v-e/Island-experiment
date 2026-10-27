using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Settings;

namespace Motu.World
{
    public static class OceanClipmapMeshBuilder
    {
        public const int MaximumVertexCount = 500000;
        public const int MaximumTriangleCount = 1000000;

        public static Mesh Build(
            float diameterMetres,
            OceanWaveRuntimeSettings settings,
            bool markNoLongerReadable = true)
        {
            var halfExtent = Mathf.Max(
                diameterMetres * 0.5f,
                settings.DisplacementFadeEndMetres + settings.FineVertexSpacingMetres);
            var coordinates = BuildAxisCoordinates(halfExtent, settings);
            var axisCount = coordinates.Count;
            var vertexCount64 = (long)axisCount * axisCount;
            var triangleCount64 = (long)(axisCount - 1) * (axisCount - 1) * 2L;
            if (vertexCount64 > MaximumVertexCount
                || triangleCount64 > MaximumTriangleCount)
            {
                throw new System.InvalidOperationException(
                    $"The ocean clipmap would require {vertexCount64:N0} vertices "
                    + $"and {triangleCount64:N0} triangles, exceeding its fixed runtime budget. "
                    + "Increase fine-grid spacing or reduce its radius.");
            }
            var vertexCount = (int)vertexCount64;
            var cellCount = checked((axisCount - 1) * (axisCount - 1));
            var vertices = new Vector3[vertexCount];
            var normals = new Vector3[vertexCount];
            var uv = new Vector2[vertexCount];
            var triangles = new int[checked(cellCount * 6)];

            for (var zIndex = 0; zIndex < axisCount; zIndex++)
            {
                var z = coordinates[zIndex];
                for (var xIndex = 0; xIndex < axisCount; xIndex++)
                {
                    var x = coordinates[xIndex];
                    var index = zIndex * axisCount + xIndex;
                    vertices[index] = new Vector3(x, 0f, z);
                    normals[index] = Vector3.up;
                    uv[index] = new Vector2(
                        x / (halfExtent * 2f) + 0.5f,
                        z / (halfExtent * 2f) + 0.5f);
                }
            }

            var triangle = 0;
            for (var zIndex = 0; zIndex < axisCount - 1; zIndex++)
            {
                for (var xIndex = 0; xIndex < axisCount - 1; xIndex++)
                {
                    var lowerLeft = zIndex * axisCount + xIndex;
                    var lowerRight = lowerLeft + 1;
                    var upperLeft = lowerLeft + axisCount;
                    var upperRight = upperLeft + 1;
                    triangles[triangle++] = lowerLeft;
                    triangles[triangle++] = upperLeft;
                    triangles[triangle++] = lowerRight;
                    triangles[triangle++] = lowerRight;
                    triangles[triangle++] = upperLeft;
                    triangles[triangle++] = upperRight;
                }
            }

            var horizontalMargin = settings.MaximumHorizontalDisplacement + 1f;
            var verticalMargin = settings.MaximumVerticalDisplacement + 1f;
            var mesh = new Mesh
            {
                name = "Player-Centred Ocean Graded Clipmap",
                indexFormat = IndexFormat.UInt32,
                vertices = vertices,
                normals = normals,
                uv = uv,
                triangles = triangles,
                bounds = new Bounds(
                    Vector3.zero,
                    new Vector3(
                        (halfExtent + horizontalMargin) * 2f,
                        verticalMargin * 2f,
                        (halfExtent + horizontalMargin) * 2f)),
            };
            mesh.UploadMeshData(markNoLongerReadable);
            return mesh;
        }

        public static List<float> BuildAxisCoordinates(
            float halfExtent,
            OceanWaveRuntimeSettings settings)
        {
            halfExtent = Mathf.Max(halfExtent, settings.FineVertexSpacingMetres);
            var positive = new List<float> { 0f };
            var spacing = settings.FineVertexSpacingMetres;
            var current = 0f;
            var fineExtent = Mathf.Min(settings.FineRadiusMetres, halfExtent);
            while (current + spacing < fineExtent)
            {
                current += spacing;
                positive.Add(current);
            }
            if (current < fineExtent)
            {
                current = fineExtent;
                positive.Add(current);
            }

            spacing *= 2f;
            var rowsAtSpacing = 0;
            var hasCoarseDisplacement = settings.DisplacementFadeEndMetres > settings.FineRadiusMetres;
            var hasDistantDisplacement = settings.DisplacementFadeEndMetres > settings.FineRadiusMetres * 2f;
            if (hasCoarseDisplacement)
            {
                // Keep enough samples for broad swell throughout the added band.
                // A single shared grid keeps every spacing transition watertight.
                var coarseExtent = Mathf.Min(settings.FineRadiusMetres * 2f,
                    Mathf.Min(settings.DisplacementFadeEndMetres, halfExtent));
                while (current < coarseExtent)
                {
                    current = Mathf.Min(current + spacing, coarseExtent);
                    positive.Add(current);
                    if (++rowsAtSpacing >= settings.RingsPerSpacingLevel)
                    {
                        rowsAtSpacing = 0;
                        spacing = Mathf.Min(spacing * 2f, settings.CoarseVertexSpacingMetres);
                    }
                }
                if (hasDistantDisplacement)
                {
                    // Preserve the existing coarse band, then add a half-density
                    // band reaching twice as far with the same shared vertices.
                    spacing = settings.DistantVertexSpacingMetres;
                    var distantExtent = Mathf.Min(settings.DisplacementFadeEndMetres, halfExtent);
                    while (current < distantExtent)
                    {
                        current = Mathf.Min(current + spacing, distantExtent);
                        positive.Add(current);
                    }
                }
                spacing *= 2f;
                rowsAtSpacing = 0;
            }
            // Beyond the last displaced vertex the surface is flat, so fewer
            // rows per band pay for the extended swell without growing the grid budget.
            var outerRows = hasDistantDisplacement ? 2 : hasCoarseDisplacement
                ? Mathf.Max(2, settings.RingsPerSpacingLevel / 2) : settings.RingsPerSpacingLevel;
            while (current < halfExtent)
            {
                current = Mathf.Min(current + spacing, halfExtent);
                positive.Add(current);
                rowsAtSpacing++;
                if (rowsAtSpacing >= outerRows)
                {
                    rowsAtSpacing = 0;
                    spacing *= 2f;
                }
            }

            var result = new List<float>(positive.Count * 2 - 1);
            for (var index = positive.Count - 1; index > 0; index--)
            {
                result.Add(-positive[index]);
            }
            result.AddRange(positive);
            return result;
        }
    }
}
