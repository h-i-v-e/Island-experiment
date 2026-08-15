using System;
using System.Runtime.InteropServices;

internal static class MotuNative
{
    private const string Library = "motu";

    [StructLayout(LayoutKind.Sequential)]
    internal struct Options
    {
        internal float maxZ;
        internal float waterRatio;
        internal float slopeMultiplier;
        internal float coastalSlopeMultiplier;
        internal float coastalErosionStrength;
        internal float beachFormationStrength;
        internal float hydraulicErosionStrength;
        internal float hydraulicDepositionStrength;
        internal float hydraulicDepositionSlopeDegrees;
        internal float riverLod2SourceThreshold;
        internal float riverLod1SourceThreshold;
        internal float riverBroadSourceThreshold;
        internal float riverLandSourceThreshold;
        internal float riverFinalSourceThreshold;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Vector3Array
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Vector2Array
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct TriangleArray
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeVector3
    {
        internal float x;
        internal float y;
        internal float z;

        internal NativeVector3(float x, float y, float z)
        {
            this.x = x;
            this.y = y;
            this.z = z;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportArea
    {
        internal NativeVector3 min;
        internal NativeVector3 max;

        internal ExportArea(float minimumX, float minimumY, float maximumX, float maximumY)
        {
            min = new NativeVector3(minimumX, minimumY, float.MinValue);
            max = new NativeVector3(maximumX, maximumY, float.MaxValue);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMesh
    {
        internal IntPtr handle;
        internal Vector3Array vertices;
        internal Vector3Array normals;
        internal TriangleArray triangles;
        internal Vector2Array uv;
        internal Vector3Array material;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMeshWithUv
    {
        internal IntPtr handle;
        internal Vector3Array vertices;
        internal Vector3Array normals;
        internal TriangleArray triangles;
        internal Vector2Array uv;
        internal Vector3Array material;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMeshGrid
    {
        internal IntPtr handle;
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportSurfaceMaps
    {
        internal IntPtr handle;
        internal int width;
        internal int height;
        internal IntPtr normalRgb;
        internal IntPtr occlusion;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct RiverEmitterExport
    {
        internal NativeVector3 position;
        internal NativeVector3 direction;
        internal float strength;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportRiverEmitters
    {
        internal IntPtr handle;
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportHeightMapWithSeaLevel
    {
        internal int width;
        internal int height;
        internal IntPtr data;
        internal float seaLevel;
    }

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateMotu(int seed, ref Options options);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMotu(IntPtr handle);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateMesh(
        IntPtr handle,
        IntPtr area,
        int lod,
        byte clampSides,
        out ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMesh(ref ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateSupportMesh(
        IntPtr handle,
        ref ExportArea area,
        int lod,
        out ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int lod,
        int divisions,
        byte clampSides,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMeshGrid(ref ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverMesh(
        IntPtr handle,
        IntPtr area,
        out ExportMeshWithUv output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int divisions,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMeshWithUV(ref ExportMeshWithUv output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverEmitters(
        IntPtr handle,
        float sharpnessDegrees,
        float spacingMetres,
        out ExportRiverEmitters output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseRiverEmitters(ref ExportRiverEmitters output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateHeightMap(IntPtr handle, int resolution);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseHeightMap(IntPtr map);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateSurfaceMaps(
        IntPtr handle,
        int lod,
        int dimension,
        out ExportSurfaceMaps output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseSurfaceMaps(ref ExportSurfaceMaps output);
}
