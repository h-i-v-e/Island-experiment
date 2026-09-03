using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;

public static class OceanClipmapMeshBuilder
{
    public static Mesh Build(float diameterMetres, OceanWaveRuntimeSettings settings)
    {
        var halfExtent = Mathf.Max(
            diameterMetres * 0.5f,
            settings.DisplacementFadeEndMetres + settings.FineVertexSpacingMetres);
        var coordinates = BuildAxisCoordinates(halfExtent, settings);
        var axisCount = coordinates.Count;
        var vertexCount = checked(axisCount * axisCount);
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
        mesh.UploadMeshData(true);
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
        while (current < halfExtent)
        {
            current = Mathf.Min(current + spacing, halfExtent);
            positive.Add(current);
            rowsAtSpacing++;
            if (rowsAtSpacing >= settings.RingsPerSpacingLevel)
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
