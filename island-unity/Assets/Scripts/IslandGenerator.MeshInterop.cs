using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    internal static IslandPreparedMesh CopyTerrainMeshData(
        MotuNative.ExportMesh source,
        int lod,
        float worldSize)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            source.environment,
            true,
            true,
            worldSize);
    }

    internal static Mesh CopyTerrainMesh(
        MotuNative.ExportMesh source,
        int lod,
        float worldSize)
    {
        return CreateTerrainMesh(CopyTerrainMeshData(source, lod, worldSize), lod);
    }

    internal static Mesh CreateTerrainMesh(IslandPreparedMesh source, int lod)
    {
        return CreateMesh(source, false);
    }

    internal static IslandPreparedMesh CopyRiverMeshData(
        MotuNative.ExportMesh source,
        float worldSize)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            source.environment,
            false,
            false,
            worldSize);
    }

    internal static Mesh CreateRiverMesh(IslandPreparedMesh source)
    {
        return CreateMesh(source, false);
    }

    internal static IslandPreparedMesh CopyGeneratedMeshData(
        MotuNative.ExportMesh source,
        float worldSize)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            source.environment,
            false,
            false,
            worldSize);
    }

    internal static Mesh CreateGeneratedMesh(IslandPreparedMesh source)
    {
        return CreateMesh(source, false);
    }

    private static IslandPreparedMesh CopyMeshData(
        MotuNative.Vector3Array sourceVertices,
        MotuNative.Vector3Array sourceNormals,
        MotuNative.TriangleArray sourceTriangles,
        MotuNative.Vector2Array sourceUv,
        MotuNative.Vector4Array sourceMaterial,
        MotuNative.Vector2Array sourceEnvironment,
        bool requireMaterial,
        bool createSurfaceMapCoordinates,
        float worldSize)
    {
        if (sourceVertices.data == IntPtr.Zero || sourceVertices.length == 0)
        {
            throw new InvalidOperationException("The Rust generator returned an empty mesh.");
        }

        var vertices = CopyVector3Array(sourceVertices, true, worldSize);
        var normals = CopyVector3Array(sourceNormals, false, worldSize);
        var triangles = new int[sourceTriangles.length];
        Marshal.Copy(sourceTriangles.data, triangles, 0, triangles.Length);

        // Rust is Z-up while Unity is Y-up. Swapping axes reflects the coordinate
        // system, so reverse each triangle to retain the original front face.
        for (var index = 0; index + 2 < triangles.Length; index += 3)
        {
            (triangles[index + 1], triangles[index + 2]) =
                (triangles[index + 2], triangles[index + 1]);
        }

        Vector2[] uv;
        if (sourceUv.data != IntPtr.Zero && sourceUv.length == vertices.Length)
        {
            uv = CopyVector2Array(sourceUv);
        }
        else if (createSurfaceMapCoordinates)
        {
            uv = CreateTerrainUv(vertices, worldSize);
        }
        else
        {
            uv = Array.Empty<Vector2>();
        }

        var material = CopyMaterialArray(sourceMaterial);
        if (requireMaterial && material.Length != vertices.Length)
        {
            throw new InvalidOperationException(
                "The Rust terrain export returned invalid material attributes.");
        }

        var environment = sourceEnvironment.data != IntPtr.Zero
            && sourceEnvironment.length == vertices.Length
                ? CopyVector2Array(sourceEnvironment)
                : Array.Empty<Vector2>();
        if (requireMaterial && environment.Length != vertices.Length)
        {
            throw new InvalidOperationException(
                "The Rust terrain export returned invalid environment attributes.");
        }

        return new IslandPreparedMesh(vertices, normals, triangles, uv, material, environment);
    }

    private static Mesh CreateMesh(IslandPreparedMesh source, bool createTangents)
    {
        var mesh = new Mesh
        {
            name = "Motu Generated Mesh",
            indexFormat = source.vertices.Length > ushort.MaxValue
                ? IndexFormat.UInt32
                : IndexFormat.UInt16,
            vertices = source.vertices,
            normals = source.normals,
            triangles = source.triangles,
        };

        if (source.uv.Length == source.vertices.Length)
        {
            mesh.uv = source.uv;
        }
        if (source.material.Length == source.vertices.Length)
        {
            mesh.colors = source.material;
        }
        if (source.environment.Length == source.vertices.Length)
        {
            // Rust environment attributes use UV1: x = forest floor, y = stones.
            mesh.uv2 = source.environment;
        }
        if (createTangents)
        {
            mesh.RecalculateTangents();
        }

        mesh.RecalculateBounds();
        mesh.UploadMeshData(false);
        return mesh;
    }

    private static Vector2[] CreateTerrainUv(Vector3[] vertices, float worldSize)
    {
        var uv = new Vector2[vertices.Length];
        for (var index = 0; index < vertices.Length; index++)
        {
            uv[index] = new Vector2(
                vertices[index].x / worldSize + 0.5f,
                vertices[index].z / worldSize + 0.5f);
        }
        return uv;
    }

    private static Vector3[] CopyVector3Array(
        MotuNative.Vector3Array source,
        bool position,
        float worldSize)
    {
        if (source.data == IntPtr.Zero || source.length == 0)
        {
            return Array.Empty<Vector3>();
        }

        var packed = new float[checked(source.length * 3)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Vector3[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            var offset = index * 3;
            var x = packed[offset];
            var y = packed[offset + 1];
            var z = packed[offset + 2];
            if (!IsFinite(x) || !IsFinite(y) || !IsFinite(z))
            {
                throw new InvalidOperationException(
                    $"The Rust generator returned a non-finite "
                    + $"{(position ? "position" : "normal")} at index {index}.");
            }
            result[index] = position
                ? new Vector3(
                    (x - 0.5f) * worldSize,
                    z * worldSize,
                    (y - 0.5f) * worldSize)
                : new Vector3(x, z, y).normalized;
        }

        return result;
    }

    private static Vector2[] CopyVector2Array(MotuNative.Vector2Array source)
    {
        var packed = new float[checked(source.length * 2)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Vector2[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            result[index] = new Vector2(packed[index * 2], packed[index * 2 + 1]);
        }

        return result;
    }

    private static Color[] CopyMaterialArray(MotuNative.Vector4Array source)
    {
        if (source.data == IntPtr.Zero || source.length == 0)
        {
            return Array.Empty<Color>();
        }

        var packed = new float[checked(source.length * 4)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Color[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            var offset = index * 4;
            result[index] = new Color(
                packed[offset],
                packed[offset + 1],
                packed[offset + 2],
                packed[offset + 3]);
        }
        return result;
    }

}
